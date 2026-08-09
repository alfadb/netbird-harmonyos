/**
 * Type declarations for the N0 native core NAPI overlay (libentry.so).
 *
 * runProbe() returns structured fields; smoke.ok is the fail-closed verdict.
 * arm64 is cross-compile only; this overlay is only claimed to load on x86_64.
 */
export interface N0SmokeResult {
  /** true when every smoke check passed (fail-closed verdict). */
  ok: boolean;
  /** true when the real BoringTun x25519 keygen/derive/base64/check passed. */
  x25519Ok: boolean;
  /** true when the real BoringTun new_tunnel + wireguard_tick passed. */
  tunnelOk: boolean;
  /** last wireguard_tick op (boringtun result_type: 0 done, 1 write-to-network, 2 error). */
  tickOp: number;
  /** last wireguard_tick size. */
  tickSize: number;
}

export interface N0ProbeResult {
  /** Rust core version string, e.g. "n0-native-core/0.1.0+boringtun-0.7.1". */
  version: string;
  /** base64 of the derived x25519 public key (44 chars). */
  key: string;
  /** narrow C ABI smoke result. */
  smoke: N0SmokeResult;
}

export const runProbe: () => N0ProbeResult;
