import UIAbility from '@ohos.app.ability.UIAbility';
import hilog from '@ohos.hilog';
import window from '@ohos.window';
import probe from 'libprobe.so';

const DOMAIN: number = 0x2900;
const TAG: string = 'R1Api24Probe';

// This ability is a short-lived build and loading probe, not a product shell.
export default class EntryAbility extends UIAbility {
  onWindowStageCreate(windowStage: window.WindowStage): void {
    windowStage.loadContent('pages/Index', (error) => {
      if (error.code) {
        hilog.error(DOMAIN, TAG, 'ArkUI load failed: %{public}s', JSON.stringify(error));
        return;
      }

      try {
        const pingResult: string = probe.ping();
        const versionResult: string = probe.version();
        hilog.info(DOMAIN, TAG, 'Node-API result ping=%{public}s version=%{public}s', pingResult, versionResult);
      } catch (error) {
        hilog.error(DOMAIN, TAG, 'Node-API invocation failed: %{public}s', String(error));
      }
    });
  }
}
