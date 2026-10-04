// Recreate lifecycle policy (2026-10-05, roam-1005-0014) — PURE LOGIC only,
// no @ohos imports. The host-side selfcheck imports this file byte-identical
// (client/entry/selfcheck/run-recreate-policy.mts), so everything here must
// stay plain TypeScript.
//
// Device-proven defect this module polices: a controlled recreate's
// destroy() → create() pair ran only ~168 ms apart; the system-side teardown
// (netdev removal + VPN icon unregister) landed AFTER the new create() and
// hit the SAME global per-user VPN slot — the fresh vpn-tun vanished while
// the fd stayed open in-process and the core kept self-reporting connected
// (RECREATE-LIFECYCLE-ANALYSIS.md §1.2). The decisions below turn that
// observation into bounded, host-verifiable policy:
//  1. SETTLE — after destroy(), wait (floor) and until the OLD vpn-tun is no
//     longer listed by the official network registry, with a cap fuse.
//  2. VERIFY — after create(), the new vpn-tun must really be there; absent
//     means ONE full recreate retry (longer settle), then honest fail-closed.
//  3. WATCHDOG — a settled generation whose vpn-tun disappears later gets the
//     same one-retry-then-fail-closed handling (never keep reporting
//     "connected" over a dead interface).
//
// Presence values are the caller's probe outcomes: 'present' (registry lists
// vpn-tun), 'absent' (registry definitively does not), 'unknown' (probe
// error/ambiguity — treated leniently everywhere: an unreadable registry must
// not tear down a working tunnel, it only cannot save a broken one).

// Proceed out of the settle wait? The floor keeps the async teardown window
// open even when the registry already dropped the old network; 'present'
// (still listed) keeps waiting; the cap is a FUSE, not acceptance — past it
// the caller proceeds and the post-create verification decides.
export function settleWaitAction(elapsedMs: number, floorMs: number, capMs: number,
  presence: string): string {
  if (elapsedMs >= floorMs + capMs) {
    return 'proceed';
  }
  if (elapsedMs >= floorMs && presence !== 'present') {
    return 'proceed';
  }
  return 'wait';
}

// Post-create verification verdict. 'present' (or an unreadable registry —
// lenient) accepts the generation; a definitively absent vpn-tun retries the
// WHOLE recreate once (the shell's attempt loop bounds maxAttempts) and then
// fails closed.
export function ifaceVerifyOutcome(presence: string, attempt: number,
  maxAttempts: number): string {
  if (presence !== 'absent') {
    return 'ok';
  }
  return attempt < maxAttempts ? 'retry' : 'fail';
}

// Watchdog verdict for a settled generation whose interface probe came back
// absent on a status tick (never during a rebuild — the caller's guards
// already produced generationLive=false there). One automatic rebuild per
// session; a second loss is an honest fail-closed disconnect.
export function ifaceLostDecision(presence: string, retryUsed: boolean,
  generationLive: boolean): string {
  if (presence !== 'absent' || !generationLive) {
    return 'ignore';
  }
  return retryUsed ? 'fail-closed' : 'rebuild';
}
