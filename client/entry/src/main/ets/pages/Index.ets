import Want from '@ohos.app.ability.Want';
import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
import cryptoFramework from '@ohos.security.cryptoFramework';
import util from '@ohos.util';
import vpnExtension from '@ohos.net.vpnExtension';
import { PublishedStatus, readPublishedStatus } from '../vpnextensionability/NetBirdConnector';
import { currentPublishedStatus } from '../vpnextensionability/NetBirdStatusPolicy';
import { DEFAULT_MANAGEMENT_URL, SETUP_CONFIG_FILE, SETUP_KEY_FILE,
  buildDeviceConfigJson, buildSetupKeyFileText, checkManagementUrl, checkSetupKey,
  readStoredManagementUrl, readStoredPrivateKey, KeyCheck, UrlCheck } from '../setup/NetBirdSetupConfig';

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';
const BUNDLE_NAME: string = 'cn.alfadb.netbird';
const EXTENSION_ABILITY: string = 'NetBirdVpnExtensionAbility';
const STATUS_POLL_MS: number = 2000;

interface BusinessFailure extends Error {
  code?: number;
}

// One in-flight start and stop request at a time; rejected requests and
// successful stops release the latch for an explicit user retry.
// Status: the platform DOES run the VpnExtensionAbility in a separate process,
// so this page reads the live document the extension publishes into the shared
// filesDir (device-verified 2026-09-14 — direct NAPI polling here rendered a
// dead "disconnected / wg sessions: 0" while the tunnel was up).
// UI scope: beyond the status texts, the only UI is the first-run
// setup/login card — it writes the SAME filesDir credential files as the
// development-only --ps channel, so the extension's reader is unchanged.
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
  // N7: real WG data-plane state (device-backed seam).
  @State wgReady: boolean = false;
  @State wgSessions: number = 0;
  // First-run setup / login card: unconfigured shows the form; configured
  // shows the current server + 重新配置 / 注销. Writes go to the SAME
  // credential files as the --ps channel (see NetBirdSetupConfig.ets).
  @State configured: boolean = false;
  @State configuredUrl: string = '';
  @State keyStored: boolean = false;
  @State showSetupForm: boolean = false;
  @State urlInput: string = DEFAULT_MANAGEMENT_URL;
  @State keyInput: string = '';
  @State formError: string = '';
  @State formHint: string = '';
  private requestId: string = `ui-${Date.now()}`;
  private observedPublishedStatus: boolean = false;
  private pollTimer: number = -1;

  aboutToAppear(): void {
    this.refreshSetupState();
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
    // The connector runs in the VpnExtension process: read the document that
    // process publishes into the shared filesDir (a NAPI call here would read
    // an empty slot and show a dead "disconnected" state).
    const status: PublishedStatus | null = currentPublishedStatus(
      readPublishedStatus(getContext(this).filesDir), Date.now());
    if (status === null) {
      this.connectorState = 'unavailable';
      this.peerCount = 0;
      this.routeCount = 0;
      this.lastErrorClass = 'none';
      this.signalRegistered = false;
      this.signalReconnects = 0;
      this.wgReady = false;
      this.wgSessions = 0;
      if (this.observedPublishedStatus) {
        this.requestSent = false;
        this.stopSent = false;
        this.statusText = 'READY';
        this.detailText = 'VPN not running';
        this.requestId = `ui-${Date.now()}`;
        this.observedPublishedStatus = false;
      }
      return;
    }
    this.observedPublishedStatus = true;
    if (status.state === 'stopped') {
      this.requestSent = false;
      this.stopSent = false;
      this.requestId = `ui-${Date.now()}`;
    } else {
      this.requestSent = true;
      if (status.request_id !== undefined && status.request_id.length > 0) {
        this.requestId = status.request_id;
      }
    }
    this.connectorState = status.state;
    this.peerCount = status.peer_count;
    this.routeCount = status.route_count;
    this.lastErrorClass =
      status.last_error_class !== undefined && status.last_error_class !== null
        ? status.last_error_class
        : 'none';
    // N5d: the real signal link (registered = offer/answer/candidate path up)
    this.signalRegistered = status.signal_registered;
    this.signalReconnects = status.signal_reconnects;
    // N7: the real WG data plane (device fed + tunnel_ready + live sessions)
    this.wgReady = status.wg_ready;
    this.wgSessions = status.wg_sessions;
  }

  // ---- first-run setup / login card --------------------------------------
  // configured = device config parses with a management URL AND the
  // setup-key file exists (both under the shared filesDir); anything else
  // shows the first-run form. The --ps channel keeps working on the same
  // files — whichever wrote last wins, the reader never changes.
  private refreshSetupState(): void {
    const filesDir: string = getContext(this).filesDir;
    let url: string = '';
    try {
      const configPath: string = `${filesDir}/${SETUP_CONFIG_FILE}`;
      if (fs.accessSync(configPath)) {
        url = readStoredManagementUrl(fs.readTextSync(configPath));
      }
    } catch (error) {
      url = '';
    }
    let keyStored: boolean = false;
    try {
      keyStored = fs.accessSync(`${filesDir}/${SETUP_KEY_FILE}`);
    } catch (error) {
      keyStored = false;
    }
    this.configured = url.length > 0 && keyStored;
    this.configuredUrl = url;
    this.keyStored = keyStored;
  }

  // Save the form: validate (Chinese, human-readable errors), then write the
  // two credential files in the EntryAbility.writeFile shape. Only the file
  // name + UTF-8 byte count are ever logged — never the setup key nor the
  // private key.
  private saveSetup(): void {
    const urlCheck: UrlCheck = checkManagementUrl(this.urlInput);
    const keyCheck: KeyCheck = checkSetupKey(this.keyInput);
    this.formError = !urlCheck.ok ? urlCheck.error : (!keyCheck.ok ? keyCheck.error : '');
    this.formHint = urlCheck.ok && keyCheck.ok ? keyCheck.hint : '';
    if (!urlCheck.ok || !keyCheck.ok) {
      return;
    }
    const privateKey: string = this.devicePrivateKey();
    if (privateKey.length === 0) {
      this.formError = '生成设备私钥失败，请重试';
      return;
    }
    const configText: string = buildDeviceConfigJson(urlCheck.url, privateKey);
    const keyText: string = buildSetupKeyFileText(this.keyInput);
    if (!this.writeSetupFile(SETUP_CONFIG_FILE, configText) ||
      !this.writeSetupFile(SETUP_KEY_FILE, keyText)) {
      this.formError = '保存失败，请重试';
      return;
    }
    // Drop the secret from UI state as soon as it is on disk.
    this.keyInput = '';
    this.showSetupForm = false;
    this.refreshSetupState();
  }

  // Device identity for the config file: REUSE the stored private key on
  // reconfigure (same peer identity), else generate a FRESH 32-byte
  // x25519/WireGuard key, base64 std — exactly what parseDeviceConfig /
  // EnvelopeKeyPair::from_secret_bytes accept. The value goes into the
  // config file only; it is never logged.
  private devicePrivateKey(): string {
    const filesDir: string = getContext(this).filesDir;
    try {
      const configPath: string = `${filesDir}/${SETUP_CONFIG_FILE}`;
      if (fs.accessSync(configPath)) {
        const stored: string = readStoredPrivateKey(fs.readTextSync(configPath));
        if (stored.length > 0) {
          return stored;
        }
      }
    } catch (error) {
      // unreadable/missing config = generate a fresh identity below
    }
    try {
      const rand: cryptoFramework.DataBlob =
        cryptoFramework.createRandom().generateRandomSync(32);
      return new util.Base64Helper().encodeToStringSync(rand.data);
    } catch (error) {
      hilog.error(DOMAIN, TAG, 'SETUP_KEYGEN_FAILED|code=%{public}s',
        String((error as BusinessFailure).code ?? 'none'));
      return '';
    }
  }

  // Same write honesty as the N2 request writers in EntryAbility: the open's
  // TRUNC already wiped any previous file, so a short/failed write is
  // reported as a failure (the extension then fails closed on the partial
  // file — credentials-missing abort, never a half-configured VPN).
  private writeSetupFile(name: string, text: string): boolean {
    const path: string = `${getContext(this).filesDir}/${name}`;
    let file: fs.File | undefined = undefined;
    let expectedBytes: number = 0;
    try {
      file = fs.openSync(path, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      const written: number = fs.writeSync(file.fd, text);
      expectedBytes = new util.TextEncoder().encode(text).byteLength;
      if (written !== expectedBytes) {
        hilog.error(DOMAIN, TAG,
          'SETUP_FILE_SAVE_FAILED|file=%{public}s|code=short-write:%{public}d-of-%{public}d',
          name, written, expectedBytes);
        return false;
      }
      hilog.warn(DOMAIN, TAG,
        'SETUP_FILE_SAVED|file=%{public}s|bytes=%{public}d|note=ui-setup-channel',
        name, expectedBytes);
      return true;
    } catch (error) {
      hilog.error(DOMAIN, TAG, 'SETUP_FILE_SAVE_FAILED|file=%{public}s|code=%{public}s',
        name, String((error as BusinessFailure).code ?? 'none'));
      return false;
    } finally {
      if (file !== undefined) {
        try {
          fs.closeSync(file);
        } catch (closeError) {
          hilog.error(DOMAIN, TAG, 'SETUP_FILE_CLOSE_FAILED|file=%{public}s|code=%{public}s',
            name, String((closeError as BusinessFailure).code ?? 'none'));
        }
      }
    }
  }

  // 注销 (logout): delete the local credentials + device config — only after
  // an explicit second confirmation.
  private requestLogout(): void {
    AlertDialog.show({
      title: '注销确认',
      message: '注销将删除本机的 Setup Key 与设备配置（含设备私钥），删除后需要重新配置才能连接。确定删除吗？',
      primaryButton: {
        value: '取消',
        action: (): void => {
        }
      },
      secondaryButton: {
        value: '删除并注销',
        fontColor: '#A12622',
        action: (): void => {
          this.performLogout();
        }
      }
    });
  }

  private performLogout(): void {
    this.removeSetupFile(SETUP_CONFIG_FILE);
    this.removeSetupFile(SETUP_KEY_FILE);
    this.urlInput = DEFAULT_MANAGEMENT_URL;
    this.keyInput = '';
    this.formError = '';
    this.formHint = '';
    this.showSetupForm = false;
    this.refreshSetupState();
  }

  private removeSetupFile(name: string): void {
    const path: string = `${getContext(this).filesDir}/${name}`;
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      fs.unlinkSync(path);
      hilog.info(DOMAIN, TAG, 'SETUP_FILE_REMOVED|file=%{public}s|note=logout', name);
    } catch (error) {
      hilog.error(DOMAIN, TAG, 'SETUP_FILE_REMOVE_FAILED|file=%{public}s|code=%{public}s',
        name, String((error as BusinessFailure).code ?? 'none'));
    }
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
      this.detailText = 'start request rejected';
      this.requestSent = false;
      hilog.error(DOMAIN, TAG, 'UI_START_REJECTED|bundle=%{public}s|code=%{public}s|requestId=%{public}s',
        BUNDLE_NAME, code, this.requestId);
      this.requestId = `ui-${Date.now()}`;
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
      this.requestSent = false;
      this.stopSent = false;
      hilog.info(DOMAIN, TAG, 'UI_STOP_RESOLVED|bundle=%{public}s|requestId=%{public}s',
        BUNDLE_NAME, this.requestId);
      this.requestId = `ui-${Date.now()}`;
    }).catch((error: Error) => {
      const code: string = String((error as BusinessFailure).code ?? 'none');
      this.statusText = `ERR:${code}`;
      this.detailText = 'stop request rejected';
      this.stopSent = false;
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
      // ---- 首次配置 / 登录 --------------------------------------------------
      // 未配置（或点击「重新配置」）时显示表单；已配置显示当前服务器与
      // 重新配置/注销入口。写入的凭据文件与 --ps 调试通道完全相同。
      if (this.showSetupForm || !this.configured) {
        Text(this.configured ? '重新配置' : '首次配置 / 登录')
          .fontSize(18)
          .fontWeight(FontWeight.Medium)
          .fontColor('#17324D')
        TextInput({ text: this.urlInput, placeholder: '管理服务器地址（https://…）' })
          .fontSize(16)
          .height(44)
          .width('100%')
          .onChange((value: string): void => {
            this.urlInput = value;
          })
        TextInput({ text: this.keyInput, placeholder: 'Setup Key（形如 6dc45f77-5f0d-4a0f-8a75-9df5b6a04bbb）' })
          .fontSize(16)
          .height(44)
          .width('100%')
          .type(InputType.Password)
          .onChange((value: string): void => {
            this.keyInput = value;
          })
        if (this.formError.length > 0) {
          Text(this.formError)
            .fontSize(13)
            .fontColor('#A12622')
            .textAlign(TextAlign.Center)
        }
        if (this.formHint.length > 0) {
          Text(this.formHint)
            .fontSize(13)
            .fontColor('#8A6D1A')
            .textAlign(TextAlign.Center)
            .maxLines(2)
        }
        Row({ space: 12 }) {
          Button('保存')
            .fontSize(16)
            .height(40)
            .backgroundColor('#17324D')
            .onClick(() => {
              hilog.info(DOMAIN, TAG, 'UI_SETUP_SAVE_CLICKED|configured=%{public}s',
                String(this.configured));
              this.saveSetup();
            })
          if (this.configured) {
            Button('取消')
              .fontSize(16)
              .height(40)
              .backgroundColor('#8A9AA6')
              .onClick(() => {
                this.showSetupForm = false;
                this.formError = '';
                this.formHint = '';
              })
          }
        }
      } else {
        Text('当前服务器')
          .fontSize(16)
          .fontWeight(FontWeight.Medium)
          .fontColor('#17324D')
        Text(`${this.configuredUrl}（Setup Key：${this.keyStored ? '已保存' : '未保存'}）`)
          .fontSize(14)
          .fontColor('#3D4A55')
          .textAlign(TextAlign.Center)
          .maxLines(2)
        Row({ space: 12 }) {
          Button('重新配置')
            .fontSize(16)
            .height(40)
            .backgroundColor('#5A3A6E')
            .onClick(() => {
              this.urlInput = this.configuredUrl;
              this.keyInput = '';
              this.formError = '';
              this.formHint = '';
              this.showSetupForm = true;
            })
          Button('注销')
            .fontSize(16)
            .height(40)
            .backgroundColor('#A12622')
            .onClick(() => {
              hilog.info(DOMAIN, TAG, 'UI_SETUP_LOGOUT_CLICKED');
              this.requestLogout();
            })
        }
      }
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
      // N7: WG data-plane status — tunnel_ready + peers with a live session.
      Text(`tunnel: ${this.wgReady ? 'ready' : 'not ready'}   ` +
        `wg sessions: ${this.wgSessions}`)
        .fontSize(16)
        .fontColor(this.wgReady ? '#176B3A' : '#8A6D1A')
      Text(`last error: ${this.lastErrorClass}`)
        .fontSize(16)
        .fontColor(this.lastErrorClass === 'none' ? '#176B3A' : '#A12622')
      Button(this.requestSent ? 'Request sent' : 'Connect VPN')
        .fontSize(17)
        .width(220)
        .height(48)
        .backgroundColor(this.requestSent || !this.configured ? '#8A9AA6' : '#17324D')
        .enabled(!this.requestSent && this.configured)
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
