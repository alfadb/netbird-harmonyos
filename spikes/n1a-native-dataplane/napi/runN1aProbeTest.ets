/**
 * N1a gate data-plane probe ArkTS test entry.
 *
 * Imports the NAPI overlay from libentry.so, runs the real dual-tunnel
 * loopback pump once and asserts the structured runN1aProbe() fields.
 * Fail-closed: any assertion failure throws.
 *
 * C5 ("not-triggered") is treated as a non-failure per the gate plan
 * ("未触发≠失败"): only "fail" throws. Every other criterion must be
 * exactly "pass".
 *
 * The returned outcome (summary + full detailJson + its SHA-256) is wrapped
 * by the ohosTest test runner (napi/ohosTest/OpenHarmonyTestRunner.ets) into
 * the machine-readable marker `N1A_PROBE_TEST_RESULT|verdict=PASS|...`; the
 * full detailJson travels separately as chunked N1A_JSON hilog lines
 * (defect 2 of EV-N1A-EMU24-20260830-0001: the single-line marker was
 * truncated by the hilog line limit and C7/C8 numbers were unrecoverable).
 * On any assertion failure the thrown N1aProbeError carries the same
 * detailJson/detailSha256 pair so the FAIL path emits the chunks too. The NAPI layer separately emits
 * the per-call C9 marker with the frozen four-field set:
 * `N1A_RESULT|verdict=<PASS|FAIL>|c5=<induced|not-triggered|fail>|throughput_mibps=<x.xx>`.
 *
 * No key material is ever printed.
 */
import n1a from 'libentry.so';
import { N1aProbeResult } from 'libentry.so';

const EXPECTED_VERSION_PREFIX = 'n1a-native-dataplane/';
const EXPECTED_VERIFIED_PACKETS = 2000; // 2 directions x 10 rounds x 200 packets
const THROUGHPUT_FLOOR_MIBPS = 5;

/** Success outcome: short summary + the full machine-readable detail. */
export interface N1aProbeTestOutcome {
  summary: string;
  detailJson: string;
  detailSha256: string;
}

/**
 * Assertion failure carrying the full probe detail so the FAIL path can emit
 * the chunked N1A_JSON lines (the hilog-truncation fix keeps the evidence
 * recoverable on both verdicts).
 */
export class N1aProbeError extends Error {
  readonly detailJson: string;
  readonly detailSha256: string;
  constructor(message: string, detailJson: string, detailSha256: string) {
    super(message);
    this.name = 'N1aProbeError';
    this.detailJson = detailJson;
    this.detailSha256 = detailSha256;
  }
}

export function runN1aProbeTest(): N1aProbeTestOutcome {
  // Frozen r3 C7(3): the ohosTest entry labels this execution's process
  // model as "testrunner" (Phase A window; the phase-B result page passes
  // "entryability" for its own independent run).
  const result: N1aProbeResult = n1a.runN1aProbe('testrunner');
  try {
    return runAssertions(result);
  } catch (e) {
    // Every assertion below runs after `result` exists; attach the full
    // detail so the ohosTest runner can chunk-emit it on the FAIL path.
    if (e instanceof N1aProbeError) {
      throw e;
    }
    const err: Error = e as Error;
    throw new N1aProbeError(err.message, result.detailJson, result.detailSha256);
  }
}

