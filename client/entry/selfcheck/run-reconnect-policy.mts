// 2026-10-08 扩展被系统回收后的自动重连 — host-side checks.
//
// Two halves, both against shipped sources:
//   A. the PURE decision module, imported BYTE-IDENTICAL (the same trick
//      run-status-freshness.mts uses for NetBirdStatusPolicy.ets): every gate and
//      the whole anti-loop state machine at exact millisecond boundaries;
//   B. the WIRING in pages/Index.ets: the decision is taken inside the poll's
//      "the status document is not fresh" branch, the action re-uses
//      requestStart() (the file must keep exactly ONE startVpnExtensionAbility
//      call site), the active branch advances the counter through
//      reconnectOnActive(), and criterion (7)'s two facts — the marker's pid and
//      the UI process's own `:vpn` APP_KILLED — are read from their owners
//      (NetBirdDiagnosticsCore's marker, the read-only
//      NetBirdSystemEvents.lastKilledVpnProcess getter) and passed in as-is.
//
// 0.0.20 additions, all of them here because they are host-checkable:
//   * the BACKGROUND trigger: a held keep-alive task (keepAliveActive) is the one
//     fact that opens the foreground gate, every other gate still bites, and the
//     record then says trigger=background;
//   * criterion (2) relaxed: a kill event naming the marker's pid makes the
//     document worthless, so the page asks without waiting for it to age out
//     (and hands "no usable document" to the same decision);
//   * the budget as a chain of CONSECUTIVE failures: an active run must last
//     RECONNECT_ACTIVE_RESET_MS (5 min) to forgive the attempts; a refused start
//     ends its own verification window at once and is recorded with the
//     platform's code.
//
// Run: node client/entry/selfcheck/run-reconnect-policy.mts
// Exit 0 = all assertions pass.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const ets = join(here, '..', 'src', 'main', 'ets');
const policySource = join(ets, 'vpnextensionability', 'NetBirdReconnectPolicy.ets');
const POLICY_TEXT = readFileSync(policySource, 'utf8');
const PAGE = readFileSync(join(ets, 'pages', 'Index.ets'), 'utf8');
const SYS_EVENTS = readFileSync(join(ets, 'vpnextensionability', 'NetBirdSystemEvents.ets'), 'utf8');
const SYS_EVENTS_CORE = readFileSync(join(ets, 'vpnextensionability', 'NetBirdSystemEventsCore.ets'), 'utf8');
// The marker vocabulary's owner (2026-10-09, 0.0.17): the policy copies the three
// state tokens as literals on purpose (this module imports nothing), so the
// literals are pinned EQUAL to NetBirdDiagnosticsCore's MARKER_STATE_* exports by
// reading the shipped core source — a drift between the two can never pass here.
const CORE_TEXT = readFileSync(join(ets, 'vpnextensionability', 'NetBirdDiagnosticsCore.ets'), 'utf8');
const coreStringConst = (name: string): string => {
  const hit = CORE_TEXT.match(new RegExp('export const ' + name + ": string = '([^']*)'"));
  return hit === null ? '' : hit[1];
};

let passed = 0;
let failed = 0;
function check(name: string, condition: boolean): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
}
function sliceFrom(src: string, begin: string, end: string): string {
  const at = src.indexOf(begin);
  if (at < 0) { return ''; }
  const stop = src.indexOf(end, at);
  return stop < 0 ? src.slice(at) : src.slice(at, stop);
}

// ---- A. the pure decision ---------------------------------------------------

