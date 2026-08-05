import Want from '@ohos.app.ability.Want';
import hilog from '@ohos.hilog';
import vpnExtension from '@ohos.net.vpnExtension';

const DOMAIN: number = 0x2900;
const TAG: string = 'E3PhysVpn';
const EXTENSION_ABILITY: string = 'E3PhysicalVpnExtensionAbility';

interface BusinessFailure extends Error {
  code?: number;
}

function clean(value: string): string {
  return value.replace(/\|/g, '/').replace(/\r/g, ' ').replace(/\n/g, ' ').slice(0, 160);
}

function safeError(error: Error): string {
  const code: string = String((error as BusinessFailure).code ?? 'none');
  const name: string = clean(error.name || 'Error');
  const message: string = clean(error.message || 'no-message');
  return `code=${code},name=${name},message=${message}`;
}

@Entry
@Component
struct Index {
  @StorageLink('bundleName') bundleName: string = 'unknown';
  @StorageLink('operationStatus') operationStatus: string = 'Ready';
  @StorageLink('operationDetail') operationDetail: string = 'No request';
  @State private operationPending: boolean = false;
  @State private activeRequestId: string = '';
  private requestSequence: number = 0;

  private nextRequestId(): string {
    this.requestSequence++;
    return `${this.bundleName}-${Date.now()}-${this.requestSequence}`;
  }

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
    if (this.operationPending) {
      hilog.info(DOMAIN, TAG,
        'UI_START_SKIPPED|bundle=%{public}s|reason=operation-pending', this.bundleName);
      return;
    }
    if (this.activeRequestId.length > 0) {
      hilog.info(DOMAIN, TAG,
        'UI_START_SKIPPED|bundle=%{public}s|requestId=%{public}s|reason=active-request',
        this.bundleName, this.activeRequestId);
      return;
    }

    const requestId: string = this.nextRequestId();
    this.activeRequestId = requestId;
    this.operationPending = true;
    this.operationStatus = 'Start pending';
    this.operationDetail = requestId;
    hilog.info(DOMAIN, TAG, 'UI_START|bundle=%{public}s|requestId=%{public}s', this.bundleName, requestId);

    vpnExtension.startVpnExtensionAbility(this.want(requestId)).then(() => {
      this.operationPending = false;
      this.operationStatus = 'Start resolved';
      this.operationDetail = requestId;
      hilog.info(DOMAIN, TAG,
        'START_PROMISE_RESOLVED|bundle=%{public}s|requestId=%{public}s', this.bundleName, requestId);
    }).catch((error: Error) => {
      const summary: string = safeError(error);
      if (this.activeRequestId === requestId) {
        this.activeRequestId = '';
      }
      this.operationPending = false;
      this.operationStatus = 'Start rejected';
      this.operationDetail = `${requestId} | ${summary}`;
      hilog.error(DOMAIN, TAG,
        'START_PROMISE_REJECTED|bundle=%{public}s|requestId=%{public}s|summary=%{public}s',
        this.bundleName, requestId, summary);
    });
  }

  private stopVpn(): void {
    if (this.operationPending) {
      hilog.info(DOMAIN, TAG,
        'UI_STOP_SKIPPED|bundle=%{public}s|reason=operation-pending', this.bundleName);
      return;
    }
    if (this.activeRequestId.length === 0) {
      this.operationStatus = 'Stop skipped';
      this.operationDetail = 'No active request';
      hilog.info(DOMAIN, TAG,
        'UI_STOP_SKIPPED|bundle=%{public}s|reason=no-active-request', this.bundleName);
      return;
    }

    const requestId: string = this.activeRequestId;
    this.operationPending = true;
    this.operationStatus = 'Stop pending';
    this.operationDetail = requestId;
    hilog.info(DOMAIN, TAG, 'UI_STOP|bundle=%{public}s|requestId=%{public}s', this.bundleName, requestId);

    vpnExtension.stopVpnExtensionAbility(this.want(requestId)).then(() => {
      if (this.activeRequestId === requestId) {
        this.activeRequestId = '';
      }
      this.operationPending = false;
      this.operationStatus = 'Stop resolved';
      this.operationDetail = requestId;
      hilog.info(DOMAIN, TAG,
        'STOP_PROMISE_RESOLVED|bundle=%{public}s|requestId=%{public}s', this.bundleName, requestId);
    }).catch((error: Error) => {
      const code: number | undefined = (error as BusinessFailure).code;
      const summary: string = safeError(error);
      if (code === 16000001 && this.activeRequestId === requestId) {
        this.activeRequestId = '';
        this.operationStatus = 'External teardown reconciled';
        this.operationDetail = 'Extension no longer exists; Start is available';
        hilog.info(DOMAIN, TAG,
          'STOP_SESSION_RELEASED|bundle=%{public}s|requestId=%{public}s|reason=ability-not-found',
          this.bundleName, requestId);
      } else {
        this.operationStatus = 'Stop rejected';
        this.operationDetail = `${requestId} | ${summary}`;
      }
      this.operationPending = false;
      hilog.error(DOMAIN, TAG,
        'STOP_PROMISE_REJECTED|bundle=%{public}s|requestId=%{public}s|summary=%{public}s',
        this.bundleName, requestId, summary);
    });
  }

  build() {
    Column({ space: 18 }) {
      Text('E3 physical VPN preflight')
        .fontSize(26)
        .fontWeight(FontWeight.Bold)
        .fontColor('#17324D')
      Text(this.bundleName)
        .fontSize(14)
        .fontColor('#3D4A55')
        .textAlign(TextAlign.Center)
      Text(this.operationStatus)
        .fontSize(18)
        .fontWeight(FontWeight.Medium)
        .fontColor(this.operationStatus.includes('rejected') ? '#A12622' : '#176B3A')
      Text(this.operationDetail)
        .fontSize(13)
        .fontColor('#3D4A55')
        .textAlign(TextAlign.Center)
        .maxLines(4)
      Row({ space: 16 }) {
        Button('Start')
          .id('start-vpn')
          .fontSize(17)
          .width(148)
          .height(52)
          .backgroundColor('#176B3A')
          .enabled(!this.operationPending && this.activeRequestId.length === 0)
          .onClick(() => this.startVpn())
        Button('Stop')
          .id('stop-vpn')
          .fontSize(17)
          .width(148)
          .height(52)
          .backgroundColor('#A12622')
          .enabled(!this.operationPending)
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
