/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// 2026-10-10 后台保活（0.0.19 实验版，方案 A）—— PURE LOGIC only.
//
// WHAT THIS FILE IS: the WHOLE decision of "should this app hold a DATA_TRANSFER
// continuous task right now, and what happens when the platform takes it away?".
// The page (pages/Index.ets) only gathers FACTS — the status document it just
// read, its own latches, the settings switch, the platform cancellation events
// the adapter buffered — and acts; every gate and the whole state machine live
// here, so they are host-verifiable without a device. The host-side selfcheck
// imports this file BYTE-IDENTICAL (client/entry/selfcheck/run-keep-alive.mts),
// so this file must stay plain TypeScript: NO @ohos import, no decorator, no
// enum, no platform call, no clock read.
//
// WHY A CONTINUOUS TASK AT ALL: the system reclaims this app's VPN extension
// process first (its reclaim priority is the lowest tier); a UIAbility that
// holds a continuous task covers EVERY process of the app, which is the only
// keep-alive instrument a third-party app has on this platform. HarmonyOS has
// no VPN task type, so the task borrows DATA_TRANSFER — the same borrowing the
// other HarmonyOS VPN client makes. The type does not match the business, so
// the platform may cancel the task at any time and may penalise repeated
// violations: this module therefore gives the platform's verdicts the LAST
// word (a user dismissal or an illegal-use verdict is never argued with), and
// every failure is answered with a bounded backoff, never with a retry storm.
//
// WHY DATA_TRANSFER IS NOT FREE: the task must publish a live-view notification
// ("实况窗") as its progress update; a DATA_TRANSFER task whose progress is not
// updated for more than 10 minutes is cancelled by the system
// (SYSTEM_CANCEL_DATA_TRANSFER_NOT_UPDATE), and a low transfer rate is a
// cancellation reason of its own (SYSTEM_CANCEL_DATA_TRANSFER_LOW_SPEED). The
// adapter owns the platform call; the throttle that decides WHEN to publish is
// shouldRefresh() below (>= KEEP_ALIVE_REFRESH_MS).
//
// WHY THE VPN DYING WITHDRAWS THE TASK: a task must not be held for a business
// that is not running ("未执行业务" is a violation in its own right, and holding
// a task with no tunnel is exactly the "恶意保活" the platform forbids). So when
// no fresh status document reports an active tunnel, decide() returns STOP — the
// adapter releases the task and the UI process may be frozen again.
//
// 0.0.20 (GRACE): with the task held the UI process keeps running in the
// background and the automatic reconnect (NetBirdReconnectPolicy) is now allowed
// to start the VPN there — but a reconnect needs a little time, and dropping the
// task the instant the document goes stale would freeze the process before its
// first attempt. So the withdrawal is DELAYED by KEEP_ALIVE_GRACE_MS (2 min),
// measured from the moment the tunnel was last seen active. Inside that window
// the task is kept, the live view says "正在重新连接…", and the reconnect gets its
// chance; when the window is over — or the reconnect has given up — the task goes
// and the reason says which of the two it was. The user's own instruments
// (switch off, logout, a disconnect just asked for) still stop the task at once:
// the grace is for a tunnel that was taken away, never for one the user ended.
//
// 0.0.28 (INTENT, NOT A CLOCK): 2 minutes was the wrong bound for a state that
// lasts hours. On the PC the connector went `reconnecting` at 11:32:08, the
// machine then slept (so the extension published nothing and the document aged
// out), and the first tick after the window closed withdrew the borrowed task as
// `ui_bg_task stage=stop reason=vpn-inactive` at 11:48:34 — 8 h 10 m before the
// network came back (v027-diag-pc §A.3). With the task gone the UI process may be
// frozen, so a kill of the extension in that window would have been unrecoverable
// until someone brought the app to the foreground — the same failure class
// 253dca5 fixed for the restart leg. The bound is therefore the USER'S INTENT, not
// a clock: while `intentOn` (see KeepAliveFacts) is true the task is kept however
// long the connector takes, exactly like upstream Android's foreground service —
// android-client@147bb4f VPNService keeps it up for as long as the ENGINE runs,
// and only the engine stopping (user stop / needs-login) reaches
// ServiceStateListener.onStopped → stopForeground(); a DISCONNECTED report in
// between only repaints the notification. The 2-minute grace stays as the
// fallback for a task held with no recorded intent, and a reconnect that gave up
// still ends it at once — that is the app's own "no reconnect is intended".
//
// WHY SUPPRESSION IS SESSION-SCOPED: if the user cancels the task (or removes
// its notification — removing the notification stops the task by platform
// rule), that is an explicit instruction about THIS connection: no automatic
// re-request until the user asks for a connection again (onUserConnect clears
// it). A system cancellation is different — it is a verdict about the request
// itself, so it earns a bounded retry; five of them end the session's budget.
// USE_ILLEGALLY ("invalid type used") is terminal for the session: the platform
// has said this app may not use this type, and asking again on the next manual
// connect would be pure violation, so only a logout/process restart clears it.
//
// REASONS (the closed set a decision can carry; the page never re-invents one):
//   vpn-active          the action was taken (the tunnel is up and no task is held)
//   disabled            the settings switch is off
//   logout              an explicit logout is in flight / just resolved (or the
//                       app has no credentials at all)
//   user-stop           the page just asked for a disconnect (a REAL one: the
//                       page's own stop-then-start re-entry leg is not this — it
//                       arms the page's restart-leg latch instead, 0.0.28)
//   vpn-inactive        no fresh status document reports an active tunnel, and the
//                       reconnect grace is over (or never started)
//   grace               the tunnel is inactive but the task is KEPT and nothing is
//                       asked for (0.0.20): inside the 2-minute window, or — with
//                       the user's intent on — for as long as the user wants this
//                       VPN, however long the connector takes (0.0.28)
//   reconnect-gave-up   the automatic reconnect has spent its attempt budget: the
//                       tunnel is not coming back by itself, so the task goes
//                       (0.0.20)
//   session-stopped     the platform answered USE_ILLEGALLY earlier in this session
//   suppressed          the user dismissed this connection's task/notification
//   task-present        the task is already held
//   vpn-starting        the page's own VPN start has not produced a live tunnel yet
//   start-in-flight     the adapter is still creating the task
//   attempt-limit       KEEP_ALIVE_MAX_ATTEMPTS platform verdicts in this session
//   backoff             < the backoff the last verdict earned

