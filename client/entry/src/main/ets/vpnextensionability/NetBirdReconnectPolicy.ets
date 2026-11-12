/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// 2026-10-08 扩展被系统回收后的自动重连 —— PURE LOGIC only.
//
// WHAT THIS FILE IS: the WHOLE decision of "may this page reconnect ONCE, by
// itself, after the system reclaimed the VPN extension?". The page
// (pages/Index.ets) only gathers FACTS — the status document it just read, the
// extension's lifecycle marker file, its own latches — and acts; every gate and
// the whole anti-loop state machine live here, so they are host-verifiable
// without a device. The host-side selfcheck imports this file BYTE-IDENTICAL
// (client/entry/selfcheck/run-reconnect-policy.mts), so this file must stay
// plain TypeScript: NO @ohos import, no decorator, no enum, no platform call.
//
// WHY THE MARKER, NOT THE STATUS STRING: the status document is published by the
// extension and simply stops being refreshed when that process dies, so its
// disappearance alone cannot tell "the system reclaimed it" from "the user
// stopped it". The diagnostics marker can: start() writes state=open, the
// onDestroy ENTRY synchronously writes state=stopping (0.0.17), and only the
// real stop callback writes state=clean — a user stop and a logout both tear the
// extension down through that callback, while a reclaim/crash never reaches it.
// A marker left at open therefore means exactly "the extension did not reach its
// stop callback" (force-stop, OOM kill and a lost stop callback look the
// same from here; see NetBirdDiagnosticsCore). Anything we cannot read —
// missing, corrupt, an unknown state token — is treated as clean: a reconnect is
// never guessed out of a file we did not understand.
//
// WHY THE MARKER ALONE IS STILL NOT ENOUGH: an open marker says the extension
// never reached its stop callback — it does NOT say the process is gone. On
// device the system suspends the background extension for up to ~94 s, during
// which it writes no status document (a document older than one publish tick is
// already read as null) and no log line, while the process stays alive and the
// marker stays open. Calling startVpnExtensionAbility again on a live extension
// is not a safe action, so criterion (7) requires the UI process's OWN kill
// record — APP_KILLED for this app's `:vpn` process, kept by
// NetBirdSystemEvents — to name the pid that wrote the marker.
//
// THRESHOLDS 8 s / 25 s are the page's EXISTING user-action guards (the same ones
// the 2026-10-05 disconnect notice uses), re-used verbatim rather than
// re-invented: a stop the user just asked for, and a logout whose credentials are
// already being deleted, must never be answered with a fresh VPN start.
//
// ANTI-LOOP, one UI process: at most RECONNECT_MAX_ATTEMPTS automatic reconnects,
// at least RECONNECT_MIN_INTERVAL_MS apart; an attempt still without an active
// document RECONNECT_VERIFY_MS later is a FAILURE — it stays counted and is never
// immediately retried (the 60 s interval still applies).
//
// 0.0.20 (BUDGET = CONSECUTIVE FAILURES). Until 0.0.19 an ACTIVE document
// disarmed the counter outright, which was sound while the UI process itself was
// frozen in the background: it could not outlive one reclaim by much. With the
// borrowed keep-alive task (NetBirdKeepAlivePolicy) the process now lives for
// hours, so "any active document" would forgive a tunnel that comes up and dies
// again every few seconds — forever. The counter is therefore disarmed only once
// an active run has LASTED RECONNECT_ACTIVE_RESET_MS (5 min): a reconnect that
// produced a healthy session is forgiven, a flapping one stays on the same chain
// of failures. See reconnectOnActive().
//
// 0.0.20 (BACKGROUND TRIGGER). `keepAliveActive` is the fact "a keep-alive task
// is held right now". With it the page's 2 s poll really does run in the
// background, so the foreground gate (2) is relaxed: the reconnect may be started
// there and the record carries trigger='background'. Every other gate — above all
// criterion (7), the UI process's OWN APP_KILLED for the pid that wrote the open
// marker — is unchanged, so the relaxation cannot start a VPN on a live-or-
// suspended extension.
//
// 0.0.20 (CRITERION (2) RELAXED). A kill event naming the marker's pid is proof
// the process is gone, so the caller no longer has to wait for the status document
// to age out (~10 s) before asking. The page treats a document covered by that
// proof as absent (it passes null); this module does not need to know — criterion
// (7) already demands exactly that proof, so asking earlier can only be answered
// "go" when the death really is confirmed.
//
// 0.0.20 (A START THE PLATFORM REFUSED). startVpnExtensionAbility may reject in
// the background. That is the NAMED failure of the attempt that was already
// counted: reconnectOnStartRejected() ends its verification window at once (no
// need to wait RECONNECT_VERIFY_MS for an answer that is already known), the
// attempt stays counted and the 60 s interval still gates the next one. The
// caller records result=failed with the platform's own code (or 'none').
//
// 0.0.59 (B1, NO KILL RECORD AT ALL). The 2026-11-04 device report (0.0.58,
// 19:28 `InvalidKillId` batch reclaim) showed the one death the UI process cannot
// confirm: the whole batch died while the UI received NO APP_KILLED for the
// `:vpn` process, so criterion (7) answered kill-record-absent forever and
// nothing recovered until the app was opened again (~4 h later; the borrowed
// keep-alive task had been suspended in the same window). The status document is
// then the only clock left. When it has not been live for
// RECONNECT_STALE_CONFIRM_MS (180 000 ms — comfortably past the ~94 s suspension
// upper bound measured on device), the extension is treated as DEAD and started
// DIRECTLY, without the stop-first leg: that leg exists to tear down a possibly
// live instance, and this path exists precisely because we cannot prove it is
// alive — a start the platform refuses costs far less than a stop of a live
// tunnel. The attempt is counted and verified by the SAME 30 s window, 60 s
// interval and 3-attempt budget as the confirmed-death path. Every other gate —
// clean/stopping/missing marker, the user guards, a newer connect-failure
// verdict — still runs FIRST, so a tunnel the user ended is never resurrected
// this way, and `stale-unconfirmed` is its own reason token: the log must tell
// the two deaths apart.
//
// 0.0.59 (B1, KILL RECORD THAT DOES NOT MATCH). The 2026-11-12 device report
// (0.0.58) still showed `ui_status_lost age_ms>=180000` up to 598.5 s while
// `stale-unconfirmed` was never taken: the UI process DID hold an APP_KILLED for
// a `:vpn` process, just not for the pid that wrote the marker, so criterion (7)
// answered vpn-pid-mismatch forever (marker-pid-invalid when the marker carried
// no usable pid). A kill record naming ANOTHER writer is one doubt more than no
// record at all — the marker's process might still be alive — so the same
// stale-document verdict needs a LONGER silence: RECONNECT_STALE_MISMATCH_MS
// (300 000 ms) instead of 180 000 ms, still far past the measured ~94 s
// suspension upper bound. Past it the extension is treated as dead and started
// DIRECTLY, exactly like stale-unconfirmed, and with the SAME one attempt budget
// (3 attempts / 60 s interval / 30 s verification); `stale-mismatch` is its own
// reason token so the log can tell "the evidence does not reconcile" apart from
// "there was no evidence at all".
//
// REASONS (the closed set a decision can carry; the page writes the token into
// one safe diagnostic record and never re-invents it):
//   marker-open         the action was taken, this attempt is attempt N
//   not-foreground      the page is not visible right now AND no keep-alive task
//                       is held (with one held, this reason is unreachable)
//   marker-clean        the extension reached its stop callback (user stop/logout)
//   marker-stopping     the extension entered onDestroy and wrote `stopping`
//                       synchronously, but the process ended before the final
//                       async `clean` transition (0.0.17) — same verdict as
//                       marker-clean, never a reconnect
//   marker-missing      no/corrupt/unknown marker — we cannot tell, so we do not
//   request-in-flight   a start or stop the page just issued is still in flight
//   recent-user-stop    the page stopped the tunnel moments ago
//   recent-logout       an explicit logout is in flight / just resolved
//   not-configured      no credentials to connect with
//   connect-failure     a connect-failure notice NEWER than the marker exists
//   cooldown            < RECONNECT_MIN_INTERVAL_MS since the last attempt
//   attempt-limit       RECONNECT_MAX_ATTEMPTS reached in this process
//   no-active-in-30s    the previous attempt is a failure (verification window)
//   kill-record-absent  no VPN APP_KILLED record in this UI process AND no live
//                       document long enough to prove the death (see
//                       stale-unconfirmed for what that length buys)
//   stale-unconfirmed   (0.0.59) no APP_KILLED at all, but no live document for
//                       RECONNECT_STALE_CONFIRM_MS: the extension is treated as
//                       dead and started DIRECTLY, never stop-first. Result=
//                       requested, counted and verified like any other attempt.
//   stale-mismatch      (0.0.59) a kill record EXISTS but does not name the marker
//                       writer (or the marker carries no usable pid), and no live
//                       document for RECONNECT_STALE_MISMATCH_MS: same DIRECT
//                       start and the SAME attempt budget as stale-unconfirmed.
//   marker-pid-invalid the marker cannot identify a usable writer pid (the
//                       conservative verdict until the document is stale enough)
//   vpn-pid-mismatch   the recorded death names another VPN writer (likewise)
// A pid we cannot tie to the marker is unknown death: never start over a possibly
// live extension — unless the document itself has been dead long enough to say
// that no live extension is left to start over (the stale-mismatch branch).

