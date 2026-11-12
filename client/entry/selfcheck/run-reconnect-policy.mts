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
import { stripTypeScriptTypes } from 'node:module';
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
    killedVpnPid: MARKER_PID,
    // 0.0.59: the base fixture is the CONFIRMED-death case, so the stale-document
    // clock is 0 (a live document was just seen) — it is the input of the
    // no-kill-record branch only, checked in its own section below.
    docStaleMs: 0
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
    noKill.reason === policy.RECONNECT_REASON_KILL_RECORD_ABSENT);
  check('and it does not count or arm an attempt',
    noKill.attempt === 0 && noKill.state.attempts === 0 && noKill.state.verifyingSinceMs === 0);
  const otherKill = decided(fresh(), { killedVpnPid: MARKER_PID + 7 });
  check('a kill of a DIFFERENT pid (the marker writer may still be alive) never reconnects',
    otherKill.action === policy.RECONNECT_ACTION_SKIPPED &&
    otherKill.reason === policy.RECONNECT_REASON_VPN_PID_MISMATCH);
  check('the marker pid itself in the kill record reconnects',
    decided(fresh(), { killedVpnPid: MARKER_PID }).action === policy.RECONNECT_ACTION_RECONNECT);
  const invalidMarker = decided(fresh(), { markerPid: 0 });
  check('an unreadable marker pid + a kill record is refused (conservative)',
    invalidMarker.reason === policy.RECONNECT_REASON_MARKER_PID_INVALID &&
    invalidMarker.action === policy.RECONNECT_ACTION_SKIPPED);
  check('a negative/unusable marker pid is refused too (conservative)',
    decided(fresh(), { markerPid: -1, killedVpnPid: -1 }).reason ===
      policy.RECONNECT_REASON_MARKER_PID_INVALID);
  check('a kill of the UI process itself (same shape, another pid) is not enough',
    decided(fresh(), { killedVpnPid: MARKER_PID + 1 }).reason ===
      policy.RECONNECT_REASON_VPN_PID_MISMATCH);
  check('death reasons form an ordered closed set: absent before invalid before mismatch',
    decided(fresh(), { killedVpnPid: null, markerPid: 0 }).reason ===
      policy.RECONNECT_REASON_KILL_RECORD_ABSENT &&
    policy.RECONNECT_REASON_KILL_RECORD_ABSENT === 'kill-record-absent' &&
    policy.RECONNECT_REASON_MARKER_PID_INVALID === 'marker-pid-invalid' &&
    policy.RECONNECT_REASON_VPN_PID_MISMATCH === 'vpn-pid-mismatch');
  const armed2 = decided(fresh(), { killedVpnPid: MARKER_PID });
  const failed2 = decided(armed2.state, { nowMs: T0 + 30000, killedVpnPid: null });
  check('the anti-loop still runs AHEAD of criterion (7): cooldown, then the failure',
    failed2.action === policy.RECONNECT_ACTION_VERIFY_FAILED &&
    decided(failed2.state, { nowMs: T0 + 59999, killedVpnPid: null }).reason ===
      policy.RECONNECT_REASON_COOLDOWN);
  check('and at the end of the cool-down an unconfirmed death is the one thing left',
    decided(failed2.state, { nowMs: T0 + 60000, killedVpnPid: null }).reason ===
      policy.RECONNECT_REASON_KILL_RECORD_ABSENT &&
    decided(failed2.state, { nowMs: T0 + 60000 }).action === policy.RECONNECT_ACTION_RECONNECT);

  // -- 0.0.59 (B1): no kill record at all — the STALE document is the evidence ---
  // The 2026-11-04 19:28 batch reclaim (`InvalidKillId`) delivered NO APP_KILLED
  // for this app's `:vpn` process, so criterion (7) answered kill-record-absent
  // forever while the tunnel was long gone. Past RECONNECT_STALE_CONFIRM_MS with
  // the marker still open, the absence of a live document for that long is
  // itself the death evidence: the extension publishes every 5 s and its measured
  // suspension upper bound is ~94 s. The extension is then started DIRECTLY (the
  // page's forceDirectStart), never stop-first.
  check('the stale-document threshold and its reason token are the documented ones',
    policy.RECONNECT_STALE_CONFIRM_MS === 180000 &&
    policy.RECONNECT_REASON_STALE_UNCONFIRMED === 'stale-unconfirmed');
  const staleAlmost = decided(fresh(), { killedVpnPid: null, docStaleMs: 179999 });
  check('one ms short of 180 s still means "cannot confirm"',
    staleAlmost.action === policy.RECONNECT_ACTION_SKIPPED &&
    staleAlmost.reason === policy.RECONNECT_REASON_KILL_RECORD_ABSENT &&
    staleAlmost.attempt === 0 && staleAlmost.state.attempts === 0 &&
    staleAlmost.state.verifyingSinceMs === 0);
  const stale = decided(fresh(), { killedVpnPid: null, docStaleMs: 180000 });
  check('at exactly 180 s without a live document the extension is presumed dead',
    stale.action === policy.RECONNECT_ACTION_RECONNECT &&
    stale.reason === policy.RECONNECT_REASON_STALE_UNCONFIRMED &&
    stale.attempt === 1);
  check('the stale attempt is counted and opens the same 30 s verification window',
    stale.state.attempts === 1 && stale.state.lastAttemptAtMs === T0 &&
    stale.state.verifyingSinceMs === T0 && stale.state.attemptDispatchedAtMs === T0 &&
    stale.state.attemptMarkerPid === 0);
  check('the stale attempt increments an existing counter (attempt N+1)',
    decided(policy.reconnectOnStartRejected(stale.state),
      { nowMs: T0 + 60000, killedVpnPid: null, docStaleMs: 180000 }).attempt === 2);
  check('the user\u2019s stop guard still wins over the stale evidence',
    decided(fresh(), { killedVpnPid: null, docStaleMs: 180000, lastUserStopAtMs: T0 - 1000 })
      .reason === policy.RECONNECT_REASON_RECENT_STOP);
  check('the logout guard, the in-flight latch and the marker gates still win',
    decided(fresh(), { killedVpnPid: null, docStaleMs: 180000, lastLogoutAtMs: T0 - 1000 })
      .reason === policy.RECONNECT_REASON_RECENT_LOGOUT &&
    decided(fresh(), { killedVpnPid: null, docStaleMs: 180000, startOrStopInFlight: true })
      .reason === policy.RECONNECT_REASON_REQUEST_IN_FLIGHT &&
    decided(fresh(), { killedVpnPid: null, docStaleMs: 180000, markerState: 'clean' })
      .reason === policy.RECONNECT_REASON_MARKER_CLEAN &&
    decided(fresh(), { killedVpnPid: null, docStaleMs: 180000, markerState: 'stopping' })
      .reason === policy.RECONNECT_REASON_MARKER_STOPPING &&
    decided(fresh(), { killedVpnPid: null, docStaleMs: 180000, markerState: '' })
      .reason === policy.RECONNECT_REASON_MARKER_MISSING);
  check('an unconfigured client / a newer failure verdict still refuses',
    decided(fresh(), { killedVpnPid: null, docStaleMs: 180000, configured: false })
      .reason === policy.RECONNECT_REASON_NOT_CONFIGURED &&
    decided(fresh(), { killedVpnPid: null, docStaleMs: 180000, connectFailureAtMs: T0 - 1000 })
      .reason === policy.RECONNECT_REASON_CONNECT_FAILURE);
  const stale2 = decided(policy.reconnectOnStartRejected(stale.state),
    { nowMs: T0 + 60000, killedVpnPid: null, docStaleMs: 180000 });
  const stale3 = decided(policy.reconnectOnStartRejected(stale2.state),
    { nowMs: T0 + 120000, killedVpnPid: null, docStaleMs: 180000 });
  const stale4 = decided(policy.reconnectOnStartRejected(stale3.state),
    { nowMs: T0 + 180000, killedVpnPid: null, docStaleMs: 180000 });
  check('the attempt budget and the 60 s interval still bound the stale path',
    decided(policy.reconnectOnStartRejected(stale.state),
      { nowMs: T0 + 59999, killedVpnPid: null, docStaleMs: 180000 }).reason ===
      policy.RECONNECT_REASON_COOLDOWN &&
    stale2.attempt === 2 && stale3.attempt === 3 &&
    stale4.action === policy.RECONNECT_ACTION_LIMIT &&
    stale4.reason === policy.RECONNECT_REASON_ATTEMPT_LIMIT);
  check('a kill record still takes its OWN path, never stale-unconfirmed',
    decided(fresh(), { killedVpnPid: MARKER_PID, docStaleMs: 180000 }).reason ===
      policy.RECONNECT_REASON_MARKER_OPEN &&
    decided(fresh(), { killedVpnPid: MARKER_PID, docStaleMs: 180000 }).action ===
      policy.RECONNECT_ACTION_RECONNECT &&
    decided(fresh(), { killedVpnPid: MARKER_PID + 7, docStaleMs: 180000 }).reason ===
      policy.RECONNECT_REASON_VPN_PID_MISMATCH &&
    decided(fresh(), { killedVpnPid: MARKER_PID, docStaleMs: 0 }).reason ===
      policy.RECONNECT_REASON_MARKER_OPEN);
  check('the stale reason token is not any of the kill-record tokens',
    policy.RECONNECT_REASON_STALE_UNCONFIRMED !== policy.RECONNECT_REASON_KILL_RECORD_ABSENT &&
    policy.RECONNECT_REASON_STALE_UNCONFIRMED !== policy.RECONNECT_REASON_MARKER_OPEN &&
    policy.RECONNECT_REASON_STALE_UNCONFIRMED !== policy.RECONNECT_REASON_VPN_PID_MISMATCH);

  // -- 0.0.59 (B1, MISMATCH): a kill record that does NOT name the marker writer --
  // The 2026-11-12 device report showed `ui_status_lost age_ms>=180000` up to
  // 598.5 s while `stale-unconfirmed` was never taken: a kill record EXISTED, it
  // just named another `:vpn` pid (or the marker carried no usable pid), so
  // criterion (7) answered vpn-pid-mismatch / marker-pid-invalid forever. The same
  // stale-document verdict therefore needs a LONGER silence (300 s) on that path,
  // with the SAME action and the SAME attempt budget as stale-unconfirmed: one
  // budget, one interval, one verification window — never a second counter.
  check('the mismatch threshold and its reason token are the documented ones',
    policy.RECONNECT_STALE_MISMATCH_MS === 300000 &&
    policy.RECONNECT_REASON_STALE_MISMATCH === 'stale-mismatch');
  const mismatchAlmost = decided(fresh(),
    { killedVpnPid: MARKER_PID + 7, docStaleMs: 299999 });
  check('a mismatching kill record + 299.999 s of silence is still the mismatch skip',
    mismatchAlmost.action === policy.RECONNECT_ACTION_SKIPPED &&
    mismatchAlmost.reason === policy.RECONNECT_REASON_VPN_PID_MISMATCH &&
    mismatchAlmost.attempt === 0 && mismatchAlmost.state.attempts === 0 &&
    mismatchAlmost.state.verifyingSinceMs === 0);
  const mismatchStale = decided(fresh(),
    { killedVpnPid: MARKER_PID + 7, docStaleMs: 300000 });
  check('at exactly 300 s the unreconciled kill record no longer withholds the start',
    mismatchStale.action === policy.RECONNECT_ACTION_RECONNECT &&
    mismatchStale.reason === policy.RECONNECT_REASON_STALE_MISMATCH &&
    mismatchStale.attempt === 1);
  check('the mismatch attempt is counted and opens the SAME 30 s verification window',
    mismatchStale.state.attempts === 1 && mismatchStale.state.lastAttemptAtMs === T0 &&
    mismatchStale.state.verifyingSinceMs === T0 &&
    mismatchStale.state.attemptDispatchedAtMs === T0 &&
    mismatchStale.state.attemptMarkerPid === 0 &&
    decided(mismatchStale.state, { nowMs: T0 + 30000, killedVpnPid: MARKER_PID + 7,
      docStaleMs: 300000 }).reason === policy.RECONNECT_REASON_VERIFY_TIMEOUT);
  const invalidAlmost = decided(fresh(), { markerPid: 0, docStaleMs: 299999 });
  check('an unusable marker pid + 299.999 s of silence is still the invalid-pid skip',
    invalidAlmost.action === policy.RECONNECT_ACTION_SKIPPED &&
    invalidAlmost.reason === policy.RECONNECT_REASON_MARKER_PID_INVALID &&
    invalidAlmost.attempt === 0);
  const invalidStale = decided(fresh(), { markerPid: 0, docStaleMs: 300000 });
  check('... and at exactly 300 s it takes the same stale-mismatch start',
    invalidStale.action === policy.RECONNECT_ACTION_RECONNECT &&
    invalidStale.reason === policy.RECONNECT_REASON_STALE_MISMATCH &&
    invalidStale.attempt === 1 && invalidStale.state.verifyingSinceMs === T0);
  check('the user\u2019s stop guard still wins over the mismatching stale evidence',
    decided(fresh(), { killedVpnPid: MARKER_PID + 7, docStaleMs: 300000,
      lastUserStopAtMs: T0 - 1000 }).reason === policy.RECONNECT_REASON_RECENT_STOP);
  check('... and so do the logout guard, the in-flight latch, the marker gates and setup',
    decided(fresh(), { killedVpnPid: MARKER_PID + 7, docStaleMs: 300000,
      lastLogoutAtMs: T0 - 1000 }).reason === policy.RECONNECT_REASON_RECENT_LOGOUT &&
    decided(fresh(), { killedVpnPid: MARKER_PID + 7, docStaleMs: 300000,
      startOrStopInFlight: true }).reason === policy.RECONNECT_REASON_REQUEST_IN_FLIGHT &&
    decided(fresh(), { killedVpnPid: MARKER_PID + 7, docStaleMs: 300000,
      markerState: 'clean' }).reason === policy.RECONNECT_REASON_MARKER_CLEAN &&
    decided(fresh(), { killedVpnPid: MARKER_PID + 7, docStaleMs: 300000,
      markerState: 'stopping' }).reason === policy.RECONNECT_REASON_MARKER_STOPPING &&
    decided(fresh(), { killedVpnPid: MARKER_PID + 7, docStaleMs: 300000,
      markerState: '' }).reason === policy.RECONNECT_REASON_MARKER_MISSING &&
    decided(fresh(), { killedVpnPid: MARKER_PID + 7, docStaleMs: 300000,
      configured: false }).reason === policy.RECONNECT_REASON_NOT_CONFIGURED &&
    decided(fresh(), { killedVpnPid: MARKER_PID + 7, docStaleMs: 300000,
      connectFailureAtMs: T0 - 1000 }).reason === policy.RECONNECT_REASON_CONNECT_FAILURE);
  const exhausted = Object.assign({}, policy.reconnectInitialState(), { attempts: 3 });
  check('mismatch + attempts=3 is the attempt limit: gate (6) still precedes (7)',
    decided(exhausted, { killedVpnPid: MARKER_PID + 7, docStaleMs: 300000 }).action ===
      policy.RECONNECT_ACTION_LIMIT &&
    decided(exhausted, { killedVpnPid: MARKER_PID + 7, docStaleMs: 300000 }).reason ===
      policy.RECONNECT_REASON_ATTEMPT_LIMIT);
  check('a kill record naming the marker writer is unchanged at ANY stale length',
    decided(fresh(), { killedVpnPid: MARKER_PID, docStaleMs: 300000 }).reason ===
      policy.RECONNECT_REASON_MARKER_OPEN &&
    decided(fresh(), { killedVpnPid: MARKER_PID, docStaleMs: 299999 }).reason ===
      policy.RECONNECT_REASON_MARKER_OPEN &&
    decided(fresh(), { killedVpnPid: MARKER_PID, docStaleMs: 300000 }).attempt === 1);
  // One budget for both stale reasons: a stale-unconfirmed attempt and a
  // stale-mismatch attempt are the SAME chain (N+1), share the 60 s interval and
  // the third one is refused by the same limit.
  const shared1 = decided(fresh(), { killedVpnPid: null, docStaleMs: 180000 });
  const shared2 = decided(policy.reconnectOnStartRejected(shared1.state),
    { nowMs: T0 + 60000, killedVpnPid: MARKER_PID + 7, docStaleMs: 300000 });
  check('both stale reasons draw from ONE attempt budget (N+1 across the two)',
    shared1.reason === policy.RECONNECT_REASON_STALE_UNCONFIRMED && shared1.attempt === 1 &&
    shared2.reason === policy.RECONNECT_REASON_STALE_MISMATCH && shared2.attempt === 2 &&
    shared2.state.attempts === 2 && shared2.state.lastAttemptAtMs === T0 + 60000 &&
    decided(policy.reconnectOnStartRejected(shared2.state),
      { nowMs: T0 + 119999, killedVpnPid: MARKER_PID + 7, docStaleMs: 300000 }).reason ===
      policy.RECONNECT_REASON_COOLDOWN);
  const shared3 = decided(policy.reconnectOnStartRejected(shared2.state),
    { nowMs: T0 + 120000, killedVpnPid: null, docStaleMs: 180000 });
  check('... and the third one across the two reasons still hits the same limit',
    shared3.reason === policy.RECONNECT_REASON_STALE_UNCONFIRMED && shared3.attempt === 3 &&
    decided(policy.reconnectOnStartRejected(shared3.state),
      { nowMs: T0 + 180000, killedVpnPid: MARKER_PID + 7, docStaleMs: 300000 }).action ===
      policy.RECONNECT_ACTION_LIMIT);
  check('stale-mismatch is none of the conservative tokens nor stale-unconfirmed',
    policy.RECONNECT_REASON_STALE_MISMATCH !== policy.RECONNECT_REASON_VPN_PID_MISMATCH &&
    policy.RECONNECT_REASON_STALE_MISMATCH !== policy.RECONNECT_REASON_MARKER_PID_INVALID &&
    policy.RECONNECT_REASON_STALE_MISMATCH !== policy.RECONNECT_REASON_STALE_UNCONFIRMED);

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
      policy.RECONNECT_REASON_KILL_RECORD_ABSENT);
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
  // ---- attempt identity and real page methods (no rewritten method bodies) ---
  const NEW_PID = MARKER_PID + 100;
  const latched = policy.reconnectWithAttemptMarker(first.state, T0 + 1, NEW_PID);
  check('dispatch records its own attempt timestamp and starts without a marker pid',
    first.state.attemptDispatchedAtMs === T0 && first.state.attemptMarkerPid === 0);
  check('old and same-time markers cannot identify the dispatched attempt',
    policy.reconnectWithAttemptMarker(first.state, T0 - 1, MARKER_PID) === first.state &&
    policy.reconnectWithAttemptMarker(first.state, T0, MARKER_PID) === first.state);
  check('the first post-dispatch marker pid is latched once',
    latched.attemptMarkerPid === NEW_PID &&
    policy.reconnectWithAttemptMarker(latched, T0 + 2, NEW_PID + 1) === latched);
  check('unreadable marker time or invalid pid does not latch',
    policy.reconnectWithAttemptMarker(first.state, NaN, NEW_PID) === first.state &&
    policy.reconnectWithAttemptMarker(first.state, T0 + 1, 0) === first.state);
  check('an old process pid cannot verify even with a new timestamp',
    !policy.reconnectStatusMatchesAttempt(latched, MARKER_PID, T0 + 2000));
  check('a pid-bearing report waits for the new marker, never falls back to time',
    !policy.reconnectStatusMatchesAttempt(first.state, NEW_PID, T0 + 2000));
  check('a matching new pid verifies the attempt',
    policy.reconnectStatusMatchesAttempt(latched, NEW_PID, T0 + 2000));
  check('a fresh old legacy report cannot verify the attempt',
    !policy.reconnectStatusMatchesAttempt(latched, undefined, T0 - 4747));
  check('legacy pid-absent documents use the inclusive 1s time tolerance',
    policy.reconnectStatusMatchesAttempt(latched, undefined, T0 - 1000) &&
    !policy.reconnectStatusMatchesAttempt(latched, undefined, T0 - 1001));
  for (const [label, settled] of [
    ['success', policy.reconnectOnActive(latched, T0 + 2000)],
    ['timeout', decided(latched, { nowMs: T0 + 30000 }).state],
    ['platform rejection', policy.reconnectOnStartRejected(latched)]
  ]) {
    check(`${label} clears attempt identity and removes the gate from later ticks`,
      settled.verifyingSinceMs === 0 && settled.attemptDispatchedAtMs === 0 &&
      settled.attemptMarkerPid === 0 &&
      policy.reconnectStatusMatchesAttempt(settled, MARKER_PID, T0 - 99999) &&
      policy.reconnectWithAttemptMarker(settled, T0 + 40000, NEW_PID) === settled);
  }
  check('requested and reconnected have separate closed result tokens',
    policy.RECONNECT_RESULT_REQUESTED === 'requested' &&
    policy.RECONNECT_RESULT_RECONNECTED === 'reconnected');

  const loadPure = async (name: string): Promise<any> => {
    const path = join(dir, name + '.ts');
    writeFileSync(path, readFileSync(join(ets, 'vpnextensionability', name + '.ets')));
    return import(pathToFileURL(path).href);
  };
  const model = await loadPure('NetBirdUiModel');
  const statusPolicy = await loadPure('NetBirdStatusPolicy');
  const core = await loadPure('NetBirdDiagnosticsCore');
  // 0.0.59 A2: `ui_status_lost` may only carry keys on the public field set —
  // the writer drops the WHOLE record for an unknown key, so a typo here would
  // make the new record silently vanish on device. Both keys are the ones the
  // existing `ui_status_read` already uses.
  check('the status-lost record keys are on the public field set',
    core.PUBLIC_FIELD_KEYS.indexOf('age_ms') >= 0 &&
    core.PUBLIC_FIELD_KEYS.indexOf('derived') >= 0);
  function method(source: string, name: string): string {
    const hit = new RegExp('^  private (?:async )?' + name + '\\(', 'm').exec(source);
    if (hit === null) { throw new Error('missing method: ' + name); }
    const end = source.indexOf('\n  }', hit.index);
    if (end < 0) { throw new Error('missing method end: ' + name); }
    return source.slice(hit.index, end + 4);
  }
  const names = ['runAutoReconnect', 'requestStart', 'startVpnExtension',
    'refreshReconnectAttemptMarker', 'extensionConfirmedDead', 'pollConnectorStatusPass',
    'diagAutoReconnect', 'diagAutoReconnectOnce'];
  const adapterTs = 'class Adapter {\n' + names.map(n => method(PAGE, n)).join('\n') + '\n}';
  writeFileSync(join(dir, 'IndexMethods.ts'), adapterTs);
  let clock = T0;
  let doc: any = null;
  let markerText = '';
  let rejectStart = false;
  // 0.0.59 (B1): switchable, so the no-kill-record branch can be driven through
  // the REAL page methods exactly like the confirmed-death one below.
  let killedRecord: any = { pid: MARKER_PID };
  const calls: string[] = [];
  const records: any[] = [];
  const env = { ...policy, ...model, ...statusPolicy, ...core,
    Date: { now: () => clock, parse: Date.parse },
    fs: { accessSync: () => true, readTextSync: () => markerText },
    getContext: () => ({ filesDir: '/fixture' }),
    lastKilledVpnProcess: () => killedRecord,
    readPublishedStatus: () => doc,
    keepAlivePollGapMs: () => 0, keepAliveResumedAfterGap: () => false,
    keepAliveOnUserConnect: (s: any) => s,
    DOMAIN: 0, TAG: 'fixture', BUNDLE_NAME: 'fixture', EXTENSION_ABILITY: 'fixture',
    // 0.0.59 (A2): the page's own module-level constant (pinned in section B). It
    // is not a class field, so the extracted methods read it from this scope.
    AUTO_RECONNECT_REPORT_INTERVAL_MS: 600000,
    hilog: { info: () => {}, warn: () => {}, error: () => {} },
    event: (_name: string, fields: any) => {
      records.push(fields); calls.push('auto:' + fields.result);
    },
    vpnExtension: { startVpnExtensionAbility: () => {
      calls.push('platform-start');
      return rejectStart ? Promise.reject({ code: 16000050 }) : Promise.resolve();
    }, stopVpnExtensionAbility: () => {
      // 0.0.59 (B1): a stop is the ONE thing the stale path must never issue —
      // recorded so its absence can be asserted, not merely assumed.
      calls.push('platform-stop');
      return Promise.resolve();
    } }
  };
  const Adapter = new Function('env', 'with (env) { ' + stripTypeScriptTypes(adapterTs) +
    '; return Adapter; }')(env);
  function page(): any {
    clock = T0; doc = null; calls.length = 0; records.length = 0;
    markerText = core.buildMarker(new Date(T0 - 60000).toISOString(), MARKER_PID, 'vpn', 'open');
    const p = new Adapter();
    Object.assign(p, { requestSent: false, stopSent: false, requestId: 'fixture',
      configured: true, pageVisible: true, bgKeepAliveState: { taskPresent: false },
      connectFailureClass: '', connectFailureAtMs: 0, actionError: '', disconnectNotice: '',
      lastUserStopAtMs: 0, lastLogoutAtMs: 0, lastRestartLegAtMs: 0,
      reconnectState: policy.reconnectInitialState(), reconnectReportedAt: new Map(),
      lastLiveDocAtMs: 0, statusLostReported: false,
      lastPollAtMs: 0, pendingResumeReason: '', routeSelectionOpen: false,
      observedPublishedStatus: false, lastLiveState: '', published: null,
      appliedStatusStamp: '', diagActionCount: 0,
      diagEvent: () => calls.push('ui-connect'), extensionGone: () => true,
      buildWant: () => ({}), clearPendingAction: () => {}, updatePendingAction: () => {},
      syncKeepAlive: () => {}, diagStatusReadNoDocument: () => {},
      diagStatusReadApplied: () => {}, refreshConnectFailure: () => {} });
    return p;
  }
  const tick = (p: any, ms: number, status: any): void => {
    clock = ms; doc = status; p.pollConnectorStatusPass();
  };
  const newMarker = (): void => {
    markerText = core.buildMarker(new Date(T0 + 583).toISOString(), NEW_PID, 'vpn', 'open');
  };
  const connected = (updated: number, pid?: number): any =>
    ({ updated_at_ms: updated, status_revision: 228, state: 'connected', pid: pid });
  const p = page(); p.runAutoReconnect(false, 0);
  check('REAL dispatch logs requested before requestStart and platform start, never reconnected',
    calls.join(',') === 'auto:requested,ui-connect,platform-start' &&
    p.reconnectState.attemptDispatchedAtMs === T0);
  check('REAL requestStart retains the manual 7s pending action window',
    p.pendingActionDeadlineMs === T0 + 7000);
  newMarker();
  tick(p, T0 + 1001, connected(T0 - 4747));
  check('REAL fresh legacy old report leaves the verification window armed',
    p.reconnectState.verifyingSinceMs === T0 && p.reconnectState.attemptMarkerPid === NEW_PID &&
    !records.some(r => r.result === 'reconnected'));
  tick(p, T0 + 2000, connected(T0 + 2000, MARKER_PID));
  check('REAL fresh report from old pid cannot settle the new attempt',
    p.reconnectState.verifyingSinceMs === T0 && !records.some(r => r.result === 'reconnected'));
  tick(p, T0 + 3000, connected(T0 + 3000, NEW_PID));
  tick(p, T0 + 4000, doc);
  check('REAL matching pid confirms once, in order after requested and platform start',
    records.filter(r => r.result === 'reconnected').length === 1 &&
    calls.indexOf('auto:reconnected') > calls.indexOf('platform-start') &&
    p.reconnectState.verifyingSinceMs === 0 && p.reconnectState.attemptMarkerPid === 0 &&
    p.reconnectState.attemptDispatchedAtMs === 0);
  tick(p, T0 + 5000, connected(T0 + 5000, MARKER_PID));
  check('REAL ticks after success no longer gate on the attempt identity',
    p.reconnectState.activeSinceMs === T0 + 3000 &&
    records.filter(r => r.result === 'reconnected').length === 1);

  for (const pid of [undefined, MARKER_PID]) {
    const q = page(); q.runAutoReconnect(false, 0); newMarker();
    const old = connected(T0 - 4747, pid);
    tick(q, T0 + 1001, old);
    // Supply a fresh but wrong-pid document at the deadline. Repeating the same
    // document tests that reader dedup cannot return before verification runs.
    const wrong = connected(T0 + 29000, MARKER_PID);
    tick(q, T0 + 29000, wrong); tick(q, T0 + 30000, wrong);
    check(`REAL old report (${pid === undefined ? 'legacy time' : 'pid'}) times out at 30s and clears identity`,
      records.some(r => r.result === 'failed' && r.reason === 'no-active-in-30s') &&
      !records.some(r => r.result === 'reconnected') && q.reconnectState.verifyingSinceMs === 0 &&
      q.reconnectState.attemptDispatchedAtMs === 0 && q.reconnectState.attemptMarkerPid === 0);
    tick(q, T0 + 31000, wrong);
    check('REAL later ticks are ungated after verify-failed',
      q.reconnectState.activeSinceMs === T0 + 31000);
  }
  const legacyPage = page(); legacyPage.runAutoReconnect(false, 0); newMarker();
  tick(legacyPage, T0 + 1000, connected(T0 - 1000));
  check('REAL pid-absent document can verify via the time fallback',
    legacyPage.reconnectState.verifyingSinceMs === 0 &&
    records.filter(r => r.result === 'reconnected').length === 1);
  const refused = page(); rejectStart = true; refused.runAutoReconnect(false, 0);
  await Promise.resolve(); await Promise.resolve(); rejectStart = false;
  check('REAL platform rejection records requested then failed and clears attempt latches',
    records.map(r => r.result).join(',') === 'requested,failed' &&
    refused.reconnectState.attemptDispatchedAtMs === 0 &&
    refused.reconnectState.attemptMarkerPid === 0);
  // 0.0.59 (B1): the REAL stale path through the REAL page methods. No APP_KILLED
  // in this process, no live document for 180 s, marker still open: the poll's
  // null branch must decide stale-unconfirmed and start DIRECTLY — the platform
  // stop leg must never run.
  killedRecord = null;
  const stalePage = page();
  stalePage.lastLiveDocAtMs = T0 - 180000;
  stalePage.pollConnectorStatusPass();
  await Promise.resolve(); await Promise.resolve();
  check('REAL stale page starts directly with the stale-unconfirmed reason',
    records.map(r => r.reason + ':' + r.result).join(',') === 'stale-unconfirmed:requested' &&
    calls.join(',') === 'auto:requested,ui-connect,platform-start' &&
    !calls.includes('platform-stop'));
  check('REAL stale dispatch is counted and verified like any other attempt',
    stalePage.reconnectState.attempts === 1 &&
    stalePage.reconnectState.verifyingSinceMs === T0 &&
    stalePage.reconnectState.attemptDispatchedAtMs === T0);
  // One ms short of the threshold keeps the old, conservative verdict — and, as
  // before, that skip is recorded through the (now time-windowed) latch.
  const quietPage = page();
  quietPage.lastLiveDocAtMs = T0 - 179999;
  quietPage.pollConnectorStatusPass();
  check('REAL stale page one ms short of 180 s still skips kill-record-absent',
    records.map(r => r.reason + ':' + r.result).join(',') === 'kill-record-absent:skipped' &&
    calls.join(',') === 'auto:skipped' && !calls.includes('platform-start') &&
    quietPage.reconnectState.attempts === 0);
  // 0.0.59 (B1, MISMATCH): the same REAL null-document branch, but the kill record
  // names ANOTHER `:vpn` writer. At 300 s of silence the page must take the
  // stale-mismatch decision and the SAME direct start (no platform stop leg).
  killedRecord = { pid: MARKER_PID + 7 };
  const mismatchPage = page();
  mismatchPage.lastLiveDocAtMs = T0 - 300000;
  mismatchPage.pollConnectorStatusPass();
  await Promise.resolve(); await Promise.resolve();
  check('REAL mismatch page starts directly with the stale-mismatch reason',
    records.map(r => r.reason + ':' + r.result).join(',') === 'stale-mismatch:requested' &&
    calls.join(',') === 'auto:requested,ui-connect,platform-start' &&
    !calls.includes('platform-stop') &&
    mismatchPage.reconnectState.attempts === 1 &&
    mismatchPage.reconnectState.verifyingSinceMs === T0);
  killedRecord = { pid: MARKER_PID + 7 };
  const mismatchQuiet = page();
  mismatchQuiet.lastLiveDocAtMs = T0 - 299999;
  mismatchQuiet.pollConnectorStatusPass();
  check('REAL mismatch page one ms short of 300 s still skips vpn-pid-mismatch',
    records.map(r => r.reason + ':' + r.result).join(',') === 'vpn-pid-mismatch:skipped' &&
    !calls.includes('platform-start') && mismatchQuiet.reconnectState.attempts === 0);
  killedRecord = { pid: MARKER_PID };
  // 0.0.59 (A2): the REAL latch. Same reason inside the 10-minute window writes
  // nothing; at exactly the window's end the SAME reason is recorded again (the
  // old process-wide Set would have kept it silent forever).
  clock = T0;
  const latchPage = page();
  latchPage.diagAutoReconnectOnce('foreground', 'skipped', 'cooldown', 0);
  latchPage.diagAutoReconnectOnce('foreground', 'skipped', 'cooldown', 0);
  check('REAL the same skip reason is recorded once inside the 10-minute window',
    records.length === 1 && records[0].reason === 'cooldown');
  clock = T0 + 599999;
  latchPage.diagAutoReconnectOnce('foreground', 'skipped', 'cooldown', 0);
  check('REAL ... and still nothing one ms before it ends', records.length === 1);
  clock = T0 + 600000;
  latchPage.diagAutoReconnectOnce('foreground', 'skipped', 'cooldown', 0);
  check('REAL at exactly 600 s the same reason is recorded again',
    records.length === 2 && latchPage.reconnectReportedAt.get('cooldown') === T0 + 600000);
  killedRecord = { pid: MARKER_PID };
  // The real publication exit adds pid without defeating the existing debounce.
  const vpe = readFileSync(join(ets, 'vpnextensionability', 'NetBirdVpnExtensionAbility.ets'), 'utf8');
  const writerTs = 'class Writer {\n' + method(vpe, 'publishStatusForUi') + '\n}';
  let stored = ''; let writes = 0;
  const writerEnv = { ...model, ...statusPolicy, Date: { now: () => clock },
    process: { pid: NEW_PID }, connectorNetworkConfig: () => null, STATUS_POLL_MS: 5000,
    DOMAIN: 0, TAG: 'fixture', safeError: () => 'fixture', hilog: env.hilog,
    fs: { OpenMode: { READ_WRITE: 1, CREATE: 2, TRUNC: 4 },
      openSync: () => ({ fd: 1 }), closeSync: () => {},
      writeSync: (_fd: number, text: string) => { stored = text; writes++; } } };
  const Writer = new Function('env', 'with (env) { ' + stripTypeScriptTypes(writerTs) +
    '; return Writer; }')(writerEnv);
  function writer(): any {
    const w = new Writer();
    Object.assign(w, { context: { filesDir: '/fixture' }, watcherTimer: 1,
      destroyRequested: false, connectorDeathHandled: false, requestId: 'fixture',
      lastStatusChangeKey: '', lastStatusPublishMs: 0, lastUiStatusLogAtMs: 0,
      diagStatusPublish: () => {} });
    return w;
  }
  clock = T0;
  const status = { state: 'connected', running: true, peer_count: 0, route_count: 0,
    reconnects: 0, status_revision: 1 };
  const w = writer(); w.publishStatusForUi(status, 'present');
  check('REAL publisher serializes numeric process.pid alongside the existing request_id',
    JSON.parse(stored).pid === NEW_PID && JSON.parse(stored).request_id === 'fixture');
  clock += 1000; w.publishStatusForUi(status, 'present');
  check('REAL writer still deduplicates unchanged reports with pid present', writes === 1);
  const w2 = writer(); w2.publishStatusForUi(status, 'present');
  check('a new extension instance never deduplicates its first identity publication', writes === 2);
  const connector = readFileSync(join(ets, 'vpnextensionability', 'NetBirdConnector.ets'), 'utf8');
  const readerTs = sliceFrom(connector, 'export function readPublishedStatus(',
    '\n// ---------------------------------------------------------------------------').replace('export ', '');
  const readStatus = new Function('env', 'with (env) { ' + stripTypeScriptTypes(readerTs) +
    '; return readPublishedStatus; }')({ Date: { now: () => clock }, STATUS_POLL_MS: 5000,
    fs: { accessSync: () => true, readTextSync: () => stored } });
  check('REAL UI reader accepts the new numeric pid field', readStatus('/fixture').pid === NEW_PID);
  const legacyDoc = JSON.parse(stored); delete legacyDoc.pid; stored = JSON.stringify(legacyDoc);
  check('REAL UI reader continues parsing a legacy document with no pid',
    readStatus('/fixture').state === 'connected' && readStatus('/fixture').pid === undefined);
} finally {
  rmSync(dir, { recursive: true, force: true });
}

