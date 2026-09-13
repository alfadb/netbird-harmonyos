import Want from '@ohos.app.ability.Want';
import hilog from '@ohos.hilog';
import vpnExtension from '@ohos.net.vpnExtension';
import { ConnectorStatusResult, pollStatusOrNull } from '../vpnextensionability/NetBirdConnector';

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';
const BUNDLE_NAME: string = 'cn.alfadb.netbird';
const EXTENSION_ABILITY: string = 'NetBirdVpnExtensionAbility';
const STATUS_POLL_MS: number = 2000;

interface BusinessFailure extends Error {
  code?: number;
}

// Minimal trigger + status surface (promoted from the n1b skeleton, extended
// in N3-6): one click-gated start and one click-gated stop request per
// attempt; each latch never resets (reject does not re-issue, no retry).
// Status rows poll core.connector_status() directly in THIS process —
// documented limitation: if the platform runs the VpnExtensionAbility in a
// separate process, the UI sees its own empty snapshot (state 'unknown' /
// 'disconnected'); no cross-process status channel exists in this increment.
// No UI design here.
@Entry
@Component
struct Index {
  @State statusText: string = 'READY';
  @State detailText: string = 'tap to send one start request';
  @State requestSent: boolean = false;
  @State stopSent: boolean = false;
  @State connectorState: string = 'unknown';
  @State peerCount: number = 0;
  @State routeCount: number = 0;
  @State lastErrorClass: string = 'none';
  // N5d: real signal link state.
  @State signalRegistered: boolean = false;
  @State signalReconnects: number = 0;
  private requestId: string = `ui-${Date.now()}`;
  private pollTimer: number = -1;

  aboutToAppear(): void {
    this.pollConnectorStatus();
    this.pollTimer = setInterval((): void => {
      this.pollConnectorStatus();
    }, STATUS_POLL_MS);
  }

  aboutToDisappear(): void {
    if (this.pollTimer >= 0) {
      clearInterval(this.pollTimer);
      this.pollTimer = -1;
    }
  }

  private pollConnectorStatus(): void {
    const status: ConnectorStatusResult | null = pollStatusOrNull();
    if (status === null) {
      this.connectorState = 'unavailable';
      return;
    }
    this.connectorState = status.state;
    this.peerCount = status.peer_count;
    this.routeCount = status.route_count;
    this.lastErrorClass =
      status.last_error !== null && status.last_error !== undefined
        ? status.last_error.class
        : 'none';
    // N5d: the real signal link (registered = offer/answer/candidate path up)
    this.signalRegistered = status.signal !== undefined && status.signal !== null
      ? status.signal.registered
      : false;
    this.signalReconnects = status.signal !== undefined && status.signal !== null
      ? status.signal.reconnects
      : 0;
  }

  private buildWant(): Want {
    const params: Record<string, Object> = { 'requestId': this.requestId };
    const want: Want = {
      bundleName: BUNDLE_NAME,
      abilityName: EXTENSION_ABILITY,
      parameters: params
    };
    return want;
  }

  private requestStart(): void {
    if (this.requestSent) {
      // Request already sent this attempt: no repeat click, no re-issue.
      return;
    }
    this.requestSent = true;
    this.statusText = 'SENT';
    this.detailText = 'awaiting system vpn authorization';
    hilog.info(DOMAIN, TAG, 'UI_START_REQUEST|bundle=%{public}s|ability=%{public}s|requestId=%{public}s',
      BUNDLE_NAME, EXTENSION_ABILITY, this.requestId);
    vpnExtension.startVpnExtensionAbility(this.buildWant()).then(() => {
      this.statusText = 'STARTED';
      this.detailText = 'start promise resolved';
      hilog.info(DOMAIN, TAG, 'UI_START_RESOLVED|bundle=%{public}s|requestId=%{public}s',
        BUNDLE_NAME, this.requestId);
    }).catch((error: Error) => {
      const code: string = String((error as BusinessFailure).code ?? 'none');
      this.statusText = `ERR:${code}`;
      this.detailText = 'start promise rejected (no retry)';
      hilog.error(DOMAIN, TAG, 'UI_START_REJECTED|bundle=%{public}s|code=%{public}s|requestId=%{public}s',
        BUNDLE_NAME, code, this.requestId);
    });
  }