export const RECONNECT_MIN_INTERVAL_MS: number = 60000;
export const RECONNECT_MAX_ATTEMPTS: number = 3;
export const RECONNECT_VERIFY_MS: number = 30000;
// 0.0.20: how long an ACTIVE document must last before the attempt counter is
// forgiven. Anything shorter is the SAME chain of failures, not a success — the
// UI process now survives in the background for hours, so "any active document"
// would forgive a tunnel that dies again ten seconds later, forever.
export const RECONNECT_ACTIVE_RESET_MS: number = 300000;
export const RECONNECT_USER_STOP_GUARD_MS: number = 8000;
export const RECONNECT_LOGOUT_GUARD_MS: number = 25000;
// 0.0.59: how long the status document must have been NOT live before "no
// APP_KILLED record" stops meaning "cannot tell" and starts meaning "the process
// is gone". The device measured suspensions of up to ~94 s, so 180 s leaves wide
// margin: a frozen extension is never mistaken for a dead one, and anything
// older than that is not a pause — the extension publishes every 5 s.
export const RECONNECT_STALE_CONFIRM_MS: number = 180000;
// 0.0.59 (B1, MISMATCH): the same stale-document verdict, but with a kill record
// we could NOT reconcile (it names another `:vpn` writer, or the marker carries no
// usable pid). That is one more doubt than "no record at all", so the document has
// to stay dead twice as long. 300 s is still far past the measured ~94 s
// suspension upper bound plus the reconnect watchdog's margin.
export const RECONNECT_STALE_MISMATCH_MS: number = 300000;