const dir = mkdtempSync(join(tmpdir(), 'nb-reconnect-policy-'));
try {
  // Purity first: the byte-identical copy below only works because the module
  // imports nothing at all and never touches the platform.
  check('the policy is pure (no @ohos import, no import statement at all)',
    !/@ohos\./.test(POLICY_TEXT) && !/^\s*import\b/m.test(POLICY_TEXT));

  const fixture = join(dir, 'NetBirdReconnectPolicy.ts');
  writeFileSync(fixture, POLICY_TEXT);
  const policy = await import(pathToFileURL(fixture).href);

  const T0 = 1800000000000;
  // The marker pid is the pid of the extension process that wrote the open
  // marker; the UI's kill record names the extension process the platform
  // reports as dead. The base fixture is the CONFIRMED-dead case, so every
  // pre-existing case below keeps testing the gate it was written for.
  const MARKER_PID = 56430;
  const base = {
    nowMs: T0,
    foreground: true,
    keepAliveActive: false,
    markerState: 'open',
    markerAtMs: T0 - 60000,
    markerPid: MARKER_PID,
    startOrStopInFlight: false,
    lastUserStopAtMs: 0,
    lastLogoutAtMs: 0,
    configured: true,
    connectFailureAtMs: 0,
    killedVpnPid: MARKER_PID
  };
  const input = (overrides: object): object => Object.assign({}, base, overrides);
  const fresh = (): object => policy.reconnectInitialState();
  const decided = (state: object, overrides: object): any =>
    policy.decideReconnect(state, input(overrides));

  // -- the reclaim itself ----------------------------------------------------
  const first = decided(fresh(), {});
  check('a reclaimed extension (marker open, no document) reconnects once',
    first.action === policy.RECONNECT_ACTION_RECONNECT &&
    first.reason === policy.RECONNECT_REASON_MARKER_OPEN &&
    first.attempt === 1);
  check('the attempt is counted and opens the 30 s verification window',
    first.state.attempts === 1 && first.state.lastAttemptAtMs === T0 &&
    first.state.verifyingSinceMs === T0);

  // -- the marker is the only proof of "not the user" ------------------------
  check('a CLEAN marker (user stop / logout) never reconnects',
    decided(fresh(), { markerState: 'clean' }).reason === policy.RECONNECT_REASON_MARKER_CLEAN);
  // 2026-10-09 (0.0.17): the extension's onDestroy ENTRY writes `stopping`
  // synchronously, because the async continuation that would write `clean` does
  // not survive the process (the JS context dies ~3 ms after onDestroy returns).
  // For this decision that state means exactly what `clean` means — an orderly
  // stop, never a reclaim — so it must skip with its own reason token.
  const stopping = decided(fresh(), { markerState: 'stopping' });
  check('a STOPPING marker (orderly stop, async clean lost) never reconnects',
    stopping.action === policy.RECONNECT_ACTION_SKIPPED &&
    stopping.reason === policy.RECONNECT_REASON_MARKER_STOPPING &&
    stopping.reason !== policy.RECONNECT_REASON_MARKER_MISSING &&
    stopping.attempt === 0);
  check('the stopping verdict is the marker gate\'s, ahead of every later gate',
    decided(fresh(), { markerState: 'stopping', startOrStopInFlight: true }).reason ===
      policy.RECONNECT_REASON_MARKER_STOPPING &&
    decided(fresh(), { markerState: 'stopping', lastUserStopAtMs: T0 }).reason ===
      policy.RECONNECT_REASON_MARKER_STOPPING &&
    decided(fresh(), { markerState: 'stopping', configured: false }).reason ===
      policy.RECONNECT_REASON_MARKER_STOPPING &&
    decided(fresh(), { markerState: 'stopping', killedVpnPid: null }).reason ===
      policy.RECONNECT_REASON_MARKER_STOPPING);
  check('the three marker tokens the policy carries are the core\'s own, one by one',
    policy.RECONNECT_MARKER_OPEN === coreStringConst('MARKER_STATE_OPEN') &&
    policy.RECONNECT_MARKER_CLEAN === coreStringConst('MARKER_STATE_CLEAN') &&
    policy.RECONNECT_MARKER_STOPPING === coreStringConst('MARKER_STATE_STOPPING') &&
    coreStringConst('MARKER_STATE_OPEN') === 'open' &&
    coreStringConst('MARKER_STATE_CLEAN') === 'clean' &&
    coreStringConst('MARKER_STATE_STOPPING') === 'stopping',
    JSON.stringify([coreStringConst('MARKER_STATE_OPEN'),
      coreStringConst('MARKER_STATE_CLEAN'), coreStringConst('MARKER_STATE_STOPPING')]));
  check('a missing marker never reconnects (conservative)',
    decided(fresh(), { markerState: '' }).reason === policy.RECONNECT_REASON_MARKER_MISSING);
  check('a corrupt/unknown marker state never reconnects (conservative)',
    decided(fresh(), { markerState: 'garbage' }).reason === policy.RECONNECT_REASON_MARKER_MISSING);
  check('the marker path never bypasses the in-flight latch',
    decided(fresh(), { markerState: 'clean' }).action === policy.RECONNECT_ACTION_SKIPPED);

  // -- the user's own actions ------------------------------------------------
  check('a start/stop still in flight blocks the reconnect',
    decided(fresh(), { startOrStopInFlight: true }).reason ===
      policy.RECONNECT_REASON_REQUEST_IN_FLIGHT);
  check('a stop the user just issued (5 s) blocks the reconnect',
    decided(fresh(), { lastUserStopAtMs: T0 - 5000 }).reason ===
      policy.RECONNECT_REASON_RECENT_STOP);
  check('the 8 s stop guard is inclusive',
    decided(fresh(), { lastUserStopAtMs: T0 - 8000 }).reason ===
      policy.RECONNECT_REASON_RECENT_STOP);
  check('past the 8 s stop guard the reconnect is allowed',
    decided(fresh(), { lastUserStopAtMs: T0 - 8001 }).action ===
      policy.RECONNECT_ACTION_RECONNECT);
  check('a logout the user just issued (20 s) blocks the reconnect',
    decided(fresh(), { lastLogoutAtMs: T0 - 20000 }).reason ===
      policy.RECONNECT_REASON_RECENT_LOGOUT);
  check('the 25 s logout guard is inclusive',
    decided(fresh(), { lastLogoutAtMs: T0 - 25000 }).reason ===
      policy.RECONNECT_REASON_RECENT_LOGOUT);
  check('past the 25 s logout guard the reconnect is allowed',
    decided(fresh(), { lastLogoutAtMs: T0 - 25001 }).action ===
      policy.RECONNECT_ACTION_RECONNECT);
  check('an unconfigured client never reconnects',
    decided(fresh(), { configured: false }).reason === policy.RECONNECT_REASON_NOT_CONFIGURED);

  // -- the failure notice newer than the marker ------------------------------
  check('a connect-failure notice NEWER than the marker blocks the reconnect',
    decided(fresh(), { markerAtMs: T0 - 60000, connectFailureAtMs: T0 - 1000 }).reason ===
      policy.RECONNECT_REASON_CONNECT_FAILURE);
  check('a connect-failure notice OLDER than the marker does not',
    decided(fresh(), { markerAtMs: T0 - 1000, connectFailureAtMs: T0 - 60000 }).action ===
      policy.RECONNECT_ACTION_RECONNECT);
  check('an unreadable marker timestamp + a notice blocks (conservative)',
    decided(fresh(), { markerAtMs: 0, connectFailureAtMs: T0 - 1000 }).reason ===
      policy.RECONNECT_REASON_CONNECT_FAILURE);

  // -- criterion (7): the process must be CONFIRMED dead ----------------------
  // A null status document only says "nothing has been published for ~10 s": the
  // system suspends the background extension for up to ~94 s while it keeps
  // running and the marker keeps saying open. Starting the extension again on
  // top of it must be impossible, so the UI's own APP_KILLED for the `:vpn`
  // process has to name the marker's pid.
  const noKill = decided(fresh(), { killedVpnPid: null });
  check('marker open + no fresh document + NO kill event never reconnects',
    noKill.action === policy.RECONNECT_ACTION_SKIPPED &&
    noKill.reason === policy.RECONNECT_REASON_VPN_NOT_DEAD);
  check('and it does not count or arm an attempt',
    noKill.attempt === 0 && noKill.state.attempts === 0 && noKill.state.verifyingSinceMs === 0);
  const otherKill = decided(fresh(), { killedVpnPid: MARKER_PID + 7 });
  check('a kill of a DIFFERENT pid (the marker writer may still be alive) never reconnects',
    otherKill.action === policy.RECONNECT_ACTION_SKIPPED &&
    otherKill.reason === policy.RECONNECT_REASON_VPN_NOT_DEAD);
  check('the marker pid itself in the kill record reconnects',
    decided(fresh(), { killedVpnPid: MARKER_PID }).action === policy.RECONNECT_ACTION_RECONNECT);
  check('an unreadable marker pid + a kill record is refused (conservative)',
    decided(fresh(), { markerPid: 0 }).reason === policy.RECONNECT_REASON_VPN_NOT_DEAD);
  check('a negative/unusable marker pid is refused too (conservative)',
    decided(fresh(), { markerPid: -1, killedVpnPid: -1 }).reason ===
      policy.RECONNECT_REASON_VPN_NOT_DEAD);
  check('a kill of the UI process itself (same shape, another pid) is not enough',
    decided(fresh(), { killedVpnPid: MARKER_PID + 1 }).reason ===
      policy.RECONNECT_REASON_VPN_NOT_DEAD);
  const armed2 = decided(fresh(), { killedVpnPid: MARKER_PID });
  const failed2 = decided(armed2.state, { nowMs: T0 + 30000, killedVpnPid: null });
  check('the anti-loop still runs AHEAD of criterion (7): cooldown, then the failure',
    failed2.action === policy.RECONNECT_ACTION_VERIFY_FAILED &&
    decided(failed2.state, { nowMs: T0 + 59999, killedVpnPid: null }).reason ===
      policy.RECONNECT_REASON_COOLDOWN);
  check('and at the end of the cool-down an unconfirmed death is the one thing left',
    decided(failed2.state, { nowMs: T0 + 60000, killedVpnPid: null }).reason ===
      policy.RECONNECT_REASON_VPN_NOT_DEAD &&
    decided(failed2.state, { nowMs: T0 + 60000 }).action === policy.RECONNECT_ACTION_RECONNECT);

  // -- foreground only -------------------------------------------------------
  check('a tick that slips through while the page is hidden never reconnects',
    decided(fresh(), { foreground: false }).reason === policy.RECONNECT_REASON_NOT_FOREGROUND);

  // -- 0.0.20: the background is allowed ONLY while a keep-alive task is held --
  // The borrowed DATA_TRANSFER task keeps the whole app (UI process included)
  // running, which is why the 2 s poll still fires in the background. That task is
  // therefore the ONE fact that opens the foreground gate — and it is a FACT, not
  // a licence: every gate below (marker, user guards, criterion (7)) still runs.
  const background = decided(fresh(), { foreground: false, keepAliveActive: true });
  check('a hidden page WITH a kept-alive task reconnects, and says so',
    background.action === policy.RECONNECT_ACTION_RECONNECT &&
    background.reason === policy.RECONNECT_REASON_MARKER_OPEN &&
    background.trigger === policy.RECONNECT_TRIGGER_BACKGROUND &&
    background.attempt === 1, JSON.stringify(background));
  check('the background attempt is counted and verified like any other',
    background.state.attempts === 1 && background.state.verifyingSinceMs === T0);
  check('... while a hidden page WITHOUT a task keeps the old refusal',
    decided(fresh(), { foreground: false, keepAliveActive: false }).action ===
      policy.RECONNECT_ACTION_SKIPPED &&
    decided(fresh(), { foreground: false, keepAliveActive: false }).reason ===
      policy.RECONNECT_REASON_NOT_FOREGROUND);
  check('the held task does NOT bypass the marker gate',
    decided(fresh(), { foreground: false, keepAliveActive: true, markerState: 'clean' }).reason ===
      policy.RECONNECT_REASON_MARKER_CLEAN &&
    decided(fresh(), { foreground: false, keepAliveActive: true, markerState: 'stopping' }).reason ===
      policy.RECONNECT_REASON_MARKER_STOPPING);
  check('... nor criterion (7): a held task is not proof that the extension is dead',
    decided(fresh(), { foreground: false, keepAliveActive: true, killedVpnPid: null }).reason ===
      policy.RECONNECT_REASON_VPN_NOT_DEAD);
  check('... nor the user\u2019s own guards',
    decided(fresh(), { foreground: false, keepAliveActive: true, lastUserStopAtMs: T0 - 1000 })
      .reason === policy.RECONNECT_REASON_RECENT_STOP &&
    decided(fresh(), { foreground: false, keepAliveActive: true, lastLogoutAtMs: T0 - 1000 })
      .reason === policy.RECONNECT_REASON_RECENT_LOGOUT);
  check('a visible page reports the foreground trigger',
    decided(fresh(), {}).trigger === policy.RECONNECT_TRIGGER_FOREGROUND &&
    decided(fresh(), { keepAliveActive: true }).trigger === policy.RECONNECT_TRIGGER_FOREGROUND);
  check('the two trigger tokens are the documented closed set',
    policy.RECONNECT_TRIGGER_FOREGROUND === 'foreground' &&
    policy.RECONNECT_TRIGGER_BACKGROUND === 'background');

  // -- the anti-loop ---------------------------------------------------------
  check('inside the 60 s cool-down no second attempt is made',
    decided(first.state, { nowMs: T0 + 10000 }).reason === policy.RECONNECT_REASON_COOLDOWN);
  const verifiedOut = decided(first.state, { nowMs: T0 + 30000 });
  check('30 s without an active document = the attempt failed',
    verifiedOut.action === policy.RECONNECT_ACTION_VERIFY_FAILED &&
    verifiedOut.reason === policy.RECONNECT_REASON_VERIFY_TIMEOUT);
  check('the failure stays counted and clears only the verification window',
    verifiedOut.state.attempts === 1 && verifiedOut.state.verifyingSinceMs === 0 &&
    verifiedOut.state.lastAttemptAtMs === T0);
  check('a failed attempt is NOT retried immediately',
    decided(verifiedOut.state, { nowMs: T0 + 30001 }).reason ===
      policy.RECONNECT_REASON_COOLDOWN);
  check('one ms before the interval is still cool-down',
    decided(verifiedOut.state, { nowMs: T0 + 59999 }).reason ===
      policy.RECONNECT_REASON_COOLDOWN);
  const second = decided(verifiedOut.state, { nowMs: T0 + 60000 });
  check('at exactly 60 s the next attempt is made, counted as the 2nd',
    second.action === policy.RECONNECT_ACTION_RECONNECT && second.attempt === 2 &&
    second.state.lastAttemptAtMs === T0 + 60000);
  check('a 29 999 ms verification window is not a failure yet',
    decided(first.state, { nowMs: T0 + 29999 }).reason === policy.RECONNECT_REASON_COOLDOWN);
  // The 2nd attempt runs its own verification window, then its own cool-down.
  const verifiedOut2 = decided(second.state, { nowMs: T0 + 90000 });
  const third = decided(verifiedOut2.state, { nowMs: T0 + 120000 });
  const verifiedOut3 = decided(third.state, { nowMs: T0 + 150000 });
  const fourth = decided(verifiedOut3.state, { nowMs: T0 + 180000 });
  check('a third attempt is still allowed inside the budget',
    verifiedOut2.action === policy.RECONNECT_ACTION_VERIFY_FAILED &&
    third.action === policy.RECONNECT_ACTION_RECONNECT && third.attempt === 3);
  check('the 4th is refused at the limit',
    fourth.action === policy.RECONNECT_ACTION_LIMIT &&
    fourth.reason === policy.RECONNECT_REASON_ATTEMPT_LIMIT && fourth.attempt === 3);
  check('the limit decision leaves the counter untouched',
    fourth.state.attempts === 3);

  // -- 0.0.20: the budget is a chain of CONSECUTIVE failures --------------------
  // With the keep-alive task the UI process outlives many reclaims, so "an active
  // document was seen once" is no longer evidence of a healthy session. The
  // outstanding attempt is settled by the first active tick (its verification
  // window closes), but the counter is forgiven only after the active run itself
  // has lasted RECONNECT_ACTIVE_RESET_MS.
  const armedOnce = first.state;
  const upAgain = policy.reconnectOnActive(armedOnce, T0 + 10000);
  check('an active document closes the outstanding attempt (it succeeded)',
    armedOnce.attempts === 1 && armedOnce.verifyingSinceMs === T0 &&
    upAgain.attempts === 1 && upAgain.verifyingSinceMs === 0 &&
    upAgain.activeSinceMs === T0 + 10000, JSON.stringify(upAgain));
  check('... but a run shorter than 5 minutes does NOT forgive the chain',
    policy.reconnectOnActive(upAgain, T0 + 10000 + 299999).attempts === 1);
  check('a run that lasts the whole 5 minutes does forgive it',
    policy.reconnectOnActive(upAgain, T0 + 10000 + 300000).attempts === 0 &&
    policy.reconnectOnActive(upAgain, T0 + 10000 + 300000).verifyingSinceMs === 0);
  check('the forgiveness keeps the last attempt stamp (never two < 60 s apart)',
    policy.reconnectOnActive(upAgain, T0 + 10000 + 300000).lastAttemptAtMs === T0);
  check('the window is the documented 5 minutes',
    policy.RECONNECT_ACTIVE_RESET_MS === 300000);
  check('a later active run is measured from ITS OWN first sighting',
    policy.reconnectOnActive(fresh(), T0 + 900000).activeSinceMs === T0 + 900000);
  check('the very first active tick of a run is not re-stamped',
    policy.reconnectOnActive(upAgain, T0 + 10001).activeSinceMs === T0 + 10000);
  const rejected = policy.reconnectOnStartRejected(armedOnce);
  check('a platform-refused start ends its verification window at once',
    rejected.verifyingSinceMs === 0 && rejected.attempts === 1 &&
    rejected.lastAttemptAtMs === T0);
  check('... and keeps the attempt counted: the next one still waits 60 s',
    decided(rejected, { nowMs: T0 + 59999 }).reason === policy.RECONNECT_REASON_COOLDOWN &&
    decided(rejected, { nowMs: T0 + 60000 }).attempt === 2);
  check('a refusal with no attempt outstanding changes nothing',
    policy.reconnectOnStartRejected(fresh()).attempts === 0 &&
    policy.reconnectOnStartRejected(fresh()).verifyingSinceMs === 0);
  // The whole point of the change, on the numbers: three reconnects that all die
  // again inside the window stay three FAILURES — the fourth is refused.
  let flapping = fresh();
  for (let i = 0; i < 3; i++) {
    flapping = decided(flapping, { nowMs: T0 + i * 60000 }).state;
    flapping = policy.reconnectOnActive(flapping, T0 + i * 60000 + 1000);
  }
  check('a tunnel that keeps coming back for seconds still exhausts the budget',
    flapping.attempts === 3 &&
    decided(flapping, { nowMs: T0 + 200000 }).action === policy.RECONNECT_ACTION_LIMIT);

  // -- an active run disarms the guard (0.0.20: after 5 minutes) ----------------
  const armed = second.state;
  const runStart = policy.reconnectOnActive(armed, T0 + 70000);
  const sustained5 = policy.reconnectOnActive(runStart, T0 + 70000 + 300000);
  check('an ACTIVE run of 5 minutes clears the attempt counter',
    armed.attempts === 2 && runStart.attempts === 2 && sustained5.attempts === 0 &&
    sustained5.verifyingSinceMs === 0);
  check('the reset keeps the last attempt stamp (never two attempts < 60 s apart)',
    sustained5.lastAttemptAtMs === T0 + 60000);
  check('after the reset a new reclaim starts again from attempt 1',
    decided(sustained5, { nowMs: T0 + 70000 + 300000 + 60000 }).attempt === 1);
} finally {
  rmSync(dir, { recursive: true, force: true });
}

