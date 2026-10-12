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
//! - **角色**：RFC 8445 的 controlling/controlled 不在 signal 报文里。
//!   官方客户端的角色是**公钥比较**的本地决策（`client/internal/peer/
//!   conn.go` `isController = LocalKey > Key`；worker_ice.go `agentDial`
//!   在收到 Offer 或 Answer 后按同一比较选 `Dial`(controlling)/
//!   `Accept`(controlled)）。本编排复制同一语义（
//!   [`deterministic_controlling`]）：本端公钥大 → controlling，与谁
//!   发 Offer 无关 —— 双侧独立计算同一反称比较，任意时刻角色互补，
//!   glare（双方同时 OFFER）不再依赖 tie-breaker 收敛（官方对端的
//!   pion/ice 不实现 RFC 8445 §7.3.1.1：静默丢弃角色冲突检查、不回
//!   487，"双方都 controlling"即永久死锁）。tie-breaker 语义保留为
//!   防御性兜底（ice_session 已测）。本端公钥未注入（无 signal 材料
//!   的构造）时回退旧规则：主动发 OFFER 的一方 controlling、应答方
//!   controlled。
//!
//! ## 受保护 socket（治理 §二.4，fail-closed）
//!
//! 候选收集与 socket 全部来自注入的
//! [`crate::ice::UdpSocketSource`]（生产
//! [`crate::ice::ProtectedUdpFdSource`]：STUN 主机名的受保护 DNS 解析，宿主
//! 路径上还是那**一枚**共享 mux 的供给；设备路径的 mux 建在 WG 外层 socket
//! 上）；空源/绑定失败 → 该 peer 记录
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
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use crate::backoff::{ExponentialBackoff, MonotonicClock, OsRandom, Rng};
use crate::connector::{ErrorClass, WgPeerApplier};
use crate::envelope::EnvelopeKeyPair;
use crate::grpc::GrpcTransport;
use crate::ice::{
    Candidate, CandidateType, GatherConfig, InterfaceSource, StunServer,
    UdpSocketSource, DEFAULT_INTERFACE_BLACKLIST, DEFAULT_STUN_TIMEOUT_MS,
};
use crate::ice_session::{IceCredentials, IceEvent, IceSession, PairState};
use crate::management::ManagementError;
use crate::mgmtsock::ManagementSocketProvider;
use crate::signal::{SignalClient, SignalLoopEvent, SignalMessage, SignalOutgoing, SignalSession};
use crate::util::jnum;

// ---------------------------------------------------------------------------
// 重连节拍（上游 `client/internal/peer/guard/guard.go`）
// ---------------------------------------------------------------------------

/// 首次（未 Connected）重连节拍的起始间隔：上游
/// `guard.initialTicker` 的 `InitialInterval: 3 * time.Second`
/// （`guard.go:188-190`）。
pub const GUARD_INITIAL_MS: u64 = 3000;

/// 断连事件（ICE disconnected / relay 断连 / signal+relay 重连 / 网络变化）
/// 之后重建节拍的起始间隔：上游 `guard.newReconnectTicker` 的
/// `InitialInterval: 800 * time.Millisecond`（`guard.go:200-208`），且它把
/// 新 ticker 的**立即首跳**消费掉（`guard.go:211`），因此下一次发起落在
/// 事件后约 800 ms（±10%）。
pub const GUARD_RECONNECT_MS: u64 = 800;

/// 每跳间隔的抖动比例：上游 `RandomizationFactor: 0.1`（`guard.go:190`、`:202`）。
const GUARD_RANDOMIZATION: f64 = 0.1;
/// 间隔增长倍率：上游 `Multiplier: 2`（`guard.go:191`、`:203`）。
const GUARD_MULTIPLIER: f64 = 2.0;

/// 每个 peer 的重连节拍上限（= 该 peer 的 `ConnConfig.Timeout`）：
/// 上游 `engine.go:91-92` `PeerConnectionTimeoutMin/Max` 30 s / 45 s，
/// 逐 peer 随机（`engine.go:2010-2011`
/// `rand.Intn(Max-Min) + Min`），再作为 guard 的 `MaxInterval`
/// （`guard.go:192`、`:205`）。故上限落在 **[30 s, 45 s)**，且每个 peer
/// 在 worker 创建时抽定一次、终身不变。
pub const PEER_TIMEOUT_MIN_MS: u64 = 30_000;
pub const PEER_TIMEOUT_MAX_MS: u64 = 45_000;

/// 抽一个 [30 s, 45 s) 的 per-peer 超时（`engine.go:2010-2011` 的同一算术）。
fn draw_peer_timeout_ms(rng: &mut dyn Rng) -> u64 {
    let span = (PEER_TIMEOUT_MAX_MS - PEER_TIMEOUT_MIN_MS) as f64;
    PEER_TIMEOUT_MIN_MS + (rng.next_uniform() * span) as u64
}

/// [`std::time::Duration`] → 注入时钟的毫秒（`ExponentialBackoff` 的出参）。
fn delay_ms(d: std::time::Duration) -> u64 {
    d.as_millis().min(u64::MAX as u128) as u64
}

/// guard 的退避策略（`guard.go:188-195` / `:201-208` 逐字段对应）：
/// `InitialInterval = initial_ms`、`RandomizationFactor = 0.1`、
/// `Multiplier = 2`、`MaxInterval = 本 peer 的 timeout`、`Stop = backoff.Stop`
/// 而 `MaxElapsedTime == 0`（永不停）。
fn guard_backoff(initial_ms: u64, timeout_ms: u64) -> ExponentialBackoff {
    ExponentialBackoff::new(
        std::time::Duration::from_millis(initial_ms),
        GUARD_RANDOMIZATION,
        GUARD_MULTIPLIER,
        std::time::Duration::from_millis(timeout_ms),
        None,
    )
}

/// 一个接口 round 的收集线程的栈大小：round 的调用链很浅（`getifaddrs` /
/// `poll` / 格式化一行日志），但一个 23 peer × 2 接口的现场会有 ~46 个短命
/// 线程同时在跑，默认 2 MiB 栈纯属浪费（虚拟地址不紧张，但没必要）。
const GATHER_THREAD_STACK: usize = 256 * 1024;

/// 本地候选收集的执行者（STAGE 1，官方对齐审计 §2.7）——上游的
/// `go w.connect(...)`（`client/internal/peer/worker_ice.go:100-155`）与
/// `go a.gatherCandidates(ctx, done)`（pion fork `gather.go:39-65`）:
/// **发起即返回**，收集在别的执行者上跑，pump 永不等待 STUN。
///
/// 生产 = [`ThreadGatherExec`]（每次调用一枚线程）。测试/harness 可换成
/// 确定性驱动（[`InlineGatherExec`]）：注入时钟的 harness 从不等待真实
/// 时间，异步执行者会让它们与真实墙钟赛跑。这是**注入缝**，不是行为开关
/// ——两种驱动跑的是同一份 round 代码（[`crate::ice::gather_srflx_round`]）。
pub trait GatherExec: Send + Sync + 'static {
    /// 开始一个 round。返回 `false` = 起不来（线程上限/执行者拒绝）：调用方
    /// 必须把它当成「这一轮没有候选」处理，绝不回落成阻塞调用。
    fn spawn(&self, job: Box<dyn FnOnce() + Send>) -> bool;
}

/// STAGE 2/3：共享 mux 的注入缝（官方对齐审计 §2.7）。设备路径由
/// [`PeerIceOrchestrator::arm_mux_from_wg_socket`] 把 mux 建在 WG 设备自己的
/// socket 上（上游 `createOrUpdateMux`）；主机 CLI 与 host 测试路径让 core 自己
/// 从 [`crate::ice::UdpSocketSource`] 的受保护供给里建一枚
/// （[`IceMux::open_wildcard`] 的 provider 版）。两种来源都不改变语义：
/// **整个 ICE 平面只有这一枚 socket**。
pub type IceMuxSlot = Arc<Mutex<Option<Arc<crate::ice_mux::IceMux>>>>;

/// STAGE 3：mux 分用出的**非 STUN** 数据包的去处 —— 上游 `ICEBind` 的接收路径
/// 把 STUN 过滤器没吃掉的那块缓冲**直接**交回 WG（`ice_bind.go:257-285`，
/// `device_kernel_unix.go:109` 的读线程同理）。我们的 WG 设备是 boringtun 的
/// 单线程泵，没有 wireguard-go 的 `Bind` API，所以最小适配是让 mux 直接调
/// [`crate::connector::WgPeerApplier::handle_udp_inbound`]（`WgDevice::handle_udp`）。
/// STAGE 4 起这是非 STUN 包的**唯一**去处：会话级 data 队列已随池删除。
struct WgApplierSink(Arc<dyn crate::connector::WgPeerApplier>);

impl crate::ice_mux::MuxUdpSink for WgApplierSink {
    fn handle_udp(&self, datagram: &[u8], src: crate::ice_mux::MappedAddr) {
        // The device clock is the injected one where a harness drives it;
        // here the mux's reader thread supplies the real monotonic instant,
        // exactly like the connector's data-plane pump.
        let _ = self.0.handle_udp_inbound(datagram, src, crate::sys::mono_ms());
    }
}

/// 生产执行者：每个 round 一枚新线程（上游每协商一枚 goroutine）。
pub struct ThreadGatherExec;

impl GatherExec for ThreadGatherExec {
    fn spawn(&self, job: Box<dyn FnOnce() + Send>) -> bool {
        std::thread::Builder::new()
            .name("ice-gather".into())
            .stack_size(GATHER_THREAD_STACK)
            .spawn(job)
            .is_ok()
    }
}

/// 确定性执行者（host harness）：`spawn` 当场把 round 跑完，事件先入 channel，
/// 由调用方在同一拍内 `pump_gather` 取走。语义与阶段 1 之前完全一致
/// （`run_once` 一拍内完成收集），因此注入时钟的既有 harness 不需要真实等待。
pub struct InlineGatherExec;

impl GatherExec for InlineGatherExec {
    fn spawn(&self, job: Box<dyn FnOnce() + Send>) -> bool {
        job();
        true
    }
}

/// STAGE 0（官方对齐审计 §2.7）：整拍墙钟超过这么长才记一行
/// `N5c_ICE|tick|…|pass_ms=…|slow=true`（见 [`PeerIceOrchestrator::run_once`]）。
///
/// 生产 tick = `ICE_PUMP_TICK_MS`（10 ms），逐拍打印是刷屏；健康的一拍只做
/// 注入时钟驱动的状态机推进（0~1 ms），因此 200 ms 是「这一拍真的被什么
/// 挡住了」的下限——正是真机报告要求可验证的那个量。
pub const TICK_SLOW_LOG_MS: u64 = 200;

