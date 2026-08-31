/**
 * Type declarations for the N1a gate data-plane probe NAPI overlay
 * (libentry.so).
 *
 * runN1aProbe() returns structured fields; ok is the fail-closed verdict.
 * Criterion values are "pass", "fail", or (C5 only) "not-triggered".
 * arm64 is cross-compile only; this overlay is only claimed to load on
 * x86_64 (Emulator gate).
 */
export interface N1aProbeResult {
  /** Rust core version string, e.g. "n1a-native-dataplane/0.1.0+boringtun-0.7.1". */
  version: string;
  /** true when every criterion passed (C5 not-triggered counts as non-failure). */
  ok: boolean;
  /** "pass" | "fail" */
  verdict: string;
  /** C1 library load (pass once the probe could run inside the loaded member). */
  c1: string;
  /** C2 handshake establishment (real wireguard_stats observable). */
  c2: string;
  /** C3 per-packet integrity, 10x200x1024 per direction, byte-verified. */
  c3: string;
  /** C4 throughput floor 5 MiB/s over the pure pump window. */
  c4: string;
  /** C5 backpressure ("not-triggered" = no saturation observed, not a failure). */
  c5: string;
  /** C6 tick paths at >= 3 no-data gaps without breaking the session. */
  c6: string;
  /** C7 fd + thread snapshots show no monotonic growth. */
  c7: string;
  /** C8 tunnel_free x2 + socket close, fd back at the pre-pump baseline. */
  c8: string;
  /** C9 structured result page (this object + the ohosTest rendering). */
  c9: string;
  /** Verified plaintext packets over both directions (expected 2000). */
  verifiedPacketsTotal: number;
  /** C3 byte mismatches (expected 0). */
  mismatchCount: number;
  /** C3 lost packets (expected 0). */
  lostCount: number;
  /** true when C5 actually observed saturation (EAGAIN or kernel drops). */
  backpressureTriggered: boolean;
  /** C4 throughput over the pure pump window, MiB/s. */
  throughputMiBps: number;
  /** C4 pure pump time, milliseconds. */
  pumpMs: number;
  /** /proc/self/fd entry count before the pump (C7/C8). */
  fdBaseline: number;
  /** /proc/self/fd entry count after cleanup (C7/C8). */
  fdAfter: number;
  /** Full machine-readable JSON detail document from the Rust core. */
  detailJson: string;
  detailSha256: string;
}

export const runN1aProbe: () => N1aProbeResult;
