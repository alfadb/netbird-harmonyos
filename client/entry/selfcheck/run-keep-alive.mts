// 2026-10-10 后台保活（0.0.19 实验版，方案 A）— host-side checks.
//
// Three halves, all against shipped sources, no device, no build, no compiler:
//   A. the PURE decision module (NetBirdKeepAlivePolicy.ets), imported
//      BYTE-IDENTICAL (the same trick run-reconnect-policy.mts uses): the whole
//      state machine at exact millisecond boundaries — connect / disconnect /
//      logout, the settings switch, the platform's cancel reasons (user dismissal,
//      low speed, not updated, illegal use), the bounded backoff and its 5 min cap,
//      the session budget, the refresh throttle and the live-view text.
//   B. the PERSISTENCE module (NetBirdKeepAliveStore.ets) over a FUNCTIONAL
//      @ohos.file.fs stub that forwards to real node fs (keep-alive-hooks.mjs):
//      default on a missing document, on corrupt JSON, on an unknown version and on
//      a wrong value type; the round-trip; the atomic temp+fsync+rename path; a short
//      write and an IO error leaving the previous document untouched; and the path
//      whitelist (only this module's own document is ever addressed).
//   C. the WIRING: the adapter is the ONLY module importing backgroundTaskManager,
//      notificationManager and wantAgent; it claims an existing task before creating
//      one, uses the notificationId-returning overload and never
//      updateBackgroundRunning; the page asks it from its EXISTING 2 s tick (still
//      exactly one setInterval and one startVpnExtensionAbility call site); the
//      ability registers on onCreate and unregisters+stops on onDestroy;
//      module.json5 declares the background mode and the permission; the version is
//      20 / 0.0.20. 0.0.20 adds the reconnect grace: the page keeps the moment the
//      tunnel went inactive and the reconnect's gave-up flag as facts, and the
//      grace republishes the live view with the policy's "正在重新连接…" through
//      refreshKeepAliveText() (the adapter still owns no text).
//
// Run: node client/entry/selfcheck/run-keep-alive.mts
// Exit 0 = all assertions pass.

import { readFileSync, mkdtempSync, writeFileSync, mkdirSync, rmSync, readdirSync,
  existsSync } from 'node:fs';
import { promises as nodeFs } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./keep-alive-hooks.mjs', import.meta.url).href);

const here = dirname(fileURLToPath(import.meta.url));
const ets = join(here, '..', 'src', 'main', 'ets');
const POLICY = join(ets, 'vpnextensionability', 'NetBirdKeepAlivePolicy.ets');
const STORE = join(ets, 'vpnextensionability', 'NetBirdKeepAliveStore.ets');
const SERVICE = join(ets, 'vpnextensionability', 'NetBirdKeepAliveService.ets');
const POLICY_TEXT = readFileSync(POLICY, 'utf8');
const STORE_TEXT = readFileSync(STORE, 'utf8');
const SERVICE_TEXT = readFileSync(SERVICE, 'utf8');
const PAGE = readFileSync(join(ets, 'pages', 'Index.ets'), 'utf8');
const ABILITY = readFileSync(join(ets, 'entryability', 'EntryAbility.ets'), 'utf8');
const MODULE_JSON5 = readFileSync(join(here, '..', 'src', 'main', 'module.json5'), 'utf8');
const BUILD_PROFILE = readFileSync(join(here, '..', '..', 'build-profile.json5'), 'utf8');
const APP_JSON5 = readFileSync(join(here, '..', '..', 'AppScope', 'app.json5'), 'utf8');

const globals = globalThis as unknown as Record<string, unknown>;

let passed = 0;
let failed = 0;
function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else {
    failed++;
    if (detail !== undefined) { console.log(`     ${detail}`); }
  }
}
function sliceFrom(src: string, begin: string, end: string): string {
  const at = src.indexOf(begin);
  if (at < 0) { return ''; }
  const stop = src.indexOf(end, at);
  return stop < 0 ? src.slice(at) : src.slice(at, stop);
}
function etsFiles(root: string): string[] {
  const out: string[] = [];
  const walk = (dirPath: string): void => {
    for (const entry of readdirSync(dirPath, { withFileTypes: true })) {
      const path = join(dirPath, entry.name);
      if (entry.isDirectory()) { walk(path); } else if (entry.name.endsWith('.ets')) { out.push(path); }
    }
  };
  walk(root);
  return out;
}

const FIXTURES = mkdtempSync(join(tmpdir(), 'nb-keep-alive-'));

// ---- A. the pure decision ---------------------------------------------------