// ---- B. the wiring in the page ---------------------------------------------

const pollBody = sliceFrom(PAGE, 'private pollConnectorStatus(): void {',
  'private refreshConnectFailure(): void {');
const nullBranch = sliceFrom(pollBody,
  'const status: PublishedStatus | null = currentPublishedStatus(',
  'this.observedPublishedStatus = true;');
const autoBody = sliceFrom(PAGE, 'private runAutoReconnect(requestInFlight: boolean): void {',
  'private diagAutoReconnect(');
const diagBody = sliceFrom(PAGE, 'private diagAutoReconnect(trigger: string, result: string',
  'private diagAutoReconnectOnce(');
const onceBody = sliceFrom(PAGE, 'private diagAutoReconnectOnce(trigger: string, result: string',
  '// ---- derived view state');
const deadBody = sliceFrom(PAGE, 'private extensionConfirmedDead(): boolean {',
  'private refreshConnectFailure(): void {');
const startBody = sliceFrom(PAGE, 'private requestStart(fromAutoReconnect: boolean = false): void {',
  'private requestStop(): void {');

check('Index imports the pure reconnect policy',
  PAGE.indexOf("'../vpnextensionability/NetBirdReconnectPolicy'") >= 0);
check('the page reads the VPN marker (the extension\'s own fact)',
  autoBody.indexOf('MARKER_BASENAME_VPN') >= 0 && autoBody.indexOf('parseMarker(') >= 0);