export const KEEP_ALIVE_REFRESH_MS: number = 30000;
// 30 s x 1, 2, 4, 8 ... capped at 5 min. Five verdicts end the session's budget
// (30 s / 60 s / 120 s / 240 s are the ones that can actually be served).
export const KEEP_ALIVE_RETRY_BASE_MS: number = 30000;
export const KEEP_ALIVE_RETRY_MAX_MS: number = 300000;
export const KEEP_ALIVE_MAX_ATTEMPTS: number = 5;
// 0.0.20: how long the borrowed task is kept after the tunnel went inactive, so
// the background automatic reconnect gets its chance before the process can be
// frozen again. Measured from the moment the tunnel was last seen active; the
// user's own instruments (switch off / logout / just asked for a disconnect)
// bypass it entirely.
export const KEEP_ALIVE_GRACE_MS: number = 120000;

// backgroundTaskManager.ContinuousTaskCancelReason / ...DetailedCancelReason,
// copied as literals on purpose (this module imports nothing). Values verified
// against the local API 26 SDK declarations.
export const KEEP_ALIVE_CANCEL_USER: number = 1;
export const KEEP_ALIVE_CANCEL_SYSTEM: number = 2;
export const KEEP_ALIVE_CANCEL_USER_REMOVE_NOTIFICATION: number = 3;
export const KEEP_ALIVE_CANCEL_LOW_SPEED: number = 4;
export const KEEP_ALIVE_CANCEL_ILLEGAL: number = 11;
export const KEEP_ALIVE_CANCEL_NOT_UPDATE: number = 12;
// 0 = the platform delivered no detailed reason at all.
export const KEEP_ALIVE_NO_DETAIL: number = 0;