try {
  check('the policy is pure (no @ohos import, no import statement at all)',
    !/@ohos\./.test(POLICY_TEXT) && !/^\s*import\b/m.test(POLICY_TEXT));
  check('the policy never reads a clock itself (every decision takes nowMs)',
    POLICY_TEXT.indexOf('Date.now(') < 0);

  writeFileSync(join(FIXTURES, 'NetBirdKeepAlivePolicy.ts'), POLICY_TEXT);
  const p = await import(pathToFileURL(join(FIXTURES, 'NetBirdKeepAlivePolicy.ts')).href);

  const T0 = 1800000000000;
  const base = {
    nowMs: T0, enabled: true, vpnActive: true, intentOn: false, userStopRecent: false,
    loggedOut: false, startInFlight: false,
    vpnInactiveSinceMs: 0, reconnectGaveUp: false
  };
  const input = (overrides: object): object => Object.assign({}, base, overrides);
  const fresh = (): any => p.keepAliveInitialState();
  const decided = (state: object, overrides: object): any =>
    p.decideKeepAlive(state, input(overrides));
  const cancelled = (state: object, reason: number, detailed: number, nowMs: number): any =>
    p.keepAliveOnCancelled(state, reason, detailed, nowMs);
  const resolved = (state: object, revision: number, ok: boolean, nowMs: number): any =>
    p.keepAliveOnStartResolved(state, revision, ok, nowMs);

  // -- the request itself -----------------------------------------------------
  const first = decided(fresh(), {});
  check('a live tunnel with the switch on asks for the borrowed task once',
    first.action === p.KEEP_ALIVE_ACTION_START &&
    first.reason === p.KEEP_ALIVE_REASON_VPN_ACTIVE && first.count === 0, JSON.stringify(first));
  check('and the returned state carries the in-flight latch (no second request next tick)',
    first.state.startInFlight === true && first.state.taskPresent === false &&
    first.state.revision === 0);
  check('a task already held is never asked for twice',
    decided(p.keepAliveWithTask(fresh(), true), {}).reason === p.KEEP_ALIVE_REASON_TASK_PRESENT);
  check('the mirror keeps the state honest in both directions',
    p.keepAliveWithTask(fresh(), true).taskPresent === true &&
    p.keepAliveWithTask(p.keepAliveWithTask(fresh(), true), false).taskPresent === false);
  check('a creation the adapter is still running blocks a second one',
    decided(first.state, {}).reason === p.KEEP_ALIVE_REASON_START_IN_FLIGHT);
  check('a VPN start still coming up blocks the request',
    decided(fresh(), { startInFlight: true }).reason === p.KEEP_ALIVE_REASON_VPN_STARTING);

  // -- the user's own instruments --------------------------------------------
  check('the switch off stops the task immediately',
    decided(fresh(), { enabled: false }).action === p.KEEP_ALIVE_ACTION_STOP &&
    decided(fresh(), { enabled: false }).reason === p.KEEP_ALIVE_REASON_DISABLED);
  check('the switch off beats every other fact (nothing keeps the task alive)',
    decided(p.keepAliveWithTask(fresh(), true), { enabled: false }).action ===
      p.KEEP_ALIVE_ACTION_STOP);
  check('a disconnect the page just asked for stops it',
    decided(fresh(), { userStopRecent: true, vpnActive: false }).reason ===
      p.KEEP_ALIVE_REASON_USER_STOP);
  check('a logout stops it',
    decided(fresh(), { loggedOut: true }).reason === p.KEEP_ALIVE_REASON_LOGOUT);
  check('no live tunnel and NO user intent stops it (0.0.28: with the intent on it is kept)',
    decided(fresh(), { vpnActive: false }).action === p.KEEP_ALIVE_ACTION_STOP &&
    decided(fresh(), { vpnActive: false }).reason === p.KEEP_ALIVE_REASON_VPN_INACTIVE);
  check('a start with no live tunnel yet is not even considered',
    decided(fresh(), { vpnActive: false, startInFlight: true }).reason ===
      p.KEEP_ALIVE_REASON_VPN_INACTIVE);

  // -- 0.0.20: the reconnect grace --------------------------------------------
  // A tunnel that was taken away gets KEEP_ALIVE_GRACE_MS of grace: the process
  // must still be running when the automatic reconnect's first attempt fires, and
  // the instant the document went stale is not yet a reason to freeze it. The
  // window is measured from the moment the tunnel was last seen active, so the
  // page's own tick cadence can neither stretch nor shorten it.
  const inactiveAt = T0 - 10000;
  const held = p.keepAliveWithTask(fresh(), true);
  const inGrace = decided(held, { vpnActive: false, vpnInactiveSinceMs: inactiveAt });
  check('a tunnel that just went inactive KEEPS the task (grace, action none)',
    inGrace.action === p.KEEP_ALIVE_ACTION_NONE &&
    inGrace.reason === p.KEEP_ALIVE_REASON_GRACE && inGrace.state.taskPresent === true,
    JSON.stringify(inGrace));
  check('... and nothing is asked for while the grace runs',
    inGrace.reason !== p.KEEP_ALIVE_REASON_VPN_ACTIVE);
  check('the grace is measured from the tunnel\'s own last active moment, not the tick',
    decided(held, { nowMs: inactiveAt + 119999, vpnActive: false,
      vpnInactiveSinceMs: inactiveAt }).reason === p.KEEP_ALIVE_REASON_GRACE);
  check('one ms past the 2 minutes the task goes, with the honest reason',
    decided(held, { nowMs: inactiveAt + 120000, vpnActive: false,
      vpnInactiveSinceMs: inactiveAt }).action === p.KEEP_ALIVE_ACTION_STOP &&
    decided(held, { nowMs: inactiveAt + 120000, vpnActive: false,
      vpnInactiveSinceMs: inactiveAt }).reason === p.KEEP_ALIVE_REASON_VPN_INACTIVE);
  check('the grace window is the documented 2 minutes',
    p.KEEP_ALIVE_GRACE_MS === 120000);
  check('a tunnel that was never seen active has NO grace (there is no reconnect to wait for)',
    decided(held, { vpnActive: false, vpnInactiveSinceMs: 0 }).action ===
      p.KEEP_ALIVE_ACTION_STOP &&
    decided(held, { vpnActive: false, vpnInactiveSinceMs: 0 }).reason ===
      p.KEEP_ALIVE_REASON_VPN_INACTIVE);
  check('a reconnect that gave up ends the grace immediately',
    decided(held, { vpnActive: false, vpnInactiveSinceMs: inactiveAt, reconnectGaveUp: true })
      .action === p.KEEP_ALIVE_ACTION_STOP &&
    decided(held, { vpnActive: false, vpnInactiveSinceMs: inactiveAt, reconnectGaveUp: true })
      .reason === p.KEEP_ALIVE_REASON_RECONNECT_GAVE_UP);
  check('the user\'s own instruments are NEVER subject to the grace',
    decided(held, { vpnActive: false, vpnInactiveSinceMs: inactiveAt, enabled: false }).reason ===
      p.KEEP_ALIVE_REASON_DISABLED &&
    decided(held, { vpnActive: false, vpnInactiveSinceMs: inactiveAt, loggedOut: true }).reason ===
      p.KEEP_ALIVE_REASON_LOGOUT &&
    decided(held, { vpnActive: false, vpnInactiveSinceMs: inactiveAt, userStopRecent: true })
      .reason === p.KEEP_ALIVE_REASON_USER_STOP);
  check('a task that is gone is not resurrected by the grace (action none, not start)',
    decided(fresh(), { vpnActive: false, vpnInactiveSinceMs: inactiveAt }).reason ===
      p.KEEP_ALIVE_REASON_GRACE &&
    decided(fresh(), { vpnActive: false, vpnInactiveSinceMs: inactiveAt }).action !==
      p.KEEP_ALIVE_ACTION_START);
  check('the grace live view says what the app is DOING, not a frozen traffic snapshot',
    p.KEEP_ALIVE_NOTIFICATION_RECONNECTING === '正在重新连接…' &&
    p.KEEP_ALIVE_NOTIFICATION_RECONNECTING !== p.KEEP_ALIVE_NOTIFICATION_TITLE);
  check('the two new reason tokens are the documented closed set',
    p.KEEP_ALIVE_REASON_GRACE === 'grace' &&
    p.KEEP_ALIVE_REASON_RECONNECT_GAVE_UP === 'reconnect-gave-up');

  // -- 0.0.28: the keep-alive follows the user's INTENT, not a 2-minute clock ---
  // v027-diag-pc REPORT §A.3 / item 10(a), HarmonyOS PC, all times CST: the
  // connector went `-> reconnecting` at 11:32:08 (netLost; the machine then slept,
  // so the extension published no document and the reader aged it out to null),
  // and the first tick after KEEP_ALIVE_GRACE_MS closed withdrew the borrowed
  // task:
  //   ui_bg_task stage=stop reason=vpn-inactive  11:48:34
  // The network came back at 19:42:36 — 8 h 10 m later. No kill happened in
  // between, so it stayed harmless; had the EXTENSION been killed in that window
  // the UI process was freezable and the tunnel unrecoverable until someone
  // foregrounded the app — the same failure class 253dca5 fixed for the restart
  // leg. Upstream Android does not do this: its VPN foreground service lives for
  // as long as the ENGINE does, and only the engine stopping (user stop /
  // needs-login) takes it down (android-client@147bb4f VPNService
  // ServiceStateListener.onStopped → stopForeground; a DISCONNECTED report in
  // between only repaints the notification). The bound is therefore the fact
  // `intentOn`, and the page owns it (pinned further down).
  const netLostAt = T0 - 986000;      // 11:32:08 → the 11:48:34 stop tick
  const reconnecting = decided(held, { vpnActive: false, intentOn: true,
    vpnInactiveSinceMs: netLostAt });
  check('a reconnecting tunnel with the user\'s intent on KEEPS the task past the grace',
    reconnecting.action === p.KEEP_ALIVE_ACTION_NONE &&
    reconnecting.reason === p.KEEP_ALIVE_REASON_GRACE &&
    reconnecting.state.taskPresent === true, JSON.stringify(reconnecting));
  check('... and the PRE-0.0.28 facts still reproduce the 11:48:34 device line',
    netLostAt === T0 - 986000 &&
    decided(held, { vpnActive: false, vpnInactiveSinceMs: netLostAt })
      .reason === p.KEEP_ALIVE_REASON_VPN_INACTIVE &&
    decided(held, { vpnActive: false, vpnInactiveSinceMs: netLostAt })
      .action === p.KEEP_ALIVE_ACTION_STOP &&
    decided(held, { vpnActive: false, intentOn: true, vpnInactiveSinceMs: netLostAt })
      .action !== p.KEEP_ALIVE_ACTION_STOP);
  check('... and 8 h 10 m later (the whole PC outage) it is STILL held',
    decided(held, { nowMs: netLostAt + 29400000, vpnActive: false, intentOn: true,
      vpnInactiveSinceMs: netLostAt }).reason === p.KEEP_ALIVE_REASON_GRACE &&
    decided(held, { nowMs: netLostAt + 29400000, vpnActive: false, intentOn: true,
      vpnInactiveSinceMs: netLostAt }).state.taskPresent === true);
  check('... with no intent-at-all stamp needed (the netLost tick may be the only tick)',
    decided(held, { vpnActive: false, intentOn: true, vpnInactiveSinceMs: 0 })
      .reason === p.KEEP_ALIVE_REASON_GRACE);
  check('a task that is gone is NOT resurrected by the intent either (no start without a live tunnel)',
    decided(fresh(), { vpnActive: false, intentOn: true, vpnInactiveSinceMs: netLostAt })
      .action === p.KEEP_ALIVE_ACTION_NONE &&
    decided(fresh(), { vpnActive: false, intentOn: true, vpnInactiveSinceMs: netLostAt })
      .action !== p.KEEP_ALIVE_ACTION_START);
  // -- the ending facts: each one is an EXPLICIT page write (pinned below) ------
  check('a terminal stop (needs-login: the page cleared the intent) releases the task',
    decided(held, { vpnActive: false, intentOn: false, vpnInactiveSinceMs: netLostAt })
      .action === p.KEEP_ALIVE_ACTION_STOP &&
    decided(held, { vpnActive: false, intentOn: false, vpnInactiveSinceMs: netLostAt })
      .reason === p.KEEP_ALIVE_REASON_VPN_INACTIVE);
  check('a reconnect that gave up beats the intent, and names itself',
    decided(held, { vpnActive: false, intentOn: true, vpnInactiveSinceMs: netLostAt,
      reconnectGaveUp: true }).action === p.KEEP_ALIVE_ACTION_STOP &&
    decided(held, { vpnActive: false, intentOn: true, vpnInactiveSinceMs: netLostAt,
      reconnectGaveUp: true }).reason === p.KEEP_ALIVE_REASON_RECONNECT_GAVE_UP);
  check('a REAL user stop releases it even with the intent still on',
    decided(held, { vpnActive: false, intentOn: true, vpnInactiveSinceMs: netLostAt,
      userStopRecent: true }).action === p.KEEP_ALIVE_ACTION_STOP &&
    decided(held, { vpnActive: false, intentOn: true, vpnInactiveSinceMs: netLostAt,
      userStopRecent: true }).reason === p.KEEP_ALIVE_REASON_USER_STOP);
  check('the switch off and a logout release it even with the intent on',
    decided(held, { vpnActive: false, intentOn: true, vpnInactiveSinceMs: netLostAt,
      enabled: false }).reason === p.KEEP_ALIVE_REASON_DISABLED &&
    decided(held, { vpnActive: false, intentOn: true, vpnInactiveSinceMs: netLostAt,
      loggedOut: true }).reason === p.KEEP_ALIVE_REASON_LOGOUT);
  check('the platform\'s verdicts still have the last word with the intent on',
    ((): boolean => {
      const gone = p.keepAliveWithTask(
        cancelled(held, p.KEEP_ALIVE_CANCEL_SYSTEM, 0, T0), false);
      const d = decided(gone, { nowMs: T0 + 1000, intentOn: true });
      return gone.taskPresent === false && d.action === p.KEEP_ALIVE_ACTION_NONE &&
        d.reason === p.KEEP_ALIVE_REASON_BACKOFF;
    })());
  check('the intent is a first-class fact of the closed fact set, documented in the policy',
    POLICY_TEXT.indexOf('intentOn: boolean;') >= 0 &&
    POLICY_TEXT.indexOf('facts.intentOn') >= 0);

  // -- 0.0.28: a kill -> automatic reconnect (stop-then-start) keeps the task ---
  // v027b device log (netbird-ui.log, 2026-10-13, all times CST): a `SwapFull`
  // kill of the extension, 1-5 s later
  //   ui_auto_reconnect count=1 reason=marker-open result=reconnected trigger=background
  // and 1-3 s after THAT
  //   ui_bg_task count=0 reason=user-stop result=ok stage=stop
  // — while the user had asked for nothing (0 `ui_disconnect` records in the whole
  // UI log). The task went, the UI process was frozen, and the NEXT kill left the
  // phone with no tunnel for 90.5 min / 28.0 min.
  //
  // The page's own restart leg fed that verdict: stopVpnExtensionThenStart()
  // wrote the USER-STOP stamp, and decideKeepAlive() answers `userStopRecent`
  // before every other fact, so the very next 2 s tick withdrew the task. The
  // sequence below drives the REAL policy with the REAL fact derivation; the page
  // pins further down prove the page now arms the restart-leg latch instead, so
  // the fact passed here is the one the page really passes.
  const killAt = T0 + 1000;
  const up = p.keepAliveWithTask(fresh(), true);   // the task is held, tunnel up
  const actions: string[] = [];
  const tick = (state: object, nowMs: number, vpnActive: boolean,
    restartLegStamp: number, inactiveSinceMs: number): any => {
    // The page's tick: `stopSent` is false (no disconnect was asked for) and the
    // stamp handed in is the one the sequence's leg armed.
    const d = decided(state, { nowMs: nowMs, vpnActive: vpnActive,
      vpnInactiveSinceMs: inactiveSinceMs,
      userStopRecent: p.keepAliveUserStopRecent(false, restartLegStamp, nowMs) });
    actions.push(d.action);
    return d;
  };
  // t0: the last healthy tick, 1 s before the kill.
  let seqState = tick(up, killAt - 1000, true, 0, 0).state;
  // t1: the kill. No fresh document, the tunnel's last-active moment is stamped
  //     here; the restart leg arms the page's OWN latch, so the USER-STOP stamp
  //     stays 0 and the grace keeps the task.
  const t1 = tick(seqState, killAt, false, 0, killAt);
  seqState = t1.state;
  // t2: the reconnect was decided and the stop-then-start leg ran. Still no
  //     document from the new instance.
  const t2 = tick(seqState, killAt + 2000, false, 0, killAt);
  seqState = t2.state;
  // t3: the new instance published an active document — the reconnect succeeded.
  const t3 = tick(seqState, killAt + 4000, true, 0, 0);
  check('the kill tick KEEPS the borrowed task (grace, never a stop)',
    t1.action === p.KEEP_ALIVE_ACTION_NONE && t1.reason === p.KEEP_ALIVE_REASON_GRACE &&
    t1.state.taskPresent === true, JSON.stringify(t1));
  check('the stop-then-start reconnect leg does not end it either',
    t2.action === p.KEEP_ALIVE_ACTION_NONE && t2.reason === p.KEEP_ALIVE_REASON_GRACE &&
    t2.state.taskPresent === true, JSON.stringify(t2));
  check('the successful reconnect keeps it, and NO step of the sequence is a stop',
    t3.action === p.KEEP_ALIVE_ACTION_NONE && t3.state.taskPresent === true &&
    actions.indexOf(p.KEEP_ALIVE_ACTION_STOP) < 0, JSON.stringify([actions, t3]));
  check('so the record the device log showed is never produced',
    t1.reason !== p.KEEP_ALIVE_REASON_USER_STOP &&
    t2.reason !== p.KEEP_ALIVE_REASON_USER_STOP &&
    t3.reason !== p.KEEP_ALIVE_REASON_USER_STOP);
  check('the OLD wiring reproduces the device line exactly (the regression is real)',
    ((): boolean => {
      // Until 0.0.28 the restart leg wrote the user-stop stamp; 2 s later that
      // stamp is still inside the 8 s guard, and the policy answers
      // STOP/user-stop on a tunnel that is UP and a task that is held.
      const oldFact = p.keepAliveUserStopRecent(false, killAt + 2000, killAt + 4000);
      const old = decided(up, { nowMs: killAt + 4000, vpnActive: true,
        userStopRecent: oldFact });
      return oldFact === true && old.action === p.KEEP_ALIVE_ACTION_STOP &&
        old.reason === p.KEEP_ALIVE_REASON_USER_STOP;
    })());
  check('the user-stop fact is true only for a REAL stop: stopSent, or a stamp < 8 s',
    p.keepAliveUserStopRecent(true, 0, T0) === true &&
    p.keepAliveUserStopRecent(false, T0 - 8000, T0) === true &&
    p.keepAliveUserStopRecent(false, T0 - 8001, T0) === false &&
    p.keepAliveUserStopRecent(false, 0, T0) === false &&
    p.KEEP_ALIVE_USER_STOP_GUARD_MS === 8000);
  check('a REAL user stop still ends it on the spot',
    decided(up, { nowMs: killAt + 4000, vpnActive: true,
      userStopRecent: p.keepAliveUserStopRecent(true, killAt + 4000, killAt + 4000) })
      .action === p.KEEP_ALIVE_ACTION_STOP &&
    decided(up, { nowMs: killAt + 4000, vpnActive: true,
      userStopRecent: p.keepAliveUserStopRecent(true, killAt + 4000, killAt + 4000) })
      .reason === p.KEEP_ALIVE_REASON_USER_STOP);
  check('a real stop that RESOLVED keeps the guard armed for exactly its 8 s',
    decided(up, { nowMs: T0 + 8000, vpnActive: true,
      userStopRecent: p.keepAliveUserStopRecent(false, T0, T0 + 8000) })
      .reason === p.KEEP_ALIVE_REASON_USER_STOP &&
    decided(up, { nowMs: T0 + 8001, vpnActive: true,
      userStopRecent: p.keepAliveUserStopRecent(false, T0, T0 + 8001) })
      .reason === p.KEEP_ALIVE_REASON_TASK_PRESENT);
  check('a platform cancellation still ends the task and is never argued with',
    ((): boolean => {
      const gone = p.keepAliveWithTask(cancelled(up, p.KEEP_ALIVE_CANCEL_SYSTEM, 0, T0), false);
      const d = decided(gone, { nowMs: T0 + 1000 });
      return gone.taskPresent === false && d.action === p.KEEP_ALIVE_ACTION_NONE &&
        d.reason === p.KEEP_ALIVE_REASON_BACKOFF;
    })());

  // -- the platform's verdicts ------------------------------------------------
  const dismissed = cancelled(fresh(), p.KEEP_ALIVE_CANCEL_USER_REMOVE_NOTIFICATION, 0, T0);
  check('removing the notification is the USER dismissing this connection',
    dismissed.suppressedForSession === true && dismissed.taskPresent === false &&
    dismissed.startInFlight === false && dismissed.cancelCount === 0 &&
    dismissed.nextRetryAtMs === 0, JSON.stringify(dismissed));
  check('a dismissed connection is never automatically re-requested',
    decided(dismissed, { nowMs: T0 + 1000000 }).action === p.KEEP_ALIVE_ACTION_NONE &&
    decided(dismissed, { nowMs: T0 + 1000000 }).reason === p.KEEP_ALIVE_REASON_SUPPRESSED);
  check('the same verdict read from either field counts (reason and detailedReason)',
    cancelled(fresh(), 3, 0, T0).suppressedForSession === true &&
    cancelled(fresh(), 2, 3, T0).suppressedForSession === true);
  check('USER_CANCEL (1) suppresses too, and a plain system cancel does not',
    cancelled(fresh(), 1, 0, T0).suppressedForSession === true &&
    cancelled(fresh(), p.KEEP_ALIVE_CANCEL_SYSTEM, 0, T0).suppressedForSession === false);
  const reconnected = p.keepAliveOnUserConnect(dismissed);
  check('the next USER connect clears the dismissal and the budget',
    reconnected.suppressedForSession === false && reconnected.cancelCount === 0 &&
    reconnected.nextRetryAtMs === 0 && reconnected.revision === dismissed.revision + 1);
  check('... and that connection gets a fresh request',
    decided(reconnected, { nowMs: T0 + 1000000 }).action === p.KEEP_ALIVE_ACTION_START);

  // -- the bounded backoff ----------------------------------------------------
  check('the backoff is 30 / 60 / 120 / 240 s, then the 5 min cap',
    p.keepAliveBackoffMs(0) === 0 && p.keepAliveBackoffMs(1) === 30000 &&
    p.keepAliveBackoffMs(2) === 60000 && p.keepAliveBackoffMs(3) === 120000 &&
    p.keepAliveBackoffMs(4) === 240000 && p.keepAliveBackoffMs(5) === 300000 &&
    p.keepAliveBackoffMs(9) === 300000,
    JSON.stringify([0, 1, 2, 3, 4, 5, 9].map((n: number): number => p.keepAliveBackoffMs(n))));
  for (const reason of [p.KEEP_ALIVE_CANCEL_LOW_SPEED, p.KEEP_ALIVE_CANCEL_NOT_UPDATE,
    p.KEEP_ALIVE_CANCEL_SYSTEM]) {
    const firstCancel = cancelled(fresh(), reason, 0, T0);
    check(`cancel reason ${reason}: one paid verdict, 30 s of backoff`,
      firstCancel.cancelCount === 1 && firstCancel.nextRetryAtMs === T0 + 30000 &&
      firstCancel.taskPresent === false, JSON.stringify(firstCancel));
    check(`cancel reason ${reason}: no request inside the backoff, one at its end`,
      decided(firstCancel, { nowMs: T0 + 29999 }).reason === p.KEEP_ALIVE_REASON_BACKOFF &&
      decided(firstCancel, { nowMs: T0 + 30000 }).action === p.KEEP_ALIVE_ACTION_START);
  }
  const secondCancel = cancelled(cancelled(fresh(), 2, 0, T0), 4, 0, T0 + 30000);
  check('the second verdict doubles the wait, measured from ITS arrival',
    secondCancel.cancelCount === 2 && secondCancel.nextRetryAtMs === T0 + 30000 + 60000);
  let storm = fresh();
  for (let i = 0; i < 5; i++) { storm = cancelled(storm, 2, 0, T0 + i * 1000); }
  check('five verdicts exhaust the session budget',
    storm.cancelCount === 5 && storm.nextRetryAtMs === T0 + 4000 + 300000);
  check('and the sixth request is refused for good, however late the tick is',
    decided(storm, { nowMs: T0 + 3600000 }).action === p.KEEP_ALIVE_ACTION_NONE &&
    decided(storm, { nowMs: T0 + 3600000 }).reason === p.KEEP_ALIVE_REASON_ATTEMPT_LIMIT);

  // -- an illegal use is terminal for the session -----------------------------
  const illegal = cancelled(fresh(), p.KEEP_ALIVE_CANCEL_ILLEGAL, 0, T0);
  check('USE_ILLEGALLY (11) stops this session',
    illegal.sessionStopped === true && illegal.taskPresent === false &&
    illegal.nextRetryAtMs === 0 && illegal.cancelCount === 0, JSON.stringify(illegal));
  check('the strongest verdict outranks the user-dismissal reading of one event',
    cancelled(fresh(), 3, 11, T0).sessionStopped === true &&
    cancelled(fresh(), 11, 3, T0).sessionStopped === true);
  check('a session stopped for an illegal type never asks again',
    decided(illegal, { nowMs: T0 + 3600000 }).action === p.KEEP_ALIVE_ACTION_NONE &&
    decided(illegal, { nowMs: T0 + 3600000 }).reason === p.KEEP_ALIVE_REASON_SESSION_STOPPED);
  const manualAfterIllegal = p.keepAliveOnUserConnect(illegal);
  check('a manual connect is NOT a new session: it does not lift an illegal verdict',
    manualAfterIllegal.sessionStopped === true &&
    decided(manualAfterIllegal, {}).action === p.KEEP_ALIVE_ACTION_NONE);
  check('a logout IS a new session: it clears every latch',
    p.keepAliveOnLogout(illegal).sessionStopped === false &&
    decided(p.keepAliveOnLogout(illegal), {}).action === p.KEEP_ALIVE_ACTION_START);

  // -- the adapter's answers --------------------------------------------------
  const started = decided(fresh(), {}).state;
  const okState = resolved(started, started.revision, true, T0 + 1200);
  check('a creation the platform accepted marks the task present and starts the throttle',
    okState.taskPresent === true && okState.startInFlight === false &&
    okState.lastRefreshMs === T0 + 1200 && okState.nextRetryAtMs === 0);
  const failedState = resolved(started, started.revision, false, T0 + 1200);
  check('a creation the platform refused is a verdict like a cancel (bounded backoff)',
    failedState.taskPresent === false && failedState.startInFlight === false &&
    failedState.cancelCount === 1 && failedState.nextRetryAtMs === T0 + 1200 + 30000);
  const afterStop = p.keepAliveOnUserStop(started);
  check('a user stop ends the session: no task, fresh budget, new identity',
    afterStop.taskPresent === false && afterStop.startInFlight === false &&
    afterStop.cancelCount === 0 && afterStop.nextRetryAtMs === 0 &&
    afterStop.revision === started.revision + 1);
  const staleAnswer = resolved(afterStop, started.revision, true, T0 + 2000);
  check('an answer that arrives after the session moved on is DISCARDED',
    staleAnswer === afterStop && staleAnswer.taskPresent === false);
  check('a stop decision invalidates an in-flight creation (revision bump)',
    decided(started, { vpnActive: false }).state.revision === started.revision + 1);
  const logoutState = p.keepAliveOnLogout(okState);
  check('a logout clears the task, the throttle and every session latch',
    logoutState.taskPresent === false && logoutState.lastRefreshMs === 0 &&
    logoutState.cancelCount === 0 && logoutState.revision === okState.revision + 1);

  // -- the refresh throttle ---------------------------------------------------
  check('no task = nothing to refresh', p.shouldRefresh(fresh(), T0) === false);
  check('a task that was never published is refreshed at once',
    p.shouldRefresh(p.keepAliveOnStartResolved(started, started.revision, true, 0), T0 + 1) === true);
  check('the live view is republished at 30 s, not before',
    p.shouldRefresh(okState, T0 + 1200 + 29999) === false &&
    p.shouldRefresh(okState, T0 + 1200 + 30000) === true);
  check('a published update restarts the throttle',
    p.keepAliveOnRefreshed(okState, T0 + 40000).lastRefreshMs === T0 + 40000 &&
    p.shouldRefresh(p.keepAliveOnRefreshed(okState, T0 + 40000), T0 + 69999) === false);
  check('the periods are the documented ones',
    p.KEEP_ALIVE_REFRESH_MS === 30000 && p.KEEP_ALIVE_MAX_ATTEMPTS === 5 &&
    p.KEEP_ALIVE_RETRY_MAX_MS === 300000 && p.KEEP_ALIVE_RETRY_BASE_MS === 30000);

  // -- the live-view text -----------------------------------------------------
  check('the live view carries the tunnel\'s own traffic, never a progress percentage',
    p.keepAliveNotificationText({ onlinePeers: 2, peerCount: 6, txBytes: 1258291,
      rxBytes: 348160 }) === '在线 2/6 · ↑1.2 MB ↓340.0 KB',
    p.keepAliveNotificationText({ onlinePeers: 2, peerCount: 6, txBytes: 1258291,
      rxBytes: 348160 }));
  check('a counter that could not be read is 未知, never 0',
    p.keepAliveNotificationText({ onlinePeers: 0, peerCount: 0, txBytes: -1, rxBytes: -1 }) ===
      '在线 0/0 · ↑未知 ↓未知' && p.keepAliveBytesText(0) === '0 B' &&
    p.keepAliveBytesText(1023) === '1023 B' && p.keepAliveBytesText(Number.NaN) === '未知');
  check('the title is the fixed, honest one',
    p.KEEP_ALIVE_NOTIFICATION_TITLE === 'NetBird VPN 已连接');

  // -- 0.0.21: the resume gap --------------------------------------------------
  // A poll pass this far from the previous one means the process was NOT RUNNING
  // (the platform suspended the whole UI process for 7362.651 s in the 0.0.20
  // device data), so the pass itself is the "resume re-check" and records it once.
  check('the resume gap is the time since the previous pass, 0 when there is none',
    p.keepAlivePollGapMs(0, T0) === 0 &&
    p.keepAlivePollGapMs(T0, T0 + 2000) === 2000 &&
    p.keepAlivePollGapMs(T0, T0 + 1000) === 1000,
    JSON.stringify([p.keepAlivePollGapMs(0, T0), p.keepAlivePollGapMs(T0, T0 + 2000)]));
  check('a clock that jumped backwards is 0, never a negative "resume"',
    p.keepAlivePollGapMs(T0, T0 - 5) === 0 && p.keepAlivePollGapMs(T0, T0) === 0);
  check('the threshold is the documented 15 s (7.5 normal ticks), strictly greater',
    p.KEEP_ALIVE_RESUME_GAP_MS === 15000 &&
    p.keepAliveResumedAfterGap(14999) === false &&
    p.keepAliveResumedAfterGap(15000) === false &&
    p.keepAliveResumedAfterGap(15001) === true);
  check('the 0.0.20 device suspension (7362.651 s) is a resume by this rule',
    p.keepAliveResumedAfterGap(7362651) === true);
  check('a resume fires ONCE: the pass writes its own stamp, the next tick is normal',
    p.keepAliveResumedAfterGap(p.keepAlivePollGapMs(T0, T0 + 7362651)) === true &&
    p.keepAliveResumedAfterGap(p.keepAlivePollGapMs(T0 + 7362651, T0 + 7362651 + 2000)) === false);
  check('the three resume reason tokens are the documented closed set',
    p.KEEP_ALIVE_RESUME_ACTIVE === 'active' && p.KEEP_ALIVE_RESUME_GAP === 'gap' &&
    p.KEEP_ALIVE_RESUME_FOREGROUND === 'foreground');

  // -- 0.0.21: the suspend message sanitizer -----------------------------------
  // The platform's `suspendMessage.message` is free text that ends up in a W-level
  // hilog line: it is reduced to printable ASCII, a token-shaped run is folded, and
  // the result is bounded. The RAW original must not survive anywhere in the output.
  const token16 = 'A1b2C3d4E5f6G7h8';
  const token15 = 'A1b2C3d4E5f6G7h';
  check('an absent or empty platform message sanitizes to the empty string',
    p.sanitizeSuspendMessage(undefined) === '' && p.sanitizeSuspendMessage('') === '',
    JSON.stringify([p.sanitizeSuspendMessage(undefined), p.sanitizeSuspendMessage('')]));
  check('printable ASCII text passes through unchanged',
    p.sanitizeSuspendMessage('system load warning: 12') === 'system load warning: 12',
    p.sanitizeSuspendMessage('system load warning: 12'));
  check('anything outside printable ASCII (controls, CJK, DEL) becomes `?`',
    p.sanitizeSuspendMessage('a\u0000b\tc\u4e2d\u6587d\u007f') === 'a?b?c??d?',
    p.sanitizeSuspendMessage('a\u0000b\tc\u4e2d\u6587d\u007f'));
  check('a 16-char token-shaped run folds to <tok>, a 15-char one is kept',
    p.sanitizeSuspendMessage(token16) === '<tok>' &&
    p.sanitizeSuspendMessage(token15) === token15 &&
    p.KEEP_ALIVE_TOKEN_MIN_CHARS === 16,
    JSON.stringify([p.sanitizeSuspendMessage(token16), p.sanitizeSuspendMessage(token15)]));
  check('the sanitized output never contains the original token',
    p.sanitizeSuspendMessage('token=' + token16) === '<tok>' &&
    p.sanitizeSuspendMessage('Bearer ' + token16 + ' end') === 'Bearer <tok> end' &&
    p.sanitizeSuspendMessage('Bearer ' + token16 + ' end').indexOf(token16) < 0,
    p.sanitizeSuspendMessage('Bearer ' + token16 + ' end'));
  check('the result is truncated to 120 characters',
    p.KEEP_ALIVE_SUSPEND_TEXT_MAX === 120 &&
    p.sanitizeSuspendMessage('y '.repeat(200)).length === 120 &&
    p.sanitizeSuspendMessage('y '.repeat(200)) === 'y '.repeat(60),
    String(p.sanitizeSuspendMessage('y '.repeat(200)).length));
} catch (error) {
  check(`harness (policy) — ${(error as Error).message}`, false);
}

