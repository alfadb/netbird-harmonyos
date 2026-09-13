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
//! ## ⚠️ 限制声明（调用方必读，不得误读为"已能连上 peer"）
//!
//! **本增量不实现 signal/ICE**。因此：
//!
//! 1. 无法为任何 peer 发现可达 endpoint，也无法建立真实 peer 隧道；
//! 2. 网络图里的 remote peers 只做**本地登记**（公钥 + allowed_ips 进
//!    [`WgPeerApplier`]，生产默认落在进程内 [`WgPeerRegistry`]）——这只是
//!    WireGuard 侧的本地配置，不是连通性；
//! 3. 路由/DNS 通过 [`ConfigApplier`] 交给壳侧（宿主）；壳侧不接时生产
//!    默认 [`LoggingConfigApplier`] 只打点，不落任何系统配置；
//! 4. "Connected" 仅指 **management 控制面连接已建立**（登录成功 + Sync
//!    流在），与 peer 连通无关。真实 peer 连通留 N4/N5。
//!
//! ## 注入 seam（宿主测试与壳侧接线点）
//!
//! - management 连接：[`ManagementFactory`]（生产实现
//!   [`GrpcManagementFactory`]：tonic/rustls，TLS CA 由配置注入，无系统根
//!   存储——见 `crate::grpc` 模块文档）。宿主测试可注入桩工厂。
//! - sync 策略：[`SyncPolicy`]（backoff/时钟/随机源，透传给
//!   [`crate::sync::SyncSession::with_policy`]，测试零真实长睡眠）。
//! - 数据面 WG peer 登记：[`WgPeerApplier`]（生产默认 [`WgPeerRegistry`]，
//!   进程内登记表；未来真实 WG 设备层实现同一 trait）。
//! - 壳侧配置应用：[`ConfigApplier`]（路由/DNS/本机地址交给宿主）。
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
use crate::network_map::NetworkMap;
use crate::state::{ConnEvent, ConnState, StateMachine};
use crate::sync::{SyncLoopEvent, SyncSession, SyncUpdate};
use crate::util::{jbool, jinum, jnum, jstr};

/// Pinned upstream commit the lifecycle semantics are modeled on (same pin
/// as [`crate::grpc::UPSTREAM_COMMIT`]).
pub const UPSTREAM_COMMIT: &str = crate::grpc::UPSTREAM_COMMIT;

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
/// endpoint and no tunnel (module limitation statement).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgPeerEntry {
    pub pub_key_b64: String,
    pub allowed_ips: Vec<config::Route>,
}

/// WG data-plane seam: register/replace the local WireGuard peer set.
/// Production default: [`WgPeerRegistry`] (in-process registry). A future
/// real WG device layer implements the same trait.
pub trait WgPeerApplier: Send + Sync + 'static {
    /// Replace the registered peer set with `peers` (full snapshot
    /// semantics: the legacy wire format this client consumes carries full
    /// NetworkMap snapshots).
    fn apply_peers(&self, peers: &[WgPeerEntry]) -> Result<(), String>;
    /// Tear down local registration (connector stop).
    fn clear(&self);
}

/// In-process WG peer registry — the production default of
/// [`WgPeerApplier`]. Honest scope: local registration only; no endpoints,
/// no handshakes, no tunnels in this increment (module limitation).
#[derive(Debug, Default)]
pub struct WgPeerRegistry {
    peers: Mutex<Vec<WgPeerEntry>>,
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
}

impl WgPeerApplier for WgPeerRegistry {
    fn apply_peers(&self, peers: &[WgPeerEntry]) -> Result<(), String> {
        *self.peers.lock_poison() = peers.to_vec();
        Ok(())
    }