export const KEEP_ALIVE_ACTION_START: string = 'start';
export const KEEP_ALIVE_ACTION_STOP: string = 'stop';
export const KEEP_ALIVE_ACTION_NONE: string = 'none';

export const KEEP_ALIVE_REASON_VPN_ACTIVE: string = 'vpn-active';
export const KEEP_ALIVE_REASON_DISABLED: string = 'disabled';
export const KEEP_ALIVE_REASON_LOGOUT: string = 'logout';
export const KEEP_ALIVE_REASON_USER_STOP: string = 'user-stop';
// 0.0.28（重启腿不再被读成用户停止）：页面自己的两个锁存器（`stopSent` 与真正用户
// 停止的时刻）降到策略的这一个事实。纯函数、导出，所以 0.0.28 修掉的那个回归在主机
// 上可复现：页面的 stop-then-start 重入腿（stopVpnExtensionThenStart）曾经写的是
// "用户停止时刻"，于是自动重连后 1-3 s 的那一拍把重入腿读成"用户刚要求断开"，
// decideKeepAlive() 当场返回 STOP/user-stop，借来的长时任务被撤销——下一次进程被杀
// 就再也没有后台重连（v027b 真机：90.5 min 与 28.0 min 两次断网）。重入腿改为写页面
// 自己的 restart-leg 锁存器，只有 requestStop()（真正用户停止）才写下面这个时刻。
export const KEEP_ALIVE_USER_STOP_GUARD_MS: number = 8000;

export function keepAliveUserStopRecent(stopSent: boolean, lastUserStopAtMs: number,
  nowMs: number): boolean {
  if (stopSent) {
    return true;
  }
  return lastUserStopAtMs > 0 && nowMs - lastUserStopAtMs <= KEEP_ALIVE_USER_STOP_GUARD_MS;
}
export const KEEP_ALIVE_REASON_VPN_INACTIVE: string = 'vpn-inactive';
// 0.0.20: the reconnect still has time left — the task is KEPT and nothing is
// asked for.
export const KEEP_ALIVE_REASON_GRACE: string = 'grace';
// 0.0.20: the automatic reconnect spent its attempt budget, so the tunnel is not
// coming back by itself and the task must not outlive it.
export const KEEP_ALIVE_REASON_RECONNECT_GAVE_UP: string = 'reconnect-gave-up';
export const KEEP_ALIVE_REASON_SESSION_STOPPED: string = 'session-stopped';
export const KEEP_ALIVE_REASON_SUPPRESSED: string = 'suppressed';
export const KEEP_ALIVE_REASON_TASK_PRESENT: string = 'task-present';
export const KEEP_ALIVE_REASON_VPN_STARTING: string = 'vpn-starting';
export const KEEP_ALIVE_REASON_START_IN_FLIGHT: string = 'start-in-flight';
export const KEEP_ALIVE_REASON_ATTEMPT_LIMIT: string = 'attempt-limit';
export const KEEP_ALIVE_REASON_BACKOFF: string = 'backoff';

// The live-view notification's fixed title. The body is the tunnel's own
// traffic, never an invented progress percentage.
export const KEEP_ALIVE_NOTIFICATION_TITLE: string = 'NetBird VPN 已连接';
// 0.0.20: the body while the app is inside the reconnect grace. The tunnel has no
// document to report, so the live view says what the app is DOING instead of
// freezing the last traffic snapshot (a frozen progress for minutes would also
// invite SYSTEM_CANCEL_DATA_TRANSFER_NOT_UPDATE).
export const KEEP_ALIVE_NOTIFICATION_RECONNECTING: string = '正在重新连接…';