// ---- B. the persistence -----------------------------------------------------

const filesDir = join(FIXTURES, 'files');
const documentPath = join(filesDir, 'netbird-keepalive.json');
const tempPath = join(filesDir, 'netbird-keepalive.json.tmp');
let store: Record<string, unknown> | null = null;
try {
  mkdirSync(filesDir, { recursive: true });
  writeFileSync(join(FIXTURES, 'NetBirdKeepAliveStore.ts'), STORE_TEXT);
  globals['__NB_KA'] = { nodeFs: nodeFs, paths: [], shortWrite: false, writeError: null };
  store = await import(pathToFileURL(join(FIXTURES, 'NetBirdKeepAliveStore.ts')).href) as
    Record<string, unknown>;
} catch (error) {
  check(`harness (store) — ${(error as Error).message}`, false);
}

if (store !== null) {
  const load = store['loadKeepAliveEnabled'] as (dir: string) => Promise<boolean>;
  const save = store['saveKeepAliveEnabled'] as (dir: string, on: boolean) => Promise<boolean>;
  const ka = globals['__NB_KA'] as { paths: string[]; shortWrite: boolean; writeError: unknown };

  check('the document name, the version and the default are the shipped literals',
    store['KEEP_ALIVE_FILE'] === 'netbird-keepalive.json' &&
    store['KEEP_ALIVE_DOCUMENT_VERSION'] === 1 &&
    store['KEEP_ALIVE_DEFAULT_ENABLED'] === true);

  check('a missing document is the default (on)', (await load(filesDir)) === true);
  check('... and reading it creates nothing',
    !existsSync(documentPath) && !existsSync(tempPath));

  writeFileSync(documentPath, '{not json');
  check('a corrupt document is the default (on)', (await load(filesDir)) === true);
  writeFileSync(documentPath, '{"v":2,"enabled":false}');
  check('an unknown version is the default (on)', (await load(filesDir)) === true);
  writeFileSync(documentPath, '{"v":1,"enabled":"no"}');
  check('a non-boolean value is the default (on)', (await load(filesDir)) === true);
  writeFileSync(documentPath, `{"v":1,"enabled":false,"pad":"${'x'.repeat(5000)}"}`);
  check('an oversized document is the default (on)', (await load(filesDir)) === true);

  ka.paths = [];
  check('the switch round-trips (off, then on)',
    (await save(filesDir, false)) === true && (await load(filesDir)) === false &&
    (await save(filesDir, true)) === true && (await load(filesDir)) === true);
  check('only this module\'s own document and its temp sibling are ever addressed',
    JSON.stringify(Array.from(new Set(ka.paths)).sort()) ===
      JSON.stringify([tempPath, documentPath].sort()), JSON.stringify(ka.paths));
  await save(filesDir, false);
  check('the written document is the versioned one and nothing else',
    readFileSync(documentPath, 'utf8') === '{"v":1,"enabled":false}',
    readFileSync(documentPath, 'utf8'));
  check('the atomic write leaves no temp sibling behind', !existsSync(tempPath));

  ka.shortWrite = true;
  check('a short write is refused and the good document survives',
    (await save(filesDir, true)) === false &&
    readFileSync(documentPath, 'utf8') === '{"v":1,"enabled":false}' && !existsSync(tempPath));
  ka.shortWrite = false;
  ka.writeError = { code: 13900012, message: 'permission denied' };
  check('an IO error is refused and the good document survives',
    (await save(filesDir, true)) === false &&
    readFileSync(documentPath, 'utf8') === '{"v":1,"enabled":false}' && !existsSync(tempPath));
  ka.writeError = null;
  check('an unusable filesDir is refused, and the load falls back to the default',
    (await save(`${filesDir}/`, true)) === false && (await load(`${filesDir}/`)) === true);
  check('the store touches no other document (no credential, no route file)',
    ka.paths.every((path: string): boolean => path === documentPath || path === tempPath));

  rmSync(documentPath, { force: true });
  check('after the document is removed the default is back', (await load(filesDir)) === true);
}

