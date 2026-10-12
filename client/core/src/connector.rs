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
//! - **登录顺序（2026-10-12 账号修复）**：`Login` 只带**持久 WireGuard 公钥**
//!   （upstream `GrpcClient::Login` grpc.go:650-656，不带任何 token），
//!   没有 JWT、没有 setup key——这正是上游 connect 循环的形状
//!   （`client/internal/connect.go:774-796` `loginToManagement`）。登录/
//!   注册的**交互步骤**（一次性 JWT 或 setup key 走 `Register`）在上游是
//!   引擎启动**之前**的 login action（`client/android/client.go:205`
//!   `auth.login(...)` → `auth.go:208-236`）；账号模式下本仓由 UI 进程的
//!   [`crate::account_auth`] login-action NAPI 完成（一次性 JWT 不落盘），
//!   setup-key 模式仍由本序列的两步 `Login` → `Register(setup_key)` 完成
//!   （触发码 = upstream `isLoginNeeded`，`InvalidArgument`/
//!   `PermissionDenied`，auth.go:491-521）。`PermissionDenied` 的账号模式
//!   判定为 `needs-login` 类（upstream `StatusNeedsLogin`，connect.go:373-377
//!   / state.go:15），壳层据此显示官方文案（`Login required`/「需要登录」）
//!   而不是无限重试。见 [`login_sequence`]。
//! - 会话期限（2026-10-12，2026-10-13 补齐续期入口）：上游把 Login/Sync 上的 3 态
//!   `sessionExpiresAt` 锚定到 watcher（`client/internal/engine_authsession.go:17-62`
//!   `ApplySessionDeadline`；nil=保持、显式零=禁用、有效值=新期限），临期
//!   警告提前量在 `client/internal/auth/sessionwatch/watcher.go:34-43`
//!   （T-10min 交互警告 / T-2min 兜底）。**续期本身是官方 App 进程的交互
//!   流程**：`client/android/session.go:127-155` `ExtendAuthSession` 先跑一次
//!   SSO 拿新 JWT（`login.go:191-204`），再调引擎的
//!   `engine_authsession.go:83-107`（空 JWT 直接拒绝 L84-86；成功后把新期限
//!   写回 L102；隧道不动、不重同步 L71-73 注释）。本模块**没有** JWT，因此只
//!   把期限发布到状态文档（`session_expiry` / `session_expires_at_unix`）供壳层
//!   显示，绝不自行静默续期。交互入口在 UI 进程：SSO（ArkWeb PKCE）→
//!   [`crate::account_auth`] 的 extend job（`account_extend_*`，JWT 只活在那一次
//!   调用里）→ 该 job 把服务端答复经 [`session_deadline_apply_live`] 写回**本进程**
//!   的连接器（engine_authsession.go:102 的同一 sink）。
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
//! 2. ICE 平面自 stage 3 起共用**一枚** socket：mux 建在 WG 外层 socket 上
//!    （`connector_ice_mux_arm()`），所以不再有「壳侧按接口喂 UDP socket」
//!    这一步；没有 mux 则候选收集 fail-closed，peer 停在 Idle 并记录
//!    Network 类错误。
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
//!   `crate::ice::InterfaceSource`、共享 mux（`connector_ice_mux_arm`，
//!   stage 3 起建在 WG 外层 socket 上）、signal 发送
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
use crate::grpc::{
    is_login_needed, GrpcTransport, LoginOutcome, LoginParams, ManagementGrpcClient, PeerMeta,
};
use crate::hilog;
use crate::management::ManagementError;
use crate::mgmtsock::{ManagementSocketProvider, ProtectedSocketFdSource};
use crate::network_map::NetworkMap;
use crate::peer_conn::{
    ice_ready_for_default_route, spawn_signal_link_shared, IceCandidateEndpoint,
    IceOrchestratorSummary, LoggingSignalExchange, PeerIceDeps, PeerIceDetail, PeerIceOrchestrator,
    PeerSignalKind, RealSignalExchange, SignalLinkConfig, SignalLinkEvent,
};
use crate::route_selector::RouteSelector;
use crate::signal::{SignalMessage, SignalOutgoing};
use crate::state::{ConnEvent, ConnState, StateMachine};
use crate::sync::{SyncLoopEvent, SyncSession, SyncUpdate};
use crate::util::{jbool, jinum, jnum, jstr};

/// Pinned upstream commit the lifecycle semantics are modeled on (same pin
/// as [`crate::grpc::UPSTREAM_COMMIT`]).
pub const UPSTREAM_COMMIT: &str = crate::grpc::UPSTREAM_COMMIT;

/// Outer bound for the EXPLICIT user logout RPC
/// ([`ConnectorHandle::logout_json`]). Each RPC already carries the
/// client-wide `request_timeout` (default 10 s; logout = GetServerKey +
/// Logout, so worst case two of them) — this fuse keeps the NAPI caller
/// (the shell's watcher tick) bounded instead of frozen for that whole
/// span; past it the honest outcome is `reason=timeout`.
pub const EXPLICIT_LOGOUT_WAIT_MS: u64 = 8_000;

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

/// T0 e15ac: the KIND of shell ACK that reported the applied platform route set
/// — the `kind` argument of the NAPI
/// `connector_route_set_applied(routesJson, isRecreate, kind?)`. The automatic
/// controlled-recreate budget ([`RECREATE_MAX`] / [`RECREATE_COOLDOWN_MS`]) is
/// spent ONLY by [`RouteAckKind::Auto`]; `Initial` and `User` record the applied
/// set and clear the pending request without touching it, because a user's live
/// route toggle must never eat the automatic churn budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RouteAckKind {
    /// The initial `create()` (legacy `is_recreate=false`).
    Initial,
    /// A COMPLETED controlled recreate driven by the automatic
    /// desired-set-divergence signal (legacy `is_recreate=true`).
    Auto,
    /// A recreate the USER forced (live route toggle): records the applied set,
    /// spends no automatic budget, arms no cooldown.
    User,
}