// 0.0.17: the page needs NO change for `stopping`. It hands the parsed state to
// the policy VERBATIM (the marker state, the pid and the stamp are read from the
// one owner, NetBirdDiagnosticsCore) and owns no state token of its own — one
// more reason the two token sets above must stay equal.
check('the page passes the marker state through verbatim (no token vocabulary of its own)',
  autoBody.indexOf('markerState = marker.state;') >= 0 &&
  autoBody.indexOf('markerState: markerState') >= 0 &&
  !/'(open|clean|stopping)'/.test(autoBody));
check('the decision is taken inside the poll\'s "no fresh document" branch',
  nullBranch.indexOf('this.runAutoReconnect(requestInFlight);') >= 0);
// 0.0.20 判据②放宽: with a kill event naming the marker's pid the page no longer
// waits for the document to age out — it hands the decision the same "there is no
// usable document" fact it would have had ~10 s later. Criterion (7) inside the
// policy is what still demands that proof, so this can only ever be answered "go"
// for a process that really is gone.
check('a document covered by the dead proof is treated as absent for every consumer',
  pollBody.indexOf('const extensionDead: boolean = this.extensionConfirmedDead();') >= 0 &&
  pollBody.indexOf('const live: PublishedStatus | null = extensionDead ? null : status;') >= 0 &&
  pollBody.indexOf('this.syncKeepAlive(live);') >= 0 &&
  pollBody.indexOf('if (live === null) {') >= 0 &&
  pollBody.indexOf('this.published = live;') >= 0);
