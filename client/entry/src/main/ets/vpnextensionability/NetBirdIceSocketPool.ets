// N5c ICE socket pool sizing — PURE module (zero imports).
//
// The protected UDP socket pool is shell-fed (core opens one unbound UDP fd per
// interface round + one long-lived check socket per local candidate), and the
// core's demand is PER PEER: `required_local_sockets() = allowed_ifaces × 2`
// (client/core/src/peer_conn.rs). A fixed 8-socket pool therefore starves as
// soon as `peers × need` exceeds 8 — device smoke 0050 measured 6 peers × 4 = 24
// needed against a pooled 8, with 2 peers starved on every round. The demand is
// NOT a constant: smoke 0051 measured `need=4` (2 allowed interfaces) and
// `need=6` (3, right after Wi-Fi returns), so 0051 §7 item 3 sizes the pool from
// the CORE's own `need_per_peer` (see iceSocketNeedPerPeer below).
//
// This module owns the testable sizing math only:
//   - iceSocketNeedPerPeer: the per-peer demand to size with — the CORE's own
//     `connector_status().ice_sockets.need_per_peer` (= allowed interfaces × 2)
//     when present, else ICE_SOCKET_PER_PEER. smoke 0051 measured BOTH 4 (2
//     allowed interfaces) and 6 (3, right after Wi-Fi returns).
//   - iceSocketTargetDepth: the pool target for a KNOWN peer count and a KNOWN
//     per-peer demand, clamped to [ICE_SOCKET_POOL_MIN, ICE_SOCKET_POOL_MAX].
//   - iceSocketTopUpCount: how many fresh sockets ONE refill pass may feed — the
//     whole gap up to ICE_TOPUP_BATCH, 0 while the network is dirty or the queue
//     already meets the target.
//
// Host self-check: client/entry/selfcheck/run-ice-socket-pool.mts runs the unit
// suite against a byte-identical mirror of this file under node.

// Lower bound: the pre-0050 pool size (device-verified sufficient for 2 peers,
// and the only size the shell can prove before a peer count is known).
export const ICE_SOCKET_POOL_MIN: number = 8;
// Upper bound: fd budget, raised 32 → 48 by smoke 0051 §7 item 3. The target is
// now `peers × core need_per_peer`, and the core measured need=6 (3 allowed
// interfaces) — 6 peers × 6 = 36 exceeded the old 32 cap, so the cap no longer
// covered even the observed device case. 48 covers 8 peers × 6.
// Cost basis: a fed socket is ONE held fd in the core (the shell's original is
// closed right after each feed; `ProtectedUdpFdSource::feed` dups once and the
// shell's fd is gone), and device run 3 already parked 26 unconsumed sockets
// with no fd pressure — 48 stays the same order of magnitude, i.e. +50% over
// the old cap and +22 fds over the deepest device-observed queue. The extension
// process fd limit itself is NOT device-measured; that is why the raise is this
// small instead of unbounded. 48 is a ceiling, reached only when the core
// reports need_per_peer=6 AND ≥8 peers exist.
export const ICE_SOCKET_POOL_MAX: number = 48;
// Fallback per-peer demand for a status document WITHOUT the core field (older
// core, no-connector snapshot, or need_per_peer=-1 "interfaces not
// enumerable"): the two-interface Wi-Fi leg measured in smoke 0050 (`need=4`;
// the single-interface window needs 2 — over-provisioning is intentional, see
// the module header). Prefer iceSocketNeedPerPeer over this constant.
export const ICE_SOCKET_PER_PEER: number = 4;
// One refill pass may close the whole gap: 0050 showed batches of 4 serving ONE
// peer per cycle, so 5 of 6 peers waited ~10 s for their sockets.
export const ICE_TOPUP_BATCH: number = 8;

// The per-peer demand to size the pool with, from the core's live status.
// `need` = connector_status().ice_sockets.need_per_peer (allowed interfaces × 2,
// -1 when the core could not enumerate them). Anything that is not a positive
// finite number — undefined/null (field missing), -1, 0, NaN — falls back to
// ICE_SOCKET_PER_PEER so the pre-0051 formula stays the floor. The result is
// clamped by ICE_SOCKET_POOL_MAX anyway, so a bogus large value cannot inflate
// the pool.
export function iceSocketNeedPerPeer(need: number | undefined | null): number {
  if (need === undefined || need === null || !Number.isFinite(need)) {
    return ICE_SOCKET_PER_PEER;
  }
  const demand: number = Math.floor(need);
  if (!(demand > 0)) { return ICE_SOCKET_PER_PEER; }
  return demand;
}

// Pool target for the live peer count: clamp(peers × need, 8, 48), where need is
// the CORE's per-peer demand when it reports one (0051 §7 item 3: 6 peers ×
// need=6 = 36) and ICE_SOCKET_PER_PEER otherwise (the pre-0051 formula). A peer
// count the shell cannot prove (0 / negative / NaN) keeps the pre-0050 minimum.
export function iceSocketTargetDepth(peers: number, needPerPeer?: number | null): number {
  if (!(peers > 0)) { return ICE_SOCKET_POOL_MIN; }
  const need: number = iceSocketNeedPerPeer(needPerPeer);
  return Math.min(Math.max(peers * need, ICE_SOCKET_POOL_MIN), ICE_SOCKET_POOL_MAX);
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
