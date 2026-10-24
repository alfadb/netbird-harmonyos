/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// 0.0.54 data-plane watchdog — PURE LOGIC only.
//
// WHAT THIS FILE IS: the WHOLE decision of "the extension says `connected`, but
// is the data plane BEHIND that word actually there?". `connector: state ->
// connected` only proves the management Sync stream; signal registration, ICE
// and WG sessions are not part of that state. The 2026-10-21 PC capture
// (artifacts/incident-20261021-pc-peers0/pc/REPORT.md) shows the exact shape:
// after a lid-close/modern-standby wake the UI said 已连接 while peers_online
// stayed 0 for 2235 s / 830 s / 4816 s; only a manual disconnect+connect (a
// FULL new start) healed it. W3 (16:17–16:31) also had NO network-change
// callback at all, so the 0.0.50 recovery leg never ran.
//
// The extension only gathers FACTS from the status document it already polls on
// the EXISTING 5 s watcher tick; every gate lives here so it is host-verifiable
// without a device. This file must stay plain TypeScript: NO @ohos import, no
// decorator, no enum, no platform call — the host selfcheck imports it
// BYTE-IDENTICAL (client/entry/selfcheck/run-dataplane-policy.mts).
//
// STALE (the closed definition):
//   * `state !== 'connected'` or `!running` — NOT stale. A user stop reads as
//     'disconnected', a slow start as 'connecting' and a terminal worker as
//     running=false: none of them owes a rebuild.
//   * `connected` and `!signalRegistered` — stale. Without the signal stream
//     registered the core cannot (re)establish sessions, which is exactly the
//     W2/W4 shape (12884 `pump error ... signal-stream-not-registered` lines).
//   * `connected`, signal registered, `peerCount > 0` and
//     `wgPeersWithSession === 0` — stale. The tunnel is up on paper and no peer
//     has a live WG session.
//   * `unknown` WG facts (an older core omits `wg`, so peers_with_session is
//     -1) never satisfy the second clause: we do not rebuild on a fact we could
//     not read. The signal clause has no such escape because `signal` is a
//     REQUIRED section of this core's status document.
//
// THRESHOLDS: stale must PERSIST for DATA_PLANE_STALE_AFTER_MS (30 s) — the
// upstream signal watchdog's own 30 s silence floor
// (shared/signal/client/grpc.go:29-41) — before any recovery, and
// watchdog-initiated recoveries are at least DATA_PLANE_RECOVER_MIN_GAP_MS
// (60 s) apart. One recovery at a time is the extension's own business
// (`recoveryActive` is a fact here, not a gate this module bypasses). A
// wall-clock gap between two watcher ticks longer than
// DATA_PLANE_RESUME_GAP_MS (15 s) is a suspend/wake edge (W3: the platform
// delivered no network callback after the wake), so the stale clock is
// RESTARTED there instead of being trusted from before the freeze — the
// watchdog then evaluates once, ~30 s after the wake, instead of recovering
// blind.
//
// NOTHING IN THIS FILE IS PERSISTED. The caller owns the clocks (the watcher
// tick is the only source), and a process restart starts with all of them at 0.

/// How long the data plane must have been observed stale before a recovery.
export const DATA_PLANE_STALE_AFTER_MS: number = 30000;
/// Minimum spacing between two WATCHDOG-initiated recoveries.
export const DATA_PLANE_RECOVER_MIN_GAP_MS: number = 60000;
/// A wall-clock watcher-tick gap above this is a suspend/wake edge.
export const DATA_PLANE_RESUME_GAP_MS: number = 15000;

/// The facts the extension's existing status poll already holds. All required:
/// the caller has the status document, so there is no "unknown state" input —
/// `running`/`state` are the connector's own word, and a missing `wg` section
/// arrives as wgPeersWithSession = -1.
export interface DataPlaneFacts {
  running: boolean;
  state: string;
  signalRegistered: boolean;
  peerCount: number;
  /// `status.wg.peers_with_session`; -1 when the core did not deliver `wg`.
  wgPeersWithSession: number;
  /// A recovery is in flight right now — the extension's pending mechanism owns
  /// the next attempt, so no new one may be started.
  recoveryActive: boolean;
  /// When the CURRENT stale run was first observed (0 = none in this process).
  staleSinceMs: number;
  /// When this watchdog last started a recovery (0 = never).
  lastRecoveryMs: number;
  nowMs: number;
}

/// Is the data plane behind `connected` demonstrably not there? See the closed
/// definition in the header.
export function dataPlaneStale(facts: DataPlaneFacts): boolean {
  if (facts.running !== true || facts.state !== 'connected') {
    return false;
  }
  if (facts.signalRegistered !== true) {
    return true;
  }
  return facts.peerCount > 0 && facts.wgPeersWithSession === 0;
}

/// The stale clock after this observation: a live stale run keeps its start, a
/// fresh one starts NOW, and a non-stale observation clears it to 0. A clock
/// that is unreadable or in the future (the wall clock moved) is restarted at
/// nowMs rather than trusted.
export function nextStaleSince(facts: DataPlaneFacts): number {
  if (!dataPlaneStale(facts)) {
    return 0;
  }
  const now: number = facts.nowMs;
  if (!Number.isFinite(now) || now <= 0) {
    return 0;
  }
  if (facts.staleSinceMs > 0 && facts.staleSinceMs <= now) {
    return facts.staleSinceMs;
  }
  return now;
}

/// Should this observation start a recovery? Only when the data plane is still
/// stale, the stale run has lasted at least `staleAfterMs`, no recovery is in
/// flight, and `recoverMinGapMs` has passed since the last watchdog recovery.
/// An unknown stale start (staleSinceMs <= 0) never triggers: the caller stores
/// the answer of [`nextStaleSince`] first, so the clock is always known here.
export function shouldRecover(facts: DataPlaneFacts,
  staleAfterMs: number = DATA_PLANE_STALE_AFTER_MS,
  recoverMinGapMs: number = DATA_PLANE_RECOVER_MIN_GAP_MS): boolean {
  if (!dataPlaneStale(facts) || facts.recoveryActive) {
    return false;
  }
  const now: number = facts.nowMs;
  if (!Number.isFinite(now) || now <= 0) {
    return false;
  }
  if (facts.staleSinceMs <= 0 || now - facts.staleSinceMs < staleAfterMs) {
    return false;
  }
  if (facts.lastRecoveryMs > 0 && now - facts.lastRecoveryMs < recoverMinGapMs) {
    return false;
  }
  return true;
}

/// The persisted evidence token of a stale observation, the closed set the
/// milestone writes: `no-signal` (the signal stream is not registered) or
/// `no-session` (registered, but no peer has a WG session). '' when not stale.
export function dataPlaneStaleReason(facts: DataPlaneFacts): string {
  if (!dataPlaneStale(facts)) {
    return '';
  }
  return facts.signalRegistered !== true ? 'no-signal' : 'no-session';
}

/// A watcher-tick wall-clock gap above DATA_PLANE_RESUME_GAP_MS means the
/// process was suspended (box sleep / modern standby / doze), not just a slow
/// tick. Boundary is STRICT: 15000 is a slow tick, 15001 is a wake.
export function resumeGapDetected(gapMs: number): boolean {
  return Number.isFinite(gapMs) && gapMs > DATA_PLANE_RESUME_GAP_MS;
}