check('the dead proof is the kill event naming the OPEN marker\'s pid, read from its owners',
  deadBody.indexOf('const killed: ProcessExitRecord | null = lastKilledVpnProcess();') >= 0 &&
  deadBody.indexOf('parseMarker(') >= 0 &&
  deadBody.indexOf('marker.state === RECONNECT_MARKER_OPEN') >= 0 &&
  deadBody.indexOf('marker.pid === killed.pid') >= 0 &&
  !/'(open|clean|stopping)'/.test(deadBody));
check('a kill of a DIFFERENT pid never covers a live document (no prefix, exact pid)',
  deadBody.indexOf('marker.pid > 0') >= 0 &&
  deadBody.indexOf('killed === null') >= 0);
check('the decision is taken after the failure notice has been read',
  nullBranch.indexOf('this.refreshConnectFailure();') <
    nullBranch.indexOf('this.runAutoReconnect(requestInFlight);'));
check('the in-flight latch is captured BEFORE the branch releases it',
  pollBody.indexOf('const requestInFlight: boolean = this.requestSent || this.stopSent;') >= 0 &&
  pollBody.indexOf('const requestInFlight') < pollBody.indexOf('this.requestSent = false;'));
check('the action goes through requestStart and nothing else',
  autoBody.indexOf('decideReconnect(this.reconnectState, facts)') >= 0 &&
  autoBody.split('this.requestStart(true);').length === 2 &&
  autoBody.indexOf('decideReconnect(') < autoBody.indexOf('this.requestStart(true);'));
