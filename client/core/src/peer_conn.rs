// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # peer_conn — per-peer ICE 编排：signal offer/answer/候选 → selected
//! pair → WG endpoint 落配（N5c）
//!
//! 把 N5a（[`crate::ice`] 候选收集）与 N5b（[`crate::ice_session`] 连通性
//! 检查/提名/keepalive）接进 connector 的 per-peer 数据面：网络图里每个
//! 有 allowed_ips 的 remote peer 一个 [`crate::ice_session::IceSession`]，
//! 经 signal 通道交换 OFFER/ANSWER（`"ufrag:pwd"` 凭证）与 CANDIDATE
//! （trickle），选出 pair 后把**对端 selected 地址**落到 WG 层
//! （[`crate::connector::WgPeerApplier::apply_endpoint`]），并暴露
//! per-peer 状态汇总（Idle/Gathering/Checking/Connected/Disconnected/
//! Failed + 错误分类，无任何密钥材料）。
//!
//! 本模块是**编排**，不重新实现任何 ICE 语义：检查、pair 状态机、提名、
//! tie-breaker、keepalive/超时全部复用 N5b 的 `IceSession`，时钟经
//! [`PeerIceOrchestrator::run_once`]`(now_ms)` 由调用方注入（模块内无墙钟
//! 读取、无 sleep；connector 生产路径用 100ms 泵线程喂数，测试直接注入
//! 模拟时钟）。
//!
//! ## 上游锚点（pinned commit `791401060d2b`，只引用 文件:行号）
//!
//! - **候选走 signal `Body{type=CANDIDATE, payload=candidate.Marshal()}`**：
//!   `client/internal/peer/signaler.go:32-41`（发送），
//!   `client/internal/engine.go:2063-2071`（接收方
//!   `ice.UnmarshalCandidate(msg.GetBody().Payload)`）。
//! - **OFFER/ANSWER payload = `"ufrag:pwd"`**：
//!   `shared/signal/client/client.go:74-101`（`MarshalCredential`，
//!   payload=L77；解析 L60-71 —— 恰好两个 `:` 分隔字段）。
//! - **消息按对端绑定**：`EncryptedMessage.key/remoteKey`（发送方公钥 /
//!   目的方公钥，`client/internal/engine.go:2039-2042`；本仓
//!   [`crate::signal`] 已实现同语义信封）。编排层因此只看「来自哪个
//!   peer 公钥」，传输细节留在 seam 之后。
//! - **ICE 成功 → WG endpoint**：`GetSelectedCandidatePair` →
//!   `ResolveUDPAddr` → `ConfigureWGEndpoint`
//!   （`client/internal/peer/worker_ice.go:293`、
//!   `client/internal/peer/conn.go:444-478`）。
//! - **keepalive 4s / 断连回落 relay**：`conn.go:489-520` —— 本增量不做
//!   relay：Disconnected/Failed 的 peer 标记不可达（绝不静默当作可用）。
//! - **角色**：RFC 8445 的 controlling/controlled 不在 signal 报文里；
//!   本编排的初始角色是**本地决策**——主动发 OFFER 的一方 controlling、
//!   应答方 controlled；双方同时 OFFER（glare）时两边都 controlling，
//!   由 N5b 已实现的 §7.3.1.1/§7.2.5.1 tie-breaker 修复收敛
//!   （`crate::ice_session` 模块文档「Role conflict」节，N5b 已测）。
//!
//! ## 受保护 socket（治理 §二.4，fail-closed）
//!
//! 候选收集与每个本地候选的检查 socket 全部来自注入的
//! [`crate::ice::UdpSocketSource`]（生产
//! [`crate::ice::ProtectedUdpFdSource`]，connector 侧经
//! `connector_ice_socket_feed` 由壳侧补给）；空源/绑定失败 → 该 peer 记录
//! Network 类错误并冷却重试，**绝不**发没有本地候选支撑的 OFFER/ANSWER，
//! 模块内无任何 `socket(2)` 调用、无未保护回退。
//!
//! ## 传输 seam
//!
//! [`SignalExchange`] 是编排对 signal 通道的唯一依赖（send 方向）；收方向
//! 由 connector 把解密后的 `SignalMessage`（`crate::signal`）转成
//! [`PeerIceOrchestrator::handle_signal`] 调用。
//!
//! 生产实现（N5d）：[`RealSignalExchange`] 把 `crate::signal::SignalSession`
//! 的真实流接进 seam——`send()` 经无界队列交给 signal worker 任务（注册后
//! 密封发送，peer=remote_key）；未注册时 `send()` 返回 `Network` 类错误
//! （upstream `ErrSignalIsNotReady` 同型，`client/internal/peer/
//! handshaker.go:16,208` —— `sendOffer` 前检查 `signaler.Ready()`，
//! handshaker.go:212-214），编排层把帧保留在 outbox 下一拍重试，
//! **绝不静默丢弃**（与 [`LoggingSignalExchange`] 的"打点丢弃"形成对照；
//! 后者保留为测试/对照实现，生产不再使用）。
//! [`spawn_signal_link`] 负责把受保护 socket 源 + `netbird_config.signal`
//! 端点组装成 worker 任务：初始受保护拨号失败按 backoff 重试（上游
//! `signal.NewClient` 在构造内重试拨号，`shared/signal/client/grpc.go:
//! 123-131`），此后 [`crate::signal::SignalSession::run_events_with_outbox`]
//! 驱动注册/收帧/发帧/断流重连（每次重连重新 register + 经 tonic 连接器
//! 重取受保护 fd，语义归 [`crate::signal`] 所有，本层不绕过）。

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::backoff::{ExponentialBackoff, MonotonicClock, OsRandom};
use crate::connector::{ErrorClass, WgPeerApplier};
use crate::envelope::EnvelopeKeyPair;
use crate::grpc::GrpcTransport;
use crate::ice::{
    Candidate, GatherConfig, InterfaceSource, StunServer, UdpSocketSource,
    DEFAULT_INTERFACE_BLACKLIST, DEFAULT_STUN_TIMEOUT_MS,
};
use crate::ice_session::{IceCredentials, IceEvent, IceSession};
use crate::management::ManagementError;
use crate::mgmtsock::ManagementSocketProvider;
use crate::signal::{SignalClient, SignalLoopEvent, SignalMessage, SignalOutgoing, SignalSession};
use crate::util::jnum;

/// 发起/应答失败后的重试冷却（注入时钟衡量；避免熵/收集失败时热循环）。
pub const RETRY_COOLDOWN_MS: u64 = 2000;

/// 握手重发周期（注入时钟衡量）：signal 服务端对未注册目的地的转发是
/// best-effort 丢弃（signal.go forwardMessageToPeer 的 not-connected
/// 路径，"todo respond to the sender?"），先发方的 OFFER/候选可能在
/// 对端注册前被丢掉且**无任何错误面**。会话未 Connected 前周期性把已
/// 发帧重新入队——上游 handshaker 对握手的重试职责（handshaker 会
/// 重发 OFFER 直到 ICE Connected / 失败重启）。
pub const HANDSHAKE_RETRY_MS: u64 = 3000;

/// 谈判启动超时（注入时钟衡量）：会话已建、本地候选已发，但 `start()`
/// 前置（远端凭证/候选）在 `3 * HANDSHAKE_RETRY_MS` 内始终没有凑齐——
/// 典型场景：对端仍持有旧会话（自认 Connected）而不再发 Offer/Answer，
/// 本端若不拆除这个永不 start 的「僵尸谈判」就永久沉默（真机
/// AUTH-DIAG-DEVICE-VALIDATION-20260913-0001 的 H3 形态）。到期即拆除
/// 会话、冷却后以**全新凭证**重新发起（上游 handshaker 超时重启语义，
/// worker_ice.go:584-593 / conn.go:495-534）。已 `start()` 的会话不适用
/// ——其死亡由 keepalive/断连/全对失败事件驱动，不由此超时判定。
pub const START_DEADLINE_MS: u64 = 3 * HANDSHAKE_RETRY_MS;

// ---------------------------------------------------------------------------
// 状态模型
// ---------------------------------------------------------------------------

/// 单个 peer 的 ICE 生命周期状态（任务规定的六态）。
///
/// `Gathering` = 会话已建、本地候选就绪但检查未启动（远端凭证/候选未齐）；
/// `Checking` = `IceSession::start()` 已跑；`Connected` = selected pair；
/// 可用（默认路由闸输入）还要求 WG endpoint 落配成功；
/// `Disconnected`/`Failed` 由 N5b 事件驱动。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerIceState {
    Idle,
    Gathering,
    Checking,
    Connected,
    Disconnected,
    Failed,
}

impl PeerIceState {
    pub fn as_str(&self) -> &'static str {
        match self {
            PeerIceState::Idle => "idle",
            PeerIceState::Gathering => "gathering",
            PeerIceState::Checking => "checking",
            PeerIceState::Connected => "connected",
            PeerIceState::Disconnected => "disconnected",
            PeerIceState::Failed => "failed",
        }
    }
}

/// 编排发出的 signal `Body.type` 子集（`signalexchange.proto` L45-52 的
/// `OFFER=0/ANSWER=1/CANDIDATE=2`；MODE/HEARTBEAT 等与本编排无关）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerSignalKind {
    Offer,
    Answer,
    Candidate,
}

impl PeerSignalKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            PeerSignalKind::Offer => "offer",
            PeerSignalKind::Answer => "answer",
            PeerSignalKind::Candidate => "candidate",
        }
    }
}

// ---------------------------------------------------------------------------
// seams
// ---------------------------------------------------------------------------

/// signal 发送 seam（编排 → 远端 peer）。收方向由 connector 把解密消息
/// 交给 [`PeerIceOrchestrator::handle_signal`]。`to_key` 是目的 peer 的
/// base64 WG 公钥（`EncryptedMessage.remoteKey` 语义）；`payload` 对
/// OFFER/ANSWER 是 `"ufrag:pwd"`，对 CANDIDATE 是 `candidate.Marshal()`。
pub trait SignalExchange: Send + Sync {
    fn send(
        &self,
        to_key: &str,
        kind: PeerSignalKind,
        payload: &str,
        wg_listen_port: u32,
    ) -> Result<(), ManagementError>;
}