// ---- B. the wiring in the page ---------------------------------------------

const pollBody = sliceFrom(PAGE, 'private pollConnectorStatus(): void {',
  'private refreshConnectFailure(): void {');
const nullBranch = sliceFrom(pollBody,
  'const status: PublishedStatus | null = currentPublishedStatus(',
  'this.observedPublishedStatus = true;');
const autoBody = sliceFrom(PAGE,
  'private runAutoReconnect(requestInFlight: boolean, docStaleMs: number): void {',
  'private diagAutoReconnect(');
const diagBody = sliceFrom(PAGE, 'private diagAutoReconnect(trigger: string, result: string',
  'private diagAutoReconnectOnce(');
const onceBody = sliceFrom(PAGE, 'private diagAutoReconnectOnce(trigger: string, result: string',
  '// ---- derived view state');
const deadBody = sliceFrom(PAGE, 'private extensionConfirmedDead(): boolean {',
  'private refreshConnectFailure(): void {');
const startBody = sliceFrom(PAGE, 'private requestStart(fromAutoReconnect: boolean = false,',
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
  nullBranch.indexOf('this.runAutoReconnect(requestInFlight, docStaleMs);') >= 0);
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
    nullBranch.indexOf('this.runAutoReconnect(requestInFlight, docStaleMs);'));