// ---- C. the wiring ----------------------------------------------------------

const allEts = etsFiles(ets);
const importersOf = (specifier: string): string[] =>
  allEts.filter((path: string): boolean => readFileSync(path, 'utf8').indexOf(specifier) >= 0);

check('the adapter is the ONLY module importing backgroundTaskManager',
  JSON.stringify(importersOf('@ohos.resourceschedule.backgroundTaskManager')) ===
    JSON.stringify([SERVICE]),
  JSON.stringify(importersOf('@ohos.resourceschedule.backgroundTaskManager')));
check('the adapter is the ONLY module importing notificationManager',
  JSON.stringify(importersOf('@ohos.notificationManager')) === JSON.stringify([SERVICE]));
check('the adapter is the ONLY module importing wantAgent',
  JSON.stringify(importersOf('@ohos.app.ability.wantAgent')) === JSON.stringify([SERVICE]));
check('the adapter starts the task with the array overload that returns the notificationId',
  SERVICE_TEXT.indexOf('startBackgroundRunning(context, [KEEP_ALIVE_BG_MODE], agent)') >= 0 &&
  SERVICE_TEXT.indexOf('task.notificationId') >= 0 &&
  SERVICE_TEXT.indexOf('task.continuousTaskId') >= 0);
check('the adapter never tries to update a DATA_TRANSFER task',
  SERVICE_TEXT.indexOf('backgroundTaskManager.updateBackgroundRunning') < 0);