// 0.0.21（挂起长窗口后的恢复复查）：两拍之间隔了这么久，只可能意味着这个进程有一段
// 时间根本没在跑——平台可以挂起整个 UI 进程，0.0.20 的真机数据里这一次是 7362 s（期间
// 2 s 轮询与 30 s 实况窗刷新整段停摆，VPN 在 17:08 被相机清理后 1 h16 m 无人重连，长时
// 任务却始终没有被撤销）。所以"这一拍离上一拍异常远"本身就是一条事实：进程刚从一次
// 挂起里回来，页面必须马上重读一次隧道状态，而不是接着用挂起前的判断。15 s = 7.5 个
// 正常节拍：后台节流、GC 停顿都不会到这里。
export const KEEP_ALIVE_RESUME_GAP_MS: number = 15000;

// `ui_resume_check` 记录的 `reason` 闭集（0.0.21）：恢复是**哪种信号**发现的。
export const KEEP_ALIVE_RESUME_ACTIVE: string = 'active';
export const KEEP_ALIVE_RESUME_GAP: string = 'gap';
export const KEEP_ALIVE_RESUME_FOREGROUND: string = 'foreground';

// 0.0.21（挂起消息清洗）：平台的 `suspendMessage.message` 是自由文本，而 W 级 hilog
// 是随时可被导出 / 抓取的一面。这条纯函数是原文进入 hilog 之前的唯一通道，也是它
// 唯一被允许出现的地方：字符集降到可打印 ASCII、凭据形状的长串折成 `<tok>`、
// 结果有界。原文绝不进入任何 `ui_bg_task` 记录（记录里只有原始字符数）。
export const KEEP_ALIVE_SUSPEND_TEXT_MAX: number = 120;
// 16 个 token 字符起算是一段凭据形状的串：保留"这里曾有一串东西"这个事实，不保留它。
export const KEEP_ALIVE_TOKEN_MIN_CHARS: number = 16;

export function sanitizeSuspendMessage(raw: string | undefined): string {
  if (raw === undefined || raw.length === 0) {
    return '';
  }
  // 可打印 ASCII（0x20..0x7e）之外一律变成 '?'：控制字符、换行、非 ASCII 文本都
  // 不会原样落到日志行里，也不会把日志行拆成两行。
  let text: string = '';
  for (let i = 0; i < raw.length; i++) {
    const code: number = raw.charCodeAt(i);
    text = text + (code >= 0x20 && code <= 0x7e ? raw.charAt(i) : '?');
  }
  text = text.replace(/[A-Za-z0-9+\/=_-]{16,}/g, '<tok>');
  return text.length > KEEP_ALIVE_SUSPEND_TEXT_MAX
    ? text.substring(0, KEEP_ALIVE_SUSPEND_TEXT_MAX) : text;
}

// 上一拍到现在隔了多久（毫秒）。0 = 本进程还没拍过（冷启动的第一拍没有"恢复"可言），
// 时钟回跳也按 0 处理——宁可少做一次复查，也不用一个负数间隔误报一次恢复。
export function keepAlivePollGapMs(lastPollAtMs: number, nowMs: number): number {
  if (lastPollAtMs <= 0 || nowMs <= lastPollAtMs) {
    return 0;
  }
  return nowMs - lastPollAtMs;
}

// 这一拍是否落在"进程刚从一段时间不运行里回来"里。严格大于阈值：正好 15 s 不算。
export function keepAliveResumedAfterGap(gapMs: number): boolean {
  return gapMs > KEEP_ALIVE_RESUME_GAP_MS;
}

// The state of ONE UI process. `revision` is the session identity: it is bumped
// whenever the meaning of an in-flight task creation changes (a user stop, a
// logout, a fresh manual connect), so a start that the platform answers AFTER
// that change is discarded instead of being adopted into the new session.
export interface KeepAlivePolicyState {
  taskPresent: boolean;
  startInFlight: boolean;
  suppressedForSession: boolean;
  sessionStopped: boolean;
  cancelCount: number;
  nextRetryAtMs: number;
  lastRefreshMs: number;
  revision: number;
}