    fn clear(&self) {
        self.peers.lock_poison().clear();
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
/// [`crate::grpc::ManagementGrpcClient::connect`]).
#[derive(Debug, Clone)]
pub struct GrpcManagementFactory {
    endpoint: String,
    transport: GrpcTransport,
    connect_timeout: Duration,
    request_timeout: Duration,
    keys: EnvelopeKeyPair,
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
        Box::pin(async move {
            ManagementGrpcClient::connect(
                &endpoint,
                transport,
                connect_timeout,
                request_timeout,
                keys,
            )
            .await
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
    ///   "session_renew_lead_ms": 600000, "renew_check_interval_ms": 30000
    /// }
    /// ```
    ///
    /// `ca_pem` (string or array of strings) is REQUIRED for `https://`
    /// (no system store is used); it is ignored for `http://` (plaintext,
    /// test only). Unknown fields are ignored.
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
        })
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
}

/// State shared between the connector worker tasks and the status/stop
/// entry points. All mutation is mutex-guarded; the six-state machine is
/// `crate::state::StateMachine` driven through guarded (legal-only)
/// transitions.
struct ConnectorShared {
    inner: Mutex<StateInner>,
    running: AtomicBool,
}

impl ConnectorShared {
    fn new() -> Self {
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
            }),
            running: AtomicBool::new(false),
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
        {
            let mut g = self.lock();
            g.last_serial = map.serial;
            g.peer_count = peers;
            g.route_count = map.routes.len();
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
            "{{{},{},{},{},{},{},{},{},{},{},{},{},{},{}}}",
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
    /// `runtime`. Returns immediately; progress is polled via `status()`.
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
    ) -> Arc<ConnectorHandle> {
        let shared = Arc::new(ConnectorShared::new());
        shared.set_running(true);
        shared.lock().started_at_unix = Some(unix_now());

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
        let stop_flag = Arc::new(AtomicBool::new(false));
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
            worker: Mutex::new(Some(worker)),
            renewal: Mutex::new(Some(renewal)),
            runtime,
        })
    }

    /// Current status snapshot.
    pub fn status(&self) -> ConnectorStatus {
        let g = self.shared.lock();
        ConnectorStatus {
            running: self.shared.is_running(),
            state: g.machine.state(),
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
        }
    }

    /// The `connector_status()` JSON document.
    pub fn status_json(&self) -> String {
        self.status().to_json()
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
        self.wg.clear();
        self.host.clear();
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
/// Starts the lifecycle asynchronously and returns immediately: progress is
/// observed by polling [`connector_status_json`] (no NAPI callbacks — the
/// export stays synchronous, matching `tun_*`/`wg_*` style). Production
/// data-plane seams: [`WgPeerRegistry`] + [`LoggingConfigApplier`].
///
/// Returns `{"started":true,"state":"connecting"}` on success, or
/// `{"started":false,"error":"<reason>"}` — reason is a stable token
/// (`already-running` / `invalid-config` / `invalid-credentials`), never
/// secret material or server text.
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
    let handle = ConnectorHandle::spawn(
        global_runtime().handle().clone(),
        Arc::new(GrpcManagementFactory::new(&config, config.keys.clone())),
        Arc::new(WgPeerRegistry::new()),
        Arc::new(LoggingConfigApplier),
        secrets,
        config.meta.clone(),
        ExponentialBackoff::upstream_stream_default(),
        SyncPolicy::production(),
        config.renew_lead,
        config.renew_check_interval,
    );
    let state = handle.status().state;
    *slot = Some(handle);
    format!("{{{}, {}}}", jbool("started", true), jstr("state", state.as_str()))
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
        }
        .to_json(),
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
        }
        .to_json();
        assert!(empty.contains("\"running\":false"), "{empty}");
        assert!(empty.contains("\"state\":\"disconnected\""), "{empty}");
        assert!(empty.contains("\"last_error\":null"), "{empty}");
        assert!(empty.contains("\"session_expiry\":\"unknown\""), "{empty}");
        assert!(empty.contains("\"session_expires_at_unix\":null"), "{empty}");
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
}
