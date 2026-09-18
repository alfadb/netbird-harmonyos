// N2-H iface-bytes availability probe (2026-09-18, diagnostic-only).
//
// What this is: ONE availability experiment for the OFFICIAL per-NIC byte
// counters — @ohos.net.statistics getIfaceRxBytes(nic) / getIfaceTxBytes(nic)
// (SDK 26.0.0.821 @ohos.net.statistics.d.ts:79/113 — since 10, syscap
// Communication.NetManager.Core, NO @permission and NO @systemapi on the two
// iface methods; ohos.permission.GET_NETWORK_STATS is carried ONLY by the
// getUid* methods, which this probe never calls and which belong to a
// different question: what OTHER apps' UIDs can be queried). Device run 0017
// proved the app cannot read /proc/net/dev directly (Permission denied,
// os error 13, collector counters=[]); this probe answers whether the public
// API works where that proc read did not.
//
// What this is NOT: not a C7 replacement claim, not part of the device
// evidence collector, never Evidence. Every line is diag=true|is_evidence=
// false. It samples BYTES only — there is NO public per-iface PACKET API, and
// no packet count is derived, divided out, or back-filled from these bytes.
// Evidence.counters / notes are untouched; no pre/post-ARM reconciliation
// happens. A byte value for 'vpn-tun', or a 0 returned for the nonexistent
// control name, is recorded verbatim and proves nothing on its own — the
// interpretation belongs to the reviewed run document
// (docs/n2h-iface-statistics-probe-20260918.md), not to this file.
//
// Caller contract: the extension's maybeArmN2hCollector SUCCESS branch only
// (the N2H_COLLECTOR_ARMED line) — at most once per diagnostic arm,
// fire-and-forget, no timer, no loop, no await added to the arm path. Every
// per-call rejection is caught HERE; the caller carries a last-resort catch,
// so an unhandled rejection is impossible either way. Field discipline: error
// CODE tokens only — never a payload, never an exception message.
import hilog from '@ohos.hilog';
import statistics from '@ohos.net.statistics';

const PROBE_DOMAIN: number = 0x2900;
const PROBE_TAG: string = 'NetBirdVpn';

// The interface under test: the name the core's TUN device registers, and the
// fixed negative-control name: deliberately NOT expected on any device, kept
// within IFNAMSIZ (Linux kernel limit of 15 name characters) so the sample
// tests "legal name, no such interface" rather than an illegal-name rejection
// (if the API answers 0 for it instead of an error, that is recorded verbatim
// — it is a finding about the API's unknown-name shape, never a proof that
// the vpn-tun value works; on a real device confirm absence against the
// interface list before reading anything into it).
const PROBE_IFACE: string = 'vpn-tun';
const PROBE_NEGATIVE_IFACE: string = 'n2h-none0';

// Same shape the shell uses elsewhere for BusinessError-like throws
// (@ohos.base BusinessError carries .code); kept local so the module stays
// import-light for the host-side fixtures.
interface BusinessFailure extends Error {
  code?: number;
}

// One interface sample. A null byte field means "no number we may print":
// the call rejected, or the resolved value failed the explicit
// finite/nonnegative check — an error is NEVER turned into 0 and a bogus
// value is NEVER printed.
class IfaceSample {
  rxBytes: number | null = null;
  txBytes: number | null = null;
  rxError: string = 'none';
  txError: string = 'none';
  startedUnixMs: number = 0;
  finishedUnixMs: number = 0;
}

function renderBytes(value: number | null): string {
  return value === null ? 'unavailable' : String(value);
}

// Error-code token only: the documented BusinessError code, 'no-code' for a
// throw without one, 'invalid-value' for a RESOLVED value that is not a
// finite nonnegative byte count. Never an exception's message text, never a
// payload.
function errorCodeOf(error: Error): string {
  const code = (error as BusinessFailure).code;
  return code === undefined || code === null ? 'no-code' : String(code);
}

async function sampleIface(requestId: string, iface: string): Promise<void> {
  const sample: IfaceSample = new IfaceSample();
  sample.startedUnixMs = Date.now();
  try {
    const rx: number = await statistics.getIfaceRxBytes(iface);
    if (Number.isFinite(rx) && rx >= 0) {
      sample.rxBytes = rx;
    } else {
      sample.rxError = 'invalid-value';
    }
  } catch (error) {
    sample.rxError = errorCodeOf(error as Error);
  }
  try {
    const tx: number = await statistics.getIfaceTxBytes(iface);
    if (Number.isFinite(tx) && tx >= 0) {
      sample.txBytes = tx;
    } else {
      sample.txError = 'invalid-value';
    }
  } catch (error) {
    sample.txError = errorCodeOf(error as Error);
  }
  sample.finishedUnixMs = Date.now();
  // Epoch-ms values ride %{public}s (never %{public}d): they exceed the
  // 32-bit int the d formatter targets, same discipline as the other
  // large-number fields in this shell.
  hilog.info(PROBE_DOMAIN, PROBE_TAG,
    'N2H_IFACE_BYTES_PROBE|requestId=%{public}s|interface=%{public}s|' +
    'rxBytes=%{public}s|txBytes=%{public}s|rxError=%{public}s|txError=%{public}s|' +
    'startedUnixMs=%{public}s|finishedUnixMs=%{public}s|diag=true|is_evidence=false',
    requestId, iface, renderBytes(sample.rxBytes), renderBytes(sample.txBytes),
    sample.rxError, sample.txError, String(sample.startedUnixMs),
    String(sample.finishedUnixMs));
}

// The whole probe: TWO interface samples (the iface under test + the negative
// control), four statistics promises total, sequential on purpose — two short
// one-shot calls, no parallelism to reason about, and the negative control
// always runs. Resolves normally in every case (each call carries its own
// catch); the caller's last-resort catch exists for the impossible path only.
export async function runInterfaceStatsProbe(requestId: string): Promise<void> {
  await sampleIface(requestId, PROBE_IFACE);
  await sampleIface(requestId, PROBE_NEGATIVE_IFACE);
}