function runAssertions(result: N1aProbeResult): N1aProbeTestOutcome {

  // Assertion order (fail-closed): version -> verdict -> per-criterion ->
  // counters -> throughput -> JSON detail consistency.
  if (typeof result.version !== 'string' || !result.version.startsWith(EXPECTED_VERSION_PREFIX)) {
    throw new Error(`N1a probe version invalid: ${JSON.stringify(result.version)}`);
  }
  if (result.ok !== true || result.verdict !== 'pass') {
    throw new Error(`N1a probe failed: verdict=${result.verdict} detail=${result.detailJson.substring(0, 400)}`);
  }

  const criteria: string[] =
    [result.c1, result.c2, result.c3, result.c4, result.c5, result.c6, result.c7, result.c8, result.c9];
  for (let i = 0; i < criteria.length; i++) {
    const name: string = `c${i + 1}`;
    if (name === 'c5') {
      // C5 is the only criterion allowed to report not-triggered; the NAPI
      // object uses the C9/marker enumeration (induced / not-triggered /
      // fail) — "induced" is C5's pass state.
      if (criteria[i] !== 'induced' && criteria[i] !== 'not-triggered') {
        throw new Error(`N1a probe ${name} failed: ${criteria[i]}`);
      }
    } else if (criteria[i] !== 'pass') {
      throw new Error(`N1a probe ${name} failed: ${criteria[i]}`);
    }
  }

  if (result.verifiedPacketsTotal !== EXPECTED_VERIFIED_PACKETS) {
    throw new Error(`N1a probe integrity count invalid: ${result.verifiedPacketsTotal}`);
  }
  if (result.mismatchCount !== 0 || result.lostCount !== 0) {
    throw new Error(`N1a probe lost/corrupt: mismatch=${result.mismatchCount} lost=${result.lostCount}`);
  }
  if (result.throughputMiBps < THROUGHPUT_FLOOR_MIBPS) {
    throw new Error(`N1a probe throughput below floor: ${result.throughputMiBps} MiB/s`);
  }
  // Frozen C7 (r3): the gate is probe-owned resources only — both loopback
  // socket fds must verify closed at T3 (the NAPI scalar mirrors the
  // per-fd fcntl gate; process-level counts are observation-only).
  if (result.fd2Closed !== true) {
    throw new Error(`N1a probe C7 per-fd gate failed: fd2 not closed`);
  }
  // Frozen C8 (r3): tunnel_free x2.
  if (result.tunnelsFreed !== 2) {
    throw new Error(`N1a probe C8 cleanup failed: tunnelsFreed=${result.tunnelsFreed}`);
  }

  // The full JSON detail must parse and agree with the typed fields.
  const detail: Record<string, Object> = JSON.parse(result.detailJson) as Record<string, Object>;
  const detailCriteria = detail['criteria'] as Record<string, Object>;
  if (detailCriteria === undefined || detailCriteria['c2'] !== 'pass' ||
      detailCriteria['c3'] !== 'pass' || detailCriteria['verdict'] === undefined) {
    throw new Error('N1a probe detail criteria inconsistent');
  }
  const verdict: string = detail['verdict'] as string;
  if (verdict !== 'pass') {
    throw new Error(`N1a probe detail verdict: ${verdict}`);
  }
  const integrity = detail['integrity'] as Record<string, Object>;
  if (Number(integrity['verified_packets_total']) !== EXPECTED_VERIFIED_PACKETS ||
      Number(integrity['mismatches']) !== 0 || Number(integrity['lost']) !== 0) {
    throw new Error('N1a probe detail integrity counters inconsistent');
  }
  const throughput = detail['throughput'] as Record<string, Object>;
  if (Number(throughput['mib_per_second']) < THROUGHPUT_FLOOR_MIBPS) {
    throw new Error('N1a probe detail throughput inconsistent');
  }
  const handshake = detail['handshake'] as Record<string, Object>;
  if (handshake['established'] !== true || handshake['first_roundtrip_ok'] !== true) {
    throw new Error('N1a probe detail handshake inconsistent');
  }

  const summary: string =
    `N1a probe PASS version=${result.version} verifiedPackets=${result.verifiedPacketsTotal} ` +
    `c5=${result.c5} throughputMiBps=${result.throughputMiBps.toFixed(3)} pumpMs=${result.pumpMs.toFixed(3)} ` +
    `fd2Closed=${result.fd2Closed} tunnelsFreed=${result.tunnelsFreed} processModel=${result.processModel}`;
  return {
    summary: summary,
    detailJson: result.detailJson,
    detailSha256: result.detailSha256,
  } as N1aProbeTestOutcome;
}
