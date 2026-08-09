/**
 * N0 native core overlay ohosTest test runner.
 *
 * Staged by prepare-hap-snapshot.sh into a temporary r1-api24-hap snapshot
 * copy as entry/src/ohosTest/ets/testrunner/OpenHarmonyTestRunner.ets, so the
 * `aa test` Hypium runner actually invokes runN0ProbeTest() (the ArkTS test
 * entry over libentry.so) instead of leaving it unreferenced.
 *
 * Fail-closed: runN0ProbeTest() throws on any assertion failure; this runner
 * catches it, prints the machine-readable FAIL marker and finishes the test
 * with a nonzero result code. On success it prints the PASS marker.
 *
 * No private key material is ever printed; the public key is only reported as
 * its length (44 chars), never its value.
 */
import abilityDelegatorRegistry from '@ohos.app.ability.abilityDelegatorRegistry';
import TestRunner from '@ohos.application.testRunner';
import hilog from '@ohos.hilog';
import { runN0ProbeTest } from '../../../main/ets/pages/runProbeTest';

const DOMAIN: number = 0x2900;
const TAG: string = 'N0NativeCoreTest';

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
      const summary: string = runN0ProbeTest();
      const marker: string = `N0_CORE_PROBE_RESULT|verdict=PASS|${summary}`;
      hilog.info(DOMAIN, TAG, '%{public}s', marker);
      delegator.printSync(marker);
      delegator.finishTest(marker, 0, () => {});
    } catch (error) {
      const detail: string = cleanDetail(error as Error);
      const marker: string = `N0_CORE_PROBE_RESULT|verdict=FAIL|detail=${detail}`;
      hilog.error(DOMAIN, TAG, '%{public}s', marker);
      delegator.printSync(marker);
      delegator.finishTest(marker, 1, () => {});
    }
  }
}
