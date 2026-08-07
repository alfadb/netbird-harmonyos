import VpnExtensionAbility from '@ohos.app.ability.VpnExtensionAbility';
import Want from '@ohos.app.ability.Want';
import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
import vpnExtension from '@ohos.net.vpnExtension';
import fdprobe from 'libfdprobe.so';
import { FdStatus } from 'libfdprobe.so';

const DOMAIN: number = 0x2900;
const TAG: string = 'E3PhysVpn';
const MAX_NATIVE_FD: number = 2147483647;
const LEDGER_FILE: string = 'e3-phys-request-ledger.txt';
const MAX_REQUEST_ID_LENGTH: number = 96;
const LEDGER_MAX_FUTURE_MS: number = 30000;
const LEDGER_MAX_AGE_MS: number = 300000;
const LEDGER_READ_BUFFER_BYTES: number = 512;

interface BusinessFailure extends Error {
  code?: number;
}

interface CreateOutcome {
  destroyRequired: boolean;
  accepted: boolean;
  reason: string;
}

interface FdSnapshot {
  fd: number;
  integer: boolean;
  nonnegative: boolean;
  probeEligible: boolean;
  probed: boolean;
  open: boolean;
  errno: number;
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

function bytesToAscii(buffer: ArrayBuffer, length: number): string {
  const view: Uint8Array = new Uint8Array(buffer, 0, length);
  let out: string = '';
  for (let i = 0; i < length; i++) {
    out += String.fromCharCode(view[i]);
  }
  return out;
}

interface LedgerConsumeResult {
  requestId: string;
  usable: boolean;
}

export default class E3PhysicalVpnExtensionAbility extends VpnExtensionAbility {
  private requestId: string = 'missing';
  private requestSource: string = 'missing';
  private connectionCreationAttempted: boolean = false;
  private connection?: vpnExtension.VpnConnection;
  private createPromise?: Promise<CreateOutcome>;
  private createSettled: boolean = false;
  private destroyPromise?: Promise<void>;
  private destroyTrigger: string = 'none';
  private tunFd: number = -1;
  private fdConfirmedOpenAfterCreate: boolean = false;

  onCreate(want: Want): void {
    const wantRequestId: string = String(want.parameters?.['requestId'] ?? '');
    // Always read+consume the sandbox ledger, even when Want already carries a requestId.
    const ledger: LedgerConsumeResult = this.consumeRequestLedger();
    let requestSource: string = 'missing';
    if (wantRequestId.length > 0 && !isValidRequestId(wantRequestId)) {
      hilog.error(DOMAIN, TAG,
        'VPN_REQUESTID_INVALID|requestId=%{public}s|reason=want-format-or-length', clean(wantRequestId));
    }
    if (isValidRequestId(wantRequestId)) {
      this.requestId = wantRequestId;
      requestSource = 'want';
      if (ledger.usable && ledger.requestId !== wantRequestId) {
        hilog.error(DOMAIN, TAG,
          'LEDGER_REQUESTID_MISMATCH|want=%{public}s|ledger=%{public}s',
          clean(wantRequestId), clean(ledger.requestId));
      }
    } else if (ledger.usable) {
      this.requestId = ledger.requestId;
      requestSource = 'sandbox';
    } else {
      this.requestId = 'missing';
      requestSource = 'missing';
      if (ledger.requestId.length > 0 && !isValidRequestId(ledger.requestId)) {
        hilog.error(DOMAIN, TAG,
          'VPN_REQUESTID_INVALID|requestId=%{public}s|reason=ledger-format-or-length', clean(ledger.requestId));
      }
    }
    this.requestSource = requestSource;
    hilog.info(DOMAIN, TAG, 'VPN_ONCREATE|bundle=%{public}s|requestId=%{public}s|requestSource=%{public}s',
      want.bundleName ?? 'unknown', this.requestId, requestSource);

    if (this.connectionCreationAttempted) {
      hilog.error(DOMAIN, TAG,
        'VPN_CREATE_REJECTED|requestId=%{public}s|phase=duplicate-onCreate|summary=connection-already-attempted',
        this.requestId);
      return;
    }
    this.connectionCreationAttempted = true;

    let connection: vpnExtension.VpnConnection;
    try {
      connection = vpnExtension.createVpnConnection(this.context);
      this.connection = connection;
      hilog.info(DOMAIN, TAG, 'VPN_CONNECTION_CREATED|requestId=%{public}s', this.requestId);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_CREATE_REJECTED|requestId=%{public}s|phase=create-connection|summary=%{public}s',
        this.requestId, safeError(error as Error));
      return;
    }

