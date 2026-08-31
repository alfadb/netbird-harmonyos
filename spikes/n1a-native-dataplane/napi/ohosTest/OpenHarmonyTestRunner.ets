/**
 * N1a gate data-plane probe ohosTest test runner.
 *
 * To be staged by the formal Emulator campaign's HAP preparation into the
 * snapshot copy as entry/src/ohosTest/ets/testrunner/OpenHarmonyTestRunner.ets
 * (mirrors the N0 runner), so the `aa test` Hypium runner actually invokes
 * runN1aProbeTest() over libentry.so.
 *
 * Fail-closed: runN1aProbeTest() throws on any assertion failure; this runner
 * catches it, prints the machine-readable FAIL marker and finishes the test
 * with a nonzero result code. On success it prints the phase-A cross-check
 * result-page evidence).
 *
 * No key material is ever printed.
 */
import abilityDelegatorRegistry from '@ohos.app.ability.abilityDelegatorRegistry';
import TestRunner from '@ohos.application.testRunner';
import hilog from '@ohos.hilog';
import { runN1aProbeTest, N1aProbeError } from '../../../main/ets/pages/runN1aProbeTest';

const DOMAIN: number = 0x2900;
const TAG: string = 'N1aProbeTest';

/**
 * Chunk size for the detailJson transport (defect 2 of
 * EV-N1A-EMU24-20260830-0001). The 0001 evidence measured the hilog message
 * truncation at ~488 bytes; the chunk header is ~57 bytes, so 384-byte data
 * chunks keep the full line (~441 message bytes) safely below the cap. The
 * probe JSON is pure ASCII (pinned by a Rust unit test), so UTF-16 code-unit
 * slicing equals byte slicing.
 */
const JSON_CHUNK_BYTES = 384;

/**
 * Emits the full detailJson as chunked `N1A_JSON|part=<i>|total=<n>|sha256=
 * <first16>|data=<chunk>` hilog lines. The runner reassembles and verifies
 * the digest; the JSON itself is byte-identical to the probe's document.
 */
function emitChunkedJson(json: string, sha256Hex: string): void {
  if (json.length === 0) {
    return;
  }
  const sha16: string = sha256Hex.substring(0, 16);
  const total: number = Math.ceil(json.length / JSON_CHUNK_BYTES);
  for (let i = 0; i < total; i++) {
    const data: string = json.substring(i * JSON_CHUNK_BYTES, (i + 1) * JSON_CHUNK_BYTES);
    const line: string = `N1A_JSON|part=${i}|total=${total}|sha256=${sha16}|data=${data}`;
    hilog.info(DOMAIN, TAG, '%{public}s', line);
  }
}

function cleanDetail(error: Error): string {
  return error.message.replace(/\|/g, '/').replace(/\r/g, ' ').replace(/\n/g, ' ').replace(/;/g, ' ');
}

export default class OpenHarmonyTestRunner implements TestRunner {
  onPrepare(): void {
    hilog.info(DOMAIN, TAG, 'TestRunner onPrepare');
  }

  onRun(): void {
    const delegator = abilityDelegatorRegistry.getAbilityDelegator();
    try {
      const outcome = runN1aProbeTest();
      const marker: string = `N1A_PROBE_TEST_RESULT|verdict=PASS|${outcome.summary}`;
      hilog.info(DOMAIN, TAG, '%{public}s', marker);
      emitChunkedJson(outcome.detailJson, outcome.detailSha256);
      delegator.printSync(marker);
      delegator.finishTest(marker, 0, () => {});
    } catch (error) {
      const err: Error = error as Error;
      const detail: string = cleanDetail(err);
      const marker: string = `N1A_PROBE_TEST_RESULT|verdict=FAIL|detail=${detail}`;
      hilog.error(DOMAIN, TAG, '%{public}s', marker);
      if (err instanceof N1aProbeError) {
        // The full detail travels on the chunked lines; the marker keeps the
        // short human-readable context only.
        emitChunkedJson(err.detailJson, err.detailSha256);
      }
      delegator.printSync(marker);
      delegator.finishTest(marker, 1, () => {});
    }
  }
}