check('the adapter claims an existing task before creating one',
  SERVICE_TEXT.indexOf('getAllContinuousTasks(context, true)') >= 0 &&
  SERVICE_TEXT.indexOf('await claimExistingTask(context)') <
    SERVICE_TEXT.indexOf('backgroundTaskManager.startBackgroundRunning(context'));
check('the adapter publishes the live view with the task\'s own id and the live-view slot',
  SERVICE_TEXT.indexOf('id: service.notificationId') >= 0 &&
  SERVICE_TEXT.indexOf('SlotType.LIVE_VIEW') >= 0 &&
  SERVICE_TEXT.indexOf('NOTIFICATION_CONTENT_SYSTEM_LIVE_VIEW') >= 0 &&
  SERVICE_TEXT.indexOf("'downloadTemplate'") >= 0);
check('the adapter registers all three long-task callbacks and unregisters them',
  SERVICE_TEXT.indexOf("on('continuousTaskCancel'") >= 0 &&
  SERVICE_TEXT.indexOf("on('continuousTaskSuspend'") >= 0 &&
  SERVICE_TEXT.indexOf("on('continuousTaskActive'") >= 0 &&
  SERVICE_TEXT.indexOf("off('continuousTaskCancel'") >= 0);
check('the adapter buffers cancels for the pure policy instead of deciding itself',
  SERVICE_TEXT.indexOf('takeKeepAliveCancels') >= 0 &&
  SERVICE_TEXT.indexOf('service.cancels.push(') >= 0 &&
  SERVICE_TEXT.indexOf('sessionStopped') < 0 && SERVICE_TEXT.indexOf('backoff') < 0);
check('the adapter is fail-soft: a refused creation returns false, never throws',
  /startKeepAlive[\s\S]*?catch \(error\)[\s\S]*?return false;/.test(SERVICE_TEXT));
check('the ONE record name and its closed field set',
  SERVICE_TEXT.indexOf("KEEP_ALIVE_EVENT_NAME: string = 'ui_bg_task'") >= 0 &&
  SERVICE_TEXT.indexOf("fields['stage']") >= 0 && SERVICE_TEXT.indexOf("fields['result']") >= 0 &&
  SERVICE_TEXT.indexOf("fields['reason']") >= 0 && SERVICE_TEXT.indexOf("fields['count']") >= 0 &&
  SERVICE_TEXT.indexOf(": string = 'start'") >= 0 &&
  SERVICE_TEXT.indexOf(": string = 'refresh'") >= 0 &&
  SERVICE_TEXT.indexOf(": string = 'stop'") >= 0 &&
  SERVICE_TEXT.indexOf(": string = 'cancel'") >= 0 &&
  SERVICE_TEXT.indexOf(": string = 'suspend'") >= 0 &&
  SERVICE_TEXT.indexOf(": string = 'active'") >= 0 &&
  SERVICE_TEXT.indexOf(": string = 'claim'") >= 0);