    const config: vpnExtension.VpnConfig = {
      addresses: [
        {
          address: {
            address: '192.0.2.1',
            family: 1
          },
          prefixLength: 32
        }
      ]
    };

    hilog.info(DOMAIN, TAG,
      'VPN_CREATE_BEGIN|requestId=%{public}s|address=192.0.2.1|prefixLength=32', this.requestId);
    try {
      const rawCreate: Promise<number> = connection.create(config);
      this.createPromise = rawCreate.then((fd: number): CreateOutcome => {
        this.createSettled = true;
        this.tunFd = fd;
        const snapshot: FdSnapshot = this.captureFdStatus();
        const accepted: boolean = snapshot.integer && snapshot.nonnegative && snapshot.probed && snapshot.open;
        const marker: string = accepted ? 'CREATE_ACCEPTED' : 'CREATE_INVALID_DESTROY_REQUIRED';
        this.fdConfirmedOpenAfterCreate = accepted;
        this.logFdSnapshot('post-create', snapshot, marker);

        hilog.info(DOMAIN, TAG,
          'VPN_CREATE_RESOLVED|requestId=%{public}s|fd=%{public}s|accepted=%{public}s|marker=%{public}s',
          this.requestId, String(fd), String(accepted), marker);
        if (accepted) {
          return {
            destroyRequired: true,
            accepted: true,
            reason: 'create-open-fd-confirmed'
          };
        }

        hilog.error(DOMAIN, TAG,
          'VPN_CREATE_INVALID_FD|requestId=%{public}s|fd=%{public}s|marker=CREATE_INVALID_DESTROY_REQUIRED',
          this.requestId, String(fd));
        this.destroyOnce('invalid-create-resolution').catch((error: Error) => {
          hilog.error(DOMAIN, TAG,
            'VPN_DESTROY_REJECTED|requestId=%{public}s|phase=invalid-create-unexpected|summary=%{public}s',
            this.requestId, safeError(error));
        });
        return {
          destroyRequired: true,
          accepted: false,
          reason: 'create-resolved-invalid-or-not-open'
        };
      }, (error: Error): CreateOutcome => {
        this.createSettled = true;
        hilog.error(DOMAIN, TAG,
          'VPN_CREATE_REJECTED|requestId=%{public}s|phase=create|summary=%{public}s',
          this.requestId, safeError(error));
        return {
          destroyRequired: false,
          accepted: false,
          reason: 'create-rejected'
        };
      });
    } catch (error) {
      this.createSettled = true;
      hilog.error(DOMAIN, TAG,
        'VPN_CREATE_REJECTED|requestId=%{public}s|phase=create-sync|summary=%{public}s',
        this.requestId, safeError(error as Error));
    }
  }

  onDestroy(): void {
    hilog.info(DOMAIN, TAG, 'VPN_ONDESTROY|requestId=%{public}s', this.requestId);
    this.destroyOnce('onDestroy').catch((error: Error) => {
      hilog.error(DOMAIN, TAG,
        'VPN_DESTROY_REJECTED|requestId=%{public}s|phase=unexpected|summary=%{public}s',
        this.requestId, safeError(error));
    });
  }