/// 周期性健康摘要的间隔（每个有会话的 peer 一行，§5 item 5）。
pub const HEALTH_SUMMARY_MS: u64 = 10_000;

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
    /// 本端广告的中继服务器 URL（`NetbirdConfig.relay.urls` 的第一个，
    /// signalexchange.proto Body 字段 8 `relayServerAddress`）。connector 的
    /// sync 路径经 [`Self::set_relay_address`] 更新；OFFER/ANSWER 出帧时
    /// 带入——上游每个 offer/answer 都携带本端 relay 地址
    /// （handshaker.go:224-242），对端**只在**该字段非空时才为我方
    /// OpenConn 建中继 lane（worker_relay.go:122-127 → :69），缺失即
    /// "WG over relay 无会话"。`None` = 未广告：字段保持缺省（无 relay
    /// 部署的行为与既有测试不变）。
    relay_address: std::sync::Mutex<Option<String>>,
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
            relay_address: std::sync::Mutex::new(None),
        });
        (exchange, rx)
    }

    /// connector sync 路径更新广告中继地址。多 URL 取**第一个**：
    /// management 的广告序即上游 picker 的候选序（同一 `urls` 列表，
    /// picker.go:47）；上游语义是「本端实际连接的 relay 实例地址」
    /// （manager.go:248-263），当前生产部署为单 URL，取首与之一致。
    /// `None`（未广告/管理端撤下）清除——出帧回到无 relay 字段形态。
    pub fn set_relay_address(&self, address: Option<String>) {
        *self.relay_address.lock().expect("relay_address lock") = address;
    }

    /// 当前广告的中继地址（诊断/测试）。
    pub fn relay_address(&self) -> Option<String> {
        self.relay_address.lock().expect("relay_address lock").clone()
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
        // OFFER/ANSWER 携带本端广告的中继地址（Body 字段 8）——上游
        // handshaker.go:224-242 对 offer/answer 一律填写；CANDIDATE 的
        // body 上游本就没有 relay 字段（signaler.go:32-38），保持缺省。
        let relay_address = match kind {
            PeerSignalKind::Offer | PeerSignalKind::Answer => self.relay_address(),
            PeerSignalKind::Candidate => None,
        };
        self.tx
            .send(SignalOutgoing {
                remote_key: to_key.to_string(),
                kind: wire_kind(kind),
                payload: payload.to_string(),
                wg_listen_port,
                relay_server_address: relay_address,
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
    spawn_signal_link_shared(runtime, cfg, exchange, Arc::new(tokio::sync::Mutex::new(rx)), on_event)
}

pub fn spawn_signal_link_shared(
    runtime: tokio::runtime::Handle,
    cfg: SignalLinkConfig,
    exchange: Arc<RealSignalExchange>,
    rx: Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<SignalOutgoing>>>,
    on_event: Arc<dyn Fn(SignalLinkEvent) + Send + Sync>,
) -> tokio::task::JoinHandle<()> {
    runtime.spawn(async move {
        let mut rx = rx.lock().await;
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
            .run_events_with_outbox_ref(&mut rx, move |ev| match ev {
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

/// selected pair 的一个端点（公开摘要：候选类型 + ip:port）。
///
/// 只投影 [`Candidate`] 中与状态展示有关的三个字段：`foundation` /
/// `priority` / `related_address` 属检查内部材料，与状态无关，不进暴露面。
/// 无凭证、无 fd、无口令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IceCandidateEndpoint {
    /// 候选类型（host/srflx/prflx/relay，与 signal 线上的 [`CandidateType::as_str`] 同名）。
    pub typ: CandidateType,
    /// 点分四段 IPv4 字符串（与 [`Candidate::address`] 同一编码域）。
    pub address: String,
    pub port: u16,
}

impl IceCandidateEndpoint {
    fn of(cand: &Candidate) -> Self {
        IceCandidateEndpoint { typ: cand.typ, address: cand.address.clone(), port: cand.port }
    }
}

/// 单 peer 的 ICE 细节快照（`connector_status()` 的 per-peer 明细输入；
/// 全公开材料，无凭证/ufrag/口令/fd）。
///
/// 与 [`PeerIceStatus`] 的分工：`PeerIceStatus` 是默认路由闸/可用性判定的
/// 输入（`selected_remote` 是落配用的 `[u8;4] + port`），本类型是**展示**
/// 面——同一个 selected pair 连同候选类型一起给出，供 status JSON 直接
/// 投影。可用性/落配两个事实与 `PeerIceStatus` 同源（`PeerConn::status`）。
///
/// 数据源是**调用时的活会话**（[`IceSession::selected_pair`]），不是缓存：
/// 迟到候选、重选 pair、会话拆除都在下一次调用如实反映；未选出 selected
/// pair 时两个端点都是 `None`（未选中就是未知，绝不回退到第一个候选）。
///
/// 刻意不含的两项（本增量不做，避免编造）：
/// - 检查 RTT：会话与 pair 都不保留它（只随 [`IceEvent::CheckSucceeded`] /
///   [`IceEvent::SelectedPair`] 瞬时出现并被日志消费），要展示须先在会话/
///   编排里如实留存；
/// - 状态变更时刻：没有这样的既有字段（`session_at_ms` 是会话建立时刻、
///   `guard_next_at_ms` 是下一跳重连节拍的到期时刻），两者都不是「上一次
///   状态变更」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerIceDetail {
    /// 远端 peer 的 base64 WG 公钥（公开材料）。
    pub pub_key: String,
    pub state: PeerIceState,
    /// 当前会话角色（`None` = 会话未建立）。
    pub controlling: Option<bool>,
    /// selected pair 的本地端点（类型 + ip:port）；`None` = 尚无 selected pair。
    pub selected_local: Option<IceCandidateEndpoint>,
    /// selected pair 的对端端点；`None` 语义同上。
    pub selected_remote: Option<IceCandidateEndpoint>,
    /// WG endpoint 落配已成功（与 [`PeerIceStatus::endpoint_applied`] 同一事实）。
    pub endpoint_applied: bool,
    /// 可用 = Connected 且 endpoint 落配成功（与 [`PeerIceStatus::reachable`] 同一事实）。
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

/// N2 v4.1 P7: the 8-char public-key short prefix this module's log lines use
/// (the N5c shape) — the `peer=` label of an outer-source receipt.
///
/// `pub(crate)` since 0050 (§8 item 2): `wg_device`'s session-lifecycle lines
/// carry the SAME label, and reusing this one function is what makes "同源同
/// 格式" a property of the code instead of a convention.
pub(crate) fn peer_short(key: &str) -> String {
    key.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect()
}

/// 0.0.26 gap 4：[`IceEvent::SelectedPair`] 的落盘证据行，唯一构造点（单测
/// 钉住格式）。`endpoint_changed=false` = 这次提名选中的远端地址与**上一次
/// 落配成功**的地址相同（物理网络换代后重新走同一条对端路径，真机 0.0.25
/// §5.4 的 `HkPMIwdu` 就是这种形态）；`true` = 真的换了远端 endpoint。
///
/// 公开运行材料：ip + port + 一个布尔，不含任何密钥/凭据。
pub(crate) fn selected_pair_line(addr: [u8; 4], port: u16, endpoint_changed: bool) -> String {
    format!(
        "N5_ICE|selected-pair|{}:{}|endpoint_changed={}",
        std::net::Ipv4Addr::from(addr),
        port,
        endpoint_changed
    )
}

/// 0.0.26 gap 1：`N5e_ICE|mux|…` 计数器行的**唯一**构造点，两处共用——
/// 回退读循环（`read` = 这一拍 drain 出的包数，`periodic=0`）与生产路径的
/// 周期性快照（`read=0`，`periodic=1`，见 [`HEALTH_SUMMARY_MS`]）。
///
/// 字段顺序是**追加式**的：0.0.25 的既有六个字段（`read`/`xor_hit`/
/// `xor_fill`/`overlay_drop`/`stun_unknown`/`data_unknown`）保持原位与原
/// 拼写，0.0.26 在行尾补 `xor_miss`/`xor_expired`/`queue_drop`/`periodic`
/// —— 真机报告 §4.3 要的 `xor_miss` 正是此前完全没有的那一个。
pub(crate) fn mux_stats_line(
    read: usize,
    s: &crate::ice_mux::MuxStats,
    periodic: bool,
) -> String {
    format!(
        "N5e_ICE|mux|read={read}|xor_hit={}|xor_fill={}|overlay_drop={}|stun_unknown={}|data_unknown={}|xor_miss={}|xor_expired={}|queue_drop={}|periodic={}",
        s.xor_hits,
        s.xor_filled,
        s.overlay_dropped,
        s.stun_unknown,
        s.data_unknown,
        s.xor_misses,
        s.xor_expired,
        s.queue_dropped,
        u8::from(periodic)
    )
}

/// ICE controlling 角色的确定性决胜（官方语义，`client/internal/peer/conn.go`：
/// `isController(config) = config.LocalKey > config.Key` —— 本端 WG 公钥的
/// base64 字符串**大于**对端公钥 → 本端 controlling；否则 controlled。
/// worker_ice.go `agentDial` 在收到 Offer **或** Answer 后按同一比较选择
/// `Dial`（controlling）/`Accept`（controlled）—— 即角色与"谁发的 Offer"
/// 完全无关，双方独立计算同一反称比较，结果必然互补。
///
/// 为什么必须复制这一语义而不是依赖 RFC 8445 §7.3.1.1 的 tie-breaker
/// （ICE-DIRECT-FAILURE-ANALYSIS §2.2，H1 根因）：官方对端用的 pion/ice
/// **不实现**角色冲突修复 —— 入站 Binding Request 带冲突角色属性时被
/// **静默丢弃**（不回 487），ErrorResponse（487）也被丢弃，`isControlling`
/// 在 start 后永不改变。若两侧从 t0 起都 controlling（双方都主动发
/// Offer 的 glare 场景），检查双向被丢、我方 487 被丢 → 永久死锁。角色
/// 在会话创建时即由公钥比较确定，任意时刻双侧互补，检查层的
/// tie-breaker 退化为防御性兜底而不再是收敛的必要条件。
///
/// `local = None`（本端公钥未知，仅测试/无 signal 材料的构造）返回
/// `None`，调用方回退旧规则（发起方 controlling、应答方 controlled）；
/// 生产两条 start 路径都带 signal 材料（= WG 密钥对），永不落入回退。
pub fn deterministic_controlling(local: Option<&str>, remote: &str) -> Option<bool> {
    local.map(|l| l > remote)
}

/// [`PeerIceOrchestrator`] 的角色决策（见 [`deterministic_controlling`]）：
/// 已知本端公钥 → 公钥比较；未知 → `fallback`（调用方传入旧规则）。
fn session_role(local_key: Option<&str>, remote_key: &str, fallback: bool) -> bool {
    deterministic_controlling(local_key, remote_key).unwrap_or(fallback)
}

/// [`PeerIceOrchestrator::start_locals`] 的本次结果。
///
/// 官方对齐审计 §2.7 **stage 2** 起只剩两态：本地候选（host 候选来自共享
/// mux 端口 × 各允许接口地址，[`crate::ice::mux_host_candidates`]）在
/// `start_locals` 里**当拍就算出来并交给会话**，所以「池不足等补给」这种
/// 中间态随壳层 socket 池一起退出 ICE 平面（`LocalsRun::WaitingForSockets`
/// 与它的等待账在 stage 2 删除；审计 §2.7 stage 2 "Deletes at the end: …
/// the pool sizing"）。
enum LocalsRun {
    /// 本地 host 候选已就位（`locals_done` 已置位）；srflx 是否还在跑由
    /// [`PeerConn::gather`] 表示（trickle，不阻塞）。
    Gathered,
    /// srflx 交换已在**别的执行者**上跑（上游「每 (STUN url, mux listen
    /// address) 一枚 goroutine」，fork `gather.go:491-513`）：本拍不阻塞。
    InFlight,
}

/// 一个 peer 正在跑的 srflx 收集（STAGE 1 的异步形状，STAGE 2 换了单位）。
///
/// 上游的对应物：`Agent.GatherCandidates()` 立即返回（fork `gather.go:39-65`），
/// 内部每个 srflx (network, STUN url, mux listen address) 一枚 goroutine
/// （`gather.go:491-513`），每枚只做一件事 —— `GetXORMappedAddr(serverAddr,
/// stunGatherTimeout)`（`gather.go:526`），而**共享 mux 的 in-flight 去重把
/// 它们收敛成每个 STUN 服务器一轮**。
///
/// 掉这个结构体 = 掉接收端：仍在跑的 worker send 失败即自行退出（它不持有
/// 任何 fd —— socket 是 mux 的，本阶段没有任何东西需要收拾）。
struct GatherInFlight {
    rx: Receiver<crate::ice::SrflxGather>,
    /// 还没回报的 srflx round 数（每 STUN 服务器一枚）：归零 = 本轮结束。
    pending: usize,
    /// 本轮收集开始的墙钟（`sys::mono_ms`）——`locals-gathered` 的 `gather_ms=`。
    started_wall_ms: u64,
    /// 已交给本会话的 host 候选数（观测）。
    added: usize,
    /// 已交给本会话的 srflx 候选数（观测）。
    srflx: usize,
    /// 全部失败原因（观测：每个失败的 srflx 交换一条；不再有 fail-closed
    /// 回滚 —— host 候选在 `start_locals` 里已经就位并已入列 OFFER）。
    errors: Vec<(String, ManagementError)>,
    /// 哪条调用路径发起的（`initiate` / `answer` / `retry` / `answer-late`）：
    /// 只进日志（`locals-gathered|source=`）。OFFER 是否欠账在**当拍**
    /// [`PeerIceOrchestrator::start_locals`] 里就按同一个 phase 判定了
    /// （host 候选就位即补 OFFER，上游 `GatherCandidates()` 后立刻 dial）。
    phase: &'static str,
}

// ---------------------------------------------------------------------------
// per-peer 内部状态
// ---------------------------------------------------------------------------

struct PeerConn {
    key: String,
    state: PeerIceState,
    session: Option<IceSession>,
    /// 本 peer 的**本地 ICE 凭证**：`ufrag`/`pwd` 在第一次会话创建时抽定一次，
    /// 之后**终身复用**，跨会话重建不变——上游 `WorkerICE.localUfrag/localPwd`
    /// 在 `NewWorkerICE` 里 `GenerateICECredentials()` 一次
    /// （`client/internal/peer/worker_ice.go:91-97`、`client/internal/peer/ice/agent.go:111-122`），
    /// 每次 `reCreateAgent` 都传同一对凭证（`worker_ice.go:220-226`），
    /// 所以重发 OFFER/ANSWER 与重建 agent 都不改变对外凭证。
    local_creds: Option<IceCredentials>,
    /// 发起策略（默认 true = 主动发 OFFER，controlling 起始；false = 纯
    /// 应答方，等对端 OFFER）。glare（双方都发起）由 tie-breaker 收敛。
    initiator: bool,
    /// 已绑定进会话的本地候选数（观测）。
    locals_signaled: usize,
    /// 当前会话的远端凭证（Offer/Answer 携带的 `"ufrag:pwd"`）。
    remote_creds: Option<IceCredentials>,
    /// 远端候选入列次数（含重复；观测与 start 前置判断）。
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
    /// 0.0.26 gap 4（真机 0.0.25 §5.4，peer `HkPMIwdu`）：**最后一次真的落配
    /// 成功**的远端 endpoint。`selected_remote` 是「当前活会话选中的对」，
    /// 会话一拆（`drop_session`）就清空；本字段**刻意不被拆会话清掉**，因为
    /// 「物理网络换代后重新提名到同一个远端地址」这件事只有跨会话比较才看得
    /// 出来 —— 它正是 `N5_ICE|selected-pair|…|endpoint_changed=false` 的判据
    /// （地址真的变了才是 `true`）。
    last_applied_endpoint: Option<([u8; 4], u16)>,
    last_error: Option<ErrorClass>,
    /// 当前会话的创建时刻（注入时钟；0 = 无会话）。**纯观测**（日志
    /// `round=`）：曾经的 9 s 截止期起点已随 `START_DEADLINE_MS`（已删除）
    /// 退场。
    session_at_ms: u64,
    /// 本 peer 的重连上限（`ConnConfig.Timeout`）：[30 s, 45 s) 随机抽定，
    /// 终身不变（`engine.go:2010-2011`）。guard 退避的 `MaxInterval`。
    timeout_ms: u64,
    /// guard 重连节拍（`guard.go:187-198` / `:200-213` 的
    /// `cenkalti/backoff` ExponentialBackOff，`crate::backoff` 同形复刻）：
    /// `current_interval` 从 [`GUARD_INITIAL_MS`] 或 [`GUARD_RECONNECT_MS`]
    /// 起，每次 `next_delay` 先抖动 ±10% 再 ×2、封顶 `timeout_ms`。
    guard: ExponentialBackoff,
    /// 下一跳的时刻（注入时钟；0 = 立即到期）。对应上游 ticker 的
    /// **保证首跳**（`NewTicker` 创建即发一次，`guard.go:101`/`:211`）。
    guard_next_at_ms: u64,
    /// 上一次输出健康摘要行的时刻（0 = 尚未输出过；见
    /// [`HEALTH_SUMMARY_MS`]）。
    last_health_ms: u64,
    /// 正在跑的异步本地候选收集（STAGE 1）。`None` = 没有收集在跑。
    gather: Option<GatherInFlight>,
}

impl PeerConn {
    /// `rng` 抽本 peer 的 `ConnConfig.Timeout`（[`PEER_TIMEOUT_MIN_MS`]..
    /// [`PEER_TIMEOUT_MAX_MS`]），与上游 `engine.go:2010-2011` 同序：
    /// 超时先抽定，再交给 guard 当 `MaxInterval`（`guard.go:192`）。
    fn new(key: String, rng: &mut dyn Rng) -> Self {
        let timeout_ms = draw_peer_timeout_ms(rng);
        PeerConn {
            key,
            state: PeerIceState::Idle,
            session: None,
            local_creds: None,
            initiator: true,
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
            last_applied_endpoint: None,
            last_error: None,
            session_at_ms: 0,
            timeout_ms,
            guard: guard_backoff(GUARD_INITIAL_MS, timeout_ms),
            // 上游 `NewTicker` 保证创建即发一次首跳（`guard.go:101`）：0 =
            // 本 peer 的第一拍就可以发起。
            guard_next_at_ms: 0,
            last_health_ms: 0,
            gather: None,
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

    /// [`PeerIceDetail`] 快照：selected pair 的两个端点取自**活会话**
    /// [`IceSession::selected_pair`]（没有会话或尚未选中 → 都是 `None`），
    /// 角色/落配/可用三个事实复用 [`Self::status`] 的同一份派生。
    ///
    /// 纯读：不 take / dup / close 任何 socket，也不改会话——收集到的
    /// socket 归属（[`IceSession`] 持有、`Drop`/`stop` 关闭）不受影响。
    fn detail(&self) -> PeerIceDetail {
        let status = self.status();
        let (selected_local, selected_remote) = match self
            .session
            .as_ref()
            .and_then(|s| s.selected_pair())
        {
            Some((local, remote)) => (
                Some(IceCandidateEndpoint::of(&local)),
                Some(IceCandidateEndpoint::of(&remote)),
            ),
            None => (None, None),
        };
        PeerIceDetail {
            pub_key: status.pub_key,
            state: status.state,
            controlling: status.controlling,
            selected_local,
            selected_remote,
            endpoint_applied: status.endpoint_applied,
            reachable: status.reachable,
            last_error: status.last_error,
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
    /// 语义见 `start_locals` 与 `docs/self-hosted-interop-plan.md`
    /// §端口映射 / 对外候选。**STAGE 2 起它绑定的是共享 mux socket**
    /// （不再是每会话一枚 socket）：候选仍保留接口地址 + 固定端口，socket 通配
    /// 绑定该端口——上游单一 `0.0.0.0:NET_PORT` 共享 socket 的形状。
    pub fixed_local_port: Option<u16>,
    /// **HOST-ONLY（N12a）**：显式对外可达候选（端口映射后的
    /// `外部IP:端口`），作为**额外的 host 型候选**随既有 signal 路径原样
    /// 发给对端（wire 形态 = [`Candidate::marshal`]，优先级比真实 host
    /// 候选低一档 local preference——见
    /// [`Candidate::advertised_host_candidate`]）。默认为空 = 不通告。
    pub advertised_candidates: Vec<Candidate>,
}

/// 006 resource snapshot: scalar-only view of the orchestrator's live ICE
/// resources. Every field is a COUNT — no peer key, no fd, no socket, no
/// URL — so the document built from it can never leak routing or credential
/// material.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IceResourceCounts {
    /// Peer entries currently holding an [`IceSession`] OBJECT, including one
    /// [`PeerIceOrchestrator::stop_all`] already stopped (the object is
    /// retained until the next renegotiation / peer removal drops it).
    pub sessions_present: usize,
    /// Sessions with [`IceSession::start`] done and no `stop` yet — the
    /// sessions actually being pumped.
    pub sessions_live: usize,
    /// Sessions already stopped whose object is still retained.
    pub sessions_stopped: usize,
    /// Sum of [`IceSession::live_local_socket_count`] over all retained
    /// sessions: fds this core still owns (0 after `stop_all`).
    pub live_local_sockets: usize,
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
    /// signal 流 false→true 的**重连**边沿（guard 的 `srReconnectedChan`，
    /// `guard.go:155-160`）：下一拍 `run_once` 为每个 peer 重建 800 ms 起跳的
    /// `newReconnectTicker` 后清除。**首次就绪不算**：那是初始连接，上游的
    /// 初始 OFFER 由 guard `initialTicker` 的立即首跳发出（`NewTicker` 创建
    /// 即发一次），不该被推迟 800 ms。
    signal_reconnected: bool,
    /// 是否曾经就绪过（区分「首次就绪」与「重连边沿」）。
    signal_ever_ready: bool,
    /// 非网络图 peer 的 signal 帧计数（观测，不作为错误）。STAGE 1 起**只**有
    /// 这一支会计数：无会话的 ANSWER 不再丢弃、改为开启一轮协商（官方语义，
    /// 见 `handle_signal`），因此它不再是「迟到 ANSWER 被丢」的指示器。
    unknown_signal: u64,
    /// HOST-ONLY（N12a）：固定绑定端口（None = 临时端口，默认）。
    fixed_local_port: Option<u16>,
    /// HOST-ONLY（N12a）：显式对外候选（默认空 = 不通告）。
    advertised_candidates: Vec<Candidate>,
    /// 本端 WG 公钥（base64）—— 官方 isController 角色决胜的输入（见
    /// [`deterministic_controlling`]）。connector 在两条生产 start 路径
    /// 都注入（signal 材料 = WG 密钥对）；None = 未知 → 会话角色回退
    /// 旧规则（发起方 controlling、应答方 controlled）。
    local_key: Option<String>,
    /// 本地候选收集的执行者（STAGE 1）：生产 = 每次收集一枚线程，
    /// 注入时钟的 harness 用确定性驱动（见 [`GatherExec`]）。
    ///
    /// ea8ede1 的「每拍最多一次阻塞收集」预算与 `gather_cursor` 轮转随本
    /// 阶段一起删除：那是「收集会挡住共享拍」的直接后果，收集离开 pump 后
    /// 就没有东西可轮转、可配给了（审计 §2.7 stage 1，"Retires: the need
    /// for E1/E2/I7"）。
    exec: Arc<dyn GatherExec>,
    /// STAGE 2/3（官方对齐审计 §2.7）：**整个 ICE 平面唯一那枚 socket**，stage 3
    /// 起它**就是** WG 外层 socket。设备路径由 connector 在壳层 protect + feed 之后
    /// 建在这枚 socket 上（[`PeerIceOrchestrator::arm_mux_from_wg_socket`]，
    /// 上游 `createOrUpdateMux`）；主机/测试路径在首次需要时从 `socks` 供给里取一枚
    /// 受保护 socket 就地建（[`PeerIceOrchestrator::arm_mux`]）——两条路都只有这一
    /// 枚，绝不每 peer 每接口一枚。
    mux: IceMuxSlot,
    /// 熵源：每 peer 的超时抽定（[`PeerConn::new`]）与 guard 每跳的 ±10% 抖动
    /// （[`PeerIceOrchestrator::guard_tick`]）。生产 = [`OsRandom`]；注入时钟的
    /// harness 用 [`PeerIceOrchestrator::set_guard_rng`] 换成定序样本，才能逐值
    /// 断言 3/6/12/24 s 序列。
    guard_rng: Box<dyn Rng + Send>,
    /// 0.0.26 gap 1（真机 0.0.25 §4.3）：上一次周期性 mux 计数行的时刻（0 =
    /// 尚未打过）。回退读循环那一行是「这一拍读了多少」，而生产跑的是 mux
    /// 自己的读线程 → 它一次都不打，`xor_hit/xor_miss` 在设备日志里
    /// 不可观测；本字段驱动与健康摘要同一节奏（[`HEALTH_SUMMARY_MS`]）的
    /// 同一行计数器快照。
    last_mux_stats_ms: u64,
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
            signal_reconnected: false,
            signal_ever_ready: false,
            unknown_signal: 0,
            fixed_local_port: deps.fixed_local_port,
            advertised_candidates: deps.advertised_candidates,
            local_key: None,
            exec: Arc::new(ThreadGatherExec),
            mux: Arc::new(Mutex::new(None)),
            guard_rng: Box::new(OsRandom),
            last_mux_stats_ms: 0,
        }
    }

    /// 换 guard 的熵源（注入缝）。生产从不调用（默认 [`OsRandom`]）；注入
    /// 时钟的 harness 在 `set_peers` **之前**装定序样本，才能同时钉住每 peer
    /// 的超时抽定与逐跳抖动（[`PeerConn::new`] 是超时的唯一抽取点）。
    pub fn set_guard_rng(&mut self, rng: Box<dyn Rng + Send>) {
        self.guard_rng = rng;
    }

    /// 换本地候选收集的执行者（STAGE 1 的注入缝）。生产从不调用：默认就是
    /// 上游形态的「每 round 一枚线程」；host harness 用
    /// [`InlineGatherExec`] 保住注入时钟的确定性（见该类型）。
    pub fn set_gather_exec(&mut self, exec: Arc<dyn GatherExec>) {
        self.exec = exec;
    }

    /// STAGE 2：把共享 mux 的槽交给本编排器（connector 在构造后立即调用；
    /// 壳层 feed 到达时填槽）。同一个槽也可能被 connector 之后替换——替换
    /// 后的下一拍 `run_once` 会把仍骑旧 mux 的会话拆掉重协商（见
    /// [`PeerIceOrchestrator::drop_stale_mux_sessions`]）。
    pub fn set_ice_mux(&mut self, slot: IceMuxSlot) {
        self.mux = slot;
    }

    /// 当前共享 mux（若已就位）。
    pub fn ice_mux(&self) -> Option<Arc<crate::ice_mux::IceMux>> {
        self.mux.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// 0.0.26 gap 1：mux 计数器的**非阻塞**快照，供
    /// `connector_status()` 的 `ice_mux` 尾字段使用。`None` = 没有 mux，
    /// 或 mux 的状态锁此刻被读线程/泵持有（本文件「busy = 未知，绝不编造
    /// 零」的同一约定）。
    pub fn mux_stats_try(&self) -> Option<crate::ice_mux::MuxStats> {
        self.ice_mux().and_then(|m| m.stats_try())
    }

    /// STAGE 2 的 fail-closed 建 mux 路径（主机 CLI 与 host 测试）：从受保护
    /// socket 供给里取**一枚**（不是每接口一枚），bind 通配地址（固定端口
    /// 模式下就是这个端口），作为整个 ICE 平面的 socket；随后像设备路径一样
    /// 挂上 WG 数据面 sink（STAGE 4 起非 STUN 包**只有**这一条去处 ——
    /// 会话级 data 队列已随池一起删除）。
    ///
    /// 设备路径不会走到这里：壳层 feed 已经把槽填上；若槽空且供给也空
    /// （壳层没 feed、也没有宿主供给），返回 `Err` ——
    /// `ice:no-mux:…`，fail-closed，绝不自己 `socket(2)` 开一枚无保护的。
    fn arm_mux(&mut self) -> Result<Arc<crate::ice_mux::IceMux>, ManagementError> {
        if let Some(mux) = self.ice_mux() {
            return Ok(mux);
        }
        let owned = self.socks.take_owned().map_err(|e| {
            ManagementError::Network(format!(
                "ice:no-mux: {}",
                crate::ice::seam_to_management(e)
            ))
        })?;
        let overlay = crate::ice::system_overlay_nets(&self.blacklist);
        let want = self.fixed_local_port.unwrap_or(0);
        let mux = crate::ice_mux::IceMux::adopt_unbound(owned, want, overlay)
            .map_err(|e| ManagementError::Network(format!("ice:no-mux: {e}")))?;
        mux.attach_udp_sink(Arc::new(WgApplierSink(self.wg.clone())));
        crate::hilog::emit(&format!(
            "N5e_ICE|mux-armed|source=provider|port={}|fixed={}",
            mux.local_port(),
            self.fixed_local_port.unwrap_or(0)
        ));
        *self.mux.lock().unwrap_or_else(|p| p.into_inner()) = Some(mux.clone());
        Ok(mux)
    }

    /// STAGE 3（官方对齐审计 §2.7）：把**唯一一枚共享 mux**建在 WG 设备自己的
    /// 外层 socket 上 —— 上游 `ICEBind.createOrUpdateMux`（`ice_bind.go:291-316`
    /// 「Don't close the old mux - it doesn't own the underlying connections」）
    /// 与 `TunKernelDevice.Up`（`device_kernel_unix.go:97-113`）都是这个形状：
    /// ICE 与 WireGuard 共用**一枚** socket，一次 open+bind+protect。
    ///
    /// 做四件事，缺一不可：
    /// 1. 从 WG 设备借一枚 fd（dup，绝不关闭对方的），`IceMux::adopt` 成 mux；
    /// 2. 填 [`PeerIceOrchestrator::mux`] 槽（旧 mux 的会话由
    ///    [`PeerIceOrchestrator::drop_stale_mux_sessions`] 拆掉重协商）；
    /// 3. 挂上 WG 数据面 sink：非 STUN 包**直接**进设备（`handle_udp`），不再
    ///    经会话队列（上游：WG 自己的接收路径接着处理 STUN 过滤器剩下的缓冲，
    ///    `ice_bind.go:257-285`）；
    /// 4. 让设备交出世面读端（`set_shared_socket_reader(true)`）：一枚 socket
    ///    只能有一个读端，否则两边互相偷包。
    ///
    /// reader 线程不在这里起：生产由 connector 在同一步 `start_reader()`，
    /// 注入时钟的 host harness 则继续自己 `drain()`（否则一个真实线程会打破
    /// 它们的确定性）。返回 `(mux, replaced)`。
    pub fn arm_mux_from_wg_socket(
        &mut self,
    ) -> Result<(Arc<crate::ice_mux::IceMux>, bool), ManagementError> {
        let Some(raw_fd) = self.wg.wg_socket_fd() else {
            return Err(ManagementError::Network("ice:no-wg-socket".into()));
        };
        let dup = crate::mgmtsock::dup_socket_fd(raw_fd)
            .map_err(|e| ManagementError::Network(format!("ice:no-wg-socket (errno={})", e.errno())))?;
        let owned = unsafe { std::os::fd::FromRawFd::from_raw_fd(dup) };
        let overlay = crate::ice::system_overlay_nets(&self.blacklist);
        // already bound + protected: the WG outer socket itself
        let mux = crate::ice_mux::IceMux::adopt(owned, overlay, 0)
            .map_err(|e| ManagementError::Network(format!("ice:mux-over-wg: {e}")))?;
        let port = mux.local_port();
        let shared = self.wg.set_shared_socket_reader(true);
        mux.attach_udp_sink(Arc::new(WgApplierSink(self.wg.clone())));
        let replaced = self
            .mux
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .replace(mux.clone())
            .is_some();
        crate::hilog::emit(&format!(
            "N5e_ICE|mux-arm|source=wg-socket|fd={raw_fd}|port={port}|replaced={}|device-reader-retired={}",
            u8::from(replaced),
            u8::from(shared)
        ));
        Ok((mux, replaced))
    }

    /// STAGE 3：放掉共享 mux —— 停掉它的读线程、关掉它自己那枚 dup（恰好一次）。
    /// connector 的 stop 路径在 `stop_all()` 之后调用（会话都已注销）；仍持有它的
    /// 会话只会让 socket 多活一会儿，读线程已经停了，不会再有人读。
    /// 返回 `true` = 确实放掉了一枚。
    pub fn drop_mux(&mut self) -> bool {
        let taken = self.mux.lock().unwrap_or_else(|p| p.into_inner()).take();
        match taken {
            Some(mux) => {
                mux.stop_reader();
                true
            }
            None => false,
        }
    }

    /// STAGE 2：把骑着一枚**已被替换**的 mux 的会话整段拆掉重协商。壳层在
    /// 物理网络切换（recreate + 重新 protect）之后会重新 feed 一枚 mux，
    /// 旧会话的候选端口在新 mux 上不存在，留着就是永久沉默的死路径。
    fn drop_stale_mux_sessions(&mut self, _now_ms: u64) {
        let current = self.ice_mux().map(|m| Arc::as_ptr(&m));
        let stale: Vec<usize> = self
            .peers
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match p.session.as_ref().and_then(|s| s.mux_ptr()) {
                Some(ptr) if Some(ptr) != current => Some(i),
                _ => None,
            })
            .collect();
        for idx in stale {
            self.drop_session(idx, PeerIceState::Failed, "ice-mux-replaced");
        }
    }


    /// 注入本端 WG 公钥（base64，与网络图 peer `wg_pub_key` 同一编码域）：
    /// 启用官方 isController 语义的角色决胜（[`deterministic_controlling`]）。
    /// connector 在 orchestrator 构造后、任何会话建立前调用（生产两条
    /// start 路径必经 signal 材料 → 必然注入）。
    pub fn set_local_key(&mut self, key: String) {
        self.local_key = Some(key);
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
            // 每个新条目抽定自己的 `ConnConfig.Timeout`（[30,45)s，
            // `engine.go:2010-2011`）；rng 从 `guard_rng` 借出（与 `peers`
            // 是不同字段，编译器按字段拆分借用）。
            let rng = &mut *self.guard_rng;
            self.peers.push(PeerConn::new(key.clone(), rng));
        }
    }

    /// signal 流就绪（connector 持有注册好的 signal 流后置位；置位前
    /// `run_once` 不发起任何东西）。**重连**边沿（曾经就绪过之后再就绪）
    /// 记一个待复位标志，由 `run_once` 在下一拍重建 guard 节拍（800 ms 起
    /// 跳）；首次就绪不记——初始 OFFER 走 guard 首跳的立即拍。
    pub fn set_signal_ready(&mut self, ready: bool) {
        if ready && !self.signal_ready {
            if self.signal_ever_ready {
                self.signal_reconnected = true;
            }
            self.signal_ever_ready = true;
        }
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

    /// 单个 peer 的 ICE 细节快照（[`PeerIceDetail`]；未知 key → `None`）。
    ///
    /// 只读、owned：不 take/dup/close 任何 socket、不改会话状态，因此受保护
    /// socket 的归属（[`IceSession`] 持有、`Drop`/`stop` 关闭）不变；迟到
    /// 候选与重选出的 selected pair 在每次调用时如实反映（无缓存）。
    pub fn peer_ice_detail(&self, key: &str) -> Option<PeerIceDetail> {
        self.peers.iter().find(|p| p.key == key).map(|p| p.detail())
    }

    /// 全部 peer 的 ICE 细节快照，按 `pub_key` 排序（与 `peer_relays` 同一
    /// 约定：JSON 输出与网络图刷新顺序无关，逐次调用稳定）。
    ///
    /// `connector_status()` 的 per-peer 明细字段直接消费本返回值。
    pub fn ice_details(&self) -> Vec<PeerIceDetail> {
        let mut details: Vec<PeerIceDetail> = self.peers.iter().map(|p| p.detail()).collect();
        details.sort_by(|a, b| a.pub_key.cmp(&b.pub_key));
        details
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

    /// 006 resource snapshot: ONE scalar copy of the live ICE resources
    /// ([`IceResourceCounts`]). Pure read under the caller's orchestrator
    /// lock — no socket is taken, dup'd or closed, no session state moves,
    /// and no peer key leaves this call.
    pub fn resource_counts(&self) -> IceResourceCounts {
        let mut counts = IceResourceCounts::default();
        for p in &self.peers {
            let Some(session) = p.session.as_ref() else { continue };
            counts.sessions_present += 1;
            if session.is_stopped() {
                counts.sessions_stopped += 1;
            } else if session.is_started() {
                counts.sessions_live += 1;
            }
            counts.live_local_sockets += session.live_local_socket_count();
        }
        counts
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
        // STAGE 0: this call is a pass of its own — its wall clock feeds the
        // `pass_ms=` field of every `locals-gathered` line it produces.
        let pass_start_ms = crate::sys::mono_ms();
        let Some(idx) = self.peers.iter().position(|p| p.key == from_key) else {
            self.unknown_signal += 1; // 非网络图 peer：丢弃，不报错
            return Ok(());
        };
        // signal-rx 行的 kind 相关附加字段（§5 item 3：candidate 记解析后
        // 的类型/地址）。
        let mut rx_extra = String::new();
        match kind {
            PeerSignalKind::Offer => {
                let creds = parse_ufrag_pwd(payload)?;
                // 已有会话时收到 Offer：官方 `WorkerICE.OnNewOffer`
                //（worker_ice.go:100-157）只按 **session id** 决定是否重建
                // agent；重建时本地凭证不变（`localUfrag/localPwd` 是 per-peer
                // 的），只是 `reCreateAgent` + 重新 dial + 重新收集。本端没有
                // 线上 session id（本仓 signal 帧不带 `Body.sessionId`），用
                // **远端凭证变化**作等价判据：凭证同样 per-peer 稳定后，它只
                // 在「对端换了实现/换了 peer worker」时触发；本端 Disconnected
                // 时已按上游 closeAgent 语义拆掉会话（见 `drain_events`），
                // 天然落入下面的全新应答路径。Failed 会话已在事件处理中拆掉
                //（session=None），同理。
                let renegotiate = self.peers[idx].session.is_some()
                    && self.peers[idx].state == PeerIceState::Connected
                    && self.peers[idx].remote_creds.as_ref() != Some(&creds);
                if renegotiate {
                    let peer = &mut self.peers[idx];
                    peer.session = None;
                    // 在跑的收集属于**旧**协商：丢接收端 = 丢掉旧一轮的候选
                    // （worker 自己收尾并关闭它的 dup socket），绝不让旧候选
                    // 落进新会话。本地凭证不在此列：它终身复用。
                    peer.gather = None;
                    peer.locals_done = false;
                    peer.started = false;
                    peer.remote_cands_seen = 0;
                    peer.pending_remote.clear();
                    peer.session_at_ms = 0;
                    peer.last_health_ms = 0;
                }
                let had_session = self.peers[idx].session.is_some();
                if !had_session {
                    // 应答方会话：角色按官方 isController 语义由公钥比较确定
                    //（上游 signal 无角色字段；本端公钥未注入时回退
                    // controlled —— 见 [`deterministic_controlling`]）。
                    self.ensure_session(idx, now_ms, "answer")?;
                    self.peers[idx]
                        .session
                        .as_mut()
                        .expect("session just ensured")
                        .set_remote_credentials(creds.clone())?;
                    {
                        let peer = &mut self.peers[idx];
                        peer.remote_creds = Some(creds);
                        peer.state = PeerIceState::Gathering;
                        peer.answer_owed = true;
                    }
                    match self.start_locals(idx, now_ms, pass_start_ms, "answer") {
                        // host 候选当拍就位（`Gathered`）或 srflx 仍在 worker 上
                        // 跑（`InFlight`）：两种都由 `pump_gather`/`on_locals_ready`
                        // 补 ANSWER。
                        Ok(LocalsRun::Gathered) | Ok(LocalsRun::InFlight) => {}
                        Err(e) => {
                            // 收集失败 = 上游 `connect()` 里 `GatherCandidates`
                            // 报错后的 `closeAgent`（worker_ice.go:257-261）：
                            // agent 随会话一起拆掉，不留半成品。重谈判由 guard
                            // 节拍驱动（对端也会继续发 OFFER）。
                            let peer = &mut self.peers[idx];
                            peer.last_error = Some(ErrorClass::from_management(&e));
                            crate::hilog::emit(&format!(
                                "N5c_ICE|locals-failed|peer={}|round={}|phase=answer|reason={e}",
                                peer_short(&peer.key),
                                peer.session_at_ms
                            ));
                            self.drop_session(idx, PeerIceState::Failed, "gather-failed");
                        }
                    }
                    // 内联执行者（host harness）在同一拍里跑完 round：立刻取件，
                    // 保住「一拍内候选 + ANSWER 就位」的既有语义。
                    let _ = self.pump_gather(idx, now_ms, pass_start_ms);
                } else {
                    // glare / 对端（重）发 Offer 时本端已有会话。官方语义
                    // （worker_ice.go OnNewOffer + handshaker.Listen）：
                    // ①角色由公钥比较在会话创建时确定，与 Offer 方向无关
                    //   —— 本会话的角色已按 [`deterministic_controlling`]
                    //   算出，双侧互补，无需也不应在这里改变（修复前
                    //   "保留本端角色（发起方必 controlling）"在双方都主动
                    //   时两侧同 controlling，pion 静默丢弃全部冲突检查且
                    //   不回 487 → 永久死锁，H1 根因）；
                    // ②每个收到的 Offer 都要回 ANSWER（handshaker.Listen
                    //   无条件 sendAnswer）：官方对**同一 session id** 的重复
                    //   Offer 也照样回 ANSWER，只是不重建 agent
                    //   （worker_ice.go:105-114 + handshaker.go:136）；
                    // ③对端换了凭证 → 采纳新凭证；官方 worker 重启 agent 时
                    //   本地凭证（localUfrag/localPwd）不变，我方同理保持本地
                    //   凭证/候选/端口，重发本地候选让对端新 agent 配对。
                    let same_creds =
                        self.peers[idx].remote_creds.as_ref() == Some(&creds);
                    if !same_creds {
                        let local_cands = {
                            let peer = &mut self.peers[idx];
                            peer.session
                                .as_mut()
                                .expect("session checked")
                                .set_remote_credentials(creds.clone())?;
                            peer.remote_creds = Some(creds);
                            peer.session.as_ref().expect("session checked").local_candidates()
                        };
                        let peer = &mut self.peers[idx];
                        for cand in &local_cands {
                            peer.outbox.push_back((PeerSignalKind::Candidate, cand.marshal()));
                        }
                        crate::hilog::emit(&format!(
                            "N5c_ICE|glare-rx|peer={}|round={}|creds=new|role={}|cands={}",
                            peer_short(&peer.key),
                            peer.session_at_ms,
                            if peer.session.as_ref().expect("session checked").is_controlling() {
                                "controlling"
                            } else {
                                "controlled"
                            },
                            local_cands.len(),
                        ));
                    } else {
                        let peer = &self.peers[idx];
                        crate::hilog::emit(&format!(
                            "N5c_ICE|glare-rx|peer={}|round={}|creds=same|role={}|cands=0",
                            peer_short(&peer.key),
                            peer.session_at_ms,
                            if peer.session.as_ref().expect("session checked").is_controlling() {
                                "controlling"
                            } else {
                                "controlled"
                            },
                        ));
                    }
                    // 回 ANSWER（上游 `sendAnswer` 对每个 Offer 无条件执行）。
                    // 本地候选未就绪（`answer_owed` 仍欠着）时跳过 ——
                    // `on_locals_ready` 首次拿到候选时统一补发。
                    if self.peers[idx].locals_done && self.peers[idx].local_creds.is_some() {
                        let (ufrag, pwd) = {
                            let c = self.peers[idx].local_creds.as_ref().expect("checked");
                            (c.ufrag.clone(), c.pwd.clone())
                        };
                        let peer = &mut self.peers[idx];
                        peer.outbox.push_back((
                            PeerSignalKind::Answer,
                            format!("{}:{}", ufrag, pwd),
                        ));
                    }
                }
            }
            PeerSignalKind::Answer => {
                let creds = parse_ufrag_pwd(payload)?;
                if self.peers[idx].session.is_none() {
                    // 无会话的 ANSWER = 对端应答了**已被拆掉**的那次 OFFER
                    // （拆会话与 ANSWER 到达只差半拍）。
                    //
                    // 官方语义：**没有「丢帧」这一支**。signal 引擎对任何
                    // 目的地都先 `ActivatePeer` 再派发（engine.go:2088-2097），
                    // handshaker 把 Offer 与 Answer 一并交给 ICE worker
                    // （handshaker.go:140-156），worker 用收到的这一帧**新建
                    // agent** 并 Dial/Accept（worker_ice.go:100-155）——迟到
                    // ANSWER 会直接开启一轮协商，而不是被丢掉（审计 §1.1 E5）。
                    //
                    // 关键：**它不回 OFFER**。`handshaker.Listen` 只在
                    // `remoteOffersCh` 分支 `sendAnswer()`（:136），
                    // `remoteAnswerCh` 分支（:140-156）只把 OfferAnswer 交给
                    // ICE worker；`connect` 里是 `Dial`/`Accept`（:607-613），
                    // 不是 `SendOffer`。凭证 per-peer 稳定之后，本端也不需要
                    // 「换一套新凭证重新发起」这个自造绕道：对端手上有本端
                    // 未变的 `ufrag:pwd`，本端直接进 Checking，候选照 trickle，
                    // 双方在对端 agent 上配对。
                    self.ensure_session(idx, now_ms, "answer-late")?;
                    self.peers[idx]
                        .session
                        .as_mut()
                        .expect("session just ensured")
                        .set_remote_credentials(creds.clone())?;
                    {
                        let peer = &mut self.peers[idx];
                        peer.remote_creds = Some(creds);
                        peer.state = PeerIceState::Gathering;
                        peer.answer_owed = false;
                    }
                    match self.start_locals(idx, now_ms, pass_start_ms, "answer-late") {
                        Ok(LocalsRun::Gathered) | Ok(LocalsRun::InFlight) => {}
                        Err(e) => {
                            let peer = &mut self.peers[idx];
                            peer.last_error = Some(ErrorClass::from_management(&e));
                            crate::hilog::emit(&format!(
                                "N5c_ICE|locals-failed|peer={}|round={}|phase=answer-late|reason={e}",
                                peer_short(&peer.key),
                                peer.session_at_ms
                            ));
                            self.drop_session(idx, PeerIceState::Failed, "gather-failed");
                        }
                    }
                    let _ = self.pump_gather(idx, now_ms, pass_start_ms);
                } else {
                    let peer = &mut self.peers[idx];
                    peer.session.as_mut().expect("session checked").set_remote_credentials(creds.clone())?;
                    peer.remote_creds = Some(creds);
                }
            }
            PeerSignalKind::Candidate => {
                let cand = Candidate::unmarshal(payload)?;
                rx_extra = format!(
                    "|cand={}/{}:{}",
                    cand.typ.as_str(),
                    cand.address,
                    cand.port
                );
                let peer = &mut self.peers[idx];
                peer.remote_cands_seen += 1;
                match peer.session.as_mut() {
                    Some(session) => session.add_remote_candidate(cand),
                    None => peer.pending_remote.push(cand), // trickle 早到：暂存
                }
            }
        }
        self.maybe_start(idx, now_ms);
        let peer = &self.peers[idx];
        crate::hilog::emit(&format!(
            "N5c_ICE|signal-rx|kind={}|peer={}|round={}{rx_extra}",
            match kind {
                PeerSignalKind::Offer => "offer", PeerSignalKind::Answer => "answer",
                PeerSignalKind::Candidate => "candidate",
            },
            peer.key.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect::<String>(),
            peer.session_at_ms,
        ));
        // 收到该 peer 的信令 = 远端不静默：不再有「连续静默」计数（那套自造
        // 退避随 guard 节拍一起删除）。重谈判的节奏只由 guard 决定，而
        // guard 的复位条件是断连/网络事件（`guard.go:141-177`），不是收帧。
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
        // STAGE 0（官方对齐审计 §2.7）：整拍墙钟。入口取一次、出口与本拍
        // 注入时钟一起记一行——「一拍 ≤ X ms」这个断言此前没有量：真机
        // 报告（relay report §4.7-3）要的正是这条。生产 tick 是
        // [`crate::connector::ICE_PUMP_TICK_MS`]（10 ms），逐拍打印就是
        // 刷屏，所以只记**慢拍**（≥ [`TICK_SLOW_LOG_MS`]）：健康拍长 0~1 ms，
        // 慢拍恰好是"有东西阻塞了泵"的现场。
        let pass_start_ms = crate::sys::mono_ms();
        // signal 流重连（false→true 的边沿）= 上游 guard 的 sr-reconnect 事件
        // （`guard.go:155-160` `srReconnectedChan`：signal/relay 重连 → 新
        // `newReconnectTicker`，800 ms 起跳，立即首跳被消费）。断流期间本端
        // 可能只是收不到帧，重连后立刻给每个 peer 一次快速重发起的机会。
        if self.signal_reconnected {
            self.signal_reconnected = false;
            for idx in 0..self.peers.len() {
                self.guard_reset(idx, now_ms);
            }
        }
        let mut first_err: Option<ManagementError> = None;
        // STAGE 2/3（官方对齐审计 §2.7）：共享 mux 的**读循环**。上游把
        // `ReadFromConn` 跑在独立 goroutine 里（`universal.go:80-108`；设备路径
        // `device_kernel_unix.go:109` `go mux.ReadFromConn`）。stage 3 起生产由
        // mux 自己的**读线程**读（`start_reader`），此时这里绝不能再读——同一枚
        // fd 两个读端会互相偷包（STUN 被设备读走就再也回不到会话，WG 数据被这一
        // 拍读走也一样）。没有读线程时（注入时钟的 host harness、主机 CLI）本拍
        // 照旧承担读循环，语义与 stage 2 完全一致。
        if let Some(mux) = self.ice_mux() {
            if !mux.reader_running() {
                let read = mux.drain();
                if read > 0 {
                    let s = mux.stats();
                    crate::hilog::emit(&mux_stats_line(read, &s, false));
                }
            }
        }
        // 0.0.26 gap 1（真机 0.0.25 §4.3）：上面那一行是回退读循环的「这一拍
        // 读了多少」，生产路径（mux 自己的读线程）**一次都不打** → `xor_hit` /
        // `xor_miss` 在真机日志里不可观测。这里补一行**同一形状**的计数器
        // 快照，节奏与健康摘要同源（[`HEALTH_SUMMARY_MS`]，`periodic=1` 标明
        // 它不是某一拍的读结果）。只读 `stats()`，不 drain、不改任何状态。
        if self.last_mux_stats_ms == 0
            || now_ms.saturating_sub(self.last_mux_stats_ms) >= HEALTH_SUMMARY_MS
        {
            if let Some(mux) = self.ice_mux() {
                self.last_mux_stats_ms = now_ms;
                let s = mux.stats();
                crate::hilog::emit(&mux_stats_line(0, &s, true));
            }
        }
        // 壳层换过 mux（物理网络切换后重新 protect + feed）：仍骑旧 mux 的
        // 会话已经是死路径，拆掉重协商。
        self.drop_stale_mux_sessions(now_ms);
        let len = self.peers.len();
        for idx in 0..len {
            // STAGE 1（官方对齐审计 §2.7）：异步收集的**取件点**。每拍开头把
            // worker 已回报的接口 round 交给会话：候选 trickle 入 outbox，
            // 首次有候选时补 OFFER/ANSWER。非阻塞——worker 还没回报就是空
            // channel，本拍什么都不做（上游 `GatherCandidates()` 立即返回、
            // 下一行就 dial，`worker_ice.go:255-266`）。
            if let Err(e) = self.pump_gather(idx, now_ms, pass_start_ms) {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
            // 重连节拍（上游 guard，`guard.go:97-184`）：本拍到期就按状态决定
            // 要不要重新发起，并把下一跳排到 backoff 之后。**没有预算、没有游标**
            // ——每个 peer 一条自己的节拍，互不排队（审计 §2.7 stage 1 的
            // E1/E2/I7 随阻塞收集一起退休）。
            if let Err(e) = self.guard_tick(idx, now_ms, pass_start_ms) {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
            if let Err(e) = self.flush_outbox(idx) {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
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
            // 周期性健康摘要（§5 item 5）：每个有已 start 会话的 peer 每
            // [`HEALTH_SUMMARY_MS`] 一行 —— 检查收发/入站/候选对状态计数。
            // `pair_snapshots()` 自 N5b 起就存在而无人调用，真机
            // 2026-10-05 的日志完全没有 per-check 观测，这一行是下次
            // 判读 H1/H3/H4 的最低配证据。
            self.emit_health_summary(idx, now_ms);
        }
        // STAGE 0: exit side of the whole-tick wall clock (entry above). Only
        // a pass that spent real time is worth a line; `round=` is the injected
        // clock the pass ran on, `pass_ms=` the wall clock it actually cost.
        let pass_ms = crate::sys::mono_ms().saturating_sub(pass_start_ms);
        if pass_ms >= TICK_SLOW_LOG_MS {
            crate::hilog::emit(&format!(
                "N5c_ICE|tick|round={now_ms}|pass_ms={pass_ms}|peers={len}|slow=true"
            ));
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    // -----------------------------------------------------------------------
    // 上游 guard：重连节拍（`client/internal/peer/guard/guard.go:97-213`）
    // -----------------------------------------------------------------------

    /// 复位重连节拍 = 上游 `newReconnectTicker`（`guard.go:200-213`）：
    /// **新建**一个 `InitialInterval = 800ms` 的 backoff，并消费掉它创建时
    /// 的立即首跳（`guard.go:211` `<-ticker.C`），因此下一次发起落在
    /// 事件后一个（±10% 抖动的）800 ms。触发条件与上游一一对应：
    ///
    /// - ICE 断连：`onICEStateDisconnected` → `guard.SetICEConnDisconnected()`
    ///   （`conn.go:537-540`，且只在**状态真的从 Connected 变过去**时才发，
    ///   `conn.go:535-539`）→ `guard.go:148-153`；
    /// - signal（+relay）重连：`SRWatcher.onReconnected`（`sr_watcher.go:76-86`）
    ///   → `guard.go:155-160`；
    /// - 物理网络变化：`NetworkWatcher.Changed()` → `guard.go:162-177`；
    /// - relay 断连：`conn.go:663-667` → `guard.go:141-146`——本核心的 ICE
    ///   平面尚无 relay 传输（relay 行为本轮不动），故无对应调用点。
    fn guard_reset(&mut self, idx: usize, now_ms: u64) {
        let timeout = self.peers[idx].timeout_ms;
        let mut bo = guard_backoff(GUARD_RECONNECT_MS, timeout);
        let delay = bo
            .next_delay(&MonotonicClock, &mut *self.guard_rng)
            .map(delay_ms)
            .unwrap_or(GUARD_RECONNECT_MS);
        let peer = &mut self.peers[idx];
        peer.guard = bo;
        peer.guard_next_at_ms = now_ms.saturating_add(delay);
    }

    /// 一拍的重连节拍（上游 `Guard.reconnectLoopWithRetry` 的 tick 分支，
    /// `guard.go:118-139`）：到点即**先推进**下一跳（ticker 每跳都前进，与
    /// 回调是否真的发起无关），再按连接状态决定要不要发起。
    ///
    /// 状态判定是上游 `isConnectedOnAllWay`（`conn.go:834-848` → `evalConnStatus`）
    /// 在本架构的对应物：
    ///
    /// - **Connected**（`guard.go:127-128`）：ICE 已选中 pair → 什么都不做；
    /// - **进行中**：上游 `iceInProgress = workerICE.InProgress()`
    ///   （`conn.go:840-845`）= `agentConnecting`（`worker_ice.go:194-199`），
    ///   即「agent 已建、尚未连上」→ 也不发起。本架构的 agent 在**拿到远端
    ///   凭证**时才等价诞生（上游 `OnNewOffer` → `reCreateAgent` → `Dial`/`Accept`），
    ///   所以「有会话但还没有远端凭证」正是上游的「没有 agent」→ 继续发起；
    /// - **Disconnected**（`guard.go:129-130`）→ 回调 `conn.onGuardEvent`
    ///   （`conn.go:688-692`）→ `handshaker.SendOffer()`：重新广播本 peer 的
    ///   OFFER（凭证 per-peer 稳定，`handshaker.go:232-237`）。
    ///
    /// `PartiallyConnected` 的「3 次后转每小时」（`guard.go:131-138`）只适用于
    /// 「relay 通、ICE 不通」，本核心无 relay 传输，故整支不搬。
    ///
    /// 发起失败（收集无 mux / 无可绑定候选）按既有错误面照常上报：上游的
    /// `connect()` 失败只 `closeAgent` + 日志（goroutine 无调用方可返回），
    /// 本仓的 pump 错误面是对等物（connector 记日志），重试由节拍的下一跳
    /// 负责（`worker_ice.go:257-261` 的 closeAgent 语义）。
    fn guard_tick(&mut self, idx: usize, now_ms: u64, pass_start_ms: u64) -> Result<(), ManagementError> {
        if now_ms < self.peers[idx].guard_next_at_ms {
            return Ok(());
        }
        // 每跳都推进（上游 ticker 的 send → NextBackOff 顺序，`ticker.go`）。
        let delay = self.peers[idx]
            .guard
            .next_delay(&MonotonicClock, &mut *self.guard_rng);
        if let Some(d) = delay {
            let at = now_ms.saturating_add(delay_ms(d));
            self.peers[idx].guard_next_at_ms = at;
        }
        // 上游 `SendOffer` 在 signal 未就绪时直接 `ErrSignalIsNotReady`
        // （`handshaker.go:207-209`）：这一跳照样被消费掉，帧不发。本架构把
        // 未就绪的帧留在 outbox（显式失败，不静默丢），但节拍照样前进。
        if !self.signal_ready {
            return Ok(());
        }
        let connected = self.peers[idx].state == PeerIceState::Connected;
        // 上游 `InProgress()`：agent 存在且尚未连上。
        let in_progress = self.peers[idx].session.is_some()
            && self.peers[idx].remote_creds.is_some();
        if connected || in_progress {
            return Ok(());
        }
        // 本架构的纯应答方（`initiator=false`，仅测试用来钉住确定性角色划分）
        // 不主动发起——上游没有这个开关，两侧都会在未连上时发 OFFER。
        if !self.peers[idx].initiator {
            return Ok(());
        }
        self.guard_offer(idx, now_ms, pass_start_ms)
    }

    /// guard 的回调体（`conn.go:688-692` → `handshaker.SendOffer`）：
    ///
    /// - **没有会话** = 上游「没有 agent」：走完整发起（建会话 + 收集本地候选
    ///   + OFFER），这正是上游 `SendOffer` 之后对端回来 `OnNewOffer` 的起点；
    /// - **有会话但还没有远端凭证** = 上游「没有 agent、offer 还在路上」：
    ///   只**重发**本 peer 的 OFFER + 本会话已收集的候选。上游 `SendOffer`
    ///   只带凭证，因为它的 agent（连同候选 trickle）只在收到对端 Offer/Answer
    ///   时才诞生（`OnNewOffer` → `reCreateAgent` → `GatherCandidates`，
    ///   `worker_ice.go:100-157`）；本架构的 agent 在发起时就诞生、候选也已
    ///   收集，所以「等价的重广播」把同一批候选一起带上——否则对端在长时间
    ///   静默后第一次收到 OFFER 时手里仍没有本端候选，只能靠 prflx 回退。
    ///   凭证本身**不变**：它是 per-peer 的（`worker_ice.go:91-97`）。
    fn guard_offer(&mut self, idx: usize, now_ms: u64, pass_start_ms: u64) -> Result<(), ManagementError> {
        if self.peers[idx].session.is_none() {
            if let Err(e) = self.start_locals(idx, now_ms, pass_start_ms, "initiate") {
                // 收集失败 = 本次发起失败：半成品会话整段回滚（`session=None`），
                // 下一跳 guard 节拍再试（上游 `connect()` 的
                // `GatherCandidates` 出错 → `closeAgent`，`worker_ice.go:257-261`）。
                let peer = &mut self.peers[idx];
                let round = peer.session_at_ms;
                let peer_prefix = peer_short(&peer.key);
                peer.last_error = Some(ErrorClass::from_management(&e));
                crate::hilog::emit(&format!(
                    "N5c_ICE|locals-failed|peer={peer_prefix}|round={round}|phase=initiate|reason={e}"
                ));
                self.drop_session(idx, PeerIceState::Failed, "gather-failed");
                return Err(e);
            } else {
                // 内联执行者（host harness）在同一拍里跑完 round：立刻取件，
                // 保住「一拍内候选 + OFFER 就位」的既有语义。
                let _ = self.pump_gather(idx, now_ms, pass_start_ms);
            }
            return Ok(());
        }
        self.readvertise(idx);
        Ok(())
    }

    /// 重发本 peer 的 OFFER 与本会话已收集的本地候选（`outbox` 里去重：同一帧
    /// 还在排队时不再入列，避免 signal 未注册期间同一 OFFER 堆叠）。
    fn readvertise(&mut self, idx: usize) {
        if !self.peers[idx].locals_done {
            // 本地候选还没就绪：`on_locals_ready` 会把 OFFER 一起入列，这里不抢。
            return;
        }
        let Some(creds) = self.peers[idx].local_creds.clone() else { return };
        let cands: Vec<String> = self.peers[idx]
            .session
            .as_ref()
            .map(|s| s.local_candidates().iter().map(|c| c.marshal()).collect())
            .unwrap_or_default();
        let peer = &mut self.peers[idx];
        let offer = (PeerSignalKind::Offer, format!("{}:{}", creds.ufrag, creds.pwd));
        if !peer.outbox.contains(&offer) {
            peer.outbox.push_back(offer);
        }
        for payload in cands {
            let frame = (PeerSignalKind::Candidate, payload);
            if !peer.outbox.contains(&frame) {
                peer.outbox.push_back(frame);
            }
        }
    }


    /// 开始本地候选收集 + 会话绑定（发起、应答、迟到 ANSWER 三条路径共用；
    /// 幂等：`locals_done` 已置位 = `Gathered`，本轮已在跑 = `InFlight`）。
    ///
    /// STAGE 2（官方对齐审计 §2.7）：本地 host 候选**当拍就算出来交给会话**，
    /// 不再取任何 socket、不再等 STUN —— 上游
    /// `gatherCandidatesLocalUDPMux`（fork `gather.go:331-405`）就是
    /// `udpMux.GetListenAddresses()` × 同一个 mux 端口，一个候选一枚，
    /// 立刻 `addCandidate`。srflx 的 STUN 交换交给 worker（每个 STUN 服务器
    /// 一枚，共享 mux 的 in-flight 去重把 N 个 agent × N 个接口收敛成
    /// **每个服务器一轮**，`fork gather.go:491-513` + `universal.go:222-270`），
    /// 结果由 [`Self::pump_gather`] 按到达顺序 trickle 进 outbox。
    ///
    /// 固定端口（HOST-ONLY，N12a）：端口落在**共享 mux** 上
    /// （[`Self::arm_mux`] 用 `fixed_local_port` bind 通配地址），候选仍是
    /// 「接口地址 × 该端口」；转发目的地址不可预知，通配绑定才使固定端口可达
    /// （上游同形：单一 `0.0.0.0:NET_PORT` 共享 socket）。
    fn start_locals(
        &mut self,
        idx: usize,
        now_ms: u64,
        pass_start_ms: u64,
        phase: &'static str,
    ) -> Result<LocalsRun, ManagementError> {
        if self.peers[idx].locals_done {
            return Ok(LocalsRun::Gathered);
        }
        if self.peers[idx].gather.is_some() {
            // 一轮收集已经在跑：不重开（上游同一 agent 只 gather 一次，
            // `gather.go:41-46` `ErrMultipleGatherAttempted`）。
            return Ok(LocalsRun::InFlight);
        }
        self.ensure_session(idx, now_ms, phase)?;
        // 整个 ICE 平面唯一那枚 socket：壳层 feed 的，或主机/测试路径自建的。
        let mux = self.arm_mux()?;
        let cfg = GatherConfig {
            blacklist: &self.blacklist,
            servers: &self.stuns,
            timeout_ms: DEFAULT_STUN_TIMEOUT_MS,
        };
        let ifaces = crate::ice::allowed_ifaces(&cfg, self.ifaces.as_ref())?;
        if ifaces.is_empty() {
            return Err(ManagementError::Network("ice:no-bindable-local-candidate".into()));
        }
        // host 候选：接口地址 × mux 端口，立刻交给会话并立刻 trickle 出去。
        // 只有**主动发起**这一条路径欠 OFFER：迟到 ANSWER 只是上游
        // `OnNewOffer(answer)`（`worker_ice.go:100-157` 的 Dial/Accept），它
        // **不回** OFFER（见 `handle_signal` 的 Answer 分支）。
        let offer_owed = phase == "initiate";
        let port = mux.local_port();
        let hosts = crate::ice::mux_host_candidates(&ifaces, port);
        let mut added = 0usize;
        {
            let peer = &mut self.peers[idx];
            let session = peer.session.as_mut().expect("session just ensured");
            for cand in hosts {
                match session.add_local_candidate_mux(cand, mux.clone()) {
                    Ok(bound) => {
                        peer.outbox.push_back((PeerSignalKind::Candidate, bound.marshal()));
                        peer.locals_signaled += 1;
                        added += 1;
                    }
                    Err(e) => {
                        crate::hilog::emit(&format!(
                            "N5c_ICE|locals-failed|peer={}|round={}|phase={phase}|reason={e}",
                            peer_short(&peer.key),
                            peer.session_at_ms
                        ));
                        return Err(e);
                    }
                }
            }
        }
        if added == 0 {
            return Err(ManagementError::Network("ice:no-bindable-local-candidate".into()));
        }
        // 上游「gather 返回 → 下一行 dial」：候选就绪当拍就补 OFFER/ANSWER 与
        // `locals-gathered` 观测行（`pump_gather` 之后只会追加 srflx 候选）。
        self.on_locals_ready(idx, now_ms, pass_start_ms, offer_owed);
        // srflx：每个 STUN 服务器一枚 worker（没有 STUN 服务器 = 无事可做）。
        if self.stuns.is_empty() {
            return Ok(LocalsRun::Gathered);
        }
        let (tx, rx) = std::sync::mpsc::channel::<crate::ice::SrflxGather>();
        let mut pending = 0usize;
        for server in self.stuns.clone() {
            let tx = tx.clone();
            let socks = self.socks.clone();
            let mux = mux.clone();
            let spawned = self.exec.spawn(Box::new(move || {
                let servers = [server];
                let round = crate::ice::gather_srflx_round(
                    &servers,
                    mux.as_ref(),
                    socks.as_ref(),
                    DEFAULT_STUN_TIMEOUT_MS,
                );
                // send 失败 = 接收端已丢（会话被拆/peer 被移除）：本阶段 worker
                // 不持有任何 fd（socket 是 mux 的），直接退出即可。
                let _ = tx.send(round);
            }));
            if spawned {
                pending += 1;
            }
        }
        drop(tx); // 只剩 worker 的发送端
        if pending == 0 {
            // srflx worker 一枚都起不来（线程上限）：host 候选已经在会话里，
            // 只是本轮没有 srflx —— 不是失败（上游同样的情形只是少一类候选）。
            crate::hilog::emit(&format!(
                "N5c_ICE|gather-srflx|peer={}|round={}|spawned=0|outcome=no-worker",
                peer_short(&self.peers[idx].key),
                self.peers[idx].session_at_ms
            ));
            return Ok(LocalsRun::Gathered);
        }
        self.peers[idx].gather = Some(GatherInFlight {
            rx,
            pending,
            started_wall_ms: crate::sys::mono_ms(),
            added,
            srflx: 0,
            errors: Vec::new(),
            phase,
        });
        Ok(LocalsRun::InFlight)
    }

    /// 建会话（三条路径共用：收到 OFFER 应答、主动发起、迟到 ANSWER 采纳）。
    /// 纯内存：`IceSession::new` + 凭证（**每 peer 一次**），不取 socket、不发 STUN。
    ///
    /// 凭证是 per-peer 的：第一次建会话时抽定后写进
    /// [`PeerConn::local_creds`]，此后每次重建会话都复用同一对——上游
    /// `WorkerICE.localUfrag/localPwd` 在 worker 构造时生成一次
    /// （`worker_ice.go:91-97`），`reCreateAgent` 每次都用它
    /// （`worker_ice.go:220-226`）。
    fn ensure_session(
        &mut self,
        idx: usize,
        now_ms: u64,
        phase: &'static str,
    ) -> Result<(), ManagementError> {
        if self.peers[idx].session.is_some() {
            return Ok(());
        }
        let creds = match self.peers[idx].local_creds.clone() {
            Some(c) => c,
            None => {
                let c = IceCredentials::generate()?;
                self.peers[idx].local_creds = Some(c.clone());
                c
            }
        };
        // 角色按官方 isController 公钥比较确定，与 Offer/Answer 方向无关；
        // 本端公钥未注入时回退旧规则（发起方 controlling、应答方 controlled）。
        let fallback = match phase {
            // 主动发起（含迟到 ANSWER 采纳后补发 OFFER）＝我们是 offerer。
            "initiate" | "answer-late" => true,
            _ => false,
        };
        let rule = if self.local_key.is_some() {
            "key-cmp"
        } else if fallback {
            "legacy-initiator"
        } else {
            "legacy-answerer"
        };
        let remote_key = self.peers[idx].key.clone();
        let controlling = session_role(self.local_key.as_deref(), &remote_key, fallback);
        crate::hilog::emit(&format!(
            "N5c_ICE|session|peer={}|round={}|role={}|rule={rule}",
            peer_short(&self.peers[idx].key),
            now_ms,
            if controlling { "controlling" } else { "controlled" },
        ));
        let mut session = IceSession::new(creds.clone(), controlling, self.tie_breaker)?;
        // N2 v4.1 P7: the session's outer-source receipts name THIS peer.
        session.set_peer_label(&peer_short(&self.peers[idx].key));
        let peer = &mut self.peers[idx];
        peer.session = Some(session);
        peer.session_at_ms = now_ms;
        Ok(())
    }

    /// STAGE 2 的**取件点**：把 worker 已回报的 srflx round 交给会话，非阻塞
    /// （`try_recv`，队列空即返回）。srflx 候选**按到达顺序**入 outbox
    /// （trickle，上游 `onICECandidate` 每收集到一个候选就发一帧，
    /// `worker_ice.go:420-470`）；全部 round 回报完毕即收尾（`finish_gather`）。
    ///
    /// 不再返回硬错误：host 候选与 OFFER 已在 [`Self::start_locals`] 当拍就位，
    /// 一个 STUN 服务器失败只是少一个 srflx 候选（上游 `gather.go:526-531`
    /// 同样只记一条 warning）。
    fn pump_gather(
        &mut self,
        idx: usize,
        _now_ms: u64,
        _pass_start_ms: u64,
    ) -> Result<(), ManagementError> {
        let mut rounds: Vec<crate::ice::SrflxGather> = Vec::new();
        if let Some(g) = self.peers[idx].gather.as_ref() {
            while let Ok(round) = g.rx.try_recv() {
                rounds.push(round);
            }
        }
        for round in rounds {
            // 本轮 srflx 候选要用的 (接口地址 × mux 端口) 清单：在借用 peer 之前
            // 先算好（它是纯读：枚举 + 黑名单 + 去重）。
            let listen_addrs = {
                let port = self.ice_mux().map(|m| m.local_port()).unwrap_or(0);
                let ifaces = crate::ice::allowed_ifaces(
                    &GatherConfig {
                        blacklist: &self.blacklist,
                        servers: &self.stuns,
                        timeout_ms: DEFAULT_STUN_TIMEOUT_MS,
                    },
                    self.ifaces.as_ref(),
                )
                .unwrap_or_default();
                (ifaces, port)
            };
            {
                let peer = &mut self.peers[idx];
                let Some(g) = peer.gather.as_mut() else { break };
                g.pending = g.pending.saturating_sub(1);
                // 会话在这轮收集期间被拆/重建 = 这一轮的 srflx 属于上一次协商，
                // 绝不塞进新会话。worker 不持有 fd，丢弃无副作用。
                let deliver = peer.session.is_some();
                // STAGE 0 观测面的替换：`gather-stun` 的单位从「接口 round」
                // 变成「(mux, STUN server) 交换」——与上游一致
                // （fork `gather.go:491-513` 的并发单位、`:526-531` 的失败日志
                // 单位），并且现在**每个服务器一次**而不是每接口一次。
                for ex in &round.exchanges {
                    let server = format!("{}:{}", ex.server.host, ex.server.port);
                    match &ex.outcome {
                        Ok((addr, port)) => crate::hilog::emit(&format!(
                            "N5c_ICE|gather-stun|iface=mux|server={server}|ms={}|outcome=mapped|mapped={}:{}",
                            ex.ms,
                            std::net::Ipv4Addr::from(*addr),
                            port
                        )),
                        Err(e) => crate::hilog::emit(&format!(
                            "N5c_ICE|gather-stun|iface=mux|server={server}|ms={}|outcome=failed|reason={e}",
                            ex.ms
                        )),
                    }
                }
                if deliver {
                    let (ifaces, port) = &listen_addrs;
                    for ex in &round.exchanges {
                        let Ok(mapped) = ex.outcome else { continue };
                        g.srflx += 1;
                        for cand in crate::ice::srflx_candidates_for_listen_addrs(ifaces, *port, mapped) {
                            peer.outbox.push_back((PeerSignalKind::Candidate, cand.marshal()));
                        }
                    }
                }
                g.errors.extend(round.errors);
            }
            let done = {
                let peer = &self.peers[idx];
                peer.gather.as_ref().map(|g| g.pending == 0).unwrap_or(true)
            };
            if done {
                self.finish_gather(idx);
            }
        }
        Ok(())
    }

    /// 首次拿到本地候选 = 「本地候选就绪」（`locals_done`）：与旧同步
    /// `ensure_locals` 的尾部同序 —— 显式对外候选 → `locals-gathered` 观测行
    /// → 补 OFFER（`offer_owed`）/ANSWER（`answer_owed`）→ `maybe_start`。后续
    /// 接口 round 的候选只是 trickle 追加（上游：dial 不等 gather 收完，候选在
    /// 协商中陆续补进来）。
    fn on_locals_ready(
        &mut self,
        idx: usize,
        now_ms: u64,
        pass_start_ms: u64,
        offer_owed: bool,
    ) {
        let (gather_ms, added, srflx_count) = match self.peers[idx].gather.as_ref() {
            Some(g) => (
                crate::sys::mono_ms().saturating_sub(g.started_wall_ms),
                g.added,
                g.srflx,
            ),
            None => (0, self.peers[idx].locals_signaled, 0),
        };
        let pass_ms = crate::sys::mono_ms().saturating_sub(pass_start_ms);
        {
            let peer = &mut self.peers[idx];
            peer.locals_done = true;
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
            // interface enumeration starved, not the peer. §5 item 3: also
            // list the ACTUAL bound candidate addresses (ip:port, bounded to
            // the first 4) — the count alone couldn't answer "which port is
            // the peer seeing".
            let bound_addrs: Vec<String> = peer
                .session
                .as_ref()
                .map(|session| {
                    session
                        .local_candidates()
                        .iter()
                        .take(4)
                        .map(|c| format!("{}:{}", c.address, c.port))
                        .collect()
                })
                .unwrap_or_default();
            crate::hilog::emit(&format!(
                "N5c_ICE|locals-gathered|peer={}|round={}|added={added}|srflx={srflx_count}|advertised={}|cands={}|gather_ms={gather_ms}|pass_ms={pass_ms}",
                peer.key.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect::<String>(),
                peer.session_at_ms,
                self.advertised_candidates.len(),
                bound_addrs.join(","),
            ));
            // 主动发起：候选就绪即入列 OFFER——上游 `GatherCandidates()` 后
            // 立刻 dial 的等价物（`worker_ice.go:255-266`）。判据是**本次收集
            // 的 phase**（`offer_owed`），不是「本 peer 曾经发起过」：应答会话
            // （对端 OFFER 触发的建会话）不能多发 OFFER。
            if offer_owed {
                if let Some(creds) = peer.local_creds.as_ref() {
                    let payload = format!("{}:{}", creds.ufrag, creds.pwd);
                    peer.outbox.push_back((PeerSignalKind::Offer, payload));
                }
            }
            // 应答方（收到 OFFER 建立会话）：候选就绪即补 ANSWER——上游
            // handshaker 收到 Offer 后无条件 `sendAnswer()`（handshaker.go:
            // 136），与候选收集无关。
            if peer.answer_owed {
                if let Some(creds) = peer.local_creds.as_ref() {
                    let payload = format!("{}:{}", creds.ufrag, creds.pwd);
                    peer.outbox.push_back((PeerSignalKind::Answer, payload));
                }
                peer.answer_owed = false;
            }
        }
        self.maybe_start(idx, now_ms);
    }

    /// 整轮 srflx 收集回报完毕（`pending == 0`）的收尾：只记账，不拆会话。
    ///
    /// STAGE 2 起这里**不再有 fail-closed 回滚**：本地 host 候选在
    /// [`Self::start_locals`] 里已经绑定进会话、已经 trickle 出去、已经补了
    /// OFFER/ANSWER，一个 STUN 服务器失败（或所有 STUN 服务器都失败）只是少一类
    /// 候选 —— 上游 `gather.go:526-531` 同样只记一条 warning 并继续
    /// （host 候选与连通性检查不受影响，审计 §2.1 末尾）。
    fn finish_gather(&mut self, idx: usize) -> Option<ManagementError> {
        let (added, srflx, errors, phase) = match self.peers[idx].gather.take() {
            Some(g) => (g.added, g.srflx, g.errors, g.phase),
            None => return None,
        };
        let peer = &mut self.peers[idx];
        crate::hilog::emit(&format!(
            "N5c_ICE|locals-gathered|phase=done|peer={}|round={}|added={added}|srflx={srflx}|errors={}|source={phase}",
            peer_short(&peer.key),
            peer.session_at_ms,
            errors.len(),
        ));
        // 失败原因进 `last_status`-级观测（不改变会话、不冷却）。
        for (ctx, e) in &errors {
            crate::hilog::emit(&format!(
                "N5c_ICE|gather-srflx-failed|peer={}|ctx={ctx}|reason={e}",
                peer_short(&peer.key)
            ));
        }
        None
    }

    /// outbox FIFO 刷给 signal seam；失败即停（剩余帧下一拍重试——上游
    /// `SendOffer` 的 `ErrSignalIsNotReady` 会把这一跳的 OFFER 直接丢掉
    /// （`handshaker.go:207-209`），本架构显式失败并把帧留在队列里，重发
    /// 由 guard 的下一跳负责）。
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
            self.peers[idx].outbox.pop_front();
        }
    }

    /// 会话启动前置：远端凭证 + 本地候选。
    ///
    /// **不需要远端候选**：上游的 agent 拿到远端凭证就进 Checking
    /// （`agent.go:328-351`），哪怕 checklist 是空的（`agent.go:480-483`
    /// "Failed to ping without candidate pairs"）；迟到 trickle 的远端候选会形成
    /// 新 pair 并在同一段 Checking 预算里被探测。这个「等一个远端候选才敢 start」
    /// 的门曾经让「对端已应答但零候选」的会话永远不 start、没有任何终态信号
    /// （真机 round-230622244），也正是曾经的 9 s `START_DEADLINE_MS`
    /// （已删除）当初被造出来的原因；上游没有这个门，所以这里也不留。
    fn maybe_start(&mut self, idx: usize, _now_ms: u64) {
        if self.peers[idx].started || self.peers[idx].session.is_none() {
            return;
        }
        if self.peers[idx].remote_creds.is_none() || !self.peers[idx].locals_done {
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
                match session.start() {
                    Ok(()) => peer.started = true,
                    Err(e) => {
                        // 0048：`start()` 失败此前被 `is_ok()` 静默吞掉，现场
                        // 完全没有可观测面——至少记一行稳定 token（含原因）。
                        peer.last_error = Some(ErrorClass::from_management(&e));
                        crate::hilog::emit(&format!(
                            "N5c_ICE|start-refused|peer={}|round={}|reason={e}",
                            peer_short(&peer.key),
                            peer.session_at_ms
                        ));
                    }
                }
            }
        }
        if self.peers[idx].started
            && matches!(
                self.peers[idx].state,
                PeerIceState::Idle | PeerIceState::Gathering | PeerIceState::Failed
            )
        {
            // Failed→Checking：拆除后的重谈判也要如实回到 Checking，
            // 否则状态永远停在上一会话的 Failed（观测面失真）。
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
                        // 0.0.26 gap 4：提名证据**先打**、且**无条件**打——
                        // 「选中了」是这一事件本身的事实，落配成功/失败是它
                        // 后面的一步；0.0.25 只在落配成功分支里打，于是
                        // 「提名了但 attach/落配失败」的对端在日志里完全没有
                        // 提名痕迹。`endpoint_changed` 与上一次**真的落配成功**
                        // 的地址比较（`last_applied_endpoint` 刻意跨会话保留），
                        // 所以「物理网络换代后沿用了同一个远端地址」这条路径
                        // 也有独立一行（真机 0.0.25 §5.4 的 `HkPMIwdu`）。
                        let endpoint_changed = peer.last_applied_endpoint != Some((addr, remote.port));
                        crate::hilog::emit(&selected_pair_line(addr, remote.port, endpoint_changed));
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
                                peer.last_applied_endpoint = Some((addr, remote.port));
                                // 本地侧 egress 证据（公开运行材料：ip+port）
                                let local = peer
                                    .session
                                    .as_ref()
                                    .and_then(|s| s.selected_pair())
                                    .map(|(l, _)| l);
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
                    //
                    // 会话本身也随之拆掉：上游 pion 报 Disconnected 时
                    // `onConnectionStateChange` 立刻 `closeAgent`
                    // （worker_ice.go:591-600，`agentConnecting` 随之清零），
                    // 紧接着 `onICEStateDisconnected` 复位 guard 节拍——但只在
                    // **真的从 Connected 变过来**时（conn.go:535-539
                    // `changed := conn.statusICE.Get() != StatusDisconnected`），
                    // 于是事件后约 800 ms 的那一跳就会重新发 OFFER
                    // （`guard.go:148-153` → `:129-130` → `conn.go:688-692`）。
                    let from_connected =
                        self.peers[idx].state == PeerIceState::Connected;
                    let peer = &mut self.peers[idx];
                    peer.state = PeerIceState::Disconnected;
                    peer.endpoint_applied = false;
                    self.wg.recycle_endpoint(&peer.key);
                    crate::hilog::emit("N11_ICE|disconnected|endpoint-recycled");
                    drop(peer);
                    self.drop_session(idx, PeerIceState::Disconnected, "ice-disconnected");
                    if from_connected {
                        self.guard_reset(idx, now_ms);
                    }
                }
                IceEvent::Failed(reason) => {
                    // 无 relay 的本增量：Failed → peer 不可达（绝不静默
                    // 当作可用；默认路由闸经 summary().reachable 生效）。
                    // N11：回收 WG 端点与 egress，并拆除死会话（关闭其
                    // socket dup = 死路径 fail-closed）；重谈判由 guard 节拍
                    // 驱动（上游 agent Failed → closeAgent，worker_ice.go:591-600）。
                    self.drop_session(idx, PeerIceState::Failed, &reason);
                }
                IceEvent::Closed => {}
                IceEvent::CheckSucceeded { remote, rtt_ms, .. } => {
                    // 修复前这里是 `=> {}`（§1.3：最大的观测盲区——整份真机
                    // 日志无法区分"检查从未发出"还是"发出未回应"）。
                    let peer = &mut self.peers[idx];
                    crate::hilog::emit(&format!(
                        "N5c_ICE|check-ok|peer={}|round={}|remote={}:{}|rtt_ms={rtt_ms}",
                        peer_short(&peer.key),
                        peer.session_at_ms,
                        remote.address,
                        remote.port
                    ));
                }
                // 本地候选的最终形态已在 start_locals/pump_gather 里直接入
                // outbox（本事件的载荷只是同一事实的重复面）。
                IceEvent::LocalCandidateReady(_) => {}
            }
        }
    }

    /// 拆掉当前会话（上游 `closeAgent`）：peer 不可达、回收 WG 端点与 egress、
    /// Drop 关闭会话的全部 dup socket、清空在途信令状态。
    ///
    /// 对应上游 `WorkerICE.closeAgent`（`worker_ice.go:354-373`）的调用点：
    /// `connect` 里 gather/dial 失败（`:257-261`、`:270-275`）、
    /// `onConnectionStateChange` 的 Failed/Disconnected/Closed
    /// （`:591-600`）。**本地凭证不在拆除范围**：`localUfrag/localPwd` 是 per-peer
    /// 的（`worker_ice.go:91-97`），会话重建后照旧广播同一对。
    ///
    /// `state` 由调用方给出：路径死了（Disconnected）还是谈判失败了（Failed）——
    /// 两者对 `summary()/status()` 都意味着不可达，但观测面上不同。
    /// `reason` 只进日志（公开诊断材料）。
    fn drop_session(&mut self, idx: usize, state: PeerIceState, reason: &str) {
        let peer = &mut self.peers[idx];
        let round = peer.session_at_ms;
        let peer_prefix = peer_short(&peer.key);
        peer.state = state;
        peer.endpoint_applied = false;
        peer.selected_remote = None;
        self.wg.recycle_endpoint(&peer.key);
        peer.session = None; // Drop 关闭该会话全部 dup socket
        // 在跑的收集属于这个死会话：丢接收端（worker 收尾时自关其 dup），
        // 免得它的候选落进下一次协商。
        peer.gather = None;
        peer.remote_creds = None;
        peer.locals_done = false;
        peer.started = false;
        peer.remote_cands_seen = 0;
        peer.pending_remote.clear();
        peer.outbox.clear();
        peer.answer_owed = false;
        peer.session_at_ms = 0;
        peer.last_health_ms = 0;
        // The payload carries WHY the session died — device run 3
        // needed exactly this and the marker alone said nothing.
        crate::hilog::emit(&format!(
            "N11_ICE|failed|session-dropped|renegotiation-armed|reason={reason}|peer={peer_prefix}|round={round}"
        ));
    }

    /// 周期性健康摘要（§5 item 5，[`HEALTH_SUMMARY_MS`]）：一行包含
    /// 检查收发计数（N5b 计数器）与候选对状态分布，会话进入终态
    /// （Connected/拆除）后自然停发。
    fn emit_health_summary(&mut self, idx: usize, now_ms: u64) {
        let due = {
            let p = &self.peers[idx];
            p.session.is_some()
                && p.started
                && (p.last_health_ms == 0
                    || now_ms.saturating_sub(p.last_health_ms) >= HEALTH_SUMMARY_MS)
        };
        if !due {
            return;
        }
        let peer = &mut self.peers[idx];
        peer.last_health_ms = now_ms;
        let Some(session) = peer.session.as_ref() else { return };
        let h = session.health();
        let mut counts = [0usize; 5]; // waiting / in-progress / succeeded / failed / other
        for snap in session.pair_snapshots() {
            match snap.state {
                PairState::Waiting => counts[0] += 1,
                PairState::InProgress => counts[1] += 1,
                PairState::Succeeded => counts[2] += 1,
                PairState::Failed => counts[3] += 1,
                PairState::Frozen => counts[4] += 1,
            }
        }
        crate::hilog::emit(&format!(
            "N5c_ICE|health|peer={}|round={}|age_ms={}|role={}|checks_tx={}|checks_ok={}|reqs_rx={}|reqs_ok={}|pairs={}/{}/{}/{}/{}|nom={}",
            peer_short(&peer.key),
            peer.session_at_ms,
            now_ms.saturating_sub(peer.session_at_ms),
            if session.is_controlling() { "ctrl" } else { "ctrl-d" },
            h.checks_sent,
            h.checks_matched,
            h.inbound_reqs,
            h.inbound_ok,
            counts[0],
            counts[1],
            counts[2],
            counts[3],
            counts[4],
            session.selected_pair().is_some() as u8,
        ));
    }

    /// 物理网络变化（外层 socket 重建 + 重新 protect/feed）：拆掉所有还骑着
    /// 旧路径的会话，并把每个 peer 的重连节拍复位到上游的 `newReconnectTicker`
    /// —— `guard.go:162-177` 的 netChanged 分支（`netWatcher.Changed()`：
    /// 「network is back, reset reconnection ticker」→ 800 ms 起跳）；于是下一次
    /// 发起落在切换后约 800 ms，而不是等当前退避档走完。
    ///
    /// 会话本身必须拆：本端接口/端口全变，旧会话的候选端口已经不存在。
    /// 本地凭证**不变**（per-peer，`worker_ice.go:91-97`）。
    pub fn restart_for_network_change(&mut self, now_ms: u64) {
        for idx in 0..self.peers.len() {
            self.drop_session(idx, PeerIceState::Failed, "physical-network-change");
            self.guard_reset(idx, now_ms);
        }
    }

    /// N2 T5a（§4 B 列 I 行）：端点定向断链。每个有会话/已选端点的 peer 都被
    /// 拆除当前会话（`drop_session`：`peer.session = None` 由 `IceSession`
    /// 的 `Drop` 关闭其全部 dup socket），并把重连节拍复位到 800 ms，使断开后
    /// 约 800 ms 就重新发起（guard 的 ICE-disconnected 语义）。
    /// 返回实际被处理的 peer 数（0 = 无可断的会话 → `no-target`）。
    pub fn break_ice(&mut self, now_ms: u64) -> usize {
        let mut acted = 0;
        for idx in 0..self.peers.len() {
            let live = self.peers[idx].session.is_some() || self.peers[idx].selected_remote.is_some();
            if !live {
                continue;
            }
            self.drop_session(idx, PeerIceState::Failed, "n2-stimulus-I");
            self.guard_reset(idx, now_ms);
            acted += 1;
        }
        acted
    }

    /// 停止全部会话（关闭各自 dup socket）；connector stop() 调用。
    pub fn stop_all(&mut self) {
        for p in self.peers.iter_mut() {
            if let Some(s) = p.session.as_mut() {
                s.stop();
            }
            // 在跑的收集（STAGE 1）随之作废：丢接收端 = 那些 worker 收尾时
            // 自行关闭它们的 socket，停止之后不会再有任何候选/日志从它们冒出。
            p.gather = None;
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

    /// STAGE 1 的 host harness 构造：确定性执行者。
    ///
    /// 这些用例用**注入时钟**驱动 pump（`run_once(now_ms)`）+ 断言同一拍内的
    /// 状态变化，而真实收集线程的完成时刻由墙钟决定——两者赛跑必然 flaky。
    /// [`InlineGatherExec`] 让 round 在 `start_locals` 里当场跑完、事件入队，
    /// 调用方紧接着 `pump_gather` 取件，语义与阶段 1 之前逐字一致。
    /// 异步模型本身由 `candidates_are_delivered_as_each_interface_round_finishes`
    /// 等用例用**默认的线程执行者**覆盖（见该用例）。
    fn test_orch(deps: PeerIceDeps) -> PeerIceOrchestrator {
        let mut orch = PeerIceOrchestrator::new(deps);
        orch.set_gather_exec(Arc::new(InlineGatherExec));
        orch
    }

    /// guard 节拍的定序熵源：恒返回构造时的样本。`0.5` 让 ±10% 抖动恰好取
    /// **裸间隔**（cenkalti/backoff `getRandomValueFromInterval`：delay =
    /// current×(1+f×(2r−1))），guard 序列因此可逐值断言 3 s / 6 s / 12 s /
    /// 24 s → 封顶（封顶值 = 该 peer 抽定的 timeout，`0.5` 时为 37.5 s）。
    /// 生产从不装它（默认 [`OsRandom`]）。
    struct GuardRng(f64);
    impl Rng for GuardRng {
        fn next_uniform(&mut self) -> f64 {
            self.0
        }
    }

    /// 装定序熵源——必须在 [`PeerIceOrchestrator::set_peers`]**之前**调用：
    /// `set_peers` 是每 peer timeout 的唯一抽取点（每个新 `PeerConn` 消耗
    /// 一个样本）。
    fn seed_guard(orch: &mut PeerIceOrchestrator) {
        orch.set_guard_rng(Box::new(GuardRng(0.5)));
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

    /// 广告的中继地址（connector sync 经 `set_relay_address` 写入）被
    /// 注入 OFFER 与 ANSWER 出帧、不注入 CANDIDATE（上游 offer/answer
    /// 经 handshaker.go:224-242 一律携带，candidate body 没有该字段，
    /// signaler.go:32-38）。
    #[test]
    fn real_exchange_stamps_relay_address_into_offer_and_answer_only() {
        let (exchange, mut rx) = RealSignalExchange::new();
        exchange.registered.store(true, Ordering::Release);
        exchange.set_relay_address(Some("rels://relay.example:28443".into()));

        for kind in [PeerSignalKind::Offer, PeerSignalKind::Answer] {
            exchange.send("REMOTEKEY", kind, "ufrag:pwd", 51_820).expect("queued");
            let out = rx.try_recv().expect("frame queued");
            assert_eq!(out.remote_key, "REMOTEKEY");
            assert_eq!(out.payload, "ufrag:pwd");
            assert_eq!(out.wg_listen_port, 51_820);
            assert_eq!(
                out.relay_server_address.as_deref(),
                Some("rels://relay.example:28443"),
                "{kind:?} must carry the advertised relay address"
            );
        }
        // candidate stays field-free (upstream candidate bodies never carry it)
        exchange
            .send("REMOTEKEY", PeerSignalKind::Candidate, "cand", 0)
            .expect("queued");
        let out = rx.try_recv().expect("frame queued");
        assert_eq!(out.relay_server_address, None, "candidate must stay field-free");
    }

    /// 回归：未广告（slot 为 `None`，含清除后）所有出帧不带中继字段——
    /// 无 relay 部署的字节形态与修复前一致。
    #[test]
    fn real_exchange_without_advertised_relay_keeps_bodies_field_free() {
        let (exchange, mut rx) = RealSignalExchange::new();
        exchange.registered.store(true, Ordering::Release);
        assert_eq!(exchange.relay_address(), None, "fresh exchange: nothing advertised");

        exchange.send("R", PeerSignalKind::Offer, "u:p", 1).expect("queued");
        assert_eq!(rx.try_recv().expect("frame").relay_server_address, None);

        // advertise, then management stops advertising → back to field-free
        exchange.set_relay_address(Some("rel://a.example:1".into()));
        exchange.send("R", PeerSignalKind::Answer, "u:p", 1).expect("queued");
        assert_eq!(
            rx.try_recv().expect("frame").relay_server_address,
            Some("rel://a.example:1".into())
        );
        exchange.set_relay_address(None);
        exchange.send("R", PeerSignalKind::Offer, "u:p", 1).expect("queued");
        assert_eq!(rx.try_recv().expect("frame").relay_server_address, None);
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
    fn guard_reoffers_a_silent_peer_on_the_backoff_until_connected() {
        // round-6 归因的回归钉子（guard 形态）：OFFER/候选交付成功后，对端
        // 一直没有回音（典型：对端尚未注册，signal 服务端 best-effort 丢弃
        // 转发、无任何错误面）时，编排层按 guard 节拍重发 OFFER（guard.go:
        // 129-130 Disconnected → conn.go:688-692 onGuardEvent → handshaker
        // SendOffer），直到该 peer Connected。凭证 per-peer 稳定
        // （worker_ice.go:91-97），所以每一跳重发的都是同一对凭证。
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
        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"));
        for _ in 0..4 {
            let fd = unsafe { socket(2, 2, 0) };
            assert!(fd >= 0, "socket() failed");
            socks.feed(fd);
        }

        let signal = Arc::new(RecordingSignal::default());
        let mut orch = test_orch(PeerIceDeps {
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
        seed_guard(&mut orch);
        orch.set_peers(&["P".into()]);
        orch.set_signal_ready(true);

        let offers = |signal: &Arc<RecordingSignal>| -> Vec<String> {
            signal
                .frames
                .lock()
                .expect("frames")
                .iter()
                .filter(|(_, k, _)| *k == PeerSignalKind::Offer)
                .map(|(_, _, p)| p.clone())
                .collect()
        };
        // 拍 1（t=1000）：guard 的保证首跳（guard_next=0，NewTicker 创建即发
        // 一次）——完整发起：会话 + 本地候选 + OFFER 交付 seam。
        orch.run_once(1000).expect("first beat converges the send");
        let first = offers(&signal);
        assert_eq!(first.len(), 1, "exactly one OFFER on the first hop");
        assert!(
            signal.frames.lock().expect("frames").iter().any(|(_, k, _)| *k == PeerSignalKind::Candidate),
            "the first hop carries the local candidates"
        );
        assert_eq!(orch.peers[0].timeout_ms, 37_500, "GuardRng(0.5) draws 30 s + 0.5×15 s");
        // 首跳的下一跳 = 裸间隔 3 s（GuardRng(0.5) → 抖动取裸间隔）。
        orch.run_once(3999).expect("beat 2");
        assert_eq!(offers(&signal).len(), 1, "no re-offer before the 3 s hop");
        orch.run_once(4000).expect("beat 3 fires the hop");
        assert_eq!(offers(&signal).len(), 2, "the guard hop re-offers");
        // 节拍按 ×2 增长：下一跳在 4000+6000。
        orch.run_once(9999).expect("beat 4");
        assert_eq!(offers(&signal).len(), 2, "no re-offer before the 6 s hop");
        orch.run_once(10_000).expect("beat 5 fires the second hop");
        assert_eq!(offers(&signal).len(), 3);
        // 12 s 之后的第三跳。
        orch.run_once(21_999).expect("beat 6");
        assert_eq!(offers(&signal).len(), 3, "no re-offer before the 12 s hop");
        orch.run_once(22_000).expect("beat 7 fires the third hop");
        assert_eq!(offers(&signal).len(), 4);
        // 每一跳重发的都是同一对凭证（per-peer 稳定，worker_ice.go:91-97）。
        let all = offers(&signal);
        assert!(
            all.windows(2).all(|w| w[0] == w[1]),
            "every re-offer carries the SAME stable credentials: {all:?}"
        );
    }

    #[test]
    fn set_peers_reconciles_add_remove_and_counts_unknown_signal() {
        let mut orch = test_orch(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![])),
            socks: Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source")),
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

    /// STAGE 2（官方对齐审计 §2.7）：**没有 mux = 没有本地候选 = fail-closed**。
    ///
    /// 旧语义（0050 §8 item 3）是「受保护 socket 池不足 = 等待，不是错误」——
    /// 那是因为每个接口 round 都要从池里取一枚。stage 2 起整个 ICE 平面只有
    /// 一枚共享 mux socket：壳层没 feed、宿主供给也空时，正确行为是**硬
    /// 失败**（绝不自开一枚无保护的 socket，governance §二.4），发起路径整段
    /// 回滚 + 冷却 + `locals-failed`。
    #[test]
    fn no_mux_fails_closed_instead_of_gathering_sockets() {
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
        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"));
        let mut orch = test_orch(PeerIceDeps {
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
        let err = orch.run_once(now).expect_err("no mux ⇒ a hard, fail-closed error");
        assert!(
            format!("{err}").contains("no-mux"),
            "the error must name the missing mux, got {err}"
        );
        {
            let p = &orch.peers[0];
            assert_eq!(p.session_at_ms, 0, "the half-built session is rolled back (0048)");
            assert!(!p.locals_done, "nothing may be signalled without a socket");
            assert_eq!(socks.taken(), 0, "no unprotected socket may be opened");
        }
        assert!(
            signal.frames.lock().expect("frames").is_empty(),
            "no frame may leave without candidates"
        );
        let st = orch.peer_status("P").expect("peer");
        assert_eq!(st.last_error, Some(ErrorClass::Network), "fail-closed is an error class");
        assert!(!ice_ready_for_default_route(&orch.summary()), "unreachable peer must not arm the gate");
        // guard 下一跳的失败重试仍走同一条 fail-closed 路径（绝不静默换成
        // 无保护 socket；收集失败不重置节拍——首跳的 3 s backoff 走完才再试）。
        now += GUARD_INITIAL_MS + 1_000;
        assert!(orch.run_once(now).is_err(), "still no mux, still fail-closed");
        assert_eq!(socks.taken(), 0);
    }

    /// 0052：默认路径 advertise 出去的 host 候选端口，必须由 **gather 绑定、
    /// STAGE 2（官方对齐审计 §2.7）：advertise 出去的 host 候选端口就是**共享
    /// mux 的端口**，而 mux 是会话**管不到**的：`session.stop()`（以及 Drop）只
    /// 注销自己在 mux 上的登记，绝不关闭那枚 fd —— 否则一个 peer 的会话结束就会
    /// 把整个 ICE 平面（其余 peer 的检查与数据）一起拆掉。上游同形
    /// （`udpmux/mux.go:284-321` 只删 map 条目，`Close()` 才是拥有者）。
    ///
    /// 旧形态（0052 修复）钉的是反面：会话**拥有**那枚 gather socket，stop 必须
    /// 把端口还回去。stage 2 起会话不再拥有 socket，所以断言反过来。
    #[test]
    fn mux_port_survives_the_session_that_advertised_it() {
        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"));
        {
            let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            assert!(fd >= 0, "socket() failed");
            socks.feed(fd).expect("feed");
            unsafe { sys::close(fd) };
        }
        let signal = Arc::new(FrameLog::default());
        let mut orch = test_orch(PeerIceDeps {
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
        orch.run_once(1000).expect("locals must converge");
        assert!(orch.peers[0].locals_done);

        let mux_port = orch.ice_mux().expect("the shared mux").local_port();
        // The candidate that was actually signaled.
        let frames = signal.frames.lock().expect("frames").clone();
        let cand = frames
            .iter()
            .filter(|(_, k, _)| *k == PeerSignalKind::Candidate)
            .map(|(_, _, p)| Candidate::unmarshal(p).expect("wire form"))
            .next()
            .expect("a candidate frame");
        assert_eq!(cand.port, mux_port, "the host candidate carries the MUX port");

        // The session's local candidate IS that signaled candidate; the session
        // owns no fd for it.
        let session = orch.peers[0].session.as_ref().expect("session");
        assert_eq!(session.local_candidates(), vec![cand.clone()]);
        assert_eq!(session.live_local_socket_count(), 0, "a mux candidate owns no fd");
        assert_eq!(session.shared_mux_fd().is_some(), true, "…it rides the shared mux");
        let sa = sys::sockaddr_in::new([127, 0, 0, 1], cand.port);
        let len = core::mem::size_of::<sys::sockaddr_in>() as u32;
        let probe = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
        assert!(probe >= 0, "socket() failed");
        assert_eq!(
            unsafe { sys::bind(probe, &sa, len) },
            -1,
            "the advertised port is held by the live shared mux"
        );
        unsafe { sys::close(probe) };
        // One interface, one peer: still exactly ONE protected socket for the
        // whole plane (the mux), not one per interface round.
        assert_eq!(socks.taken(), 1, "the whole ICE plane takes one socket");

        // stop() unregisters from the mux and closes NOTHING shared: the port
        // stays held by the mux (the next session — and every other peer —
        // keeps using it).
        orch.peers[0].session.as_mut().expect("session").stop();
        assert_eq!(
            orch.ice_mux().expect("mux").session_count(),
            0,
            "the stopped session left the mux registry"
        );
        let probe = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
        assert!(probe >= 0, "socket() failed");
        assert_eq!(
            unsafe { sys::bind(probe, &sa, len) },
            -1,
            "session stop must NOT release the shared mux port"
        );
        unsafe { sys::close(probe) };
        // …and the mux is still usable afterwards.
        assert!(orch.ice_mux().is_some());
    }

    /// 006: the resource counts must distinguish a RETAINED session OBJECT
    /// from the fds it still owns — `stop_all` keeps the object but closes
    /// every dup, so `live_local_sockets` drops to 0 while
    /// `sessions_present` stays > 0.
    #[test]
    fn resource_counts_track_retained_sessions_and_live_sockets() {
        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"));
        {
            let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            assert!(fd >= 0, "socket() failed");
            socks.feed(fd).expect("feed");
            unsafe { sys::close(fd) };
        }
        let mut orch = test_orch(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![crate::ice::InterfaceAddr {
                name: "eth0".into(),
                addr: [127, 0, 0, 1],
            }])),
            socks: socks.clone(),
            signal: Arc::new(LoggingSignalExchange::default()),
            wg: noop_wg(),
            tie_breaker: Some(7),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        });

        // No peer at all: every count is a real zero (measured, not unknown).
        assert_eq!(orch.resource_counts(), IceResourceCounts::default());

        orch.set_peers(&["P".into()]);
        orch.set_signal_ready(true);
        orch.run_once(1000).expect("locals must converge");
        assert!(orch.peers[0].locals_done);

        let live = orch.resource_counts();
        assert_eq!(live.sessions_present, 1, "the peer holds one session object");
        assert_eq!(live.sessions_stopped, 0, "nothing was stopped yet");
        // STAGE 2: a mux-backed session owns NO fd of its own — the shared mux
        // socket is the ICE plane's (counted once, not once per peer).
        assert_eq!(live.live_local_sockets, 0, "the session owns no socket: the mux does");
        assert!(
            orch.peers[0].session.as_ref().expect("session").shared_mux_fd().is_some(),
            "…but it does ride the one shared mux socket"
        );

        // stop_all: the OBJECT is retained by the peer entry, its fds are not.
        orch.stop_all();
        let stopped = orch.resource_counts();
        assert_eq!(stopped.sessions_present, 1, "stop_all retains the session object");
        assert_eq!(stopped.sessions_stopped, 1, "the retained session is stopped");
        assert_eq!(stopped.sessions_live, 0, "a stopped session is not live");
        assert_eq!(stopped.live_local_sockets, 0, "a stopped session owns no fd");
    }

    #[test]
    fn set_stuns_parses_netbird_config_uris() {
        let mut orch = test_orch(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![])),
            socks: Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source")),
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

    /// 无活动会话时收到的信令 —— 官方语义（审计 §1.1 E5 + 本阶段"follow
    /// upstream"）：
    ///
    /// - **CANDIDATE**（trickle 早到）：暂存不丢、不 panic、不建会话（上游
    ///   `OnRemoteCandidate` 在 agent 尚未建立时只记一条 warn 并丢弃，
    ///   `worker_ice.go:167-171`；本端暂存到会话建立后再喂，比上游更保守）。
    /// - **ANSWER**：上游**没有「丢帧」这一支**——引擎先 `ActivatePeer`
    ///   （engine.go:2088-2097），handshaker 把 Offer/Answer 一并交给 ICE
    ///   worker，worker 用这一帧新建 agent 并 Dial/Accept
    ///   （handshaker.go:140-156 / worker_ice.go:100-155）。因此这里**开启一轮
    ///   协商**（建会话 + 采纳远端凭证 + 收集本地候选进 Checking），**不回
    ///   OFFER**（remoteAnswerCh 分支没有 sendOffer），而不是丢弃计数；
    ///   `signal-no-session` 行随之删除（上游没有对应行）。
    #[test]
    fn signal_without_an_active_session_starts_negotiation_like_upstream() {
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
        let signal = Arc::new(RecordingSignal::default());
        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"));
        for _ in 0..8 {
            let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            assert!(fd >= 0, "socket() failed");
            socks.feed(fd).expect("feed");
            unsafe { sys::close(fd) };
        }
        let mut orch = test_orch(PeerIceDeps {
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
        // 早到的候选：暂存、不建会话、不 start
        let cand = Candidate::host_candidate([127, 0, 0, 1], 51820);
        orch.handle_signal("P", PeerSignalKind::Candidate, &cand.marshal(), 10).expect("buffered");
        assert_eq!(orch.peer_status("P").expect("peer").state, PeerIceState::Idle);
        assert!(orch.peers[0].session.is_none(), "a candidate alone must not open a session");
        assert_eq!(orch.peers[0].pending_remote.len(), 1, "the early candidate is held, not lost");
        // 无会话的 ANSWER：开启新协商（采纳凭证 + 收集本地候选），不计入
        // unknown_signal；**不回 OFFER**——上游 remoteAnswerCh 分支只把帧交给
        // ICE worker 去 Dial/Accept（handshaker.go:140-156），对端手里已有
        // 本端稳定的 ufrag:pwd（per-peer 凭证），无需任何「换新凭证重新
        // 发起」的自造绕道。
        orch.handle_signal("P", PeerSignalKind::Answer, "abcd:0123456789012345678901", 20)
            .expect("answer accepted");
        assert_eq!(orch.unknown_signal(), 0, "upstream has no drop-on-no-session branch");
        assert!(orch.peers[0].session.is_some(), "the late ANSWER starts a negotiation");
        assert!(
            orch.peers[0].started,
            "the held candidate plus the answer credentials start the checks"
        );
        assert_eq!(orch.peers[0].pending_remote.len(), 0, "the held candidate was handed over");
        orch.run_once(21).expect("beat");
        let frames = signal.frames.lock().expect("frames");
        assert!(
            frames.iter().any(|(k, kind, _)| k == "P" && *kind == PeerSignalKind::Candidate),
            "the local candidates trickle out: {frames:?}"
        );
        assert!(
            !frames.iter().any(|(k, kind, _)| k == "P" && *kind == PeerSignalKind::Offer),
            "an ANSWER is never answered with an OFFER (no re-offer workaround): {frames:?}"
        );
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
        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"));
        for _ in 0..2 {
            let fd = unsafe { sys::socket(2, 2, 0) };
            assert!(fd >= 0, "socket() failed");
            socks.feed(fd);
        }
        let signal = Arc::new(RecordingSignal::default());
        let mut orch = test_orch(PeerIceDeps {
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
        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"));
        let fd = unsafe { sys::socket(2, 2, 0) };
        assert!(fd >= 0);
        socks.feed(fd);
        let mut orch = test_orch(PeerIceDeps {
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
        // closeAgent 语义：半成品会话整段拆掉，观测面 = Failed（不是 Idle）。
        assert_eq!(orch.peer_status("P").expect("peer").state, PeerIceState::Failed);
        unsafe { sys::close(holder) };
    }

    // -----------------------------------------------------------------------
    // STAGE 1：异步 per-peer 收集（官方对齐审计 §2.7）
    // -----------------------------------------------------------------------

    /// 每取出（= 每次收集绑定）一枚 socket 就计一次数的池。
    struct CountingSocks {
        inner: crate::ice::ProtectedUdpFdSource,
        taken: AtomicU64,
    }
    impl CountingSocks {
        fn feed_batch(&self, n: usize) {
            for _ in 0..n {
                let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
                assert!(fd >= 0, "socket() failed");
                self.inner.feed(fd).expect("feed");
                unsafe { sys::close(fd) };
            }
        }
    }
    impl UdpSocketSource for CountingSocks {
        fn take_fd(&self) -> Result<i32, crate::mgmtsock::SocketSeamError> {
            self.inner.take_fd()
        }
        fn take_owned(&self) -> Result<std::os::fd::OwnedFd, crate::mgmtsock::SocketSeamError> {
            let owned = self.inner.take_owned()?;
            self.taken.fetch_add(1, Ordering::Relaxed);
            Ok(owned)
        }
        fn available(&self) -> Option<usize> {
            Some(self.inner.pending())
        }
    }

    /// 手动执行者：`spawn` 只入队，测试自己决定何时跑哪个 round（trickle 的
    /// **确定性**夹具：round 完成时刻不再由墙钟决定）。
    #[derive(Default)]
    struct ManualGatherExec {
        jobs: Mutex<Vec<Box<dyn FnOnce() + Send>>>,
    }
    impl ManualGatherExec {
        /// 跑一个已入队的 round（返回是否真的跑了一个）。
        fn run_next(&self) -> bool {
            let job = self.jobs.lock().expect("jobs").pop();
            match job {
                Some(job) => {
                    job();
                    true
                }
                None => false,
            }
        }
        fn queued(&self) -> usize {
            self.jobs.lock().expect("jobs").len()
        }
    }
    impl GatherExec for ManualGatherExec {
        fn spawn(&self, job: Box<dyn FnOnce() + Send>) -> bool {
            self.jobs.lock().expect("jobs").push(job);
            true
        }
    }

    /// (b) H1 回归（真机 2026-10-11 PC：23 个对端全部恒走中继）—— **20 个 peer
    /// 同一拍各自开跑**，不再有「每拍最多一个 peer 做阻塞收集」的预算，也**不再
    /// 每个 peer 每接口取一枚 socket**。
    ///
    /// ea8ede1 的模型是「一拍一个 peer、一次收集封顶 [`DEFAULT_STUN_TIMEOUT_MS`]」：
    /// 20 个 peer 要 20 拍，`now_ms` 被 STUN 冻结，下一拍把上一拍建的会话当僵尸
    /// 拆掉。stage 2 的模型是上游的模型：**一枚共享 mux socket**，host 候选当拍
    /// 就位（`udpMux.GetListenAddresses()` × 一个端口），srflx 才在 worker 上跑。
    /// 本用例钉死三件事：
    /// ① 一拍之内 20 个 peer 全部建会话、全部拿到本地候选并发出 OFFER；
    /// ② 那一拍本身不等任何 STUN（墙钟下限）；
    /// ③ 整个 ICE 平面只取走**一枚** socket（与 peer 数、接口数无关）。
    #[test]
    fn many_peers_start_gathering_in_one_tick_without_a_budget() {
        const PEERS: usize = 20;
        let socks = Arc::new(CountingSocks {
            inner: crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"),
            taken: AtomicU64::new(0),
        });
        socks.feed_batch(PEERS + 4);
        let signal = Arc::new(FrameLog::default());
        let keys: Vec<String> = (0..PEERS).map(|i| format!("P{i:02}peer")).collect();
        // 显式用**生产形态**的执行者（默认值就是它；写出来是给读者看的）。
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
        orch.set_peers(&keys);
        orch.set_signal_ready(true);

        let t0 = std::time::Instant::now();
        orch.run_once(1_000).expect("beat 1");
        let beat1 = t0.elapsed();
        assert!(
            beat1.as_millis() < 250,
            "one tick must not wait for 20 gathers: {beat1:?}"
        );
        for (i, p) in orch.peers.iter().enumerate() {
            assert!(p.session.is_some(), "peer {i} must own its session in beat 1");
            assert!(
                p.locals_done,
                "peer {i} must have its host candidates in beat 1 (no STUN wait)"
            );
        }
        assert_eq!(
            socks.taken.load(Ordering::Relaxed),
            1,
            "the whole ICE plane takes exactly ONE socket, whatever the peer count"
        );
        let frames = signal.frames.lock().expect("frames");
        for key in &keys {
            assert!(
                frames.iter().any(|(k, kind, _)| k == key && *kind == PeerSignalKind::Offer),
                "{key} never sent its OFFER"
            );
            assert!(
                frames.iter().any(|(k, kind, _)| k == key && *kind == PeerSignalKind::Candidate),
                "{key} never sent a local candidate"
            );
        }
    }

    /// (c) 候选**按到达时刻**陆续交付（trickle）：host 候选当拍就位（一枚
    /// mux 端口 × 每个允许接口地址，[`crate::ice::mux_host_candidates`]），
    /// srflx 候选随共享 mux 的 STUN round 完成**在下一拍**追加 —— 既不等到
    /// 全部收完才发，也不会丢在别的候选后面。
    ///
    /// 这是上游 `onICECandidate` 的语义（pion fork `gather.go:39-65` 起收集、
    /// 每得一个候选就回调发一帧）：`GatherCandidates()` 立即返回，`dial` 与候选
    /// trickle 同时在飞。用 [`ManualGatherExec`] 手动放行 srflx round，让
    /// 「哪一拍的候选是哪一个」完全确定。
    #[test]
    fn candidates_are_delivered_as_each_round_finishes() {
        let exec = Arc::new(ManualGatherExec::default());
        let socks = Arc::new(CountingSocks {
            inner: crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"),
            taken: AtomicU64::new(0),
        });
        socks.feed_batch(8);
        let signal = Arc::new(FrameLog::default());
        let mut orch = PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![
                crate::ice::InterfaceAddr { name: "eth0".into(), addr: [127, 0, 0, 1] },
                crate::ice::InterfaceAddr { name: "eth1".into(), addr: [127, 0, 0, 2] },
            ])),
            socks: socks.clone(),
            signal: signal.clone(),
            wg: noop_wg(),
            tie_breaker: Some(7),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        });
        // 一个 STUN 服务器：srflx 才有东西可跑（黑洞端口 = 永远不答）。
        assert_eq!(orch.set_stuns(&[format!("stun:127.0.0.1:{}", {
            let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            s.local_addr().unwrap().port()
        })]), 1);
        orch.set_gather_exec(exec.clone());
        orch.set_peers(&["P".into()]);
        orch.set_signal_ready(true);

        orch.run_once(1_000).expect("beat 1");
        assert_eq!(exec.queued(), 1, "one srflx round per STUN server (not per interface)");
        let port = orch.ice_mux().expect("mux").local_port();
        {
            // host 候选当拍就位：两个接口地址、**同一个** mux 端口，OFFER 随行。
            let frames = signal.frames.lock().expect("frames");
            let cands: Vec<&(String, PeerSignalKind, String)> = frames
                .iter()
                .filter(|(_, k, _)| *k == PeerSignalKind::Candidate)
                .collect();
            assert_eq!(cands.len(), 2, "one host candidate per allowed interface: {frames:?}");
            for (_, _, payload) in &cands {
                assert!(
                    payload.contains(&format!(" {port} typ host")),
                    "every host candidate carries the ONE mux port: {payload}"
                );
            }
            assert!(frames.iter().any(|(_, k, _)| *k == PeerSignalKind::Offer), "OFFER rides it");
        }
        assert!(orch.peers[0].locals_done, "host candidates make the locals ready");
        assert!(orch.peers[0].gather.is_some(), "the srflx round is still in flight");
        assert_eq!(socks.taken.load(Ordering::Relaxed), 1, "ONE mux socket for the whole plane");

        // srflx round 跑完（黑洞 → 超时失败）：没有 srflx 候选，收集随之结束，
        // 会话与 host 候选不受影响（上游 gather.go:526-531 只记一条 warning）。
        assert!(exec.run_next(), "the srflx round");
        orch.run_once(1_100).expect("beat 2");
        assert!(orch.peers[0].gather.is_none(), "the srflx round reported: the gather is over");
        assert!(orch.peers[0].locals_done, "host candidates survive a dead STUN server");
        let frames = signal.frames.lock().expect("frames");
        assert_eq!(
            frames.iter().filter(|(_, k, _)| *k == PeerSignalKind::Candidate).count(),
            2,
            "a dead STUN server adds no candidate: {frames:?}"
        );
        drop(frames);
        assert!(orch.peers[0].last_error.is_none(), "a dead STUN server is not a session error");
    }

    /// (a) 黑洞 STUN **不阻塞 pump、也不阻塞别的 peer**：真实黑洞（本机绑定但
    /// 从不回包的 UDP sink）让每个 round 都卡满 STUN 超时，而 pump 每拍仍然是
    /// 微秒级。用例同时给出「STUN 真的卡住了」的证据（首个候选不早于超时），
    /// 否则「pump 很快」可能只是因为收集早就结束了。
    #[test]
    fn black_holed_stun_never_blocks_the_pump() {
        let sink = std::net::UdpSocket::bind("127.0.0.1:0").expect("black-hole sink");
        let port = sink.local_addr().expect("sink addr").port();
        let socks = Arc::new(CountingSocks {
            inner: crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"),
            taken: AtomicU64::new(0),
        });
        socks.feed_batch(8);
        let signal = Arc::new(FrameLog::default());
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
        assert_eq!(orch.set_stuns(&[format!("stun:127.0.0.1:{port}")]), 1);
        orch.set_peers(&["P0".into(), "P1".into()]);
        orch.set_signal_ready(true);

        let t0 = std::time::Instant::now();
        orch.run_once(1_000).expect("beat 1");
        assert!(
            t0.elapsed().as_millis() < 250,
            "the first tick must not wait for the STUN exchange: {:?}",
            t0.elapsed()
        );
        // STAGE 2：host 候选当拍就位（不再等 STUN），srflx 交换在两个 peer 上
        // 同时在飞——而且是**同一枚 mux socket 上的同一轮**交换（去重）。
        assert_eq!(orch.peers.iter().filter(|p| p.gather.is_some()).count(), 2);
        assert!(
            orch.peers.iter().all(|p| p.locals_done),
            "host candidates are immediate; only the srflx exchange is in flight"
        );
        let mux = orch.ice_mux().expect("the shared mux");
        assert_eq!(mux.stats().xor_misses, 1, "ONE STUN round for both peers' agents");

        // 收集被黑洞卡住的期间猛推 pump：每一拍都必须立刻返回。
        let mut worst_tick = std::time::Duration::ZERO;
        let mut pump_total = std::time::Duration::ZERO;
        let window = std::time::Instant::now() + std::time::Duration::from_millis(300);
        while std::time::Instant::now() < window {
            let now = 1_000 + t0.elapsed().as_millis() as u64;
            let tick = std::time::Instant::now();
            orch.run_once(now).expect("beat");
            let dt = tick.elapsed();
            worst_tick = worst_tick.max(dt);
            pump_total += dt;
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(
            worst_tick.as_millis() < 50,
            "a tick blocked on STUN would cost the STUN timeout; worst tick = {worst_tick:?}"
        );
        assert!(
            orch.peers.iter().all(|p| p.gather.is_some()),
            "the black hole must still be holding both srflx exchanges"
        );

        // 超时到点：两个 peer 的 srflx 交换各自以 timeout 收尾（host 候选早已
        // 就位、早已 advertise），pump 花的墙钟总量仍然远小于它。
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut first_candidate_at = None;
        while orch.peers.iter().any(|p| p.gather.is_some()) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(2));
            let now = 1_000 + t0.elapsed().as_millis() as u64;
            let tick = std::time::Instant::now();
            orch.run_once(now).expect("beat");
            let dt = tick.elapsed();
            worst_tick = worst_tick.max(dt);
            pump_total += dt;
            if first_candidate_at.is_none() && orch.peers[0].gather.is_none() {
                first_candidate_at = Some(t0.elapsed());
            }
        }
        let first_candidate_at = first_candidate_at.expect("the blocked exchange must still finish");
        assert!(
            first_candidate_at.as_millis() as u64 >= DEFAULT_STUN_TIMEOUT_MS - 700,
            "the srflx exchange must run for the full STUN timeout ({first_candidate_at:?})"
        );
        assert!(orch.peers.iter().all(|p| p.locals_done), "both peers keep their host candidates");
        assert!(
            pump_total < first_candidate_at / 4,
            "pump total {pump_total:?} must be dwarfed by the blocked gather {first_candidate_at:?}"
        );
        assert!(worst_tick.as_millis() < 50, "worst tick over the whole window = {worst_tick:?}");
        let frames = signal.frames.lock().expect("frames");
        assert!(
            frames.iter().filter(|(_, k, _)| *k == PeerSignalKind::Candidate).count() >= 2,
            "each peer advertises its own candidate: {frames:?}"
        );
        drop(frames);
        drop(sink);
    }

    /// STAGE 2（官方对齐审计 §2.7）：**host 候选不再 bind** —— 它只是
    /// 「接口地址 × mux 端口」的一条广播，所以一个本机根本绑不上的地址
    /// （TEST-NET-3，203.0.113.0/24）照样是一条合法候选：不起会话回滚、不冒
    /// pump 错误、不留 `last_error`。这与上游一致（`gatherCandidatesLocalUDPMux`
    /// 只读 `GetListenAddresses()`，一个 socket 都不开，fork `gather.go:331-405`）。
    ///
    /// 真正的 fail-closed 面收窄成一条：**没有 mux**（见
    /// `no_mux_fails_closed_instead_of_gathering_sockets`）。
    #[test]
    fn an_unbindable_interface_address_is_still_advertised_off_the_mux() {
        let socks = Arc::new(CountingSocks {
            inner: crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"),
            taken: AtomicU64::new(0),
        });
        socks.feed_batch(4);
        let signal = Arc::new(FrameLog::default());
        let mut orch = PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![crate::ice::InterfaceAddr {
                name: "eth0".into(),
                addr: [203, 0, 113, 7],
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

        // 第一拍：候选（接口地址 × mux 端口）与 OFFER 当拍就位，绝不等 STUN。
        orch.run_once(1_000).expect("beat 1 gathers host candidates immediately");
        let mux_port = orch.ice_mux().expect("the shared mux").local_port();
        let p = &orch.peers[0];
        assert!(p.session.is_some(), "the initiate path builds the session");
        assert!(p.locals_done, "host candidates are emitted immediately (no STUN wait)");
        assert_eq!(socks.taken.load(Ordering::Relaxed), 1, "ONE mux socket, not one per interface");
        let frames = signal.frames.lock().expect("frames");
        let cands: Vec<String> = frames
            .iter()
            .filter(|(_, k, _)| *k == PeerSignalKind::Candidate)
            .map(|(_, _, p)| p.clone())
            .collect();
        assert_eq!(cands.len(), 1, "the unbindable interface is still advertised: {cands:?}");
        assert!(
            cands[0].contains(&format!("203.0.113.7 {mux_port} ")),
            "the candidate carries the interface address and the MUX port: {}",
            cands[0]
        );
        assert!(
            frames.iter().any(|(_, k, _)| *k == PeerSignalKind::Offer),
            "the OFFER rides the first candidate"
        );
        assert!(orch.peers[0].last_error.is_none(), "an unreachable address is not an error");
        assert!(orch.peers[0].started == false, "no remote candidates yet: still Gathering");
    }

    // -----------------------------------------------------------------------
    // guard 重连节拍（`client/internal/peer/guard/guard.go:97-213`）
    // -----------------------------------------------------------------------

    /// 单接口 + 指定深度 socket 池的编排器，一个静默 peer（定序 guard 熵源）。
    fn silent_peer_orch(pool: usize) -> PeerIceOrchestrator {
        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"));
        for _ in 0..pool {
            let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            assert!(fd >= 0, "socket() failed");
            socks.feed(fd).expect("feed");
            unsafe { sys::close(fd) };
        }
        let mut orch = test_orch(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![crate::ice::InterfaceAddr {
                name: "eth0".into(),
                addr: [127, 0, 0, 1],
            }])),
            socks,
            signal: Arc::new(LoggingSignalExchange::default()),
            wg: noop_wg(),
            tie_breaker: Some(7),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        });
        seed_guard(&mut orch);
        orch.set_peers(&["P".into()]);
        orch.set_signal_ready(true);
        orch
    }

    /// guard 参数与每 peer 超时抽定的算术：3 s 起跳、×2、±10% 抖动、封顶 =
    /// 本 peer 的 `ConnConfig.Timeout` ∈ [30 s, 45 s)（`guard.go:188-195`、
    /// `engine.go:91-92`、`engine.go:2010-2011`）。
    #[test]
    fn guard_backoff_arithmetic_and_peer_timeout_bounds_are_frozen() {
        assert_eq!(GUARD_INITIAL_MS, 3_000, "guard.initialTicker InitialInterval (guard.go:188-190)");
        assert_eq!(GUARD_RECONNECT_MS, 800, "newReconnectTicker InitialInterval (guard.go:200-208)");
        assert_eq!(GUARD_RANDOMIZATION, 0.1, "RandomizationFactor (guard.go:190/:202)");
        assert_eq!(GUARD_MULTIPLIER, 2.0, "Multiplier (guard.go:191/:203)");
        assert_eq!(PEER_TIMEOUT_MIN_MS, 30_000, "PeerConnectionTimeoutMin (engine.go:91)");
        assert_eq!(PEER_TIMEOUT_MAX_MS, 45_000, "PeerConnectionTimeoutMax (engine.go:92)");
        // `rand.Intn(Max-Min)+Min` 的镜像：draw ∈ [30 s, 45 s)。
        let mut rng = GuardRng(0.0);
        assert_eq!(draw_peer_timeout_ms(&mut rng), 30_000, "sample 0 → the Min bound");
        let mut rng = GuardRng(1.0 - f64::EPSILON);
        let top = draw_peer_timeout_ms(&mut rng);
        assert!(top >= 30_000 && top < 45_000, "sample →1 must stay below Max, got {top}");
        // 抖动包络：±10%（GuardRng(0.0) 取下端、GuardRng(0.5) 取裸间隔）。
        let mut bo = guard_backoff(GUARD_INITIAL_MS, 45_000);
        let mut rng = GuardRng(0.0);
        assert_eq!(
            bo.next_delay(&MonotonicClock, &mut rng).map(delay_ms),
            Some(2_700),
            "sample 0 → current×(1−0.1)"
        );
        let mut bo = guard_backoff(GUARD_INITIAL_MS, 45_000);
        let mut rng = GuardRng(0.5);
        assert_eq!(
            bo.next_delay(&MonotonicClock, &mut rng).map(delay_ms),
            Some(3_000),
            "sample 0.5 → the bare interval"
        );
    }

    /// 静默 peer 的 guard 节拍逐值冻结：3 s → 6 s → 12 s → 24 s → 封顶（封顶
    /// 值 = 本 peer 抽定的 timeout，GuardRng(0.5) 时 37.5 s ∈ [30,45) s），
    /// 并从此停在封顶上。`START_DEADLINE_MS`（9 s 拆除）没有对应物：会话
    /// 留下、guard 每一跳重发同一对凭证（`guard.go:129-130`）。
    #[test]
    fn silent_remote_reoffers_on_the_guard_sequence_capped_at_the_peer_timeout() {
        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"));
        for _ in 0..64 {
            let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            assert!(fd >= 0, "socket() failed");
            socks.feed(fd).expect("feed");
            unsafe { sys::close(fd) };
        }
        let signal = Arc::new(FrameLog::default());
        let mut orch = test_orch(PeerIceDeps {
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
        seed_guard(&mut orch);
        orch.set_peers(&["P".into()]);
        orch.set_signal_ready(true);

        let offer_count = |signal: &Arc<FrameLog>| -> u64 {
            signal
                .frames
                .lock()
                .expect("frames")
                .iter()
                .filter(|(_, k, _)| *k == PeerSignalKind::Offer)
                .count() as u64
        };
        let mut offer_ticks: Vec<u64> = Vec::new();
        let mut prev = 0u64;
        let mut now = 1_000u64;
        while now <= 130_000 {
            orch.run_once(now).expect("a silent beat is not an error");
            let n = offer_count(&signal);
            if n > prev {
                offer_ticks.push(now);
                prev = n;
            }
            now += 250;
        }
        // 首跳立即（guard_next=0），后续 3/6/12/24 s，然后停在 37.5 s 封顶。
        assert_eq!(
            offer_ticks,
            vec![1_000, 4_000, 10_000, 22_000, 46_000, 83_500, 121_000],
            "guard hops: +3 s, +6 s, +12 s, +24 s, then the cap (37.5 s) forever"
        );
        assert_eq!(orch.peers[0].timeout_ms, 37_500, "the cap IS the drawn peer timeout");
        // 贯穿全程没有会话被拆过：guard 只重发，从不 teardown（没有 9 s
        // 僵尸拆除；会话自首次发起起一直在）。
        assert!(orch.peers[0].session.is_some(), "no START_DEADLINE_MS style teardown exists");
        let creds = orch.peers[0].local_creds.clone().expect("stable creds");
        let payloads: std::collections::HashSet<String> = signal
            .frames
            .lock()
            .expect("frames")
            .iter()
            .filter(|(_, k, _)| *k == PeerSignalKind::Offer)
            .map(|(_, _, p)| p.clone())
            .collect();
        assert_eq!(
            payloads,
            std::collections::HashSet::from([format!("{}:{}", creds.ufrag, creds.pwd)]),
            "every hop re-offers the SAME per-peer credentials"
        );
    }

    /// 退避中途到达的对端 Offer：**当拍被应答**（建会话 + 收集 + ANSWER 入列，
    /// 下一拍发出），且 guard 节拍**不**因收帧复位——上游的复位条件只有
    /// 断连/网络事件（`guard.go:141-177`），收帧从不重置 ticker。
    #[test]
    fn remote_offer_mid_backoff_is_answered_immediately_and_does_not_rearm() {
        let signal = Arc::new(FrameLog::default());
        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"));
        for _ in 0..64 {
            let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            assert!(fd >= 0, "socket() failed");
            socks.feed(fd).expect("feed");
            unsafe { sys::close(fd) };
        }
        let mut orch = test_orch(PeerIceDeps {
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
        seed_guard(&mut orch);
        orch.set_peers(&["P".into()]);
        orch.set_signal_ready(true);

        // 首跳发起（t=1000），下一跳 4000。
        orch.run_once(1_000).expect("first hop");
        assert_eq!(orch.peers[0].guard_next_at_ms, 4_000, "the next hop is 3 s out");

        // 对端 Offer 在两跳之间到达（退避期内）：当拍建会话 + 欠 ANSWER。
        let answer_at = 2_500u64;
        orch.handle_signal("P", PeerSignalKind::Offer, "abcd:0123456789012345678901", answer_at)
            .expect("offer accepted");
        assert!(orch.peers[0].session.is_some(), "the offer is answered in the same beat");
        assert!(
            orch.peers[0].outbox.iter().any(|(kind, _)| *kind == PeerSignalKind::Answer),
            "the ANSWER is queued in the same beat the offer arrived"
        );
        // guard 节拍不被收帧复位：下一跳仍在 4000（guard.go 的复位只在
        // relay/ICE 断连、sr 重连与网络变化分支）。
        assert_eq!(
            orch.peers[0].guard_next_at_ms, 4_000,
            "a received signal never re-arms the guard ticker"
        );
        // 下一拍：ANSWER 照常发出。
        orch.run_once(answer_at + 2).expect("beat");
        let frames = signal.frames.lock().expect("frames");
        assert!(
            frames.iter().any(|(k, kind, _)| k == "P" && *kind == PeerSignalKind::Answer),
            "the ANSWER must go out on the next beat: {frames:?}"
        );
    }

    /// guard 的两个复位事件，各落在 **800 ms** 的下一跳（`newReconnectTicker`
    /// 消费立即首跳后的第一个 backoff 间隔，`guard.go:200-213`）：
    /// - 物理网络变化（`restart_for_network_change` → guard.go:162-177）；
    /// - signal 流重连的 false→true 边沿（`set_signal_ready` →
    ///   `srReconnectedChan`，guard.go:155-160）。
    /// 会话随网络变化整段拆掉（旧候选端口已不存在），本地凭证不变。
    #[test]
    fn network_change_and_signal_reconnect_rearm_at_the_800ms_ticker() {
        let mut orch = silent_peer_orch(64);
        // 首跳发起，之后节拍走到 4000。
        orch.run_once(1_000).expect("first hop");
        assert_eq!(orch.peers[0].guard_next_at_ms, 4_000);
        assert!(orch.peers[0].session.is_some(), "precondition: the session exists");

        // 物理网络变化：会话拆掉、节拍复位到 800 ms、凭证保持。
        let creds_before = orch.peers[0].local_creds.clone();
        orch.restart_for_network_change(5_000);
        assert!(orch.peers[0].session.is_none(), "the stale-path session is dropped");
        assert_eq!(orch.peers[0].state, PeerIceState::Failed);
        assert_eq!(
            orch.peers[0].local_creds.as_ref(),
            creds_before.as_ref(),
            "local credentials survive the recreation (per-peer, worker_ice.go:91-97)"
        );
        assert_eq!(orch.peers[0].guard_next_at_ms, 5_800, "reset to now+800 ms (bare interval)");
        // 800 ms 内不到点；到点即重新发起（同一对凭证的 OFFER）。
        orch.run_once(5_799).expect("pre-hop beat");
        assert!(orch.peers[0].session.is_none(), "no initiate before the 800 ms hop");
        orch.run_once(5_800).expect("the 800 ms hop initiates");
        assert!(orch.peers[0].session.is_some(), "the hop re-initiates");

        // signal 重连边沿（false→true）：下一拍复位到 800 ms。
        orch.set_signal_ready(false);
        orch.set_signal_ready(true);
        orch.run_once(6_000).expect("the reconnect beat");
        assert_eq!(
            orch.peers[0].guard_next_at_ms, 6_800,
            "the sr-reconnect edge re-arms the ticker at +800 ms"
        );
    }

    // -------------------------------------------------------------------
    // glare 角色决胜（官方 isController 公钥比较）+ Checking 看门狗
    // -------------------------------------------------------------------

    /// 模块级记录 signal seam（glare/角色测试共用；函数内同名局部定义
    /// 各自遮蔽，互不影响）。
    #[derive(Default)]
    struct FrameLog {
        frames: Mutex<Vec<(String, PeerSignalKind, String)>>,
    }
    impl SignalExchange for FrameLog {
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

    /// 带 `pool` 枚受保护 socket、一个 loopback 接口、FrameLog signal 的
    /// 编排器；`local_key` 注入官方角色决胜的输入。
    fn glare_orch(local_key: Option<&str>, peer: &str, pool: usize) -> (PeerIceOrchestrator, Arc<FrameLog>) {
        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"));
        for _ in 0..pool {
            let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            assert!(fd >= 0, "socket() failed");
            socks.feed(fd).expect("feed");
            unsafe { sys::close(fd) };
        }
        let signal = Arc::new(FrameLog::default());
        let mut orch = test_orch(PeerIceDeps {
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
        if let Some(k) = local_key {
            orch.set_local_key(k.to_string());
        }
        // guard 节拍的定序熵源（±10% 抖动取裸间隔）：set_peers 之前装定，
        // 每 peer 的 timeout 抽定（GuardRng(0.5) → 37.5 s）与逐跳间隔都确定。
        seed_guard(&mut orch);
        orch.set_peers(&[peer.to_string()]);
        orch.set_signal_ready(true);
        (orch, signal)
    }

    /// 官方 `isController = LocalKey > Key` 的镜像语义（base64 字符串序，
    /// 双侧独立计算同一反称比较 → 必然互补）。
    #[test]
    fn deterministic_controlling_matches_official_is_controller() {
        // 大 → controlling，小 → controlled（两个方向是同一比较）
        assert_eq!(deterministic_controlling(Some("zzzKey"), "aaaKey"), Some(true));
        assert_eq!(deterministic_controlling(Some("aaaKey"), "zzzKey"), Some(false));
        // base64 字符串序的比较语义（'A' < 'Z' < 'a'）
        assert_eq!(deterministic_controlling(Some("AAA"), "ZZZ"), Some(false));
        assert_eq!(deterministic_controlling(Some("a"), "Z"), Some(true));
        // 相等（对端=自己，现实中不可能）：> 为假 → controlled
        assert_eq!(deterministic_controlling(Some("same"), "same"), Some(false));
        // 本端公钥未知（无 signal 材料的构造）：无决策，调用方回退旧规则
        assert_eq!(deterministic_controlling(None, "any"), None);
    }

    /// 会话角色在**创建时**即按公钥比较确定，与 Offer 方向无关（官方
    /// worker_ice.go `agentDial` 语义：发起方也可能是 Accept/controlled，
    /// 应答方也可能是 Dial/controlling）。
    #[test]
    fn session_role_follows_public_key_comparison() {
        // 发起方 + 本端公钥大 → controlling offerer
        let (mut orch, _) = glare_orch(Some("ZZZZlocal"), "PPPPpeer", 4);
        orch.run_once(1000).expect("initiate");
        assert_eq!(
            orch.peer_status("PPPPpeer").expect("peer").controlling,
            Some(true),
            "bigger local key → controlling offerer"
        );
        // 发起方 + 本端公钥小 → controlled offerer（官方 Accept 侧）
        let (mut orch, _) = glare_orch(Some("AAAAlocal"), "PPPPpeer", 4);
        orch.run_once(1000).expect("initiate");
        assert_eq!(
            orch.peer_status("PPPPpeer").expect("peer").controlling,
            Some(false),
            "smaller local key → controlled offerer (official Accept semantics)"
        );
        // 应答方 + 本端公钥大 → controlling ANSWERER（官方 Dial 侧）
        let (mut orch, _) = glare_orch(Some("ZZZZlocal"), "PPPPpeer", 4);
        orch.set_initiator("PPPPpeer", false);
        orch.handle_signal("PPPPpeer", PeerSignalKind::Offer, "abcd:0123456789012345678901", 1000)
            .expect("offer");
        assert_eq!(
            orch.peer_status("PPPPpeer").expect("peer").controlling,
            Some(true),
            "bigger local key → controlling answerer (role is key-based, not offer-direction-based)"
        );
        // 同一对密钥在两侧得到互补角色：本端 ZZZZlocal/对端 PPPPpeer=true 的
        // 镜像 = 本端 PPPPpeer/对端 ZZZZlocal=false。
        assert_eq!(
            deterministic_controlling(Some("PPPPpeer"), "ZZZZlocal"),
            Some(false),
            "the mirrored comparison must be antisymmetric"
        );
    }

    /// glare（官方 isController 公钥比较）+ guard 安静语义：我方已发起（Offer
    /// 已发）后收到对端 Offer —— ①必须回 ANSWER（官方 handshaker.Listen 对
    /// 每个 Offer 无条件 sendAnswer，handshaker.go:136）；②角色不变（公钥
    /// 比较在创建时定死）；③对端换了凭证时采纳新凭证并重发本地候选（对端
    /// 新 agent 的候选缓存已死）；④采纳后本端「agent 进行中」，guard 不再
    /// 重发 OFFER——上游 guard 只在 Disconnected 状态才回调（guard.go:127-130），
    /// offer 风暴（§2.4）由这个安静面终结。
    #[test]
    fn glare_offer_is_answered_and_the_guard_goes_quiet() {
        let (mut orch, signal) = glare_orch(Some("ZZZZlocal"), "PPPPpeer", 4);
        orch.run_once(1000).expect("initiate beat sends offer+candidates");
        let offers_before = signal
            .frames
            .lock()
            .expect("frames")
            .iter()
            .filter(|(_, k, _)| *k == PeerSignalKind::Offer)
            .count();
        assert_eq!(offers_before, 1, "precondition: exactly our OFFER was signaled");

        // 对端 Offer 到达（新凭证 = 其 agent 重启）：glare 分支。
        orch.handle_signal("PPPPpeer", PeerSignalKind::Offer, "wxyz:0123456789012345678902", 1100)
            .expect("glare offer");
        {
            let p = &orch.peers[0];
            // ① ANSWER 已入列（本地凭证不变）
            let answer_payload = &p
                .outbox
                .iter()
                .find(|(k, _)| *k == PeerSignalKind::Answer)
                .expect("the glare branch MUST queue an ANSWER (official: sendAnswer per offer)")
                .1;
            let creds = parse_ufrag_pwd(answer_payload).expect("answer is ufrag:pwd");
            assert_eq!(
                creds,
                p.local_creds.as_ref().expect("local creds").clone(),
                "the answer carries OUR (unchanged) local credentials"
            );
            // ② 角色不变（公钥比较在创建时确定，不随 Offer 方向摆动）
            assert!(p.session.as_ref().expect("session").is_controlling());
            // ③ 对端新凭证已采纳 + 本地候选重新入列（对端新 agent 的
            //    候选缓存已死）
            assert_eq!(p.remote_creds.as_ref().expect("remote creds").ufrag, "wxyz");
            assert!(
                p.outbox.iter().any(|(k, _)| *k == PeerSignalKind::Candidate),
                "local candidates must be re-trickled for the peer's fresh agent"
            );
        }
        // ④ 刷出 ANSWER，再走过两个 guard 跳（4000、10000，定序熵源）：
        //    零新 OFFER——远端凭证在手 = InProgress，guard 不回调。
        orch.run_once(1200).expect("flush the answer");
        for t in [4_000u64, 10_000] {
            orch.run_once(t).expect("guard hop");
        }
        let frames = signal.frames.lock().expect("frames").clone();
        let offers_total = frames.iter().filter(|(_, k, _)| *k == PeerSignalKind::Offer).count();
        assert_eq!(offers_total, 1, "no OFFER re-send after the glare answer (guard quiet)");
        assert!(
            frames.iter().any(|(to, k, _)| to == "PPPPpeer" && *k == PeerSignalKind::Answer),
            "the ANSWER must have left"
        );

        // 对端重发同一 Offer（同凭证，幂等）：再次应答、仍零新 OFFER。
        orch.handle_signal("PPPPpeer", PeerSignalKind::Offer, "wxyz:0123456789012345678902", 10_001)
            .expect("duplicate offer");
        orch.run_once(10_001).expect("flush");
        let frames = signal.frames.lock().expect("frames").clone();
        let offers_total = frames.iter().filter(|(_, k, _)| *k == PeerSignalKind::Offer).count();
        assert_eq!(offers_total, 1, "duplicate offer re-answers but never re-offers");
        assert!(
            frames.iter().filter(|(_, k, _)| *k == PeerSignalKind::Answer).count() >= 2,
            "both the glare offer and the duplicate were answered"
        );
    }

    /// 对端 ANSWER 到达（正常应答路径，非 glare）：远端凭证 + 本地候选就位即
    /// start（**不需要远端候选**——上游的 agent 拿到远端凭证就进 Checking，
    /// 空检查清单也一样，`agent.go:328-351`/`:480-483`）；此后 guard 安静
    /// （InProgress），候选零重发——上游 SendOffer 只在 Disconnected 时发生。
    #[test]
    fn answer_receipt_starts_checks_and_goes_quiet_on_the_guard() {
        let (mut orch, signal) = glare_orch(Some("ZZZZlocal"), "PPPPpeer", 4);
        orch.run_once(1000).expect("offer + candidates out");
        // 对端 ANSWER 到达（零远端候选）。
        orch.handle_signal("PPPPpeer", PeerSignalKind::Answer, "wxyz:0123456789012345678901", 1100)
            .expect("answer");
        {
            let p = &orch.peers[0];
            assert!(p.started, "credentials alone start the checks (empty checklist)");
            assert_eq!(p.state, PeerIceState::Checking);
            assert_eq!(p.remote_cands_seen, 0, "no candidate ever followed");
        }
        // 走过两个 guard 跳（4000、10000，定序熵源）：零帧外出。
        for t in [4_000u64, 10_000] {
            orch.run_once(t).expect("guard hop");
        }
        let frames = signal.frames.lock().expect("frames").clone();
        assert_eq!(
            frames.iter().filter(|(_, k, _)| *k == PeerSignalKind::Offer).count(),
            1,
            "exactly the ONE original offer, no re-sends once the answer arrived"
        );
        assert_eq!(
            frames.iter().filter(|(_, k, _)| *k == PeerSignalKind::Candidate).count(),
            1,
            "no candidate re-signaling while the agent is in progress"
        );
    }

    /// 真机 round-230622244 停摆回归（guard/pion 形态重述）：Answer 到达但
    /// **零远端候选**。上游没有「等一个候选才敢 start」的门——agent 拿到
    /// 远端凭证就进 Checking，空检查清单照样起跳（`agent.go:328-351`、
    /// `:480-483`）；pion 对 Checking 的唯一预算是 disconnected+failed
    /// （netbird 各 6 s，`peer/ice/agent.go:23-24`）= start 后 12 s 判 Failed
    /// （`agent.go:381-387`）。Failed → `closeAgent`（`worker_ice.go:591-600`）
    /// 拆掉会话，guard 节拍的下一跳**以同一对凭证**重新发起
    /// （`worker_ice.go:91-97`：localUfrag/localPwd 是 per-peer 的）。
    ///
    /// 即本用例同时钉住两件事：①pion 的 12 s Checking 预算驱动重谈判；
    /// ②**凭证跨会话重建稳定**——重发起不换凭证（曾经的自造「全新凭证
    /// 重发起」绕道已删，对端手里永远有本端有效的 ufrag:pwd）。
    #[test]
    fn answered_but_candidateless_times_out_at_the_pion_budget_and_reoffers_stable_creds() {
        #[derive(Default)]
        struct RecyclingWg {
            recycled: Mutex<Vec<String>>,
        }
        impl WgPeerApplier for RecyclingWg {
            fn apply_peers(&self, _peers: &[WgPeerEntry]) -> Result<(), String> {
                Ok(())
            }
            fn clear(&self) {}
            fn recycle_endpoint(&self, pub_key_b64: &str) {
                self.recycled.lock().expect("recycled").push(pub_key_b64.to_string());
            }
        }

        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"));
        for _ in 0..4 {
            let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            assert!(fd >= 0, "socket() failed");
            socks.feed(fd).expect("feed");
            unsafe { sys::close(fd) };
        }
        let wg = Arc::new(RecyclingWg::default());
        let signal = Arc::new(FrameLog::default());
        let mut orch = test_orch(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![crate::ice::InterfaceAddr {
                name: "eth0".into(),
                addr: [127, 0, 0, 1],
            }])),
            socks,
            signal: signal.clone(),
            wg: wg.clone(),
            tie_breaker: Some(7),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        });
        seed_guard(&mut orch);
        orch.set_local_key("ZZZZlocal".to_string());
        orch.set_peers(&["PPPPpeer".to_string()]);
        orch.set_signal_ready(true);

        const T0: u64 = 1000;
        orch.run_once(T0).expect("offer + local candidates out");
        assert!(orch.peers[0].session.is_some(), "precondition: the negotiation exists");
        assert!(orch.peers[0].locals_done, "precondition: local candidates gathered");
        let creds = orch.peers[0].local_creds.clone().expect("stable creds");
        let first_offer = format!("{}:{}", creds.ufrag, creds.pwd);

        // Answer 到达（远端凭证在手）但零远端候选——round-230622244 形态。
        // start 不再等候选：凭证即 Checking。
        orch.handle_signal("PPPPpeer", PeerSignalKind::Answer, "wxyz:0123456789012345678901", 1100)
            .expect("answer");
        {
            let p = &orch.peers[0];
            assert!(p.started, "credentials alone start the checks (empty checklist)");
            assert_eq!(p.state, PeerIceState::Checking);
            assert_eq!(p.remote_cands_seen, 0, "and no candidate ever followed");
        }
        // 会话的 Checking 预算从 start（首个泵拍）起算。次拍把它泵进 Checking。
        orch.run_once(1101).expect("pump the started session");
        assert_eq!(
            orch.peers[0].session.as_ref().expect("session").checking_since_ms(),
            Some(1101),
            "pion arms the Checking budget on the first task-loop pass (agent.go:378-387)"
        );

        // 12 s 预算内：活；到点：Failed → 拆除 + WG 端点回收。
        orch.run_once(1101 + crate::ice_session::CHECKING_TIMEOUT_MS - 1)
            .expect("inside the budget");
        assert!(orch.peers[0].session.is_some(), "alive for the whole disconnected+failed budget");
        orch.run_once(1101 + crate::ice_session::CHECKING_TIMEOUT_MS).expect("budget exhausted");
        {
            let p = &orch.peers[0];
            assert!(p.session.is_none(), "the Checking budget tears the session down");
            assert_eq!(p.state, PeerIceState::Failed, "observable as Failed");
            assert_eq!(p.remote_creds, None, "the dead negotiation's remote side is void");
            assert!(p.local_creds.is_some(), "local credentials are NOT part of the teardown");
        }
        assert_eq!(
            wg.recycled.lock().expect("recycled").clone(),
            vec!["PPPPpeer".to_string()],
            "the WG endpoint/egress of the dead negotiation must be recycled"
        );

        // guard 下一跳（首跳 1000 → 4000/10000/22000 都在预算内被 InProgress
        // 吞掉；12000 拆除后的下一跳是 22000）：**同一对凭证**重新发起。
        orch.run_once(22_000).expect("the guard hop re-initiates");
        {
            let p = &orch.peers[0];
            assert!(p.session.is_some(), "a fresh session is armed by the guard hop");
            assert_eq!(p.local_creds.as_ref().expect("same creds").clone(), creds,
                "the re-initiation reuses the SAME per-peer credentials");
        }
        let offers: Vec<String> = signal
            .frames
            .lock()
            .expect("frames")
            .iter()
            .filter(|(to, k, _)| to == "PPPPpeer" && *k == PeerSignalKind::Offer)
            .map(|(_, _, payload)| payload.clone())
            .collect();
        assert_eq!(offers.len(), 2, "the guard hop offers again: {offers:?}");
        assert_eq!(offers[0], offers[1], "both offers carry the SAME stable credentials");
        assert_eq!(offers[1], first_offer);

        // 远端随后补齐「凭证 + 候选」（对端 guard 同样在重发，凭证同样稳定）：
        // 正常 start（恢复路径没被堵死）。
        let cand = Candidate::host_candidate([127, 0, 0, 1], 51820);
        orch.handle_signal("PPPPpeer", PeerSignalKind::Answer, "wxyz:0123456789012345678901", 22_001)
            .expect("fresh answer");
        orch.handle_signal("PPPPpeer", PeerSignalKind::Candidate, &cand.marshal(), 22_001)
            .expect("late candidate");
        orch.run_once(22_002).expect("pump");
        assert!(
            orch.peers[0].started && orch.peers[0].state == PeerIceState::Checking,
            "credentials + candidate start the fresh session inside its own budget"
        );
    }

    /// 正常路径：Answer **和**候选都在 12 s 预算内到达 → 照常 start（进入
    /// Checking）。
    #[test]
    fn answer_with_candidate_starts_checks_normally() {
        let (mut orch, _signal) = glare_orch(Some("ZZZZlocal"), "PPPPpeer", 4);
        const T0: u64 = 1000;
        orch.run_once(T0).expect("offer + local candidates out");
        orch.handle_signal("PPPPpeer", PeerSignalKind::Answer, "wxyz:0123456789012345678901", 1100)
            .expect("answer");
        let cand = Candidate::host_candidate([127, 0, 0, 1], 51820);
        orch.handle_signal("PPPPpeer", PeerSignalKind::Candidate, &cand.marshal(), 1100)
            .expect("candidate");
        let p = &orch.peers[0];
        assert!(p.started, "credentials + a candidate start the negotiation");
        assert_eq!(p.state, PeerIceState::Checking);
        assert!(p.session.is_some());
    }

    /// pion 的 Checking 预算（netbird 把 disconnected/failed 各覆盖成 6 s，
    /// 起点 = 进入 Checking 的首个泵拍，`agent.go:378-387`）：预算内永不拆；
    /// 到点判 Failed → closeAgent 拆掉会话（`worker_ice.go:591-600`）+ 回收
    /// WG 端点；重谈判由 guard 的下一跳**以同一对凭证**发起。
    ///
    /// 场景 = 曾被 all-pairs-failed 掩盖的僵尸 Checking：pair 存在但检查永不
    /// 成功（黑洞端口）——pion 把静默 pair 判 Failed 后**永不重试**、agent 仍
    /// Checking（`agent.go:477-494`），直到预算到点。预算是**固定窗口**：
    /// 上游没有「检查成功滑动重置」——成功的会话早已离开 Checking（曾经
    /// 的滑动看门狗 `CHECKING_WATCHDOG_MS` 是自造机制，随本轮删除）。
    #[test]
    fn pion_checking_budget_fails_stalled_session_and_guard_reoffers_stable_creds() {
        let (mut orch, signal) = glare_orch(Some("ZZZZlocal"), "PPPPpeer", 8);
        const T0: u64 = 1000;
        // 真实发起（guard 首跳）：会话 + 本地候选就位。
        orch.run_once(T0).expect("initiate hop");
        let creds = orch.peers[0].local_creds.clone().expect("stable creds");
        // 远端黑洞候选 + 凭证：pair 存在、检查永远无响应。
        let dead = Candidate::host_candidate([127, 0, 0, 1], 1);
        orch.handle_signal("PPPPpeer", PeerSignalKind::Candidate, &dead.marshal(), T0 + 1)
            .expect("dead candidate");
        orch.handle_signal("PPPPpeer", PeerSignalKind::Answer, "wxyz:0123456789012345678901", T0 + 1)
            .expect("answer");
        assert!(
            orch.peers[0].started && orch.peers[0].state == PeerIceState::Checking,
            "the stalled session is really in Checking"
        );
        // 首个泵拍武装 pion 预算（T0+2）。
        orch.run_once(T0 + 2).expect("arm beat");
        assert_eq!(
            orch.peers[0].session.as_ref().expect("session").checking_since_ms(),
            Some(T0 + 2),
            "the budget arms on the first pump after start (agent.go:378-387)"
        );
        let deadline = T0 + 2 + crate::ice_session::CHECKING_TIMEOUT_MS;
        // 截止前 1 ms：绝不拆。
        assert!(orch.run_once(deadline - 1).is_ok());
        assert!(
            orch.peers[0].session.is_some(),
            "a Checking session lives for the whole disconnected+failed budget"
        );
        // 到点：Failed 事件 → 拆除 + WG 端点回收。
        assert!(orch.run_once(deadline).is_ok());
        assert!(
            orch.peers[0].session.is_none(),
            "the stalled Checking session is dropped at the pion budget"
        );
        assert_eq!(orch.peer_status("PPPPpeer").expect("peer").state, PeerIceState::Failed);
        // guard 下一跳（定序熵源：T0 + 3 s / 6 s / 12 s）以同一对凭证重新
        // 发起——凭证跨会话重建稳定（worker_ice.go:91-97）。
        orch.run_once(T0 + 21_000).expect("the guard hop re-initiates");
        {
            let p = &orch.peers[0];
            assert!(p.session.is_some(), "a fresh session is armed by the guard hop");
            assert_eq!(p.local_creds.as_ref().expect("same creds").clone(), creds,
                "the re-negotiation reuses the SAME per-peer credentials");
        }
        assert!(
            signal
                .frames
                .lock()
                .expect("frames")
                .iter()
                .any(|(to, k, payload)| to == "PPPPpeer"
                    && *k == PeerSignalKind::Offer
                    && *payload == format!("{}:{}", creds.ufrag, creds.pwd)),
            "the fresh negotiation signals an OFFER with the stable credentials"
        );
    }

    // -------------------------------------------------------------------
    // per-peer ICE 细节快照（connector_status 明细面）
    // -------------------------------------------------------------------

    /// 未选出 selected pair：细节两个端点都是 `None`，**绝不回退到第一个
    /// 候选**。前置条件在同一用例里钉死：会话已有本地候选与 pair，所以
    /// `None` 只可能来自「未选中」；状态是仍有信息量的 Checking，角色已定。
    #[test]
    fn ice_detail_reports_unknown_without_a_selected_pair() {
        let (mut orch, _) = glare_orch(Some("ZZZZlocal"), "PPPPpeer", 4);
        const T0: u64 = 1000;
        orch.run_once(T0).expect("initiate beat gathers and creates the session");
        // 远端凭证 + 一个无人应答的候选（黑洞端口）→ 会话 start（Checking）、
        // pair 已存在，但永远没有成功检查 → 没有 selected pair。
        let dead = Candidate::host_candidate([127, 0, 0, 1], 1);
        orch.handle_signal(
            "PPPPpeer",
            PeerSignalKind::Answer,
            "wxyz:0123456789012345678901",
            T0 + 1,
        )
        .expect("answer accepted");
        orch.handle_signal("PPPPpeer", PeerSignalKind::Candidate, &dead.marshal(), T0 + 2)
            .expect("candidate accepted");
        for beat in 1..=3u64 {
            orch.run_once(T0 + 2 + beat * 100).expect("checking beat");
        }
        {
            let p = orch.peers.iter().find(|p| p.key == "PPPPpeer").expect("peer");
            let session = p.session.as_ref().expect("session");
            assert_eq!(p.state, PeerIceState::Checking);
            assert!(
                !session.local_candidates().is_empty(),
                "precondition: the session HAS local candidates (None must mean 'not selected')"
            );
            assert!(
                !session.pair_snapshots().is_empty(),
                "precondition: the session HAS pairs (None must not fall back to one of them)"
            );
            assert!(session.selected_pair().is_none(), "precondition: nothing succeeded yet");
        }
        let d = orch.peer_ice_detail("PPPPpeer").expect("detail");
        assert_eq!(d.pub_key, "PPPPpeer");
        assert_eq!(d.state, PeerIceState::Checking, "checking is a useful state, not a blank");
        assert_eq!(d.controlling, Some(true), "ZZZZlocal > PPPPpeer → controlling");
        assert_eq!(d.selected_local, None, "no selected pair → unknown local endpoint");
        assert_eq!(d.selected_remote, None, "no selected pair → unknown remote endpoint");
        assert!(!d.endpoint_applied);
        assert!(!d.reachable, "Checking is never reachable");
        assert_eq!(d.last_error, None);
        // 无会话的 peer：角色未知（None，不是 false）、端点未知。
        // 键序故意与网络图顺序相反（AAApeer 后注册、但排序在前），用来钉住
        // `ice_details` 的稳定序。
        orch.set_peers(&["PPPPpeer".to_string(), "AAApeer".to_string()]);
        let idle = orch.peer_ice_detail("AAApeer").expect("idle detail");
        assert_eq!(idle.state, PeerIceState::Idle);
        assert_eq!(idle.controlling, None, "no session → role unknown");
        assert_eq!(idle.selected_local, None);
        assert_eq!(idle.selected_remote, None);
        assert_eq!(orch.peer_ice_detail("nobody"), None, "unknown key → None");
        assert_eq!(
            orch.ice_details().iter().map(|d| d.pub_key.clone()).collect::<Vec<_>>(),
            vec!["AAApeer".to_string(), "PPPPpeer".to_string()],
            "ice_details: one owned detail per peer, sorted by pub_key (not network-map order)"
        );
    }

    /// 选出 selected pair：细节带出两侧候选的**类型 + ip:port**（本端 host、
    /// 对端 srflx），并与既有 `peer_status().selected_remote` 同一事实。
    ///
    /// 一枚真实 loopback UDP socket 当测试桩扮演对端：收到检查就按对端口令
    /// 签名回 Binding Success（`on_check_response` 的门是 txn + MI + FP，
    /// XOR-MAPPED 地址不参与判定）→ controlling 侧提名 → selected pair。
    /// 这是 in-file 能做到的最小真实选择路径（无需第二个编排器/信令链路）。
    #[test]
    fn ice_detail_surfaces_the_selected_pair_types_and_addresses() {
        let (mut orch, _) = glare_orch(Some("ZZZZlocal"), "PPPPpeer", 4);
        const T0: u64 = 1000;
        orch.run_once(T0).expect("initiate beat gathers and creates the session");

        let stub = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
        assert!(stub >= 0, "socket() failed");
        let any = sys::sockaddr_in::new([127, 0, 0, 1], 0);
        assert_eq!(
            unsafe { sys::bind(stub, &any, core::mem::size_of::<sys::sockaddr_in>() as u32) },
            0,
            "bind failed"
        );
        let mut bound = sys::sockaddr_in::new([0, 0, 0, 0], 0);
        let mut bound_len = core::mem::size_of::<sys::sockaddr_in>() as u32;
        assert_eq!(unsafe { sys::getsockname(stub, &mut bound, &mut bound_len) }, 0);
        let stub_port = u16::from_be(bound.sin_port);
        assert_ne!(stub_port, 0, "the stub socket must have a real port");

        let peer_creds =
            IceCredentials { ufrag: "wxyz".into(), pwd: "0123456789012345678901".into() };
        orch.handle_signal(
            "PPPPpeer",
            PeerSignalKind::Answer,
            &format!("{}:{}", peer_creds.ufrag, peer_creds.pwd),
            T0 + 1,
        )
        .expect("answer accepted");
        // 对端候选：srflx 型（地址就是桩），类型必须如实进细节。
        let base = Candidate::host_candidate([127, 0, 0, 1], 40000);
        let peer_cand = Candidate::srflx_candidate(&base, [127, 0, 0, 1], stub_port);
        orch.handle_signal("PPPPpeer", PeerSignalKind::Candidate, &peer_cand.marshal(), T0 + 2)
            .expect("candidate accepted");

        let mut now = T0 + 2;
        let mut replied = 0usize;
        for _ in 0..32 {
            now += 50;
            orch.run_once(now).expect("checking beat");
            let mut buf = [0u8; 1500];
            loop {
                let mut src = sys::sockaddr_in::new([0, 0, 0, 0], 0);
                let mut src_len = core::mem::size_of::<sys::sockaddr_in>() as u32;
                let n = unsafe {
                    sys::recvfrom(
                        stub,
                        buf.as_mut_ptr() as *mut core::ffi::c_void,
                        buf.len(),
                        sys::MSG_DONTWAIT,
                        &mut src,
                        &mut src_len,
                    )
                };
                if n <= 0 {
                    break;
                }
                let Ok(parsed) = crate::ice_session::parse_stun(&buf[..n as usize]) else {
                    continue;
                };
                let reply = crate::ice_session::build_success_response(
                    parsed.txn,
                    (src.sin_addr, u16::from_be(src.sin_port)),
                    &peer_creds.pwd,
                );
                let sent = unsafe {
                    sys::sendto(
                        stub,
                        reply.as_ptr() as *const core::ffi::c_void,
                        reply.len(),
                        0,
                        &src,
                        src_len,
                    )
                };
                assert!(sent > 0, "the stub must answer every check it receives");
                replied += 1;
            }
            if orch.peer_status("PPPPpeer").expect("peer").state == PeerIceState::Connected {
                break;
            }
        }
        unsafe { sys::close(stub) };

        assert!(
            replied >= 2,
            "the stub must answer the check AND the nomination check (answered {replied})"
        );
        let d = orch.peer_ice_detail("PPPPpeer").expect("detail");
        assert_eq!(d.state, PeerIceState::Connected);
        assert_eq!(d.controlling, Some(true));
        let local = d.selected_local.clone().expect("selected local endpoint");
        assert_eq!(local.typ, CandidateType::Host, "our gathered candidate is host-typed");
        assert_eq!(local.address, "127.0.0.1");
        assert_ne!(local.port, 0, "the local endpoint is the bound check socket");
        let remote = d.selected_remote.clone().expect("selected remote endpoint");
        assert_eq!(
            remote.typ,
            CandidateType::Srflx,
            "the peer's signaled candidate keeps its type"
        );
        assert_eq!(remote.address, "127.0.0.1");
        assert_eq!(remote.port, stub_port);
        // 与既有 status 面同一事实（同一 selected pair 的 ip:port）。
        let s = orch.peer_status("PPPPpeer").expect("status");
        assert_eq!(s.selected_remote, Some(([127, 0, 0, 1], stub_port)));
        assert_eq!(s.state, PeerIceState::Connected);
        // NullWg 的 fail-closed 默认：ICE 选中（Connected）但 endpoint 未落配
        // → 不可用；细节把「选中」与「落配」两个事实分开报。
        assert!(!d.endpoint_applied, "NullWg cannot land endpoints (fail-closed default)");
        assert!(!d.reachable);
        // 会话仍持有它收集到的 socket（本快照是纯读，不动归属）。
        assert_eq!(
            orch.peers[0].session.as_ref().expect("session").local_candidates().len(),
            1
        );
    }

    // ---- 0.0.26 gap 1 / gap 4: the two new evidence lines -------------------

    /// 0.0.26 gap 1（真机 0.0.25 §4.3）：周期性 mux 计数行的格式与字段顺序
    /// 钉死在这里。0.0.25 的既有六个字段保持原位（`read`…`data_unknown`），
    /// 新增的 `xor_miss`/`xor_expired`/`queue_drop`/`periodic` 追加在行尾——
    /// 真机报告要的 `xor_miss` 正是此前完全没有的那个。
    #[test]
    fn mux_stats_line_pins_the_counter_fields() {
        let s = crate::ice_mux::MuxStats {
            reads: 9,
            xor_hits: 3,
            xor_misses: 5,
            xor_expired: 1,
            xor_filled: 2,
            overlay_dropped: 4,
            stun_unknown: 6,
            data_unknown: 7,
            queue_dropped: 8,
        };
        assert_eq!(
            mux_stats_line(0, &s, true),
            "N5e_ICE|mux|read=0|xor_hit=3|xor_fill=2|overlay_drop=4|stun_unknown=6|\
             data_unknown=7|xor_miss=5|xor_expired=1|queue_drop=8|periodic=1"
        );
        // the fallback read-loop shape stays the SAME head, `read` = this pass
        assert_eq!(
            mux_stats_line(11, &s, false),
            "N5e_ICE|mux|read=11|xor_hit=3|xor_fill=2|overlay_drop=4|stun_unknown=6|\
             data_unknown=7|xor_miss=5|xor_expired=1|queue_drop=8|periodic=0"
        );
    }

    /// 0.0.26 gap 4（真机 0.0.25 §5.4）：提名证据行的格式——远端 `ip:port`
    /// 之后追加 `endpoint_changed`，所以「重新提名到同一个远端地址」与
    /// 「真的换了远端路径」在日志里可区分，而不是两条一模一样的行。
    #[test]
    fn selected_pair_line_marks_an_unchanged_endpoint() {
        assert_eq!(
            selected_pair_line([47, 251, 179, 205], 51820, false),
            "N5_ICE|selected-pair|47.251.179.205:51820|endpoint_changed=false"
        );
        assert_eq!(
            selected_pair_line([192, 168, 50, 8], 37538, true),
            "N5_ICE|selected-pair|192.168.50.8:37538|endpoint_changed=true"
        );
    }
}
