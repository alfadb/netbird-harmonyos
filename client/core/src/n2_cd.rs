// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N2 C/D 列的 **App 侧夹心**（`n2_cd_plan` 私有通道）— 状态机修订版
//! （2026-09-29，依据 `~/n2h-kit/status-20260927/N2-CD-RING-T0-REVIEW.md`）。
//!
//! 冻结 preregistration v4.1 §3 要求每个生产事件「前控 → 生产包 → 后控」，且前后
//! 正控都须在 vpn-tun 的 from-OS 侧命中（核心 tee 丢弃、不外发）。C 列
//! （Wi-Fi → 蜂窝）与 D 列（蜂窝 → Wi-Fi）在蜂窝阶段主机无法下发任何命令，所以
//! 主机在 Wi-Fi 阶段先经私有通道下发一份「切网夹心计划」，App 自己在蜂窝阶段夹
//! 对照包，日志留在设备上（见 `crate::n2_ring`），回到 Wi-Fi 后主机统一取回。
//!
//! ## 修订后的计划生命周期（每一步一行 `N2_CD_SANDWICH`）
//!
//! 1. **`phase=plan`** — 计划在 **Wi-Fi 上受理**。C 列受理即就位；D 列是
//!    **预置**：`result=planned|reason=await-start-bearer`，受理时不发任何控件。
//!    受理时记下 `target`/`proto`（冻结:L42 的 marker/目标/协议**先于发送**预注册）。
//! 2. **`phase=prepare`**（仅 D）— 观察到 Wi‑Fi → 蜂窝的**准备跳**：
//!    `result=prepared|reason=not-a-d-measurement`。这一跳**不是** D 列的测量事件，
//!    不计 C/D 事件、不发前控也不发后控。蜂窝侧恢复完毕后再记一行
//!    `result=ready|reason=preparation-recovered`，此后才等**正式**的
//!    蜂窝 → Wi‑Fi 切换。
//! 3. **`phase=switch`**（正式切换）— 壳侧纯模块的方向判定 + **身份基线**：
//!    `net_id_after`（新网络的平台 netId）与该 kind 在**旧网络**上的 live 注册身份
//!    `identity_at_switch = fd:dup_fd:protect_generation`。缺任一项即
//!    `unsat|switch-identity-required` / `switch-identity-missing` /
//!    `switch-net-id-unmeasured` / `switch-generation-unmeasured`（fail-closed）。
//! 4. **`phase=pre`** — **正式切换之后**、壳侧恢复生产连接**之前**，App 在**新网络**
//!    上发前控 `<id>-pre`。`result=hit|reason=gate-open` 必须由核心 tee 观察到
//!    （`control_hit`，见 `wg_device::N2DestinationTee::consume_control`），且 HIT
//!    必须晚于正式切换；否则 `unsat|pre-hit-unobserved`。**只有**这一步开了门，
//!    `poll` 才会给出 `step=recover`（放行壳侧恢复生产连接）。
//!    旧网络上的前控/旧网络上的 HIT 一律不认（`pre-before-switch` /
//!    `pre-hit-before-switch`）。
//! 5. **`phase=receipt`** — 新网络上的**新注册**回执，由 [`receipt`] 的**输入**判定
//!    （见下）。`Wb` 结构性不发包，用新代 `protect_settle|status=success` 作为替代
//!    回执（`result=exempted|reason=wb-protect-settle-success`），但**仍**要前控与
//!    后控。
//! 6. **`phase=post`** — 后控 `<id>-post` 的结果；`observed` 要求核心 tee 的 HIT
//!    晚于回执（回调次序，`post_hit_ms >= receipt_ms`），之后计划才终结。`Wb`
//!    的后控**必须**等到新代 protect settle 之后才发：`exempted` 不再是终结
//!    状态；且 `Wb` 的后控 HIT 必须**严格晚于该新代自身的 protect settle**
//!    （回执携带的核心 `protect_mono_ms`，**不是**回执回调时刻）——冻结次序为
//!    `pre_hit_ms < protect_mono_ms < post_hit_ms`，同毫秒相等不能证明该严格
//!    关系，任一等号成立即 `unsat|post-before-receipt`（获批 R2(a)）。
//!
//! ## 回执判据（不靠全局 live 注册序号）
//!
//! 旧版把 `n2_socket_trace::live_seq(kind)` 的变化当作回执——本修订**删除**了这条
//! 路径：登记序号是进程全局的，证明不了「新连接、新网络」，旧 socket 上的任意一次
//! I/O 就会改变它。现在回执由 [`receipt`] 的输入事实判定，**全部**满足才接受：
//!
//! * `identity = (fd, dup_fd, protect_generation)` 是**新**身份：
//!   `protect_generation` 必须异于切换基线（不是同一个保护代），且 `(fd, dup_fd)`
//!   必须异于基线（不是同一个描述符被重新 protect）。**只换 fd 数字不算身份**：
//!   数字会被平台复用，所以代际同样参与判定；
//! * `net_id` 必须是切换时实测的**新网络** netId（0 = 未实测，拒绝）；
//! * `protect_ok = true`（protect 成功）；
//! * 首个 I/O 的 mono 时刻**晚于**新网络前控 HIT（且非 0 = 未实测）；
//! * `M`/`S`/`R` 必须是真实的 `phase=subsequent_io` **业务写**，`connect` 完成不算；
//! * `Wb` 必须是新代 `protect_settle|status=success`。
//!
//! 逐类替代证据（`I` 的 `stun_txn`、`D` 的 `N2_DNS matched`、`Ws` 的新选中 dup
//! `wg_sendto`）由**判定器**事后 join：本状态机只判身份/网络/保护/时序/写法，
//! 不自行放宽也不新增冻结未批准的通过条件。
//!
//! ## Wiring (who feeds what)
//!
//! * **In** — the host issues the plan through the `n2_cd_plan` private channel
//!   (`connector::n2_cd_plan_json` / NAPI `n2_cd_plan`); the shell drives the
//!   step entries (`n2_cd_prepare` / `n2_cd_ready` / `n2_cd_switch_identified` /
//!   `n2_cd_pre` / `n2_cd_poll` / `n2_cd_post`), each of which is a thin
//!   pass-through to this module (they answer the `N2_CD_SANDWICH` line the core
//!   emitted, or `{ok,error}` when unarmed).
//! * **In, core-owned** — the RECEIPT facts are **not** a shell report: the
//!   [`crate::n2_socket_trace`] call sites that already produce the `N2_SOCK`
//!   receipts (`first_io` for the `I`/`D`/`Ws` sends, `post_connect_write` for
//!   the `M`/`S`/`R` business write, and `adopted` for `Wb`, whose base socket
//!   never sends and whose liveness is therefore the CORE TAKEOVER of the
//!   validated dup) hand their own arguments to [`receipt_candidate`] →
//!   [`receipt`]. A shell cannot fabricate a receipt, and the identity tuple
//!   is the `N2_SOCK` `fd=`/`dup_fd=`/`protect_generation=` triple — for `Wb`
//!   the `dup_fd` is the taken-over core descriptor, and the receipt's instant
//!   is that registration's OWN protect settle, never the takeover clock.
//! * **In, core-owned** — the pre/post HITs come from the armed tee's
//!   `consume_control` ([`control_hit`], called from
//!   `wg_device::N2DestinationTee::consume_control`); the shell's own `hit`
//!   claim alone can never complete a sandwich.
//!
//! ## 兼容入口（fail-closed）
//!
//! 五个旧入口（`plan`/`pre`/`switch`/`poll`/`post`）保持签名与「未武装静默」
//! 行为，但语义已按上述阶段收紧：
//!
//! * **`switch`（旧五参）** 不带身份事实，一律 fail-closed 终结为
//!   `unsat|switch-identity-required`，绝不打开 [`receipt`] 的门。它是 core 的
//!   兼容入口（`tests/n2_cd_state_revision.rs` 钉住该语义），**桥**不再暴露它：
//!   `n2_cd_switch` 已改造为 `n2_cd_switch_identified`；
//! * **`pre`** 在正式切换**之前**调用一律 `rejected|pre-before-switch`，绝不放行；
//! * **`poll`** 不再读全局登记序号，因此旧调用者拿不到「假回执」。
//!
//! 因此任何未按新签名驱动的壳侧路径都无法完成计划——`N2_CD_SANDWICH` 只会
//! 出现具名的 `rejected`/`unsat` 行。
//!
//! 遗留（不在本增量）：壳侧 `N2_SOCK|phase=protect_settle` 行本身仍只有
//! `wall_ms`，没有核心 mono 字段——E1「`protect_settle` 时刻严格早于 tee attach
//! 锚点」的该字段齐全性属另一待办（`Wb` 回执侧已改用核心在**该次 protect 调用
//! 时刻**取到的 mono，见 [`crate::n2_socket_trace::adopted`]）。壳侧
//! `NetBirdVpnExtensionAbility.ets` 已按新签名驱动（`n2_cd_switch_identified`、
//! 切换后由 `step=pre` 发前控、`step=recover` 才重馈）。
//!
//! ## Gating
//!
//! 未武装时每个入口都是**静默 no-op**：不留状态、不打行、不发控制（壳侧还有一层
//! `n2Evidence !== undefined` 的外门）。[`reset`] 在每次武装变化时清空。

use std::os::fd::RawFd;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use crate::hilog;

/// The two lifecycle columns this orchestration covers.
pub const COLUMNS: [&str; 2] = ["C", "D"];
/// The seven frozen outer classes.
pub const KINDS: [&str; 7] = ["M", "S", "R", "D", "I", "Wb", "Ws"];
/// D column: the bound from plan acceptance (on Wi-Fi) to the preparation being
/// complete on the cellular side (`phase=prepare|result=ready`).
pub const PREPARE_WAIT_MS: u64 = 300_000;
/// D column: the MINIMUM dwell on the PREPARED bearer (cellular) before the
/// formal cellular → Wi-Fi transition is accepted as the D measurement. An
/// earlier return is a flap, not the D event.
pub const DWELL_MIN_MS: u64 = 3_000;
/// The bound for the shell to send the PRE control on the NEW network after the
/// formal switch and report its verdict. The control registry's own witness
/// window is 3 s; this is the plan-level fuse.
pub const PRE_WAIT_MS: u64 = 30_000;
/// Written upper bound of the wait for the kind's production receipt on the NEW
/// network (same order of magnitude as the frozen R wait).
pub const RECEIPT_WAIT_MS: u64 = 90_000;
/// Written upper bound between handing the shell a receipt and its post control
/// arriving back (a bounded plan must never linger forever).
pub const POST_WAIT_MS: u64 = 90_000;
/// Written upper bound of the wait for the FORMAL switch after the plan is in
/// position (for D: after the preparation is complete).
pub const SWITCH_WAIT_MS: u64 = 300_000;
/// The longest accepted plan id: `<id>-post` must still satisfy the core control
/// registry's `[A-Za-z0-9_-]{1,64}` marker rule.
pub const MAX_ID: usize = 59;
/// In-arming plan-id memory. Campaign-wide uniqueness is the runner's job
/// (review §5.3: one `plan_id` per campaign, a restart never re-sends an id);
/// this bounded list is the local backstop that refuses a repeat inside ONE
/// arming.
const USED_MAX: usize = 256;

