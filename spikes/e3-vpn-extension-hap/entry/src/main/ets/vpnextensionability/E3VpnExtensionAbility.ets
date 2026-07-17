import VpnExtensionAbility from '@ohos.app.ability.VpnExtensionAbility';
import Want from '@ohos.app.ability.Want';
import hilog from '@ohos.hilog';

const DOMAIN: number = 0x2900;
const TAG: string = 'E3VpnGate';

export default class E3VpnExtensionAbility extends VpnExtensionAbility {
  onCreate(want: Want): void {
    const requestId: string = String(want.parameters?.['requestId'] ?? 'missing');
    hilog.info(DOMAIN, TAG,
      'VPN_ONCREATE|bundle=%{public}s|requestId=%{public}s|connectionApiUsed=false',
      want.bundleName ?? 'unknown', requestId);
  }

  onDestroy(): void {
    hilog.info(DOMAIN, TAG, 'VPN_ONDESTROY|connectionApiUsed=false');
  }
}
