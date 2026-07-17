import abilityDelegatorRegistry from '@ohos.app.ability.abilityDelegatorRegistry';
import TestRunner from '@ohos.application.testRunner';
import hilog from '@ohos.hilog';
import probe from 'libprobe.so';

const DOMAIN: number = 0x2900;
const TAG: string = 'R1Api24ProbeTest';
const EXPECTED_PING: string = 'pong';
const EXPECTED_VERSION: string = 'r1-api24-probe/0.0.1';

export default class OpenHarmonyTestRunner implements TestRunner {
  onPrepare(): void {
    hilog.info(DOMAIN, TAG, 'TestRunner onPrepare');
  }

  onRun(): void {
    const delegator = abilityDelegatorRegistry.getAbilityDelegator();

    try {
      const pingResult: string = probe.ping();
      const versionResult: string = probe.version();
      const baselinePassed: boolean = pingResult === EXPECTED_PING && versionResult === EXPECTED_VERSION;
      const baselineSummary: string = `BASELINE_RESULT|functional=${baselinePassed ? 'PASS' : 'FAIL'}|ping=${pingResult}|version=${versionResult}`;
      hilog.info(DOMAIN, TAG, '%{public}s', baselineSummary);
      delegator.printSync(baselineSummary);
      if (!baselinePassed) {
        delegator.finishTest(`Node-API baseline assertion failed: ${baselineSummary}`, 1, () => {});
        return;
      }

      delegator.finishTest(baselineSummary, 0, () => {});
    } catch (error) {
      const failure: string = `Probe invocation failed: ${String(error)}`;
      hilog.error(DOMAIN, TAG, '%{public}s', failure);
      delegator.finishTest(failure, 1, () => {});
    }
  }
}