/// The identity of ONE protect/registration observation of an outer socket.
///
/// A bare fd NUMBER is not an identity: the platform reuses numbers after a
/// close, so the identity a receipt is proven against is the whole tuple
/// `(fd, dup_fd, protect_generation)`. `generation` is the shell's
/// `protect_generation` (its `networkEpoch`, bumped on a measured network
/// switch) as the `N2_SOCK` receipts name it; `0` means "not measured" and
/// every verdict refuses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub fd: RawFd,
    pub dup_fd: RawFd,
    pub generation: u64,
}

impl Identity {
    pub fn new(fd: RawFd, dup_fd: RawFd, generation: u64) -> Self {
        Identity { fd, dup_fd, generation }
    }
}

/// One production receipt on the NEW network, as the CALLER measured it. The
/// core cannot read a registration's `net_id`/`protect_generation` out of the
/// socket-trace registry, so the facts arrive as inputs and the core owns the
/// ORDERING verdict (`receipt`), never the platform I/O.
///
/// The caller is expected to be the socket-trace call site that produced the
/// `N2_SOCK` line (`protect` + `first_io`, and for `M`/`S`/`R` the
/// `post_connect_write` that makes it a real business write), so the facts are
/// the core's own arguments and not a shell self-report.
#[derive(Clone, Copy, Debug)]
pub struct Receipt<'a> {
    /// The NEW registration's `(fd, dup_fd, protect_generation)`.
    pub identity: Identity,
    /// The new network's platform net id (`0` = not measured → refused).
    pub net_id: u64,
    /// `protect_fd` returned success.
    pub protect_ok: bool,
    /// The mono stamp of the receipting I/O. For `M`/`S`/`R` this is the
    /// completed post-connect write, not the connect itself.
    pub io_mono_ms: u64,
    /// `true` only for a real `phase=subsequent_io` business write. Required for
    /// `M`/`S`/`R`; a `connect` completion is not a business send.
    pub subsequent_io: bool,
    /// `Wb` only: this observation IS the new generation's
    /// `phase=protect_settle|status=success` (the exempted class has no
    /// production write).
    pub protect_settle: bool,
    /// The receipt's own `detail` (e.g. `stun_txn_…`, `wg_sendto`, `dns`),
    /// recorded verbatim for the per-kind judge; not judged here.
    pub detail: &'a str,
    pub remote: &'a str,
    pub local: &'a str,
    pub request_id: &'a str,
}

/// The plan's internal phase. Never widened without a frozen-criteria check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// D only: accepted on Wi-Fi, waiting for the Wi-Fi → cellular preparation.
    AwaitStartBearer,
    /// D only: the preparation hop was seen, waiting for its recovery.
    Preparation,
    /// In position on the start bearer: waiting for the FORMAL switch.
    Ready,
    /// Formal switch seen: the pre control is owed on the NEW network.
    Switched,
    /// The core tee HIT the pre control on the new network: the shell may
    /// recover the production connections of this kind.
    Released,
    /// A new-network receipt was accepted: the post control is owed.
    ReceiptIssued,
}

#[derive(Clone, Debug)]
struct Plan {
    id: String,
    column: String,
    kind: String,
    rep: u32,
    /// Frozen:L42 pre-registration: the marker's target and protocol are fixed
    /// BEFORE any control is sent.
    target: String,
    proto: &'static str,
    net_before: String,
    local_before: String,
    planned_ms: u64,
    phase: Phase,
    /// D: the Wi-Fi → cellular preparation hop (never the D measurement).
    prepare_ms: Option<u64>,
    /// D: the preparation's recovery completed on the cellular side.
    ready_ms: Option<u64>,
    /// The FORMAL switch instant.
    switch_ms: Option<u64>,
    /// The NEW network's platform net id, measured by the shell at the switch.
    net_id_after: u64,
    /// The identity of the live registration on the OLD network at the switch —
    /// the baseline every receipt must differ from.
    identity_at_switch: Option<Identity>,
    net_after: String,
    local_after: String,
    /// The pre control's HIT instant, as the core tee observed it.
    pre_hit_ms: Option<u64>,
    /// The release instant (pre HIT confirmed after the formal switch).
    release_ms: Option<u64>,
    receipt_ms: Option<u64>,
    /// `step=receipt` (send the post) was handed to the shell once.
    post_offered: bool,
    receipt_fd: RawFd,
    receipt_dup: RawFd,
    receipt_generation: u64,
    receipt_net_id: u64,
    /// The receipt's own I/O instant. For an accepted `Wb` substitute receipt
    /// this IS the new generation's core `protect_mono_ms` (the shell-verdict /
    /// platform-completion upper bound sampled when the protect result was
    /// handed to the core) — **never** the later receipt callback clock
    /// (`receipt_ms`). The `Wb` post strictness (`post_hit_ms > protect_mono_ms`)
    /// compares against THIS field.
    receipt_io_ms: u64,
    receipt_remote: String,
    /// The post control's HIT instant, as the core tee observed it.
    post_hit_ms: Option<u64>,
}

impl Plan {
    fn receipt_identity(&self) -> Identity {
        Identity::new(self.receipt_fd, self.receipt_dup, self.receipt_generation)
    }
}

#[derive(Default)]
struct State {
    plan: Option<Plan>,
    /// Plan ids already consumed: the derived control markers must be fresh,
    /// and a repeated id would be refused by the control registry anyway.
    used: Vec<String>,
    last_line: Option<String>,
    plans: u64,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

fn state() -> MutexGuard<'static, State> {
    STATE.get_or_init(|| Mutex::new(State::default())).lock().unwrap_or_else(PoisonError::into_inner)
}

/// Drop everything. Called on every arming change.
pub fn reset() {
    *state() = State::default();
}

/// One emitted line plus its parsed fields. An UNARMED call answers
/// `rejected|not-armed` with an EMPTY `line` and emits nothing.
#[derive(Debug)]
pub struct Outcome {
    pub result: &'static str,
    pub reason: String,
    pub id: String,
    pub line: String,
}

fn silent(result: &'static str, reason: &str, id: &str) -> Outcome {
    Outcome { result, reason: reason.to_string(), id: id.to_string(), line: String::new() }
}

/// One `n2_cd_poll` answer: the shell's next action, if any.
#[derive(Clone, Debug)]
pub struct Step {
    /// `idle` (no plan) / `wait` / `pre` (send the PRE control on the NEW
    /// network NOW — production recovery stays blocked) / `recover` (the tee
    /// HIT the pre: the shell may recover the production connections) /
    /// `receipt` (send the post control now) / `closed`.
    pub step: &'static str,
    /// The phase of the line this step emitted (`""` when it emitted none); the
    /// shell mirrors it in its own receipt so both logs join on the same phase.
    pub phase: String,
    pub id: String,
    pub kind: String,
    pub column: String,
    pub fd: RawFd,
    pub dup_fd: RawFd,
    pub remote: String,
    /// The control's own marker: the PRE marker on `step=pre`, the POST marker
    /// on `step=receipt` and on the post-in-flight `wait`.
    pub marker: String,
    pub result: String,
    pub reason: String,
    /// The line this poll emitted (`""` when it emitted none).
    pub line: String,
}

impl Step {
    fn idle() -> Self {
        Step { step: "idle", phase: String::new(), id: String::new(), kind: String::new(),
            column: String::new(), fd: -1, dup_fd: -1, remote: "unknown".into(),
            marker: String::new(), result: String::new(), reason: String::new(),
            line: String::new() }
    }
}

/// The volatile identity facts a line carries (the review's
/// `net_id`/`protect_generation`/`request_id`/`event_mono_ms` ask).
#[derive(Clone, Debug, Default)]
struct Facts {
    net_id: u64,
    generation: u64,
    identity: Option<Identity>,
    event_mono_ms: u64,
    local: String,
    request_id: String,
    detail: String,
}

impl Facts {
    fn of(identity: Option<Identity>, net_id: u64, generation: u64, event_mono_ms: u64,
        local: &str, request_id: &str, detail: &str) -> Self {
        Facts { net_id, generation, identity, event_mono_ms, local: local.to_string(),
            request_id: request_id.to_string(), detail: detail.to_string() }
    }
}

fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-' | '_' | '@') { c } else { '_' })
        .collect()
}

/// `clean`, with an empty value spelled `none` instead of an empty field.
fn val(text: &str) -> String {
    if text.is_empty() { "none".to_string() } else { clean(text) }
}

fn valid_marker(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn valid_target(target: &str) -> bool {
    let Some((ip, port)) = target.rsplit_once(':') else { return false };
    if ip.parse::<std::net::Ipv4Addr>().is_err() { return false; }
    matches!(port.parse::<u16>(), Ok(port) if port != 0)
}

/// The frozen outer classes' transport: `M`/`S`/`R` are TCP (a connect plus a
/// real business write), everything else is UDP.
pub fn proto_of(kind: &str) -> &'static str {
    if matches!(kind, "M" | "S" | "R") { "tcp" } else { "udp" }
}

fn identity_field(identity: Option<Identity>) -> String {
    match identity {
        Some(identity) => format!("{}:{}:{}", identity.fd, identity.dup_fd, identity.generation),
        None => "none".to_string(),
    }
}