  private requestStop(): void {
    if (!this.requestSent || this.stopSent) {
      // Stop is only meaningful after a start request; one attempt, no retry.
      return;
    }
    this.stopSent = true;
    this.statusText = 'STOP_SENT';
    this.detailText = 'stop request in flight';
    hilog.info(DOMAIN, TAG, 'UI_STOP_REQUEST|bundle=%{public}s|requestId=%{public}s',
      BUNDLE_NAME, this.requestId);
    vpnExtension.stopVpnExtensionAbility(this.buildWant()).then(() => {
      this.statusText = 'STOPPED';
      this.detailText = 'stop promise resolved';
      hilog.info(DOMAIN, TAG, 'UI_STOP_RESOLVED|bundle=%{public}s|requestId=%{public}s',
        BUNDLE_NAME, this.requestId);
    }).catch((error: Error) => {
      const code: string = String((error as BusinessFailure).code ?? 'none');
      this.statusText = `ERR:${code}`;
      this.detailText = 'stop promise rejected (no retry)';
      hilog.error(DOMAIN, TAG, 'UI_STOP_REJECTED|bundle=%{public}s|code=%{public}s|requestId=%{public}s',
        BUNDLE_NAME, code, this.requestId);
    });
  }

  build() {
    Column({ space: 18 }) {
      Text('NetBird HarmonyOS client')
        .fontSize(24)
        .fontWeight(FontWeight.Bold)
        .fontColor('#17324D')
      Text(BUNDLE_NAME)
        .fontSize(14)
        .fontColor('#3D4A55')
        .textAlign(TextAlign.Center)
      // Minimal connector status rows (control plane state, peer count,
      // route count, last error classification). Data: core.connector_status.
      Text(`state: ${this.connectorState}`)
        .fontSize(16)
        .fontColor('#17324D')
      Text(`peers: ${this.peerCount}   routes: ${this.routeCount}`)
        .fontSize(16)
        .fontColor('#17324D')
      // N5d: signal channel status — registered / reconnect count.
      Text(`signal: ${this.signalRegistered ? 'registered' : 'not registered'}   ` +
        `reconnects: ${this.signalReconnects}`)
        .fontSize(16)
        .fontColor(this.signalRegistered ? '#176B3A' : '#8A6D1A')
      Text(`last error: ${this.lastErrorClass}`)
        .fontSize(16)
        .fontColor(this.lastErrorClass === 'none' ? '#176B3A' : '#A12622')
      Button(this.requestSent ? 'Request sent' : 'Connect VPN')
        .fontSize(17)
        .width(220)
        .height(48)
        .backgroundColor(this.requestSent ? '#8A9AA6' : '#17324D')
        .enabled(!this.requestSent)
        .onClick(() => {
          hilog.info(DOMAIN, TAG, 'UI_START_BUTTON_CLICKED|bundle=%{public}s', BUNDLE_NAME);
          this.requestStart();
        })
      Text(this.statusText)
        .fontSize(19)
        .fontWeight(FontWeight.Medium)
        .fontColor(this.statusText.startsWith('ERR') ? '#A12622' : '#176B3A')
      Button(this.stopSent ? 'Stop requested' : 'Stop VPN')
        .fontSize(17)
        .width(220)
        .height(44)
        .backgroundColor(this.stopSent || !this.requestSent ? '#8A9AA6' : '#5A3A6E')
        .enabled(this.requestSent && !this.stopSent)
        .onClick(() => {
          hilog.info(DOMAIN, TAG, 'UI_STOP_BUTTON_CLICKED|bundle=%{public}s', BUNDLE_NAME);
          this.requestStop();
        })
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
