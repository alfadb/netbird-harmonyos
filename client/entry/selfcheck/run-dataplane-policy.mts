// 0.0.54 data-plane watchdog — host-side checks, no device, no build.
//
// WHY: the 2026-10-21 PC capture (artifacts/incident-20261021-pc-peers0/pc/
// REPORT.md) shows `connector: state -> connected` while peers_online stayed 0
// for 2235 s / 830 s / 4816 s after a lid-close/modern-standby wake. W2/W4 ran
// the 0.0.50 recovery leg but signal did not register and ICE was never
// rebuilt; W3 (16:17–16:31) woke with NO network-change callback at all, so the
// recovery leg never ran. A manual disconnect+connect (a FULL new start) healed
// it every time.
//
// The decision is a zero-import pure module; this harness copies it
// BYTE-IDENTICALLY into an OS-temp fixture and runs the truth table against the
// shipped code. The second half pins the WIRING in the real extension (the 5 s
// watcher tick keeps the clocks, the same status document feeds the policy, and
// a due recovery goes through the EXISTING recoverPhysicalNetwork).
//
// Run: node client/entry/selfcheck/run-dataplane-policy.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_DIR = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability');
const SOURCE = join(REAL_DIR, 'NetBirdDataPlanePolicy.ets');
const EXT = join(REAL_DIR, 'NetBirdVpnExtensionAbility.ets');

let passed: number = 0;
let failedCount: number = 0;
function check(name: string, condition: boolean, detail?: string): void {
  if (condition) {
    passed++;
    console.log('ok   ' + name);
  } else {
    failedCount++;
    console.log('FAIL ' + name + (detail !== undefined ? ' — ' + detail : ''));
  }
}

const NOW = 1800000000000;
// Healthy: connected, signal registered, 13 peers with live WG sessions.
function healthy(): Record<string, unknown> {
  return {
    running: true, state: 'connected', signalRegistered: true,
    peerCount: 13, wgPeersWithSession: 13, recoveryActive: false,
    staleSinceMs: 0, lastRecoveryMs: 0, nowMs: NOW
  };
}
// W4 shape: connected, signal never registered, no peer session.
function noSignal(): Record<string, unknown> {
  return { ...healthy(), signalRegistered: false, wgPeersWithSession: 0 };
}
// Registered signal, peers configured, but no WG session (session-expired tail).
function noSession(): Record<string, unknown> {
  return { ...healthy(), wgPeersWithSession: 0 };
}