impl RouteAckKind {
    /// Parse the NAPI `kind` argument: `initial` | `auto` | `user`. An ABSENT
    /// argument never reaches here — the legacy 2-arg entry maps its boolean
    /// instead (see [`connector_route_set_applied_json`]).
    fn parse(text: &str) -> Option<Self> {
        match text {
            "initial" => Some(RouteAckKind::Initial),
            "auto" => Some(RouteAckKind::Auto),
            "user" => Some(RouteAckKind::User),
            _ => None,
        }
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
    /// 2026-10-12: this peer needs a fresh interactive login — upstream
    /// `StatusNeedsLogin` (`client/internal/state.go:15`), set when the
    /// JWT-less/setup-key-less `Login` is refused with a login-needed code
    /// (`client/internal/connect.go:373-377`). Token: `needs-login` — the SAME
    /// stable token the shell's connect-failure channel renders as the
    /// official UI label (`Login required` /「需要登录」). It is NOT the
    /// generic `Auth` class: no retry of the same credential can fix it.
    NeedsLogin { status: u16 },
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
            ManagementError::NeedsLogin { status, .. } => {
                ErrorClass::NeedsLogin { status: *status }
            }
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
            ErrorClass::NeedsLogin { .. } => "needs-login",
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
            | ErrorClass::NeedsLogin { status }
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

/// Redacted management error detail for the DEVICE LOG only (2026-10-06,
/// login-failure visibility): the numeric gRPC/HTTP status plus the
/// server-sent message with secret material scrubbed — any occurrence of
/// the setup key becomes `[redacted]`, control characters collapse
/// to spaces (single-line log discipline) and the text is capped so a
/// chatty server cannot flood the log. The STATUS/ERROR JSON surfaces stay
/// class-only (credential discipline is unchanged); this is the diagnostic
/// log channel, where the message is the thing the operator actually needs
/// ("invalid setup key" vs "peer login has expired" ...). Empty for classes
/// that carry no server message (network/timeout/parse/...).
fn redacted_login_detail(err: &ManagementError, secrets: &ConnectorSecrets) -> String {
    let (status, message) = match err {
        ManagementError::Auth { status, message }
        | ManagementError::NeedsLogin { status, message }
        | ManagementError::Request { status, message } => (*status, message.as_str()),
        ManagementError::Server { status } => (*status, ""),
        _ => return String::new(),
    };
    let mut text = message.to_string();
    if secrets.setup_key.len() >= 8 && text.contains(secrets.setup_key.as_str()) {
        text = text.replace(secrets.setup_key.as_str(), "[redacted]");
    }
    let flat = text.replace(['\n', '\r', '\t'], " ");
    const CAP: usize = 160;
    let mut capped: String = flat.chars().take(CAP).collect();
    if capped.chars().count() < flat.chars().count() {
        capped.push('…');
    }
    if capped.is_empty() {
        format!("status {status}")
    } else {
        format!("status {status}: {capped}")
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
    /// OFFICIAL-ALIGNMENT STAGE 3 (audit §2.7): the BORROWED fd of the WG outer
    /// socket this seam's device reads and writes — the socket the ONE shared
    /// ICE mux is built over (upstream `createOrUpdateMux` wraps the WG bind's
    /// own conn, `ice_bind.go:291-316`). `None` (the default) = a seam with no
    /// shared-socket device: the mux then comes from the ICE socket provider
    /// instead, and the shell's arm entry fails closed (`no-wg-socket`).
    fn wg_socket_fd(&self) -> Option<i32> {
        None
    }
    /// OFFICIAL-ALIGNMENT STAGE 3: hand this device's socket read half to the
    /// shared mux's reader thread — there is exactly ONE reader of the socket
    /// (`WgDevice::service_udp` goes quiet). No-op by default (a seam without a
    /// shared socket has nothing to hand over). Returns whether the state
    /// changed.
    fn set_shared_socket_reader(&self, _on: bool) -> bool {
        false
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
    /// N6: apply the management-issued network routes (map `routes`) to the
    /// WG routing table — the device groups them into HA sets and assigns
    /// each network to ONE routing peer's allowed_ips (see
    /// `crate::wg_device` module docs, managed route overlay). Without this,
    /// LAN destinations behind routing peers hit `no-route`. Default: an
    /// honest no-op for seams with no WG routing table (the registry/test
    /// seams — same discipline as `recycle_endpoint`).
    fn apply_managed_routes(
        &self,
        _routes: &[crate::network_map::ManagedRoute],
    ) -> Result<(), String> {
        Ok(())
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
    /// N6: last apply_managed_routes payload (observability/test assert —
    /// the registry has no WG routing table to install it into).
    managed_routes: Mutex<Vec<crate::network_map::ManagedRoute>>,
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

    /// N6: the last recorded apply_managed_routes payload (snapshot).
    pub fn managed_routes(&self) -> Vec<crate::network_map::ManagedRoute> {
        self.managed_routes.lock_poison().clone()
    }
}

impl WgPeerApplier for WgPeerRegistry {
    fn apply_peers(&self, peers: &[WgPeerEntry]) -> Result<(), String> {
        *self.peers.lock_poison() = peers.to_vec();
        Ok(())
    }

    /// N6: record-only (no WG routing table exists on this seam).
    fn apply_managed_routes(
        &self,
        routes: &[crate::network_map::ManagedRoute],
    ) -> Result<(), String> {
        *self.managed_routes.lock_poison() = routes.to_vec();
        Ok(())
    }

    fn clear(&self) {
        self.peers.lock_poison().clear();
        self.endpoints.lock_poison().clear();
        self.managed_routes.lock_poison().clear();
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
    /// Startup route selection (`route_selection`, optional, **decision B**):
    /// the per-NetID selection state applied to every NetworkMap route list of
    /// THIS connector generation. Absent ⇒ [`RouteSelector::default`] =
    /// everything selected (the pre-existing behavior, byte for byte).
    ///
    /// Parsed with the SAME reader/validator as the persisted state document
    /// ([`RouteSelector::from_value`]) — malformed values FAIL the whole config
    /// parse (start returns `invalid-config`) instead of silently falling back
    /// to "all selected". The state is stored on the connector for the
    /// generation and read at the ONE filtering point in
    /// [`ConnectorShared::apply_update`]; since T0 e15ac the LIVE setter
    /// (`connector_route_selection_set_json` →
    /// [`ConnectorShared::route_selection_set_at`]) may replace it at runtime,
    /// serialized with the map pipeline, so a toggle needs no restart. This
    /// field changes neither [`RECREATE_MAX`] nor the default-route gate.
    ///
    /// v1 filters with [`RouteSelector::filter_selected`] EXACTLY: exit-node
    /// reconciliation / `skip_auto_apply` / exclusive auto-selection are
    /// deliberately NOT invoked, so a config without `route_selection` behaves
    /// exactly as before this increment.
    pub route_selection: RouteSelector,
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
    ///   "force_default_route": false,
    ///   "allow_unprotected_management": false,
    ///   "relay_enabled": false,
    ///   "ice_fixed_port": 0,
    ///   "advertised_candidates": [],
    ///   "route_selection": {
    ///     "selected_routes": [], "deselected_routes": ["HostInternal"],
    ///     "deselect_all": false
    ///   }
    /// }
    /// ```
    ///
    /// `ca_pem` (string or array of strings) is REQUIRED for `https://`
    /// (no system store is used); it is ignored for `http://` (plaintext,
    /// test only). Unknown fields are ignored.
    ///
    /// `route_selection` (optional, decision B): startup route-selection state
    /// (the exact [persisted document](RouteSelector::to_json) shape).
    /// Absent = everything selected; malformed = the whole config is refused
    /// (`invalid-config`), never a silent default.
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
    ///
    /// `route_selection` (optional, default = absent = all selected): the
    /// STARTUP selection state, validated BEFORE the first Sync, stored on the
    /// connector and applied to every NetworkMap route list at the single
    /// filtering point. A malformed object — wrong type, bad NetID array,
    /// duplicate key, or a non-object value — makes THIS parse fail, so
    /// `connector_start*` answers `invalid-config`; it never degrades to
    /// "all selected" (no silent fallback).
    ///
    /// T0 e15ac: this is the SEED only. The same state can be replaced live by
    /// `connector_route_selection_set_json`, serialized with the map pipeline,
    /// so a user toggle takes effect without a restart/reconnect.
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
        let mut force_default_route = false;
        let mut allow_unprotected_management = false;
        let mut ice_fixed_port: u16 = 0;
        let mut advertised_candidates: Vec<crate::ice::Candidate> = Vec::new();
        let mut relay_enabled = false;
        // decision B: the optional startup selection state (absent = default
        // all-selected). Validated here, before any Sync can run.
        let mut route_selection: Option<RouteSelector> = None;

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
                // decision B: startup route selection. The SAME strict parser
                // utilities as the persisted document (`parse_document` already
                // ran over the whole start config; `from_value` validates this
                // parsed object). Malformed ⇒ this whole parse fails ⇒
                // `connector_start*` reports `invalid-config` (stable token).
                "route_selection" => {
                    if route_selection.is_some() {
                        return Err(ConfigError::Field {
                            field: "route_selection",
                            reason: "duplicate field".into(),
                        });
                    }
                    route_selection = Some(
                        RouteSelector::from_value(val).map_err(|e| ConfigError::Field {
                            field: "route_selection",
                            reason: e.to_string(),
                        })?,
                    );
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
            force_default_route,
            allow_unprotected_management,
            ice_fixed_port,
            advertised_candidates,
            relay_enabled,
            route_selection: route_selection.unwrap_or_default(),
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

/// 2026-10-12: what a connect may still carry is the SETUP KEY only. The
/// management JWT is deliberately NOT a connect credential any more: upstream
/// keeps no token on disk (`client/internal/profilemanager/config.go:62-135`
/// has no token field) and uses it only inside the interactive login action
/// that runs before the engine starts (`client/android/client.go:194-215`).
/// In this shell that action is the UI-process [`crate::account_auth`] NAPI,
/// so every connect is token-less — account mode sends `{}`.
/// Manual `Debug`: the field is a secret and must never format through.
#[derive(Clone, Default)]
pub struct ConnectorSecrets {
    /// Pre-authorized setup key (empty when absent — account mode).
    pub setup_key: String,
}

impl ConnectorSecrets {
    /// Parse `{"setup_key": "..."}`. 2026-10-12: the setup key may be absent
    /// (account mode) and the JWT key is GONE — a normal connect identifies the
    /// peer by its persistent WireGuard public key alone (upstream
    /// `GrpcClient::Login`, grpc.go:650-656). An unknown `jwt`/`jwt_token` key
    /// is ignored like every other unknown field.
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
                _ => {}
            }
        }
        Ok(secrets)
    }
}

// Debug redaction: never print secret material (credential discipline).
impl core::fmt::Debug for ConnectorSecrets {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConnectorSecrets")
            .field("setup_key", &"REDACTED")
            .finish()
    }
}

// ---------------------------------------------------------------------------
// shared state + status
// ---------------------------------------------------------------------------

/// decision B: the UNFILTERED route catalog of one applied NetworkMap.
///
/// The selection filter runs only on the EFFECTIVE map handed to the shell/WG
/// seams; this copy keeps every advertised candidate (including user-disabled
/// HA rows) so the future public catalog can list them with their `enabled`
/// flag. Pure data, no secrets.
#[derive(Clone, Debug, PartialEq, Eq)]
struct AdvertisedRoutes {
    /// `NetworkMap.serial` of the snapshot these rows came from.
    serial: u64,
    /// Every advertised managed route, in map order (unfiltered).
    routes: Vec<crate::network_map::ManagedRoute>,
}

/// T0 e15ac: outcome of one live route-selection update
/// ([`ConnectorShared::route_selection_set_at`]) — the data behind the
/// `connector_route_selection_set_json` document. Public routing material only
/// (NetIDs/prefixes are not even rendered here; just prefixes and counts).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RouteSelectionOutcome {
    /// A NetworkMap was applied: the advertised catalog exists, so the
    /// selection was filtered against it and the seams were updated.
    available: bool,
    /// The stored selection actually changed (an identical re-send is still
    /// applied, idempotently, and reports `false`).
    changed: bool,
    /// The EFFECTIVE route set after the update (default-route gate applied), in
    /// map order — exactly what the platform config should hold.
    desired_routes: Vec<String>,
    /// The platform route set must be replaced for `desired_routes` to take
    /// effect (see [`recreate_needed_for`]).
    recreate_needed: bool,
    /// The WG seam refused the filtered routes: state updated, data plane not.
    wg_apply_failed: bool,
}

/// T0 e15ac: does the platform route set have to be replaced?
///
/// Compares the PREFIX SET, not the row list: the platform installs one route
/// per prefix, so an HA owner/metric swap on the same prefixes (management can
/// move which peer serves a network, and the WG device picks the actual owner
/// per session, without changing the advertised prefixes) must NOT ask for an OS
/// replace — the WG table is re-applied either way. An UNKNOWN applied set
/// (`None`: the shell never ACKed one, e.g. an older shell) always reports the
/// need, so the signal is never silently absent.
///
/// T0 native5f: the same rule now ALSO governs the automatic
/// `recreate.required` signal in [`ConnectorShared::refresh_net_gate_at`], so
/// the setter's `recreate_needed` and the watcher's auto signal can never
/// disagree about a row-shape-only change (an HA owner swap on unchanged
/// prefixes must not consume the [`RECREATE_MAX`] / [`RECREATE_COOLDOWN_MS`]
/// budget). The budget, the cooldown, the ACK kinds and the frozen
/// `connector_status` fields are unchanged.
fn recreate_needed_for(desired: &[String], applied: Option<&[String]>) -> bool {
    let Some(applied) = applied else {
        return true;
    };
    let mut desired: Vec<&str> = desired.iter().map(String::as_str).collect();
    let mut applied: Vec<&str> = applied.iter().map(String::as_str).collect();
    desired.sort_unstable();
    desired.dedup();
    applied.sort_unstable();
    applied.dedup();
    desired != applied
}

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
    ///
    /// The startup route selection (`route_selection`) is ALREADY applied: this
    /// list is built from the effective (filtered) map, so a user-disabled
    /// network can never be re-exported by a later gate refresh.
    net_routes_all: Vec<ShellRouteEntry>,
    /// decision B: the route selection of THIS connector. Seeded from the
    /// start config (`route_selection`) before the first Sync and read at the
    /// ONE filtering point in `apply_update`. T0 e15ac: the generation-fixed
    /// rule is GONE — the live selection setter
    /// ([`ConnectorShared::route_selection_set_at`], reached from
    /// `connector_route_selection_set_json`) mutates it under `route_apply`,
    /// so a map apply and a preference change can never interleave. It stays
    /// a single field read through the same `filter_selected` helper, so there
    /// is exactly one filter semantics in the client.
    route_selection: RouteSelector,
    /// decision B: the UNFILTERED advertised route catalog of the last applied
    /// NetworkMap (`None` before the first map / after stop). Kept separate
    /// from `net_routes_all` on purpose: the future public catalog must show
    /// user-disabled candidates (and their `enabled` flag) even though the
    /// effective map excludes them. Cleared on stop like the snapshot.
    advertised_routes: Option<AdvertisedRoutes>,
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
    /// N5c: the protected-UDP source behind the ICE orchestrator. It is the
    /// STUN hostname's protected-DNS resolver ([`crate::ice::ProtectedUdpFdSource::
    /// refresh_stun_dns`]) and, on the HOST, the supply the ONE shared mux is
    /// built from (`PeerIceOrchestrator::arm_mux`).
    ///
    /// STAGE 4 (official-alignment §2.7): the DEVICE shell no longer feeds it —
    /// the ICE plane takes no per-session socket any more, and the mux is built
    /// over the WG outer socket. The queue counters therefore stay at zero on
    /// device (the device NAPI feed entry is gone); the source itself stays for
    /// the two jobs above.
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
    /// N13-D3: per-peer relay URL ADVERTISED by the remote (signal
    /// OFFER/ANSWER `relayServerAddress`), recorded by the signal receive
    /// path ([`route_signal_message`]) and handed to the relay carrier as
    /// its per-peer relay-server choice input
    /// ([`RelayCarrier::new_with_ads`]). Created with the CONNECTOR, not with
    /// the carrier: the receive path records advertisements for a carrier
    /// that may not be armed (and a `relay_enabled:false` connector records
    /// nothing — no signal link, no relay dial). Peer keys are public WG
    /// material, URLs are public routing data.
    relay_ads: Arc<Mutex<HashMap<String, String>>>,
    /// T0 e15ac (LIVE route update): the ONE serialization door of the map
    /// route pipeline. [`ConnectorShared::apply_update`] holds it from the
    /// NetworkMap serial gate through `host.apply` (the relay/STUN/signal
    /// bookkeeping ABOVE the serial gate deliberately stays outside), and the
    /// live selection setter ([`ConnectorShared::route_selection_set_at`])
    /// holds it while it re-filters the advertised catalog, rebuilds the shell
    /// snapshot and re-applies the WG managed routes. Consequence: the route
    /// list a map apply installs and the list the setter installs can never
    /// interleave — the seams end up with whichever ran LAST, and both the
    /// state and the seams describe the SAME selection.
    ///
    /// Lock order: `route_apply` → `inner` (then the pre-existing `inner` →
    /// `ice`/`relay` order inside the pipeline). `route_apply` is NEVER
    /// acquired while `inner`, `ice`, `relay`, a carrier leaf lock or a WG
    /// device lock is held.
    route_apply: Mutex<()>,
    /// 2026-10-09 (ArkTS main-thread freeze fix): the LAST SUCCESSFULLY read
    /// ICE sub-snapshot — the `ice` summary and the per-peer `ice_peers`
    /// details, both from ONE orchestrator acquisition — with its monotonic read
    /// time. `connector_status()` and
    /// the network-config gate read the orchestrator with `try_lock` ONLY: the
    /// 10 ms ICE pump holds that mutex across a synchronous gather
    /// (`ensure_locals` → the STUN round, `DEFAULT_STUN_TIMEOUT_MS` per
    /// peer) and on the 2026-10-09 device run four offline peers re-negotiating
    /// held it 8.4–11 s, freezing the ArkTS main thread on its 5 s status poll.
    /// A busy lock now serves THIS sample (with its age) instead of queueing —
    /// never a fabricated empty state (0 peers / no ICE surface) that made the
    /// UI flicker. `stale`/`age_ms` are rendered by
    /// [`with_ice_sockets_section`] as `ice_snapshot_stale` /
    /// `ice_snapshot_age_ms`.
    ///
    /// 2026-10-12（阶段 1，官方对齐审计 §2.7）：那段「泵在一次同步收集里长持
    /// 锁」的历史已随本地候选收集移到 worker 上而结束——现在泵一拍只做状态机
    /// 推进与非阻塞取件。`try_lock` 行为**不变**（它防的是任何持锁方，不只是
    /// 收集），只是「持锁 8~11 s」不再是本快照新鲜度设计的动机。
    ///
    /// Lock order: `inner` → `ice` → this leaf. This leaf is taken with NO
    /// orchestrator guard and NO other leaf held (only `inner` may be held by
    /// [`ConnectorShared::refresh_net_gate_at`]), and a guard of this leaf is
    /// never held while acquiring `inner` or `ice`.
    ice_snapshot: Mutex<Option<IceStatusSample>>,
}

/// 2026-10-09 (ArkTS main-thread freeze fix): ONE ICE status sub-snapshot —
/// everything `connector_status()` needs from the peer-ICE orchestrator, read
/// under a single acquisition (or served from the last one when the lock is
/// busy). Counts / state tokens / public keys only: no ufrag, no password, no
/// fd, no token.
#[derive(Debug, Clone, PartialEq)]
struct IceStatusSample {
    /// The orchestrator summary (`ice`): per-state counts, endpoint landings,
    /// reachability and the class-only last error.
    ice: IceOrchestratorSummary,
    /// The per-peer detail table (`ice_peers`): `Some(..)` for an armed
    /// orchestrator (`Some(vec![])` = armed with ZERO peers). `None` = no
    /// orchestrator at all (known absent) — or, when `stale` is set, an armed
    /// orchestrator that was never successfully read (the documented unknown
    /// shape, flagged by `ice_snapshot_stale`).
    ice_peers: Option<Vec<PeerIceDetail>>,
    /// 0.0.26 gap 1: the shared mux's counters, read in the SAME orchestrator
    /// acquisition as `ice`/`ice_peers` (non-blocking: a busy mux lock answers
    /// `None` = unknown). `None` also means "no mux at all".
    mux: Option<crate::ice_mux::MuxStats>,
    /// Monotonic ms of the successful read that produced `ice`/`ice_peers`.
    at_ms: u64,
    /// TRUE when this sample is served BECAUSE the orchestrator lock was busy
    /// — i.e. it is the last successful read, not a read taken now.
    stale: bool,
    /// Age of the served sample, ALWAYS non-negative: `Some(0)` = fresh read,
    /// `Some(n)` = the cached sample is n ms old, `None` = there is no
    /// successful ICE read to age (rendered `null`, this document's own
    /// "unknown"). A NEGATIVE sentinel is deliberately not used: the crate's
    /// strict config reader rejects negative numbers
    /// ([`crate::config::parse_document`] — the N2-H freeze-facts reader parses
    /// THIS document with it), and the frozen `queued` `-1` sentinel is the only
    /// negative anybody signed up for.
    age_ms: Option<u64>,
}

impl IceStatusSample {
    /// No orchestrator object at all: a KNOWN absent ICE surface (not an
    /// unread one), so it is never marked stale and carries no age.
    fn absent() -> Self {
        IceStatusSample {
            ice: IceOrchestratorSummary::default(),
            ice_peers: None,
            mux: None,
            at_ms: 0,
            stale: false,
            age_ms: None,
        }
    }

    /// An armed orchestrator that was NEVER successfully read (the lock has
    /// been busy since the first poll): the documented unknown shape — zeroed
    /// summary and `ice_peers:null` — plus an explicit `stale` mark and an
    /// UNKNOWN (`null`) age, so the shell reads it as unknown, never as a real
    /// zero-peered orchestrator read just now.
    fn unknown_busy() -> Self {
        IceStatusSample {
            ice: IceOrchestratorSummary::default(),
            ice_peers: None,
            mux: None,
            at_ms: 0,
            stale: true,
            age_ms: None,
        }
    }
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
    /// Build with the pre-existing default selection (everything on) — every
    /// non-start caller (host tests, injected seams) keeps its behavior.
    /// TEST-ONLY: production construction goes through
    /// [`ConnectorShared::new_selected`] with the start config's selection.
    #[cfg(test)]
    fn new(force_default_route: bool, relay_attach: Option<RelayAttach>) -> Self {
        Self::new_selected(force_default_route, relay_attach, RouteSelector::new())
    }

    /// [`ConnectorShared::new`] plus the generation's startup route selection
    /// (`ConnectorConfig::route_selection`). The selection is validated before
    /// this point (config parse) and stored ONCE for the generation.
    fn new_selected(
        force_default_route: bool,
        relay_attach: Option<RelayAttach>,
        route_selection: RouteSelector,
    ) -> Self {
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
                wg_apply_failed: false,
                wg_apply_errors: 0,
                logout_ok: None,
                started_at_unix: None,
                net_config: None,
                net_routes_all: Vec::new(),
                route_selection,
                advertised_routes: None,
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
            relay_ads: Arc::new(Mutex::new(HashMap::new())),
            route_apply: Mutex::new(()),
            ice_snapshot: Mutex::new(None),
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
            // 2026-10-12（diag/0027 item 3）：管理状态机迁移是**证据行**
            // （每次迁移一行），走 phone log 的小额优先预算。
            hilog::emit_evidence(&format!("connector: state -> {}", to.as_str()));
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

    /// The `connector_routes_json()` payload (decision B, internal helper —
    /// NAPI registration comes later). Side-effect-free snapshot under ONE
    /// mutex acquisition: unlike [`ConnectorHandle::network_config_json`] it
    /// does NOT run `refresh_net_gate`, so reading it never advances the
    /// recreate state.
    fn routes_json(&self) -> String {
        let g = self.lock();
        routes_document(&g.route_selection, g.advertised_routes.as_ref())
    }

    // -----------------------------------------------------------------------
    // T0 e15ac: LIVE route-selection update (no restart)
    // -----------------------------------------------------------------------

    /// Apply a NEW selection to the running connector — the mutation
    /// counterpart of [`ConnectorShared::routes_json`]. `selection` is the
    /// already strictly parsed raw selector document (the public entry
    /// [`connector_route_selection_set_json`] owns the parsing, so an invalid
    /// document never reaches this function).
    ///
    /// This is the LIVE half of the ONE filtering point: the map route pipeline
    /// of [`ConnectorShared::apply_update`] and this rebuild share ONE critical
    /// section ([`ConnectorShared::route_apply`]), so a preference change can
    /// never interleave with a map apply — either the map ran with the old
    /// selection (and this call overwrites the WS/shell result) or it ran with
    /// the new one. The filter itself is the same
    /// [`RouteSelector::filter_selected`] helper the map path uses; there is no
    /// second filter semantics to drift.
    ///
    /// Order of operations (the contract):
    /// 1. take `route_apply`, then `inner`; store the selection, remember
    ///    whether it actually changed;
    /// 2. re-filter the UNFILTERED advertised catalog
    ///    ([`AdvertisedRoutes::routes`] — it is never rewritten, so the public
    ///    catalog keeps showing disabled candidates), rebuild
    ///    `net_routes_all`, `route_count` and the gated `net_config.routes`
    ///    (same default-route gate rule as [`ConnectorShared::refresh_net_gate`]
    ///    — the gate DECISION itself is readiness-driven and is reused, not
    ///    recomputed here);
    /// 3. release `inner`, THEN hand the filtered routes to the WG seam
    ///    (`apply_managed_routes`) — never a WG/device call under `inner`;
    /// 4. peers, their base `allowed_ips`, ICE/relay state and every landed
    ///    session are NOT touched: the selection governs managed routes only.
    ///
    /// With no applied NetworkMap there is nothing to filter: the selection is
    /// stored for the NEXT map and the outcome reports `available:false` —
    /// this function never claims an applied route set it cannot see.
    fn route_selection_set_at(
        &self,
        selection: RouteSelector,
        wg: &dyn WgPeerApplier,
    ) -> RouteSelectionOutcome {
        let mut g = self.lock();
        let changed = g.route_selection != selection;
        g.route_selection = selection;
        let filtered = g
            .advertised_routes
            .as_ref()
            .map(|adv| g.route_selection.filter_selected(&adv.routes));
        let mut outcome = RouteSelectionOutcome { available: false, changed, ..Default::default() };
        let mut wg_routes: Option<Vec<crate::network_map::ManagedRoute>> = None;
        if let Some(filtered) = filtered {
            outcome.available = true;
            // net_routes_all is the UNGATED record of the EFFECTIVE list (the
            // live gate re-exports from it); route_count follows it.
            g.net_routes_all = filtered
                .iter()
                .map(|r| ShellRouteEntry {
                    network: route_network_string(&r.network),
                    is_default: r.network.is_default(),
                })
                .collect();
            let count = g.net_routes_all.len();
            g.route_count = count;
            // The default-route gate is a readiness decision, NOT a selection
            // outcome: reuse the live snapshot's stored verdict and rebuild the
            // exported list from the new record, exactly like
            // `refresh_net_gate` does when the verdict moves.
            let default_allowed =
                g.net_config.as_ref().map_or(false, |snap| snap.default_route_allowed);
            let gated: Vec<ShellRouteEntry> = g
                .net_routes_all
                .iter()
                .filter(|r| !r.is_default || default_allowed)
                .cloned()
                .collect();
            outcome.desired_routes = gated.iter().map(|r| r.network.clone()).collect();
            if let Some(snap) = g.net_config.as_mut() {
                snap.routes = gated;
            }
            outcome.recreate_needed =
                recreate_needed_for(&outcome.desired_routes, g.applied_route_set.as_deref());
            wg_routes = Some(filtered);
        }
        drop(g);
        if let Some(routes) = wg_routes {
            if wg.apply_managed_routes(&routes).is_err() {
                // fail-closed: the state now names a route set the data plane
                // does not have; the shell must not treat the toggle as
                // installed (the VPE keeps/rejects its config in that case).
                let mut g = self.lock();
                g.wg_apply_failed = true;
                g.wg_apply_errors += 1;
                outcome.wg_apply_failed = true;
            }
        }
        outcome
    }

    /// 2026-10-09 (ArkTS main-thread freeze fix): THE non-blocking ICE status
    /// read — the `ice` summary and the per-peer `ice_peers` details from a
    /// SINGLE acquisition, taken with `try_lock`
    /// ONLY. Callers on the shell's synchronous status / network-config path
    /// (the ArkTS main thread) must go through this; the pre-fix assembly took
    /// the same orchestrator lock three times and each acquisition could queue
    /// behind the ICE pump's synchronous gather (device run 2026-10-09: 8.4 s
    /// of one tick held the lock, 4 offline peers re-negotiating).
    ///
    /// Busy lock = serve the LAST successful sample with its age (`stale`) —
    /// never a fabricated empty state. A never-read armed orchestrator renders
    /// the documented unknown shape marked stale with an UNKNOWN (`null`) age.
    /// A fresh read also refreshes the cache, so the next busy poll keeps
    /// reporting the most recent real state.
    fn ice_status_sample(&self) -> IceStatusSample {
        let Some(orch) = self.ice.get() else {
            // No orchestrator object at all (unit-test construction / before an
            // ICE-capable start): a KNOWN absent ICE surface, never stale.
            return IceStatusSample::absent();
        };
        // One acquisition, two pure reads; the guard is released inside the
        // `map` before the cache below is touched (lock order: `ice` → cache,
        // and never the reverse).
        let fresh = try_lock_poison(orch).map(|g| IceStatusSample {
            ice: g.summary(),
            ice_peers: Some(g.ice_details()),
            // 0.0.26 gap 1: same acquisition, non-blocking leaf read.
            mux: g.mux_stats_try(),
            at_ms: crate::sys::mono_ms(),
            stale: false,
            age_ms: Some(0),
        });
        if let Some(sample) = fresh {
            *self.ice_snapshot.lock_poison() = Some(sample.clone());
            return sample;
        }
        // Busy: the ICE pump is inside a gather. Serve the last successful
        // sample (age measured against the monotonic clock), or the explicit
        // unknown shape when there never was one.
        let now = crate::sys::mono_ms();
        match self.ice_snapshot.lock_poison().as_ref() {
            Some(last) => IceStatusSample {
                stale: true,
                age_ms: Some(now.saturating_sub(last.at_ms)),
                ..last.clone()
            },
            None => IceStatusSample::unknown_busy(),
        }
    }

    /// 2026-10-09 (ArkTS main-thread freeze fix): the default-route gate's ICE
    /// view. Read with the SAME non-blocking rule as the status surface but
    /// WITHOUT the per-peer details / socket demand (this path needs the
    /// summary alone, and it stays as cheap as the pre-fix
    /// `lock_poison().summary()`): a busy orchestrator serves the CACHED
    /// sample's summary, or the documented unknown default when there never was
    /// one — never a blocking acquisition, and never a gate decision invented
    /// from a fabricated empty state (the cache is only ever written by
    /// [`ConnectorShared::ice_status_sample`], so it is one coherent
    /// acquisition, never a summary stitched onto older details).
    fn ice_summary_for_gate(&self) -> IceOrchestratorSummary {
        let Some(orch) = self.ice.get() else {
            // No orchestrator (unit-test construction): the unchanged N3-7
            // "no peers" rule — `ice_ready_for_default_route` reads a zeroed
            // summary as ready, exactly like the pre-fix `None => true`.
            return IceOrchestratorSummary::default();
        };
        if let Some(summary) = try_lock_poison(orch).map(|g| g.summary()) {
            return summary;
        }
        self.ice_snapshot
            .lock_poison()
            .as_ref()
            .map(|cached| cached.ice.clone())
            .unwrap_or_default()
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
    ///
    /// T0 native5f: that divergence test is the PREFIX SET
    /// ([`recreate_needed_for`]), not the row list — identical to the live
    /// setter's `recreate_needed`, so a duplicate/permuted ACK of the same
    /// prefixes is silent on BOTH surfaces and spends no auto budget. An
    /// unknown applied set (`None`) still keeps the signal silent (initial
    /// create pending), never treated as an empty set.
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
        // 2026-10-09: the ICE leaf is read with the SAME non-blocking rule as
        // `connector_status()` ([`ConnectorShared::ice_summary_for_gate`]): a
        // busy orchestrator serves the last known summary (or the documented
        // unknown default) instead of queueing this read behind the ICE pump's
        // 8–11 s gather. The gate is re-decided on EVERY read, so a stale view
        // self-corrects on the next successful one — and it is never replaced
        // by a fabricated one.
        let ice_ready = ice_ready_for_default_route(&self.ice_summary_for_gate());
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
            // T0 native5f: the SAME PREFIX-SET rule the live setter uses
            // ([`recreate_needed_for`]). The platform installs one route per
            // prefix, so an HA owner/metric swap — or merely duplicate/permuted
            // rows — on the same prefixes is NOT a desired-set change and must
            // neither raise the signal nor spend automatic recreate budget.
            Some(applied) if !recreate_needed_for(&desired, Some(applied)) => {
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

    /// N8 / T0 e15ac: shell ACK — `routes` is now the route set APPLIED to the
    /// platform VpnConfig, reported with the KIND of recreate that produced it
    /// ([`RouteAckKind`]). The legacy boolean maps onto that kind: `false` =
    /// [`RouteAckKind::Initial`], `true` = [`RouteAckKind::Auto`].
    ///
    /// - `Initial`: the initial `create()`. Records the applied set and clears
    ///   the pending request; spends NO budget and arms NO cooldown. `routes`
    ///   may be EMPTY (T0 e15ac): an explicitly ACKed empty platform set is a
    ///   real state (e.g. every network deselected), and recording it arms the
    ///   desired-vs-applied comparison so a network management introduces later
    ///   — default-ENABLED — still raises the recreate signal.
    /// - `Auto`: a COMPLETED controlled recreate driven by the automatic
    ///   divergence signal. Counts against [`RECREATE_MAX`] and arms the
    ///   [`RECREATE_COOLDOWN_MS`] window (the auto budget stays 3 / 30s).
    /// - `User`: a recreate the USER forced (a live route toggle). Records the
    ///   applied set and clears the pending request, but touches NO automatic
    ///   count / cooldown / exhausted state — a user toggle must never consume
    ///   the automatic churn budget, which stays fully available afterwards.
    ///
    /// Every kind records the EXACT set the shell reported, even if the map
    /// moved on in the meantime: the ACK is evidence of what the platform
    /// holds, never a claim about the current desired state. A stale ACK
    /// therefore just keeps `desired != applied` and the next gate refresh
    /// re-raises `recreate.required`. Idempotent-safe: repeated ACKs of the
    /// same set without `is_recreate` only re-record it.
    pub fn ack_applied_route_set(&self, routes: Vec<String>, is_recreate: bool, now_ms: u64) {
        self.ack_applied_route_set_kind(
            routes,
            if is_recreate { RouteAckKind::Auto } else { RouteAckKind::Initial },
            now_ms,
        );
    }

    /// [`ConnectorShared::ack_applied_route_set`] with the ACK kind explicit
    /// (the NAPI entry carries `kind` as a string — see
    /// [`connector_route_set_applied_kind_json`]).
    fn ack_applied_route_set_kind(&self, routes: Vec<String>, kind: RouteAckKind, now_ms: u64) {
        let mut g = self.lock();
        g.applied_route_set = Some(routes.clone());
        match kind {
            RouteAckKind::Initial => {
                g.recreate.required = false;
                hilog::emit(&format!(
                    "connector: applied route set acked (initial, routes={})",
                    routes.len()
                ));
            }
            RouteAckKind::Auto => {
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
            }
            RouteAckKind::User => {
                // Deliberately NOT touching `count` / `last_recreate_ms` /
                // `exhausted` (and not re-rendering `cooling_down`): the
                // automatic churn budget and its cooldown window are a
                // platform-churn bound, and a USER-forced recreate is not part
                // of that churn. The derived `cooling_down` rendering is
                // re-derived by the next `refresh_net_gate` read.
                g.recreate.required = false;
                hilog::emit(&format!(
                    "connector: user route change acked (auto budget untouched, routes={})",
                    routes.len()
                ));
            }
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
    /// N13-D3: the SAME call supplies the carrier's per-peer advertisement
    /// map (shared with the receive path) and the foreign-relay spawn
    /// context of this sync's relay set.
    fn relay_carrier_client_started(
        &self,
        client: crate::relay_client::RelayClient,
        relay: &crate::network_map::RelayServers,
    ) {
        let (Some(wg), Some(rt)) = (self.relay_wg.get(), self.relay_runtime.get()) else {
            return;
        };
        if !wg.carrier_capable() {
            return;
        }
        if self.relay_carrier.get().is_none() {
            // N13-D3: the carrier reads the SAME advertisement map the signal
            // receive path writes — created here, shared with the connector.
            let _ = self
                .relay_carrier
                .set(RelayCarrier::new_with_ads(wg.clone(), self.relay_ads.clone()));
        }
        let carrier = self.relay_carrier.get().expect("carrier slot just set");
        if !carrier.has_client(&client) {
            carrier.attach_client(client);
            carrier.ensure_pump(rt);
        }
        // N13-D3: the foreign-relay spawn context rides the SAME sync that
        // (re)armed the home client (official manager.go `openConnVia` reuses
        // one token store + peerID across home and foreign relay clients).
        // Runs on EVERY relay sync — including the same-URL token refresh,
        // where it rotates the token of every LIVE foreign client without
        // reconnecting them (`update_token`).
        self.relay_foreign_ctx(carrier, relay);
    }

    /// N13-D3: hand the carrier the foreign-relay spawn context derived from
    /// this sync's relay set. Same fail-closed gate as
    /// [`ConnectorShared::relay_apply`]: an empty URL set or an unusable
    /// token leaves the PREVIOUS context in place — never a half-built one.
    /// The URLs parse ALL-OR-NOTHING, mirroring
    /// [`crate::relay_client::RelayClientConfig::new`]: a set the home client
    /// would refuse can never become a foreign dial target.
    fn relay_foreign_ctx(
        &self,
        carrier: &std::sync::Arc<RelayCarrier>,
        relay: &crate::network_map::RelayServers,
    ) {
        let Some(attach) = self.relay_attach.as_ref() else {
            return;
        };
        if relay.urls.is_empty() {
            return;
        }
        let token = match crate::relay::AuthToken::from_management(
            &relay.token_payload,
            &relay.token_signature,
        ) {
            Ok(t) => t,
            // relay_apply already reported this sync's token as unusable.
            Err(_) => return,
        };
        let mut urls = Vec::with_capacity(relay.urls.len());
        for url in &relay.urls {
            match crate::relay_client::RelayUrl::parse(url) {
                Ok(parsed) => urls.push(parsed),
                Err(_) => return,
            }
        }
        carrier.set_foreign_ctx(attach.clone(), urls, token);
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
            current_url: stats.current_url.clone(),
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
                            self.relay_carrier_client_started(client, r);
                        }
                    }
                    None => {
                        self.relay_drop("relay-removed-from-sync");
                        if let Some(carrier) = self.relay_carrier.get() {
                            // N13-D3: management revoked the relay — the
                            // FOREIGN clients must go with the home one (no
                            // orphan outbound relay connection). Before
                            // `detach_all`: closing a foreign client takes
                            // the lifecycle lock itself, so it must not run
                            // under a holder (see [`RelayCarrier::shutdown`]).
                            carrier.stop_foreign_all("relay-removed-from-sync");
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
        // T0 e15ac: enter the SHARED route critical section. From here through
        // `host.apply` below, the map serial gate, the filter/rebuild of
        // `net_routes_all` + the shell snapshot, the WG managed-route apply and
        // `host.apply` form ONE section against the live selection setter
        // ([`ConnectorShared::route_selection_set_at`]). Everything ABOVE this
        // line (config/relay/STUN/signal bookkeeping) is deliberately NOT
        // serialized with it. Lock order: `route_apply` → `inner`.
        let _route_apply = self.route_apply.lock_poison();
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
        for (index, route) in map.routes.iter().enumerate() {
            hilog::emit(&crate::route_audit::managed_route_line(
                map.serial,
                index,
                map.routes.len(),
                route,
            ));
        }
        // decision B — the ONE filtering point. Order matters and is the
        // contract:
        //   1. the audit lines above ran on the UNFILTERED map (every
        //      advertised row is witnessed, including user-disabled ones);
        //   2. the UNFILTERED catalog is snapshotted for the future public
        //      catalog (`connector_routes_json`, disabled candidates visible);
        //   3. only then is the EFFECTIVE clone built and shadowed over `map`,
        //      so every consumer below — the shell snapshot
        //      (`ShellNetworkConfig::from_map_gated`), `net_routes_all`, the
        //      WG managed-route apply, and `host.apply` — sees the SAME
        //      filtered route list. Peer `allowed_ips` (the overlay /32s) are
        //      NOT filtered: the selection governs managed routes only.
        // v1 calls `filter_selected` EXACTLY (default-on minus explicit
        // deselections, `deselect_all` ⇒ none): exit-node reconciliation /
        // `skip_auto_apply` / exclusive auto-selection are deliberately NOT
        // invoked, so an absent `route_selection` is byte-identical to the
        // pre-existing behavior.
        let effective = {
            let mut g = self.lock();
            let selected = g.route_selection.filter_selected(&map.routes);
            g.advertised_routes =
                Some(AdvertisedRoutes { serial: map.serial, routes: map.routes.clone() });
            let mut effective = map.clone();
            effective.routes = selected;
            effective
        };
        let map = &effective;
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
        match self.relay_carrier.get() {
            Some(carrier) => carrier.set_peers(&connectable_keys),
            // N13-D3: no carrier (relay never armed / seam not carrier-capable)
            // — the advertisement map is still the same per-peer routing
            // record, so bound it to the connectable set exactly as
            // `set_peers` does. The sender key of a signal frame is only as
            // authenticated as its envelope (the server routes by RECIPIENT),
            // so entries for keys outside the network map must never
            // accumulate without bound.
            None => {
                self.relay_ads
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .retain(|k, _| connectable_keys.contains(k));
            }
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
        // N6: management-issued network routes → WG routing table. The map's
        // peer allowed_ips only carry the overlay /32s; the routed subnets
        // (e.g. 192.168.50.0/24 behind routing peers, HA = two entries with
        // the same network) would otherwise hit `no-route`. System-side
        // routing is untouched (the shell installs managed routes into the
        // platform VpnConfig).
        if wg.apply_managed_routes(&map.routes).is_err() {
            let mut g = self.lock();
            g.wg_apply_failed = true;
            g.wg_apply_errors += 1;
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
    /// Foreign relay clients started (one per distinct advertised URL,
    /// official manager.go `relayClients` map entries).
    pub foreign_opened: u64,
    /// Foreign relay clients closed (idle sweep / revoked / teardown /
    /// server disconnect).
    pub foreign_closed: u64,
}

/// Retry backoff for a lane whose `open_conn` failed (e.g. the peer stays
/// offline and the 30s §4.1 window timed out). This port re-opens lanes on
/// the pump reconcile where upstream re-opens strictly on demand, so
/// without it a fast-failing open would retry every pump pass; with it an
/// offline peer costs one attempt per backoff window (the blocking 30s
/// window itself stays the dominant rate limiter). Cost: recovery after a
/// FAILED open is delayed by up to one window.
const LANE_OPEN_RETRY: Duration = Duration::from_secs(5);

/// Foreign-relay idle policy — official `shared/relay/client/manager.go`:
/// `relayCleanupInterval = 60s` (sweep cadence) and
/// `keepUnusedServerTime = 5s` (grace before a foreign relay with NO live
/// lane may be closed). Effective idle window is therefore 5–65s, exactly
/// upstream's shape.
const FOREIGN_RELAY_SWEEP: Duration = Duration::from_secs(60);
const FOREIGN_RELAY_GRACE: Duration = Duration::from_secs(5);

/// Which relay server a lane to a peer must ride — the official
/// `WorkerRelay.OnNewOffer` → `Manager.OpenConn(serverAddress, peerKey)`
/// decision (worker_relay.go:48-69 `preferredRelayServer` +
/// manager.go:162-194). The CONTROLLER's home relay is the meet point:
/// the controller (higher WG public key, upstream `isController =
/// LocalKey > Key`) opens on its own home relay, the other side dials the
/// controller's ADVERTISED URL (`relayServerAddress` of the received
/// offer/answer) — as a foreign relay client when it differs from home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayChoice {
    /// The lane rides our home relay client (also the fallback when the
    /// remote advertises nothing — the pre-foreign behavior).
    Home,
    /// The lane rides a dedicated client to this remote-advertised URL.
    Foreign(crate::relay_client::RelayUrl),
}

/// Upstream `isController(config) = config.LocalKey > config.Key`
/// (conn.go:1040-1042, base64 WG public key STRING compare, lexicographic)
/// — so the REMOTE is the controller (and its advertised relay the meet
/// point) exactly when the peer's key sorts above ours. The same
/// asymmetric comparison our ICE layer already replicates
/// (`peer_conn::deterministic_controlling`); both sides compute it
/// independently and agree.
pub fn remote_is_controller(local_wg_pubkey: &str, peer_wg_pubkey: &str) -> bool {
    peer_wg_pubkey > local_wg_pubkey
}

/// Canonical relay-URL comparison: parse both sides so scheme case and
/// default ports normalize (`RELS://x` ≡ `rels://x:443`). An unparseable
/// home string never equals a canonical form (mirrors upstream's plain
/// string compare, which treats any mismatch as foreign).
fn same_relay_url(candidate: &str, canonical: &str) -> bool {
    match crate::relay_client::RelayUrl::parse(candidate) {
        Ok(url) => url.config_string() == canonical,
        Err(_) => candidate.trim() == canonical,
    }
}

/// Decide the relay server for one peer's lane (pure — no client is
/// created here; `RelayCarrier::lane_client` resolves the decision).
///
/// - We are the controller → [`RelayChoice::Home`] (upstream
///   `preferredRelayServer` returns the controller's own address).
/// - Remote is the controller: its advertised URL is the meet point —
///   [`RelayChoice::Foreign`] unless it is (canonically) our home relay.
///   No advertisement / an unparseable one falls back to Home (our
///   long-standing single-relay behavior; upstream would skip the relay
///   lane entirely for a non-advertising remote).
/// - While the home client has no live session yet, an advertised URL
///   that is INSIDE our configured set still counts as Home (the dial
///   round may land there); once connected, only the live session URL is
///   Home (upstream compares against the connected instance URL,
///   manager.go:430-436).
pub fn choose_relay_url(
    advertised: Option<&str>,
    remote_controller: bool,
    home_current_url: Option<&str>,
    configured: &[crate::relay_client::RelayUrl],
) -> RelayChoice {
    if !remote_controller {
        return RelayChoice::Home;
    }
    let Some(advertised) = advertised else {
        return RelayChoice::Home;
    };
    let Ok(url) = crate::relay_client::RelayUrl::parse(advertised) else {
        return RelayChoice::Home;
    };
    let canonical = url.config_string();
    match home_current_url {
        Some(current) => {
            if same_relay_url(current, &canonical) {
                RelayChoice::Home
            } else {
                RelayChoice::Foreign(url)
            }
        }
        None => {
            if configured.iter().any(|u| u.config_string() == canonical) {
                RelayChoice::Home
            } else {
                RelayChoice::Foreign(url)
            }
        }
    }
}

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
    /// The live HOME relay client (set at the first start / refreshed when
    /// the URL set changes — a swap aborts the old pump and detaches lanes:
    /// presence does not survive a relay reconnect, D1 semantics).
    client: Mutex<Option<crate::relay_client::RelayClient>>,
    wg: std::sync::Arc<dyn WgPeerApplier>,
    /// Connectable peer keys ↔ their relay peer ids (the reverse map for
    /// the inbound pump; rebuilt from every network-map sync).
    peers: Mutex<Vec<(String, crate::relay::PeerId)>>,
    /// Currently attached lanes: peer key → (the carrier handle the WG
    /// seam received, the relay URL the lane rides). The URL enables the
    /// stale-lane reconcile: when a peer's advertised relay changes (or
    /// the choice flips home/foreign), the old lane is detached and
    /// re-opened on the new choice. Identity for idempotent attach/detach
    /// bookkeeping remains the carrier handle.
    attached: Mutex<HashMap<String, (std::sync::Arc<crate::relay_client::RelayWgCarrier>, String)>>,
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
    /// (client/peers/attached/pending_opens/open_retry_after/ads/
    /// foreign_ctx/foreign); never held across an await; no leaf holder
    /// ever acquires `lifecycle` (no ABBA).
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
    /// Per-peer relay URL ADVERTISED by the remote (signal OFFER/ANSWER
    /// `relayServerAddress`, recorded by the signal link). Shared with the
    /// connector (written by the signal route, read here) so entries exist
    /// even before the carrier is first armed. Peer keys are PUBLIC WG
    /// material; URLs are public routing data — no secret ever enters.
    ads: Arc<Mutex<HashMap<String, String>>>,
    /// Spawn context for foreign relay clients (official manager.go
    /// `openConnVia`: same token store + peerID + dial config for every
    /// foreign URL). `None` = foreign lanes unavailable (carrier built
    /// without a relay attach — host/test constructions; lanes then fall
    /// back to the home relay).
    foreign_ctx: Mutex<Option<ForeignRelayCtx>>,
    /// Live foreign relay clients, keyed by CANONICAL URL string
    /// (re-use per relay server, official `relayClients` map). Each entry
    /// owns its inbound pump task feeding [`RelayCarrier::ingest`].
    foreign: Mutex<HashMap<String, ForeignRelayEntry>>,
    /// Idle policy for the foreign sweep (grace / sweep interval),
    /// overridable for tests; see [`FOREIGN_RELAY_GRACE`].
    foreign_timing: Mutex<(Duration, Duration)>,
    /// Monotonic anchor of the last foreign sweep (pump tick gate).
    last_foreign_sweep: Mutex<Option<std::time::Instant>>,
}

/// One foreign relay client + its inbound pump (official manager.go
/// `RelayTrack`: the dialed client plus the reuse/cleanup bookkeeping).
struct ForeignRelayEntry {
    client: crate::relay_client::RelayClient,
    pump: tokio::task::JoinHandle<()>,
    created: std::time::Instant,
}

/// Everything needed to start a relay client for a FOREIGN URL: the SAME
/// management-issued token as the home client (one token store upstream),
/// the local WG public key (peerID derivation) and the injected seams.
/// The token is SECRET: manual Debug redacts it (credential discipline).
pub struct ForeignRelayCtx {
    pub attach: RelayAttach,
    pub token: crate::relay::AuthToken,
    /// The HOME configured URL set (the home-match rule of
    /// [`choose_relay_url`] while the home client has no live session).
    pub urls: Vec<crate::relay_client::RelayUrl>,
}

impl Clone for ForeignRelayCtx {
    fn clone(&self) -> Self {
        ForeignRelayCtx {
            attach: self.attach.clone(),
            token: self.token.clone(),
            urls: self.urls.clone(),
        }
    }
}

impl core::fmt::Debug for ForeignRelayCtx {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ForeignRelayCtx")
            .field("urls", &self.urls.iter().map(|u| u.config_string()).collect::<Vec<_>>())
            .field("token", &"<redacted>")
            .finish()
    }
}

impl RelayCarrier {
    /// An unarmed orchestrator over `wg` (host/test entry: arm it with
    /// [`RelayCarrier::attach_client`] once a relay client exists; the
    /// connector arms it automatically from the sync path). Carries its own
    /// (empty) peer → advertised-relay-URL map — the connector shares the
    /// signal link's map through [`RelayCarrier::new_with_ads`].
    pub fn new(wg: std::sync::Arc<dyn WgPeerApplier>) -> std::sync::Arc<Self> {
        Self::new_with_ads(wg, Arc::new(Mutex::new(HashMap::new())))
    }

    /// [`RelayCarrier::new`] with the connector's SHARED advertisement map:
    /// `ads` is the peer → advertised-relay-URL map the signal link records
    /// OFFER/ANSWER advertisements into (shared handle, may pre-date arming).
    pub fn new_with_ads(
        wg: std::sync::Arc<dyn WgPeerApplier>,
        ads: Arc<Mutex<HashMap<String, String>>>,
    ) -> std::sync::Arc<Self> {
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
            ads,
            foreign_ctx: Mutex::new(None),
            foreign: Mutex::new(HashMap::new()),
            foreign_timing: Mutex::new((FOREIGN_RELAY_GRACE, FOREIGN_RELAY_SWEEP)),
            last_foreign_sweep: Mutex::new(None),
        })
    }

    /// Override the foreign idle policy (grace, sweep interval). Test seam
    /// only — production keeps the official manager.go values.
    pub fn set_foreign_idle_params(&self, grace: Duration, sweep_every: Duration) {
        *self.foreign_timing.lock().unwrap_or_else(PoisonError::into_inner) = (grace, sweep_every);
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

    /// 006 resource snapshot: attached lane count under a NON-BLOCKING probe
    /// of the `attached` leaf. `None` = the leaf is busy → unknown. ONE leaf
    /// is taken here and no other carrier lock is held, matching the field
    /// docs' lock order (the caller must not hold a carrier lock either).
    pub fn lane_count_try(&self) -> Option<usize> {
        Some(try_lock_poison(&self.attached)?.len())
    }

    /// 006 resource snapshot: in-flight lane-open tasks under a NON-BLOCKING
    /// probe of the `pending_opens` leaf. `None` = busy → unknown.
    pub fn pending_open_count_try(&self) -> Option<usize> {
        Some(try_lock_poison(&self.pending_opens)?.len())
    }

    /// 006 resource snapshot: live foreign relay client count under a
    /// NON-BLOCKING probe of the `foreign` leaf. `None` = busy → unknown.
    pub fn foreign_count_try(&self) -> Option<usize> {
        Some(try_lock_poison(&self.foreign)?.len())
    }

    /// THE ACTUAL per-peer relay lane map for the status document: one
    /// [`PeerRelayStatus`] per lane in [`RelayCarrier::attached`] — the peer
    /// key → relay URL pairs the WG seam really received, in a deterministic
    /// (public-key sorted) order. This is the ONLY honest per-peer relay
    /// source: the remote's ADVERTISED URL ([`RelayCarrier::ads`]) and the
    /// configured/home URL set say nothing about whether a path exists, and
    /// a peer with no lane here has no entry. Takes exactly one leaf lock
    /// (`attached`), holds it for the duration of the copy and takes no
    /// other lock — callers must not hold a carrier lock (see the field
    /// docs' lock order).
    pub fn peer_relays(&self) -> Vec<PeerRelayStatus> {
        let attached = self.attached.lock().unwrap_or_else(PoisonError::into_inner);
        let mut out: Vec<PeerRelayStatus> = attached
            .iter()
            .map(|(key, (_, via))| PeerRelayStatus {
                pub_key_b64: key.clone(),
                relay_url: via.clone(),
            })
            .collect();
        // Deterministic document order (the attached table is a HashMap).
        out.sort_by(|a, b| a.pub_key_b64.cmp(&b.pub_key_b64));
        out
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

    /// Whether `client` is a client a lane may ride: the armed HOME client
    /// or a LIVE foreign relay client (official manager.go keys per-peer
    /// connections by relay instance). A lane open that resolves after its
    /// foreign client was closed ([`RelayCarrier::close_foreign`] — idle
    /// sweep / server disconnect / teardown) must NOT commit: the client is
    /// gone from the table and the lane would be an orphan no sweep could
    /// find. Lock order: caller holds `lifecycle`, this takes one leaf.
    fn client_is_live(&self, client: &crate::relay_client::RelayClient) -> bool {
        self.has_client(client)
            || self
                .foreign
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .values()
                .any(|e| e.client.same_client(client))
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
        drop(attached);
        // Advertisements of departed peers go with them (bounded memory; a
        // returning peer re-advertises on its next OFFER/ANSWER).
        self.ads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|k, _| keys.contains(k));
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
    /// Which RELAY SERVER the lane rides is decided per attempt
    /// ([`RelayCarrier::lane_via`]: the controller's home relay — for a
    /// remote-controller peer its ADVERTISED URL, dialed as a foreign
    /// relay client when it differs from home, official
    /// `Manager.OpenConn(serverAddress, peerKey)`).
    ///
    /// Presence reconcile (upstream client lifecycle): an attached lane
    /// whose peer the server reported OFFLINE (`PeersWentOffline`) is
    /// detached here — the offline check runs on the lane's OWN client
    /// (foreign lanes report presence on the foreign client, not home).
    /// The relay server's came-online interest is ONE-SHOT
    /// (consumed by the first `PeersOnline` delivery — only a NEW
    /// `SubscribePeerState` re-arms it), so a lane kept attached across the
    /// peer's offline window can never see the peer's return: this is the
    /// upstream "went-offline → close per-peer connection → re-`OpenConn`
    /// on demand" half our port was missing. The re-open below IS that
    /// fresh `open_conn`: it blocks in the §4.1 wait until the peer returns
    /// (fresh subscribe re-arms the interest → `PeersOnline` → presence
    /// restored → lane re-attached, egress accepted again), or fails typed
    /// and backs off [`LANE_OPEN_RETRY`] before the next attempt. The same
    /// detach+re-open runs when the peer's advertised relay CHANGED (a
    /// stale lane on the wrong server can never meet the peer).
    pub async fn ensure_lane(self: &std::sync::Arc<Self>, key: &str) -> bool {
        let Some(home) = self.client.lock().unwrap_or_else(PoisonError::into_inner).clone()
        else {
            return false;
        };
        let pid = crate::relay::PeerId::from_wg_pubkey_string(key);
        // Resolve the lane's relay server (may START a foreign client —
        // official manager.go `openConnVia` get-or-create).
        let Some((client, via)) = self.lane_client(&home, key) else {
            return false;
        };
        {
            let attached = self.attached.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some((lane, lane_via)) = attached.get(key) {
                if lane_via == &via && !lane.client().is_offline(&pid) {
                    return true;
                }
            }
        }
        // Stale lane (peer went offline on this client, or the choice moved
        // to another relay): detach so the re-open below re-subscribes on
        // the CURRENT server (idempotent: the attached table is keyed,
        // open_conn guards duplicate pending subscribes, and the WG seam
        // attach is an upsert).
        if self
            .attached
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(key)
            .is_some()
        {
            self.wg.detach_carrier(key);
            self.bump(|s| s.detach_ok += 1);
            hilog::emit(&format!(
                "connector: relay carrier lane detached (offline or relay changed)|peer={}|relay={}",
                pid, via
            ));
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
        // The per-peer relay selection line (task requirement): which relay
        // URL this peer's lane uses, and whether it is our home relay.
        let is_home = client.same_client(&home);
        hilog::emit(&format!(
            "connector: relay lane opening|peer={}|relay={}|home={}",
            pid, via, is_home
        ));
        match client.open_conn(&pid).await {
            Ok(()) => {
                self.open_retry_after
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(key);
                self.commit_lane_open(key, pid, client, via)
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
                    "connector: relay lane open failed|peer={} relay={} class={}",
                    pid,
                    via,
                    relay_error_class_token(e.class())
                ));
                false
            }
        }
    }

    /// The relay URL a lane to `key` must ride, as a canonical string
    /// (decision only — NEVER starts a foreign client; safe under the
    /// lifecycle guard). `None` = no home client armed.
    fn lane_via(&self, home: &crate::relay_client::RelayClient, key: &str) -> Option<String> {
        let advertised = self
            .ads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .cloned();
        let ctx = self.foreign_ctx.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(ctx) = ctx.as_ref() else {
            return home.stats().current_url;
        };
        let current = home.stats().current_url;
        match choose_relay_url(
            advertised.as_deref(),
            remote_is_controller(&ctx.attach.wg_pubkey_b64, key),
            current.as_deref(),
            &ctx.urls,
        ) {
            RelayChoice::Home => current,
            RelayChoice::Foreign(url) => Some(url.config_string()),
        }
    }

    /// Resolve the lane's relay server to a LIVE client (home or foreign —
    /// foreign is created here when missing, official manager.go
    /// `openConnVia`: publish-then-dial, re-use per URL). `None` = carrier
    /// unarmed or the foreign client could not be started.
    fn lane_client(
        self: &std::sync::Arc<Self>,
        home: &crate::relay_client::RelayClient,
        key: &str,
    ) -> Option<(crate::relay_client::RelayClient, String)> {
        let via = self.lane_via(home, key)?;
        // The decision landed on our OWN live relay session (a Home choice,
        // or an advertisement canonically equal to the connected URL): ride
        // the home client — never dial a SECOND client to our own relay
        // server (official manager.go reuses the connected instance,
        // manager.go:430-436).
        if home
            .stats()
            .current_url
            .as_deref()
            .is_some_and(|current| same_relay_url(&via, current))
        {
            return Some((home.clone(), via));
        }
        if let Some(url) = self
            .foreign
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&via)
        {
            return Some((url.client.clone(), via));
        }
        let Ok(parsed) = crate::relay_client::RelayUrl::parse(&via) else {
            // Unparsable (no live home session either) — nothing to dial;
            // ride the home client, fail-closed at its `open_conn`.
            return Some((home.clone(), via));
        };
        match self.foreign_client(&parsed) {
            Some(client) => Some((client, via)),
            None => Some((home.clone(), via)),
        }
    }

    /// Build + start ONE relay client for an explicit URL list from the
    /// injected seams — the foreign twin of `ConnectorShared::relay_start`:
    /// the SAME management token store, the SAME peerID (our WG public key)
    /// and the SAME injected clock / dialer / TLS connector, so a foreign
    /// client is indistinguishable from home on the wire (official
    /// manager.go `openConnVia` reuses one config per relay instance).
    /// Requires a running tokio runtime (called from the lane-open path).
    fn start_relay_client(
        attach: &RelayAttach,
        urls: &[String],
        token: crate::relay::AuthToken,
    ) -> Result<crate::relay_client::RelayClient, crate::relay_client::RelayClientError> {
        let mut cfg = crate::relay_client::RelayClientConfig::new(
            urls,
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

    /// Get-or-create the foreign relay client for `url` (keyed by the
    /// CANONICAL URL string — re-use per relay server, official
    /// `relayClients` map). The client uses the SAME management token and
    /// injected seams as home (one token store) and gets its own inbound pump
    /// feeding [`RelayCarrier::ingest`].
    ///
    /// Check → dial → insert run in ONE critical section under the documented
    /// order (`lifecycle` first, then the `foreign` leaf): two peers whose
    /// advertisements resolve to the same relay URL must never both dial it —
    /// the loser's client would be untracked and no sweep could close it (an
    /// orphan outbound relay connection, N13 plan §4). The `lifecycle` read is
    /// what makes a DISARMED carrier refuse to create one at all:
    /// [`RelayCarrier::shutdown`] drains the table while holding that guard,
    /// so no client (and no pump task) can be born past teardown. Nothing here
    /// awaits (`RelayClient::start` spawns the worker and returns;
    /// `tokio::spawn` neither awaits nor polls), so the guards may be held.
    fn foreign_client(
        self: &std::sync::Arc<Self>,
        url: &crate::relay_client::RelayUrl,
    ) -> Option<crate::relay_client::RelayClient> {
        let canonical = url.config_string();
        let lifecycle = self.lifecycle.lock().unwrap_or_else(PoisonError::into_inner);
        if !*lifecycle {
            hilog::emit(&format!(
                "connector: foreign relay refused (carrier disarmed)|url={}",
                canonical
            ));
            return None;
        }
        let mut foreign = self.foreign.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(entry) = foreign.get(&canonical) {
            return Some(entry.client.clone());
        }
        let ctx = self.foreign_ctx.lock().unwrap_or_else(PoisonError::into_inner).clone();
        let Some(ctx) = ctx else {
            hilog::emit(&format!(
                "connector: foreign relay unavailable (no spawn context) — lane rides home|url={}",
                canonical
            ));
            return None;
        };
        let client = Self::start_relay_client(
            &ctx.attach,
            &[canonical.clone()],
            ctx.token.clone(),
        )
        .inspect_err(|e| {
            hilog::emit(&format!(
                "connector: foreign relay start failed ({e:?}) — lane rides home|url={}",
                canonical
            ));
        })
        .ok()?;
        let pump = tokio::spawn(foreign_relay_pump(
            self.clone(),
            canonical.clone(),
            client.clone(),
        ));
        foreign.insert(
            canonical.clone(),
            ForeignRelayEntry { client: client.clone(), pump, created: std::time::Instant::now() },
        );
        drop(foreign);
        drop(lifecycle);
        self.bump(|s| s.foreign_opened += 1);
        hilog::emit(&format!(
            "connector: foreign relay client started|url={}",
            canonical
        ));
        Some(client)
    }

    /// Idle sweep (official manager.go `cleanUpUnusedRelays`, driven from
    /// the pump tick): a foreign client older than the grace window with
    /// NO attached lane riding it is stopped and dropped — no orphan
    /// outbound relay connection (N13 plan §4).
    fn sweep_foreign_if_due(&self) {
        let now = std::time::Instant::now();
        let (grace, sweep_every) = *self.foreign_timing.lock().unwrap_or_else(PoisonError::into_inner);
        {
            let mut last = self.last_foreign_sweep.lock().unwrap_or_else(PoisonError::into_inner);
            if last.is_some_and(|t| now.duration_since(t) < sweep_every) {
                return;
            }
            *last = Some(now);
        }
        let entries: Vec<(String, crate::relay_client::RelayClient)> = self
            .foreign
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(url, e)| (url.clone(), e.client.clone()))
            .collect();
        for (url, client) in entries {
            if now.duration_since(self.foreign_created(&url).unwrap_or(now)) < grace {
                continue;
            }
            let used = self
                .attached
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .values()
                .any(|(lane, _)| lane.client().same_client(&client));
            if !used {
                self.close_foreign(&url, "unused");
            }
        }
    }

    /// `created` anchor of a tracked foreign client (sweep grace input).
    fn foreign_created(&self, url: &str) -> Option<std::time::Instant> {
        self.foreign
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(url)
            .map(|e| e.created)
    }

    /// Close + drop one foreign client: its lanes detach first (fail-closed
    /// — the bearer must not look like a path), then the client stops
    /// (graceful relay Close frame) and its pump task is reaped. Under the
    /// lifecycle lock (invalidating transition for late lane commits).
    fn close_foreign(&self, url: &str, why: &str) {
        let lifecycle = self.lifecycle.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(entry) = self.foreign.lock().unwrap_or_else(PoisonError::into_inner).remove(url)
        else {
            return;
        };
        let stale: Vec<String> = self
            .attached
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(_, (lane, _))| lane.client().same_client(&entry.client))
            .map(|(k, _)| k.clone())
            .collect();
        for key in stale {
            self.attached.lock().unwrap_or_else(PoisonError::into_inner).remove(&key);
            self.wg.detach_carrier(&key);
            self.bump(|s| s.detach_ok += 1);
        }
        drop(lifecycle);
        self.retire_foreign(url, entry, why);
    }

    /// The stop half of [`RelayCarrier::close_foreign`]: graceful Close +
    /// pump reap + accounting, with NO carrier lock held (the entry is
    /// already out of the table and its lanes are already detached, so this
    /// section cannot race a lane commit). Never call it while holding
    /// `lifecycle` — [`RelayCarrier::shutdown`] drains the table under that
    /// guard and retires the drained entries HERE, after dropping it.
    fn retire_foreign(&self, url: &str, entry: ForeignRelayEntry, why: &str) {
        entry.client.stop();
        entry.pump.abort();
        self.bump(|s| s.foreign_closed += 1);
        hilog::emit(&format!(
            "connector: foreign relay client closed ({why})|url={}",
            url
        ));
    }

    /// The foreign client's own pump ended (`recv()` drained to `None` ⇒
    /// its worker exited: stopped or terminal `Dead`) — official
    /// `onServerDisconnected` → `evictForeignRelay`: drop the entry and its
    /// lanes so the next pass re-dials fresh instead of reusing a dead
    /// client.
    fn foreign_ended(&self, url: &str) {
        let present = self
            .foreign
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(url);
        if present {
            self.close_foreign(url, "server-disconnected");
        }
    }

    /// Stop EVERY foreign client (management revoked relay) — no orphan
    /// outbound relay connection survives. Called from the sync path's
    /// relay-removed branch, NOT from [`RelayCarrier::shutdown`] (which drains
    /// the table under its own disarm guard; see the note there).
    pub fn stop_foreign_all(&self, why: &str) {
        let urls: Vec<String> =
            self.foreign.lock().unwrap_or_else(PoisonError::into_inner).keys().cloned().collect();
        for url in urls {
            self.close_foreign(&url, why);
        }
    }

    /// Live foreign relay client count (observability / reuse assertions).
    pub fn foreign_count(&self) -> usize {
        self.foreign.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    /// Canonical URLs of the live foreign relay clients (observability).
    pub fn foreign_urls(&self) -> Vec<String> {
        self.foreign
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect()
    }

    /// Arm/refresh the foreign-relay spawn context (called from the sync
    /// path on every relay advertisement): the same token the home client
    /// authenticates with, plus the home configured URL set. A token
    /// rotation propagates to every LIVE foreign client too (official
    /// shares one token store, manager.go `tokenStore`).
    pub fn set_foreign_ctx(
        &self,
        attach: RelayAttach,
        urls: Vec<crate::relay_client::RelayUrl>,
        token: crate::relay::AuthToken,
    ) {
        *self.foreign_ctx.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(ForeignRelayCtx { attach, urls, token: token.clone() });
        for entry in self.foreign.lock().unwrap_or_else(PoisonError::into_inner).values() {
            entry.client.update_token(token.clone());
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
    /// `via` (the canonical relay URL the lane rides) is stored WITH the
    /// lane: the attached table is the stale-lane reconcile input (a lane
    /// on the wrong server is detached and re-opened) and the foreign
    /// sweep's liveness input. Lock order: lifecycle → one leaf at a time;
    /// no await under any lock.
    fn commit_lane_open(
        &self,
        key: &str,
        pid: crate::relay::PeerId,
        client: crate::relay_client::RelayClient,
        via: String,
    ) -> bool {
        let lifecycle = self.lifecycle.lock().unwrap_or_else(PoisonError::into_inner);
        let still_wanted =
            *lifecycle
                && self.client_is_live(&client)
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
                    .insert(key.to_string(), (lane, via));
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
            // A live lane is skipped only while it is BOTH healthy on ITS OWN
            // relay client — a foreign lane reports presence on the foreign
            // client, so asking the home client would freeze a lane the
            // foreign relay already declared offline — AND still on the relay
            // server the CURRENT choice resolves to: a changed advertisement
            // (or a home/foreign flip) must move the lane, and `ensure_lane`
            // is what detaches and re-opens it. `lane_via` is decision-only
            // (it never dials), so this stays inside the lifecycle guard's
            // await-free discipline.
            let live_lane = self
                .attached
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&key)
                .map(|(lane, via)| (lane.client().clone(), via.clone()));
            if let Some((lane_client, lane_via)) = live_lane {
                if !lane_client.is_offline(&pid)
                    && self.lane_via(&client, &key).as_deref() == Some(lane_via.as_str())
                {
                    continue;
                }
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

    /// Pump ONE inbound relay frame of the HOME relay client into the WG
    /// device. `false` = the relay client is gone/stopped (the pump exits on
    /// that).
    pub async fn pump_once(&self) -> bool {
        let Some(client) = self.client.lock().unwrap_or_else(PoisonError::into_inner).clone()
        else {
            return false;
        };
        self.pump_once_via(&client).await
    }

    /// [`RelayCarrier::pump_once`] for an EXPLICIT client (home or a foreign
    /// relay client — each foreign client has its own inbound pump feeding
    /// the same [`RelayCarrier::ingest`] reverse map). `false` = that
    /// client's inbound channel drained (worker stopped / terminal `Dead`).
    pub async fn pump_once_via(&self, client: &crate::relay_client::RelayClient) -> bool {
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
    /// detached (connector stop / worker end / host-driven teardown) + every
    /// foreign relay client stopped. Idempotent. Under the lifecycle lock
    /// (disarm): mutually exclusive with the lane-open commit — a commit
    /// either finished before this (detached here) or sees the disarmed
    /// carrier and drops.
    ///
    /// DEADLOCK CONTRACT: [`RelayCarrier::close_foreign`] takes `lifecycle`
    /// itself, so it must NEVER run while this method holds it. The foreign
    /// table is therefore DRAINED inside the disarmed critical section and the
    /// clients are retired after the guard is released. Draining under the
    /// guard (instead of closing them before it) is also what makes the
    /// teardown airtight: `foreign_client` refuses a disarmed carrier under
    /// that same guard, so no lane open in flight can create a client — and
    /// with it a pump task and an outbound relay connection — past this point.
    pub fn shutdown(&self) {
        let mut lifecycle = self.lifecycle.lock().unwrap_or_else(PoisonError::into_inner);
        *lifecycle = false;
        if let Some(h) = self.task.lock().unwrap_or_else(PoisonError::into_inner).take() {
            h.abort();
        }
        self.abort_pending_lanes();
        self.detach_all_locked(&lifecycle);
        let foreign: Vec<(String, ForeignRelayEntry)> =
            self.foreign.lock().unwrap_or_else(PoisonError::into_inner).drain().collect();
        drop(lifecycle);
        for (url, entry) in foreign {
            self.retire_foreign(&url, entry, "teardown");
        }
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

/// Inbound pump for ONE foreign relay client (official manager.go
/// `RelayTrack`): every frame it receives is fed into the carrier's ingest
/// reverse map (the same WG carrier ingress the home pump uses), and its
/// tick drives the foreign idle sweep — the home pump only serves the home
/// client, so without this task a foreign client's frames would never
/// arrive and its cleanup would never run. Cadence matches the home pump
/// (500 ms); the sweep itself gates at [`FOREIGN_RELAY_SWEEP`].
///
/// Exits when `client.recv()` drains to `None` (the worker stopped — relay
/// Close / terminal `Dead`) or the state machine is already `Dead`: the
/// carrier then drops the entry and its lanes
/// ([`RelayCarrier::foreign_ended`] → official `onServerDisconnected` →
/// `evictForeignRelay`), so the next lane pass re-dials fresh instead of
/// reusing a dead client. A client closed by the sweep aborts this task —
/// the sweep already detached its lanes.
async fn foreign_relay_pump(
    carrier: std::sync::Arc<RelayCarrier>,
    url: String,
    client: crate::relay_client::RelayClient,
) {
    loop {
        if client.state() == crate::relay_client::RelayState::Dead {
            carrier.foreign_ended(&url);
            return;
        }
        tokio::select! {
            // pump this foreign client's inbound frames as they arrive;
            // `false` = its inbound channel drained (worker gone)
            kept = async {
                loop {
                    if !carrier.pump_once_via(&client).await {
                        return false;
                    }
                }
            } => {
                if !kept {
                    carrier.foreign_ended(&url);
                    return;
                }
            }
            _ = tokio::time::sleep(core::time::Duration::from_millis(500)) => {
                // idle sweep: close foreign relays with no live lane and no
                // fresh use (no orphan outbound relay connection)
                carrier.sweep_foreign_if_due();
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

/// One ACTUAL relay lane in the status document's `peer_relays` tail: the
/// peer's PUBLIC WireGuard key and the relay URL its attached lane rides —
/// the ACTUAL per-peer relay, read from the live lane table
/// ([`RelayCarrier::peer_relays`]), never from a remote advertisement or a
/// global home URL. A peer with no attached lane of its own has NO entry
/// (the UI's "this peer is not relayed" signal). Both values are public
/// routing material — `relay_url` is the canonical configured form
/// (`rel://host:port` / `rels://host:port`) and carries NO token material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerRelayStatus {
    pub pub_key_b64: String,
    pub relay_url: String,
}

/// The `peer_relays` tail: `null` (no relay carrier armed at all — the
/// array would be an invented statement) or the actual lanes, sorted by
/// public key so the document is deterministic.
fn peer_relays_json(v: Option<&[PeerRelayStatus]>) -> String {
    let Some(v) = v else {
        return "\"peer_relays\":null".to_string();
    };
    let entries: Vec<String> = v
        .iter()
        .map(|p| {
            format!(
                "{{{},{}}}",
                jstr("pub_key_b64", &p.pub_key_b64),
                jstr("relay_url", &p.relay_url),
            )
        })
        .collect();
    format!("\"peer_relays\":[{}]", entries.join(","))
}

/// One selected-pair endpoint as `"key":{"type":...,"address":...,"port":...}`
/// (`type` = [`crate::ice::CandidateType::as_str`]: host/srflx/prflx/relay).
/// `None` renders the honest `null`: a pair that was NOT selected has no
/// endpoint, and the first collected candidate is never substituted for it
/// (the detail's own contract — `crate::peer_conn::PeerIceDetail`).
fn ice_endpoint_json(key: &str, v: Option<&IceCandidateEndpoint>) -> String {
    match v {
        Some(e) => format!(
            "\"{key}\":{{{},{},{}}}",
            jstr("type", e.typ.as_str()),
            jstr("address", &e.address),
            jnum("port", e.port as u64),
        ),
        None => format!("\"{key}\":null"),
    }
}

/// One per-peer ICE detail as a JSON object (public material only: public
/// WireGuard key, state token, role, the selected pair's endpoint types +
/// ip:port, the landing/reachability facts and the class-only last error).
///
/// No latency and no state-change timestamp: neither is retained anywhere in
/// the session/orchestrator (see `crate::peer_conn::PeerIceDetail` docs), so
/// inventing one here is exactly what the surface must not do. `reachable` is
/// carried VERBATIM from the detail (derived as `Connected && endpoint_applied`
/// in `crate::peer_conn`): a Disconnected peer that retains its selected
/// endpoint still reads `reachable:false` — the serializer never upgrades a
/// retained pair into a live connection.
fn ice_detail_json(d: &PeerIceDetail) -> String {
    format!(
        "{{{},{},{},{},{},{},{},{}}}",
        jstr("pub_key", &d.pub_key),
        jstr("state", d.state.as_str()),
        match d.controlling {
            Some(c) => jbool("controlling", c),
            None => "\"controlling\":null".to_string(),
        },
        ice_endpoint_json("selected_local", d.selected_local.as_ref()),
        ice_endpoint_json("selected_remote", d.selected_remote.as_ref()),
        jbool("endpoint_applied", d.endpoint_applied),
        jbool("reachable", d.reachable),
        format!(
            "\"last_error\":{}",
            d.last_error
                .as_ref()
                .map(ErrorClass::to_json)
                .unwrap_or_else(|| "null".to_string())
        ),
    )
}

/// The `ice_peers` tail: `null` (no ICE orchestrator at all — an empty array
/// would claim one exists), `[]` (orchestrator armed with ZERO peers) or one
/// object per peer, already public-key sorted by
/// [`crate::peer_conn::PeerIceOrchestrator::ice_details`].
fn ice_peers_json(v: Option<&[PeerIceDetail]>) -> String {
    let Some(v) = v else {
        return "\"ice_peers\":null".to_string();
    };
    let entries: Vec<String> = v.iter().map(ice_detail_json).collect();
    format!("\"ice_peers\":[{}]", entries.join(","))
}

/// 0.0.26 gap 1: the `ice_mux` tail — the shared mux's counters
/// ([`crate::ice_mux::MuxStats`]). `null` = no mux at all, or the mux state
/// lock was busy when the sample was taken (this document's own "unknown",
/// never a fabricated zero). Counters only: no fd, no address, no payload.
fn ice_mux_json(v: Option<crate::ice_mux::MuxStats>) -> String {
    let Some(s) = v else {
        return "\"ice_mux\":null".to_string();
    };
    format!(
        "\"ice_mux\":{{{}}}",
        [
            jnum("reads", s.reads),
            jnum("xor_hits", s.xor_hits),
            jnum("xor_misses", s.xor_misses),
            jnum("xor_expired", s.xor_expired),
            jnum("xor_filled", s.xor_filled),
            jnum("overlay_dropped", s.overlay_dropped),
            jnum("stun_unknown", s.stun_unknown),
            jnum("data_unknown", s.data_unknown),
            jnum("queue_dropped", s.queue_dropped),
        ]
        .join(",")
    )
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
    /// THE ACTUAL per-peer relay lane map (`null` = no carrier armed at all,
    /// `[]` = carrier armed with no lane attached): one
    /// [`PeerRelayStatus`] per attached lane, sorted by public key. Read
    /// from [`RelayCarrier::peer_relays`] — the live attached-lane table —
    /// NEVER from the remote advertisement map (`relay_ads`) or from a
    /// global home URL: an advertised or merely configured relay is not a
    /// path. A peer whose lane is attached but idle still has an entry
    /// (the lane exists); a peer with no entry has no relay lane of its own.
    pub peer_relays: Option<Vec<PeerRelayStatus>>,
    /// 2026-10-07 (per-peer ICE detail): the per-peer ICE DETAIL table
    /// (`null` = no ICE orchestrator at all, `[]` = orchestrator armed with
    /// ZERO peers, otherwise one entry per peer, public-key sorted) —
    /// [`crate::peer_conn::PeerIceDetail`]: state token, role, the selected
    /// pair's endpoint types + ip:port (`null` = not selected — never the
    /// first candidate), endpoint landing and reachability facts, class-only
    /// last error. Read from the SAME orchestrator lock as `ice` (one lock,
    /// one snapshot) and TAIL-appended after `peer_relays`; every pre-existing
    /// field keeps its position. Public material only: no ufrag/password, no
    /// fd, no token.
    pub ice_peers: Option<Vec<PeerIceDetail>>,
    /// 0.0.26 gap 1 (真机 0.0.25 §4.3): the shared ICE/WG mux's counters
    /// (`crate::ice_mux::MuxStats`), TAIL-appended after `ice_peers` — additive
    /// only, every pre-existing field keeps its position. `null` = no mux at
    /// all, or its state lock was busy at the sample instant (this document's
    /// own "unknown"; never a fabricated zero). The counters the device log
    /// could not carry before: `xor_hits`/`xor_misses` (srflx cache),
    /// `xor_filled`/`xor_expired`, `overlay_dropped`, `stun_unknown`,
    /// `data_unknown`, `queue_dropped`, `reads`. Read from the SAME
    /// orchestrator acquisition as `ice`/`ice_peers`, and deliberately NOT
    /// blocking: the shell's synchronous poll must never queue behind the mux
    /// reader. No peer key, fd or payload material.
    pub ice_mux: Option<crate::ice_mux::MuxStats>,
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
    /// THE connector-status assembly (production AND tests): refreshes the
    /// default-route gate against LIVE data-plane readiness, then reads the
    /// per-component summaries and the shared state under the documented
    /// lock order (`inner` first, then one leaf at a time — see the
    /// [`ConnectorShared`] field order). `peer_relays` is injected by the
    /// caller ([`ConnectorHandle::status`]) because it belongs to the relay
    /// carrier, which is read WITHOUT the shared lock held. `ice` is the ICE
    /// sub-snapshot the caller read with the NON-BLOCKING
    /// [`ConnectorShared::ice_status_sample`] — the summary and the per-peer
    /// details come from that ONE sample, so they always describe the same
    /// peer set (and a busy orchestrator never blocks this assembly; the
    /// sample carries its own `stale`/`age_ms`).
    fn snapshot(
        shared: &ConnectorShared,
        wg: &dyn WgPeerApplier,
        peer_relays: Option<Vec<PeerRelayStatus>>,
        ice: &IceStatusSample,
    ) -> ConnectorStatus {
        shared.refresh_net_gate(wg);
        let g = shared.lock();
        let running = shared.is_running();
        let state = g.machine.state();
        // The frozen `ice` summary and the per-peer `ice_peers` details come
        // from the SAME [`IceStatusSample`] the caller read under a single
        // orchestrator acquisition (see `ice_status_sample`): two separate
        // acquisitions could observe a different peer set / selected pair in
        // between, so the counts and the details would describe two different
        // snapshots. No orchestrator guard is taken here at all, so this read
        // can never queue behind the ICE pump's gather.
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
            wg_apply_failed: g.wg_apply_failed,
            wg_apply_errors: g.wg_apply_errors,
            logout_ok: g.logout_ok,
            terminal: is_terminal_state(running, state),
            ice: ice.ice.clone(),
            signal: shared.signal.get().map(|rt| rt.status()).unwrap_or_default(),
            wg: wg.dataplane_status().unwrap_or_default(),
            recreate: g.recreate.clone(),
            relay: shared.relay_status(),
            peer_relays,
            ice_peers: ice.ice_peers.clone(),
            // 0.0.26 gap 1: the mux counters ride the SAME sample as `ice` /
            // `ice_peers` (one orchestrator acquisition; the mux leaf read is
            // non-blocking — `None` = no mux or a busy mux lock).
            ice_mux: ice.mux,
        }
    }

    /// The `connector_status()` JSON document. Count/class-only fields —
    /// no secret material, no server messages (module discipline).
    pub fn to_json(&self) -> String {
        format!(
            "{{{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}}}",
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
            // 2026-10-06 (netbird status -d): the ACTUAL per-peer relay lane
            // map, TAIL-appended after the frozen `relay` object — additive
            // only, every pre-existing field keeps its position. Sourced
            // from the live attached-lane table, never from advertisements.
            peer_relays_json(self.peer_relays.as_deref()),
            // 2026-10-07 (per-peer ICE detail): the per-peer ICE table,
            // TAIL-appended after `peer_relays` — additive only, every
            // pre-existing field keeps its position. Same three-state shape:
            // `null` = no orchestrator, `[]` = orchestrator with zero peers.
            ice_peers_json(self.ice_peers.as_deref()),
            // 0.0.26 gap 1: the shared-mux counters, TAIL-appended after
            // `ice_peers` — additive only. `null` = no mux / busy mux lock.
            ice_mux_json(self.ice_mux),
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
    /// Peer FQDN (`PeerInfo.fqdn`, e.g. "host-1.netbird.cloud") — the UI's
    /// peer NAME. `None` when management sent none. Public material.
    pub fqdn: Option<String>,
    /// UI phase 1 (2026-10-06, peer online semantics): TRUE when management
    /// listed the peer in `NetworkMap.remotePeers` (online per management —
    /// it has a recent signal/management presence), FALSE when it came from
    /// `offlinePeers`. This is the management-side online verdict, kept
    /// SEPARATE from the local WG session state (`connector_status().wg`):
    /// a management-online peer without a local session is 空闲/idle, NOT
    /// offline — official-client peer-list semantics.
    pub mgmt_online: bool,
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
    /// Own peer FQDN (`PeerConfig.fqdn`, e.g. "me.netbird.cloud") — the Home
    /// screen's domain line. `None` when management sent none. Public material.
    pub fqdn: Option<String>,
    /// The tunnel MTU management ANNOUNCED (`PeerConfig.mtu`, field 7) —
    /// upstream's `selectMTU` second input (`client/internal/connect.go:740`
    /// @67c2041: local config MTU wins, else this, else `iface.DefaultMTU`).
    /// Exported only when it is USABLE on this platform
    /// (`config::MTU_MIN..=config::MTU_MAX`): the shell hands the value to
    /// `VpnConfig.mtu`, which the platform rejects outside [576,1500]
    /// (device-verified create-401, 2026-09-15) — an unusable announcement is
    /// therefore NOT the shell-applicable subset and stays out of the document.
    /// `None` = nothing usable announced (the official server never populates
    /// the field: `toPeerConfig` in
    /// management/internals/shared/grpc/conversion.go:121 leaves it 0), so the
    /// shell keeps the official default 1280.
    pub mtu: Option<u16>,
}

/// Canonical dotted-quad/prefix rendering of a parsed route network.
fn route_network_string(route: &config::Route) -> String {
    format!(
        "{}.{}.{}.{}/{}",
        route.addr[0], route.addr[1], route.addr[2], route.addr[3], route.prefix_len
    )
}

/// Render the `connector_routes_json()` document (decision B) from the
/// generation's selection + the UNFILTERED advertised catalog:
///
/// ```json
/// {
///   "available": true,
///   "serial": 42,
///   "selection": {"selected_routes":[],"deselected_routes":["HostInternal"],"deselect_all":false},
///   "networks": [
///     {"net_id":"HostInternal","networks":["192.168.50.0/24"],
///      "candidate_count":2,"enabled":false,"is_exit":false}
///   ]
/// }
/// ```
///
/// `available` means "a NetworkMap was APPLIED" (the map-seen state), NOT
/// "the applied map advertised routes": it is false only before the first map,
/// after stop and with no connector; an applied map that advertised NO routes
/// is a real connected-but-empty catalog — `available:true` with the map's own
/// `serial`, the generation's `selection` and `networks:[]`. It is about the
/// CATALOG, not the selection — with `deselect_all` (or every NetID
/// deselected) the catalog is still `available:true` and every row simply
/// carries `enabled:false`. Fixed key order, existing escaping helpers, no
/// secrets (NetIDs/prefixes are public routing material).
fn routes_document(selection: &RouteSelector, advertised: Option<&AdvertisedRoutes>) -> String {
    let routes: &[crate::network_map::ManagedRoute] =
        advertised.map(|a| a.routes.as_slice()).unwrap_or(&[]);
    // The map-seen `Option` IS the availability state: `Some` = the map was
    // applied (even with zero advertised routes — the UI must be able to tell
    // "connected, no routes" from "no snapshot"); `None` = no catalog yet.
    let available = advertised.is_some();
    // `available:false` renders `serial:0` (no map ⇒ no serial); an applied
    // empty map renders its OWN serial.
    let serial = advertised.map(|a| a.serial).unwrap_or(0);
    let networks = selection.logical_networks(routes);

    let mut out = String::from("{");
    out.push_str(&jbool("available", available));
    out.push(',');
    out.push_str(&jnum("serial", serial));
    out.push_str(",\"selection\":");
    out.push_str(&selection.to_json());
    out.push_str(",\"networks\":[");
    for (index, net) in networks.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"net_id\":");
        out.push_str(&format!("\"{}\"", crate::util::json_escape(net.net_id)));
        out.push_str(",\"networks\":[");
        for (prefix_index, prefix) in net.prefixes.iter().enumerate() {
            if prefix_index > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "\"{}\"",
                crate::util::json_escape(&route_network_string(prefix))
            ));
        }
        out.push_str("],");
        out.push_str(&jnum("candidate_count", net.candidate_count as u64));
        out.push(',');
        out.push_str(&jbool("enabled", net.enabled));
        out.push(',');
        out.push_str(&jbool("is_exit", net.is_exit));
        out.push('}');
    }
    out.push_str("]}");
    out
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
            .map(|p| (p, true))
            .chain(map.offline_peers.iter().map(|p| (p, false)))
            .map(|(p, mgmt_online)| ShellPeerSummary {
                pub_key_b64: p.wg_pub_key.clone(),
                allowed_ips: p.allowed_ips.len(),
                vpn_addresses: p
                    .allowed_ips
                    .iter()
                    .map(route_network_string)
                    .collect(),
                fqdn: p.fqdn.clone(),
                mgmt_online,
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
            fqdn: map.peer.as_ref().and_then(|p| p.fqdn.clone()),
            // Upstream's per-map MTU announcement (`PeerConfig.mtu`), gated to
            // the envelope this platform's `VpnConfig.mtu` accepts — see the
            // field doc. `map.peer.mtu` is already normalised to `None` for the
            // unset/0 wire value.
            mtu: map
                .peer
                .as_ref()
                .and_then(|p| p.mtu)
                .filter(|m| (config::MTU_MIN as i32..=config::MTU_MAX as i32).contains(m))
                .map(|m| m as u16),
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
            // Tail-append discipline (UI phase 1): `fqdn` comes AFTER the
            // pre-existing fields — the entry head keeps its order/shape.
            let fqdn = match p.fqdn.as_ref() {
                Some(f) => jstr("fqdn", f),
                None => "\"fqdn\":null".to_string(),
            };
            // 2026-10-06 peer-online semantics: `mgmt_online` tail-appends
            // AFTER `fqdn` (same discipline) — the management-side online
            // verdict, read leniently by the shell (missing = remote).
            peers.push_str(&format!(
                "{{{},{},\"vpn_addresses\":[{}],{},\"mgmt_online\":{}}}",
                jstr("pub_key", &p.pub_key_b64),
                jnum("allowed_ips", p.allowed_ips as u64),
                vpn,
                fqdn,
                p.mgmt_online
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
        // Own FQDN (UI phase 1): the Home screen's domain line. Tail-appended
        // AFTER `stuns` — every pre-existing field keeps its order and shape.
        let fqdn = match self.fqdn.as_ref() {
            Some(f) => jstr("fqdn", f),
            None => "\"fqdn\":null".to_string(),
        };
        // Upstream MTU alignment (2026-10-12): the announced `PeerConfig.mtu`,
        // TAIL-appended after every pre-existing field (same a1947b9 discipline
        // — `fqdn` stays directly before it). `null` = nothing usable was
        // announced, which is the official server's normal answer — the shell
        // then keeps the official default 1280.
        let mtu = match self.mtu {
            Some(m) => jnum("mtu", m as u64),
            None => "\"mtu\":null".to_string(),
        };
        format!(
            "{{{},{},{},{},{},{},\"routes\":[{}],\"dns\":{{{},{}}},{},\"peers\":[{}],\"default_route\":{{{},{}}},\"stuns\":[{}],{},{}}}",
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
            fqdn,
            mtu,
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
    /// UI phase 1 (2026-10-06, 详情页中继地址): the effective relay URL of
    /// the live (or last live) session, `rel://`/`rels://` form — the
    /// address every relayed peer's carrier traffic actually flows through
    /// (one live session serves all relayed peers). `None` while nothing
    /// was ever dialed. Public routing material, same class as `urls`.
    pub current_url: Option<String>,
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
            current_url: None,
        }
    }

    /// JSON object (embedded into the status document). FROZEN SHAPE: the
    /// `relay` object carries the RUNTIME view only — the advertised view
    /// renders separately via [`RelayStatus::advertised_to_json`] so an
    /// advertised url can never be misread as a relay session in use.
    /// (2026-10-06 additive exception, a1947b9 tail-append discipline:
    /// `current_url` rides AFTER `last_error_class` — the one field added
    /// since the freeze; every pre-existing field keeps order and shape.)
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
        // 2026-10-06: `current_url` tail-appends AFTER `last_error_class`
        // (a1947b9 tail-append discipline — every pre-existing field keeps
        // its order and shape; consumers read by name).
        let current_url = match self.current_url.as_ref() {
            Some(u) => jstr("current_url", u),
            None => "\"current_url\":null".to_string(),
        };
        format!(
            "{{{},{},{},{},{},{},{},{},{},{}}}",
            jbool("enabled", self.enabled),
            jstr("state", &self.state),
            format!("\"urls\":[{urls}]"),
            jnum("reconnects", self.reconnects),
            jnum("frames_tx", self.frames_tx),
            jnum("frames_rx", self.frames_rx),
            jnum("transport_bytes", self.transport_bytes),
            jbool("token_valid", self.token_valid),
            format!("\"last_error_class\":{last_error}"),
            current_url,
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
    /// N13-D3: the connector's per-peer advertised-relay map
    /// ([`ConnectorShared::relay_ads`]) — the receive path records every
    /// OFFER/ANSWER `relayServerAddress` here, and the relay carrier reads
    /// the SAME map for its per-peer relay-server choice.
    ads: Arc<Mutex<HashMap<String, String>>>,
}

impl SignalRuntime {
    fn new(
        materials: SignalLinkMaterials,
        exchange: Arc<RealSignalExchange>,
        outbox: tokio::sync::mpsc::UnboundedReceiver<SignalOutgoing>,
        ads: Arc<Mutex<HashMap<String, String>>>,
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
            ads,
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
        let ads_cb = self.ads.clone();
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
            SignalLinkEvent::Message(m) => route_signal_message(&orch_cb, &ads_cb, &m),
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
fn route_signal_message(
    orch: &Mutex<PeerIceOrchestrator>,
    ads: &Mutex<HashMap<String, String>>,
    m: &SignalMessage,
) {
    use crate::signal::proto::body::Type;
    let kind = match m.kind {
        Type::Offer => PeerSignalKind::Offer,
        Type::Answer => PeerSignalKind::Answer,
        Type::Candidate => PeerSignalKind::Candidate,
        _ => return,
    };
    // N13-D3 ordering: the advertisement is recorded BEFORE the frame reaches
    // the ICE orchestrator. Upstream stamps `relayServerAddress` into every
    // OFFER and ANSWER (handshaker.go:211,218) and a receiver opens its relay
    // lane toward us on the strength of a non-empty value
    // (worker_relay.go:122-127 → :69); CANDIDATE/HEARTBEAT frames never carry
    // it. Recording first means the lane-decision input can never lag the
    // frame that announced it; the lane itself is still opened (and, for an
    // already attached lane, re-decided — see `spawn_pending_lanes`) on the
    // carrier's own clock, so an advertisement that arrives after the first
    // lane attempt moves the lane instead of being lost.
    if matches!(kind, PeerSignalKind::Offer | PeerSignalKind::Answer) {
        let advertised = m.relay_server_address.as_deref().map(str::trim).filter(|u| !u.is_empty());
        if let Some(url) = advertised {
            let previous = ads
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(m.from_key.clone(), url.to_string());
            if previous.as_deref() != Some(url) {
                hilog::emit(&format!(
                    "connector: relay advertisement recorded|peer={}|url={}",
                    crate::relay::PeerId::from_wg_pubkey_string(&m.from_key),
                    url
                ));
            }
        }
    }
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

/// Everything the connection worker needs (owned `Arc`s and clones; no
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

/// A started connector: owns the connection worker task and exposes
/// `status()` / `stop()`. `stop()` is idempotent; a stopped (or
/// fatal-failed) slot is replaced by the next `connector_start`.
pub struct ConnectorHandle {
    shared: Arc<ConnectorShared>,
    wg: Arc<dyn WgPeerApplier>,
    host: Arc<dyn ConfigApplier>,
    /// Network-change generation watched by the management worker.
    network_change: tokio::sync::watch::Sender<u64>,
    /// 006: this handle's START GENERATION — the value of the process-global
    /// successful-start counter taken at spawn, so two starts in one process
    /// never share it. It is an IDENTITY, not a resource measure: it is NOT a
    /// live task/thread count and `generation - 1` says nothing about what is
    /// still running (see [`connector_resource_status_json`]).
    generation: u64,
    management_factory: Arc<dyn ManagementFactory>,
    /// Latest logged-in management client. Held for the N2 A-trigger M
    /// column ([`ConnectorATrigger::management_round_trip`], frozen design
    /// §4 M: ONE `GetServerKey` on the SAME tonic channel the Sync stream
    /// runs over) and for the EXPLICIT user logout
    /// ([`ConnectorHandle::logout_json`]). NEVER consumed by `stop()` —
    /// stopping a connection is not deregistration.
    logout_slot: Arc<Mutex<Option<ManagementGrpcClient>>>,
    stop_flag: Arc<AtomicBool>,
    /// N3-7: the protected-socket source behind the management factory, when
    /// the connector was started through `connector_start_with_socket`.
    /// `connector_socket_feed` resupplies it across the NAPI boundary.
    socket_source: Option<Arc<ProtectedSocketFdSource>>,
    /// N5c: the protected-UDP source behind the ICE orchestrator. `None` =
    /// caller-supplied orchestrator (tests).
    ///
    /// STAGE 4 (official-alignment §2.7): the DEVICE shell no longer feeds it —
    /// the ICE plane takes no per-session socket, and the ONE shared mux is
    /// armed over the WG outer socket (`connector_ice_mux_arm`). The source stays
    /// as the protected-DNS resolver for STUN hostnames and as the HOST
    /// harness's mux supply (`host_sockets::feed_ice`).
    ice_sockets: Option<Arc<crate::ice::ProtectedUdpFdSource>>,
    /// STAGE 2/3: the ONE shared mux slot the orchestrator reads. The shell
    /// arms it OVER the WG device's own protected socket (`connector_ice_mux_arm`
    /// → `arm_mux_from_wg_socket`); host/test paths let the orchestrator build it
    /// from `ice_sockets` (or arm it explicitly). `None` = caller-supplied
    /// orchestrator without a mux slot (tests).
    ice_mux: Option<crate::peer_conn::IceMuxSlot>,
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
    /// Build deps + spawn the connection worker on
    /// `runtime`. Returns immediately; progress is observed via `status()`.
    /// N3-7: `force_default_route` arms the default-route gate override;
    /// `socket_source` (when set) is the resupply handle for
    /// `connector_socket_feed`. N5c: `ice = None` builds the production
    /// orchestrator (SystemInterfaces + a fresh protected-UDP source that
    /// resolves STUN hostnames through the protected DNS supply, and on the
    /// HOST supplies the ONE shared mux) plus the same WG seam, and starts the
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
    ///
    /// No startup route selection (default all-selected) — the start seams use
    /// [`ConnectorHandle::spawn_with_relay_selected`]; existing callers keep
    /// their exact behavior.
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
        force_default_route: bool,
        socket_source: Option<Arc<ProtectedSocketFdSource>>,
        ice: Option<Arc<Mutex<PeerIceOrchestrator>>>,
        signal_material: Option<SignalMaterial>,
        wg_feed: Option<Arc<crate::wg_device::WgDeviceFeed>>,
        ice_tuning: HostIceTuning,
        relay_enabled: bool,
        relay_attach: Option<RelayAttach>,
    ) -> Arc<ConnectorHandle> {
        Self::spawn_with_relay_selected(
            runtime,
            factory,
            wg,
            host,
            secrets,
            meta,
            policy_backoff,
            sync_policy,
            force_default_route,
            socket_source,
            ice,
            signal_material,
            wg_feed,
            ice_tuning,
            relay_enabled,
            relay_attach,
            RouteSelector::new(),
        )
    }

    /// [`ConnectorHandle::spawn_with_relay`] plus the generation's startup
    /// route selection (`ConnectorConfig::route_selection`). The
    /// selection was validated with the config and is stored on the connector
    /// BEFORE the worker can run its first Sync; since T0 e15ac it is the START
    /// seed of a state the live setter (`connector_route_selection_set_json`)
    /// may replace at runtime (serialized with the map route pipeline). Not
    /// part of the crate's public surface — only the start seams in this module
    /// call it.
    #[allow(clippy::too_many_arguments)]
    fn spawn_with_relay_selected(
        runtime: tokio::runtime::Handle,
        factory: Arc<dyn ManagementFactory>,
        wg: Arc<dyn WgPeerApplier>,
        host: Arc<dyn ConfigApplier>,
        secrets: ConnectorSecrets,
        meta: PeerMeta,
        policy_backoff: ExponentialBackoff,
        sync_policy: SyncPolicy,
        force_default_route: bool,
        socket_source: Option<Arc<ProtectedSocketFdSource>>,
        ice: Option<Arc<Mutex<PeerIceOrchestrator>>>,
        signal_material: Option<SignalMaterial>,
        wg_feed: Option<Arc<crate::wg_device::WgDeviceFeed>>,
        ice_tuning: HostIceTuning,
        relay_enabled: bool,
        relay_attach: Option<RelayAttach>,
        route_selection: RouteSelector,
    ) -> Arc<ConnectorHandle> {
        // The flag is authoritative: materials without the opt-in are
        // dropped (the device default keeps EVERY relay seam detached).
        let relay_attach = relay_enabled.then_some(relay_attach).flatten();
        // N2-H: the session's INTERNAL management dial address, read BEFORE
        // the factory is moved into the worker task.
        let management_dial = factory.dial_address();
        // 006: successful-start identity + cumulative start counter. Taken
        // BEFORE anything can fail so a spawn that reaches this line is one
        // start; the counter is process-global and outlives the handle.
        let generation = STARTS_TOTAL.fetch_add(1, Ordering::AcqRel) + 1;
        let shared =
            Arc::new(ConnectorShared::new_selected(force_default_route, relay_attach, route_selection));
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
        // stop flag as the worker. N5d: the production build swaps the
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
        let ice_mux: Option<crate::peer_conn::IceMuxSlot>;
        let ice = match ice {
            Some(orch) => {
                ice_sockets = None;
                ice_mux = None;
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
                                shared.relay_ads.clone(),
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
                // STAGE 2: ONE shared ICE mux slot for the whole ICE plane.
                // The shell fills it (protect + feed) on device; host runs arm
                // it lazily from `socks` (see `PeerIceOrchestrator::arm_mux`).
                let mux_slot: crate::peer_conn::IceMuxSlot =
                    std::sync::Arc::new(std::sync::Mutex::new(None));
                orch.lock_poison().set_ice_mux(mux_slot.clone());
                ice_mux = Some(mux_slot);
                orch
            }
        };
        let _ = shared.ice.set(ice.clone());
        // N5c glare fix: the ICE orchestrator decides controlling/controlled
        // by the official isController rule (public-key comparison). The
        // signal materials ARE the WG keypair (the same `config.keys` whose
        // `public_key_base64()` is the relay peerID), so inject it whenever
        // present — covers both the freshly built and the injected
        // orchestrator, before any session can form.
        if let Some(mat) = signal_material.as_ref() {
            ice.lock_poison().set_local_key(mat.keys.public_key_base64());
        }
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

        Arc::new(ConnectorHandle {
            management_factory: worker_factory,
            network_change,
            generation,
            shared,
            wg,
            host,
            logout_slot,
            stop_flag,
            socket_source,
            ice_sockets,
            ice_mux,
            signal_sockets,
            wg_feed,
            ice,
            management_dial,
            worker: Mutex::new(Some(worker)),
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
    ///
    /// 2026-10-06: `peer_relays` (the ACTUAL per-peer relay lane map) is read
    /// HERE, from the live lane table, and carried into the snapshot — the
    /// carrier's `attached` leaf lock is taken with NO other carrier or shared
    /// lock held (lock-order discipline), before [`ConnectorShared`] is
    /// locked. `None` = no carrier armed.
    pub fn status(&self) -> ConnectorStatus {
        let ice = self.shared.ice_status_sample();
        self.status_with_ice(&ice)
    }

    /// [`ConnectorHandle::status`] with the ICE sub-snapshot ALREADY read:
    /// [`ConnectorHandle::status_json`] needs the same sample for the
    /// `ice_sockets` tail (the freshness marks), and the
    /// frozen `ice`/`ice_peers` fields must describe that very sample.
    fn status_with_ice(&self, ice: &IceStatusSample) -> ConnectorStatus {
        let peer_relays =
            self.shared.relay_carrier.get().map(|carrier| carrier.peer_relays());
        ConnectorStatus::snapshot(&self.shared, self.wg.as_ref(), peer_relays, ice)
    }

    /// The `connector_status()` JSON document.
    pub fn status_json(&self) -> String {
        // 2026-10-09 (ArkTS main-thread freeze fix): ONE non-blocking ICE read
        // ([`ConnectorShared::ice_status_sample`]) supplies the frozen
        // `ice`/`ice_peers` fields. The pre-fix path took the orchestrator lock
        // repeatedly (summary, details, demand) and each acquisition could queue
        // behind the ICE pump's 8–11 s gather, freezing the shell's 5 s status
        // poll.
        let ice = self.shared.ice_status_sample();
        let base = self.status_with_ice(&ice).to_json();
        // N5c socket-depth observability (HOST-CLI FEED): expose the remaining
        // depth of the protected UDP source so a FEEDER can read the real value.
        // A feeder that only learns the depth from its own feed calls keeps a
        // stale "still queued" reading and stops resupplying exactly when the
        // queue is empty — device run 5 and the host CLI both sat in endless
        // "protected-udp: no-protected-socket" that way. -1 = no source attached.
        // The device shell no longer feeds this queue (stage 4: the ICE plane
        // rides the ONE shared mux over the WG socket), so on device it renders
        // zeros; the count stays because it is the frozen document's own field
        // and it is still the host harness's supply surface.
        let (queued, taken) = match self.shared.ice_sockets.get() {
            Some(s) => (s.pending() as i64, s.taken() as i64),
            None => (-1, -1),
        };
        with_ice_sockets_section(
            &base,
            queued,
            taken,
            ice.stale,
            ice.age_ms,
        )
    }

    /// 006: fill the connector-owned half of the resource snapshot with
    /// NON-BLOCKING probes — one leaf at a time, each guard released before
    /// the next leaf is taken, no await anywhere. Deliberately does NOT touch
    /// the WG seam: `WgPeerApplier::dataplane_status()` takes the device's
    /// shared inner lock with a blocking lock (and copies per-peer rows), so
    /// it can contend with the data-plane pump — see the `wg` section note in
    /// [`connector_resource_status_json`].
    fn fill_resource_probe(&self, snap: &mut ResourceSnapshot) {
        // leaf 1: shared state — the state token ONLY (no gate refresh, no
        // side effect; `connector_status()` remains the gate-advancing read).
        snap.state = try_lock_poison(&self.shared.inner).map(|g| g.machine.state().as_str());
        // leaf 2: ICE orchestrator — scalar counts only.
        match self.shared.ice.get() {
            Some(orch) => {
                snap.ice_available = true;
                snap.ice = try_lock_poison(orch).map(|g| g.resource_counts());
            }
            // No orchestrator object at all (unit-test construction): the
            // counts are UNMEASURED, not zero.
            None => snap.ice_available = false,
        }
        // leaf 3: the protected-UDP source. `taken` is an atomic; `pending` is
        // read with the source's own NON-BLOCKING try_lock so a shell feed /
        // gather holding the queue can never stall this diagnostic: a busy
        // queue renders `queued:null` (unknown) while `taken` stays measured.
        match self.shared.ice_sockets.get() {
            Some(source) => {
                snap.ice_sockets_available = true;
                snap.ice_sockets = Some((source.pending_try(), source.taken()));
            }
            // No shell-fed source attached: the queue depth is UNMEASURED
            // (the frozen `ice_sockets` section renders -1 for the same fact).
            None => snap.ice_sockets_available = false,
        }
        // leaf 4..6: the relay carrier's three leaves, taken ONE AT A TIME
        // (each `*_try` releases its guard before returning).
        match self.shared.relay_carrier.get() {
            Some(carrier) => {
                snap.relay_available = true;
                snap.relay_lanes = carrier.lane_count_try();
                snap.relay_pending_opens = carrier.pending_open_count_try();
                snap.relay_foreign = carrier.foreign_count_try();
            }
            // No carrier armed: nothing can be attached, opened or dialed —
            // a KNOWN zero.
            None => {
                snap.relay_available = false;
                snap.relay_lanes = Some(0);
                snap.relay_pending_opens = Some(0);
                snap.relay_foreign = Some(0);
            }
        }
    }

    /// The `connector_network_config()` JSON document (read-only snapshot of
    /// the last applied NetworkMap — the parts the shell applies platform
    /// side; `no-network-map` before the first map). N7: the default-route
    /// gate is re-decided against LIVE data-plane readiness at read time.
    pub fn network_config_json(&self) -> String {
        self.shared.refresh_net_gate(self.wg.as_ref());
        self.shared.network_config_json()
    }

    /// The `connector_routes_json()` document for THIS connector (decision B):
    /// the UNFILTERED advertised route catalog grouped per NetID + the startup
    /// selection. Side-effect-free (plain mutex snapshot) — no gate refresh,
    /// no recreate bookkeeping, no IO.
    fn routes_json(&self) -> String {
        self.shared.routes_json()
    }

    /// Stop the connector: close the Sync stream (worker abort), clear the
    /// data-plane seams. Idempotent: a second call reports `already_stopped`
    /// and does nothing.
    ///
    /// NO management `Logout` is sent on ANY stop path (user disconnect,
    /// automatic fail-closed, extension destroy, reconnect failure): the
    /// Logout RPC deregisters and DELETES the peer server-side (upstream
    /// client/server/server.go), so a stop-time Logout turned automatic
    /// disconnects into device deletion (device audit 2026-10-01: a
    /// fail-closed recreate at 01:34:08 ended with the management System
    /// deleting the peer). Upstream clients call Logout ONLY on the user's
    /// explicit logout — that lives in [`ConnectorHandle::logout_json`].
    pub fn stop(&self) -> StopOutcome {
        let worker = self.worker.lock_poison().take();
        let already = worker.is_none();
        if already {
            return StopOutcome { already_stopped: true, state: self.shared.state() };
        }
        // 006: a REAL stop transition is counted once (an idempotent repeat
        // above returns before this). Cumulative, not a live-resource count.
        STOPS_TOTAL.fetch_add(1, Ordering::AcqRel);
        self.stop_flag.store(true, Ordering::Release);
        self.shared.set_running(false);
        if let Some(h) = worker {
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
        // STAGE 3: the ONE shared socket goes with the sessions it served — the
        // mux (and with it its reader thread and its own dup) is released here,
        // after EVERY session has unregistered, so a stopped connector leaves no
        // reader thread and no fd behind. The fd closes exactly once (the mux's
        // own owned dup); the WG device's and the shell's descriptors are theirs.
        self.ice.lock_poison().drop_mux();
        self.wg.clear();
        self.host.clear();
        {
            let mut g = self.shared.lock();
            g.net_config = None;
            g.net_routes_all.clear();
            // decision B: the advertised catalog goes with the applied config
            // — the stop document must not keep offering routes of a session
            // that no longer exists (the NEXT start carries its own selection).
            g.advertised_routes = None;
            // N8: the applied-set bookkeeping goes with the applied config —
            // a fresh start begins with no recreate history
            g.applied_route_set = None;
            g.last_recreate_ms = None;
            g.recreate = RecreateStatus::default();
        }
        // NO Logout here: stopping a connection is not deregistration (see
        // the method doc). The logged-in client stays in `logout_slot` for
        // the N2 A-trigger M column and the explicit `connector_logout()`.
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

    /// The `connector_logout()` JSON document: ONE explicit user logout —
    /// send the management `Logout` RPC on the CURRENT logged-in client
    /// (from `logout_slot`: the same tonic channel the Sync stream runs
    /// over, no new dial, no new stream) and report the sanitized outcome.
    /// This is the ONLY Logout call in the client — `stop()` never logs
    /// out (stopping a connection is not deregistration; see
    /// [`ConnectorHandle::stop`]).
    ///
    /// The client is TAKEN from the slot (one shot): a Logout deletes the
    /// peer server-side, so the channel is spent either way and a repeat
    /// call reports `no-live-management-connection` instead of firing a
    /// second RPC. Bounded by [`EXPLICIT_LOGOUT_WAIT_MS`] even though each
    /// RPC already carries the client-wide `request_timeout` — the caller
    /// (the shell, on its watcher tick) must not be frozen longer.
    ///
    /// `NotFound` counts as success ("peer already gone" — upstream
    /// logoutPeerGone, client/server/server.go:1602-1611). The outcome is
    /// recorded in the status field `logout_ok` (which `stop()` leaves
    /// untouched at `null`).
    pub fn logout_json(&self) -> String {
        let Some(mut client) = self.logout_slot.lock_poison().take() else {
            return format!(
                "{{{},{},{}}}",
                jbool("ok", false),
                jstr("error", "no-live-management-connection"),
                jstr("reason", "login-not-completed-or-already-logged-out"),
            );
        };
        let waited = self.runtime.block_on(async {
            tokio::time::timeout(
                core::time::Duration::from_millis(EXPLICIT_LOGOUT_WAIT_MS),
                client.logout(),
            )
            .await
        });
        let (ok, reason) = match waited {
            Ok(Ok(())) => (true, "ok"),
            // upstream logoutPeerGone: NotFound = the peer is already gone
            // (tonic Code::NotFound = 5 → ManagementError::Request)
            Ok(Err(e))
                if matches!(ErrorClass::from_management(&e), ErrorClass::Request { status: 5 }) =>
            {
                (true, "peer-already-gone")
            }
            Ok(Err(e)) => (false, ErrorClass::from_management(&e).as_str()),
            Err(_) => (false, "timeout"),
        };
        self.shared.set_logout_ok(ok);
        hilog::emit(&format!("connector: explicit logout ok={ok} reason={reason}"));
        format!("{{{},{}}}", jbool("ok", ok), jstr("reason", reason))
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
/// clock-injected and never sleeps — this loop only FEEDS it). 100 ms → 10 ms
/// (100 Hz): one session's per-run receive budget is spent once per run, so
/// its receive rate is budget × (1 / tick). This loop's idle cost is one
/// wakeup per tick, each running the injected-clock pump.
pub const ICE_PUMP_TICK_MS: u64 = 10;

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

/// 2026-10-12 (account login sequencing): ONE management `Login` per call,
/// carrying the peer's persistent WireGuard public key and — in setup-key
/// mode — the setup key's `Register` second step.
///
/// The official model (upstream `client/internal/connect.go:774-796`
/// `loginToManagement`; `client/internal/auth/auth.go:208-236`, `:327-355`,
/// `:491-521`; `client/internal/state.go:15`; `client/android/client.go:205`):
///
/// * a connect sends `Login` with **no JWT and no setup key** — the peer is
///   identified by its persistent WireGuard public key
///   (`shared/management/client/grpc.go:650-656`), which the management server
///   accepts forever for a registered peer;
/// * a management JWT is a **one-shot identity proof** consumed at
///   (re)registration: it belongs to the interactive login action that runs
///   BEFORE the engine starts (`client/android/client.go:194-215`
///   `auth.login(urlOpener, …)` → `auth.go:184-236`), which in this shell is
///   the UI-process [`crate::account_auth`] login-action NAPI. A connect never
///   sees a token, so there is nothing to persist and nothing to replay
///   (the old shape persisted it and died on the second connect with
///   `status 16: JWT already used`);
/// * account mode (empty `setup_key`): a login-needed verdict
///   (`PermissionDenied`/`InvalidArgument` = upstream `isLoginNeeded`,
///   auth.go:491-521 — the codes the server uses for "peer login has expired"
///   and "no peer auth method provided") means the peer needs the interactive
///   login again: [`ManagementError::NeedsLogin`], upstream `StatusNeedsLogin`
///   (`connect.go:373-377`), never the generic permanent `Auth`;
/// * setup-key mode: the key-less `Login` runs FIRST and the setup key goes to
///   the second step only, exactly like upstream `Auth.Login` →
///   `doMgmLogin` → `registerPeer` on a login-needed verdict
///   (auth.go:208-236, :336-355; the wire call is
///   [`ManagementGrpcClient::register`], grpc.go:641-648). The old shape put
///   the key inside `Login` — the server tolerates it, but it is not the
///   official client's wire shape.
///
/// Returns the login verdict.
async fn login_sequence(
    client: &mut ManagementGrpcClient,
    secrets: &ConnectorSecrets,
    meta: &PeerMeta,
) -> Result<LoginOutcome, ManagementError> {
    let params = |setup_key: &str| LoginParams {
        setup_key: setup_key.to_string(),
        jwt_token: String::new(),
        meta: meta.clone(),
        ..Default::default()
    };
    let setup_key: &str = &secrets.setup_key;
    // the device-log attempt token: a setup-key profile's FIRST step is the
    // key-less login, an account connect's only step is the token-less login
    let keyless_attempt: &str = if setup_key.is_empty() { "login" } else { "login-keyless" };
    match client.login(params("")).await {
        Ok(outcome) => {
            emit_login_attempt(keyless_attempt, Ok(&outcome), secrets);
            Ok(outcome)
        }
        Err(err) if is_login_needed(&err) => {
            if setup_key.is_empty() {
                // Account mode: the interactive login (with the JWT) is the
                // ONLY thing that can register this peer. Park on the
                // needs-login state, exactly like upstream's connect loop.
                let needed = needs_login(&err);
                emit_login_attempt("login", Err(&needed), secrets);
                Err(needed)
            } else {
                let verdict = client.register(params(setup_key)).await;
                emit_login_attempt("register-setup-key", verdict.as_ref(), secrets);
                verdict
            }
        }
        Err(err) => {
            emit_login_attempt(keyless_attempt, Err(&err), secrets);
            Err(err)
        }
    }
}

/// Re-class one management error as the needs-login state, keeping the numeric
/// status and the server message (the message never leaves the redacted
/// device-log channel — the status/error JSON surfaces stay class-only).
fn needs_login(err: &ManagementError) -> ManagementError {
    let message = match err {
        ManagementError::Auth { message, .. }
        | ManagementError::Request { message, .. }
        | ManagementError::NeedsLogin { message, .. } => message.clone(),
        _ => String::new(),
    };
    ManagementError::NeedsLogin { status: err.status_code(), message }
}

/// ONE device-log line per login attempt outcome (2026-10-11 A3). `attempt` is
/// a fixed token (`login` / `login-keyless` / `register-setup-key`) and `result`
/// the sanitized class token (`ok`, `needs-login`, `auth`, ...); the
/// parenthesized detail is the redacted server verdict — the setup key is never
/// printed (see [`redacted_login_detail`]), and neither is the peer's private
/// key. Logging must never change the login verdict, so this is observational
/// only.
fn emit_login_attempt(
    attempt: &str,
    result: Result<&LoginOutcome, &ManagementError>,
    secrets: &ConnectorSecrets,
) {
    match result {
        Ok(_) => hilog::emit_evidence(&format!("connector: login attempt={attempt} result=ok")),
        Err(e) => {
            let detail = redacted_login_detail(e, secrets);
            hilog::emit_evidence(&format!(
                "connector: login attempt={attempt} result={}{}",
                ErrorClass::from_management(e).as_str(),
                if detail.is_empty() { String::new() } else { format!(" ({detail})") }
            ));
        }
    }
}

/// The connection worker: connect → login → Sync session (internal
/// reconnect loop). Fatal (Auth / needs-login) and backoff
/// exhaustion both end the worker with the state machine parked in `Failed`;
/// `stop()` aborts the task for a clean `Disconnected`.
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
    // L436-438/L468-470 for the stream). 2026-10-12: the login itself is the
    // sequence in [`login_sequence`] (key-less Login; setup-key mode adds the
    // upstream `Register` second step; the interactive JWT step lives in the
    // UI-process login action, [`crate::account_auth`]).
    let mut login_backoff = policy_backoff;
    let mut login_rng = OsRandom;
    let login_clock = MonotonicClock;
    let client = loop {
        let attempt = async {
            let mut client = factory.connect().await?;
            let outcome = login_sequence(&mut client, &secrets, &meta).await;
            outcome.map(|outcome| (client, outcome))
        };
        match attempt.await {
            Ok((client, outcome)) => {
                // anchor the 3-state deadline from LoginResponse
                // (engine_authsession.go:17-62 semantics)
                shared.lock().deadline.apply_wire(outcome.session_deadline_unix);
                shared.remember_login_signal(&outcome.response);
                if outcome.peer_address.is_some() {
                    hilog::emit_evidence("connector: login ok (peer address assigned)");
                } else {
                    hilog::emit_evidence("connector: login ok");
                }
                *logout_slot.lock_poison() = Some(client.clone());
                break client;
            }
            Err(e) => {
                if matches!(e, ManagementError::NeedsLogin { .. }) {
                    // 2026-10-12: the peer needs a FRESH interactive login
                    // (upstream `StatusNeedsLogin`, connect.go:373-377).
                    // Permanent for THIS attempt — but its own class, so the
                    // shell shows the official label (`Login required` /
                    // 「需要登录」) instead of hammering a retry that cannot
                    // succeed.
                    shared.record_error(&e);
                    shared.transition(ConnEvent::FatalError);
                    shared.set_running(false);
                    hilog::emit_evidence(&format!(
                        "connector: login needs-login, giving up ({})",
                        redacted_login_detail(&e, &secrets)
                    ));
                    return;
                }
                if matches!(e, ManagementError::Auth { .. }) {
                    // Permanent: an invalid setup key / revoked peer will
                    // not heal by retrying.
                    shared.record_error(&e);
                    shared.transition(ConnEvent::FatalError);
                    shared.set_running(false);
                    // 2026-10-06: the device log carries the server's own
                    // verdict (status + redacted message) — the status/error
                    // surfaces stay class-only.
                    hilog::emit_evidence(&format!(
                        "connector: login fatal (auth), giving up ({})",
                        redacted_login_detail(&e, &secrets)
                    ));
                    return;
                }
                shared.record_error(&e);
                shared.count_retryable_failure();
                shared.transition(ConnEvent::Lost);
                let detail = redacted_login_detail(&e, &secrets);
                hilog::emit_evidence(&format!(
                    "connector: login attempt failed ({}), will retry{}",
                    ErrorClass::from_management(&e).as_str(),
                    if detail.is_empty() {
                        String::new()
                    } else {
                        format!(" ({detail})")
                    }
                ));
            }
        }
        match login_backoff.next_delay(&login_clock, &mut login_rng) {
            Some(delay) => tokio::time::sleep(delay).await,
            None => {
                // retry budget spent — upstream retry.go:34-39
                shared.transition(ConnEvent::RetryExhausted);
                shared.set_running(false);
                hilog::emit_evidence("connector: login retry budget exhausted");
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

/// 006: cumulative SUCCESSFUL starts in this process — the source of each
/// handle's [`ConnectorHandle::generation`]. Monotonic, never reset, so a
/// deployment can compare two snapshots and see whether the process keeps
/// starting connectors (growth) instead of guessing from a single reading.
static STARTS_TOTAL: AtomicU64 = AtomicU64::new(0);

/// 006: cumulative COMPLETED stop transitions (`stop()` that was not already
/// stopped). Monotonic too. `starts_total - stops_total` is a net change and
/// is deliberately NOT a live thread/task count.
static STOPS_TOTAL: AtomicU64 = AtomicU64::new(0);

fn connector_slot() -> MutexGuard<'static, Option<Arc<ConnectorHandle>>> {
    CONNECTOR.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 006 resource snapshot: a NON-BLOCKING, poison-tolerant lock probe. `None`
/// = the lock is held by another thread right now (WouldBlock) → the caller
/// renders the value as `null` (unknown) instead of a false 0. A poisoned
/// mutex is recovered exactly like `lock_poison` (panic=abort in release, so
/// poison is a host/test artifact). This call can never block.
fn try_lock_poison<T>(m: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match m.try_lock() {
        Ok(guard) => Some(guard),
        Err(std::sync::TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(std::sync::TryLockError::WouldBlock) => None,
    }
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
    let handle = ConnectorHandle::spawn_with_relay_selected(
        global_runtime().handle().clone(),
        Arc::new(GrpcManagementFactory::new(&config, config.keys.clone())),
        wg_feed.clone(),
        Arc::new(LoggingConfigApplier),
        secrets,
        config.meta.clone(),
        ExponentialBackoff::upstream_stream_default(),
        SyncPolicy::production(),
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
        // decision B: the validated startup route selection (absent = default
        // all-selected) — stored on the connector before its first Sync.
        config.route_selection.clone(),
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
    let handle = ConnectorHandle::spawn_with_relay_selected(
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
        // decision B: see `connector_start_json` — same validated state.
        config.route_selection.clone(),
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

/// The `connector_ice_socket_feed(fd)` implementation (N5c; **HOST-CLI ONLY**
/// since official-alignment stage 4) — resupply of protected UDP sockets into
/// the provider queue the host harness builds its ONE shared mux from
/// ([`crate::peer_conn::PeerIceOrchestrator::arm_mux`], `host_sockets::feed_ice`).
/// The DEVICE shell never calls this: the ICE plane rides the shared mux over
/// the WG outer socket (`connector_ice_mux_arm`), so the NAPI export and the
/// shell-side pool that used to drive it are gone. Same fail-closed contract as
/// [`connector_socket_feed_json`]: without a socket the host mux cannot be built
/// and peers stay Idle with a Network-class error, never unprotected.
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

/// The `connector_ice_mux_arm()` implementation (official-alignment §2.7
/// **stage 3**) — build the ONE shared ICE mux OVER the WireGuard device's own
/// outer socket and start its reader thread.
///
/// This replaces stage 2's `ice_mux_open()` + `connector_ice_mux_feed(fd)` pair:
/// there is no second socket any more. Upstream does the same thing —
/// `ICEBind.createOrUpdateMux` wraps the WG bind's own connection
/// (`client/iface/bind/ice_bind.go:291-316`, *"the sockets are managed by
/// WireGuard's StdNetBind, not by us"*) and the kernel-device path builds the mux
/// over `sharedsock.Listen(wgPort)` (`client/iface/device/device_kernel_unix.go:97-113`).
/// The shell therefore keeps ONE open+bind (`wg_fwd_open`) and ONE
/// `VpnConnection.protect` for both planes; this entry only *arms* what is
/// already protected and fed.
///
/// Failure tokens (fail-closed): `no-connector`, `no-mux-slot` (caller-supplied
/// orchestrator / host-test construction), `no-wg-device` (no live WG device —
/// nothing to share a socket with), `no-wg-socket` (dead fd), `socket-fd-invalid`
/// (not dup-able), `mux-over-wg` (the socket was unbound / a bind mismatch).
/// Success: `{"ok":true,"port":<n>,"replaced":<bool>,"reader":<bool>}`.
pub fn connector_ice_mux_arm_json() -> String {
    let slot = connector_slot();
    let Some(handle) = slot.as_ref() else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-connector"));
    };
    if handle.ice_mux.is_none() {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-mux-slot"));
    }
    let Some(feed) = handle.wg_feed.as_ref() else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-wg-device"));
    };
    if feed.socket_fd().is_none() {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-wg-socket"));
    }
    let (mux, replaced) = match handle.ice.lock_poison().arm_mux_from_wg_socket() {
        Ok(armed) => armed,
        Err(e) => {
            let token = match &e {
                crate::management::ManagementError::Network(msg) if msg.contains("no-wg-socket") => {
                    "no-wg-socket"
                }
                crate::management::ManagementError::Network(msg) if msg.contains("unbound") => {
                    "socket-unbound"
                }
                crate::management::ManagementError::Network(msg) if msg.contains("port-mismatch") => {
                    "port-mismatch"
                }
                _ => "socket-fd-invalid",
            };
            return format!("{{{},{}}}", jbool("ok", false), jstr("error", token));
        }
    };
    // The ONE reader: upstream `go mux.ReadFromConn(t.ctx)`
    // (`device_kernel_unix.go:109`). From here the mux demuxes every datagram on
    // the shared socket — STUN to the sessions, everything else straight into WG
    // (`handle_udp`) — and the ICE pump stops draining it.
    let reader = mux.start_reader();
    let port = mux.local_port();
    crate::hilog::emit(&format!(
        "N5e_ICE|mux-arm|port={port}|replaced={}|reader={}|source=wg-socket",
        u8::from(replaced),
        u8::from(reader)
    ));
    format!(
        "{{{},{},{},{}}}",
        jbool("ok", true),
        jinum("port", port as i64),
        jbool("replaced", replaced),
        jbool("reader", reader)
    )
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

/// The `connector_tun_release()` implementation (roam-1005-0124) — the
/// shell's pre-destroy leg of a controlled recreate: while the device is up,
/// close its current TUN dup so the platform can actually remove the OLD
/// vpn-tun netdev (the kernel keeps a netdev alive until its last fd closes;
/// a dup held across destroy() is exactly what made the old teardown land
/// AFTER the new create and kill the same-named fresh interface). The TUN
/// half of the data plane pauses; peers, tunnels and WG sessions stay — the
/// subsequent `connector_tun_fd_feed(new_fd)` re-attaches via the N8 replace
/// path.
///
/// Failure tokens: `no-connector`, `no-wg-device` (same meaning as the feed
/// seams), `device-not-up` (no device — nothing to release; the shell logs
/// it and proceeds with the recreate). Success:
/// `{"ok":true,"released":<bool>,"fd":<closed dup or -1>,"device_up":<bool>}`.
pub fn connector_tun_release_json() -> String {
    let slot = connector_slot();
    let Some(handle) = slot.as_ref() else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-connector"));
    };
    let Some(feed) = handle.wg_feed.as_ref() else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-wg-device"));
    };
    match feed.release_tun() {
        Ok(fd) => format!(
            "{{{},{},{},{}}}",
            jbool("ok", true),
            jbool("released", fd.is_some()),
            jinum("fd", fd.unwrap_or(-1) as i64),
            jbool("device_up", feed.device_up())
        ),
        Err(token) => format!("{{{},{}}}", jbool("ok", false), jstr("error", token)),
    }
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
/// Failure tokens: `route-set-invalid` (malformed JSON / non-string entries /
/// missing `routes` field) or `no-connector`. The document is validated BEFORE
/// the connector lookup, so a malformed `routesJson` answers `route-set-invalid`
/// even with nothing running.
///
/// T0 e15ac: this legacy 2-arg signature is KEPT verbatim for API
/// compatibility (the NAPI `(routesJson, isRecreate)` binding keeps working);
/// it delegates to the kind-aware [`connector_route_set_applied_kind_json`] with
/// `initial` / `auto`. An explicitly EMPTY `routes` array is now a legal applied
/// set (see that entry point) — recording it is what lets a later,
/// default-ENABLED network raise the recreate signal.
pub fn connector_route_set_applied_json(routes_json: &str, is_recreate: bool) -> String {
    route_set_applied_entry(routes_json, move |handle, routes| {
        handle
            .shared
            .ack_applied_route_set(routes, is_recreate, crate::sys::mono_ms());
    })
}

/// The kind-aware shell ACK — `connector_route_set_applied(routesJson,
/// isRecreate, kind?)` with the recreate KIND made explicit, for the shell that
/// performs USER-forced recreates on a live route toggle.
///
/// `kind` is one of:
/// - `initial` — the initial `create()` (legacy `isRecreate=false`);
/// - `auto` — a COMPLETED controlled recreate from the automatic
///   desired-set-divergence signal (legacy `isRecreate=true`): the ONLY kind that
///   spends the automatic budget ([`RECREATE_MAX`] = 3) and arms the
///   [`RECREATE_COOLDOWN_MS`] = 30s window;
/// - `user` — a recreate the USER forced (live route toggle). It records the
///   same applied set and clears the pending request but touches NO automatic
///   count / cooldown / exhausted state, so user toggles can never eat the
///   automatic churn budget (which stays fully available afterwards).
///
/// An ABSENT third NAPI argument is the legacy boolean, handled by
/// [`connector_route_set_applied_json`] — it does not reach this function. Any
/// other `kind` string is refused with `kind-invalid` (no silent fallback onto
/// the budget-spending kind).
///
/// `routes` is the EXACT set the shell captured as installed on the platform.
/// It is recorded verbatim, even when the current map/serial moved on in the
/// meantime: the ACK is evidence about the PLATFORM, not about the desired state,
/// so a stale ACK is accepted (never rejected) and simply keeps
/// `desired != applied`; the next gate refresh re-raises `recreate.required`.
/// `{"routes":[]}` is legal and meaningful: with an empty applied set recorded,
/// a later map that advertises a default-ENABLED network produces
/// `desired != applied` and therefore the recreate signal — without this ACK the
/// comparison would stay silent (`applied` = `None`).
///
/// Result: `{"ok":true,"recreate":{…}}` (same document as the legacy entry) or
/// `{"ok":false,"error":"kind-invalid"|"route-set-invalid"|"no-connector"}` —
/// checked in THAT order, so a request is fully validated before any state is
/// touched (a refused request mutates nothing and never even looks up the
/// running connector).
pub fn connector_route_set_applied_kind_json(routes_json: &str, kind: &str) -> String {
    let Some(kind) = RouteAckKind::parse(kind) else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "kind-invalid"));
    };
    route_set_applied_entry(routes_json, move |handle, routes| {
        handle
            .shared
            .ack_applied_route_set_kind(routes, kind, crate::sys::mono_ms());
    })
}

/// Shared body of the two ACK entries ([`connector_route_set_applied_json`] /
/// [`connector_route_set_applied_kind_json`]): validate the applied-set document,
/// resolve the running connector, record the ACK through `ack` and render the
/// frozen `{"ok":true,"recreate":{…}}` document. Validation happens BEFORE the
/// connector lookup, so a refused document answers `route-set-invalid` even with
/// nothing running and mutates nothing either way.
fn route_set_applied_entry(
    routes_json: &str,
    ack: impl FnOnce(&ConnectorHandle, Vec<String>),
) -> String {
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
    let slot = connector_slot();
    let Some(handle) = slot.as_ref() else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "no-connector"));
    };
    ack(handle, routes);
    let recreate = handle.shared.lock().recreate.clone();
    format!(
        "{{{},{}}}",
        jbool("ok", true),
        format!("\"recreate\":{}", recreate.to_json())
    )
}

/// Parse the `{"routes":["a.b.c.d/p",...]}` argument of
/// [`connector_route_set_applied_json`]. The `routes` key must be present and
/// an array of strings (missing field / non-array / non-string entry ⇒ error).
/// An EMPTY array is VALID (T0 e15ac): an explicitly ACKed empty platform set is
/// a real state and is what arms the desired-vs-applied comparison for a later
/// default-enabled network. Duplicate entries are preserved verbatim — the set
/// is what the shell captured, not a normalized form.
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
            wg_apply_failed: false,
            wg_apply_errors: 0,
            logout_ok: None,
            terminal: false,
            ice: IceOrchestratorSummary::default(),
            signal: SignalLinkStatus::default(),
            wg: crate::wg_device::WgDataplaneStatus::default(),
            recreate: RecreateStatus::default(),
            relay: RelayStatus::disabled(),
            // no connector at all ⇒ no relay carrier ⇒ no actual lane: the
            // honest `null` (an empty array would claim a carrier exists)
            peer_relays: None,
            // ... and no ICE orchestrator either: `null`, not `[]`
            ice_peers: None,
            // 0.0.26 gap 1: no mux either
            ice_mux: None,
        }
        .to_json(),
    }
}

// ---------------------------------------------------------------------------
// 006: additive native resource snapshot (read-only, non-blocking)
// ---------------------------------------------------------------------------

/// 006: the `wg` section of the resource document. DELIBERATELY not
/// collected — see the locking note on [`connector_resource_status_json`].
const WG_NOT_COLLECTED_JSON: &str = "{\"available\":false,\"reason\":\"not-collected\"}";

/// 006: every scalar `connector_resource_status()` carries. An `Option` field
/// is "could not be read right now" and renders as JSON `null`; a present
/// value is measured OR known-disposed. NOTHING here is a key, URL, fd, id or
/// secret — counts only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ResourceSnapshot {
    /// `None` = the global connector slot was busy → even presence is unknown.
    present: Option<bool>,
    /// `None` = no connector handle exists (no generation to report).
    generation: Option<u64>,
    running: Option<bool>,
    /// `None` = the shared-state leaf was busy, or no handle exists.
    state: Option<&'static str>,
    starts_total: u64,
    stops_total: u64,
    /// An ICE orchestrator object exists (a real spawn built one). `false`
    /// with `present:false` ⇒ the counts are known zeros; `false` with a
    /// handle present (no orchestrator object — test construction) ⇒ they are
    /// `null` (unmeasured), never 0.
    ice_available: bool,
    /// `None` = the orchestrator leaf was busy → counts unknown.
    ice: Option<crate::peer_conn::IceResourceCounts>,
    ice_sockets_available: bool,
    /// `None` = no shell-fed protected-UDP source attached → both counts
    /// unmeasured. `Some((queued, taken))`: `queued` is `None` when the
    /// source's short queue lock was busy at read time (unknown, never a
    /// fabricated 0), while `taken` is the source's independent atomic and
    /// stays a measured number whenever the source object exists.
    ice_sockets: Option<(Option<usize>, u64)>,
    /// `None` = the global socket-supply slot itself was busy.
    supply: Option<crate::socket_supply::SupplyCounts>,
    relay_available: bool,
    relay_lanes: Option<usize>,
    relay_pending_opens: Option<usize>,
    relay_foreign: Option<usize>,
    /// `None` = the DNS lookup map was busy.
    dns_pending: Option<usize>,
}

/// `"key":<n>` or `"key":null` (unknown).
fn jnum_opt(key: &str, v: Option<u64>) -> String {
    match v {
        Some(n) => jnum(key, n),
        None => format!("\"{key}\":null"),
    }
}

/// `"key":true|false` or `"key":null` (unknown).
fn jbool_opt(key: &str, v: Option<bool>) -> String {
    match v {
        Some(b) => jbool(key, b),
        None => format!("\"{key}\":null"),
    }
}

/// `"key":"..."` or `"key":null` (unknown / not applicable).
fn jstr_opt(key: &str, v: Option<&str>) -> String {
    match v {
        Some(s) => jstr(key, s),
        None => format!("\"{key}\":null"),
    }
}

impl ResourceSnapshot {
    fn to_json(&self) -> String {
        format!(
            "{{{},{},{},{},{},{},{},{}}}",
            jbool("ok", true),
            format!("\"connector\":{}", self.connector_json()),
            format!("\"ice\":{}", self.ice_json()),
            format!("\"ice_sockets\":{}", self.ice_sockets_json()),
            format!("\"socket_supply\":{}", self.supply_json()),
            format!("\"relay_carrier\":{}", self.relay_json()),
            format!("\"dns_lookups\":{}", self.dns_json()),
            format!("\"wg\":{}", WG_NOT_COLLECTED_JSON),
        )
    }

    fn connector_json(&self) -> String {
        format!(
            "{{{},{},{},{},{},{}}}",
            jbool_opt("present", self.present),
            jnum_opt("generation", self.generation),
            jbool_opt("running", self.running),
            jstr_opt("state", self.state),
            jnum("starts_total", self.starts_total),
            jnum("stops_total", self.stops_total),
        )
    }

    fn ice_json(&self) -> String {
        format!(
            "{{{},{},{},{},{}}}",
            jbool("available", self.ice_available),
            jnum_opt("sessions_present", self.ice.map(|c| c.sessions_present as u64)),
            jnum_opt("sessions_live", self.ice.map(|c| c.sessions_live as u64)),
            jnum_opt("sessions_stopped", self.ice.map(|c| c.sessions_stopped as u64)),
            jnum_opt("live_local_sockets", self.ice.map(|c| c.live_local_sockets as u64)),
        )
    }

    fn ice_sockets_json(&self) -> String {
        format!(
            "{{{},{},{}}}",
            jbool("available", self.ice_sockets_available),
            jnum_opt("queued", self.ice_sockets.and_then(|(queued, _)| queued).map(|n| n as u64)),
            jnum_opt("taken", self.ice_sockets.map(|(_, taken)| taken)),
        )
    }

    fn supply_json(&self) -> String {
        match self.supply {
            Some(s) => format!(
                "{{{},{},{}}}",
                jbool("installed", s.installed),
                jnum_opt("pending", s.pending.map(|n| n as u64)),
                jnum_opt("queued", s.queued.map(|n| n as u64)),
            ),
            None => "{\"installed\":null,\"pending\":null,\"queued\":null}".to_string(),
        }
    }

    fn relay_json(&self) -> String {
        format!(
            "{{{},{},{},{}}}",
            jbool("available", self.relay_available),
            jnum_opt("attached_lanes", self.relay_lanes.map(|n| n as u64)),
            jnum_opt("pending_opens", self.relay_pending_opens.map(|n| n as u64)),
            jnum_opt("foreign_clients", self.relay_foreign.map(|n| n as u64)),
        )
    }

    fn dns_json(&self) -> String {
        format!("{{{}}}", jnum_opt("pending_receivers", self.dns_pending.map(|n| n as u64)))
    }
}

/// 006: gather the resource snapshot with NON-BLOCKING probes only. The two
/// process-global surfaces (socket supply, DNS lookups) are read independently
/// of the connector slot, so they are still captured when the slot is busy or
/// empty; every connector-owned count then stays `null` (busy) or becomes a
/// known zero (no handle).
fn connector_resource_probe() -> ResourceSnapshot {
    let mut snap = ResourceSnapshot {
        starts_total: STARTS_TOTAL.load(Ordering::Acquire),
        stops_total: STOPS_TOTAL.load(Ordering::Acquire),
        supply: crate::socket_supply::installed_counts_try(),
        dns_pending: try_lock_poison(dns_lookups()).map(|g| g.len()),
        ..Default::default()
    };
    match try_lock_poison(&CONNECTOR) {
        // Global slot busy: presence itself is unknown → connector-owned
        // counts stay `null`, never a false 0.
        None => {}
        Some(slot) => {
            if let Some(handle) = slot.as_ref() {
                let handle = handle.clone();
                // release the global BEFORE taking any handle leaf
                drop(slot);
                snap.present = Some(true);
                snap.generation = Some(handle.generation);
                snap.running = Some(handle.shared.is_running());
                handle.fill_resource_probe(&mut snap);
            } else {
                // No connector installed: a KNOWN disposed state — every
                // connector-owned resource is an honest zero, not unknown.
                snap.present = Some(false);
                snap.running = Some(false);
                snap.ice_available = false;
                snap.ice = Some(crate::peer_conn::IceResourceCounts::default());
                snap.ice_sockets_available = false;
                snap.ice_sockets = Some((Some(0), 0));
                snap.relay_available = false;
                snap.relay_lanes = Some(0);
                snap.relay_pending_opens = Some(0);
                snap.relay_foreign = Some(0);
            }
        }
    }
    snap
}

/// 006: the ADDITIVE native resource snapshot — a read-only, non-blocking
/// document of the counts core still OWNS, for the shell's 60 s sample and the
/// 006 system-kill investigation.
///
/// Purely additive: `connector_status()` and its frozen field order are NOT
/// touched (separate function, separate NAPI export), and no cleanup
/// behaviour changes — the endpoint only observes.
///
/// ## Locking
/// Every acquisition is a NON-BLOCKING probe (`try_lock`), released before the
/// next one; no leaf is nested with another leaf, the relay leaves are never
/// read while the shared-state leaf is held, and there is no await anywhere.
/// A busy lock (or a poisoned one) is reported as JSON `null` = UNKNOWN —
/// never as a false `0`. `wg` is deliberately omitted: the only cheap WG read
/// (`WgPeerApplier::dataplane_status`) takes the device's shared inner lock
/// with a BLOCKING lock and copies per-peer rows, so it could contend with the
/// data-plane pump; `connector_status().wg` stays the WG read surface.
///
/// ## Schema (exact null semantics)
/// ```json
/// {"ok":true,
///  "connector":{"present":bool|null,"generation":n|null,"running":bool|null,
///               "state":"connecting"|...|null,"starts_total":n,"stops_total":n},
///  "ice":{"available":bool,"sessions_present":n|null,"sessions_live":n|null,
///         "sessions_stopped":n|null,"live_local_sockets":n|null},
///  "ice_sockets":{"available":bool,"queued":n|null,"taken":n|null},
///  "socket_supply":{"installed":bool|null,"pending":n|null,"queued":n|null},
///  "relay_carrier":{"available":bool,"attached_lanes":n|null,
///                   "pending_opens":n|null,"foreign_clients":n|null},
///  "dns_lookups":{"pending_receivers":n|null},
///  "wg":{"available":false,"reason":"not-collected"}}
/// ```
/// - `connector.present` — `null` = the global connector slot was busy;
///   `false` = no connector is installed (a KNOWN disposed state, so EVERY
///   connector-owned count below is an honest zero); `true` = a handle exists.
/// - `connector.generation` — the handle's successful-start ordinal (unique
///   per start in this process); `null` when no handle exists. It is an
///   IDENTITY: `generation - 1` is NOT a live task/thread count.
/// - `connector.starts_total` / `stops_total` — process-cumulative successful
///   starts and completed stop transitions; always numbers (they outlive the
///   handle). Their DIFFERENCE is a net change, not a live resource count.
///   Growth between two samples = the process keeps starting connectors; a
///   single sample cannot prove a leak (normal stops set the flag and the
///   threads eventually exit).
/// - `ice.available` — an orchestrator object exists. With `present:false` the
///   four counts are known zeros; with `present:true` and `available:false` (a
///   handle built without an orchestrator object — test construction) they are
///   `null`: UNMEASURED, never a fabricated 0. `null` also when the leaf was
///   busy.
/// - `ice.sessions_*` — `sessions_present` counts retained `IceSession`
///   OBJECTS (including ones `stop_all` already stopped); `sessions_live`
///   counts started-and-not-stopped sessions; `sessions_stopped` counts
///   stopped-but-retained ones (a constructed-but-not-started session is in
///   `sessions_present` only).
/// - `ice.live_local_sockets` — fds (`fd >= 0`) the sessions still own, summed
///   over ALL retained sessions. After `stop_all` this is 0 while
///   `sessions_present` may stay > 0 (the objects are retained).
/// - `ice_sockets` — the protected-ICE queue depth (`queued`) and the
///   cumulative handed-out count (`taken`) of the shell-fed UDP source.
///   `present:false` ⇒ known zeros; `present:true` with `available:false` (no
///   shell-fed source attached) ⇒ both counts `null` (depth unmeasured — the
///   same fact the frozen `ice_sockets` section renders as `-1`). A `queued`
///   that is `null` while `available:true` means the source's queue lock was
///   held at read time (a feed/gather in flight): the depth is UNKNOWN while
///   `taken` (an independent atomic) stays a measured number.
/// - `socket_supply` — the GLOBAL per-dial request bridge: requests awaiting a
///   protect reply (`pending`) and requests not yet polled (`queued`).
///   `installed:false` is a known "no supply" (counts 0); all three fields are
///   `null` only while the global slot lock was busy. Read INDEPENDENTLY of
///   the connector slot, so it is captured with no connector at all.
/// - `relay_carrier` — attached WG relay lanes, in-flight lane opens and live
///   foreign relay clients. `available:false` (no carrier armed) ⇒ known
///   zeros; an individual `null` = that leaf was busy at read time.
/// - `dns_lookups.pending_receivers` — DNS requests whose oneshot receiver is
///   still registered (unpolled); read INDEPENDENTLY of the connector slot.
///   This is a depth, NOT evidence of a leak: an unpolled entry disappears on
///   the next poll or on `physical_network_changed`.
/// - No field carries a peer key, a relay URL, an fd number, a request id or
///   any secret — counts only.
pub fn connector_resource_status_json() -> String {
    connector_resource_probe().to_json()
}

/// The N5c socket-depth section appended to `connector_status()`.
///
/// `queued`/`taken` are the protected-UDP source's queue observables (see
/// [`ConnectorHandle::status_json`]); `-1` = no source attached. The retired
/// per-peer pool demand (`need_per_peer`) is gone with the shell pool (stage 4).
fn ice_sockets_section(queued: i64, taken: i64) -> String {
    format!(
        "\"ice_sockets\":{{\"queued\":{queued},\"taken\":{taken}}}"
    )
}

/// The full `connector_status()` text: `base` with the N5c socket-depth section
/// AND the 2026-10-09 ICE-snapshot freshness marks appended INSIDE its root
/// object (the base document's trailing `}` is replaced by
/// `,<ice_snapshot_stale>,<ice_snapshot_age_ms>,<ice_sockets>}`). Extracted so
/// the unit test exercises the SAME brace dance the status surface uses — a
/// hand-assembled fixture in the test would not have caught a missing closing
/// brace.
///
/// 2026-10-09 (`ice_snapshot_stale` / `ice_snapshot_age_ms`): the two marks are
/// added BEFORE the trailing `ice_sockets` section so that (a) every
/// pre-existing key keeps its bytes and its relative order (the base document
/// up to `ice_peers` is untouched) and (b) the document still ENDS with the
/// `ice_sockets` object — the trailing section the compatibility tests pin.
/// `ice_snapshot_stale` = `true` when the `ice`/`ice_peers`
/// values in this document are the LAST SUCCESSFUL sample rather than a fresh
/// read (the orchestrator lock was busy with a gather); `ice_snapshot_age_ms`
/// = `0` for a fresh read, the sample's age in ms when it is cached, and `null`
/// when there is no successful ICE read to age (this document's own "unknown")
/// — never a fabricated `0` age, never a negative sentinel (the crate's strict
/// config reader rejects negatives and the N2-H freeze-facts reader parses this
/// document with it) and never a silent empty ICE state.
fn with_ice_sockets_section(
    base: &str,
    queued: i64,
    taken: i64,
    ice_snapshot_stale: bool,
    ice_snapshot_age_ms: Option<u64>,
) -> String {
    let age = match ice_snapshot_age_ms {
        Some(ms) => ms.to_string(),
        None => "null".to_string(),
    };
    format!(
        "{},\"ice_snapshot_stale\":{},\"ice_snapshot_age_ms\":{},{}}}",
        &base[..base.len() - 1],
        ice_snapshot_stale,
        age,
        ice_sockets_section(queued, taken)
    )
}

/// The `connector_network_config()` implementation: read-only snapshot of
/// the shell-applicable subset of the last applied NetworkMap (tunnel
/// address/prefix, managed routes with the default-route mark, DNS servers,
/// peer summary — public keys + allowed-ips counts only). N2-H I2: the tail
/// carries the announced `stuns[].uri` list (the shell's exclusion-derivation
/// channel — see [`ShellNetworkConfig::stuns`]). 2026-10-12 upstream MTU
/// alignment: the final tail field is the announced `PeerConfig.mtu` when it
/// is usable on this platform (see [`ShellNetworkConfig::mtu`]). Always valid
/// JSON; with no connector or before the first map:
/// `{"available":false,"reason":"no-network-map"}`.
pub fn connector_network_config_json() -> String {
    let slot = connector_slot();
    match slot.as_ref() {
        Some(handle) => handle.network_config_json(),
        None => NO_NETWORK_MAP_JSON.to_string(),
    }
}

/// The `connector_routes_json()` document (decision B, internal helper — the
/// NAPI registration is a SEPARATE follow-up increment; nothing in `napi.rs`/ArkTS
/// changes here). Dedicated document, NOT an extension of the frozen
/// `connector_status()` / `connector_network_config()` shapes:
///
/// ```json
/// {
///   "available": true,
///   "serial": 42,
///   "selection": {"selected_routes":[],"deselected_routes":["HostInternal"],"deselect_all":false},
///   "networks": [
///     {"net_id":"HostInternal","networks":["192.168.50.0/24"],
///      "candidate_count":2,"enabled":false,"is_exit":false}
///   ]
/// }
/// ```
///
/// The catalog is the UNFILTERED advertised route list of the last applied
/// NetworkMap, so a user-disabled network (and every one of its HA candidates)
/// stays visible with `enabled:false`; `selection` is the generation's startup
/// state verbatim.
///
/// `available` marks the MAP-SEEN state, not the route count: `available:true`
/// with the applied map's own `serial` — and `networks:[]` when that map
/// advertised no routes — is the honest "connected, no routes" catalog the UI
/// needs to distinguish from "no snapshot". `available:false` + `serial:0` +
/// `networks:[]` means "no route catalog": no connector, before the first map,
/// or after stop — never a fabricated empty-but-active state. No secrets:
/// NetIDs, prefixes and counts are public routing material.
///
/// Read-only and side-effect-free: plain mutex snapshot, no gate refresh, no
/// recreate bookkeeping, no network/IO.
pub fn connector_routes_json() -> String {
    let slot = connector_slot();
    match slot.as_ref() {
        Some(handle) => handle.routes_json(),
        None => routes_document(&RouteSelector::new(), None),
    }
}

/// The engine-side sink of the INTERACTIVE session extend — upstream
/// `Engine.ExtendAuthSession` (`client/internal/engine_authsession.go:83-107`)
/// ends by applying the server's own 3-state answer to the ONE deadline anchor
/// (`ApplySessionDeadline`, L102 → `sessionwatch`/status recorder).
///
/// This is that write, for the connector running in THIS process: the value
/// goes into exactly the same [`SessionDeadline`] the Login/Sync path folds
/// into (`apply_wire` semantics: `None` = no info → KEEP, `Some(0)` = expiry
/// disabled, `Some(t)` = new absolute deadline), so the published
/// `session_expiry` / `session_expires_at_unix` can never disagree with what
/// the server answered to the extend.
///
/// `true` = a connector was running here and holds the new anchor; `false` =
/// no connector in this process. The caller ([`crate::account_auth`]'s extend
/// job) still hands the server's answer back to whoever asked: in the shell
/// split the extend runs in the UI process (the only one with the SSO
/// surface), whose connector slot is empty by construction, so the `false`
/// branch is the production case and the UI displays the returned deadline.
pub fn session_deadline_apply_live(wire: Option<i64>) -> bool {
    let slot = connector_slot();
    let Some(handle) = slot.as_ref() else {
        return false;
    };
    handle.shared.lock().deadline.apply_wire(wire);
    true
}

/// The LIVE route-selection update (T0 e15ac): `selectionJson` is the RAW
/// selector document — the exact shape [`RouteSelector::to_json`] emits,
/// `{"selected_routes":[...],"deselected_routes":[...],"deselect_all":bool}`
/// (missing keys = empty/false; an empty or `null` document = the all-default
/// selector). It is consumed by the SAME strict parser as the persisted state
/// document and the start config ([`RouteSelector::from_json`]): duplicate known
/// keys, wrong value types and non-object documents are REJECTED, and a rejected
/// request mutates NOTHING (no partial selection, no WG/device call).
///
/// Unlike the start-config field this is a RUNTIME setter: the running connector
/// re-filters the routes it already advertised, rebuilds its shell snapshot and
/// re-applies the WG managed routes, serialized with the map pipeline — a toggle
/// therefore needs NO restart. The user-forced platform recreate a live route
/// change may still require (the HarmonyOS VPE fixes its route set at
/// `create()` time) is a SEPARATE, `user`-kind ACK
/// ([`connector_route_set_applied_kind_json`]) that never spends the automatic
/// recreate budget.
///
/// Result (fixed key order, always valid JSON):
///
/// ```json
/// {"ok":true,"available":true,"changed":true,
///  "desired_routes":["172.16.0.0/12"],"recreate_needed":true}
/// ```
///
/// - `ok:false` + `error` — one of `no-connector` (nothing running),
///   `selection-invalid` (parser refused; nothing mutated) or
///   `wg-apply-failed` (selection + snapshot were updated, the WG seam refused
///   the routes: the state now names a route set the data plane does not have,
///   `wg_apply_failed` stays latched — fail closed).
/// - `available` — a NetworkMap was applied, so the selection was filtered
///   against the advertised catalog and the seams were updated. With no map the
///   selection is stored for the NEXT map and `available:false` is returned: no
///   applied route set is claimed.
/// - `changed` — the stored selection actually differs from the previous one (an
///   identical re-send is still applied, idempotently, and reports `false`).
/// - `desired_routes` — the EFFECTIVE route set after the update, in map order
///   with the default-route gate applied: exactly the list the platform config
///   should hold (empty with no map).
/// - `recreate_needed` — the platform route set must be replaced for
///   `desired_routes` to take effect: no applied set was ever ACKed, or the
///   desired PREFIX SET differs from the ACKed one. An HA candidate/owner swap
///   that keeps the same prefixes needs NO OS replace. This is the raw need; the
///   automatic budget/cooldown governing the auto recreate path is unchanged and
///   reported separately by `connector_status().recreate`.
///
/// On a rejected request (`no-connector` / `selection-invalid`) the four
/// informational fields carry the no-claim values (`false`/`false`/`[]`/`false`);
/// only `wg-apply-failed` reports the real post-update facts, because there the
/// mutation DID happen. The frozen `connector_status()` /
/// `connector_network_config()` documents are NOT touched — this is a separate
/// document (the NAPI registration lands with the shell-side owner).
pub fn connector_route_selection_set_json(selection_json: &str) -> String {
    let selection = match RouteSelector::from_json(selection_json) {
        Ok(selection) => selection,
        Err(_) => {
            return route_selection_result_json(
                false,
                Some("selection-invalid"),
                &RouteSelectionOutcome::default(),
            )
        }
    };
    let slot = connector_slot();
    let Some(handle) = slot.as_ref() else {
        return route_selection_result_json(
            false,
            Some("no-connector"),
            &RouteSelectionOutcome::default(),
        );
    };
    let outcome = handle.shared.route_selection_set_at(selection, handle.wg.as_ref());
    let error = outcome.wg_apply_failed.then_some("wg-apply-failed");
    route_selection_result_json(error.is_none(), error, &outcome)
}

/// Render the `connector_route_selection_set_json` document: fixed key order
/// `ok` / `error` (only on failure) / `available` / `changed` /
/// `desired_routes` / `recreate_needed`. The informational fields are ALWAYS
/// present so the shell never has to guess; see the entry point for what they
/// claim on a rejected request.
fn route_selection_result_json(
    ok: bool,
    error: Option<&'static str>,
    outcome: &RouteSelectionOutcome,
) -> String {
    let mut out = String::from("{");
    out.push_str(&jbool("ok", ok));
    if let Some(error) = error {
        out.push(',');
        out.push_str(&jstr("error", error));
    }
    out.push(',');
    out.push_str(&jbool("available", outcome.available));
    out.push(',');
    out.push_str(&jbool("changed", outcome.changed));
    out.push_str(",\"desired_routes\":[");
    for (index, route) in outcome.desired_routes.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\"", crate::util::json_escape(route)));
    }
    out.push_str("],");
    out.push_str(&jbool("recreate_needed", outcome.recreate_needed));
    out.push('}');
    out
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

/// The `connector_logout()` implementation: the EXPLICIT user logout —
/// ONE management `Logout` RPC on the live connector's logged-in client
/// (the server deregisters and DELETES the peer, upstream
/// client/server/server.go). This is the ONLY Logout entry in the client:
/// `connector_stop()` never logs out, on any stop path. Safe with no
/// connector at all (honest refusal — the shell then just drops the local
/// credentials and points the user at the management console).
pub fn connector_logout_json() -> String {
    let slot = connector_slot();
    match slot.as_ref() {
        Some(handle) => handle.logout_json(),
        None => format!(
            "{{{},{},{}}}",
            jbool("ok", false),
            jstr("error", "no-connector"),
            jstr("reason", "not-running"),
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

    /// 006: a BUSY global slot and a BUSY ICE leaf must render `null`
    /// (unknown), never a false 0 — and the read must return promptly
    /// (non-blocking `try_lock` only). The independent socket-supply / DNS
    /// sections stay captured while the connector slot is busy.
    #[tokio::test(flavor = "multi_thread")]
    async fn resource_status_reports_busy_leaves_as_null_and_never_blocks() {
        struct BusyProbeFactory;
        impl ManagementFactory for BusyProbeFactory {
            fn connect(
                &self,
            ) -> Pin<
                Box<
                    dyn Future<Output = Result<ManagementGrpcClient, ManagementError>> + Send + '_,
                >,
            > {
                Box::pin(async { Err(ManagementError::Network("busy-probe".into())) })
            }
        }

        let cfg = ConnectorConfig::from_json(&format!(
            "{{\"management_url\":\"http://127.0.0.1:1\",\"private_key\":\"{}\",\
              \"hostname\":\"ohos-resource\",\"allow_unprotected_management\":true}}",
            test_private_key_b64(9)
        ))
        .expect("test config");
        let handle = ConnectorHandle::spawn(
            tokio::runtime::Handle::current(),
            Arc::new(BusyProbeFactory),
            Arc::new(WgPeerRegistry::default()),
            Arc::new(NoopHost),
            ConnectorSecrets::default(),
            cfg.meta.clone(),
            ExponentialBackoff::upstream_stream_default(),
            SyncPolicy::production(),
            false,
            None,
            None,
            None,
            None,
            HostIceTuning::default(),
        );

        // 1) BUSY GLOBAL SLOT: presence itself is unknown; every
        // connector-owned count is null; the independent global sections are
        // still read (a number or null, never an invented value).
        {
            let _held = connector_slot();
            let t0 = std::time::Instant::now();
            let doc = connector_resource_status_json();
            assert!(t0.elapsed() < Duration::from_secs(1), "busy global must not block: {doc}");
            assert!(doc.contains("\"present\":null"), "{doc}");
            assert!(doc.contains("\"generation\":null"), "{doc}");
            assert!(doc.contains("\"running\":null"), "{doc}");
            assert!(
                doc.contains("\"ice\":{\"available\":false,\"sessions_present\":null"),
                "{doc}"
            );
            assert!(
                doc.contains("\"ice_sockets\":{\"available\":false,\"queued\":null,\"taken\":null}"),
                "{doc}"
            );
            assert!(
                doc.contains("\"relay_carrier\":{\"available\":false,\"attached_lanes\":null"),
                "{doc}"
            );
            assert!(doc.contains("\"socket_supply\":{"), "{doc}");
            assert!(doc.contains("\"dns_lookups\":{\"pending_receivers\":"), "{doc}");
            assert!(doc.contains("\"wg\":{\"available\":false,\"reason\":\"not-collected\"}"), "{doc}");
        }

        // 2) A live handle in the slot: real, measured values.
        *connector_slot() = Some(handle.clone());
        let doc = connector_resource_status_json();
        assert!(doc.contains("\"present\":true"), "{doc}");
        assert!(doc.contains("\"running\":true"), "{doc}");
        assert!(doc.contains("\"available\":true,\"sessions_present\":0"), "{doc}");
        assert!(
            doc.contains("\"ice_sockets\":{\"available\":true,\"queued\":0,\"taken\":0}"),
            "{doc}"
        );
        assert!(doc.contains("\"wg\":{\"available\":false,\"reason\":\"not-collected\"}"), "{doc}");
        assert!(!doc.contains("pub_key"), "counts only — no key material: {doc}");

        // 3) BUSY ICE LEAF: `available` stays true, the counts are unknown,
        // and the OTHER leaves remain readable.
        {
            let _held_ice = handle.ice.lock_poison();
            let t0 = std::time::Instant::now();
            let doc = connector_resource_status_json();
            assert!(t0.elapsed() < Duration::from_secs(1), "busy ICE leaf must not block: {doc}");
            assert!(
                doc.contains("\"ice\":{\"available\":true,\"sessions_present\":null"),
                "{doc}"
            );
            assert!(
                doc.contains("\"ice_sockets\":{\"available\":true,\"queued\":0,\"taken\":0}"),
                "{doc}"
            );
        }

        // 4) Released: measured zeros again (no peers in this construction).
        let doc = connector_resource_status_json();
        assert!(
            doc.contains(
                "\"available\":true,\"sessions_present\":0,\"sessions_live\":0,\
                 \"sessions_stopped\":0,\"live_local_sockets\":0"
            ),
            "{doc}"
        );

        // cleanup: free the global slot and stop the handle (threads exit on
        // the stop flag).
        *connector_slot() = None;
        handle.stop();
    }

    /// 006: the protected-ICE queue read inside the resource probe is
    /// NON-BLOCKING too. While a feed/gather holds the source's queue lock,
    /// `ice_sockets.queued` is UNKNOWN (`null`) — never a false 0 — the
    /// independent `taken` atomic stays measured, `available` stays true, and
    /// the probe returns promptly. Driven through `fill_resource_probe`
    /// directly so the process-global connector slot is untouched (no race
    /// with the busy-leaf test above).
    #[tokio::test(flavor = "multi_thread")]
    async fn resource_probe_reports_a_busy_ice_socket_queue_as_unknown() {
        struct BusyQueueProbeFactory;
        impl ManagementFactory for BusyQueueProbeFactory {
            fn connect(
                &self,
            ) -> Pin<
                Box<
                    dyn Future<Output = Result<ManagementGrpcClient, ManagementError>> + Send + '_,
                >,
            > {
                Box::pin(async { Err(ManagementError::Network("busy-queue-probe".into())) })
            }
        }

        let cfg = ConnectorConfig::from_json(&format!(
            "{{\"management_url\":\"http://127.0.0.1:1\",\"private_key\":\"{}\",\
              \"hostname\":\"ohos-resource-queue\",\"allow_unprotected_management\":true}}",
            test_private_key_b64(11)
        ))
        .expect("test config");
        let handle = ConnectorHandle::spawn(
            tokio::runtime::Handle::current(),
            Arc::new(BusyQueueProbeFactory),
            Arc::new(WgPeerRegistry::default()),
            Arc::new(NoopHost),
            ConnectorSecrets::default(),
            cfg.meta.clone(),
            ExponentialBackoff::upstream_stream_default(),
            SyncPolicy::production(),
            false,
            None,
            None,
            None,
            None,
            HostIceTuning::default(),
        );
        let source = handle
            .ice_sockets
            .as_ref()
            .expect("the production spawn attaches the UDP source")
            .clone();

        // Released: the real depth (0 — no shell feed in this construction).
        let mut released = ResourceSnapshot::default();
        handle.fill_resource_probe(&mut released);
        assert_eq!(released.ice_sockets, Some((Some(0), 0)));
        assert!(released.ice_sockets_available);

        {
            let _held = source.hold_queue_for_test();
            let mut busy = ResourceSnapshot::default();
            let t0 = std::time::Instant::now();
            handle.fill_resource_probe(&mut busy);
            let elapsed = t0.elapsed();
            assert!(elapsed < Duration::from_secs(1), "a held queue must not block the probe");
            assert_eq!(busy.ice_sockets, Some((None, 0)), "a held queue is unknown, not 0");
            assert!(busy.ice_sockets_available, "the source object still exists");
            let doc = busy.to_json();
            assert!(
                doc.contains("\"ice_sockets\":{\"available\":true,\"queued\":null,\"taken\":0}"),
                "{doc}"
            );
        }

        // Released again: a measured zero, not a sticky unknown.
        let mut again = ResourceSnapshot::default();
        handle.fill_resource_probe(&mut again);
        assert_eq!(again.ice_sockets, Some((Some(0), 0)));
        assert!(
            again
                .to_json()
                .contains("\"ice_sockets\":{\"available\":true,\"queued\":0,\"taken\":0}"),
        );

        handle.stop();
    }

    const SENTINEL_KEY: &str = "SENTINEL-SETUP-KEY-n3-5-DO-NOT-LOG";

    /// 006: the renderer's EXACT null semantics — `None` (unknown/unmeasured)
    /// renders `null` and never a fabricated 0, and the field order is the
    /// documented one.
    #[test]
    fn resource_snapshot_renderer_renders_unknown_as_null() {
        let unmeasured = ResourceSnapshot {
            present: Some(true),
            generation: None,
            running: Some(true),
            state: None,
            starts_total: 4,
            stops_total: 1,
            ice_available: false,
            ice: None,
            ice_sockets_available: false,
            ice_sockets: None,
            supply: None,
            relay_available: true,
            relay_lanes: None,
            relay_pending_opens: Some(2),
            relay_foreign: Some(0),
            dns_pending: None,
        };
        assert_eq!(
            unmeasured.to_json(),
            "{\"ok\":true,\
             \"connector\":{\"present\":true,\"generation\":null,\"running\":true,\"state\":null,\
             \"starts_total\":4,\"stops_total\":1},\
             \"ice\":{\"available\":false,\"sessions_present\":null,\"sessions_live\":null,\
             \"sessions_stopped\":null,\"live_local_sockets\":null},\
             \"ice_sockets\":{\"available\":false,\"queued\":null,\"taken\":null},\
             \"socket_supply\":{\"installed\":null,\"pending\":null,\"queued\":null},\
             \"relay_carrier\":{\"available\":true,\"attached_lanes\":null,\"pending_opens\":2,\
             \"foreign_clients\":0},\
             \"dns_lookups\":{\"pending_receivers\":null},\
             \"wg\":{\"available\":false,\"reason\":\"not-collected\"}}"
        );
    }

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
            wg_apply_failed: false,
            wg_apply_errors: 0,
            logout_ok: None,
            terminal: true,
            ice: IceOrchestratorSummary::default(),
            signal: SignalLinkStatus::default(),
            wg: crate::wg_device::WgDataplaneStatus::default(),
            recreate: RecreateStatus::default(),
            relay: RelayStatus::disabled(),
            peer_relays: None,
            ice_peers: None,
            ice_mux: None,
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
    /// key bytes in any Debug-visible form, and the secrets struct redacts its
    /// field.
    #[test]
    fn debug_output_never_contains_secrets() {
        let secrets = ConnectorSecrets { setup_key: SENTINEL_KEY.into() };
        let debug = format!("{secrets:?}");
        assert!(!debug.contains(SENTINEL_KEY), "setup key leaked via Debug: {debug}");
        assert!(debug.contains("REDACTED"));

        // the config's private key flows through EnvelopeKeyPair whose
        // Debug prints the PUBLIC key only (crate::envelope)
        let cfg = ConnectorConfig::from_json(&test_config_json()).expect("config");
        let debug = format!("{cfg:?}");
        assert!(!debug.contains(&test_private_key_b64(1)), "private key leaked: {debug}");

        // and a handle's Debug stays secret-free too (built with a sentinel)
        let handle_debug =
            format!("{:?}", ConnectorSecrets { setup_key: SENTINEL_KEY.into() });
        assert!(!handle_debug.contains(SENTINEL_KEY));
    }

    /// 2026-10-12: a connect carries the SETUP KEY at most. The JWT is not a
    /// connect credential any more — an unknown `jwt`/`jwt_token` key is
    /// ignored like any other unknown field (so a stale shell cannot smuggle a
    /// token into the connect path), and every document without a setup key is
    /// the plain public-key-only reconnect (upstream grpc.go:650-656).
    #[test]
    fn secrets_parse_and_validate() {
        let ok = ConnectorSecrets::from_json(&format!("{{\"setup_key\":\"{SENTINEL_KEY}\"}}"))
            .expect("setup key only");
        assert_eq!(ok.setup_key, SENTINEL_KEY);

        let ok = ConnectorSecrets::from_json("{}").expect("no credentials is a valid mode");
        assert!(ok.setup_key.is_empty());
        let ok = ConnectorSecrets::from_json("{\"setup_key\":\"\"}").expect("empty setup key");
        assert!(ok.setup_key.is_empty());
        // a JWT is NOT a connector credential: the key is ignored, never parsed
        let ok = ConnectorSecrets::from_json("{\"jwt\":\"tok\",\"jwt_token\":\"tok2\"}")
            .expect("a token-shaped key is not a connector credential");
        assert!(ok.setup_key.is_empty());
        // a NON-empty malformed document is still rejected (type/shape errors)
        let err = ConnectorSecrets::from_json("{\"setup_key\":5}").unwrap_err();
        assert!(matches!(err, ConfigError::Field { field: "setup_key", .. }), "{err}");
        let err = ConnectorSecrets::from_json("not json").unwrap_err();
        assert!(matches!(err, ConfigError::Json { .. }), "{err}");
    }

    /// 2026-10-12: the needs-login class keeps its numeric status, carries no
    /// message across the boundary, and is NOT the generic auth class (that is
    /// the whole point — the shell shows the official `Login required` /
    ///「需要登录」 label, not a retry).
    #[test]
    fn needs_login_is_its_own_class() {
        let err = ManagementError::NeedsLogin {
            status: 7,
            message: "peer login has expired, please log in once more".into(),
        };
        let class = ErrorClass::from_management(&err);
        assert_eq!(class, ErrorClass::NeedsLogin { status: 7 });
        assert_eq!(class.as_str(), "needs-login");
        assert_eq!(class.status_code(), 7);
        assert_eq!(class.to_json(), "{\"class\":\"needs-login\",\"status\":7}");
        assert!(!class.to_json().contains("peer login has expired"));
        assert_ne!(class, ErrorClass::Auth { status: 7 });

        // the re-classing keeps the server's status as the class status
        let needed = needs_login(&ManagementError::Auth {
            status: 7,
            message: "peer login has expired, please log in once more".into(),
        });
        assert_eq!(
            needed,
            ManagementError::NeedsLogin {
                status: 7,
                message: "peer login has expired, please log in once more".into(),
            }
        );
        assert_eq!(needs_login(&ManagementError::NeedsLogin { status: 3, message: "x".into() }).status_code(), 3);
        assert_eq!(needs_login(&ManagementError::Timeout).status_code(), 0);
    }

    /// 2026-10-12: the login-needed predicate is upstream's
    /// `InvalidArgument || PermissionDenied` and nothing else — in particular
    /// NOT `Unauthenticated` (a verdict about a TOKEN, which a connect never
    /// carries).
    #[test]
    fn login_needed_is_upstreams_two_codes_only() {
        assert!(is_login_needed(&ManagementError::Auth { status: 7, message: String::new() }));
        assert!(is_login_needed(&ManagementError::Request { status: 3, message: String::new() }));
        assert!(!is_login_needed(&ManagementError::Auth { status: 16, message: String::new() }));
        assert!(!is_login_needed(&ManagementError::Timeout));
        assert!(!is_login_needed(&ManagementError::Network("x".into())));
        assert!(!is_login_needed(&ManagementError::Request { status: 5, message: String::new() }));
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

    /// 2026-10-06 login-failure visibility: the redacted device-log detail
    /// carries the numeric status + the server message with secrets
    /// scrubbed, flattened and capped — and stays empty for classes
    /// without a server message. 2026-10-12: the setup key is the only
    /// secret a connect still carries (there is no JWT in this path).
    #[test]
    fn redacted_login_detail_scrubs_and_caps() {
        let secrets = ConnectorSecrets { setup_key: SENTINEL_KEY.into() };
        // plain server message: status + text verbatim
        let d = redacted_login_detail(
            &ManagementError::Auth { status: 401, message: "invalid setup key".into() },
            &secrets,
        );
        assert_eq!(d, "status 401: invalid setup key");
        // a server echoing the setup key: scrubbed, never logged
        let d = redacted_login_detail(
            &ManagementError::Request {
                status: 400,
                message: format!("bad key {SENTINEL_KEY} rejected"),
            },
            &secrets,
        );
        assert_eq!(d, "status 400: bad key [redacted] rejected");
        assert!(!d.contains(SENTINEL_KEY));
        // the needs-login class keeps its server verdict in the log detail
        let d = redacted_login_detail(
            &ManagementError::NeedsLogin {
                status: 7,
                message: "peer login has expired, please log in once more".into(),
            },
            &secrets,
        );
        assert_eq!(d, "status 7: peer login has expired, please log in once more");
        // newlines collapse (single-line log discipline)
        let d = redacted_login_detail(
            &ManagementError::Auth { status: 401, message: "line1\nline2".into() },
            &secrets,
        );
        assert_eq!(d, "status 401: line1 line2");
        // over-long message: capped with an ellipsis
        let d = redacted_login_detail(
            &ManagementError::Server { status: 500 },
            &ConnectorSecrets::default(),
        );
        assert_eq!(d, "status 500");
        let long = "x".repeat(300);
        let d = redacted_login_detail(
            &ManagementError::Auth { status: 401, message: long.clone() },
            &ConnectorSecrets::default(),
        );
        assert!(d.starts_with("status 401: "), "{d}");
        assert!(d.ends_with('…') && d.chars().count() < long.chars().count() + 16, "{d}");
        // classes without a server message stay empty (no invented text)
        assert!(redacted_login_detail(&ManagementError::Timeout, &secrets).is_empty());
        assert!(redacted_login_detail(&ManagementError::Network("refused".into()), &secrets).is_empty());
        // short secrets (under the scrub floor) are not loop-replaced, but
        // real setup keys are far longer than the floor
        let tiny = ConnectorSecrets { setup_key: "abc".into() };
        let d = redacted_login_detail(
            &ManagementError::Auth { status: 401, message: "abcd".into() },
            &tiny,
        );
        assert_eq!(d, "status 401: abcd");
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

    /// 0051 §7 item 3 → STAGE 4：`connector_status()` 的 `ice_sockets` 段只报
    /// 受保护 UDP 源的队列深度（`queued`/`taken`；-1 = 无源）。曾随段携带的
    /// 每 peer socket 需求（`need_per_peer`）已随壳层 socket 池一起删除
    /// （官方对齐审计 §2.7 stage 4）。逐字冻结：这一段仍是根对象的尾段。
    #[test]
    fn ice_sockets_section_carries_the_socket_queue_depth() {
        assert_eq!(
            ice_sockets_section(7, 12),
            "\"ice_sockets\":{\"queued\":7,\"taken\":12}"
        );
        assert_eq!(
            ice_sockets_section(-1, -1),
            "\"ice_sockets\":{\"queued\":-1,\"taken\":-1}",
            "-1 = no protected-UDP source attached"
        );
        // 接进一份状态文档后仍须被严格读取器接受（尾段追加，前文字段不动，
        // 且与生产路径同一段拼接代码——缺右花括号这类错误必须在这里就暴露）
        let doc = with_ice_sockets_section(
            "{\"running\":true,\"state\":\"disconnected\"}",
            3,
            4,
            false,
            Some(0),
        );
        assert_eq!(
            doc,
            "{\"running\":true,\"state\":\"disconnected\",\
             \"ice_snapshot_stale\":false,\"ice_snapshot_age_ms\":0,\
             \"ice_sockets\":{\"queued\":3,\"taken\":4}}"
        );
        assert!(
            matches!(config::parse_document(&doc), Ok(Json::Obj(_))),
            "the strict reader accepts the full appended document: {doc}"
        );
        assert!(
            doc.ends_with(",\"ice_sockets\":{\"queued\":3,\"taken\":4}}"),
            "the ice_sockets section stays the trailing section of the root object: {doc}"
        );
        // 2026-10-09: the freshness marks — a cached sample renders stale with
        // its real age; every pre-existing key keeps its position (the marks
        // ride between `ice_peers` and the trailing `ice_sockets` section).
        let stale_doc = with_ice_sockets_section(
            "{\"running\":true,\"state\":\"connected\",\"ice_peers\":[]}",
            3,
            4,
            true,
            Some(8421),
        );
        assert_eq!(
            stale_doc,
            "{\"running\":true,\"state\":\"connected\",\"ice_peers\":[],\
             \"ice_snapshot_stale\":true,\"ice_snapshot_age_ms\":8421,\
             \"ice_sockets\":{\"queued\":3,\"taken\":4}}"
        );
        assert!(matches!(config::parse_document(&stale_doc), Ok(Json::Obj(_))));
        // ... and "no successful ICE read to age" is the document's own
        // `null` — NOT a negative sentinel, which the strict reader rejects.
        let unread_doc = with_ice_sockets_section(
            "{\"running\":true,\"state\":\"connecting\"}",
            0,
            0,
            true,
            None,
        );
        assert!(
            unread_doc.contains(
                "\"ice_snapshot_stale\":true,\"ice_snapshot_age_ms\":null,\
                 \"ice_sockets\":{\"queued\":0,\"taken\":0}}"
            ),
            "{unread_doc}"
        );
        assert!(matches!(config::parse_document(&unread_doc), Ok(Json::Obj(_))), "{unread_doc}");
    }

    /// 2026-10-09（ArkTS 主线程冻结修复）：orchestrator 锁被「另一线程长时间
    /// 持有」（真机形态：ICE pump 在 gather 里持锁 8.4–11 s）时，`status_json()`
    /// 必须于有界时间内返回：`try_lock` 读不到就回最近一次成功的 ICE 子快照
    /// （`ice`/`ice_peers` 两者同一份），以
    /// `ice_snapshot_stale:true` + 真实 `ice_snapshot_age_ms` 标明；从未成功
    /// 读过则输出既有的「未知」形态并 stale、age=null —— 绝不把在线 peer 读成 0
    /// 或把 ICE 面抹成空，也绝不阻塞。锁释放后下一次读取必须重新新鲜且拿到更新
    /// 后的值。
    #[tokio::test(flavor = "multi_thread")]
    async fn status_json_serves_the_last_ice_snapshot_while_the_orchestrator_is_busy() {
        // 读一个 JSON 数字字段（本测试的 stale/age/peers 断言用）。
        fn num_field(doc: &str, key: &str) -> i64 {
            let needle = format!("\"{key}\":");
            let at = doc.find(&needle).unwrap_or_else(|| panic!("missing {key}: {doc}"))
                + needle.len();
            let rest = &doc[at..];
            let end = rest
                .find(|c: char| !c.is_ascii_digit() && c != '-')
                .unwrap_or(rest.len());
            rest[..end].parse().unwrap_or_else(|e| panic!("{key} is not an integer: {e} in {doc}"))
        }

        // 直接搭一个「无 worker / 无 pump」的 handle：本测试自己持有
        // orchestrator 锁，任何后台线程都会把时序变成掷骰子。
        fn bare_status_handle(
            shared: Arc<ConnectorShared>,
            wg: Arc<dyn WgPeerApplier>,
            ice: Arc<Mutex<PeerIceOrchestrator>>,
        ) -> Arc<ConnectorHandle> {
            struct NeverConnects;
            impl ManagementFactory for NeverConnects {
                fn connect(
                    &self,
                ) -> Pin<
                    Box<
                        dyn Future<Output = Result<ManagementGrpcClient, ManagementError>>
                            + Send
                            + '_,
                    >,
                > {
                    Box::pin(async { Err(ManagementError::Network("status-busy-probe".into())) })
                }
            }
            struct NeverApplies;
            impl ConfigApplier for NeverApplies {
                fn apply(&self, _map: &NetworkMap) {}
                fn clear(&self) {}
            }
            let (network_change, _rx) = tokio::sync::watch::channel(0u64);
            // the production spawn always attaches the UDP source (an empty one
            // here, on BOTH the shared snapshot and the handle): without it the
            // frozen `ice_sockets` section renders the `-1` no-source sentinels,
            // which is a different (pre-existing) shape than the one this test
            // is about.
            let udp = Arc::new(
                crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"),
            );
            let _ = shared.ice_sockets.set(udp.clone());
            Arc::new(ConnectorHandle {
                shared,
                wg,
                host: Arc::new(NeverApplies),
                network_change,
                generation: 0,
                management_factory: Arc::new(NeverConnects),
                logout_slot: Arc::new(Mutex::new(None)),
                stop_flag: Arc::new(AtomicBool::new(false)),
                socket_source: None,
                ice_sockets: Some(udp),
                ice_mux: None,
                signal_sockets: None,
                wg_feed: None,
                ice,
                management_dial: None,
                worker: Mutex::new(None),
                runtime: tokio::runtime::Handle::current(),
            })
        }

        let wg: Arc<dyn WgPeerApplier> = Arc::new(WgPeerRegistry::new());
        let ice = Arc::new(Mutex::new(PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![])),
            socks: Arc::new(
                crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"),
            ),
            signal: Arc::new(LoggingSignalExchange::default()),
            wg: wg.clone(),
            tie_breaker: Some(7),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        })));
        ice.lock_poison().set_peers(&["QUJD".to_string()]);

        // handle A 先成功读一次（缓存变热）；handle B 从未成功读过。
        let shared_a = Arc::new(ConnectorShared::new(false, None));
        let _ = shared_a.ice.set(ice.clone());
        let handle_a = bare_status_handle(shared_a.clone(), wg.clone(), ice.clone());
        let shared_b = Arc::new(ConnectorShared::new(false, None));
        let _ = shared_b.ice.set(ice.clone());
        let handle_b = bare_status_handle(shared_b, wg.clone(), ice.clone());

        // ① 新鲜读：stale=false / age=0 / 拿到真实的一 peer 快照。
        let fresh = handle_a.status_json();
        assert!(fresh.contains("\"ice_snapshot_stale\":false"), "{fresh}");
        assert_eq!(num_field(&fresh, "ice_snapshot_age_ms"), 0, "{fresh}");
        assert!(fresh.contains("\"ice\":{\"peers\":1,"), "{fresh}");
        assert!(
            fresh.contains("\"ice_peers\":[{\"pub_key\":\"QUJD\",\"state\":\"idle\""),
            "{fresh}"
        );
        assert!(matches!(config::parse_document(&fresh), Ok(Json::Obj(_))), "{fresh}");

        // ② 另一线程持锁（pump 在 gather 里的形态：锁被占用且不放手，由测试显式
        //    放行，不靠 sleep 猜时序）。
        let (acquired_tx, acquired_rx) = std::sync::mpsc::channel::<()>();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let ice_holder = ice.clone();
        let holder = std::thread::spawn(move || {
            let _guard = ice_holder.lock_poison();
            acquired_tx.send(()).expect("signal acquisition");
            release_rx.recv().expect("hold until the test releases");
        });
        acquired_rx.recv().expect("the holder took the orchestrator lock");

        let t0 = std::time::Instant::now();
        let busy_a = handle_a.status_json();
        let busy_a_elapsed = t0.elapsed();
        let t1 = std::time::Instant::now();
        let busy_b = handle_b.status_json();
        let busy_b_elapsed = t1.elapsed();
        assert!(
            busy_a_elapsed < Duration::from_millis(50),
            "a busy orchestrator must not block the status read: {busy_a_elapsed:?}"
        );
        assert!(
            busy_b_elapsed < Duration::from_millis(50),
            "a busy orchestrator must not block the never-read status read: {busy_b_elapsed:?}"
        );

        // A：缓存值原样返回（不是 0 peer、不是空 ICE 面），stale + 真实 age。
        assert!(busy_a.contains("\"ice_snapshot_stale\":true"), "{busy_a}");
        assert!(busy_a.contains("\"ice\":{\"peers\":1,"), "{busy_a}");
        assert!(
            busy_a.contains("\"ice_peers\":[{\"pub_key\":\"QUJD\",\"state\":\"idle\""),
            "{busy_a}"
        );
        let age = num_field(&busy_a, "ice_snapshot_age_ms");
        assert!((0..=60_000).contains(&age), "a cached sample carries its real age: {age}");
        assert!(matches!(config::parse_document(&busy_a), Ok(Json::Obj(_))), "{busy_a}");

        // B：从未成功读过 ⇒ 既有的「未知」形态（0 peer 摘要 + ice_peers:null），
        //    但 stale=true 且 age=null，不会被当成真实状态。
        //    （age 用文档自己的 null 而不是负数哨兵：负数会被 crate 自己的严格
        //    读取器拒收，N2-H freeze facts 就是用它读这份文档的。）
        assert!(busy_b.contains("\"ice_snapshot_stale\":true"), "{busy_b}");
        assert!(busy_b.contains("\"ice_snapshot_age_ms\":null"), "{busy_b}");
        assert!(busy_b.contains("\"ice\":{\"peers\":0,"), "{busy_b}");
        assert!(busy_b.contains("\"ice_peers\":null"), "{busy_b}");

        // ③ 放锁：下一次读取重新新鲜，且拿到更新后的值（B 也补上缓存）。
        release_tx.send(()).expect("release the orchestrator lock");
        holder.join().expect("the holder thread");

        let after = handle_a.status_json();
        assert!(after.contains("\"ice_snapshot_stale\":false"), "{after}");
        assert_eq!(num_field(&after, "ice_snapshot_age_ms"), 0, "{after}");
        let after_b = handle_b.status_json();
        assert!(after_b.contains("\"ice_snapshot_stale\":false"), "{after_b}");
        assert!(after_b.contains("\"ice\":{\"peers\":1,"), "{after_b}");

        ice.lock_poison().set_peers(&["QUJD".to_string(), "QkJC".to_string()]);
        let updated = handle_a.status_json();
        assert!(updated.contains("\"ice_snapshot_stale\":false"), "{updated}");
        assert_eq!(num_field(&updated, "ice_snapshot_age_ms"), 0, "{updated}");
        assert!(updated.contains("\"ice\":{\"peers\":2,"), "{updated}");
        assert!(updated.contains("\"pub_key\":\"QUJD\""), "{updated}");
        assert!(updated.contains("\"pub_key\":\"QkJC\""), "{updated}");
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
                // 0.0.26 gap 3: a NON-zero value on purpose — the field must be
                // proven exported, not merely defaulted to 0.
                unknown_peer_drops: 4,
                // N13-D1: the per-peer attribution tail — one peer with a
                // live session riding the carrier, one registered peer with
                // no session (both entry shapes frozen here). N13-D4 appends
                // the per-peer transfer counters (plaintext bytes) at the END
                // of each entry.
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
                        tx_bytes: 512,
                        rx_bytes: 1024,
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
                        tx_bytes: 0,
                        rx_bytes: 0,
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
            peer_relays: None,
            ice_peers: None,
            ice_mux: None,
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
        // document — the `wg` object only moves its own tail). N13-D4: the
        // per-peer transfer counters (`tx_bytes`/`rx_bytes`, PLAINTEXT bytes)
        // are appended LAST inside each peer entry, so positional readers of
        // the N13-D1/D2 head are unaffected.
        assert!(
            json.contains(
                "\"wg\":{\"fed_socket\":true,\"fed_tun\":true,\"device_up\":true,\"ready\":true,\
                 \"peers_with_session\":1,\"handshakes\":3,\"tx_packets\":9,\"tx_bytes\":900,\
                 \"rx_packets\":8,\"rx_bytes_to_tun\":800,\
                 \"dropped_no_route\":1,\"decrypt_errors\":2,\"unknown_peer_drops\":4,\
                 \"carrier_tx_packets\":0,\
                 \"carrier_tx_bytes\":0,\"carrier_rejects\":0,\"carrier_takeovers\":0,\
                 \"direct_restores\":0,\"wg_peers\":[\
                 {\"pub_key_b64\":\"bjEzLWQxLXBlZXItc2Vzc2lvbg==\",\"endpoint\":null,\
                 \"allowed_ips\":[\"10.30.30.2/32\"],\"session_established\":true,\
                 \"expired\":false,\"last_handshake_s\":12,\
                 \"egress_local\":\"192.168.1.5:51820\",\"carrier_attached\":true,\
                 \"on_carrier\":true,\"tx_bytes\":512,\"rx_bytes\":1024},\
                 {\"pub_key_b64\":\"bjEzLWQxLXBlZXItaWRsZQ==\",\
                 \"endpoint\":\"203.0.113.9:51820\",\
                 \"allowed_ips\":[\"10.30.30.3/32\",\"10.30.30.4/32\"],\
                 \"session_established\":false,\"expired\":false,\"last_handshake_s\":null,\
                 \"egress_local\":null,\"carrier_attached\":false,\"on_carrier\":false,\
                 \"tx_bytes\":0,\"rx_bytes\":0}]}"
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
        // the relay segment stays the LAST pre-existing section of the
        // document (the ets consumer reads by field; N13-D1 must not move it);
        // the 2026-10-06 `peer_relays` tail follows it ADDITIVELY, the
        // 2026-10-07 `ice_peers` tail follows THAT and the 0.0.26 `ice_mux`
        // tail follows those.
        assert!(
            json.ends_with(&format!(
                "\"relay\":{},\"peer_relays\":null,\"ice_peers\":null,\"ice_mux\":null}}",
                status.relay.to_json()
            )),
            "relay keeps its position; peer_relays / ice_peers / ice_mux are the tails: {json}"
        );
        assert!(matches!(config::parse_document(&json), Ok(Json::Obj(_))));

        // 2026-10-06 (netbird status -d): `peer_relays` — the ACTUAL per-peer
        // relay lane map — is TAIL-appended AFTER the frozen `relay` object.
        // Shape contract: `null` = no carrier armed, `[]` = carrier armed with
        // no lane attached, entries = the attached lanes (public key + the
        // relay URL the lane rides), public-key sorted.
        assert_eq!(
            peer_relays_json(None),
            "\"peer_relays\":null",
            "no carrier armed is the honest null, never an empty array"
        );
        assert_eq!(peer_relays_json(Some(&[])), "\"peer_relays\":[]");
        let mut with_lanes = status.clone();
        with_lanes.peer_relays = Some(vec![
            PeerRelayStatus {
                pub_key_b64: "YWFhYQ==".to_string(),
                relay_url: "rels://ctrl.example:28443".to_string(),
            },
            PeerRelayStatus {
                pub_key_b64: "YmJiYg==".to_string(),
                relay_url: "rel://127.0.0.1:33000".to_string(),
            },
        ]);
        let lanes_json = with_lanes.to_json();
        assert!(
            lanes_json.contains(
                "\"peer_relays\":[{\"pub_key_b64\":\"YWFhYQ==\",\
                 \"relay_url\":\"rels://ctrl.example:28443\"},\
                 {\"pub_key_b64\":\"YmJiYg==\",\"relay_url\":\"rel://127.0.0.1:33000\"}]"
            ),
            "{lanes_json}"
        );
        assert!(
            lanes_json.contains(&format!(
                "\"relay\":{},\"peer_relays\":[",
                status.relay.to_json()
            )),
            "the frozen relay object keeps its exact shape before the tail: {lanes_json}"
        );
        assert!(
            lanes_json.ends_with("],\"ice_peers\":null,\"ice_mux\":null}"),
            "peer_relays keeps its shape and the ice_peers/ice_mux tails follow (additive only): {lanes_json}"
        );
        assert!(matches!(config::parse_document(&lanes_json), Ok(Json::Obj(_))));

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
            wg_apply_failed: false,
            wg_apply_errors: 0,
            logout_ok: None,
            terminal: false,
            ice: IceOrchestratorSummary::default(),
            signal: SignalLinkStatus::default(),
            wg: crate::wg_device::WgDataplaneStatus::default(),
            recreate: RecreateStatus::default(),
            relay: RelayStatus::disabled(),
            peer_relays: None,
            ice_peers: None,
            ice_mux: None,
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
                 \"dropped_no_route\":0,\"decrypt_errors\":0,\"unknown_peer_drops\":0,\
                 \"carrier_tx_packets\":0,\
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
        assert!(
            empty.contains("\"relay\":")
                && empty.ends_with("\"peer_relays\":null,\"ice_peers\":null,\"ice_mux\":null}"),
            "the no-carrier default renders all three tails as null: {empty}"
        );
        assert!(matches!(config::parse_document(&empty), Ok(Json::Obj(_))));
    }

    /// 2026-10-07 (per-peer ICE detail): the `ice_peers` tail is the
    /// three-state contract (`null` = no orchestrator at all, `[]` =
    /// orchestrator armed with ZERO peers, array = one entry per peer,
    /// public-key sorted) with an EXACT byte shape per entry: state token,
    /// role, the selected pair's `{type,address,port}` endpoints (`null` when
    /// NOTHING was selected — the first collected candidate is never
    /// substituted), the landing/reachability facts and the class-only
    /// `last_error` (the same safe serialization as the frozen `ice` summary).
    /// No latency, no state-change timestamp: neither is retained anywhere, so
    /// neither is invented here.
    #[test]
    fn status_ice_peers_tail_exact_serialization_and_reader_contract() {
        // no orchestrator → the honest null (an `[]` would claim one exists)
        assert_eq!(ice_peers_json(None), "\"ice_peers\":null");
        // orchestrator armed, ZERO peers → the honest empty array
        assert_eq!(ice_peers_json(Some(&[])), "\"ice_peers\":[]");

        let idle = PeerIceDetail {
            pub_key: "QUJD".to_string(),
            state: crate::peer_conn::PeerIceState::Idle,
            controlling: None,
            selected_local: None,
            selected_remote: None,
            endpoint_applied: false,
            reachable: false,
            last_error: None,
        };
        let connected = PeerIceDetail {
            pub_key: "REVG".to_string(),
            state: crate::peer_conn::PeerIceState::Connected,
            controlling: Some(true),
            selected_local: Some(IceCandidateEndpoint {
                typ: crate::ice::CandidateType::Host,
                address: "192.168.1.5".to_string(),
                port: 51820,
            }),
            selected_remote: Some(IceCandidateEndpoint {
                typ: crate::ice::CandidateType::Srflx,
                address: "203.0.113.9".to_string(),
                port: 51821,
            }),
            endpoint_applied: true,
            reachable: true,
            last_error: None,
        };
        // Disconnected KEEPS its selected pair (the live session snapshot
        // still holds it) but `reachable` stays FALSE: a retained endpoint is
        // never upgraded into a connection, and the class-only error renders.
        let disconnected = PeerIceDetail {
            pub_key: "R0hJ".to_string(),
            state: crate::peer_conn::PeerIceState::Disconnected,
            controlling: Some(false),
            selected_local: Some(IceCandidateEndpoint {
                typ: crate::ice::CandidateType::Prflx,
                address: "10.0.0.7".to_string(),
                port: 41000,
            }),
            selected_remote: Some(IceCandidateEndpoint {
                typ: crate::ice::CandidateType::Relay,
                address: "198.51.100.5".to_string(),
                port: 28443,
            }),
            endpoint_applied: true,
            reachable: false,
            last_error: Some(ErrorClass::Auth { status: 7 }),
        };
        let details = ice_peers_json(Some(&[idle.clone(), connected.clone(), disconnected.clone()]));
        assert_eq!(
            details,
            "\"ice_peers\":[\
             {\"pub_key\":\"QUJD\",\"state\":\"idle\",\"controlling\":null,\
             \"selected_local\":null,\"selected_remote\":null,\
             \"endpoint_applied\":false,\"reachable\":false,\"last_error\":null},\
             {\"pub_key\":\"REVG\",\"state\":\"connected\",\"controlling\":true,\
             \"selected_local\":{\"type\":\"host\",\"address\":\"192.168.1.5\",\"port\":51820},\
             \"selected_remote\":{\"type\":\"srflx\",\"address\":\"203.0.113.9\",\"port\":51821},\
             \"endpoint_applied\":true,\"reachable\":true,\"last_error\":null},\
             {\"pub_key\":\"R0hJ\",\"state\":\"disconnected\",\"controlling\":false,\
             \"selected_local\":{\"type\":\"prflx\",\"address\":\"10.0.0.7\",\"port\":41000},\
             \"selected_remote\":{\"type\":\"relay\",\"address\":\"198.51.100.5\",\"port\":28443},\
             \"endpoint_applied\":true,\"reachable\":false,\
             \"last_error\":{\"class\":\"auth\",\"status\":7}}]"
        );
        // every candidate type token is reachable through the projection
        for token in [
            "\"type\":\"host\"",
            "\"type\":\"srflx\"",
            "\"type\":\"prflx\"",
            "\"type\":\"relay\"",
        ] {
            assert!(details.contains(token), "missing {token}: {details}");
        }
        // the deliberately absent facts stay absent
        assert!(!details.contains("rtt"), "no latency is retained: {details}");
        assert!(
            !details.contains("timestamp") && !details.contains("_ms"),
            "no state-change timestamp is retained: {details}"
        );

        // document level: `peer_relays` keeps its exact bytes and `ice_peers`
        // rides immediately after it (the additive tail); 0.0.26 appends the
        // `ice_mux` tail after THAT, and the strict reader accepts the whole
        // document.
        let registry = WgPeerRegistry::new();
        let shared = ConnectorShared::new(false, None);
        let mut status = ConnectorStatus::snapshot(
            &shared,
            &registry,
            Some(vec![PeerRelayStatus {
                pub_key_b64: "QUJD".to_string(),
                relay_url: "rel://relay.example:33000".to_string(),
            }]),
            &shared.ice_status_sample(),
        );
        status.ice_peers = Some(vec![idle, connected, disconnected]);
        status.ice_mux = Some(crate::ice_mux::MuxStats {
            reads: 7,
            xor_hits: 3,
            xor_misses: 5,
            xor_filled: 2,
            ..Default::default()
        });
        let json = status.to_json();
        let lanes = "\"peer_relays\":[{\"pub_key_b64\":\"QUJD\",\
                     \"relay_url\":\"rel://relay.example:33000\"}]";
        let mux = "\"ice_mux\":{\"reads\":7,\"xor_hits\":3,\"xor_misses\":5,\
                   \"xor_expired\":0,\"xor_filled\":2,\"overlay_dropped\":0,\
                   \"stun_unknown\":0,\"data_unknown\":0,\"queue_dropped\":0}";
        assert!(
            json.contains(&format!("{lanes},{details},{mux}")),
            "ice_peers must ride immediately after peer_relays: {json}"
        );
        assert!(
            json.ends_with(&format!("{mux}}}")),
            "ice_mux is the ConnectorStatus document tail: {json}"
        );
        assert!(
            json.rfind("\"peer_relays\":").expect("peer_relays") < json.rfind("\"ice_peers\":").expect("ice_peers"),
            "the tail order is peer_relays then ice_peers: {json}"
        );
        assert!(
            json.rfind("\"ice_peers\":").expect("ice_peers") < json.rfind("\"ice_mux\":").expect("ice_mux"),
            "the tail order is ice_peers then ice_mux: {json}"
        );

        // the contract reader: by key, the array carries exactly the eight
        // documented keys in order; each endpoint object is exactly
        // {type,address,port} in order; the frozen last_error object is the
        // same safe class+status shape (never a message).
        fn obj_keys(v: &Json) -> Vec<String> {
            match v {
                Json::Obj(entries) => entries.iter().map(|(k, _)| k.clone()).collect(),
                other => panic!("expected a JSON object, got {other:?}"),
            }
        }
        fn obj_get<'a>(v: &'a Json, key: &str) -> &'a Json {
            match v {
                Json::Obj(entries) => entries
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v)
                    .unwrap_or_else(|| panic!("missing key {key} in {v:?}")),
                other => panic!("expected a JSON object, got {other:?}"),
            }
        }
        let Json::Obj(root) = config::parse_document(&json).expect("strict reader") else {
            panic!("the status document must be an object");
        };
        let root = Json::Obj(root);
        let Json::Arr(items) = obj_get(&root, "ice_peers") else {
            panic!("ice_peers must be an array");
        };
        assert_eq!(items.len(), 3, "one entry per peer");
        let joined = |v: &Json| obj_keys(v).join(",");
        assert_eq!(
            joined(&items[0]),
            "pub_key,state,controlling,selected_local,selected_remote,\
             endpoint_applied,reachable,last_error",
            "the per-peer key order is the frozen contract"
        );
        assert_eq!(joined(obj_get(&items[1], "selected_local")), "type,address,port");
        assert_eq!(joined(obj_get(&items[1], "selected_remote")), "type,address,port");
        assert_eq!(obj_get(obj_get(&items[1], "selected_local"), "port"), &Json::Num(51820));
        assert_eq!(
            obj_get(obj_get(&items[2], "selected_remote"), "type"),
            &Json::Str("relay".to_string())
        );
        assert_eq!(obj_get(&items[0], "controlling"), &Json::Null, "no session → role unknown");
        assert_eq!(obj_get(&items[0], "selected_local"), &Json::Null, "nothing selected → null");
        assert_eq!(obj_get(&items[1], "reachable"), &Json::Bool(true));
        assert_eq!(
            obj_get(&items[2], "reachable"),
            &Json::Bool(false),
            "a retained endpoint is not a connection"
        );
        assert_eq!(
            obj_get(&items[2], "last_error"),
            &Json::Obj(vec![
                ("class".to_string(), Json::Str("auth".to_string())),
                ("status".to_string(), Json::Num(7)),
            ]),
            "the class-only error shape is preserved"
        );
        let Json::Arr(peer_relays) = obj_get(&root, "peer_relays") else {
            panic!("peer_relays must be an array");
        };
        assert_eq!(peer_relays.len(), 1);
        assert_eq!(
            joined(&peer_relays[0]),
            "pub_key_b64,relay_url",
            "the pre-existing entry shape is unchanged"
        );
        // 0.0.26 gap 1: the `ice_mux` tail is the strict-reader-visible counter
        // object and carries exactly the documented keys in order.
        let mux = obj_get(&root, "ice_mux");
        let Json::Obj(mux_entries) = mux else {
            panic!("ice_mux must be an object here");
        };
        assert_eq!(
            mux_entries.iter().map(|(k, _)| k.clone()).collect::<Vec<String>>().join(","),
            "reads,xor_hits,xor_misses,xor_expired,xor_filled,overlay_dropped,\
             stun_unknown,data_unknown,queue_dropped",
            "the ice_mux key order is the frozen contract"
        );
        assert_eq!(obj_get(mux, "xor_misses"), &Json::Num(5));
    }

    /// 2026-10-07 (2026-10-09: caller-supplied sample): `ConnectorStatus::snapshot`
    /// fills BOTH `ice` (summary) and `ice_peers` (details) from ONE
    /// [`IceStatusSample`] — the sample the caller took with the SINGLE
    /// non-blocking [`ConnectorShared::ice_status_sample`] acquisition — so the
    /// two surfaces always describe the same peer set (a second read could
    /// straddle a `set_peers`/re-selection and publish counts and details of
    /// different snapshots). Three states: no orchestrator → `null`;
    /// orchestrator with ZERO peers → `[]`; armed with peers → one
    /// public-key-sorted detail per peer.
    #[test]
    fn status_snapshot_reads_the_orchestrator_once_for_summary_and_details() {
        let wg: Arc<dyn WgPeerApplier> = Arc::new(WgPeerRegistry::new());

        // (1) no orchestrator (unit-test construction): null, never []
        let bare = ConnectorShared::new(false, None);
        let status = ConnectorStatus::snapshot(&bare, wg.as_ref(), None, &bare.ice_status_sample());
        assert_eq!(status.ice_peers, None);
        assert_eq!(status.ice, IceOrchestratorSummary::default());
        assert!(
            status.to_json().ends_with("\"peer_relays\":null,\"ice_peers\":null,\"ice_mux\":null}"),
            "{}",
            status.to_json()
        );

        // (2) orchestrator armed with ZERO peers: [] (not null)
        let shared = ConnectorShared::new(false, None);
        let ice = Arc::new(Mutex::new(PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![])),
            socks: Arc::new(
                crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"),
            ),
            signal: Arc::new(LoggingSignalExchange::default()),
            wg: wg.clone(),
            tie_breaker: Some(7),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        })));
        let _ = shared.ice.set(ice.clone());
        let empty_peers =
            ConnectorStatus::snapshot(&shared, wg.as_ref(), None, &shared.ice_status_sample());
        assert_eq!(empty_peers.ice_peers, Some(vec![]), "armed with zero peers is [], never null");
        assert_eq!(empty_peers.ice, IceOrchestratorSummary::default());
        assert!(
            empty_peers.to_json().ends_with(",\"ice_peers\":[],\"ice_mux\":null}"),
            "{}",
            empty_peers.to_json()
        );

        // (3) one peer with no session yet: Idle / unknown role / unknown
        // endpoints — and the summary and the detail table describe the SAME
        // peer set (the one-lock rule, observed on the production assembly).
        ice.lock_poison().set_peers(&["UEVFUjA=".to_string()]);
        let status = ConnectorStatus::snapshot(&shared, wg.as_ref(), None, &shared.ice_status_sample());
        let details = status.ice_peers.clone().expect("armed orchestrator → Some");
        assert_eq!(details.len(), status.ice.peers, "counts and details are one snapshot");
        assert_eq!(status.ice.idle, 1);
        assert_eq!(details[0].state, crate::peer_conn::PeerIceState::Idle);
        assert_eq!(details[0].controlling, None);
        assert_eq!(details[0].selected_local, None);
        assert_eq!(details[0].selected_remote, None);
        assert!(!details[0].reachable);
        assert_eq!(details, ice.lock_poison().ice_details(), "equal to the orchestrator's own read");
        let json = status.to_json();
        assert!(
            json.ends_with(
                ",\"ice_peers\":[{\"pub_key\":\"UEVFUjA=\",\"state\":\"idle\",\
                 \"controlling\":null,\"selected_local\":null,\"selected_remote\":null,\
                 \"endpoint_applied\":false,\"reachable\":false,\"last_error\":null}],\
                 \"ice_mux\":null}"
            ),
            "{json}"
        );
        assert!(matches!(config::parse_document(&json), Ok(Json::Obj(_))));
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

    /// N6 wiring: `apply_update` must hand the map's managed routes to the WG
    /// seam (the registry RECORDS them; the real device installs them into
    /// its routing table — `wg_device::route_overlay_tests` pins the data
    /// plane). Full-snapshot semantics: a later map without routes clears
    /// the seam's route set.
    #[test]
    fn apply_update_hands_managed_routes_to_the_wg_seam() {
        let shared = ConnectorShared::new(false, None);
        let registry = Arc::new(WgPeerRegistry::default());
        shared.apply_update(
            registry.as_ref(),
            &NoopHost,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: None,
                network_map: Some(shell_test_map()),
            },
        );
        let routes = registry.managed_routes();
        assert_eq!(routes.len(), 2, "both map routes must reach the WG seam");
        assert!(
            routes
                .iter()
                .any(|r| r.network.prefix_len == 0 && r.peer == "relay" && r.metric == 9999)
        );
        assert!(
            routes
                .iter()
                .any(|r| r.network.addr == [172, 16, 0, 0] && r.network.prefix_len == 12 && r.metric == 10)
        );
        assert_eq!(registry.len(), 3, "peers ride the same snapshot");
        // a later snapshot without routes clears the seam's route set
        shared.apply_update(
            registry.as_ref(),
            &NoopHost,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: None,
                network_map: Some(NetworkMap { routes: vec![], ..shell_test_map() }),
            },
        );
        assert!(registry.managed_routes().is_empty(), "route set is full-snapshot");
    }

    // decision B: startup route selection ------------------------------------

    /// Recording host seam: captures the effective route list of every map
    /// `apply_update` hands to the shell side.
    #[derive(Default)]
    struct RecordingHost {
        routes: Mutex<Vec<String>>,
    }

    impl RecordingHost {
        fn routes(&self) -> Vec<String> {
            self.routes.lock_poison().clone()
        }
    }

    impl ConfigApplier for RecordingHost {
        fn apply(&self, map: &NetworkMap) {
            *self.routes.lock_poison() =
                map.routes.iter().map(|r| route_network_string(&r.network)).collect();
        }
        fn clear(&self) {
            self.routes.lock_poison().clear();
        }
    }

    /// The `docs/route-audit.md` HA shape: ONE logical network
    /// (`HostInternal`, 192.168.50.0/24) carried by TWO candidates (metric
    /// 200/100 on two peers) plus an unrelated subnet (`corp`, 172.16.0.0/12).
    fn route_selection_test_map(serial: u64) -> NetworkMap {
        use crate::network_map::ManagedRoute;
        let mut map = shell_test_map();
        map.serial = serial;
        map.routes = vec![
            ManagedRoute {
                id: "ha-200".into(),
                network: config::Route { addr: [192, 168, 50, 0], prefix_len: 24 },
                domains: vec![],
                net_id: "HostInternal".into(),
                network_type: 1,
                peer: "peer-a".into(),
                metric: 200,
                masquerade: true,
                keep_route: true,
                skip_auto_apply: false,
            },
            ManagedRoute {
                id: "ha-100".into(),
                network: config::Route { addr: [192, 168, 50, 0], prefix_len: 24 },
                domains: vec![],
                net_id: "HostInternal".into(),
                network_type: 1,
                peer: "peer-b".into(),
                metric: 100,
                masquerade: true,
                keep_route: true,
                skip_auto_apply: false,
            },
            ManagedRoute {
                id: "corp".into(),
                network: config::Route { addr: [172, 16, 0, 0], prefix_len: 12 },
                domains: vec![],
                net_id: "corp".into(),
                network_type: 1,
                peer: "peer-a".into(),
                metric: 10,
                masquerade: false,
                keep_route: true,
                skip_auto_apply: false,
            },
        ];
        map
    }

    fn selector_from(json: &str) -> RouteSelector {
        RouteSelector::from_json(json).expect("test selector state")
    }

    /// One map through the REAL `apply_update` path.
    fn apply_selected_map(
        shared: &ConnectorShared,
        wg: &dyn WgPeerApplier,
        host: &dyn ConfigApplier,
        map: NetworkMap,
    ) {
        shared.apply_update(
            wg,
            host,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: None,
                network_map: Some(map),
            },
        );
    }

    /// A start config WITHOUT `route_selection` is the default selector and the
    /// connector applies every advertised route — the pre-existing behavior,
    /// byte for byte (and the exact `connector_routes_json` document).
    #[test]
    fn absent_route_selection_keeps_the_preexisting_behavior_and_is_the_start_default() {
        let cfg = ConnectorConfig::from_json(&test_config_json()).expect("minimal config");
        assert_eq!(cfg.route_selection, RouteSelector::new());
        assert_eq!(
            cfg.route_selection.to_json(),
            "{\"selected_routes\":[],\"deselected_routes\":[],\"deselect_all\":false}"
        );

        let shared = ConnectorShared::new_selected(false, None, cfg.route_selection.clone());
        let registry = Arc::new(WgPeerRegistry::default());
        let host = RecordingHost::default();
        apply_selected_map(&shared, registry.as_ref(), &host, route_selection_test_map(7));

        assert_eq!(registry.managed_routes().len(), 3, "all three rows (2 HA + corp) reach the WG seam");
        assert_eq!(
            host.routes(),
            vec!["192.168.50.0/24", "192.168.50.0/24", "172.16.0.0/12"],
            "the shell receives the same effective list"
        );
        let shell = shared.network_config_json();
        assert!(shell.contains("192.168.50.0/24") && shell.contains("172.16.0.0/12"), "{shell}");
        assert_eq!(
            shared.routes_json(),
            "{\"available\":true,\"serial\":7,\"selection\":{\"selected_routes\":[],\"deselected_routes\":[],\"deselect_all\":false},\"networks\":[{\"net_id\":\"HostInternal\",\"networks\":[\"192.168.50.0/24\"],\"candidate_count\":2,\"enabled\":true,\"is_exit\":false},{\"net_id\":\"corp\",\"networks\":[\"172.16.0.0/12\"],\"candidate_count\":1,\"enabled\":true,\"is_exit\":false}]}"
        );
    }

    /// The startup preference is applied BEFORE the first map reaches any
    /// consumer: no disabled route in the WG seam / shell snapshot /
    /// `net_routes_all` / host apply, while the catalog keeps BOTH HA rows.
    #[test]
    fn startup_route_selection_filters_every_consumer_but_keeps_the_unfiltered_catalog() {
        let shared = ConnectorShared::new_selected(
            false,
            None,
            selector_from(
                "{\"selected_routes\":[],\"deselected_routes\":[\"HostInternal\"],\"deselect_all\":false}",
            ),
        );
        let registry = Arc::new(WgPeerRegistry::default());
        let host = RecordingHost::default();
        apply_selected_map(&shared, registry.as_ref(), &host, route_selection_test_map(7));

        // WG managed routes (N6): the whole HA group is gone, corp stays.
        let wg_routes = registry.managed_routes();
        assert_eq!(wg_routes.len(), 1, "no disabled route may reach the WG seam");
        assert_eq!(wg_routes[0].net_id, "corp");

        // Shell snapshot + host apply: the SAME effective list.
        let shell = shared.network_config_json();
        assert!(
            !shell.contains("192.168.50.0/24"),
            "disabled network leaked into the shell snapshot: {shell}"
        );
        assert!(shell.contains("172.16.0.0/12"), "{shell}");
        assert_eq!(host.routes(), vec!["172.16.0.0/12"]);

        // net_routes_all (the ungated record `refresh_net_gate` re-exports
        // from) is already filtered, so a later gate refresh cannot resurrect
        // the disabled network; the status count is the EFFECTIVE count.
        {
            let g = shared.lock();
            let nets: Vec<String> = g.net_routes_all.iter().map(|r| r.network.clone()).collect();
            assert_eq!(nets, vec!["172.16.0.0/12".to_string()]);
            assert_eq!(g.route_count, 1);
        }

        // Catalog: BOTH HA rows (and their candidate count) stay visible,
        // `enabled:false` for the disabled network only.
        assert_eq!(
            shared.routes_json(),
            "{\"available\":true,\"serial\":7,\"selection\":{\"selected_routes\":[],\"deselected_routes\":[\"HostInternal\"],\"deselect_all\":false},\"networks\":[{\"net_id\":\"HostInternal\",\"networks\":[\"192.168.50.0/24\"],\"candidate_count\":2,\"enabled\":false,\"is_exit\":false},{\"net_id\":\"corp\",\"networks\":[\"172.16.0.0/12\"],\"candidate_count\":1,\"enabled\":true,\"is_exit\":false}]}"
        );
    }

    /// Preferences about NetIDs that do not exist are inert, and a network that
    /// management introduces later is default-ON (the state carries only
    /// explicit deselections).
    #[test]
    fn unknown_persisted_net_id_is_harmless_and_a_new_management_network_defaults_on() {
        let shared = ConnectorShared::new_selected(
            false,
            None,
            selector_from(
                "{\"selected_routes\":[\"ghost-net\"],\"deselected_routes\":[\"gone-net\"],\"deselect_all\":false}",
            ),
        );
        let registry = Arc::new(WgPeerRegistry::default());
        let host = RecordingHost::default();
        apply_selected_map(&shared, registry.as_ref(), &host, route_selection_test_map(7));
        assert_eq!(registry.managed_routes().len(), 3, "unknown NetIDs change nothing");
        assert!(shared.routes_json().contains("\"enabled\":true"), "{}", shared.routes_json());

        // A later map adds a network nobody mentioned: default-on.
        use crate::network_map::ManagedRoute;
        let mut next = route_selection_test_map(8);
        next.routes.push(ManagedRoute {
            id: "new".into(),
            network: config::Route { addr: [10, 20, 0, 0], prefix_len: 16 },
            domains: vec![],
            net_id: "new-net".into(),
            network_type: 1,
            peer: "peer-a".into(),
            metric: 5,
            masquerade: false,
            keep_route: true,
            skip_auto_apply: false,
        });
        apply_selected_map(&shared, registry.as_ref(), &host, next);
        let ids: Vec<String> = registry.managed_routes().iter().map(|r| r.net_id.clone()).collect();
        assert_eq!(ids, vec!["HostInternal", "HostInternal", "corp", "new-net"]);
        assert!(
            shared
                .routes_json()
                .contains("{\"net_id\":\"new-net\",\"networks\":[\"10.20.0.0/16\"],\"candidate_count\":1,\"enabled\":true,\"is_exit\":false}"),
            "{}",
            shared.routes_json()
        );
    }

    /// `deselect_all` empties the effective route list in EVERY consumer but
    /// leaves the peer base allowed-ips (the overlay lanes) untouched.
    #[test]
    fn deselect_all_empties_the_effective_routes_but_keeps_peer_base_allowed_ips() {
        let shared = ConnectorShared::new_selected(false, None, selector_from("{\"deselect_all\":true}"));
        let registry = Arc::new(WgPeerRegistry::default());
        let host = RecordingHost::default();
        apply_selected_map(&shared, registry.as_ref(), &host, route_selection_test_map(7));

        assert!(registry.managed_routes().is_empty(), "deselect_all leaves no managed route");
        assert!(host.routes().is_empty());
        let shell = shared.network_config_json();
        assert!(!shell.contains("192.168.50.0/24") && !shell.contains("172.16.0.0/12"), "{shell}");

        // Peer base allowed-ips are NOT part of the selection: the WG seam and
        // the shell snapshot keep the overlay /32 (+ its accompanying prefix)
        // exactly as management advertised them.
        let peers = registry.snapshot();
        assert_eq!(peers.len(), 3, "peers ride the same snapshot");
        assert_eq!(
            peers[0].allowed_ips,
            vec![
                config::Route { addr: [10, 30, 30, 1], prefix_len: 32 },
                config::Route { addr: [192, 168, 7, 0], prefix_len: 24 },
            ]
        );
        assert_eq!(peers[1].allowed_ips, vec![]);
        assert_eq!(
            peers[2].allowed_ips,
            vec![config::Route { addr: [10, 9, 9, 9], prefix_len: 32 }]
        );
        assert!(
            shell.contains("\"vpn_addresses\":[\"10.30.30.1/32\",\"192.168.7.0/24\"]"),
            "the peer overlay addresses must survive an all-off selection: {shell}"
        );

        // The catalog is still available (it is about the CATALOG, not the
        // selection): every row visible, every row disabled.
        let doc = shared.routes_json();
        assert!(
            doc.starts_with(
                "{\"available\":true,\"serial\":7,\"selection\":{\"selected_routes\":[],\"deselected_routes\":[],\"deselect_all\":true}"
            ),
            "{doc}"
        );
        assert!(doc.contains("\"enabled\":false"), "{doc}");
        assert!(!doc.contains("\"enabled\":true"), "{doc}");
    }

    /// The effective filter is the PURE MODULE's semantics — no invented
    /// `is_selected`: `selected_routes` is not a whitelist, and a conflicting
    /// (hand-written) state resolves deselected-wins.
    #[test]
    fn selected_routes_is_not_a_whitelist_and_conflicts_resolve_deselected() {
        let registry = Arc::new(WgPeerRegistry::default());
        let host = RecordingHost::default();

        let picked = ConnectorShared::new_selected(
            false,
            None,
            selector_from(
                "{\"selected_routes\":[\"corp\"],\"deselected_routes\":[],\"deselect_all\":false}",
            ),
        );
        apply_selected_map(&picked, registry.as_ref(), &host, route_selection_test_map(7));
        assert_eq!(
            registry.managed_routes().len(),
            3,
            "an explicit selection never excludes the networks it does not mention"
        );

        let conflicting = ConnectorShared::new_selected(
            false,
            None,
            selector_from(
                "{\"selected_routes\":[\"HostInternal\"],\"deselected_routes\":[\"HostInternal\"],\"deselect_all\":false}",
            ),
        );
        apply_selected_map(&conflicting, registry.as_ref(), &host, route_selection_test_map(7));
        let ids: Vec<String> = registry.managed_routes().iter().map(|r| r.net_id.clone()).collect();
        assert_eq!(ids, vec!["corp"], "deselected wins");
        assert_eq!(
            conflicting.routes_json(),
            "{\"available\":true,\"serial\":7,\"selection\":{\"selected_routes\":[\"HostInternal\"],\"deselected_routes\":[\"HostInternal\"],\"deselect_all\":false},\"networks\":[{\"net_id\":\"HostInternal\",\"networks\":[\"192.168.50.0/24\"],\"candidate_count\":2,\"enabled\":false,\"is_exit\":false},{\"net_id\":\"corp\",\"networks\":[\"172.16.0.0/12\"],\"candidate_count\":1,\"enabled\":true,\"is_exit\":false}]}"
        );
    }

    // -----------------------------------------------------------------------
    // T0 e15ac: the LIVE route-selection update (toggle without a restart) and
    // the kind-aware recreate ACK
    // -----------------------------------------------------------------------

    /// The networks of the last shell snapshot's exported route list — exactly
    /// what the platform VpnConfig should hold after a live update.
    fn shell_route_networks(shared: &ConnectorShared) -> Vec<String> {
        shared
            .lock()
            .net_config
            .as_ref()
            .expect("an applied map owns a shell snapshot")
            .routes
            .iter()
            .map(|r| r.network.clone())
            .collect()
    }

    /// The networks currently held by the WG seam, in apply order.
    fn wg_route_networks(wg: &WgPeerRegistry) -> Vec<String> {
        wg.managed_routes().iter().map(|r| route_network_string(&r.network)).collect()
    }

    /// The exact refusal document of a malformed selector (the public entry
    /// validates BEFORE it looks up the connector, so this answer is
    /// deterministic even in a test process where another test owns the slot).
    const SELECTION_INVALID: &str = "{\"ok\":false,\"error\":\"selection-invalid\",\
                                     \"available\":false,\"changed\":false,\
                                     \"desired_routes\":[],\"recreate_needed\":false}";

    /// Host tests that drive the map pipeline emit `hilog` lines, and `hilog`
    /// TEES every line into whatever N2 ring is armed PROCESS-WIDE — while the N2
    /// ring's own lib tests assert on the exact seq numbers and rotations of THEIR
    /// appends. So every test in this group holds the same lock those tests hold
    /// for its whole run (the ring state is global, exactly as its doc says).
    /// Without it a route-test record can land inside an armed ring window and
    /// break the ring test's assertions — a pre-existing isolation limit of the
    /// ring tests, unrelated to the route pipeline.
    fn ring_quiet() -> std::sync::MutexGuard<'static, ()> {
        crate::n2_ring::test_guard()
    }

    /// A live toggle needs NO restart: the running connector re-filters the map
    /// it already advertised, and the WG seam and the shell snapshot end up with
    /// the SAME effective list — the property the shell depends on.
    #[test]
    fn runtime_route_selection_setter_filters_wg_and_shell_together_and_restores() {
        let _quiet = ring_quiet();
        let shared = ConnectorShared::new_selected(false, None, RouteSelector::new());
        let registry = Arc::new(WgPeerRegistry::default());
        let host = RecordingHost::default();
        apply_selected_map(&shared, registry.as_ref(), &host, route_selection_test_map(7));
        let all_on = vec![
            "192.168.50.0/24".to_string(),
            "192.168.50.0/24".to_string(),
            "172.16.0.0/12".to_string(),
        ];
        assert_eq!(wg_route_networks(&registry), all_on);

        // OFF (one NetID): every HA candidate of that network goes together.
        let out = shared.route_selection_set_at(
            selector_from("{\"deselected_routes\":[\"HostInternal\"]}"),
            registry.as_ref(),
        );
        assert!(out.available, "a map was applied, so the catalog exists");
        assert!(out.changed);
        assert!(!out.wg_apply_failed);
        assert!(out.recreate_needed, "no applied set was ever ACKed");
        assert_eq!(out.desired_routes, vec!["172.16.0.0/12".to_string()]);
        assert_eq!(
            route_selection_result_json(true, None, &out),
            "{\"ok\":true,\"available\":true,\"changed\":true,\
             \"desired_routes\":[\"172.16.0.0/12\"],\"recreate_needed\":true}",
            "the exact document the shell reads"
        );
        assert_eq!(wg_route_networks(&registry), out.desired_routes, "WG holds the effective set");
        assert_eq!(
            shell_route_networks(&shared),
            out.desired_routes,
            "and the shell snapshot holds the SAME list"
        );
        assert_eq!(shared.lock().route_count, 1);
        assert_eq!(
            host.routes(),
            all_on,
            "the setter rebuilds the shell SNAPSHOT (read via connector_network_config); \
             the host seam is not re-driven — same as the existing gate refresh"
        );

        // The catalog keeps the UNFILTERED advertised rows, now disabled.
        assert_eq!(shared.lock().advertised_routes.as_ref().expect("catalog").routes.len(), 3);
        let catalog = shared.routes_json();
        assert!(catalog.contains("\"serial\":7"), "{catalog}");
        assert!(
            catalog.contains(
                "{\"net_id\":\"HostInternal\",\"networks\":[\"192.168.50.0/24\"],\
                 \"candidate_count\":2,\"enabled\":false,\"is_exit\":false}"
            ),
            "{catalog}"
        );
        assert!(
            catalog.contains(
                "{\"net_id\":\"corp\",\"networks\":[\"172.16.0.0/12\"],\
                 \"candidate_count\":1,\"enabled\":true,\"is_exit\":false}"
            ),
            "{catalog}"
        );

        // An identical re-send is applied idempotently and reports changed:false.
        let again = shared.route_selection_set_at(
            selector_from("{\"deselected_routes\":[\"HostInternal\"]}"),
            registry.as_ref(),
        );
        assert!(!again.changed);
        assert_eq!(again.desired_routes, vec!["172.16.0.0/12".to_string()]);
        assert_eq!(wg_route_networks(&registry), vec!["172.16.0.0/12".to_string()]);

        // ON again: everything is back, in map order, in both seams.
        let out = shared.route_selection_set_at(RouteSelector::new(), registry.as_ref());
        assert!(out.changed);
        assert_eq!(out.desired_routes, all_on);
        assert_eq!(wg_route_networks(&registry), all_on);
        assert_eq!(shell_route_networks(&shared), all_on);
        assert_eq!(shared.lock().route_count, 3);
    }

    /// The selection governs managed routes ONLY: peer base `allowed_ips` and a
    /// landed (ICE-selected) session survive a live toggle untouched.
    #[test]
    fn runtime_route_selection_setter_leaves_peer_base_ips_and_landed_sessions_untouched() {
        let _quiet = ring_quiet();
        let shared = ConnectorShared::new_selected(false, None, RouteSelector::new());
        let registry = Arc::new(WgPeerRegistry::default());
        let host = RecordingHost::default();
        apply_selected_map(&shared, registry.as_ref(), &host, route_selection_test_map(7));
        // the session material a toggle must not disturb: the ICE-selected pair
        // of a registered peer
        registry
            .apply_endpoint("UEVFUjA=", [10, 30, 30, 9], 51820)
            .expect("the peer is registered");
        let peers_before = registry.snapshot();
        let endpoints_before = registry.endpoints();

        let out = shared.route_selection_set_at(
            selector_from("{\"deselect_all\":true}"),
            registry.as_ref(),
        );
        assert!(out.available && out.changed);
        assert!(out.desired_routes.is_empty(), "all off: nothing is exported");
        assert!(out.recreate_needed, "an empty desired set differs from the unknown applied one");
        assert!(registry.managed_routes().is_empty());
        assert!(shell_route_networks(&shared).is_empty());
        assert_eq!(registry.snapshot(), peers_before, "peer base allowed_ips are NOT the selection");
        assert_eq!(registry.endpoints(), endpoints_before, "a landed session survives a toggle");
        assert_eq!(shared.lock().peer_count, 3);
        assert!(
            shared
                .network_config_json()
                .contains("\"vpn_addresses\":[\"10.30.30.1/32\",\"192.168.7.0/24\"]"),
            "{}",
            shared.network_config_json()
        );
    }

    /// Before the first map there is nothing to filter: the selection is STORED
    /// for the next map, and the outcome claims neither availability nor an
    /// applied route set.
    #[test]
    fn runtime_route_selection_setter_without_a_map_stores_the_selection_and_claims_nothing() {
        let _quiet = ring_quiet();
        let shared = ConnectorShared::new_selected(false, None, RouteSelector::new());
        let registry = Arc::new(WgPeerRegistry::default());
        let out = shared.route_selection_set_at(
            selector_from("{\"deselected_routes\":[\"HostInternal\"]}"),
            registry.as_ref(),
        );
        assert!(out.changed);
        assert!(!out.available, "no map was applied: nothing to filter, nothing to claim");
        assert!(out.desired_routes.is_empty());
        assert!(!out.recreate_needed, "no map: no applied claim, nothing to reconcile");
        assert_eq!(
            route_selection_result_json(true, None, &out),
            "{\"ok\":true,\"available\":false,\"changed\":true,\
             \"desired_routes\":[],\"recreate_needed\":false}",
            "the exact document for a stored-but-not-applied selection"
        );
        assert!(registry.managed_routes().is_empty(), "no map -> the WG seam is never called");
        {
            let g = shared.lock();
            assert!(g.applied_route_set.is_none());
            assert!(g.net_config.is_none());
            assert_eq!(g.route_count, 0);
            assert_eq!(
                g.route_selection.to_json(),
                "{\"selected_routes\":[],\"deselected_routes\":[\"HostInternal\"],\"deselect_all\":false}"
            );
        }
        assert_eq!(
            shared.routes_json(),
            "{\"available\":false,\"serial\":0,\"selection\":\
             {\"selected_routes\":[],\"deselected_routes\":[\"HostInternal\"],\"deselect_all\":false},\
             \"networks\":[]}"
        );

        // The stored selection is the one the NEXT map is filtered with.
        let host = RecordingHost::default();
        apply_selected_map(&shared, registry.as_ref(), &host, route_selection_test_map(7));
        assert_eq!(wg_route_networks(&registry), vec!["172.16.0.0/12".to_string()]);
        assert_eq!(shell_route_networks(&shared), vec!["172.16.0.0/12".to_string()]);
    }

    /// A WG apply refusal is reported honestly (`ok:false`,
    /// `wg-apply-failed`) with the REAL post-update facts: the selection and the
    /// shell snapshot DID move, the data plane did not, so the shell still sees
    /// the desired set it has to install — fail closed, nothing is claimed as
    /// installed.
    #[test]
    fn runtime_route_selection_setter_reports_a_wg_apply_failure_and_fails_closed() {
        let _quiet = ring_quiet();
        struct FailingWg(WgPeerRegistry);
        impl WgPeerApplier for FailingWg {
            fn apply_peers(&self, peers: &[WgPeerEntry]) -> Result<(), String> {
                self.0.apply_peers(peers)
            }
            fn clear(&self) {
                self.0.clear();
            }
            fn apply_managed_routes(
                &self,
                _routes: &[crate::network_map::ManagedRoute],
            ) -> Result<(), String> {
                Err("wg-managed-routes-unsupported".to_string())
            }
        }
        let shared = ConnectorShared::new_selected(false, None, RouteSelector::new());
        let wg = Arc::new(FailingWg(WgPeerRegistry::default()));
        let host = RecordingHost::default();
        apply_selected_map(&shared, wg.as_ref(), &host, route_selection_test_map(7));
        assert!(shared.lock().wg_apply_failed, "the map apply already latched the refusal");

        let out = shared.route_selection_set_at(
            selector_from("{\"deselected_routes\":[\"HostInternal\"]}"),
            wg.as_ref(),
        );
        assert!(out.available && out.changed && out.wg_apply_failed);
        assert_eq!(
            route_selection_result_json(false, Some("wg-apply-failed"), &out),
            "{\"ok\":false,\"error\":\"wg-apply-failed\",\"available\":true,\"changed\":true,\
             \"desired_routes\":[\"172.16.0.0/12\"],\"recreate_needed\":true}"
        );
        assert_eq!(shell_route_networks(&shared), vec!["172.16.0.0/12".to_string()]);
        assert!(wg.0.managed_routes().is_empty(), "the seam refused: it holds no route");
        let g = shared.lock();
        assert!(g.wg_apply_failed, "the flag stays latched until a seam accepts");
        assert!(g.wg_apply_errors >= 2, "the refused map apply plus this one");
    }

    /// A malformed selector document mutates NOTHING: the public entry answers
    /// the no-claim refusal document, and the SAME strict parser the start config
    /// and the persisted state use is what refuses it (the mutation layer is only
    /// reachable through a parsed selector).
    #[test]
    fn runtime_route_selection_setter_refuses_malformed_documents_without_mutating() {
        let _quiet = ring_quiet();
        let shared = ConnectorShared::new_selected(false, None, RouteSelector::new());
        let registry = Arc::new(WgPeerRegistry::default());
        let host = RecordingHost::default();
        apply_selected_map(&shared, registry.as_ref(), &host, route_selection_test_map(7));
        let catalog_before = shared.routes_json();
        let wg_before = wg_route_networks(&registry);

        for bad in [
            "{",
            "[]",
            "\"all\"",
            "{\"deselected_routes\":\"HostInternal\"}",
            "{\"deselected_routes\":[1]}",
            "{\"deselect_all\":1}",
            "{\"selected_routes\":{}}",
            "{\"deselect_all\":true,\"deselect_all\":false}",
        ] {
            assert_eq!(connector_route_selection_set_json(bad), SELECTION_INVALID, "{bad}");
            assert!(RouteSelector::from_json(bad).is_err(), "{bad} must not parse");
        }

        // Nothing moved, and a valid document still applies cleanly afterwards
        // (no half-updated state left behind by the refusals).
        assert_eq!(shared.routes_json(), catalog_before);
        assert_eq!(wg_route_networks(&registry), wg_before);
        assert_eq!(shared.lock().route_count, 3);
        let out = shared.route_selection_set_at(
            selector_from("{\"deselected_routes\":[\"corp\"]}"),
            registry.as_ref(),
        );
        assert!(out.available && out.changed);
        assert_eq!(
            wg_route_networks(&registry),
            vec!["192.168.50.0/24".to_string(), "192.168.50.0/24".to_string()]
        );
        assert_eq!(shell_route_networks(&shared), out.desired_routes);
    }

    /// The stored selection is a state, not a one-shot patch: the CURRENT map was
    /// filtered with it (live), and every FUTURE map — a new HA candidate of a
    /// disabled NetID, or a network nobody mentioned yet — goes through the very
    /// same helper.
    #[test]
    fn runtime_route_selection_filters_the_current_and_every_future_map_identically() {
        let _quiet = ring_quiet();
        use crate::network_map::ManagedRoute;
        let shared = ConnectorShared::new_selected(false, None, RouteSelector::new());
        let registry = Arc::new(WgPeerRegistry::default());
        let host = RecordingHost::default();
        apply_selected_map(&shared, registry.as_ref(), &host, route_selection_test_map(7));
        shared.route_selection_set_at(
            selector_from("{\"deselected_routes\":[\"HostInternal\"]}"),
            registry.as_ref(),
        );
        assert_eq!(wg_route_networks(&registry), vec!["172.16.0.0/12".to_string()]);

        // The NEXT map: one more HA candidate for the disabled NetID plus a
        // brand-new network (default-ON, nobody mentioned it).
        let mut next = route_selection_test_map(8);
        next.routes.insert(
            1,
            ManagedRoute {
                id: "ha-050".into(),
                network: config::Route { addr: [192, 168, 50, 0], prefix_len: 24 },
                domains: vec![],
                net_id: "HostInternal".into(),
                network_type: 1,
                peer: "peer-c".into(),
                metric: 50,
                masquerade: true,
                keep_route: true,
                skip_auto_apply: false,
            },
        );
        next.routes.push(ManagedRoute {
            id: "new".into(),
            network: config::Route { addr: [10, 20, 0, 0], prefix_len: 16 },
            domains: vec![],
            net_id: "new-net".into(),
            network_type: 1,
            peer: "peer-a".into(),
            metric: 5,
            masquerade: false,
            keep_route: true,
            skip_auto_apply: false,
        });
        apply_selected_map(&shared, registry.as_ref(), &host, next);

        let expected = vec!["172.16.0.0/12".to_string(), "10.20.0.0/16".to_string()];
        assert_eq!(wg_route_networks(&registry), expected);
        assert_eq!(shell_route_networks(&shared), expected);
        assert_eq!(shared.lock().route_count, 2);
        let catalog = shared.routes_json();
        assert!(catalog.contains("\"serial\":8"), "{catalog}");
        assert!(
            catalog.contains(
                "{\"net_id\":\"HostInternal\",\"networks\":[\"192.168.50.0/24\"],\
                 \"candidate_count\":3,\"enabled\":false,\"is_exit\":false}"
            ),
            "{catalog}"
        );
        assert!(
            catalog.contains(
                "{\"net_id\":\"new-net\",\"networks\":[\"10.20.0.0/16\"],\
                 \"candidate_count\":1,\"enabled\":true,\"is_exit\":false}"
            ),
            "{catalog}"
        );
    }

    /// WG seam wrapper that records every `apply_managed_routes` payload in apply
    /// order (the observation the serialization test needs).
    struct LoggingWg {
        inner: WgPeerRegistry,
        log: Mutex<Vec<Vec<String>>>,
    }

    impl WgPeerApplier for LoggingWg {
        fn apply_peers(&self, peers: &[WgPeerEntry]) -> Result<(), String> {
            self.inner.apply_peers(peers)
        }
        fn clear(&self) {
            self.inner.clear();
        }
        fn apply_managed_routes(
            &self,
            routes: &[crate::network_map::ManagedRoute],
        ) -> Result<(), String> {
            self.log
                .lock_poison()
                .push(routes.iter().map(|r| route_network_string(&r.network)).collect());
            self.inner.apply_managed_routes(routes)
        }
    }

    /// Host seam that PARKs the map pipeline inside `host.apply` (the LAST call
    /// of the route section) until the test releases it — a deterministic
    /// barrier, no sleeping and no timing assumption.
    struct ParkingHost {
        entered: std::sync::mpsc::Sender<()>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl ConfigApplier for ParkingHost {
        fn apply(&self, _map: &NetworkMap) {
            let _ = self.entered.send(());
            let _ = self.release.lock_poison().recv();
        }
        fn clear(&self) {}
    }

    /// The map pipeline and the live setter share ONE critical section: while a
    /// map apply sits in `host.apply`, `route_apply` is HELD (so the setter must
    /// wait) and `inner` is NOT (so WG/device calls never happen under it). The
    /// pinned interleaving has the setter's WG application as the LAST one, which
    /// is what the mutex guarantees: the seam ends up holding the toggled set.
    #[test]
    fn runtime_route_selection_setter_and_map_apply_serialize_on_the_route_gate() {
        let _quiet = ring_quiet();
        let shared = Arc::new(ConnectorShared::new_selected(false, None, RouteSelector::new()));
        let wg = Arc::new(LoggingWg { inner: WgPeerRegistry::default(), log: Mutex::new(Vec::new()) });
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let host = Arc::new(ParkingHost { entered: entered_tx, release: Mutex::new(release_rx) });

        let apply_shared = shared.clone();
        let apply_wg = wg.clone();
        let apply_host = host.clone();
        let map = route_selection_test_map(7);
        let map_apply = std::thread::spawn(move || {
            apply_shared.apply_update(
                apply_wg.as_ref(),
                apply_host.as_ref(),
                &SyncUpdate {
                    session_deadline_unix: None,
                    netbird_config: None,
                    network_map: Some(map),
                },
            );
        });
        // deterministic barrier: the map pipeline is now inside `host.apply`
        entered_rx.recv().expect("the map pipeline reached host.apply");

        assert!(
            shared.route_apply.try_lock().is_err(),
            "the map route section must hold route_apply through host.apply"
        );
        assert!(
            shared.inner.try_lock().is_ok(),
            "lock order route_apply -> inner: `inner` must be FREE while calling out"
        );

        // A live setter issued now must WAIT for that whole section.
        let set_shared = shared.clone();
        let set_wg = wg.clone();
        let setter = std::thread::spawn(move || {
            set_shared.route_selection_set_at(
                selector_from("{\"deselected_routes\":[\"HostInternal\"]}"),
                set_wg.as_ref(),
            )
        });
        release_tx.send(()).expect("release host.apply");
        map_apply.join().expect("the map apply thread");
        let outcome = setter.join().expect("the setter thread");

        assert!(outcome.available && outcome.changed);
        assert_eq!(outcome.desired_routes, vec!["172.16.0.0/12".to_string()]);
        assert_eq!(
            *wg.log.lock_poison(),
            vec![
                vec![
                    "192.168.50.0/24".to_string(),
                    "192.168.50.0/24".to_string(),
                    "172.16.0.0/12".to_string()
                ],
                vec!["172.16.0.0/12".to_string()],
            ],
            "the setter's application must be LAST: the seam holds the toggled set"
        );
        assert_eq!(wg_route_networks(&wg.inner), vec!["172.16.0.0/12".to_string()]);
        assert_eq!(shell_route_networks(&shared), vec!["172.16.0.0/12".to_string()]);
    }

    /// A USER-forced recreate ACK records the captured platform set but spends NO
    /// automatic budget and arms NO cooldown — after four of them the automatic
    /// path can still spend its FULL [`RECREATE_MAX`] budget at the unchanged
    /// [`RECREATE_COOLDOWN_MS`] spacing.
    #[test]
    fn user_kind_ack_records_the_platform_snapshot_without_spending_the_automatic_budget() {
        let _quiet = ring_quiet();
        let shared = ConnectorShared::new_selected(false, None, RouteSelector::new());
        let registry = Arc::new(WgPeerRegistry::default());
        let host = RecordingHost::default();
        apply_selected_map(&shared, registry.as_ref(), &host, route_selection_test_map(7));
        let t = crate::sys::mono_ms();
        let all_on = wg_route_networks(&registry);
        shared.ack_applied_route_set(all_on.clone(), false, t);
        assert_eq!(shared.lock().recreate.count, 0, "the initial create spends nothing");

        // FOUR user-forced cycles: toggle -> the automatic gate sees the
        // divergence IMMEDIATELY (nothing armed a cooldown) -> the shell recreates
        // and ACKs with kind `user`.
        for cycle in 0..4u64 {
            let selection = if cycle % 2 == 0 {
                selector_from("{\"deselected_routes\":[\"HostInternal\"]}")
            } else {
                RouteSelector::new()
            };
            let out = shared.route_selection_set_at(selection, registry.as_ref());
            assert!(out.available && out.changed);
            assert!(out.recreate_needed, "cycle {cycle}: the desired set diverged");
            let now = t + cycle;
            shared.refresh_net_gate_at(registry.as_ref(), now);
            assert!(
                shared.lock().recreate.required,
                "cycle {cycle}: the gate must ask for the recreate"
            );
            let applied = shell_route_networks(&shared);
            assert_eq!(applied, out.desired_routes);
            shared.ack_applied_route_set_kind(applied, RouteAckKind::User, now);
            let rec = shared.lock().recreate.clone();
            assert!(!rec.required, "cycle {cycle}: the ACK cleared the request");
            assert_eq!(rec.count, 0, "cycle {cycle}: a user ACK must not spend the auto budget");
            assert!(!rec.exhausted, "cycle {cycle}");
            assert_eq!(
                shared.lock().last_recreate_ms,
                None,
                "cycle {cycle}: a user ACK must not arm the cooldown"
            );
        }

        // The automatic path is untouched and still exhaustive: three auto
        // recreates (legacy `isRecreate=true`), one cooldown window apart.
        for cycle in 0..RECREATE_MAX {
            let selection = if cycle % 2 == 0 {
                selector_from("{\"deselected_routes\":[\"HostInternal\"]}")
            } else {
                RouteSelector::new()
            };
            shared.route_selection_set_at(selection, registry.as_ref());
            let now = t + 1_000 + (cycle + 1) * (RECREATE_COOLDOWN_MS + 1);
            shared.refresh_net_gate_at(registry.as_ref(), now);
            assert!(
                shared.lock().recreate.required,
                "auto recreate {cycle} must still be raised"
            );
            shared.ack_applied_route_set(shell_route_networks(&shared), true, now);
            assert_eq!(shared.lock().recreate.count, cycle + 1);
        }
        let rec = shared.lock().recreate.clone();
        assert_eq!(rec.count, RECREATE_MAX, "no automatic unit was consumed by the user ACKs");
        assert!(rec.exhausted, "the automatic budget still latches at RECREATE_MAX");
    }

    /// An EXPLICITLY empty applied set is legal (T0 e15ac) and it is what makes a
    /// later, default-ENABLED network raise the recreate signal — without it the
    /// comparison stays silent because `applied` is unknown.
    #[test]
    fn an_explicitly_empty_applied_set_is_accepted_and_arms_the_next_recreate() {
        let _quiet = ring_quiet();
        // The frozen 2-arg Rust signatures stay available verbatim (fn-pointer
        // proof), plus the new kind-aware entry and the new live setter.
        let _legacy: fn(&str, bool) -> String = connector_route_set_applied_json;
        let _kind: fn(&str, &str) -> String = connector_route_set_applied_kind_json;
        let _setter: fn(&str) -> String = connector_route_selection_set_json;
        assert_eq!(connector_route_set_applied_kind_json("{}", "nope"),
                   "{\"ok\":false,\"error\":\"kind-invalid\"}");
        assert_eq!(connector_route_set_applied_kind_json("{", "user"),
                   "{\"ok\":false,\"error\":\"route-set-invalid\"}");
        assert_eq!(RouteAckKind::parse("initial"), Some(RouteAckKind::Initial));
        assert_eq!(RouteAckKind::parse("auto"), Some(RouteAckKind::Auto));
        assert_eq!(RouteAckKind::parse("user"), Some(RouteAckKind::User));
        assert_eq!(RouteAckKind::parse(""), None);

        // The parser: explicit empty OK, duplicates verbatim, malformed refused.
        assert_eq!(
            parse_route_set("{\"routes\":[]}").expect("an explicit empty applied set"),
            Vec::<String>::new()
        );
        assert_eq!(
            parse_route_set("{\"routes\":[\"a.b.c.d/32\",\"a.b.c.d/32\"]}").expect("verbatim"),
            vec!["a.b.c.d/32".to_string(), "a.b.c.d/32".to_string()]
        );
        assert!(parse_route_set("{}").is_err(), "missing field still refused");
        assert!(parse_route_set("{\"routes\":[1]}").is_err(), "non-string still refused");
        assert!(parse_route_set("{\"routes\":{}}").is_err(), "non-array still refused");
        assert!(parse_route_set("[]").is_err(), "non-object still refused");

        // Behaviour: "all off" through the LIVE setter, then the empty ACK.
        let shared = ConnectorShared::new_selected(false, None, RouteSelector::new());
        let registry = Arc::new(WgPeerRegistry::default());
        let host = RecordingHost::default();
        apply_selected_map(&shared, registry.as_ref(), &host, route_selection_test_map(7));
        let t = crate::sys::mono_ms();
        let all_on = wg_route_networks(&registry);
        let out = shared.route_selection_set_at(
            selector_from("{\"deselect_all\":true}"),
            registry.as_ref(),
        );
        assert!(out.desired_routes.is_empty());
        assert!(wg_route_networks(&registry).is_empty());
        assert!(shell_route_networks(&shared).is_empty());
        // Without any ACK the comparison is silent (the initial create is pending).
        shared.refresh_net_gate_at(registry.as_ref(), t);
        assert!(!shared.lock().recreate.required);

        // The shell's initial create applied NOTHING and says so explicitly.
        shared.ack_applied_route_set(Vec::new(), false, t + 1);
        assert_eq!(shared.lock().applied_route_set, Some(Vec::<String>::new()));
        shared.refresh_net_gate_at(registry.as_ref(), t + 2);
        assert!(!shared.lock().recreate.required, "empty desired == empty applied");

        // Everything back ON: NOW the recorded empty set makes the divergence
        // visible, so the newly enabled routes can reach the platform.
        let out = shared.route_selection_set_at(RouteSelector::new(), registry.as_ref());
        assert!(out.changed && out.recreate_needed);
        assert_eq!(out.desired_routes, all_on);
        shared.refresh_net_gate_at(registry.as_ref(), t + 3);
        let rec = shared.lock().recreate.clone();
        assert!(rec.required, "{rec:?}");
        assert_eq!(rec.reason, "route-set-changed");
        assert_eq!(rec.count, 0, "still no automatic recreate was completed");

        // The user-forced recreate is ACKed with kind `user`: recorded, nothing spent.
        shared.ack_applied_route_set_kind(all_on.clone(), RouteAckKind::User, t + 4);
        let rec = shared.lock().recreate.clone();
        assert!(!rec.required);
        assert_eq!(rec.count, 0);
        assert!(!rec.exhausted);
        assert_eq!(shared.lock().last_recreate_ms, None);
        shared.refresh_net_gate_at(registry.as_ref(), t + 5);
        assert!(!shared.lock().recreate.required, "converged");
        assert_eq!(wg_route_networks(&registry), all_on);
    }

    /// The gated snapshot is rebuilt too: with the dev force flag the exit route
    /// is exported, and turning its NetID off removes it from BOTH the WG table
    /// and the shell snapshot (the gate verdict itself is reused, not re-decided).
    #[test]
    fn runtime_route_selection_setter_rebuilds_the_default_route_gated_snapshot() {
        let _quiet = ring_quiet();
        use crate::network_map::ManagedRoute;
        let shared = ConnectorShared::new_selected(true, None, RouteSelector::new());
        let registry = Arc::new(WgPeerRegistry::default());
        let host = RecordingHost::default();
        let mut map = route_selection_test_map(7);
        map.routes.push(ManagedRoute {
            id: "exit".into(),
            network: config::Route { addr: [0, 0, 0, 0], prefix_len: 0 },
            domains: vec![],
            net_id: "net-d".into(),
            network_type: 1,
            peer: "relay".into(),
            metric: 9999,
            masquerade: false,
            keep_route: false,
            skip_auto_apply: false,
        });
        apply_selected_map(&shared, registry.as_ref(), &host, map);
        let with_exit = vec![
            "192.168.50.0/24".to_string(),
            "192.168.50.0/24".to_string(),
            "172.16.0.0/12".to_string(),
            "0.0.0.0/0".to_string(),
        ];
        assert_eq!(wg_route_networks(&registry), with_exit);
        assert_eq!(shell_route_networks(&shared), with_exit, "the forced gate exports the exit route");
        assert_eq!(shared.lock().route_count, 4);

        let out = shared.route_selection_set_at(
            selector_from("{\"deselected_routes\":[\"net-d\"]}"),
            registry.as_ref(),
        );
        let without_exit = vec![
            "192.168.50.0/24".to_string(),
            "192.168.50.0/24".to_string(),
            "172.16.0.0/12".to_string(),
        ];
        assert_eq!(out.desired_routes, without_exit);
        assert_eq!(wg_route_networks(&registry), without_exit);
        assert_eq!(shell_route_networks(&shared), without_exit);
        assert_eq!(shared.lock().route_count, 3);
        assert!(out.recreate_needed);
    }

    /// An HA OWNER/metric change on the same prefixes needs NO OS route replace
    /// (the platform installs one route per prefix), while a stale ACK is still
    /// recorded verbatim — the mismatch simply stays visible.
    #[test]
    fn ha_owner_change_needs_no_os_replace_and_a_stale_ack_is_recorded_verbatim() {
        let _quiet = ring_quiet();
        let shared = ConnectorShared::new_selected(false, None, RouteSelector::new());
        let registry = Arc::new(WgPeerRegistry::default());
        let host = RecordingHost::default();
        apply_selected_map(&shared, registry.as_ref(), &host, route_selection_test_map(7));
        let t = crate::sys::mono_ms();
        let all_on = wg_route_networks(&registry);
        shared.ack_applied_route_set(all_on.clone(), false, t);

        // The HA owner moves: the two candidates swap peer/metric on the SAME
        // prefixes, and the WG table is re-applied with the new rows.
        let mut moved = route_selection_test_map(8);
        moved.routes[0].peer = "peer-b".into();
        moved.routes[0].metric = 100;
        moved.routes[1].peer = "peer-a".into();
        moved.routes[1].metric = 200;
        apply_selected_map(&shared, registry.as_ref(), &host, moved);
        assert_eq!(wg_route_networks(&registry), all_on);

        let out = shared.route_selection_set_at(RouteSelector::new(), registry.as_ref());
        assert!(!out.changed);
        assert_eq!(out.desired_routes, all_on);
        assert!(!out.recreate_needed, "same prefixes -> no OS route replace is needed");
        shared.refresh_net_gate_at(registry.as_ref(), t + 1);
        assert!(!shared.lock().recreate.required, "the legacy comparison agrees here");

        // The OS set is a SET: a captured set with one HA row fewer still needs no
        // replace, because the prefix set is identical.
        shared.ack_applied_route_set(
            vec!["172.16.0.0/12".to_string(), "192.168.50.0/24".to_string()],
            false,
            t + 2,
        );
        let out = shared.route_selection_set_at(RouteSelector::new(), registry.as_ref());
        assert!(!out.recreate_needed, "an owner change keeps the same prefix set");

        // A STALE ACK is never rejected: it is evidence about the platform, and
        // the mismatch keeps `desired != applied`.
        shared.ack_applied_route_set(vec!["10.20.0.0/16".to_string()], false, t + 3);
        assert_eq!(
            shared.lock().applied_route_set,
            Some(vec!["10.20.0.0/16".to_string()])
        );
        shared.refresh_net_gate_at(registry.as_ref(), t + 4);
        let rec = shared.lock().recreate.clone();
        assert!(rec.required, "a stale ACK re-raises the divergence: {rec:?}");
        assert_eq!(rec.reason, "route-set-changed");
        assert_eq!(rec.count, 0);
    }

    /// T0 native5f: the automatic divergence signal (`recreate.required`) uses the
    /// SAME prefix-set rule as the live setter. An ACKed platform set that differs
    /// from the desired rows only by DUPLICATES or ORDER is not a desired-set
    /// change: the setter reports `recreate_needed=false`, the gate refresh keeps
    /// `required=false` and spends NOTHING (count / cooldown / exhausted
    /// untouched). A REAL prefix difference still raises the signal through the
    /// unchanged budget / cooldown path.
    #[test]
    fn prefix_set_equality_governs_the_auto_recreate_signal_too() {
        let _quiet = ring_quiet();
        let shared = ConnectorShared::new_selected(false, None, RouteSelector::new());
        let registry = Arc::new(WgPeerRegistry::default());
        let host = RecordingHost::default();
        apply_selected_map(&shared, registry.as_ref(), &host, route_selection_test_map(7));
        let t = crate::sys::mono_ms();
        let all_on = wg_route_networks(&registry);
        assert_eq!(
            all_on,
            vec![
                "192.168.50.0/24".to_string(),
                "192.168.50.0/24".to_string(),
                "172.16.0.0/12".to_string()
            ],
            "the HA shape: two rows for one prefix plus the corp subnet"
        );

        // The ACKed platform set holds the SAME prefix set with a different ROW
        // shape (reversed order, and the HA prefix duplicated once more). The OS
        // installs one route per prefix, so neither surface may see a divergence.
        shared.ack_applied_route_set(
            vec![
                "172.16.0.0/12".to_string(),
                "192.168.50.0/24".to_string(),
                "192.168.50.0/24".to_string(),
            ],
            false,
            t,
        );
        let out = shared.route_selection_set_at(RouteSelector::new(), registry.as_ref());
        assert!(!out.changed);
        assert_eq!(out.desired_routes, all_on);
        assert!(!out.recreate_needed, "same prefix set -> no OS route replace");
        shared.refresh_net_gate_at(registry.as_ref(), t + 1);
        let rec = shared.lock().recreate.clone();
        assert!(!rec.required, "the auto signal must agree with the setter: {rec:?}");
        assert_eq!(rec.reason, "none");
        assert!(!rec.cooling_down);
        assert_eq!(rec.count, 0, "a row-shape-only change must consume no auto unit");
        assert!(!rec.exhausted);
        assert_eq!(shared.lock().last_recreate_ms, None, "and must not arm the cooldown");

        // A REAL prefix difference (the HA subnet is gone) still raises through the
        // unchanged path: required / `route-set-changed`, nothing counted until the
        // shell ACKs a completed AUTO recreate.
        shared.ack_applied_route_set(vec!["172.16.0.0/12".to_string()], false, t + 2);
        let out = shared.route_selection_set_at(RouteSelector::new(), registry.as_ref());
        assert!(out.recreate_needed, "a missing prefix is a real divergence");
        shared.refresh_net_gate_at(registry.as_ref(), t + 3);
        let rec = shared.lock().recreate.clone();
        assert!(rec.required, "{rec:?}");
        assert_eq!(rec.reason, "route-set-changed");
        assert_eq!(rec.count, 0, "raised, not yet completed");
        assert_eq!(shared.lock().last_recreate_ms, None);
        shared.ack_applied_route_set_kind(all_on.clone(), RouteAckKind::Auto, t + 4);
        let rec = shared.lock().recreate.clone();
        assert!(!rec.required);
        assert_eq!(rec.count, 1, "the unchanged auto budget counts the completion");
        assert_eq!(shared.lock().last_recreate_ms, Some(t + 4), "and arms the cooldown");
    }

    /// The start field is validated with the strict parser: a malformed
    /// `route_selection` fails the whole config (stable `invalid-config` at the
    /// start seam) instead of silently defaulting to all-selected.
    #[test]
    fn route_selection_start_field_parses_strictly_and_malformed_state_fails_the_config() {
        let valid = format!(
            "{{\"management_url\":\"http://127.0.0.1:8012\",\"private_key\":\"{}\",\"route_selection\":{{\"selected_routes\":[\"a\"],\"deselected_routes\":[\"b\"],\"deselect_all\":false}}}}",
            test_private_key_b64(1)
        );
        let cfg = ConnectorConfig::from_json(&valid).expect("valid route_selection");
        assert_eq!(
            cfg.route_selection.to_json(),
            "{\"selected_routes\":[\"a\"],\"deselected_routes\":[\"b\"],\"deselect_all\":false}"
        );

        let cases = [
            "\"route_selection\":[]",
            "\"route_selection\":\"all\"",
            "\"route_selection\":{\"selected_routes\":{}}",
            "\"route_selection\":{\"deselected_routes\":[1]}",
            "\"route_selection\":{\"deselect_all\":1}",
            "\"route_selection\":{\"deselect_all\":true,\"deselect_all\":false}",
            "\"route_selection\":{\"deselect_all\":true},\"route_selection\":{}",
        ];
        for case in cases {
            let json = format!(
                "{{\"management_url\":\"http://127.0.0.1:8012\",\"private_key\":\"{}\",{case}}}",
                test_private_key_b64(1)
            );
            let err = ConnectorConfig::from_json(&json).unwrap_err();
            assert!(
                matches!(&err, ConfigError::Field { field, .. } if *field == "route_selection"),
                "{case} must fail as field 'route_selection', got {err}"
            );
        }

        // The start seam renders that config error as its frozen token.
        let bad = format!(
            "{{\"management_url\":\"http://127.0.0.1:8012\",\"private_key\":\"{}\",\"route_selection\":[]}}",
            test_private_key_b64(1)
        );
        assert_eq!(
            connector_start_json(&bad, "{\"setup_key\":\"k\"}"),
            "{\"started\":false,\"error\":\"invalid-config\"}"
        );
    }

    /// `connector_routes_json` is a side-effect-free snapshot: no catalog
    /// before the first map, the applied map's OWN serial + an empty `networks`
    /// list for a map that advertised no routes (connected, no routes — not
    /// "no snapshot"), no gate refresh, no recreate bookkeeping moved by a
    /// read.
    #[test]
    fn routes_json_is_a_side_effect_free_snapshot_and_keeps_the_empty_catalog_of_an_applied_map() {
        let shared = ConnectorShared::new_selected(
            false,
            None,
            selector_from("{\"deselected_routes\":[\"HostInternal\"]}"),
        );
        // Only "no NetworkMap applied yet" is unavailable.
        assert_eq!(
            shared.routes_json(),
            "{\"available\":false,\"serial\":0,\"selection\":{\"selected_routes\":[],\"deselected_routes\":[\"HostInternal\"],\"deselect_all\":false},\"networks\":[]}"
        );

        // An applied map that advertised no routes is AVAILABLE with the map's
        // own serial, the generation's selection and zero networks — the
        // honest "connected, no routes" catalog (distinct from "no snapshot").
        let registry = Arc::new(WgPeerRegistry::default());
        let host = RecordingHost::default();
        apply_selected_map(
            &shared,
            registry.as_ref(),
            &host,
            NetworkMap { routes: vec![], ..route_selection_test_map(9) },
        );
        assert_eq!(
            shared.routes_json(),
            "{\"available\":true,\"serial\":9,\"selection\":{\"selected_routes\":[],\"deselected_routes\":[\"HostInternal\"],\"deselect_all\":false},\"networks\":[]}"
        );

        // Reading the document never advances the N8 recreate state (unlike
        // `network_config_json`, which re-decides the gate).
        shared.lock().applied_route_set = Some(vec!["172.16.0.0/12".to_string()]);
        let _ = shared.routes_json();
        {
            let g = shared.lock();
            assert_eq!(g.applied_route_set.as_deref(), Some(&["172.16.0.0/12".to_string()][..]));
            assert_eq!(g.recreate, RecreateStatus::default(), "no recreate state moved");
            assert!(
                g.net_config.as_ref().is_some_and(|c| c.routes.is_empty()),
                "the read synthesized nothing — the snapshot is the empty applied map"
            );
        }

        // With no connector installed the free helper answers the same shape.
        assert_eq!(
            connector_routes_json(),
            "{\"available\":false,\"serial\":0,\"selection\":{\"selected_routes\":[],\"deselected_routes\":[],\"deselect_all\":false},\"networks\":[]}"
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
                    fqdn: Some("peer-a.netbird.cloud".into()),
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

    // 2026-10-12 upstream MTU alignment: the announced `PeerConfig.mtu`
    // (`client/internal/connect.go:740` selectMTU's second input) reaches the
    // shell ONLY inside this platform's `VpnConfig.mtu` envelope — an
    // out-of-envelope announcement is not the shell-applicable subset and must
    // not travel (the platform rejects the whole VpnConfig outside [576,1500],
    // device-verified create-401 on 2026-09-15).
    #[test]
    fn announced_mtu_is_exported_only_inside_the_platform_envelope() {
        let with_mtu = |mtu: i32| {
            let mut map = shell_test_map();
            if let Some(peer) = map.peer.as_mut() {
                peer.mtu = Some(mtu);
            }
            ShellNetworkConfig::from_map_gated(&map, false, false)
        };

        // usable announcements travel verbatim
        assert_eq!(with_mtu(1380).mtu, Some(1380));
        assert_eq!(with_mtu(576).mtu, Some(576));
        assert_eq!(with_mtu(1500).mtu, Some(1500));

        // outside the envelope: not exported (the shell keeps the default)
        assert_eq!(with_mtu(575).mtu, None);
        assert_eq!(with_mtu(1501).mtu, None);
        assert_eq!(with_mtu(8192).mtu, None);
        assert_eq!(with_mtu(-1).mtu, None);
        assert_eq!(with_mtu(0).mtu, None);

        // unset: the official server's normal answer (`toPeerConfig` leaves the
        // field 0) — nothing announced, nothing exported
        let mut unset = shell_test_map();
        if let Some(peer) = unset.peer.as_mut() {
            peer.mtu = None;
        }
        let snap = ShellNetworkConfig::from_map_gated(&unset, false, false);
        assert_eq!(snap.mtu, None);
        assert!(snap.to_json().ends_with(",\"mtu\":null}"), "{}", snap.to_json());
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
        // peers = remote + offline, public key + allowed-ips count + fqdn
        assert_eq!(snap.peers.len(), 3);
        assert_eq!(snap.peers[0].pub_key_b64, "UEVFUjA=");
        assert_eq!(snap.peers[0].allowed_ips, 2);
        assert_eq!(
            snap.peers[0].vpn_addresses,
            vec!["10.30.30.1/32".to_string(), "192.168.7.0/24".to_string()]
        );
        assert_eq!(snap.peers[0].fqdn.as_deref(), Some("peer-a.netbird.cloud"));
        assert_eq!(snap.peers[1].fqdn, None);
        assert_eq!(snap.peers[2].pub_key_b64, "T0ZGTElORQ==");
        assert_eq!(snap.peers[2].allowed_ips, 1);
        // 2026-10-06 peer-online semantics: remote peers carry the
        // management-online verdict, offlinePeers do not
        assert!(snap.peers[0].mgmt_online && snap.peers[1].mgmt_online);
        assert!(!snap.peers[2].mgmt_online);
        // own fqdn (UI phase 1): the Home screen's domain line
        assert_eq!(snap.fqdn.as_deref(), Some("me.netbird.cloud"));

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
              \"vpn_addresses\":[\"10.30.30.1/32\",\"192.168.7.0/24\"],\"fqdn\":\"peer-a.netbird.cloud\",\
              \"mgmt_online\":true}",
            // a peer without fqdn renders an explicit null (never invented)
            "\"fqdn\":null",
            // the offline peer carries its management verdict verbatim
            "{\"pub_key\":\"T0ZGTElORQ==\",\"allowed_ips\":1,\
              \"vpn_addresses\":[\"10.9.9.9/32\"],\"fqdn\":null,\"mgmt_online\":false}",
            // N3-7 gate markers
            "\"default_route\":{\"allowed\":false,\"reason\":\"default-route-held:data-plane-not-ready\"}",
            // N2-H I2: the tail-appended stun section (nothing announced on a
            // from_map_gated snapshot = an honest empty array, never invented)
            "\"stuns\":[]",
            // 2026-10-12 upstream MTU alignment: the announced PeerConfig.mtu
            // (shell_test_map carries 1380) rides the tail verbatim
            "\"mtu\":1380",
        ] {
            assert!(json.contains(fragment), "missing {fragment} in {json}");
        }
        // N2-H I2 tail-append discipline + UI phase 1 + upstream MTU alignment:
        // the announced-MTU section is the NEW last field — every pre-existing
        // field keeps its order and shape (fqdn stays directly before it)
        assert!(
            json.ends_with(",\"stuns\":[],\"fqdn\":\"me.netbird.cloud\",\"mtu\":1380}"),
            "{json}"
        );
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
            shared.relay_ads.clone(),
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
            "\"stuns\":[{{\"uri\":\"{}\"}},{{\"uri\":\"{}\"}},{{\"uri\":\"{}\"}},{{\"uri\":\"{}\"}}],\"fqdn\":\"me.netbird.cloud\",\"mtu\":1380}}",
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
        assert!(
            cleared.ends_with(",\"stuns\":[],\"fqdn\":\"me.netbird.cloud\",\"mtu\":1380}"),
            "{cleared}"
        );
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

        // shape validation (pure parser). T0 e15ac: an EXPLICIT empty array is a
        // legal applied set now (see the dedicated test) — the missing key, bad
        // entry types and a non-object root stay loud errors, and the document is
        // validated BEFORE the connector lookup.
        assert!(parse_route_set("{\"routes\":[\"0.0.0.0/0\",\"10.0.0.0/8\"]}").is_ok());
        assert_eq!(parse_route_set("{\"routes\":[]}").expect("explicit empty"), Vec::<String>::new());
        assert!(parse_route_set("{\"routes\":[1,2]}").is_err(), "non-string rejected");
        assert!(parse_route_set("{}").is_err(), "missing field rejected");
        assert!(parse_route_set("[]").is_err(), "non-object rejected");
        assert_eq!(
            connector_route_set_applied_json("{\"routes\":[1,2]}", true),
            "{\"ok\":false,\"error\":\"route-set-invalid\"}",
            "a malformed document is refused even with no connector running"
        );
        assert_eq!(
            connector_route_set_applied_kind_json("{\"routes\":[\"0.0.0.0/0\"]}", "userish"),
            "{\"ok\":false,\"error\":\"kind-invalid\"}",
            "an unknown kind never falls back onto a budget-spending kind"
        );
        // the legacy 2-arg NAPI surface is preserved verbatim
        let _legacy: fn(&str, bool) -> String = connector_route_set_applied_json;
        let _kind: fn(&str, &str) -> String = connector_route_set_applied_kind_json;
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
            shared.relay_ads.clone(),
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
            wg_apply_failed: false,
            wg_apply_errors: 0,
            logout_ok: None,
            terminal: false,
            ice: orch.lock_poison().summary(),
            signal: rt.status(),
            wg: crate::wg_device::WgDataplaneStatus::default(),
            recreate: RecreateStatus::default(),
            relay: RelayStatus::disabled(),
            peer_relays: None,
            ice_peers: None,
            ice_mux: None,
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
        assert_eq!(st.to_json(), "{\"enabled\":false,\"state\":\"disabled\",\"urls\":[],\"reconnects\":0,\"frames_tx\":0,\"frames_rx\":0,\"transport_bytes\":0,\"token_valid\":false,\"last_error_class\":null,\"current_url\":null}");
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
        // object keeps its pre-change field order (current_url is the one
        // tail-appended addition), and NO token material appears anywhere
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
             \"last_error_class\":null,\"current_url\":null}"
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
            shared.relay_ads.clone(),
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
            wg_apply_failed: false,
            wg_apply_errors: 0,
            logout_ok: None,
            terminal: false,
            ice: IceOrchestratorSummary::default(),
            signal: SignalLinkStatus::default(),
            wg: crate::wg_device::WgDataplaneStatus::default(),
            recreate: RecreateStatus::default(),
            relay: st,
            peer_relays: None,
            ice_peers: None,
            ice_mux: None,
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
                 \"token_valid\":false,\"last_error_class\":null,\"current_url\":null}"
            ),
            "frozen relay object keeps its field order (current_url tail-appended): {json}"
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

    // -------------------------------------------------------------------
    // N13-D3 — per-peer relay server (controller-based meeting point)
    // -------------------------------------------------------------------

    /// The official `isController` rule (conn.go:1040-1042): the REMOTE is
    /// the controller — and its advertised relay the meeting point — exactly
    /// when its WG public key sorts ABOVE ours. Same asymmetric string
    /// compare the ICE layer uses, so both sides agree without negotiation.
    #[test]
    fn remote_controller_is_the_higher_wg_public_key() {
        let low = relay_lane_key(0x10);
        let high = relay_lane_key(0x50);
        assert!(!remote_is_controller(&high, &low), "a lower remote key means WE control");
        assert!(remote_is_controller(&low, &high), "a higher remote key means the remote controls");
        assert!(!remote_is_controller(&low, &low), "equal keys are not 'the remote controls'");
    }

    /// The per-peer relay choice is CONTROLLER-based — never "always the
    /// remote's advertised URL" (which would break our own controller role)
    /// and never "always home" (which would ignore the controller's meet
    /// point):
    /// - we control → OUR relay, even when the peer advertises another one;
    /// - the remote controls → the URL it advertised, UNLESS that URL IS our
    ///   live home relay (canonical compare: scheme case and default ports
    ///   normalize) or, with no live session yet, it is one of our configured
    ///   URLs (the dial round may land there);
    /// - nothing / an unparseable advertisement → home (our long-standing
    ///   single-relay shape; upstream would skip the relay lane entirely).
    #[test]
    fn relay_choice_follows_the_controller_and_is_never_unconditional() {
        let configured = vec![
            crate::relay_client::RelayUrl::parse("rel://home.example").expect("configured home"),
        ];
        let other = crate::relay_client::RelayUrl::parse("rels://ctrl.example").expect("peer relay");
        let live = Some("rel://home.example:80"); // canonical form of the configured home

        // we control: the peer's advertisement is NOT the meeting point
        assert_eq!(
            choose_relay_url(Some("rels://ctrl.example"), false, live, &configured),
            RelayChoice::Home
        );
        // the remote controls and advertises a DIFFERENT relay
        assert_eq!(
            choose_relay_url(Some("rels://ctrl.example"), true, live, &configured),
            RelayChoice::Foreign(other.clone())
        );
        // the remote controls but advertises OUR live relay, spelled with a
        // different scheme case / default port → canonical compare says home
        assert_eq!(
            choose_relay_url(Some("RELS://home.example"), true, Some("rels://home.example:443"), &configured),
            RelayChoice::Home
        );
        assert_eq!(
            choose_relay_url(Some("rel://home.example"), true, live, &configured),
            RelayChoice::Home
        );
        // no live session yet: an advertisement inside our configured set is
        // still home
        assert_eq!(
            choose_relay_url(Some("rel://home.example:80"), true, None, &configured),
            RelayChoice::Home
        );
        // nothing advertised / unparseable → home, never a blind foreign dial
        assert_eq!(choose_relay_url(None, true, live, &configured), RelayChoice::Home);
        assert_eq!(choose_relay_url(Some("not-a-relay-url"), true, live, &configured), RelayChoice::Home);
    }

    /// An OFFER/ANSWER frame exactly as the signal worker hands it to the
    /// receive path (the `SignalLinkEvent::Message` body).
    fn relay_offer_from(peer_key: &str, advertised: Option<&str>) -> SignalMessage {
        SignalMessage {
            from_key: peer_key.to_string(),
            remote_key: relay_lane_key(0x40),
            kind: crate::signal::proto::body::Type::Offer,
            payload: "ufrag:pwd".to_string(),
            wg_listen_port: 51820,
            net_bird_version: String::new(),
            session_id: None,
            relay_server_address: advertised.map(str::to_string),
        }
    }

    fn relay_wiring_attach(
        clock: Arc<crate::relay_client::VirtualClock>,
        wg_pubkey_b64: &str,
    ) -> RelayAttach {
        RelayAttach {
            wg_pubkey_b64: wg_pubkey_b64.to_string(),
            materials: Arc::new(RelayMaterials {
                clock,
                dialer: Arc::new(crate::relay_client::TcpDialer),
                tls: None, // rel:// loopback topology only (spec §1.1)
            }),
        }
    }

    /// N13-D3 fixture: a REAL home relay client (Ready against `home`), the
    /// carrier armed through the production relay-sync path (shared
    /// advertisement map + foreign spawn context) and a carrier-capable
    /// recording WG seam. The two relay seams `ConnectorHandle::
    /// spawn_with_relay` fills are filled here the same way.
    struct RelayWiringFixture {
        home: RelayTestServer,
        foreign: RelayTestServer,
        shared: Arc<ConnectorShared>,
        wg: Arc<CarrierProbeWg>,
        carrier: Arc<RelayCarrier>,
        home_client: crate::relay_client::RelayClient,
        local_key: String,
        home_canonical: String,
        foreign_canonical: String,
    }

    async fn relay_wiring_fixture() -> RelayWiringFixture {
        let home = RelayTestServer::start(TestServerConfig::default()).await.expect("home relay");
        let foreign =
            RelayTestServer::start(TestServerConfig::default()).await.expect("foreign relay");
        let clock = Arc::new(crate::relay_client::VirtualClock::new(RELAY_TEST_EPOCH));
        // Local WG key chosen so the peer keys below sort predictably against
        // it (the premise asserts in the tests pin this).
        let local_key = relay_lane_key(0x40);
        let shared =
            Arc::new(ConnectorShared::new(false, Some(relay_wiring_attach(clock, &local_key))));
        let wg = Arc::new(CarrierProbeWg::default());
        let _ = shared.relay_wg.set(wg.clone() as Arc<dyn WgPeerApplier>);
        let _ = shared.relay_runtime.set(tokio::runtime::Handle::current());
        let _ = shared.ice.set(Arc::new(Mutex::new(PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![])),
            socks: Arc::new(
                crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"),
            ),
            signal: Arc::new(LoggingSignalExchange::default()),
            wg: wg.clone(),
            tie_breaker: Some(7),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        }))));

        let home_url = format!("rel://127.0.0.1:{}", home.addr().port());
        let foreign_url = format!("rel://127.0.0.1:{}", foreign.addr().port());
        // THE production relay-sync path (the relay block of `apply_update`):
        // home client starts, carrier armed with the shared ads map, foreign
        // spawn context set.
        shared.apply_update(
            wg.as_ref(),
            &NoopHost,
            &advertised_urls_update(Some(advertised_relay_config(vec![home_url.clone()]))),
        );
        assert!(
            relay_wait_until(Duration::from_secs(5), || shared.relay_ready()).await,
            "the sync's relay client must reach Ready against the fake server"
        );
        let carrier = shared.relay_carrier.get().expect("the sync must arm the carrier").clone();
        let home_client = shared.relay_handle().expect("the sync must start the home client");
        RelayWiringFixture {
            home_canonical: crate::relay_client::RelayUrl::parse(&home_url)
                .expect("home url")
                .config_string(),
            foreign_canonical: crate::relay_client::RelayUrl::parse(&foreign_url)
                .expect("foreign url")
                .config_string(),
            home,
            foreign,
            shared,
            wg,
            carrier,
            home_client,
            local_key,
        }
    }

    /// The per-peer lane rides the relay server the CONTROLLER rule chose —
    /// proven end to end over real sockets: the sync path arms the carrier
    /// with the connector's shared advertisement map and the foreign spawn
    /// context, the signal receive path records each peer's advertised relay
    /// ([`route_signal_message`], the worker's message arm), and the lane
    /// then dials a REAL second relay client to a REAL second fake relay
    /// server for the remote-controller peers only. Two peers advertising the
    /// same URL share ONE client, and connector teardown closes it
    /// gracefully with no lane and no client left behind.
    #[tokio::test(flavor = "multi_thread")]
    async fn relay_sync_advertisement_moves_the_lane_to_the_controller_relay() {
        let fx = relay_wiring_fixture().await;

        // the two peers whose key puts THEM in control advertise a second
        // relay; the peer WE control advertises it too — which must not win.
        let pub_ctrl_a = relay_lane_key(0x50);
        let pub_ctrl_b = relay_lane_key(0x51);
        let pub_ours = relay_lane_key(0x10);
        assert!(remote_is_controller(&fx.local_key, &pub_ctrl_a), "premise: peer sorts above us");
        assert!(remote_is_controller(&fx.local_key, &pub_ctrl_b), "premise: peer sorts above us");
        assert!(!remote_is_controller(&fx.local_key, &pub_ours), "premise: we control this peer");

        let ice = fx.shared.ice.get().expect("orchestrator").clone();
        for key in [&pub_ctrl_a, &pub_ctrl_b, &pub_ours] {
            route_signal_message(
                &ice,
                &fx.shared.relay_ads,
                &relay_offer_from(key, Some(&fx.foreign_canonical)),
            );
        }
        {
            let ads = fx.shared.relay_ads.lock().unwrap_or_else(PoisonError::into_inner);
            assert_eq!(
                ads.get(&pub_ctrl_a).map(String::as_str),
                Some(fx.foreign_canonical.as_str()),
                "the receive path must record the advertisement into the carrier's map"
            );
            assert_eq!(ads.len(), 3);
        }

        // the lane universe the sync path feeds from the network map
        fx.carrier.set_peers(&[pub_ours.clone(), pub_ctrl_a.clone(), pub_ctrl_b.clone()]);

        // each peer comes online on the relay its lane dialed. The
        // remote-controller lanes need the SECOND client to have dialed and
        // subscribed first: the first attempt races the fresh client's dial
        // (typed `not-ready`, observed) and the retry lands after
        // LANE_OPEN_RETRY — hence the wider fuse here.
        assert!(
            relay_wait_until(Duration::from_secs(20), || {
                fx.foreign.stats().frames_rx(crate::relay::MSG_SUBSCRIBE_PEER_STATE) >= 2
            })
            .await,
            "both remote-controller peers must subscribe on the advertised relay"
        );
        let _ = fx.foreign.set_peer_online(&PeerId::from_wg_pubkey_string(&pub_ctrl_a), true);
        let _ = fx.foreign.set_peer_online(&PeerId::from_wg_pubkey_string(&pub_ctrl_b), true);
        assert!(
            relay_wait_until(Duration::from_secs(5), || {
                fx.home.stats().frames_rx(crate::relay::MSG_SUBSCRIBE_PEER_STATE) >= 1
            })
            .await,
            "the peer we control must subscribe on OUR relay"
        );
        let _ = fx.home.set_peer_online(&PeerId::from_wg_pubkey_string(&pub_ours), true);
        assert!(
            relay_wait_until(Duration::from_secs(5), || fx.carrier.lane_count() == 3).await,
            "all three lanes must attach (stats={:?})",
            fx.carrier.stats()
        );

        // every lane is on the relay the controller rule chose
        {
            let attached = fx.carrier.attached.lock().unwrap_or_else(PoisonError::into_inner);
            let (lane_ours, via_ours) =
                attached.get(&pub_ours).expect("the controlled peer's lane");
            assert_eq!(
                via_ours, &fx.home_canonical,
                "we control this peer: the meeting point is OUR relay, not its advertisement"
            );
            assert!(
                lane_ours.client().same_client(&fx.home_client),
                "…and the lane rides the home client"
            );
            for key in [&pub_ctrl_a, &pub_ctrl_b] {
                let (lane, via) = attached.get(key).expect("the controller peer's lane");
                assert_eq!(
                    via, &fx.foreign_canonical,
                    "the remote controls: its advertised relay is the meeting point"
                );
                assert!(
                    !lane.client().same_client(&fx.home_client),
                    "…and the lane rides a SECOND relay client, not home"
                );
            }
        }
        assert_eq!(fx.carrier.foreign_count(), 1, "one client per relay URL (two peers share it)");
        assert_eq!(fx.carrier.foreign_urls(), vec![fx.foreign_canonical.clone()]);
        assert_eq!(
            fx.foreign.stats().connections_accepted,
            1,
            "the shared foreign client dialed the advertised relay exactly once"
        );
        assert_eq!(
            fx.home.stats().connections_accepted,
            1,
            "the home client never re-dialed the advertised relay"
        );

        // teardown: no lane, no foreign client, no orphan outbound connection
        fx.shared.stop_relay();
        assert_eq!(fx.carrier.foreign_count(), 0, "stop must tear the foreign clients down");
        assert_eq!(fx.carrier.lane_count(), 0, "no lane may survive stop");
        assert!(
            relay_wait_until(Duration::from_secs(5), || {
                fx.foreign.stats().frames_rx(crate::relay::MSG_CLOSE) == 1
            })
            .await,
            "the foreign client must close gracefully (relay Close frame)"
        );
        assert_eq!(fx.carrier.stats().foreign_opened, 1);
        assert_eq!(fx.carrier.stats().foreign_closed, 1);
        {
            let detached = fx.wg.detached.lock().unwrap_or_else(PoisonError::into_inner);
            for key in [&pub_ours, &pub_ctrl_a, &pub_ctrl_b] {
                assert!(detached.contains(key), "every lane must be detached from the WG seam");
            }
        }
        let RelayWiringFixture { home, foreign, .. } = fx;
        home.shutdown().await;
        foreign.shutdown().await;
    }

    /// The ordering case the lane wiring must tolerate: the lane can attach
    /// on OUR relay BEFORE the peer's advertisement has arrived (the sync
    /// starts lanes off the network map; the OFFER/ANSWER that carries
    /// `relayServerAddress` comes later over the signal link). When the
    /// advertisement lands, the next pump pass must MOVE that lane to the
    /// controller's relay — detaching the home lane and re-opening it on a
    /// REAL second relay client; without that reconcile the lane would stay
    /// on a relay the controller is not on for the whole session.
    #[tokio::test(flavor = "multi_thread")]
    async fn advertisement_arriving_after_the_lane_moves_it_to_the_controller_relay() {
        let fx = relay_wiring_fixture().await;
        let pub_ctrl = relay_lane_key(0x50);
        assert!(remote_is_controller(&fx.local_key, &pub_ctrl), "premise: peer sorts above us");

        // 1. no advertisement yet: the lane opens on OUR relay
        fx.carrier.set_peers(&[pub_ctrl.clone()]);
        assert!(
            relay_wait_until(Duration::from_secs(5), || {
                fx.home.stats().frames_rx(crate::relay::MSG_SUBSCRIBE_PEER_STATE) >= 1
            })
            .await,
            "the pre-advertisement lane must open on the home relay"
        );
        let _ = fx.home.set_peer_online(&PeerId::from_wg_pubkey_string(&pub_ctrl), true);
        assert!(
            relay_wait_until(Duration::from_secs(5), || fx.carrier.lane_count() == 1).await,
            "the lane must attach on the home relay (stats={:?})",
            fx.carrier.stats()
        );
        {
            let attached = fx.carrier.attached.lock().unwrap_or_else(PoisonError::into_inner);
            let (lane, via) = attached.get(&pub_ctrl).expect("home lane");
            assert_eq!(via, &fx.home_canonical, "no advertisement ⇒ our relay (fallback)");
            assert!(lane.client().same_client(&fx.home_client));
        }

        // 2. the advertisement arrives over the signal link
        let ice = fx.shared.ice.get().expect("orchestrator").clone();
        route_signal_message(
            &ice,
            &fx.shared.relay_ads,
            &relay_offer_from(&pub_ctrl, Some(&fx.foreign_canonical)),
        );

        // 3. the reconcile: the home lane is detached and re-opened on the
        //    controller's relay (the fresh client's first attempt races its
        //    dial and retries after LANE_OPEN_RETRY — wider fuse).
        assert!(
            relay_wait_until(Duration::from_secs(20), || {
                fx.foreign.stats().frames_rx(crate::relay::MSG_SUBSCRIBE_PEER_STATE) >= 1
            })
            .await,
            "the changed advertisement must move the lane to the controller's relay"
        );
        let _ = fx.foreign.set_peer_online(&PeerId::from_wg_pubkey_string(&pub_ctrl), true);
        assert!(
            relay_wait_until(Duration::from_secs(5), || {
                fx.carrier.lane_count() == 1
                    && fx
                        .carrier
                        .attached
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .get(&pub_ctrl)
                        .is_some_and(|(_, via)| via == &fx.foreign_canonical)
            })
            .await,
            "the lane must end up on the controller's relay (stats={:?})",
            fx.carrier.stats()
        );
        {
            let attached = fx.carrier.attached.lock().unwrap_or_else(PoisonError::into_inner);
            let (lane, via) = attached.get(&pub_ctrl).expect("moved lane");
            assert_eq!(via, &fx.foreign_canonical);
            assert!(!lane.client().same_client(&fx.home_client), "…on a second relay client");
        }
        assert_eq!(fx.carrier.foreign_count(), 1);
        assert!(
            fx.wg
                .detached
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .contains(&pub_ctrl),
            "the superseded home lane must be detached from the WG seam"
        );
        assert_eq!(
            fx.home.stats().connections_accepted,
            1,
            "the home client is re-used as-is — the move never re-dials our own relay"
        );

        fx.shared.stop_relay();
        assert_eq!(fx.carrier.foreign_count(), 0);
        let RelayWiringFixture { home, foreign, .. } = fx;
        home.shutdown().await;
        foreign.shutdown().await;
    }

    /// N13-D3 cleanup: a foreign client with NO live lane is swept away by the
    /// idle policy (official `keepUnusedServerTime` / `relayCleanupInterval`)
    /// — the foreign pump's own tick closes it, so a peer that leaves the
    /// network map cannot leave an outbound relay connection behind.
    #[tokio::test(flavor = "multi_thread")]
    async fn foreign_relay_idle_sweep_closes_the_unused_client() {
        let fx = relay_wiring_fixture().await;
        let pub_ctrl = relay_lane_key(0x50);
        assert!(remote_is_controller(&fx.local_key, &pub_ctrl), "premise: peer sorts above us");

        let ice = fx.shared.ice.get().expect("orchestrator").clone();
        route_signal_message(
            &ice,
            &fx.shared.relay_ads,
            &relay_offer_from(&pub_ctrl, Some(&fx.foreign_canonical)),
        );
        fx.carrier.set_peers(&[pub_ctrl.clone()]);

        assert!(
            relay_wait_until(Duration::from_secs(20), || {
                fx.foreign.stats().frames_rx(crate::relay::MSG_SUBSCRIBE_PEER_STATE) >= 1
            })
            .await,
            "the lane open must reach the advertised relay (retry after LANE_OPEN_RETRY)"
        );
        let _ = fx.foreign.set_peer_online(&PeerId::from_wg_pubkey_string(&pub_ctrl), true);
        assert!(
            relay_wait_until(Duration::from_secs(5), || fx.carrier.lane_count() == 1).await,
            "the controller peer's lane must attach on the foreign relay (stats={:?})",
            fx.carrier.stats()
        );
        assert_eq!(fx.carrier.foreign_count(), 1);

        // the peer leaves the map → its lane detaches and the client has no
        // user left. Deterministic idle policy: no grace, sweep every tick.
        fx.carrier.set_foreign_idle_params(Duration::ZERO, Duration::ZERO);
        fx.carrier.set_peers(&[]);
        assert!(
            relay_wait_until(Duration::from_secs(5), || fx.carrier.lane_count() == 0).await,
            "the departed peer's lane must detach"
        );
        assert!(
            relay_wait_until(Duration::from_secs(5), || fx.carrier.foreign_count() == 0).await,
            "the unused foreign client must be swept away (stats={:?})",
            fx.carrier.stats()
        );
        assert!(
            relay_wait_until(Duration::from_secs(5), || {
                fx.foreign.stats().frames_rx(crate::relay::MSG_CLOSE) == 1
            })
            .await,
            "the swept client must close gracefully (relay Close frame)"
        );
        assert_eq!(fx.carrier.stats().foreign_opened, 1);
        assert_eq!(fx.carrier.stats().foreign_closed, 1);

        fx.shared.stop_relay();
        let RelayWiringFixture { home, foreign, .. } = fx;
        home.shutdown().await;
        foreign.shutdown().await;
    }

    /// N13-D3 bound: an advertisement is only ever useful for a peer the
    /// carrier may open a lane to, so the shared map is pruned to the
    /// connectable set on every network map — also WITHOUT a carrier
    /// (`relay_enabled:false`), where nothing else would ever prune it. The
    /// sender key of a signal frame is only as authenticated as its envelope
    /// (the server routes by RECIPIENT), so a key outside the network map
    /// must not be able to grow the map without bound.
    #[tokio::test]
    async fn relay_ads_are_bounded_to_the_connectable_peer_set_without_a_carrier() {
        let shared = Arc::new(ConnectorShared::new(false, None));
        {
            let mut ads = shared.relay_ads.lock().unwrap_or_else(PoisonError::into_inner);
            // the connectable peer, the map's allowed_ips-less peer and a key
            // that is not in the map at all
            ads.insert("UEVFUjA=".to_string(), "rels://ctrl.example".to_string());
            ads.insert("UEVFUjE=".to_string(), "rels://ctrl.example".to_string());
            ads.insert(relay_lane_key(0x99), "rels://rogue.example".to_string());
        }
        assert!(shared.relay_carrier.get().is_none(), "no relay opt-in ⇒ no carrier");

        let wg = WgPeerRegistry::new();
        shared.apply_update(
            &wg,
            &NoopHost,
            &SyncUpdate {
                session_deadline_unix: None,
                netbird_config: None,
                network_map: Some(shell_test_map()),
            },
        );
        let ads = shared.relay_ads.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(
            ads.keys().cloned().collect::<Vec<_>>(),
            vec!["UEVFUjA=".to_string()],
            "only a peer with allowed_ips (the carrier's lane universe) keeps an advertisement"
        );
    }

    /// 2026-10-06 (`netbird status -d` per-peer relay): the status document's
    /// `peer_relays` tail must carry the ACTUAL relay of each attached lane —
    /// the relay URL the lane really rides, taken from the carrier's live
    /// attached table — and each entry must DISAPPEAR with its lane on
    /// detach. The remote's ADVERTISED URL (`relay_ads`) and the global home
    /// URL are NOT evidence of an actual per-peer relay: the peer we control
    /// advertises the FOREIGN relay here and still must be reported on OUR
    /// relay (the controller rule decides the meeting point, not the
    /// advertisement). Lanes run over the existing REAL fake-relay fixture —
    /// no new fixture.
    #[tokio::test(flavor = "multi_thread")]
    async fn status_peer_relays_exports_the_actual_attached_lane_and_clears_on_detach() {
        let fx = relay_wiring_fixture().await;
        let pub_ctrl = relay_lane_key(0x50);
        let pub_ours = relay_lane_key(0x10);
        assert!(remote_is_controller(&fx.local_key, &pub_ctrl), "premise: peer sorts above us");
        assert!(!remote_is_controller(&fx.local_key, &pub_ours), "premise: we control this peer");

        // BOTH peers advertise the FOREIGN relay — only the controller's lane
        // may actually ride it.
        let ice = fx.shared.ice.get().expect("orchestrator").clone();
        for key in [&pub_ctrl, &pub_ours] {
            route_signal_message(
                &ice,
                &fx.shared.relay_ads,
                &relay_offer_from(key, Some(&fx.foreign_canonical)),
            );
        }
        fx.carrier.set_peers(&[pub_ours.clone(), pub_ctrl.clone()]);

        // the controlled peer's lane lands on OUR relay
        assert!(
            relay_wait_until(Duration::from_secs(5), || {
                fx.home.stats().frames_rx(crate::relay::MSG_SUBSCRIBE_PEER_STATE) >= 1
            })
            .await,
            "the controlled peer must subscribe on our relay"
        );
        let _ = fx.home.set_peer_online(&PeerId::from_wg_pubkey_string(&pub_ours), true);
        // the controller peer's lane lands on ITS advertised (foreign) relay —
        // the fresh client's first attempt races its dial and retries
        assert!(
            relay_wait_until(Duration::from_secs(20), || {
                fx.foreign.stats().frames_rx(crate::relay::MSG_SUBSCRIBE_PEER_STATE) >= 1
            })
            .await,
            "the controller peer must subscribe on its advertised relay"
        );
        let _ = fx.foreign.set_peer_online(&PeerId::from_wg_pubkey_string(&pub_ctrl), true);
        assert!(
            relay_wait_until(Duration::from_secs(5), || fx.carrier.lane_count() == 2).await,
            "both lanes must attach (stats={:?})",
            fx.carrier.stats()
        );

        // THE EXPORT: the carrier's actual lane map — public-key sorted, one
        // ACTUAL URL per attached lane (never the advertisement as such).
        let lanes = fx.carrier.peer_relays();
        assert_eq!(
            lanes,
            vec![
                PeerRelayStatus {
                    pub_key_b64: pub_ours.clone(),
                    relay_url: fx.home_canonical.clone(),
                },
                PeerRelayStatus {
                    pub_key_b64: pub_ctrl.clone(),
                    relay_url: fx.foreign_canonical.clone(),
                },
            ],
            "peer_relays must report the relay each lane ACTUALLY rides"
        );
        assert!(
            !lanes.iter().any(|p| p.relay_url.contains("token") || p.relay_url.contains('@')),
            "only public URLs cross the status boundary: {lanes:?}"
        );

        // the status DOCUMENT carries it (production assembly + tail JSON).
        let status = ConnectorStatus::snapshot(
            &fx.shared,
            fx.wg.as_ref(),
            Some(lanes),
            &fx.shared.ice_status_sample(),
        );
        let json = status.to_json();
        assert!(
            json.contains(&format!(
                "\"peer_relays\":[{{\"pub_key_b64\":\"{pub_ours}\",\
                 \"relay_url\":\"{}\"}},{{\"pub_key_b64\":\"{pub_ctrl}\",\
                 \"relay_url\":\"{}\"}}]",
                fx.home_canonical, fx.foreign_canonical
            )),
            "the document must export the actual per-peer relay: {json}"
        );
        // ... and the 2026-10-07 `ice_peers` tail follows it (this fixture arms
        // a REAL orchestrator, so the tail is an ARRAY — `[]`/entries — never
        // the no-orchestrator `null`).
        assert!(
            json.contains(",\"ice_peers\":["),
            "the armed orchestrator renders an ice_peers array, not null: {json}"
        );
        assert!(
            json.rfind("\"peer_relays\":").expect("peer_relays section")
                < json.rfind("\"ice_peers\":").expect("ice_peers section"),
            "ice_peers is tail-appended AFTER peer_relays: {json}"
        );
        assert!(
            json.ends_with("],\"ice_mux\":null}"),
            "the ice_mux tail follows the ice_peers array: {json}"
        );
        assert!(matches!(config::parse_document(&json), Ok(Json::Obj(_))), "{json}");
        // the negative premise, pinned: the controlled peer's ADVERTISEMENT is
        // the foreign relay while its exported ACTUAL relay is ours.
        assert_eq!(
            fx.shared
                .relay_ads
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&pub_ours)
                .map(String::as_str),
            Some(fx.foreign_canonical.as_str()),
            "an advertisement is NOT an actual relay (the controller rule won)"
        );

        // DETACH: the peer leaves the lane universe → no lane, no entry.
        fx.carrier.set_foreign_idle_params(Duration::ZERO, Duration::ZERO);
        fx.carrier.set_peers(&[]);
        assert!(
            relay_wait_until(Duration::from_secs(5), || fx.carrier.lane_count() == 0).await,
            "every lane must detach (stats={:?})",
            fx.carrier.stats()
        );
        assert!(
            fx.carrier.peer_relays().is_empty(),
            "a detached lane must leave NO peer_relays entry: {:?}",
            fx.carrier.peer_relays()
        );
        let empty_status = ConnectorStatus::snapshot(
            &fx.shared,
            fx.wg.as_ref(),
            Some(fx.carrier.peer_relays()),
            &fx.shared.ice_status_sample(),
        );
        let empty_json = empty_status.to_json();
        assert!(
            empty_json.contains("\"peer_relays\":[]"),
            "the armed-but-laneless carrier renders an empty array, not null: {empty_json}"
        );

        fx.shared.stop_relay();
        let RelayWiringFixture { home, foreign, .. } = fx;
        home.shutdown().await;
        foreign.shutdown().await;
    }
}