/// The exact `N2_CD_SANDWICH` line of the frozen format baseline. Every volatile
/// input is an argument, so the log-format baseline pins the FORMAT and not one
/// run's clocks.
///
/// This is the **compat/baseline** formatter: `tests/n2_log_samples.rs` renders
/// the frozen fixture from it, so its shape must not move. The state machine
/// emits [`format_line_ext`], which is the same field sequence with the
/// identity facts the review asked for appended after `mono_ms`.
#[allow(clippy::too_many_arguments)]
pub fn format_line(id: &str, column: &str, kind: &str, rep: u32, phase: &str, marker: &str,
    result: &str, reason: &str, net_before: &str, net_after: &str, local_before: &str,
    local_after: &str, fd: RawFd, dup_fd: RawFd, remote: &str, wait_ms: u64,
    mono_ms: u64) -> String {
    format!(
        "N2_CD_SANDWICH|id={}|column={}|kind={}|rep={rep}|phase={}|marker={}|result={}|reason={}|net_before={}|net_after={}|local_before={}|local_after={}|fd={fd}|dup_fd={dup_fd}|remote={}|wait_ms={wait_ms}|mono_ms={mono_ms}|diag=true|is_evidence=false",
        clean(id), clean(column), clean(kind), clean(phase), clean(marker), clean(result),
        clean(reason), clean(net_before), clean(net_after), clean(local_before),
        clean(local_after), clean(remote))
}

/// The state machine's line: [`format_line`]'s field sequence plus the identity
/// block (`net_id`, `protect_generation`, `identity`, `event_mono_ms`, `local`,
/// `request_id`) and the frozen:L42 pre-registration (`target`, `proto`,
/// `detail`) before the `diag`/`is_evidence` tail.
#[allow(clippy::too_many_arguments)]
fn format_line_ext(id: &str, column: &str, kind: &str, rep: u32, phase: &str, marker: &str,
    result: &str, reason: &str, net_before: &str, net_after: &str, local_before: &str,
    local_after: &str, fd: RawFd, dup_fd: RawFd, remote: &str, wait_ms: u64, mono_ms: u64,
    facts: &Facts, target: &str, proto: &str) -> String {
    format!(
        "N2_CD_SANDWICH|id={}|column={}|kind={}|rep={rep}|phase={}|marker={}|result={}|reason={}|net_before={}|net_after={}|local_before={}|local_after={}|fd={fd}|dup_fd={dup_fd}|remote={}|wait_ms={wait_ms}|mono_ms={mono_ms}|net_id={}|protect_generation={}|identity={}|event_mono_ms={}|local={}|request_id={}|target={}|proto={}|detail={}|diag=true|is_evidence=false",
        clean(id), clean(column), clean(kind), clean(phase), clean(marker), clean(result),
        clean(reason), clean(net_before), clean(net_after), clean(local_before),
        clean(local_after), clean(remote), facts.net_id, facts.generation,
        identity_field(facts.identity), facts.event_mono_ms, val(&facts.local),
        val(&facts.request_id), val(target), val(proto), val(&facts.detail))
}

#[allow(clippy::too_many_arguments)]
fn plan_line(plan: &Plan, phase: &str, marker: &str, result: &str, reason: &str, fd: RawFd,
    dup_fd: RawFd, remote: &str, wait_ms: u64, mono_ms: u64, facts: &Facts) -> String {
    format_line_ext(&plan.id, &plan.column, &plan.kind, plan.rep, phase, marker, result, reason,
        &plan.net_before, &plan.net_after, &plan.local_before, &plan.local_after, fd, dup_fd,
        remote, wait_ms, mono_ms, facts, &plan.target, plan.proto)
}

#[allow(clippy::too_many_arguments)]
fn record(plan: &Plan, phase: &str, marker: &str, result: &'static str, reason: &str,
    wait_ms: u64, now: u64, fd: RawFd, dup_fd: RawFd, remote: &str, facts: &Facts) -> Outcome {
    let line = plan_line(plan, phase, marker, result, reason, fd, dup_fd, remote, wait_ms, now, facts);
    hilog::emit(&line);
    state().last_line = Some(line.clone());
    Outcome { result, reason: reason.to_string(), id: plan.id.clone(), line }
}

/// Emit a terminal line for `plan` and retire it (move the id to `used`).
#[allow(clippy::too_many_arguments)]
fn close(planned: Plan, phase: &str, marker: &str, result: &'static str, reason: &str,
    wait_ms: u64, now: u64, fd: RawFd, dup_fd: RawFd, remote: &str, facts: &Facts) -> Outcome {
    let outcome = record(&planned, phase, marker, result, reason, wait_ms, now, fd, dup_fd, remote,
        facts);
    let mut state = state();
    state.plan = None;
    if state.used.len() >= USED_MAX {
        state.used.remove(0);
    }
    state.used.push(planned.id);
    outcome
}

fn pre_marker(id: &str) -> String { format!("{id}-pre") }
fn post_marker(id: &str) -> String { format!("{id}-post") }

/// Accept ONE switching-sandwich plan. `start_verdict` is the SHELL's
/// start-network classification (`""` = the plan's start bearer was measured,
/// otherwise the named refusal token): the core cannot read the platform's
/// bearer information, so the shell measures and the core records.
///
/// Both columns are issued **while the host still has the Wi-Fi link**, so the
/// measured start bearer must be `wifi`; anything else (including `unknown`) is
/// refused by name. `C` is in position immediately; `D` is PRE-SET
/// (`reason=await-start-bearer`) and waits for its preparation.
#[allow(clippy::too_many_arguments)]
pub fn plan(now: u64, id: &str, column: &str, kind: &str, rep: u32, target: &str,
    net_before: &str, local_before: &str, start_verdict: &str) -> Outcome {
    if !crate::n2_socket_trace::armed() {
        return silent("rejected", "not-armed", id);
    }
    // A refusal before any plan state exists still needs a carrier line: build
    // a throwaway plan-shaped record so every refused request is visible by id.
    let probe = Plan { id: id.to_string(), column: column.to_string(), kind: kind.to_string(),
        rep, target: target.to_string(), proto: proto_of(kind), net_before: net_before.to_string(),
        local_before: local_before.to_string(), planned_ms: now, phase: Phase::Ready,
        prepare_ms: None, ready_ms: None, switch_ms: None, net_id_after: 0,
        identity_at_switch: None, net_after: "unknown".into(), local_after: "unknown".into(),
        pre_hit_ms: None, release_ms: None, receipt_ms: None, post_offered: false,
        receipt_fd: -1, receipt_dup: -1, receipt_generation: 0, receipt_net_id: 0,
        receipt_io_ms: 0, receipt_remote: "unknown".into(), post_hit_ms: None };
    let facts = Facts::of(None, 0, 0, 0, "", "", "");
    let refuse = |reason: &'static str| -> Outcome {
        record(&probe, "plan", "none", "rejected", reason, SWITCH_WAIT_MS, now, -1, -1, "unknown",
            &facts)
    };
    if !COLUMNS.contains(&column) { return refuse("invalid-column"); }
    if !KINDS.contains(&kind) { return refuse("invalid-kind"); }
    if !(1..=999).contains(&rep) { return refuse("invalid-rep"); }
    if !valid_marker(id) { return refuse("invalid-id"); }
    if !valid_target(target) { return refuse("invalid-target"); }
    if !start_verdict.is_empty() {
        return record(&probe, "plan", "none", "rejected", start_verdict, SWITCH_WAIT_MS, now,
            -1, -1, "unknown", &facts);
    }
    // The plan is issued over the Wi-Fi hdc link; a plan whose measured start
    // bearer is not Wi-Fi cannot be the C/D start.
    if net_before != "wifi" { return refuse("start-bearer-not-wifi"); }
    let busy = {
        let state = state();
        if state.plan.is_some() {
            Some("plan-already-active")
        } else if state.used.iter().any(|used| used == id) {
            Some("plan-already-used")
        } else {
            None
        }
    };
    if let Some(reason) = busy {
        return refuse(reason);
    }
    let (phase, ready_ms, reason, wait_ms) = match column {
        "D" => (Phase::AwaitStartBearer, None, "await-start-bearer", PREPARE_WAIT_MS),
        _ => (Phase::Ready, Some(now), "ok", SWITCH_WAIT_MS),
    };
    let planned = Plan { phase, ready_ms, ..probe };
    {
        let mut state = state();
        state.plans = state.plans.saturating_add(1);
        state.plan = Some(planned.clone());
    }
    record(&planned, "plan", "none", "planned", reason, wait_ms, now, -1, -1, "unknown", &facts)
}

/// D column: the Wi-Fi → cellular **preparation hop**. Recorded, never counted
/// as the D measurement: no control is sent for it and it does not open the
/// production-recovery gate.
pub fn prepare(now: u64, id: &str, verdict: &str, net_after: &str, local_after: &str) -> Outcome {
    if !crate::n2_socket_trace::armed() {
        return silent("rejected", "not-armed", id);
    }
    let planned = state().plan.clone();
    let Some(planned) = planned.filter(|plan| plan.id == id) else {
        return silent("rejected", "no-active-plan", id);
    };
    let mut measured = planned.clone();
    measured.net_after = net_after.to_string();
    measured.local_after = local_after.to_string();
    let facts = Facts::of(None, 0, 0, now, local_after, "", "not-a-d-measurement");
    if planned.column != "D" {
        return record(&measured, "prepare", "none", "rejected", "not-a-d-column",
            PREPARE_WAIT_MS, now, -1, -1, "unknown", &facts);
    }
    if planned.phase != Phase::AwaitStartBearer {
        return record(&measured, "prepare", "none", "rejected", "prepare-not-owed",
            PREPARE_WAIT_MS, now, -1, -1, "unknown", &facts);
    }
    if !verdict.is_empty() {
        return close(measured, "prepare", "none", "unsat", verdict, PREPARE_WAIT_MS, now,
            -1, -1, "unknown", &facts);
    }
    measured.phase = Phase::Preparation;
    measured.prepare_ms = Some(now);
    state().plan = Some(measured.clone());
    record(&measured, "prepare", "none", "prepared", "not-a-d-measurement", PREPARE_WAIT_MS,
        now, -1, -1, "unknown", &facts)
}