check('the in-flight latch is captured BEFORE the branch releases it',
  pollBody.indexOf('const requestInFlight: boolean = this.requestSent || this.stopSent;') >= 0 &&
  pollBody.indexOf('const requestInFlight') < pollBody.indexOf('this.requestSent = false;'));
check('the action goes through requestStart and nothing else',
  autoBody.indexOf('decideReconnect(this.reconnectState, facts)') >= 0 &&
  autoBody.split('this.requestStart(').length === 3 &&
  autoBody.split('this.requestStart(true);').length === 2 &&
  autoBody.indexOf('decideReconnect(') < autoBody.indexOf('this.requestStart(true);'));
check('the auto path never copies the extension start call',
  autoBody.indexOf('startVpnExtensionAbility') < 0 && autoBody.indexOf('buildWant') < 0);
// 0.0.59 (B1): the stale-document death has its own token and its own start leg —
// the confirmed-death path must keep `requestStart(true)` EXACTLY as it was, and
// only the stale one may take the direct start (forceDirectStart = no stop-first).
check('the stale-document death is wired with its own reason token',
  PAGE.indexOf('RECONNECT_REASON_STALE_UNCONFIRMED') >= 0 &&
  PAGE.indexOf('stale-unconfirmed') >= 0 &&
  autoBody.indexOf('decision.reason === RECONNECT_REASON_STALE_UNCONFIRMED') >= 0);
