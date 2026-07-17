import AbilityConstant from '@ohos.app.ability.AbilityConstant';
import UIAbility from '@ohos.app.ability.UIAbility';
import Want from '@ohos.app.ability.Want';
import hilog from '@ohos.hilog';
import window from '@ohos.window';
import probe from 'libprobe.so';

const DOMAIN: number = 0x2900;
const TAG: string = 'R1Api24Probe';

// This ability is a short-lived build and loading probe, not a product shell.
export default class EntryAbility extends UIAbility {
  private onCreateObserved: boolean = false;

  onCreate(_want: Want, _launchParam: AbilityConstant.LaunchParam): void {
    this.onCreateObserved = true;
    hilog.info(DOMAIN, TAG, 'EntryAbility onCreate');
  }

  onWindowStageCreate(windowStage: window.WindowStage): void {
    hilog.info(DOMAIN, TAG, 'EntryAbility windowStage onWindowStageCreate');
    windowStage.loadContent('pages/Index', (error) => {
      if (error.code) {
        hilog.error(DOMAIN, TAG, 'ArkUI load failed: %{public}s', JSON.stringify(error));
        return;
      }

      try {
        const pingResult: string = probe.ping();
        const versionResult: string = probe.version();
        hilog.info(DOMAIN, TAG, 'Node-API result ping=%{public}s version=%{public}s', pingResult, versionResult);
        hilog.info(DOMAIN, TAG, 'EntryAbility onCreate observed=%{public}s', this.onCreateObserved ? 'true' : 'false');
      } catch (error) {
        hilog.error(DOMAIN, TAG, 'Node-API invocation failed: %{public}s', String(error));
      }
    });
  }

  onForeground(): void {
    hilog.info(DOMAIN, TAG, 'EntryAbility foreground onForeground');
  }
}
