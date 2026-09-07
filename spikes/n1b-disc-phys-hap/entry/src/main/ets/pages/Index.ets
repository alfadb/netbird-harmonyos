import Want from '@ohos.app.ability.Want';
import hilog from '@ohos.hilog';
import vpnExtension from '@ohos.net.vpnExtension';

const DOMAIN: number = 0x2900;
const TAG: string = 'N1BDiscVpn';
const BUNDLE_NAME: string = 'cn.alfadb.netbird.n1bdisc';
const EXTENSION_ABILITY: string = 'N1BDiscVpnExtensionAbility';

interface BusinessFailure extends Error {
  code?: number;
}

// Single-shot UI trigger. Exactly one click-gated startVpnExtensionAbility
// request per attempt: the latch never resets (reject does not re-issue, no
// retry), so one probe attempt cannot re-launch the extension onCreate flow.
@Entry
@Component
struct Index {
  @State statusText: string = 'READY';
  @State detailText: string = 'tap to send one start request';
  @State requestSent: boolean = false;

  // Frozen identity (gate-plan / AGC registration): bundle + ability only.
  private buildWant(): Want {
    return {
      bundleName: BUNDLE_NAME,
      abilityName: EXTENSION_ABILITY
    };
  }

  private requestStart(): void {
    if (this.requestSent) {
      // Request already sent this attempt: no repeat click, no re-issue.
      return;
    }
    this.requestSent = true;
    this.statusText = 'SENT';
    this.detailText = 'awaiting system vpn authorization';
    hilog.info(DOMAIN, TAG, 'UI_START_REQUEST|bundle=%{public}s|ability=%{public}s',
      BUNDLE_NAME, EXTENSION_ABILITY);
    vpnExtension.startVpnExtensionAbility(this.buildWant()).then(() => {
      this.statusText = 'STARTED';
      this.detailText = 'start promise resolved';
      hilog.info(DOMAIN, TAG, 'UI_START_RESOLVED|bundle=%{public}s', BUNDLE_NAME);
    }).catch((error: Error) => {
      const code: string = String((error as BusinessFailure).code ?? 'none');
      this.statusText = `ERR:${code}`;
      this.detailText = 'start promise rejected (no retry)';
      hilog.error(DOMAIN, TAG, 'UI_START_REJECTED|bundle=%{public}s|code=%{public}s',
        BUNDLE_NAME, code);
    });
  }

  build() {
    Column({ space: 18 }) {
      Text('N1BDISC physical HAP skeleton')
        .fontSize(24)
        .fontWeight(FontWeight.Bold)
        .fontColor('#17324D')
      Text(BUNDLE_NAME)
        .fontSize(14)
        .fontColor('#3D4A55')
        .textAlign(TextAlign.Center)
      Button(this.requestSent ? 'Request sent' : 'Trigger VPN start')
        .fontSize(17)
        .width(220)
        .height(48)
        .backgroundColor(this.requestSent ? '#8A9AA6' : '#17324D')
        .enabled(!this.requestSent)
        .onClick(() => {
          this.requestStart();
        })
      Text(this.statusText)
        .fontSize(19)
        .fontWeight(FontWeight.Medium)
        .fontColor(this.statusText.startsWith('ERR') ? '#A12622' : '#176B3A')
      Text(this.detailText)
        .fontSize(13)
        .fontColor('#3D4A55')
        .textAlign(TextAlign.Center)
        .maxLines(2)
    }
    .width('100%')
    .height('100%')
    .justifyContent(FlexAlign.Center)
    .padding(24)
    .backgroundColor('#F2F6F8')
  }
}