try {
  const dir = mkdtempSync(join(tmpdir(), 'nb-dataplane-policy-'));
  const fixture = join(dir, 'NetBirdDataPlanePolicy.ts');
  writeFileSync(fixture, readFileSync(SOURCE));
  const m = await import(pathToFileURL(fixture).href) as Record<string, any>;

  const stale = (f: Record<string, unknown>): boolean => m.dataPlaneStale(f);
  const next = (f: Record<string, unknown>): number => m.nextStaleSince(f);
  const recover = (f: Record<string, unknown>, after?: number, gap?: number): boolean =>
    after === undefined && gap === undefined ? m.shouldRecover(f) :
      gap === undefined ? m.shouldRecover(f, after) : m.shouldRecover(f, after, gap);
  const reason = (f: Record<string, unknown>): string => m.dataPlaneStaleReason(f);

  // -------------------------------------------------------------------------
  // A. Constants: the calibration the extension relies on.
  // -------------------------------------------------------------------------
  check('A1 stale floor is 30 s (upstream signal watchdog silence floor)',
    m.DATA_PLANE_STALE_AFTER_MS === 30000);
  check('A2 watchdog recovery floor is 60 s', m.DATA_PLANE_RECOVER_MIN_GAP_MS === 60000);
  check('A3 suspend/wake gap is 15 s', m.DATA_PLANE_RESUME_GAP_MS === 15000);

  // -------------------------------------------------------------------------
  // B. dataPlaneStale: the closed definition.
  // -------------------------------------------------------------------------
  check('B1 connected + signal registered + sessions flowing is NOT stale',
    stale(healthy()) === false && reason(healthy()) === '');
  check('B2 connected + signal NOT registered is stale (no-signal)',
    stale(noSignal()) === true && reason(noSignal()) === 'no-signal');
  check('B3 connected + signal registered + peers>0 + no WG session is stale (no-session)',
    stale(noSession()) === true && reason(noSession()) === 'no-session');
  check('B4 peers=0 is NOT stale on the session clause (nothing to session with)',
    stale({ ...healthy(), peerCount: 0, wgPeersWithSession: 0 }) === false &&
    reason({ ...healthy(), peerCount: 0, wgPeersWithSession: 0 }) === '');
  check('B5 the signal clause does not depend on peer_count',
    stale({ ...healthy(), peerCount: 0, signalRegistered: false }) === true);
  check('B6 an unreadable WG section (-1) never satisfies the session clause',
    stale({ ...healthy(), wgPeersWithSession: -1 }) === false);
  check('B7 a non-connected state is never stale, even with no signal',
    stale({ ...noSignal(), state: 'connecting' }) === false &&
    stale({ ...noSignal(), state: 'disconnected' }) === false &&
    stale({ ...noSignal(), state: 'reconnecting' }) === false &&
    stale({ ...noSignal(), state: 'failed' }) === false);
  check('B8 a not-running connector is never stale, even in state connected',
    stale({ ...noSignal(), running: false }) === false);
  check('B9 stale states are exactly connected+running (the closed set)',
    ['connected', 'connecting', 'disconnected', 'reconnecting', 'failed', '']
      .every((s: string): boolean => {
        const f: Record<string, unknown> = { ...noSignal(), state: s };
        return stale(f) === (s === 'connected');
      }));

  // -------------------------------------------------------------------------
  // C. nextStaleSince: keep the start, set it, or clear it.
  // -------------------------------------------------------------------------
  check('C1 a fresh stale observation starts the clock at now',
    next({ ...noSignal(), staleSinceMs: 0 }) === NOW);
  check('C2 an ongoing stale run KEEPS its original start',
    next({ ...noSignal(), staleSinceMs: NOW - 12000 }) === NOW - 12000);
  check('C3 a healthy observation clears the clock to 0',
    next(healthy()) === 0 &&
    next({ ...noSignal(), state: 'disconnected', staleSinceMs: NOW - 12000 }) === 0);
  check('C4 a stale start in the future (wall clock moved) is restarted at now',
    next({ ...noSignal(), staleSinceMs: NOW + 5000 }) === NOW);
  check('C5 an unreadable nowMs is never trusted',
    next({ ...noSignal(), nowMs: NaN }) === 0 &&
    next({ ...noSignal(), nowMs: 0 }) === 0);

  // -------------------------------------------------------------------------
  // D. shouldRecover: stale + persisted + not active + outside the gap.
  // -------------------------------------------------------------------------
  const dueStale = (over: Record<string, unknown> = {}): Record<string, unknown> =>
    ({ ...noSignal(), staleSinceMs: NOW - 30000, ...over });
  check('D1 a healthy data plane never recovers',
    recover(healthy()) === false &&
    recover({ ...healthy(), staleSinceMs: NOW - 3600000 }) === false);
  check('D2 stale for 29999 ms does NOT recover yet',
    recover(dueStale({ staleSinceMs: NOW - 29999 })) === false);
  check('D3 stale for exactly 30000 ms recovers (>= the floor)',
    recover(dueStale()) === true);
  check('D4 a stale observation without a known start never recovers',
    recover(dueStale({ staleSinceMs: 0 })) === false);
  check('D5 recoveryActive refuses to start a second attempt',
    recover(dueStale({ recoveryActive: true })) === false);
  check('D6 59999 ms after the last watchdog recovery is still the gap',
    recover(dueStale({ lastRecoveryMs: NOW - 59999 })) === false);
  check('D7 exactly 60000 ms after the last watchdog recovery recovers',
    recover(dueStale({ lastRecoveryMs: NOW - 60000 })) === true);
  check('D8 a last recovery in the future (clock moved) stays inside the gap',
    recover(dueStale({ lastRecoveryMs: NOW + 1000 })) === false);
  check('D9 an unreadable nowMs never recovers',
    recover(dueStale({ nowMs: NaN })) === false && recover(dueStale({ nowMs: 0 })) === false);
  check('D10 the thresholds are the shipped defaults (and overridable for tests)',
    recover(dueStale({ staleSinceMs: NOW - 1000 }), 1000) === true &&
    recover(dueStale({ staleSinceMs: NOW - 70000, lastRecoveryMs: NOW - 70000 }), 30000, 120000)
      === false);
  check('D11 a user stop (disconnected) is refused even when stale facts are passed',
    recover({ ...dueStale(), state: 'disconnected' }) === false &&
    recover({ ...dueStale(), running: false }) === false);

  // -------------------------------------------------------------------------
  // E. resumeGapDetected: the suspend/wake edge, strict boundary.
  // -------------------------------------------------------------------------
  check('E1 15000 ms is a slow tick, not a wake', m.resumeGapDetected(15000) === false);
  check('E2 15001 ms is a wake', m.resumeGapDetected(15001) === true);
  check('E3 a normal 5 s tick and an unreadable gap are not wakes',
    m.resumeGapDetected(5000) === false && m.resumeGapDetected(0) === false &&
    m.resumeGapDetected(NaN) === false && m.resumeGapDetected(-1) === false);
  check('E4 a multi-minute freeze (the 599 s starvation in the capture) is a wake',
    m.resumeGapDetected(599000) === true);

  // -------------------------------------------------------------------------
  // F. The module is PURE (so the host fixture is the shipped code).
  // -------------------------------------------------------------------------
  const policyText = readFileSync(SOURCE, 'utf8');
  check('F1 the policy has no @ohos / platform / sibling import',
    !/^\s*import\s/m.test(policyText) && !policyText.includes('@ohos.') &&
    !policyText.includes("from './") && !policyText.includes('libnetbird_core'));
  check('F2 the reasons are the closed set the milestone writes',
    ['no-signal', 'no-session'].every((r: string): boolean =>
      policyText.includes("'" + r + "'")) &&
    policyText.includes('dataPlaneStaleReason'));

  // -------------------------------------------------------------------------
  // G. The WIRING in the real extension.
  // -------------------------------------------------------------------------
  const extText = readFileSync(EXT, 'utf8');
  const watcherBody = extText.slice(
    extText.indexOf('private startStatusAndNetcfgWatcher(): void {'),
    extText.indexOf('  // N3-7 gap 3: connector died by itself'));
  const watchdogBody = extText.slice(
    extText.indexOf('private noteDataPlaneWatchdog(status: ConnectorStatusResult): void {'),
    extText.indexOf('  // N1(ii): "the tunnel is broken and owes a recovery"'));

  check('G1 the extension imports the policy by value and its fact type type-only',
    extText.includes("{ dataPlaneStaleReason, nextStaleSince, resumeGapDetected, shouldRecover }\n" +
      "  from './NetBirdDataPlanePolicy';") &&
    extText.includes("import type { DataPlaneFacts } from './NetBirdDataPlanePolicy';"));

  check('G2 the wake edge rides the SAME 5 s watcher tick, before the status read',
    watcherBody.includes('this.noteWatcherTick();') &&
    watcherBody.indexOf('this.noteWatcherTick();') <
      watcherBody.indexOf('const status: ConnectorStatusResult | null = pollStatusOrNull();') &&
    !extText.slice(0, extText.indexOf('private noteWatcherTick(): void {'))
      .includes('setInterval((): void => { this.noteWatcherTick();'));

  check('G3 the wake edge RESTARTS the stale clock and never recovers directly',
    (() => {
      const body = extText.slice(extText.indexOf('private noteWatcherTick(): void {'),
        extText.indexOf('  // 0.0.54 data-plane watchdog, part 2'));
      return body.includes('resumeGapDetected(now - this.lastWatcherTickMs)') &&
        body.includes('this.dataPlaneStaleSinceMs = now;') &&
        body.includes('this.lastWatcherTickMs = now;') &&
        !body.includes('recoverPhysicalNetwork') && !body.includes('diagMilestone');
    })());

  check('G4 the SAME status document feeds the policy, right after the health read',
    watcherBody.includes('this.noteConnectorHealth(status);') &&
    watcherBody.includes('this.noteDataPlaneWatchdog(status);') &&
    watcherBody.indexOf('this.noteConnectorHealth(status);') <
      watcherBody.indexOf('this.noteDataPlaneWatchdog(status);') &&
    watchdogBody.includes('signalRegistered') && watchdogBody.includes('peers_with_session'));

  check('G5 the policy owns the clocks, the extension only keeps them',
    watchdogBody.includes('nextStaleSince(facts)') &&
    watchdogBody.includes('shouldRecover(facts)') &&
    watchdogBody.includes('this.dataPlaneStaleSinceMs = since;') &&
    extText.includes('private dataPlaneStaleSinceMs: number = 0;') &&
    extText.includes('private lastDataPlaneRecoveryMs: number = 0;') &&
    extText.includes('private lastWatcherTickMs: number = 0;'));

  check('G6 a due recovery goes through the EXISTING recoverPhysicalNetwork (no new timer)',
    watchdogBody.includes('this.recoverPhysicalNetwork().catch((error: Error): void => {') &&
    watchdogBody.includes('this.n2NetworkRecoveryFail(error);') &&
    !watchdogBody.includes('setInterval') && !watchdogBody.includes('setTimeout'));

  check('G7 the stale evidence is persisted with reason and age (seconds)',
    watchdogBody.includes("const reason: string = dataPlaneStaleReason(facts);") &&
    watchdogBody.includes("const milestoneFields: DiagFields = { 'reason': reason, 'age': ageSeconds };") &&
    watchdogBody.includes("this.diagMilestone('vpn_dataplane_stale', milestoneFields);") &&
    watchdogBody.includes('Math.floor((now - since) / 1000)'));

  check('G8 the action arms the existing recovery + retry bookkeeping and resets both clocks',
    watchdogBody.includes('this.networkDirty = true;') &&
    watchdogBody.includes('this.recoveryRetryArmed = true;') &&
    watchdogBody.indexOf('this.recoveryRetryArmed = true;') <
      watchdogBody.indexOf('this.recoverPhysicalNetwork()') &&
    watchdogBody.indexOf('this.lastDataPlaneRecoveryMs = Date.now();') >
      watchdogBody.indexOf('this.recoverPhysicalNetwork()') &&
    watchdogBody.includes('this.dataPlaneStaleSinceMs = this.lastDataPlaneRecoveryMs;'));

  check('G9 teardown never rebuilds: destroyRequested is checked before the action',
    watchdogBody.includes('if (this.destroyRequested || !shouldRecover(facts)) {'));

  check('G10 the watchdog reuses the watcher status (no second status/core read)',
    watchdogBody.includes('noteDataPlaneWatchdog(status: ConnectorStatusResult)') &&
    !watchdogBody.includes('pollStatusOrNull') && !watchdogBody.includes('connectorStatus()'));
} catch (error) {
  failedCount++;
  console.log('FAIL harness — ' + (error as Error).message);
} finally {
  rmSync(join(tmpdir(), 'nb-dataplane-policy-'), { recursive: true, force: true });
}

console.log('');
console.log(`${passed} passed, ${failedCount} failed`);
process.exit(failedCount === 0 ? 0 : 1);
