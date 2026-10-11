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
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use crate::backoff::{ExponentialBackoff, MonotonicClock, OsRandom};
use crate::connector::{ErrorClass, WgPeerApplier};
use crate::envelope::EnvelopeKeyPair;
use crate::grpc::GrpcTransport;
use crate::ice::{
    Candidate, CandidateType, GatherConfig, IfaceGather, InterfaceSource, StunServer,
    UdpSocketSource, DEFAULT_INTERFACE_BLACKLIST, DEFAULT_STUN_TIMEOUT_MS,
};
use crate::ice_session::{IceCredentials, IceEvent, IceSession, PairState};
use crate::management::ManagementError;
use crate::mgmtsock::ManagementSocketProvider;
use crate::signal::{SignalClient, SignalLoopEvent, SignalMessage, SignalOutgoing, SignalSession};
use crate::util::jnum;

/// 发起/应答失败后的重试冷却（注入时钟衡量；避免熵/收集失败时热循环）。
pub const RETRY_COOLDOWN_MS: u64 = 2000;

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
/// ——两种驱动跑的是同一份 round 代码（[`crate::ice::gather_interface_round`]）。
pub trait GatherExec: Send + Sync + 'static {
    /// 开始一个 round。返回 `false` = 起不来（线程上限/执行者拒绝）：调用方
    /// 必须把它当成「这一轮没有候选」处理，绝不回落成阻塞调用。
    fn spawn(&self, job: Box<dyn FnOnce() + Send>) -> bool;
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

/// 「受保护 socket 池不足」等待行的限频周期（注入时钟衡量；0050 残余项 3）。
///
/// 池不足不是错误，而是**等待补给**：每一次被挡住的尝试都记一行
/// `N5c_ICE|locals-waiting` 会把 6 个 peer × 每 2 s 重试变成刷屏（真机
/// 0050 的 `locals-failed` 3.5 min 内 39 行 / 25 个去重事件）。等待的**首行
/// 必记**（进入等待的时刻必须可观测），其后每个 peer 每个周期至多一行，
/// 行内 `waited_ms` 给出已等待时长，因此「等了多久」不因限频丢失。
pub const LOCALS_WAIT_LOG_MS: u64 = 5000;

/// STAGE 0（官方对齐审计 §2.7）：整拍墙钟超过这么长才记一行
/// `N5c_ICE|tick|…|pass_ms=…|slow=true`（见 [`PeerIceOrchestrator::run_once`]）。
///
/// 生产 tick = `ICE_PUMP_TICK_MS`（10 ms），逐拍打印是刷屏；健康的一拍只做
/// 注入时钟驱动的状态机推进（0~1 ms），因此 200 ms 是「这一拍真的被什么
/// 挡住了」的下限——正是真机报告要求可验证的那个量。
pub const TICK_SLOW_LOG_MS: u64 = 200;

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

/// Checking 看门狗（注入时钟衡量）：会话已 `start()` 进入 Checking 后，
/// 这么长时间内没有任何检查成功（无 selected pair、无 [`IceEvent::
/// CheckSucceeded`]）即判 failed、拆除会话并允许全新协商。
///
/// 为什么需要它（ICE-DIRECT-FAILURE-ANALYSIS §2.3，真机 2026-10-05）：
/// `all-pairs-failed` 会被"新候选持续流入 → 新 pair 进 Waiting"永久掩盖，
/// `Disconnected` 只在收到过有效入站后才可能触发 —— start 后永不成功的
/// 会话没有任何终态信号，能僵死数小时（对端显示我们一直 Connecting）。
pub const CHECKING_WATCHDOG_MS: u64 = 18_000;

/// 周期性健康摘要的间隔（每个有会话的 peer 一行，§5 item 5）。
pub const HEALTH_SUMMARY_MS: u64 = 10_000;

/// 「远端静默」退避的触发门槛（0051 §7 item 2）：连续这么多次
/// `negotiation-start-timeout`、且**期间从未**收到该 peer 任何信令
/// （offer/answer/candidate），才认为对端真的不应答，开始拉长下一次发起。
/// 真机 0051：`TrBfURPG`/`ai4IGfaf` 各 12 次超时、两侧 0 条 `signal-rx`，
/// 每 ~11 s（[`START_DEADLINE_MS`] + [`RETRY_COOLDOWN_MS`]）重发起一轮，
/// 4 min 内 24 次全是同样的空转。
pub const INITIATE_BACKOFF_MIN_MISSES: u32 = 3;

/// 退避的基数 = 第一次进入退避时下一次发起前的延迟（9 s，与
/// [`START_DEADLINE_MS`] 同量级）：延迟序列 9 s → 18 s → 36 s → … →
/// [`INITIATE_BACKOFF_MAX_MS`]。实际发起间隔 = 谈判截止期（9 s，到期才判
/// 「静默」）+ 本延迟。
pub const INITIATE_BACKOFF_BASE_MS: u64 = START_DEADLINE_MS;

/// 退避延迟上限（5 min）：静默 peer 的重发起频率不高于每 ~5 min 一次；
/// 一旦收到该 peer 任何信令或网络切换/重连事件立即复位（见
/// [`PeerIceOrchestrator::reset_initiate_backoff`]）。
pub const INITIATE_BACKOFF_MAX_MS: u64 = 300_000;

/// 第 `misses`（≥ [`INITIATE_BACKOFF_MIN_MISSES`]）次静默超时对应的下一次
/// 发起延迟：指数增长、封顶 [`INITIATE_BACKOFF_MAX_MS`]。纯函数，供日志与
/// 单测共用同一份算术。
pub fn initiate_backoff_ms(misses: u32) -> u64 {
    let steps = misses.saturating_sub(INITIATE_BACKOFF_MIN_MISSES).min(32);
    INITIATE_BACKOFF_BASE_MS
        .saturating_mul(1u64 << steps)
        .min(INITIATE_BACKOFF_MAX_MS)
}

/// 退避进入/升级行（进入时与每次升级各一行；升级间隔本身按指数增长，
/// 无需额外限频）。字段顺序即格式：`peer=|misses=|next_ms=`。
fn initiate_backoff_line(peer: &str, misses: u32, next_ms: u64) -> String {
    format!("N5c_ICE|initiate-backoff|peer={peer}|misses={misses}|next_ms={next_ms}")
}

/// 退避复位行（每个活动退避周期至多一行，复位即清零，天然限频）。
fn backoff_reset_line(peer: &str, reason: &str) -> String {
    format!("N5c_ICE|backoff-reset|peer={peer}|reason={reason}")
}

/// 记一次 [`START_DEADLINE_MS`] 静默超时（`drop_session_rearm` 在
/// `reason=negotiation-start-timeout` 时调用）：累计「连续静默」次数并按
/// [`initiate_backoff_ms`] 把冷却拉长；返回需要输出的
/// `initiate-backoff` 行参数 `(misses, next_ms)`（进入退避与每次升级各一行，
/// 其余 `None`）。
///
/// 连续性的判据是 `rx_since_timeout`：上一拍超时之后收到过该 peer **任何**
/// 信令（offer/answer/candidate，一条 candidate 也算远端还活着）就不是连续
/// 静默，计数从零起算。收到 Offer 的应答路径从不经过这里。
fn note_silent_timeout(peer: &mut PeerConn, now_ms: u64) -> Option<(u32, u64)> {
    if peer.rx_since_timeout {
        peer.backoff_misses = 0;
        peer.backoff_logged_misses = 0;
    } else {
        peer.backoff_misses = peer.backoff_misses.saturating_add(1);
    }
    peer.rx_since_timeout = false;
    if peer.backoff_misses < INITIATE_BACKOFF_MIN_MISSES {
        return None;
    }
    let next_ms = initiate_backoff_ms(peer.backoff_misses);
    peer.cooldown_until = now_ms.saturating_add(next_ms);
    peer.backoff_active = true;
    if peer.backoff_misses > peer.backoff_logged_misses {
        peer.backoff_logged_misses = peer.backoff_misses;
        Some((peer.backoff_misses, next_ms))
    } else {
        None
    }
}

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
/// - 状态变更时刻：没有这样的既有字段（`session_at_ms` 是会话建立时刻，
///   `checking_since_ms` 是 Checking 看门狗的计时起点、每次检查成功都滑动
///   重置），两者都不是「上一次状态变更」。
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
/// 池不足**不是错误**（0050 §2.6/§8 item 3）：现场把它当成 `Network` 错误
/// 上抛，于是每一拍都冒出 `peer-conn: pump error (network)`，而 ICE pump 被
/// 设计成「逐 tick 打印错误类并继续运行」→「等待」被实现成了「刷错误日志 +
/// 重试」。等待态必须：①不进 `run_once` 的 `first_err`（不冒 pump error）；
/// ②不消耗 [`START_DEADLINE_MS`]（饿死不是「对端不应答」，不能拿它拆会话，
/// 见 [`PeerConn::negotiation_age_ms`]）。
enum LocalsRun {
    /// 本地候选已收集并绑定（`locals_done` 已置位）。
    Gathered,
    /// 收集已在**别的执行者**上跑（STAGE 1，官方对齐审计 §2.7）：本拍不阻塞。
    /// 候选就绪时由 [`PeerIceOrchestrator::pump_gather`] 交给本会话并发
    /// OFFER/ANSWER——上游「`GatherCandidates()` 立即返回、下一行就
    /// dial」（`worker_ice.go:255-266`）在注入时钟模型里的等价物。
    InFlight,
    /// 受保护 socket 池不足：本次**一枚都没取**（可重试），peer 进入等待。
    WaitingForSockets { need: usize, have: usize, waited_ms: u64, log: bool },
}