// Everything the decision is allowed to depend on. `nowMs` is passed in (never
// read inside) so the whole machine is testable at exact boundaries.
export interface KeepAliveFacts {
  nowMs: number;
  // The settings switch's current value (true = 后台保活 on).
  enabled: boolean;
  // A FRESH status document reports an active tunnel (connected/connecting/
  // reconnecting). A stale or absent document is false.
  vpnActive: boolean;
  // 0.0.28（保活跟随用户意图，而不是连接器的瞬时状态）: the user still wants this
  // VPN. True once a tunnel has been seen RUNNING in this UI process, and until
  // something ends that intent: a real user stop (requestStop), a logout, the
  // settings switch turned off, a terminal verdict ("the VPN never came up" — a
  // connect-failure notice, needs-login included) or a document that reports a
  // DEAD state (the engine itself said it stopped). It deliberately stays TRUE
  // across every TRANSIENT loss — connecting, reconnecting, netLost, a suspended
  // or killed extension — because that is exactly the window in which the
  // borrowed task has to survive (see the 0.0.28 note at the top of this file).
  // The page owns the latch; this module only consumes it as a fact.
  intentOn: boolean;
  // The page just asked for a disconnect: a stop request is in flight, or the
  // resolved one is younger than the page's 8 s user-stop guard.
  userStopRecent: boolean;
  // An explicit logout is in flight / just resolved (25 s guard), or the app
  // holds no credentials at all.
  loggedOut: boolean;
  // The page's own VPN start request is out and no active tunnel exists yet.
  startInFlight: boolean;
  // 0.0.20: when the tunnel was last seen going from ACTIVE to inactive (0 =
  // "never observed active in this process"). The reconnect grace is measured
  // from here, so the page's own tick cadence never stretches or shortens it.
  vpnInactiveSinceMs: number;
  // 0.0.20: the automatic reconnect has spent its attempt budget
  // (NetBirdReconnectPolicy: RECONNECT_MAX_ATTEMPTS consecutive failures). The
  // grace is pointless then — the tunnel is not coming back by itself.
  reconnectGaveUp: boolean;
}

export interface KeepAliveDecision {
  action: string;
  reason: string;
  // The number the ONE diagnostic record carries: the platform verdicts this
  // session has already paid for (0 = the first attempt).
  count: number;
  state: KeepAlivePolicyState;
}

// What one live-view update needs. Byte counters < 0 mean "not readable" and
// render as 未知 — a counter that could not be read is never shown as 0.
export interface KeepAliveStatus {
  onlinePeers: number;
  peerCount: number;
  txBytes: number;
  rxBytes: number;
}

// One platform cancellation the adapter buffered for the page (the adapter
// never owns a second copy of the policy state).
export interface KeepAliveCancelEvent {
  reason: number;
  detailedReason: number;
}

export function keepAliveInitialState(): KeepAlivePolicyState {
  return keepAliveState(false, false, false, false, 0, 0, 0, 0);
}

function keepAliveState(taskPresent: boolean, startInFlight: boolean,
  suppressedForSession: boolean, sessionStopped: boolean, cancelCount: number,
  nextRetryAtMs: number, lastRefreshMs: number,
  revision: number): KeepAlivePolicyState {
  const state: KeepAlivePolicyState = {
    taskPresent: taskPresent,
    startInFlight: startInFlight,
    suppressedForSession: suppressedForSession,
    sessionStopped: sessionStopped,
    cancelCount: cancelCount,
    nextRetryAtMs: nextRetryAtMs,
    lastRefreshMs: lastRefreshMs,
    revision: revision
  };
  return state;
}

// A field-by-field copy: every transition returns a NEW state, so a caller that
// kept the previous one can never observe a half-applied transition.
function keepAliveCopy(state: KeepAlivePolicyState): KeepAlivePolicyState {
  return keepAliveState(state.taskPresent, state.startInFlight,
    state.suppressedForSession, state.sessionStopped, state.cancelCount,
    state.nextRetryAtMs, state.lastRefreshMs, state.revision);
}