// The extension marker's state tokens (NetBirdDiagnosticsCore MARKER_STATE_*).
// Copied as literals on purpose: this module is imported byte-identically into a
// host check, so it may not import anything at all.
export const RECONNECT_MARKER_OPEN: string = 'open';
export const RECONNECT_MARKER_CLEAN: string = 'clean';
// 2026-10-09 (0.0.17): the extension's onDestroy ENTRY writes `stopping`
// SYNCHRONOUSLY, because the asynchronous continuation that writes `clean` does
// not survive the process on the device (the JS context dies ~3 ms after
// onDestroy returns). For THIS decision the meaning is the same as `clean`: an
// ORDERLY stop was entered (user stop / logout), so the extension is never
// started again out of it. A reclaim/crash never reaches onDestroy at all and
// therefore still leaves `open`. Token equality with
// NetBirdDiagnosticsCore.MARKER_STATE_* is pinned by
// client/entry/selfcheck/run-reconnect-policy.mts.
export const RECONNECT_MARKER_STOPPING: string = 'stopping';

// What the decision says the page must do.
export const RECONNECT_ACTION_RECONNECT: string = 'reconnect';
export const RECONNECT_ACTION_SKIPPED: string = 'skipped';
export const RECONNECT_ACTION_LIMIT: string = 'limit';
export const RECONNECT_ACTION_VERIFY_FAILED: string = 'verify-failed';