check('a repeated platform failure is recorded once per process',
  SERVICE_TEXT.indexOf('service.reported.has(key)') >= 0 &&
  SERVICE_TEXT.indexOf('service.reported.add(key)') >= 0 &&
  SERVICE_TEXT.indexOf('return;') < SERVICE_TEXT.indexOf('service.reported.add(key)'));
// 0.0.21 (the 0.0.20 record said only `stage=suspend reason=-1`): the suspend
// handler must read the fields by the names the API 26 d.ts declares, and must
// not invent one. The -1 in that device log is the platform's OWN number for a
// reason the enum (4..19) does not contain — the reader was already correct — so
// it is recorded verbatim rather than translated into a token.
const suspendBody = sliceFrom(SERVICE_TEXT, 'function onContinuousTaskSuspend(',
  'function onContinuousTaskActive(');
check('the suspend handler reads the DOCUMENTED fields of ContinuousTaskSuspendInfo',
  suspendBody.indexOf('info.continuousTaskId') >= 0 &&
  suspendBody.indexOf('info.suspendState') >= 0 &&
  suspendBody.indexOf('info.suspendReason') >= 0 &&
  suspendBody.indexOf('info.suspendMessage') >= 0 &&
  suspendBody.indexOf('const suspended: boolean = info.suspendState === true;') >= 0 &&
  !/info\.(reason|message|state|id)\b/.test(suspendBody),
  suspendBody.slice(0, 400));
check('the platform\'s own suspendReason number is recorded verbatim, `na` when absent',
  suspendBody.indexOf('const reason: number = info.suspendReason;') >= 0 &&
  suspendBody.indexOf("Number.isFinite(reason) ? String(reason) : 'na'") >= 0);
check('the suspend message never enters a record — length only, text to hilog at W',
  suspendBody.indexOf('messageChars = text.length;') >= 0 &&
  suspendBody.indexOf('UI_BG_TASK_SUSPEND_MSG') >= 0 &&
  suspendBody.indexOf('hilog.warn(DOMAIN, TAG,') >= 0);
// 0.0.21: the W-level line is the ONE place the platform text may appear, and it
// must carry the pure sanitizer's OUTPUT — never the raw original.
check('the W-level suspend line hilogs the sanitized text, never the platform original',
  suspendBody.indexOf('messageChars, sanitizeSuspendMessage(text))') >= 0 &&
  suspendBody.indexOf('messageChars, text)') < 0 &&
  /import\s*\{[^}]*sanitizeSuspendMessage[^}]*\}\s*from\s*'\.\/NetBirdKeepAlivePolicy'/
    .test(SERVICE_TEXT),
  suspendBody.slice(suspendBody.indexOf('messageChars'),
    suspendBody.indexOf('messageChars') + 360));
check('the callback record carries the task id in `count` and the boolean in `state`',
  SERVICE_TEXT.indexOf('function recordTaskCallback(stage: string, reason: string, taskId: number,') >= 0 &&
  SERVICE_TEXT.indexOf("fields['state'] = suspended ? 1 : 0;") >= 0 &&
  SERVICE_TEXT.indexOf("fields['count'] = taskId;") >= 0 &&
  SERVICE_TEXT.indexOf("fields['bytes'] = messageChars;") >= 0 &&
  suspendBody.indexOf('recordTaskCallback(suspended ? KEEP_ALIVE_STAGE_SUSPEND ' +
    ': KEEP_ALIVE_STAGE_ACTIVE,') >= 0);
const activeBody = sliceFrom(SERVICE_TEXT, 'function onContinuousTaskActive(',
  'function ensureNotificationEnabled(');
check('the active handler records the id it was given and fires the resume signal',
  activeBody.indexOf('const taskId: number = info.id;') >= 0 &&
  activeBody.indexOf("recordTaskCallback(KEEP_ALIVE_STAGE_ACTIVE, 'platform-active', taskId, false, 0);") >= 0 &&
  activeBody.indexOf('notifyKeepAliveResume(KEEP_ALIVE_RESUME_ACTIVE);') >= 0);
check('the resume channel is one fail-soft listener owned by the page',
  SERVICE_TEXT.indexOf('export type KeepAliveResumeListener = (reason: string) => void;') >= 0 &&
  SERVICE_TEXT.indexOf('export function setKeepAliveResumeListener(') >= 0 &&
  SERVICE_TEXT.indexOf('export function notifyKeepAliveResume(reason: string): void {') >= 0 &&
  SERVICE_TEXT.indexOf('UI_BG_TASK_RESUME_LISTENER_FAILED') >= 0);
check('onForeground hands the SAME signal in with its own closed token',
  ABILITY.indexOf('notifyKeepAliveResume(KEEP_ALIVE_RESUME_FOREGROUND);') >= 0 &&
  ABILITY.indexOf('KEEP_ALIVE_RESUME_FOREGROUND') >= 0 &&
  sliceFrom(ABILITY, 'onForeground(): void {', 'onBackground(): void {')
    .indexOf('this.recordLifecycle(\'onForeground\');') >= 0);

const pollBody = sliceFrom(PAGE, 'private pollConnectorStatus(): void {',
  'private refreshConnectFailure(): void {');
check('the page rides the EXISTING tick: the sync is called from the poll',
  pollBody.indexOf('this.syncKeepAlive(live);') >= 0);
check('the sync has exactly TWO call sites (the tick and the settings switch)',
  PAGE.split('this.syncKeepAlive(').length === 3);
check('the page still has exactly ONE timer (no second ticker was added)',
  PAGE.split('setInterval(').length === 2);
check('the page still has exactly ONE extension-start call site',
  PAGE.split('startVpnExtensionAbility(').length === 2);
check('the page imports the pure policy, the adapter and the store',
  PAGE.indexOf("'../vpnextensionability/NetBirdKeepAlivePolicy'") >= 0 &&
  PAGE.indexOf("'../vpnextensionability/NetBirdKeepAliveService'") >= 0 &&
  PAGE.indexOf("'../vpnextensionability/NetBirdKeepAliveStore'") >= 0);
// 0.0.21: the resume re-check. The page owns the ONE clock (lastPollAtMs) and the
// threshold itself lives in the pure policy, where the boundary and the "fires
// once" property are checked at exact milliseconds above.
const pollEntry = sliceFrom(PAGE, 'private pollConnectorStatus(): void {',
  'private onResumeSignal(reason: string): void {');
const resumeSignalBody = sliceFrom(PAGE, 'private onResumeSignal(reason: string): void {',
  'private diagResumeCheck(');
const resumeDiagBody = sliceFrom(PAGE, 'private diagResumeCheck(reason: string, gapMs: number): void {',
  'private pollConnectorStatusPass()');
check('the poll is single-flight: a second pass never overlaps a running one',
  pollEntry.indexOf('if (this.pollRunning) {') >= 0 &&
  pollEntry.indexOf('this.pollRunning = true;') >= 0 &&
  pollEntry.indexOf('try {') >= 0 && pollEntry.indexOf('} finally {') >= 0 &&
  pollEntry.indexOf('this.pollConnectorStatusPass();') >= 0 &&
  pollEntry.indexOf('this.pollConnectorStatusPass();') <
    pollEntry.indexOf('this.pollRunning = false;') &&
  PAGE.split('this.pollConnectorStatusPass();').length === 2);
check('a resume signal runs the EXISTING pass on the spot (no new timer, no second read)',
  resumeSignalBody.indexOf('if (this.pollRunning) {') >= 0 &&
  resumeSignalBody.indexOf('this.lastPollAtMs <= 0') >= 0 &&
  resumeSignalBody.indexOf('this.pendingResumeReason = reason;') >= 0 &&
  resumeSignalBody.indexOf('this.pollConnectorStatus();') >= 0 &&
  resumeSignalBody.indexOf('setInterval(') < 0);
check('the page registers the ONE resume listener and removes it when it goes away',
  PAGE.indexOf('setKeepAliveResumeListener((reason: string): void => {') >= 0 &&
  PAGE.indexOf('this.onResumeSignal(reason);') >= 0 &&
  PAGE.indexOf('setKeepAliveResumeListener(undefined);') >= 0);
check('the pass measures its own gap and records the resume exactly once',
  pollBody.indexOf('const gapMs: number = keepAlivePollGapMs(this.lastPollAtMs, nowMs);') >= 0 &&
  pollBody.indexOf('this.lastPollAtMs = nowMs;') >= 0 &&
  pollBody.indexOf('if (keepAliveResumedAfterGap(gapMs)) {') >= 0 &&
  pollBody.indexOf('resumeReason.length > 0 ? resumeReason : KEEP_ALIVE_RESUME_GAP') >= 0 &&
  PAGE.indexOf('private lastPollAtMs: number = 0;') >= 0 &&
  PAGE.indexOf('private pollRunning: boolean = false;') >= 0);
check('the resume record is `ui_resume_check` on the public key set, in seconds',
  resumeDiagBody.indexOf("event('ui_resume_check', fields)") >= 0 &&
  resumeDiagBody.indexOf("fields['reason'] = reason;") >= 0 &&
  resumeDiagBody.indexOf("fields['count'] = Math.round(gapMs / 1000);") >= 0);
check('the follow-up re-checks inside 10 s ride the EXISTING 2 s tick',
  pollBody.indexOf('页面自己的 2 s 轮询在 10 s 内就是 5 拍') >= 0 &&
  PAGE.split('setInterval(').length === 2);
