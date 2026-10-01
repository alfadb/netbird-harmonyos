// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # connector — 连接生命周期编排（N3-5）
//!
//! 把 N3-2/N3-3/N3-4 的控制面协议链（gRPC 通道 + 信封加密 → `Login` →
//! `Sync` 会话）串成一个可启动/可查询/可停止的连接生命周期，并把网络图
//! 中"可应用"的部分推到数据面 seam 上。上游对照（pinned commit
//! `791401060d2b95e5f51e3439c0649729132f571e`，一律只引用文件:行号，不抄
//! 代码）：
//!
//! - 生命周期形状 ≈ 上游 engine 的 management 连接循环
//!   `shared/management/client/grpc.go:224-275`（`withMgmtStream`）+
//!   `handleSyncStream`（grpc.go:427-476）：连接 → 登录 → 持有 Sync 流，
//!   断流按 backoff 重连，`PermissionDenied`/`Unauthenticated` 是
//!   Permanent（grpc.go:436-438/L468-470，本仓 [`crate::sync`] 已按此二分）。
//! - 会话续期时机：上游把 Login/Sync 上的 3 态 `sessionExpiresAt` 锚定到
//!   watcher（`client/internal/engine_authsession.go:17-62`
//!   `ApplySessionDeadline`；nil=保持、显式零=禁用、有效值=新期限），临期
//!   警告提前量在 `client/internal/auth/sessionwatch/watcher.go:34-43`
//!   （T-10min 交互警告 / T-2min 兜底）。本模块在剩余寿命小于
//!   `session_renew_lead_ms`（默认 10 分钟，同 WarningLead）时主动调用
//!   `ExtendAuthSession`；上游续期入口语义见
//!   `engine_authsession.go:83-107`（空 JWT 直接拒绝 L84-86；成功后把新
//!   期限写回 L102；隧道不动、不重同步 L71-73 注释）。
//! - 快照顺序：`client/internal/engine.go:1572-1576` — serial 严格小于
//!   已应用值的 NetworkMap 直接丢弃（相等仍应用）。
//!
//! ## N3-7 fail-closed 缺口修复
//!
//! 1. **管理连接自身 socket 过 protect 门**：`GrpcManagementFactory` 可携带
//!    受保护 socket 源（[`crate::mgmtsock::ManagementSocketProvider`]），
//!    每次 dial（首连 + tonic/hyper 每次重连 + 会话续期连接）都取一条**新鲜**
//!    已保护 socket，经 `Endpoint::connect_with_connector` 让 gRPC 通道走该
//!    socket；fd 只消费 dup 副本（`F_DUPFD_CLOEXEC`，绝不使用/关闭原始 fd，
//!    见 `crate::mgmtsock` fd 合同）。`connector_start_with_socket` 未提供
//!    有效 fd 时**拒绝启动**；无保护直连只能通过配置项
//!    `allow_unprotected_management=true` 显式 opt-in（非上游行为、危险，
//!    启动时打 hilog 警示）。上游语义出处见 `crate::mgmtsock` 模块文档。
//! 2. **默认路由安全闸**：`ShellNetworkConfig::from_map_gated` 只在
//!    「已登记 peer > 0 且 WG 数据面 `tunnel_ready()`」时才把 `0.0.0.0/0`
//!    导出给壳安装，否则剥离并携带 `default-route-held:*` 原因
//!    token（`force_default_route` 为显式开发期 opt-in，标注黑洞风险）。
//! 3. **connector 死亡语义**：`ConnectorStatus.terminal` = worker 自行终止
//!    （fatal/backoff 耗尽，`running:false` 且 `state:failed`）；壳侧
//!    watcher 据此拆除 VPN（见 notes N3-7 节）。
//!
//! ## ⚠️ 限制声明（调用方必读，不得误读为"已能连上 peer"）
//!
//! **N5c 已接入 per-peer ICE 编排**（`crate::peer_conn`，connector_status
//! 的 `ice` 字段）：网络图里有 allowed_ips 的 remote peer 各建一个 ICE
//! 会话（候选收集 + signal OFFER/ANSWER/候选交换 + selected pair → WG
//! endpoint 落配）。**N5d 起真实 signal 流已接入**：sync 带来的
//! `netbird_config.signal` 地址生效后由 [`SignalRuntime`] 启动
//! `crate::peer_conn::spawn_signal_link`（真实 `SignalSession`：注册、
//! 信封加密收发、断流重连重注册），ICE 发送 seam 换成
//! [`crate::peer_conn::RealSignalExchange`]（未注册显式失败、编排层
//! outbox 重试，不再打点丢弃），收帧按发送方公钥路由进编排。剩余硬边界：
//!
//! 1. signal socket 仍由壳侧补给：`connector_signal_socket_feed(fd, addr)`
//!    注入已 protect 的 signal socket 与 DNS 解析结果；壳不喂则拨号
//!    fail-closed 重试，`signal_ready` 保持 false（peer 不发起 ICE）。
//! 2. ICE 的 UDP socket 来自壳侧补给的受保护源
//!    （`connector_ice_socket_feed(fd)`）；壳侧不喂则候选收集 fail-closed，
//!    peer 停在 Idle 并记录 Network 类错误。
//! 3. 网络图里的 remote peers 的 WG 侧配置走 [`WgPeerApplier`]：**N7 起生产
//!    默认是 [`crate::wg_device::WgDeviceFeed`]**（真实 [`WgDeviceApplier`]，
//!    fd 由壳侧 feed 补给后建成设备，feed 前缓冲 peer/endpoint、
//!    `tunnel_ready` 恒 false——fail-closed，默认路由 HOLD）；N6 的真实设备
//!    驱动 [`crate::wg_device::WgDeviceApplier`] 是其直连形态（测试/宿主
//!    注入用），N3-5 的 [`WgPeerRegistry`]（本地登记 + endpoint 记录、
//!    `tunnel_ready` 恒 false）保留为测试/对照 seam。「登记 ≠ 隧道」对
//!    registry 仍成立，设备驱动则给出真实会话状态。
//! 4. 路由/DNS 通过 [`ConfigApplier`] 交给壳侧（宿主）；壳侧不接时生产
//!    默认 [`LoggingConfigApplier`] 只打点，不落任何系统配置。
//! 5. "Connected" 仍指 **management 控制面连接已建立**（登录成功 + Sync
//!    流在），与 peer 连通无关；peer 级状态在 status 的 `ice` 字段，
//!    signal 通道状态在 `signal` 字段（registered/reconnects/last_error），
//!    WG 数据面在 `wg` 字段（N7：fed_socket/fed_tun/device_up/ready + 真实
//!    设备计数——`crate::wg_device::WgDataplaneStatus`）。
//!
//! ## 注入 seam（宿主测试与壳侧接线点）
//!
//! - management 连接：[`ManagementFactory`]（生产实现
//!   [`GrpcManagementFactory`]：tonic/rustls，TLS CA 由配置注入，无系统根
//!   存储——见 `crate::grpc` 模块文档）。宿主测试可注入桩工厂。
//! - sync 策略：[`SyncPolicy`]（backoff/时钟/随机源，透传给
//!   [`crate::sync::SyncSession::with_policy`]，测试零真实长睡眠）。
//! - 数据面 WG peer 登记：[`WgPeerApplier`]（**N7 生产默认
//!   [`crate::wg_device::WgDeviceFeed`]**——壳侧 feed 补给 fd 后建成的真实
//!   设备；[`WgPeerRegistry`] 保留为测试/对照，N6 直连形态
//!   [`crate::wg_device::WgDeviceApplier`] 供宿主测试注入）。
//! - 壳侧配置应用：[`ConfigApplier`]（路由/DNS/本机地址交给宿主）。
//! - N5c per-peer ICE：[`crate::peer_conn::PeerIceOrchestrator`]（接口枚举
//!   `crate::ice::InterfaceSource`、受保护 UDP 源
//!   `connector_ice_socket_feed` 补给、signal 发送
//!   [`crate::peer_conn::SignalExchange`]、endpoint 落配置用同一个
//!   [`WgPeerApplier`]）。
//!
//! ## 凭据与日志纪律
//!
//! setup key、JWT、设备私钥**不得**出现在日志、错误消息、`Debug` 输出或
//! JSON 导出里：[`ConnectorSecrets`] 手写 `Debug` 全遮蔽；
//! [`ConnectorConfig`] 只持有 [`crate::envelope::EnvelopeKeyPair`]（其
//! `Debug` 仅暴露公钥，见 `crate::envelope`）；状态/错误只走
//! [`ErrorClass`]（分类 + 状态码，**无** server/transport message）；
//! hilog 行只含状态与计数；私钥在解析后不保留任何原始字节形式。

use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;

use base64::Engine as _;

use crate::backoff::{Clock, ExponentialBackoff, MonotonicClock, OsRandom, Rng};
use crate::config::{self, ConfigError, Json};
use crate::envelope::EnvelopeKeyPair;
use crate::grpc::{GrpcTransport, LoginParams, ManagementGrpcClient, PeerMeta};
use crate::hilog;
use crate::management::ManagementError;
use crate::mgmtsock::{ManagementSocketProvider, ProtectedSocketFdSource};
use crate::network_map::NetworkMap;
use crate::peer_conn::{
    ice_ready_for_default_route, spawn_signal_link_shared, IceOrchestratorSummary, LoggingSignalExchange,
    PeerIceDeps, PeerIceOrchestrator, PeerSignalKind, RealSignalExchange, SignalLinkConfig,
    SignalLinkEvent,
};
use crate::signal::{SignalMessage, SignalOutgoing};
use crate::state::{ConnEvent, ConnState, StateMachine};
use crate::sync::{SyncLoopEvent, SyncSession, SyncUpdate};
use crate::util::{jbool, jinum, jnum, jstr};

/// Pinned upstream commit the lifecycle semantics are modeled on (same pin
/// as [`crate::grpc::UPSTREAM_COMMIT`]).
pub const UPSTREAM_COMMIT: &str = crate::grpc::UPSTREAM_COMMIT;

// ---------------------------------------------------------------------------
// N8: controlled-recreate state (platform VpnConfig is fixed at create())
// ---------------------------------------------------------------------------

/// Recreate budget: at most this many completed recreates per connector
/// lifetime. Bounds a flapping data plane (each desired-set change consumes
/// one) so the shell can never be driven into a recreate storm.
pub const RECREATE_MAX: u64 = 3;

/// Recreate cooldown (monotonic ms): after a completed recreate, a NEW
/// desired-set change is not re-raised until this much time has passed. The
/// watcher's periodic `status()`/`network_config()` reads re-run the refresh,
/// so the requirement re-surfaces after the cooldown without any spinning.
pub const RECREATE_COOLDOWN_MS: u64 = 30_000;

/// N8 controlled-recreate state (exposed through `connector_status()`'s
/// `recreate` object — no key material). The platform VpnConfig is fixed at
/// `VpnConnection.create()` time, so when the DESIRED route set (the live
/// gate decision applied to the last network map's routes) diverges from the
/// route set the shell last reported as APPLIED, the default route can only
/// reach the tunnel through a controlled connection rebuild. This surface
/// tells the shell WHEN (`required`) and bounds the churn (`count` /
/// `exhausted` / `cooling_down`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecreateStatus {
    /// The shell should rebuild the connection NOW (desired route set !=
    /// applied route set, budget left, cooldown passed). Level-based: it
    /// stays true across refreshes until the shell ACKs a new applied set.
    pub required: bool,
    /// Completed recreates (ACKed with `is_recreate=true`).
    pub count: u64,
    /// The budget is spent — no further recreates will be requested this
    /// connector lifetime (logged loudly; the snapshot gate stays honest).
    pub exhausted: bool,
    /// A desired-set change exists but is parked inside the cooldown window.
    pub cooling_down: bool,
    /// Stable reason token: `none` / `route-set-changed` / `limit-reached`.
    pub reason: String,
}

impl Default for RecreateStatus {
    fn default() -> Self {
        RecreateStatus {
            required: false,
            count: 0,
            exhausted: false,
            cooling_down: false,
            reason: "none".to_string(),
        }
    }
}

impl RecreateStatus {
    /// The `connector_status()` `recreate` JSON object.
    pub fn to_json(&self) -> String {
        format!(
            "{{{},{},{},{},{}}}",
            jbool("required", self.required),
            jinum("count", self.count as i64),
            jbool("exhausted", self.exhausted),
            jbool("cooling_down", self.cooling_down),
            jstr("reason", &self.reason),
        )
    }
}

// ---------------------------------------------------------------------------
// sanitized error surface (classification only — never a message)
// ---------------------------------------------------------------------------

/// Error classification exposed by the connector. Deliberately carries NO
/// server/transport message: `ManagementError`'s `Display` may embed server
/// text, and the connector's status/error surfaces must not leak it
/// (credential discipline: only class + numeric status cross the boundary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    Network,
    Timeout,
    Auth { status: u16 },
    Request { status: u16 },
    Server { status: u16 },
    Parse,
    UnsupportedUrl,
}

impl ErrorClass {
    /// Reduce a [`ManagementError`] to its class + numeric status.
    pub fn from_management(err: &ManagementError) -> Self {
        match err {
            ManagementError::Network(_) => ErrorClass::Network,
            ManagementError::Timeout => ErrorClass::Timeout,
            ManagementError::Auth { status, .. } => ErrorClass::Auth { status: *status },
            ManagementError::Request { status, .. } => ErrorClass::Request { status: *status },
            ManagementError::Server { status } => ErrorClass::Server { status: *status },
            ManagementError::Parse(_) => ErrorClass::Parse,
            ManagementError::UnsupportedUrl(_) => ErrorClass::UnsupportedUrl,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorClass::Network => "network",
            ErrorClass::Timeout => "timeout",
            ErrorClass::Auth { .. } => "auth",
            ErrorClass::Request { .. } => "request",
            ErrorClass::Server { .. } => "server",
            ErrorClass::Parse => "parse",
            ErrorClass::UnsupportedUrl => "unsupported_url",
        }
    }

    /// The preserved gRPC/HTTP numeric status (0 when the class has none).
    pub fn status_code(&self) -> u16 {
        match self {
            ErrorClass::Auth { status }
            | ErrorClass::Request { status }
            | ErrorClass::Server { status } => *status,
            _ => 0,
        }
    }

    /// JSON object: `{"class":"...","status":N}`.
    pub fn to_json(&self) -> String {
        format!("{{\"class\":\"{}\",\"status\":{}}}", self.as_str(), self.status_code())
    }
}

// ---------------------------------------------------------------------------
// session deadline (3-state, upstream ApplySessionDeadline semantics)
// ---------------------------------------------------------------------------

/// Anchored SSO session deadline, 3-state exactly like the wire field
/// (`LoginResponse.sessionExpiresAt` / `SyncResponse.sessionExpiresAt`,
/// management.proto L288-291; upstream application at
/// engine_authsession.go:17-62): `Unknown` = never anchored / snapshot
/// unset → KEEP the previous value; `Disabled` = explicit zero → expiry
/// disabled / not SSO; `At(t)` = absolute deadline in unix seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionDeadline {
    Unknown,
    Disabled,
    At(i64),
}

impl SessionDeadline {
    /// Apply one wire observation (3-state). `None` keeps the current value.
    pub fn apply_wire(&mut self, wire: Option<i64>) {
        *self = match wire {
            None => *self, // unset → keep (no-op)
            Some(0) => SessionDeadline::Disabled,
            Some(t) => SessionDeadline::At(t),
        };
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SessionDeadline::Unknown => "unknown",
            SessionDeadline::Disabled => "disabled",
            SessionDeadline::At(_) => "set",
        }
    }
}

// ---------------------------------------------------------------------------
// data-plane seams
// ---------------------------------------------------------------------------

/// One WireGuard peer registration derived from a NetworkMap remote peer:
/// public key (base64, as sent) + allowed_ips (masked IPv4 prefixes).
/// NOTE: this is LOCAL configuration only — without signal/ICE there is no
/// endpoint and no tunnel (module limitation statement; N5c adds per-peer
/// endpoints through [`WgPeerApplier::apply_endpoint`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgPeerEntry {
    pub pub_key_b64: String,
    pub allowed_ips: Vec<config::Route>,
}

/// WG data-plane seam: register/replace the local WireGuard peer set.
/// **N7 production default: [`crate::wg_device::WgDeviceFeed`]** (the
/// shell-fed real device). [`WgPeerRegistry`] stays as the test/reference
/// seam; [`crate::wg_device::WgDeviceApplier`] is the direct (pre-fed)
/// device driver for host tests.
pub trait WgPeerApplier: Send + Sync + 'static {
    /// Replace the registered peer set with `peers` (full snapshot
    /// semantics: the legacy wire format this client consumes carries full
    /// NetworkMap snapshots).
    fn apply_peers(&self, peers: &[WgPeerEntry]) -> Result<(), String>;
    /// Tear down local registration (connector stop).
    fn clear(&self);
    /// N3-7 default-route gate input: can the WG data plane actually carry
    /// traffic RIGHT NOW (tunnel device up + workable handshake state)?
    /// The registry default is ALWAYS false — with no shell-fed WG socket
    /// there are no handshakes. N6: [`crate::wg_device::WgDeviceApplier`]
    /// replaces the approximation with the REAL session state (an
    /// established, non-expired WG session on at least one peer —
    /// `crate::wg_device` module docs); a registered peer alone must never
    /// flip the default route on.
    fn tunnel_ready(&self) -> bool {
        false
    }
    /// N5c: configure the endpoint of one registered peer from its ICE
    /// selected pair (upstream `ConfigureWGEndpoint`, `conn.go:444-478`,
    /// driven by `worker_ice.go:293`). Default: UNSUPPORTED and loud —
    /// a seam that cannot land endpoints must fail the peer's reachability,
    /// never pretend the pair landed (fail-closed).
    fn apply_endpoint(&self, _pub_key_b64: &str, _addr: [u8; 4], _port: u16) -> Result<(), String> {
        Err("wg-endpoint-unsupported".to_string())
    }
    /// N7: the real data-plane status behind `connector_status()`'s `wg`
    /// field (feed/device/ready + REAL device counters). `None` = the seam
    /// has no device capability at all (e.g. the test registry); the status
    /// renders the all-false default in that case, so the observation stays
    /// honest either way.
    fn dataplane_status(&self) -> Option<crate::wg_device::WgDataplaneStatus> {
        None
    }
    /// N11: attach the peer's egress socket — a dup of the ICE-selected
    /// pair's LOCAL socket — so WG data leaves via the selected transport
    /// (upstream: WG and ICE share the selected UDP path, demuxed by
    /// `client/iface/bind/ice_bind.go`; the remote address is read off that
    /// transport at `conn.go:453-460`). The raw number is BORROWED
    /// (dup-only; the original belongs to its provider/session). Default:
    /// UNSUPPORTED and loud — a seam that cannot ride the selected socket
    /// must fail the peer's reachability, never fake it (fail-closed).
    fn attach_egress_socket(&self, _pub_key_b64: &str, _raw_fd: i32) -> Result<(), String> {
        Err("wg-egress-unsupported".to_string())
    }
    /// N11: feed one demuxed (non-STUN) datagram into the WG data plane —
    /// the device's source-match rules apply unchanged (upstream: the
    /// shared receive loop hands non-STUN packets to WG,
    /// `ice_bind.go:279-303`). Returns datagrams produced in reply.
    fn handle_udp_inbound(&self, _datagram: &[u8], _src: ([u8; 4], u16), _now_ms: u64) -> usize {
        0
    }
    /// N11: recycle the peer's WG endpoint + egress after ICE teardown
    /// (upstream `RemoveEndpointAddress`, `conn.go:531`) — afterwards the
    /// device must not send anything for the peer (fail-closed; no silent
    /// riding of a dead path).
    fn recycle_endpoint(&self, _pub_key_b64: &str) {}
    /// N2 T5a (§4 B column, Wb/Ws row): drop EVERY peer's currently selected
    /// WG path — endpoint cleared, the selected egress duplicate closed, the
    /// handshake campaign reset — so the next ICE nomination re-attaches a
    /// fresh selected socket and re-handshakes over it. Returns how many peers
    /// were acted on; `0` = nothing to break (or a seam that cannot drop a
    /// path — a seam must never pretend it broke one).
    fn reset_peer_paths(&self) -> usize {
        0
    }

    /// N13-D2: attach the peer's NON-UDP egress carrier — the relay bearer
    /// (upstream wgProxy equivalent). The carrier is a pure Rust handle
    /// (`crate::relay_client::RelayWgCarrier` → `send_to_peer`); no fd
    /// crosses this seam (fd contract: the relay WSS socket is native-owned
    /// by the relay client; the platform TUN raw fd stays exclusively the
    /// shell's, closed only by `VpnConnection.destroy()`). Default:
    /// UNSUPPORTED and loud — a seam that cannot host a lane must fail the
    /// relay carrier attach, never fake it (fail-closed, mirrors
    /// `attach_egress_socket`).
    fn attach_carrier(
        &self,
        _pub_key_b64: &str,
        _carrier: std::sync::Arc<dyn crate::wg_device::WgEgressCarrier>,
    ) -> Result<(), String> {
        Err("wg-carrier-unsupported".to_string())
    }
    /// N13-D2: detach the peer's relay carrier (relay revoked/unavailable,
    /// connector teardown). After this the peer's only remaining path is a
    /// nominated ICE endpoint — or none (fail-closed).
    fn detach_carrier(&self, _pub_key_b64: &str) {}
    /// N13-D2: detach EVERY relay carrier (relay removed from sync /
    /// connector worker end). No-op by default.
    fn detach_all_carriers(&self) {}
    /// N13-D2: can this seam host relay lanes at all? The connector's
    /// carrier pump only starts (and only opens relay lanes) for capable
    /// seams — a registry-style test seam must never cause relay
    /// subscriptions it cannot use.
    fn carrier_capable(&self) -> bool {
        false
    }
    /// N13-D2: feed one datagram that arrived over the relay carrier —
    /// the device's carrier-match rules apply (sender key authoritative;
    /// `crate::wg_device` module docs「入向匹配映射」). Returns datagrams
    /// produced in reply.
    fn handle_carrier_inbound(&self, _datagram: &[u8], _pub_key_b64: &str, _now_ms: u64) -> usize {
        0
    }
}

/// In-process WG peer registry — the N3-5..N6 production default, kept as
/// the TEST/REFERENCE [`WgPeerApplier`] since N7 (the production default is
/// now the shell-fed [`crate::wg_device::WgDeviceFeed`]). Honest scope:
/// local registration + (N5c) per-peer endpoint records + `tunnel_ready()`
/// always false (no handshakes can exist here — keep the gate fail-closed).
#[derive(Debug, Default)]
pub struct WgPeerRegistry {
    peers: Mutex<Vec<WgPeerEntry>>,
    /// N5c: per-peer endpoint landed from the ICE selected pair.
    endpoints: Mutex<Vec<(String, [u8; 4], u16)>>,
}

impl WgPeerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Current registered peer set (snapshot).
    pub fn snapshot(&self) -> Vec<WgPeerEntry> {
        self.peers.lock_poison().clone()
    }

    pub fn len(&self) -> usize {
        self.peers.lock_poison().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// N5c: the endpoint currently configured for `pub_key_b64`, if any.
    pub fn endpoint(&self, pub_key_b64: &str) -> Option<([u8; 4], u16)> {
        self.endpoints
            .lock_poison()
            .iter()
            .find(|(k, _, _)| k == pub_key_b64)
            .map(|(_, a, p)| (*a, *p))
    }

    /// N5c: all landed endpoints (snapshot; latest record per peer wins).
    pub fn endpoints(&self) -> Vec<(String, [u8; 4], u16)> {
        self.endpoints.lock_poison().clone()
    }
}

impl WgPeerApplier for WgPeerRegistry {
    fn apply_peers(&self, peers: &[WgPeerEntry]) -> Result<(), String> {
        *self.peers.lock_poison() = peers.to_vec();
        Ok(())
    }

    fn clear(&self) {
        self.peers.lock_poison().clear();
        self.endpoints.lock_poison().clear();
    }

    /// N5c: record/replace the endpoint for the peer. Unknown peers are
    /// REJECTED (a WG endpoint for an unregistered peer is a config bug,
    /// not something to silently store).
    fn apply_endpoint(&self, pub_key_b64: &str, addr: [u8; 4], port: u16) -> Result<(), String> {
        let registered = self.peers.lock_poison().iter().any(|p| p.pub_key_b64 == pub_key_b64);
        if !registered {
            return Err(format!("peer '{pub_key_b64}' is not registered"));
        }
        let mut eps = self.endpoints.lock_poison();
        if let Some(slot) = eps.iter_mut().find(|(k, _, _)| k == pub_key_b64) {
            *slot = (pub_key_b64.to_string(), addr, port);
        } else {
            eps.push((pub_key_b64.to_string(), addr, port));
        }
        Ok(())
    }

    /// N11: drop the peer's endpoint record (no egress/transport exists on
    /// the registry seam — the honest mirror of the device-side recycle).
    fn recycle_endpoint(&self, pub_key_b64: &str) {
        self.endpoints.lock_poison().retain(|(k, _, _)| k != pub_key_b64);
    }

    /// N2 T5a: the registry seam has no sockets to close; dropping every
    /// recorded endpoint is the honest whole of what it can do. Counts the
    /// peers that actually had one.
    fn reset_peer_paths(&self) -> usize {
        let mut endpoints = self.endpoints.lock_poison();
        let acted = endpoints.len();
        endpoints.clear();
        acted
    }
}

/// Shell-side (host) configuration seam: the parts of a NetworkMap this
/// crate cannot apply itself — managed routes, DNS config and our own
/// address — are handed to the host here. The host decides what to do with
/// them (system route table / DNS settings on HarmonyOS are shell-side).
pub trait ConfigApplier: Send + Sync + 'static {
    /// One applied NetworkMap snapshot (routes + DNS + own address
    /// included).
    fn apply(&self, map: &NetworkMap);
    /// Connector stop: the host may tear down what it applied.
    fn clear(&self);
}

/// Production-default [`ConfigApplier`]: emits count-only hilog markers.
/// The real HarmonyOS shell applier (ArkTS side) is a later increment; until
/// then routes/DNS are NOT applied anywhere — they are only visible in the
/// connector status counts.
#[derive(Debug, Default)]
pub struct LoggingConfigApplier;

impl ConfigApplier for LoggingConfigApplier {
    fn apply(&self, map: &NetworkMap) {
        hilog::emit(&format!(
            "connector: host-config routes={} dns_groups={} own_addr={}",
            map.routes.len(),
            map.dns.as_ref().map(|d| d.name_server_groups.len()).unwrap_or(0),
            map.peer.as_ref().and_then(|p| p.address.clone()).is_some(),
        ));
    }

    fn clear(&self) {
        hilog::emit("connector: host-config cleared");
    }
}

// ---------------------------------------------------------------------------
// management seam
// ---------------------------------------------------------------------------

/// Management connection seam. Production: [`GrpcManagementFactory`] (a
/// fresh tonic channel per attempt — upstream's retry loop also wraps the
/// dial, grpc.go:224-275). Tests may stub it.
pub trait ManagementFactory: Send + Sync + 'static {
    fn connect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<ManagementGrpcClient, ManagementError>> + Send + '_>>;
    /// The address EVERY dial of this factory connects to, when the factory
    /// has one fixed address — the N3-7 protected-socket path, where the shell
    /// resolved the management host and core dials exactly that address.
    /// `None` = no fixed dial address (the unprotected opt-in dev dial, or a
    /// test stub): the N2-H collector then records a reasoned freeze gap
    /// instead of inventing an address. Diagnostics only — this never dials.
    /// Update the protected dial target after shell-side re-resolution.
    fn update_dial_address(&self, _addr: SocketAddr) {}
    fn dial_address(&self) -> Option<SocketAddr> {
        None
    }
}

/// Production factory: a fresh [`ManagementGrpcClient`] per call, TLS with
/// the caller-injected CA roots for `https://`, plaintext for `http://`
/// (mismatch rules enforced by
/// [`crate::grpc::ManagementGrpcClient::connect`]). N3-7: with a socket
/// source set, EVERY dial runs through a freshly-taken protected socket
/// (`crate::grpc::ManagementGrpcClient::connect_with_socket_source`) —
/// fail-closed when the source cannot hand out a socket.
#[derive(Clone)]
pub struct GrpcManagementFactory {
    endpoint: String,
    transport: GrpcTransport,
    connect_timeout: Duration,
    request_timeout: Duration,
    keys: EnvelopeKeyPair,
    /// `(protected socket source, shell-resolved connect address)`. `None`
    /// means the (DANGEROUS, opt-in) unprotected direct dial.
    socket: Option<(Arc<dyn ManagementSocketProvider>, Arc<Mutex<SocketAddr>>)>,
}

impl core::fmt::Debug for GrpcManagementFactory {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // secret-free shape: envelope keys Debug prints the public key only
        // (crate::envelope); the socket source is a count-free seam handle.
        f.debug_struct("GrpcManagementFactory")
            .field("endpoint", &self.endpoint)
            .field("connect_timeout", &self.connect_timeout)
            .field("request_timeout", &self.request_timeout)
            .field("protected_socket", &self.socket.is_some())
            .finish()
    }
}

impl GrpcManagementFactory {
    /// Build from a validated [`ConnectorConfig`] + identity keys.
    pub fn new(config: &ConnectorConfig, keys: EnvelopeKeyPair) -> Self {
        GrpcManagementFactory {
            endpoint: config.management_url.clone(),
            transport: config.transport.clone(),
            connect_timeout: config.connect_timeout,
            request_timeout: config.request_timeout,
            keys,
            socket: None,
        }
    }

    /// N3-7: protected-socket variant — every dial takes a fresh protected
    /// socket from `source` and connects it to `connect_addr` (DNS resolved
    /// shell-side; TLS still verifies the endpoint URL host).
    pub fn with_socket_source(
        config: &ConnectorConfig,
        keys: EnvelopeKeyPair,
        source: Arc<dyn ManagementSocketProvider>,
        connect_addr: SocketAddr,
    ) -> Self {
        GrpcManagementFactory {
            endpoint: config.management_url.clone(),
            transport: config.transport.clone(),
            connect_timeout: config.connect_timeout,
            request_timeout: config.request_timeout,
            keys,
            socket: Some((source, Arc::new(Mutex::new(connect_addr)))),
        }
    }
}

impl ManagementFactory for GrpcManagementFactory {
    /// N2-H (design §4.0 management row): the shell-resolved dial address —
    /// `socket.1` of the protected-socket variant, i.e. the address core
    /// actually connects to on every (re)dial. `None` on the unprotected
    /// dev dial, whose address core resolves itself.
    fn update_dial_address(&self, addr: SocketAddr) {
        if let Some((_, target)) = &self.socket { *target.lock_poison() = addr; }
    }

    fn dial_address(&self) -> Option<SocketAddr> {
        self.socket.as_ref().map(|(_source, addr)| *addr.lock_poison())
    }

    fn connect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<ManagementGrpcClient, ManagementError>> + Send + '_>>
    {
        let endpoint = self.endpoint.clone();
        let transport = self.transport.clone();
        let keys = self.keys.clone();
        let connect_timeout = self.connect_timeout;
        let request_timeout = self.request_timeout;
        let socket = self.socket.clone();
        Box::pin(async move {
            match socket {
                Some((source, connect_addr)) => {
                    let target = *connect_addr.lock_poison();
                    ManagementGrpcClient::connect_with_socket_source(
                        &endpoint,
                        transport,
                        connect_timeout,
                        request_timeout,
                        keys,
                        source,
                        target,
                    )
                    .await
                }
                None => {
                    ManagementGrpcClient::connect(
                        &endpoint,
                        transport,
                        connect_timeout,
                        request_timeout,
                        keys,
                    )
                    .await
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// configuration + credentials (parsed from the NAPI JSON args)
// ---------------------------------------------------------------------------

/// Session-renewal defaults: lead = upstream interactive warning lead
/// (sessionwatch/watcher.go:34-37); check interval bounds renewal latency.
pub const DEFAULT_RENEW_LEAD: Duration = Duration::from_secs(10 * 60);
pub const DEFAULT_RENEW_CHECK_INTERVAL: Duration = Duration::from_secs(30);
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Connector configuration parsed from the `connector_start(configJson)`
/// argument. Holds NO secret material in raw form: the device private key
/// is reduced to an [`EnvelopeKeyPair`] at parse time (its `Debug` exposes
/// the public key only — `crate::envelope`).
#[derive(Debug, Clone)]
pub struct ConnectorConfig {
    pub management_url: String,
    pub transport: GrpcTransport,
    pub keys: EnvelopeKeyPair,
    pub meta: PeerMeta,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    /// Extend the SSO session when remaining lifetime drops below this.
    pub renew_lead: Duration,
    /// How often the renewal task re-checks the deadline.
    pub renew_check_interval: Duration,
    /// N3-7: dev opt-in — export the managed default route even when the
    /// data plane cannot carry traffic (traffic-black-hole risk acknowledged
    /// by the caller). Default FALSE; honored state is logged loudly.
    pub force_default_route: bool,
    /// N3-7: dev opt-in — permit an UNPROTECTED direct management dial
    /// (`connector_start` without a protected socket). NOT upstream
    /// behavior; the production path is `connector_start_with_socket`.
    pub allow_unprotected_management: bool,
    /// **HOST-ONLY（N12a）**：主机联调 CLI 的固定 ICE 端口（`--ice-port` /
    /// 配置字段 `ice_fixed_port`）。`0` = 临时端口（默认；设备壳永不携带
    /// 该字段，设备路径行为逐字节不变）。不新建任何 socket——只改变
    /// seam 所供 UDP socket 的 bind() 端口。
    pub ice_fixed_port: u16,
    /// **HOST-ONLY（N12a）**：显式对外可达候选（`--advertise-candidate` /
    /// 配置字段 `advertised_candidates`），额外的 host 型候选经既有 signal
    /// 路径发给对端。默认为空 = 不通告（设备路径不变）。
    pub advertised_candidates: Vec<crate::ice::Candidate>,
    /// **N13-D1（默认 FALSE，硬要求）**：接入 relay 客户端（`rel://`/`rels://`
    /// 的 NetBird relay，`NetbirdConfig.relay`）。关闭时连接器行为与本增量
    /// 之前完全一致（不启动 relay 客户端、状态 `relay.enabled:false`、不拨
    /// 任何 relay 连接）。
    ///
    /// 理由（T0 Q3 / `docs/n13-relay-increment-plan-20260914.md` §5）：默认
    /// 路由下启用 WSS 前，必须先把 relay 的 **TCP** 端点纳入 N2-H 冻结并排
    /// 除——那属于增量 D2 与设备侧工作，本期不得在设备路径上默认启用。
    pub relay_enabled: bool,
}

impl ConnectorConfig {
    /// Parse + validate the `configJson` document.
    ///
    /// Shape (snake_case, matching `config.rs` conventions):
    ///
    /// ```json
    /// {
    ///   "management_url": "https://mgmt.example:443",
    ///   "ca_pem": "-----BEGIN CERTIFICATE-----\n...",
    ///   "server_name": "optional SNI override",
    ///   "private_key": "<base64 std, 32 bytes>",
    ///   "hostname": "ohos-device",
    ///   "os_name": "harmonyos", "os_version": "5.0.0",
    ///   "netbird_version": "0.1.0",
    ///   "connect_timeout_ms": 5000, "request_timeout_ms": 10000,
    ///   "session_renew_lead_ms": 600000, "renew_check_interval_ms": 30000,
    ///   "force_default_route": false,
    ///   "allow_unprotected_management": false,
    ///   "relay_enabled": false,
    ///   "ice_fixed_port": 0,
    ///   "advertised_candidates": []
    /// }
    /// ```
    ///
    /// `ca_pem` (string or array of strings) is REQUIRED for `https://`
    /// (no system store is used); it is ignored for `http://` (plaintext,
    /// test only). Unknown fields are ignored.
    ///
    /// `force_default_route` (N3-7, default FALSE): DEVELOPMENT opt-in to
    /// export the managed default route while the data plane is not ready —
    /// that combination is a traffic black hole, so the default is HOLD.
    ///
    /// `allow_unprotected_management` (N3-7, default FALSE): DEVELOPMENT
    /// opt-in for an UNPROTECTED direct management dial via
    /// `connector_start`. NOT upstream behavior; default is REFUSE.
    ///
    /// `relay_enabled` (N13-D1, default FALSE): opt-in to run the relay
    /// client against `NetbirdConfig.relay` from every Sync. The relay TLS
    /// trust root is the SAME `ca_pem` injected here (management
    /// consistency); a `rels://` URL with no usable root is refused typed
    /// (never plaintext-downgraded). With the key absent the device path is
    /// byte-identical to the pre-N13 connector.
    ///
    /// `ice_fixed_port` / `advertised_candidates` (N12a, defaults `0` /
    /// `[]`): **HOST-ONLY** interop tuning for the port-mapping scenario —
    /// the host CLI pins the peer's UDP port and advertises an externally
    /// reachable candidate. Never set by the device shell; absent keys
    /// leave the device path byte-identical (and no socket is created by
    /// either knob — see [`HostIceTuning`]).
    pub fn from_json(text: &str) -> Result<ConnectorConfig, ConfigError> {
        let doc = config::parse_document(text)?;
        let entries = match doc {
            Json::Obj(entries) => entries,
            _ => {
                return Err(ConfigError::Field {
                    field: "(root)",
                    reason: "expected a JSON object".into(),
                })
            }
        };

        let mut management_url: Option<String> = None;
        let mut ca_pem: Vec<Vec<u8>> = Vec::new();
        let mut server_name: Option<String> = None;
        let mut private_key: Option<[u8; 32]> = None;
        let mut hostname = "netbird-ohos".to_string();
        let mut os_name = "harmonyos".to_string();
        let mut os_version = "unknown".to_string();
        let mut netbird_version = "0.1.0".to_string();
        let mut connect_timeout = DEFAULT_CONNECT_TIMEOUT;
        let mut request_timeout = DEFAULT_REQUEST_TIMEOUT;
        let mut renew_lead = DEFAULT_RENEW_LEAD;
        let mut renew_check_interval = DEFAULT_RENEW_CHECK_INTERVAL;
        let mut force_default_route = false;
        let mut allow_unprotected_management = false;
        let mut ice_fixed_port: u16 = 0;
        let mut advertised_candidates: Vec<crate::ice::Candidate> = Vec::new();
        let mut relay_enabled = false;

        for (key, val) in &entries {
            match key.as_str() {
                "management_url" => management_url = Some(field_str(val, "management_url")?.to_string()),
                "ca_pem" => {
                    ca_pem = match val {
                        Json::Str(pem) => vec![pem.as_bytes().to_vec()],
                        Json::Arr(items) => {
                            let mut out = Vec::with_capacity(items.len());
                            for item in items {
                                out.push(field_str(item, "ca_pem")?.as_bytes().to_vec());
                            }
                            out
                        }
                        _ => {
                            return Err(ConfigError::Field {
                                field: "ca_pem",
                                reason: "expected a PEM string or array of strings".into(),
                            })
                        }
                    };
                }
                "server_name" => server_name = Some(field_str(val, "server_name")?.to_string()),
                "private_key" => {
                    let s = field_str(val, "private_key")?;
                    let raw = base64::engine::general_purpose::STANDARD
                        .decode(s.trim())
                        .map_err(|_| ConfigError::Field {
                            field: "private_key",
                            reason: "not valid base64".into(),
                        })?;
                    let bytes: [u8; 32] = raw.try_into().map_err(|v: Vec<u8>| {
                        ConfigError::Field {
                            field: "private_key",
                            reason: format!("expected 32 bytes, got {}", v.len()),
                        }
                    })?;
                    private_key = Some(bytes);
                }
                "hostname" => hostname = field_str(val, "hostname")?.to_string(),
                "os_name" => os_name = field_str(val, "os_name")?.to_string(),
                "os_version" => os_version = field_str(val, "os_version")?.to_string(),
                "netbird_version" => netbird_version = field_str(val, "netbird_version")?.to_string(),
                "connect_timeout_ms" => {
                    connect_timeout = Duration::from_millis(field_u64(val, "connect_timeout_ms")?)
                }
                "request_timeout_ms" => {
                    request_timeout = Duration::from_millis(field_u64(val, "request_timeout_ms")?)
                }
                "session_renew_lead_ms" => {
                    renew_lead = Duration::from_millis(field_u64(val, "session_renew_lead_ms")?)
                }
                "renew_check_interval_ms" => {
                    renew_check_interval =
                        Duration::from_millis(field_u64(val, "renew_check_interval_ms")?)
                }
                "force_default_route" => {
                    force_default_route = field_bool(val, "force_default_route")?
                }
                "allow_unprotected_management" => {
                    allow_unprotected_management =
                        field_bool(val, "allow_unprotected_management")?
                }
                // N13-D1: relay client opt-in (DEFAULT FALSE — device path
                // unchanged; see the field doc for the T0 Q3 rationale).
                "relay_enabled" => relay_enabled = field_bool(val, "relay_enabled")?,
                // N12a HOST-ONLY tuning (never set by the device shell —
                // absent keys keep the device path byte-identical).
                "ice_fixed_port" => {
                    let p = field_u64(val, "ice_fixed_port")?;
                    if p == 0 || p > u16::MAX as u64 {
                        return Err(ConfigError::Field {
                            field: "ice_fixed_port",
                            reason: "must be a UDP port in 1..=65535".into(),
                        });
                    }
                    ice_fixed_port = p as u16;
                }
                "advertised_candidates" => {
                    let items = match val {
                        Json::Arr(items) => items,
                        _ => {
                            return Err(ConfigError::Field {
                                field: "advertised_candidates",
                                reason: "expected an array of \"ip:port\" strings".into(),
                            })
                        }
                    };
                    for item in items {
                        let s = field_str(item, "advertised_candidates")?;
                        match crate::ice::parse_advertised_candidate(s) {
                            Ok(c) => advertised_candidates.push(c),
                            Err(e) => {
                                return Err(ConfigError::Field {
                                    field: "advertised_candidates",
                                    reason: format!("{e}"),
                                })
                            }
                        }
                    }
                }
                _ => {} // unknown fields ignored
            }
        }

        let management_url = management_url.ok_or_else(|| ConfigError::Field {
            field: "management_url",
            reason: "missing field".into(),
        })?;
        let private_key = private_key.ok_or_else(|| ConfigError::Field {
            field: "private_key",
            reason: "missing field".into(),
        })?;

        // Transport pre-flight: https needs an injected trust root (no
        // system store — see crate::grpc module docs); http is plaintext.
        let transport = if management_url.starts_with("https://") {
            if ca_pem.is_empty() {
                return Err(ConfigError::Field {
                    field: "ca_pem",
                    reason: "https:// endpoint requires an injected CA PEM (no system store is used)"
                        .into(),
                });
            }
            let mut tls = crate::grpc::GrpcTlsConfig::new(ca_pem);
            if let Some(name) = server_name {
                tls = tls.with_server_name(name);
            }
            GrpcTransport::Tls(tls)
        } else {
            GrpcTransport::Plaintext
        };

        Ok(ConnectorConfig {
            management_url,
            transport,
            keys: EnvelopeKeyPair::from_secret_bytes(&private_key),
            meta: PeerMeta { hostname, os_name, os_version, netbird_version },
            connect_timeout,
            request_timeout,
            renew_lead,
            renew_check_interval,
            force_default_route,
            allow_unprotected_management,
            ice_fixed_port,
            advertised_candidates,
            relay_enabled,
        })
    }
}

/// N12a HOST-ONLY interop tuning carried into the production orchestrator
/// (`ConnectorHandle::spawn`; built from [`ConnectorConfig`] at the start
/// seams). Every device config leaves both fields at their defaults
/// (`0` / empty), which keeps the device path byte-identical: no extra
/// socket is created anywhere — the fixed port only changes the `bind()`
/// of a seam-provided UDP socket, and advertised candidates are extra
/// SIGNAL entries, not sockets. See
/// `docs/self-hosted-interop-plan.md` §端口映射 / 对外候选.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostIceTuning {
    /// `0` = ephemeral (default).
    pub ice_fixed_port: u16,
    /// Extra host-type candidates signaled verbatim.
    pub advertised_candidates: Vec<crate::ice::Candidate>,
}

impl HostIceTuning {
    /// The defaults (`0` / empty): the exact pre-N12a behavior.
    pub fn is_default(&self) -> bool {
        self.ice_fixed_port == 0 && self.advertised_candidates.is_empty()
    }

    /// Extract the HOST-ONLY tuning from a parsed config (the start seams'
    /// single construction point).
    pub fn from_config(cfg: &ConnectorConfig) -> Self {
        HostIceTuning {
            ice_fixed_port: cfg.ice_fixed_port,
            advertised_candidates: cfg.advertised_candidates.clone(),
        }
    }
}

/// N13-D1: injected seams for the relay client a connector runs. The same
/// pattern as [`SignalMaterial`]: production builds one value at the start
/// seams ([`RelayMaterials::from_connector_transport`]); tests inject a
/// virtual clock / scripted dialer / no-op TLS connector.
#[derive(Clone)]
pub struct RelayMaterials {
    /// Time source for token expiry and every relay deadline (injected in
    /// tests; [`SystemClock`] in production).
    pub clock: Arc<dyn crate::relay_client::RelayClock>,
    /// Transport seam ([`crate::relay_client::ProtectedRelayDialer`] in
    /// production; host tests may explicitly inject a bare TCP dialer).
    pub dialer: Arc<dyn crate::relay_client::RelayDialer>,
    /// TLS connector for `rels://`. `None` = `rels://` is refused typed
    /// (`TlsRequired`) BEFORE any wire byte — never plaintext-downgraded.
    pub tls: Option<Arc<dyn crate::relay_client::RelayTlsConnector>>,
}

impl RelayMaterials {
    /// Production materials: system clock, per-dial protected TCP socket and
    /// — when the management transport carries an injected CA (`ca_pem`) —
    /// a rustls connector trusting exactly those roots. Without a socket
    /// supply, the relay dialer refuses to connect.
    pub fn from_connector_transport(
        transport: &GrpcTransport,
        supply: Option<Arc<crate::socket_supply::SocketSupply>>,
    ) -> Self {
        let tls = match transport {
            GrpcTransport::Tls(tls_cfg) => {
                // A root set the TLS connector cannot use (empty/unparseable
                // PEM) fails CLOSED here — `tls: None` means every `rels://`
                // dial is later refused typed (`TlsRequired`), never
                // plaintext-downgraded. No panic crosses the NAPI boundary.
                match crate::relay_client::RustlsRelayTlsConnector::new(
                    tls_cfg.root_certs_pem.clone(),
                ) {
                    Ok(connector) => {
                        Some(Arc::new(connector) as Arc<dyn crate::relay_client::RelayTlsConnector>)
                    }
                    Err(_) => None,
                }
            }
            GrpcTransport::Plaintext => None,
        };
        RelayMaterials {
            clock: Arc::new(crate::relay_client::SystemClock),
            dialer: Arc::new(crate::relay_client::ProtectedRelayDialer::new(supply)),
            tls,
        }
    }
}

impl core::fmt::Debug for RelayMaterials {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Shape only: seams are trait objects and carry no secret material.
        f.debug_struct("RelayMaterials")
            .field("clock", &"injected")
            .field("dialer", &"injected")
            .field("tls", &self.tls.is_some())
            .finish()
    }
}

/// N13-D1: everything a spawned connector needs to run the relay client —
/// the LOCAL peer's WireGuard public key (relay peerID source, relay spec
/// §3.2) and the injected seams. Built only when `relay_enabled` is set.
#[derive(Clone, Debug)]
pub struct RelayAttach {
    pub wg_pubkey_b64: String,
    pub materials: Arc<RelayMaterials>,
}

/// Login credentials parsed from the optional `setupKeyJson` argument.
/// Manual `Debug`: both fields are secrets and must never format through.
#[derive(Clone, Default)]
pub struct ConnectorSecrets {
    /// Pre-authorized setup key (empty when absent).
    pub setup_key: String,
    /// SSO JWT token (empty when absent).
    pub jwt: String,
}

impl ConnectorSecrets {
    /// Parse `{"setup_key": "...", "jwt": "..."}`; at least one non-empty
    /// value is required (`crate::grpc` login pre-flight has the same rule).
    pub fn from_json(text: &str) -> Result<ConnectorSecrets, ConfigError> {
        let doc = config::parse_document(text)?;
        let entries = match doc {
            Json::Obj(entries) => entries,
            _ => {
                return Err(ConfigError::Field {
                    field: "(root)",
                    reason: "expected a JSON object".into(),
                })
            }
        };
        let mut secrets = ConnectorSecrets::default();
        for (key, val) in &entries {
            match key.as_str() {
                "setup_key" => secrets.setup_key = field_str(val, "setup_key")?.to_string(),
                "jwt" | "jwt_token" => secrets.jwt = field_str(val, "jwt")?.to_string(),
                _ => {}
            }
        }
        if secrets.setup_key.is_empty() && secrets.jwt.is_empty() {
            return Err(ConfigError::Field {
                field: "setup_key",
                reason: "at least one of setup_key / jwt is required".into(),
            });
        }
        Ok(secrets)
    }
}

// Debug redaction: never print secret material (credential discipline).
impl core::fmt::Debug for ConnectorSecrets {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConnectorSecrets")
            .field("setup_key", &"REDACTED")
            .field("jwt", &"REDACTED")
            .finish()
    }
}

// ---------------------------------------------------------------------------
// shared state + status
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct StateInner {
    machine: StateMachine,
    last_error: Option<ErrorClass>,
    last_update_unix: Option<i64>,
    last_serial: u64,
    peer_count: usize,
    route_count: usize,
    deadline: SessionDeadline,
    reconnects: u64,
    renew_attempts: u64,
    wg_apply_failed: bool,
    wg_apply_errors: u64,
    logout_ok: Option<bool>,
    started_at_unix: Option<i64>,
    /// Last applied shell network-config snapshot (N3-6); `None` until the
    /// first NetworkMap arrives, reset on stop.
    net_config: Option<ShellNetworkConfig>,
    /// N7: the UNGATED route list of the last applied map (same rendering as
    /// the snapshot's routes). `refresh_net_gate` rebuilds the snapshot's
    /// gated `routes` from here when live data-plane readiness moves the
    /// default-route decision between reads.
    net_routes_all: Vec<ShellRouteEntry>,
    /// N3-7: the (dev opt-in) force flag for the default-route gate.
    force_default_route: bool,
    /// N8: the route set the shell last reported as APPLIED to the platform
    /// VpnConfig at its (re)create() time (`None` = the shell never ACKed —
    /// e.g. an older shell; recreate signaling stays silent in that case).
    applied_route_set: Option<Vec<String>>,
    /// N8: monotonic anchor of the last COMPLETED recreate (cooldown).
    last_recreate_ms: Option<u64>,
    /// N8: the controlled-recreate state machine (required/count/exhausted).
    recreate: RecreateStatus,
}

/// State shared between the connector worker tasks and the status/stop
/// entry points. All mutation is mutex-guarded; the six-state machine is
/// `crate::state::StateMachine` driven through guarded (legal-only)
/// transitions.
struct ConnectorShared {
    inner: Mutex<StateInner>,
    running: AtomicBool,
    /// N5c: per-peer ICE orchestrator (set once at spawn; unit tests that
    /// build `ConnectorShared::new` directly simply run without ICE).
    ice: std::sync::OnceLock<Arc<Mutex<PeerIceOrchestrator>>>,
    /// N5c: the protected-UDP source behind the ICE orchestrator (resupply
    /// handle for `connector_ice_socket_feed`).
    ice_sockets: std::sync::OnceLock<Arc<crate::ice::ProtectedUdpFdSource>>,
    /// N5d: the real signal link runtime (set once at spawn when
    /// [`SignalMaterial`] was provided; unit tests that build
    /// `ConnectorShared::new` directly simply run without signal).
    signal: std::sync::OnceLock<Arc<SignalRuntime>>,
    /// N13-D1: relay opt-in — `false` keeps the device path relay-free
    /// (hard default; the T0 Q3/D2 rationale is on [`ConnectorConfig`]).
    relay_enabled: bool,
    /// N13-D1: LOCAL WG public key + injected seams (set at spawn; `None`
    /// = relay never attachable).
    relay_attach: Option<RelayAttach>,
    /// N13-D1: the live relay client slot. `client` is started by the first
    /// `NetbirdConfig.relay` sync and stays (after `stop_relay` it is kept
    /// only so status can render its terminal `Dead` state — the worker is
    /// gone by then, so it can never be reused). Lock order: `inner` BEFORE
    /// `relay` — never the reverse.
    relay: Mutex<RelaySlot>,
    /// N13-D2: the WG relay-carrier orchestrator (created at the first relay
    /// client start; only when the WG seam is carrier-capable).
    relay_carrier: std::sync::OnceLock<std::sync::Arc<RelayCarrier>>,
    /// N13-D2: the WG seam the carrier lanes attach to (set at spawn, with
    /// the relay opt-in only).
    relay_wg: std::sync::OnceLock<std::sync::Arc<dyn WgPeerApplier>>,
    /// N13-D2: the runtime the carrier pump task spawns on (set at spawn,
    /// with the relay opt-in only).
    relay_runtime: std::sync::OnceLock<tokio::runtime::Handle>,
}

/// N13-D1: relay slot contents (urls = the configured set, public routing
/// material — safe to render into the status document).
#[derive(Default)]
struct RelaySlot {
    client: Option<crate::relay_client::RelayClient>,
    urls: Vec<String>,
    /// Management-ADVERTISED relay URL set (`NetbirdConfig.relay.urls`) from
    /// the last sync carrying a netbird config. Recorded ALWAYS — also when
    /// `relay_enabled:false` — so the shell can learn the relay host and
    /// freeze its N2-H endpoint exclusions BEFORE it enables relay (with the
    /// old record-only-while-running rule the exclusion needed data that was
    /// exposed only after it had to be in place: a circular dependency).
    /// Pure bookkeeping: recording never dials; the client lifecycle stays
    /// gated by `relay_enabled` exactly as before. URLs only — the relay
    /// token never enters this slot (credential discipline).
    advertised_urls: Vec<String>,
}

impl ConnectorShared {
    fn new(force_default_route: bool, relay_attach: Option<RelayAttach>) -> Self {
        let relay_enabled = relay_attach.is_some();
        ConnectorShared {
            inner: Mutex::new(StateInner {
                machine: StateMachine::new(),
                last_error: None,
                last_update_unix: None,
                last_serial: 0,
                peer_count: 0,
                route_count: 0,
                deadline: SessionDeadline::Unknown,
                reconnects: 0,
                renew_attempts: 0,
                wg_apply_failed: false,
                wg_apply_errors: 0,
                logout_ok: None,
                started_at_unix: None,
                net_config: None,
                net_routes_all: Vec::new(),
                force_default_route,
                applied_route_set: None,
                last_recreate_ms: None,
                recreate: RecreateStatus::default(),
            }),
            running: AtomicBool::new(false),
            ice: std::sync::OnceLock::new(),
            ice_sockets: std::sync::OnceLock::new(),
            signal: std::sync::OnceLock::new(),
            relay_enabled,
            relay_attach,
            relay: Mutex::new(RelaySlot::default()),
            relay_carrier: std::sync::OnceLock::new(),
            relay_wg: std::sync::OnceLock::new(),
            relay_runtime: std::sync::OnceLock::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, StateInner> {
        self.inner.lock_poison()
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    fn set_running(&self, v: bool) {
        self.running.store(v, Ordering::Release);
    }

    /// Drive the state machine, ignoring transitions that are illegal from
    /// the current state (e.g. a second `Lost` while already
    /// `Reconnecting`): the machine stays consistent by construction.
    fn transition(&self, event: ConnEvent) {
        let to = {
            let mut g = self.lock();
            g.machine.transition(event).ok()
        };
        if let Some(to) = to {
            hilog::emit(&format!("connector: state -> {}", to.as_str()));
        }
    }

    fn state(&self) -> ConnState {
        self.lock().machine.state()
    }

    /// The `connector_network_config()` payload: the last applied snapshot,
    /// or the explicit no-network-map answer (before the first map / after
    /// stop).
    fn network_config_json(&self) -> String {
        let g = self.lock();
        match g.net_config.as_ref() {
            Some(cfg) => cfg.to_json(),
            None => NO_NETWORK_MAP_JSON.to_string(),
        }
    }

    /// N7: re-decide the snapshot's default-route gate against LIVE
    /// data-plane readiness (`wg.tunnel_ready()` on the device-backed seam ×
    /// the ICE view). The snapshot built at sync time is gated with the
    /// readiness THEN; with feeds/handshakes completing between syncs the
    /// decision can move, and every status / network-config read must
    /// reflect reality (the shell gates the platform config on this). When
    /// the live decision differs, the gate fields AND the exported routes
    /// are rebuilt (the default route is re-exported / re-held from the
    /// ungated record).
    ///
    /// N8: the same read also advances the controlled-recreate state — the
    /// DESIRED route set (gate decision applied to the ungated record) is
    /// compared against the route set the shell last ACKed as applied; a
    /// divergence (either direction: a released default route must be
    /// INSTALLED, a re-held one should be REMOVED to keep the black-hole
    /// rule) raises `recreate.required` for the shell, bounded by
    /// [`RECREATE_MAX`] + [`RECREATE_COOLDOWN_MS`].
    fn refresh_net_gate(&self, wg: &dyn WgPeerApplier) {
        self.refresh_net_gate_at(wg, crate::sys::mono_ms());
    }

    fn refresh_net_gate_at(&self, wg: &dyn WgPeerApplier, now_ms: u64) {
        let mut g = self.lock();
        // disjoint field borrows on the guard data: `net_config.as_mut()`
        // holds the mutable borrow of that field while the decision reads
        // `force_default_route` / `net_routes_all`
        let inner = &mut *g;
        let Some(snap) = inner.net_config.as_mut() else {
            return;
        };
        let ice_ready = match self.ice.get() {
            Some(ice) => ice_ready_for_default_route(&ice.lock_poison().summary()),
            None => true, // no orchestrator (unit-test construction): unchanged rule
        };
        let (allowed, reason) =
            self.decide_default_route(inner.force_default_route, wg, ice_ready, snap.peers.len());
        if allowed != snap.default_route_allowed || reason != snap.default_route_reason {
            hilog::emit(&format!(
                "connector: default-route gate refresh allowed={} reason={}",
                allowed, reason
            ));
            snap.default_route_allowed = allowed;
            snap.default_route_reason = reason.clone();
            snap.routes = inner
                .net_routes_all
                .iter()
                .filter(|r| !r.is_default || allowed)
                .cloned()
                .collect();
        }
        // N8: desired (live-gated) vs applied route set → recreate signal.
        let desired: Vec<String> = inner
            .net_routes_all
            .iter()
            .filter(|r| !r.is_default || allowed)
            .map(|r| r.network.clone())
            .collect();
        inner.recreate.cooling_down = false;
        match inner.applied_route_set.as_ref() {
            // the shell never ACKed an applied set (older shell / initial
            // create still pending): the signal stays silent — N7 behavior
            None => {
                inner.recreate.required = false;
                inner.recreate.reason = "none".to_string();
            }
            Some(applied) if *applied == desired => {
                inner.recreate.required = false;
                inner.recreate.reason = "none".to_string();
            }
            Some(_) => {
                if inner.recreate.exhausted {
                    // budget spent: never again this connector lifetime
                    inner.recreate.required = false;
                    inner.recreate.reason = "limit-reached".to_string();
                } else if inner
                    .last_recreate_ms
                    .map_or(false, |t| now_ms.saturating_sub(t) < RECREATE_COOLDOWN_MS)
                {
                    // parked inside the cooldown; the next periodic read
                    // re-raises it once the window has passed
                    inner.recreate.required = false;
                    inner.recreate.cooling_down = true;
                    inner.recreate.reason = "route-set-changed".to_string();
                } else {
                    if !inner.recreate.required {
                        hilog::emit(
                            "connector: recreate required (desired route set != applied route set)",
                        );
                    }
                    inner.recreate.required = true;
                    inner.recreate.reason = "route-set-changed".to_string();
                }
            }
        }
    }

    /// N8: shell ACK — `routes` is now the route set APPLIED to the platform
    /// VpnConfig (`is_recreate=false`: the initial create; `true`: a
    /// completed controlled recreate — counts against [`RECREATE_MAX`] and
    /// arms the [`RECREATE_COOLDOWN_MS`] window). Idempotent-safe: repeated
    /// ACKs of the same set without `is_recreate` just re-record it.
    pub fn ack_applied_route_set(&self, routes: Vec<String>, is_recreate: bool, now_ms: u64) {
        let mut g = self.lock();
        g.applied_route_set = Some(routes.clone());
        if is_recreate {
            g.recreate.count = g.recreate.count.saturating_add(1);
            g.last_recreate_ms = Some(now_ms);
            g.recreate.required = false;
            g.recreate.cooling_down = false;
            if g.recreate.count >= RECREATE_MAX {
                g.recreate.exhausted = true;
                hilog::emit(&format!(
                    "connector: recreate budget exhausted (count={})",
                    g.recreate.count
                ));
            }
            hilog::emit(&format!(
                "connector: recreate acked (count={}, routes={})",
                g.recreate.count,
                routes.len()
            ));
        } else {
            g.recreate.required = false;
            hilog::emit(&format!("connector: applied route set acked (routes={})", routes.len()));
        }
    }

    // -----------------------------------------------------------------------
    // N13-D1: two-path data-plane readiness (T0 Q3)
    // -----------------------------------------------------------------------

    /// The default-route decision under the two-path readiness rule: the
    /// data plane carries traffic only with a live WG session
    /// (`wg.tunnel_ready()`) AND a carrier — an ICE-nominated path (the
    /// pre-N13 rule) OR the relay client Ready with a valid token
    /// ([`ConnectorShared::relay_ready`]). Feeds the UNCHANGED
    /// [`ShellNetworkConfig::default_route_decision`]; when ONLY the relay
    /// path enables the route the reason token names it, so the shell and
    /// the diagnostics can always tell the paths apart:
    /// - (a) relay Ready only → allowed, `…-relay-ready`
    /// - (b) ICE nominated → allowed, `…-tunnel-ready` (unchanged tokens)
    /// - (c) ICE Failed and relay not ready → HELD (`data-plane-not-ready`)
    ///   — ICE Failed is NEVER silently treated as reachable.
    fn decide_default_route(
        &self,
        force: bool,
        wg: &dyn WgPeerApplier,
        ice_nominated: bool,
        peer_count: usize,
    ) -> (bool, String) {
        let relay_ready = self.relay_ready();
        let ready = two_path_readiness(wg.tunnel_ready(), ice_nominated, relay_ready);
        let (allowed, mut reason) =
            ShellNetworkConfig::default_route_decision(peer_count, ready, force);
        if allowed && !force && !ice_nominated && relay_ready {
            reason = DEFAULT_ROUTE_REASON_RELAY_READY.to_string();
        }
        (allowed, reason)
    }

    fn record_error(&self, err: &ManagementError) {
        self.lock().last_error = Some(ErrorClass::from_management(err));
    }

    // -----------------------------------------------------------------------
    // N13-D1: relay client lifecycle (opt-in via relay_enabled)
    // -----------------------------------------------------------------------

    /// 「relay 可用」for the data-plane readiness judgement: enabled, the
    /// client is at `Ready` (authenticated session live) AND the current
    /// token is still valid — fail-closed on expiry (the plan §3.6 stop
    /// condition 「令牌过期静默假通」: the session is killed by the client,
    /// and the connector must not report readiness across that window).
    /// T0 Q3: judged ONLY from relay state — the ICE counters/summary are
    /// never touched, so relay bytes can never count as ICE success.
    fn relay_ready(&self) -> bool {
        if !self.relay_enabled {
            return false;
        }
        let slot = self.relay.lock_poison();
        let Some(client) = slot.client.as_ref() else {
            return false;
        };
        client.state() == crate::relay_client::RelayState::Ready
            && matches!(
                client.token_validity(),
                crate::relay_client::TokenValidity::Valid { .. }
            )
    }

    /// Apply one Sync's `RelayServers` (relay spec §6.1/§6.2): the first
    /// advertisement starts the client; the SAME URL set only refreshes the
    /// token (`update_token` — a live session is NOT reconnected, the
    /// protocol has no in-band re-auth); a CHANGED URL set replaces the
    /// client (old one stopped — its teardown is the relay Close frame +
    /// socket close). Fail-closed: an unusable token/URL set never touches a
    /// live client and never starts one; every outcome is logged (counts and
    /// url values only — never token material).
    fn relay_apply(&self, relay: &crate::network_map::RelayServers) {
        let Some(attach) = self.relay_attach.as_ref() else {
            return;
        };
        if relay.urls.is_empty() {
            self.relay_drop("relay-urls-empty");
            return;
        }
        let token = match crate::relay::AuthToken::from_management(
            &relay.token_payload,
            &relay.token_signature,
        ) {
            Ok(t) => t,
            Err(e) => {
                hilog::emit(&format!(
                    "connector: relay token unusable (reason token: {e:?}) — relay not updated"
                ));
                return;
            }
        };
        let mut slot = self.relay.lock_poison();
        match slot.client.as_ref() {
            Some(client) if slot.urls == relay.urls && client.state() == crate::relay_client::RelayState::Dead => {
                slot.client = None;
                slot.urls = relay.urls.clone();
                match Self::relay_start(attach, relay, token) {
                    Ok(new) => {
                        slot.client = Some(new);
                        hilog::emit("connector: dead relay client restarted (same urls)");
                    }
                    Err(e) => hilog::emit(&format!(
                        "connector: dead relay restart failed ({e:?}) — relay unavailable (fail-closed)"
                    )),
                }
            }
            Some(client) if slot.urls == relay.urls => {
                client.update_token(token);
                hilog::emit(&format!(
                    "connector: relay token refreshed (urls={})",
                    slot.urls.len()
                ));
            }
            Some(client) => {
                // URL set changed (spec §6.2 step 1): replace the client.
                client.stop();
                hilog::emit("connector: relay urls changed — restarting relay client");
                slot.client = None;
                slot.urls = relay.urls.clone();
                match Self::relay_start(attach, relay, token) {
                    Ok(new) => {
                        slot.client = Some(new);
                        hilog::emit(&format!(
                            "connector: relay client restarted (urls={})",
                            relay.urls.len()
                        ));
                    }
                    Err(e) => hilog::emit(&format!(
                        "connector: relay restart failed ({e:?}) — relay unavailable (fail-closed)"
                    )),
                }
            }
            None => {
                slot.urls = relay.urls.clone();
                match Self::relay_start(attach, relay, token) {
                    Ok(new) => {
                        slot.client = Some(new);
                        hilog::emit(&format!(
                            "connector: relay client started (urls={})",
                            relay.urls.len()
                        ));
                    }
                    Err(e) => hilog::emit(&format!(
                        "connector: relay start failed ({e:?}) — relay unavailable (fail-closed)"
                    )),
                }
            }
        }
    }

    /// Build + start one relay client from the injected seams. Requires a
    /// running tokio runtime (always true: called from the sync worker).
    fn relay_start(
        attach: &RelayAttach,
        relay: &crate::network_map::RelayServers,
        token: crate::relay::AuthToken,
    ) -> Result<crate::relay_client::RelayClient, crate::relay_client::RelayClientError> {        let mut cfg = crate::relay_client::RelayClientConfig::new(
            &relay.urls,
            &attach.wg_pubkey_b64,
            token,
        )?
        .with_clock(attach.materials.clock.clone())
        .with_dialer(attach.materials.dialer.clone());
        if let Some(tls) = attach.materials.tls.as_ref() {
            cfg = cfg.with_tls(tls.clone());
        }
        crate::relay_client::RelayClient::start(cfg)
    }

    /// Management stopped advertising relay (or an empty url set arrived):
    /// stop the client — no orphan outbound relay connection. The handle is
    /// kept ONLY so status renders the terminal state; the urls record goes.
    fn relay_drop(&self, why: &'static str) {
        let mut slot = self.relay.lock_poison();
        if let Some(client) = slot.client.as_ref() {
            client.stop();
            hilog::emit(&format!("connector: relay client stopped ({why})"));
        }
        slot.urls.clear();
    }

    /// 撤销清理 (N13 plan §4: 「注销/destroy 后 WSS 必须消失，不得留出站连
    /// 接」): called from `ConnectorHandle::stop()` AND when the connector
    /// worker ends by itself — the relay client must never outlive the
    /// connector. The handle is kept so status can render `Dead`.
    /// N13-D2: the relay-carrier pump and every attached lane are torn down
    /// with it (no reachable bearer, no residual outbound connection).
    fn stop_relay(&self) {
        {
            let slot = self.relay.lock_poison();
            if let Some(client) = slot.client.as_ref() {
                client.stop();
                hilog::emit("connector: relay client stopped (connector teardown)");
            }
        }
        if let Some(carrier) = self.relay_carrier.get() {
            carrier.shutdown();
        }
    }

    /// N13-D2 — arm the WG relay-carrier pump after a relay client start
    /// (called from the sync path; idempotent). No-op unless the WG seam is
    /// carrier-capable: a seam that cannot host lanes must never cause
    /// relay subscriptions it cannot use (fail-closed honesty).
    fn relay_carrier_client_started(&self, client: crate::relay_client::RelayClient) {
        let (Some(wg), Some(rt)) = (self.relay_wg.get(), self.relay_runtime.get()) else {
            return;
        };
        if !wg.carrier_capable() {
            return;
        }
        if self.relay_carrier.get().is_none() {
            let _ = self.relay_carrier.set(RelayCarrier::new(wg.clone()));
        }
        let carrier = self.relay_carrier.get().expect("carrier slot just set");
        if !carrier.has_client(&client) {
            carrier.attach_client(client);
            carrier.ensure_pump(rt);
        }
    }

    /// The running relay client handle, if any (slot reader shared by the
    /// status section and the handle accessor).
    fn relay_handle(&self) -> Option<crate::relay_client::RelayClient> {
        self.relay.lock_poison().client.clone()
    }

    /// The status document's `relay` section (counts/urls/class only —
    /// never token material; credential discipline as everywhere else).
    /// The ADVERTISED url set rides along in EVERY state (also
    /// `relay_enabled:false`); the runtime fields (enabled/state/counters)
    /// keep their exact pre-existing semantics.
    fn relay_status(&self) -> RelayStatus {
        let advertised_urls = self.relay.lock_poison().advertised_urls.clone();
        let advertised = !advertised_urls.is_empty();
        if !self.relay_enabled {
            return RelayStatus {
                advertised,
                advertised_urls,
                ..RelayStatus::disabled()
            };
        }
        let slot = self.relay.lock_poison();
        let urls = slot.urls.clone();
        let Some(client) = slot.client.as_ref() else {
            return RelayStatus {
                enabled: true,
                state: relay_state_token(crate::relay_client::RelayState::Disconnected).into(),
                urls,
                advertised,
                advertised_urls,
                ..RelayStatus::empty_counters()
            };
        };
        let stats = client.stats();
        RelayStatus {
            enabled: true,
            state: relay_state_token(stats.state).into(),
            urls,
            advertised,
            advertised_urls,
            reconnects: stats.reconnects,
            frames_tx: stats.frames_tx.iter().sum(),
            frames_rx: stats.frames_rx.iter().sum(),
            transport_bytes: stats.transport_tx_bytes + stats.transport_rx_bytes,
            token_valid: matches!(
                client.token_validity(),
                crate::relay_client::TokenValidity::Valid { .. }
            ),
            last_error_class: stats.last_error_class.map(|c| relay_error_class_token(c).to_string()),
        }
    }

    fn count_retryable_failure(&self) {
        self.lock().reconnects += 1;
    }

    fn set_logout_ok(&self, ok: bool) {
        self.lock().logout_ok = Some(ok);
    }

    /// N2-H (design §4.0 stun row): `netbird_config.stuns[]` as the ICE
    /// orchestrator PARSED it (what `set_stuns` accepted — the exact set core
    /// gathers against). Read from the session's own state: neither public
    /// document carries it, and §5.2 forbids adding it there.
    fn stuns(&self) -> Vec<crate::ice::StunServer> {
        self.ice.get().map(|ice| ice.lock_poison().stuns().to_vec()).unwrap_or_default()
    }

    fn remember_login_signal(&self, response: &crate::grpc::proto::LoginResponse) {
        if let Some(uri) = response.netbird_config.as_ref()
            .and_then(|cfg| cfg.signal.as_ref()).map(|signal| signal.uri.as_str())
            .filter(|uri| !uri.is_empty())
        {
            if let Some(rt) = self.signal.get() {
                rt.set_uri(uri);
                hilog::emit("connector: signal URI received from login");
            }
        }
    }

    /// Apply one decoded Sync update to the internal state and the data
    /// plane. Snapshot ordering follows upstream engine.go:1572-1576: a
    /// NetworkMap with a serial STRICTLY BELOW the last applied one is
    /// ignored (an equal serial still applies).
    fn apply_update(&self, wg: &dyn WgPeerApplier, host: &dyn ConfigApplier, update: &SyncUpdate) {
        if update.session_deadline_unix.is_some() {
            self.lock().deadline.apply_wire(update.session_deadline_unix);
        }
        // N5c: STUN server set rides every sync (netbird_config.stuns —
        // engine.go:1525-1541 updateSTUNs); applied even without a map.
        // N5d: the signal URI rides the same config (connectToSignal is fed
        // from netbirdConfig upstream, connect.go:715-731) and ARMS the real
        // signal link once both the URI and the shell-fed connect address
        // exist.
        let mut signal_uri: Option<String> = None;
        // N2-H I2 (design §13.13): the announced `stuns[]` URIs ride the same
        // netbird-config as the signal URI. `None` = this update carried no
        // config at all: the PREVIOUS announcement stays visible (mirrors both
        // the signal handling below and `ConnectorShared::stuns()`, whose
        // orchestrator set is only replaced when a config actually arrives).
        let mut stun_uris: Option<Vec<String>> = None;
        if let Some(cfg) = update.netbird_config.as_ref() {
            if let Some(ice) = self.ice.get() {
                ice.lock_poison().set_stuns(&cfg.stuns);
                if let Some(sockets) = self.ice_sockets.get() {
                    let stuns = ice.lock_poison().stuns().to_vec();
                    sockets.refresh_stun_dns(&stuns);
                }
            }
            stun_uris = Some(cfg.stuns.clone());
            if let Some(uri) = cfg.signal.as_ref() {
                signal_uri = Some(uri.clone());
            }
            // Relay advertisement (NetbirdConfig.relay): the urls are public
            // routing material, the token is a management-issued credential —
            // log ONLY url count/values and whether a token rode along, never
            // the token itself. This is the read-only probe that confirms a
            // deployment actually advertises relays (relay implementation is
            // not in this increment).
            match cfg.relay.as_ref() {
                Some(r) => hilog::emit(&format!(
                    "connector: netbird-config relay urls={} token_present={} (uris={})",
                    r.urls.len(),
                    !r.token_payload.is_empty() && !r.token_signature.is_empty(),
                    r.urls.join(",")
                )),
                None => hilog::emit("connector: netbird-config relay none"),
            }
            // The ADVERTISEMENT is recorded on every sync REGARDLESS of the
            // `relay_enabled` opt-in (loop-dependency fix): the shell reads
            // `advertised_urls` from the status document to install its
            // N2-H relay exclusion BEFORE it may enable relay. `None` (no
            // relay in this sync's config) clears the record — the status
            // then honestly says "not advertised". Bookkeeping only: this
            // never dials; the client lifecycle below stays gated exactly
            // as before, so `relay_enabled:false` keeps ZERO outbound relay
            // behavior.
            {
                let mut slot = self.relay.lock_poison();
                slot.advertised_urls =
                    cfg.relay.as_ref().map(|r| r.urls.clone()).unwrap_or_default();
                // signal seam 镜像（WG over relay 修复）：OFFER/ANSWER 的
                // Body 必须携带本端广告的中继地址（signalexchange.proto
                // 字段 8；上游 handshaker.go:224-242 每个 offer/answer 都
                // 填），对端**只在**该字段非空时才为我方 OpenConn 建中继
                // lane（worker_relay.go:122-127 → :69）——缺失即"帧在发、
                // 对端不建 lane、wgSessions=0"。取广告集**第一个**
                // （management 广告序 = 上游 picker 的候选序；生产部署单
                // URL，与上游"本端连接的实例地址"一致，多 URL 见
                // `RealSignalExchange::set_relay_address` 注释）。未广告 →
                // `None` 清除，字段回到缺省（无 relay 部署行为不变）。
                // 广告记录本身无条件于 `relay_enabled`（上面的既有规则），
                // 镜像沿用同一规则：仅 bookkeeping，绝不 dial。
                let advertised_first = slot.advertised_urls.first().cloned();
                drop(slot);
                if let Some(rt) = self.signal.get() {
                    rt.exchange.set_relay_address(advertised_first);
                }
            }
            // N13-D1: run/update the relay client (opt-in only — with
            // `relay_enabled:false` (the hard default) NOTHING below runs
            // and the device path behaves exactly as before). Relay rides
            // netbird_config, so this happens BEFORE the map-serial gate
            // (every sync refreshes, upstream engine.go:1185-1214).
            // N13-D2: a started relay client arms the WG carrier pump (the
            // relay bearer over the WG seam); management dropping the relay
            // advertisement detaches every lane (fail-closed).
            if self.relay_enabled {
                match cfg.relay.as_ref() {
                    Some(r) => {
                        self.relay_apply(r);
                        if let Some(client) = self.relay_handle() {
                            self.relay_carrier_client_started(client);
                        }
                    }
                    None => {
                        self.relay_drop("relay-removed-from-sync");
                        if let Some(carrier) = self.relay_carrier.get() {
                            carrier.detach_all();
                        }
                    }
                }
            }
        } else {
            // config-less snapshot: keep the previously announced URI visible
            signal_uri = self.lock().net_config.as_ref().and_then(|c| c.signal.clone());
            // …and the previously announced stun set (see `stun_uris` above)
            stun_uris = self.lock().net_config.as_ref().map(|c| c.stuns.clone());
        }
        // LoginResponse can carry signal even when later Sync updates omit it.
        // Preserve it in the runtime so config-less NetworkMaps expose the URI.
        if signal_uri.is_none() {
            signal_uri = self.signal.get().and_then(|rt| rt.uri.lock_poison().clone());
        }
        if let Some(uri) = signal_uri.as_ref() {
            if let Some(rt) = self.signal.get() {
                rt.set_uri(uri);
                if self.is_running() {
                    rt.maybe_start();
                }
            }
        }
        let Some(map) = update.network_map.as_ref() else {
            // No NetworkMap in this snapshot; the receipt still refreshes
            // the "last update" marker (the session deadline above was
            // already applied).
            self.lock().last_update_unix = Some(unix_now());
            return;
        };
        {
            let g = self.lock();
            if g.last_serial > map.serial {
                let last = g.last_serial;
                drop(g);
                hilog::emit(&format!(
                    "connector: outdated network map serial {last} > {}, ignored",
                    map.serial
                ));
                self.lock().last_update_unix = Some(unix_now());
                return;
            }
        }
        let peers = map.peers.len() + map.offline_peers.len();
        // N5c: default-route gate input now includes the ICE view — the data
        // plane is only "ICE-ready" when at least one peer has a LANDED
        // endpoint (Connected + apply_endpoint OK). All-Failed/never-connected
        // peers keep the default route HELD (N3-7 linkage).
        // N13-D1: the same gate now accepts a SECOND carrier — the relay
        // client at Ready (T0 Q3 two-path rule, see `two_path_readiness`).
        let ice_ready = {
            let summary = self.ice.get().map(|ice| ice.lock_poison().summary());
            match summary {
                Some(s) => ice_ready_for_default_route(&s),
                None => true, // no orchestrator (unit-test construction): N3-7 semantics unchanged
            }
        };
        {
            let mut g = self.lock();
            g.last_serial = map.serial;
            g.peer_count = peers;
            g.route_count = map.routes.len();
            // N3-6/N3-7: snapshot the shell-applicable subset, through the
            // default-route safety gate (force flag + live WG readiness).
            // N5d: the signal URI rides the snapshot so the shell can
            // resolve + protect + feed the signal socket.
            // N13-D1: relay readiness is the second carrier (independent of
            // the ICE view — never merged into ICE counters).
            let relay_ready = self.relay_ready();
            let data_plane_ready =
                two_path_readiness(wg.tunnel_ready(), ice_ready, relay_ready);
            let mut snapshot =
                ShellNetworkConfig::from_map_gated(map, g.force_default_route, data_plane_ready);
            if data_plane_ready && !g.force_default_route && !ice_ready && relay_ready {
                // the relay path enabled the route — name it in the token
                snapshot.default_route_reason = DEFAULT_ROUTE_REASON_RELAY_READY.to_string();
            }
            snapshot.signal = signal_uri;
            // N2-H I2: the announced stun URIs (verbatim; empty = honest gap)
            snapshot.stuns = stun_uris.unwrap_or_default();
            // N7: keep the UNGATED route record so `refresh_net_gate` can
            // re-export / re-hold the default route as live readiness moves
            g.net_routes_all = map
                .routes
                .iter()
                .map(|r| ShellRouteEntry {
                    network: route_network_string(&r.network),
                    is_default: r.network.is_default(),
                })
                .collect();
            g.net_config = Some(snapshot);
        }
        // N5c: reconcile the per-peer ICE orchestrator with the map's
        // connectable peers (allowed_ips only — that is the data plane this
        // client can route to).
        // N13-D2: the same key set is the relay-carrier lane universe (the
        // carrier pump opens lanes exactly for these peers).
        let connectable_keys: Vec<String> = map
            .peers
            .iter()
            .filter(|p| !p.allowed_ips.is_empty())
            .map(|p| p.wg_pub_key.clone())
            .collect();
        if let Some(ice) = self.ice.get() {
            ice.lock_poison().set_peers(&connectable_keys);
        }
        if let Some(carrier) = self.relay_carrier.get() {
            carrier.set_peers(&connectable_keys);
        }
        // WG data plane: register remote + offline peers (identity +
        // allowed_ips only — module limitation statement).
        let mut entries = Vec::with_capacity(peers);
        for p in map.peers.iter().chain(map.offline_peers.iter()) {
            entries.push(WgPeerEntry {
                pub_key_b64: p.wg_pub_key.clone(),
                allowed_ips: p.allowed_ips.clone(),
            });
        }
        match wg.apply_peers(&entries) {
            Ok(()) => self.lock().wg_apply_failed = false,
            Err(_) => {
                let mut g = self.lock();
                g.wg_apply_failed = true;
                g.wg_apply_errors += 1;
            }
        }
        // Routes / DNS / own address go to the shell side.
        host.apply(map);
        self.lock().last_update_unix = Some(unix_now());
        hilog::emit(&format!(
            "connector: network map applied serial={} peers={} routes={}",
            map.serial,
            map.peers.len(),
            map.routes.len()
        ));
    }
}

// ---------------------------------------------------------------------------
// N13-D2: the WG relay-carrier orchestrator (lanes + inbound pump)
// ---------------------------------------------------------------------------

/// Observability of the relay-carrier orchestration (counters only — peer
/// keys are PUBLIC material but even they are not rendered here; the
/// per-carrier byte/packet counts live on the WG device stats).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RelayCarrierStats {
    /// `open_conn` calls that saw `PeersOnline` (§4.1).
    pub lanes_open_ok: u64,
    /// `open_conn` failures (typed; retried on the next pump pass).
    pub lanes_open_failed: u64,
    /// Lane handles actually attached to the WG seam.
    pub attach_ok: u64,
    /// WG seam carrier-attach refusals (e.g. unregistered peer — config bug).
    pub attach_err: u64,
    /// Lane detachments (revocation / relay-down fail-closed / stop /
    /// peer went offline — the presence reconcile in [`RelayCarrier::ensure_lane`]).
    pub detach_ok: u64,
    /// Inbound relay datagrams fed to the WG device (matched senders).
    pub frames_in: u64,
    /// Reply datagrams the device produced while processing them.
    pub frames_in_replies: u64,
    /// Inbound relay datagrams whose sender matched NO registered peer
    /// (dropped + counted — never fed to the device).
    pub frames_in_unknown: u64,
    /// Last `open_conn` failure class (stable token, diagnostics only).
    pub last_open_error: Option<&'static str>,
}

/// Retry backoff for a lane whose `open_conn` failed (e.g. the peer stays
/// offline and the 30s §4.1 window timed out). This port re-opens lanes on
/// the pump reconcile where upstream re-opens strictly on demand, so
/// without it a fast-failing open would retry every pump pass; with it an
/// offline peer costs one attempt per backoff window (the blocking 30s
/// window itself stays the dominant rate limiter). Cost: recovery after a
/// FAILED open is delayed by up to one window.
const LANE_OPEN_RETRY: Duration = Duration::from_secs(5);

/// N13-D2: the relay↔WG bridge for one connector — upstream's wgProxy
/// orchestrator in minimal form. Duties:
///
/// - **lanes**: for every connectable peer (network-map keys) hold one
///   subscribed relay lane: `open_conn` (waits for `PeersOnline`, §4.1) →
///   attach a [`crate::relay_client::RelayWgCarrier`] to the WG seam. The
///   device's own dispatch keeps the bearer priority (`Relay < ICE`):
///   attaching a lane NEVER diverts traffic that has an ICE-selected path.
/// - **inbound pump**: `RelayClient::recv()` frames are reversed from the
///   sender peer id to the WG public key (the peer id IS derived from it,
///   relay spec §3.2 — the matching rule is documented in
///   `crate::wg_device`, N13-D2「入向匹配映射」) and fed to the device's
///   carrier ingress. Unknown senders are dropped + counted.
/// - **fail-closed lifecycle**: relay not Ready ⇒ lanes detached (a dead
///   relay must not look like a path); peer removed from the map ⇒ lane
///   detached; `shutdown()` (connector stop / worker end) ⇒ pump aborted +
///   all lanes detached — no reachable relay bearer survives the connector.
///
/// fd contract: this struct holds NO fd. The lane handles are pure Rust
/// objects over the relay client; the relay WSS/TCP socket is native-owned
/// by the relay client session and outlives nothing (D1 teardown); the
/// platform TUN raw fd is never touched here (closed only by the shell's
/// `VpnConnection.destroy()`).
pub struct RelayCarrier {
    /// The live relay client (set at the first start / refreshed when the
    /// URL set changes — a swap aborts the old pump and detaches lanes:
    /// presence does not survive a relay reconnect, D1 semantics).
    client: Mutex<Option<crate::relay_client::RelayClient>>,
    wg: std::sync::Arc<dyn WgPeerApplier>,
    /// Connectable peer keys ↔ their relay peer ids (the reverse map for
    /// the inbound pump; rebuilt from every network-map sync).
    peers: Mutex<Vec<(String, crate::relay::PeerId)>>,
    /// Currently attached lanes: peer key → the carrier handle the WG seam
    /// received (identity for idempotent attach/detach bookkeeping).
    attached: Mutex<HashMap<String, std::sync::Arc<crate::relay_client::RelayWgCarrier>>>,
    /// Per-peer earliest instant the next `open_conn` attempt is allowed
    /// ([`LANE_OPEN_RETRY`] backoff after a failed open; entries clear on
    /// success).
    open_retry_after: Mutex<HashMap<String, std::time::Instant>>,
    /// Lifecycle guard (T0-0018 follow-up): the lane-open COMMIT — validity
    /// re-check + WG attach + attached-table insert, see
    /// [`RelayCarrier::commit_lane_open`] — and every invalidating transition
    /// ([`RelayCarrier::attach_client`] re-arm/swap, [`RelayCarrier::set_peers`]
    /// removal, [`RelayCarrier::shutdown`]) run under THIS mutex, so they are
    /// mutually exclusive: a late open either sees invalid state and drops,
    /// or commits before the invalidator, whose detach under the same lock
    /// then removes it — a removed/stopped carrier can never be resurrected
    /// by a late open. `true` = armed (between attach_client and shutdown).
    /// LOCK ORDER: `lifecycle` first, then one leaf lock at a time
    /// (client/peers/attached/pending_opens/open_retry_after/stats); never
    /// held across an await; no leaf holder ever acquires `lifecycle` (no
    /// ABBA).
    lifecycle: Mutex<bool>,
    /// Per-peer in-flight lane-open task — at most ONE per peer (0018 fix:
    /// opens run as managed tasks so a peer that never comes online parks
    /// only its own task, never the inbound pump). The pump spawns and never
    /// awaits them; entries carry a generation (`AtomicU64` counter) so a
    /// task's finish removes ONLY its own registration — an aborted-but-
    /// never-yielded old task must not unregister a newer task's entry (that
    /// would orphan it from abort/shutdown bookkeeping and break
    /// single-flight). Aborted on peer removal / client swap / pump exit /
    /// teardown; reaped on the pump tick if a task died without bookkeeping.
    pending_opens: Mutex<HashMap<String, (u64, tokio::task::JoinHandle<()>)>>,
    /// Generation counter for [`RelayCarrier::pending_opens`] entries.
    next_open_gen: std::sync::atomic::AtomicU64,
    stats: Mutex<RelayCarrierStats>,
    /// The production pump task (spawned on the connector runtime).
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl RelayCarrier {
    /// An unarmed orchestrator over `wg` (host/test entry: arm it with
    /// [`RelayCarrier::attach_client`] once a relay client exists; the
    /// connector arms it automatically from the sync path).
    pub fn new(wg: std::sync::Arc<dyn WgPeerApplier>) -> std::sync::Arc<Self> {
        std::sync::Arc::new(RelayCarrier {
            client: Mutex::new(None),
            wg,
            peers: Mutex::new(Vec::new()),
            attached: Mutex::new(HashMap::new()),
            open_retry_after: Mutex::new(HashMap::new()),
            lifecycle: Mutex::new(false),
            pending_opens: Mutex::new(HashMap::new()),
            next_open_gen: std::sync::atomic::AtomicU64::new(0),
            stats: Mutex::new(RelayCarrierStats::default()),
            task: Mutex::new(None),
        })
    }

    fn bump(&self, f: impl FnOnce(&mut RelayCarrierStats)) {
        f(&mut self.stats.lock().unwrap_or_else(PoisonError::into_inner));
    }

    /// Snapshot of the orchestration counters.
    pub fn stats(&self) -> RelayCarrierStats {
        *self.stats.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Currently attached lane count (observability).
    pub fn lane_count(&self) -> usize {
        self.attached.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    /// Whether `client` is the client this orchestrator currently serves.
    fn has_client(&self, client: &crate::relay_client::RelayClient) -> bool {
        self.client
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|c| c.same_client(client))
            .unwrap_or(false)
    }

    /// (Re)arm with a relay client: detaches all lanes (presence does not
    /// survive a relay reconnect), aborts any pump AND any pending lane open
    /// of a PREVIOUS client. Runs under the lifecycle lock: mutually exclusive
    /// with the lane-open commit — an old client's open that already passed
    /// its await either commits BEFORE this (and is detached right here) or
    /// sees the swapped client in its commit check and drops.
    pub fn attach_client(&self, client: crate::relay_client::RelayClient) {
        let mut lifecycle = self.lifecycle.lock().unwrap_or_else(PoisonError::into_inner);
        *lifecycle = true;
        self.detach_all_locked(&lifecycle);
        if let Some(h) = self.task.lock().unwrap_or_else(PoisonError::into_inner).take() {
            h.abort();
        }
        self.abort_pending_lanes();
        *self.client.lock().unwrap_or_else(PoisonError::into_inner) = Some(client);
        drop(lifecycle);
        hilog::emit("connector: relay carrier armed with the relay client");
    }

    /// The connectable peer set from the last network map (lane universe).
    /// Lanes of peers that left the map are detached immediately. Public:
    /// host/test drivers of the orchestrator feed the same map-derived key
    /// set the sync path does.
    pub fn set_peers(&self, keys: &[String]) {
        // Under the lifecycle lock: the map replacement + removal handling is
        // mutually exclusive with the lane-open commit (a commit either sees
        // the peer gone and drops, or attached before this run — the stale
        // loop below then detaches it).
        let lifecycle = self.lifecycle.lock().unwrap_or_else(PoisonError::into_inner);
        let removed: Vec<String> = {
            let mut peers = self.peers.lock().unwrap_or_else(PoisonError::into_inner);
            let removed: Vec<String> = peers
                .iter()
                .filter(|(k, _)| !keys.contains(k))
                .map(|(k, _)| k.clone())
                .collect();
            *peers = keys
                .iter()
                .map(|k| (k.clone(), crate::relay::PeerId::from_wg_pubkey_string(k)))
                .collect();
            removed
        };
        // A removed peer's pending open must never fire into a late attach:
        // abort the task (a task already past its last await is unaffected by
        // abort — its commit sees the peer gone and drops); also drop its
        // backoff entry — the map must not remember a peer the lane universe
        // forgot.
        for key in &removed {
            if let Some((_, h)) =
                self.pending_opens.lock().unwrap_or_else(PoisonError::into_inner).remove(key)
            {
                h.abort();
            }
            self.open_retry_after
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(key);
        }
        let mut attached = self.attached.lock().unwrap_or_else(PoisonError::into_inner);
        let stale: Vec<String> = attached
            .keys()
            .filter(|k| !keys.contains(k))
            .cloned()
            .collect();
        for key in stale {
            attached.remove(&key);
            self.wg.detach_carrier(&key);
            self.bump(|s| s.detach_ok += 1);
            hilog::emit("connector: relay carrier lane detached (peer left the map)");
        }
        drop(lifecycle);
    }

    /// Spawn the production pump task (idempotent) on `rt`.
    fn ensure_pump(self: &std::sync::Arc<Self>, rt: &tokio::runtime::Handle) {
        let mut task = self.task.lock().unwrap_or_else(PoisonError::into_inner);
        if task.is_some() {
            return;
        }
        *task = Some(rt.spawn(relay_carrier_pump(self.clone())));
        hilog::emit("connector: relay carrier pump started");
    }

    /// One `open_conn` + attach for `key` if no lane is attached yet.
    /// `true` = a lane is attached (maybe from an earlier pass).
    ///
    /// Presence reconcile (upstream client lifecycle): an attached lane
    /// whose peer the server reported OFFLINE (`PeersWentOffline`) is
    /// detached here. The relay server's came-online interest is ONE-SHOT
    /// (consumed by the first `PeersOnline` delivery — only a NEW
    /// `SubscribePeerState` re-arms it), so a lane kept attached across the
    /// peer's offline window can never see the peer's return: this is the
    /// upstream "went-offline → close per-peer connection → re-`OpenConn`
    /// on demand" half our port was missing. The re-open below IS that
    /// fresh `open_conn`: it blocks in the §4.1 wait until the peer returns
    /// (fresh subscribe re-arms the interest → `PeersOnline` → presence
    /// restored → lane re-attached, egress accepted again), or fails typed
    /// and backs off [`LANE_OPEN_RETRY`] before the next attempt.
    pub async fn ensure_lane(self: &std::sync::Arc<Self>, key: &str) -> bool {
        let Some(client) = self.client.lock().unwrap_or_else(PoisonError::into_inner).clone()
        else {
            return false;
        };
        let pid = crate::relay::PeerId::from_wg_pubkey_string(key);
        if self.attached.lock().unwrap_or_else(PoisonError::into_inner).contains_key(key) {
            if !client.is_offline(&pid) {
                return true;
            }
            // PeersWentOffline after attach: the lane cannot serve the peer
            // and cannot learn about its return — detach so the re-open
            // below re-subscribes (idempotent: the attached table is keyed,
            // open_conn guards duplicate pending subscribes, and the WG
            // seam attach is an upsert).
            self.attached
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(key);
            self.wg.detach_carrier(key);
            self.bump(|s| s.detach_ok += 1);
            hilog::emit("connector: relay carrier lane detached (peer went offline)");
        }
        if self
            .open_retry_after
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .is_some_and(|t| std::time::Instant::now() < *t)
        {
            return false;
        }
        match client.open_conn(&pid).await {
            Ok(()) => {
                self.open_retry_after
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(key);
                self.commit_lane_open(key, pid, client)
            }
            Err(e) => {
                self.open_retry_after
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(key.to_string(), std::time::Instant::now() + LANE_OPEN_RETRY);
                self.bump(|s| {
                    s.lanes_open_failed += 1;
                    s.last_open_error = Some(relay_error_class_token(e.class()));
                });
                hilog::emit(&format!(
                    "connector: relay lane open failed|peer={} class={}",
                    pid,
                    relay_error_class_token(e.class())
                ));
                false
            }
        }
    }

    /// Atomic lane-open commit (T0-0018 follow-up). The open ran as a
    /// detached task, so between its resolution and the attach the carrier may
    /// have been disarmed (shutdown), swapped to another client
    /// ([`RelayCarrier::attach_client`]) or the peer removed from the map
    /// ([`RelayCarrier::set_peers`]). The validity re-check AND the WG attach
    /// + attached-table insert happen inside ONE lifecycle critical section,
    /// mutually exclusive with those transitions: a late open either (a) sees
    /// invalid state and drops, or (b) commits BEFORE the invalidator — whose
    /// detach, under the same lock, then removes the lane. Either way a
    /// removed/stopped/swapped carrier is never resurrected by a late open.
    /// Lock order: lifecycle → one leaf at a time; no await under any lock.
    fn commit_lane_open(&self, key: &str, pid: crate::relay::PeerId, client: crate::relay_client::RelayClient) -> bool {
        let lifecycle = self.lifecycle.lock().unwrap_or_else(PoisonError::into_inner);
        let still_wanted =
            *lifecycle
                && self.has_client(&client)
                && self
                    .peers
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .iter()
                    .any(|(k, _)| k == key);
        if !still_wanted {
            drop(lifecycle);
            hilog::emit(&format!(
                "connector: relay lane open resolved late, dropped|peer={}",
                pid
            ));
            return false;
        }
        let lane =
            std::sync::Arc::new(crate::relay_client::RelayWgCarrier::new(client, pid));
        match self.wg.attach_carrier(key, lane.clone()) {
            Ok(()) => {
                self.attached
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(key.to_string(), lane);
                self.bump(|s| {
                    s.lanes_open_ok += 1;
                    s.attach_ok += 1;
                });
                drop(lifecycle);
                hilog::emit("connector: relay carrier lane attached (peers-online)");
                true
            }
            Err(e) => {
                self.bump(|s| s.attach_err += 1);
                drop(lifecycle);
                hilog::emit(&format!(
                    "connector: relay carrier attach failed (seam: {e})"
                ));
                false
            }
        }
    }

    /// Fan the lane universe out into managed per-peer open tasks (the pump's
    /// per-pass pass; 0018 fix). NEVER awaits an open: the pre-fix version
    /// awaited every peer's `open_conn` sequentially here, so one peer that
    /// never came online parked the whole pump for the 30s §4.1 budget per
    /// pass — inbound frames of already-connected lanes starved behind it
    /// (0018: transport bytes flowing, wg.rx_packets 0, session never up).
    ///
    /// Registration is ATOMIC and lifecycle-gated (T0 residual): the whole
    /// await-free pass runs under the lifecycle guard (an unarmed carrier
    /// registers nothing — a pump pass that survives a concurrent teardown
    /// cannot register post-shutdown orphans), and per peer the vacant check →
    /// `tokio::spawn` → insert happen under ONE `pending_opens` critical
    /// section — two transient pumps can never check-miss each other into
    /// overwriting a registered handle. `spawn` neither awaits nor polls the
    /// child, so holding the std mutex across it is safe (a child that races
    /// ahead parks on the same lock until the insert is done).
    /// Discipline per peer: skip if a lane is live (presence reconcile still
    /// runs inside `ensure_lane` for an offline-announced lane), if an open
    /// is already in flight, or inside the [`LANE_OPEN_RETRY`] backoff — so
    /// opens are neither duplicated nor re-spammed per tick. Lock order:
    /// lifecycle first, then one leaf at a time, never nested leaves.
    fn spawn_pending_lanes(self: &std::sync::Arc<Self>) {
        let lifecycle = self.lifecycle.lock().unwrap_or_else(PoisonError::into_inner);
        if !*lifecycle {
            return;
        }
        let Some(client) = self.client.lock().unwrap_or_else(PoisonError::into_inner).clone()
        else {
            return;
        };
        let keys: Vec<String> = self
            .peers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(k, _)| k.clone())
            .collect();
        for key in keys {
            // leaf checks first — each taken and released before the next
            if self
                .open_retry_after
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&key)
                .is_some_and(|t| std::time::Instant::now() < *t)
            {
                continue;
            }
            let pid = crate::relay::PeerId::from_wg_pubkey_string(&key);
            if self.attached.lock().unwrap_or_else(PoisonError::into_inner).contains_key(&key)
                && !client.is_offline(&pid)
            {
                continue;
            }
            // Single short critical section: vacant check → spawn → insert
            // (generational entry — see `finish_open`; an aborted-but-never-
            // yielded old task finishing late removes only its OWN (key, gen)
            // entry, never a newer task's).
            let mut pending =
                self.pending_opens.lock().unwrap_or_else(PoisonError::into_inner);
            if pending.contains_key(&key) {
                continue;
            }
            // Bounded metadata (0018 diagnostics): distinguishes "pump never
            // ran" from "open never resolved". Rate-limited by the per-peer
            // single-flight guard + backoff; the peer id is the public
            // `sha-` form — no payload, no key material.
            hilog::emit(&format!("connector: relay lane open waiting|peer={}", pid));
            let gen = self.next_open_gen.fetch_add(1, Ordering::Relaxed);
            let this = self.clone();
            let task_key = key.clone();
            let handle = tokio::spawn(async move {
                let _ = this.ensure_lane(&task_key).await;
                this.finish_open(&task_key, gen);
            });
            pending.insert(key, (gen, handle));
        }
    }

    /// Unregister one pending open — but ONLY if `gen` still owns `key`.
    /// (T0-0018 follow-up: bare-key removal had an ABA — a drained-then-
    /// re-registered key made the old task's tail delete the NEW task's
    /// entry, leaving it unmanaged by abort/shutdown and breaking
    /// single-flight.)
    fn finish_open(&self, key: &str, gen: u64) {
        let mut pending = self.pending_opens.lock().unwrap_or_else(PoisonError::into_inner);
        if pending.get(key).is_some_and(|(g, _)| *g == gen) {
            pending.remove(key);
        }
    }

    /// Abort every pending lane open (client swap / pump exit / teardown).
    /// Aborting removes the entry AND the task, so nothing can attach later;
    /// a task already past its last await is unaffected by `abort` — its
    /// commit re-validates under the lifecycle lock and drops.
    fn abort_pending_lanes(&self) {
        let handles: Vec<_> =
            self.pending_opens.lock().unwrap_or_else(PoisonError::into_inner).drain().collect();
        if handles.is_empty() {
            return;
        }
        for (_, (_, h)) in handles {
            h.abort();
        }
        hilog::emit("connector: relay pending lane opens aborted");
    }

    /// Reap open tasks that finished WITHOUT self-removing (a panic between
    /// `ensure_lane` and the map cleanup would otherwise block that peer's
    /// lane forever). Cheap (no join, just a finished flag) — run per tick.
    fn reap_finished_lanes(&self) {
        self.pending_opens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|_, (_, h)| !h.is_finished());
    }

    /// Detach every lane (relay revoked / down / teardown). The device then
    /// routes only via a nominated ICE path — or nothing (fail-closed).
    /// Takes the lifecycle lock itself (pump-path entry).
    fn detach_all(&self) {
        let lifecycle = self.lifecycle.lock().unwrap_or_else(PoisonError::into_inner);
        self.detach_all_locked(&lifecycle);
    }

    /// [`RelayCarrier::detach_all`] for callers already holding the lifecycle
    /// lock (attach_client / shutdown): the `_lifecycle` guard documents and
    /// enforces that the section is mutually exclusive with the lane-open
    /// commit.
    fn detach_all_locked(&self, _lifecycle: &MutexGuard<'_, bool>) {
        let mut attached = self.attached.lock().unwrap_or_else(PoisonError::into_inner);
        if attached.is_empty() {
            return;
        }
        for (key, _) in attached.drain() {
            self.wg.detach_carrier(&key);
            self.bump(|s| s.detach_ok += 1);
        }
        hilog::emit("connector: relay carrier lanes detached");
    }

    /// Pump ONE inbound relay frame into the WG device. `false` = the relay
    /// client is gone/stopped (the pump exits on that).
    pub async fn pump_once(&self) -> bool {
        let Some(client) = self.client.lock().unwrap_or_else(PoisonError::into_inner).clone()
        else {
            return false;
        };
        let Some((sender, payload)) = client.recv().await else {
            return false;
        };
        self.ingest(&sender, &payload);
        true
    }

    /// Reverse-map the sender peer id to the registered WG public key and
    /// feed the device's carrier ingress. Unknown senders: dropped+counted.
    pub fn ingest(&self, sender: &crate::relay::PeerId, payload: &[u8]) {
        let key = {
            let peers = self.peers.lock().unwrap_or_else(PoisonError::into_inner);
            peers
                .iter()
                .find(|(_, pid)| pid == sender)
                .map(|(k, _)| k.clone())
        };
        match key {
            Some(key) => {
                // One-shot metadata (0018 diagnostics): proves the pump IS
                // running and the sender mapping MATCHED — the counterpoint
                // to the lane-open markers when a device log is read offline.
                // Fires once per carrier, never per frame.
                if self.stats.lock().unwrap_or_else(PoisonError::into_inner).frames_in == 0 {
                    hilog::emit(&format!(
                        "connector: relay first inbound frame matched|peer={}",
                        sender
                    ));
                }
                let replies = self
                    .wg
                    .handle_carrier_inbound(payload, &key, crate::sys::mono_ms());
                self.bump(|s| {
                    s.frames_in += 1;
                    s.frames_in_replies += replies as u64;
                });
            }
            None => {
                // One-shot metadata (0018 diagnostics): distinguishes "frame
                // arrived from an unregistered sender" (mapping miss / rogue
                // sender) from "pump not running". Fires once, counted always.
                if self.stats.lock().unwrap_or_else(PoisonError::into_inner).frames_in_unknown == 0
                {
                    hilog::emit(&format!(
                        "connector: relay first unknown-sender frame dropped|sender={}",
                        sender
                    ));
                }
                self.bump(|s| s.frames_in_unknown += 1);
            }
        }
    }

    /// Teardown: pump aborted + every pending open aborted + every lane
    /// detached (connector stop / worker end / host-driven teardown).
    /// Idempotent. Under the lifecycle lock (disarm): mutually exclusive with
    /// the lane-open commit — a commit either finished before this (detached
    /// here) or sees the disarmed carrier and drops.
    pub fn shutdown(&self) {
        let mut lifecycle = self.lifecycle.lock().unwrap_or_else(PoisonError::into_inner);
        *lifecycle = false;
        if let Some(h) = self.task.lock().unwrap_or_else(PoisonError::into_inner).take() {
            h.abort();
        }
        self.abort_pending_lanes();
        self.detach_all_locked(&lifecycle);
    }
}

/// The production pump loop: lane opens are FANNED OUT as managed per-peer
/// tasks (never awaited inline — a pending open must not park the pump, 0018)
/// while (and only while) the relay is Ready; inbound frames are pumped
/// between passes. Exits when the client stops (recv drained to `None`) or
/// the state machine reaches `Dead` — teardown additionally aborts this task.
async fn relay_carrier_pump(carrier: std::sync::Arc<RelayCarrier>) {
    loop {
        let Some(client) = carrier
            .client
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        else {
            carrier.abort_pending_lanes();
            return;
        };
        match client.state() {
            crate::relay_client::RelayState::Dead => {
                carrier.abort_pending_lanes();
                return;
            }
            // spawns only — no open is awaited on the pump's path
            crate::relay_client::RelayState::Ready => carrier.spawn_pending_lanes(),
            // relay down: no lane may look like a path (fail-closed)
            _ => carrier.detach_all(),
        }
        tokio::select! {
            // pump inbound frames as they arrive; `false` = client gone
            kept = async {
                loop {
                    if !carrier.pump_once().await {
                        return false;
                    }
                }
            } => {
                carrier.abort_pending_lanes();
                if !kept {
                    return;
                }
            }
            _ = tokio::time::sleep(core::time::Duration::from_millis(500)) => {
                // periodic re-check: lanes to open / relay state changes; also
                // reaps open tasks that died without bookkeeping
                carrier.reap_finished_lanes();
            }
        }
    }
}

trait LockPoison<T> {
    fn lock_poison(&self) -> MutexGuard<'_, T>;
}

impl<T> LockPoison<T> for Mutex<T> {
    /// Poison-tolerant lock: panics must not cross the NAPI boundary
    /// (panic=abort in release), so a poisoned mutex unwraps into its data.
    fn lock_poison(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Immutable snapshot of the connector status (the `connector_status()`
/// payload).
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectorStatus {
    pub running: bool,
    pub state: ConnState,
    pub started_at_unix: Option<i64>,
    pub last_update_unix: Option<i64>,
    pub peer_count: usize,
    pub route_count: usize,
    pub reconnects: u64,
    pub last_error: Option<ErrorClass>,
    pub deadline: SessionDeadline,
    pub renew_attempts: u64,
    pub wg_apply_failed: bool,
    pub wg_apply_errors: u64,
    pub logout_ok: Option<bool>,
    /// N3-7: the worker ENDED BY ITSELF — fatal (auth) or retry-budget
    /// exhaustion, i.e. `running:false` AND `state:failed`. This is the
    /// signal the shell watcher tears the VPN down on (a user stop is NOT
    /// terminal: state is `disconnected`; a reconnecting connector is
    /// `running:true`). Semantics pinned by Rust tests (gap 3).
    pub terminal: bool,
    /// N5c: per-peer ICE summary (counts + endpoint landings + error class;
    /// zero-peered default until a network map registers peers).
    pub ice: IceOrchestratorSummary,
    /// N5d: real signal link state (registered / reconnects / last error
    /// class). All-false default until the link registers.
    pub signal: SignalLinkStatus,
    /// N7: WG data-plane status (feeds / device / REAL readiness + device
    /// counters; `crate::wg_device::WgDataplaneStatus`). All-false default
    /// when the seam has no device capability (registry — test/reference).
    pub wg: crate::wg_device::WgDataplaneStatus,
    /// N8: controlled-recreate state (required/count/exhausted/cooling_down
    /// + reason token) — the shell's trigger to rebuild the connection when
    /// the desired route set diverges from the applied one.
    pub recreate: RecreateStatus,
    /// N13-D1: relay client section (enabled/state/urls/counters/token —
    /// [`RelayStatus`]; counts and shape tokens only, no token material).
    pub relay: RelayStatus,
}

/// N3-7: single definition of "the connector died on its own".
pub fn is_terminal_state(running: bool, state: ConnState) -> bool {
    !running && state == ConnState::Failed
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn opt_unix_json(key: &str, v: Option<i64>) -> String {
    match v {
        Some(t) => jinum(key, t),
        None => format!("\"{key}\":null"),
    }
}

impl ConnectorStatus {
    /// The `connector_status()` JSON document. Count/class-only fields —
    /// no secret material, no server messages (module discipline).
    pub fn to_json(&self) -> String {
        format!(
            "{{{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}}}",
            jbool("running", self.running),
            jstr("state", self.state.as_str()),
            opt_unix_json("started_at_unix", self.started_at_unix),
            opt_unix_json("last_update_unix", self.last_update_unix),
            jnum("peer_count", self.peer_count as u64),
            jnum("route_count", self.route_count as u64),
            jnum("reconnects", self.reconnects),
            format!(
                "\"last_error\":{}",
                self.last_error
                    .as_ref()
                    .map(ErrorClass::to_json)
                    .unwrap_or_else(|| "null".to_string())
            ),
            jstr("session_expiry", self.deadline.as_str()),
            match self.deadline {
                SessionDeadline::At(t) => jinum("session_expires_at_unix", t),
                _ => "\"session_expires_at_unix\":null".to_string(),
            },
            jnum("session_renew_attempts", self.renew_attempts),
            jbool("wg_apply_failed", self.wg_apply_failed),
            jnum("wg_apply_errors", self.wg_apply_errors),
            format!(
                "\"logout_ok\":{}",
                self.logout_ok.map(|b| b.to_string()).unwrap_or_else(|| "null".to_string())
            ),
            jbool("terminal", self.terminal),
            format!("\"ice\":{}", self.ice.to_json()),
            format!("\"signal\":{}", self.signal.to_json()),
            format!("\"wg\":{}", self.wg.to_json()),
            format!("\"recreate\":{}", self.recreate.to_json()),
            // N13 loop-dependency fix: the management ADVERTISEMENT rides the
            // document ALWAYS (also `relay_enabled:false`), BEFORE the frozen
            // `relay` runtime object (whose byte shape the compatibility
            // tests pin, relay last in the document).
            format!("\"relay_advertised\":{}", self.relay.advertised_to_json()),
            format!("\"relay\":{}", self.relay.to_json()),
        )
    }
}

// ---------------------------------------------------------------------------
// shell network-config snapshot (N3-6)
// ---------------------------------------------------------------------------

/// `connector_network_config()` answer before the first NetworkMap is
/// applied (also after stop: the applied config is torn down with it).
const NO_NETWORK_MAP_JSON: &str = "{\"available\":false,\"reason\":\"no-network-map\"}";

/// One managed route of the shell snapshot: canonical masked network plus
/// the default-route mark (`0.0.0.0/0`, [`crate::config::Route::is_default`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellRouteEntry {
    pub network: String,
    pub is_default: bool,
}

/// One resolver of the shell snapshot (management order, deduplicated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellDnsServer {
    pub ip: String,
    pub port: u16,
}

/// Per-peer summary: public key (PUBLIC material — safe to export) and the
/// allowed-ips COUNT. No key material beyond the public key and no endpoint
/// data exists at this layer (no signal/ICE — module limitation statement).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellPeerSummary {
    pub pub_key_b64: String,
    pub allowed_ips: usize,
    /// The peer's allowed IPs as dotted-quad/prefix strings (the peer's VPN
    /// address lives here). Public runtime material — the interop CLI reads
    /// this for probe targeting (`--probe-dst`); nothing secret.
    pub vpn_addresses: Vec<String>,
}

/// Read-only snapshot of the shell-applicable subset of the LAST APPLIED
/// NetworkMap: own tunnel address, managed routes, DNS and the peer summary.
/// Built at apply time in [`ConnectorShared::apply_update`] and served by
/// `connector_network_config()`; runtime changes after the shell applied a
/// snapshot are the shell's to observe (HarmonyOS VpnConfig is fixed at
/// create() time — see docs/n3-shell-integration-notes.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellNetworkConfig {
    pub serial: u64,
    /// Own tunnel address as assigned by management (e.g. "10.64.0.7").
    pub address: Option<String>,
    /// 32 when `address` parses as IPv4 (NetBird assigns a host address);
    /// `None` when unparseable (e.g. IPv6 — not supported yet).
    pub address_prefix_len: Option<u8>,
    /// Management DNS resolver for the interface (`PeerConfig.dns`).
    pub interface_dns: Option<String>,
    pub routes: Vec<ShellRouteEntry>,
    pub dns_service_enable: bool,
    pub dns_servers: Vec<ShellDnsServer>,
    pub peers: Vec<ShellPeerSummary>,
    /// N3-7 default-route safety gate: TRUE only when the data plane can
    /// actually carry traffic (registered peers + WG `tunnel_ready()`), or
    /// when the dev opt-in `force_default_route` was honored.
    pub default_route_allowed: bool,
    /// Stable reason token: `default-route-allowed:*` /
    /// `default-route-held:*` / `default-route-forced:*`. The held tokens
    /// name the exact reason the `0.0.0.0/0` route was NOT exported.
    pub default_route_reason: String,
    /// N5d: the signal server URI from `netbird_config.signal`
    /// (`host:port`, possibly scheme-prefixed). The shell resolves it
    /// (DNS shell-side), opens + protects a socket and feeds it back via
    /// `connector_signal_socket_feed(fd, addr)`. `None` = no signal config
    /// seen yet (peers cannot connect without it).
    pub signal: Option<String>,
    /// N2-H (design §13.13, I2): `netbird_config.stuns[].uri` VERBATIM, as
    /// announced by the last sync that carried a netbird-config — the shell's
    /// channel for deriving the stun class of the endpoint exclusion set. The
    /// shell applies the SAME parse core applies to the same strings
    /// (`crate::ice::parse_stun_uri`: `stun:host[:port]`, default port 3478,
    /// `turn:` / other schemes refused), so what it freezes is exactly what
    /// core gathers against; `turn:` URIs are refused shell-side with a named
    /// reason (this build has no turn path). Empty = nothing announced: the
    /// honest gap, never an invented endpoint. Public routing material — no
    /// secret crosses this boundary.
    pub stuns: Vec<String>,
}

/// Canonical dotted-quad/prefix rendering of a parsed route network.
fn route_network_string(route: &config::Route) -> String {
    format!(
        "{}.{}.{}.{}/{}",
        route.addr[0], route.addr[1], route.addr[2], route.addr[3], route.prefix_len
    )
}

impl ShellNetworkConfig {
    /// The N3-7 default-route safety gate (pure function, unit-tested):
    ///
    /// - `0.0.0.0/0` is exported for shell install ONLY when at least one
    ///   peer is registered AND the WG data plane reports `tunnel_ready()`.
    ///   With the default registry seam that combination cannot happen
    ///   (registry readiness is always false — no shell-fed WG socket ⇒ no
    ///   handshake); N6's device-backed seam
    ///   ([`crate::wg_device::WgDeviceApplier`]) reports REAL session state,
    ///   so the gate now means "the data plane can actually carry traffic".
    ///   Without that, the default route stays HELD — installing it without
    ///   a working data plane is a traffic black hole.
    /// - `force=true` (explicit dev opt-in `force_default_route`) overrides
    ///   the hold and carries the black-hole warning token.
    pub fn default_route_decision(
        peer_count: usize,
        tunnel_ready: bool,
        force: bool,
    ) -> (bool, String) {
        if force {
            (true, "default-route-forced:debug-opt-in-black-hole-risk".to_string())
        } else if peer_count == 0 {
            (false, "default-route-held:no-usable-peer".to_string())
        } else if !tunnel_ready {
            (false, "default-route-held:data-plane-not-ready".to_string())
        } else {
            (true, "default-route-allowed:peers-registered-and-tunnel-ready".to_string())
        }
    }

    /// Reduce a converted NetworkMap to the shell-applicable subset. Peers
    /// include offline peers (the WG seam registers both). The default-route
    /// gate decides whether `0.0.0.0/0` stays in `routes`: when held, the
    /// entry is REMOVED (a mapping-only shell cannot install what is not
    /// exported) and the reason token records the hold.
    pub fn from_map_gated(
        map: &NetworkMap,
        force_default_route: bool,
        tunnel_ready: bool,
    ) -> ShellNetworkConfig {
        // NetBird delivers "ip/prefix" ("100.102.55.28/16"): the platform
        // VpnConfig LinkAddress takes a BARE IP plus a separate prefixLength.
        // Device-verified 2026-09-13: handing the raw "ip/prefix" string to
        // the shell made NETMANAGER_EXT reject the whole config
        // ("invalid ip address"/"ParseAddress failed", code 401), and the
        // previous hardcoded 32 produced a /32 host route for a /16 overlay.
        // So: strip the suffix AND use the value it carries.
        let raw_address = map.peer.as_ref().and_then(|p| p.address.clone());
        let (address, address_prefix_len) = match raw_address.as_deref() {
            Some(raw) => {
                let mut parts = raw.splitn(2, '/');
                let ip = parts.next().unwrap_or("").trim().to_string();
                let prefix = parts.next().and_then(|p| p.trim().parse::<u8>().ok());
                if config::parse_ipv4(&ip).is_some() {
                    (Some(ip), Some(prefix.unwrap_or(32)))
                } else {
                    // IPv6 or unparseable: do not hand a bad address to the shell.
                    (None, None)
                }
            }
            None => (None, None),
        };
        let peer_count = map.peers.len() + map.offline_peers.len();
        let (default_route_allowed, default_route_reason) =
            Self::default_route_decision(peer_count, tunnel_ready, force_default_route);
        let mut routes = Vec::with_capacity(map.routes.len());
        for r in &map.routes {
            let is_default = r.network.is_default();
            if is_default && !default_route_allowed {
                // held: not exported for install — the reason token is the
                // observable proof (see to_json)
                continue;
            }
            routes.push(ShellRouteEntry {
                network: route_network_string(&r.network),
                is_default,
            });
        }
        let mut dns_servers: Vec<ShellDnsServer> = Vec::new();
        if let Some(dns) = map.dns.as_ref() {
            for group in &dns.name_server_groups {
                for ns in &group.name_servers {
                    // dedup (one resolver may repeat across groups), keep
                    // management order — the shell consumes it as priority
                    if !dns_servers
                        .iter()
                        .any(|s| s.ip == ns.ip && s.port == ns.port)
                    {
                        dns_servers.push(ShellDnsServer { ip: ns.ip.clone(), port: ns.port });
                    }
                }
            }
        }
        let peers = map
            .peers
            .iter()
            .chain(map.offline_peers.iter())
            .map(|p| ShellPeerSummary {
                pub_key_b64: p.wg_pub_key.clone(),
                allowed_ips: p.allowed_ips.len(),
                vpn_addresses: p
                    .allowed_ips
                    .iter()
                    .map(route_network_string)
                    .collect(),
            })
            .collect();
        ShellNetworkConfig {
            serial: map.serial,
            address,
            address_prefix_len,
            interface_dns: map.peer.as_ref().and_then(|p| p.interface_dns.clone()),
            routes,
            dns_service_enable: map.dns.as_ref().map(|d| d.service_enable).unwrap_or(false),
            dns_servers,
            peers,
            default_route_allowed,
            default_route_reason,
            // N5d: the caller (apply_update) stamps the signal URI on the
            // snapshot — from_map_gated sees only the map, the URI rides the
            // sync config.
            signal: None,
            // N2-H I2 (design §13.13): same stamping discipline for the
            // announced `stuns[]` URIs (they ride the sync config too).
            stuns: Vec::new(),
        }
    }

    /// The `connector_network_config()` payload (with the `available` mark).
    /// Public keys are PUBLIC material; nothing secret crosses this boundary
    /// (module credential discipline).
    pub fn to_json(&self) -> String {
        let mut routes = String::new();
        for (i, r) in self.routes.iter().enumerate() {
            if i > 0 {
                routes.push(',');
            }
            routes.push_str(&format!(
                "{{{},{}}}",
                jstr("network", &r.network),
                jbool("is_default", r.is_default)
            ));
        }
        let mut dns_servers = String::new();
        for (i, s) in self.dns_servers.iter().enumerate() {
            if i > 0 {
                dns_servers.push(',');
            }
            dns_servers.push_str(&format!(
                "{{{},{}}}",
                jstr("ip", &s.ip),
                jnum("port", s.port as u64)
            ));
        }
        let mut peers = String::new();
        for (i, p) in self.peers.iter().enumerate() {
            if i > 0 {
                peers.push(',');
            }
            let vpn = p
                .vpn_addresses
                .iter()
                .map(|a| format!("\"{a}\""))
                .collect::<Vec<String>>()
                .join(",");
            peers.push_str(&format!(
                "{{{},{},\"vpn_addresses\":[{}]}}",
                jstr("pub_key", &p.pub_key_b64),
                jnum("allowed_ips", p.allowed_ips as u64),
                vpn
            ));
        }
        let address = match self.address.as_ref() {
            Some(a) => jstr("address", a),
            None => "\"address\":null".to_string(),
        };
        let address_prefix_len = match self.address_prefix_len {
            Some(p) => jinum("address_prefix_len", p as i64),
            None => "\"address_prefix_len\":null".to_string(),
        };
        let interface_dns = match self.interface_dns.as_ref() {
            Some(d) => jstr("interface_dns", d),
            None => "\"interface_dns\":null".to_string(),
        };
        let signal = match self.signal.as_ref() {
            Some(u) => jstr("signal", u),
            None => "\"signal\":null".to_string(),
        };
        // N2-H I2 (design §13.13): the announced `stuns[]` URIs as
        // `[{"uri":"…"}]`, TAIL-appended after every pre-existing field (the
        // a1947b9 tail-append discipline: existing field order and shape are
        // untouched, consumers read fields by name). Empty when nothing was
        // announced — an honest empty array, never a fabricated endpoint.
        let mut stuns = String::new();
        for (i, uri) in self.stuns.iter().enumerate() {
            if i > 0 {
                stuns.push(',');
            }
            stuns.push_str(&format!("{{{}}}", jstr("uri", uri)));
        }
        format!(
            "{{{},{},{},{},{},{},\"routes\":[{}],\"dns\":{{{},{}}},{},\"peers\":[{}],\"default_route\":{{{},{}}},\"stuns\":[{}]}}",
            jbool("available", true),
            jnum("serial", self.serial),
            address,
            address_prefix_len,
            interface_dns,
            signal,
            routes,
            jbool("service_enable", self.dns_service_enable),
            format!("\"servers\":[{dns_servers}]"),
            jnum("peer_count", self.peers.len() as u64),
            peers,
            jbool("allowed", self.default_route_allowed),
            jstr("reason", &self.default_route_reason),
            stuns,
        )
    }
}

/// N13-D1: the reason token when the default route is enabled by the RELAY
/// carrier (and NOT by an ICE nomination) — stable, JSON-safe, mirrors the
/// shape of the pre-N13 `…-tunnel-ready` token.
pub const DEFAULT_ROUTE_REASON_RELAY_READY: &str =
    "default-route-allowed:peers-registered-and-relay-ready";

/// N13-D1 two-path data-plane readiness (T0 Q3, pure — unit-pinned):
///
/// `tunnel_ready && (ice_nominated || relay_ready)`
///
/// - (b) ICE 已提名 → 就绪（既有语义，relay 无关）；
/// - (a) 仅 relay Ready（且 WG 会话在）→ 就绪，理由 token 用
///   [`DEFAULT_ROUTE_REASON_RELAY_READY`];
/// - (c) ICE Failed 且 relay 未就绪 → **不得**判就绪（默认路由继续 HOLD，
///   绝不静默当作可达）;
/// - 双路径并存时互不污染：relay 的字节/帧计数只存在于 relay 客户端统计，
///   [`crate::peer_conn::IceOrchestratorSummary`] 不接收任何 relay 输入，
///   反之亦然（各自独立计数与判定）。
fn two_path_readiness(tunnel_ready: bool, ice_nominated: bool, relay_ready: bool) -> bool {
    tunnel_ready && (ice_nominated || relay_ready)
}

/// `RelayState` → stable status token (state only — no URLs/peers here).
fn relay_state_token(state: crate::relay_client::RelayState) -> &'static str {
    use crate::relay_client::RelayState as S;
    match state {
        S::Disconnected => "disconnected",
        S::Dialing => "dialing",
        S::Handshaking => "handshaking",
        S::Authenticating => "authenticating",
        S::Ready => "ready",
        S::Reconnecting => "reconnecting",
        S::Dead => "dead",
    }
}

/// [`crate::relay_client::RelayErrorClass`] → stable lowercase token for the
/// status document (class only — never a server/error message).
fn relay_error_class_token(class: crate::relay_client::RelayErrorClass) -> &'static str {
    use crate::relay_client::RelayErrorClass as C;
    match class {
        C::Url => "url",
        C::TlsRequired => "tls-required",
        C::Tls => "tls",
        C::Dial => "dial",
        C::DialTimeout => "dial-timeout",
        C::WsHandshake => "ws-handshake",
        C::Ws => "ws",
        C::Codec => "codec",
        C::FrameTooLarge => "frame-too-large",
        C::AuthRejected => "auth-rejected",
        C::AuthTimeout => "auth-timeout",
        C::AuthProtocol => "auth-protocol",
        C::ServerClosed => "server-closed",
        C::Io => "io",
        C::KeepaliveTimeout => "keepalive-timeout",
        C::TokenExpired => "token-expired",
        C::ConnectionLost => "connection-lost",
        C::OpenConnTimeout => "open-conn-timeout",
        C::SubscribeDuplicate => "subscribe-duplicate",
        C::PeerOffline => "peer-offline",
        C::Backpressured => "backpressured",
        C::NotReady => "not-ready",
        C::Stopped => "stopped",
    }
}

/// The `relay` section of the status document (N13-D1). Counters and shape
/// tokens ONLY: no token material, no peer ids, no server messages — the
/// credential discipline of the whole status boundary applies.
///
/// Two information classes live here, and they must never be read as each
/// other:
/// - ADVERTISEMENT (always present, also while the client is disabled):
///   [`RelayStatus::advertised`] + [`RelayStatus::advertised_urls`] — what
///   MANAGEMENT offers (`NetbirdConfig.relay`), not what this connector
///   uses. Rendered in the separate `relay_advertised` JSON section.
/// - RUNTIME (only meaningful while `enabled`): `enabled`, `state`, `urls`,
///   the counters and `token_valid`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayStatus {
    /// The `relay_enabled` opt-in (hard default false — device path unchanged).
    pub enabled: bool,
    /// Client state token (`disabled` when not enabled).
    pub state: String,
    /// The configured relay URL set from the last `NetbirdConfig.relay`
    /// (public routing material).
    pub urls: Vec<String>,
    /// Management ADVERTISES relay in the latest sync (`advertised_urls`
    /// non-empty). Advertisement is NOT usage — the runtime fields above
    /// carry the running view.
    pub advertised: bool,
    /// The management-advertised relay URL set (`NetbirdConfig.relay.urls`)
    /// from the LAST sync carrying a netbird config — recorded ALWAYS, also
    /// while `enabled:false` (the shell freezes its N2-H relay endpoint
    /// exclusions from this BEFORE enabling relay). Public routing material
    /// only; never the relay token.
    pub advertised_urls: Vec<String>,
    /// Established sessions that ended and triggered a reconnect.
    pub reconnects: u64,
    /// TOTAL relay frames sent (all types — reconciliation surface).
    pub frames_tx: u64,
    /// TOTAL relay frames received (all types).
    pub frames_rx: u64,
    /// Transport payload bytes, outbound + inbound (§4.2).
    pub transport_bytes: u64,
    /// The current token parses and has NOT expired against the clock
    /// (fail-closed visibility: `false` ⇒ the relay is/becomes unusable).
    pub token_valid: bool,
    /// Last failure class token (never a message).
    pub last_error_class: Option<String>,
}

impl RelayStatus {
    /// The disabled shape (the pre-N13 device path): zero counters, no urls,
    /// nothing ever dialed.
    pub fn disabled() -> Self {
        RelayStatus {
            enabled: false,
            state: "disabled".to_string(),
            urls: Vec::new(),
            advertised: false,
            advertised_urls: Vec::new(),
            ..RelayStatus::empty_counters()
        }
    }

    fn empty_counters() -> Self {
        RelayStatus {
            enabled: true,
            state: "disconnected".to_string(),
            urls: Vec::new(),
            advertised: false,
            advertised_urls: Vec::new(),
            reconnects: 0,
            frames_tx: 0,
            frames_rx: 0,
            transport_bytes: 0,
            token_valid: false,
            last_error_class: None,
        }
    }

    /// JSON object (embedded into the status document). FROZEN SHAPE: the
    /// `relay` object carries the RUNTIME view only — the advertised view
    /// renders separately via [`RelayStatus::advertised_to_json`] so an
    /// advertised url can never be misread as a relay session in use.
    pub fn to_json(&self) -> String {
        let urls = self
            .urls
            .iter()
            .map(|u| format!("\"{u}\""))
            .collect::<Vec<_>>()
            .join(",");
        let last_error = self
            .last_error_class
            .as_ref()
            .map(|c| format!("\"{c}\""))
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{{},{},{},{},{},{},{},{},{}}}",
            jbool("enabled", self.enabled),
            jstr("state", &self.state),
            format!("\"urls\":[{urls}]"),
            jnum("reconnects", self.reconnects),
            jnum("frames_tx", self.frames_tx),
            jnum("frames_rx", self.frames_rx),
            jnum("transport_bytes", self.transport_bytes),
            jbool("token_valid", self.token_valid),
            format!("\"last_error_class\":{last_error}"),
        )
    }

    /// The status document's `relay_advertised` section — the management
    /// ADVERTISEMENT (`advertised` + `advertised_urls`), deliberately OUT of
    /// the frozen `relay` runtime object: advertisement and usage must not
    /// be readable as each other. URLs only — never token material.
    pub fn advertised_to_json(&self) -> String {
        let urls = self
            .advertised_urls
            .iter()
            .map(|u| format!("\"{u}\""))
            .collect::<Vec<_>>()
            .join(",");
        format!("{{\"advertised\":{},\"advertised_urls\":[{urls}]}}", self.advertised)
    }
}

// ---------------------------------------------------------------------------
// sync policy (injected backoff / clock / rng)
// ---------------------------------------------------------------------------

/// Injectable sync policy handed to [`crate::sync::SyncSession::with_policy`].
/// Production: upstream stream backoff preset + OS randomness + monotonic
/// clock (`crate::backoff` module docs). Tests inject short deterministic
/// policies.
pub struct SyncPolicy {
    pub backoff: ExponentialBackoff,
    pub rng: Box<dyn Rng + Send>,
    pub clock: Box<dyn Clock + Send>,
}

impl SyncPolicy {
    /// Upstream-shaped production policy.
    pub fn production() -> Self {
        SyncPolicy {
            backoff: ExponentialBackoff::upstream_stream_default(),
            rng: Box::new(OsRandom),
            clock: Box::new(MonotonicClock),
        }
    }
}

// ---------------------------------------------------------------------------
// N5d: real signal link (netbird_config.signal → SignalSession worker)
// ---------------------------------------------------------------------------

/// Per-spawn static material for the signal link (N5d). Production start
/// paths build it from the parsed [`ConnectorConfig`]; host tests pass
/// `None` and simply run without a signal link.
#[derive(Clone)]
pub struct SignalMaterial {
    pub transport: GrpcTransport,
    pub keys: EnvelopeKeyPair,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
}

/// signal 通道状态快照（`connector_status()` 的 `signal` 字段）。只有
/// 注册态 / 重连计数 / 错误分类——无消息文本（凭据纪律）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalLinkStatus {
    /// `ConnectStream` 当前已注册（`Registered` 事件置真，`Broken`/
    /// worker 结束置假）。
    pub registered: bool,
    /// 流断开重连次数（初始注册不计；`crate::signal` 诊断口径）。
    pub reconnects: u64,
    pub last_error: Option<ErrorClass>,
}

impl Default for SignalLinkStatus {
    fn default() -> Self {
        SignalLinkStatus { registered: false, reconnects: 0, last_error: None }
    }
}

impl SignalLinkStatus {
    fn to_json(&self) -> String {
        format!(
            "{{{},{},{}}}",
            jbool("registered", self.registered),
            jnum("reconnects", self.reconnects),
            format!(
                "\"last_error\":{}",
                self.last_error
                    .as_ref()
                    .map(ErrorClass::to_json)
                    .unwrap_or_else(|| "null".to_string())
            ),
        )
    }
}

/// `netbird_config.signal` URI → gRPC endpoint + transport。
///
/// 上游 `connectToSignal`（`client/internal/connect.go:715-731`）按
/// `HostConfig.protocol == HTTPS` 决定 TLS（L717-721）；本仓网络图只保留
/// `uri`（`crate::network_map`，上游 engine.go:1185 注 "todo update
/// signal"，protocol 字段未进模型）。因此：显式 scheme 按 scheme；
/// 裸 `host:port`（线上形态，如 `signal.netbird.io:10000`）**继承
/// management 传输的安全等级**——两者共用同一注入 CA（无系统根存储）。
fn derive_signal_endpoint(
    uri: &str,
    mgmt: &GrpcTransport,
) -> Result<(String, GrpcTransport), ConfigError> {
    let bad = |reason: &'static str| {
        ConfigError::Field { field: "signal", reason: reason.into() }
    };
    if let Some(rest) = uri.strip_prefix("https://") {
        if rest.is_empty() {
            return Err(bad("empty https authority"));
        }
        match mgmt {
            GrpcTransport::Tls(_) => Ok((uri.to_string(), mgmt.clone())),
            GrpcTransport::Plaintext => Err(bad(
                "https:// signal uri requires TLS material (injected CA), but management is plaintext",
            )),
        }
    } else if let Some(rest) = uri.strip_prefix("http://") {
        if rest.is_empty() {
            return Err(bad("empty http authority"));
        }
        Ok((uri.to_string(), GrpcTransport::Plaintext))
    } else if uri.is_empty() {
        Err(bad("empty uri"))
    } else {
        // bare host:port → inherit the management transport's security level
        match mgmt {
            GrpcTransport::Tls(_) => Ok((format!("https://{uri}"), mgmt.clone())),
            GrpcTransport::Plaintext => Ok((format!("http://{uri}"), GrpcTransport::Plaintext)),
        }
    }
}

/// signal link 的静态启动材料（spawn 时一次性注入 [`SignalRuntime`]）。
struct SignalLinkMaterials {
    runtime: tokio::runtime::Handle,
    transport: GrpcTransport,
    connect_timeout: Duration,
    request_timeout: Duration,
    keys: EnvelopeKeyPair,
    /// 受保护 signal socket 源——壳侧经 `connector_signal_socket_feed`
    /// 补给；空源 → 每次拨号 fail-closed（绝无未保护回退）。
    sockets: Arc<ProtectedSocketFdSource>,
}

/// 连接器持有的真实 signal 链路（N5d）：惰性启动（`netbird_config.signal`
/// URI 与壳侧 `connect_addr` 都到位后），收帧路由进 per-peer ICE 编排，
/// 事件落 [`SignalLinkStatus`]。适配器（exchange）与出帧队列在构造时
/// 一次性建立并交给编排 seam——同一实例贯穿 send 侧与 worker 侧。
struct SignalRuntime {
    materials: std::sync::OnceLock<SignalLinkMaterials>,
    orch: std::sync::OnceLock<Arc<Mutex<PeerIceOrchestrator>>>,
    /// 最近一次 sync 看到的 signal URI（首次非空生效；上游 engine.go:1185
    /// "todo update signal"——运行中变更不支持，如实记录）。
    uri: Mutex<Option<String>>,
    /// 壳侧首次 feed 的已解析地址（先到先得，与 management 的固定
    /// connect_addr 同约定；后续 feed 只补给 fd）。
    connect_addr: Mutex<Option<SocketAddr>>,
    /// 运行中的 worker 任务句柄（stop 时 abort）。
    link: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// 编排 seam 持有的同一适配器（registered 闸的真源）。
    exchange: Arc<RealSignalExchange>,
    /// 出帧队列接收端（maybe_start 时交给 worker；已取走即已启动）。
    outbox: Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<SignalOutgoing>>>,
    state: Arc<Mutex<SignalLinkStatus>>,
}

impl SignalRuntime {
    fn new(
        materials: SignalLinkMaterials,
        exchange: Arc<RealSignalExchange>,
        outbox: tokio::sync::mpsc::UnboundedReceiver<SignalOutgoing>,
    ) -> Self {
        SignalRuntime {
            materials: {
                let m = std::sync::OnceLock::new();
                let _ = m.set(materials);
                m
            },
            orch: std::sync::OnceLock::new(),
            uri: Mutex::new(None),
            connect_addr: Mutex::new(None),
            link: Mutex::new(None),
            exchange,
            outbox: Arc::new(tokio::sync::Mutex::new(outbox)),
            state: Arc::new(Mutex::new(SignalLinkStatus::default())),
        }
    }

    fn record(&self, f: impl FnOnce(&mut SignalLinkStatus)) {
        let mut g = self.state.lock_poison();
        f(&mut g);
    }

    fn status(&self) -> SignalLinkStatus {
        let registered = self.exchange.is_registered();
        let g = self.state.lock_poison();
        SignalLinkStatus { registered, ..g.clone() }
    }

    /// sync 带来 signal URI：记录（首次非空生效）。
    fn set_uri(&self, uri: &str) {
        let mut g = self.uri.lock_poison();
        if g.is_none() {
            *g = Some(uri.to_string());
        }
    }

    /// 壳侧 feed 的已解析地址（首次生效）。
    fn set_connect_addr(&self, addr: SocketAddr) {
        *self.connect_addr.lock_poison() = Some(addr);
    }

    /// URI 与 connect_addr 齐备且尚未启动 → 启动 worker（幂等）。返回
    /// 是否真的启动了。
    fn maybe_start(&self) -> bool {
        let (Some(materials), Some(orch)) = (self.materials.get(), self.orch.get()) else {
            return false;
        };
        let (Some(uri), Some(addr)) = (
            self.uri.lock_poison().clone(),
            self.connect_addr.lock_poison().as_ref().copied(),
        ) else {
            return false; // 前置未齐：URI 来自 sync，addr 来自壳侧 feed
        };
        let mut link = self.link.lock_poison();
        if link.is_some() {
            return false;
        }
        let rx = self.outbox.clone();
        // endpoint 派生失败（如 https URI 但无 TLS 材料）→ 显式失败，不启动
        let (endpoint, transport) = match derive_signal_endpoint(&uri, &materials.transport) {
            Ok(t) => t,
            Err(e) => {
                self.record(|s| {
                    s.last_error = Some(ErrorClass::Parse);
                });
                hilog::emit(&format!(
                    "connector: signal link NOT started (parse class: {})",
                    e
                ));
                return false;
            }
        };
        let state = self.state.clone();
        let orch_cb = orch.clone();
        let on_event: Arc<dyn Fn(SignalLinkEvent) + Send + Sync> = Arc::new(move |ev| match ev {
            SignalLinkEvent::DialFailed(_) => {
                // 初始受保护拨号失败：fail-closed，记录分类后由 worker 退避重试
                state.lock_poison().last_error = Some(ErrorClass::Network);
                hilog::emit("connector: signal protected dial failed (network), will retry");
            }
            SignalLinkEvent::Registered => {
                orch_cb.lock_poison().set_signal_ready(true);
                hilog::emit("connector: signal stream registered (ice initiation armed)");
            }
            SignalLinkEvent::Message(m) => route_signal_message(&orch_cb, &m),
            SignalLinkEvent::Malformed => {
                // 帧级失败已上报、流保持（crate::signal；grpc.go:600-602）
                hilog::emit("connector: signal malformed frame (stream stays up)");
            }
            SignalLinkEvent::Broken(_) => {
                let mut g = state.lock_poison();
                g.reconnects += 1;
                g.last_error = Some(ErrorClass::Network);
                drop(g);
                orch_cb.lock_poison().set_signal_ready(false);
                hilog::emit("connector: signal stream broken (backoff, will re-register)");
            }
            SignalLinkEvent::Ended(result) => {
                if let Err(e) = result {
                    state.lock_poison().last_error = Some(ErrorClass::from_management(&e));
                }
                orch_cb.lock_poison().set_signal_ready(false);
                hilog::emit("connector: signal worker ended");
            }
        });
        let handle = spawn_signal_link_shared(
            materials.runtime.clone(),
            SignalLinkConfig {
                endpoint,
                transport,
                connect_timeout: materials.connect_timeout,
                request_timeout: materials.request_timeout,
                keys: materials.keys.clone(),
                sockets: materials.sockets.clone(),
                connect_addr: addr,
            },
            self.exchange.clone(),
            rx,
            on_event,
        );
        *link = Some(handle);
        hilog::emit("connector: signal link started (real SignalSession worker)");
        true
    }

    /// connector stop：abort worker、放掉注册态。幂等。
    fn shutdown(&self) {
        if let Some(h) = self.link.lock_poison().take() {
            // abort 路径跳过 worker 的收尾（Ended 事件），这里补齐注册态
            self.exchange.mark_unregistered();
            h.abort();
        }
        self.record(|s| s.registered = false);
    }
}

/// 收帧路由（N5d 核心规则）：`from_key` = 发送方 WG 公钥（信封
/// `EncryptedMessage.key`，grpc.go:414-431），与网络图 peer 的
/// `wg_pub_key` 同一身份域——`PeerIceOrchestrator::handle_signal` 正是按
/// 该键匹配 peer（engine.go:2043-2045 按消息 key 找 peerConn 的同型；
/// 未知 key 由编排层丢弃并计数）。HEARTBEAT/MODE/GO_IDLE 与 ICE 无关
/// （engine.go:2036-2040 心跳短路；GO_IDLE 走 connMgr，非本增量）。
fn route_signal_message(orch: &Mutex<PeerIceOrchestrator>, m: &SignalMessage) {
    use crate::signal::proto::body::Type;
    let kind = match m.kind {
        Type::Offer => PeerSignalKind::Offer,
        Type::Answer => PeerSignalKind::Answer,
        Type::Candidate => PeerSignalKind::Candidate,
        _ => return,
    };
    let now = crate::sys::mono_ms();
    // Frame-level diagnostics: everything else about a stalled negotiation is
    // silent (device run 5: both sides looped on negotiation-start-timeout
    // with unknown_signal=0, so "were frames routed at all?" was unanswerable).
    hilog::emit(&format!(
        "connector: signal frame routed (kind={})",
        match kind {
            PeerSignalKind::Offer => "offer",
            PeerSignalKind::Answer => "answer",
            PeerSignalKind::Candidate => "candidate",
        }
    ));
    let mut guard = orch.lock_poison();
    if let Err(e) = guard.handle_signal(&m.from_key, kind, &m.payload, now) {
        if kind == PeerSignalKind::Candidate {
            let fields: Vec<&str> = m.payload.split_whitespace().take(9).collect();
            let candidate_type = match fields.get(7).copied() {
                Some("host") => "host", Some("srflx") => "srflx",
                Some("prflx") => "prflx", Some("relay") => "relay", _ => "unknown",
            };
            let transport = match fields.get(2).map(|s| s.to_ascii_lowercase()).as_deref() {
                Some("udp") => "udp", Some("tcp") => "tcp", _ => "unknown",
            };
            let family = match fields.get(4).and_then(|s| s.parse::<std::net::IpAddr>().ok()) {
                Some(std::net::IpAddr::V4(_)) => "ipv4",
                Some(std::net::IpAddr::V6(_)) => "ipv6",
                None => "unknown",
            };
            let reason = match &e {
                ManagementError::Parse(text) => text.strip_prefix("candidate:")
                    .and_then(|s| s.split_once(' ')).map(|(token, _)| token)
                    .filter(|s| matches!(*s, "missing-prefix" | "empty" | "bad-field" |
                        "short" | "transport" | "address" | "port" | "no-typ" |
                        "no-type" | "type" | "raddr" | "rport" | "related-pair"))
                    .unwrap_or("unknown"),
                _ => "unknown",
            };
            hilog::emit(&format!(
                "connector: candidate rejected|reason={reason}|type={candidate_type}|transport={transport}|family={family}|peer={}",
                m.from_key.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect::<String>()
            ));
        } else {
            hilog::emit(&format!(
                "connector: signal frame rejected ({})|peer={}",
                ErrorClass::from_management(&e).as_str(), m.from_key.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect::<String>()
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// the connector handle
// ---------------------------------------------------------------------------

/// Everything the worker/renewal tasks need (owned `Arc`s and clones; no
/// borrowing of the handle).
struct WorkerDeps {
    shared: Arc<ConnectorShared>,
    factory: Arc<dyn ManagementFactory>,
    wg: Arc<dyn WgPeerApplier>,
    host: Arc<dyn ConfigApplier>,
    secrets: ConnectorSecrets,
    meta: PeerMeta,
    policy_backoff: ExponentialBackoff,
    sync_backoff: ExponentialBackoff,
    sync_rng: Box<dyn Rng + Send>,
    sync_clock: Box<dyn Clock + Send>,
}

/// `connector_stop()` outcome (also rendered into JSON).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StopOutcome {
    pub already_stopped: bool,
    pub state: ConnState,
}

/// A started connector: owns the worker/renewal tasks and exposes
/// `status()` / `stop()`. `stop()` is idempotent; a stopped (or
/// fatal-failed) slot is replaced by the next `connector_start`.
pub struct ConnectorHandle {
    shared: Arc<ConnectorShared>,
    wg: Arc<dyn WgPeerApplier>,
    host: Arc<dyn ConfigApplier>,
    /// Network-change generation watched by the management worker.
    network_change: tokio::sync::watch::Sender<u64>,
    management_factory: Arc<dyn ManagementFactory>,
    /// Latest logged-in management client, taken by stop() for logout.
    logout_slot: Arc<Mutex<Option<ManagementGrpcClient>>>,
    stop_flag: Arc<AtomicBool>,
    /// N3-7: the protected-socket source behind the management factory, when
    /// the connector was started through `connector_start_with_socket`.
    /// `connector_socket_feed` resupplies it across the NAPI boundary.
    socket_source: Option<Arc<ProtectedSocketFdSource>>,
    /// N5c: the protected-UDP source behind the ICE orchestrator (resupply
    /// handle for `connector_ice_socket_feed`). `None` = caller-supplied
    /// orchestrator (tests).
    ice_sockets: Option<Arc<crate::ice::ProtectedUdpFdSource>>,
    /// N5d: the protected signal-socket source behind the signal link
    /// (resupply handle for `connector_signal_socket_feed`). `None` = no
    /// signal material provided (host-test construction).
    signal_sockets: Option<Arc<ProtectedSocketFdSource>>,
    /// N7: the production WG data-plane seam when the connector was started
    /// with one (feed handles + the pump owner). `None` = a host-injected
    /// seam (registry / direct device applier — tests).
    wg_feed: Option<Arc<crate::wg_device::WgDeviceFeed>>,
    /// N5c: per-peer ICE orchestrator (status/stop surface).
    ice: Arc<Mutex<PeerIceOrchestrator>>,
    /// N2-H (design §4.0 management row): the shell-resolved address every
    /// protected management dial connects to ([`ManagementFactory::dial_address`]),
    /// captured at spawn. `None` = no fixed dial address (unprotected dev
    /// dial). INTERNAL only — the device collector reads it through
    /// [`core_freeze_inputs`]; the public status/network documents keep their
    /// frozen shape (§5.2).
    management_dial: Option<SocketAddr>,
    worker: Mutex<Option<tokio::task::JoinHandle<()>>>,
    renewal: Mutex<Option<tokio::task::JoinHandle<()>>>,
    runtime: tokio::runtime::Handle,
}

impl core::fmt::Debug for ConnectorHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // stable, secret-free shape (the handle holds no secret material)
        f.debug_struct("ConnectorHandle")
            .field("state", &self.shared.state().as_str())
            .field("running", &self.shared.is_running())
            .finish()
    }
}

impl ConnectorHandle {
    /// Build deps + spawn the worker and session-renewal tasks on
    /// `runtime`. Returns immediately; progress is observed via `status()`.
    /// N3-7: `force_default_route` arms the default-route gate override;
    /// `socket_source` (when set) is the resupply handle for
    /// `connector_socket_feed`. N5c: `ice = None` builds the production
    /// orchestrator (SystemInterfaces + a fresh protected-UDP source fed by
    /// `connector_ice_socket_feed` + the same WG seam) and starts the
    /// injected-clock pump thread; tests may inject a fully-stubbed
    /// orchestrator instead. N5d: `signal_material = Some(..)` attaches the
    /// real signal runtime — the ICE orchestrator's send seam becomes a
    /// [`crate::peer_conn::RealSignalExchange`] and the link starts once
    /// `netbird_config.signal` (sync) and the shell-fed connect address are
    /// both present; `None` keeps the link absent (tests). N7:
    /// `wg_feed = Some(feed)` marks the PRODUCTION device-backed seam: the
    /// handle stores the feed (resupply surface for the `connector_*_feed`
    /// NAPI entries) and spawns the data-plane pump thread on the stop
    /// flag. Tests injecting other seams pass `None`.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        runtime: tokio::runtime::Handle,
        factory: Arc<dyn ManagementFactory>,
        wg: Arc<dyn WgPeerApplier>,
        host: Arc<dyn ConfigApplier>,
        secrets: ConnectorSecrets,
        meta: PeerMeta,
        policy_backoff: ExponentialBackoff,
        sync_policy: SyncPolicy,
        renew_lead: Duration,
        renew_check_interval: Duration,
        force_default_route: bool,
        socket_source: Option<Arc<ProtectedSocketFdSource>>,
        ice: Option<Arc<Mutex<PeerIceOrchestrator>>>,
        signal_material: Option<SignalMaterial>,
        wg_feed: Option<Arc<crate::wg_device::WgDeviceFeed>>,
        ice_tuning: HostIceTuning,
    ) -> Arc<ConnectorHandle> {
        // N13-D1: the pre-existing signature keeps the hard default — NO
        // relay client (relay_enabled=false; the T0 Q3/D2 rationale is on
        // [`ConnectorConfig::relay_enabled`]). Opt-in via
        // [`ConnectorHandle::spawn_with_relay`].
        Self::spawn_with_relay(
            runtime,
            factory,
            wg,
            host,
            secrets,
            meta,
            policy_backoff,
            sync_policy,
            renew_lead,
            renew_check_interval,
            force_default_route,
            socket_source,
            ice,
            signal_material,
            wg_feed,
            ice_tuning,
            false,
            None,
        )
    }

    /// [`ConnectorHandle::spawn`] plus the N13-D1 relay opt-in:
    /// `relay_enabled=true` starts/updates a [`crate::relay_client::RelayClient`]
    /// from every Sync's `NetbirdConfig.relay` (lifecycle in
    /// [`ConnectorShared::relay_apply`]), tears it down on stop/worker end
    /// (no orphan outbound relay connection), and feeds the two-path
    /// readiness rule. `relay_attach.wg_pubkey_b64` is the LOCAL peer's
    /// WireGuard public key (base64) — the relay peerID source (relay spec
    /// §3.2).
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_with_relay(
        runtime: tokio::runtime::Handle,
        factory: Arc<dyn ManagementFactory>,
        wg: Arc<dyn WgPeerApplier>,
        host: Arc<dyn ConfigApplier>,
        secrets: ConnectorSecrets,
        meta: PeerMeta,
        policy_backoff: ExponentialBackoff,
        sync_policy: SyncPolicy,
        renew_lead: Duration,
        renew_check_interval: Duration,
        force_default_route: bool,
        socket_source: Option<Arc<ProtectedSocketFdSource>>,
        ice: Option<Arc<Mutex<PeerIceOrchestrator>>>,
        signal_material: Option<SignalMaterial>,
        wg_feed: Option<Arc<crate::wg_device::WgDeviceFeed>>,
        ice_tuning: HostIceTuning,
        relay_enabled: bool,
        relay_attach: Option<RelayAttach>,
    ) -> Arc<ConnectorHandle> {
        // The flag is authoritative: materials without the opt-in are
        // dropped (the device default keeps EVERY relay seam detached).
        let relay_attach = relay_enabled.then_some(relay_attach).flatten();
        // N2-H: the session's INTERNAL management dial address, read BEFORE
        // the factory is moved into the worker/renewal tasks.
        let management_dial = factory.dial_address();
        let shared = Arc::new(ConnectorShared::new(force_default_route, relay_attach));
        // N13-D2: with the opt-in, the carrier orchestrator needs the WG
        // seam + the runtime (used from the sync path when the first relay
        // client starts). Without the opt-in nothing is stored.
        if relay_enabled {
            let _ = shared.relay_wg.set(wg.clone());
            let _ = shared.relay_runtime.set(runtime.clone());
        }
        shared.set_running(true);
        shared.lock().started_at_unix = Some(unix_now());

        // N5c: per-peer ICE orchestrator + pump thread (injected clock:
        // the thread only supplies monotonic ms; the orchestrator itself
        // never reads a wall clock or sleeps). The pump exits on the same
        // stop flag as worker/renewal. N5d: the production build swaps the
        // send seam to the real exchange and attaches the signal runtime
        // (the link itself starts lazily — see apply_update / signal feed).
        let stop_flag = Arc::new(AtomicBool::new(false));
        let mut signal_sockets: Option<Arc<ProtectedSocketFdSource>> = None;
        let socket_supply = socket_source.as_ref().map(|source| {
            let supply = source.socket_supply().unwrap_or_else(|| {
                let supply = Arc::new(crate::socket_supply::SocketSupply::new());
                source.attach_supply(supply.clone(), crate::socket_supply::Purpose::Management);
                supply
            });
            crate::socket_supply::install(supply.clone());
            supply
        });
        let ice_sockets: Option<Arc<crate::ice::ProtectedUdpFdSource>>;
        let ice = match ice {
            Some(orch) => {
                ice_sockets = None;
                orch
            }
            None => {
                let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"));
                if let Some(supply) = socket_supply.as_ref() {
                    socks.attach_dns(supply.clone(), runtime.clone());
                }
                let seam: Arc<dyn crate::peer_conn::SignalExchange> =
                    match signal_material.as_ref() {
                        Some(mat) => {
                            // N5d: real link materials — ONE exchange is
                            // created here and shared by the orchestrator
                            // seam (send side) and the worker (queue side).
                            let src = Arc::new(ProtectedSocketFdSource::new_with_fd(-1).expect("empty tcp source"));
                            if let Some(supply) = socket_supply.as_ref() {
                                src.attach_supply(supply.clone(), crate::socket_supply::Purpose::Signal);
                            }
                            signal_sockets = Some(src.clone());
                            let (exchange, rx) = RealSignalExchange::new();
                            let rt = Arc::new(SignalRuntime::new(
                                SignalLinkMaterials {
                                    runtime: runtime.clone(),
                                    transport: mat.transport.clone(),
                                    connect_timeout: mat.connect_timeout,
                                    request_timeout: mat.request_timeout,
                                    keys: mat.keys.clone(),
                                    sockets: src,
                                },
                                exchange.clone(),
                                rx,
                            ));
                            let _ = shared.signal.set(rt);
                            exchange
                        }
                        None => Arc::new(LoggingSignalExchange::default()),
                    };
                let orch = Arc::new(Mutex::new(PeerIceOrchestrator::new(PeerIceDeps {
                    ifaces: Arc::new(crate::ice::SystemInterfaces),
                    socks: socks.clone(),
                    signal: seam,
                    wg: wg.clone(),
                    tie_breaker: None,
                    // N12a HOST-ONLY tuning: defaults (0/empty) in every
                    // device config = the exact pre-N12a orchestrator.
                    fixed_local_port: (ice_tuning.ice_fixed_port != 0)
                        .then_some(ice_tuning.ice_fixed_port),
                    advertised_candidates: ice_tuning.advertised_candidates.clone(),
                })));
                let _ = shared.ice_sockets.set(socks.clone());
                ice_sockets = Some(socks);
                orch
            }
        };
        let _ = shared.ice.set(ice.clone());
        // N5d: the signal runtime routes received frames into the
        // orchestrator — same handle in both branches (injected-orchestrator
        // constructions simply have no signal runtime attached).
        if let Some(rt) = shared.signal.get() {
            let _ = rt.orch.set(ice.clone());
        }
        spawn_ice_pump(ice.clone(), stop_flag.clone());
        // N7: the production data-plane pump (device-backed seam only). The
        // pump exits on the same stop flag; with no device yet it no-ops
        // until the shell feeds both fds.
        if let Some(feed) = wg_feed.as_ref() {
            feed.spawn_data_plane_pump(stop_flag.clone());
        }

        let worker_deps = WorkerDeps {
            shared: shared.clone(),
            factory: factory.clone(),
            wg: wg.clone(),
            host: host.clone(),
            secrets: secrets.clone(),
            meta: meta.clone(),
            policy_backoff: policy_backoff.clone(),
            sync_backoff: sync_policy.backoff,
            sync_rng: sync_policy.rng,
            sync_clock: sync_policy.clock,
        };
        let logout_slot: Arc<Mutex<Option<ManagementGrpcClient>>> = Arc::new(Mutex::new(None));
        // the start transition happens synchronously so the very first
        // status poll already reports `connecting` (no spawn race)
        shared.transition(ConnEvent::Connect);
        let (network_change, network_rx) = tokio::sync::watch::channel(0u64);
        let worker = runtime.spawn(worker_supervisor(worker_deps, logout_slot.clone(), network_rx));

        let worker_factory = factory.clone();
        let renewal = runtime.spawn(renewal_main(
            shared.clone(),
            factory,
            secrets,
            meta,
            renew_lead,
            renew_check_interval,
            stop_flag.clone(),
        ));

        Arc::new(ConnectorHandle {
            management_factory: worker_factory,
            network_change,
            shared,
            wg,
            host,
            logout_slot,
            stop_flag,
            socket_source,
            ice_sockets,
            signal_sockets,
            wg_feed,
            ice,
            management_dial,
            worker: Mutex::new(Some(worker)),
            renewal: Mutex::new(Some(renewal)),
            runtime,
        })
    }

    /// The network-change event already ran; update only its resolved dial target.
    pub fn update_management_address(&self, addr: SocketAddr) -> Result<(), &'static str> {
        if !self.is_running() { return Err("connector-stopped"); }
        self.management_factory.update_dial_address(addr);
        if let Some(source) = &self.ice_sockets {
            let servers = self.ice.lock_poison().stuns().to_vec();
            source.refresh_stun_dns(&servers);
        }
        Ok(())
    }

    /// Called only after the replacement WG socket has been protected and fed.
    /// Old queued sockets belong to the previous physical network and must not
    /// be used for any new management, signal or ICE dial.
    pub fn physical_network_changed(&self, management_addr: Option<SocketAddr>) -> Result<(), &'static str> {
        if !self.is_running() { return Err("connector-stopped"); }
        dns_lookups().lock_poison().clear();
        if let Some(addr) = management_addr {
            self.management_factory.update_dial_address(addr);
        }
        if let Some(source) = &self.socket_source {
            if let Some(supply) = source.socket_supply() {
                supply.cancel_pending_for_network_change();
                if management_addr.is_none() { supply.set_dns_servers(Vec::new()); }
            }
            source.clear_pending();
        }
        if let Some(source) = &self.signal_sockets { source.clear_pending(); }
        if let Some(source) = &self.ice_sockets { source.clear_pending(); }
        self.ice.lock_poison().restart_for_network_change(crate::sys::mono_ms());
        if let Some(source) = &self.ice_sockets {
            let servers = if management_addr.is_some() {
                self.ice.lock_poison().stuns().to_vec()
            } else { Vec::new() };
            source.refresh_stun_dns(&servers);
        }
        if let Some(rt) = self.shared.signal.get() {
            rt.shutdown();
            *rt.connect_addr.lock_poison() = None;
        }
        // The relay worker owns its reconnect loop. Tear down its old-network
        // session without stopping the worker; its next dial goes through the
        // protected socket supply and its current DNS/network generation.
        if let Some(client) = self.shared.relay_handle() { client.network_changed(); }
        self.network_change.send_modify(|generation| *generation = generation.wrapping_add(1));
        Ok(())
    }

    /// Current status snapshot. N7: refreshes the default-route gate
    /// against LIVE data-plane readiness first, so the snapshot and the
    /// `wg` field always describe reality at read time. N8: the same read
    /// advances the controlled-recreate state (`recreate` field).
    pub fn status(&self) -> ConnectorStatus {
        self.shared.refresh_net_gate(self.wg.as_ref());
        let g = self.shared.lock();
        let running = self.shared.is_running();
        let state = g.machine.state();
        ConnectorStatus {
            running,
            state,
            started_at_unix: g.started_at_unix,
            last_update_unix: g.last_update_unix,
            peer_count: g.peer_count,
            route_count: g.route_count,
            reconnects: g.reconnects,
            last_error: g.last_error,
            deadline: g.deadline,
            renew_attempts: g.renew_attempts,
            wg_apply_failed: g.wg_apply_failed,
            wg_apply_errors: g.wg_apply_errors,
            logout_ok: g.logout_ok,
            terminal: is_terminal_state(running, state),
            ice: self.ice.lock_poison().summary(),
            signal: self
                .shared
                .signal
                .get()
                .map(|rt| rt.status())
                .unwrap_or_default(),
            wg: self.wg.dataplane_status().unwrap_or_default(),
            recreate: g.recreate.clone(),
            relay: self.shared.relay_status(),
        }
    }

    /// The `connector_status()` JSON document.
    pub fn status_json(&self) -> String {
        let base = self.status().to_json();
        // N5c starvation observability: expose the ICE socket queue depth so a
        // FEEDER can read the real value. A feeder that only learns the depth
        // from its own feed calls keeps a stale "still queued" reading and
        // stops resupplying exactly when the queue is empty — device run 5 and
        // the host CLI both sat in endless "protected-udp:
        // no-protected-socket" that way. -1 = no shell-fed source attached.
        let (queued, taken) = match self.shared.ice_sockets.get() {
            Some(s) => (s.pending() as i64, s.taken() as i64),
            None => (-1, -1),
        };
        // 0051 §7 item 3 (demand-sized pool): the per-peer socket demand the
        // CORE will actually require (`allowed_ifaces × 2`), so the shell can
        // size the pool from core's real need instead of assuming 4. -1 =
        // interface enumeration failed → the shell keeps its own constant.
        let need_per_peer =
            self.ice.lock_poison().need_per_peer().map(|n| n as i64).unwrap_or(-1);
        with_ice_sockets_section(&base, queued, taken, need_per_peer)
    }

    /// The `connector_network_config()` JSON document (read-only snapshot of
    /// the last applied NetworkMap — the parts the shell applies platform
    /// side; `no-network-map` before the first map). N7: the default-route
    /// gate is re-decided against LIVE data-plane readiness at read time.
    pub fn network_config_json(&self) -> String {
        self.shared.refresh_net_gate(self.wg.as_ref());
        self.shared.network_config_json()
    }

    /// Stop the connector: close the Sync stream (worker abort), logout
    /// with the last logged-in client (failure does NOT block the stop),
    /// clear the data-plane seams. Idempotent: a second call reports
    /// `already_stopped` and does nothing.
    pub fn stop(&self) -> StopOutcome {
        let worker = self.worker.lock_poison().take();
        let already = worker.is_none();
        if already {
            return StopOutcome { already_stopped: true, state: self.shared.state() };
        }
        let renewal = self.renewal.lock_poison().take();
        self.stop_flag.store(true, Ordering::Release);
        self.shared.set_running(false);
        if let Some(h) = worker {
            h.abort();
        }
        if let Some(h) = renewal {
            h.abort();
        }
        self.shared.transition(ConnEvent::Disconnect);
        // cleanup: local registrations and host-applied config go away
        // (the shell snapshot goes with them — nothing applied remains)
        // N5c: ICE sessions too (their dup sockets close exactly once).
        // N5d: the signal worker aborts with the rest.
        if let Some(rt) = self.shared.signal.get() {
            rt.shutdown();
        }
        // N13-D1 撤销清理: the relay client is torn down with the connector —
        // no outbound relay connection may outlive `connector_stop()`
        // (N13 plan §4). Runs even when never started (no-op).
        self.shared.stop_relay();
        self.ice.lock_poison().stop_all();
        self.wg.clear();
        self.host.clear();
        {
            let mut g = self.shared.lock();
            g.net_config = None;
            g.net_routes_all.clear();
            // N8: the applied-set bookkeeping goes with the applied config —
            // a fresh start begins with no recreate history
            g.applied_route_set = None;
            g.last_recreate_ms = None;
            g.recreate = RecreateStatus::default();
        }
        // logout is best-effort and must never block the stop
        if let Some(mut client) = self.logout_slot.lock_poison().take() {
            let shared = self.shared.clone();
            self.runtime.spawn(async move {
                let ok = client.logout().await.is_ok();
                shared.set_logout_ok(ok);
                hilog::emit(&format!("connector: logout ok={ok}"));
            });
        }
        hilog::emit("connector: stopped");
        StopOutcome { already_stopped: false, state: self.shared.state() }
    }

    /// The `connector_stop()` JSON document (idempotent).
    pub fn stop_json(&self) -> String {
        let outcome = self.stop();
        format!(
            "{{{},{},{}}}",
            jbool("ok", true),
            jbool("already_stopped", outcome.already_stopped),
            jstr("state", outcome.state.as_str()),
        )
    }

    pub fn is_running(&self) -> bool {
        self.shared.is_running()
    }

    /// N13-D1: the relay client handle, when one is running (opt-in +
    /// first relay sync seen). This is the D2 WG-carrier attachment point:
    /// `open_conn`/`send_to_peer`/`recv` pump the WG bind through it once
    /// the relay stream becomes a WG outer carrier. `None` = relay disabled
    /// or no relay sync seen yet.
    pub fn relay_client(&self) -> Option<crate::relay_client::RelayClient> {
        if !self.shared.relay_enabled {
            return None;
        }
        self.shared.relay_handle()
    }

    /// N13-D2: the relay-carrier orchestration counters, when the carrier
    /// is armed (relay enabled + a carrier-capable WG seam + a relay client
    /// started). `None` = relay disabled / no capable seam / not started.
    pub fn relay_carrier_stats(&self) -> Option<RelayCarrierStats> {
        self.shared.relay_carrier.get().map(|c| c.stats())
    }

    /// N13-D2: currently attached relay-carrier lane count (`None` when the
    /// carrier is not armed).
    pub fn relay_carrier_lanes(&self) -> Option<usize> {
        self.shared.relay_carrier.get().map(|c| c.lane_count())
    }
}

// ---------------------------------------------------------------------------
// worker tasks
// ---------------------------------------------------------------------------

/// N5c ICE pump tick (production cadence; the orchestrator itself is fully
/// clock-injected and never sleeps — this loop only FEEDS it).
pub const ICE_PUMP_TICK_MS: u64 = 100;

/// The ICE pump thread: feeds the orchestrator with monotonic ms every
/// [`ICE_PUMP_TICK_MS`], until the connector stop flag fires. A detached
/// std::thread (not a tokio task) so a blocking candidate gather (real STUN
/// timeouts) can never stall a runtime worker; errors are logged by class
/// and never kill the pump.
fn spawn_ice_pump(ice: Arc<Mutex<PeerIceOrchestrator>>, stop_flag: Arc<AtomicBool>) {
    let _ = std::thread::Builder::new().name("ice-pump".into()).spawn(move || {
        while !stop_flag.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(ICE_PUMP_TICK_MS));
            if stop_flag.load(Ordering::Acquire) {
                return;
            }
            let now = crate::sys::mono_ms();
            let mut orch = ice.lock_poison();
            if let Err(e) = orch.run_once(now) {
                // Device diagnostics: the class alone ("network") cannot tell a
                // missing protected socket from a gather/parse failure — print
                // the message too (device run 3 was blind without it).
                hilog::emit(&format!(
                    "peer-conn: pump error ({}): {}",
                    ErrorClass::from_management(&e).as_str(),
                    e
                ));
            }
        }
    });
}

async fn worker_supervisor(
    initial: WorkerDeps,
    logout_slot: Arc<Mutex<Option<ManagementGrpcClient>>>,
    mut changes: tokio::sync::watch::Receiver<u64>,
) {
    let shared = initial.shared.clone();
    let factory = initial.factory.clone();
    let wg = initial.wg.clone();
    let host = initial.host.clone();
    let secrets = initial.secrets.clone();
    let meta = initial.meta.clone();
    let policy_backoff = initial.policy_backoff.clone();
    let sync_backoff = initial.sync_backoff.clone();
    let mut first = Some(initial);
    loop {
        let deps = first.take().unwrap_or_else(|| WorkerDeps {
            shared: shared.clone(), factory: factory.clone(), wg: wg.clone(), host: host.clone(),
            secrets: secrets.clone(), meta: meta.clone(), policy_backoff: policy_backoff.clone(),
            sync_backoff: sync_backoff.clone(), sync_rng: Box::new(OsRandom),
            sync_clock: Box::new(MonotonicClock),
        });
        tokio::select! {
            _ = worker_main(deps, logout_slot.clone()) => return,
            changed = changes.changed() => {
                if changed.is_err() { return; }
                shared.transition(ConnEvent::Lost);
                *logout_slot.lock_poison() = None;
                hilog::emit("connector: physical network changed; management redialing");
            }
        }
    }
}

/// The connection worker: connect → login → Sync session (internal
/// reconnect loop). Fatal (Auth) and backoff exhaustion both end the worker
/// with the state machine parked in `Failed`; `stop()` aborts the task for
/// a clean `Disconnected`.
async fn worker_main(deps: WorkerDeps, logout_slot: Arc<Mutex<Option<ManagementGrpcClient>>>) {
    let WorkerDeps {
        shared,
        factory,
        wg,
        host,
        secrets,
        meta,
        policy_backoff,
        sync_backoff,
        sync_rng,
        sync_clock,
        ..
    } = deps;
    // (the Connect transition was applied synchronously by spawn())

    // ---- phase 1: dial + login (retry with backoff; Auth = Permanent) ----
    // Upstream shape: the retry loop wraps the dial AND the login; only
    // PermissionDenied is Permanent (grpc.go:322-324 for the login path,
    // L436-438/L468-470 for the stream).
    let mut login_backoff = policy_backoff;
    let mut login_rng = OsRandom;
    let login_clock = MonotonicClock;
    let client = loop {
        let attempt = async {
            let mut client = factory.connect().await?;
            let logged_in = client
                .login(LoginParams {
                    setup_key: secrets.setup_key.clone(),
                    jwt_token: secrets.jwt.clone(),
                    meta: meta.clone(),
                    ..Default::default()
                })
                .await
                .map(|outcome| (client, outcome));
            logged_in
        };
        match attempt.await {
            Ok((client, outcome)) => {
                // anchor the 3-state deadline from LoginResponse
                // (engine_authsession.go:17-62 semantics)
                shared.lock().deadline.apply_wire(outcome.session_deadline_unix);
                shared.remember_login_signal(&outcome.response);
                if outcome.peer_address.is_some() {
                    hilog::emit("connector: login ok (peer address assigned)");
                } else {
                    hilog::emit("connector: login ok");
                }
                *logout_slot.lock_poison() = Some(client.clone());
                break client;
            }
            Err(e) => {
                if matches!(e, ManagementError::Auth { .. }) {
                    // Permanent: an invalid setup key / revoked peer will
                    // not heal by retrying.
                    shared.record_error(&e);
                    shared.transition(ConnEvent::FatalError);
                    shared.set_running(false);
                    hilog::emit("connector: login fatal (auth), giving up");
                    return;
                }
                shared.record_error(&e);
                shared.count_retryable_failure();
                shared.transition(ConnEvent::Lost);
                hilog::emit(&format!(
                    "connector: login attempt failed ({}), will retry",
                    ErrorClass::from_management(&e).as_str()
                ));
            }
        }
        match login_backoff.next_delay(&login_clock, &mut login_rng) {
            Some(delay) => tokio::time::sleep(delay).await,
            None => {
                // retry budget spent — upstream retry.go:34-39
                shared.transition(ConnEvent::RetryExhausted);
                shared.set_running(false);
                hilog::emit("connector: login retry budget exhausted");
                return;
            }
        }
    };

    // ---- phase 2: the Sync session (reconnects internally) ----
    let mut session =
        SyncSession::new(client, meta).with_policy(sync_backoff, sync_rng, sync_clock);
    let result = session
        .run_events(|event| match event {
            SyncLoopEvent::Opened => {
                shared.transition(ConnEvent::Established);
                hilog::emit("connector: management stream established");
            }
            SyncLoopEvent::Update(update) => shared.apply_update(wg.as_ref(), host.as_ref(), update),
            SyncLoopEvent::Broken(e) => {
                shared.record_error(e);
                shared.count_retryable_failure();
                shared.transition(ConnEvent::Lost);
            }
        })
        .await;
    // run_events never returns Ok(()) — its loop only exits through a fatal
    // error or backoff exhaustion (crate::sync).
    if let Err(e) = result {
        shared.record_error(&e);
        if matches!(e, ManagementError::Auth { .. }) {
            shared.transition(ConnEvent::FatalError);
            hilog::emit("connector: sync session fatal (auth)");
        } else {
            shared.transition(ConnEvent::RetryExhausted);
            hilog::emit("connector: sync session retry budget exhausted");
        }
    }
    // N13-D1: the worker ended by itself — the relay client must not
    // outlive it (撤销清理; the expired-token fail-closed rule also forbids
    // keeping an unattended relay session alive past its management owner).
    shared.stop_relay();
    shared.set_running(false);
}

/// The session-renewal task: while running, wake on `renew_check_interval`
/// and extend the SSO session when the anchored deadline is within
/// `renew_lead`. Upstream semantics mirrored (file:line only):
/// - the empty-JWT guard (engine_authsession.go:84-86 — a setup-key peer
///   has nothing to extend);
/// - only the deadline is refreshed, no resync / no tunnel churn
///   (engine_authsession.go:71-73 comment, write-back at L102);
/// - the 3-state reply application (engine_authsession.go:34-45 /
///   management.proto L306-310).
/// A failed extension is recorded (sanitized class) and retried on the next
/// tick — upstream surfaces the error to its caller and lets the warning
/// flow re-trigger (engine_authsession.go:98-100); the equivalent daemon
/// warning flow has no HarmonyOS counterpart yet, so the tick retry is the
/// closest safe behavior.
async fn renewal_main(
    shared: Arc<ConnectorShared>,
    factory: Arc<dyn ManagementFactory>,
    secrets: ConnectorSecrets,
    meta: PeerMeta,
    renew_lead: Duration,
    renew_check_interval: Duration,
    stop_flag: Arc<AtomicBool>,
) {
    loop {
        tokio::time::sleep(renew_check_interval).await;
        if stop_flag.load(Ordering::Acquire) || !shared.is_running() {
            return;
        }
        if secrets.jwt.is_empty() {
            continue; // engine_authsession.go:84-86 (local guard, no I/O)
        }
        let deadline = shared.lock().deadline;
        let SessionDeadline::At(t) = deadline else {
            continue; // nothing anchored: nothing to extend
        };
        if unix_now() + (renew_lead.as_secs() as i64) < t {
            continue; // not close to expiry yet
        }
        shared.lock().renew_attempts += 1;
        match factory.connect().await {
            Ok(mut client) => match client.extend_auth_session(&meta, &secrets.jwt).await {
                Ok(outcome) => {
                    shared.lock().deadline.apply_wire(outcome.session_deadline_unix);
                    hilog::emit("connector: session renewed");
                }
                Err(e) => {
                    shared.record_error(&e);
                    hilog::emit(&format!(
                        "connector: session renewal failed ({})",
                        ErrorClass::from_management(&e).as_str()
                    ));
                }
            },
            Err(e) => {
                shared.record_error(&e);
                hilog::emit(&format!(
                    "connector: session renewal could not reach management ({})",
                    ErrorClass::from_management(&e).as_str()
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// global runtime + NAPI entry points (JSON in / JSON out, synchronous)
// ---------------------------------------------------------------------------

static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();

/// The cdylib-wide tokio runtime (already a frozen-stack dependency;
/// multi-thread). Created lazily on first connector use.
pub fn global_runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("global tokio runtime")
    })
}

static CONNECTOR: Mutex<Option<Arc<ConnectorHandle>>> = Mutex::new(None);

fn connector_slot() -> MutexGuard<'static, Option<Arc<ConnectorHandle>>> {
    CONNECTOR.lock().unwrap_or_else(PoisonError::into_inner)
}

/// N2-H (design §7-I5 wiring): register the DEVICE COLLECTOR's process-side
/// host seam over `feed` — the one call that takes the collector out of its
/// inert `no-connector` state (`client/core/src/n2h_collector.rs`,
/// [`crate::n2h_collector::WgFeedHost`]).
///
/// Registration ONLY: no thread, no tee, no packet, no clock read. Until an
/// arm arrives the shipped process behaves exactly as before (§6.1), and an
/// arm stays refused until the shell feeds the device and a session is
/// established (`device-not-up` / `tunnel-not-ready`).
///
/// Idempotent by REPLACE (one process = one connector slot = one session):
/// every start entry may call it unconditionally, and a restart re-points the
/// seam at the NEW feed — keeping a stale feed would refuse every later arm
/// with `device-not-up`. The seam is removed by `connector_stop_json`, so its
/// lifetime is exactly the connector session's.
fn install_collector_host(feed: &Arc<crate::wg_device::WgDeviceFeed>) {
    crate::n2h_collector::install_host(Arc::new(crate::n2h_collector::WgFeedHost::new(feed.clone())));
}

/// The `connector_start(configJson, setupKeyJson?)` implementation.
///
/// N3-7 (fail-closed gap 1): starting WITHOUT a protected management socket
/// is REFUSED by default — `{"started":false,"error":"management-socket-required"}`.
/// The production path is [`connector_start_with_socket_json`]. The only way
/// past the gate is the explicit config opt-in
/// `"allow_unprotected_management": true`, which is a DEVELOPMENT/debug mode:
/// NOT upstream behavior (upstream protects every management dial,
/// protectsocket_android.go:22-46) and DANGEROUS once the tunnel owns the
/// default route (the management dial gets eaten by our own tunnel —
/// bootstrap loop). The opt-in is logged loudly.
///
/// Returns `{"started":true,"state":"connecting"}` on success, or
/// `{"started":false,"error":"<reason>"}` — reason is a stable token
/// (`management-socket-required` / `already-running` / `invalid-config` /
/// `invalid-credentials`), never secret material or server text.
pub fn connector_start_json(config_json: &str, credentials_json: &str) -> String {
    let mut slot = connector_slot();
    if let Some(existing) = slot.as_ref() {
        if existing.is_running() {
            return format!(
                "{{{},{}}}",
                jbool("started", false),
                jstr("error", "already-running")
            );
        }
    }
    let config = match ConnectorConfig::from_json(config_json) {
        Ok(c) => c,
        Err(_) => {
            return format!(
                "{{{},{}}}",
                jbool("started", false),
                jstr("error", "invalid-config")
            )
        }
    };
    if !config.allow_unprotected_management {
        // fail-closed: no protected socket, no opt-in → no start
        return format!(
            "{{{},{}}}",
            jbool("started", false),
            jstr("error", "management-socket-required")
        );
    }
    hilog::emit(
        "connector: UNPROTECTED management direct dial (allow_unprotected_management=true) \
         — NOT upstream behavior, DANGEROUS: unprotected traffic + bootstrap-loop risk \
         while the tunnel owns the default route",
    );
    let secrets = match ConnectorSecrets::from_json(credentials_json) {
        Ok(s) => s,
        Err(_) => {
            return format!(
                "{{{},{}}}",
                jbool("started", false),
                jstr("error", "invalid-credentials")
            )
        }
    };
    // N7: the PRODUCTION WG seam — the shell-fed device slot. Without feeds
    // the data plane never starts (tunnel_ready=false ⇒ default route held);
    // there is no unprotected/implicit socket fallback.
    let wg_feed = Arc::new(crate::wg_device::WgDeviceFeed::new(
        crate::wg_device::WgDeviceConfig::new(config.keys.secret_base64()),
    ));
    install_collector_host(&wg_feed);
    let handle = ConnectorHandle::spawn_with_relay(
        global_runtime().handle().clone(),
        Arc::new(GrpcManagementFactory::new(&config, config.keys.clone())),
        wg_feed.clone(),
        Arc::new(LoggingConfigApplier),
        secrets,
        config.meta.clone(),
        ExponentialBackoff::upstream_stream_default(),
        SyncPolicy::production(),
        config.renew_lead,
        config.renew_check_interval,
        config.force_default_route,
        None, // N3-7: no protected management socket source
        None, // N5c: build the production ICE orchestrator
        Some(SignalMaterial {
            // N5d: the signal link rides the same injected trust root and
            // identity keys as management (connect.go:722-723 parity)
            transport: config.transport.clone(),
            keys: config.keys.clone(),
            connect_timeout: config.connect_timeout,
            request_timeout: config.request_timeout,
        }),
        Some(wg_feed.clone()), // N7: device-backed WG seam + data-plane pump
        HostIceTuning::from_config(&config), // N12a HOST-ONLY tuning
        // N13-D1: relay opt-in (DEFAULT FALSE — see ConnectorConfig).
        // No socket broker in this explicit unprotected-management path:
        // relay itself still fails closed.
        config.relay_enabled,
        config.relay_enabled.then(|| RelayAttach {
            wg_pubkey_b64: config.keys.public_key_base64(),
            materials: Arc::new(RelayMaterials::from_connector_transport(&config.transport, None)),
        }),
    );
    let state = handle.status().state;
    *slot = Some(handle);
    // N2 T5a: the session's stimulus sink (I/Wb/Ws endpoint break + D fresh
    // protected query). It resolves the live connector through the slot at
    // call time and is removed by `connector_stop_json`, so it can never
    // outlive the session it belongs to.
    crate::n2_stimulus::install_sink(Arc::new(ConnectorStimulusSink));
    // N2 P4: the session's A-column trigger seam (M on the EXISTING
    // management connection, D requery, R timeout arm). Same lifetime rule:
    // removed by `connector_stop_json`.
    crate::n2_a_event::install_trigger(Arc::new(ConnectorATrigger));
    format!("{{{}, {}}}", jbool("started", true), jstr("state", state.as_str()))
}

/// The `connector_start_with_socket(fd, configJson, setupKeyJson?, addrJson)`
/// implementation — the N3-7 PRODUCTION start path (fail-closed gap 1).
///
/// The shell (ArkTS) must, before calling this: resolve the management host
/// (DNS shell-side), open a TCP socket via `mgmt_socket_open`, and
/// successfully `VpnConnection.protect(fd)` it. The fd crosses as a NUMBER
/// only; native dups it (`F_DUPFD_CLOEXEC`) per dial and never uses or
/// closes the original (fd contract — `crate::mgmtsock`).
///
/// `addrJson` is `{"connect_addr":"ip:port"}` — the shell-resolved
/// management address the protected socket is connected to; TLS still
/// verifies the endpoint URL host.
///
/// Refusals (fail-closed, stable tokens): `already-running`,
/// `invalid-config`, `invalid-credentials`, `socket-fd-missing` (fd < 0),
/// `socket-fd-invalid` (not dup-able — e.g. protect path closed it or the
/// protect gate failed shell-side), `socket-addr-invalid`.
pub fn connector_start_with_socket_json(
    fd: i32,
    config_json: &str,
    credentials_json: &str,
    connect_addr_json: &str,
) -> String {
    let mut slot = connector_slot();
    if let Some(existing) = slot.as_ref() {
        if existing.is_running() {
            return format!(
                "{{{},{}}}",
                jbool("started", false),
                jstr("error", "already-running")
            );
        }
    }
    let config = match ConnectorConfig::from_json(config_json) {
        Ok(c) => c,
        Err(_) => {
            return format!(
                "{{{},{}}}",
                jbool("started", false),
                jstr("error", "invalid-config")
            )
        }
    };
    let secrets = match ConnectorSecrets::from_json(credentials_json) {
        Ok(s) => s,
        Err(_) => {
            return format!(
                "{{{},{}}}",
                jbool("started", false),
                jstr("error", "invalid-credentials")
            )
        }
    };
    // fail-closed fd gate: a missing or dead fd refuses the start
    if fd < 0 {
        return format!(
            "{{{},{}}}",
            jbool("started", false),
            jstr("error", "socket-fd-missing")
        );
    }
    if let Err(e) = crate::mgmtsock::probe_fd_dupable(fd) {
        hilog::emit(&format!(
            "connector: start refused ({} errno={}) — protect gate failed or fd dead",
            e.token(),
            e.errno()
        ));
        return format!(
            "{{{},{}}}",
            jbool("started", false),
            jstr("error", e.token())
        );
    }
    let connect_addr = match parse_connect_addr(connect_addr_json) {
        Ok(a) => a,
        Err(_) => {
            return format!(
                "{{{},{}}}",
                jbool("started", false),
                jstr("error", "socket-addr-invalid")
            )
        }
    };
    let source = match ProtectedSocketFdSource::new_with_fd(fd) {
        Ok(source) => Arc::new(source),
        Err(error) => return format!("{{{},{}}}", jbool("started", false), jstr("error", error.token())),
    };
    let supply = Arc::new(crate::socket_supply::SocketSupply::new());
    if let Ok(crate::config::Json::Obj(fields)) = crate::config::parse_document(connect_addr_json) {
        if let Some((_, servers)) = fields.iter().find(|(key, _)| key == "dns_servers") {
            if let crate::config::Json::Arr(items) = servers {
                let addresses: Vec<_> = items.iter().filter_map(|item| match item {
                    crate::config::Json::Str(ip) => ip.parse::<std::net::Ipv4Addr>().ok(),
                    _ => None,
                }).collect();
                supply.set_dns_servers(addresses);
            }
        }
    }
    source.attach_supply(supply.clone(), crate::socket_supply::Purpose::Management);
    // N7: the PRODUCTION WG seam — the shell-fed device slot (same as
    // connector_start; the feeds arrive later, after create()+protect)
    let wg_feed = Arc::new(crate::wg_device::WgDeviceFeed::new(
        crate::wg_device::WgDeviceConfig::new(config.keys.secret_base64()),
    ));
    install_collector_host(&wg_feed);
    let handle = ConnectorHandle::spawn_with_relay(
        global_runtime().handle().clone(),
        Arc::new(GrpcManagementFactory::with_socket_source(
            &config,
            config.keys.clone(),
            source.clone(),
            connect_addr,
        )),
        wg_feed.clone(),
        Arc::new(LoggingConfigApplier),
        secrets,
        config.meta.clone(),
        ExponentialBackoff::upstream_stream_default(),
        SyncPolicy::production(),
        config.renew_lead,
        config.renew_check_interval,
        config.force_default_route,
        Some(source),
        None, // N5c: build the production ICE orchestrator
        Some(SignalMaterial {
            // N5d: the signal link rides the same injected trust root and
            // identity keys as management (connect.go:722-723 parity)
            transport: config.transport.clone(),
            keys: config.keys.clone(),
            connect_timeout: config.connect_timeout,
            request_timeout: config.request_timeout,
        }),
        Some(wg_feed.clone()), // N7: device-backed WG seam + data-plane pump
        HostIceTuning::from_config(&config), // N12a HOST-ONLY tuning
        // N13-D1: relay opt-in (DEFAULT FALSE — see ConnectorConfig).
        // The relay TLS root is the SAME injected ca_pem as management.
        config.relay_enabled,
        config.relay_enabled.then(|| RelayAttach {
            wg_pubkey_b64: config.keys.public_key_base64(),
            materials: Arc::new(RelayMaterials::from_connector_transport(
                &config.transport, Some(supply),
            )),
        }),
    );
    let state = handle.status().state;
    *slot = Some(handle);
    hilog::emit("connector: started over protected management socket (fd dup-consumed per dial)");
    // N2 T5a: see `connector_start_json` — one sink per session.
    crate::n2_stimulus::install_sink(Arc::new(ConnectorStimulusSink));
    // N2 P4: the session's A-column trigger seam (M on the EXISTING
    // management connection, D requery, R timeout arm). Same lifetime rule:
    // removed by `connector_stop_json`.
    crate::n2_a_event::install_trigger(Arc::new(ConnectorATrigger));
    format!(
        "{{{},{},{}}}",
        jbool("started", true),
        jstr("state", state.as_str()),
        jbool("protected", true)
    )
}

type DnsResult = Result<std::net::Ipv4Addr, std::io::Error>;
static DNS_LOOKUPS: OnceLock<Mutex<HashMap<u64, tokio::sync::oneshot::Receiver<DnsResult>>>> = OnceLock::new();
static NEXT_DNS_LOOKUP: AtomicU64 = AtomicU64::new(0);

fn dns_lookups() -> &'static Mutex<HashMap<u64, tokio::sync::oneshot::Receiver<DnsResult>>> {
    DNS_LOOKUPS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn start_dns_lookup(runtime: &tokio::runtime::Handle, lookup: impl std::future::Future<Output = DnsResult> + Send + 'static) -> u64 {
    let id = NEXT_DNS_LOOKUP.fetch_add(1, Ordering::Relaxed) + 1;
    let (tx, rx) = tokio::sync::oneshot::channel();
    dns_lookups().lock_poison().insert(id, rx);
    runtime.spawn(async move { let _ = tx.send(lookup.await); });
    id
}

pub fn connector_protected_dns_start_json(host: &str) -> String {
    let Some(handle) = connector_slot().as_ref().cloned() else {
        return "{\"ok\":false}".into();
    };
    let Some(supply) = handle.socket_source.as_ref().and_then(|s| s.socket_supply()) else {
        return "{\"ok\":false}".into();
    };
    if supply.dns_servers().is_empty() || host.is_empty() || host.len() > 253 {
        return "{\"ok\":false}".into();
    }
    // N2 T5a: the D stimulus re-queries the class's OWN current hostname; this
    // is the only place a protected-resolution hostname is remembered. It is
    // the public routing material `protected-dns:` already logs — never key
    // material.
    *last_dns_host().lock_poison() = host.to_string();
    let name = host.to_string();
    let id = start_dns_lookup(&handle.runtime, async move {
        crate::protected_dns::resolve(&name, &supply).await
    });
    format!("{{\"ok\":true,\"id\":{id}}}")
}

pub fn connector_protected_dns_poll_json(id: u64) -> String {
    let mut requests = dns_lookups().lock_poison();
    let Some(rx) = requests.get_mut(&id) else { return "{\"ok\":false,\"error\":\"dns-lookup-not-found\"}".into() };
    match rx.try_recv() {
        Err(tokio::sync::oneshot::error::TryRecvError::Empty) => "{\"ok\":true,\"pending\":true}".into(),
        result => {
            requests.remove(&id);
            match result {
                Ok(Ok(ip)) => format!("{{\"ok\":true,{}}}", jstr("ip", &ip.to_string())),
                _ => "{\"ok\":false,\"error\":\"dns-resolution-failed\"}".into(),
            }
        }
    }
}

pub fn connector_management_address_update_json(addr_json: &str) -> String {
    let addr = match parse_connect_addr(addr_json) {
        Ok(addr) => addr,
        Err(_) => return format!("{{{},{}}}", jbool("ok", false), jstr("error", "socket-addr-invalid")),
    };
    let slot = connector_slot();
    let Some(handle) = slot.as_ref() else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-connector"));
    };
    match handle.update_management_address(addr) {
        Ok(()) => format!("{{{}}}", jbool("ok", true)),
        Err(error) => format!("{{{},{}}}", jbool("ok", false), jstr("error", error)),
    }
}

pub fn connector_physical_network_changed_json(addr_json: &str) -> String {
    let addr = if addr_json == "\"\"" {
        None
    } else {
        match parse_connect_addr(addr_json) {
            Ok(addr) => Some(addr),
            Err(_) => return format!("{{{},{}}}", jbool("ok", false), jstr("error", "socket-addr-invalid")),
        }
    };
    let slot = connector_slot();
    let Some(handle) = slot.as_ref() else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-connector"));
    };
    match handle.physical_network_changed(addr) {
        Ok(()) => format!("{{{}}}", jbool("ok", true)),
        Err(error) => format!("{{{},{}}}", jbool("ok", false), jstr("error", error)),
    }
}

/// The `connector_socket_feed(fd)` implementation — shell-side resupply of
/// fresh protected sockets for reconnect dials. The shell opens a NEW
/// socket (`mgmt_socket_open`), protects it, and feeds it here; the queue is
/// consumed FIFO by the per-dial seam. Without resupply, reconnect dials
/// fail CLOSED (`no-protected-socket`), never unprotected.
pub fn connector_socket_feed_json(fd: i32) -> String {
    let slot = connector_slot();
    let Some(handle) = slot.as_ref() else {
        return format!(
            "{{{},{}}}",
            jbool("ok", false),
            jstr("error", "no-connector")
        );
    };
    let Some(source) = handle.socket_source.as_ref() else {
        return format!(
            "{{{},{}}}",
            jbool("ok", false),
            jstr("error", "no-socket-source")
        );
    };
    if fd < 0 {
        return format!(
            "{{{},{}}}",
            jbool("ok", false),
            jstr("error", "socket-fd-missing")
        );
    }
    if let Err(e) = source.feed(fd) {
        return format!(
            "{{{},{}}}",
            jbool("ok", false),
            jstr("error", e.token())
        );
    }
    format!(
        "{{{},{}}}",
        jbool("ok", true),
        jinum("queued", source.pending() as i64)
    )
}

/// The `connector_ice_socket_feed(fd)` implementation (N5c) — shell-side
/// resupply of fresh protected UDP sockets for the per-peer ICE sessions
/// (candidate-gather sockets + one long-lived check socket per local
/// candidate, see `crate::peer_conn`). Same fail-closed contract as
/// [`connector_socket_feed_json`]: without resupply, gathers fail CLOSED
/// (no candidates, peers stay Idle with a Network-class error), never
/// unprotected.
pub fn connector_ice_socket_feed_json(fd: i32) -> String {
    let slot = connector_slot();
    let Some(handle) = slot.as_ref() else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-connector"));
    };
    let Some(source) = handle.ice_sockets.as_ref() else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-socket-source"));
    };
    if fd < 0 {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "socket-fd-missing"));
    }
    if let Err(e) = source.feed(fd) {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", e.token()));
    }
    format!("{{{},{}}}", jbool("ok", true), jinum("queued", source.pending() as i64))
}

/// The `connector_signal_socket_feed(fd, addrJson)` implementation (N5d) —
/// shell-side resupply of PROTECTED signal sockets plus the shell-resolved
/// signal address. The shell parses `netbird_config.signal` (from
/// `connector_network_config()`), resolves the host DNS-side, opens a TCP
/// socket, `VpnConnection.protect(fd)`s it and feeds it here; the real
/// signal link dials ONLY over such sockets (every dial takes a fresh one —
/// initial AND per reconnect; empty source fails the dial CLOSED).
///
/// `addrJson` is `{"connect_addr":"ip:port"}` — required: without it the
/// link cannot dial (nothing is queued, `{"ok":false,"error":
/// "socket-addr-invalid"}`). The FIRST fed address wins for the lifetime of
/// the link (same fixed-address contract as the management factory); later
/// feeds only refill the fd queue.
///
/// Failure tokens (fail-closed): `no-connector`, `no-socket-source` (host
/// started without signal material), `socket-fd-missing` (fd < 0),
/// `socket-fd-invalid` (not dup-able), `socket-addr-invalid`.
pub fn connector_signal_socket_feed_json(fd: i32, connect_addr_json: &str) -> String {
    let slot = connector_slot();
    let Some(handle) = slot.as_ref() else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-connector"));
    };
    let Some(source) = handle.signal_sockets.as_ref() else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-socket-source"));
    };
    let Some(rt) = handle.shared.signal.get() else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-socket-source"));
    };
    if fd < 0 {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "socket-fd-missing"));
    }
    let connect_addr = match parse_connect_addr(connect_addr_json) {
        Ok(a) => a,
        Err(_) => {
            return format!("{{{},{}}}", jbool("ok", false), jstr("error", "socket-addr-invalid"))
        }
    };
    if let Err(e) = source.feed(fd) {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", e.token()));
    }
    rt.set_connect_addr(connect_addr);
    // the link starts once BOTH the sync-delivered URI and this address
    // exist (whichever arrives last triggers the start); if the connector
    // was stopped in the meantime, do not leave a worker behind
    if handle.shared.is_running() {
        rt.maybe_start();
    } else {
        rt.shutdown();
    }
    format!("{{{},{}}}", jbool("ok", true), jinum("queued", source.pending() as i64))
}

/// The `connector_wg_socket_feed(fd)` implementation (N7) — shell-side feed
/// of the PROTECTED WG outer UDP socket (native-opened via `wg_fwd_open`,
/// then `VpnConnection.protect`ed by the shell BEFORE any datagram flows,
/// governance §二.4). First of the two feeds the real WireGuard data plane
/// needs; the number is validated by a dup probe and stored BORROWED (the
/// device dups it at adopt time, never uses/closes the original — fd
/// contract). Both feeds together bring the device up; until then the data
/// plane stays down (`tunnel_ready=false`, default route HELD) and there is
/// NO unprotected fallback.
///
/// Failure tokens (fail-closed): `no-connector`, `no-wg-device` (connector
/// started without the device seam — host-test construction),
/// `socket-fd-missing`, `socket-fd-invalid`. Success:
/// `{"ok":true,"device_up":<bool>}`.
pub fn connector_wg_socket_feed_json(fd: i32) -> String {
    wg_feed_json(fd, FeedFdKind::WgSocket)
}

/// The `connector_tun_fd_feed(fd)` implementation (N7) — shell-side feed of
/// the platform TUN fd from `VpnConnection.create()`. The shell KEEPS the
/// raw fd (its close belongs exclusively to `VpnConnection.destroy()`);
/// native consumes ONLY a dup copy via `TunFd::dup_from_raw` at device
/// construction (fd contract). Second of the two data-plane feeds; see
/// [`connector_wg_socket_feed_json`] for tokens and the fail-closed rules.
pub fn connector_tun_fd_feed_json(fd: i32) -> String {
    wg_feed_json(fd, FeedFdKind::Tun)
}

#[derive(Clone, Copy)]
enum FeedFdKind {
    WgSocket,
    Tun,
}

fn wg_feed_json(fd: i32, kind: FeedFdKind) -> String {
    let slot = connector_slot();
    let Some(handle) = slot.as_ref() else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-connector"));
    };
    let Some(feed) = handle.wg_feed.as_ref() else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-wg-device"));
    };
    let fed = match kind {
        FeedFdKind::WgSocket => feed.feed_wg_socket(fd),
        FeedFdKind::Tun => feed.feed_tun(fd),
    };
    match fed {
        Ok(()) => format!(
            "{{{},{},{}}}",
            jbool("ok", true),
            jbool("device_up", feed.device_up()),
            // N2 T7a (item 6): the core's monotonic instant of this feed — the
            // shell logs it as `mono_ms=` on `VPN_WG_DATAPLANE_FED`, so that
            // shell line is alignable with the core's own
            // `N7_WG_FEED|adopted|…|mono_ms=` from the same call.
            jinum("mono_ms", crate::sys::mono_ms() as i64)
        ),
        Err(e) => format!(
            "{{{},{}}}",
            jbool("ok", false),
            jstr("error", e.token())
        ),
    }
}

/// The `connector_route_set_applied(routesJson, isRecreate)` implementation
/// (N8) — shell ACK that `routes` is the route set now APPLIED to the
/// platform VpnConfig. `routesJson` = `{"routes":["0.0.0.0/0","a.b.c.d/p"]}`
/// (canonical `network` strings exactly as `connector_network_config()`
/// renders them). `isRecreate=false` records the INITIAL create()'s applied
/// set (arms the N8 comparison); `true` records a COMPLETED controlled
/// recreate (counts against `recreate.count`, arms the cooldown window).
/// After the ACK the live refresh re-compares desired-vs-applied, so the
/// `recreate.required` flag clears exactly when the shell's applied set
/// matches the desired one.
///
/// Failure tokens: `no-connector`, `route-set-invalid` (malformed JSON /
/// non-string entries / empty list — an empty platform route set would be a
/// black hole by construction and is never accepted as an applied state).
pub fn connector_route_set_applied_json(routes_json: &str, is_recreate: bool) -> String {
    let slot = connector_slot();
    let Some(handle) = slot.as_ref() else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-connector"));
    };
    let routes = match parse_route_set(routes_json) {
        Ok(r) => r,
        Err(_) => {
            return format!(
                "{{{},{}}}",
                jbool("ok", false),
                jstr("error", "route-set-invalid")
            )
        }
    };
    handle
        .shared
        .ack_applied_route_set(routes, is_recreate, crate::sys::mono_ms());
    let recreate = handle.shared.lock().recreate.clone();
    format!(
        "{{{},{}}}",
        jbool("ok", true),
        format!("\"recreate\":{}", recreate.to_json())
    )
}

/// Parse the `{"routes":["a.b.c.d/p",...]}` argument of
/// [`connector_route_set_applied_json`]. At least one route is required
/// (an empty applied set is never a legal platform state for this client).
fn parse_route_set(json: &str) -> Result<Vec<String>, ConfigError> {
    let doc = config::parse_document(json)?;
    let entries = match doc {
        Json::Obj(entries) => entries,
        _ => {
            return Err(ConfigError::Field {
                field: "(root)",
                reason: "expected a JSON object".into(),
            })
        }
    };
    for (key, val) in &entries {
        if key == "routes" {
            let items = match val {
                Json::Arr(items) => items,
                _ => {
                    return Err(ConfigError::Field {
                        field: "routes",
                        reason: "expected an array of network strings".into(),
                    })
                }
            };
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(field_str(item, "routes")?.to_string());
            }
            if out.is_empty() {
                return Err(ConfigError::Field {
                    field: "routes",
                    reason: "at least one route is required".into(),
                });
            }
            return Ok(out);
        }
    }
    Err(ConfigError::Field { field: "routes", reason: "missing field".into() })
}

/// Parse the `{"connect_addr":"ip:port"}` argument of
/// [`connector_start_with_socket_json`] and
/// [`connector_signal_socket_feed_json`].
fn parse_connect_addr(json: &str) -> Result<SocketAddr, ConfigError> {
    let doc = config::parse_document(json)?;
    let entries = match doc {
        Json::Obj(entries) => entries,
        _ => {
            return Err(ConfigError::Field {
                field: "(root)",
                reason: "expected a JSON object".into(),
            })
        }
    };
    for (key, val) in &entries {
        if key == "connect_addr" {
            let s = field_str(val, "connect_addr")?;
            return s.parse::<SocketAddr>().map_err(|_| ConfigError::Field {
                field: "connect_addr",
                reason: "expected ip:port".into(),
            });
        }
    }
    Err(ConfigError::Field { field: "connect_addr", reason: "missing field".into() })
}

/// The `connector_status()` implementation (always valid JSON, even with no
/// connector: a fresh disconnected snapshot).
pub fn connector_status_json() -> String {
    let slot = connector_slot();
    match slot.as_ref() {
        Some(handle) => handle.status_json(),
        None => ConnectorStatus {
            running: false,
            state: ConnState::Disconnected,
            started_at_unix: None,
            last_update_unix: None,
            peer_count: 0,
            route_count: 0,
            reconnects: 0,
            last_error: None,
            deadline: SessionDeadline::Unknown,
            renew_attempts: 0,
            wg_apply_failed: false,
            wg_apply_errors: 0,
            logout_ok: None,
            terminal: false,
            ice: IceOrchestratorSummary::default(),
            signal: SignalLinkStatus::default(),
            wg: crate::wg_device::WgDataplaneStatus::default(),
            recreate: RecreateStatus::default(),
            relay: RelayStatus::disabled(),
        }
        .to_json(),
    }
}

/// The N5c socket-pool section appended to `connector_status()`.
///
/// `queued`/`taken` are the queue observables (see [`ConnectorHandle::status_json`]);
/// `need_per_peer` is the CORE's per-peer demand for THIS interface set
/// ([`crate::peer_conn::PeerIceOrchestrator::need_per_peer`] = allowed ifaces × 2),
/// `-1` when it cannot be enumerated. The shell sizes its pool from
/// `peers × need_per_peer` and falls back to its own constant on `-1`
/// (0051 §7 item 3: `need=6` × 6 peers = 36 exceeded the old 24 target).
fn ice_sockets_section(queued: i64, taken: i64, need_per_peer: i64) -> String {
    format!(
        "\"ice_sockets\":{{\"queued\":{queued},\"taken\":{taken},\"need_per_peer\":{need_per_peer}}}"
    )
}

/// The full `connector_status()` text: `base` with the N5c socket-pool section
/// appended INSIDE its root object (the base document's trailing `}` is
/// replaced by `,<section>}`). Extracted so the unit test exercises the SAME
/// brace dance the status surface uses — a hand-assembled fixture in the test
/// would not have caught a missing closing brace.
fn with_ice_sockets_section(base: &str, queued: i64, taken: i64, need_per_peer: i64) -> String {
    format!(
        "{},{}}}",
        &base[..base.len() - 1],
        ice_sockets_section(queued, taken, need_per_peer)
    )
}

/// The `connector_network_config()` implementation: read-only snapshot of
/// the shell-applicable subset of the last applied NetworkMap (tunnel
/// address/prefix, managed routes with the default-route mark, DNS servers,
/// peer summary — public keys + allowed-ips counts only). N2-H I2: the tail
/// carries the announced `stuns[].uri` list (the shell's exclusion-derivation
/// channel — see [`ShellNetworkConfig::stuns`]). Always valid JSON;
/// with no connector or before the first map:
/// `{"available":false,"reason":"no-network-map"}`.
pub fn connector_network_config_json() -> String {
    let slot = connector_slot();
    match slot.as_ref() {
        Some(handle) => handle.network_config_json(),
        None => NO_NETWORK_MAP_JSON.to_string(),
    }
}

/// N2-H (design §4.0): the session knowledge the device collector freezes
/// from that the two PUBLIC connector documents deliberately do NOT carry —
/// `netbird_config.stuns[]` and the management dial address. The §5.2 frozen
/// contract of `connector_status()` / `connector_network_config()` is
/// untouched: this is an internal (non-JSON) accessor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoreFreezeInputs {
    /// The shell-resolved address every protected management dial connects
    /// to. `None` = the unprotected dev dial has no fixed address, or no
    /// connector is running — the collector records the reasoned gap.
    pub management_dial: Option<SocketAddr>,
    /// `netbird_config.stuns[]` as parsed by the ICE orchestrator
    /// (`crate::ice::parse_stun_uri`): the set core gathers against, with
    /// unusable URIs already dropped by that parser.
    pub stuns: Vec<crate::ice::StunServer>,
}

impl ConnectorHandle {
    /// This session's INTERNAL N2-H freeze inputs (see [`CoreFreezeInputs`]).
    fn core_freeze_inputs(&self) -> CoreFreezeInputs {
        CoreFreezeInputs {
            management_dial: self.management_dial,
            stuns: self.shared.stuns(),
        }
    }
}

/// The installed connector's INTERNAL N2-H freeze inputs — the ONE read the
/// device collector's production host ([`crate::n2h_collector::WgFeedHost`])
/// performs, at arm time (design §4.0: freeze instant = arm instant, so the
/// stun set of the LAST applied sync is what gets frozen). No connector ⇒
/// empty inputs, and the collector then records honest reasoned absences
/// (fail-closed; never an invented address).
pub fn core_freeze_inputs() -> CoreFreezeInputs {
    match connector_slot().as_ref() {
        Some(handle) => handle.core_freeze_inputs(),
        None => CoreFreezeInputs::default(),
    }
}

/// Enable diagnostic destination counts for this connector session. The same
/// bounded destination parser used by the collector validates the ticket.
pub fn n2_tun_dest_configure_json(options_json: &str) -> String {
    let options = match crate::n2h_collector::ArmOptions::parse_json(options_json) {
        Ok(options) if !options.n2_capture_32.is_empty() => options,
        _ => return format!("{{{},{}}}", jbool("ok", false), jstr("error", "invalid-targets")),
    };
    let outer = match config::parse_document(options_json) {
        Ok(Json::Obj(fields)) => {
            let Some((_, value)) = fields.iter().find(|(key, _)| key == "n2_outer_destinations") else {
                return format!("{{{},{}}}", jbool("ok", false), jstr("error", "outer-destinations-required"));
            };
            let Json::Arr(items) = value else {
                return format!("{{{},{}}}", jbool("ok", false), jstr("error", "invalid-outer-destinations"));
            };
            if items.is_empty() || items.len() > 128 {
                return format!("{{{},{}}}", jbool("ok", false), jstr("error", "invalid-outer-destinations"));
            }
            let mut outer = Vec::new();
            for item in items {
                let Json::Str(text) = item else {
                    return format!("{{{},{}}}", jbool("ok", false), jstr("error", "invalid-outer-destinations"));
                };
                let Some((proto_text, destination)) = text.split_once('/') else {
                    return format!("{{{},{}}}", jbool("ok", false), jstr("error", "invalid-outer-destinations"));
                };
                let proto = match proto_text { "tcp" => 6, "udp" => 17, _ => {
                    return format!("{{{},{}}}", jbool("ok", false), jstr("error", "invalid-outer-destinations"));
                } };
                let Some((ip_text, port_text)) = destination.rsplit_once(':') else {
                    return format!("{{{},{}}}", jbool("ok", false), jstr("error", "invalid-outer-destinations"));
                };
                let Ok(ip) = ip_text.parse::<std::net::Ipv4Addr>() else {
                    return format!("{{{},{}}}", jbool("ok", false), jstr("error", "invalid-outer-destinations"));
                };
                let Ok(port) = port_text.parse::<u16>() else {
                    return format!("{{{},{}}}", jbool("ok", false), jstr("error", "invalid-outer-destinations"));
                };
                let tuple = (ip.octets(), proto, port);
                if port == 0 || !options.n2_capture_32.contains(&tuple.0) || outer.contains(&tuple) {
                    return format!("{{{},{}}}", jbool("ok", false), jstr("error", "invalid-outer-destinations"));
                }
                outer.push(tuple);
            }
            outer
        }
        _ => return format!("{{{},{}}}", jbool("ok", false), jstr("error", "invalid-outer-destinations")),
    };
    let slot = connector_slot();
    let Some(feed) = slot.as_ref().and_then(|handle| handle.wg_feed.as_ref()) else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-connector"));
    };
    if !feed.set_n2_destination_targets(options.n2_capture_32) {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "tun-already-attached"));
    }
    if !feed.set_n2_outer_targets(outer) {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "tun-already-attached"));
    }
    format!("{{{}}}", jbool("ok", true))
}

/// A control is only possible while the N2 diagnostic destination tee and
/// data plane are live. The caller owns fd and must close it after this call.
pub fn n2_control_send_json(fd: i32, destination: &str, proto: &str, id: &str) -> String {
    let tee = {
        let slot = connector_slot();
        slot.as_ref().and_then(|handle| handle.wg_feed.as_ref())
            .and_then(|feed| feed.n2_control_tee())
    };
    let Some(tee) = tee else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "not-armed-or-no-tun"));
    };
    match crate::n2_control::send(&tee, fd, destination, proto, id) {
        Ok(line) => {
            crate::hilog::emit(&line);
            format!("{{{},{}}}", jbool("ok", true), jstr("status", tee.control_status(id)))
        }
        Err(error) => format!("{{{},{}}}", jbool("ok", false), jstr("error", error)),
    }
}

pub fn n2_control_status_json(id: &str) -> String {
    let tee = {
        let slot = connector_slot();
        slot.as_ref().and_then(|handle| handle.wg_feed.as_ref())
            .and_then(|feed| feed.n2_control_tee())
    };
    let Some(tee) = tee else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "not-armed-or-no-tun"));
    };
    format!("{{{},{},{}}}", jbool("ok", true), jstr("status", tee.control_status(id)),
        jnum("leaks", tee.leak_count()))
}

// ---------------------------------------------------------------------------
// N2 T5a「B 断流刺激」(preregistration v4 §4 B column / §7 T5a)
// ---------------------------------------------------------------------------

/// The hostname of the last protected DNS resolution the shell asked for.
/// The D stimulus re-queries exactly this host (the class's own current
/// query); with none there is nothing to re-query and D answers `no-target`
/// instead of inventing a name.
fn last_dns_host() -> &'static Mutex<String> {
    static LAST_DNS_HOST: OnceLock<Mutex<String>> = OnceLock::new();
    LAST_DNS_HOST.get_or_init(|| Mutex::new(String::new()))
}

/// The connector-side half of the stimulus: the classes no socket-level
/// syscall can break.
///
/// * `I` — drop every peer's current ICE session and rearm negotiation
///   (`PeerIceOrchestrator::break_ice`): the session's dup sockets are closed
///   and the next pump re-gathers/re-checks. That is the frozen
///   "endpoint-directed break → re-check" for the ICE class.
/// * `Wb` / `Ws` — drop every peer's currently selected WG path
///   (`WgPeerApplier::reset_peer_paths`): endpoint cleared, the selected
///   egress duplicate closed, the handshake campaign reset, so the next ICE
///   nomination re-attaches a fresh selected socket and re-handshakes over it.
///   LIMIT: this does NOT close the base device socket (`wg_fwd_open` dup) —
///   that fd has no core-side rebuild path (the shell re-feeds it via
///   `connector_wg_socket_feed`), so a Wb-only peer stays pathless until ICE
///   nominates again.
/// * `D` — force ONE fresh protected DNS query for the remembered host (new
///   protected fd, new `N2_SOCK` registration, own receipt line).
struct ConnectorStimulusSink;

impl crate::n2_stimulus::Sink for ConnectorStimulusSink {
    fn act(&self, kind: &str) -> usize {
        // Never hold the connector slot while locking the orchestrator/WG
        // seam (the stop path takes them in this order too).
        let handle = {
            let slot = connector_slot();
            slot.as_ref().cloned()
        };
        let Some(handle) = handle else { return 0 };
        match kind {
            "I" => handle.ice.lock_poison().break_ice(crate::sys::mono_ms()),
            "Wb" | "Ws" => handle.wg.reset_peer_paths(),
            "D" => force_protected_dns_requery(&handle),
            _ => 0,
        }
    }
}

/// D: one fresh protected query on the SAME hostname the session last
/// resolved. `0` = nothing to act on (no remembered host, no protected socket
/// supply, no resolver) — the caller records `no-target`, never a fake apply.
fn force_protected_dns_requery(handle: &Arc<ConnectorHandle>) -> usize {
    let host = last_dns_host().lock_poison().clone();
    if host.is_empty() {
        return 0;
    }
    let Some(supply) = handle.socket_source.as_ref().and_then(|source| source.socket_supply()) else {
        return 0;
    };
    if supply.dns_servers().is_empty() {
        return 0;
    }
    let runtime = handle.runtime.clone();
    // NOT `start_dns_lookup`: nothing polls that id, so the receipt is emitted
    // here instead and the lookup map stays free of orphan entries.
    runtime.spawn(async move {
        let started = std::time::Instant::now();
        let outcome = crate::protected_dns::resolve(&host, &supply).await;
        let (result, detail) = match outcome {
            Ok(ip) => ("ok", ip.to_string()),
            Err(error) => ("failed", error.to_string()),
        };
        hilog::emit(&format!(
            "N2_STIMULUS_DNS|host={host}|result={result}|detail={}|elapsed_ms={}|diag=true|is_evidence=false",
            detail.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-' | '_') {
                c
            } else {
                '_'
            }).collect::<String>(),
            started.elapsed().as_millis()));
    });
    1
}

/// N2 T5a: apply one outer-stream stimulus and answer the shell. See
/// [`crate::n2_stimulus`] for the frozen scope, the gating and the marker.
pub fn n2_stimulus_json(kind: &str, id: &str) -> String {
    let outcome = crate::n2_stimulus::apply(kind, id);
    format!(
        "{{{},{},{},{},{},{}}}",
        jbool("ok", outcome.result == "applied"),
        jstr("result", outcome.result),
        jstr("reason", &outcome.reason),
        jinum("target_fd", outcome.target_fd as i64),
        jstr("remote", &outcome.remote),
        jstr("line", &outcome.line)
    )
}

// ---------------------------------------------------------------------------
// N2 P4「A 列生产事件触发原语」(v4.1 §4 A 列操作性定义) + P5 夹心编排
// ---------------------------------------------------------------------------

/// The connector-side seam of the A-column trigger (P4). It resolves the live
/// connector through the slot at call time (never holds one), so it can never
/// outlive its session.
///
/// * `M` — ONE `GetServerKey` on the EXISTING logged-in management client
///   (`logout_slot` holds a clone whose tonic `Channel` is the same connection
///   the Sync stream runs over). NO new dial and NO new `Sync` stream: a fresh
///   stream is exactly the recreate-risk shape the frozen text forbids.
/// * `D` — the existing D requery primitive (`force_protected_dns_requery`),
///   one fresh protected query on a new protected fd; the `N2_DNS` receipt it
///   emits is the authoritative device record of the query.
/// * `R` — arms the [`crate::n2_a_event::R_WAIT_MS`] timeout on the session's
///   own runtime; the relay reader calls back when the server-driven
///   HealthCheck is answered.
/// * `S` — N2 T7a (item 5, frozen v4.1 §4 S row (i)): arms the
///   [`crate::n2_a_event::S_WAIT_MS`] timeout on the session's own runtime and
///   sends NOTHING. The registered signal receive watchdog calls back when it
///   sends its NEXT natural silence-triggered HEARTBEAT
///   ([`crate::n2_a_event::natural_heartbeat_about_to_send`] /
///   `natural_heartbeat_sent` from `signal.rs`), so only a production frame
///   the client would have sent anyway can complete the event.
struct ConnectorATrigger;

impl crate::n2_a_event::Trigger for ConnectorATrigger {
    fn management_round_trip(&self) -> Result<(), String> {
        // Never hold the connector slot across the RPC.
        let (handle, client) = {
            let slot = connector_slot();
            let Some(handle) = slot.as_ref().cloned() else { return Err("no-connector".into()) };
            let client = handle.logout_slot.lock_poison().clone();
            (handle, client)
        };
        let Some(mut client) = client else {
            return Err("no-live-management-connection".into());
        };
        let runtime = handle.runtime.clone();
        let waited = runtime.block_on(async {
            tokio::time::timeout(
                core::time::Duration::from_millis(crate::n2_a_event::M_RPC_WAIT_MS),
                client.get_server_key(),
            )
            .await
        });
        match waited {
            Ok(Ok(_key)) => Ok(()),
            // Sanitized class token only — never server text, never key bytes.
            Ok(Err(error)) => Err(format!("management-rpc-{}", ErrorClass::from_management(&error).as_str())),
            Err(_) => Err("management-rpc-timeout".into()),
        }
    }

    fn dns_requery(&self) -> Result<(), String> {
        let handle = { connector_slot().as_ref().cloned() };
        let Some(handle) = handle else { return Err("no-connector".into()) };
        if force_protected_dns_requery(&handle) == 0 {
            return Err("no-target".into());
        }
        Ok(())
    }

    fn arm_s_watch(&self, id: &str, wait_ms: u64, seq: u64) -> Result<(), String> {
        let handle = { connector_slot().as_ref().cloned() };
        let Some(handle) = handle else { return Err("no-connector".into()) };
        let id = id.to_string();
        handle.runtime.spawn(async move {
            tokio::time::sleep(core::time::Duration::from_millis(wait_ms)).await;
            // The (id, seq) pair makes a stale timer harmless: it can only
            // complete the watch it was armed for.
            let _ = crate::n2_a_event::s_watch_timeout(&id, seq);
        });
        Ok(())
    }

    fn arm_healthcheck_watch(&self, id: &str, wait_ms: u64, seq: u64) -> Result<(), String> {
        let handle = { connector_slot().as_ref().cloned() };
        let Some(handle) = handle else { return Err("no-connector".into()) };
        let id = id.to_string();
        handle.runtime.spawn(async move {
            tokio::time::sleep(core::time::Duration::from_millis(wait_ms)).await;
            // The (id, seq) pair makes a stale timer harmless: it can only
            // complete the watch it was armed for.
            let _ = crate::n2_a_event::r_watch_timeout(&id, seq);
        });
        Ok(())
    }
}

/// N2 P4: apply one A-column trigger and answer the shell. See
/// [`crate::n2_a_event`] for the frozen scope, the gating and the marker.
pub fn n2_a_event_json(kind: &str, id: &str) -> String {
    let outcome = crate::n2_a_event::apply(kind, id);
    let ok = matches!(outcome.result, "sent" | "armed" | "requery" | "observed");
    format!(
        "{{{},{},{},{},{},{},{}}}",
        jbool("ok", ok),
        jstr("result", outcome.result),
        jstr("reason", &outcome.reason),
        jstr("kind", &outcome.kind),
        jinum("fd", outcome.fd as i64),
        jinum("dup_fd", outcome.dup_fd as i64),
        jstr("line", &outcome.line)
    )
}

/// N2 P5: register one PRE sandwich leg (called by the shell BEFORE it sends
/// the pre control). `marker` is the pre control's id and `post_marker` the
/// POST control's own (different) id — N2 T7a item 1. The core keeps the
/// ordering verdict.
pub fn n2_sandwich_pre_json(kind: &str, marker: &str, post_marker: &str) -> String {
    let outcome = crate::n2_sandwich::pre(kind, marker, post_marker);
    format!(
        "{{{},{},{},{},{}}}",
        jbool("ok", outcome.result == "registered"),
        jstr("result", outcome.result),
        jstr("reason", &outcome.reason),
        jstr("kind", &outcome.kind),
        jstr("marker", &outcome.marker)
    )
}

/// N2 P5: close one leg with the shell's post outcome (`observed` = the post
/// control HIT, `unsat` = one of the named misses). `marker` is the POST
/// control's own marker (N2 T7a item 1), exactly the one `n2_sandwich_poll`
/// handed back.
pub fn n2_sandwich_post_json(kind: &str, marker: &str, result: &str, reason: &str) -> String {
    let outcome = crate::n2_sandwich::post(kind, marker, result, reason);
    format!(
        "{{{},{},{},{},{}}}",
        jbool("ok", outcome.result != "rejected"),
        jstr("result", outcome.result),
        jstr("reason", &outcome.reason),
        jstr("kind", &outcome.kind),
        jstr("marker", &outcome.marker)
    )
}

/// N2 P5: which legs are ready for their POST control right now. Called on
/// every shell watcher tick while armed; also closes legs whose first
/// production send never arrived within the documented bound.
pub fn n2_sandwich_poll_json() -> String {
    let posts = crate::n2_sandwich::poll(crate::sys::mono_ms());
    let items = posts
        .iter()
        .map(|post| format!("{{{},{}}}", jstr("kind", &post.kind), jstr("marker", &post.marker)))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{},\"posts\":[{}]}}", jbool("ok", true), items)
}

// ---------------------------------------------------------------------------
// N2 第一批 C/D 列「App 侧切网夹心」(2026-09-29)
// ---------------------------------------------------------------------------
//
// The host issues the plan over the `n2_cd_plan` private channel while it still
// has a live link (Wi-Fi). Everything after that happens on the device, inside
// the armed session: the shell sends the pre control through the production
// opener, the network observer reports the switch, the core observes the kind's
// production receipt (a NEW live registration of that class) and the shell
// sends the post control. The core owns the ordering verdict; the shell owns
// the platform I/O.

/// Accept one C/D switching-sandwich plan. `start_verdict` is the SHELL's
/// start-network classification (empty = the column's expected starting bearer
/// was measured). Answers `{ok,result,reason,id,pre_marker,post_marker,line}`.
#[allow(clippy::too_many_arguments)]
pub fn n2_cd_plan_json(id: &str, column: &str, kind: &str, rep: i32, target: &str,
    net_before: &str, local_before: &str, start_verdict: &str) -> String {
    let rep = if rep < 0 { 0 } else { rep as u32 };
    let outcome = crate::n2_cd::plan(crate::sys::mono_ms(), id, column, kind, rep, target,
        net_before, local_before, start_verdict);
    format!(
        "{{{},{},{},{},{},{},{}}}",
        jbool("ok", outcome.result == "planned"),
        jstr("result", outcome.result),
        jstr("reason", &outcome.reason),
        jstr("id", &outcome.id),
        jstr("pre_marker", &format!("{id}-pre")),
        jstr("post_marker", &format!("{id}-post")),
        jstr("line", &outcome.line)
    )
}

/// The shell's verdict of the PRE control (`hit` only when its bounded wait saw
/// the core tee's HIT).
pub fn n2_cd_pre_json(id: &str, result: &str, reason: &str) -> String {
    let outcome = crate::n2_cd::pre(crate::sys::mono_ms(), id, result, reason);
    cd_outcome_json(&outcome)
}

/// D column: the Wi-Fi → cellular PREPARATION hop (`verdict` = the shell's
/// named refusal token, empty when the hop was measured). Recorded, never the D
/// measurement: no control is sent for it and it does not open the recovery
/// gate.
pub fn n2_cd_prepare_json(id: &str, verdict: &str, net_after: &str, local_after: &str) -> String {
    let outcome = crate::n2_cd::prepare(crate::sys::mono_ms(), id, verdict, net_after,
        local_after);
    cd_outcome_json(&outcome)
}

/// D column: the preparation's recovery on the cellular side (the shell's
/// `VPN_NETWORK_RECOVERED`). Only then is the plan in position for the FORMAL
/// switch back to Wi-Fi.
pub fn n2_cd_ready_json(id: &str) -> String {
    let outcome = crate::n2_cd::ready(crate::sys::mono_ms(), id);
    cd_outcome_json(&outcome)
}

/// The shell's switch report WITH the identity baseline the receipt verdict
/// needs: `verdict` is the pure module's direction verdict (empty = the
/// measured transition matches the column), `net_id_after` the NEW network's
/// platform net id, and `identity_at_switch` the OLD network's live
/// registration of this kind as the `N2_SOCK` receipt names it —
/// `(fd = shell original, dup_fd = core-owned descriptor, generation =
/// protect_generation)`. A missing/negative pair is `None`: the core then
/// closes the plan `unsat|switch-identity-required` (fail-closed) instead of
/// falling back to the process-global registration counter.
#[allow(clippy::too_many_arguments)]
pub fn n2_cd_switch_identified_json(id: &str, verdict: &str, net_after: &str, local_after: &str,
    net_id_after: i64, identity_fd: i32, identity_dup_fd: i32, identity_generation: i64) -> String {
    let net_id_after = if net_id_after < 0 { 0 } else { net_id_after as u64 };
    let generation = if identity_generation < 0 { 0 } else { identity_generation as u64 };
    let identity = if identity_fd >= 0 && identity_dup_fd >= 0 && identity_generation >= 0 {
        Some(crate::n2_cd::Identity::new(identity_fd, identity_dup_fd, generation))
    } else {
        None
    };
    let outcome = crate::n2_cd::switch_identified(crate::sys::mono_ms(), id, verdict, net_after,
        local_after, net_id_after, identity);
    cd_outcome_json(&outcome)
}

/// N2 C/D: the identity of the LIVE registration of one outer class, as the
/// `N2_SOCK` receipts name it — `{ok,fd,dup_fd,generation,net_id}` where `fd`
/// is the shell's original descriptor, `dup_fd` the core-owned copy and
/// `generation` the shell's `protect_generation`.
///
/// This is the read-only seam the switch report is fed from: the platform net
/// id is the shell's own measurement, but the `dup_fd`/generation of the
/// registration the receipt must differ from can only come from the armed
/// trace. Nothing is emitted and no state is created — an unarmed session (or a
/// class without a live registration) answers `{ok:false,error:"no-live-registration"}`,
/// and the shell then reports `-1`/`-1`/`-1` so the core fail-closes by name.
pub fn n2_cd_identity_json(kind: &str) -> String {
    match crate::n2_socket_trace::live_identity(kind) {
        Some((fd, dup_fd, generation, net_id)) => format!(
            "{{{},{},{},{},{}}}",
            jbool("ok", true),
            jinum("fd", fd as i64),
            jinum("dup_fd", dup_fd as i64),
            jnum("generation", generation),
            jnum("net_id", net_id)
        ),
        None => format!("{{{},{}}}", jbool("ok", false),
            jstr("error", "no-live-registration")),
    }
}

/// One watcher-tick step of the plan. `{ok,step,phase,id,kind,column,fd,dup_fd,remote,
/// marker,result,reason,line}`; `step=receipt` means "send the POST control with
/// `marker` now", `step=pre` "send the PRE control with `marker` now",
/// `step=recover` "the tee HIT the pre: recover the production connections".
pub fn n2_cd_poll_json() -> String {
    let step = crate::n2_cd::poll(crate::sys::mono_ms());
    format!(
        "{{{},{},{},{},{},{},{},{},{},{},{},{},{}}}",
        jbool("ok", true),
        jstr("step", step.step),
        jstr("phase", &step.phase),
        jstr("id", &step.id),
        jstr("kind", &step.kind),
        jstr("column", &step.column),
        jinum("fd", step.fd as i64),
        jinum("dup_fd", step.dup_fd as i64),
        jstr("remote", &step.remote),
        jstr("marker", &step.marker),
        jstr("result", &step.result),
        jstr("reason", &step.reason),
        jstr("line", &step.line)
    )
}

/// Close the plan with the shell's post-control verdict (`observed` requires the
/// core tee's HIT and it must follow the receipt).
pub fn n2_cd_post_json(id: &str, result: &str, reason: &str) -> String {
    let outcome = crate::n2_cd::post(crate::sys::mono_ms(), id, result, reason);
    cd_outcome_json(&outcome)
}

fn cd_outcome_json(outcome: &crate::n2_cd::Outcome) -> String {
    format!(
        "{{{},{},{},{},{}}}",
        jbool("ok", outcome.result != "rejected"),
        jstr("result", outcome.result),
        jstr("reason", &outcome.reason),
        jstr("id", &outcome.id),
        jstr("line", &outcome.line)
    )
}

/// N2 C/D: configure the armed-only app-internal evidence ring. Validation
/// only; `{ok}` / `{ok:false,error}`.
pub fn n2_ring_configure_json(path: &str, max_bytes: i64, segments: i64) -> String {
    let max_bytes = if max_bytes < 0 { 0 } else { max_bytes as u64 };
    let segments = if segments < 0 { 0 } else { segments as u32 };
    match crate::n2_ring::configure(path, max_bytes, segments) {
        Ok(()) => format!("{{{}}}", jbool("ok", true)),
        Err(error) => format!("{{{},{}}}", jbool("ok", false), jstr("error", error)),
    }
}

/// N2 C/D: append ONE shell-side line verbatim to the ring. Silent no-op while
/// the ring is unconfigured (the core's own lines ride `hilog::emit`).
pub fn n2_ring_append_json(line: &str) -> String {
    crate::n2_ring::append(line);
    format!("{{{},{}}}", jbool("ok", true), jbool("enabled", crate::n2_ring::enabled()))
}

/// N2 C/D: disable the ring (every arming change / teardown). The FILE stays —
/// the operator still has to fetch it.
pub fn n2_ring_disable_json() -> String {
    crate::n2_ring::disable();
    format!("{{{}}}", jbool("ok", true))
}

/// N2 C/D: the current ring counters as one `N2_RING_STATS` line (emitted here,
/// so the line is itself recorded in the ring) plus the machine-readable view.
pub fn n2_ring_status_json() -> String {
    let line = crate::n2_ring::status_line_now();
    crate::hilog::emit(&line);
    format!("{{{},{},{}}}", jbool("ok", true), jbool("enabled", crate::n2_ring::enabled()),
        jstr("line", &line))
}

/// N2 C/D: the ring's head/tail/failure/gap MANIFEST as one `N2_RING_MANIFEST`
/// line. Like the status line it is emitted through [`crate::hilog::emit`], so
/// the manifest request is itself recorded in the ring, and the same line comes
/// back in the reply: `{ok,enabled,line}`.
pub fn n2_ring_manifest_json() -> String {
    let line = crate::n2_ring::manifest_line_now();
    crate::hilog::emit(&line);
    format!("{{{},{},{}}}", jbool("ok", true), jbool("enabled", crate::n2_ring::enabled()),
        jstr("line", &line))
}

/// N2 C/D: one seq-paged dump. The request document is the core's own
/// (`{"from_seq":N,"to_seq":N,"count":N,"session":"…"}`; the legacy
/// `{"start":<line index>}` form is refused with
/// `legacy-start-index-unsupported`) and the reply is
/// `{ok,result,reason,from_seq,to_seq,count,emitted,frozen_tail_seq,first_seq,
/// last_seq,records,bytes,malformed,lost_records,overflow,acked_seq,
/// lines:[…],"summary":"…"}`.
///
/// `acked_seq` is the retrieved watermark AS IT STANDS (the shell mirrors it in
/// its receipt, which therefore never claims an advancement): a dump moves
/// nothing — only [`n2_ring_ack_json`] can.
///
/// `lines` are the `N2_RING_CHUNK` records ([`crate::n2_ring::chunk_line`], one
/// durable `seq` each, `line=` verbatim and LAST) and `summary` is the single
/// `N2_RING_DUMP` line ([`crate::n2_ring::dump_summary_line`]). NOTHING is
/// emitted on HiLog here: the caller decides where the chunk lines go, and the
/// SUMMARY MUST NOT be written back into the ring (it is not ring content).
pub fn n2_ring_dump_json(request_json: &str) -> String {
    let page = crate::n2_ring::dump_page(request_json);
    let lines: Vec<String> = page.lines.iter()
        .map(|(seq, line)| crate::n2_ring::chunk_line(*seq, page.count, line))
        .collect();
    let emitted = lines.len();
    let summary = crate::n2_ring::dump_summary_line(&page, emitted);
    let body = lines.iter()
        .map(|line| format!("\"{}\"", crate::util::json_escape(line)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},\"lines\":[{}],{}}}",
        jbool("ok", page.result != "rejected"),
        jstr("result", page.result),
        jstr("reason", page.reason),
        jnum("from_seq", page.from_seq),
        jnum("to_seq", page.to_seq),
        jnum("count", page.count as u64),
        jnum("emitted", emitted as u64),
        jnum("frozen_tail_seq", page.frozen_tail_seq),
        jnum("first_seq", page.first_seq),
        jnum("last_seq", page.last_seq),
        jnum("records", page.records),
        jnum("bytes", page.bytes),
        jnum("malformed", page.malformed),
        jnum("lost_records", page.lost_records),
        jbool("overflow", page.overflow),
        jnum("acked_seq", page.acked_seq),
        body,
        jstr("summary", &summary)
    )
}

/// N2 C/D: the EXPLICIT host ACK — confirm that every record with
/// `seq <= up_to_seq` of the arm named by `session` was retrieved and persisted
/// by the host. This is the ONLY call that moves the retrieved watermark, and
/// therefore the only thing that lets the next rotation drop the oldest
/// segment; the dump/manifest/status surfaces never move it.
///
/// The request document is the core's own
/// (`{"session":"<arm token>","up_to_seq":<durable seq>}`; BOTH keys required).
/// Answers `{ok,result,reason,session,up_to_seq,acked_seq,previous_acked_seq,
/// advanced}`; a refusal keeps `advanced=false` and the untouched watermark.
pub fn n2_ring_ack_json(request_json: &str) -> String {
    let request = match crate::n2_ring::parse_ack_request(request_json) {
        Ok(request) => request,
        Err(reason) => {
            return format!(
                "{{{},{},{},\"session\":\"\",\"up_to_seq\":0,\"acked_seq\":0,\
                 \"previous_acked_seq\":0,\"advanced\":false}}",
                jbool("ok", false), jstr("result", "rejected"), jstr("reason", reason)
            )
        }
    };
    match crate::n2_ring::ack_seq(request.up_to_seq, &request.session) {
        Ok(outcome) => format!(
            "{{{},{},{},{},{},{},{},{}}}",
            jbool("ok", true),
            jstr("result", "acked"),
            jstr("reason", "ok"),
            jstr("session", &outcome.session),
            jnum("up_to_seq", outcome.up_to_seq),
            jnum("acked_seq", outcome.acked_seq),
            jnum("previous_acked_seq", outcome.previous_acked_seq),
            jbool("advanced", outcome.advanced)
        ),
        Err(reason) => {
            let unchanged = crate::n2_ring::acked_seq();
            format!(
                "{{{},{},{},{},{},{},{},{}}}",
                jbool("ok", false),
                jstr("result", "rejected"),
                jstr("reason", reason),
                jstr("session", &request.session),
                jnum("up_to_seq", request.up_to_seq),
                jnum("acked_seq", unchanged),
                jnum("previous_acked_seq", unchanged),
                jbool("advanced", false)
            )
        }
    }
}

/// The `connector_stop()` implementation (idempotent; safe with no
/// connector at all).
pub fn connector_stop_json() -> String {
    let mut slot = connector_slot();
    // N2-H: the collector's host seam is installed by the start entries and
    // lives EXACTLY as long as the session (see `install_collector_host`) —
    // a stopped connector leaves no seam behind, so no connector state (WG
    // feed/config incl. key material) outlives `connector_stop()`. An already
    // ARMED window keeps its own host handle: its document stays cached for
    // the §2.4-R2 replay.
    crate::n2h_collector::uninstall_host();
    // N2 T5a: the stimulus sink is per-session too (it resolves the live
    // connector through the slot, which is cleared below). Dropping it here
    // means no stimulus can act on a stopped session.
    crate::n2_stimulus::clear_sink();
    crate::n2_a_event::clear_trigger();
    *last_dns_host().lock_poison() = String::new();
    crate::socket_supply::shutdown();
    match slot.as_ref() {
        Some(handle) => {
            let json = handle.stop_json();
            // after an explicit stop the slot is free for a fresh start
            *slot = None;
            json
        }
        None => format!(
            "{{{},{},{}}}",
            jbool("ok", true),
            jbool("already_stopped", true),
            jstr("state", ConnState::Disconnected.as_str()),
        ),
    }
}

// ---------------------------------------------------------------------------
// shared field parsers (JSON helpers on top of the crate's strict reader)
// ---------------------------------------------------------------------------

fn field_str<'a>(val: &'a Json, field: &'static str) -> Result<&'a str, ConfigError> {
    match val {
        Json::Str(s) => Ok(s),
        _ => Err(ConfigError::Field { field, reason: "expected a string".into() }),
    }
}

fn field_u64(val: &Json, field: &'static str) -> Result<u64, ConfigError> {
    match val {
        Json::Num(n) => Ok(*n),
        _ => Err(ConfigError::Field { field, reason: "expected a number".into() }),
    }
}

fn field_bool(val: &Json, field: &'static str) -> Result<bool, ConfigError> {
    match val {
        Json::Bool(b) => Ok(*b),
        _ => Err(ConfigError::Field { field, reason: "expected a boolean".into() }),
    }
}

// ---------------------------------------------------------------------------
// unit tests (no I/O: parsing, sanitization, deadline semantics, registry)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn protected_dns_start_poll_success_failure_timeout_and_invalidation() {
        // The task completion is controlled here; protected_dns has separate
        // fake-server tests for the actual protected UDP exchange.
        for (outcome, expected) in [
            (Ok(std::net::Ipv4Addr::new(192, 0, 2, 19)), "{\"ok\":true,\"ip\":\"192.0.2.19\"}"),
            (Err(std::io::Error::other("dns:invalid-response")), "{\"ok\":false,\"error\":\"dns-resolution-failed\"}"),
            (Err(std::io::Error::other("socket-supply-timeout")), "{\"ok\":false,\"error\":\"dns-resolution-failed\"}"),
        ] {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let id = start_dns_lookup(&tokio::runtime::Handle::current(), async move {
                rx.await.expect("test completion")
            });
            assert_eq!(connector_protected_dns_poll_json(id), "{\"ok\":true,\"pending\":true}");
            tx.send(outcome).expect("send DNS result");
            let final_state = tokio::time::timeout(std::time::Duration::from_secs(1), async {
                loop {
                    let state = connector_protected_dns_poll_json(id);
                    if !state.contains("\"pending\"") { break state; }
                    tokio::task::yield_now().await;
                }
            }).await.expect("DNS task completion");
            assert_eq!(final_state, expected);
            assert_eq!(connector_protected_dns_poll_json(id), "{\"ok\":false,\"error\":\"dns-lookup-not-found\"}");
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        let id = start_dns_lookup(&tokio::runtime::Handle::current(), async move {
            rx.await.expect("test completion")
        });
        assert_eq!(connector_protected_dns_poll_json(id), "{\"ok\":true,\"pending\":true}");
        // physical_network_changed clears this same map before cancelling supply.
        dns_lookups().lock_poison().remove(&id);
        assert_eq!(connector_protected_dns_poll_json(id), "{\"ok\":false,\"error\":\"dns-lookup-not-found\"}");
        drop(tx);
    }

    const SENTINEL_KEY: &str = "SENTINEL-SETUP-KEY-n3-5-DO-NOT-LOG";
    const SENTINEL_JWT: &str = "SENTINEL-JWT-header.payload.sig";

    fn test_private_key_b64(seed: u8) -> String {
        base64::engine::general_purpose::STANDARD.encode([seed; 32])
    }

    fn test_config_json() -> String {
        format!(
            "{{\"management_url\":\"http://127.0.0.1:8012\",\"private_key\":\"{}\"}}",
            test_private_key_b64(1)
        )
    }

    #[test]
    fn config_parses_with_defaults_and_transport_rules() {
        let cfg = ConnectorConfig::from_json(&test_config_json()).expect("minimal config");
        assert_eq!(cfg.management_url, "http://127.0.0.1:8012");
        assert_eq!(cfg.transport, GrpcTransport::Plaintext);
        assert_eq!(cfg.connect_timeout, DEFAULT_CONNECT_TIMEOUT);
        assert_eq!(cfg.request_timeout, DEFAULT_REQUEST_TIMEOUT);
        assert_eq!(cfg.renew_lead, DEFAULT_RENEW_LEAD);
        assert_eq!(cfg.renew_check_interval, DEFAULT_RENEW_CHECK_INTERVAL);
        assert_eq!(cfg.meta.os_name, "harmonyos");
        assert_eq!(cfg.meta.hostname, "netbird-ohos");

        // https requires an injected CA (no system store)
        let https_no_ca = format!(
            "{{\"management_url\":\"https://mgmt.example:443\",\"private_key\":\"{}\"}}",
            test_private_key_b64(7)
        );
        let err = ConnectorConfig::from_json(&https_no_ca).unwrap_err();
        assert!(matches!(err, ConfigError::Field { field: "ca_pem", .. }), "{err}");

        // https with a CA selects the TLS transport
        let https = format!(
            "{{\"management_url\":\"https://mgmt.example:443\",\"ca_pem\":[\"-----BEGIN CERTIFICATE-----\"],\"private_key\":\"{}\"}}",
            test_private_key_b64(7)
        );
        let cfg = ConnectorConfig::from_json(&https).expect("https config");
        assert!(matches!(cfg.transport, GrpcTransport::Tls(_)));

        // missing url / bad key are rejected
        let err = ConnectorConfig::from_json("{}").unwrap_err();
        assert!(matches!(err, ConfigError::Field { field: "management_url", .. }), "{err}");
        let bad_key = format!(
            "{{\"management_url\":\"http://x:1\",\"private_key\":\"{}\"}}",
            base64::engine::general_purpose::STANDARD.encode([1u8; 8])
        );
        let err = ConnectorConfig::from_json(&bad_key).unwrap_err();
        assert!(matches!(err, ConfigError::Field { field: "private_key", .. }), "{err}");

        // N3-7 flags default FALSE (fail-closed defaults) and parse as bools
        assert!(!cfg.force_default_route);
        assert!(!cfg.allow_unprotected_management);
        let opted = ConnectorConfig::from_json(&format!(
            "{{\"management_url\":\"http://127.0.0.1:8012\",\"private_key\":\"{}\",\
             \"force_default_route\":true,\"allow_unprotected_management\":true}}",
            test_private_key_b64(2)
        ))
        .expect("opt-in config");
        assert!(opted.force_default_route);
        assert!(opted.allow_unprotected_management);
        let bad_flag = format!(
            "{{\"management_url\":\"http://x:1\",\"private_key\":\"{}\",\"force_default_route\":\"yes\"}}",
            test_private_key_b64(2)
        );
        let err = ConnectorConfig::from_json(&bad_flag).unwrap_err();
        assert!(matches!(err, ConfigError::Field { field: "force_default_route", .. }), "{err}");
    }

    /// N3-7 gap 3: `terminal` = the worker ENDED BY ITSELF (fatal /
    /// budget exhaustion). A user stop (`disconnected`) is NOT terminal and
    /// neither is any running state.
    #[test]
    fn terminal_semantics_pinned() {
        assert!(is_terminal_state(false, ConnState::Failed), "fatal/exhausted");
        assert!(!is_terminal_state(true, ConnState::Failed), "cannot be running+failed");
        assert!(!is_terminal_state(true, ConnState::Connecting));
        assert!(!is_terminal_state(true, ConnState::Connected));
        assert!(!is_terminal_state(true, ConnState::Reconnecting));
        assert!(!is_terminal_state(false, ConnState::Disconnected), "user stop");
        assert!(!is_terminal_state(false, ConnState::Closed));
        // and the JSON field renders it
        let mut s = ConnectorStatus {
            running: false,
            state: ConnState::Failed,
            started_at_unix: None,
            last_update_unix: None,
            peer_count: 0,
            route_count: 0,
            reconnects: 0,
            last_error: Some(ErrorClass::Network),
            deadline: SessionDeadline::Unknown,
            renew_attempts: 0,
            wg_apply_failed: false,
            wg_apply_errors: 0,
            logout_ok: None,
            terminal: true,
            ice: IceOrchestratorSummary::default(),
            signal: SignalLinkStatus::default(),
            wg: crate::wg_device::WgDataplaneStatus::default(),
            recreate: RecreateStatus::default(),
            relay: RelayStatus::disabled(),
        };
        assert!(s.to_json().contains("\"terminal\":true"), "{}", s.to_json());
        s.terminal = false;
        assert!(s.to_json().contains("\"terminal\":false"), "{}", s.to_json());
        // N8: the recreate summary rides the document and renders its tokens
        s.recreate = RecreateStatus {
            required: true,
            count: 1,
            exhausted: false,
            cooling_down: false,
            reason: "route-set-changed".to_string(),
        };
        assert!(
            s.to_json().contains(
                "\"recreate\":{\"required\":true,\"count\":1,\"exhausted\":false,\
                 \"cooling_down\":false,\"reason\":\"route-set-changed\"}"
            ),
            "{}",
            s.to_json()
        );
        // N5d: the signal summary rides the document too (registered /
        // reconnects / class-only last error)
        s.signal = SignalLinkStatus {
            registered: true,
            reconnects: 3,
            last_error: Some(ErrorClass::Network),
        };
        let js = s.to_json();
        assert!(
            js.contains("\"signal\":{\"registered\":true,\"reconnects\":3,\
                         \"last_error\":{\"class\":\"network\",\"status\":0}}"),
            "{js}"
        );
        assert!(matches!(config::parse_document(&js), Ok(Json::Obj(_))));
    }

    /// Credential discipline: the parsed config must not retain raw private
    /// key bytes in any Debug-visible form, and the secrets struct redacts
    /// both fields.
    #[test]
    fn debug_output_never_contains_secrets() {
        let secrets = ConnectorSecrets {
            setup_key: SENTINEL_KEY.into(),
            jwt: SENTINEL_JWT.into(),
        };
        let debug = format!("{secrets:?}");
        assert!(!debug.contains(SENTINEL_KEY), "setup key leaked via Debug: {debug}");
        assert!(!debug.contains(SENTINEL_JWT), "jwt leaked via Debug: {debug}");
        assert!(debug.contains("REDACTED"));

        // the config's private key flows through EnvelopeKeyPair whose
        // Debug prints the PUBLIC key only (crate::envelope)
        let cfg = ConnectorConfig::from_json(&test_config_json()).expect("config");
        let debug = format!("{cfg:?}");
        assert!(!debug.contains(&test_private_key_b64(1)), "private key leaked: {debug}");

        // and a handle's Debug stays secret-free too (built with sentinels)
        let handle_debug = format!(
            "{:?}",
            ConnectorSecrets { setup_key: SENTINEL_KEY.into(), jwt: SENTINEL_JWT.into() }
        );
        assert!(!handle_debug.contains(SENTINEL_KEY));
    }

    #[test]
    fn secrets_parse_and_validate() {
        let ok = ConnectorSecrets::from_json(&format!("{{\"setup_key\":\"{SENTINEL_KEY}\"}}"))
            .expect("setup key only");
        assert_eq!(ok.setup_key, SENTINEL_KEY);
        assert!(ok.jwt.is_empty());

        let ok = ConnectorSecrets::from_json("{\"jwt\":\"tok\"}").expect("jwt only");
        assert_eq!(ok.jwt, "tok");

        let ok = ConnectorSecrets::from_json("{\"jwt_token\":\"tok2\"}").expect("jwt alias");
        assert_eq!(ok.jwt, "tok2");

        let err = ConnectorSecrets::from_json("{}").unwrap_err();
        assert!(matches!(err, ConfigError::Field { field: "setup_key", .. }), "{err}");
        let err = ConnectorSecrets::from_json("{\"setup_key\":\"\",\"jwt\":\"\"}").unwrap_err();
        assert!(matches!(err, ConfigError::Field { field: "setup_key", .. }), "{err}");
    }

    #[test]
    fn error_classes_carry_no_message_text() {
        let cases: Vec<(ManagementError, ErrorClass, u16)> = vec![
            (
                ManagementError::Auth { status: 7, message: "invalid setup key".into() },
                ErrorClass::Auth { status: 7 },
                7,
            ),
            (ManagementError::Timeout, ErrorClass::Timeout, 0),
            (ManagementError::Server { status: 13 }, ErrorClass::Server { status: 13 }, 13),
            (ManagementError::Network("refused".into()), ErrorClass::Network, 0),
            (ManagementError::Parse("boom".into()), ErrorClass::Parse, 0),
            (
                ManagementError::UnsupportedUrl("http://x".into()),
                ErrorClass::UnsupportedUrl,
                0,
            ),
            (
                ManagementError::Request { status: 5, message: "not found".into() },
                ErrorClass::Request { status: 5 },
                5,
            ),
        ];
        for (err, want, code) in cases {
            let class = ErrorClass::from_management(&err);
            assert_eq!(class, want);
            assert_eq!(class.status_code(), code);
            let json = class.to_json();
            assert!(!json.contains("invalid"), "message leaked: {json}");
            assert!(!json.contains("boom"), "message leaked: {json}");
            assert!(!json.contains("refused"), "message leaked: {json}");
            assert!(!json.contains("not found"), "message leaked: {json}");
        }
        assert_eq!(
            ErrorClass::Auth { status: 7 }.to_json(),
            "{\"class\":\"auth\",\"status\":7}"
        );
    }

    /// 3-state deadline application (upstream ApplySessionDeadline,
    /// engine_authsession.go:17-62): unset keeps, zero disables, value sets.
    #[test]
    fn session_deadline_three_state_application() {
        let mut d = SessionDeadline::Unknown;
        d.apply_wire(None);
        assert_eq!(d, SessionDeadline::Unknown, "unset keeps");
        d.apply_wire(Some(1_700_000_000));
        assert_eq!(d, SessionDeadline::At(1_700_000_000));
        d.apply_wire(None);
        assert_eq!(d, SessionDeadline::At(1_700_000_000), "unset keeps the anchor");
        d.apply_wire(Some(0));
        assert_eq!(d, SessionDeadline::Disabled, "explicit zero disables");
        d.apply_wire(Some(42));
        assert_eq!(d, SessionDeadline::At(42));
        assert_eq!(d.as_str(), "set");
        assert_eq!(SessionDeadline::Unknown.as_str(), "unknown");
        assert_eq!(SessionDeadline::Disabled.as_str(), "disabled");
    }

    /// 0051 §7 item 3：`connector_status()` 的 `ice_sockets` 段带 core 的每
    /// peer 需求（`need_per_peer` = 允许接口数 × 2；-1 = 枚举失败，壳层回退）。
    /// 逐字冻结：壳层按 `peers × need_per_peer` 定池，改字段名就是改协议。
    #[test]
    fn ice_sockets_section_carries_the_core_per_peer_demand() {
        assert_eq!(
            ice_sockets_section(7, 12, 6),
            "\"ice_sockets\":{\"queued\":7,\"taken\":12,\"need_per_peer\":6}"
        );
        assert_eq!(
            ice_sockets_section(-1, -1, -1),
            "\"ice_sockets\":{\"queued\":-1,\"taken\":-1,\"need_per_peer\":-1}",
            "-1 on all three = no shell-fed source and no enumerable interface set"
        );
        // 接进一份状态文档后仍须被严格读取器接受（尾段追加，前文字段不动，
        // 且与生产路径同一段拼接代码——缺右花括号这类错误必须在这里就暴露）
        let doc = with_ice_sockets_section(
            "{\"running\":true,\"state\":\"disconnected\"}",
            3,
            4,
            6,
        );
        assert_eq!(
            doc,
            "{\"running\":true,\"state\":\"disconnected\",\
             \"ice_sockets\":{\"queued\":3,\"taken\":4,\"need_per_peer\":6}}"
        );
        assert!(
            matches!(config::parse_document(&doc), Ok(Json::Obj(_))),
            "the strict reader accepts the full appended document: {doc}"
        );
        assert!(
            doc.ends_with(",\"ice_sockets\":{\"queued\":3,\"taken\":4,\"need_per_peer\":6}}"),
            "the section is appended last, inside the root object: {doc}"
        );
    }

    #[test]
    fn status_json_contract_and_strict_reader_roundtrip() {
        let status = ConnectorStatus {
            running: true,
            state: ConnState::Connected,
            started_at_unix: Some(1_000),
            last_update_unix: Some(2_000),
            peer_count: 3,
            route_count: 2,
            reconnects: 1,
            last_error: Some(ErrorClass::Auth { status: 7 }),
            deadline: SessionDeadline::At(1_700_000_000),
            renew_attempts: 1,
            wg_apply_failed: false,
            wg_apply_errors: 0,
            logout_ok: None,
            terminal: false,
            ice: IceOrchestratorSummary {
                peers: 2,
                connected: 1,
                failed: 1,
                endpoints_applied: 1,
                reachable: 1,
                last_error: Some(ErrorClass::Network),
                ..Default::default()
            },
            signal: SignalLinkStatus {
                registered: true,
                reconnects: 1,
                last_error: Some(ErrorClass::Timeout),
            },
            wg: crate::wg_device::WgDataplaneStatus {
                fed_socket: true,
                fed_tun: true,
                device_up: true,
                ready: true,
                peers_with_session: 1,
                handshakes: 3,
                tx_packets: 9,
                tx_bytes: 900,
                rx_packets: 8,
                rx_bytes_to_tun: 800,
                dropped_no_route: 1,
                decrypt_errors: 2,
                // N13-D1: the per-peer attribution tail — one peer with a
                // live session riding the carrier, one registered peer with
                // no session (both entry shapes frozen here).
                wg_peers: vec![
                    crate::wg_device::WgPeerStatus {
                        pub_key_b64: "bjEzLWQxLXBlZXItc2Vzc2lvbg==".to_string(),
                        endpoint: None,
                        allowed_ips: vec![([10, 30, 30, 2], 32)],
                        session_established: true,
                        expired: false,
                        last_handshake_s: 12,
                        egress_local: Some(([192, 168, 1, 5], 51820)),
                        carrier_attached: true,
                        on_carrier: true,
                    },
                    crate::wg_device::WgPeerStatus {
                        pub_key_b64: "bjEzLWQxLXBlZXItaWRsZQ==".to_string(),
                        endpoint: Some(([203, 0, 113, 9], 51820)),
                        allowed_ips: vec![([10, 30, 30, 3], 32), ([10, 30, 30, 4], 32)],
                        session_established: false,
                        expired: false,
                        last_handshake_s: -1,
                        egress_local: None,
                        carrier_attached: false,
                        on_carrier: false,
                    },
                ],
                ..Default::default()
            },
            recreate: RecreateStatus {
                required: false,
                count: 2,
                exhausted: false,
                cooling_down: true,
                reason: "route-set-changed".to_string(),
            },
            relay: RelayStatus::disabled(),
        };
        let json = status.to_json();
        assert!(json.contains("\"running\":true"), "{json}");
        assert!(json.contains("\"state\":\"connected\""), "{json}");
        assert!(json.contains("\"peer_count\":3"), "{json}");
        assert!(json.contains("\"route_count\":2"), "{json}");
        assert!(json.contains("\"reconnects\":1"), "{json}");
        assert!(json.contains("\"last_error\":{\"class\":\"auth\",\"status\":7}"), "{json}");
        assert!(json.contains("\"session_expiry\":\"set\""), "{json}");
        assert!(json.contains("\"session_expires_at_unix\":1700000000"), "{json}");
        assert!(json.contains("\"logout_ok\":null"), "{json}");
        // N5c: the per-peer ICE summary rides the status document
        // (unknown_signal = signal frames dropped for an unmatched sender key;
        // device run 3 stalled the whole negotiation with this as the ONLY
        // symptom, so it is part of the frozen contract).
        assert!(
            json.contains(
                "\"ice\":{\"peers\":2,\"idle\":0,\"gathering\":0,\"checking\":0,\"connected\":1,\
                 \"disconnected\":0,\"failed\":1,\"endpoints_applied\":1,\"reachable\":1,\
                 \"unknown_signal\":0,\
                 \"last_error\":{\"class\":\"network\",\"status\":0}}"
            ),
            "{json}"
        );
        // N7: the WG data-plane summary (feeds / device / REAL readiness +
        // device counters — no key material). N2-H: byte counters ride along
        // so counter reconciliation can read connector_status() directly.
        // N13-D2: the relay-carrier counters are APPENDED after the pre-D2
        // fields (positional contract; zero here — the fixture has no lane).
        // N13-D1: the per-peer session attribution is APPENDED after those,
        // so `wg_peers` is the LAST `wg` field (relay stays last in the
        // document — the `wg` object only moves its own tail).
        assert!(
            json.contains(
                "\"wg\":{\"fed_socket\":true,\"fed_tun\":true,\"device_up\":true,\"ready\":true,\
                 \"peers_with_session\":1,\"handshakes\":3,\"tx_packets\":9,\"tx_bytes\":900,\
                 \"rx_packets\":8,\"rx_bytes_to_tun\":800,\
                 \"dropped_no_route\":1,\"decrypt_errors\":2,\"carrier_tx_packets\":0,\
                 \"carrier_tx_bytes\":0,\"carrier_rejects\":0,\"carrier_takeovers\":0,\
                 \"direct_restores\":0,\"wg_peers\":[\
                 {\"pub_key_b64\":\"bjEzLWQxLXBlZXItc2Vzc2lvbg==\",\"endpoint\":null,\
                 \"allowed_ips\":[\"10.30.30.2/32\"],\"session_established\":true,\
                 \"expired\":false,\"last_handshake_s\":12,\
                 \"egress_local\":\"192.168.1.5:51820\",\"carrier_attached\":true,\
                 \"on_carrier\":true},\
                 {\"pub_key_b64\":\"bjEzLWQxLXBlZXItaWRsZQ==\",\
                 \"endpoint\":\"203.0.113.9:51820\",\
                 \"allowed_ips\":[\"10.30.30.3/32\",\"10.30.30.4/32\"],\
                 \"session_established\":false,\"expired\":false,\"last_handshake_s\":null,\
                 \"egress_local\":null,\"carrier_attached\":false,\"on_carrier\":false}]}"
            ),
            "{json}"
        );
        // N8: the controlled-recreate summary rides the same document
        assert!(
            json.contains(
                "\"recreate\":{\"required\":false,\"count\":2,\"exhausted\":false,\
                 \"cooling_down\":true,\"reason\":\"route-set-changed\"}"
            ),
            "{json}"
        );
        // the relay segment stays the LAST section of the document (the ets
        // consumer reads by field; N13-D1 must not move it)
        assert!(
            json.ends_with(&format!("\"relay\":{}}}", status.relay.to_json())),
            "relay must stay last in the document: {json}"
        );
        assert!(matches!(config::parse_document(&json), Ok(Json::Obj(_))));

        let empty = ConnectorStatus {
            running: false,
            state: ConnState::Disconnected,
            started_at_unix: None,
            last_update_unix: None,
            peer_count: 0,
            route_count: 0,
            reconnects: 0,
            last_error: None,
            deadline: SessionDeadline::Unknown,
            renew_attempts: 0,
            wg_apply_failed: false,
            wg_apply_errors: 0,
            logout_ok: None,
            terminal: false,
            ice: IceOrchestratorSummary::default(),
            signal: SignalLinkStatus::default(),
            wg: crate::wg_device::WgDataplaneStatus::default(),
            recreate: RecreateStatus::default(),
            relay: RelayStatus::disabled(),
        }
        .to_json();
        assert!(empty.contains("\"running\":false"), "{empty}");
        assert!(empty.contains("\"state\":\"disconnected\""), "{empty}");
        assert!(empty.contains("\"last_error\":null"), "{empty}");
        assert!(empty.contains("\"session_expiry\":\"unknown\""), "{empty}");
        assert!(empty.contains("\"session_expires_at_unix\":null"), "{empty}");
        assert!(
            empty.contains("\"ice\":{\"peers\":0,\"idle\":0"),
            "default ICE summary must render: {empty}"
        );
        assert!(
            empty.contains("\"signal\":{\"registered\":false,\"reconnects\":0,\"last_error\":null}"),
            "default signal summary must render: {empty}"
        );
        assert!(
            empty.contains(
                "\"wg\":{\"fed_socket\":false,\"fed_tun\":false,\"device_up\":false,\
                 \"ready\":false,\"peers_with_session\":0,\"handshakes\":0,\"tx_packets\":0,\
                 \"tx_bytes\":0,\"rx_packets\":0,\"rx_bytes_to_tun\":0,\
                 \"dropped_no_route\":0,\"decrypt_errors\":0,\"carrier_tx_packets\":0,\
                 \"carrier_tx_bytes\":0,\"carrier_rejects\":0,\"carrier_takeovers\":0,\
                 \"direct_restores\":0,\"wg_peers\":[]}"
            ),
            "default wg summary must render: {empty}"
        );
        assert!(
            empty.contains(
                "\"recreate\":{\"required\":false,\"count\":0,\"exhausted\":false,\
                 \"cooling_down\":false,\"reason\":\"none\"}"
            ),
            "default recreate summary must render: {empty}"
        );
        assert!(matches!(config::parse_document(&empty), Ok(Json::Obj(_))));
    }

    #[test]
    fn wg_registry_replaces_and_clears() {
        let registry = WgPeerRegistry::new();
        assert!(registry.is_empty());
        let peers = vec![
            WgPeerEntry {
                pub_key_b64: "QUJDREVGRw==".into(),
                allowed_ips: vec![config::Route { addr: [10, 30, 30, 1], prefix_len: 32 }],
            },
            WgPeerEntry { pub_key_b64: "T0ZGTElORQ==".into(), allowed_ips: vec![] },
        ];
        registry.apply_peers(&peers).expect("apply");
        assert_eq!(registry.snapshot(), peers);
        assert_eq!(registry.len(), 2);
        // full-snapshot semantics: a smaller map replaces, not appends
        registry.apply_peers(&peers[..1]).expect("apply");
        assert_eq!(registry.snapshot(), peers[..1]);
        registry.clear();
        assert!(registry.is_empty());
    }

    /// N5c: endpoint landing on the registry — registered peers only
    /// (fail-closed), latest record wins, clear() tears endpoints down.
    #[test]
    fn wg_registry_endpoints_apply_replace_and_clear() {
        let registry = WgPeerRegistry::new();
        let peers = vec![WgPeerEntry {
            pub_key_b64: "UDI=".into(),
            allowed_ips: vec![config::Route { addr: [10, 0, 0, 1], prefix_len: 32 }],
        }];
        registry.apply_peers(&peers).expect("apply");
        // unregistered peer is REJECTED, never silently stored
        assert!(registry.apply_endpoint("VU5LTk9XTg==", [127, 0, 0, 1], 51820).is_err());
        registry.apply_endpoint("UDI=", [127, 0, 0, 1], 51820).expect("land");
        assert_eq!(registry.endpoint("UDI="), Some(([127, 0, 0, 1], 51820)));
        // re-selection replaces the endpoint
        registry.apply_endpoint("UDI=", [127, 0, 0, 1], 51821).expect("replace");
        assert_eq!(registry.endpoint("UDI="), Some(([127, 0, 0, 1], 51821)));
        assert_eq!(registry.endpoints().len(), 1);
        registry.clear();
        assert_eq!(registry.endpoint("UDI="), None, "clear() must tear endpoints down");

        // the trait default is loud-unsupported (fail-closed for seams that
        // cannot land endpoints)
        struct NoEndpointWg;
        impl WgPeerApplier for NoEndpointWg {
            fn apply_peers(&self, _: &[WgPeerEntry]) -> Result<(), String> {
                Ok(())
            }
            fn clear(&self) {}
        }
        assert_eq!(
            NoEndpointWg.apply_endpoint("X", [1, 2, 3, 4], 1),
            Err("wg-endpoint-unsupported".to_string())
        );
    }

    /// N5c × N3-7 linkage through the REAL apply_update path: with the ICE
    /// orchestrator holding peers but NONE reachable, the default route
    /// stays HELD even when the WG seam reports tunnel_ready().
    #[test]
    fn apply_update_gate_holds_default_route_while_no_ice_peer_is_reachable() {
        struct ReadyWg;
        impl WgPeerApplier for ReadyWg {
            fn apply_peers(&self, _: &[WgPeerEntry]) -> Result<(), String> {
                Ok(())
            }
            fn clear(&self) {}
            fn tunnel_ready(&self) -> bool {
                true
            }
        }
        let shared = ConnectorShared::new(false, None);
        let ice = Arc::new(Mutex::new(PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![])),
            socks: Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source")),
            signal: Arc::new(crate::peer_conn::LoggingSignalExchange::default()),
            wg: Arc::new(ReadyWg),
            tie_breaker: Some(7),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        })));
        ice.lock_poison().set_peers(&["UEVFUjA=".to_string()]);
        let _ = shared.ice.set(ice.clone());

        shared.apply_update(
            &ReadyWg,
            &NoopHost,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: Some(crate::network_map::NetbirdServers {
                    stuns: vec!["stun:stun.netbird.io:3478".into()],
                    ..Default::default()
                }),
                network_map: Some(shell_test_map()),
            },
        );
        let json = shared.network_config_json();
        assert!(
            json.contains("\"default_route\":{\"allowed\":false,\"reason\":\"default-route-held:data-plane-not-ready\"}"),
            "no reachable ICE peer must hold the default route: {json}"
        );
        assert!(!json.contains("{\"network\":\"0.0.0.0/0\""), "held route leaked: {json}");
        // the orchestrator got the map's connectable peers + the stuns
        let guard = ice.lock_poison();
        assert_eq!(guard.peer_keys(), vec!["UEVFUjA=".to_string()]);
        assert_eq!(guard.stuns().len(), 1);
        // no peers at all in the orchestrator view → N3-7 semantics unchanged
        drop(guard);
        ice.lock_poison().set_peers(&[]);
        shared.apply_update(
            &ReadyWg,
            &NoopHost,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: None,
                network_map: Some(shell_test_map()),
            },
        );
        let ready_json = shared.network_config_json();
        assert!(
            ready_json.contains("\"default_route\":{\"allowed\":true"),
            "peers==0 must not change N3-7 semantics (ReadyWg is ready): {ready_json}"
        );
    }

    /// N2-H (design §4.0 stun + management rows): the collector's freeze
    /// sources come from the session's INTERNAL state — the stun set the ICE
    /// orchestrator parsed out of `netbird_config.stuns` and the address the
    /// management factory dials. Neither is in the public documents (§5.2).
    #[test]
    fn core_freeze_inputs_read_the_sessions_internal_stuns_and_dial_address() {
        struct NoIceWg;
        impl WgPeerApplier for NoIceWg {
            fn apply_peers(&self, _: &[WgPeerEntry]) -> Result<(), String> {
                Ok(())
            }
            fn clear(&self) {}
        }
        let shared = ConnectorShared::new(false, None);
        // no orchestrator (unit-test construction): no stuns, nothing invented
        assert!(shared.stuns().is_empty());
        let ice = Arc::new(Mutex::new(PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![])),
            socks: Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source")),
            signal: Arc::new(crate::peer_conn::LoggingSignalExchange::default()),
            wg: Arc::new(NoIceWg),
            tie_breaker: None,
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        })));
        let _ = shared.ice.set(ice);
        shared.apply_update(
            &NoIceWg,
            &NoopHost,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: Some(crate::network_map::NetbirdServers {
                    stuns: vec![
                        "stun:stun.netbird.io:3478".into(),
                        "stun:203.0.113.9".into(),
                        "turn:turn.netbird.io:3478".into(),
                        "not-a-uri".into(),
                    ],
                    ..Default::default()
                }),
                network_map: None,
            },
        );
        // exactly what the orchestrator gathers against: stun: URIs only
        // (`turn:` is rejected by `set_stuns`, `not-a-uri` by the parser)
        let stuns = shared.stuns();
        assert_eq!(stuns.len(), 2, "internal stun set: {stuns:?}");
        assert_eq!(stuns[0], crate::ice::StunServer { host: "stun.netbird.io".into(), port: 3478 });
        assert_eq!(stuns[1].host, "203.0.113.9");
        assert_eq!(stuns[1].port, 3478, "the stun: URI default port");

        // management: the protected factory carries the shell-resolved dial
        // address; the unprotected dev dial has none (→ reasoned gap)
        let cfg = ConnectorConfig::from_json(&test_config_json()).expect("config");
        let protected = GrpcManagementFactory::with_socket_source(
            &cfg,
            cfg.keys.clone(),
            Arc::new(ProtectedSocketFdSource::new_with_fd(-1).expect("empty tcp source")),
            "198.51.100.7:443".parse().expect("dial address"),
        );
        let expected: Option<SocketAddr> = Some("198.51.100.7:443".parse().expect("addr"));
        assert_eq!(protected.dial_address(), expected);
        assert_eq!(GrpcManagementFactory::new(&cfg, cfg.keys.clone()).dial_address(), None);
        struct StubFactory;
        impl ManagementFactory for StubFactory {
            fn connect(
                &self,
            ) -> Pin<
                Box<
                    dyn Future<Output = Result<ManagementGrpcClient, ManagementError>> + Send + '_,
                >,
            > {
                Box::pin(async { Err(ManagementError::Network("stub".into())) })
            }
        }
        assert_eq!(StubFactory.dial_address(), None, "a stub never invents an address");
        println!("[I5-wire] connector internal stuns={stuns:?} dial={expected:?}");
    }

    // N3-6: shell network-config snapshot -----------------------------------

    /// Recording-free host seam for the snapshot tests.
    struct NoopHost;
    impl ConfigApplier for NoopHost {
        fn apply(&self, _map: &NetworkMap) {}
        fn clear(&self) {}
    }

    fn shell_test_map() -> NetworkMap {
        use crate::network_map::{DnsConfig, ManagedRoute, NameServer, NameServerGroup, PeerInfo, PeerSelfConfig};
        NetworkMap {
            serial: 42,
            peer: Some(PeerSelfConfig {
                address: Some("10.64.0.7".into()),
                interface_dns: Some("100.100.0.1".into()),
                fqdn: Some("me.netbird.cloud".into()),
                mtu: Some(1380),
                routing_peer_dns_resolution_enabled: true,
                lazy_connection_enabled: false,
            }),
            peers: vec![
                PeerInfo {
                    wg_pub_key: "UEVFUjA=".into(),
                    allowed_ips: vec![
                        config::Route { addr: [10, 30, 30, 1], prefix_len: 32 },
                        config::Route { addr: [192, 168, 7, 0], prefix_len: 24 },
                    ],
                    fqdn: None,
                },
                PeerInfo {
                    wg_pub_key: "UEVFUjE=".into(),
                    allowed_ips: vec![],
                    fqdn: None,
                },
            ],
            peers_is_empty: false,
            offline_peers: vec![PeerInfo {
                wg_pub_key: "T0ZGTElORQ==".into(),
                allowed_ips: vec![config::Route { addr: [10, 9, 9, 9], prefix_len: 32 }],
                fqdn: None,
            }],
            routes: vec![
                ManagedRoute {
                    id: "r-default".into(),
                    network: config::Route { addr: [0, 0, 0, 0], prefix_len: 0 },
                    domains: vec![],
                    net_id: "net-d".into(),
                    network_type: 1,
                    peer: "relay".into(),
                    metric: 9999,
                    masquerade: false,
                    keep_route: false,
                    skip_auto_apply: false,
                },
                ManagedRoute {
                    id: "r-1".into(),
                    network: config::Route { addr: [172, 16, 0, 0], prefix_len: 12 },
                    domains: vec![],
                    net_id: "net-1".into(),
                    network_type: 1,
                    peer: "relay".into(),
                    metric: 10,
                    masquerade: false,
                    keep_route: false,
                    skip_auto_apply: false,
                },
            ],
            skipped_routes: vec![],
            dns: Some(DnsConfig {
                service_enable: true,
                name_server_groups: vec![
                    NameServerGroup {
                        name_servers: vec![
                            NameServer { ip: "1.1.1.1".into(), ns_type: 0, port: 53 },
                            NameServer { ip: "8.8.8.8".into(), ns_type: 1, port: 853 },
                        ],
                        primary: true,
                        domains: vec![],
                        search_domains_enabled: false,
                    },
                    // same resolver again: deduplicated in the snapshot
                    NameServerGroup {
                        name_servers: vec![NameServer { ip: "1.1.1.1".into(), ns_type: 0, port: 53 }],
                        primary: false,
                        domains: vec![],
                        search_domains_enabled: false,
                    },
                ],
            }),
        }
    }

    #[test]
    fn shell_address_strips_the_cidr_suffix_and_uses_its_prefix() {
        // Device-verified 2026-09-13 (device run 1): management delivers the
        // peer's own address as "ip/prefix" ("100.106.188.170/16"). Handing
        // that raw string to the shell made the platform reject the WHOLE
        // VpnConfig (NETMANAGER_EXT "invalid ip address" / "ParseAddress
        // failed", code 401 "Parameter error"), and the previous hardcoded
        // prefix produced a /32 host route for a /16 overlay. Both halves are
        // pinned here.
        let mut map = shell_test_map();
        if let Some(peer) = map.peer.as_mut() {
            peer.address = Some("100.106.188.170/16".into());
        }
        let snap = ShellNetworkConfig::from_map_gated(&map, false, false);
        assert_eq!(snap.address.as_deref(), Some("100.106.188.170"));
        assert_eq!(snap.address_prefix_len, Some(16));

        // A bare address (no suffix) keeps the host /32 default.
        if let Some(peer) = map.peer.as_mut() {
            peer.address = Some("100.106.188.170".into());
        }
        let bare = ShellNetworkConfig::from_map_gated(&map, false, false);
        assert_eq!(bare.address.as_deref(), Some("100.106.188.170"));
        assert_eq!(bare.address_prefix_len, Some(32));

        // IPv6 / unparseable: not handed to the shell at all.
        if let Some(peer) = map.peer.as_mut() {
            peer.address = Some("fd00::1/128".into());
        }
        let v6 = ShellNetworkConfig::from_map_gated(&map, false, false);
        assert_eq!(v6.address, None);
        assert_eq!(v6.address_prefix_len, None);
    }

    // N2-H I2: this contract test had LOST its `#[test]` attribute in
    // d9346c3 (dead since then — the shape assertions below never ran);
    // restored here because this increment extends exactly this contract.
    #[test]
    fn shell_network_config_snapshot_and_json_contract() {
        // N3-7 default: the WG data plane is NOT ready (registry), so the
        // managed default route is HELD — not exported for install.
        let snap = ShellNetworkConfig::from_map_gated(&shell_test_map(), false, false);
        assert_eq!(snap.serial, 42);
        assert_eq!(snap.address.as_deref(), Some("10.64.0.7"));
        assert_eq!(snap.address_prefix_len, Some(32));
        assert_eq!(snap.interface_dns.as_deref(), Some("100.100.0.1"));
        // default route HELD: absent from the exported routes, with reason
        assert!(!snap.default_route_allowed);
        assert_eq!(snap.default_route_reason, "default-route-held:data-plane-not-ready");
        assert_eq!(
            snap.routes,
            vec![
                ShellRouteEntry { network: "172.16.0.0/12".into(), is_default: false },
            ]
        );
        // resolvers deduplicated across groups, management order kept
        assert_eq!(
            snap.dns_servers,
            vec![
                ShellDnsServer { ip: "1.1.1.1".into(), port: 53 },
                ShellDnsServer { ip: "8.8.8.8".into(), port: 853 },
            ]
        );
        assert!(snap.dns_service_enable);
        // peers = remote + offline, public key + allowed-ips count only
        assert_eq!(snap.peers.len(), 3);
        assert_eq!(snap.peers[0].pub_key_b64, "UEVFUjA=");
        assert_eq!(snap.peers[0].allowed_ips, 2);
        assert_eq!(
            snap.peers[0].vpn_addresses,
            vec!["10.30.30.1/32".to_string(), "192.168.7.0/24".to_string()]
        );
        assert_eq!(snap.peers[2].pub_key_b64, "T0ZGTElORQ==");
        assert_eq!(snap.peers[2].allowed_ips, 1);

        let json = snap.to_json();
        for fragment in [
            "\"available\":true",
            "\"serial\":42",
            "\"address\":\"10.64.0.7\"",
            "\"address_prefix_len\":32",
            "\"interface_dns\":\"100.100.0.1\"",
            "{\"network\":\"172.16.0.0/12\",\"is_default\":false}",
            "\"service_enable\":true",
            "{\"ip\":\"1.1.1.1\",\"port\":53}",
            "{\"ip\":\"8.8.8.8\",\"port\":853}",
            "\"peer_count\":3",
            "{\"pub_key\":\"UEVFUjA=\",\"allowed_ips\":2,\
              \"vpn_addresses\":[\"10.30.30.1/32\",\"192.168.7.0/24\"]}",
            // N3-7 gate markers
            "\"default_route\":{\"allowed\":false,\"reason\":\"default-route-held:data-plane-not-ready\"}",
            // N2-H I2: the tail-appended stun section (nothing announced on a
            // from_map_gated snapshot = an honest empty array, never invented)
            "\"stuns\":[]",
        ] {
            assert!(json.contains(fragment), "missing {fragment} in {json}");
        }
        // N2-H I2 tail-append discipline: the stun section is the LAST field —
        // every pre-existing field keeps its order and shape
        assert!(json.ends_with(",\"stuns\":[]}"), "{json}");
        // the held default route must NOT appear as an installable route
        assert!(
            !json.contains("{\"network\":\"0.0.0.0/0\""),
            "held default route leaked into routes: {json}"
        );
        // the strict crate reader must accept the document (JSON contract)
        assert!(matches!(config::parse_document(&json), Ok(Json::Obj(_))));
        // credential discipline: no private-key field of any kind
        assert!(!json.contains("private"), "{json}");
        assert!(!json.contains("setup"), "{json}");
    }

    // N3-7: default-route safety gate --------------------------------------

    #[test]
    fn default_route_gate_zero_peers_holds_even_with_ready_tunnel() {
        let (allowed, reason) =
            ShellNetworkConfig::default_route_decision(0, true, false);
        assert!(!allowed);
        assert_eq!(reason, "default-route-held:no-usable-peer");
    }

    #[test]
    fn default_route_gate_peers_without_tunnel_holds() {
        let (allowed, reason) =
            ShellNetworkConfig::default_route_decision(3, false, false);
        assert!(!allowed);
        assert_eq!(reason, "default-route-held:data-plane-not-ready");
    }

    #[test]
    fn default_route_gate_install_requires_peers_and_ready_tunnel() {
        let (allowed, reason) =
            ShellNetworkConfig::default_route_decision(2, true, false);
        assert!(allowed);
        assert_eq!(reason, "default-route-allowed:peers-registered-and-tunnel-ready");
    }

    #[test]
    fn default_route_gate_force_opt_in_overrides_and_warns() {
        // explicit dev opt-in installs even while the data plane is dead,
        // and the reason token names the black-hole risk
        let (allowed, reason) =
            ShellNetworkConfig::default_route_decision(0, false, true);
        assert!(allowed);
        assert_eq!(reason, "default-route-forced:debug-opt-in-black-hole-risk");
    }

    #[test]
    fn default_route_exported_only_when_gate_allows() {
        // ready tunnel + peers: the default route is exported for install
        let snap = ShellNetworkConfig::from_map_gated(&shell_test_map(), false, true);
        assert!(snap.default_route_allowed);
        assert!(snap.routes.contains(&ShellRouteEntry {
            network: "0.0.0.0/0".into(),
            is_default: true,
        }));
        let json = snap.to_json();
        assert!(
            json.contains("{\"network\":\"0.0.0.0/0\",\"is_default\":true}"),
            "{json}"
        );
        assert!(
            json.contains("\"default_route\":{\"allowed\":true,\"reason\":\"default-route-allowed:peers-registered-and-tunnel-ready\"}"),
            "{json}"
        );

        // forced opt-in installs the route with the warning token
        let forced = ShellNetworkConfig::from_map_gated(&shell_test_map(), true, false);
        assert!(forced.default_route_allowed);
        assert!(forced.routes.contains(&ShellRouteEntry {
            network: "0.0.0.0/0".into(),
            is_default: true,
        }));
        assert_eq!(
            forced.default_route_reason,
            "default-route-forced:debug-opt-in-black-hole-risk"
        );

        // no peers at all (empty map): held even with a ready tunnel
        let mut empty_map = shell_test_map();
        empty_map.peers.clear();
        empty_map.offline_peers.clear();
        let held = ShellNetworkConfig::from_map_gated(&empty_map, false, true);
        assert!(!held.default_route_allowed);
        assert_eq!(held.default_route_reason, "default-route-held:no-usable-peer");
        assert!(!held
            .routes
            .iter()
            .any(|r| r.network == "0.0.0.0/0"));
    }

    #[tokio::test]
    async fn login_signal_survives_configless_sync_into_first_shell_snapshot() {
        let shared = ConnectorShared::new(false, None);
        let (exchange, rx) = RealSignalExchange::new();
        let rt = Arc::new(SignalRuntime::new(
            SignalLinkMaterials {
                runtime: tokio::runtime::Handle::current(),
                transport: GrpcTransport::Plaintext,
                connect_timeout: Duration::from_secs(1),
                request_timeout: Duration::from_secs(1),
                keys: EnvelopeKeyPair::from_secret_bytes(&[7u8; 32]),
                sockets: Arc::new(ProtectedSocketFdSource::new_with_fd(-1).expect("empty source")),
            },
            exchange,
            rx,
        ));
        let _ = shared.signal.set(rt);
        let login = crate::grpc::proto::LoginResponse {
            netbird_config: Some(crate::grpc::proto::NetbirdConfig {
                signal: Some(crate::grpc::proto::HostConfig {
                    uri: "https://signal.example:443".into(),
                    protocol: 1,
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        shared.remember_login_signal(&login);
        shared.apply_update(
            &WgPeerRegistry::new(),
            &NoopHost,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: None,
                network_map: Some(shell_test_map()),
            },
        );
        let json = shared.network_config_json();
        assert!(json.contains("\"signal\":\"https://signal.example:443\""), "{json}");
    }

    #[test]
    fn network_config_unavailable_until_first_map_then_cleared_on_stop_shape() {
        let shared = ConnectorShared::new(false, None);
        assert_eq!(
            shared.network_config_json(),
            "{\"available\":false,\"reason\":\"no-network-map\"}"
        );
        shared.apply_update(
            &WgPeerRegistry::new(),
            &NoopHost,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: None,
                network_map: Some(shell_test_map()),
            },
        );
        let json = shared.network_config_json();
        assert!(json.contains("\"available\":true"), "{json}");
        assert!(json.contains("\"serial\":42"), "{json}");
        // an update WITHOUT a network map keeps the last snapshot (only the
        // receipt is recorded) — same as the deadline/counts handling
        shared.apply_update(
            &WgPeerRegistry::new(),
            &NoopHost,
            &SyncUpdate { session_deadline_unix: None, netbird_config: None, network_map: None },
        );
        assert!(shared.network_config_json().contains("\"serial\":42"));
        // the global export path answers no-network-map with no connector
        // (CONNECTOR slot untouched by the unit tests)
    }

    /// N2-H I2 (design §13.13): the announced `netbird_config.stuns[].uri`
    /// list reaches the shell in the document's TAIL, VERBATIM — the shell
    /// applies core's own parse to the same strings, so a `turn:` URI must
    /// arrive as-is (it is refused shell-side by name), never pre-filtered
    /// into a silent omission.
    #[test]
    fn network_config_tail_carries_the_announced_stun_uris_verbatim() {
        let shared = ConnectorShared::new(false, None);
        let announced = vec![
            "stun:stun.netbird.io:3478".to_string(),
            "stun:203.0.113.9".to_string(),
            "turn:turn.netbird.io:3478".to_string(),
            "not-a-uri".to_string(),
        ];
        shared.apply_update(
            &WgPeerRegistry::new(),
            &NoopHost,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: Some(crate::network_map::NetbirdServers {
                    stuns: announced.clone(),
                    ..Default::default()
                }),
                network_map: Some(shell_test_map()),
            },
        );
        let json = shared.network_config_json();
        for uri in &announced {
            assert!(json.contains(&format!("{{\"uri\":\"{uri}\"}}")), "{uri} in {json}");
        }
        assert!(json.contains(&format!(
            "\"stuns\":[{{\"uri\":\"{}\"}},{{\"uri\":\"{}\"}},{{\"uri\":\"{}\"}},{{\"uri\":\"{}\"}}]}}",
            announced[0], announced[1], announced[2], announced[3]
        )), "announced order + tail position: {json}");
        assert!(matches!(config::parse_document(&json), Ok(Json::Obj(_))), "{json}");
        // every pre-existing field keeps its exact previous shape
        for fragment in [
            "\"available\":true",
            "\"serial\":42",
            "\"interface_dns\":\"100.100.0.1\"",
            "\"peer_count\":3",
            "\"default_route\":{\"allowed\":false,\"reason\":\"default-route-held:data-plane-not-ready\"}",
        ] {
            assert!(json.contains(fragment), "missing {fragment} in {json}");
        }

        // a config-less update keeps the last announcement visible (the same
        // rule `ConnectorShared::stuns()` follows: the orchestrator set is
        // only replaced by a config, so the shell must keep seeing it too)
        shared.apply_update(
            &WgPeerRegistry::new(),
            &NoopHost,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: None,
                network_map: Some(shell_test_map()),
            },
        );
        assert!(shared.network_config_json().contains(&format!(
            "{{\"uri\":\"{}\"}}", announced[0]
        )));

        // an explicit empty announcement clears the list: the honest gap,
        // never a stale endpoint
        shared.apply_update(
            &WgPeerRegistry::new(),
            &NoopHost,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: Some(crate::network_map::NetbirdServers {
                    stuns: vec![],
                    ..Default::default()
                }),
                network_map: Some(shell_test_map()),
            },
        );
        let cleared = shared.network_config_json();
        assert!(cleared.ends_with(",\"stuns\":[]}"), "{cleared}");
        assert!(!cleared.contains("stun.netbird.io"), "{cleared}");
    }

    // N8: controlled recreate ------------------------------------------------

    /// Test seam with an injectable `tunnel_ready` (the data-plane readiness
    /// the gate + recreate state react to).
    struct FlipWg(std::sync::atomic::AtomicBool);
    impl FlipWg {
        fn new(ready: bool) -> FlipWg {
            FlipWg(std::sync::atomic::AtomicBool::new(ready))
        }
        fn set(&self, ready: bool) {
            self.0.store(ready, Ordering::Release);
        }
    }
    impl WgPeerApplier for FlipWg {
        fn apply_peers(&self, _: &[WgPeerEntry]) -> Result<(), String> {
            Ok(())
        }
        fn clear(&self) {}
        fn tunnel_ready(&self) -> bool {
            self.0.load(Ordering::Acquire)
        }
    }

    /// Shared N8 fixture: a connector shared state with the shell_test_map
    /// applied (default route + 172.16.0.0/12) and the shell's INITIAL
    /// create() ACKed with the HELD route set (no 0.0.0.0/0).
    fn recreate_fixture(wg: &FlipWg, initial_ack_ms: u64) -> ConnectorShared {
        let shared = ConnectorShared::new(false, None);
        wg.set(false);
        shared.apply_update(
            wg,
            &NoopHost,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: None,
                network_map: Some(shell_test_map()),
            },
        );
        // the shell applied the HELD set at create() (no default route) and
        // ACKed it (initial create, not a recreate)
        shared.ack_applied_route_set(vec!["172.16.0.0/12".to_string()], false, initial_ack_ms);
        shared
    }

    /// 闸翻转触发重建一次：HOLD→ALLOWED（真实握手完成由 tunnel_ready 模拟）
    /// 恰好产生一个 `required` 请求（级别信号，不重复计数），recreate ACK
    /// 后清除，且期望集合不变时不再触发第二次。
    #[test]
    fn recreate_required_raises_once_on_gate_open_and_clears_on_recreate_ack() {
        let wg = FlipWg::new(false);
        let shared = recreate_fixture(&wg, 10_000);
        let mut now = 10_100u64;

        // still HELD and applied==desired: nothing requested
        shared.refresh_net_gate_at(&wg, now);
        let rec = shared.lock().recreate.clone();
        assert!(!rec.required && rec.count == 0 && rec.reason == "none", "{rec:?}");

        // the data plane becomes REALLY ready (handshake done): the desired
        // set now contains 0.0.0.0/0 → ONE required request, count untouched
        wg.set(true);
        shared.refresh_net_gate_at(&wg, now + 100);
        let rec = shared.lock().recreate.clone();
        assert!(rec.required, "gate open must demand a recreate: {rec:?}");
        assert_eq!(rec.reason, "route-set-changed");
        assert_eq!(rec.count, 0, "requesting must not consume the budget");
        assert!(!rec.exhausted && !rec.cooling_down);

        // repeated reads keep the LEVEL (idempotent) — the shell acts once
        for i in 1..5 {
            shared.refresh_net_gate_at(&wg, now + 100 + i * 1000);
            let rec = shared.lock().recreate.clone();
            assert!(rec.required && rec.count == 0, "level must persist: {rec:?}");
        }

        // the shell rebuilds and ACKs the NEW applied set (with the default)
        shared.ack_applied_route_set(
            vec!["0.0.0.0/0".to_string(), "172.16.0.0/12".to_string()],
            true,
            now + 6_000,
        );
        let rec = shared.lock().recreate.clone();
        assert_eq!(rec.count, 1, "exactly one completed recreate");
        assert!(!rec.required);
        // desired == applied now: further refreshes never re-request
        for i in 1..5 {
            shared.refresh_net_gate_at(&wg, now + 6_000 + i * 1_000);
            let rec = shared.lock().recreate.clone();
            assert!(!rec.required && rec.reason == "none", "{rec:?}");
        }
        // ...and the snapshot's exported routes really contain the default
        let json = shared.network_config_json();
        assert!(json.contains("{\"network\":\"0.0.0.0/0\",\"is_default\":true}"), "{json}");
    }

    /// 不重复重建 + 有界：连续翻转受冷却窗与次数上限约束；预算耗尽后
    /// `required` 永不再置位（`limit-reached`），快照闸保持诚实。
    #[test]
    fn recreate_flap_is_bounded_by_cooldown_and_limit() {
        let wg = FlipWg::new(true);
        let shared = ConnectorShared::new(false, None);
        shared.apply_update(
            &wg,
            &NoopHost,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: None,
                network_map: Some(shell_test_map()),
            },
        );
        // initial create applied the FULL (allowed) set
        shared.ack_applied_route_set(
            vec!["0.0.0.0/0".to_string(), "172.16.0.0/12".to_string()],
            false,
            1_000,
        );

        let full = vec!["0.0.0.0/0".to_string(), "172.16.0.0/12".to_string()];
        let held = vec!["172.16.0.0/12".to_string()];
        let mut t = 2_000u64;

        // flap 1: data plane dies → desired=held ≠ applied=full. No recreate
        // has completed yet → no cooldown anchor → requested immediately.
        wg.set(false);
        shared.refresh_net_gate_at(&wg, t);
        assert!(shared.lock().recreate.required);
        shared.ack_applied_route_set(held.clone(), true, t + 500);
        assert_eq!(shared.lock().recreate.count, 1);

        // flap 2 requested INSIDE the cooldown window: parked (cooling_down),
        // not required — the shell cannot be stormed
        wg.set(true);
        shared.refresh_net_gate_at(&wg, t + 500 + 1_000);
        let rec = shared.lock().recreate.clone();
        assert!(!rec.required && rec.cooling_down, "{rec:?}");
        // once the window has passed, the SAME divergence resurfaces
        shared.refresh_net_gate_at(&wg, t + 500 + RECREATE_COOLDOWN_MS + 1);
        let rec = shared.lock().recreate.clone();
        assert!(rec.required && !rec.cooling_down, "{rec:?}");
        shared.ack_applied_route_set(full.clone(), true, t + 500 + RECREATE_COOLDOWN_MS + 2);
        assert_eq!(shared.lock().recreate.count, 2);

        // flap 3: same cooldown shape, then the LAST budget unit is spent
        wg.set(false);
        shared.refresh_net_gate_at(&wg, t + 500 + RECREATE_COOLDOWN_MS + 3);
        assert!(!shared.lock().recreate.required, "inside cooldown");
        shared.refresh_net_gate_at(&wg, t + 1_000 + 2 * RECREATE_COOLDOWN_MS);
        assert!(shared.lock().recreate.required);
        shared.ack_applied_route_set(held.clone(), true, t + 1_000 + 2 * RECREATE_COOLDOWN_MS + 1);
        let rec = shared.lock().recreate.clone();
        assert_eq!(rec.count, RECREATE_MAX, "budget spent");
        assert!(rec.exhausted, "hitting the limit latches exhausted");

        // flap 4 (past exhaustion): the divergence exists but is NEVER
        // requested again — the churn is bounded by construction
        wg.set(true);
        shared.refresh_net_gate_at(&wg, t + 2_000 + 3 * RECREATE_COOLDOWN_MS);
        let rec = shared.lock().recreate.clone();
        assert!(!rec.required, "exhausted must never re-request: {rec:?}");
        assert_eq!(rec.reason, "limit-reached");
        assert_eq!(shared.lock().recreate.count, RECREATE_MAX);
    }

    /// 失败语义（Rust 半边）：重建途中喂入死 TUN fd 在 seam 边界被拒收，
    /// 设备保持旧 TUN 原样（无半状态）；ACK 校验拒绝空集合/坏形状。
    #[test]
    fn route_set_ack_rejects_bad_shapes_and_no_connector() {
        // no connector in the global slot (unit tests never start one)
        let json = connector_route_set_applied_json("{\"routes\":[\"0.0.0.0/0\"]}", true);
        assert_eq!(json, "{\"ok\":false,\"error\":\"no-connector\"}");

        // shape validation (pure parser)
        assert!(parse_route_set("{\"routes\":[\"0.0.0.0/0\",\"10.0.0.0/8\"]}").is_ok());
        assert!(parse_route_set("{\"routes\":[]}").is_err(), "empty set rejected");
        assert!(parse_route_set("{\"routes\":[1,2]}").is_err(), "non-string rejected");
        assert!(parse_route_set("{}").is_err(), "missing field rejected");
        assert!(parse_route_set("[]").is_err(), "non-object rejected");
    }

    // N5d: real signal link --------------------------------------------------

    #[test]
    fn signal_endpoint_derivation_rules() {
        // bare host:port (the wire shape, engine.go:1185) inherits the
        // management transport's security level
        let (ep, t) = derive_signal_endpoint(
            "signal.netbird.io:10000",
            &GrpcTransport::Tls(crate::grpc::GrpcTlsConfig::new(vec![b"CA".to_vec()])),
        )
        .expect("bare uri over tls mgmt");
        assert_eq!(ep, "https://signal.netbird.io:10000");
        assert!(matches!(t, GrpcTransport::Tls(_)));
        let (ep, t) =
            derive_signal_endpoint("sig.example:10000", &GrpcTransport::Plaintext).expect("bare");
        assert_eq!(ep, "http://sig.example:10000");
        assert_eq!(t, GrpcTransport::Plaintext);
        // explicit schemes are honored
        let (ep, t) = derive_signal_endpoint(
            "https://sig.example:10000",
            &GrpcTransport::Tls(crate::grpc::GrpcTlsConfig::new(vec![])),
        )
        .expect("explicit https");
        assert!(matches!(t, GrpcTransport::Tls(_)));
        assert_eq!(ep, "https://sig.example:10000");
        let (_ep, t) =
            derive_signal_endpoint("http://sig.example:10000", &GrpcTransport::Plaintext)
                .expect("explicit http");
        assert_eq!(t, GrpcTransport::Plaintext);
        // https uri over a plaintext management config → refused (no CA)
        assert!(derive_signal_endpoint("https://sig.example:10000", &GrpcTransport::Plaintext)
            .is_err());
        // empty variants refused
        assert!(derive_signal_endpoint("", &GrpcTransport::Plaintext).is_err());
        assert!(
            derive_signal_endpoint(
                "https://",
                &GrpcTransport::Tls(crate::grpc::GrpcTlsConfig::new(vec![]))
            )
            .is_err()
        );
    }

    #[test]
    fn signal_feed_requires_connector_source_and_addr() {
        // no connector at all (the CONNECTOR slot is untouched by unit tests)
        let json = connector_signal_socket_feed_json(3, "{\"connect_addr\":\"127.0.0.1:1\"}");
        assert_eq!(json, "{\"ok\":false,\"error\":\"no-connector\"}");
        let json = connector_signal_socket_feed_json(-1, "{\"connect_addr\":\"127.0.0.1:1\"}");
        assert_eq!(json, "{\"ok\":false,\"error\":\"no-connector\"}");
    }

    /// N5d wiring through the REAL apply_update path: the sync-delivered
    /// `netbird_config.signal` URI + the shell-fed connect address arm the
    /// signal link; with an EMPTY protected source the initial dial fails
    /// CLOSED (Network class, `registered` stays false, ICE never armed),
    /// and the shell snapshot carries the URI for the shell to feed.
    #[tokio::test]
    async fn apply_update_arms_signal_link_and_dials_fail_closed_without_fds() {
        let shared = Arc::new(ConnectorShared::new(false, None));
        let orch = Arc::new(Mutex::new(PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![])),
            socks: Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source")),
            signal: Arc::new(LoggingSignalExchange::default()),
            wg: Arc::new(WgPeerRegistry::new()),
            tie_breaker: Some(7),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        })));
        let _ = shared.ice.set(orch.clone());

        // signal runtime with an EMPTY protected fd source: nothing dials,
        // nothing unprotected — fail-closed by construction
        let (exchange, rx) = RealSignalExchange::new();
        let rt = Arc::new(SignalRuntime::new(
            SignalLinkMaterials {
                runtime: tokio::runtime::Handle::current(),
                transport: GrpcTransport::Plaintext,
                connect_timeout: Duration::from_secs(1),
                request_timeout: Duration::from_secs(1),
                keys: EnvelopeKeyPair::from_secret_bytes(&[9u8; 32]),
                sockets: Arc::new(ProtectedSocketFdSource::new_with_fd(-1).expect("empty tcp source")),
            },
            exchange.clone(),
            rx,
        ));
        let _ = rt.orch.set(orch.clone());
        let _ = shared.signal.set(rt.clone());
        shared.set_running(true);

        // BEFORE the uri: no link, signal_ready false
        assert!(!orch.lock_poison().signal_ready());

        // sync WITHOUT signal uri → nothing starts
        shared.apply_update(
            &WgPeerRegistry::new(),
            &NoopHost,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: Some(crate::network_map::NetbirdServers {
                    stuns: vec![],
                    ..Default::default()
                }),
                network_map: Some(shell_test_map()),
            },
        );
        assert!(
            shared.network_config_json().contains("\"signal\":null"),
            "no uri announced yet: {}",
            shared.network_config_json()
        );
        assert!(!orch.lock_poison().signal_ready());

        // sync WITH the uri + a pre-set connect addr → the link starts and
        // the initial protected dial fails closed (no fds)
        rt.set_connect_addr(std::net::SocketAddr::from(([127, 0, 0, 1], 1)));
        shared.apply_update(
            &WgPeerRegistry::new(),
            &NoopHost,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: Some(crate::network_map::NetbirdServers {
                    stuns: vec![],
                    signal: Some("127.0.0.1:1".into()),
                    ..Default::default()
                }),
                network_map: Some(shell_test_map()),
            },
        );
        let json = shared.network_config_json();
        assert!(
            json.contains("\"signal\":\"127.0.0.1:1\""),
            "uri must ride the shell snapshot: {json}"
        );
        assert!(json.contains("\"available\":true"), "{json}");
        // the link is running (worker spawned) and its dial fails CLOSED
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while rt.status().last_error != Some(ErrorClass::Network) {
            assert!(
                std::time::Instant::now() < deadline,
                "expected a fail-closed dial error (Network class), got {:?}",
                rt.status()
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let st = rt.status();
        assert!(!st.registered, "no fds → never registered");
        assert_eq!(st.reconnects, 0, "initial dial failures are not reconnects");
        assert!(
            !orch.lock_poison().signal_ready(),
            "ICE must stay unarmed while signal is down"
        );
        // status surface: registered=false, class-only error
        let status_json = ConnectorStatus {
            running: true,
            state: ConnState::Connected,
            started_at_unix: None,
            last_update_unix: None,
            peer_count: 0,
            route_count: 0,
            reconnects: 0,
            last_error: None,
            deadline: SessionDeadline::Unknown,
            renew_attempts: 0,
            wg_apply_failed: false,
            wg_apply_errors: 0,
            logout_ok: None,
            terminal: false,
            ice: orch.lock_poison().summary(),
            signal: rt.status(),
            wg: crate::wg_device::WgDataplaneStatus::default(),
            recreate: RecreateStatus::default(),
            relay: RelayStatus::disabled(),
        }
        .to_json();
        assert!(
            status_json.contains("\"signal\":{\"registered\":false,\"reconnects\":0,\
                                  \"last_error\":{\"class\":\"network\",\"status\":0}}"),
            "{status_json}"
        );
        assert!(matches!(config::parse_document(&status_json), Ok(Json::Obj(_))));

        // stop tears the worker down and clears the registered flag
        rt.shutdown();
    }

    /// N7: the network-config default-route gate reflects LIVE data-plane
    /// readiness (the device-backed seam's `tunnel_ready`), not only the
    /// readiness captured at sync time. Readiness flipping between reads
    /// re-gates the snapshot: held ⇄ allowed, and the `0.0.0.0/0` route is
    /// exported / re-held accordingly (rebuilt from the ungated record).
    #[test]
    fn network_config_gate_refreshes_with_live_tunnel_ready() {
        struct FlipWg(AtomicBool);
        impl WgPeerApplier for FlipWg {
            fn apply_peers(&self, _: &[WgPeerEntry]) -> Result<(), String> {
                Ok(())
            }
            fn clear(&self) {}
            fn tunnel_ready(&self) -> bool {
                self.0.load(Ordering::Acquire)
            }
        }
        let wg = Arc::new(FlipWg(AtomicBool::new(false)));
        let shared = ConnectorShared::new(false, None);

        // sync arrives while the data plane is down (the N7 production
        // shape: feeds/handshakes complete after create()): HELD
        shared.apply_update(
            wg.as_ref(),
            &NoopHost,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: None,
                network_map: Some(shell_test_map()),
            },
        );
        let held = shared.network_config_json();
        assert!(
            held.contains("\"default_route\":{\"allowed\":false,\"reason\":\
                           \"default-route-held:data-plane-not-ready\"}"),
            "{held}"
        );
        assert!(!held.contains("{\"network\":\"0.0.0.0/0\""), "{held}");

        // the WG session comes up between syncs: the next read re-gates
        wg.0.store(true, Ordering::Release);
        shared.refresh_net_gate(wg.as_ref());
        let allowed = shared.network_config_json();
        assert!(
            allowed.contains(
                "\"default_route\":{\"allowed\":true,\"reason\":\
                 \"default-route-allowed:peers-registered-and-tunnel-ready\"}"
            ),
            "{allowed}"
        );
        assert!(
            allowed.contains("{\"network\":\"0.0.0.0/0\",\"is_default\":true}"),
            "allowed default route must be exported: {allowed}"
        );
        // non-default routes survive the rebuild unchanged
        assert!(
            allowed.contains("{\"network\":\"172.16.0.0/12\",\"is_default\":false}"),
            "{allowed}"
        );

        // and it flips back when readiness is lost (session expiry shape)
        wg.0.store(false, Ordering::Release);
        shared.refresh_net_gate(wg.as_ref());
        let held_again = shared.network_config_json();
        assert!(
            held_again.contains("\"default_route\":{\"allowed\":false"),
            "{held_again}"
        );
        assert!(!held_again.contains("{\"network\":\"0.0.0.0/0\""), "{held_again}");
    }

    // -----------------------------------------------------------------------
    // N13-D1: relay connector integration
    // -----------------------------------------------------------------------

    /// The T0 Q3 two-path readiness matrix (pure, exact):
    /// (a) relay Ready only → ready; (b) ICE nominated → ready (unchanged);
    /// (c) ICE Failed AND relay not ready → NOT ready (never silently
    /// reachable); the WG session itself stays mandatory in every branch.
    #[test]
    fn relay_two_path_readiness_matrix() {
        assert!(two_path_readiness(true, false, true), "(a) relay Ready only");
        assert!(two_path_readiness(true, true, false), "(b) ICE nominated only");
        assert!(
            !two_path_readiness(true, false, false),
            "(c) ICE Failed + no relay must NOT be ready"
        );
        assert!(two_path_readiness(true, true, true), "both carriers");
        // no live WG session → never ready, whatever the carriers say
        assert!(!two_path_readiness(false, true, true));
        assert!(!two_path_readiness(false, false, true));
    }

    /// `relay_enabled` defaults FALSE (hard requirement) and parses as a
    /// bool; the relay TLS material comes from the SAME injected ca_pem as
    /// management (Tls transport → rustls connector), while a plaintext
    /// transport carries no root at all (rels:// then fails closed).
    #[test]
    fn relay_config_defaults_and_tls_material_mapping() {
        let base = format!(
            "{{\"management_url\":\"https://mgmt.example:443\",\"ca_pem\":[\"-----BEGIN CERTIFICATE-----\\nX\\n-----END CERTIFICATE-----\"],\"private_key\":\"{}\"}}",
            test_private_key_b64(3)
        );
        let cfg = ConnectorConfig::from_json(&base).expect("https config");
        assert!(!cfg.relay_enabled, "relay_enabled must DEFAULT false");
        assert!(matches!(cfg.transport, GrpcTransport::Tls(_)));

        let opted = ConnectorConfig::from_json(&format!(
            "{{\"management_url\":\"http://127.0.0.1:8012\",\"private_key\":\"{}\",\"relay_enabled\":true}}",
            test_private_key_b64(3)
        ))
        .expect("opted config");
        assert!(opted.relay_enabled);

        let bad = format!(
            "{{\"management_url\":\"http://x:1\",\"private_key\":\"{}\",\"relay_enabled\":\"yes\"}}",
            test_private_key_b64(3)
        );
        let err = ConnectorConfig::from_json(&bad).unwrap_err();
        assert!(matches!(err, ConfigError::Field { field: "relay_enabled", .. }), "{err}");

        // TLS material mapping: a usable root set → the rustls connector;
        // garbage PEM / plaintext transport → None (rels:// then refused
        // typed, never downgraded). A VALID certificate is exercised by the
        // tests/relay_connector_e2e.rs TLS round-trip.
        let ca = vec![b"-----BEGIN CERTIFICATE-----\nMII garbage\n-----END CERTIFICATE-----".to_vec()];
        let tls_transport = GrpcTransport::Tls(crate::grpc::GrpcTlsConfig::new(ca.clone()));
        let mats = RelayMaterials::from_connector_transport(&tls_transport, None);
        assert!(mats.tls.is_none(), "unparseable roots fail closed (no TLS connector)");
        let plain = RelayMaterials::from_connector_transport(&GrpcTransport::Plaintext, None);
        assert!(plain.tls.is_none(), "no root material → no TLS connector (fail-closed)");

        // and an EMPTY root set is refused at the connector itself
        let err = crate::relay_client::RustlsRelayTlsConnector::new(Vec::new()).unwrap_err();
        assert_eq!(err, crate::relay_client::RelayClientError::TlsRequired);
        let err = crate::relay_client::RustlsRelayTlsConnector::new(ca.clone()).unwrap_err();
        assert_eq!(err, crate::relay_client::RelayClientError::TlsRequired);
    }

    /// Fabricated (NOT secret) 32-byte signature, base64 — same fixture
    /// shape as tests/relay_client_e2e.rs.
    const RELAY_TEST_SIG_B64: &str = "paWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaU=";
    /// Fabricated virtual epoch for relay lifecycle tests.
    const RELAY_TEST_EPOCH: u64 = 1_770_000_000;

    fn relay_test_attach(clock: Arc<crate::relay_client::VirtualClock>) -> RelayAttach {
        RelayAttach {
            wg_pubkey_b64: "ZW1yZWxheS1jb25uZWN0b3ItbG9jYWwtd2drZXk=".to_string(),
            materials: Arc::new(RelayMaterials {
                clock,
                dialer: Arc::new(crate::relay_client::TcpDialer),
                tls: None, // rel:// loopback topology only (spec §1.1)
            }),
        }
    }

    fn relay_servers(urls: Vec<String>, expires_at_unix: u64) -> crate::network_map::RelayServers {
        crate::network_map::RelayServers {
            urls,
            token_payload: expires_at_unix.to_string(),
            token_signature: RELAY_TEST_SIG_B64.to_string(),
        }
    }

    /// Bounded real-time fuse for events that MUST happen (the relay client
    /// itself runs on the injected virtual clock — nothing here sleeps).
    async fn relay_wait(fuse: Duration, mut pred: impl FnMut() -> bool, what: &str) {
        let deadline = std::time::Instant::now() + fuse;
        loop {
            if pred() {
                return;
            }
            assert!(std::time::Instant::now() < deadline, "condition not met within fuse: {what}");
            tokio::task::yield_now().await;
        }
    }

    /// Full relay lifecycle through the REAL apply path: first sync starts
    /// the client (Ready over the fake loopback server), a second sync with
    /// the SAME urls only refreshes the token, expiry goes fail-closed and
    /// VISIBLE (token_valid:false + not ready), a fresh relay sync recovers
    /// it with ZERO real waiting (injected clock), and `stop_relay` leaves
    /// the terminal Dead state with the server observing the close.
    #[tokio::test(flavor = "multi_thread")]
    async fn relay_lifecycle_ready_refresh_expire_and_revoke() {
        use crate::relay_client::{RelayState, TokenValidity};
        use crate::relay_testserver::{RelayTestServer, TestServerConfig};

        let server = RelayTestServer::start(TestServerConfig::default()).await.expect("fake relay");
        let clock = Arc::new(crate::relay_client::VirtualClock::new(RELAY_TEST_EPOCH));
        let shared =
            Arc::new(ConnectorShared::new(false, Some(relay_test_attach(clock.clone()))));

        // before the first relay sync: enabled, idle, nothing dialed
        let st = shared.relay_status();
        assert!(st.enabled);
        assert_eq!(st.state, "disconnected");
        assert!(st.urls.is_empty());
        assert!(!st.token_valid);

        // first sync: near-expiry token (60 virtual seconds)
        let urls = vec![format!("rel://127.0.0.1:{}", server.addr().port())];
        shared.relay_apply(&relay_servers(urls.clone(), RELAY_TEST_EPOCH + 60));
        relay_wait(Duration::from_secs(5), || {
            shared.relay_status().state == "ready"
        }, "ready after first relay sync")
        .await;
        let st = shared.relay_status();
        assert!(st.token_valid, "token starts valid");
        assert_eq!(st.urls, urls);
        assert_eq!(st.last_error_class, None);
        assert_eq!(st.frames_tx, 1, "exactly the Auth frame so far");
        assert_eq!(st.frames_rx, 1, "exactly the AuthResponse so far");
        assert_eq!(st.transport_bytes, 0);
        assert_eq!(server.stats().connections_accepted, 1);

        // same-urls sync → token refresh ONLY (spec §6.1: no reconnect)
        shared.relay_apply(&relay_servers(urls.clone(), RELAY_TEST_EPOCH + 600));
        let st = shared.relay_status();
        assert!(st.token_valid);
        assert_eq!(server.stats().connections_accepted, 1, "no reconnect on token refresh");

        // expiry: the session is killed fail-closed; status makes it VISIBLE
        clock.advance(Duration::from_secs(700));
        relay_wait(Duration::from_secs(5), || {
            !shared.relay_status().token_valid
        }, "token_valid flips false at expiry")
        .await;
        relay_wait(Duration::from_secs(5), || {
            shared.relay_status().state == "reconnecting"
        }, "relay unusable while the token stays expired")
        .await;
        assert!(!shared.relay_ready(), "expired token ⇒ relay NOT ready (fail-closed)");
        assert_eq!(
            shared.relay_handle().expect("client present").token_validity(),
            TokenValidity::Expired { expired_for: Duration::from_secs(100) }
        );

        // new sync carries a fresh RelayServers → recovery, zero real wait
        shared.relay_apply(&relay_servers(urls.clone(), RELAY_TEST_EPOCH + 700 + 3_600));
        relay_wait(Duration::from_secs(5), || {
            shared.relay_status().state == "ready" && shared.relay_status().token_valid
        }, "relay recovered after the token refresh")
        .await;
        assert!(shared.relay_ready());
        assert_eq!(server.stats().frames_rx(crate::relay::MSG_CLOSE), 0);

        // revoke: stop_relay → terminal Dead + the server observed the close
        shared.stop_relay();
        relay_wait(Duration::from_secs(5), || {
            shared.relay_status().state == "dead"
        }, "relay state reaches Dead after teardown")
        .await;
        relay_wait(Duration::from_secs(5), || {
            server.stats().frames_rx(crate::relay::MSG_CLOSE) == 1
        }, "the server observed exactly one relay Close frame")
        .await;
        let accepted = server.stats().connections_accepted;
        assert_eq!(accepted, 2, "initial + post-refresh session, never more");
        assert!(!shared.relay_ready(), "Dead relay is not ready");
        assert_eq!(shared.relay_handle().unwrap().state(), RelayState::Dead);
        let dead = shared.relay_handle().expect("dead handle");
        shared.relay_apply(&relay_servers(urls, RELAY_TEST_EPOCH + 700 + 7_200));
        relay_wait(Duration::from_secs(5), || shared.relay_ready(),
            "same-URL sync replaces dead relay handle").await;
        assert!(!dead.same_client(&shared.relay_handle().expect("replacement")));
        assert_eq!(server.stats().connections_accepted, 3);
        shared.stop_relay();
        server.shutdown().await;
    }

    /// The default-route gate through the LIVE readiness path with relay:
    /// (a) relay Ready (+ WG session up, ICE not nominated) → allowed with
    /// the relay-ready reason token; (c) the same view WITHOUT a relay
    /// client stays HELD — ICE Failed is never silently reachable.
    #[tokio::test(flavor = "multi_thread")]
    async fn relay_ready_opens_the_default_route_gate_only_via_the_relay_path() {
        use crate::relay_testserver::{RelayTestServer, TestServerConfig};

        struct ReadyWg;
        impl WgPeerApplier for ReadyWg {
            fn apply_peers(&self, _: &[WgPeerEntry]) -> Result<(), String> {
                Ok(())
            }
            fn clear(&self) {}
            fn tunnel_ready(&self) -> bool {
                true
            }
        }

        let server = RelayTestServer::start(TestServerConfig::default()).await.expect("fake relay");
        let clock = Arc::new(crate::relay_client::VirtualClock::new(RELAY_TEST_EPOCH));
        let shared =
            Arc::new(ConnectorShared::new(false, Some(relay_test_attach(clock.clone()))));
        // an ICE orchestrator holding ONE peer that never connects:
        // peers=1, reachable=0 → the ICE view is NOT nominated.
        let ice = Arc::new(Mutex::new(PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![])),
            socks: Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source")),
            signal: Arc::new(crate::peer_conn::LoggingSignalExchange::default()),
            wg: Arc::new(ReadyWg),
            tie_breaker: Some(7),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        })));
        ice.lock_poison().set_peers(&["UEVFUjA=".to_string()]);
        let _ = shared.ice.set(ice.clone());

        // (c) WITHOUT relay: ICE not nominated + no relay → gate HELD
        let (allowed, reason) = shared.decide_default_route(false, &ReadyWg, false, 1);
        assert!(!allowed, "(c) ICE Failed without relay must hold the default route");
        assert_eq!(reason, "default-route-held:data-plane-not-ready", "{reason}");

        // bring the relay to Ready via the real apply path
        let urls = vec![format!("rel://127.0.0.1:{}", server.addr().port())];
        shared.relay_apply(&relay_servers(urls, RELAY_TEST_EPOCH + 3_600));
        relay_wait(Duration::from_secs(5), || {
            shared.relay_status().state == "ready"
        }, "relay Ready")
        .await;

        // (a) relay Ready + WG session up + ICE not nominated → allowed,
        // with the relay path NAMED in the reason token
        let (allowed, reason) = shared.decide_default_route(false, &ReadyWg, false, 1);
        assert!(allowed, "(a) relay Ready must enable the gate");
        assert_eq!(reason, DEFAULT_ROUTE_REASON_RELAY_READY, "{reason}");
        // ... and the force flag still wins over both (unchanged semantics)
        let (allowed, reason) = shared.decide_default_route(true, &ReadyWg, false, 1);
        assert!(allowed);
        assert_eq!(reason, "default-route-forced:debug-opt-in-black-hole-risk", "{reason}");
        // (b) ICE nominated (empty peers view) → the tunnel token,
        // relay-independent
        ice.lock_poison().set_peers(&[]);
        let (allowed, reason) = shared.decide_default_route(false, &ReadyWg, true, 1);
        assert!(allowed, "(b) ICE nominated must enable the gate");
        assert_eq!(reason, "default-route-allowed:peers-registered-and-tunnel-ready", "{reason}");

        // the disabled shape: status renders enabled:false and nothing else
        let plain = ConnectorShared::new(false, None);
        let st = plain.relay_status();
        assert!(!st.enabled);
        assert_eq!(st.state, "disabled");
        assert_eq!(st.to_json(), "{\"enabled\":false,\"state\":\"disabled\",\"urls\":[],\"reconnects\":0,\"frames_tx\":0,\"frames_rx\":0,\"transport_bytes\":0,\"token_valid\":false,\"last_error_class\":null}");
        assert!(matches!(config::parse_document(&st.to_json()), Ok(Json::Obj(_))));
        server.shutdown().await;
    }

    /// `rels://` with no usable trust root is refused typed BEFORE any wire
    /// byte: the relay client starts, never dials, and the status document
    /// exposes `tls-required` — with the loopback server observing ZERO
    /// connections (no plaintext downgrade, T0 Q3).
    #[tokio::test(flavor = "multi_thread")]
    async fn relay_rels_without_root_is_refused_and_never_downgrades() {
        use crate::relay_testserver::{RelayTestServer, TestServerConfig};

        let server = RelayTestServer::start(TestServerConfig::default()).await.expect("fake relay");
        let clock = Arc::new(crate::relay_client::VirtualClock::new(RELAY_TEST_EPOCH));
        // materials with tls: None == a plaintext management transport (no
        // ca_pem): exactly what the connector would build for rels:// here.
        let shared =
            Arc::new(ConnectorShared::new(false, Some(relay_test_attach(clock.clone()))));
        shared.relay_apply(&relay_servers(
            vec![format!("rels://127.0.0.1:{}", server.addr().port())],
            RELAY_TEST_EPOCH + 3_600,
        ));
        relay_wait(Duration::from_secs(5), || {
            let st = shared.relay_status();
            st.last_error_class == Some("tls-required".to_string()) && st.state == "reconnecting"
        }, "typed TlsRequired refusal surfaced in status, client held")
        .await;
        let st = shared.relay_status();
        assert_eq!(st.state, "reconnecting", "held, never downgraded");
        assert_eq!(st.urls, vec![format!("rels://127.0.0.1:{}", server.addr().port())]);
        relay_wait(Duration::from_secs(5), || {
            server.stats().connections_accepted == 0
        }, "the server must NEVER see a connection (no downgrade)")
        .await;
        // even long past any dial budget, nothing ever connects
        clock.advance(Duration::from_secs(600));
        relay_wait(Duration::from_secs(5), || {
            server.stats().connections_accepted == 0
        }, "still no connection after clock movement")
        .await;
        server.shutdown().await;
    }

    // -----------------------------------------------------------------------
    // loop-dependency fix: management-advertised relay urls ride the status
    // ALWAYS (also `relay_enabled:false`) — bookkeeping only, never a dial.
    // -----------------------------------------------------------------------

    /// Fabricated (NOT secret) relay sync carrying `RelayServers`; the token
    /// payload is a distinctive sentinel so the tests can pin that it NEVER
    /// reaches the status document.
    fn advertised_relay_config(urls: Vec<String>) -> crate::network_map::NetbirdServers {
        crate::network_map::NetbirdServers {
            relay: Some(crate::network_map::RelayServers {
                urls,
                token_payload: "4242424242".to_string(),
                token_signature: RELAY_TEST_SIG_B64.to_string(),
            }),
            ..Default::default()
        }
    }

    fn advertised_urls_update(
        relay: Option<crate::network_map::NetbirdServers>,
    ) -> SyncUpdate {
        SyncUpdate {
            session_deadline_unix: None,
            netbird_config: relay,
            network_map: None,
        }
    }

    /// `relay_enabled:false` (the hard default) + a sync advertising
    /// `relay.urls` → the status exposes the advertised set while the
    /// runtime section stays exactly "disabled/empty", and the token
    /// material never crosses the status boundary.
    #[test]
    fn relay_advertised_urls_exposed_while_relay_disabled() {
        let shared = ConnectorShared::new(false, None); // relay_enabled = false
        let wg = WgPeerRegistry::new();
        let urls = vec!["rels://a.example:28443".to_string()];
        shared.apply_update(
            &wg,
            &NoopHost,
            &advertised_urls_update(Some(advertised_relay_config(urls.clone()))),
        );

        let st = shared.relay_status();
        assert!(st.advertised, "management advertised relay");
        assert_eq!(st.advertised_urls, urls, "advertisement recorded while disabled");
        // runtime view UNCHANGED: not running, nothing configured, no counters
        assert!(!st.enabled, "relay stays disabled — advertisement is not usage");
        assert_eq!(st.state, "disabled");
        assert!(st.urls.is_empty(), "runtime url set stays empty while disabled");
        assert_eq!(st.frames_tx + st.frames_rx + st.transport_bytes, 0);
        assert!(!st.token_valid);
        assert!(st.last_error_class.is_none());
        assert!(shared.relay_handle().is_none(), "no relay client exists (nothing to dial with)");

        // the advertised view renders in its own section, the frozen runtime
        // object is byte-identical to the pre-change shape, and NO token
        // material appears anywhere
        let adv = st.advertised_to_json();
        assert_eq!(
            adv,
            "{\"advertised\":true,\"advertised_urls\":[\"rels://a.example:28443\"]}"
        );
        let runtime_json = st.to_json();
        assert_eq!(
            runtime_json,
            "{\"enabled\":false,\"state\":\"disabled\",\"urls\":[],\"reconnects\":0,\
             \"frames_tx\":0,\"frames_rx\":0,\"transport_bytes\":0,\"token_valid\":false,\
             \"last_error_class\":null}"
        );
        assert!(!runtime_json.contains("4242424242"), "token payload leaked: {runtime_json}");
        assert!(!runtime_json.contains(RELAY_TEST_SIG_B64), "token signature leaked");
        assert!(!adv.contains("4242424242"), "token payload leaked: {adv}");
    }

    /// The advertisement follows the LATEST sync (second sync replaces the
    /// set; management dropping relay clears it) — still with zero runtime
    /// effect while disabled.
    #[test]
    fn relay_advertised_urls_follow_latest_sync_and_clear_when_management_stops() {
        let shared = ConnectorShared::new(false, None);
        let wg = WgPeerRegistry::new();

        shared.apply_update(
            &wg,
            &NoopHost,
            &advertised_urls_update(Some(advertised_relay_config(vec![
                "rels://a.example:28443".into(),
            ]))),
        );
        assert_eq!(
            shared.relay_status().advertised_urls,
            vec!["rels://a.example:28443".to_string()]
        );

        // second sync carries a NEW url set → the record updates
        shared.apply_update(
            &wg,
            &NoopHost,
            &advertised_urls_update(Some(advertised_relay_config(vec![
                "rels://b.example:28443".into(),
                "rel://c.example:28443".into(),
            ]))),
        );
        let st = shared.relay_status();
        assert!(st.advertised);
        assert_eq!(
            st.advertised_urls,
            vec!["rels://b.example:28443".to_string(), "rel://c.example:28443".to_string()]
        );
        assert!(!st.enabled, "updating the advertisement still never enables relay");
        assert_eq!(st.state, "disabled");
        assert!(shared.relay_handle().is_none());

        // management stops advertising (relay absent from the sync config)
        // → the record clears: the status honestly says "not advertised"
        shared.apply_update(&wg, &NoopHost, &advertised_urls_update(Some(Default::default())));
        let st = shared.relay_status();
        assert!(!st.advertised);
        assert!(st.advertised_urls.is_empty());
        assert_eq!(
            st.advertised_to_json(),
            "{\"advertised\":false,\"advertised_urls\":[]}"
        );
        assert_eq!(st.state, "disabled", "disabled semantics unchanged");
    }

    /// WG over relay 修复的注入点断言：sync 广告 relay 时，signal seam 的
    /// exchange 拿到**第一个**广告 URL；管理端撤下（sync 无 relay）→
    /// 清除。slot → OFFER body 的注入由 peer_conn 的 seam 测试钉住，
    /// 两侧合并即 sync → offer body 的完整链路。全程 `relay_enabled:
    /// false`：镜像只是 bookkeeping，绝不 dial。
    #[tokio::test]
    async fn sync_mirrors_first_advertised_relay_url_into_signal_seam() {
        let shared = Arc::new(ConnectorShared::new(false, None));
        let (exchange, rx) = RealSignalExchange::new();
        let rt = Arc::new(SignalRuntime::new(
            SignalLinkMaterials {
                runtime: tokio::runtime::Handle::current(),
                transport: GrpcTransport::Plaintext,
                connect_timeout: Duration::from_secs(1),
                request_timeout: Duration::from_secs(1),
                keys: EnvelopeKeyPair::from_secret_bytes(&[7u8; 32]),
                sockets: Arc::new(ProtectedSocketFdSource::new_with_fd(-1).expect("empty tcp source")),
            },
            exchange.clone(),
            rx,
        ));
        let _ = shared.signal.set(rt);
        let wg = WgPeerRegistry::new();

        shared.apply_update(
            &wg,
            &NoopHost,
            &advertised_urls_update(Some(advertised_relay_config(vec![
                "rels://a.example:28443".into(),
                "rel://b.example:28443".into(),
            ]))),
        );
        assert_eq!(
            exchange.relay_address().as_deref(),
            Some("rels://a.example:28443"),
            "multi-URL advertisement → the FIRST url rides the signal seam"
        );

        // management stops advertising (relay absent from the sync config)
        // → the seam clears: offers go back to the field-free shape
        shared.apply_update(&wg, &NoopHost, &advertised_urls_update(Some(Default::default())));
        assert_eq!(
            exchange.relay_address(),
            None,
            "no advertisement → field-free offer bodies (pre-fix behavior)"
        );
    }

    /// The enabled-but-idle default shape: no sync yet → nothing advertised,
    /// runtime idle (documents the field defaults next to the runtime ones).
    #[test]
    fn relay_advertised_urls_default_to_empty_before_any_sync() {
        let shared = ConnectorShared::new(false, None);
        let st = shared.relay_status();
        assert!(!st.advertised);
        assert!(st.advertised_urls.is_empty());
        assert!(!st.enabled);
        assert_eq!(st.state, "disabled");

        // and the document renders both sections in the fixed order:
        // relay_advertised BEFORE the frozen relay object
        let status = ConnectorStatus {
            running: false,
            state: ConnState::Disconnected,
            started_at_unix: None,
            last_update_unix: None,
            peer_count: 0,
            route_count: 0,
            reconnects: 0,
            last_error: None,
            deadline: SessionDeadline::Unknown,
            renew_attempts: 0,
            wg_apply_failed: false,
            wg_apply_errors: 0,
            logout_ok: None,
            terminal: false,
            ice: IceOrchestratorSummary::default(),
            signal: SignalLinkStatus::default(),
            wg: crate::wg_device::WgDataplaneStatus::default(),
            recreate: RecreateStatus::default(),
            relay: st,
        };
        let json = status.to_json();
        let adv_pos = json.find("\"relay_advertised\":").expect("advertised section");
        let relay_pos = json.find("\"relay\":{").expect("relay section");
        assert!(adv_pos < relay_pos, "advertised section must precede the runtime section");
        assert!(
            json.contains(
                "\"relay_advertised\":{\"advertised\":false,\"advertised_urls\":[]}"
            ),
            "{json}"
        );
        assert!(
            json.contains(
                "\"relay\":{\"enabled\":false,\"state\":\"disabled\",\"urls\":[],\
                 \"reconnects\":0,\"frames_tx\":0,\"frames_rx\":0,\"transport_bytes\":0,\
                 \"token_valid\":false,\"last_error_class\":null}"
            ),
            "frozen relay object must stay byte-identical: {json}"
        );
        assert!(matches!(config::parse_document(&json), Ok(Json::Obj(_))), "{json}");
    }

    // -------------------------------------------------------------------
    // 0018 — relay pump starvation behind pending lane opens
    // -------------------------------------------------------------------
    //
    // These tests drive the REAL production pump (`relay_carrier_pump` via
    // `ensure_pump`) over a REAL `RelayClient` against the in-process fake
    // relay server (`crate::relay_testserver`) — no re-implemented pump.
    // The injected `VirtualClock` freezes the §4.1 open budget so a pending
    // lane open is deterministic; all other time is real loopback IO.

    use crate::relay::PeerId;
    use crate::relay_client::{OPEN_CONN_TIMEOUT, RelayState};
    use crate::relay_testserver::{RelayTestServer, TestServerConfig};
    use std::time::Instant;

    /// Carrier-capable recording WG seam: attaches/detaches and carrier
    /// inbound are recorded for assertions.
    #[derive(Default)]
    struct CarrierProbeWg {
        attached: Mutex<Vec<String>>,
        detached: Mutex<Vec<String>>,
        inbound: Mutex<Vec<(String, Vec<u8>)>>,
    }

    impl WgPeerApplier for CarrierProbeWg {
        fn apply_peers(&self, _: &[WgPeerEntry]) -> Result<(), String> {
            Ok(())
        }
        fn clear(&self) {}
        fn carrier_capable(&self) -> bool {
            true
        }
        fn attach_carrier(
            &self,
            pub_key_b64: &str,
            _carrier: std::sync::Arc<dyn crate::wg_device::WgEgressCarrier>,
        ) -> Result<(), String> {
            self.attached
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(pub_key_b64.to_string());
            Ok(())
        }
        fn detach_carrier(&self, pub_key_b64: &str) {
            self.detached
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(pub_key_b64.to_string());
        }
        fn handle_carrier_inbound(&self, datagram: &[u8], pub_key_b64: &str, _now_ms: u64) -> usize {
            self.inbound
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((pub_key_b64.to_string(), datagram.to_vec()));
            0
        }
    }

    /// A well-shaped WG public-key stand-in (the relay path only ever hashes
    /// the base64 STRING — no key validation exists below this seam).
    fn relay_lane_key(byte: u8) -> String {
        base64::engine::general_purpose::STANDARD.encode([byte; 32])
    }

    /// Fabricated but well-shaped relay token: payload = expiry unix second.
    fn relay_fabricated_token(expires_at_unix: u64) -> crate::relay::AuthToken {
        crate::relay::AuthToken::from_management(
            &expires_at_unix.to_string(),
            &base64::engine::general_purpose::STANDARD.encode([7u8; 32]),
        )
        .expect("fabricated token is well-shaped")
    }

    /// Bounded REAL-time wait (client/pump run on real loopback IO; only the
    /// §4.1 open budget rides the frozen virtual clock).
    async fn relay_wait_until(fuse: Duration, mut pred: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + fuse;
        loop {
            // DEADLINE FIRST: the fuse is authoritative. A run that is
            // starved past the deadline must surface as an honest timeout
            // (the caller's assert message), never as a late "pred finally
            // true" pass that silently bypassed the fuse.
            if Instant::now() >= deadline {
                return false;
            }
            if pred() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Bounded real-time negative window: `true` = pred stayed false.
    /// PRED FIRST: at least one observation is guaranteed even when the
    /// task is starved past the whole window (a deadline-first loop would
    /// skip the check on wake and silently PASS without ever looking).
    async fn relay_stays_false_for(window: Duration, mut pred: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + window;
        loop {
            if pred() {
                return false;
            }
            if Instant::now() >= deadline {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// A raw remote endpoint on the fake relay server (plays the remote
    /// node's relay session: auth, then Transport sender).
    async fn relay_remote(
        server: &RelayTestServer,
        id: &PeerId,
        token: &crate::relay::AuthToken,
    ) -> crate::ws::WsClient<tokio::net::TcpStream> {
        let tcp = tokio::net::TcpStream::connect(server.addr()).await.expect("remote dials");
        let key = crate::ws::random_sec_websocket_key().expect("random key");
        let mut ws = crate::ws::client_handshake(tcp, &server.addr().to_string(), "/relay", &key)
            .await
            .expect("remote ws handshake");
        ws.write_binary(
            &crate::relay::Frame::Auth { peer_id: id.clone(), token: token.clone() }
                .encode()
                .expect("auth frame"),
        )
        .await
        .expect("remote auth send");
        match ws.read_message().await.expect("remote auth response") {
            crate::ws::WsMessage::Binary(bytes) => match crate::relay::Frame::decode(&bytes) {
                Ok(crate::relay::Frame::AuthResponse { .. }) => ws,
                other => panic!("remote must be authenticated first, got {other:?}"),
            },
            other => panic!("expected auth response binary, got {other:?}"),
        }
    }

    async fn relay_starvation_fixture(
    ) -> (
        RelayTestServer,
        std::sync::Arc<crate::relay_client::VirtualClock>,
        std::sync::Arc<RelayCarrier>,
        std::sync::Arc<CarrierProbeWg>,
        crate::relay_client::RelayClient,
    ) {
        let server = RelayTestServer::start(TestServerConfig::default())
            .await
            .expect("fake relay server");
        let unix_base: u64 = 1_800_000_000;
        let token = relay_fabricated_token(unix_base + 3_600);
        let clock = std::sync::Arc::new(crate::relay_client::VirtualClock::new(unix_base));
        let pub_self = relay_lane_key(0xC3);
        let client = crate::relay_client::RelayClient::start(
            crate::relay_client::RelayClientConfig::new(
                &[format!("rel://127.0.0.1:{}", server.addr().port())],
                &pub_self,
                token,
            )
            .expect("relay config")
            .with_clock(clock.clone())
            .with_dialer(Arc::new(crate::relay_client::TcpDialer)),
        )
        .expect("relay client starts");
        assert!(
            relay_wait_until(Duration::from_secs(5), || {
                client.state() == RelayState::Ready
            })
            .await,
            "relay client must become Ready against the fake server"
        );
        let wg = std::sync::Arc::new(CarrierProbeWg::default());
        let carrier = RelayCarrier::new(wg.clone() as std::sync::Arc<dyn WgPeerApplier>);
        // Production arming order (relay_carrier_client_started): arm the
        // carrier with the client, THEN spawn the pump.
        carrier.attach_client(client.clone());
        (server, clock, carrier, wg, client)
    }

    /// THE 0018 REGRESSION: an already-connected peer's inbound frame must be
    /// ingested in bounded real time (far below the 30s §4.1 window) even
    /// while ANOTHER peer's lane open is pending. The pre-fix pump ran
    /// `ensure_lanes` sequentially to completion BEFORE pumping, so the
    /// pending open parked the whole pump for the open budget per pass.
    #[tokio::test(flavor = "multi_thread")]
    async fn relay_pump_ingests_inbound_while_another_lane_open_is_still_pending() {
        let (server, clock, carrier, wg, client) = relay_starvation_fixture().await;

        let pub_online = relay_lane_key(0xA1);
        let pub_silent = relay_lane_key(0xB2);
        let pub_self = relay_lane_key(0xC3);
        let pid_online = PeerId::from_wg_pubkey_string(&pub_online);
        let pid_self = PeerId::from_wg_pubkey_string(&pub_self);

        // The ONLINE peer authenticates BEFORE the client subscribes: its
        // open_conn resolves immediately. The SILENT peer never shows up: its
        // open_conn parks until the (frozen) injected clock crosses the §4.1
        // budget — a deterministic stand-in for 0018's never-online peers.
        let mut online =
            relay_remote(&server, &pid_online, &relay_fabricated_token(1_800_003_600)).await;

        // Lane universe ORDER mirrors 0018: a connected peer listed before a
        // dead one. Production wiring: the pump task via `ensure_pump`.
        carrier.set_peers(&[pub_online.clone(), pub_silent.clone()]);
        carrier.ensure_pump(&tokio::runtime::Handle::current());

        assert!(
            relay_wait_until(Duration::from_secs(5), || carrier.stats().lanes_open_ok == 1).await,
            "the online peer's lane must attach (stats={:?})",
            carrier.stats(),
        );

        // Inbound from the online peer while the silent peer's open parks.
        online
            .write_binary(
                &crate::relay::Frame::Transport {
                    peer_id: pid_self.clone(),
                    payload: vec![1, 2, 3, 4],
                }
                .encode()
                .expect("frame encodes"),
            )
            .await
            .expect("online peer sends transport");

        // The frame REACHED the client (its reader is independent of the pump)
        // — the starvation under test is between the client queue and WG.
        assert!(
            relay_wait_until(Duration::from_secs(2), || client.stats().transport_rx_bytes >= 4)
                .await,
            "fixture broken: the frame never arrived at the relay client"
        );

        // THE ASSERTION: ingest in bounded real time, DURING the pending open.
        assert!(
            relay_wait_until(Duration::from_secs(2), || carrier.stats().frames_in == 1).await,
            "inbound frame starved behind a pending lane open (stats={:?})",
            carrier.stats(),
        );
        {
            let inbound = wg.inbound.lock().unwrap_or_else(PoisonError::into_inner);
            assert_eq!(inbound.len(), 1, "exactly one frame must reach the WG seam");
            assert_eq!(inbound[0].0, pub_online, "frame keyed by the sender's WG pubkey");
            assert_eq!(inbound[0].1, vec![1, 2, 3, 4]);
        }
        assert_eq!(
            carrier.stats().lanes_open_failed, 0,
            "the silent open must STILL be pending — this proved ingestion DURING the wait (stats={:?})",
            carrier.stats(),
        );

        // Unblock the parked open the deterministic way: virtual time crosses
        // the §4.1 budget → typed timeout → backoff; nothing else breaks.
        clock.advance(OPEN_CONN_TIMEOUT + Duration::from_secs(1));
        assert!(
            relay_wait_until(Duration::from_secs(5), || carrier.stats().lanes_open_failed == 1)
                .await,
            "the parked open must time out once virtual time crosses its budget (stats={:?})",
            carrier.stats(),
        );
        assert_eq!(carrier.stats().last_open_error, Some("open-conn-timeout"));

        // The pump survived the timeout: the next frame still flows bounded.
        online
            .write_binary(
                &crate::relay::Frame::Transport {
                    peer_id: pid_self.clone(),
                    payload: vec![9, 9],
                }
                .encode()
                .expect("frame encodes"),
            )
            .await
            .expect("online peer sends transport");
        assert!(
            relay_wait_until(Duration::from_secs(2), || carrier.stats().frames_in == 2).await,
            "pump must keep ingesting after a lane-open timeout (stats={:?})",
            carrier.stats(),
        );

        // Unknown sender end-to-end: counted, NEVER fed to the WG seam.
        let pid_stranger = PeerId::from_wg_pubkey_string(&relay_lane_key(0xDD));
        let mut stranger =
            relay_remote(&server, &pid_stranger, &relay_fabricated_token(1_800_003_600)).await;
        stranger
            .write_binary(
                &crate::relay::Frame::Transport {
                    peer_id: pid_self.clone(),
                    payload: vec![7u8; 16],
                }
                .encode()
                .expect("frame encodes"),
            )
            .await
            .expect("stranger sends transport");
        assert!(
            relay_wait_until(Duration::from_secs(2), || carrier.stats().frames_in_unknown == 1)
                .await,
            "unknown sender must be counted (stats={:?})",
            carrier.stats(),
        );
        assert_eq!(
            wg.inbound.lock().unwrap_or_else(PoisonError::into_inner).len(),
            2,
            "unknown sender must NOT reach the WG seam (only the two online-peer frames may)"
        );
        assert!(
            wg.inbound
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .all(|(k, _)| *k == pub_online),
            "every fed frame must be keyed by the online peer"
        );

        carrier.shutdown();
    }

    /// The pending lane open must END on peer removal and on connector stop:
    /// no late attach for a removed peer, no resurrected lane after teardown.
    #[tokio::test(flavor = "multi_thread")]
    async fn relay_pump_pending_lane_open_ends_on_peer_removal_and_on_stop() {
        let (server, _clock, carrier, wg, _client) = relay_starvation_fixture().await;

        let pub_silent = relay_lane_key(0xB2);
        let pid_silent = PeerId::from_wg_pubkey_string(&pub_silent);

        carrier.set_peers(&[pub_silent.clone()]);
        carrier.ensure_pump(&tokio::runtime::Handle::current());

        // The open actually went out (server saw the subscribe) and parks.
        assert!(
            relay_wait_until(Duration::from_secs(5), || {
                server.stats().frames_rx(crate::relay::MSG_SUBSCRIBE_PEER_STATE) >= 1
            })
            .await,
            "the pending open must reach the server as a SubscribePeerState"
        );
        assert_eq!(carrier.stats().lanes_open_ok, 0);

        // Peer removal while the open is pending … then the peer comes
        // online: a surviving open would attach a lane for a REMOVED peer.
        carrier.set_peers(&[]);
        assert!(
            server.set_peer_online(&pid_silent, true),
            "came-online must fire to the registered subscriber"
        );
        assert!(
            relay_stays_false_for(Duration::from_millis(700), || {
                carrier.stats().lanes_open_ok > 0
                    || carrier.lane_count() > 0
                    || !wg.attached.lock().unwrap_or_else(PoisonError::into_inner).is_empty()
            })
            .await,
            "a removed peer's pending open must not attach after removal"
        );
        assert_eq!(carrier.lane_count(), 0);

        // Connector-stop shape: went-offline first (the §4.1 came-online
        // interest is one-shot in faithful mode), a fresh open goes out for
        // the re-added peer, then teardown aborts it — the peer coming
        // online afterwards must never produce a late attach.
        assert!(server.set_peer_online(&pid_silent, false), "went-offline must fire");
        carrier.set_peers(&[pub_silent.clone()]);
        assert!(
            relay_wait_until(Duration::from_secs(5), || {
                server.stats().frames_rx(crate::relay::MSG_SUBSCRIBE_PEER_STATE) >= 2
            })
            .await,
            "a fresh open must go out for the re-added peer"
        );
        carrier.shutdown();
        assert!(server.set_peer_online(&pid_silent, true), "came-online must fire again");
        assert!(
            relay_stays_false_for(Duration::from_millis(700), || {
                carrier.stats().lanes_open_ok > 0
                    || carrier.lane_count() > 0
                    || !wg.attached.lock().unwrap_or_else(PoisonError::into_inner).is_empty()
            })
            .await,
            "a stopped connector's pending open must not attach after teardown"
        );
        assert_eq!(carrier.lane_count(), 0);
    }

    // -------------------------------------------------------------------
    // 0018 follow-up (T0 review) — lane-open COMMIT atomicity
    // -------------------------------------------------------------------
    //
    // The open task's COMMIT (validity re-check + WG attach + attached-table
    // insert) must be mutually exclusive with every invalidating transition
    // (shutdown / client swap / peer removal). These tests park the commit
    // INSIDE the probe seam's attach_carrier and interleave a concurrent
    // teardown from another thread — the exact "passed the await, mid-commit"
    // window an abort cannot cancel.

    /// Probe seam whose `attach_carrier` parks on a gate: models "open
    /// resolved, check passed, attach in flight". `active` is the
    /// attach−detach BALANCE — the attach history alone would mislead
    /// (attached-then-detached is not residue).
    #[derive(Default)]
    struct GatedCommitWg {
        /// Handshake: set (and signalled) BEFORE the commit parks, so the
        /// test waits for "parked at the gate" — a verified premise, not an
        /// inference from "attach entered".
        at_gate: AtomicBool,
        /// Set when the commit is about to RETURN from attach_carrier
        /// (gate released, or its own long fuse expired): past this point
        /// "the commit is still parked" is no longer a valid premise.
        commit_returned: AtomicBool,
        gate: std::sync::Mutex<bool>,
        gate_cv: std::sync::Condvar,
        active: std::sync::Mutex<std::collections::HashSet<String>>,
    }

    impl GatedCommitWg {
        fn release_gate(&self) {
            let mut held = self.gate.lock().unwrap();
            *held = true;
            self.gate_cv.notify_all();
        }

        /// Premise check for the negative windows: the teardown race is
        /// only under test while the commit is genuinely parked here.
        fn commit_still_parked(&self) -> bool {
            !self.commit_returned.load(Ordering::SeqCst)
        }
    }

    impl WgPeerApplier for GatedCommitWg {
        fn apply_peers(&self, _: &[WgPeerEntry]) -> Result<(), String> {
            Ok(())
        }
        fn clear(&self) {}
        fn carrier_capable(&self) -> bool {
            true
        }
        fn attach_carrier(
            &self,
            pub_key_b64: &str,
            _carrier: std::sync::Arc<dyn crate::wg_device::WgEgressCarrier>,
        ) -> Result<(), String> {
            // Signal BEFORE parking: the test may proceed as soon as the
            // commit is verifiably at the gate.
            self.at_gate.store(true, Ordering::SeqCst);
            self.gate_cv.notify_all();
            // Park until the test releases the gate (bounded so a broken
            // test cannot hang the suite). The fuse is ~100x the 300ms
            // negative windows so it must never be the event that ends a
            // healthy window; if it ever fires first, the tests report
            // premise-broken instead of a bogus mutex failure.
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut held = self.gate.lock().unwrap();
            while !*held && Instant::now() < deadline {
                let (guard, _) =
                    self.gate_cv.wait_timeout(held, Duration::from_millis(100)).unwrap();
                held = guard;
            }
            self.active.lock().unwrap().insert(pub_key_b64.to_string());
            // The commit is leaving the park: invalidate the "still parked"
            // premise from here on.
            self.commit_returned.store(true, Ordering::SeqCst);
            Ok(())
        }
        fn detach_carrier(&self, pub_key_b64: &str) {
            self.active.lock().unwrap().remove(pub_key_b64);
        }
    }

    async fn relay_commit_race_fixture(
    ) -> (
        RelayTestServer,
        std::sync::Arc<GatedCommitWg>,
        std::sync::Arc<RelayCarrier>,
        crate::relay_client::RelayClient,
    ) {
        let server = RelayTestServer::start(TestServerConfig::default())
            .await
            .expect("fake relay server");
        let unix_base: u64 = 1_800_000_000;
        let token = relay_fabricated_token(unix_base + 3_600);
        let clock = std::sync::Arc::new(crate::relay_client::VirtualClock::new(unix_base));
        let pub_self = relay_lane_key(0xC3);
        let client = crate::relay_client::RelayClient::start(
            crate::relay_client::RelayClientConfig::new(
                &[format!("rel://127.0.0.1:{}", server.addr().port())],
                &pub_self,
                token,
            )
            .expect("relay config")
            .with_clock(clock.clone())
            .with_dialer(Arc::new(crate::relay_client::TcpDialer)),
        )
        .expect("relay client starts");
        assert!(
            relay_wait_until(Duration::from_secs(5), || {
                client.state() == RelayState::Ready
            })
            .await,
            "relay client must become Ready against the fake server"
        );
        let wg = std::sync::Arc::new(GatedCommitWg::default());
        let carrier = RelayCarrier::new(wg.clone() as std::sync::Arc<dyn WgPeerApplier>);
        (server, wg, carrier, client)
    }

    /// Drive one silent peer's lane open into the commit so it parks inside
    /// the probe seam's attach_carrier. Returns once came-online fired; the
    /// "commit is verifiably PARKED at the gate" premise is established by
    /// [`relay_parked_commit_interleave`]'s handshake, not inferred here.
    async fn relay_park_commit_mid_attach(
        server: &RelayTestServer,
        _wg: &GatedCommitWg,
        carrier: &std::sync::Arc<RelayCarrier>,
        client: &crate::relay_client::RelayClient,
    ) {
        let pub_silent = relay_lane_key(0xB2);
        let pid_silent = PeerId::from_wg_pubkey_string(&pub_silent);
        carrier.set_peers(&[pub_silent]);
        carrier.attach_client(client.clone());
        carrier.ensure_pump(&tokio::runtime::Handle::current());

        // The open goes out; the peer comes online; open_conn resolves Ok;
        // the commit parks inside the probe seam's attach_carrier.
        assert!(
            relay_wait_until(Duration::from_secs(5), || {
                server.stats().frames_rx(crate::relay::MSG_SUBSCRIBE_PEER_STATE) >= 1
            })
            .await,
            "the open must reach the server as a SubscribePeerState"
        );
        assert!(
            server.set_peer_online(&pid_silent, true),
            "came-online must fire to the registered subscriber"
        );
    }

    /// Drive the teardown-vs-parked-commit interleaving on a REAL OS thread,
    /// NOT the tokio runtime. Every wait below is an Instant-deadline poll:
    /// the parked commit's worker thread blocks in the gate condvar and can
    /// pin the runtime's timer driver, so a runtime-side wait here would
    /// stall with it (observed: a 20ms poll left unscheduled for the full
    /// 30s gate fuse) and turn the gate fuse into the release event. Kernel
    /// threads, mutexes and condvars are immune to that stall.
    ///
    /// Returns `Err` = premise-broken or honest timeout; `Ok` carries
    /// (pred stayed false through the negative window, teardown completed).
    fn relay_parked_commit_interleave(
        wg: std::sync::Arc<GatedCommitWg>,
        teardown: impl FnOnce() + Send + 'static,
        done: std::sync::Arc<AtomicBool>,
    ) -> Result<(bool, bool), String> {
        // 1. Handshake: wait until the commit signals it is AT the gate.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !wg.at_gate.load(Ordering::SeqCst) {
            if Instant::now() >= deadline {
                return Err(
                    "premise not established: the lane-open commit never reached the \
                     attach gate (honest timeout)"
                        .into(),
                );
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        // 2. Premise: the commit must still be parked (not blown through).
        if !wg.commit_still_parked() {
            return Err(
                "premise broken: the commit already RETURNED from the attach gate \
                 before the test could interleave — mutual exclusion is not under \
                 test in this run"
                    .into(),
            );
        }
        // 3. Teardown contender: blocks on the lifecycle mutex the parked
        // commit holds.
        std::thread::spawn(teardown);
        // 4. Negative window (300ms ≪ 30s gate fuse, so the fuse can never
        // be the release event of a healthy window). Pred-first: a starved
        // poll still observes at least once.
        let deadline = Instant::now() + Duration::from_millis(300);
        let mut stayed = true;
        loop {
            if done.load(Ordering::SeqCst) {
                stayed = false;
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if !stayed && !wg.commit_still_parked() {
            return Err(
                "premise broken: the commit un-parked (gate fuse) during the \
                 negative window, releasing the lifecycle lock — the teardown \
                 completion is the documented fuse release, not a mutex failure"
                    .into(),
            );
        }
        // 5. Release the gate; the serialized teardown must now complete.
        wg.release_gate();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if done.load(Ordering::SeqCst) {
                return Ok((stayed, true));
            }
            if Instant::now() >= deadline {
                return Ok((stayed, false));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// The commit is mid-attach when shutdown runs: teardown must serialize
    /// against the commit — no lane may survive, no unmanaged pending open.
    #[tokio::test(flavor = "multi_thread")]
    async fn relay_lane_open_commit_is_atomic_against_shutdown() {
        let (server, wg, carrier, client) = relay_commit_race_fixture().await;
        relay_park_commit_mid_attach(&server, &wg, &carrier, &client).await;

        let stop_done = Arc::new(AtomicBool::new(false));
        let stop_flag = stop_done.clone();
        let carrier_for_stop = carrier.clone();
        let wg_for_interleave = wg.clone();
        // The whole interleaving (handshake → premise → negative window →
        // gate release → completion wait) runs on a real thread with
        // Instant-deadline polls; see relay_parked_commit_interleave.
        let (stayed, completed) = relay_parked_commit_interleave(
            wg_for_interleave,
            move || {
                carrier_for_stop.shutdown();
                stop_flag.store(true, Ordering::SeqCst);
            },
            stop_done,
        )
        .unwrap_or_else(|msg| panic!("{msg}"));
        assert!(
            stayed,
            "shutdown completed while a lane-open commit was mid-attach (no mutual exclusion)"
        );
        assert!(
            completed,
            "shutdown must complete once the commit gate is released (honest timeout)"
        );
        assert!(
            wg.active.lock().unwrap().is_empty(),
            "a late lane-open commit must not resurrect a lane past shutdown"
        );
        assert_eq!(carrier.lane_count(), 0, "no lane may survive shutdown");
        assert!(
            carrier.pending_opens.lock().unwrap_or_else(PoisonError::into_inner).is_empty(),
            "no unmanaged pending open may survive shutdown"
        );
    }

    /// Same race against a client swap: the old client's mid-commit open
    /// must never land a lane on the swapped-in carrier.
    #[tokio::test(flavor = "multi_thread")]
    async fn relay_lane_open_commit_is_atomic_against_client_swap() {
        let (server, wg, carrier, client) = relay_commit_race_fixture().await;
        relay_park_commit_mid_attach(&server, &wg, &carrier, &client).await;

        let pub_self2 = relay_lane_key(0xC4);
        let clock2 = Arc::new(crate::relay_client::VirtualClock::new(1_800_000_000));
        let client2 = crate::relay_client::RelayClient::start(
            crate::relay_client::RelayClientConfig::new(
                &[format!("rel://127.0.0.1:{}", server.addr().port())],
                &pub_self2,
                relay_fabricated_token(1_800_003_600),
            )
            .expect("relay config 2")
            .with_clock(clock2)
            .with_dialer(Arc::new(crate::relay_client::TcpDialer)),
        )
        .expect("second relay client starts");

        let swap_done = Arc::new(AtomicBool::new(false));
        let swap_flag = swap_done.clone();
        let carrier_for_swap = carrier.clone();
        let client2_for_swap = client2.clone();
        let wg_for_interleave = wg.clone();
        // Same real-thread interleaving contract as the shutdown twin above.
        let (stayed, completed) = relay_parked_commit_interleave(
            wg_for_interleave,
            move || {
                carrier_for_swap.attach_client(client2_for_swap);
                swap_flag.store(true, Ordering::SeqCst);
            },
            swap_done,
        )
        .unwrap_or_else(|msg| panic!("{msg}"));
        assert!(
            stayed,
            "client swap completed while a lane-open commit was mid-attach (no mutual exclusion)"
        );
        assert!(
            completed,
            "client swap must complete once the commit gate is released (honest timeout)"
        );
        assert!(
            wg.active.lock().unwrap().is_empty(),
            "the old client's late commit must not leave a lane behind"
        );
        assert_eq!(carrier.lane_count(), 0);
        assert!(
            carrier.pending_opens.lock().unwrap_or_else(PoisonError::into_inner).is_empty(),
            "no unmanaged pending open may survive a client swap"
        );
        assert!(carrier.has_client(&client2), "the carrier must serve the new client");
        assert!(!carrier.has_client(&client), "the old client must be disarmed");
        carrier.shutdown();
    }

    /// Generational pending-open bookkeeping (T0-0018 follow-up): an old
    /// task's late unregister must never evict a NEWER registration of the
    /// same key — that would orphan the new task from abort/shutdown
    /// bookkeeping and break per-peer single-flight.
    #[tokio::test]
    async fn pending_open_unregister_is_generational() {
        let carrier = RelayCarrier::new(
            Arc::new(CarrierProbeWg::default()) as std::sync::Arc<dyn WgPeerApplier>
        );
        // Two registered "opens" for the same key: gen 1 (an old task that
        // got drained and is finishing late) and gen 2 (the current task).
        let h1 = tokio::spawn(std::future::pending::<()>());
        let h2 = tokio::spawn(std::future::pending::<()>());
        {
            let mut pending = carrier.pending_opens.lock().unwrap_or_else(PoisonError::into_inner);
            pending.insert("k".to_string(), (1, h1));
            pending.insert("k".to_string(), (2, h2));
        }
        // The OLD task finishes late: its unregister must NOT evict gen 2.
        carrier.finish_open("k", 1);
        assert_eq!(
            carrier
                .pending_opens
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get("k")
                .map(|(g, _)| *g),
            Some(2),
            "a stale generation must not unregister the newer pending open"
        );
        // The newer entry is still managed: shutdown drains and aborts it.
        carrier.shutdown();
        assert!(
            carrier.pending_opens.lock().unwrap_or_else(PoisonError::into_inner).is_empty(),
            "shutdown must drain the current pending open"
        );
        // And the matching generation still unregisters its own entry.
        let h3 = tokio::spawn(std::future::pending::<()>());
        {
            let mut pending = carrier.pending_opens.lock().unwrap_or_else(PoisonError::into_inner);
            pending.insert("k".to_string(), (3, h3));
        }
        carrier.finish_open("k", 3);
        assert!(
            carrier.pending_opens.lock().unwrap_or_else(PoisonError::into_inner).is_empty(),
            "a matching generation must unregister its own entry"
        );
    }

    /// Registration atomicity + lifecycle gate (T0 residual): repeated spawn
    /// passes for the same key register exactly ONE pending open — the second
    /// pass sees it vacant-checked under the same lock and neither duplicates
    /// nor overwrites the generation — and a pass after shutdown registers
    /// nothing (unarmed gate).
    #[tokio::test(flavor = "multi_thread")]
    async fn pending_open_registration_is_atomic_and_gated() {
        let (_server, _clock, carrier, _wg, client) = relay_starvation_fixture().await;
        let pub_silent = relay_lane_key(0xB2);

        // The fixture arms the carrier; disarm it to exercise the gate.
        carrier.shutdown();

        // Unarmed: a pass must register nothing.
        carrier.set_peers(&[pub_silent.clone()]);
        carrier.spawn_pending_lanes();
        assert!(
            carrier.pending_opens.lock().unwrap_or_else(PoisonError::into_inner).is_empty(),
            "an unarmed carrier must not register lane opens"
        );

        // Armed: two consecutive passes for the same key — one registration,
        // stable generation (no duplicate, no overwrite).
        carrier.attach_client(client.clone());
        carrier.spawn_pending_lanes();
        let gen_after_first = {
            let pending = carrier.pending_opens.lock().unwrap_or_else(PoisonError::into_inner);
            assert_eq!(pending.len(), 1, "the first pass must register exactly one open");
            pending.values().next().map(|(g, _)| *g)
        };
        carrier.spawn_pending_lanes();
        {
            let pending = carrier.pending_opens.lock().unwrap_or_else(PoisonError::into_inner);
            assert_eq!(
                pending.len(),
                1,
                "a second pass must not register a duplicate pending open"
            );
            assert_eq!(
                pending.values().next().map(|(g, _)| *g),
                gen_after_first,
                "a second pass must not overwrite the registered generation"
            );
        }

        // Teardown drains the registration; a post-shutdown pass stays empty.
        carrier.shutdown();
        assert!(
            carrier.pending_opens.lock().unwrap_or_else(PoisonError::into_inner).is_empty(),
            "shutdown must drain the pending open"
        );
        carrier.spawn_pending_lanes();
        assert!(
            carrier.pending_opens.lock().unwrap_or_else(PoisonError::into_inner).is_empty(),
            "a pass on a disarmed carrier must not register lane opens"
        );
    }
}