// Closed result vocabulary: requested = dispatch, reconnected = verified active.
export const RECONNECT_RESULT_REQUESTED: string = 'requested';
export const RECONNECT_RESULT_RECONNECTED: string = 'reconnected';
export const RECONNECT_RESULT_SKIPPED: string = 'skipped';
export const RECONNECT_RESULT_LIMIT: string = 'limit';
export const RECONNECT_RESULT_FAILED: string = 'failed';

// The page poll normally runs while the page is in the foreground; this is the
// `trigger` token those records carry. 0.0.20 adds the background one: the poll
// kept running there because the borrowed keep-alive task is held, which is also
// the ONLY way past the foreground gate.
export const RECONNECT_TRIGGER_FOREGROUND: string = 'foreground';
export const RECONNECT_TRIGGER_BACKGROUND: string = 'background';

export const RECONNECT_REASON_MARKER_OPEN: string = 'marker-open';
export const RECONNECT_REASON_NOT_FOREGROUND: string = 'not-foreground';
export const RECONNECT_REASON_MARKER_CLEAN: string = 'marker-clean';
// 0.0.17: `stopping` — the onDestroy entry recorded an orderly stop whose final
// async `clean` transition never got to run. Same verdict as marker-clean.
export const RECONNECT_REASON_MARKER_STOPPING: string = 'marker-stopping';
export const RECONNECT_REASON_MARKER_MISSING: string = 'marker-missing';
export const RECONNECT_REASON_REQUEST_IN_FLIGHT: string = 'request-in-flight';
export const RECONNECT_REASON_RECENT_STOP: string = 'recent-user-stop';
export const RECONNECT_REASON_RECENT_LOGOUT: string = 'recent-logout';
export const RECONNECT_REASON_NOT_CONFIGURED: string = 'not-configured';
export const RECONNECT_REASON_CONNECT_FAILURE: string = 'connect-failure';
export const RECONNECT_REASON_COOLDOWN: string = 'cooldown';
export const RECONNECT_REASON_ATTEMPT_LIMIT: string = 'attempt-limit';
export const RECONNECT_REASON_VERIFY_TIMEOUT: string = 'no-active-in-30s';
export const RECONNECT_REASON_KILL_RECORD_ABSENT: string = 'kill-record-absent';
export const RECONNECT_REASON_MARKER_PID_INVALID: string = 'marker-pid-invalid';
export const RECONNECT_REASON_VPN_PID_MISMATCH: string = 'vpn-pid-mismatch';
// 0.0.59: the reconnect was decided from the stale-document evidence ALONE (no
// APP_KILLED record exists in this UI process). Its own token because the page
// starts this one DIRECTLY (no stop-first leg) and the log has to be able to tell
// the two deaths apart.
export const RECONNECT_REASON_STALE_UNCONFIRMED: string = 'stale-unconfirmed';
// 0.0.59 (B1, MISMATCH): the stale document decided it while a kill record does
// NOT name the marker writer (mismatch, or no usable marker pid). The action is
// stale-unconfirmed's exactly — started DIRECTLY, counted and verified by the SAME
// budget — but the token is its own: the log must tell "the evidence does not
// reconcile" apart from "there was no evidence at all".
export const RECONNECT_REASON_STALE_MISMATCH: string = 'stale-mismatch';

// The anti-loop counters of ONE UI process.
export interface ReconnectPolicyState {
  attempts: number;
  lastAttemptAtMs: number;
  verifyingSinceMs: number;
  // 0.0.20: when the CURRENT active document was first seen (0 = no active
  // document right now). The attempt counter is forgiven only once this run has
  // lasted RECONNECT_ACTIVE_RESET_MS. Kept across a reconnect decision, because a
  // reconnect is what ENDS the previous run: the next one gets its own clock.
  activeSinceMs: number;
  // Identity of the outstanding automatic attempt only; cleared on settlement.
  attemptDispatchedAtMs: number;
  attemptMarkerPid: number;
}