const syncBody = sliceFrom(PAGE, 'private syncKeepAlive(status: PublishedStatus | null): void {',
  'private stopKeepAliveNow(reason: string): void {');
check('the decision is the pure policy\'s, over facts the tick already has',
  syncBody.indexOf('decideKeepAlive(this.bgKeepAliveState, facts)') >= 0 &&
  syncBody.indexOf('takeKeepAliveCancels()') >= 0 &&
  syncBody.indexOf('keepAliveTaskPresent()') >= 0 &&
  syncBody.indexOf('vpnActiveState(status.state)') >= 0 &&
  syncBody.indexOf('this.bgKeepAliveEnabled') >= 0);
check('the live-view body comes from the SAME document, never a second read',
  syncBody.indexOf('keepAliveStatusOf(status)') >= 0 &&
  syncBody.indexOf('readPublishedStatus') < 0 && syncBody.indexOf('fs.') < 0);
check('the refresh is throttled by the pure policy, not by the adapter',
  syncBody.indexOf('shouldRefresh(this.bgKeepAliveState, nowMs)') >= 0 &&
  syncBody.indexOf('keepAliveOnRefreshed(') >= 0);
// 0.0.20: the grace needs two facts from OTHER state machines — when the tunnel
// went inactive (the page's own clock) and whether the automatic reconnect has
// spent its budget — and it republishes the live view with the policy's
// "正在重新连接…" body when there is no fresh document left to render.
check('the page maintains the tunnel\'s last-active moment and hands it over as a fact',
  syncBody.indexOf('this.vpnInactiveSinceMs = nowMs;') >= 0 &&
  syncBody.indexOf('vpnInactiveSinceMs: this.vpnInactiveSinceMs') >= 0 &&
  PAGE.indexOf('private vpnInactiveSinceMs: number = 0;') >= 0 &&
  PAGE.indexOf('private vpnActiveSeen: boolean = false;') >= 0);
check('the reconnect budget is handed over as a fact (not re-decided here)',
  syncBody.indexOf('reconnectGaveUp: this.reconnectState.attempts >= RECONNECT_MAX_ATTEMPTS') >= 0 &&
  PAGE.indexOf("'../vpnextensionability/NetBirdReconnectPolicy'") >= 0);
// 0.0.28: the keep-alive's bound is the USER'S INTENT, and the page owns that
// latch. It is SET in exactly one place — a live ACTIVE document, i.e. a tunnel
// really ran in this process — and CLEARED in exactly five: the engine's own
// DEAD state, a connect-failure verdict (the VPN never came up; needs-login is
// one of its classes), a real user stop, a logout, and the switch going off.
// The counts are pinned so a new writer cannot appear unnoticed: `= true` twice
// (the declaration is `vpnIntentOn: boolean = false;`, which does not match) and
// `= false` six times.
const refreshFailureBody = sliceFrom(PAGE, 'private refreshConnectFailure(): void {',
  'private notifyFailureOnce(');
const requestStopBody = sliceFrom(PAGE, 'private requestStop(): void {',
  '// ---- 诊断日志导出');
const performLogoutBody = sliceFrom(PAGE, 'private performLogout(): void {',
  'private writeLogoutTicket(');
const toggleBodyEarly = sliceFrom(PAGE, 'private onKeepAliveToggle(isOn: boolean): void {',
  '// ---- derived view state');
check('the page hands the user\'s VPN-on intent over as the policy\'s fact',
  syncBody.indexOf('intentOn: this.vpnIntentOn') >= 0 &&
  pollBody.indexOf('this.vpnIntentOn = true;') >= 0 &&
  PAGE.indexOf('private vpnIntentOn: boolean = false;') >= 0);
check('... set by ONE fact (a live tunnel) and cleared by exactly the five that end the intent',
  PAGE.split('this.vpnIntentOn = true;').length === 2 &&
  PAGE.split('this.vpnIntentOn = false;').length === 6 &&
  pollBody.indexOf('this.vpnIntentOn = false;') >= 0 &&
  refreshFailureBody.indexOf('this.vpnIntentOn = false;') >= 0 &&
  requestStopBody.indexOf('this.vpnIntentOn = false;') >= 0 &&
  performLogoutBody.indexOf('this.vpnIntentOn = false;') >= 0 &&
  toggleBodyEarly.indexOf('this.vpnIntentOn = false;') >= 0,
  String([PAGE.split('this.vpnIntentOn = true;').length,
    PAGE.split('this.vpnIntentOn = false;').length]));
check('the grace republishes the live view with the reconnecting body',
  syncBody.indexOf('refreshKeepAliveText(') >= 0 &&
  syncBody.indexOf('KEEP_ALIVE_NOTIFICATION_RECONNECTING') >= 0 &&
  syncBody.indexOf('decision.reason === KEEP_ALIVE_REASON_GRACE') >= 0 &&
  SERVICE_TEXT.indexOf('export async function refreshKeepAliveText(') >= 0 &&
  SERVICE_TEXT.indexOf('return await publishLiveView(context, text, reason);') >= 0);
check('the adapter owns no notification text of its own (the caller builds the body)',
  SERVICE_TEXT.indexOf('publishLiveView(context: common.UIAbilityContext, body: string,') >= 0 &&
  SERVICE_TEXT.indexOf('keepAliveNotificationText(status)') >= 0 &&
  SERVICE_TEXT.indexOf("'正在重新连接…'") < 0);
check('the start answer is applied under its own revision (a stale one cannot resurrect a task)',
  syncBody.indexOf('keepAliveOnStartResolved(this.bgKeepAliveState, revision, ok,') >= 0 &&
  syncBody.indexOf('const revision: number = this.bgKeepAliveState.revision;') >= 0);
check('a manual connect and a disconnect are the policy transitions they claim to be',
  PAGE.indexOf('keepAliveOnUserConnect(this.bgKeepAliveState)') >= 0 &&
  PAGE.indexOf('keepAliveOnUserStop(this.bgKeepAliveState)') >= 0 &&
  PAGE.indexOf('keepAliveOnLogout(this.bgKeepAliveState)') >= 0);
// 0.0.28: the page's own stop-then-start re-entry leg is NOT a user stop. It arms
// the restart-leg latch; the ONLY writer of the user-stop latch is the real
// disconnect leg (requestStop), and the keep-alive fact is the pure policy's
// derivation rather than an inline expression the host cannot drive.
const restartLegBody = sliceFrom(PAGE,
  'private stopVpnExtensionThenStart(fromAutoReconnect: boolean): void {',
  'private async waitExtensionGoneThenStart(');
check('the stop-then-start re-entry leg arms the RESTART-LEG latch, never the user-stop one',
  restartLegBody.indexOf('this.lastRestartLegAtMs = Date.now();') >= 0 &&
  restartLegBody.indexOf('this.lastUserStopAtMs') < 0, restartLegBody.replace(/\n/g, ' | '));
check('the real disconnect leg is the only writer of the user-stop latch',
  PAGE.split('this.lastUserStopAtMs = ').length === 2 &&
  sliceFrom(PAGE, 'private requestStop(): void {', '// ---- 诊断日志导出')
    .indexOf('this.lastUserStopAtMs = Date.now();') >= 0);
check('the disconnect notice reads the two latches as the two different facts they are',
  pollBody.indexOf('Date.now() - this.lastUserStopAtMs > 8000') >= 0 &&
  pollBody.indexOf('Date.now() - this.lastRestartLegAtMs > 8000') >= 0 &&
  PAGE.indexOf('private lastRestartLegAtMs: number = 0;') >= 0);
check('the keep-alive user-stop fact is the pure policy\'s derivation, not an inline expression',
  syncBody.indexOf('userStopRecent: keepAliveUserStopRecent(this.stopSent, ' +
    'this.lastUserStopAtMs, nowMs)') >= 0 &&
  syncBody.indexOf('this.stopSent || nowMs - this.lastUserStopAtMs <= 8000') < 0 &&
  POLICY_TEXT.indexOf('export function keepAliveUserStopRecent(') >= 0);
const toggleBody = sliceFrom(PAGE, 'private onKeepAliveToggle(isOn: boolean): void {',
  '// ---- derived view state');
check('the settings page carries the switch and its one-line explanation',
  PAGE.indexOf('private onKeepAliveToggle(isOn: boolean): void {') >= 0 &&
  PAGE.indexOf('@Builder\n  keepAliveCard()') >= 0 &&
  PAGE.indexOf('通过系统长时任务保持 VPN 在后台运行，通知栏会常驻流量通知') >= 0 &&
  PAGE.indexOf('Toggle({ type: ToggleType.Switch, isOn: this.bgKeepAliveEnabled })') >= 0);
check('the switch writes through at once and acts on the spot',
  toggleBody.indexOf('this.syncKeepAlive(this.published);') >= 0 &&
  toggleBody.indexOf('saveKeepAliveEnabled(') >= 0);
check('a re-emitted switch value cannot flip anything (act on a REAL change only)',
  sliceFrom(PAGE, '@Builder\n  keepAliveCard()', 'build() {')
    .indexOf('if (isOn !== this.bgKeepAliveEnabled) {') >= 0);

const createBody = sliceFrom(ABILITY, 'onCreate(want: Want, launchParam',
  'private recordLifecycle(stage: string): void {');
const destroyBody = sliceFrom(ABILITY, 'onDestroy(): void {',
  'private persistCredentialOverrides');
check('the ability registers after the diagnostics writer is open',
  createBody.indexOf("startDiagnostics(this.context.filesDir, 'ui')") >= 0 &&
  createBody.indexOf('registerKeepAlive(this.context)') >
    createBody.indexOf("startDiagnostics(this.context.filesDir, 'ui')"));