/// 一个 peer 正在跑的异步收集（STAGE 1）。
///
/// 上游的对应物是「每协商一枚 goroutine」（`worker_ice.go:100-155`）加上
/// pion 内部「每 (network, STUN url, listen addr) 一枚 goroutine」
/// （fork `gather.go:491-513`）。这里把**接口 round** 当作并发单位：一个
/// round = 一枚受保护 socket + 该接口的全部 STUN 交换（[`IfaceGather`]），
/// 一个 peer 同时最多有一轮收集在跑（`gather.is_some()` 即「已在收集」），
/// 轮内各接口并行。
///
/// 掉这个结构体 = 掉接收端：仍在跑的 round 线程 send 失败即自行关闭其
/// socket（绝不会有孤儿 fd，也绝不会把上一轮协商的候选塞进新会话）。
struct GatherInFlight {
    rx: Receiver<IfaceGather>,
    /// 还没回报的接口 round 数：归零 = 本轮收集结束。
    pending: usize,
    /// 本轮收集开始的墙钟（`sys::mono_ms`）——`locals-gathered` 的 `gather_ms=`。
    started_wall_ms: u64,
    /// 已交给本会话的 host 候选数（失败判据：整轮零候选 = fail-closed）。
    added: usize,
    /// 已交给本会话的 srflx 候选数（观测）。
    srflx: usize,
    /// 全部失败原因（整轮零候选时取第一条当硬错误，与旧 `ensure_locals` 的
    /// `Err` 同形）。
    errors: Vec<(String, ManagementError)>,
    /// 哪条调用路径发起的（`initiate` / `answer` / `retry` / `answer-late`）：
    /// 零候选失败时按它选回滚语义（发起路径整段回滚，应答/重试路径保留会话），
    /// 并决定候选就绪时要不要发 OFFER（`offer_owed`）。
    phase: &'static str,
    /// 候选就绪时要补一个 OFFER（本会话是**为了发起**而建的：主动发起，或
    /// 迟到 ANSWER 采纳后重新发起）。应答方/Fixed-port 调用方自己入列 OFFER
    /// 的路径保持 `false`——OFFER 与 ANSWER 都不在同一个会话里重复发。
    offer_owed: bool,
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
    /// 本会话内「等受保护 socket 补给」已累计的时长（[`LocalsRun::
    /// WaitingForSockets`]）：[`START_DEADLINE_MS`] 的计时只算**非等待**的
    /// 龄期（`negotiation_age_ms`），会话重建时归零。
    locals_wait_ms: u64,
    /// 最近一次进入等待的时刻（0 = 当前不在等待）。跨会话重建保持连续，
    /// 因此 `waited_ms` 报的是「池饿了多久」，不是「本会话饿了多久」。
    locals_wait_since_ms: u64,
    /// 上一次输出等待观测行的时刻（0 = 本段等待尚未记过行）；首行必记，
    /// 其后按 [`LOCALS_WAIT_LOG_MS`] 限频。
    locals_wait_log_ms: u64,
    /// 连续「远端静默」的 [`START_DEADLINE_MS`] 超时次数（0051 §7 item 2）：
    /// 期间收到该 peer 任何信令即清零；达到
    /// [`INITIATE_BACKOFF_MIN_MISSES`] 后按 [`initiate_backoff_ms`] 退避。
    backoff_misses: u32,
    /// 上一次超时之后是否收到过该 peer 的信令（连续静默的判据：一条
    /// candidate 也算「远端还活着」，不能算静默）。
    rx_since_timeout: bool,
    /// 当前是否处于退避（`backoff-reset` 行的去重标志：只在活动退避上记
    /// 一行复位，绝不因每帧信令刷屏）。
    backoff_active: bool,
    /// 已按哪个 `misses` 值记过 `initiate-backoff` 行（进入 + 每次升级各
    /// 一行；升级间隔按指数增长）。
    backoff_logged_misses: u32,
    /// Checking 看门狗的计时起点（注入时钟；0 = 未在计时，即会话未 start
    /// 或已终态）。[`CHECKING_WATCHDOG_MS`] 内没有任何检查成功（无
    /// [`IceEvent::CheckSucceeded`]/selected pair）→ 判 failed 重协商；
    /// 每次检查成功滑动重置。会话拆除/回滚时清零。
    checking_since_ms: u64,
    /// 上一次输出健康摘要行的时刻（0 = 尚未输出过；见
    /// [`HEALTH_SUMMARY_MS`]）。
    last_health_ms: u64,
    /// 正在跑的异步本地候选收集（STAGE 1）。`None` = 没有收集在跑。
    gather: Option<GatherInFlight>,
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
            locals_wait_ms: 0,
            locals_wait_since_ms: 0,
            locals_wait_log_ms: 0,
            backoff_misses: 0,
            rx_since_timeout: false,
            backoff_active: false,
            backoff_logged_misses: 0,
            checking_since_ms: 0,
            last_health_ms: 0,
            gather: None,
        }
    }

    /// [`START_DEADLINE_MS`] 判据使用的「龄期」：从 `session_at_ms` 起算，
    /// 扣掉本会话等受保护 socket 补给的全部时长（含**正在**等待的这一段，
    /// 其归属从 `max(等待起点, 会话起点)` 算起，不把会话之前的等待算进
    /// 本会话）。等待不是谈判没有进展的证据——池不足时本端连候选都绑不
    /// 出来，拿这段时间去拆会话只会「饿死 → 拆 → 重新收集 → 再饿死」。
    fn negotiation_age_ms(&self, now_ms: u64) -> u64 {
        let mut age = now_ms.saturating_sub(self.session_at_ms);
        if self.locals_wait_since_ms != 0 {
            let wait_start = self.locals_wait_since_ms.max(self.session_at_ms);
            age = age.saturating_sub(now_ms.saturating_sub(wait_start));
        }
        age.saturating_sub(self.locals_wait_ms)
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
    /// signal 流 false→true 的重连边沿（0051 §7 item 2）：下一拍 `run_once`
    /// 复位全部 peer 的「远端静默」退避后清除。
    signal_reconnected: bool,
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
            unknown_signal: 0,
            fixed_local_port: deps.fixed_local_port,
            advertised_candidates: deps.advertised_candidates,
            local_key: None,
            exec: Arc::new(ThreadGatherExec),
        }
    }

    /// 换本地候选收集的执行者（STAGE 1 的注入缝）。生产从不调用：默认就是
    /// 上游形态的「每 round 一枚线程」；host harness 用
    /// [`InlineGatherExec`] 保住注入时钟的确定性（见该类型）。
    pub fn set_gather_exec(&mut self, exec: Arc<dyn GatherExec>) {
        self.exec = exec;
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
            self.peers.push(PeerConn::new(key.clone()));
        }
    }

    /// signal 流就绪（connector 持有注册好的 signal 流后置位；置位前
    /// `run_once` 不发起任何东西）。0051 §7 item 2：false→true 的重连边沿
    /// 记一个待复位标志，由 `run_once`（有注入时钟）在下一拍复位「远端静默」
    /// 退避——断流期间的静默可能是本端收不到帧，不是对端不应答。
    pub fn set_signal_ready(&mut self, ready: bool) {
        if ready && !self.signal_ready {
            self.signal_reconnected = true;
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

    /// 每个 peer 的受保护 socket 需求（0051 §7 item 3）：**当前允许接口数
    /// × 1**（0052 起：gather 的绑定 socket 被会话直接接管，不再另取第二
    /// 枚），与 [`PeerConn::required_local_sockets`] 同一份枚举/黑名单/去重
    /// 规则（两者都走 [`crate::ice::allowed_ifaces`]），因此壳层的池目标不会
    /// 与 core 的实际预算漂移。2 个允许接口 → `need_per_peer = 2`；
    /// 3 个 → 3。
    ///
    /// 接口枚举失败返回 `None`（状态文档写 `-1`）：壳层据此回退到自己的
    /// 保守常量（每 peer 4），绝不把「测不出来」当成「需求为 0」。
    pub fn need_per_peer(&self) -> Option<usize> {
        let cfg = GatherConfig {
            blacklist: &self.blacklist,
            servers: &self.stuns,
            timeout_ms: DEFAULT_STUN_TIMEOUT_MS,
        };
        crate::ice::allowed_ifaces(&cfg, self.ifaces.as_ref())
            .ok()
            .map(|ifaces| ifaces.len())
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
                    // 在跑的收集属于**旧**协商：丢接收端 = 丢掉旧一轮的候选
                    // （worker 自己收尾并关闭它的 socket），绝不让旧候选落进
                    // 新会话。
                    peer.gather = None;
                    peer.creds = None;
                    peer.locals_done = false;
                    peer.started = false;
                    peer.remote_cands_seen = 0;
                    peer.pending_remote.clear();
                    peer.signaled.clear();
                    peer.retry_at_ms = 0;
                    peer.session_at_ms = 0;
                    peer.locals_wait_ms = 0; // 新会话的等待账从零起算
                    peer.checking_since_ms = 0; // 新会话的看门狗从零起算
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
                        // 收集在 worker 上跑：候选就绪时 `pump_gather` 补 ANSWER。
                        Ok(LocalsRun::Gathered) | Ok(LocalsRun::InFlight) => {}
                        Ok(LocalsRun::WaitingForSockets { need, have, waited_ms, log }) => {
                            // 池不足＝等待（0050 §8 item 3）：不进 `last_error`
                            // （它不是错误），只按既有冷却节奏重试；`answer_owed`
                            // 仍在，补给到达后 `want_locals` 会补 ANSWER。
                            self.peers[idx].cooldown_until = now_ms + RETRY_COOLDOWN_MS;
                            if log {
                                self.emit_locals_waiting(idx, "answer", need, have, waited_ms);
                            }
                        }
                        Err(e) => {
                            let peer = &mut self.peers[idx];
                            peer.last_error = Some(ErrorClass::from_management(&e));
                            peer.cooldown_until = now_ms + RETRY_COOLDOWN_MS;
                            // 应答方收集失败同样不得静默：`answer_owed` 仍在，
                            // `want_locals` 冷却后会重试补 ANSWER。
                            crate::hilog::emit(&format!(
                                "N5c_ICE|locals-failed|peer={}|round={}|phase=answer|reason={e}",
                                peer_short(&peer.key),
                                peer.session_at_ms
                            ));
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
                    //   无条件 sendAnswer）—— 修复前 glare 分支不回
                    //   ANSWER，对端挂起的 Dial 永远等不到应答；
                    // ③对端换了凭证（其 agent 重启，凭证每会话随机）→
                    //   采纳新凭证；官方 worker 重启 agent 时本地凭证
                    //   （localUfrag/localPwd）不变，我方同理保持本地
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
                    // 回 ANSWER（幂等：`signaled` 母本按内容去重，重复
                    // Offer 只会把同一 ANSWER 重新入列）。本地候选未就绪
                    // （`answer_owed` 仍欠着）时跳过 —— `on_locals_ready`
                    // 首次拿到候选时统一补发。
                    if self.peers[idx].locals_done && self.peers[idx].creds.is_some() {
                        let (ufrag, pwd) = {
                            let c = self.peers[idx].creds.as_ref().expect("checked");
                            (c.ufrag.clone(), c.pwd.clone())
                        };
                        let peer = &mut self.peers[idx];
                        peer.outbox.push_back((
                            PeerSignalKind::Answer,
                            format!("{}:{}", ufrag, pwd),
                        ));
                    }
                    // Offer 使命已完成（对端凭证在手 + ANSWER 已回）：
                    // 停止重发 OFFER，避免周期性重摆对端状态（§2.4 的
                    // 160 次/8 min offer 风暴的 glare 半边）。
                    self.peers[idx].signaled.retain(|(k, _)| *k != PeerSignalKind::Offer);
                }
            }
            PeerSignalKind::Answer => {
                let creds = parse_ufrag_pwd(payload)?;
                if self.peers[idx].session.is_none() {
                    // 无会话的 ANSWER = 对端应答了**已被拆掉**的那次 OFFER
                    // （僵尸拆除与 ANSWER 到达只差半拍）。
                    //
                    // 官方语义：**没有「丢帧」这一支**。signal 引擎对任何
                    // 目的地都先 `ActivatePeer` 再派发（engine.go:2088-2097），
                    // handshaker 把 Offer 与 Answer 一并交给 ICE worker
                    // （handshaker.go:150-156），worker 用收到的这一帧**新建
                    // agent** 并 Dial/Accept（worker_ice.go:100-155）——迟到
                    // ANSWER 会直接开启一轮协商，而不是被丢掉（审计 §1.1 E5）。
                    // 因此这里不再计数、不再记 `signal-no-session`（上游没有
                    // 对应行），而是照做：采纳该 ANSWER 的远端凭证、立即以全新
                    // 本地凭证重新发起（`initiated = true`）——对端的新凭证
                    // OFFER 会让它的 worker 重建 agent（同 worker_ice.go:105-130
                    // 的 recreate 形态），双方收敛；否则本端要白等一个
                    // [`START_DEADLINE_MS`] 才重发起。
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
                        peer.initiated = true; // 重新发起：OFFER 随候选就绪出列
                        peer.answer_owed = false;
                    }
                    match self.start_locals(idx, now_ms, pass_start_ms, "answer-late") {
                        Ok(LocalsRun::Gathered) | Ok(LocalsRun::InFlight) => {}
                        Ok(LocalsRun::WaitingForSockets { need, have, waited_ms, log }) => {
                            self.peers[idx].cooldown_until = now_ms + RETRY_COOLDOWN_MS;
                            if log {
                                self.emit_locals_waiting(idx, "answer-late", need, have, waited_ms);
                            }
                        }
                        Err(e) => {
                            let peer = &mut self.peers[idx];
                            peer.last_error = Some(ErrorClass::from_management(&e));
                            peer.cooldown_until = now_ms + RETRY_COOLDOWN_MS;
                            peer.initiated = false;
                            peer.session = None;
                            peer.gather = None;
                            peer.creds = None;
                            crate::hilog::emit(&format!(
                                "N5c_ICE|locals-failed|peer={}|round={}|phase=answer-late|reason={e}",
                                peer_short(&peer.key),
                                peer.session_at_ms
                            ));
                        }
                    }
                    let _ = self.pump_gather(idx, now_ms, pass_start_ms);
                } else {
                    let peer = &mut self.peers[idx];
                    peer.session.as_mut().expect("session checked").set_remote_credentials(creds.clone())?;
                    peer.remote_creds = Some(creds);
                    // 对端已应答我方的 OFFER：停止重发 OFFER，只保留候选
                    // trickle（任务 §6.4 —— 修复前 HANDSHAKE_RETRY_MS 把
                    // OFFER 一并重发，8 min 发出 ~160 次，对端逐条幂等应答，
                    // 每轮重摆 glare/凭证状态）。ANSWER/候选帧的重发语义
                    // 不变（对端 answer 路径按 Offer 幂等）。
                    peer.signaled.retain(|(k, _)| *k != PeerSignalKind::Offer);
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
        // 0051 §7 item 2：收到该 peer 任何信令（offer/answer/candidate）＝远端
        // 不是静默的 → 退避立即复位、回到正常节奏；`rx_since_timeout` 同时
        // 打断「连续静默」计数（一条 candidate 也算远端还活着）。应答路径
        // 从不经过退避（上面的会话建立/收集是即时的），退避只影响**下一次
        // 主动发起**的时刻。
        self.peers[idx].rx_since_timeout = true;
        self.reset_initiate_backoff(idx, now_ms, "signal");
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
        // signal 流重连（false→true 的边沿）：断流期间的「远端静默」推断作废
        // ——本端可能只是收不到帧。带注入时钟复位，避免退避延迟继续生效。
        if self.signal_reconnected {
            self.signal_reconnected = false;
            for idx in 0..self.peers.len() {
                self.reset_initiate_backoff(idx, now_ms, "signal-reconnect");
            }
        }
        let mut first_err: Option<ManagementError> = None;
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
            if self.signal_ready {
                // 发起（仅 signal 就绪后；每 peer 冷却后重试；纯应答方不发起）。
                // **没有预算、没有游标**：每个 peer 起自己那一轮收集，互不排队
                // ——ea8ede1 的「每拍最多一个 peer 做阻塞收集 + 轮转游标」是
                // 「收集挡住共享拍」的直接后果，收集离开 pump 后就没有东西
                // 可配给（审计 §2.7 stage 1 的 E1/E2/I7 退休）。
                let want_initiate = {
                    let p = &self.peers[idx];
                    p.initiator && p.session.is_none() && !p.initiated && now_ms >= p.cooldown_until
                };
                if want_initiate {
                    self.peers[idx].initiated = true;
                    match self.start_locals(idx, now_ms, pass_start_ms, "initiate") {
                        // 固定端口（HOST-ONLY，N12a）路径当拍就位；OFFER 已由
                        // `on_locals_ready` 按 `offer_owed` 入列（两条路径同一
                        // 规则，避免重复入列）。
                        Ok(LocalsRun::Gathered) => {}
                        // 收集已在 worker 上跑：候选就绪时由 `pump_gather`
                        // 入列 OFFER（`on_locals_ready`），本拍绝不等它。
                        Ok(LocalsRun::InFlight) => {}
                        Ok(LocalsRun::WaitingForSockets { need, have, waited_ms, log }) => {
                            // 池不足＝等待（0050 §2.6/§8 item 3）：现场把这里
                            // 当成 `Network` 错误上抛，每一拍冒一条
                            // `peer-conn: pump error (network)`；等待态不该
                            // 有错误面。清场照旧、`last_error` 不置位、
                            // 不进 `first_err`、冷却期满按既有节奏重试。
                            if log {
                                self.emit_locals_waiting(idx, "initiate", need, have, waited_ms);
                            }
                            let peer = &mut self.peers[idx];
                            peer.cooldown_until = now_ms + RETRY_COOLDOWN_MS;
                            peer.initiated = false; // 冷却后重试
                            peer.session = None; // Drop 关闭半成品会话的 dup socket
                            peer.gather = None; // 半成品：丢掉在跑的收集（worker 自关 socket）
                            peer.creds = None;
                            peer.remote_creds = None;
                            peer.locals_done = false;
                            peer.started = false;
                            peer.remote_cands_seen = 0;
                            peer.pending_remote.clear();
                            peer.signaled.clear();
                            peer.outbox.clear(); // 半成品候选帧不得替死会话发出
                            peer.retry_at_ms = 0;
                            peer.answer_owed = false;
                            peer.session_at_ms = 0;
                            peer.locals_wait_ms = 0;
                        }
                        Err(e) => {
                            // 0048：收集只在会话已建后才可能失败（gather/绑定，
                            // `take_owned` 队列空即 NoSocket；或一个允许接口都
                            // 没有）。半成品会话必须整段回滚：`want_initiate`
                            // 的门是 `session.is_none()`，留下 `session = Some`
                            // 会让这次谈判永久沉默——`want_locals` 之后只补发
                            // 候选、永不补发凭据，对端因缺远端凭证永不
                            // `start()`。回滚后冷却期满重新走完整发起；期间对端
                            // OFFER 到达则落入正常应答路径（`answer_owed` 语义
                            // 不变，也不会出现「同时发 OFFER 又发 ANSWER」）。
                            let peer = &mut self.peers[idx];
                            let round = peer.session_at_ms;
                            let peer_prefix = peer_short(&peer.key);
                            peer.last_error = Some(ErrorClass::from_management(&e));
                            peer.cooldown_until = now_ms + RETRY_COOLDOWN_MS;
                            peer.initiated = false; // 冷却后重试
                            peer.session = None; // Drop 关闭半成品会话的 dup socket
                            peer.gather = None;
                            peer.creds = None;
                            peer.remote_creds = None;
                            peer.locals_done = false;
                            peer.started = false;
                            peer.remote_cands_seen = 0;
                            peer.pending_remote.clear();
                            peer.signaled.clear();
                            peer.outbox.clear(); // 半成品候选帧不得替死会话发出
                            peer.retry_at_ms = 0;
                            peer.answer_owed = false;
                            peer.session_at_ms = 0;
                            peer.locals_wait_ms = 0;
                            crate::hilog::emit(&format!(
                                "N5c_ICE|locals-failed|peer={peer_prefix}|round={round}|phase=initiate|reason={e}"
                            ));
                            if first_err.is_none() {
                                first_err = Some(e);
                            }
                        }
                    }
                    // 内联执行者（host harness）在同一拍里把 round 跑完了：立刻
                    // 取件，保住「一拍内候选 + OFFER 就位」的既有语义。生产
                    // （线程执行者）这里通常是空队列。
                    if let Err(e) = self.pump_gather(idx, now_ms, pass_start_ms) {
                        if first_err.is_none() {
                            first_err = Some(e);
                        }
                    }
                }
                // 应答方/重试方的本地候选收集（幂等；已在跑 = 什么都不做）
                let want_locals = {
                    let p = &self.peers[idx];
                    p.session.is_some() && !p.locals_done && now_ms >= p.cooldown_until
                };
                if want_locals {
                    match self.start_locals(idx, now_ms, pass_start_ms, "retry") {
                        Ok(LocalsRun::Gathered) | Ok(LocalsRun::InFlight) => {}
                        Ok(LocalsRun::WaitingForSockets { need, have, waited_ms, log }) => {
                            // 同上：等待不是错误，会话保留（`want_locals` 的
                            // 门只看 `session`/`locals_done`），冷却后重试。
                            self.peers[idx].cooldown_until = now_ms + RETRY_COOLDOWN_MS;
                            if log {
                                self.emit_locals_waiting(idx, "retry", need, have, waited_ms);
                            }
                        }
                        Err(e) => {
                            let peer = &mut self.peers[idx];
                            peer.last_error = Some(ErrorClass::from_management(&e));
                            peer.cooldown_until = now_ms + RETRY_COOLDOWN_MS;
                            // 失败分支的观测面（0048：这条路径此前完全无日志）。
                            crate::hilog::emit(&format!(
                                "N5c_ICE|locals-failed|peer={}|round={}|phase=retry|reason={e}",
                                peer_short(&peer.key),
                                peer.session_at_ms
                            ));
                            if first_err.is_none() {
                                first_err = Some(e);
                            }
                        }
                    }
                    if let Err(e) = self.pump_gather(idx, now_ms, pass_start_ms) {
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
            // Checking 看门狗（§6.3）：start 后 [`CHECKING_WATCHDOG_MS`]
            // 无任何检查成功 → 判 failed、拆除、冷却后全新协商。覆盖
            // "all-pairs-failed 被持续流入的候选掩盖、Disconnected 永不
            // 触发"的僵尸 Checking（真机 2026-10-05 形态）。
            self.expire_dead_checking(idx, now_ms);
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

    /// 开始本地候选收集 + 会话绑定（发起方、应答方、重试方共用；幂等：
    /// `locals_done` 已置位 = `Gathered`，本轮已在跑 = `InFlight`）。
    ///
    /// STAGE 1（官方对齐审计 §2.7）：收集**不在本调用里跑**。这里做的事只有
    /// 「建会话（纯内存）+ 枚举允许接口 + 池预检 + 把每个接口 round 交给
    /// [`GatherExec`]」，然后立刻返回。上游同形：`connect` 调
    /// `GatherCandidates()` 后**下一行就 dial**（`worker_ice.go:255-266`），
    /// 而 `GatherCandidates()` 内部 `go a.gatherCandidates(ctx, done)` 立即
    /// 返回（pion fork `gather.go:39-65`，spawn 在 `:59`），每个 (network,
    /// STUN url, listen addr) 一枚 goroutine（`gather.go:491-513`）。
    ///
    /// 两条收集路径：
    /// - **默认**（`fixed_local_port = None`，设备与既有主机行为）：每接口
    ///   一枚受保护 socket、临时端口，`round` 由 worker 跑
    ///   （[`crate::ice::gather_interface_round`]）；round 完成后**不关闭**
    ///   已绑定的 socket，而是随 [`IfaceGather::socket`] 回报，由
    ///   [`IceSession::add_local_candidate_bound`] 直接接管为本地候选。
    ///   于是 advertise 出去的 host 端口与 srflx 的 `raddr/rport`（base）
    ///   就是会话真正收发会话包的那枚 socket——修复「gather 关 socket +
    ///   另绑新端口，srflx 指向死映射」的 0052 缺陷；
    /// - **固定端口**（HOST-ONLY，N12a）：不做 per-interface 收集也不做
    ///   STUN（端口映射场景里显式对外候选取代 srflx），取**首个允许
    ///   接口地址**一枚候选、以 [`IceSession::add_local_candidate_fixed_port`]
    ///   通配绑定 `0.0.0.0:<port>`（转发目的地址不可预知，通配绑定才使
    ///   固定端口可达；上游同形：单一 `0.0.0.0:NET_PORT` 共享 socket），
    ///   候选保留接口地址 + 固定端口（getsockname 回填校验）。这条路径没有
    ///   阻塞点（只 bind 一枚 socket，不发 STUN），因此仍是同步完成。
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
        let fixed_port = self.fixed_local_port;
        // 0049 §5.1-2（活锁，关键）：受保护 UDP socket 池由壳层单向补给（池
        // 目标 8、每批 ≤4），而一次成功收集要「允许接口数」枚（每接口
        // gather 一枚；那枚已绑定的 socket 由会话直接接管，不再另取第二枚
        // 绑定长留——0052 修复把每接口 2 枚降到 1 枚）。失败路径仍是「先消耗
        // 再失败、不回流」，于是池里 1..need-1 枚的尝试会把它们全部取走仍然
        // 失败 —— 每批补给都被注定失败的尝试吃掉，池**永远攒不到 need**：
        // 实测切网后 18 s 补给 20 枚、0 次成功。这里在任何 `take_owned`
        // 之前先按「gather 将要走的接口清单」算本次预算，不足即**一枚都不
        // 消耗**地进入等待（0050 §8 item 3 把原来的可重试**错误**改成
        // `LocalsRun::WaitingForSockets`——沿用既有 cooldown 重试节奏，不
        // 新增定时器），把池让给后续批次去攒。
        //
        // 与阶段 1 的关系（审计 §2.7）：这份「预算」不是 ea8ede1 那个**每拍
        // 一次**的时间预算（那个随本阶段删除），而是**每 peer 一轮**的 socket
        // 数量预检——池还在（stage 2 起被 mux 取代），所以它暂时留着。注意它
        // 现在是预检而非预留：worker 与预检并发取 socket，池可能在中途见底，
        // 那时该接口 round 记一条 per-round 错误、其余接口照常（fail-closed
        // 不变：整轮零候选仍然是硬失败，见 [`Self::finish_gather`]）。
        let required = match fixed_port {
            // 固定端口路径整个收集阶段只消耗一枚（首个候选的通配绑定）。
            Some(_) => usize::from(self.allowed_iface_addrs()?.first().is_some()),
            None => self.required_local_sockets()?,
        };
        if let Some(have) = self.socks.available() {
            if have < required {
                // 池不足 = **等待补给**，不是错误（0050 §8 item 3）：一枚都
                // 不取，记等待账（供 `negotiation_age_ms` 扣除），把池让给
                // 后续批次去攒。
                let (waited_ms, log) = self.note_locals_wait(idx, now_ms);
                return Ok(LocalsRun::WaitingForSockets { need: required, have, waited_ms, log });
            }
        }
        if fixed_port.is_some() {
            self.gather_fixed_port(idx, now_ms, pass_start_ms, phase)?;
            self.clear_locals_wait(idx, now_ms);
            return Ok(LocalsRun::Gathered);
        }
        // 允许接口清单就是这一轮的并发单位（与 `required_local_sockets`
        // 同一份枚举/黑名单/去重）。
        let cfg = GatherConfig {
            blacklist: &self.blacklist,
            servers: &self.stuns,
            timeout_ms: DEFAULT_STUN_TIMEOUT_MS,
        };
        let ifaces = crate::ice::allowed_ifaces(&cfg, self.ifaces.as_ref())?;
        if ifaces.is_empty() {
            // 一个允许接口都没有：与旧路径同形（gather 无候选 → 整段回滚）。
            return Err(ManagementError::Network("ice:no-bindable-local-candidate".into()));
        }
        let (tx, rx) = std::sync::mpsc::channel::<IfaceGather>();
        let mut pending = 0usize;
        for iface in ifaces {
            let tx = tx.clone();
            let socks = self.socks.clone();
            let servers = self.stuns.clone();
            let timeout_ms = DEFAULT_STUN_TIMEOUT_MS;
            let spawned = self.exec.spawn(Box::new(move || {
                let round = crate::ice::gather_interface_round(
                    &iface,
                    &servers,
                    socks.as_ref(),
                    timeout_ms,
                );
                // send 失败 = 接收端已丢（会话被拆/peer 被移除）：`round`
                // 随之 drop，其 socket 在这一行关闭——绝不留孤儿 fd。
                let _ = tx.send(round);
            }));
            if spawned {
                pending += 1;
            }
        }
        drop(tx); // 只剩 worker 的发送端
        if pending == 0 {
            // 一枚 round 都起不来（线程上限）：当作这一轮没有候选，
            // fail-closed 交给上面的回滚/冷却语义——绝不回落成阻塞调用。
            return Err(ManagementError::Network("ice:no-bindable-local-candidate".into()));
        }
        self.peers[idx].gather = Some(GatherInFlight {
            rx,
            pending,
            started_wall_ms: crate::sys::mono_ms(),
            added: 0,
            srflx: 0,
            errors: Vec::new(),
            phase,
            offer_owed: matches!(phase, "initiate" | "answer-late"),
        });
        // 供给到了（收集已开跑）：本次等待结束，把它记进本会话的等待账
        //（`negotiation_age_ms` 扣除的就是这笔账），并让下一段等待的首行重新可记。
        self.clear_locals_wait(idx, now_ms);
        Ok(LocalsRun::InFlight)
    }

    /// 建会话（三条路径共用：收到 OFFER 应答、主动发起、迟到 ANSWER 采纳）。
    /// 纯内存：`IceSession::new` + 凭证生成，不取 socket、不发 STUN。
    fn ensure_session(
        &mut self,
        idx: usize,
        now_ms: u64,
        phase: &'static str,
    ) -> Result<(), ManagementError> {
        if self.peers[idx].session.is_some() {
            return Ok(());
        }
        let creds = IceCredentials::generate()?;
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
        peer.creds = Some(creds);
        peer.session = Some(session);
        peer.session_at_ms = now_ms;
        // 新会话的等待账从零起算：`negotiation_age_ms` 只扣本会话内的等待。
        peer.locals_wait_ms = 0;
        peer.checking_since_ms = 0;
        Ok(())
    }

    /// 固定端口（HOST-ONLY，N12a）收集：一枚通配绑定 + 显式对外候选。
    /// 同步完成（无 STUN），成功后置 `locals_done` 并补 ANSWER/OFFER。
    fn gather_fixed_port(
        &mut self,
        idx: usize,
        now_ms: u64,
        pass_start_ms: u64,
        phase: &'static str,
    ) -> Result<(), ManagementError> {
        let port = self.fixed_local_port.expect("fixed-port path only");
        let fixed_addr = self.allowed_iface_addrs()?.first().copied();
        let mut added = 0usize;
        if let Some(addr) = fixed_addr {
            let peer = &mut self.peers[idx];
            let session = peer.session.as_mut().expect("session just ensured");
            // 固定端口：候选保留指定端口（绑定失败 = 端口被占，fail-closed
            // 硬错误——绝不静默回落临时端口）。
            let fresh = Candidate::host_candidate(addr, port);
            let bound = session.add_local_candidate_fixed_port(fresh, self.socks.as_ref())?;
            peer.outbox.push_back((PeerSignalKind::Candidate, bound.marshal()));
            peer.locals_signaled += 1;
            added += 1;
        }
        if added == 0 {
            return Err(ManagementError::Network("ice:no-bindable-local-candidate".into()));
        }
        // `gather_ms` = 0（固定端口路径不跑 STUN）；`pass_ms` 用调用方本拍的
        // 墙钟起点——两条路径共用同一行格式（STAGE 0）。OFFER 的归属与异步
        // 路径同一规则（`offer_owed`）:发起会话在候选就绪时补 OFFER。
        let offer_owed = matches!(phase, "initiate" | "answer-late");
        self.on_locals_ready(idx, now_ms, pass_start_ms, offer_owed);
        Ok(())
    }

    /// STAGE 1 的**取件点**：把 worker 已回报的接口 round 交给会话，非阻塞
    /// （`try_recv`，队列空即返回）。候选**按到达顺序**入 outbox（trickle，
    /// 上游 `onICECandidate` 每收集到一个候选就发一帧，`worker_ice.go:420-
    /// 470`）；首次拿到候选时补 OFFER/ANSWER（`on_locals_ready`）；整轮回报
    /// 完毕且零候选时按发起路径回滚（`finish_gather`）。
    ///
    /// 返回 `Err` 的语义与旧的同步 `ensure_locals` 一致：只有发起/重试路径
    /// 的**整轮零候选**才冒错（应答路径只记 `last_error` + 冷却）。
    fn pump_gather(
        &mut self,
        idx: usize,
        now_ms: u64,
        pass_start_ms: u64,
    ) -> Result<(), ManagementError> {
        let mut rounds: Vec<IfaceGather> = Vec::new();
        if let Some(g) = self.peers[idx].gather.as_ref() {
            while let Ok(round) = g.rx.try_recv() {
                rounds.push(round);
            }
        }
        let mut err: Option<ManagementError> = None;
        for round in rounds {
            let mut became_ready = false;
            {
                let peer = &mut self.peers[idx];
                let Some(g) = peer.gather.as_mut() else { break };
                g.pending = g.pending.saturating_sub(1);
                // 会话在这轮收集期间被拆/重建 = 这一轮整个作废：它的 socket 与
                // 候选都属于上一次协商，绝不塞进新会话（socket 随 `round`
                // 在本块末尾 drop，即关闭）。
                let deliver = peer.session.is_some();
                if let Some(sock) = round.socket {
                    if deliver {
                        let host_name = format!("{}:{}", sock.cand.address, sock.cand.port);
                        // 这枚 socket 既是 host 候选、又是该接口 srflx 的 base：
                        // 交给会话长留（getsockname 端口校验在里面）。
                        match peer
                            .session
                            .as_mut()
                            .expect("session checked")
                            .add_local_candidate_bound(sock.cand, sock.fd)
                        {
                            Ok(bound) => {
                                peer.outbox
                                    .push_back((PeerSignalKind::Candidate, bound.marshal()));
                                peer.locals_signaled += 1;
                                g.added += 1;
                                became_ready = !peer.locals_done;
                            }
                            // 会话已 stop：socket 已被关掉（
                            // `add_local_candidate_bound` 的清理路径），记一条
                            // 原因、继续。
                            Err(e) => g.errors.push((host_name, e)),
                        }
                    }
                    // `deliver == false`：`sock` 在这里 drop → 关闭。
                }
                if deliver {
                    // srflx：与上面 host 候选同一枚 socket 的 STUN 映射，
                    // base = raddr/rport（socket 活着 → 映射有背书）。
                    g.srflx += round.srflx.len();
                    for cand in round.srflx {
                        peer.outbox.push_back((PeerSignalKind::Candidate, cand.marshal()));
                    }
                }
                g.errors.extend(round.errors);
            }
            if became_ready {
                let offer_owed = self.peers[idx]
                    .gather
                    .as_ref()
                    .map(|g| g.offer_owed)
                    .unwrap_or(false);
                self.on_locals_ready(idx, now_ms, pass_start_ms, offer_owed);
            }
            let done = {
                let peer = &self.peers[idx];
                peer.gather.as_ref().map(|g| g.pending == 0).unwrap_or(true)
            };
            if done {
                err = self.finish_gather(idx, now_ms);
            }
        }
        match err {
            Some(e) => Err(e),
            None => Ok(()),
        }
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
            // 发起方（主动发起；或迟到 ANSWER 采纳后重新发起）：候选就绪即
            // 入列 OFFER——上游 `GatherCandidates()` 后立刻 dial 的等价物。
            // 用 `offer_owed`（发起时记录）而不是 `peer.initiated`：后者在
            // 一次成功发起后会一直为真，拿它当判据会在这之后的**应答**会话
            // （对端新 OFFER 触发的重建）里多发一个 OFFER = 上游没有的 glare。
            if offer_owed {
                if let Some(creds) = peer.creds.as_ref() {
                    let payload = format!("{}:{}", creds.ufrag, creds.pwd);
                    peer.outbox.push_back((PeerSignalKind::Offer, payload));
                }
            }
            // 应答方（收到 OFFER 建立会话）：候选就绪即补 ANSWER——上游
            // handshaker 收到 Offer 后无条件 `sendAnswer()`（handshaker.go:
            // 150-156），与候选收集无关。
            if peer.answer_owed {
                if let Some(creds) = peer.creds.as_ref() {
                    let payload = format!("{}:{}", creds.ufrag, creds.pwd);
                    peer.outbox.push_back((PeerSignalKind::Answer, payload));
                }
                peer.answer_owed = false;
            }
        }
        self.maybe_start(idx, now_ms);
        self.clear_locals_wait(idx, now_ms);
    }

    /// 整轮收集回报完毕（`pending == 0`）的结算：零候选 = fail-closed，按
    /// 发起时记录的 `phase` 选回滚语义（与旧同步路径逐字对齐）：
    /// - `initiate`：整段回滚半成品会话 + 冷却 + `locals-failed` + 冒错
    ///   （调用方的 `first_err`）；
    /// - `retry`：保留会话，冷却后重试 + `locals-failed` + 冒错；
    /// - `answer` / `answer-late`：保留会话（`answer_owed` 语义不变），冷却
    ///   后重试 + `locals-failed`，**不冒错**（旧 `handle_signal` 的语义）。
    /// 有候选则收集结束，什么都不做（后续补候选由新一轮协商负责）。
    fn finish_gather(&mut self, idx: usize, now_ms: u64) -> Option<ManagementError> {
        let (added, phase, errors) = match self.peers[idx].gather.take() {
            Some(g) => (g.added, g.phase, g.errors),
            None => return None,
        };
        if added > 0 {
            return None;
        }
        let e = errors
            .into_iter()
            .next()
            .map(|(_, e)| e)
            .unwrap_or_else(|| {
                ManagementError::Network("ice:no-bindable-local-candidate".into())
            });
        match phase {
            "initiate" => {
                let peer = &mut self.peers[idx];
                let round = peer.session_at_ms;
                let peer_prefix = peer_short(&peer.key);
                peer.last_error = Some(ErrorClass::from_management(&e));
                peer.cooldown_until = now_ms + RETRY_COOLDOWN_MS;
                peer.initiated = false; // 冷却后重试
                peer.session = None; // Drop 关闭半成品会话的 dup socket
                peer.creds = None;
                peer.remote_creds = None;
                peer.locals_done = false;
                peer.started = false;
                peer.remote_cands_seen = 0;
                peer.pending_remote.clear();
                peer.signaled.clear();
                peer.outbox.clear(); // 半成品候选帧不得替死会话发出
                peer.retry_at_ms = 0;
                peer.answer_owed = false;
                peer.session_at_ms = 0;
                peer.locals_wait_ms = 0;
                crate::hilog::emit(&format!(
                    "N5c_ICE|locals-failed|peer={peer_prefix}|round={round}|phase=initiate|reason={e}"
                ));
                Some(e)
            }
            "retry" => {
                let peer = &mut self.peers[idx];
                peer.last_error = Some(ErrorClass::from_management(&e));
                peer.cooldown_until = now_ms + RETRY_COOLDOWN_MS;
                // 失败分支的观测面（0048：这条路径此前完全无日志）。
                crate::hilog::emit(&format!(
                    "N5c_ICE|locals-failed|peer={}|round={}|phase=retry|reason={e}",
                    peer_short(&peer.key),
                    peer.session_at_ms
                ));
                Some(e)
            }
            other => {
                let peer = &mut self.peers[idx];
                peer.last_error = Some(ErrorClass::from_management(&e));
                peer.cooldown_until = now_ms + RETRY_COOLDOWN_MS;
                // 应答方收集失败同样不得静默：`answer_owed` 仍在，
                // `want_locals` 冷却后会重试补 ANSWER。
                crate::hilog::emit(&format!(
                    "N5c_ICE|locals-failed|peer={}|round={}|phase={other}|reason={e}",
                    peer_short(&peer.key),
                    peer.session_at_ms
                ));
                None
            }
        }
    }

    /// 记录 peer 进入「池不足，等补给」：返回 `(已等待时长, 本拍是否记观测
    /// 行)`。`waited_ms` 自**本次等待的起点**连续累计（跨会话重建不重置，
    /// 见 [`PeerConn::locals_wait_since_ms`]），所以它报的是池饿了多久。首行
    /// 必记，其后按 [`LOCALS_WAIT_LOG_MS`] 限频。
    fn note_locals_wait(&mut self, idx: usize, now_ms: u64) -> (u64, bool) {
        let p = &mut self.peers[idx];
        if p.locals_wait_since_ms == 0 {
            p.locals_wait_since_ms = now_ms;
        }
        let waited = now_ms.saturating_sub(p.locals_wait_since_ms);
        let due = p.locals_wait_log_ms == 0
            || now_ms.saturating_sub(p.locals_wait_log_ms) >= LOCALS_WAIT_LOG_MS;
        if due {
            p.locals_wait_log_ms = now_ms;
        }
        (waited, due)
    }

    /// 等待结束（本次收集成功）：把这一段等待归入本会话的等待账。
    fn clear_locals_wait(&mut self, idx: usize, now_ms: u64) {
        let p = &mut self.peers[idx];
        if p.locals_wait_since_ms != 0 {
            // 只把落在**本会话**内的等待记进账（会话之前的等待不算本会话的）。
            let wait_start = p.locals_wait_since_ms.max(p.session_at_ms);
            p.locals_wait_ms = p.locals_wait_ms.saturating_add(now_ms.saturating_sub(wait_start));
            p.locals_wait_since_ms = 0;
        }
        p.locals_wait_log_ms = 0;
    }

    /// 「池不足，等补给」的观测行（三处调用点共用同一字段顺序；行尾
    /// `reason=insufficient-sockets` 让既有的 token 检索仍然命中——它只是
    /// 不再是错误、不再走 `locals-failed`）。
    fn emit_locals_waiting(
        &self,
        idx: usize,
        phase: &str,
        need: usize,
        have: usize,
        waited_ms: u64,
    ) {
        let peer = &self.peers[idx];
        crate::hilog::emit(&format!(
            "N5c_ICE|locals-waiting|peer={}|round={}|phase={phase}|need={need}|have={have}|waited_ms={waited_ms}|reason=insufficient-sockets",
            peer_short(&peer.key),
            peer.session_at_ms
        ));
    }

    /// 一次成功收集的 socket 预算（默认路径）＝ gather 将要访问的接口数——
    /// 每接口**恰好一枚**：gather 绑定的那枚 socket 不再关闭，而是随
    /// [`crate::ice::GatheredSocket`] 交给会话接管
    /// （[`IceSession::add_local_candidate_bound`]），所以不再需要「每个
    /// host 候选再取一枚绑定长留」。接口清单与 [`crate::ice::allowed_ifaces`]
    /// 同源，绝不与 gather 的实际遍历漂移。
    ///
    /// 旧账（0052 修复前）：每接口 2 枚 = gather 1 + 绑定长留 1；那第二枚绑
    /// 到**新**的临时端口，正是「srflx 映射指向死 socket」缺陷的一半。
    ///
    /// 上限（0049 残余风险）：壳层池目标 8、每批 ≤4，池深实际封顶在 8，
    /// 所以 `required > 8`（同时有 ≥9 个允许接口）时守护会一直等待。本项
    /// 修复后设备侧接口数由 3 降到 2（overlay 被黑名单挡掉）→ required=2。
    fn required_local_sockets(&self) -> Result<usize, ManagementError> {
        let cfg = GatherConfig {
            blacklist: &self.blacklist,
            servers: &self.stuns,
            timeout_ms: DEFAULT_STUN_TIMEOUT_MS,
        };
        Ok(crate::ice::allowed_ifaces(&cfg, self.ifaces.as_ref())?.len())
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
    /// `now_ms`（注入时钟）用于武装 Checking 看门狗（[`CHECKING_WATCHDOG_MS`]）。
    fn maybe_start(&mut self, idx: usize, now_ms: u64) {
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
            // Failed→Checking：拆除后的重谈判也要如实回到 Checking——
            // 否则状态永远停在上一会话的 Failed（观测面失真），且
            // Checking 看门狗（`expire_dead_checking`）永远不再覆盖
            // 重谈判出来的会话。
            self.peers[idx].state = PeerIceState::Checking;
        }
        if self.peers[idx].started && self.peers[idx].checking_since_ms == 0 {
            self.peers[idx].checking_since_ms = now_ms;
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
                IceEvent::CheckSucceeded { remote, rtt_ms, .. } => {
                    // 修复前这里是 `=> {}`（§1.3：最大的观测盲区——整份真机
                    // 日志无法区分"检查从未发出"还是"发出未回应"）。一行
                    // 简摘 + 滑动重置 Checking 看门狗（有检查在成功 = 会话
                    // 没有僵死，不能按看门狗拆）。
                    let peer = &mut self.peers[idx];
                    peer.checking_since_ms = now_ms;
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

    /// 拆除当前会话并重新武装谈判（`IceEvent::Failed` 与僵尸谈判超时
    /// [`Self::expire_dead_negotiation`] 共用）：peer 不可达、回收 WG
    /// 端点与 egress、Drop 关闭会话全部 dup socket、清空在途信令状态、
    /// 冷却后允许重新发起。`reason` 只进日志（公开诊断材料）。
    fn drop_session_rearm(&mut self, idx: usize, now_ms: u64, reason: &str) {
        let peer = &mut self.peers[idx];
        let round = peer.session_at_ms;
        let peer_prefix = peer.key.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect::<String>();
        peer.state = PeerIceState::Failed;
        peer.endpoint_applied = false;
        peer.selected_remote = None;
        self.wg.recycle_endpoint(&peer.key);
        peer.session = None; // Drop 关闭该会话全部 dup socket
        // 在跑的收集属于这个死会话：丢接收端（worker 收尾时自关其 socket），
        // 免得它的候选落进下一次协商。
        peer.gather = None;
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
        peer.locals_wait_ms = 0; // 等待账随会话作废（下一会话从零起算）
        peer.checking_since_ms = 0; // 看门狗随会话作废
        peer.last_health_ms = 0;
        peer.cooldown_until = now_ms + RETRY_COOLDOWN_MS;
        // 0051 §7 item 2：连续的「远端静默」超时→指数退避拉长下一次发起。
        // 只统计 `negotiation-start-timeout`（会话已建、本地候选已发、远端
        // 凭证/候选从未到达）：Failed 事件、网络切换、诊断断链都不是「对端
        // 静默」的证据，不得计入。
        let enter_backoff = if reason == "negotiation-start-timeout" {
            note_silent_timeout(peer, now_ms)
        } else {
            None
        };
        // The payload carries WHY the session died — device run 3
        // needed exactly this and the marker alone said nothing.
        crate::hilog::emit(&format!(
            "N11_ICE|failed|session-dropped|renegotiation-armed|reason={reason}|peer={peer_prefix}|round={round}"
        ));
        if let Some((misses, next_ms)) = enter_backoff {
            crate::hilog::emit(&initiate_backoff_line(&peer_prefix, misses, next_ms));
        }
    }

    /// 复位「远端静默」退避（0051 §7 item 2）：收到该 peer 任何信令或网络
    /// 切换/重连事件时调用。`backoff-reset` 行只在**活动**退避上记一行
    /// （复位即清零，天然限频）；有活动退避时把冷却拉回正常节奏
    /// （至多 `RETRY_COOLDOWN_MS`），否则不动既有的短冷却。
    fn reset_initiate_backoff(&mut self, idx: usize, now_ms: u64, reason: &str) {
        let peer = &mut self.peers[idx];
        if !peer.backoff_active && peer.backoff_misses == 0 && peer.backoff_logged_misses == 0 {
            return;
        }
        let was_active = peer.backoff_active;
        peer.backoff_misses = 0;
        peer.backoff_active = false;
        peer.backoff_logged_misses = 0;
        if was_active {
            peer.cooldown_until =
                peer.cooldown_until.min(now_ms.saturating_add(RETRY_COOLDOWN_MS));
            let peer_prefix = peer_short(&peer.key);
            crate::hilog::emit(&backoff_reset_line(&peer_prefix, reason));
        }
    }

    /// 僵尸谈判拆除（H3 修复，[`START_DEADLINE_MS`]）：会话已建、本地
    /// 候选已发，但 `start()` 始终没有发生 —— 判据只认「未 start」，
    /// **不再区分远端有没有应答**：
    ///
    /// - 远端从未应答（`remote_creds` 为空）：典型是对端仍持有旧会话、
    ///   自认 Connected、不再发 Offer/Answer；0048 形态：对端只 trickle
    ///   候选、从不发凭据（本端候选照收、凭证为零，`maybe_start` 永远不
    ///   满足）。
    /// - **远端已应答但零候选**（`remote_creds` 有、`remote_cands_seen == 0`）：
    ///   `maybe_start` 还要求 ≥1 个远端候选，于是会话永远停在
    ///   `session = Some && started == false`，`IceSession::run_once` 因
    ///   `!started` 立即返回、永不收包，socket 被永久占住。真机
    ///   round-230622244：手机 07:09:03 发本地候选、07:09:04 收到 Answer、
    ///   此后零候选，9 min 内没有任何重试。修复前的 `remote_creds.is_none()`
    ///   守卫把「已应答」误当成健康，正是这条停摆。
    ///
    /// 两种情况都没有事件面可拆会话（`want_initiate` 要 `session.is_none()`，
    /// `want_locals` 要 `!locals_done`），到期即拆除重武装：冷却后以全新
    /// 凭证重新发起，对端会把新凭证的 Offer 采纳为新谈判并应答 + trickle
    /// 候选。已 start 的会话不适用——其死亡由 keepalive/断连/全对失败/
    /// [`CHECKING_WATCHDOG_MS`] 诚实驱动。
    ///
    /// 计时用 [`PeerConn::negotiation_age_ms`] 而不是裸的
    /// `now - session_at_ms`（0050 §8 item 3）：等受保护 socket 补给的
    /// 时间不算「对端不应答」，不能拿它去拆会话。
    ///
    /// 退避口径不变：`drop_session_rearm` 按
    /// `reason=negotiation-start-timeout` 调用 `note_silent_timeout`，而
    /// 后者只在**期间没有收到任何信令**（`rx_since_timeout` 为假）时才累计
    /// 「远端静默」——已应答过的 peer 收到过 Answer，不计入退避，只走既有的
    /// [`RETRY_COOLDOWN_MS`] 节奏。
    fn expire_dead_negotiation(&mut self, idx: usize, now_ms: u64) {
        let zombie = {
            let p = &self.peers[idx];
            p.session.is_some()
                && !p.started
                && p.session_at_ms != 0
                && p.negotiation_age_ms(now_ms) >= START_DEADLINE_MS
        };
        if zombie {
            self.drop_session_rearm(idx, now_ms, "negotiation-start-timeout");
        }
    }

    /// Checking 看门狗（§6.3，[`CHECKING_WATCHDOG_MS`]）：会话已 start、
    /// 处于 Checking，但自 `checking_since_ms` 起没有任何检查成功——
    /// 判 failed、拆除会话（`drop_session_rearm` → 冷却后全新协商）。
    ///
    /// 为什么 start 后的会话也需要拆除器（真机 2026-10-05，§2.3）：
    /// - `all-pairs-failed` 要求**所有** pair 同时 Failed，而远端候选
    ///   持续流入会不断产生新的 Waiting pair + 入站有效检查会把 Failed
    ///   回暖为 Waiting —— 在线对端的会话**永不** all-failed；
    /// - `Disconnected` 只在收到过有效入站后才可能触发
    ///  （`last_inbound_ms = None` 时整个定时器块被跳过）；
    /// - 于是 start 后永不成功的会话没有任何终态信号，只能靠外因
    ///   （网络切换）拆除 —— 对端看到我们永远 "Connecting"。
    ///
    /// 每次检查成功（[`IceEvent::CheckSucceeded`]）滑动重置计时
    /// （`drain_events`）：还在取得进展的慢会话不会被误杀。
    fn expire_dead_checking(&mut self, idx: usize, now_ms: u64) {
        let dead = {
            let p = &self.peers[idx];
            p.session.is_some()
                && p.started
                && p.state == PeerIceState::Checking
                && p.checking_since_ms != 0
                && now_ms.saturating_sub(p.checking_since_ms) >= CHECKING_WATCHDOG_MS
        };
        if dead {
            self.drop_session_rearm(idx, now_ms, "checking-timeout");
        }
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

    /// Force fresh ICE credentials, candidates and protected sockets after an
    /// outer-network change; the next pump renegotiates each known peer.
    ///
    /// 0051 §7 item 2：网络切换同样复位「远端静默」退避——切换后旧的静默
    /// 推断作废（本端接口/端口全变，对端也没机会应答），必须立刻回到正常
    /// 节奏重新尝试。
    pub fn restart_for_network_change(&mut self, now_ms: u64) {
        for idx in 0..self.peers.len() {
            self.drop_session_rearm(idx, now_ms, "physical-network-change");
            self.peers[idx].cooldown_until = now_ms;
            self.reset_initiate_backoff(idx, now_ms, "network-change");
        }
    }

    /// N2 T5a（§4 B 列 I 行）：端点定向断链。每个有会话/已选端点的 peer 都被
    /// 拆除当前会话（`drop_session_rearm`：`peer.session = None` 由 `IceSession`
    /// 的 `Drop` 关闭其全部 dup socket），并清掉冷却，使下一次 pump 立即重新
    /// gather/check/signal —— 即冻结的「关闭该 ICE 外层 socket 触发重建」。
    /// 返回实际被处理的 peer 数（0 = 无可断的会话 → `no-target`）。
    pub fn break_ice(&mut self, now_ms: u64) -> usize {
        let mut acted = 0;
        for idx in 0..self.peers.len() {
            let live = self.peers[idx].session.is_some() || self.peers[idx].selected_remote.is_some();
            if !live {
                continue;
            }
            self.drop_session_rearm(idx, now_ms, "n2-stimulus-I");
            self.peers[idx].cooldown_until = now_ms;
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

    /// 0050 §8 item 3：**池不足 = 等待，不是错误**。
    ///
    /// 旧语义（至 f5ae3d8）把池不足当 `Network` 错误上抛，于是 ICE pump 每拍
    /// 冒一条 `peer-conn: pump error (network)`（真机 0050 discover 14 行）。
    /// 新语义：`run_once` 保持 `Ok`、不置 `last_error`、一枚 socket 都不取、
    /// 不发任何帧（fail-closed 不变），冷却期满按既有节奏重试，补给一到即
    /// 完成收集并发 OFFER。
    #[test]
    fn empty_socket_pool_waits_instead_of_failing_closed() {
        // 空 provider：发起停在池预检处（等待），绝不发出任何帧
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
        // 第一拍：池空 → 等待（Ok、无 pump error、无 last_error），半成品
        // 会话照旧整段回滚（0048：留着会让 `want_initiate` 永久沉默）。
        assert!(orch.run_once(now).is_ok(), "an empty pool is WAITING, not a pump error");
        {
            let p = &orch.peers[0];
            assert!(p.locals_wait_since_ms != 0, "the peer must be recorded as waiting for supply");
            assert_eq!(p.session_at_ms, 0, "the half-built session is rolled back (0048)");
            assert_eq!(p.locals_wait_ms, 0, "no session → no session-scoped wait accounting");
        }
        // 冷却窗口内的后续拍不重试、不再报错（退避语义）
        for _ in 0..4 {
            now += 100;
            assert!(orch.run_once(now).is_ok(), "cooldown window must suppress the retry");
        }
        // 冷却结束：仍不报错，仍在等待（同一段等待记账）
        now += RETRY_COOLDOWN_MS;
        assert!(orch.run_once(now).is_ok(), "a starved retry must stay a wait, not an error");
        assert!(orch.peers[0].locals_wait_since_ms != 0, "still waiting");
        assert!(
            signal.frames.lock().expect("frames").is_empty(),
            "no frame may leave without candidates"
        );
        assert_eq!(socks.taken(), 0, "a waited-out attempt must hand out nothing");
        let st = orch.peer_status("P").expect("peer");
        assert_eq!(st.state, PeerIceState::Idle, "never leaves Idle without candidates");
        assert_eq!(st.last_error, None, "waiting is not an error (0050)");
        assert!(!ice_ready_for_default_route(&orch.summary()), "unreachable peer must not arm the gate");
        // 补给到达（`need = 1 × 1` 个允许接口）：下一次尝试完成收集并发 OFFER。
        for _ in 0..1 {
            let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            assert!(fd >= 0, "socket() failed");
            socks.feed(fd).expect("feed");
            unsafe { sys::close(fd) };
        }
        now += RETRY_COOLDOWN_MS;
        assert!(orch.run_once(now).is_ok(), "the supplied peer must converge");
        assert!(orch.peers[0].locals_done, "the top-up must complete local gathering");
        assert_eq!(socks.taken(), 1, "one interface → exactly one adopted gather socket");
        assert_eq!(orch.peers[0].locals_wait_since_ms, 0, "supply ends the waiting state");
        assert!(
            signal
                .frames
                .lock()
                .expect("frames")
                .iter()
                .any(|(_, k, _)| *k == PeerSignalKind::Offer),
            "the supplied peer must signal an OFFER"
        );
    }

    /// 0052：默认路径 advertise 出去的 host 候选端口，必须由 **gather 绑定、
    /// 会话直接接管**（[`IceSession::add_local_candidate_bound`]）的那枚仍活着
    /// 的 socket 持有——修复前 gather 当场关掉它、ensure_locals 再取第二枚绑到
    /// 新的临时端口，于是 signal 出去的是另一个 socket 的端口，srflx 的 base
    /// 指向已关闭的映射。
    #[test]
    fn signaled_host_port_is_backed_by_the_live_adopted_socket() {
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

        // The candidate that was actually signaled.
        let frames = signal.frames.lock().expect("frames").clone();
        let cand = frames
            .iter()
            .filter(|(_, k, _)| *k == PeerSignalKind::Candidate)
            .map(|(_, _, p)| Candidate::unmarshal(p).expect("wire form"))
            .next()
            .expect("a candidate frame");
        assert_ne!(cand.port, 0, "the signaled candidate carries the bound port");

        // The session's local candidate IS that signaled candidate, and its
        // socket still holds the advertised address:port right now.
        let session = orch.peers[0].session.as_ref().expect("session");
        assert_eq!(session.local_candidates(), vec![cand.clone()]);
        let sa = sys::sockaddr_in::new([127, 0, 0, 1], cand.port);
        let len = core::mem::size_of::<sys::sockaddr_in>() as u32;
        let probe = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
        assert!(probe >= 0, "socket() failed");
        assert_eq!(
            unsafe { sys::bind(probe, &sa, len) },
            -1,
            "the advertised port must still be held by the live adopted socket"
        );
        unsafe { sys::close(probe) };
        // One interface = exactly ONE protected socket (the gather's), which
        // the session adopted: no second bind.
        assert_eq!(socks.taken(), 1, "no second socket for the same interface");

        // stop() closes the adopted fd exactly once → the port is released.
        orch.peers[0].session.as_mut().expect("session").stop();
        let probe = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
        assert!(probe >= 0, "socket() failed");
        assert_eq!(
            unsafe { sys::bind(probe, &sa, len) },
            0,
            "session stop must close the adopted fd (advertised port released)"
        );
        unsafe { sys::close(probe) };
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
        assert_eq!(live.live_local_sockets, 1, "one interface → one adopted socket");

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
    ///   （handshaker.go:150-156 / worker_ice.go:100-155）。因此这里**开启一轮
    ///   协商**（建会话 + 采纳远端凭证 + 以全新本地凭证重新发起 OFFER），
    ///   而不是丢弃计数；`signal-no-session` 行随之删除（上游没有对应行）。
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
        // 无会话的 ANSWER：开启新协商（采纳凭证 + 重新发起），不计入 unknown_signal
        orch.handle_signal("P", PeerSignalKind::Answer, "abcd:0123456789012345678901", 20)
            .expect("answer accepted");
        assert_eq!(orch.unknown_signal(), 0, "upstream has no drop-on-no-session branch");
        assert!(orch.peers[0].session.is_some(), "the late ANSWER starts a negotiation");
        assert!(orch.peers[0].initiated, "and re-offers with fresh local credentials");
        assert!(
            orch.peers[0].started,
            "the held candidate plus the answer credentials start the checks"
        );
        assert_eq!(orch.peers[0].pending_remote.len(), 0, "the held candidate was handed over");
        orch.run_once(21).expect("beat");
        let frames = signal.frames.lock().expect("frames");
        assert!(
            frames.iter().any(|(k, kind, _)| k == "P" && *kind == PeerSignalKind::Offer),
            "the re-offer goes out: {frames:?}"
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
        assert_eq!(orch.peer_status("P").expect("peer").state, PeerIceState::Idle);
        unsafe { sys::close(holder) };
    }

    /// 0049 §5.1-2 活锁回归钉子 + socket 账本：**池容量小于 N 个 peer 的
    /// 总需求、按批补给**（现场：池 8、每批 4、每 peer 6）。修复前失败路径
    /// 「先消耗再失败、不回流」会把每批补给吃光——池永远攒不到 6 枚，切网
    /// 后 18 s 补给的 20 枚 0 次成功；修复后供给不足的 peer 在任何 `take`
    /// 之前就被挡住（0050 起是**等待**：`N5c_ICE|locals-waiting|…|need=6|
    /// have=…|reason=insufficient-sockets`，不再是 `Err`/pump error），池得以
    /// 跨批次攒够，3 个 peer 最终全部完成本地收集，且被挡住的尝试一枚 socket
    /// 都不消耗。（0052 起每 peer 需求由 6 降到 3：gather 的绑定 socket 由
    /// 会话接管，不再另取一枚——账本公式不变，`NEED` 变小。）
    ///
    /// fd 账本（现有 harness 方式）：`LedgeredSocks` 把 `take_owned` 的每一
    /// 枚记进 `handed` 并当场用 `F_GETFD` 证明取到的是有效 fd；于是可断言
    /// ①`handed == 3 × need`（没有一枚被失败尝试白烧）、②`taken == handed`、
    /// ③`fed == pending + handed`（喂进来的每一枚 dup 都有归属，既不凭空
    /// 消失也不重复计入）。本修复是纯前置检查：不 take / 不 close / 不 dup，
    /// 因此不存在重复关闭或泄漏的新路径——被挡住的拍位零消耗即其直接证据。
    #[test]
    fn starvation_waits_for_topup_instead_of_burning_it() {
        use std::os::fd::{AsRawFd, OwnedFd};

        /// 受保护池 + 逐枚 handed-out 账本。
        struct LedgeredSocks {
            inner: crate::ice::ProtectedUdpFdSource,
            handed: Mutex<Vec<i32>>,
        }
        impl LedgeredSocks {
            /// 壳层 `feedOneIceSocket` 的测试等价物：真实 UDP fd，喂给源之后
            /// 原 fd 归测试并立即关闭（源留住自己的 dup）。
            fn feed_batch(&self, n: usize) {
                for _ in 0..n {
                    let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
                    assert!(fd >= 0, "socket() failed");
                    self.inner.feed(fd).expect("feed");
                    unsafe { sys::close(fd) };
                }
            }
        }
        impl UdpSocketSource for LedgeredSocks {
            fn take_fd(&self) -> Result<i32, crate::mgmtsock::SocketSeamError> {
                self.inner.take_fd()
            }
            fn take_owned(&self) -> Result<OwnedFd, crate::mgmtsock::SocketSeamError> {
                let owned = self.inner.take_owned()?;
                let fd = owned.as_raw_fd();
                assert!(
                    unsafe { sys::fcntl(fd, sys::F_GETFD) } >= 0,
                    "a handed-out socket must be an open fd"
                );
                self.handed.lock().expect("handed").push(fd);
                Ok(owned)
            }
            /// 只有生产源会报告可用量；这里转发它，守护才生效（把这一行临时
            /// 改成 `None` 即复现修复前的活锁）。
            fn available(&self) -> Option<usize> {
                Some(self.inner.pending())
            }
        }

        // 3 个允许接口 ⇒ 每 peer 需要 3 枚（每接口一枚 gather socket，会话
        // 直接接管），与 0049 的 `added=3` 同型；壳层池目标 8、每批 4 枚
        //（ICE_TOPUP_BATCH）。
        //
        // 初始只灌 2 枚（H1 起一拍只允许一个 peer 收集）：不足一次收集所需的
        // 3 枚，于是第一次尝试必然落在「等待补给」分支 —— 守护才真的被走到。
        // 若初始灌满 8 枚，每拍唯一的那次尝试都能当场成功，本用例会退化成
        // 「没人被饿过」的空转。
        const NEED: usize = 3;
        let socks = Arc::new(LedgeredSocks {
            inner: crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"),
            handed: Mutex::new(Vec::new()),
        });
        socks.feed_batch(2); // 不足一次收集（启动灌池的半批，故意饿一次）
        let mut orch = test_orch(PeerIceDeps {
            ifaces: Arc::new(crate::ice::StaticInterfaces(vec![
                crate::ice::InterfaceAddr { name: "eth0".into(), addr: [127, 0, 0, 1] },
                crate::ice::InterfaceAddr { name: "eth1".into(), addr: [127, 0, 0, 2] },
                crate::ice::InterfaceAddr { name: "eth2".into(), addr: [127, 0, 0, 3] },
            ])),
            socks: socks.clone(),
            signal: Arc::new(LoggingSignalExchange::default()),
            wg: noop_wg(),
            tie_breaker: Some(7),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        });
        orch.set_peers(&["P1".into(), "P2".into(), "P3".into()]);
        orch.set_signal_ready(true);

        let mut fed = 2usize; // 与上面的启动灌池一致（不足一次收集 ⇒ 首拍必饿）
        let mut starved_rounds = 0usize;
        let mut now = 1000u64;
        let mut rounds = 0usize;
        while !orch.peers.iter().all(|p| p.locals_done) && rounds < 40 {
            let taken_before = socks.inner.taken();
            let done_before = orch.peers.iter().filter(|p| p.locals_done).count();
            // 0050 §8 item 3：池不足是**等待**——被挡住的拍绝不冒 pump error
            // （修复前这里是 `Err(Network("…insufficient-sockets…"))`，ICE pump
            // 每拍打印一条 `peer-conn: pump error (network)`）。
            assert!(
                orch.run_once(now).is_ok(),
                "a starved beat must stay a wait, never a pump error (round {rounds})"
            );
            let waiting = orch.peers.iter().filter(|p| p.locals_wait_since_ms != 0).count();
            if waiting > 0 {
                starved_rounds += 1;
            }
            // 打断活锁的全部机制：被挡住的尝试一枚 socket 都不消耗。同一拍里
            // 别的 peer 可能刚好成功（那是它自己的预算），所以逐拍只断言
            // 「消耗量 == 本拍成功数 × need」。
            let newly_done = orch.peers.iter().filter(|p| p.locals_done).count() - done_before;
            assert_eq!(
                socks.inner.taken() - taken_before,
                (newly_done * NEED) as u64,
                "a starved attempt must not consume any socket (round {rounds})"
            );
            // 等待不是错误：`last_error` 不得被等待拍写脏。
            assert!(
                orch.peers.iter().all(|p| p.last_error.is_none()),
                "waiting must not record an error (round {rounds})"
            );
            // 壳层补给等价物：池深 < 8 时补一批 4 枚（4 s 定时器 + 5 s 状态
            // 轮询两个驱动的合并节奏）；重试节奏沿用 RETRY_COOLDOWN_MS。
            if socks.inner.pending() < 8 {
                socks.feed_batch(4);
                fed += 4;
            }
            now += RETRY_COOLDOWN_MS + 1;
            rounds += 1;
        }

        for (i, p) in orch.peers.iter().enumerate() {
            assert!(
                p.locals_done,
                "peer {i} must converge after top-ups (rounds={rounds}, taken={}, pending={}, \
                 starved={starved_rounds})",
                socks.inner.taken(),
                socks.inner.pending()
            );
        }
        assert!(starved_rounds > 0, "the scenario must actually exercise the starvation guard");
        // 每次成功收集的预算仍是 `need`（旧断言 `insufficient-sockets|need=6`
        // 的等价物：等待行里的 `need=` 就是这个数）。
        assert_eq!(orch.required_local_sockets().expect("budget"), NEED);
        let handed = socks.handed.lock().expect("handed").len();
        assert_eq!(handed, 3 * NEED, "no socket may be spent outside a successful gather+bind");
        assert_eq!(socks.inner.taken() as usize, handed);
        assert_eq!(
            socks.inner.pending() + handed,
            fed,
            "fd ledger must balance: every fed socket is either queued or handed out"
        );
    }

    /// 0050 §8 item 3 的第二半：**等补给的时间不得计入
    /// [`START_DEADLINE_MS`]**（「等待期间暂停计时」）。
    ///
    /// 真机形态：会话已建、本地候选还没收集（池补给尚未到达），远端凭证也
    /// 没来。修复前每一拍都会把 `now - session_at_ms` 喂给僵尸判据，于是饿死
    /// 3 个 deadline 周期后会话被当「对端不应答」拆掉（真机 0050 观测到
    /// TrBfURPG 12 次 / ai4IGfaf 11 次 `negotiation-start-timeout`，见汇总
    /// §2.6 的推测链）。本用例钉死两点：①等待期间会话既不被拆、计时起点也
    /// 不被重排；②等待结束后计时**恢复**——再走满一个 deadline（期间无等待）
    /// 仍然诚实拆除，暂停不是禁用。
    ///
    /// 状态用字段级构造：公开路径在发起侧被挡住时会回滚会话（0048 修复），
    /// 而本用例要的正是「会话还在时饿死」这一形态（答方在池补给前已建会话）。
    #[test]
    fn starved_session_does_not_burn_the_negotiation_deadline() {
        let socks = Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source"));
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
        orch.set_peers(&["P".into()]);
        orch.set_signal_ready(true);
        const T0: u64 = 1000;
        {
            let p = &mut orch.peers[0];
            let creds = IceCredentials::generate().expect("creds");
            p.session = Some(IceSession::new(creds.clone(), true, Some(7)).expect("session"));
            p.creds = Some(creds);
            p.state = PeerIceState::Gathering;
            p.session_at_ms = T0;
        }
        // 饿死超过 3 个 deadline：会话必须还在，计时起点不被重排，`run_once`
        // 全程 `Ok`（无 pump error），peer 状态里也没有错误。
        let mut now = T0;
        let mut first_wait_at = 0u64;
        while now < T0 + 3 * START_DEADLINE_MS {
            now += 500;
            assert!(orch.run_once(now).is_ok(), "a starved beat is a wait, not an error ({now})");
            assert!(orch.peers[0].session.is_some(), "starvation must not tear the session down ({now})");
            assert_eq!(orch.peers[0].session_at_ms, T0, "the timer must not be re-armed ({now})");
            assert!(orch.peers[0].locals_wait_since_ms != 0, "the peer is waiting for supply ({now})");
            assert_eq!(orch.peers[0].last_error, None, "waiting is not an error ({now})");
            if first_wait_at == 0 {
                first_wait_at = now;
            }
        }
        // 补给到达：收集完成（同一个会话，`negotiation_age_ms` 里只留下等待
        // 开始之前那一小段非等待龄期）。等过最后一次冷却，走既有重试节奏。
        for _ in 0..2 {
            let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            assert!(fd >= 0, "socket() failed");
            socks.feed(fd).expect("feed");
            unsafe { sys::close(fd) };
        }
        now += RETRY_COOLDOWN_MS + 1;
        assert!(orch.run_once(now).is_ok(), "the supplied peer gathers");
        assert!(orch.peers[0].locals_done, "the top-up must complete the gathering");
        assert_eq!(orch.peers[0].session_at_ms, T0, "the SAME session finished gathering");
        assert_eq!(orch.peers[0].locals_wait_since_ms, 0, "supply ends the waiting state");
        let age_at_supply = orch.peers[0].negotiation_age_ms(now);
        assert_eq!(
            age_at_supply,
            first_wait_at - T0,
            "the whole waiting span is excluded from the deadline"
        );
        // 计时恢复：deadline 只剩最后 1 ms 时不拆；走满剩余的非等待量才拆。
        let remaining = START_DEADLINE_MS - age_at_supply;
        assert!(remaining > 0, "the supply beat must leave some deadline to run");
        assert!(orch.run_once(now + remaining - 1).is_ok());
        assert!(orch.peers[0].session.is_some(), "a paused timer must not expire on the old total");
        assert!(orch.run_once(now + remaining).is_ok());
        assert!(
            orch.peers[0].session.is_none(),
            "after a full non-waiting deadline the zombie is still torn down (pause, not disable)"
        );
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
    /// 同一拍各自开跑**，不再有「每拍最多一个 peer 做阻塞收集」的预算。
    ///
    /// ea8ede1 的模型是「一拍一个 peer、一次收集封顶 [`DEFAULT_STUN_TIMEOUT_MS`]」：
    /// 20 个 peer 要 20 拍，`now_ms` 被 STUN 冻结，下一拍把上一拍建的会话当僵尸
    /// 拆掉。本阶段收集在 worker 上跑（上游 `worker_ice.go:100-155` 的每协商一枚
    /// goroutine），本用例用**真实墙钟 + 默认线程执行者**钉死三件事：
    /// ① 一拍之内 20 个 peer 全部建会话 + 各自开跑（`gather.is_some()`）；
    /// ② 那一拍本身不等任何 STUN（墙钟下限）；
    /// ③ 20 轮收集并行收尾（总时长远小于串行的 20 × 超时），每个 peer 都拿到
    ///    本地候选并发出 OFFER，且每接口 round 恰好取一枚 socket。
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
            assert!(p.gather.is_some(), "peer {i} must have its own gather in flight in beat 1");
            assert!(!p.locals_done, "peer {i} is not waiting for anything in beat 1");
        }

        // 候选随 worker 完成陆续到达：注入时钟与实际墙钟同步推进（生产同形，
        // `connector::spawn_ice_pump` 传的就是 `sys::mono_ms()`）。
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while orch.peers.iter().any(|p| !p.locals_done) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(1));
            orch.run_once(1_000 + t0.elapsed().as_millis() as u64).expect("beat");
        }
        assert!(
            orch.peers.iter().all(|p| p.locals_done),
            "every peer must finish gathering within the deadline"
        );
        let elapsed = t0.elapsed();
        assert!(
            elapsed.as_secs() < 4,
            "20 parallel gathers must not serialize (one timeout each): {elapsed:?}"
        );
        assert_eq!(
            socks.taken.load(Ordering::Relaxed),
            PEERS as u64,
            "exactly one protected socket per interface round"
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

    /// (c) 候选**按 round 完成的时刻**陆续交付（trickle）：两个接口分两次跑完，
    /// 第一次跑完的候选立刻入列（并带出 OFFER），第二次跑完的候选在**下一拍**
    /// 追加 —— 既不等到整轮收完才发，也不会丢在后一个 round 后面。
    ///
    /// 这是上游 `onICECandidate` 的语义（pion fork `gather.go:39-65` 起收集、
    /// 每得一个候选就回调发一帧）：`GatherCandidates()` 立即返回，`dial` 与候选
    /// trickle 同时在飞。
    #[test]
    fn candidates_are_delivered_as_each_interface_round_finishes() {
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
        orch.set_gather_exec(exec.clone());
        orch.set_peers(&["P".into()]);
        orch.set_signal_ready(true);

        orch.run_once(1_000).expect("beat 1");
        assert_eq!(exec.queued(), 2, "one round per allowed interface");
        assert!(
            signal.frames.lock().expect("frames").is_empty(),
            "nothing may be signaled before any round has produced a candidate"
        );
        assert!(!orch.peers[0].locals_done);

        // 第一个 round 跑完（另一个还在跑）：候选 + OFFER 立刻入列，收集仍在飞。
        assert!(exec.run_next(), "first round");
        orch.run_once(1_100).expect("beat 2");
        {
            let frames = signal.frames.lock().expect("frames");
            let cands: Vec<&(String, PeerSignalKind, String)> = frames
                .iter()
                .filter(|(_, k, _)| *k == PeerSignalKind::Candidate)
                .collect();
            assert_eq!(cands.len(), 1, "exactly the finished round's candidate: {frames:?}");
            assert!(frames.iter().any(|(_, k, _)| *k == PeerSignalKind::Offer), "OFFER rides it");
        }
        assert!(orch.peers[0].locals_done, "the first candidate makes the locals ready");
        assert!(orch.peers[0].gather.is_some(), "the second round is still in flight");
        assert_eq!(socks.taken.load(Ordering::Relaxed), 1, "one socket per finished round");

        // 第二个 round 跑完：候选在**下一拍**追加（trickle），收集随之结束。
        assert!(exec.run_next(), "second round");
        orch.run_once(1_200).expect("beat 3");
        let frames = signal.frames.lock().expect("frames");
        let cands: Vec<(String, PeerSignalKind, String)> = frames
            .clone()
            .into_iter()
            .filter(|(_, k, _)| *k == PeerSignalKind::Candidate)
            .collect();
        assert_eq!(cands.len(), 2, "both interface rounds' candidates are delivered: {frames:?}");
        let ports: Vec<&str> = cands.iter().map(|(_, _, p)| p.as_str()).collect();
        assert_ne!(ports[0], ports[1], "each round advertises its OWN bound socket: {ports:?}");
        assert_eq!(socks.taken.load(Ordering::Relaxed), 2);
        drop(frames);
        assert!(orch.peers[0].gather.is_none(), "all rounds reported: the gather is over");
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
        assert_eq!(orch.peers.iter().filter(|p| p.gather.is_some()).count(), 2);
        assert!(
            orch.peers.iter().all(|p| !p.locals_done),
            "both peers' rounds are blocked on the silent STUN server"
        );

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
            orch.peers.iter().all(|p| !p.locals_done),
            "the black hole must still be holding both gathers"
        );

        // 超时到点：两个 peer 各自拿到自己的 host 候选（延迟 = 一个 STUN 超时，
        // 不是两个串联），pump 花的墙钟总量仍然远小于它。
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut first_candidate_at = None;
        while orch.peers.iter().any(|p| !p.locals_done) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(2));
            let now = 1_000 + t0.elapsed().as_millis() as u64;
            let tick = std::time::Instant::now();
            orch.run_once(now).expect("beat");
            let dt = tick.elapsed();
            worst_tick = worst_tick.max(dt);
            pump_total += dt;
            if first_candidate_at.is_none() && orch.peers[0].locals_done {
                first_candidate_at = Some(t0.elapsed());
            }
        }
        let first_candidate_at = first_candidate_at.expect("the blocked round must still finish");
        assert!(
            first_candidate_at.as_millis() as u64 >= DEFAULT_STUN_TIMEOUT_MS - 700,
            "the candidate must not appear before the STUN timeout ({first_candidate_at:?})"
        );
        assert!(orch.peers.iter().all(|p| p.locals_done), "both peers finish off the pump");
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

    /// 整轮零候选 = fail-closed（不发没有候选支撑的 OFFER），且**失败面出现在
    /// 收集跑完之后的那一拍**：STAGE 1 把收集搬走，错误不再当拍返回，所以这条
    /// 「迟到一拍」的回滚语义必须钉住——半成品会话整段回滚、`initiated` 复位、
    /// 冷却后重试，`run_once` 冒一次错（与旧同步路径同形）。
    ///
    /// 接口地址取 TEST-NET-3（203.0.113.0/24，保留文档段，本机绑定必然
    /// `EADDRNOTAVAIL`）：round 一个候选都绑不出来。
    #[test]
    fn async_gather_failure_rolls_back_the_initiate_session() {
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
            socks,
            signal: signal.clone(),
            wg: noop_wg(),
            tie_breaker: Some(7),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        });
        orch.set_peers(&["P".into()]);
        orch.set_signal_ready(true);

        // 第一拍：只发起、只把 round 交给 worker——绝不冒错。
        orch.run_once(1_000).expect("beat 1 only starts the round");
        assert!(orch.peers[0].session.is_some(), "the initiate path builds the session");
        assert!(orch.peers[0].gather.is_some(), "the round is in flight, off the pump");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut failed = false;
        while !failed && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(1));
            let now = 1_000 + std::time::Instant::now().elapsed().as_millis() as u64;
            failed = orch.run_once(now).is_err();
        }
        assert!(failed, "a zero-candidate round must surface as a pump error");
        let p = &orch.peers[0];
        assert!(p.session.is_none(), "the half-built session is rolled back");
        assert!(!p.locals_done && !p.started, "nothing is armed for a failed gather");
        assert!(!p.initiated, "the peer may retry from scratch after the cooldown");
        assert!(p.last_error.is_some(), "the failure is not silent");
        assert!(
            signal.frames.lock().expect("frames").is_empty(),
            "no candidate ⇒ no OFFER, no ANSWER (fail-closed)"
        );
    }

    // -----------------------------------------------------------------------
    // 0051 §7 item 2：无回应 peer 的指数退避
    // -----------------------------------------------------------------------

    /// 单接口（need=2）+ 指定深度 socket 池的编排器，一个静默 peer。
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
        orch.set_peers(&["P".into()]);
        orch.set_signal_ready(true);
        orch
    }

    /// 把静默 peer 驱动到 `target` 次 `negotiation-start-timeout`；返回最后
    /// 一拍的注入时刻。返回每次静默超时后「本轮设置的下一次发起延迟」。
    fn drive_silent_timeouts(
        orch: &mut PeerIceOrchestrator,
        start_now: u64,
        target: u32,
    ) -> (u64, Vec<(u32, u64)>) {
        const TICK: u64 = 250;
        let mut now = start_now;
        let mut last_misses = orch.peers[0].backoff_misses;
        let mut delays: Vec<(u32, u64)> = Vec::new();
        while orch.peers[0].backoff_misses < target {
            now += TICK;
            assert!(now < 10_000_000, "the silent peer never timed out (misses={})", orch.peers[0].backoff_misses);
            orch.run_once(now).expect("a silent beat is not an error");
            let misses = orch.peers[0].backoff_misses;
            if misses > last_misses {
                delays.push((misses, orch.peers[0].cooldown_until.saturating_sub(now)));
                last_misses = misses;
            }
        }
        (now, delays)
    }

    /// 0051 §7 item 2 的算术与格式：延迟序列 9 s → 18 s → 36 s → … 封顶
    /// 5 min；两行日志逐字冻结（真机判读依赖这两个 token）。
    #[test]
    fn initiate_backoff_arithmetic_and_lines_are_frozen() {
        assert_eq!(INITIATE_BACKOFF_MIN_MISSES, 3);
        assert_eq!(INITIATE_BACKOFF_BASE_MS, 9_000);
        assert_eq!(INITIATE_BACKOFF_MAX_MS, 300_000);
        assert_eq!(initiate_backoff_ms(3), 9_000);
        assert_eq!(initiate_backoff_ms(4), 18_000);
        assert_eq!(initiate_backoff_ms(5), 36_000);
        assert_eq!(initiate_backoff_ms(6), 72_000);
        assert_eq!(initiate_backoff_ms(8), 288_000);
        assert_eq!(initiate_backoff_ms(9), 300_000, "9 s << 6 = 576 s is capped");
        assert_eq!(initiate_backoff_ms(40), 300_000, "the cap also holds for absurd counts");
        assert_eq!(initiate_backoff_ms(u32::MAX), 300_000, "no shift overflow at the extremes");
        assert_eq!(
            initiate_backoff_line("TrBfURPG", 3, 9_000),
            "N5c_ICE|initiate-backoff|peer=TrBfURPG|misses=3|next_ms=9000"
        );
        assert_eq!(
            backoff_reset_line("TrBfURPG", "signal"),
            "N5c_ICE|backoff-reset|peer=TrBfURPG|reason=signal"
        );
    }

    /// 远端静默 → 协商超时按退避增长：前两次仍是既有节奏
    /// （[`RETRY_COOLDOWN_MS`]），第 3 次起延迟 9 s / 18 s / 36 s / 72 s，
    /// 且实际发起间隔逐档变长；上限由 [`initiate_backoff_ms`] 钉住。
    #[test]
    fn silent_remote_grows_the_initiate_interval_up_to_the_cap() {
        let mut orch = silent_peer_orch(64);
        let mut initiate_at: Vec<u64> = Vec::new();
        let (_, delays) = {
            // 记录发起时刻：session_at_ms 每次发起被置为该拍注入时钟
            const TICK: u64 = 250;
            let mut now = 1_000u64;
            let mut last_session = 0u64;
            let mut delays: Vec<(u32, u64)> = Vec::new();
            let mut last_misses = 0u32;
            while orch.peers[0].backoff_misses < 6 {
                now += TICK;
                assert!(now < 10_000_000, "silent peer never timed out");
                orch.run_once(now).expect("a silent beat is not an error");
                let session = orch.peers[0].session_at_ms;
                if session != 0 && session != last_session {
                    initiate_at.push(session);
                    last_session = session;
                } else if session == 0 {
                    last_session = 0;
                }
                let misses = orch.peers[0].backoff_misses;
                if misses != last_misses {
                    delays.push((misses, orch.peers[0].cooldown_until.saturating_sub(now)));
                    last_misses = misses;
                }
            }
            (now, delays)
        };
        assert_eq!(
            delays,
            vec![(1, RETRY_COOLDOWN_MS), (2, RETRY_COOLDOWN_MS), (3, 9_000), (4, 18_000),
                 (5, 36_000), (6, 72_000)],
            "the first 3 timeouts keep the normal pace, then 9/18/36/72 s"
        );
        assert!(orch.peers[0].backoff_active, "the 6th silent timeout is in backoff");
        assert_eq!(orch.peers[0].backoff_misses, 6);
        // 实际发起间隔：前几拍是既有的 ~11 s（9 s 截止期 + 2 s 冷却），进入
        // 退避后逐档变长（9/18/36/72 s 的延迟叠加在 9 s 截止期上）。
        assert!(initiate_at.len() >= 5, "expected several initiations, got {initiate_at:?}");
        let gaps: Vec<u64> = initiate_at.windows(2).map(|w| w[1] - w[0]).collect();
        let normal = START_DEADLINE_MS + RETRY_COOLDOWN_MS + 2 * 250;
        assert!(gaps[0] <= normal && gaps[1] <= normal, "no backoff before the 3rd miss: {gaps:?}");
        for pair in gaps.windows(2).skip(1) {
            assert!(pair[1] > pair[0], "the interval must grow in backoff: {gaps:?}");
        }
        assert!(
            gaps[gaps.len() - 1] >= 45_000,
            "the last interval must carry the 36 s delay of miss 5 (9 s deadline + 36 s): {gaps:?}"
        );
    }

    /// 收到该 peer 任何信令 → 退避立即复位、回到正常节奏，且**应答不受退避
    /// 影响**：Offer 到手当拍即建会话、收集本地候选并欠下 ANSWER，下一拍就
    /// 把 ANSWER 发出去（0051 的静默 peer 从不发 Offer，但这条语义必须钉死，
    /// 否则退避会变成「对端终于说话了也不理」）。
    #[test]
    fn remote_signal_resets_backoff_and_is_answered_immediately() {
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
        orch.set_peers(&["P".into()]);
        orch.set_signal_ready(true);
        let (now, delays) = drive_silent_timeouts(&mut orch, 1_000, 4);
        assert_eq!(
            delays,
            vec![(1, RETRY_COOLDOWN_MS), (2, RETRY_COOLDOWN_MS), (3, 9_000), (4, 18_000)]
        );
        assert!(orch.peers[0].backoff_active, "the 4th silent timeout is in backoff");
        let armed_until = orch.peers[0].cooldown_until;
        assert!(armed_until > now + 10_000, "the next initiate is held off by the backoff");

        // 远端 Offer 到达（退避期内）：立即建会话 + 收集 + 欠 ANSWER，退避复位
        let answer_at = now + 1;
        orch.handle_signal("P", PeerSignalKind::Offer, "abcd:0123456789012345678901", answer_at)
            .expect("offer accepted");
        assert!(!orch.peers[0].backoff_active, "any remote signal resets the backoff");
        assert_eq!(orch.peers[0].backoff_misses, 0);
        assert!(orch.peers[0].session.is_some(), "the offer is answered in the same beat");
        assert!(
            orch.peers[0].outbox.iter().any(|(kind, _)| *kind == PeerSignalKind::Answer),
            "the ANSWER is queued in the same beat the offer arrived"
        );
        assert!(
            orch.peers[0].cooldown_until <= answer_at + RETRY_COOLDOWN_MS,
            "the backoff delay is replaced by the normal cooldown"
        );
        // 下一拍：ANSWER 照常发出（退避从未挡住应答路径）
        orch.run_once(answer_at + 2).expect("beat");
        let frames = signal.frames.lock().expect("frames");
        assert!(
            frames.iter().any(|(k, kind, _)| k == "P" && *kind == PeerSignalKind::Answer),
            "the ANSWER must go out on the next beat: {frames:?}"
        );
        drop(frames);

        // 反向：一条 candidate 也打断「连续静默」——新的静默 peer 攒到 2 次
        // 超时后插一条 candidate，再超时时计数从零重新起算（要再走 3 次才
        // 进入退避），因此不会把「远端只 trickle 候选」误判成离线。
        let mut fresh = silent_peer_orch(64);
        let (now2, first) = drive_silent_timeouts(&mut fresh, 1_000, 2);
        assert_eq!(first, vec![(1, RETRY_COOLDOWN_MS), (2, RETRY_COOLDOWN_MS)]);
        assert_eq!(fresh.peers[0].backoff_misses, 2);
        fresh
            .handle_signal(
                "P",
                PeerSignalKind::Candidate,
                "candidate:1 1 udp 1 127.0.0.1 9 typ host",
                now2 + 1,
            )
            .expect("candidate accepted");
        let (_, after) = drive_silent_timeouts(&mut fresh, now2 + 1, 3);
        assert_eq!(
            after,
            vec![(1, RETRY_COOLDOWN_MS), (2, RETRY_COOLDOWN_MS), (3, 9_000)],
            "a candidate between timeouts restarts the silent streak"
        );
    }

    /// 网络切换与 signal 流重连（false→true 边沿）同样复位退避：切换后的静默
    /// 推断作废，立即回到正常节奏。
    #[test]
    fn network_change_and_signal_reconnect_reset_backoff() {
        let mut orch = silent_peer_orch(64);
        let (now, _) = drive_silent_timeouts(&mut orch, 1_000, 3);
        assert!(orch.peers[0].backoff_active);
        orch.restart_for_network_change(now + 1_000);
        assert!(!orch.peers[0].backoff_active, "a network change clears the backoff");
        assert_eq!(orch.peers[0].backoff_misses, 0);
        assert_eq!(orch.peers[0].cooldown_until, now + 1_000, "normal pace restored");
        // signal 重连边沿：先重新攒到退避，再 false→true，复位落在下一拍
        let (now, _) = drive_silent_timeouts(&mut orch, now + 1_000, 3);
        assert!(orch.peers[0].backoff_active);
        assert!(orch.peers[0].cooldown_until > now + 5_000, "backoff armed before the reconnect");
        orch.set_signal_ready(false);
        orch.set_signal_ready(true);
        orch.run_once(now + 1).expect("the reconnect beat resets the backoff");
        assert!(!orch.peers[0].backoff_active, "a signal reconnect clears the backoff");
        assert_eq!(orch.peers[0].backoff_misses, 0);
    }

    /// 0051 §7 item 3 的 core 侧事实源：`need_per_peer` = 允许接口数 × 1
    /// （与 `required_local_sockets` 同一份 `allowed_ifaces` 规则；0052 起
    /// gather 的绑定 socket 由会话接管，不再每接口另取一枚），接口枚举
    /// 失败才是 `None`（壳层据此回退）。overlay 接口（`vpn-tun`）被黑名单
    /// 挡住，因此 3 个物理接口 + overlay 是 3。
    #[test]
    fn need_per_peer_counts_the_gather_budget_of_allowed_ifaces() {
        fn orch_with(ifaces: Vec<crate::ice::InterfaceAddr>) -> PeerIceOrchestrator {
            test_orch(PeerIceDeps {
                ifaces: Arc::new(crate::ice::StaticInterfaces(ifaces)),
                socks: Arc::new(crate::ice::ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source")),
                signal: Arc::new(LoggingSignalExchange::default()),
                wg: noop_wg(),
                tie_breaker: Some(7),
                fixed_local_port: None,
                advertised_candidates: Vec::new(),
            })
        }
        let i = |name: &str, addr: [u8; 4]| crate::ice::InterfaceAddr { name: name.into(), addr };
        assert_eq!(orch_with(vec![]).need_per_peer(), Some(0));
        assert_eq!(orch_with(vec![i("wlan0", [192, 168, 1, 2])]).need_per_peer(), Some(1));
        assert_eq!(
            orch_with(vec![i("wlan0", [192, 168, 1, 2]), i("rmnet0", [10, 0, 0, 2])]).need_per_peer(),
            Some(2),
            "2 allowed interfaces → one adopted gather socket each"
        );
        assert_eq!(
            orch_with(vec![
                i("wlan0", [192, 168, 1, 2]),
                i("rmnet0", [10, 0, 0, 2]),
                i("vpn-tun", [100, 108, 144, 165]),
                i("eth1", [172, 17, 1, 1]),
            ])
            .need_per_peer(),
            Some(3),
            "3 allowed interfaces (overlay blacklisted) → need=3"
        );
        // 与 PeerConn 的实际预算同源：同一个接口集下 required_local_sockets 相等
        let mut orch = orch_with(vec![i("wlan0", [192, 168, 1, 2]), i("eth1", [172, 17, 1, 1])]);
        orch.set_stuns(&[]);
        let cfg = GatherConfig {
            blacklist: &orch.blacklist,
            servers: &orch.stuns,
            timeout_ms: DEFAULT_STUN_TIMEOUT_MS,
        };
        assert_eq!(
            orch.need_per_peer(),
            Some(crate::ice::allowed_ifaces(&cfg, orch.ifaces.as_ref()).expect("enumerate").len()),
            "need_per_peer is exactly the gather budget's input"
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

    /// glare 死锁修复（H1）核心行为：我方已发起（Offer 已发）后收到对端
    /// Offer —— ①必须回 ANSWER（修复前 glare 分支不回，对端 Dial 永久
    /// 挂起）；②角色不变（公钥比较在创建时定死）；③对端换了凭证时采纳
    /// 新凭证并重发本地候选；④此后停止重发 OFFER（§2.4 offer 风暴），
    /// 只保留候选/ANSWER 重发。
    #[test]
    fn glare_offer_is_answered_and_offer_resend_stops() {
        let (mut orch, signal) = glare_orch(Some("ZZZZlocal"), "PPPPpeer", 4);
        orch.run_once(1000).expect("initiate beat sends offer+candidates");
        {
            let p = &orch.peers[0];
            assert!(
                p.signaled.iter().any(|(k, _)| *k == PeerSignalKind::Offer),
                "precondition: our OFFER was signaled"
            );
            assert!(p.session.as_ref().expect("session").is_controlling());
        }
        let offers_before = signal
            .frames
            .lock()
            .expect("frames")
            .iter()
            .filter(|(_, k, _)| *k == PeerSignalKind::Offer)
            .count();
        assert_eq!(offers_before, 1);

        // 对端 Offer 到达（新凭证 = 其 agent 重启）：glare 分支。
        orch.handle_signal("PPPPpeer", PeerSignalKind::Offer, "wxyz:0123456789012345678902", 1100)
            .expect("glare offer");
        {
            let p = &orch.peers[0];
            // ① ANSWER 已入列（本地凭证不变）
            assert!(
                p.outbox.iter().any(|(k, _)| *k == PeerSignalKind::Answer),
                "the glare branch MUST queue an ANSWER (official: sendAnswer per offer)"
            );
            let answer_payload = &p
                .outbox
                .iter()
                .find(|(k, _)| *k == PeerSignalKind::Answer)
                .expect("answer queued")
                .1;
            let creds = parse_ufrag_pwd(answer_payload).expect("answer is ufrag:pwd");
            assert_eq!(
                creds,
                p.creds.as_ref().expect("local creds").clone(),
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
            // ④ OFFER 不再重发（从重发母本移除）
            assert!(
                !p.signaled.iter().any(|(k, _)| *k == PeerSignalKind::Offer),
                "the OFFER must leave the re-signal set once answered"
            );
        }
        // 刷出 ANSWER，再走过两个重发周期：零新 OFFER、ANSWER/候选照常。
        orch.run_once(1200).expect("flush the answer");
        for t in [1200 + HANDSHAKE_RETRY_MS, 1200 + 2 * HANDSHAKE_RETRY_MS] {
            orch.run_once(t).expect("retry beat");
        }
        let frames = signal.frames.lock().expect("frames").clone();
        let offers_total = frames.iter().filter(|(_, k, _)| *k == PeerSignalKind::Offer).count();
        assert_eq!(offers_total, 1, "no OFFER re-send after the glare answer");
        assert!(
            frames.iter().any(|(to, k, _)| to == "PPPPpeer" && *k == PeerSignalKind::Answer),
            "the ANSWER must have left"
        );

        // 对端重发同一 Offer（同凭证，幂等）：再次应答、仍零新 OFFER。
        orch.handle_signal("PPPPpeer", PeerSignalKind::Offer, "wxyz:0123456789012345678902", 1200 + 3 * HANDSHAKE_RETRY_MS)
            .expect("duplicate offer");
        orch.run_once(1200 + 3 * HANDSHAKE_RETRY_MS).expect("flush");
        let offers_total = signal
            .frames
            .lock()
            .expect("frames")
            .iter()
            .filter(|(_, k, _)| *k == PeerSignalKind::Offer)
            .count();
        assert_eq!(offers_total, 1, "duplicate offer re-answers but never re-offers");
    }

    /// 对端 ANSWER 到达（正常应答路径，非 glare）：OFFER 停止重发、候选
    /// trickle 保持（§6.4 —— 修复前 3 s 周期把 OFFER 一并重发，8 min ~160
    /// 次、对端逐条应答、每轮重摆 glare/凭证状态）。
    #[test]
    fn answer_receipt_stops_offer_resend_but_keeps_candidates() {
        let (mut orch, signal) = glare_orch(Some("ZZZZlocal"), "PPPPpeer", 4);
        orch.run_once(1000).expect("offer + candidates out");
        // 对端 ANSWER 到达
        orch.handle_signal("PPPPpeer", PeerSignalKind::Answer, "wxyz:0123456789012345678901", 1100)
            .expect("answer");
        assert!(
            !orch.peers[0]
                .signaled
                .iter()
                .any(|(k, _)| *k == PeerSignalKind::Offer),
            "the ANSWER retires the OFFER from the re-signal set"
        );
        assert!(
            orch.peers[0]
                .signaled
                .iter()
                .any(|(k, _)| *k == PeerSignalKind::Candidate),
            "candidate frames keep trickling on retry"
        );
        for t in [1100 + HANDSHAKE_RETRY_MS, 1100 + 2 * HANDSHAKE_RETRY_MS] {
            orch.run_once(t).expect("retry beat");
        }
        // 截止期前 1 ms：谈判仍在（Answer 不是候选），候选继续重发——本用例的
        // 原意全在这个窗口内（修复前 3 s 周期把 OFFER 也一并重发）。
        let edge = 1000 + START_DEADLINE_MS - 1;
        orch.run_once(edge).expect("pre-deadline edge beat");
        assert!(
            orch.peers[0].session.is_some() && !orch.peers[0].started,
            "1 ms before the deadline the answered negotiation still waits for a candidate"
        );
        let frames = signal.frames.lock().expect("frames").clone();
        assert_eq!(
            frames.iter().filter(|(_, k, _)| *k == PeerSignalKind::Offer).count(),
            1,
            "exactly the ONE original offer, no re-sends after the answer"
        );
        assert!(
            frames.iter().filter(|(_, k, _)| *k == PeerSignalKind::Candidate).count() > 1,
            "candidates continue to be re-signaled until Connected or the start deadline"
        );
        // 到截止期：这个「已应答但零候选」的谈判同样被拆除（round-230622244
        // 停摆修复）——修复前它会永远占着会话与 socket、不再重试。
        orch.run_once(1000 + START_DEADLINE_MS).expect("deadline beat");
        assert!(
            orch.peers[0].session.is_none(),
            "an ANSWER without any candidate is still bounded by the start deadline"
        );
        assert_eq!(orch.peers[0].state, PeerIceState::Failed);
    }

    /// 真机 round-230622244 停摆回归：手机 07:09:03 发出本地候选、07:09:04
    /// 收到 Answer，此后**零远端候选**、9 min 内没有任何重试。`maybe_start`
    /// 要求「远端凭证 + 本地候选 + ≥1 远端候选」，Answer 只给凭证，于是
    /// `started` 永远 false；修复前 [`PeerConn::expire_dead_negotiation`] 的
    /// `remote_creds.is_none()` 守卫把「已应答」当成健康，会话永不拆除 ——
    /// socket 被占住且 `IceSession::run_once` 因 `!started` 立即返回、永不收包。
    ///
    /// 修复后 [`START_DEADLINE_MS`] 约束**所有**未 start 的谈判：到期即
    /// [`PeerIceOrchestrator::drop_session_rearm`]（Drop 关闭全部 dup socket、
    /// 回收 WG 端点、按既有冷却重新武装），冷却期满以全新凭证重新发起；
    /// 远端随后补齐候选即正常 start（恢复路径没被堵死）。远端**应答过**，
    /// 因此不计入「远端静默」退避（`note_silent_timeout` 的 `rx_since_timeout`）。
    #[test]
    fn answered_but_candidateless_negotiation_still_expires_and_recovers() {
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
        orch.set_local_key("ZZZZlocal".to_string());
        orch.set_peers(&["PPPPpeer".to_string()]);
        orch.set_signal_ready(true);

        const T0: u64 = 1000;
        orch.run_once(T0).expect("offer + local candidates out");
        assert!(orch.peers[0].session.is_some(), "precondition: the negotiation exists");
        assert!(orch.peers[0].locals_done, "precondition: local candidates gathered");

        // Answer 到达（远端凭证在手）但零远端候选——真机 round-230622244 形态。
        orch.handle_signal("PPPPpeer", PeerSignalKind::Answer, "wxyz:0123456789012345678901", 1100)
            .expect("answer");
        {
            let p = &orch.peers[0];
            assert!(p.remote_creds.is_some(), "the answer delivered remote credentials");
            assert_eq!(p.remote_cands_seen, 0, "and no candidate ever followed");
            assert!(!p.started, "credentials without a candidate can never start");
        }

        // 截止前 1 ms：仍在等待，不拆除、不置错。
        orch.run_once(T0 + START_DEADLINE_MS - 1).expect("pre-deadline beat");
        {
            let p = &orch.peers[0];
            assert!(p.session.is_some(), "1 ms before the deadline the negotiation is still alive");
            assert!(!p.started, "still waiting for the first remote candidate");
            assert_ne!(p.state, PeerIceState::Failed, "no teardown before the deadline");
        }

        // 到截止期：拆除 + 资源回收 + 冷却重武装。
        let deadline = T0 + START_DEADLINE_MS;
        orch.run_once(deadline).expect("deadline beat");
        {
            let p = &orch.peers[0];
            assert!(p.session.is_none(), "the answered-but-stalled session must be torn down");
            assert_eq!(p.state, PeerIceState::Failed, "the teardown is observable as Failed");
            assert_eq!(p.remote_creds, None, "the dead negotiation's credentials are void");
            assert!(!p.started && !p.locals_done, "the negotiation state is fully re-armed");
            assert_eq!(p.cooldown_until, deadline + RETRY_COOLDOWN_MS, "re-armed after the cooldown");
            assert_eq!(p.backoff_misses, 0, "an ANSWERED peer is not a silent remote (no backoff)");
        }
        assert_eq!(
            wg.recycled.lock().expect("recycled").clone(),
            vec!["PPPPpeer".to_string()],
            "the WG endpoint/egress of the dead negotiation must be recycled"
        );

        // 冷却期满：全新凭证重新发起（自愈，无需对端重启）。
        let restart = deadline + RETRY_COOLDOWN_MS;
        orch.run_once(restart).expect("re-initiate after cooldown");
        {
            let p = &orch.peers[0];
            assert!(p.session.is_some(), "a fresh negotiation must be armed");
            assert!(!p.started, "the fresh negotiation is not started yet");
            assert!(p.creds.is_some() && p.remote_creds.is_none(), "fresh credentials, no remote yet");
            assert_eq!(p.session_at_ms, restart, "the fresh session starts its own deadline");
        }
        let offers: Vec<String> = signal
            .frames
            .lock()
            .expect("frames")
            .iter()
            .filter(|(to, k, _)| to == "PPPPpeer" && *k == PeerSignalKind::Offer)
            .map(|(_, _, payload)| payload.clone())
            .collect();
        assert_eq!(offers.len(), 2, "the retry offers again: {offers:?}");
        assert_ne!(offers[0], offers[1], "the re-initiation carries NEW credentials");

        // 重试后远端补齐「凭证 + 候选」：正常 start（恢复路径没被堵死）。
        let cand = Candidate::host_candidate([127, 0, 0, 1], 51820);
        orch.handle_signal("PPPPpeer", PeerSignalKind::Answer, "abcd:0123456789012345678903", restart)
            .expect("fresh answer");
        orch.handle_signal("PPPPpeer", PeerSignalKind::Candidate, &cand.marshal(), restart)
            .expect("fresh candidate");
        {
            let p = &orch.peers[0];
            assert!(p.started, "credentials + a candidate start the fresh negotiation");
            assert_eq!(p.state, PeerIceState::Checking, "start moves the peer to Checking");
        }
    }

    /// 正常路径不受新边界影响：Answer **和**候选都在截止期前到达 → 照常
    /// start（进入 Checking）。已 start 的会话不归 [`START_DEADLINE_MS`] 管，
    /// 其生死由 [`CHECKING_WATCHDOG_MS`] 与 N5b 事件诚实驱动——
    /// `checking_watchdog_fails_stalled_session_and_renegotiates` 用已 start
    /// 的会话走满 18 s 窗口即其证据。
    #[test]
    fn answer_with_candidate_before_deadline_starts_normally() {
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
        assert_eq!(
            p.checking_since_ms, 1100,
            "the STARTED session is now governed by the Checking watchdog, not the start deadline"
        );
    }

    /// Checking 看门狗：start 后 [`CHECKING_WATCHDOG_MS`] 无任何检查成功
    /// → 判 failed、拆除会话、冷却后**全新协商**（重新发起 Offer）。
    /// 覆盖 §2.3 的"僵尸 Checking"（all-pairs-failed 被候选流入掩盖、
    /// Disconnected 永不触发 → 无终态信号）。
    #[test]
    fn checking_watchdog_fails_stalled_session_and_renegotiates() {
        let (mut orch, signal) = glare_orch(Some("ZZZZlocal"), "PPPPpeer", 8);
        const T0: u64 = 1000;
        // 直接注入一个已 start、处于 Checking 的会话（字段级构造，同
        // `starved_session_does_not_burn_the_negotiation_deadline` 的手法）：
        // 远端候选指向一个无人监听的端口 → 检查永远无响应，也永不成功。
        {
            let p = &mut orch.peers[0];
            let creds = IceCredentials::generate().expect("creds");
            let mut session = IceSession::new(creds.clone(), true, Some(7)).expect("session");
            session
                .set_remote_credentials(IceCredentials {
                    ufrag: "wxyz".into(),
                    pwd: "0123456789012345678901".into(),
                })
                .expect("remote creds");
            session.add_remote_candidate(Candidate::host_candidate([127, 0, 0, 1], 1));
            p.session = Some(session);
            p.creds = Some(creds);
            p.remote_creds = Some(IceCredentials {
                ufrag: "wxyz".into(),
                pwd: "0123456789012345678901".into(),
            });
            p.state = PeerIceState::Checking;
            p.started = true;
            p.locals_done = true;
            p.session_at_ms = T0;
            p.checking_since_ms = T0;
        }
        // 截止前 1 ms：绝不拆。
        assert!(orch.run_once(T0 + CHECKING_WATCHDOG_MS - 1).is_ok());
        assert!(
            orch.peers[0].session.is_some(),
            "a Checking session within the watchdog window must not be torn down"
        );
        // 到点：拆除 + Failed。
        assert!(orch.run_once(T0 + CHECKING_WATCHDOG_MS).is_ok());
        assert!(
            orch.peers[0].session.is_none(),
            "the stalled Checking session must be dropped at the watchdog deadline"
        );
        assert_eq!(orch.peer_status("PPPPpeer").expect("peer").state, PeerIceState::Failed);
        // 冷却期满：全新协商（Offer 路径重新发起）。
        assert!(
            orch.peers[0].cooldown_until <= T0 + CHECKING_WATCHDOG_MS + RETRY_COOLDOWN_MS,
            "the watchdog drop arms the normal renegotiation cooldown"
        );
        orch.run_once(T0 + CHECKING_WATCHDOG_MS + RETRY_COOLDOWN_MS + 1).expect("re-initiate");
        assert!(
            orch.peers[0].session.is_some(),
            "after the cooldown a FRESH negotiation must start"
        );
        assert!(
            orch.peers[0].initiated,
            "the re-negotiation goes through the offer path"
        );
        assert!(
            signal
                .frames
                .lock()
                .expect("frames")
                .iter()
                .any(|(to, k, _)| to == "PPPPpeer" && *k == PeerSignalKind::Offer),
            "the fresh negotiation signals an OFFER"
        );
    }

    /// 看门狗的滑动重置：检查成功（[`IceEvent::CheckSucceeded`]）把计时
    /// 起点推到成功时刻 —— 还在取得进展的慢会话不会被误杀。
    #[test]
    fn checking_watchdog_slides_on_check_success() {
        let (mut orch, _) = glare_orch(Some("ZZZZlocal"), "PPPPpeer", 4);
        const T0: u64 = 1000;
        {
            let p = &mut orch.peers[0];
            let creds = IceCredentials::generate().expect("creds");
            p.session = Some(IceSession::new(creds.clone(), true, Some(7)).expect("session"));
            p.creds = Some(creds);
            p.state = PeerIceState::Checking;
            p.started = true;
            p.locals_done = true;
            p.session_at_ms = T0;
            p.checking_since_ms = T0;
        }
        // T0+5000 检查成功（直接模拟 drain_events 的滑动重置）。
        orch.peers[0].checking_since_ms = T0 + 5000;
        // 原截止点（T0+WATCHDOG）：不拆（计时已被成功滑走）。
        assert!(orch.run_once(T0 + CHECKING_WATCHDOG_MS).is_ok());
        assert!(
            orch.peers[0].session.is_some(),
            "a session with recent check progress must survive the old deadline"
        );
        // 滑动后的截止点（成功时刻 + WATCHDOG）：拆除。
        assert!(orch.run_once(T0 + 5000 + CHECKING_WATCHDOG_MS).is_ok());
        assert!(
            orch.peers[0].session.is_none(),
            "the watchdog fires from the LAST check success, not from start"
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
}
