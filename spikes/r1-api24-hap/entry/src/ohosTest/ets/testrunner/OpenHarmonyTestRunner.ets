import abilityDelegatorRegistry from '@ohos.app.ability.abilityDelegatorRegistry';
import TestRunner from '@ohos.application.testRunner';
import hilog from '@ohos.hilog';
import probe from 'libprobe.so';
import type { DynamicTlsProbeResult, TlsModelResult } from 'libprobe.so';

const DOMAIN: number = 0x2900;
const TAG: string = 'R1Api24ProbeTest';
const EXPECTED_PING: string = 'pong';
const EXPECTED_VERSION: string = 'r1-api24-probe/0.0.1';

function modelSummary(result: TlsModelResult): string {
  return `TLS_MODEL_RESULT|model=${result.model}|library=${result.library}|functional=${result.functionalStatus}|attempted=${result.attempted}|required=${result.required}|ok=${result.ok}|expectedBlock=${result.expectedBlock}|environmentDrift=${result.environmentDrift}|preCreatedBeforeDlopen=${result.preThreadCreatedBeforeDlopen}|postCreatedAfterDlopen=${result.postThreadCreatedAfterDlopen}|mainValue=${result.mainValue}|preValue=${result.preThreadValue}|postValue=${result.postThreadValue}|mainInitial=${result.mainInitial}|preInitial=${result.preThreadInitial}|postInitial=${result.postThreadInitial}|mainIterations=${result.mainIterations}|preIterations=${result.preThreadIterations}|postIterations=${result.postThreadIterations}|mainReset=${result.mainReset}|preReset=${result.preThreadReset}|postReset=${result.postThreadReset}|stage=${result.stage}|loaderErrno=${result.loaderErrno}|loaderError=${result.loaderError}|detail=${result.detail}`;
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

      const suite: DynamicTlsProbeResult = probe.runDynamicTlsProbe();
      const models: TlsModelResult[] = [suite.initialExec, suite.globalDynamic, suite.tlsDesc, suite.localDynamic];
      models.forEach((model: TlsModelResult) => {
        const summary: string = modelSummary(model);
        hilog.info(DOMAIN, TAG, '%{public}s', summary);
        delegator.printSync(summary);
      });
      const suiteSummary: string = `TLS_SUITE_RESULT|verdict=${suite.verdict}|ok=${suite.ok}|ieBlocked=${suite.ieBlocked}|environmentDrift=${suite.environmentDrift}|classicGD=${suite.globalDynamic.functionalStatus}|tlsdesc=${suite.tlsDesc.functionalStatus}|localDynamic=${suite.localDynamic.functionalStatus}|tier1Pass=${suite.tier1Pass}|reason=${suite.stopReason}`;
      hilog.info(DOMAIN, TAG, '%{public}s', suiteSummary);
      delegator.printSync(suiteSummary);
      const passed: boolean = suite.ok && suite.ieBlocked && !suite.environmentDrift && suite.tier1Pass;
      delegator.finishTest(`${baselineSummary}; ${suiteSummary}`, passed ? 0 : 1, () => {});
    } catch (error) {
      const failure: string = `Probe invocation failed: ${String(error)}`;
      hilog.error(DOMAIN, TAG, '%{public}s', failure);
      delegator.finishTest(failure, 1, () => {});
    }
  }
}