/// 测试/对照用 [`SignalExchange`]：发进这里的帧被计数后丢弃（`Logging
/// ConfigApplier` 先例）。**生产不再使用**——N5d 起生产路径是
/// [`RealSignalExchange`]；保留本实现既有的测试继续成立，并作为"静默
/// 丢弃"反例被 [`RealSignalExchange`] 的显式失败语义对照钉住。
#[derive(Debug, Default)]
pub struct LoggingSignalExchange {
    sent: std::sync::atomic::AtomicU64,
}

impl LoggingSignalExchange {
    pub fn sent(&self) -> u64 {
        self.sent.load(std::sync::atomic::Ordering::Acquire)
    }
}

impl SignalExchange for LoggingSignalExchange {
    fn send(
        &self,
        to_key: &str,
        kind: PeerSignalKind,
        _payload: &str,
        _wg_listen_port: u32,
    ) -> Result<(), ManagementError> {
        self.sent.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        crate::hilog::emit(&format!(
            "peer-conn: signal frame dropped (no signal stream) type={} peer={}chars",
            kind.as_str(),
            to_key.len()
        ));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 真实 signal 适配器（N5d）：SignalSession 流 ↔ SignalExchange seam
// ---------------------------------------------------------------------------

/// [`PeerSignalKind`] → 线上 `Body.type`（`signalexchange.proto` L45-52：
/// `OFFER=0 / ANSWER=1 / CANDIDATE=2`）。
fn wire_kind(kind: PeerSignalKind) -> crate::signal::proto::body::Type {
    use crate::signal::proto::body::Type;
    match kind {
        PeerSignalKind::Offer => Type::Offer,
        PeerSignalKind::Answer => Type::Answer,
        PeerSignalKind::Candidate => Type::Candidate,
    }
}

/// 生产 [`SignalExchange`]（N5d）：把编排层的帧经无界队列交给
/// [`spawn_signal_link`] 启动的 signal worker，由
/// [`crate::signal::SignalSession`] 密封（peer=remote_key，
/// grpc.go:434-451）并经 **unary `SignalExchange/Send`** 送达
/// （signal.go:95-104；N10b：0.78.1 起服务端 `ConnectStream` 从不读
/// 请求体，流侧发送会被静默黑洞——见 `docs/interop-run-1-20260913.md`）。
///
/// **未注册 → 显式失败，不静默丢弃**：`send()` 在 link 未注册时返回
/// `Network` 类错误（upstream `ErrSignalIsNotReady` 同型，
/// handshaker.go:16,208,212-214），编排层把帧留在 outbox 下一拍重试
/// （`run_once`/`flush_outbox` 的既有重试语义）。
#[derive(Debug)]
pub struct RealSignalExchange {
    registered: Arc<AtomicBool>,
    tx: tokio::sync::mpsc::UnboundedSender<SignalOutgoing>,
    /// 未注册期间被拒绝的 send 计数（诊断）。
    refused: AtomicU64,
}

impl RealSignalExchange {
    /// 建立适配器与其出帧队列的接收端（接收端交给
    /// [`spawn_signal_link`]）。
    pub fn new() -> (Arc<Self>, tokio::sync::mpsc::UnboundedReceiver<SignalOutgoing>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SignalOutgoing>();
        let exchange = Arc::new(RealSignalExchange {
            registered: Arc::new(AtomicBool::new(false)),
            tx,
            refused: AtomicU64::new(0),
        });
        (exchange, rx)
    }

    /// signal 流是否已注册（worker 任务维护；`send()` 的放行条件）。
    pub fn is_registered(&self) -> bool {
        self.registered.load(Ordering::Acquire)
    }

    /// 强制清注册态（connector stop 的 abort 路径跳过 worker 收尾时补齐）。
    pub fn mark_unregistered(&self) {
        self.registered.store(false, Ordering::Release);
    }

    /// 未注册期间被拒绝的 send 计数（诊断）。
    pub fn refused_sends(&self) -> u64 {
        self.refused.load(Ordering::Acquire)
    }
}

impl SignalExchange for RealSignalExchange {
    fn send(
        &self,
        to_key: &str,
        kind: PeerSignalKind,
        payload: &str,
        wg_listen_port: u32,
    ) -> Result<(), ManagementError> {
        if !self.is_registered() {
            self.refused.fetch_add(1, Ordering::AcqRel);
            return Err(ManagementError::Network(
                "signal-stream-not-registered (frame retained in orchestrator outbox)".into(),
            ));
        }
        self.tx
            .send(SignalOutgoing {
                remote_key: to_key.to_string(),
                kind: wire_kind(kind),
                payload: payload.to_string(),
                wg_listen_port,
            })
            .map_err(|_| {
                ManagementError::Network("signal-link-closed (worker gone)".into())
            })
    }
}

/// [`spawn_signal_link`] 的静态材料：endpoint/传输（TLS 注入 CA）+ 身份
/// 密钥 + 受保护 socket 源与已解析地址（DNS 壳侧解析；协议语义见
/// [`crate::signal::SignalClient::connect_with_socket_source`]）。
pub struct SignalLinkConfig {
    pub endpoint: String,
    pub transport: GrpcTransport,
    pub connect_timeout: core::time::Duration,
    pub request_timeout: core::time::Duration,
    pub keys: EnvelopeKeyPair,
    pub sockets: Arc<dyn ManagementSocketProvider>,
    pub connect_addr: SocketAddr,
}

/// signal worker 事件（connector 侧消费：状态暴露 + 编排路由）。
#[derive(Debug, Clone, PartialEq)]
pub enum SignalLinkEvent {
    /// 初始受保护拨号失败（携带分类错误；按 backoff 重试中——上游
    /// `signal.NewClient` 同样在构造内重试拨号，grpc.go:123-131）。
    DialFailed(ManagementError),
    /// 流注册成功（首次或每次重连；此后 `send()` 放行）。
    Registered,
    /// 解密后的收帧（`from_key` = 发送方 WG 公钥，grpc.go:414-431）。
    Message(SignalMessage),
    /// 帧级解密/解码失败：已上报、流保持（grpc.go:600-602）。
    Malformed,
    /// 流断开（传输类；退避后将重连+重注册）。
    Broken(ManagementError),
    /// worker 结束（fatal Auth / 退避预算耗尽 / 拨号预算耗尽）。
    Ended(Result<(), ManagementError>),
}

/// 启动真实 signal worker（N5d connector 生产路径的唯一启动口）：
///
/// 1. **初始受保护拨号**（重试直至成功/预算耗尽；空 fd 源即失败关闭，
///    绝无未保护回退——每次拨号经 [`crate::mgmtsock::ProtectedSocketConnector`]
///    取新鲜 fd）；
/// 2. [`crate::signal::SignalSession::run_events_with_outbox`] 驱动注册/
///    收帧/发帧/断流重连：每次重连**重新 register**（重注册必然经 tonic
///    连接器**重取受保护 fd**，`crate::signal` 既有语义，本层不绕过）；
/// 3. 事件转发给 `on_event`；`Registered`/`Broken` 同步维护适配器的
///    registered 闸（`send()` 放行条件）。
///
/// 停止：abort 返回的 task（connector stop），或丢弃 exchange（队列
/// `None` → worker 干净退出）。
pub fn spawn_signal_link(
    runtime: tokio::runtime::Handle,
    cfg: SignalLinkConfig,
    exchange: Arc<RealSignalExchange>,
    rx: tokio::sync::mpsc::UnboundedReceiver<SignalOutgoing>,
    on_event: Arc<dyn Fn(SignalLinkEvent) + Send + Sync>,
) -> tokio::task::JoinHandle<()> {
    runtime.spawn(async move {
        // 初始拨号退避：与会话重连同参（上游 defaultBackoff，
        // grpc.go:173-183）；OS 随机源 + 单调时钟（无 sleep 参数断言）。
        let mut dial_backoff = ExponentialBackoff::upstream_stream_default();
        let mut rng = OsRandom;
        let clock = MonotonicClock;
        let client = loop {
            match SignalClient::connect_with_socket_source(
                &cfg.endpoint,
                cfg.transport.clone(),
                cfg.connect_timeout,
                cfg.request_timeout,
                cfg.keys.clone(),
                cfg.sockets.clone(),
                cfg.connect_addr,
            )
            .await
            {
                Ok(c) => break c,
                Err(e) => {
                    on_event(SignalLinkEvent::DialFailed(e));
                    match dial_backoff.next_delay(&clock, &mut rng) {
                        Some(d) => tokio::time::sleep(d).await,
                        None => {
                            on_event(SignalLinkEvent::Ended(Err(ManagementError::Network(
                                "signal dial retry budget exhausted".into(),
                            ))));
                            return;
                        }
                    }
                }
            }
        };
        let registered = exchange.registered.clone();
        let sink = on_event.clone();
        let mut session = SignalSession::new(client);
        let result = session
            .run_events_with_outbox(rx, move |ev| match ev {
                SignalLoopEvent::Registered => {
                    registered.store(true, Ordering::Release);
                    sink(SignalLinkEvent::Registered);
                }
                SignalLoopEvent::Message(m) => sink(SignalLinkEvent::Message(m.clone())),
                SignalLoopEvent::Malformed(_) => sink(SignalLinkEvent::Malformed),
                SignalLoopEvent::Broken(e) => {
                    registered.store(false, Ordering::Release);
                    sink(SignalLinkEvent::Broken(e.clone()));
                }
            })
            .await;
        exchange.registered.store(false, Ordering::Release);
        on_event(SignalLinkEvent::Ended(result));
    })
}

/// OFFER/ANSWER payload（`"ufrag:pwd"`，`client.go:74-101`）解析：恰好一个
/// `:` 分隔、两侧都是合法 ice-char 凭证（RFC 8445 §16 下限在
/// [`IceCredentials::validate`] 里强制）。畸形 → [`ManagementError::Parse`]。
pub fn parse_ufrag_pwd(payload: &str) -> Result<IceCredentials, ManagementError> {
    let bad = || {
        ManagementError::Parse(format!(
            "ice-credential-payload: expected \"ufrag:pwd\", got {} chars",
            payload.len()
        ))
    };
    let Some((ufrag, pwd)) = payload.split_once(':') else { return Err(bad()) };
    let creds = IceCredentials { ufrag: ufrag.to_string(), pwd: pwd.to_string() };
    creds.validate().map_err(|_| bad())?;
    Ok(creds)
}

// ---------------------------------------------------------------------------
// 状态快照（status 暴露面，无密钥材料）
// ---------------------------------------------------------------------------

/// 单 peer 状态快照（测试/诊断/status 汇总的数据源）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerIceStatus {
    /// 远端 peer 的 base64 WG 公钥（公开材料）。
    pub pub_key: String,
    pub state: PeerIceState,
    /// 当前会话角色（None = 会话未建立）。
    pub controlling: Option<bool>,
    /// selected pair 的对端地址（已/拟落配到 WG 层）。
    pub selected_remote: Option<([u8; 4], u16)>,
    /// WG endpoint 落配已成功。
    pub endpoint_applied: bool,
    /// 可用 = Connected 且 endpoint 落配成功（N3-7 默认路由闸的输入）。
    pub reachable: bool,
    /// 最后一次错误的分类（无消息文本，密钥纪律同 connector）。
    pub last_error: Option<ErrorClass>,
}

/// 全体 peer 的 ICE 汇总（`connector_status()` 的 `ice` 字段）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IceOrchestratorSummary {
    pub peers: usize,
    pub idle: usize,
    pub gathering: usize,
    pub checking: usize,
    pub connected: usize,
    pub disconnected: usize,
    pub failed: usize,
    /// endpoint 落配成功数。
    pub endpoints_applied: usize,
    /// 可用 peer 数（Connected + endpoint 已落配）。
    pub reachable: usize,
    // Signal frames dropped because their sender key matched no peer in the
    // local ICE map (peer_conn handle_signal). Silent by design in the log;
    // exposed here because a key-format mismatch makes the whole negotiation
    // stall with NO other symptom (device run 3: peer stayed `idle` forever).
    pub unknown_signal: u64,
    pub last_error: Option<ErrorClass>,
}

