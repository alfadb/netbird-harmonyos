import abilityDelegatorRegistry from '@ohos.app.ability.abilityDelegatorRegistry';
import TestRunner from '@ohos.application.testRunner';
import hilog from '@ohos.hilog';
import probe from 'libprobe.so';
import type { GoProbeResult, GoThreadResult } from 'libprobe.so';

const DOMAIN: number = 0x2900;
const TAG: string = 'R1Api24ProbeTest';
const EXPECTED_PING: string = 'pong';
const EXPECTED_VERSION: string = 'r1-api24-probe/0.0.1';

function threadSummary(result: GoThreadResult): string {
  return `GO_THREAD_RESULT|role=${result.role}|ok=${result.ok}|started=${result.started}|resolved=${result.resolved}|helloOk=${result.helloOk}|runtimeOk=${result.runtimeOk}|netDialOk=${result.netDialOk}|hello=${result.hello}|runtimeBytes=${result.runtimeBytes}|netDialCode=${result.netDialCode}|stage=${result.stage}|detail=${result.detail}`;
}

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

      const suite: GoProbeResult = probe.runGoProbe();
      const preSummary: string = threadSummary(suite.preThread);
      const postSummary: string = threadSummary(suite.postThread);
      hilog.info(DOMAIN, TAG, '%{public}s', preSummary);
      hilog.info(DOMAIN, TAG, '%{public}s', postSummary);
      delegator.printSync(preSummary);
      delegator.printSync(postSummary);
      const suiteSummary: string = `GO_SPIKE_RESULT|verdict=${suite.verdict}|ok=${suite.ok}|pid=${suite.processId}|preCreatedBeforeDlopen=${suite.preThreadCreatedBeforeDlopen}|dlopenLoaded=${suite.dlopenLoaded}|postCreatedAfterDlopen=${suite.postThreadCreatedAfterDlopen}|pre=${suite.preThread.ok}|post=${suite.postThread.ok}|stage=${suite.stage}|loaderErrno=${suite.loaderErrno}|loaderError=${suite.loaderError}|detail=${suite.detail}`;
      hilog.info(DOMAIN, TAG, '%{public}s', suiteSummary);
      delegator.printSync(suiteSummary);
      const passed: boolean = suite.ok && suite.preThreadCreatedBeforeDlopen && suite.dlopenLoaded &&
        suite.postThreadCreatedAfterDlopen && suite.preThread.ok && suite.postThread.ok;
      delegator.finishTest(`${baselineSummary}; ${suiteSummary}`, passed ? 0 : 1, () => {});
    } catch (error) {
      const failure: string = `Probe invocation failed: ${String(error)}`;
      hilog.error(DOMAIN, TAG, '%{public}s', failure);
      delegator.finishTest(failure, 1, () => {});
    }
  }
}