/// D column: the preparation's recovery completed on the cellular side (the
/// shell's `VPN_NETWORK_RECOVERED`). Only then is the plan in position for the
/// FORMAL cellular → Wi-Fi transition.
pub fn ready(now: u64, id: &str) -> Outcome {
    if !crate::n2_socket_trace::armed() {
        return silent("rejected", "not-armed", id);
    }
    let planned = state().plan.clone();
    let Some(planned) = planned.filter(|plan| plan.id == id) else {
        return silent("rejected", "no-active-plan", id);
    };
    let facts = Facts::of(None, 0, 0, now, &planned.local_after, "", "preparation-recovered");
    if planned.phase != Phase::Preparation {
        return record(&planned, "prepare", "none", "rejected", "recovery-not-owed",
            SWITCH_WAIT_MS, now, -1, -1, "unknown", &facts);
    }
    let mut in_position = planned.clone();
    in_position.phase = Phase::Ready;
    in_position.ready_ms = Some(now);
    state().plan = Some(in_position.clone());
    record(&in_position, "prepare", "none", "ready", "preparation-recovered", SWITCH_WAIT_MS,
        now, -1, -1, "unknown", &facts)
}

/// The FORMAL switch report, with the identity baseline the receipt verdict
/// needs:
///
/// * `verdict` — the shell's pure direction verdict (`""` = the measured
///   transition matches the column; any other value is a named refusal token);
/// * `net_id_after` — the NEW network's platform net id (0 = not measured);
/// * `identity_at_switch` — the identity of the live registration of this kind
///   on the OLD network at the switch instant (`None` = not measured).
///
/// Every missing fact is a named `unsat` (fail-closed): without them the only
/// available "receipt" criterion would be the process-global live registration
/// counter, which is exactly what the T0 review rejected.
#[allow(clippy::too_many_arguments)]
pub fn switch_identified(now: u64, id: &str, verdict: &str, net_after: &str, local_after: &str,
    net_id_after: u64, identity_at_switch: Option<Identity>) -> Outcome {
    if !crate::n2_socket_trace::armed() {
        return silent("rejected", "not-armed", id);
    }
    let planned = state().plan.clone();
    let Some(planned) = planned.filter(|plan| plan.id == id) else {
        return silent("rejected", "no-active-plan", id);
    };
    let mut measured = planned.clone();
    measured.net_after = net_after.to_string();
    measured.local_after = local_after.to_string();
    let baseline = identity_at_switch.filter(|identity| identity.fd >= 0 && identity.dup_fd >= 0);
    let facts = Facts::of(baseline, net_id_after, baseline.map_or(0, |b| b.generation), now,
        local_after, "", "awaiting-pre-hit");
    if !verdict.is_empty() {
        return close(measured, "switch", "none", "unsat", verdict, RECEIPT_WAIT_MS, now,
            -1, -1, "unknown", &facts);
    }
    match planned.phase {
        Phase::AwaitStartBearer => {
            // D's Wi-Fi → cellular hop is the PREPARE hop. A "formal" report for
            // it means the caller skipped [`prepare`] and [`ready`].
            return close(measured, "switch", "none", "unsat", "prepare-hop-required",
                RECEIPT_WAIT_MS, now, -1, -1, "unknown", &facts);
        }
        Phase::Preparation => {
            return close(measured, "switch", "none", "unsat", "switch-before-preparation-complete",
                RECEIPT_WAIT_MS, now, -1, -1, "unknown", &facts);
        }
        Phase::Ready => {}
        Phase::Switched | Phase::Released | Phase::ReceiptIssued => {
            return record(&measured, "switch", "none", "rejected", "switch-already-reported",
                PRE_WAIT_MS, now, -1, -1, "unknown", &facts);
        }
    }
    if planned.column == "D" {
        // The prepared bearer has to be dwelled on: a faster return is a flap,
        // not the D column's event.
        let from = planned.ready_ms.unwrap_or(planned.planned_ms);
        if now.saturating_sub(from) < DWELL_MIN_MS {
            return close(measured, "switch", "none", "unsat", "dwell-min-not-reached",
                RECEIPT_WAIT_MS, now, -1, -1, "unknown", &facts);
        }
    }
    match (baseline, net_id_after) {
        (None, 0) => {
            return close(measured, "switch", "none", "unsat", "switch-identity-required",
                RECEIPT_WAIT_MS, now, -1, -1, "unknown", &facts);
        }
        (None, _) => {
            return close(measured, "switch", "none", "unsat", "switch-identity-missing",
                RECEIPT_WAIT_MS, now, -1, -1, "unknown", &facts);
        }
        (Some(_), 0) => {
            return close(measured, "switch", "none", "unsat", "switch-net-id-unmeasured",
                RECEIPT_WAIT_MS, now, -1, -1, "unknown", &facts);
        }
        (Some(identity), _) if identity.generation == 0 => {
            return close(measured, "switch", "none", "unsat", "switch-generation-unmeasured",
                RECEIPT_WAIT_MS, now, -1, -1, "unknown", &facts);
        }
        (Some(identity), _) => {
            measured.switch_ms = Some(now);
            measured.net_id_after = net_id_after;
            measured.identity_at_switch = Some(identity);
            measured.phase = Phase::Switched;
            state().plan = Some(measured.clone());
            record(&measured, "switch", "none", "switched", "ok", PRE_WAIT_MS, now, -1, -1,
                "unknown", &facts)
        }
    }
}

/// The legacy five-argument switch entry. It carries NO identity facts, so it
/// can only be judged by the process-global live counter — the criterion this
/// revision removed. Kept for signature compatibility and **fail-closed**: a
/// plan driven through it closes `unsat|switch-identity-required` and the
/// production-recovery gate never opens.
pub fn switch(now: u64, id: &str, verdict: &str, net_after: &str, local_after: &str) -> Outcome {
    switch_identified(now, id, verdict, net_after, local_after, 0, None)
}

/// The armed tee consumed a control marker (an `N2_CONTROL_HIT`). Records the
/// pre/post HIT the ordering verdict is judged on — the shell's own report is
/// never sufficient.
///
/// The PRE marker only counts while the plan is in the `Switched` phase: a HIT
/// the tee saw on the OLD network (before or during the preparation) must never
/// become the gate opener.
pub fn control_hit(marker: &str, now: u64) {
    if !crate::n2_socket_trace::armed() {
        return;
    }
    let mut state = state();
    let Some(plan) = state.plan.as_mut() else { return };
    if marker == pre_marker(&plan.id) && plan.phase == Phase::Switched
        && plan.switch_ms.is_some_and(|switch_ms| now >= switch_ms)
        && plan.pre_hit_ms.is_none()
    {
        plan.pre_hit_ms = Some(now);
    } else if marker == post_marker(&plan.id) && plan.phase == Phase::ReceiptIssued
        && plan.receipt_ms.is_some_and(|receipt_ms| now >= receipt_ms)
        && plan.post_hit_ms.is_none()
    {
        plan.post_hit_ms = Some(now);
    }
}

/// The shell's verdict of the PRE control (`result` = `hit` when its bounded
/// wait saw the HIT, `rejected`/`timeout` otherwise). A `hit` the core never
/// observed is `unsat|pre-hit-unobserved`: the shell's claim alone can never
/// complete a sandwich.
///
/// Before the FORMAL switch this is refused by name and the gate stays shut —
/// the pre must precede the production recovery on the NEW network, not sit on
/// the old one.
pub fn pre(now: u64, id: &str, result: &str, reason: &str) -> Outcome {
    if !crate::n2_socket_trace::armed() {
        return silent("rejected", "not-armed", id);
    }
    let planned = state().plan.clone();
    let Some(planned) = planned.filter(|plan| plan.id == id) else {
        return silent("rejected", "no-active-plan", id);
    };
    let marker = pre_marker(id);
    let facts = Facts::of(planned.identity_at_switch, planned.net_id_after,
        planned.identity_at_switch.map_or(0, |b| b.generation), planned.switch_ms.unwrap_or(now),
        &planned.local_after, "", "pre-control");
    match planned.phase {
        Phase::Released | Phase::ReceiptIssued => {
            return record(&planned, "pre", &marker, "rejected", "pre-already-reported",
                RECEIPT_WAIT_MS, now, -1, -1, "unknown", &facts);
        }
        Phase::Switched => {}
        Phase::Ready | Phase::Preparation | Phase::AwaitStartBearer => {
            // The plan is not on the new network yet: this pre belongs to the
            // OLD network and can never open the gate.
            return record(&planned, "pre", &marker, "rejected", "pre-before-switch",
                PRE_WAIT_MS, now, -1, -1, "unknown", &facts);
        }
    }
    if result == "rejected" || result == "timeout" {
        let reason = if reason.is_empty() {
            if result == "timeout" { "pre-hit-timeout" } else { "pre-control-miss" }
        } else {
            reason
        };
        return close(planned, "pre", &marker, "unsat", reason, PRE_WAIT_MS, now, -1, -1,
            "unknown", &facts);
    }
    if result != "hit" {
        return record(&planned, "pre", &marker, "rejected", "invalid-result", PRE_WAIT_MS, now,
            -1, -1, "unknown", &facts);
    }
    let Some(pre_hit_ms) = planned.pre_hit_ms else {
        return close(planned, "pre", &marker, "unsat", "pre-hit-unobserved", PRE_WAIT_MS, now,
            -1, -1, "unknown", &facts);
    };
    let switch_ms = planned.switch_ms.unwrap_or(now);
    if pre_hit_ms < switch_ms {
        return close(planned, "pre", &marker, "unsat", "pre-hit-before-switch", PRE_WAIT_MS, now,
            -1, -1, "unknown", &facts);
    }
    let mut released = planned.clone();
    released.phase = Phase::Released;
    released.release_ms = Some(now);
    state().plan = Some(released.clone());
    record(&released, "pre", &marker, "hit", "gate-open", RECEIPT_WAIT_MS, now, -1, -1,
        "unknown", &Facts::of(released.identity_at_switch, released.net_id_after,
            released.identity_at_switch.map_or(0, |b| b.generation), pre_hit_ms,
            &released.local_after, "", "release-production-recovery"))
}