// Everything the decision is allowed to depend on. `nowMs` is passed in (never
// read inside) so the whole machine is testable at exact boundaries.
export interface ReconnectInput {
  nowMs: number;
  foreground: boolean;
  // 0.0.20: a keep-alive continuous task is held right now (the borrowed
  // DATA_TRANSFER task of NetBirdKeepAlivePolicy, mirrored into the page's policy
  // state). It is why the 2 s poll keeps running in the background; with it the
  // foreground gate is relaxed and the record's trigger becomes 'background'.
  // Every other gate still applies, criterion (7) above all.
  keepAliveActive: boolean;
  // '' when the marker is absent, empty, corrupt or carries an unknown state.
  markerState: string;
  // The marker's own timestamp in ms; 0 when it is absent/unparseable.
  markerAtMs: number;
  // The marker's own pid — the process that wrote it; 0 when the marker is
  // absent/unreadable or its pid is unusable. Criterion (7) compares it with
  // killedVpnPid.
  markerPid: number;
  startOrStopInFlight: boolean;
  lastUserStopAtMs: number;
  lastLogoutAtMs: number;
  configured: boolean;
  // at_ms of the connect-failure notice that matches the current attempt; 0 when
  // there is none.
  connectFailureAtMs: number;
  // The pid of THIS app's `:vpn` process whose APP_KILLED the UI process itself
  // received (NetBirdSystemEvents.lastKilledVpnProcess), or null when no such
  // event arrived in this process. Passed in as a plain fact: this module never
  // talks to the platform.
  killedVpnPid: number | null;
  // 0.0.59: how long ago the last LIVE status document was seen; 0 = no such
  // evidence in this process (a live document was never observed here). The stale
  // branches use it to treat a long-unupdated extension as dead — the branch with
  // NO APP_KILLED at all, and the one whose kill record cannot be tied to the
  // marker's writer (the longer threshold).
  docStaleMs: number;
}

export interface ReconnectDecision {
  action: string;
  reason: string;
  // Where the tick that produced this decision ran: 'foreground' when the page
  // was visible, 'background' otherwise (which is only ever reachable with
  // keepAliveActive). A faithful description of the moment, carried into the
  // record so a log reader can tell the two apart.
  trigger: string;
  // The attempt ordinal this decision is about (the page's `count` field).
  attempt: number;
  state: ReconnectPolicyState;
}

export function reconnectInitialState(): ReconnectPolicyState {
  return reconnectState(0, 0, 0, 0);
}

// An ACTIVE status document was seen for at least RECONNECT_ACTIVE_RESET_MS: the
// tunnel really is up and stays up, so the next reclaim gets the full budget
// again. The last attempt's timestamp is KEPT on purpose — two automatic
// reconnects in the same process are never closer than RECONNECT_MIN_INTERVAL_MS,
// however many successful connections happen between them.
function reconnectForgive(state: ReconnectPolicyState,
  activeSinceMs: number): ReconnectPolicyState {
  return reconnectState(0, state.lastAttemptAtMs, 0, activeSinceMs);
}

// 0.0.20: one tick saw an ACTIVE document. Two things happen and they are
// different: the outstanding attempt — if any — has SUCCEEDED (its verification
// window closes now), while the attempt counter is only forgiven once this run
// has lasted RECONNECT_ACTIVE_RESET_MS. A tunnel that comes up and dies again in
// between still belongs to the same chain of failures.
export function reconnectOnActive(state: ReconnectPolicyState,
  nowMs: number): ReconnectPolicyState {
  const activeSinceMs: number = state.activeSinceMs > 0 ? state.activeSinceMs : nowMs;
  if (nowMs - activeSinceMs >= RECONNECT_ACTIVE_RESET_MS) {
    return state.attempts === 0 && state.verifyingSinceMs === 0
      ? state : reconnectForgive(state, activeSinceMs);
  }
  if (state.activeSinceMs === activeSinceMs && state.verifyingSinceMs === 0) {
    return state;
  }
  return reconnectState(state.attempts, state.lastAttemptAtMs, 0, activeSinceMs);
}