check('the auto path never copies the extension start call',
  autoBody.indexOf('startVpnExtensionAbility') < 0 && autoBody.indexOf('buildWant') < 0);
// 0.0.20: the ONE fact that opens the foreground gate in the background — the
// borrowed keep-alive task, mirrored from the adapter into the page's policy state.
check('the page hands the held keep-alive task to the decision (background trigger)',
  autoBody.indexOf('keepAliveActive: this.bgKeepAliveState.taskPresent') >= 0 &&
  PAGE.indexOf("'../vpnextensionability/NetBirdKeepAlivePolicy'") >= 0);
// Criterion (7)'s facts: the marker's pid AND the UI process's own kill record.
check('the page reads the marker pid that criterion (7) compares against',
  autoBody.indexOf('marker.pid') >= 0 && autoBody.indexOf('markerPid') >= 0 &&
  autoBody.indexOf('markerPid: markerPid') >= 0);
check('Index imports the read-only VPN-exit getter',
  PAGE.indexOf('lastKilledVpnProcess') >= 0 &&
  PAGE.indexOf("'../vpnextensionability/NetBirdSystemEvents'") >= 0);
check('Index passes the getter result into the decision (not a re-derived guess)',
  autoBody.indexOf('const killedVpn: ProcessExitRecord | null = lastKilledVpnProcess();') >= 0 &&
  autoBody.indexOf('killedVpnPid: killedVpn === null ? null : killedVpn.pid') >= 0 &&
  autoBody.indexOf('lastKilledVpnProcess()') < autoBody.indexOf('decideReconnect('));