/// Feed ONE candidate production receipt on the NEW network. Accepts only when
/// every identity criterion in the module docs holds; every refusal is a named,
/// NON-terminal `rejected` line (the plan keeps waiting for a valid receipt
/// until `receipt-wait-exceeded`), so a stale old-network observation can never
/// finish the sandwich **and** cannot silently kill a live plan either.
pub fn receipt(now: u64, id: &str, receipt: &Receipt<'_>) -> Outcome {
    if !crate::n2_socket_trace::armed() {
        return silent("rejected", "not-armed", id);
    }
    let planned = state().plan.clone();
    let Some(planned) = planned.filter(|plan| plan.id == id) else {
        return silent("rejected", "no-active-plan", id);
    };
    let facts = Facts::of(Some(receipt.identity), receipt.net_id, receipt.identity.generation,
        receipt.io_mono_ms, receipt.local, receipt.request_id, receipt.detail);
    let refuse = |reason: &'static str| -> Outcome {
        record(&planned, "receipt", "none", "rejected", reason, RECEIPT_WAIT_MS, now,
            receipt.identity.fd, receipt.identity.dup_fd, receipt.remote, &facts)
    };
    match planned.phase {
        Phase::Released => {}
        Phase::Switched => return refuse("receipt-before-pre-hit"),
        Phase::ReceiptIssued => return refuse("receipt-already-issued"),
        Phase::Ready | Phase::Preparation | Phase::AwaitStartBearer => {
            return refuse("receipt-before-formal-switch");
        }
    }
    if receipt.identity.fd < 0 || receipt.identity.dup_fd < 0 { return refuse("invalid-fd"); }
    if receipt.identity.generation == 0 { return refuse("generation-unmeasured"); }
    if receipt.net_id == 0 { return refuse("net-id-unmeasured"); }
    if receipt.net_id != planned.net_id_after { return refuse("net-id-mismatch"); }
    if !receipt.protect_ok { return refuse("protect-failed"); }
    if receipt.io_mono_ms == 0 { return refuse("io-time-unmeasured"); }
    let pre_hit_ms = planned.pre_hit_ms.unwrap_or(0);
    if receipt.io_mono_ms <= pre_hit_ms { return refuse("io-before-pre-hit"); }
    let baseline = planned.identity_at_switch;
    if baseline.is_some_and(|base| base.generation == receipt.identity.generation) {
        // Not merely "a different fd number": the protect generation is the
        // same, so this is the OLD generation re-appearing (a reused number, or
        // the old socket re-reported).
        return refuse("generation-unchanged");
    }
    if baseline.is_some_and(|base| base.fd == receipt.identity.fd
        && base.dup_fd == receipt.identity.dup_fd)
    {
        return refuse("identity-unchanged");
    }
    if matches!(planned.kind.as_str(), "M" | "S" | "R") && !receipt.subsequent_io {
        // A completed `connect` is not a business send (frozen L38: a state line
        // is not an independent frame).
        return refuse("connect-only");
    }
    if planned.kind == "Wb" && !receipt.protect_settle {
        // Wb has no production write, so the new generation's protect settle IS
        // its receipt; `exempted` is never handed out at the switch.
        return refuse("wb-protect-settle-missing");
    }
    let (result, reason) = if planned.kind == "Wb" {
        ("exempted", "wb-protect-settle-success")
    } else {
        ("receipt", "ok")
    };
    let mut issued = planned.clone();
    issued.phase = Phase::ReceiptIssued;
    issued.receipt_ms = Some(now);
    issued.post_offered = false;
    issued.receipt_fd = receipt.identity.fd;
    issued.receipt_dup = receipt.identity.dup_fd;
    issued.receipt_generation = receipt.identity.generation;
    issued.receipt_net_id = receipt.net_id;
    issued.receipt_io_ms = receipt.io_mono_ms;
    issued.receipt_remote = receipt.remote.to_string();
    state().plan = Some(issued.clone());
    record(&issued, "receipt", "none", result, reason, POST_WAIT_MS, now, receipt.identity.fd,
        receipt.identity.dup_fd, receipt.remote, &facts)
}

/// Whether an ARMED plan of exactly this outer class is active — the cheap gate
/// the socket-trace call sites use before doing any extra work for the C/D
/// wiring (a `getsockname`, building a receipt). `false` while no plan is
/// armed, and for every class other than the plan's own.
pub fn awaiting_kind(kind: &str) -> bool {
    if !crate::n2_socket_trace::armed() {
        return false;
    }
    state().plan.as_ref().is_some_and(|plan| plan.kind == kind)
}

/// The CORE-OWNED receipt entry point (the ONE the socket-trace call sites
/// use): hand a receipt candidate measured by the core itself to the active
/// plan **of the same class**. `None` = no armed plan of that class, and then
/// NOTHING is emitted; `Some(outcome)` = the verdict line, already emitted.
///
/// This is [`receipt`] plus the class gate: the gate lives here — inside the
/// module that owns the plan — so the trace call sites stay silent (and cheap)
/// while no C/D sandwich measures that class.
pub fn receipt_candidate(now: u64, kind: &str, candidate: &Receipt<'_>) -> Option<Outcome> {
    if !crate::n2_socket_trace::armed() {
        return None;
    }
    let id = {
        let state = state();
        state.plan.as_ref().filter(|plan| plan.kind == kind).map(|plan| plan.id.clone())?
    };
    Some(receipt(now, &id, candidate))
}

fn wait_step(plan: &Plan, reason: &str, marker: &str, fd: RawFd, dup_fd: RawFd,
    remote: &str) -> Step {
    action_step("wait", plan, reason, marker, fd, dup_fd, remote)
}

fn action_step(step: &'static str, plan: &Plan, reason: &str, marker: &str, fd: RawFd,
    dup_fd: RawFd, remote: &str) -> Step {
    Step { step, phase: String::new(), id: plan.id.clone(), kind: plan.kind.clone(),
        column: plan.column.clone(), fd, dup_fd, remote: remote.to_string(),
        marker: marker.to_string(), result: step.to_string(), reason: reason.to_string(),
        line: String::new() }
}

fn closed_step(phase: &str, plan: &Plan, outcome: &Outcome, fd: RawFd, dup_fd: RawFd,
    remote: &str) -> Step {
    Step { step: "closed", phase: phase.to_string(), id: plan.id.clone(), kind: plan.kind.clone(),
        column: plan.column.clone(), fd, dup_fd, remote: remote.to_string(),
        marker: String::new(), result: outcome.result.to_string(), reason: outcome.reason.clone(),
        line: outcome.line.clone() }
}

/// One watcher-tick step. Emits a terminal line when the plan times out, asks
/// for the PRE control on the new network (`step=pre`), states the standing
/// production-recovery permission (`step=recover`, only after the tee HIT), or
/// offers the POST control (`step=receipt`) once.
pub fn poll(now: u64) -> Step {
    if !crate::n2_socket_trace::armed() {
        return Step::idle();
    }
    let Some(planned) = state().plan.clone() else { return Step::idle() };
    let pre = pre_marker(&planned.id);
    let post = post_marker(&planned.id);
    let none = Facts::default();
    match planned.phase {
        Phase::AwaitStartBearer => {
            if now.saturating_sub(planned.planned_ms) >= PREPARE_WAIT_MS {
                let outcome = close(planned.clone(), "prepare", "none", "unsat",
                    "prepare-wait-exceeded", PREPARE_WAIT_MS, now, -1, -1, "unknown", &none);
                return closed_step("prepare", &planned, &outcome, -1, -1, "unknown");
            }
            wait_step(&planned, "await-start-bearer", "", -1, -1, "unknown")
        }
        Phase::Preparation => {
            let from = planned.prepare_ms.unwrap_or(planned.planned_ms);
            if now.saturating_sub(from) >= PREPARE_WAIT_MS {
                let outcome = close(planned.clone(), "prepare", "none", "unsat",
                    "prepare-recovery-wait-exceeded", PREPARE_WAIT_MS, now, -1, -1, "unknown",
                    &none);
                return closed_step("prepare", &planned, &outcome, -1, -1, "unknown");
            }
            wait_step(&planned, "await-preparation-recovery", "", -1, -1, "unknown")
        }
        Phase::Ready => {
            let from = planned.ready_ms.unwrap_or(planned.planned_ms);
            if now.saturating_sub(from) >= SWITCH_WAIT_MS {
                let outcome = close(planned.clone(), "switch", "none", "unsat",
                    "switch-wait-exceeded", SWITCH_WAIT_MS, now, -1, -1, "unknown", &none);
                return closed_step("switch", &planned, &outcome, -1, -1, "unknown");
            }
            wait_step(&planned, "await-switch", "", -1, -1, "unknown")
        }
        Phase::Switched => {
            let from = planned.switch_ms.unwrap_or(now);
            if now.saturating_sub(from) >= PRE_WAIT_MS {
                let outcome = close(planned.clone(), "pre", "none", "unsat",
                    "pre-wait-exceeded", PRE_WAIT_MS, now, -1, -1, "unknown", &none);
                return closed_step("pre", &planned, &outcome, -1, -1, "unknown");
            }
            action_step("pre", &planned, "send-pre-now", &pre, -1, -1, "unknown")
        }
        Phase::Released => {
            let from = planned.release_ms.unwrap_or(now);
            if now.saturating_sub(from) >= RECEIPT_WAIT_MS {
                let outcome = close(planned.clone(), "receipt", "none", "unsat",
                    "receipt-wait-exceeded", RECEIPT_WAIT_MS, now, -1, -1, "unknown", &none);
                return closed_step("receipt", &planned, &outcome, -1, -1, "unknown");
            }
            // The standing permission: re-stated every tick until a receipt
            // arrives (the shell latches its own recovery).
            action_step("recover", &planned, "pre-hit-observed", "", -1, -1, "unknown")
        }
        Phase::ReceiptIssued => {
            let from = planned.receipt_ms.unwrap_or(now);
            if now.saturating_sub(from) >= POST_WAIT_MS {
                let facts = Facts::of(Some(planned.receipt_identity()), planned.receipt_net_id,
                    planned.receipt_generation, planned.receipt_io_ms, "", "", "post-wait-exceeded");
                let outcome = close(planned.clone(), "post", &post, "unsat",
                    "post-wait-exceeded", POST_WAIT_MS, now, planned.receipt_fd,
                    planned.receipt_dup, &planned.receipt_remote, &facts);
                return closed_step("post", &planned, &outcome, planned.receipt_fd,
                    planned.receipt_dup, &planned.receipt_remote);
            }
            if !planned.post_offered {
                if let Some(active) = state().plan.as_mut() {
                    active.post_offered = true;
                }
                return action_step("receipt", &planned, "send-post-now", &post,
                    planned.receipt_fd, planned.receipt_dup, &planned.receipt_remote);
            }
            wait_step(&planned, "post-in-flight", &post, planned.receipt_fd, planned.receipt_dup,
                &planned.receipt_remote)
        }
    }
}