check('only the stale decision starts DIRECTLY; the confirmed-death path is unchanged',
  autoBody.split('this.requestStart(true, true);').length === 2 &&
  autoBody.split('this.requestStart(true);').length === 2 &&
  autoBody.indexOf('this.requestStart(true, true);') <
    autoBody.indexOf('this.requestStart(true);') &&
  autoBody.indexOf('decision.reason === RECONNECT_REASON_STALE_UNCONFIRMED') <
    autoBody.indexOf('this.requestStart(true, true);'));
// 0.0.59 (B1, MISMATCH): the SECOND stale reason goes through the SAME one direct
// start — a mismatch branch that never reached this condition would leave the
// device symptom (a kill record naming another writer) exactly where it was.
check('the unreconciled-kill reason shares the SAME direct start (mismatch wired)',
  PAGE.indexOf('RECONNECT_REASON_STALE_MISMATCH') >= 0 &&
  autoBody.indexOf('decision.reason === RECONNECT_REASON_STALE_MISMATCH') >= 0 &&
  autoBody.indexOf('decision.reason === RECONNECT_REASON_STALE_UNCONFIRMED') <
    autoBody.indexOf('decision.reason === RECONNECT_REASON_STALE_MISMATCH') &&
  autoBody.indexOf('decision.reason === RECONNECT_REASON_STALE_MISMATCH') <
    autoBody.indexOf('this.requestStart(true, true);') &&
  autoBody.indexOf('this.requestStart(true, true);') <
    autoBody.indexOf('this.requestStart(true);'));