check('the runtime records the `:vpn` exit and exposes it read-only',
  SYS_EVENTS.indexOf("this.role !== 'ui'") >= 0 &&
  SYS_EVENTS.indexOf('this.killedVpn.note(') >= 0 &&
  SYS_EVENTS.indexOf('export function lastKilledVpnProcess(): ProcessExitRecord | null') >= 0 &&
  SYS_EVENTS.indexOf('private killedVpn: KilledProcessTracker = new KilledProcessTracker();') >= 0);
check('the `:vpn` process name lives in the pure core, exactly (no prefix match)',
  SYS_EVENTS_CORE.indexOf("VPN_PROCESS_NAME: string = 'cn.alfadb.netbird:vpn'") >= 0 &&
  SYS_EVENTS_CORE.indexOf("fields['ev_process_name'] !== VPN_PROCESS_NAME") >= 0);
check('startVpnExtensionAbility still has exactly ONE call site in Index',
  PAGE.split('startVpnExtensionAbility(').length === 2);
check('an ACTIVE document settles the outstanding attempt and the 5-minute run forgives',
  pollBody.indexOf('this.reconnectState = reconnectOnActive(this.reconnectState, Date.now());') >
    pollBody.indexOf('this.observedPublishedStatus = true;') &&
  PAGE.indexOf('reconnectOnActive') >= 0);