// 0.0.20: the platform REFUSED the start itself (startVpnExtensionAbility
// rejected). The attempt was counted when the decision was made; a refusal is its
// named failure, so its verification window ends now instead of after
// RECONNECT_VERIFY_MS. The attempt stays counted (it is still one failure) and the
// 60 s interval is still what gates the next one.
export function reconnectOnStartRejected(
  state: ReconnectPolicyState): ReconnectPolicyState {
  if (state.verifyingSinceMs <= 0) {
    return state;
  }
  return reconnectState(state.attempts, state.lastAttemptAtMs, 0, state.activeSinceMs);
}

function reconnectState(attempts: number, lastAttemptAtMs: number,
  verifyingSinceMs: number, activeSinceMs: number, attemptDispatchedAtMs: number = 0,
  attemptMarkerPid: number = 0): ReconnectPolicyState {
  const state: ReconnectPolicyState = {
    attempts: attempts,
    lastAttemptAtMs: lastAttemptAtMs,
    verifyingSinceMs: verifyingSinceMs,
    activeSinceMs: activeSinceMs,
    attemptDispatchedAtMs: attemptDispatchedAtMs,
    attemptMarkerPid: attemptMarkerPid
  };
  return state;
}

// Latch the first marker written AFTER dispatch, never the reclaimed writer's
// marker. Once latched, a later marker cannot change this attempt's identity.
export function reconnectWithAttemptMarker(state: ReconnectPolicyState,
  markerAtMs: number, markerPid: number): ReconnectPolicyState {
  if (state.verifyingSinceMs <= 0 || state.attemptMarkerPid > 0 ||
    markerAtMs <= state.attemptDispatchedAtMs || !Number.isFinite(markerAtMs) ||
    !Number.isInteger(markerPid) || markerPid <= 0) {
    return state;
  }
  return reconnectState(state.attempts, state.lastAttemptAtMs, state.verifyingSinceMs,
    state.activeSinceMs, state.attemptDispatchedAtMs, markerPid);
}

// Only the outstanding automatic attempt is gated. Older documents omit pid;
// those fall back to the write time with a 1 s clock tolerance. A present pid
// never falls back to time, even while the new marker has not been observed yet.
export function reconnectStatusMatchesAttempt(state: ReconnectPolicyState,
  pid: number | undefined, updatedAtMs: number): boolean {
  if (state.verifyingSinceMs <= 0) {
    return true;
  }
  if (pid !== undefined) {
    return state.attemptMarkerPid > 0 && pid === state.attemptMarkerPid;
  }
  return Number.isFinite(updatedAtMs) &&
    updatedAtMs >= state.attemptDispatchedAtMs - 1000;
}

function reconnectDecision(action: string, reason: string, trigger: string,
  attempt: number, state: ReconnectPolicyState): ReconnectDecision {
  const decision: ReconnectDecision = {
    action: action,
    reason: reason,
    trigger: trigger,
    attempt: attempt,
    state: state
  };
  return decision;
}

function skipped(reason: string, trigger: string,
  state: ReconnectPolicyState): ReconnectDecision {
  return reconnectDecision(RECONNECT_ACTION_SKIPPED, reason, trigger, state.attempts, state);
}

// A stamp of 0 means "never happened" and never guards anything; a stamp in the
// future (a clock that moved) still guards, never unlocks.
function isRecent(nowMs: number, stampMs: number, guardMs: number): boolean {
  return stampMs > 0 && nowMs - stampMs <= guardMs;
}

// 0.0.59 (B1): the ONE action both stale branches take — "no APP_KILLED at all"
// and "a kill record that does not name the marker writer" are the same verdict
// once the status document has been dead long enough. The page starts the
// extension DIRECTLY: the stop-first leg exists to tear down a possibly LIVE
// instance, and the whole point of these branches is that we could NOT prove it is
// alive. The attempt is counted NOW and verified by the SAME one
// RECONNECT_VERIFY_MS window, RECONNECT_MIN_INTERVAL_MS interval and
// RECONNECT_MAX_ATTEMPTS budget as the confirmed-death path — the two stale reasons
// share one anti-loop budget, never a second counter.
function reconnectStale(reason: string, trigger: string, input: ReconnectInput,
  state: ReconnectPolicyState): ReconnectDecision {
  const attempt: number = state.attempts + 1;
  return reconnectDecision(RECONNECT_ACTION_RECONNECT, reason, trigger, attempt,
    reconnectState(attempt, input.nowMs, input.nowMs, 0, input.nowMs));
}