/// Close the plan with the shell's post-control verdict. `observed` requires
/// the core tee to have seen the POST HIT and it must follow the receipt
/// (`post_hit_ms >= receipt_ms`, the callback order — never relaxed). `Wb`
/// additionally requires the HIT to be STRICTLY later than the new
/// generation's OWN protect settle (`receipt_io_ms`, the core `protect_mono_ms`
/// — NOT the receipt callback clock), completing the frozen sandwich
/// `pre_hit_ms < protect_mono_ms < post_hit_ms`: a same-millisecond post is
/// refused `post-before-receipt`, because an equal sign proves no strict order.
pub fn post(now: u64, id: &str, result: &str, reason: &str) -> Outcome {
    if !crate::n2_socket_trace::armed() {
        return silent("rejected", "not-armed", id);
    }
    let planned = state().plan.clone();
    let Some(planned) = planned.filter(|plan| plan.id == id) else {
        return silent("rejected", "no-active-plan", id);
    };
    let marker = post_marker(id);
    let (receipt_fd, receipt_dup, receipt_remote) =
        (planned.receipt_fd, planned.receipt_dup, planned.receipt_remote.clone());
    let facts = Facts::of(Some(planned.receipt_identity()), planned.receipt_net_id,
        planned.receipt_generation, planned.receipt_io_ms, "", "", "post-control");
    let Some(receipt_ms) = planned.receipt_ms else {
        return close(planned, "post", &marker, "unsat", "post-before-receipt", POST_WAIT_MS, now,
            -1, -1, "unknown", &facts);
    };
    match result {
        "observed" => {
            let Some(post_hit_ms) = planned.post_hit_ms else {
                return close(planned, "post", &marker, "unsat", "post-hit-unobserved",
                    POST_WAIT_MS, now, receipt_fd, receipt_dup, &receipt_remote, &facts);
            };
            if post_hit_ms < receipt_ms {
                return close(planned, "post", &marker, "unsat", "post-before-receipt",
                    POST_WAIT_MS, now, receipt_fd, receipt_dup, &receipt_remote, &facts);
            }
            if planned.kind == "Wb" && post_hit_ms <= planned.receipt_io_ms {
                // `Wb` (C/D, same continuously-mounted tee): the approved
                // substitute ordering is STRICT —
                // `pre_hit_ms < protect_mono_ms < post_hit_ms`. The comparison
                // is against the new generation's OWN protect settle
                // (`receipt_io_ms`, the core `protect_mono_ms`), never the
                // later receipt callback clock (`receipt_ms`): a post HIT in
                // the SAME millisecond as the protect settle proves no strict
                // order and is `UNSAT`. The pre side (`io-before-pre-hit`) is
                // enforced at receipt time, so both boundaries are strict.
                return close(planned, "post", &marker, "unsat", "post-before-receipt",
                    POST_WAIT_MS, now, receipt_fd, receipt_dup, &receipt_remote, &facts);
            }
            close(planned, "post", &marker, "observed", "control-hit-after-receipt", POST_WAIT_MS,
                now, receipt_fd, receipt_dup, &receipt_remote, &facts)
        }
        "unsat" | "timeout" => {
            let reason = if reason.is_empty() { "post-control-miss" } else { reason };
            close(planned, "post", &marker, "unsat", reason, POST_WAIT_MS, now, receipt_fd,
                receipt_dup, &receipt_remote, &facts)
        }
        _ => close(planned, "post", &marker, "rejected", "invalid-result", POST_WAIT_MS, now,
            receipt_fd, receipt_dup, &receipt_remote, &facts),
    }
}

// ---------------------------------------------------------------------------
// read-only inspection (host tests + the campaign judge)
// ---------------------------------------------------------------------------

/// The active plan's id (`""` = none).
pub fn active_id() -> String {
    state().plan.as_ref().map(|plan| plan.id.clone()).unwrap_or_default()
}

/// The active plan's phase (`""` = no plan): `await-start-bearer` /
/// `preparation` / `ready` / `switched` / `released` / `receipt-issued`.
pub fn active_phase() -> String {
    let state = state();
    let Some(plan) = state.plan.as_ref() else { return String::new() };
    match plan.phase {
        Phase::AwaitStartBearer => "await-start-bearer".into(),
        Phase::Preparation => "preparation".into(),
        Phase::Ready => "ready".into(),
        Phase::Switched => "switched".into(),
        Phase::Released => "released".into(),
        Phase::ReceiptIssued => "receipt-issued".into(),
    }
}

/// Whether the production-recovery gate of the active plan is OPEN (the tee hit
/// the pre control on the NEW network). `false` while no plan exists.
pub fn released() -> bool {
    let state = state();
    matches!(state.plan.as_ref().map(|plan| plan.phase),
        Some(Phase::Released) | Some(Phase::ReceiptIssued))
}

/// How many plans were accepted in this arming.
pub fn plan_count() -> u64 { state().plans }

/// The last `N2_CD_SANDWICH` line this process emitted.
pub fn last_line() -> Option<String> { state().last_line.clone() }