  private consumeRequestLedger(): LedgerConsumeResult {
    const empty: LedgerConsumeResult = { requestId: '', usable: false };
    const ledgerPath: string = `${this.context.filesDir}/${LEDGER_FILE}`;
    let content: string = '';
    let fd: number = -1;
    try {
      const opened = fs.openSync(ledgerPath, fs.OpenMode.READ_ONLY);
      fd = opened.fd;
      const buffer: ArrayBuffer = new ArrayBuffer(LEDGER_READ_BUFFER_BYTES);
      const readLen: number = fs.readSync(fd, buffer);
      content = bytesToAscii(buffer, readLen);
    } catch (error) {
      const summary: string = safeError(error as Error);
      const code: number | undefined = (error as BusinessFailure).code;
      if (code === 13900002 || summary.indexOf('No such file') >= 0 || summary.indexOf('no such file') >= 0) {
        hilog.info(DOMAIN, TAG,
          'LEDGER_MISSING|requestId=%{public}s|summary=%{public}s', this.requestId, summary);
      } else {
        hilog.error(DOMAIN, TAG,
          'LEDGER_READ_REJECTED|requestId=%{public}s|summary=%{public}s', this.requestId, summary);
      }
      return empty;
    } finally {
      if (fd >= 0) {
        try {
          fs.closeSync(fd);
        } catch (closeError) {
          hilog.error(DOMAIN, TAG,
            'LEDGER_READ_REJECTED|requestId=%{public}s|summary=%{public}s',
            this.requestId, safeError(closeError as Error));
        }
      }
    }

    const lines: string[] = content.split('\n');
    const candidate: string = lines.length > 0 ? lines[0].trim() : '';
    const timestampText: string = lines.length > 1 ? lines[1].trim() : '';
    hilog.info(DOMAIN, TAG, 'LEDGER_READ_RESOLVED|requestId=%{public}s', clean(candidate));
    try {
      fs.unlinkSync(ledgerPath);
      hilog.info(DOMAIN, TAG, 'LEDGER_CONSUME_RESOLVED|requestId=%{public}s', clean(candidate));
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'LEDGER_CONSUME_REJECTED|requestId=%{public}s|summary=%{public}s',
        clean(candidate), safeError(error as Error));
    }

