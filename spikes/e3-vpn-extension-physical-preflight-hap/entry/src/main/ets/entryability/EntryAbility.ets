import AbilityConstant from '@ohos.app.ability.AbilityConstant';
import UIAbility from '@ohos.app.ability.UIAbility';
import Want from '@ohos.app.ability.Want';
import hilog from '@ohos.hilog';
import window from '@ohos.window';

const DOMAIN: number = 0x2900;
const TAG: string = 'E3PhysVpn';

export default class EntryAbility extends UIAbility {
  onCreate(want: Want, _launchParam: AbilityConstant.LaunchParam): void {
    const bundleName: string = want.bundleName ?? this.context.abilityInfo.bundleName;
    AppStorage.setOrCreate<string>('bundleName', bundleName);
    AppStorage.setOrCreate<string>('operationStatus', 'Ready');
    AppStorage.setOrCreate<string>('operationDetail', 'No request');
    hilog.info(DOMAIN, TAG, 'ENTRY_ONCREATE|bundle=%{public}s', bundleName);
  }

  onWindowStageCreate(windowStage: window.WindowStage): void {
    windowStage.loadContent('pages/Index', (error) => {
      if (error.code) {
        hilog.error(DOMAIN, TAG, 'ENTRY_LOAD_REJECTED|code=%{public}d', error.code);
        return;
      }
      hilog.info(DOMAIN, TAG, 'ENTRY_PAGE_READY|bundle=%{public}s',
        AppStorage.get<string>('bundleName') ?? 'unknown');
    });
  }
}