check('onDestroy unregisters and stops the borrowed task before the writer closes',
  destroyBody.indexOf('unregisterKeepAlive();') >= 0 &&
  destroyBody.indexOf("stopKeepAlive(this.context, KEEP_ALIVE_REVISION_SHUTDOWN, 'shutdown')") >= 0 &&
  destroyBody.indexOf('unregisterKeepAlive();') < destroyBody.indexOf('stopKeepAlive(') &&
  destroyBody.indexOf('stopKeepAlive(') < destroyBody.indexOf('stopDiagnostics();'));

check('module.json5 declares the borrowed background mode on the UIAbility',
  /name: 'EntryAbility'[\s\S]*?backgroundModes: \[\s*'dataTransfer'\s*\]/.test(MODULE_JSON5));
check('module.json5 asks for the continuous-task permission, inside requestPermissions',
  MODULE_JSON5.indexOf("name: 'ohos.permission.KEEP_BACKGROUND_RUNNING'") >= 0 &&
  MODULE_JSON5.indexOf('requestPermissions') <
    MODULE_JSON5.indexOf('KEEP_BACKGROUND_RUNNING'));
check('the version is 28 / 0.0.28 in both profiles',
  /versionCode: 28/.test(BUILD_PROFILE) && /versionName: '0\.0\.28'/.test(BUILD_PROFILE) &&
  /versionCode: 28/.test(APP_JSON5) && /versionName: '0\.0\.28'/.test(APP_JSON5) &&
  BUILD_PROFILE.indexOf("'0.0.27'") < 0 && APP_JSON5.indexOf("'0.0.27'") < 0);

// ---- D. the platform callbacks and the resume signal (0.0.21) --------------
//
// The REAL adapter is staged byte-for-byte next to a byte-identical copy of the
// pure policy and a recording `./NetBirdDiagnostics` seam, and driven through the
// callback objects its own register keepAlive() handed to the platform stub. The
// callback bodies under test are the shipped ones — no copy, no rewrite.

const SERVICE_FIXTURES = mkdtempSync(join(tmpdir(), 'nb-ka-service-'));
const DIAG_STUB = [
  'export function event(name, fields) {',
  '  const s = globalThis.__NB_KA;',
  '  s.records.push({ name: name, fields: fields === undefined ? {} : fields });',
  '  return true;',
  '}',
  ''
].join('\n');
try {
  writeFileSync(join(SERVICE_FIXTURES, 'NetBirdKeepAlivePolicy.ts'), POLICY_TEXT);
  writeFileSync(join(SERVICE_FIXTURES, 'NetBirdDiagnostics.ts'), DIAG_STUB);
  writeFileSync(join(SERVICE_FIXTURES, 'NetBirdKeepAliveService.ts'), SERVICE_TEXT);
  globals['__NB_KA'] = {
    nodeFs: nodeFs, paths: [], shortWrite: false, writeError: null,
    records: [], hilog: [], btm: {}, btmTasks: [], btmStopped: 0,
    btmStartResult: { notificationId: 5, continuousTaskId: 3 }, published: []
  };
  const svc = await import(
    pathToFileURL(join(SERVICE_FIXTURES, 'NetBirdKeepAliveService.ts')).href) as
    Record<string, unknown>;
  const ka = globals['__NB_KA'] as {
    records: { name: string; fields: Record<string, unknown> }[];
    hilog: { level: string; tag: string; format: string; args: unknown[] }[];
    btm: Record<string, (info: Record<string, unknown>) => void>;
  };
  const emit = (type: string, info: Record<string, unknown>): void => {
    const callback = ka.btm[type];
    if (callback === undefined) {
      throw new Error(`the adapter never registered a '${type}' callback`);
    }
    callback(info);
  };
  svc['registerKeepAlive']({ filesDir: SERVICE_FIXTURES });

  check('the adapter registers all three callbacks with the platform',
    ka.btm['continuousTaskCancel'] !== undefined &&
    ka.btm['continuousTaskSuspend'] !== undefined &&
    ka.btm['continuousTaskActive'] !== undefined);

  // The parent's stub, verbatim: {continuousTaskId:7, suspendState:true,
  // suspendReason:12, suspendMessage:'x'} — plus the real API 26 shape.
  emit('continuousTaskSuspend',
    { continuousTaskId: 7, suspendState: true, suspendReason: 12,
      suspendMessage: { message: 'x', reason: 12 } });
  let last = ka.records[ka.records.length - 1];
  check('a suspension is mapped field for field into ONE ui_bg_task record',
    last.name === 'ui_bg_task' && last.fields['stage'] === 'suspend' &&
    last.fields['result'] === 'ok' && last.fields['state'] === 1 &&
    last.fields['reason'] === '12' && last.fields['count'] === 7 &&
    last.fields['bytes'] === 1,
    JSON.stringify(last));
  check('the message TEXT is never written into a record, and is hilog-ed at W level',
    JSON.stringify(last.fields).indexOf('x') < 0 &&
    ka.hilog.some((line): boolean => line.level === 'warn' &&
      line.format.indexOf('UI_BG_TASK_SUSPEND_MSG') >= 0 &&
      line.args.indexOf('x') >= 0),
    JSON.stringify(ka.hilog));
  // 0.0.21: the SAME line is the sanitizer's output, not the platform's bytes: the
  // token-shaped run is folded, the raw original is gone, `bytes` stays the RAW length.
  check('the W-level line carries the sanitized text and keeps the raw length in `bytes`',
    ((): boolean => {
      const token = 'A1b2C3d4E5f6G7h8';
      emit('continuousTaskSuspend',
        { continuousTaskId: 7, suspendState: true, suspendReason: 12,
          suspendMessage: { message: 'token=' + token, reason: 12 } });
      const lines = ka.hilog.filter((line): boolean =>
        line.format.indexOf('UI_BG_TASK_SUSPEND_MSG') >= 0);
      const warn = lines[lines.length - 1];
      const record = ka.records[ka.records.length - 1];
      // `token=` is itself part of the token-shaped run, so the whole run folds.
      return warn !== undefined && warn.args.indexOf('<tok>') >= 0 &&
        warn.args.indexOf(token) < 0 &&
        record.fields['bytes'] === 'token='.length + token.length;
    })(),
    JSON.stringify(ka.hilog.slice(-2)));
  check('the platform\'s own reason number is kept verbatim (the 0.0.20 `-1` too)',
    ((): boolean => {
      emit('continuousTaskSuspend',
        { continuousTaskId: 7, suspendState: true, suspendReason: -1 });
      const record = ka.records[ka.records.length - 1];
      return record.fields['reason'] === '-1' && record.fields['bytes'] === 0;
    })());
  check('a missing reason is `na`, never silently turned into a number',
    ((): boolean => {
      emit('continuousTaskSuspend',
        { continuousTaskId: 7, suspendState: true, suspendMessage: 'not the SDK shape' });
      const record = ka.records[ka.records.length - 1];
      return record.fields['reason'] === 'na' && record.fields['bytes'] === 0;
    })());

  let resumed: string[] = [];
  svc['setKeepAliveResumeListener']((reason: string): void => { resumed.push(reason); });
  emit('continuousTaskActive', { id: 7 });
  last = ka.records[ka.records.length - 1];
  check('an activation is recorded with its task id and fires the resume signal',
    last.fields['stage'] === 'active' && last.fields['state'] === 0 &&
    last.fields['reason'] === 'platform-active' && last.fields['count'] === 7 &&
    resumed.length === 1 && resumed[0] === 'active', JSON.stringify([last, resumed]));
  check('a suspend callback that reports "activated again" fires it too',
    ((): boolean => {
      emit('continuousTaskSuspend',
        { continuousTaskId: 7, suspendState: false, suspendReason: 12 });
      return ka.records[ka.records.length - 1].fields['stage'] === 'active' &&
        resumed.length === 2;
    })());
  check('a THROWING listener cannot escape into the platform callback (fail-soft)',
    ((): boolean => {
      svc['setKeepAliveResumeListener']((): void => { throw new Error('boom'); });
      emit('continuousTaskActive', { id: 7 });
      return ka.records[ka.records.length - 1].fields['stage'] === 'active' &&
        ka.hilog.some((line): boolean =>
          line.format.indexOf('UI_BG_TASK_RESUME_LISTENER_FAILED') >= 0);
    })());
  check('with no listener the signal is a no-op (the page may not exist yet)',
    ((): boolean => {
      svc['setKeepAliveResumeListener'](undefined);
      const before = ka.records.length;
      emit('continuousTaskActive', { id: 7 });
      return ka.records.length === before + 1;
    })());
  check('only THIS app\'s task is answered: a foreign task id is dropped',
    await (async (): Promise<boolean> => {
      // Adopt a task (id 3) through the REAL start path, then emit for id 9.
      const startKeepAlive = svc['startKeepAlive'] as (context: object, revision: number,
        reason: string, status: object) => Promise<boolean>;
      const started: boolean = await startKeepAlive({ filesDir: SERVICE_FIXTURES }, 0,
        'vpn-active', { onlinePeers: 0, peerCount: 0, txBytes: -1, rxBytes: -1 });
      const before = ka.records.length;
      emit('continuousTaskSuspend',
        { continuousTaskId: 9, suspendState: true, suspendReason: 12 });
      return started === true && ka.records.length === before;
    })(),
    'the claim path or the foreign-task filter did not behave');
} catch (error) {
  check(`harness (service) — ${(error as Error).message}`, false);
}
rmSync(SERVICE_FIXTURES, { recursive: true, force: true });

rmSync(FIXTURES, { recursive: true, force: true });

console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