/// Host-test serialization (the SAME process-global lock the other N2 modules
/// take, so parallel test threads cannot arm/disarm each other).
#[cfg(test)]
pub(crate) fn test_guard() -> MutexGuard<'static, ()> {
    crate::n2_stimulus::test_guard()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset_all() {
        crate::n2_socket_trace::arm(false);
        reset();
    }

    /// A C-column plan, accepted on Wi-Fi.
    fn planned(kind: &str, id: &str, now: u64) -> Outcome {
        plan(now, id, "C", kind, 1, "119.29.29.119:443", "wifi", "192.168.50.199:0", "")
    }

    /// Drive a C plan to the released phase with synthetically observed HITs.
    fn release_c(kind: &str, id: &str, at: u64) -> Vec<Outcome> {
        let mut out = vec![planned(kind, id, at)];
        out.push(switch_identified(at + 10, id, "", "cellular", "10.77.0.5:0", 31,
            Some(Identity::new(21, 22, 1))));
        control_hit(&pre_marker(id), at + 20);
        out.push(pre(at + 30, id, "hit", "ok"));
        out
    }

    /// A receipt whose I/O was completed at `io_mono_ms` and whose identity is
    /// a NEW registration (fd 41/42, generation 2, net 31).
    fn business_receipt(io_mono_ms: u64) -> Receipt<'static> {
        Receipt { identity: Identity::new(41, 42, 2), net_id: 31, protect_ok: true,
            io_mono_ms, subsequent_io: true, protect_settle: false, detail: "post_connect_write",
            remote: "192.0.2.9:443", local: "10.77.0.5:51000", request_id: "42" }
    }

    /// A `Wb` substitute receipt: a NEW registration (fd 61/62, generation 2,
    /// net 31) whose protect settle is `io_mono_ms`. `io_mono_ms` IS the core
    /// `protect_mono_ms` of that generation (the Wb takeover feeds it verbatim);
    /// the `now` the caller passes to [`receipt`] is the separate callback clock.
    fn wb_settle(io_mono_ms: u64) -> Receipt<'static> {
        Receipt { identity: Identity::new(61, 62, 2), net_id: 31, protect_ok: true,
            io_mono_ms, subsequent_io: false, protect_settle: true, detail: "protect_settle",
            remote: "unknown", local: "192.168.50.199:0", request_id: "wb" }
    }

    #[test]
    fn unarmed_is_silent_and_refuses_every_entry_point() {
        let _guard = test_guard();
        reset_all();
        assert_eq!(planned("M", "cd-C-M-1", 1_000).reason, "not-armed");
        assert!(planned("M", "cd-C-M-1", 1_000).line.is_empty());
        assert!(pre(1_100, "cd-C-M-1", "hit", "ok").line.is_empty());
        assert!(prepare(1_150, "cd-D-M-1", "", "cellular", "10.0.0.5:0").line.is_empty());
        assert!(ready(1_160, "cd-D-M-1").line.is_empty());
        assert!(switch(1_200, "cd-C-M-1", "", "cellular", "10.0.0.5:0").line.is_empty());
        assert!(switch_identified(1_200, "cd-C-M-1", "", "cellular", "10.0.0.5:0", 31, None)
            .line.is_empty());
        assert!(receipt(1_250, "cd-C-M-1", &business_receipt(1_240)).line.is_empty());
        assert!(post(1_300, "cd-C-M-1", "observed", "ok").line.is_empty());
        assert_eq!(poll(1_400).step, "idle");
        assert_eq!(active_id(), "");
        assert_eq!(active_phase(), "");
        assert!(!released());
        assert_eq!(plan_count(), 0);
        assert!(last_line().is_none(), "zero output while unarmed");
        reset_all();
    }

    #[test]
    fn plan_validation_refusals_are_named_and_the_start_bearer_is_wifi() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        let cases = [
            ("CD-1", "E", "M", 1, "119.29.29.119:443", "wifi", "invalid-column"),
            ("CD-1", "C", "W", 1, "119.29.29.119:443", "wifi", "invalid-kind"),
            ("CD-1", "C", "M", 0, "119.29.29.119:443", "wifi", "invalid-rep"),
            ("bad marker", "C", "M", 1, "119.29.29.119:443", "wifi", "invalid-id"),
            ("CD-1", "C", "M", 1, "119.29.29.119:0", "wifi", "invalid-target"),
            ("CD-1", "C", "M", 1, "home.alfadb.cn:443", "wifi", "invalid-target"),
            ("CD-1", "C", "M", 1, "119.29.29.119:443", "cellular", "start-bearer-not-wifi"),
            ("CD-1", "D", "M", 1, "119.29.29.119:443", "unknown", "start-bearer-not-wifi"),
        ];
        for (id, column, kind, rep, target, bearer, reason) in cases {
            let outcome = plan(1_000, id, column, kind, rep, target, bearer, "unknown", "");
            assert_eq!(outcome.result, "rejected", "{outcome:?}");
            assert_eq!(outcome.reason, reason, "{id} {column} {kind}");
            assert!(outcome.line.contains(&format!("|reason={reason}|")), "{}", outcome.line);
        }
        // the start-network classification is the SHELL's measurement, recorded
        // verbatim and refused by name when it does not match the column
        let wrong_start = plan(1_000, "cd-C-M-2", "C", "M", 1, "119.29.29.119:443",
            "wifi", "192.168.50.199:0", "start-network-mismatch");
        assert_eq!(wrong_start.reason, "start-network-mismatch");
        // an accepted C plan, then: a second plan / a reused id are refused
        assert_eq!(planned("M", "cd-C-M-3", 2_000).result, "planned");
        assert_eq!(planned("S", "cd-C-S-1", 2_100).reason, "plan-already-active");
        assert_eq!(planned("M", "cd-C-M-3", 2_200).reason, "plan-already-active");
        assert_eq!(post(2_300, "cd-C-M-3", "observed", "ok").reason, "post-before-receipt");
        assert_eq!(active_id(), "", "the post refusal closes the plan");
        assert_eq!(planned("M", "cd-C-M-3", 2_400).reason, "plan-already-used");
        reset_all();
    }

    #[test]
    fn a_d_plan_is_pre_set_on_wifi_and_the_preparation_hop_is_not_the_d_measurement() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        let planned = plan(1_000, "cd-D-M-1", "D", "M", 1, "119.29.29.119:443", "wifi",
            "192.168.50.199:0", "");
        assert_eq!(planned.result, "planned");
        assert!(planned.line.contains("|phase=plan|marker=none|result=planned|reason=await-start-bearer|"),
            "{}", planned.line);
        assert!(planned.line.contains(&format!("|wait_ms={PREPARE_WAIT_MS}|")), "{}", planned.line);
        assert!(planned.line.contains("|target=119.29.29.119:443|proto=tcp|"),
            "the target/proto are pre-registered: {}", planned.line);
        assert_eq!(poll(1_100).reason, "await-start-bearer");
        // the Wi-Fi → cellular preparation hop: recorded, not a D measurement
        let prepared = prepare(1_200, "cd-D-M-1", "", "cellular", "10.77.0.5:0");
        assert_eq!(prepared.result, "prepared");
        assert!(prepared.line.contains("|phase=prepare|marker=none|result=prepared|reason=not-a-d-measurement|"),
            "{}", prepared.line);
        assert_eq!(active_phase(), "preparation");
        // …and a formal switch before the preparation is complete is unsat
        assert_eq!(switch_identified(1_300, "cd-D-M-1", "", "wifi", "192.168.50.199:0", 21,
            Some(Identity::new(21, 22, 1))).reason, "switch-before-preparation-complete");
        assert_eq!(active_id(), "");
        // a second rep: the full two-hop path
        assert_eq!(plan(2_000, "cd-D-M-2", "D", "M", 2, "119.29.29.119:443", "wifi",
            "192.168.50.199:0", "").result, "planned");
        assert_eq!(prepare(2_100, "cd-D-M-2", "", "cellular", "10.77.0.5:0").result, "prepared");
        assert_eq!(ready(2_200, "cd-D-M-2").result, "ready");
        assert_eq!(active_phase(), "ready");
        // the minimum dwell on the prepared bearer
        let early = switch_identified(2_200 + DWELL_MIN_MS - 1, "cd-D-M-2", "", "wifi",
            "192.168.50.199:0", 21, Some(Identity::new(11, 12, 1)));
        assert_eq!(early.reason, "dwell-min-not-reached");
        // the legacy entry carries no identity: fail-closed, no gate
        assert_eq!(plan(3_000, "cd-D-M-3", "D", "M", 3, "119.29.29.119:443", "wifi",
            "192.168.50.199:0", "").result, "planned");
        assert_eq!(prepare(3_100, "cd-D-M-3", "", "cellular", "10.77.0.5:0").result, "prepared");
        assert_eq!(ready(3_200, "cd-D-M-3").result, "ready");
        let legacy = switch(3_200 + DWELL_MIN_MS, "cd-D-M-3", "", "wifi", "192.168.50.199:0");
        assert_eq!(legacy.result, "unsat");
        assert_eq!(legacy.reason, "switch-identity-required");
        assert!(!released());
        reset_all();
    }

    #[test]
    fn a_pre_before_the_formal_switch_is_rejected_and_an_old_network_hit_never_opens_the_gate() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        assert_eq!(planned("M", "cd-C-M-1", 1_000).result, "planned");
        // the old network (Wi-Fi) HIT the tee — it must not even be recorded
        control_hit("cd-C-M-1-pre", 1_050);
        let early = pre(1_100, "cd-C-M-1", "hit", "ok");
        assert_eq!(early.result, "rejected");
        assert_eq!(early.reason, "pre-before-switch");
        assert!(!released(), "the gate stays shut on the old network");
        assert_eq!(active_id(), "cd-C-M-1", "the plan is still in position");
        // the formal switch happens; the old HIT is still not a usable witness
        assert_eq!(switch_identified(1_200, "cd-C-M-1", "", "cellular", "10.77.0.5:0", 31,
            Some(Identity::new(21, 22, 1))).result, "switched");
        assert_eq!(poll(1_300).step, "pre");
        assert_eq!(poll(1_300).marker, "cd-C-M-1-pre");
        let late = pre(1_400, "cd-C-M-1", "hit", "ok");
        assert_eq!(late.result, "unsat");
        assert_eq!(late.reason, "pre-hit-unobserved");
        assert!(!released());
        assert_eq!(active_id(), "");
        reset_all();
    }

    #[test]
    fn the_gate_opens_only_on_a_tee_hit_and_only_then_is_a_receipt_accepted() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        assert_eq!(planned("M", "cd-C-M-1", 1_000).result, "planned");
        assert_eq!(switch_identified(1_010, "cd-C-M-1", "", "cellular", "10.77.0.5:0", 31,
            Some(Identity::new(21, 22, 1))).result, "switched");
        // a receipt offered while the gate is still shut is refused by name
        let early = receipt(1_020, "cd-C-M-1", &business_receipt(1_015));
        assert_eq!(early.reason, "receipt-before-pre-hit");
        // the tee observes the pre HIT, the shell reports it
        control_hit("cd-C-M-1-pre", 1_030);
        let hit = pre(1_040, "cd-C-M-1", "hit", "ok");
        assert_eq!(hit.result, "hit");
        assert!(hit.line.contains("|phase=pre|marker=cd-C-M-1-pre|result=hit|reason=gate-open|"),
            "{}", hit.line);
        assert!(released());
        let step = poll(1_050);
        assert_eq!(step.step, "recover");
        assert_eq!(step.reason, "pre-hit-observed");
        // the switch baseline (fd 21/22, generation 1) can never be the receipt
        let stale = Receipt { identity: Identity::new(21, 22, 1), ..business_receipt(1_100) };
        assert_eq!(receipt(1_060, "cd-C-M-1", &stale).reason, "generation-unchanged");
        // …and a DIFFERENT fd number alone is not a new identity: the protect
        // generation is the same one the switch measured
        let other_number = Receipt { identity: Identity::new(99, 100, 1), ..business_receipt(1_100) };
        assert_eq!(receipt(1_070, "cd-C-M-1", &other_number).reason, "generation-unchanged");
        // the same descriptor re-protected (new generation, same number)
        let same_descriptor = Receipt { identity: Identity::new(21, 22, 2), ..business_receipt(1_100) };
        assert_eq!(receipt(1_080, "cd-C-M-1", &same_descriptor).reason, "identity-unchanged");
        let wrong_net = Receipt { net_id: 77, ..business_receipt(1_100) };
        assert_eq!(receipt(1_090, "cd-C-M-1", &wrong_net).reason, "net-id-mismatch");
        let unmeasured_net = Receipt { net_id: 0, ..business_receipt(1_100) };
        assert_eq!(receipt(1_100, "cd-C-M-1", &unmeasured_net).reason, "net-id-unmeasured");
        let connect_only = Receipt { subsequent_io: false, ..business_receipt(1_100) };
        assert_eq!(receipt(1_110, "cd-C-M-1", &connect_only).reason, "connect-only");
        let unprotected = Receipt { protect_ok: false, ..business_receipt(1_100) };
        assert_eq!(receipt(1_120, "cd-C-M-1", &unprotected).reason, "protect-failed");
        let early_io = Receipt { io_mono_ms: 1_030, ..business_receipt(1_030) };
        assert_eq!(receipt(1_130, "cd-C-M-1", &early_io).reason, "io-before-pre-hit");
        let unmeasured_io = Receipt { io_mono_ms: 0, ..business_receipt(0) };
        assert_eq!(receipt(1_140, "cd-C-M-1", &unmeasured_io).reason, "io-time-unmeasured");
        assert_eq!(active_phase(), "released", "every refusal is non-terminal");
        // a valid new business receipt
        let good = receipt(1_200, "cd-C-M-1", &business_receipt(1_150));
        assert_eq!(good.result, "receipt");
        assert!(good.line.contains("|phase=receipt|marker=none|result=receipt|reason=ok|"),
            "{}", good.line);
        assert!(good.line.contains("|net_id=31|protect_generation=2|identity=41:42:2|"), "{}",
            good.line);
        assert!(good.line.contains("|event_mono_ms=1150|"), "{}", good.line);
        assert!(good.line.contains("|local=10.77.0.5:51000|request_id=42|"), "{}", good.line);
        let offer = poll(1_300);
        assert_eq!(offer.step, "receipt");
        assert_eq!(offer.marker, "cd-C-M-1-post");
        assert_eq!(poll(1_400).step, "wait", "the post is offered once");
        // the post HIT comes from the tee, after the receipt
        control_hit("cd-C-M-1-post", 1_450);
        let post = post(1_500, "cd-C-M-1", "observed", "ok");
        assert_eq!(post.result, "observed");
        assert!(post.line.contains("|phase=post|marker=cd-C-M-1-post|result=observed|reason=control-hit-after-receipt|"),
            "{}", post.line);
        assert_eq!(active_id(), "");
        assert_eq!(plan_count(), 1);
        reset_all();
    }

    #[test]
    fn a_wb_plan_still_owes_both_controls_and_the_new_generation_protect_settle() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        assert_eq!(planned("Wb", "cd-C-Wb-1", 1_000).result, "planned");
        assert_eq!(switch_identified(1_010, "cd-C-Wb-1", "", "cellular", "10.77.0.5:0", 31,
            Some(Identity::new(51, 52, 1))).result, "switched");
        // Wb DOES send a pre control now (the switch no longer exempts it)
        assert_eq!(poll(1_020).step, "pre");
        control_hit("cd-C-Wb-1-pre", 1_030);
        assert_eq!(pre(1_040, "cd-C-Wb-1", "hit", "ok").result, "hit");
        assert_eq!(poll(1_050).step, "recover");
        // without the new generation's protect settle nothing completes
        let no_settle = Receipt { identity: Identity::new(61, 62, 2), net_id: 31, protect_ok: true,
            io_mono_ms: 1_060, subsequent_io: false, protect_settle: false,
            detail: "protect_settle", remote: "unknown", local: "192.168.50.199:0",
            request_id: "wb" };
        assert_eq!(receipt(1_070, "cd-C-Wb-1", &no_settle).reason, "wb-protect-settle-missing");
        assert_eq!(active_phase(), "released");
        // the substitute evidence: `phase=protect_settle|status=success`
        let settled = Receipt { protect_settle: true, ..no_settle };
        let good = receipt(1_080, "cd-C-Wb-1", &settled);
        assert_eq!(good.result, "exempted");
        assert!(good.line.contains("|result=exempted|reason=wb-protect-settle-success|"),
            "{}", good.line);
        // …and the post control is still owed
        let offer = poll(1_090);
        assert_eq!(offer.step, "receipt");
        assert_eq!(offer.marker, "cd-C-Wb-1-post");
        control_hit("cd-C-Wb-1-post", 1_100);
        assert_eq!(post(1_110, "cd-C-Wb-1", "observed", "ok").result, "observed");
        assert_eq!(active_id(), "");
        reset_all();
    }

    /// Wb×C/D (approved R2(a)): the substitute ordering is STRICT —
    /// `pre_hit_ms < protect_mono_ms < post_hit_ms`. The post HIT is judged
    /// against the new generation's OWN protect settle (`receipt_io_ms`, the
    /// core `protect_mono_ms`) and never the receipt's later callback clock
    /// (`receipt_ms`); equality proves no strict order, so a post HIT in the
    /// SAME millisecond as the protect settle is `UNSAT`.
    #[test]
    fn a_wb_post_hit_must_be_strictly_later_than_the_new_generations_protect_settle() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);

        // (1) PRE side stays strict: a protect settle at, or before, the pre HIT
        // is refused `io-before-pre-hit`, and a LATE receipt callback (the late
        // adopt) cannot back-date it into "protect after pre".
        assert_eq!(planned("Wb", "cd-C-Wb-1", 1_000).result, "planned");
        assert_eq!(switch_identified(1_010, "cd-C-Wb-1", "", "cellular", "10.77.0.5:0", 31,
            Some(Identity::new(51, 52, 1))).result, "switched");
        control_hit("cd-C-Wb-1-pre", 1_020);
        assert_eq!(pre(1_030, "cd-C-Wb-1", "hit", "ok").result, "hit");
        // protect settle in the SAME millisecond as the pre HIT
        assert_eq!(receipt(1_900, "cd-C-Wb-1", &wb_settle(1_020)).reason, "io-before-pre-hit");
        // protect settle strictly BEFORE the pre HIT, receipt callback long after
        assert_eq!(receipt(1_900, "cd-C-Wb-1", &wb_settle(1_019)).reason, "io-before-pre-hit");
        assert_eq!(active_phase(), "released", "the refusals are non-terminal");

        // (2) The accepted substitute receipt: protect settle strictly after the
        // pre HIT. Its callback clock equals the protect settle, exactly as the
        // shipped `Wb` takeover feeds it.
        assert_eq!(receipt(1_100, "cd-C-Wb-1", &wb_settle(1_100)).result, "exempted");
        assert_eq!(active_phase(), "receipt-issued");

        // (3) A post HIT in the SAME millisecond as the protect settle: the
        // callback rule alone (`post_hit_ms >= receipt_ms`, here equal) PASSES,
        // so only the new strict `Wb` boundary can refuse it — `UNSAT`, terminal.
        control_hit("cd-C-Wb-1-post", 1_100);
        let same_ms = post(1_100, "cd-C-Wb-1", "observed", "ok");
        assert_eq!(same_ms.result, "unsat", "{}", same_ms.line);
        assert_eq!(same_ms.reason, "post-before-receipt", "{}", same_ms.line);
        assert!(same_ms.line.contains(
            "|phase=post|marker=cd-C-Wb-1-post|result=unsat|reason=post-before-receipt|"),
            "{}", same_ms.line);
        assert_eq!(active_id(), "", "the post verdict is terminal");

        // (4) A post HIT EARLIER than the protect settle is refused too. The
        // receipt callback is back-dated below the protect settle so that the
        // `post_hit_ms >= receipt_ms` callback rule alone would pass and only the
        // strict protect comparison can refuse it.
        assert_eq!(planned("Wb", "cd-C-Wb-2", 2_000).result, "planned");
        assert_eq!(switch_identified(2_010, "cd-C-Wb-2", "", "cellular", "10.77.0.5:0", 31,
            Some(Identity::new(51, 52, 1))).result, "switched");
        control_hit("cd-C-Wb-2-pre", 2_020);
        assert_eq!(pre(2_030, "cd-C-Wb-2", "hit", "ok").result, "hit");
        assert_eq!(receipt(2_050, "cd-C-Wb-2", &wb_settle(2_100)).result, "exempted");
        control_hit("cd-C-Wb-2-post", 2_060);
        let early = post(2_060, "cd-C-Wb-2", "observed", "ok");
        assert_eq!(early.result, "unsat", "{}", early.line);
        assert_eq!(early.reason, "post-before-receipt", "{}", early.line);
        assert_eq!(active_id(), "");

        // (5) `pre < protect < post` strictly closes the sandwich `observed`.
        assert_eq!(planned("Wb", "cd-C-Wb-3", 3_000).result, "planned");
        assert_eq!(switch_identified(3_010, "cd-C-Wb-3", "", "cellular", "10.77.0.5:0", 31,
            Some(Identity::new(51, 52, 1))).result, "switched");
        control_hit("cd-C-Wb-3-pre", 3_020);
        assert_eq!(pre(3_030, "cd-C-Wb-3", "hit", "ok").result, "hit");
        assert_eq!(receipt(3_100, "cd-C-Wb-3", &wb_settle(3_100)).result, "exempted");
        control_hit("cd-C-Wb-3-post", 3_101);
        let good = post(3_110, "cd-C-Wb-3", "observed", "ok");
        assert_eq!(good.result, "observed", "{}", good.line);
        assert!(good.line.contains(
            "|phase=post|marker=cd-C-Wb-3-post|result=observed|reason=control-hit-after-receipt|"),
            "{}", good.line);

        // (6) The binding comparison is the PROTECT settle, not the callback
        // clock: a post HIT in the SAME millisecond as the LATER receipt callback
        // is accepted, because it is still strictly after the protect settle.
        // (A `post_hit_ms <= receipt_ms` comparison would wrongly refuse it.)
        assert_eq!(planned("Wb", "cd-C-Wb-4", 4_000).result, "planned");
        assert_eq!(switch_identified(4_010, "cd-C-Wb-4", "", "cellular", "10.77.0.5:0", 31,
            Some(Identity::new(51, 52, 1))).result, "switched");
        control_hit("cd-C-Wb-4-pre", 4_020);
        assert_eq!(pre(4_030, "cd-C-Wb-4", "hit", "ok").result, "hit");
        assert_eq!(receipt(4_500, "cd-C-Wb-4", &wb_settle(4_100)).result, "exempted");
        control_hit("cd-C-Wb-4-post", 4_500);
        let after_protect = post(4_510, "cd-C-Wb-4", "observed", "ok");
        assert_eq!(after_protect.result, "observed", "{}", after_protect.line);

        reset_all();
    }

    #[test]
    fn the_bounds_are_named_and_a_bounded_plan_never_lingers() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        // D: the preparation never arrives
        assert_eq!(plan(1_000, "cd-D-M-1", "D", "M", 1, "119.29.29.119:443", "wifi",
            "192.168.50.199:0", "").result, "planned");
        assert_eq!(poll(1_000 + PREPARE_WAIT_MS - 1).step, "wait");
        let expired = poll(1_000 + PREPARE_WAIT_MS);
        assert_eq!(expired.step, "closed");
        assert_eq!(expired.reason, "prepare-wait-exceeded");
        // C: the pre is never reported after the switch
        assert_eq!(planned("M", "cd-C-M-1", 5_000).result, "planned");
        assert_eq!(switch_identified(5_010, "cd-C-M-1", "", "cellular", "10.77.0.5:0", 31,
            Some(Identity::new(21, 22, 1))).result, "switched");
        assert_eq!(poll(5_010 + PRE_WAIT_MS - 1).step, "pre");
        assert_eq!(poll(5_010 + PRE_WAIT_MS).reason, "pre-wait-exceeded");
        // C: the receipt never arrives after the release
        assert_eq!(planned("I", "cd-C-I-1", 9_000).result, "planned");
        assert_eq!(switch_identified(9_010, "cd-C-I-1", "", "cellular", "10.77.0.5:0", 31,
            Some(Identity::new(21, 22, 1))).result, "switched");
        control_hit("cd-C-I-1-pre", 9_020);
        assert_eq!(pre(9_030, "cd-C-I-1", "hit", "ok").result, "hit");
        let after = poll(9_030 + RECEIPT_WAIT_MS);
        assert_eq!(after.step, "closed");
        assert_eq!(after.reason, "receipt-wait-exceeded");
        assert!(after.line.contains("|phase=receipt|marker=none|result=unsat|reason=receipt-wait-exceeded|"),
            "{}", after.line);
        assert_eq!(active_id(), "");
        reset_all();
    }

    #[test]
    fn the_control_markers_are_reported_per_phase_and_capped_by_the_marker_rule() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        // 59 chars is the longest id whose -pre/-post markers stay ≤ 64
        let id = "a".repeat(MAX_ID);
        assert_eq!(plan(1_000, &id, "C", "M", 1, "119.29.29.119:443", "wifi", "unknown", "")
            .result, "planned");
        assert_eq!(plan(1_000, &"a".repeat(MAX_ID + 1), "C", "M", 1, "119.29.29.119:443",
            "wifi", "unknown", "").reason, "invalid-id");
        assert_eq!(proto_of("M"), "tcp");
        assert_eq!(proto_of("I"), "udp");
        reset_all();
    }
}