    if (!isValidRequestId(candidate)) {
      return { requestId: candidate, usable: false };
    }
    if (timestampText.length === 0 || !/^[0-9]+$/.test(timestampText)) {
      hilog.error(DOMAIN, TAG,
        'LEDGER_AGE_REJECTED|requestId=%{public}s|reason=timestamp-missing-or-invalid', clean(candidate));
      return { requestId: candidate, usable: false };
    }
    const stampedAt: number = Number(timestampText);
    const ageMs: number = Date.now() - stampedAt;
    if (ageMs < -LEDGER_MAX_FUTURE_MS || ageMs > LEDGER_MAX_AGE_MS) {
      hilog.error(DOMAIN, TAG,
        'LEDGER_AGE_REJECTED|requestId=%{public}s|ageMs=%{public}s',
        clean(candidate), String(ageMs));
      return { requestId: candidate, usable: false };
    }
    return { requestId: candidate, usable: true };
  }

  private captureFdStatus(): FdSnapshot {
    const fd: number = this.tunFd;
    const integer: boolean = Number.isInteger(fd);
    const nonnegative: boolean = integer && fd >= 0;
    const probeEligible: boolean = nonnegative && fd <= MAX_NATIVE_FD;
    if (!probeEligible) {
      return {
        fd: fd,
        integer: integer,
        nonnegative: nonnegative,
        probeEligible: false,
        probed: false,
        open: false,
        errno: -1
      };
    }

    try {
      const status: FdStatus = fdprobe.status(fd);
      return {
        fd: fd,
        integer: integer,
        nonnegative: nonnegative,
        probeEligible: true,
        probed: true,
        open: status.open,
        errno: status.errno
      };
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_FD_PROBE_REJECTED|requestId=%{public}s|fd=%{public}s|summary=%{public}s',
        this.requestId, String(fd), safeError(error as Error));
      return {
        fd: fd,
        integer: integer,
        nonnegative: nonnegative,
        probeEligible: true,
        probed: false,
        open: false,
        errno: -1
      };
    }
  }

  private logFdSnapshot(phase: string, snapshot: FdSnapshot, marker: string): void {
    hilog.info(DOMAIN, TAG,
      'VPN_FD_SNAPSHOT|requestId=%{public}s|phase=%{public}s|fd=%{public}s|integer=%{public}s|' +
      'nonnegative=%{public}s|probeEligible=%{public}s|probed=%{public}s|open=%{public}s|' +
      'errno=%{public}d|marker=%{public}s',
      this.requestId, phase, String(snapshot.fd), String(snapshot.integer), String(snapshot.nonnegative),
      String(snapshot.probeEligible), String(snapshot.probed), String(snapshot.open), snapshot.errno, marker);
  }

  private postDestroyMarker(snapshot: FdSnapshot): string {
    if (!snapshot.probed) {
      return 'FD_STATE_UNCONFIRMED';
    }
    if (snapshot.open) {
      return 'FD_STILL_OPEN';
    }
    if (this.fdConfirmedOpenAfterCreate) {
      return 'FD_CLOSED_CONFIRMED';
    }
    return 'FD_NOT_OPEN_AFTER_DESTROY';
  }

  private destroyOnce(trigger: string): Promise<void> {
    if (this.destroyPromise !== undefined) {
      hilog.info(DOMAIN, TAG,
        'VPN_DESTROY_ALREADY|requestId=%{public}s|firstTrigger=%{public}s|requestedTrigger=%{public}s',
        this.requestId, this.destroyTrigger, trigger);
      return this.destroyPromise;
    }
    this.destroyTrigger = trigger;
    this.destroyPromise = this.runDestroyOnce();
    return this.destroyPromise;
  }

  private async runDestroyOnce(): Promise<void> {
    const createPromise: Promise<CreateOutcome> | undefined = this.createPromise;
    if (createPromise === undefined) {
      hilog.info(DOMAIN, TAG,
        'VPN_DESTROY_SKIPPED|requestId=%{public}s|reason=create-not-started-or-sync-rejected', this.requestId);
      return;
    }

    if (!this.createSettled) {
      hilog.info(DOMAIN, TAG, 'VPN_DESTROY_WAIT_CREATE|requestId=%{public}s', this.requestId);
    }
    const outcome: CreateOutcome = await createPromise;
    if (!outcome.destroyRequired) {
      hilog.info(DOMAIN, TAG,
        'VPN_DESTROY_SKIPPED|requestId=%{public}s|reason=%{public}s', this.requestId, outcome.reason);
      return;
    }

    const connection: vpnExtension.VpnConnection | undefined = this.connection;
    if (connection === undefined) {
      hilog.info(DOMAIN, TAG,
        'VPN_DESTROY_SKIPPED|requestId=%{public}s|reason=connection-invariant-missing', this.requestId);
      return;
    }

    const preDestroy: FdSnapshot = this.captureFdStatus();
    const preMarker: string = preDestroy.probed ?
      (preDestroy.open ? 'PRE_DESTROY_OPEN' : 'PRE_DESTROY_NOT_OPEN') : 'FD_STATE_UNCONFIRMED';
    this.logFdSnapshot('pre-destroy', preDestroy, preMarker);
    hilog.info(DOMAIN, TAG,
      'VPN_DESTROY_BEGIN|requestId=%{public}s|trigger=%{public}s|createAccepted=%{public}s|reason=%{public}s',
      this.requestId, this.destroyTrigger, String(outcome.accepted), outcome.reason);

    try {
      await connection.destroy();
      const postDestroy: FdSnapshot = this.captureFdStatus();
      const marker: string = this.postDestroyMarker(postDestroy);
      this.logFdSnapshot('post-destroy-resolved', postDestroy, marker);
      hilog.info(DOMAIN, TAG,
        'VPN_DESTROY_RESOLVED|requestId=%{public}s|fdOpen=%{public}s|fdMarker=%{public}s',
        this.requestId, String(postDestroy.open), marker);
    } catch (error) {
      const postDestroy: FdSnapshot = this.captureFdStatus();
      const marker: string = this.postDestroyMarker(postDestroy);
      this.logFdSnapshot('post-destroy-rejected', postDestroy, marker);
      hilog.error(DOMAIN, TAG,
        'VPN_DESTROY_REJECTED|requestId=%{public}s|phase=destroy|fdOpen=%{public}s|' +
        'fdMarker=%{public}s|summary=%{public}s',
        this.requestId, String(postDestroy.open), marker, safeError(error as Error));
    }
  }
}
