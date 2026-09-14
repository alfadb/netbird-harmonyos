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

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use base64::Engine as _;

use crate::backoff::{Clock, ExponentialBackoff, MonotonicClock, OsRandom, Rng};
use crate::config::{self, ConfigError, Json};
use crate::envelope::EnvelopeKeyPair;
use crate::grpc::{GrpcTransport, LoginParams, ManagementGrpcClient, PeerMeta};
use crate::hilog;
use crate::management::ManagementError;
use crate::mgmtsock::{dup_socket_fd, ManagementSocketProvider, ProtectedSocketFdSource};
use crate::network_map::NetworkMap;
use crate::peer_conn::{
    ice_ready_for_default_route, spawn_signal_link, IceOrchestratorSummary, LoggingSignalExchange,
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
    socket: Option<(Arc<dyn ManagementSocketProvider>, SocketAddr)>,
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
            socket: Some((source, connect_addr)),
        }
    }
}

impl ManagementFactory for GrpcManagementFactory {
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
                    ManagementGrpcClient::connect_with_socket_source(
                        &endpoint,
                        transport,
                        connect_timeout,
                        request_timeout,
                        keys,
                        source,
                        connect_addr,
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
}

impl ConnectorShared {
    fn new(force_default_route: bool) -> Self {
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
        let (allowed, reason) = ShellNetworkConfig::default_route_decision(
            snap.peers.len(),
            wg.tunnel_ready() && ice_ready,
            inner.force_default_route,
        );
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

    fn record_error(&self, err: &ManagementError) {
        self.lock().last_error = Some(ErrorClass::from_management(err));
    }

    fn count_retryable_failure(&self) {
        self.lock().reconnects += 1;
    }

    fn set_logout_ok(&self, ok: bool) {
        self.lock().logout_ok = Some(ok);
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
        if let Some(cfg) = update.netbird_config.as_ref() {
            if let Some(ice) = self.ice.get() {
                ice.lock_poison().set_stuns(&cfg.stuns);
            }
            if let Some(uri) = cfg.signal.as_ref() {
                signal_uri = Some(uri.clone());
            }
        } else {
            // config-less snapshot: keep the previously announced URI visible
            signal_uri = self.lock().net_config.as_ref().and_then(|c| c.signal.clone());
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
            let mut snapshot =
                ShellNetworkConfig::from_map_gated(map, g.force_default_route, wg.tunnel_ready() && ice_ready);
            snapshot.signal = signal_uri;
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
        if let Some(ice) = self.ice.get() {
            let keys: Vec<String> = map
                .peers
                .iter()
                .filter(|p| !p.allowed_ips.is_empty())
                .map(|p| p.wg_pub_key.clone())
                .collect();
            ice.lock_poison().set_peers(&keys);
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
            "{{{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}}}",
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
        format!(
            "{{{},{},{},{},{},{},\"routes\":[{}],\"dns\":{{{},{}}},{},\"peers\":[{}],\"default_route\":{{{},{}}}}}",
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
        )
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
    outbox: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<SignalOutgoing>>>,
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
            outbox: Mutex::new(Some(outbox)),
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
        let mut g = self.connect_addr.lock_poison();
        if g.is_none() {
            *g = Some(addr);
        }
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
        let Some(rx) = self.outbox.lock_poison().take() else {
            return false; // 队列已被并发启动取走：link guard 保证不再重复
        };
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
        let handle = spawn_signal_link(
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
    let mut guard = orch.lock_poison();
    if let Err(e) = guard.handle_signal(&m.from_key, kind, &m.payload, now) {
        // 畸形 payload（坏凭证/坏候选）：分类记录，不断流
        hilog::emit(&format!(
            "connector: signal frame rejected ({})",
            ErrorClass::from_management(&e).as_str()
        ));
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
        let shared = Arc::new(ConnectorShared::new(force_default_route));
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
        let ice_sockets: Option<Arc<crate::ice::ProtectedUdpFdSource>>;
        let ice = match ice {
            Some(orch) => {
                ice_sockets = None;
                orch
            }
            None => {
                let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1));
                let seam: Arc<dyn crate::peer_conn::SignalExchange> =
                    match signal_material.as_ref() {
                        Some(mat) => {
                            // N5d: real link materials — ONE exchange is
                            // created here and shared by the orchestrator
                            // seam (send side) and the worker (queue side).
                            let src = Arc::new(ProtectedSocketFdSource::new_with_fd(-1));
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
        let worker = runtime.spawn(worker_main(worker_deps, logout_slot.clone()));

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
            worker: Mutex::new(Some(worker)),
            renewal: Mutex::new(Some(renewal)),
            runtime,
        })
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
        }
    }

    /// The `connector_status()` JSON document.
    pub fn status_json(&self) -> String {
        self.status().to_json()
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
    let handle = ConnectorHandle::spawn(
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
    );
    let state = handle.status().state;
    *slot = Some(handle);
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
    if let Err(e) = dup_socket_fd(fd) {
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
    let source = Arc::new(ProtectedSocketFdSource::new_with_fd(fd));
    // N7: the PRODUCTION WG seam — the shell-fed device slot (same as
    // connector_start; the feeds arrive later, after create()+protect)
    let wg_feed = Arc::new(crate::wg_device::WgDeviceFeed::new(
        crate::wg_device::WgDeviceConfig::new(config.keys.secret_base64()),
    ));
    let handle = ConnectorHandle::spawn(
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
    );
    let state = handle.status().state;
    *slot = Some(handle);
    hilog::emit("connector: started over protected management socket (fd dup-consumed per dial)");
    format!(
        "{{{},{},{}}}",
        jbool("started", true),
        jstr("state", state.as_str()),
        jbool("protected", true)
    )
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
    if let Err(e) = dup_socket_fd(fd) {
        return format!(
            "{{{},{}}}",
            jbool("ok", false),
            jstr("error", e.token())
        );
    }
    source.feed(fd);
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
    if let Err(e) = dup_socket_fd(fd) {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", e.token()));
    }
    source.feed(fd);
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
    if let Err(e) = dup_socket_fd(fd) {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", e.token()));
    }
    source.feed(fd);
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
            "{{{},{}}}",
            jbool("ok", true),
            jbool("device_up", feed.device_up())
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
        }
        .to_json(),
    }
}

/// The `connector_network_config()` implementation: read-only snapshot of
/// the shell-applicable subset of the last applied NetworkMap (tunnel
/// address/prefix, managed routes with the default-route mark, DNS servers,
/// peer summary — public keys + allowed-ips counts only). Always valid JSON;
/// with no connector or before the first map:
/// `{"available":false,"reason":"no-network-map"}`.
pub fn connector_network_config_json() -> String {
    let slot = connector_slot();
    match slot.as_ref() {
        Some(handle) => handle.network_config_json(),
        None => NO_NETWORK_MAP_JSON.to_string(),
    }
}

/// The `connector_stop()` implementation (idempotent; safe with no
/// connector at all).
pub fn connector_stop_json() -> String {
    let mut slot = connector_slot();
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
            },
            recreate: RecreateStatus {
                required: false,
                count: 2,
                exhausted: false,
                cooling_down: true,
                reason: "route-set-changed".to_string(),
            },
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
        assert!(
            json.contains(
                "\"wg\":{\"fed_socket\":true,\"fed_tun\":true,\"device_up\":true,\"ready\":true,\
                 \"peers_with_session\":1,\"handshakes\":3,\"tx_packets\":9,\"tx_bytes\":900,\
                 \"rx_packets\":8,\"rx_bytes_to_tun\":800,\
                 \"dropped_no_route\":1,\"decrypt_errors\":2}"
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
                 \"dropped_no_route\":0,\"decrypt_errors\":0}"
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
        let shared = ConnectorShared::new(false);
        let ice = Arc::new(Mutex::new(PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![])),
            socks: Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1)),
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
        ] {
            assert!(json.contains(fragment), "missing {fragment} in {json}");
        }
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

    #[test]
    fn network_config_unavailable_until_first_map_then_cleared_on_stop_shape() {
        let shared = ConnectorShared::new(false);
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
        let shared = ConnectorShared::new(false);
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
        let shared = ConnectorShared::new(false);
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
        let shared = Arc::new(ConnectorShared::new(false));
        let orch = Arc::new(Mutex::new(PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![])),
            socks: Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1)),
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
                sockets: Arc::new(ProtectedSocketFdSource::new_with_fd(-1)),
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
        let shared = ConnectorShared::new(false);

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
}
