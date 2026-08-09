import Want from '@ohos.app.ability.Want';
import common from '@ohos.app.ability.common';
import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
import vpnExtension from '@ohos.net.vpnExtension';

const DOMAIN: number = 0x2900;
const TAG: string = 'E3PhysVpn';
const EXTENSION_ABILITY: string = 'E3PhysicalVpnExtensionAbility';
const LEDGER_FILE: string = 'e3-phys-request-ledger.txt';
const MAX_REQUEST_ID_LENGTH: number = 96;
const PENDING_RELEASE_TIMEOUT_MS: number = 65000;

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

function isValidRequestId(value: string): boolean {
  if (value.length === 0 || value.length > MAX_REQUEST_ID_LENGTH) { return false; }
  for (let i = 0; i < value.length; i++) {
    const ch: string = value.charAt(i);
    const code: number = ch.charCodeAt(0);
    const alnum: boolean = (code >= 48 && code <= 57) || (code >= 65 && code <= 90) || (code >= 97 && code <= 122);
    if (!alnum && ch !== '.' && ch !== '_' && ch !== '-') { return false; }
  }
  return true;
}

@Entry
@Component
struct Index {
  @StorageLink('bundleName') bundleName: string = 'unknown';
  @StorageLink('operationStatus') operationStatus: string = 'Ready';
  @StorageLink('operationDetail') operationDetail: string = 'No request';
  @State private operationPending: boolean = false;
  @State private activeRequestId: string = '';
  private lastRequestId: string = '';
  private requestSequence: number = 0;
  private startGeneration: number = 0;
  private pendingReleaseTimer: number = -1;

  aboutToDisappear(): void {
    this.clearPendingRelease();
  }

