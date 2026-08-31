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
import { runN1aProbeTest } from '../../../main/ets/pages/runN1aProbeTest';

const DOMAIN: number = 0x2900;
const TAG: string = 'N1aProbeTest';

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
      const summary: string = runN1aProbeTest();
      const marker: string = `N1A_PROBE_TEST_RESULT|verdict=PASS|${summary}`;
      hilog.info(DOMAIN, TAG, '%{public}s', marker);
      delegator.printSync(marker);
      delegator.finishTest(marker, 0, () => {});
    } catch (error) {
      const detail: string = cleanDetail(error as Error);
      const marker: string = `N1A_PROBE_TEST_RESULT|verdict=FAIL|detail=${detail}`;
      hilog.error(DOMAIN, TAG, '%{public}s', marker);
      delegator.printSync(marker);
      delegator.finishTest(marker, 1, () => {});
    }
  }
}
