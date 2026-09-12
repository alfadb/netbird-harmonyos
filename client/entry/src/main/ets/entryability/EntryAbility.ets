import AbilityConstant from '@ohos.app.ability.AbilityConstant';
import UIAbility from '@ohos.app.ability.UIAbility';
import Want from '@ohos.app.ability.Want';
import hilog from '@ohos.hilog';
import window from '@ohos.window';

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';
const BUNDLE_NAME: string = 'cn.alfadb.netbird';

export default class EntryAbility extends UIAbility {
  onCreate(want: Want, _launchParam: AbilityConstant.LaunchParam): void {
    hilog.info(DOMAIN, TAG, 'ENTRY_ONCREATE|bundle=%{public}s', want.bundleName ?? BUNDLE_NAME);
  }

  onWindowStageCreate(windowStage: window.WindowStage): void {
    windowStage.loadContent('pages/Index', (error) => {
      if (error.code) {
        hilog.error(DOMAIN, TAG, 'ENTRY_LOAD_REJECTED|code=%{public}d', error.code);
        return;
      }
      hilog.info(DOMAIN, TAG, 'ENTRY_PAGE_READY|bundle=%{public}s', BUNDLE_NAME);
    });
  }
}