function keepAliveDecision(action: string, reason: string,
  state: KeepAlivePolicyState): KeepAliveDecision {
  const decision: KeepAliveDecision = {
    action: action,
    reason: reason,
    count: state.cancelCount,
    state: state
  };
  return decision;
}

// WITHDRAW the task. The decision is a request, not a promise: the adapter's
// stop() is fail-soft, and a task that is already gone makes it a no-op.
function keepAliveStop(state: KeepAlivePolicyState, reason: string): KeepAliveDecision {
  const next: KeepAlivePolicyState = keepAliveCopy(state);
  next.taskPresent = false;
  // A creation the platform has not answered yet belongs to the session that is
  // ending: bump the revision so its late answer is DISCARDED (the adapter
  // withdraws the task it created; see NetBirdKeepAliveService).
  if (next.startInFlight) {
    next.revision = state.revision + 1;
  }
  next.startInFlight = false;
  return keepAliveDecision(KEEP_ALIVE_ACTION_STOP, reason, next);
}

function keepAliveNone(state: KeepAlivePolicyState, reason: string): KeepAliveDecision {
  return keepAliveDecision(KEEP_ALIVE_ACTION_NONE, reason, state);
}

// 30 s, 60 s, 120 s, 240 s, then the 5 min cap. Kept exported (and pure) so the
// cap is checkable independently of how many verdicts a session may pay for.
export function keepAliveBackoffMs(cancelCount: number): number {
  if (cancelCount <= 0) {
    return 0;
  }
  const step: number = cancelCount - 1 > 4 ? 4 : cancelCount - 1;
  const raw: number = KEEP_ALIVE_RETRY_BASE_MS * Math.pow(2, step);
  return raw > KEEP_ALIVE_RETRY_MAX_MS ? KEEP_ALIVE_RETRY_MAX_MS : raw;
}

// THE decision. Pure: same state + same facts = same answer, no clock, no IO.
export function decideKeepAlive(state: KeepAlivePolicyState,
  facts: KeepAliveFacts): KeepAliveDecision {
  // (1) The user's own instruments first: the settings switch, an explicit
  //     logout, a disconnect the page just asked for. All three mean "this app
  //     must not be holding a task right now".
  if (!facts.enabled) {
    return keepAliveStop(state, KEEP_ALIVE_REASON_DISABLED);
  }
  if (facts.loggedOut) {
    return keepAliveStop(state, KEEP_ALIVE_REASON_LOGOUT);
  }
  if (facts.userStopRecent) {
    return keepAliveStop(state, KEEP_ALIVE_REASON_USER_STOP);
  }
  // (2) No live tunnel = no business = no task — but ONLY once the user's intent
  //     is over, or the reconnect has given up. Upstream keeps its foreground
  //     service for as long as the ENGINE runs, and a connector that is
  //     reconnecting (or whose document stopped being refreshed because the
  //     machine slept / the process was reclaimed) is still the same business.
  //     0.0.28: `intentOn` is that bound; the 0.0.20 grace remains as the
  //     fallback for a task held with no recorded intent. The user's own
  //     instruments were answered above and are never subject to either.
  if (!facts.vpnActive) {
    if (!facts.reconnectGaveUp && (facts.intentOn ||
      (facts.vpnInactiveSinceMs > 0 &&
        facts.nowMs - facts.vpnInactiveSinceMs < KEEP_ALIVE_GRACE_MS))) {
      return keepAliveNone(state, KEEP_ALIVE_REASON_GRACE);
    }
    return keepAliveStop(state, facts.reconnectGaveUp
      ? KEEP_ALIVE_REASON_RECONNECT_GAVE_UP : KEEP_ALIVE_REASON_VPN_INACTIVE);
  }
  // (3) The platform's verdicts, given the last word. USE_ILLEGALLY is terminal
  //     for the session (asking again would be a knowing violation); a user
  //     dismissal suppresses this connection only.
  if (state.sessionStopped) {
    return keepAliveNone(state, KEEP_ALIVE_REASON_SESSION_STOPPED);
  }
  if (state.suppressedForSession) {
    return keepAliveNone(state, KEEP_ALIVE_REASON_SUPPRESSED);
  }
  // (4) Already held / a tunnel that is still coming up / a task the adapter is
  //     still creating: asking again would stack requests.
  if (state.taskPresent) {
    return keepAliveNone(state, KEEP_ALIVE_REASON_TASK_PRESENT);
  }
  if (facts.startInFlight) {
    return keepAliveNone(state, KEEP_ALIVE_REASON_VPN_STARTING);
  }
  if (state.startInFlight) {
    return keepAliveNone(state, KEEP_ALIVE_REASON_START_IN_FLIGHT);
  }
  // (5) The bounded budget of this session.
  if (state.cancelCount >= KEEP_ALIVE_MAX_ATTEMPTS) {
    return keepAliveNone(state, KEEP_ALIVE_REASON_ATTEMPT_LIMIT);
  }
  if (facts.nowMs < state.nextRetryAtMs) {
    return keepAliveNone(state, KEEP_ALIVE_REASON_BACKOFF);
  }
  // (6) Go. The latch is set HERE, in the returned state, so the very next tick
  //     cannot ask for a second task while the platform is still answering.
  const next: KeepAlivePolicyState = keepAliveCopy(state);
  next.startInFlight = true;
  return keepAliveDecision(KEEP_ALIVE_ACTION_START, KEEP_ALIVE_REASON_VPN_ACTIVE, next);
}