check('the user sees that the reconnect is already happening — IN THE FOREGROUND ONLY',
  PAGE.indexOf('正在自动重新连接…') >= 0 && autoBody.indexOf('正在自动重新连接…') >= 0 &&
  autoBody.indexOf('if (this.pageVisible) {') >= 0 &&
  autoBody.indexOf('if (this.pageVisible) {') < autoBody.indexOf('正在自动重新连接…'));
// 0.0.20: whether the system allows a VPN start from a background UI process is
// exactly what this device has not answered yet, so a REFUSAL has to be visible in
// the log: the platform's own code on the public field set, and the attempt folded
// into the policy as the failure it is.
check('a start the platform refused is recorded with its code and counted as a failure',
  startBody.indexOf('if (fromAutoReconnect) {') >= 0 &&
  startBody.indexOf('reconnectOnStartRejected(this.reconnectState)') >= 0 &&
  startBody.indexOf('RECONNECT_RESULT_FAILED, code,') >= 0 &&
  startBody.indexOf('RECONNECT_TRIGGER_BACKGROUND') >= 0 &&
  startBody.indexOf('const code: string = String((error as BusinessFailure).code ?? \'none\');') >= 0);
check('the decision records ONE safe diagnostic event on the public key set',
  diagBody.indexOf("event('ui_auto_reconnect', fields)") >= 0 &&
  diagBody.indexOf("fields['trigger'] = trigger;") >= 0 &&
  diagBody.indexOf("fields['result']") >= 0 &&
  diagBody.indexOf("fields['reason']") >= 0 && diagBody.indexOf("fields['count']") >= 0 &&
  autoBody.indexOf('this.diagAutoReconnect(decision.trigger,') >= 0);
check('a repeated skip reason is never logged twice in one process',
  PAGE.indexOf('private reconnectReported: Set<string> = new Set<string>();') >= 0 &&
  onceBody.indexOf('this.reconnectReported.has(reason)') >= 0 &&
  onceBody.indexOf('this.reconnectReported.add(reason)') >= 0 &&
  onceBody.indexOf('return;') < onceBody.indexOf('this.reconnectReported.add(reason)'));

console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