check('the mismatch branch also skips the stop-first leg through the same flag',
  startBody.indexOf('forceDirectStart: boolean = false') >= 0 &&
  startBody.indexOf('if (forceDirectStart) {') >= 0 &&
  startBody.indexOf('this.startVpnExtension(fromAutoReconnect);') <
    startBody.indexOf('connectStartLeg(this.extensionGone())'));
check('the direct-start flag skips the stop-first leg and adds no start call site',
  startBody.indexOf('forceDirectStart: boolean = false') >= 0 &&
  startBody.indexOf('if (forceDirectStart) {') >= 0 &&
  startBody.indexOf('UI_START_DIRECT|reason=stale-unconfirmed') >= 0 &&
  startBody.indexOf('this.startVpnExtension(fromAutoReconnect);') >= 0 &&
  startBody.indexOf('if (forceDirectStart) {') <
    startBody.indexOf('connectStartLeg(this.extensionGone())') &&
  // the request-owning half only: the ONE platform call lives in the helper below.
  startBody.slice(0, startBody.indexOf('private startVpnExtension('))
    .indexOf('startVpnExtensionAbility(') < 0);
check('the page measures the stale clock off the last LIVE document',
  PAGE.indexOf('private lastLiveDocAtMs: number = 0;') >= 0 &&
  pollBody.indexOf('this.lastLiveDocAtMs = nowMs;') >= 0 &&
  pollBody.indexOf('const docStaleMs: number = this.lastLiveDocAtMs > 0 ? ' +
    'nowMs - this.lastLiveDocAtMs : 0;') >= 0 &&
  nullBranch.indexOf('this.runAutoReconnect(requestInFlight, docStaleMs);') >= 0 &&
  autoBody.indexOf('docStaleMs: docStaleMs') >= 0);