export function decideReconnect(state: ReconnectPolicyState,
  input: ReconnectInput): ReconnectDecision {
  // Where the tick that answered ran. A page that is not visible can only get
  // here with a keep-alive task held (gate 2 below), which is exactly the
  // background trigger.
  const trigger: string = input.foreground
    ? RECONNECT_TRIGGER_FOREGROUND : RECONNECT_TRIGGER_BACKGROUND;
  // (1) Settle the PREVIOUS attempt first: RECONNECT_VERIFY_MS without an active
  //     document is a failure of that attempt. It stays counted (the attempt was
  //     counted when it was made), and the 60 s interval below still has to pass
  //     before another one — a failure is never "retry harder".
  if (state.verifyingSinceMs > 0 &&
    input.nowMs - state.verifyingSinceMs >= RECONNECT_VERIFY_MS) {
    return reconnectDecision(RECONNECT_ACTION_VERIFY_FAILED,
      RECONNECT_REASON_VERIFY_TIMEOUT, trigger, state.attempts,
      reconnectState(state.attempts, state.lastAttemptAtMs, 0, state.activeSinceMs));
  }
  // (2) The trigger is the page poll, and the poll is a foreground timer; the
  //     explicit flag (onPageShow / onPageHide) turns a tick that still slipped
  //     through while the process was frozen into a no-op instead of a doomed
  //     background VPN start. 0.0.20: a HELD keep-alive task is the exception —
  //     it is why the process is still running and the tick still fires, so the
  //     gate opens for the background exactly as far as the task reaches. Every
  //     gate below, criterion (7) above all, still has to pass.
  if (!input.foreground && !input.keepAliveActive) {
    return skipped(RECONNECT_REASON_NOT_FOREGROUND, trigger, state);
  }
  // (3) The extension's own marker — the ONE fact that separates "the system
  //     reclaimed it" from "the user ended it".
  if (input.markerState === RECONNECT_MARKER_CLEAN) {
    return skipped(RECONNECT_REASON_MARKER_CLEAN, trigger, state);
  }
  // An orderly stop whose final `clean` transition was lost to the process end
  // (the extension's onDestroy entry wrote `stopping` synchronously): still an
  // end the user/logout initiated, never a reclaim.
  if (input.markerState === RECONNECT_MARKER_STOPPING) {
    return skipped(RECONNECT_REASON_MARKER_STOPPING, trigger, state);
  }
  if (input.markerState !== RECONNECT_MARKER_OPEN) {
    return skipped(RECONNECT_REASON_MARKER_MISSING, trigger, state);
  }
  // (4) No user action: a start/stop the page just issued is still in flight, or
  //     resolved moments ago.
  if (input.startOrStopInFlight) {
    return skipped(RECONNECT_REASON_REQUEST_IN_FLIGHT, trigger, state);
  }
  if (isRecent(input.nowMs, input.lastUserStopAtMs, RECONNECT_USER_STOP_GUARD_MS)) {
    return skipped(RECONNECT_REASON_RECENT_STOP, trigger, state);
  }
  if (isRecent(input.nowMs, input.lastLogoutAtMs, RECONNECT_LOGOUT_GUARD_MS)) {
    return skipped(RECONNECT_REASON_RECENT_LOGOUT, trigger, state);
  }
  // (5) There is something to reconnect with: credentials are configured, and no
  //     connect-failure notice NEWER than the marker already states a verdict the
  //     user has to act on (login expired, ...). A notice older than the marker
  //     belongs to the attempt that the reclaim ended.
  if (!input.configured) {
    return skipped(RECONNECT_REASON_NOT_CONFIGURED, trigger, state);
  }
  if (input.connectFailureAtMs > 0 && input.connectFailureAtMs > input.markerAtMs) {
    return skipped(RECONNECT_REASON_CONNECT_FAILURE, trigger, state);
  }
  // (6) The anti-loop itself.
  if (state.attempts >= RECONNECT_MAX_ATTEMPTS) {
    return reconnectDecision(RECONNECT_ACTION_LIMIT, RECONNECT_REASON_ATTEMPT_LIMIT,
      trigger, state.attempts, state);
  }
  if (state.lastAttemptAtMs > 0 &&
    input.nowMs - state.lastAttemptAtMs < RECONNECT_MIN_INTERVAL_MS) {
    return skipped(RECONNECT_REASON_COOLDOWN, trigger, state);
  }
  // (7) The process really is dead — the necessary condition the null document
  //     only LOOKS like. The status document stops being refreshed when the
  //     system freezes the background extension, so "no fresh document" cannot
  //     distinguish a reclaimed process from a suspended one; the UI process's
  //     own APP_KILLED for the `:vpn` process is the fact that can. It must name
  //     the pid the open marker came from: no event, another process's exit, or
  //     an unusable marker pid all mean "cannot confirm", and an extension
  //     start on top of a possibly live process is never guessed. This is also
  //     what makes the 0.0.20 backgrounds trigger and the relaxed criterion (2)
  //     safe: the caller may ask earlier, the answer is still "go" only here.
  //     0.0.59: the ONE second way to reach "dead" is the stale document itself
  //     (see the branches below) — it never weakens the kill-record path, it only
  //     answers the cases where that record will never arrive or cannot be tied to
  //     the marker's writer.
  if (input.killedVpnPid === null) {
    // 0.0.59 (B1): no APP_KILLED for the `:vpn` process at all. That alone only
    // says "cannot confirm". If the status document has ALSO been dead for
    // RECONNECT_STALE_CONFIRM_MS, it is no longer a suspension (the device's
    // measured upper bound is ~94 s) but a process that will never publish again
    // — the 2026-11-04 batch reclaim left exactly this shape behind. Start it
    // DIRECTLY: no stop-first leg, because that leg is for a possibly LIVE
    // instance and here the whole point is that we cannot prove it is alive.
    // The attempt is counted now and verified by the same window as any other.
    if (input.docStaleMs < RECONNECT_STALE_CONFIRM_MS) {
      return skipped(RECONNECT_REASON_KILL_RECORD_ABSENT, trigger, state);
    }
    return reconnectStale(RECONNECT_REASON_STALE_UNCONFIRMED, trigger, input, state);
  }
  if (input.markerPid <= 0) {
    // 0.0.59 (B1, MISMATCH): a kill record exists, we simply cannot tie it to the
    // marker's writer. One doubt more than "no record at all", so the document must
    // have been dead for the LONGER RECONNECT_STALE_MISMATCH_MS before the same
    // stale verdict applies — with the same DIRECT start and the same attempt
    // budget (reconnectStale), never a second counter.
    if (input.docStaleMs >= RECONNECT_STALE_MISMATCH_MS) {
      return reconnectStale(RECONNECT_REASON_STALE_MISMATCH, trigger, input, state);
    }
    return skipped(RECONNECT_REASON_MARKER_PID_INVALID, trigger, state);
  }
  if (input.killedVpnPid !== input.markerPid) {
    // 0.0.59 (B1, MISMATCH): the recorded death names ANOTHER `:vpn` writer, so
    // the marker's process may still be alive — the conservative skip until the
    // status document itself has been dead for RECONNECT_STALE_MISMATCH_MS; then
    // the stale-document verdict is the same as everywhere above.
    if (input.docStaleMs >= RECONNECT_STALE_MISMATCH_MS) {
      return reconnectStale(RECONNECT_REASON_STALE_MISMATCH, trigger, input, state);
    }
    return skipped(RECONNECT_REASON_VPN_PID_MISMATCH, trigger, state);
  }
  // (8) Go: one attempt, counted now, verified by the caller within
  //     RECONNECT_VERIFY_MS. activeSinceMs is cleared because this attempt ENDS
  //     the run it was measured from — the tunnel the new process brings up gets
  //     its own 5-minute clock (reconnectOnActive).
  const attempt: number = state.attempts + 1;
  return reconnectDecision(RECONNECT_ACTION_RECONNECT, RECONNECT_REASON_MARKER_OPEN,
    trigger, attempt, reconnectState(attempt, input.nowMs, input.nowMs, 0, input.nowMs));
}