// One platform cancellation, folded into the state. `detailedReason` is
// KEEP_ALIVE_NO_DETAIL when the platform delivered none; either field may carry
// the precise reason, so both are honoured.
export function keepAliveOnCancelled(state: KeepAlivePolicyState, reason: number,
  detailedReason: number, nowMs: number): KeepAlivePolicyState {
  const next: KeepAlivePolicyState = keepAliveCopy(state);
  next.taskPresent = false;
  next.startInFlight = false;
  const userCancelled: boolean = reason === KEEP_ALIVE_CANCEL_USER ||
    reason === KEEP_ALIVE_CANCEL_USER_REMOVE_NOTIFICATION ||
    detailedReason === KEEP_ALIVE_CANCEL_USER ||
    detailedReason === KEEP_ALIVE_CANCEL_USER_REMOVE_NOTIFICATION;
  const illegal: boolean = reason === KEEP_ALIVE_CANCEL_ILLEGAL ||
    detailedReason === KEEP_ALIVE_CANCEL_ILLEGAL;
  // The platform refused the TYPE itself: the strongest verdict there is, so it
  // outranks the user-dismissal reading of the same event.
  if (illegal) {
    next.sessionStopped = true;
    next.nextRetryAtMs = 0;
    return next;
  }
  if (userCancelled) {
    next.suppressedForSession = true;
    next.nextRetryAtMs = 0;
    return next;
  }
  next.cancelCount = state.cancelCount + 1;
  next.nextRetryAtMs = nowMs + keepAliveBackoffMs(next.cancelCount);
  return next;
}

// The adapter's answer for one start attempt. `revision` is the identity the
// attempt was made under: an answer that arrives after the session changed is
// DISCARDED (the adapter withdraws the task it created; the state stays what
// the newer session made it).
export function keepAliveOnStartResolved(state: KeepAlivePolicyState, revision: number,
  ok: boolean, nowMs: number): KeepAlivePolicyState {
  if (revision !== state.revision) {
    return state;
  }
  const next: KeepAlivePolicyState = keepAliveCopy(state);
  next.startInFlight = false;
  if (ok) {
    // The adapter publishes one live-view update immediately after the task is
    // created, so the refresh throttle starts now.
    next.taskPresent = true;
    next.lastRefreshMs = nowMs;
    next.nextRetryAtMs = 0;
    return next;
  }
  // A creation that failed is a verdict like a cancellation: it earns the same
  // bounded backoff (the 2 s tick must never become a request storm).
  next.taskPresent = false;
  next.cancelCount = state.cancelCount + 1;
  next.nextRetryAtMs = nowMs + keepAliveBackoffMs(next.cancelCount);
  return next;
}

