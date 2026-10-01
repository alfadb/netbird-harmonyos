// N5c ICE socket pool sizing — PURE module (zero imports).
//
// The protected UDP socket pool is shell-fed (core opens one unbound UDP fd per
// interface round + one long-lived check socket per local candidate), and the
// core's demand is PER PEER: `required_local_sockets() = allowed_ifaces × 2`
// (client/core/src/peer_conn.rs). A fixed 8-socket pool therefore starves as
// soon as `peers × need` exceeds 8 — device smoke 0050 measured 6 peers × 4 = 24
// needed against a pooled 8, with 2 peers starved on every round.
//
// This module owns the testable sizing math only:
//   - iceSocketTargetDepth: the pool target for a KNOWN peer count, clamped to
//     [ICE_SOCKET_POOL_MIN, ICE_SOCKET_POOL_MAX]. ICE_SOCKET_PER_PEER = 4 is the
//     two-interface Wi-Fi-leg need measured in 0050; a single-interface window
//     needs 2 and is deliberately over-provisioned (a fed socket is cheap, a
//     starved gather costs a peer's session).
//   - iceSocketTopUpCount: how many fresh sockets ONE refill pass may feed — the
//     whole gap up to ICE_TOPUP_BATCH, 0 while the network is dirty or the queue
//     already meets the target.
//
// Host self-check: client/entry/selfcheck/run-ice-socket-pool.mts runs the unit
// suite against a byte-identical mirror of this file under node.

// Lower bound: the pre-0050 pool size (device-verified sufficient for 2 peers,
// and the only size the shell can prove before a peer count is known).
export const ICE_SOCKET_POOL_MIN: number = 8;
// Upper bound: fd budget. A fed socket costs the core one held dup (the shell's
// own original is closed right after each feed), and device run 3 already parked
// 26 unconsumed sockets without fd pressure — 32 stays in the same order of
// magnitude and far below the extension process fd limit.
export const ICE_SOCKET_POOL_MAX: number = 32;
// Per-peer demand on the two-interface Wi-Fi leg measured in smoke 0050
// (`need=4`; the single-interface window needs 2 — over-provisioning is
// intentional, see the module header).
export const ICE_SOCKET_PER_PEER: number = 4;
// One refill pass may close the whole gap: 0050 showed batches of 4 serving ONE
// peer per cycle, so 5 of 6 peers waited ~10 s for their sockets.
export const ICE_TOPUP_BATCH: number = 8;

// Pool target for the live peer count: clamp(peers × 4, 8, 32). A peer count the
// shell cannot prove (0 / negative / NaN) keeps the pre-0050 minimum.
export function iceSocketTargetDepth(peers: number): number {
  if (!(peers > 0)) { return ICE_SOCKET_POOL_MIN; }
  return Math.min(Math.max(peers * ICE_SOCKET_PER_PEER, ICE_SOCKET_POOL_MIN), ICE_SOCKET_POOL_MAX);
}

// How many sockets one refill pass should feed now: the gap to the target, capped
// at ICE_TOPUP_BATCH; 0 when paused (network-dirty / gate / no connection, decided
// by the caller) or when the queue already meets the target. `queued` is the
// LATEST depth the shell knows (-1 = no shell-fed source attached ⇒ full gap).
export function iceSocketTopUpCount(queued: number, target: number, paused: boolean): number {
  if (paused) { return 0; }
  const gap: number = target - queued;
  if (gap <= 0) { return 0; }
  return Math.min(gap, ICE_TOPUP_BATCH);
}
