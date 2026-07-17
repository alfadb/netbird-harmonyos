import Want from '@ohos.app.ability.Want';
import hilog from '@ohos.hilog';
import vpnExtension from '@ohos.net.vpnExtension';

const DOMAIN: number = 0x2900;
const TAG: string = 'E3VpnGate';
const EXTENSION_ABILITY: string = 'E3VpnExtensionAbility';

interface BusinessFailure extends Error {
  code?: number;
}

function clean(value: string): string {
  return value.replace(/\|/g, '/').replace(/\r/g, ' ').replace(/\n/g, ' ');
}

@Entry
@Component
struct Index {
  @StorageLink('bundleName') bundleName: string = 'unknown';
  @StorageLink('operationStatus') operationStatus: string = 'Ready';
  @StorageLink('operationDetail') operationDetail: string = 'No VPN Extension request has been made';
  private requestSequence: number = 0;

  private want(requestId: string): Want {
    return {
      deviceId: '',
      bundleName: this.bundleName,
      abilityName: EXTENSION_ABILITY,
      parameters: {
        requestId: requestId
      }
    };
  }

  private startVpn(): void {
    this.requestSequence++;
    const requestId: string = `${this.bundleName}-${Date.now()}-${this.requestSequence}`;
    const startedAt: number = Date.now();
    this.operationStatus = 'Start requested';
    this.operationDetail = requestId;
    hilog.info(DOMAIN, TAG, 'UI_START|bundle=%{public}s|requestId=%{public}s', this.bundleName, requestId);
    vpnExtension.startVpnExtensionAbility(this.want(requestId)).then(() => {
      const elapsedMs: number = Date.now() - startedAt;
      this.operationStatus = 'Start promise resolved';
      this.operationDetail = `${requestId} | ${elapsedMs} ms`;
      hilog.info(DOMAIN, TAG,
        'START_PROMISE|bundle=%{public}s|requestId=%{public}s|settled=resolved|elapsedMs=%{public}d',
        this.bundleName, requestId, elapsedMs);
    }).catch((error: Error) => {
      const elapsedMs: number = Date.now() - startedAt;
      const value: object = error as object;
      const detail: string = clean(JSON.stringify(value));
      const code: string = String((error as BusinessFailure).code ?? 'none');
      this.operationStatus = 'Start promise rejected';
      this.operationDetail = `${requestId} | code ${code}`;
      hilog.error(DOMAIN, TAG,
        'START_PROMISE|bundle=%{public}s|requestId=%{public}s|settled=rejected|' +
        'elapsedMs=%{public}d|code=%{public}s|detail=%{public}s',
        this.bundleName, requestId, elapsedMs, code, detail);
    });
  }

  private stopVpn(): void {
    this.requestSequence++;
    const requestId: string = `${this.bundleName}-stop-${Date.now()}-${this.requestSequence}`;
    const startedAt: number = Date.now();
    this.operationStatus = 'Stop requested';
    this.operationDetail = requestId;
    hilog.info(DOMAIN, TAG, 'UI_STOP|bundle=%{public}s|requestId=%{public}s', this.bundleName, requestId);
    vpnExtension.stopVpnExtensionAbility(this.want(requestId)).then(() => {
      const elapsedMs: number = Date.now() - startedAt;
      this.operationStatus = 'Stop promise resolved';
      this.operationDetail = `${requestId} | ${elapsedMs} ms`;
      hilog.info(DOMAIN, TAG,
        'STOP_PROMISE|bundle=%{public}s|requestId=%{public}s|settled=resolved|elapsedMs=%{public}d',
        this.bundleName, requestId, elapsedMs);
    }).catch((error: Error) => {
      const elapsedMs: number = Date.now() - startedAt;
      const value: object = error as object;
      const detail: string = clean(JSON.stringify(value));
      const code: string = String((error as BusinessFailure).code ?? 'none');
      this.operationStatus = 'Stop promise rejected';
      this.operationDetail = `${requestId} | code ${code}`;
      hilog.error(DOMAIN, TAG,
        'STOP_PROMISE|bundle=%{public}s|requestId=%{public}s|settled=rejected|' +
        'elapsedMs=%{public}d|code=%{public}s|detail=%{public}s',
        this.bundleName, requestId, elapsedMs, code, detail);
    });
  }

  build() {
    Column({ space: 18 }) {
      Text('E3 VPN Extension gate')
        .fontSize(26)
        .fontWeight(FontWeight.Bold)
        .fontColor('#17324D')
      Text(this.bundleName)
        .fontSize(14)
        .fontColor('#3D4A55')
        .textAlign(TextAlign.Center)
      Text(this.operationStatus)
        .fontSize(19)
        .fontWeight(FontWeight.Medium)
        .fontColor(this.operationStatus.includes('rejected') ? '#A12622' : '#176B3A')
      Text(this.operationDetail)
        .fontSize(13)
        .fontColor('#3D4A55')
        .textAlign(TextAlign.Center)
        .maxLines(3)
      Row({ space: 16 }) {
        Button('Start VPN')
          .id('start-vpn')
          .fontSize(17)
          .width(150)
          .height(52)
          .backgroundColor('#176B3A')
          .onClick(() => this.startVpn())
        Button('Stop VPN')
          .id('stop-vpn')
          .fontSize(17)
          .width(150)
          .height(52)
          .backgroundColor('#A12622')
          .onClick(() => this.stopVpn())
      }
    }
    .width('100%')
    .height('100%')
    .justifyContent(FlexAlign.Center)
    .padding(24)
    .backgroundColor('#F2F6F8')
  }
}