// One live-view update was actually published: restart the 30 s throttle.
export function keepAliveOnRefreshed(state: KeepAlivePolicyState,
  nowMs: number): KeepAlivePolicyState {
  const next: KeepAlivePolicyState = keepAliveCopy(state);
  next.lastRefreshMs = nowMs;
  return next;
}

// The adapter is the owner of "a task exists right now"; the page mirrors it
// before every decision, so a task the adapter dropped on its own (a failed
// stop, a claim at start-up) can never leave the policy believing otherwise.
export function keepAliveWithTask(state: KeepAlivePolicyState,
  taskPresent: boolean): KeepAlivePolicyState {
  if (state.taskPresent === taskPresent) {
    return state;
  }
  const next: KeepAlivePolicyState = keepAliveCopy(state);
  next.taskPresent = taskPresent;
  return next;
}

export function shouldRefresh(state: KeepAlivePolicyState, nowMs: number): boolean {
  if (!state.taskPresent) {
    return false;
  }
  return state.lastRefreshMs <= 0 || nowMs - state.lastRefreshMs >= KEEP_ALIVE_REFRESH_MS;
}

// A manual connect asks for the tunnel again: this is the ONE thing that lifts
// the user's dismissal, and it starts the failure budget over. The illegal-type
// verdict is deliberately NOT lifted here — a manual connect is not a new app
// session, and no reconnect may turn into a re-violation.
export function keepAliveOnUserConnect(state: KeepAlivePolicyState): KeepAlivePolicyState {
  const next: KeepAlivePolicyState = keepAliveCopy(state);
  next.startInFlight = false;
  next.suppressedForSession = false;
  next.cancelCount = 0;
  next.nextRetryAtMs = 0;
  next.revision = state.revision + 1;
  return next;
}

// The connection this task belonged to is over: the task must go with it. The
// dismissal is moot afterwards (a new connect clears it anyway), and the next
// connection gets the full budget.
export function keepAliveOnUserStop(state: KeepAlivePolicyState): KeepAlivePolicyState {
  const next: KeepAlivePolicyState = keepAliveCopy(state);
  next.taskPresent = false;
  next.startInFlight = false;
  next.suppressedForSession = false;
  next.cancelCount = 0;
  next.nextRetryAtMs = 0;
  next.revision = state.revision + 1;
  return next;
}

// A logout ends the app session: every latch of it goes, including the
// illegal-type verdict (the next session is a new set of requests).
export function keepAliveOnLogout(state: KeepAlivePolicyState): KeepAlivePolicyState {
  const next: KeepAlivePolicyState = keepAliveCopy(state);
  next.taskPresent = false;
  next.startInFlight = false;
  next.suppressedForSession = false;
  next.sessionStopped = false;
  next.cancelCount = 0;
  next.nextRetryAtMs = 0;
  next.lastRefreshMs = 0;
  next.revision = state.revision + 1;
  return next;
}

// "在线 x/y · ↑tx ↓rx" — the live view's body, built from the SAME status
// document the page already read (no second read, no invented progress).
export function keepAliveNotificationText(status: KeepAliveStatus): string {
  const online: number = status.onlinePeers > 0 ? status.onlinePeers : 0;
  const total: number = status.peerCount > 0 ? status.peerCount : 0;
  return `在线 ${online}/${total} · ↑${keepAliveBytesText(status.txBytes)}` +
    ` ↓${keepAliveBytesText(status.rxBytes)}`;
}

// Binary units, one decimal. A negative/non-finite counter is 未知 — never 0.
export function keepAliveBytesText(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) {
    return '未知';
  }
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units: string[] = ['KB', 'MB', 'GB', 'TB', 'PB'];
  let value: number = bytes / 1024;
  let index: number = 0;
  while (value >= 1024 && index < units.length - 1) {
    value = value / 1024;
    index = index + 1;
  }
  return `${value.toFixed(1)} ${units[index]}`;
}