// 0.0.59 (A2): one `ui_status_lost` per loss episode, on the whitelisted scalar
// keys the existing ui_status_read already uses, emitted before the episode is
// reset; the per-reason latch is now a 10-minute TIME window.
check('a status-loss episode leaves ONE record with whitelisted scalar keys',
  PAGE.indexOf("event('ui_status_lost', lostFields)") >= 0 &&
  PAGE.indexOf("const lostFields: DiagFields = { 'age_ms': docStaleMs, 'derived': false };") >= 0 &&
  PAGE.indexOf('private statusLostReported: boolean = false;') >= 0 &&
  pollBody.indexOf('this.statusLostReported = false;') >= 0 &&
  pollBody.indexOf('!this.statusLostReported) {') >= 0 &&
  pollBody.indexOf('this.diagStatusReadNoDocument(nowMs);') <
    pollBody.indexOf("event('ui_status_lost', lostFields)") &&
  pollBody.indexOf("event('ui_status_lost', lostFields)") <
    pollBody.indexOf('if (this.observedPublishedStatus) {'));
check('the skip latch is a per-reason TIME window, not one record per process',
  PAGE.indexOf('private reconnectReportedAt: Map<string, number> = ' +
    'new Map<string, number>();') >= 0 &&
  PAGE.indexOf('const AUTO_RECONNECT_REPORT_INTERVAL_MS: number = 600000;') >= 0 &&
  onceBody.indexOf('this.reconnectReportedAt.get(reason)') >= 0 &&
  onceBody.indexOf('this.reconnectReportedAt.set(reason, nowMs)') >= 0 &&
  onceBody.indexOf('nowMs - lastAtMs < AUTO_RECONNECT_REPORT_INTERVAL_MS') >= 0 &&
  onceBody.indexOf('return;') < onceBody.indexOf('this.reconnectReportedAt.set(reason, nowMs)') &&
  PAGE.indexOf('reconnectReported: Set<string>') < 0);
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

console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