impl IceOrchestratorSummary {
    /// JSON 对象（计数 + 分类，无密钥材料）。
    pub fn to_json(&self) -> String {
        format!(
            "{{{},{},{},{},{},{},{},{},{},{},{}}}",
            jnum("peers", self.peers as u64),
            jnum("idle", self.idle as u64),
            jnum("gathering", self.gathering as u64),
            jnum("checking", self.checking as u64),
            jnum("connected", self.connected as u64),
            jnum("disconnected", self.disconnected as u64),
            jnum("failed", self.failed as u64),
            jnum("endpoints_applied", self.endpoints_applied as u64),
            jnum("reachable", self.reachable as u64),
            jnum("unknown_signal", self.unknown_signal),
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

/// N3-7 默认路由安全闸联动（纯函数，单测钉住）：编排里**没有任何可用
/// peer**（全部 Failed/未连通）时，ICE 不得把 WG 数据面判成就绪 ——
/// 默认路由仍不得装。
///
/// `peers == 0`（没有 ICE 编排对象，例如 connector 未拿到网络图）时返回
/// true：不改变 N3-7 原语义（彼时闸只看登记 peer 数与 `tunnel_ready()`）。
pub fn ice_ready_for_default_route(summary: &IceOrchestratorSummary) -> bool {
    summary.peers == 0 || summary.reachable > 0
}

// ---------------------------------------------------------------------------
// per-peer 内部状态
// ---------------------------------------------------------------------------

struct PeerConn {
    key: String,
    state: PeerIceState,
    session: Option<IceSession>,
    /// 本地凭证（会话创建即生成，OFFER/ANSWER 发出）。
    creds: Option<IceCredentials>,
    /// 发起策略（默认 true = 主动发 OFFER，controlling 起始；false = 纯
    /// 应答方，等对端 OFFER）。glare（双方都发起）由 tie-breaker 收敛。
    initiator: bool,
    /// 我们是发起方（已/将发 OFFER，controlling 起始）。
    initiated: bool,
    /// 已绑定进会话的本地候选数（观测 + fail-closed 判断）。
    locals_signaled: usize,
    /// N11：当前会话的远端凭证（新协商检测：不同凭证的 Offer = 对端新会话）。
    remote_creds: Option<IceCredentials>,
    /// 远端候选入列次数（含重复；仅用于 start 前置判断）。
    remote_cands_seen: usize,
    /// 会话已 start（进入 Checking）。
    started: bool,
    /// 本地候选收集+绑定已完成（发起方或应答方）。
    locals_done: bool,
    /// 应答方欠一个 ANSWER（本地候选就绪后随候选一起发）。
    answer_owed: bool,
    /// 发送队列：失败保留、下一拍重试（FIFO：offer → 候选 → answer）。
    outbox: VecDeque<(PeerSignalKind, String)>,
    /// trickle 早于会话到达的远端候选暂存。
    pending_remote: Vec<Candidate>,
    selected_remote: Option<([u8; 4], u16)>,
    endpoint_applied: bool,
    last_error: Option<ErrorClass>,
    cooldown_until: u64,
    /// 已成功送进 signal seam 的帧（offer/候选/answer），供握手重发
    /// （见 [`HANDSHAKE_RETRY_MS`]：对端未注册时服务端按 best-effort
    /// 丢弃转发，重发直到 Connected 是上游 handshaker 的职责）。
    signaled: Vec<(PeerSignalKind, String)>,
    /// 下一次握手重发的时刻（0 = 尚无已发帧）。
    retry_at_ms: u64,
    /// 当前会话的创建时刻（注入时钟；0 = 无会话）。[`START_DEADLINE_MS`]
    /// 的计时起点：创建后始终未 `start()` 的谈判到期拆除重发起。
    session_at_ms: u64,
}

impl PeerConn {
    fn new(key: String) -> Self {
        PeerConn {
            key,
            state: PeerIceState::Idle,
            session: None,
            creds: None,
            initiator: true,
            initiated: false,
            locals_signaled: 0,
            remote_creds: None,
            remote_cands_seen: 0,
            started: false,
            locals_done: false,
            answer_owed: false,
            outbox: VecDeque::new(),
            pending_remote: Vec::new(),
            selected_remote: None,
            endpoint_applied: false,
            last_error: None,
            cooldown_until: 0,
            signaled: Vec::new(),
            retry_at_ms: 0,
            session_at_ms: 0,
        }
    }

    fn status(&self) -> PeerIceStatus {
        let controlling = self.session.as_ref().map(|s| s.is_controlling());
        PeerIceStatus {
            pub_key: self.key.clone(),
            state: self.state,
            controlling,
            selected_remote: self.selected_remote,
            endpoint_applied: self.endpoint_applied,
            reachable: self.state == PeerIceState::Connected && self.endpoint_applied,
            last_error: self.last_error,
        }
    }
}

// ---------------------------------------------------------------------------
// 编排器
// ---------------------------------------------------------------------------

/// 构造依赖（全部注入；生产在 [`crate::connector::ConnectorHandle::spawn`]
/// 组装，测试用桩）。
pub struct PeerIceDeps {
    pub ifaces: Arc<dyn InterfaceSource>,
    pub socks: Arc<dyn UdpSocketSource>,
    pub signal: Arc<dyn SignalExchange>,
    pub wg: Arc<dyn WgPeerApplier>,
    /// tie-breaker 覆盖（测试确定性；生产 None = 每会话抽熵）。
    pub tie_breaker: Option<u64>,
    /// **HOST-ONLY（N12a）**：本地 ICE socket 的固定绑定端口。`None` =
    /// 临时端口（默认，设备路径与既有行为完全一致）。仅主机联调 CLI
    /// （`nbinterop --ice-port`，经配置字段 `ice_fixed_port` 注入）设置；
    /// 语义见 `ensure_locals` 与 `docs/self-hosted-interop-plan.md`
    /// §端口映射 / 对外候选。不新建任何 socket——只改变 seam 所供 socket
    /// 的 bind() 端口。
    pub fixed_local_port: Option<u16>,
    /// **HOST-ONLY（N12a）**：显式对外可达候选（端口映射后的
    /// `外部IP:端口`），作为**额外的 host 型候选**随既有 signal 路径原样
    /// 发给对端（wire 形态 = [`Candidate::marshal`]，优先级比真实 host
    /// 候选低一档 local preference——见
    /// [`Candidate::advertised_host_candidate`]）。默认为空 = 不通告。
    pub advertised_candidates: Vec<Candidate>,
}

/// Per-peer ICE 编排器：`set_peers`（网络图）→ 发起/应答（signal seam）→
/// `run_once`（注入时钟泵检查/keepalive/超时）→ SelectedPair 落配 WG
/// endpoint。所有会话共享同一 [`crate::ice::UdpSocketSource`]（fd 供给方
/// 的 taken 计数因此可整体对账）。
pub struct PeerIceOrchestrator {
    ifaces: Arc<dyn InterfaceSource>,
    socks: Arc<dyn UdpSocketSource>,
    signal: Arc<dyn SignalExchange>,
    wg: Arc<dyn WgPeerApplier>,
    tie_breaker: Option<u64>,
    /// 解析后的 `netbird_config.stuns`（`set_stuns` 维护）。
    stuns: Vec<StunServer>,
    blacklist: Vec<&'static str>,
    peers: Vec<PeerConn>,
    signal_ready: bool,
    /// 非网络图 peer 的 signal 帧计数（观测，不作为错误）。
    unknown_signal: u64,
    /// HOST-ONLY（N12a）：固定绑定端口（None = 临时端口，默认）。
    fixed_local_port: Option<u16>,
    /// HOST-ONLY（N12a）：显式对外候选（默认空 = 不通告）。
    advertised_candidates: Vec<Candidate>,
}

impl PeerIceOrchestrator {
    pub fn new(deps: PeerIceDeps) -> Self {
        PeerIceOrchestrator {
            ifaces: deps.ifaces,
            socks: deps.socks,
            signal: deps.signal,
            wg: deps.wg,
            tie_breaker: deps.tie_breaker,
            stuns: Vec::new(),
            blacklist: DEFAULT_INTERFACE_BLACKLIST.to_vec(),
            peers: Vec::new(),
            signal_ready: false,
            unknown_signal: 0,
            fixed_local_port: deps.fixed_local_port,
            advertised_candidates: deps.advertised_candidates,
        }
    }

    /// `netbird_config.stuns[].uri` → 解析后的 STUN 服务器集
    /// （`engine.go:1525-1541 updateSTUNs` 语义：`stun:` URI；`turn:` 被拒，
    /// 本增量无 relay client）。坏 URI 跳过，返回成功解析的个数。
    pub fn set_stuns(&mut self, uris: &[String]) -> usize {
        self.stuns.clear();
        for uri in uris {
            if let Ok(s) = crate::ice::parse_stun_uri(uri) {
                self.stuns.push(s);
            }
        }
        self.stuns.len()
    }

    /// 与网络图对账：`keys` = 有 allowed_ips 的 remote peer 公钥。新增的
    /// 建立条目（Idle），消失的拆除（Drop 关闭其会话 socket）+ N11 回收
    /// 其 WG endpoint/egress，既有的保持现状（上游对网络图刷新不拆已有
    /// 连接）。
    pub fn set_peers(&mut self, keys: &[String]) {
        let removed: Vec<String> = self
            .peers
            .iter()
            .filter(|p| !keys.contains(&p.key))
            .map(|p| p.key.clone())
            .collect();
        self.peers.retain(|p| keys.contains(&p.key));
        for key in removed {
            self.wg.recycle_endpoint(&key);
        }
        for key in keys {
            if self.peers.iter().any(|p| &p.key == key) {
                continue;
            }
            self.peers.push(PeerConn::new(key.clone()));
        }
    }

    /// signal 流就绪（connector 持有注册好的 signal 流后置位；置位前
    /// `run_once` 不发起任何东西）。
    pub fn set_signal_ready(&mut self, ready: bool) {
        self.signal_ready = ready;
    }

    /// signal 流就绪状态（N5d：link 的 `Registered`/`Broken` 事件维护；
    /// 测试/诊断断言 `signal_ready` 只在注册后为真）。
    pub fn signal_ready(&self) -> bool {
        self.signal_ready
    }

    /// 发起策略（N5c）：`initiator = false` 把该 peer 设为纯应答方（不发
    /// OFFER，只回答对端的 OFFER；角色为 controlled 起始）。默认 true ——
    /// 双方都主动时即 glare，由 N5b tie-breaker 语义收敛。
    pub fn set_initiator(&mut self, key: &str, initiator: bool) {
        if let Some(p) = self.peers.iter_mut().find(|p| p.key == key) {
            p.initiator = initiator;
        }
    }

    pub fn peer_keys(&self) -> Vec<String> {
        self.peers.iter().map(|p| p.key.clone()).collect()
    }

    pub fn peer_status(&self, key: &str) -> Option<PeerIceStatus> {
        self.peers.iter().find(|p| p.key == key).map(|p| p.status())
    }

    pub fn summary(&self) -> IceOrchestratorSummary {
        let mut s = IceOrchestratorSummary {
            peers: self.peers.len(),
            unknown_signal: self.unknown_signal,
            ..Default::default()
        };
        for p in &self.peers {
            match p.state {
                PeerIceState::Idle => s.idle += 1,
                PeerIceState::Gathering => s.gathering += 1,
                PeerIceState::Checking => s.checking += 1,
                PeerIceState::Connected => s.connected += 1,
                PeerIceState::Disconnected => s.disconnected += 1,
                PeerIceState::Failed => s.failed += 1,
            }
            if p.endpoint_applied {
                s.endpoints_applied += 1;
            }
            if p.state == PeerIceState::Connected && p.endpoint_applied {
                s.reachable += 1;
            }
            if s.last_error.is_none() {
                s.last_error = p.last_error;
            }
        }
        s
    }

    /// 收到一帧解密后的 signal 消息（`from_key` = 发送方 WG 公钥，
    /// `engine.go:2039-2042` 的绑定语义；负载已被
    /// [`crate::signal::SignalClient::decrypt_envelope`] 打开）。
    pub fn handle_signal(
        &mut self,
        from_key: &str,
        kind: PeerSignalKind,
        payload: &str,
        now_ms: u64,
    ) -> Result<(), ManagementError> {
        let Some(idx) = self.peers.iter().position(|p| p.key == from_key) else {
            self.unknown_signal += 1; // 非网络图 peer：丢弃，不报错
            return Ok(());
        };
        match kind {
            PeerSignalKind::Offer => {
                let creds = parse_ufrag_pwd(payload)?;
                // N11：本地会话处于 Disconnected（旧路径已死、endpoint 已
                // 回收）时收到的 Offer = 对端重启了协商（上游按新
                // session-id recreate agent 的形态，worker_ice.go:105-130）：
                // 拆除死会话，走全新应答流程（新凭证、新候选、新选择）。
                // 同理，Connected 状态下收到**不同凭证**的 Offer 也是新
                // 协商（凭证是每会话随机生成，同会话的重发必然同值 = 幂等
                // 去重，不会误触发）。Failed 会话已在事件处理中拆掉
                //（session=None），天然落入全新应答路径。
                let renegotiate = self.peers[idx].session.is_some()
                    && (self.peers[idx].state == PeerIceState::Disconnected
                        || (self.peers[idx].state == PeerIceState::Connected
                            && self.peers[idx].remote_creds.as_ref() != Some(&creds)));
                if renegotiate {
                    let peer = &mut self.peers[idx];
                    peer.session = None;
                    peer.creds = None;
                    peer.locals_done = false;
                    peer.started = false;
                    peer.remote_cands_seen = 0;
                    peer.pending_remote.clear();
                    peer.signaled.clear();
                    peer.retry_at_ms = 0;
                    peer.session_at_ms = 0;
                }
                let had_session = self.peers[idx].session.is_some();
                if !had_session {
                    // 应答方：controlled 起始（上游 signal 无角色字段；
                    // 冲突由 N5b tie-breaker 修复）。
                    let local = IceCredentials::generate()?;
                    let mut session = IceSession::new(local.clone(), false, self.tie_breaker)?;
                    session.set_remote_credentials(creds.clone())?;
                    let peer = &mut self.peers[idx];
                    peer.creds = Some(local);
                    peer.remote_creds = Some(creds);
                    peer.session = Some(session);
                    peer.state = PeerIceState::Gathering;
                    peer.answer_owed = true;
                    peer.session_at_ms = now_ms;
                    if let Err(e) = self.ensure_locals(idx, now_ms) {
                        let peer = &mut self.peers[idx];
                        peer.last_error = Some(ErrorClass::from_management(&e));
                        peer.cooldown_until = now_ms + RETRY_COOLDOWN_MS;
                    }
                } else {
                    // glare（双方同时 OFFER）：已有会话，采纳对端凭证、
                    // 保留本端角色，冲突交给检查层的 tie-breaker。
                    let peer = &mut self.peers[idx];
                    peer.session.as_mut().expect("session checked").set_remote_credentials(creds.clone())?;
                    peer.remote_creds = Some(creds);
                }
            }
            PeerSignalKind::Answer => {
                if self.peers[idx].session.is_none() {
                    self.unknown_signal += 1; // 无会话的 ANSWER：丢弃
                    return Ok(());
                }
                let creds = parse_ufrag_pwd(payload)?;
                let peer = &mut self.peers[idx];
                peer.session.as_mut().expect("session checked").set_remote_credentials(creds.clone())?;
                peer.remote_creds = Some(creds);
            }
            PeerSignalKind::Candidate => {
                let cand = Candidate::unmarshal(payload)?;
                let peer = &mut self.peers[idx];
                peer.remote_cands_seen += 1;
                match peer.session.as_mut() {
                    Some(session) => session.add_remote_candidate(cand),
                    None => peer.pending_remote.push(cand), // trickle 早到：暂存
                }
            }
        }
        self.maybe_start(idx);
        Ok(())
    }

    /// 一拍：发起待发 peer、补齐本地候选、刷发送队列（以上仅 signal 就绪
    /// 时——它们是信令活动）；**会话泵、事件处理、WG 入向数据则无条件
    /// 运行**：ICE 会话的收包/keepalive/超时与 signal 流健康无关（上游
    /// pion agent 由独立的 ICE worker 泵送；signal 只承载信令）。把会话
    /// 泵挂在 `signal_ready` 下会让断流侧的会话既聋又冻结——本端不再读
    /// selected socket、对端把我们判 disconnected，而本端状态却永远停在
    /// Connected（真机 12:00 成功连接后 ~7s `disconnected→failed` 的形态
    /// ：对端断流冻结，本端诚实超时）。`now_ms` 注入。
    pub fn run_once(&mut self, now_ms: u64) -> Result<(), ManagementError> {
        let mut first_err: Option<ManagementError> = None;
        for idx in 0..self.peers.len() {
            if self.signal_ready {
                // 发起（仅 signal 就绪后；每 peer 冷却后重试；纯应答方不发起）
                let want_initiate = {
                    let p = &self.peers[idx];
                    p.initiator && p.session.is_none() && !p.initiated && now_ms >= p.cooldown_until
                };
                if want_initiate {
                    self.peers[idx].initiated = true;
                    match self.ensure_locals(idx, now_ms) {
                        Ok(()) => {
                            let peer = &mut self.peers[idx];
                            let creds = peer.creds.as_ref().expect("creds after locals");
                            let payload = format!("{}:{}", creds.ufrag, creds.pwd);
                            peer.outbox.push_back((PeerSignalKind::Offer, payload));
                        }
                        Err(e) => {
                            let peer = &mut self.peers[idx];
                            peer.last_error = Some(ErrorClass::from_management(&e));
                            peer.cooldown_until = now_ms + RETRY_COOLDOWN_MS;
                            peer.initiated = false; // 冷却后重试
                            if first_err.is_none() {
                                first_err = Some(e);
                            }
                        }
                    }
                }
                // 应答方/重试方的本地候选收集（幂等）
                let want_locals = {
                    let p = &self.peers[idx];
                    p.session.is_some() && !p.locals_done && now_ms >= p.cooldown_until
                };
                if want_locals {
                    if let Err(e) = self.ensure_locals(idx, now_ms) {
                        let peer = &mut self.peers[idx];
                        peer.last_error = Some(ErrorClass::from_management(&e));
                        peer.cooldown_until = now_ms + RETRY_COOLDOWN_MS;
                        if first_err.is_none() {
                            first_err = Some(e);
                        }
                    }
                }
                // 握手重发（HANDSHAKE_RETRY_MS 周期，注入时钟）：会话未
                // Connected 前，把已送进 seam 的帧（OFFER/候选/ANSWER）
                // 重新入队——对端未注册时服务端会静默丢弃转发，若不重发
                // 握手就永久卡死（round-6 现场归因）。对端已就绪时这些
                // 重复帧是幂等的（凭证采纳为 no-op、候选按 (addr,port)
                // 去重）。
                let needs_retry = {
                    let p = &self.peers[idx];
                    !p.signaled.is_empty()
                        && p.state != PeerIceState::Connected
                        && p.retry_at_ms != 0
                        && now_ms >= p.retry_at_ms
                };
                if needs_retry {
                    let p = &mut self.peers[idx];
                    p.retry_at_ms = now_ms + HANDSHAKE_RETRY_MS;
                    for frame in p.signaled.clone() {
                        p.outbox.push_back(frame);
                    }
                }
                if let Err(e) = self.flush_outbox(idx) {
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
                {
                    let peer = &mut self.peers[idx];
                    if peer.retry_at_ms == 0 && !peer.signaled.is_empty() {
                        peer.retry_at_ms = now_ms + HANDSHAKE_RETRY_MS;
                    }
                }
            }
            // 僵尸谈判拆除（H3）：会话已建但始终未 start（对端持旧会话、
            // 不再应答）——到期拆除，冷却后重新发起。signal 无关的本地
            // 策略，不受断流影响。
            self.expire_dead_negotiation(idx, now_ms);
            // 会话泵（无条件）：收包/检查/keepalive/超时不依赖 signal 流。
            {
                let peer = &mut self.peers[idx];
                if let Some(session) = peer.session.as_mut() {
                    if let Err(e) = session.run_once(now_ms) {
                        peer.last_error = Some(ErrorClass::from_management(&e));
                        if first_err.is_none() {
                            first_err = Some(e);
                        }
                    }
                }
            }
            self.drain_events(idx, now_ms);
            // N11: WG 数据面入向 —— ICE 会话分用出的非 STUN（WG）包喂给
            // 设备（上游：共享接收循环把非 STUN 包交给 WG，
            // ice_bind.go:279-303）。设备来源匹配 peer endpoint 的既有
            // 规则不变；会话已拆（Failed 回收）则数据随之消亡（路径已死）。
            // 无条件执行：入向数据面同样不得随 signal 流断而停。
            let datagrams = match self.peers[idx].session.as_mut() {
                Some(s) => s.take_data_rx(),
                None => Vec::new(),
            };
            for (src, src_port, datagram) in datagrams {
                let _sent = self.wg.handle_udp_inbound(&datagram, (src, src_port), now_ms);
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// 本地候选收集 + 会话绑定（发起方与应答方共用；幂等，`locals_done`
    /// 防重）。部分失败（个别接口/STUN server）不阻塞：只要有 host 候选
    /// 绑定成功就继续；一个候选都没有 → 硬错误（不发没有候选支撑的
    /// OFFER/ANSWER，fail-closed）。成功后候选入 outbox（trickle），应答方
    /// 再补 ANSWER。
    ///
    /// 两条收集路径：
    /// - **默认**（`fixed_local_port = None`，设备与既有主机行为）：
    ///   [`crate::ice::gather_candidates`]——每接口一枚受保护 socket、
    ///   临时端口，候选端口交给内核重分（gather 的 socket 已关）；
    /// - **固定端口**（HOST-ONLY，N12a）：不做 per-interface 收集也不做
    ///   STUN（端口映射场景里显式对外候选取代 srflx），取**首个允许
    ///   接口地址**一枚候选、以 [`IceSession::add_local_candidate_fixed_port`]
    ///   通配绑定 `0.0.0.0:<port>`（转发目的地址不可预知，通配绑定才使
    ///   固定端口可达；上游同形：单一 `0.0.0.0:NET_PORT` 共享 socket），
    ///   候选保留接口地址 + 固定端口（getsockname 回填校验）。
    ///   之后 [`advertised_candidates`] 逐个以既有 wire 形态追加进 outbox
    ///   （extra host 候选，随握手重发语义一起重发）。
    fn ensure_locals(&mut self, idx: usize, now_ms: u64) -> Result<(), ManagementError> {
        if self.peers[idx].locals_done {
            return Ok(());
        }
        if self.peers[idx].session.is_none() {
            let creds = IceCredentials::generate()?;
            let controlling = self.peers[idx].initiated;
            let session = IceSession::new(creds.clone(), controlling, self.tie_breaker)?;
            self.peers[idx].creds = Some(creds);
            self.peers[idx].session = Some(session);
            self.peers[idx].session_at_ms = now_ms;
        }
        let fixed_port = self.fixed_local_port;
        let (host_list, srflx_list) = match fixed_port {
            Some(port) => {
                let addrs = self.allowed_iface_addrs()?;
                (addrs.first().map(|a| Candidate::host_candidate(*a, port)).into_iter().collect::<Vec<_>>(), Vec::new())
            }
            None => {
                let gathered = {
                    // 共享借用收敛在块内：gather 用 seam/配置（字段级
                    // disjoint），结果 owned，块后 peers[idx] 可再独占借用。
                    let cfg = GatherConfig {
                        blacklist: &self.blacklist,
                        servers: &self.stuns,
                        timeout_ms: DEFAULT_STUN_TIMEOUT_MS,
                    };
                    crate::ice::gather_candidates(&cfg, self.ifaces.as_ref(), self.socks.as_ref())?
                };
                (gathered.host, gathered.srflx)
            }
        };
        let added = {
            let peer = &mut self.peers[idx];
            let session = peer.session.as_mut().expect("session just ensured");
            let mut added = 0usize;
            for cand in &host_list {
                let addr = parse_ipv4(&cand.address)?;
                let fresh = match fixed_port {
                    // 固定端口：候选保留指定端口（绑定失败 = 端口被占，
                    // fail-closed 硬错误——绝不静默回落临时端口）。
                    Some(_) => Candidate::host_candidate(addr, cand.port),
                    // 默认：端口交给内核重分（gather 的 socket 已关，避免
                    // 端口复用竞态）；add_local_candidate 返回 FINAL 候选
                    // （getsockname 回填后的端口），直接以其线格式入 outbox
                    //（signaler.go:32-41 形态）。
                    None => Candidate::host_candidate(addr, 0),
                };
                let bound = if fixed_port.is_some() {
                    session.add_local_candidate_fixed_port(fresh, self.socks.as_ref())?
                } else {
                    session.add_local_candidate(fresh, self.socks.as_ref())?
                };
                peer.outbox.push_back((PeerSignalKind::Candidate, bound.marshal()));
                peer.locals_signaled += 1;
                added += 1;
            }
            // srflx：N5a 的收集 socket 是一次性的（模块文档「未做项」），候选
            // 仍按上游 signaler 语义 trickle 出去；NAT 拓扑下其 pair 可能
            // 不通，host pair 才是本设计的承载路径。
            for cand in &srflx_list {
                peer.outbox.push_back((PeerSignalKind::Candidate, cand.marshal()));
            }
            // HOST-ONLY（N12a）：显式对外候选——额外的 host 型候选，wire
            // 形态与本地 host 候选一致（[`Candidate::advertised_host_candidate`]，
            // 优先级低一档）；仅入 signal outbox，不注册为本会话本地候选
            // （对端检查到达时按 (本地socket × 远端候选) 配对，无需别名）。
            for cand in &self.advertised_candidates {
                peer.outbox.push_back((PeerSignalKind::Candidate, cand.marshal()));
            }
            // Device diagnostics (run 3 was blind here): how many local
            // candidates the gather actually bound decides whether ICE can
            // ever pair — a zero/low count means the socket pool or the
            // interface enumeration starved, not the peer.
            crate::hilog::emit(&format!(
                "N5c_ICE|locals-gathered|added={added}|srflx={}|advertised={}",
                srflx_list.len(),
                self.advertised_candidates.len()
            ));
            added
        };
        if added == 0 {
            return Err(ManagementError::Network("ice:no-bindable-local-candidate".into()));
        }
        let peer = &mut self.peers[idx];
        peer.locals_done = true;
        if peer.answer_owed {
            let creds = peer.creds.as_ref().expect("creds").clone();
            peer.outbox.push_back((PeerSignalKind::Answer, format!("{}:{}", creds.ufrag, creds.pwd)));
            peer.answer_owed = false;
        }
        self.maybe_start(idx);
        Ok(())
    }

    /// 允许接口的 IPv4 地址列表（固定端口路径专用）：与 gather 相同的
    /// 枚举 + blacklist 前缀过滤 + 去重，但**不消耗任何 socket**——固定
    /// 端口路径整个收集阶段只消耗一枚 socket（首个候选的通配绑定）。
    fn allowed_iface_addrs(&self) -> Result<Vec<[u8; 4]>, ManagementError> {
        let all = self.ifaces.list()?;
        let mut out: Vec<[u8; 4]> = Vec::new();
        for iface in all {
            if !crate::ice::interface_allowed(&iface.name, &self.blacklist) {
                continue;
            }
            if !out.contains(&iface.addr) {
                out.push(iface.addr);
            }
        }
        Ok(out)
    }

    /// outbox FIFO 刷给 signal seam；失败即停（剩余帧下一拍重试）。
    /// 成功送出的帧记入 `signaled`（握手重发的母本，去重）。
    fn flush_outbox(&mut self, idx: usize) -> Result<(), ManagementError> {
        loop {
            let (kind, payload, key) = {
                let peer = &mut self.peers[idx];
                match peer.outbox.front() {
                    Some((k, p)) => (*k, p.clone(), peer.key.clone()),
                    None => return Ok(()),
                }
            };
            self.signal.send(&key, kind, &payload, 0)?;
            {
                let peer = &mut self.peers[idx];
                peer.outbox.pop_front();
                if !peer.signaled.contains(&(kind, payload.clone())) {
                    peer.signaled.push((kind, payload));
                }
            }
        }
    }

    /// 会话启动前置：远端凭证 + 本地候选 + 至少一个远端候选。
    fn maybe_start(&mut self, idx: usize) {
        if self.peers[idx].started || self.peers[idx].session.is_none() {
            return;
        }
        if self.peers[idx].remote_creds.is_none()
            || !self.peers[idx].locals_done
            || self.peers[idx].remote_cands_seen == 0
        {
            return;
        }
        {
            let peer = &mut self.peers[idx];
            if !peer.pending_remote.is_empty() {
                let session = peer.session.as_mut().expect("session checked");
                for cand in peer.pending_remote.drain(..) {
                    session.add_remote_candidate(cand);
                }
            }
            if let Some(session) = peer.session.as_mut() {
                if session.start().is_ok() {
                    peer.started = true;
                }
            }
        }
        if self.peers[idx].started
            && (self.peers[idx].state == PeerIceState::Idle
                || self.peers[idx].state == PeerIceState::Gathering)
        {
            self.peers[idx].state = PeerIceState::Checking;
        }
    }

    fn drain_events(&mut self, idx: usize, now_ms: u64) {
        let events = match self.peers[idx].session.as_mut() {
            Some(s) => s.take_events(),
            None => return,
        };
        for ev in events {
            match ev {
                IceEvent::SelectedPair { remote, .. } => {
                    let peer = &mut self.peers[idx];
                    if let Ok(addr) = parse_ipv4(&remote.address) {
                        peer.selected_remote = Some((addr, remote.port));
                        // N11：WG 骑选中连接 —— 先把选中 pair 的本地 socket
                        // dup 进设备做 egress，再落配 endpoint（落配即触发
                        // 握手，握手必须从选中路径发出）。attach 失败 =
                        // 路径不可骑：不落配、记错误、不给可用信号
                        // （fail-closed，绝不回退到旧 socket 发握手——
                        // round-7 的 bug 形态被结构性堵死）。
                        let egress_fd =
                            peer.session.as_ref().and_then(|s| s.selected_local_fd());
                        let attached = match egress_fd {
                            Some(fd) => self.wg.attach_egress_socket(&peer.key, fd),
                            None => Err("ice:no-selected-local-socket".to_string()),
                        };
                        let landed = match attached {
                            Ok(()) => self.wg.apply_endpoint(&peer.key, addr, remote.port),
                            Err(msg) => Err(msg),
                        };
                        match landed {
                            Ok(()) => {
                                peer.endpoint_applied = true;
                                // selected-pair evidence (public runtime
                                // material: ip + port only)
                                let local = peer
                                    .session
                                    .as_ref()
                                    .and_then(|s| s.selected_pair())
                                    .map(|(l, _)| l);
                                crate::hilog::emit(&format!(
                                    "N5_ICE|selected-pair|{}:{}",
                                    std::net::Ipv4Addr::from(addr),
                                    remote.port
                                ));
                                if let Some(l) = local {
                                    let laddr = parse_ipv4(&l.address).unwrap_or([0, 0, 0, 0]);
                                    crate::hilog::emit(&format!(
                                        "N11_ICE|wg-egress|local={}:{}",
                                        std::net::Ipv4Addr::from(laddr),
                                        l.port
                                    ));
                                }
                            }
                            Err(msg) => {
                                peer.endpoint_applied = false;
                                let e = ManagementError::Network(format!("wg-endpoint: {msg}"));
                                peer.last_error = Some(ErrorClass::from_management(&e));
                                crate::hilog::emit(
                                    "peer-conn: wg endpoint apply failed (class=network)",
                                );
                            }
                        }
                    } else {
                        peer.endpoint_applied = false;
                    }
                    // Connected 只表示 ICE 选中；reachable 还要求落配成功
                    //（summary()/status() 派生）。
                    peer.state = PeerIceState::Connected;
                }
                IceEvent::Disconnected => {
                    // N11：ICE 断开 —— 立即回收 endpoint + egress（上游
                    // `RemoveEndpointAddress`，conn.go:531）：设备从此无路径
                    // 可发（fail-closed），绝不静默沿用旧路径；恢复连接须经
                    // 新一轮协商（重选 pair 后重新 attach + 落配）。
                    let peer = &mut self.peers[idx];
                    peer.state = PeerIceState::Disconnected;
                    peer.endpoint_applied = false;
                    self.wg.recycle_endpoint(&peer.key);
                    crate::hilog::emit("N11_ICE|disconnected|endpoint-recycled");
                }
                IceEvent::Failed(reason) => {
                    // 无 relay 的本增量：Failed → peer 不可达（绝不静默
                    // 当作可用；默认路由闸经 summary().reachable 生效）。
                    // N11：回收 WG 端点与 egress，并拆除死会话（关闭其
                    // socket dup = 死路径 fail-closed）+ 冷却后重新发起
                    // （上游：agent Failed → closeAgent → handshaker 重启
                    // 协商，worker_ice.go:584-593 / conn.go:495-534）。
                    self.drop_session_rearm(idx, now_ms, &reason);
                }
                IceEvent::Closed => {}
                IceEvent::CheckSucceeded { .. } => {}
                // 本地候选的最终形态已在 ensure_locals 里直接入 outbox。
                IceEvent::LocalCandidateReady(_) => {}
            }
        }
    }

    /// 拆除当前会话并重新武装谈判（`IceEvent::Failed` 与僵尸谈判超时
    /// [`Self::expire_dead_negotiation`] 共用）：peer 不可达、回收 WG
    /// 端点与 egress、Drop 关闭会话全部 dup socket、清空在途信令状态、
    /// 冷却后允许重新发起。`reason` 只进日志（公开诊断材料）。
    fn drop_session_rearm(&mut self, idx: usize, now_ms: u64, reason: &str) {
        let peer = &mut self.peers[idx];
        peer.state = PeerIceState::Failed;
        peer.endpoint_applied = false;
        peer.selected_remote = None;
        self.wg.recycle_endpoint(&peer.key);
        peer.session = None; // Drop 关闭该会话全部 dup socket
        peer.creds = None;
        peer.remote_creds = None;
        peer.locals_done = false;
        peer.started = false;
        peer.remote_cands_seen = 0;
        peer.pending_remote.clear();
        peer.signaled.clear();
        peer.outbox.clear();
        peer.retry_at_ms = 0;
        peer.initiated = false;
        peer.answer_owed = false; // 死会话不欠 ANSWER：重发起走 Offer 路径
        peer.session_at_ms = 0;
        peer.cooldown_until = now_ms + RETRY_COOLDOWN_MS;
        // The payload carries WHY the session died — device run 3
        // needed exactly this and the marker alone said nothing.
        crate::hilog::emit(&format!(
            "N11_ICE|failed|session-dropped|renegotiation-armed|reason={reason}"
        ));
    }

    /// 僵尸谈判拆除（H3 修复，[`START_DEADLINE_MS`]）：会话已建、本地
    /// 候选已发，但远端凭证/候选始终没有凑齐（`started == false`）——
    /// 典型：对端仍持有旧会话、自认 Connected、不再发 Offer/Answer。
    /// 不拆除则本端永久沉默（want_initiate 要 `session.is_none()`，
    /// want_locals 要 `!locals_done`，无事件可拆会话）。到期即拆除重
    /// 武装：冷却后以全新凭证重新发起，对端（其旧会话仍活着）会把新
    /// 凭证的 Offer 采纳为新谈判并应答。已 start 的会话不适用——其
    /// 死亡由 keepalive/断连/全对失败事件诚实驱动。
    fn expire_dead_negotiation(&mut self, idx: usize, now_ms: u64) {
        let zombie = {
            let p = &self.peers[idx];
            p.session.is_some()
                && !p.started
                && p.session_at_ms != 0
                && now_ms.saturating_sub(p.session_at_ms) >= START_DEADLINE_MS
                // Only a negotiation the remote NEVER answered is a zombie.
                // Device run 5: a HEALTHY negotiation legitimately needs
                // ~15-30 s (gather → signal → checks → nomination), so
                // expiring on the deadline alone tore down sessions that were
                // still converging (and, in the host's fixed-port mode, wedged
                // the port). Any progress — remote credentials or a single
                // remote candidate — hands the session over to its own honest
                // failure paths (all-pairs-failed / disconnected / keepalive).
                && p.remote_creds.is_none()
                && p.remote_cands_seen == 0
        };
        if zombie {
            self.drop_session_rearm(idx, now_ms, "negotiation-start-timeout");
        }
    }

    /// 停止全部会话（关闭各自 dup socket）；connector stop() 调用。
    pub fn stop_all(&mut self) {
        for p in self.peers.iter_mut() {
            if let Some(s) = p.session.as_mut() {
                s.stop();
            }
            p.outbox.clear();
        }
    }

    /// 非网络图 peer 的 signal 帧计数（观测）。
    pub fn unknown_signal(&self) -> u64 {
        self.unknown_signal
    }

    /// 单测/诊断：STUN 服务器集。
    pub fn stuns(&self) -> &[StunServer] {
        &self.stuns
    }
}

/// 严格 dotted-quad 解析（与 `ice_session.rs` 同规则；loopback 与生产接口
/// 地址都是字面量）。
fn parse_ipv4(s: &str) -> Result<[u8; 4], ManagementError> {
    let octets: Vec<&str> = s.trim().split('.').collect();
    if octets.len() != 4 {
        return Err(ManagementError::Parse(format!("ice:address-not-ipv4 '{s}'")));
    }
    let mut addr = [0u8; 4];
    for (i, o) in octets.iter().enumerate() {
        if o.is_empty() || o.len() > 3 || !o.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ManagementError::Parse(format!("ice:address-not-ipv4 '{s}'")));
        }
        addr[i] =
            o.parse().map_err(|_| ManagementError::Parse(format!("ice:address-not-ipv4 '{s}'")))?;
    }
    Ok(addr)
}

// ---------------------------------------------------------------------------
// 单元测试（纯逻辑；loopback 集成在 tests/peer_conn_e2e.rs）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::WgPeerEntry;
    use crate::sys;
    use std::sync::Mutex;

    fn noop_wg() -> Arc<dyn WgPeerApplier> {
        Arc::new(NullWg)
    }
    struct NullWg;
    impl WgPeerApplier for NullWg {
        fn apply_peers(&self, _peers: &[WgPeerEntry]) -> Result<(), String> {
            Ok(())
        }
        fn clear(&self) {}
    }

    #[test]
    fn parse_ufrag_pwd_roundtrip_and_rejects() {
        let creds = parse_ufrag_pwd("abcd:0123456789012345678901").expect("minimal legal");
        assert_eq!(creds.ufrag, "abcd");
        assert_eq!(creds.pwd, "0123456789012345678901");
        // 上游 client.go:74-101 形态（生成凭证长度）往返
        let gen = IceCredentials::generate().expect("entropy");
        let wire = format!("{}:{}", gen.ufrag, gen.pwd);
        let back = parse_ufrag_pwd(&wire).expect("generated roundtrip");
        assert_eq!(back, gen);
        // 畸形：无冒号 / 坏字符 / 过短
        assert!(parse_ufrag_pwd("nocolon").is_err());
        assert!(parse_ufrag_pwd("abc:0123456789012345678901").is_err(), "ufrag<4");
        assert!(parse_ufrag_pwd("abcd:short").is_err(), "pwd<22");
        assert!(parse_ufrag_pwd("abcd:012345678901234567890:").is_err(), "bad char");
    }

    #[test]
    fn ice_state_names_and_summary_json_contract() {
        assert_eq!(PeerIceState::Idle.as_str(), "idle");
        assert_eq!(PeerIceState::Gathering.as_str(), "gathering");
        assert_eq!(PeerIceState::Checking.as_str(), "checking");
        assert_eq!(PeerIceState::Connected.as_str(), "connected");
        assert_eq!(PeerIceState::Disconnected.as_str(), "disconnected");
        assert_eq!(PeerIceState::Failed.as_str(), "failed");
        assert_eq!(PeerSignalKind::Offer.as_str(), "offer");
        assert_eq!(PeerSignalKind::Answer.as_str(), "answer");
        assert_eq!(PeerSignalKind::Candidate.as_str(), "candidate");
        let s = IceOrchestratorSummary {
            peers: 3,
            connected: 1,
            failed: 1,
            endpoints_applied: 1,
            reachable: 1,
            last_error: Some(ErrorClass::Network),
            ..Default::default()
        };
        let json = s.to_json();
        for frag in [
            "\"peers\":3",
            "\"idle\":0",
            "\"connected\":1",
            "\"failed\":1",
            "\"endpoints_applied\":1",
            "\"reachable\":1",
            "\"last_error\":{\"class\":\"network\",\"status\":0}",
        ] {
            assert!(json.contains(frag), "missing {frag} in {json}");
        }
        assert!(crate::config::parse_document(&json).is_ok(), "strict reader must accept {json}");
    }

    #[test]
    fn default_route_gate_linkage_holds_without_reachable_peer() {
        // 有登记 peer、ICE 全灭（无 reachable）→ 数据面不得就绪
        let dead = IceOrchestratorSummary { peers: 2, failed: 2, ..Default::default() };
        assert!(!ice_ready_for_default_route(&dead));
        // 无 ICE 编排（peers==0）→ 不改变 N3-7 原语义
        assert!(ice_ready_for_default_route(&IceOrchestratorSummary::default()));
        // 至少一个 reachable → 就绪
        let ok = IceOrchestratorSummary { peers: 2, connected: 1, reachable: 1, ..Default::default() };
        assert!(ice_ready_for_default_route(&ok));

        // 与既有 N3-7 闸的组合行为：reachable==0 时默认路由仍被 HOLD
        let tunnel_ready = true && ice_ready_for_default_route(&dead);
        let (allowed, reason) =
            crate::connector::ShellNetworkConfig::default_route_decision(2, tunnel_ready, false);
        assert!(!allowed);
        assert_eq!(reason, "default-route-held:data-plane-not-ready");
    }

    #[test]
    fn handshake_is_re_signaled_periodically_until_connected() {
        // round-6 归因的回归钉子：OFFER/候选在信号 seam 交付成功后，若对端
        // 一直没有回音（典型：对端尚未注册，服务端 best-effort 丢弃转发，
        // 无任何错误面），编排层必须按 HANDSHAKE_RETRY_MS 周期重发已发帧，
        // 直到该 peer Connected；重发不得无限增长 signaled 母本。
        #[derive(Default)]
        struct RecordingSignal {
            frames: Mutex<Vec<(String, PeerSignalKind, String)>>,
        }
        impl SignalExchange for RecordingSignal {
            fn send(
                &self,
                to_key: &str,
                kind: PeerSignalKind,
                payload: &str,
                _port: u32,
            ) -> Result<(), ManagementError> {
                self.frames
                    .lock()
                    .expect("frames")
                    .push((to_key.to_string(), kind, payload.to_string()));
                Ok(())
            }
        }

        // 真实可绑定的本地候选（host 测试：原始 UDP socket 喂给源）
        extern "C" {
            fn socket(domain: i32, ty: i32, protocol: i32) -> i32;
        }
        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1));
        for _ in 0..4 {
            let fd = unsafe { socket(2, 2, 0) };
            assert!(fd >= 0, "socket() failed");
            socks.feed(fd);
        }

        let signal = Arc::new(RecordingSignal::default());
        let mut orch = PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![crate::ice::InterfaceAddr {
                name: "eth0".into(),
                addr: [127, 0, 0, 1],
            }])),
            socks,
            signal: signal.clone(),
            wg: noop_wg(),
            tie_breaker: Some(7),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        });
        orch.set_peers(&["P".into()]);
        orch.set_signal_ready(true);

        let sent = || signal.frames.lock().expect("frames").len();
        // 拍 1（t=1000）：发起 → gather → OFFER + 候选交付 seam
        orch.run_once(1000).expect("first beat converges the send");
        let first = sent();
        assert!(first >= 2, "offer + candidates must be delivered, got {first}");
        // 首发后重发定时器已武装：冷却窗口内（<1000+3000）无重发
        orch.run_once(2000).expect("beat 2");
        orch.run_once(3999).expect("beat 3");
        assert_eq!(sent(), first, "no re-signal before HANDSHAKE_RETRY_MS");
        // 到点：重发同一批帧（offer + 候选），且母本不增长（去重）
        orch.run_once(4000).expect("beat 4 fires the retry");
        assert!(sent() >= first * 2, "re-signal must re-deliver the frames");
        {
            let p = &orch.peers[0];
            assert_eq!(
                p.signaled.len(),
                first,
                "signaled store stays deduplicated ({} vs {})",
                p.signaled.len(),
                first
            );
        }
        // 继续周期重发（t=7000 再到点）
        orch.run_once(6999).expect("beat 5");
        assert_eq!(sent(), first * 2, "still inside the retry window");
        orch.run_once(7000).expect("beat 6 fires the second retry");
        assert!(sent() >= first * 3, "periodic re-signal continues until Connected");
    }

    #[test]
    fn set_peers_reconciles_add_remove_and_counts_unknown_signal() {
        let mut orch = PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![])),
            socks: Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1)),
            signal: Arc::new(LoggingSignalExchange::default()),
            wg: noop_wg(),
            tie_breaker: Some(7),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        });
        orch.set_peers(&["P1".into(), "P2".into()]);
        assert_eq!(orch.peer_keys(), vec!["P1".to_string(), "P2".to_string()]);
        assert_eq!(orch.summary().peers, 2);
        assert_eq!(orch.summary().idle, 2);
        // 收缩 + 新增
        orch.set_peers(&["P2".into(), "P3".into()]);
        assert_eq!(orch.peer_keys(), vec!["P2".to_string(), "P3".to_string()]);
        // 未知 peer 的 signal 帧被丢弃且计数
        orch.handle_signal("GHOST", PeerSignalKind::Offer, "abcd:0123456789012345678901", 10)
            .expect("unknown peer frames are dropped, not errors");
        assert_eq!(orch.unknown_signal(), 1);
        assert_eq!(orch.peer_status("GHOST"), None);
    }

    #[test]
    fn empty_socket_source_fails_closed_without_signaling() {
        // 空 provider：发起在候选收集处 fail-closed，绝不发出任何帧
        #[derive(Default)]
        struct CountingSignal {
            frames: Mutex<Vec<(String, PeerSignalKind, String)>>,
        }
        impl SignalExchange for CountingSignal {
            fn send(
                &self,
                to_key: &str,
                kind: PeerSignalKind,
                payload: &str,
                _port: u32,
            ) -> Result<(), ManagementError> {
                self.frames
                    .lock()
                    .expect("frames")
                    .push((to_key.to_string(), kind, payload.to_string()));
                Ok(())
            }
        }
        let signal = Arc::new(CountingSignal::default());
        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1));
        let mut orch = PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![crate::ice::InterfaceAddr {
                name: "eth0".into(),
                addr: [127, 0, 0, 1],
            }])),
            socks: socks.clone(),
            signal: signal.clone(),
            wg: noop_wg(),
            tie_breaker: Some(7),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        });
        orch.set_peers(&["P".into()]);
        orch.set_signal_ready(true);
        let mut now = 1000u64;
        // 第一拍：发起在候选收集处 fail-closed（Network 错误上抛）
        assert!(orch.run_once(now).is_err(), "empty provider must surface a Network error");
        // 冷却窗口内的后续拍不重试、不再报错（退避语义）
        for _ in 0..4 {
            now += 100;
            assert!(orch.run_once(now).is_ok(), "cooldown window must suppress the retry");
        }
        // 冷却结束：重试再次 fail-closed
        now += RETRY_COOLDOWN_MS;
        assert!(orch.run_once(now).is_err(), "retry after cooldown must fail closed again");
        assert!(
            signal.frames.lock().expect("frames").is_empty(),
            "no frame may leave without candidates"
        );
        assert_eq!(socks.taken(), 0, "empty provider must hand out nothing");
        let st = orch.peer_status("P").expect("peer");
        assert_eq!(st.state, PeerIceState::Idle, "never leaves Idle without candidates");
        assert_eq!(st.last_error, Some(ErrorClass::Network), "fail-closed error recorded");
        assert!(!ice_ready_for_default_route(&orch.summary()), "unreachable peer must not arm the gate");
    }

    #[test]
    fn set_stuns_parses_netbird_config_uris() {
        let mut orch = PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![])),
            socks: Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1)),
            signal: Arc::new(LoggingSignalExchange::default()),
            wg: noop_wg(),
            tie_breaker: None,
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        });
        let n = orch.set_stuns(&[
            "stun:stun.netbird.io:3478".to_string(),
            "stun:stun.l.google.com:19302".to_string(),
            "turn:turn.example.com:3478".to_string(), // 拒绝（无 relay client）
            "garbage".to_string(),                     // 拒绝（无 scheme）
        ]);
        assert_eq!(n, 2);
        assert_eq!(orch.stuns().len(), 2);
        assert_eq!(orch.stuns()[0].host, "stun.netbird.io");
        assert_eq!(orch.stuns()[0].port, 3478);
        assert_eq!(orch.stuns()[1].port, 19302);
    }

    #[test]
    fn candidate_before_session_is_buffered_until_start_prerequisites() {
        // trickle 早到（无会话）：候选暂存不丢、不 panic、不 start。
        let mut orch = PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![])),
            socks: Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1)),
            signal: Arc::new(LoggingSignalExchange::default()),
            wg: noop_wg(),
            tie_breaker: Some(7),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        });
        orch.set_peers(&["P".into()]);
        let cand = Candidate::host_candidate([127, 0, 0, 1], 51820);
        orch.handle_signal("P", PeerSignalKind::Candidate, &cand.marshal(), 10).expect("buffered");
        assert_eq!(orch.peer_status("P").expect("peer").state, PeerIceState::Idle);
        // 无会话的 ANSWER 同样丢弃计数
        orch.handle_signal("P", PeerSignalKind::Answer, "abcd:0123456789012345678901", 20)
            .expect("dropped");
        assert_eq!(orch.unknown_signal(), 1);
    }

    /// 一枚空闲 UDP 端口（绑定→读 getsockname→关闭；测试端口极少竞争，
    /// 与既有 e2e 的临时端口获取同一量级的 TOCTOU 容忍）。
    fn grab_free_udp_port() -> u16 {
        let fd = crate::host_sockets::open_udp_ephemeral().expect("probe socket");
        let port = crate::host_sockets::udp_bound_port(fd).expect("probe port");
        unsafe { sys::close(fd) };
        assert!(port > 0);
        port
    }

    /// N12a（HOST-ONLY）固定端口 + 显式对外候选的信号形态：
    /// - 固定端口路径消耗**恰好一枚** socket，通配绑定 0.0.0.0:P——候选
    ///   端口经 getsockname 回填校验后等于指定端口（接口地址可以是本机
    ///   不存在的 10.99.0.7：固定路径绝不按接口地址绑定）；
    /// - 对外候选以**额外 host 型候选**入 signal（wire 形态与
    ///   `Candidate::marshal` 一致，优先级比真实 host 低一档 = −256）。
    #[test]
    fn fixed_port_binds_specified_port_and_advertised_candidate_is_signaled() {
        #[derive(Default)]
        struct RecordingSignal {
            frames: Mutex<Vec<(String, PeerSignalKind, String)>>,
        }
        impl SignalExchange for RecordingSignal {
            fn send(
                &self,
                to_key: &str,
                kind: PeerSignalKind,
                payload: &str,
                _port: u32,
            ) -> Result<(), ManagementError> {
                self.frames
                    .lock()
                    .expect("frames")
                    .push((to_key.to_string(), kind, payload.to_string()));
                Ok(())
            }
        }

        let port = grab_free_udp_port();
        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1));
        for _ in 0..2 {
            let fd = unsafe { sys::socket(2, 2, 0) };
            assert!(fd >= 0, "socket() failed");
            socks.feed(fd);
        }
        let signal = Arc::new(RecordingSignal::default());
        let mut orch = PeerIceOrchestrator::new(PeerIceDeps {
            // 不可绑定（本机不存在）的接口地址：固定路径只**枚举**地址，
            // 绝不按它绑定——通配绑定由 add_local_candidate_fixed_port 做。
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![crate::ice::InterfaceAddr {
                name: "eth0".into(),
                addr: [10, 99, 0, 7],
            }])),
            socks: socks.clone(),
            signal: signal.clone(),
            wg: noop_wg(),
            tie_breaker: Some(7),
            fixed_local_port: Some(port),
            advertised_candidates: vec![Candidate::advertised_host_candidate([127, 0, 0, 1], port)],
        });
        orch.set_peers(&["P".into()]);
        orch.set_signal_ready(true);

        orch.run_once(1000).expect("fixed-port locals must converge");
        let frames = signal.frames.lock().expect("frames").clone();
        let cands: Vec<Candidate> = frames
            .iter()
            .filter(|(_, k, _)| *k == PeerSignalKind::Candidate)
            .map(|(_, _, p)| Candidate::unmarshal(p).expect("wire form"))
            .collect();
        assert_eq!(cands.len(), 2, "one real host candidate + one advertised: {frames:?}");
        // ① 真实 host 候选：接口地址 + 固定端口。端口 == 指定值即
        //    getsockname 校验通过的证据（socket 确实绑到该端口）。
        let host = cands.iter().find(|c| c.address == "10.99.0.7").expect("host candidate");
        assert_eq!(host.port, port, "fixed port must reach the signaled candidate");
        assert_eq!(host.priority, crate::ice::priority_for(crate::ice::CandidateType::Host));
        // ② 对外候选：host 型、指定地址:端口、优先级低一档（−256）。
        let adv = cands.iter().find(|c| c.address == "127.0.0.1").expect("advertised candidate");
        assert_eq!(adv.port, port);
        assert_eq!(adv.typ, crate::ice::CandidateType::Host);
        assert_eq!(adv.priority, crate::ice::priority_for(crate::ice::CandidateType::Host) - 256);
        assert_eq!(adv.marshal(), Candidate::advertised_host_candidate([127, 0, 0, 1], port).marshal());
        // 固定端口路径的 socket 预算：整个收集恰好一枚（通配绑定）。
        assert_eq!(socks.taken(), 1, "fixed-port gather consumes exactly one socket");
        // marshal/unmarshal 往返一致（本机侧再验一次 wire 契约）。
        for c in &cands {
            assert_eq!(Candidate::unmarshal(&c.marshal()).unwrap(), *c);
        }
    }

    /// N12a fail-closed：固定端口被占（本测试先占住它）→ 收集硬错误、
    /// 绝不发没有候选支撑的帧、peer 留在 Idle、冷却后重试同样失败。
    #[test]
    fn fixed_port_conflict_fails_closed_without_signaling() {
        #[derive(Default)]
        struct CountingSignal {
            frames: Mutex<Vec<(String, PeerSignalKind, String)>>,
        }
        impl SignalExchange for CountingSignal {
            fn send(
                &self,
                to_key: &str,
                kind: PeerSignalKind,
                payload: &str,
                _port: u32,
            ) -> Result<(), ManagementError> {
                self.frames
                    .lock()
                    .expect("frames")
                    .push((to_key.to_string(), kind, payload.to_string()));
                Ok(())
            }
        }
        let port = grab_free_udp_port();
        // 先占住端口（0.0.0.0:P 与会话的通配绑定冲突 → EADDRINUSE）
        let holder = {
            let fd = unsafe { sys::socket(2, 2, 0) };
            assert!(fd >= 0);
            let sa = sys::sockaddr_in::new([0, 0, 0, 0], port);
            assert_eq!(
                unsafe { sys::bind(fd, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) },
                0,
                "holder bind must succeed"
            );
            fd
        };
        let signal = Arc::new(CountingSignal::default());
        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1));
        let fd = unsafe { sys::socket(2, 2, 0) };
        assert!(fd >= 0);
        socks.feed(fd);
        let mut orch = PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![crate::ice::InterfaceAddr {
                name: "eth0".into(),
                addr: [10, 99, 0, 7],
            }])),
            socks,
            signal: signal.clone(),
            wg: noop_wg(),
            tie_breaker: Some(7),
            fixed_local_port: Some(port),
            advertised_candidates: Vec::new(),
        });
        orch.set_peers(&["P".into()]);
        orch.set_signal_ready(true);
        assert!(orch.run_once(1000).is_err(), "bind conflict must fail the gather");
        assert!(
            signal.frames.lock().expect("frames").is_empty(),
            "no frame may leave without a bound candidate"
        );
        assert_eq!(orch.peer_status("P").expect("peer").state, PeerIceState::Idle);
        unsafe { sys::close(holder) };
    }
}