  private nextRequestId(): string {
    this.requestSequence++;
    const requestId: string = `${this.bundleName}-${Date.now()}-${this.requestSequence}`;
    return isValidRequestId(requestId) ? requestId : `${this.bundleName}-${Date.now()}`;
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

  private writeRequestLedger(requestId: string): void {
    const context = getContext(this) as common.UIAbilityContext;
    const ledgerPath: string = `${context.filesDir}/${LEDGER_FILE}`;
    const payload: string = `${requestId}\n${Date.now()}\n`;
    let fd: number = -1;
    try {
      const opened = fs.openSync(ledgerPath, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      fd = opened.fd;
      fs.writeSync(fd, payload);
      hilog.info(DOMAIN, TAG,
        'LEDGER_PERSISTED|bundle=%{public}s|requestId=%{public}s', this.bundleName, requestId);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'LEDGER_WRITE_REJECTED|bundle=%{public}s|requestId=%{public}s|summary=%{public}s',
        this.bundleName, requestId, safeError(error as Error));
    } finally {
      if (fd >= 0) {
        try {
          fs.closeSync(fd);
        } catch (closeError) {
          hilog.error(DOMAIN, TAG,
            'LEDGER_WRITE_REJECTED|bundle=%{public}s|requestId=%{public}s|summary=%{public}s',
            this.bundleName, requestId, safeError(closeError as Error));
        }
      }
    }
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
    this.startGeneration++;
    const generation: number = this.startGeneration;
    this.activeRequestId = requestId;
    // A new Start overwrites the last-known request id so a later Stop always targets the
    // most recent request; the bounded pending release never clears this field.
    this.lastRequestId = requestId;
    this.operationPending = true;
    this.operationStatus = 'Start pending';
    this.operationDetail = requestId;
    this.writeRequestLedger(requestId);
    hilog.info(DOMAIN, TAG, 'UI_START|bundle=%{public}s|requestId=%{public}s', this.bundleName, requestId);
    this.schedulePendingRelease(requestId, generation);

    vpnExtension.startVpnExtensionAbility(this.want(requestId)).then(() => {
      if (this.startGeneration !== generation) {
        hilog.info(DOMAIN, TAG,
          'START_PROMISE_LATE_RESOLVED|bundle=%{public}s|requestId=%{public}s', this.bundleName, requestId);
        return;
      }
      this.clearPendingRelease();
      this.operationPending = false;
      // Accurate copy: the start request/ability settled, but VPN create is still governed by
      // the Extension lifecycle events, so this never implies an active VPN.
      this.operationStatus = 'Start request resolved';
      this.operationDetail = `${requestId} | VPN create follows extension events`;
      hilog.info(DOMAIN, TAG,
        'START_PROMISE_RESOLVED|bundle=%{public}s|requestId=%{public}s', this.bundleName, requestId);
    }).catch((error: Error) => {
      const summary: string = safeError(error);
      if (this.startGeneration !== generation) {
        hilog.error(DOMAIN, TAG,
          'START_PROMISE_LATE_REJECTED|bundle=%{public}s|requestId=%{public}s|summary=%{public}s',
          this.bundleName, requestId, summary);
        return;
      }
      this.clearPendingRelease();
      if (this.activeRequestId === requestId) {
        this.activeRequestId = '';
      }
      // A terminal start rejection leaves no session to stop; clean the last-known id as well.
      if (this.lastRequestId === requestId) {
        this.lastRequestId = '';
      }
      this.operationPending = false;
      this.operationStatus = 'Start rejected';
      this.operationDetail = `${requestId} | ${summary}`;
      hilog.error(DOMAIN, TAG,
        'START_PROMISE_REJECTED|bundle=%{public}s|requestId=%{public}s|summary=%{public}s',
        this.bundleName, requestId, summary);
    });
  }

  private schedulePendingRelease(requestId: string, generation: number): void {
    this.clearPendingRelease();
    this.pendingReleaseTimer = setTimeout(() => {
      this.pendingReleaseTimer = -1;
      if (this.startGeneration !== generation) { return; }
      if (this.activeRequestId !== requestId) { return; }
      // Advance generation so any late then/catch for this start can only emit LATE markers.
      this.startGeneration++;
      this.activeRequestId = '';
      this.operationPending = false;
      this.operationStatus = 'Start available';
      this.operationDetail = 'Pending start released after bounded timeout';
      hilog.info(DOMAIN, TAG,
        'START_PENDING_RELEASED|bundle=%{public}s|requestId=%{public}s|reason=bounded-timeout|thresholdMs=%{public}d',
        this.bundleName, requestId, PENDING_RELEASE_TIMEOUT_MS);
    }, PENDING_RELEASE_TIMEOUT_MS);
  }

  private clearPendingRelease(): void {
    if (this.pendingReleaseTimer !== -1) {
      clearTimeout(this.pendingReleaseTimer);
      this.pendingReleaseTimer = -1;
    }
  }

  private stopVpn(): void {
    if (this.operationPending) {
      hilog.info(DOMAIN, TAG,
        'UI_STOP_SKIPPED|bundle=%{public}s|reason=operation-pending', this.bundleName);
      return;
    }
    if (this.activeRequestId.length === 0 && this.lastRequestId.length === 0) {
      this.operationStatus = 'Stop skipped';
      this.operationDetail = 'No active or last-known request';
      hilog.info(DOMAIN, TAG,
        'UI_STOP_SKIPPED|bundle=%{public}s|reason=no-active-request', this.bundleName);
      return;
    }

    // Stop prefers the active request; when the bounded pending release already cleared it
    // (e.g. the first S3 Stop after a long gap), fall back to the last-known request id so the
    // first Stop is not skipped and no manual second Start is needed.
    const requestId: string = this.activeRequestId.length > 0 ? this.activeRequestId : this.lastRequestId;
    const basis: string = this.activeRequestId.length > 0 ? 'active-request' : 'last-known-request';
    this.operationPending = true;
    this.operationStatus = 'Stop pending';
    this.operationDetail = requestId;
    hilog.info(DOMAIN, TAG, 'UI_STOP|bundle=%{public}s|requestId=%{public}s|basis=%{public}s',
      this.bundleName, requestId, basis);

    vpnExtension.stopVpnExtensionAbility(this.want(requestId)).then(() => {
      if (this.activeRequestId === requestId) {
        this.activeRequestId = '';
      }
      if (this.lastRequestId === requestId) {
        this.lastRequestId = '';
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
      } else if (code === 16000001) {
        // Terminal rejection on the last-known request: the session no longer exists.
        if (this.lastRequestId === requestId) {
          this.lastRequestId = '';
        }
        this.operationStatus = 'External teardown reconciled';
        this.operationDetail = 'Extension no longer exists; Start is available';
        hilog.info(DOMAIN, TAG,
          'STOP_SESSION_RELEASED_LAST_KNOWN|bundle=%{public}s|requestId=%{public}s|reason=ability-not-found',
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
