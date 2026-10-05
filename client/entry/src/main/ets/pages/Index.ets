import Want from '@ohos.app.ability.Want';
import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
import cryptoFramework from '@ohos.security.cryptoFramework';
import util from '@ohos.util';
import bundleManager from '@ohos.bundle.bundleManager';
import vpnExtension from '@ohos.net.vpnExtension';
import { PublishedStatus, readPublishedStatus, sleep } from '../vpnextensionability/NetBirdConnector';
import { currentPublishedStatus } from '../vpnextensionability/NetBirdStatusPolicy';
import { UiPeerEntry, connectionLabel, filterPeers, homeStateText, lastErrorText,
  lastHandshakeText, onlinePeerCount, sortPeers, vpnActiveState } from '../vpnextensionability/NetBirdUiModel';
import { DEFAULT_MANAGEMENT_URL, SETUP_CONFIG_FILE, SETUP_KEY_FILE,
  buildDeviceConfigJson, buildSetupKeyFileText, checkManagementUrl, checkSetupKey,
  readStoredManagementUrl, readStoredPrivateKey, KeyCheck, UrlCheck } from '../setup/NetBirdSetupConfig';

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';
const BUNDLE_NAME: string = 'cn.alfadb.netbird';
const EXTENSION_ABILITY: string = 'NetBirdVpnExtensionAbility';
const STATUS_POLL_MS: number = 2000;
// Explicit logout channel (stop-time Logout defect, 2026-10-01): the UI
// process has NO connector (it lives in the VPN extension process), so the
// management Logout goes through the extension's one-shot ticket file and
// its answer file — the same shared-filesDir channel as the setup files.
// Budget: one extension watcher tick (5 s) + the native logout bound
// (8 s) + margin.
const LOGOUT_FILE_NAME: string = 'netbird-logout';
const LOGOUT_RESULT_FILE_NAME: string = 'netbird-logout-result';
const LOGOUT_WAIT_MS: number = 15000;
const LOGOUT_POLL_MS: number = 250;

interface BusinessFailure extends Error {
  code?: number;
}

// The extension's `netbird-logout-result` answer body.
interface LogoutResultFile {
  ok: boolean;
  reason?: string;
  ticket_ms: number;
}

// One in-flight start and stop request at a time; rejected requests and
// successful stops release the latch for an explicit user retry.
// Status: the platform DOES run the VpnExtensionAbility in a separate process,
// so this page reads the live document the extension publishes into the shared
// filesDir (device-verified 2026-09-14 — direct NAPI polling here rendered a
// dead "disconnected / wg sessions: 0" while the tunnel was up).
// UI scope (phase 1: 主页/对端/设置): the ONLY credential writes stay in the
// setup card (settings tab) — the SAME filesDir files as the development-only
// --ps channel, so the extension's reader is unchanged.
@Entry
@Component
struct Index {
  // ---- live status (published document, fresh within 10 s) ----
  @State published: PublishedStatus | null = null;
  @State requestSent: boolean = false;
  @State stopSent: boolean = false;
  @State actionError: string = '';
  // roam-1005-0014 A.3: shown when the status document VANISHES while the
  // last live snapshot was in an active state and no user stop is in flight
  // — the extension fail-closed (interface lost) or died; either way the
  // tunnel is down and the user must reconnect explicitly. Cleared by a new
  // start request.
  @State disconnectNotice: string = '';
  // ---- navigation ----
  @State activeTab: number = 0;
  @State selectedPeer: UiPeerEntry | null = null;
  @State peerQuery: string = '';
  @State showDiagnostics: boolean = false;
  @State versionName: string = '';
  // ---- first-run setup / login card (settings tab): unconfigured shows the
  // form; configured shows the current server + 重新配置 / 注销. Writes go to
  // the SAME credential files as the --ps channel (see NetBirdSetupConfig).
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
  // The state of the last LIVE document (drives the disappearance notice:
  // active → null without a user stop = the extension fail-closed / died).
  private lastLiveState: string = '';
  private lastUserStopAtMs: number = 0;
  // Explicit logout in flight (ticket written): masks the extension's
  // teardown-driven status-document disappearance so the logout the user
  // just asked for is not reported as an unexpected "VPN 已断开".
  private lastLogoutAtMs: number = 0;
  private logoutTicketMs: number = 0;
  private pollTimer: number = -1;

  aboutToAppear(): void {
    this.refreshSetupState();
    // First open without credentials: guide straight to configuration
    // (设置 tab owns the first-run form since phase 1).
    if (!this.configured) {
      this.activeTab = 2;
    }
    this.readVersionName();
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

  private readVersionName(): void {
    try {
      const info: bundleManager.BundleInfo = bundleManager.getBundleInfoForSelfSync(
        bundleManager.BundleFlag.GET_BUNDLE_INFO_DEFAULT);
      this.versionName = info.versionName !== undefined && info.versionName !== null
        ? info.versionName : '';
    } catch (error) {
      this.versionName = '';
    }
  }

  private pollConnectorStatus(): void {
    // The connector runs in the VpnExtension process: read the document that
    // process publishes into the shared filesDir (a NAPI call here would read
    // an empty slot and show a dead "disconnected" state). A document older
    // than two publish ticks counts as NOT running (B.3) — the page then
    // shows 未连接 instead of passing off a stale snapshot as live state.
    const status: PublishedStatus | null = currentPublishedStatus(
      readPublishedStatus(getContext(this).filesDir), Date.now());
    if (status === null) {
      if (this.observedPublishedStatus) {
        // the document disappeared = the extension is gone (stopped or
        // destroyed): release the request latches for an explicit retry
        // roam-1005-0014 A.3: if the last live state was ACTIVE and no user
        // stop just resolved, the tunnel ended without the user asking —
        // surface the reconnect notice (the extension's fail-closed path
        // removed the document after its VPN_RECREATE_VERIFY failure).
        if (vpnActiveState(this.lastLiveState) && !this.stopSent &&
          Date.now() - this.lastUserStopAtMs > 8000 &&
          Date.now() - this.lastLogoutAtMs > 25000) {
          this.disconnectNotice = 'VPN 已断开，请重新连接';
        }
        this.requestSent = false;
        this.stopSent = false;
        this.requestId = `ui-${Date.now()}`;
        this.observedPublishedStatus = false;
        this.lastLiveState = '';
      }
      this.published = null;
      return;
    }
    this.observedPublishedStatus = true;
    this.lastLiveState = status.state;
    if (vpnActiveState(status.state)) {
      this.requestSent = true;
      if (status.request_id !== undefined && status.request_id.length > 0) {
        this.requestId = status.request_id;
      }
    } else {
      // A live document in a dead state (failed/disconnected/closed/stopped):
      // the tunnel is not coming back on its own — re-arm the switch.
      this.requestSent = false;
      this.stopSent = false;
      this.requestId = `ui-${Date.now()}`;
    }
    this.published = status;
  }

  // ---- derived view state -------------------------------------------------

  private peersList(): UiPeerEntry[] {
    return this.published !== null && this.published.peers !== undefined &&
      this.published.peers !== null ? this.published.peers : [];
  }

  private visiblePeers(): UiPeerEntry[] {
    return sortPeers(filterPeers(this.peersList(), this.peerQuery));
  }

  private stateText(): string {
    const state: string = this.published !== null ? this.published.state : '';
    // 已连接 requires the core state connected AND the own overlay IP (the
    // network map arrived) AND the vpn-tun interface actually existing
    // (roam-1005-0014: the interface was removed while the core kept
    // self-reporting connected); a live tunnel without the map or without the
    // interface is 连接中… / 无法连接服务器 (与红字错误提示一致, see
    // NetBirdUiModel.homeStateText).
    const hasOverlayIp: boolean = this.published !== null &&
      this.published.address !== undefined && this.published.address !== null &&
      this.published.address.length > 0;
    const ifacePresent: boolean = this.published === null ||
      this.published.iface_present !== false;
    const errorClass: string = this.published !== null &&
      this.published.last_error_class !== undefined &&
      this.published.last_error_class !== null ? this.published.last_error_class : 'none';
    return homeStateText(state, this.requestSent, this.stopSent, hasOverlayIp,
      ifacePresent, errorClass);
  }

  // The FULLY connected gate (B.2): core state connected + own overlay IP +
  // interface listed. While this holds, the core's last_error is HISTORY
  // (it never self-clears — the device log carried lastError=network through
  // a healthy connected phase), so the red error line is suppressed.
  private fullyConnected(): boolean {
    return this.published !== null && this.published.state === 'connected' &&
      this.published.address !== undefined && this.published.address !== null &&
      this.published.address.length > 0 && this.published.iface_present !== false;
  }

  // Home-card row values (roam-1005-0014 B.1): these MUST be method calls on
  // observed state evaluated at the Text node — the previous fieldRow(...)
  // @Builder calls passed BY-VALUE expression arguments, which ArkUI never
  // re-renders on state change, so the rows froze at their mount-time '—'/'0/0'
  // values while the status text kept updating (the device symptom:
  // 已连接 + NetBird IP — + 对端 0/0).
  private homeAddressText(): string {
    return this.published !== null && this.published.address !== undefined &&
      this.published.address !== null ? this.published.address : '';
  }

  private homeFqdnText(): string {
    return this.published !== null && this.published.fqdn !== undefined &&
      this.published.fqdn !== null ? this.published.fqdn : '';
  }

  private homePeersText(): string {
    return `${onlinePeerCount(this.peersList())}/${this.peersList().length} 在线`;
  }

  private stateColor(): string {
    const text: string = this.stateText();
    if (text === '已连接') {
      return '#176B3A';
    }
    if (text === '连接中…' || text === '正在断开…') {
      return '#8A6D1A';
    }
    if (text === '连接失败' || text === '无法连接服务器') {
      return '#A12622';
    }
    return '#3D4A55';
  }

  private toggleOn(): boolean {
    return this.published !== null && vpnActiveState(this.published.state);
  }

  private toggleBusy(): boolean {
    if (this.stopSent) {
      return true;
    }
    return this.requestSent && this.published === null;
  }

  private toggleLabel(): string {
    if (this.stopSent) {
      return '正在断开…';
    }
    if (this.toggleOn()) {
      return '断开';
    }
    return this.requestSent ? '连接中…' : '连接';
  }

  private toggleColor(): string {
    if (this.toggleOn()) {
      return '#176B3A';
    }
    if (this.toggleBusy()) {
      return '#8A9AA6';
    }
    return '#17324D';
  }

  private toggleEnabled(): boolean {
    if (!this.configured) {
      return false;
    }
    if (this.stopSent) {
      return false;
    }
    if (this.toggleOn()) {
      return true;
    }
    return !this.requestSent;
  }

  private lastErrorLine(): string {
    if (this.actionError.length > 0) {
      return this.actionError;
    }
    // A vanished document after an active state: the reconnect notice (the
    // extension fail-closed or died mid-connection).
    if (this.published === null) {
      return this.disconnectNotice;
    }
    // B.2: while fully connected the core's last_error is a HISTORICAL
    // record (it does not self-clear) — showing it as a live red line was
    // the "已连接 + 网络不可用红字" device symptom. Only a NOT-fully-connected
    // card may surface it.
    if (this.fullyConnected()) {
      return '';
    }
    const errorClass: string = this.published.last_error_class !== undefined &&
      this.published.last_error_class !== null ? this.published.last_error_class : 'none';
    return lastErrorText(errorClass);
  }

  // ---- first-run setup / login card (settings tab) -------------------------
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
  // an explicit second confirmation. When the VPN is LIVE, the management
  // Logout is attempted FIRST through the extension (the only process with
  // the connector + its logged-in management client): a one-shot
  // `netbird-logout` ticket answered by `netbird-logout-result` (bounded
  // wait — a Logout failure or timeout never blocks the local deletion).
  // When the VPN is not running there is no live management session to
  // logout with: only the local files go, and the hint points the user at
  // the management console.
  private requestLogout(): void {
    AlertDialog.show({
      title: '注销确认',
      message: '注销将删除本机的 Setup Key 与设备配置（含设备私钥）；若当前已连接，还会通知管理端删除本设备记录。确定注销吗？',
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
    const vpnLive: boolean = this.published !== null && vpnActiveState(this.published.state);
    if (!vpnLive) {
      hilog.info(DOMAIN, TAG, 'UI_LOGOUT_LOCAL_ONLY|reason=vpn-not-running');
      this.finishLogout(false);
      return;
    }
    this.lastLogoutAtMs = Date.now();
    this.writeLogoutTicket();
    this.waitForLogoutResult().then((ok: boolean) => {
      this.finishLogout(ok);
    });
  }

  private writeLogoutTicket(): void {
    // fresh answer channel: drop a stale result file from an earlier logout
    this.removeSetupFile(LOGOUT_RESULT_FILE_NAME);
    const issuedAtMs: number = Date.now();
    this.logoutTicketMs = issuedAtMs;
    const written: boolean =
      this.writeSetupFile(LOGOUT_FILE_NAME, `{"issued_at_ms":${issuedAtMs}}`);
    if (written) {
      hilog.info(DOMAIN, TAG, 'UI_LOGOUT_TICKET_WRITTEN|file=%{public}s|ticketMs=%{public}d',
        LOGOUT_FILE_NAME, issuedAtMs);
    } else {
      hilog.error(DOMAIN, TAG, 'UI_LOGOUT_TICKET_WRITE_FAILED|file=%{public}s', LOGOUT_FILE_NAME);
    }
  }

  private async waitForLogoutResult(): Promise<boolean> {
    const deadline: number = Date.now() + LOGOUT_WAIT_MS;
    while (Date.now() < deadline) {
      await sleep(LOGOUT_POLL_MS);
      const ok: boolean | null = this.readLogoutResult();
      if (ok !== null) {
        return ok;
      }
    }
    hilog.warn(DOMAIN, TAG, 'UI_LOGOUT_RESULT_TIMEOUT|waitMs=%{public}d', LOGOUT_WAIT_MS);
    return false;
  }

  // The extension's answer: `{"ok":bool,"reason":str,"ticket_ms":ms}`.
  // `null` while no MATCHING answer exists (still pending / a stale answer
  // from an earlier logout — matched on the ticket stamp).
  private readLogoutResult(): boolean | null {
    const path: string = `${getContext(this).filesDir}/${LOGOUT_RESULT_FILE_NAME}`;
    try {
      if (!fs.accessSync(path)) {
        return null;
      }
      const parsed: LogoutResultFile = JSON.parse(fs.readTextSync(path)) as LogoutResultFile;
      if (parsed.ticket_ms !== this.logoutTicketMs) {
        return null;
      }
      this.removeSetupFile(LOGOUT_RESULT_FILE_NAME);
      hilog.info(DOMAIN, TAG, 'UI_LOGOUT_RESULT|ok=%{public}s|reason=%{public}s',
        String(parsed.ok), parsed.reason !== undefined ? parsed.reason : 'none');
      return parsed.ok === true;
    } catch (error) {
      return null;
    }
  }

  private finishLogout(serverLogoutConfirmed: boolean): void {
    this.removeSetupFile(SETUP_CONFIG_FILE);
    this.removeSetupFile(SETUP_KEY_FILE);
    // a ticket that was never consumed (extension gone before its tick)
    this.removeSetupFile(LOGOUT_FILE_NAME);
    this.urlInput = DEFAULT_MANAGEMENT_URL;
    this.keyInput = '';
    this.formError = '';
    this.formHint = serverLogoutConfirmed
      ? '已注销：本机凭据已删除，管理端已删除本设备。'
      : '已删除本机凭据。如需从管理端移除本设备，请在管理后台删除。';
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

  // ---- VPN start / stop (single toggle on the home tab) --------------------

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
    this.actionError = '';
    this.disconnectNotice = '';
    hilog.info(DOMAIN, TAG, 'UI_START_REQUEST|bundle=%{public}s|ability=%{public}s|requestId=%{public}s',
      BUNDLE_NAME, EXTENSION_ABILITY, this.requestId);
    vpnExtension.startVpnExtensionAbility(this.buildWant()).then(() => {
      // The extension accepted the start; the published status document now
      // drives everything the home tab shows.
      hilog.info(DOMAIN, TAG, 'UI_START_RESOLVED|bundle=%{public}s|requestId=%{public}s',
        BUNDLE_NAME, this.requestId);
    }).catch((error: Error) => {
      const code: string = String((error as BusinessFailure).code ?? 'none');
      this.actionError = `连接请求未成功（错误码 ${code}），请重试`;
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
    hilog.info(DOMAIN, TAG, 'UI_STOP_REQUEST|bundle=%{public}s|requestId=%{public}s',
      BUNDLE_NAME, this.requestId);
    vpnExtension.stopVpnExtensionAbility(this.buildWant()).then(() => {
      this.requestSent = false;
      this.stopSent = false;
      // the document disappears a moment AFTER this resolves — mark the stop
      // so the disappearance is not misread as a fail-closed disconnect
      this.lastUserStopAtMs = Date.now();
      this.disconnectNotice = '';
      hilog.info(DOMAIN, TAG, 'UI_STOP_RESOLVED|bundle=%{public}s|requestId=%{public}s',
        BUNDLE_NAME, this.requestId);
      this.requestId = `ui-${Date.now()}`;
    }).catch((error: Error) => {
      const code: string = String((error as BusinessFailure).code ?? 'none');
      this.actionError = `断开请求未成功（错误码 ${code}），请重试`;
      this.stopSent = false;
      hilog.error(DOMAIN, TAG, 'UI_STOP_REJECTED|bundle=%{public}s|code=%{public}s|requestId=%{public}s',
        BUNDLE_NAME, code, this.requestId);
    });
  }

  // ---- shared small widgets -------------------------------------------------

  @Builder
  fieldRow(label: string, value: string) {
    Row({ space: 8 }) {
      Text(label)
        .fontSize(13)
        .fontColor('#8A9AA6')
        .width(96)
      Text(value.length > 0 ? value : '—')
        .fontSize(14)
        .fontColor('#17324D')
        .layoutWeight(1)
        .maxLines(3)
        .textOverflow({ overflow: TextOverflow.Ellipsis })
    }
    .width('100%')
    .padding({ top: 6, bottom: 6 })
  }

  @Builder
  sectionTitle(title: string) {
    Text(title)
      .fontSize(15)
      .fontWeight(FontWeight.Medium)
      .fontColor('#3D4A55')
      .width('100%')
      .margin({ top: 18, bottom: 6 })
  }

  @Builder
  statusDot(color: string, size: number) {
    Circle({ width: size, height: size })
      .fill(color)
  }

  // ---- 主页 -------------------------------------------------------------------

  @Builder
  homeTab() {
    Scroll() {
      Column({ space: 16 }) {
        Text('NetBird')
          .fontSize(28)
          .fontWeight(FontWeight.Bold)
          .fontColor('#17324D')
          .margin({ top: 12 })
        if (!this.configured) {
          Column({ space: 10 }) {
            Text('尚未配置服务器')
              .fontSize(17)
              .fontWeight(FontWeight.Medium)
              .fontColor('#8A6D1A')
            Text('请先在「设置」中填写管理服务器地址与 Setup Key，完成后再连接。')
              .fontSize(14)
              .fontColor('#3D4A55')
              .textAlign(TextAlign.Center)
            Button('去设置')
              .fontSize(16)
              .height(40)
              .backgroundColor('#17324D')
              .onClick(() => {
                this.activeTab = 2;
              })
          }
          .width('100%')
          .padding(16)
          .borderRadius(16)
          .backgroundColor('#FFFFFF')
          .alignItems(HorizontalAlign.Center)
        }
        // 大号连接开关：单一按钮切换连接/断开（含连接中状态）。
        Button(this.toggleLabel())
          .fontSize(22)
          .fontWeight(FontWeight.Medium)
          .fontColor('#FFFFFF')
          .width(132)
          .height(132)
          .borderRadius(66)
          .backgroundColor(this.toggleColor())
          .enabled(this.toggleEnabled())
          .onClick(() => {
            if (this.toggleOn()) {
              hilog.info(DOMAIN, TAG, 'UI_STOP_BUTTON_CLICKED|bundle=%{public}s', BUNDLE_NAME);
              this.requestStop();
            } else {
              hilog.info(DOMAIN, TAG, 'UI_START_BUTTON_CLICKED|bundle=%{public}s', BUNDLE_NAME);
              this.requestStart();
            }
          })
        // 状态卡片：已连接/连接中/未连接 + 本机 NetBird IP 与域名 + 在线对端数。
        // The rows are INLINE (not fieldRow calls): a @Builder by-value
        // expression argument never re-renders on state change in ArkUI, and
        // the frozen rows were the B.1 device defect — the Text values below
        // are method calls on observed state, the same proven-fresh pattern
        // as the status text above (主页与对端页同读 this.published 一份数据).
        Column({ space: 4 }) {
          Row({ space: 10 }) {
            this.statusDot(this.stateColor(), 12)
            Text(this.stateText())
              .fontSize(20)
              .fontWeight(FontWeight.Medium)
              .fontColor(this.stateColor())
          }
          .margin({ bottom: 6 })
          Row({ space: 8 }) {
            Text('NetBird IP')
              .fontSize(13)
              .fontColor('#8A9AA6')
              .width(96)
            Text(this.homeAddressText().length > 0 ? this.homeAddressText() : '—')
              .fontSize(14)
              .fontColor('#17324D')
              .layoutWeight(1)
              .maxLines(3)
              .textOverflow({ overflow: TextOverflow.Ellipsis })
          }
          .width('100%')
          .padding({ top: 6, bottom: 6 })
          Row({ space: 8 }) {
            Text('域名')
              .fontSize(13)
              .fontColor('#8A9AA6')
              .width(96)
            Text(this.homeFqdnText().length > 0 ? this.homeFqdnText() : '—')
              .fontSize(14)
              .fontColor('#17324D')
              .layoutWeight(1)
              .maxLines(3)
              .textOverflow({ overflow: TextOverflow.Ellipsis })
          }
          .width('100%')
          .padding({ top: 6, bottom: 6 })
          Row({ space: 8 }) {
            Text('对端')
              .fontSize(13)
              .fontColor('#8A9AA6')
              .width(96)
            Text(this.homePeersText())
              .fontSize(14)
              .fontColor('#17324D')
              .layoutWeight(1)
              .maxLines(3)
              .textOverflow({ overflow: TextOverflow.Ellipsis })
          }
          .width('100%')
          .padding({ top: 6, bottom: 6 })
          if (this.lastErrorLine().length > 0) {
            Text(this.lastErrorLine())
              .fontSize(13)
              .fontColor('#A12622')
              .width('100%')
              .margin({ top: 6 })
              .maxLines(3)
          }
        }
        .width('100%')
        .padding(16)
        .borderRadius(16)
        .backgroundColor('#FFFFFF')
        .alignItems(HorizontalAlign.Start)
      }
      .width('100%')
      .padding(20)
    }
    .width('100%')
    .height('100%')
    .backgroundColor('#F2F6F8')
    .align(Alignment.Top)
    .scrollBar(BarState.Off)
  }

  // ---- 对端 -------------------------------------------------------------------

  @Builder
  peerRow(peer: UiPeerEntry) {
    Row({ space: 12 }) {
      Column({ space: 2 }) {
        Text(peer.name)
          .fontSize(16)
          .fontColor('#17324D')
          .maxLines(1)
          .textOverflow({ overflow: TextOverflow.Ellipsis })
        Text(peer.ip.length > 0 ? peer.ip : '无 IP')
          .fontSize(13)
          .fontColor('#8A9AA6')
      }
      .layoutWeight(1)
      .alignItems(HorizontalAlign.Start)
      Row({ space: 6 }) {
        if (peer.online) {
          Text(connectionLabel(peer.connection))
            .fontSize(12)
            .fontColor(peer.connection === 'direct' ? '#176B3A' : '#8A6D1A')
            .padding({ left: 8, right: 8, top: 3, bottom: 3 })
            .borderRadius(10)
            .backgroundColor(peer.connection === 'direct' ? '#E3EFE7' : '#F5EBCF')
        }
        this.statusDot(peer.online ? '#176B3A' : '#8A9AA6', 10)
      }
      .alignItems(VerticalAlign.Center)
    }
    .width('100%')
    .padding({ top: 12, bottom: 12 })
    .onClick(() => {
      this.selectedPeer = peer;
    })
  }

  @Builder
  peersTab() {
    if (this.selectedPeer !== null) {
      // 对端详情：展示可得的全部字段。
      Column({ space: 8 }) {
        Row({ space: 12 }) {
          Button('返回')
            .fontSize(15)
            .height(36)
            .backgroundColor('#8A9AA6')
            .onClick(() => {
              this.selectedPeer = null;
            })
          Text('对端详情')
            .fontSize(20)
            .fontWeight(FontWeight.Medium)
            .fontColor('#17324D')
        }
        .width('100%')
        .margin({ top: 8 })
        Column({ space: 4 }) {
          Row({ space: 10 }) {
            this.statusDot(this.selectedPeer.online ? '#176B3A' : '#8A9AA6', 12)
            Text(this.selectedPeer.name)
              .fontSize(19)
              .fontWeight(FontWeight.Medium)
              .fontColor('#17324D')
              .layoutWeight(1)
              .maxLines(2)
              .textOverflow({ overflow: TextOverflow.Ellipsis })
          }
          .margin({ bottom: 6 })
          this.fieldRow('域名', this.selectedPeer.fqdn !== null ? this.selectedPeer.fqdn : '')
          this.fieldRow('NetBird IP', this.selectedPeer.ip)
          this.fieldRow('状态', this.selectedPeer.online ? '在线' : '离线')
          this.fieldRow('连接方式', this.selectedPeer.online ? connectionLabel(this.selectedPeer.connection) : '')
          this.fieldRow('最近握手', lastHandshakeText(this.selectedPeer.last_handshake_s))
          this.fieldRow('公钥', this.selectedPeer.pub_key)
        }
        .width('100%')
        .padding(16)
        .borderRadius(16)
        .backgroundColor('#FFFFFF')
        .alignItems(HorizontalAlign.Start)
      }
      .width('100%')
      .height('100%')
      .padding(20)
      .backgroundColor('#F2F6F8')
    } else {
      Column({ space: 12 }) {
        Row({ space: 10 }) {
          Text('对端')
            .fontSize(24)
            .fontWeight(FontWeight.Bold)
            .fontColor('#17324D')
            .layoutWeight(1)
          Text(`${onlinePeerCount(this.peersList())}/${this.peersList().length} 在线`)
            .fontSize(14)
            .fontColor('#3D4A55')
        }
        .width('100%')
        .margin({ top: 8 })
        TextInput({ text: this.peerQuery, placeholder: '搜索对端名称或 IP' })
          .fontSize(15)
          .height(42)
          .width('100%')
          .backgroundColor('#FFFFFF')
          .onChange((value: string): void => {
            this.peerQuery = value;
          })
        if (this.published === null) {
          Text('VPN 未连接，对端信息在连接后可用')
            .fontSize(14)
            .fontColor('#8A9AA6')
            .margin({ top: 32 })
        } else if (this.visiblePeers().length === 0) {
          Text(this.peerQuery.length > 0 ? '没有匹配的对端' : '暂无对端')
            .fontSize(14)
            .fontColor('#8A9AA6')
            .margin({ top: 32 })
        } else {
          List({ space: 0 }) {
            ForEach(this.visiblePeers(), (peer: UiPeerEntry) => {
              ListItem() {
                this.peerRow(peer)
              }
            }, (peer: UiPeerEntry) => peer.pub_key)
          }
          .width('100%')
          .layoutWeight(1)
          .backgroundColor('#FFFFFF')
          .borderRadius(16)
          .padding({ left: 16, right: 16 })
          .scrollBar(BarState.Off)
        }
      }
      .width('100%')
      .height('100%')
      .padding(20)
      .backgroundColor('#F2F6F8')
    }
  }

  // ---- 设置 -------------------------------------------------------------------

  @Builder
  setupFormCard() {
    Column({ space: 12 }) {
      Text(this.configured ? '重新配置' : '首次配置 / 登录')
        .fontSize(16)
        .fontWeight(FontWeight.Medium)
        .fontColor('#17324D')
        .width('100%')
      TextInput({ text: this.urlInput, placeholder: '管理服务器地址（https://…）' })
        .fontSize(15)
        .height(44)
        .width('100%')
        .onChange((value: string): void => {
          this.urlInput = value;
        })
      TextInput({ text: this.keyInput, placeholder: 'Setup Key（形如 6dc45f77-5f0d-4a0f-8a75-9df5b6a04bbb）' })
        .fontSize(15)
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
          .width('100%')
      }
      if (this.formHint.length > 0) {
        Text(this.formHint)
          .fontSize(13)
          .fontColor('#8A6D1A')
          .width('100%')
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
    }
    .width('100%')
    .padding(16)
    .borderRadius(16)
    .backgroundColor('#FFFFFF')
    .alignItems(HorizontalAlign.Start)
  }

  @Builder
  serverConfigCard() {
    Column({ space: 10 }) {
      Text('当前服务器')
        .fontSize(16)
        .fontWeight(FontWeight.Medium)
        .fontColor('#17324D')
        .width('100%')
      Text(`${this.configuredUrl}（Setup Key：${this.keyStored ? '已保存' : '未保存'}）`)
        .fontSize(14)
        .fontColor('#3D4A55')
        .width('100%')
        .maxLines(2)
      Row({ space: 12 }) {
        Button('重新配置')
          .fontSize(15)
          .height(38)
          .backgroundColor('#5A3A6E')
          .onClick(() => {
            this.urlInput = this.configuredUrl;
            this.keyInput = '';
            this.formError = '';
            this.formHint = '';
            this.showSetupForm = true;
          })
        Button('注销')
          .fontSize(15)
          .height(38)
          .backgroundColor('#A12622')
          .onClick(() => {
            hilog.info(DOMAIN, TAG, 'UI_SETUP_LOGOUT_CLICKED');
            this.requestLogout();
          })
      }
    }
    .width('100%')
    .padding(16)
    .borderRadius(16)
    .backgroundColor('#FFFFFF')
    .alignItems(HorizontalAlign.Start)
  }

  @Builder
  diagnosticsView() {
    Column({ space: 8 }) {
      Row({ space: 12 }) {
        Button('返回')
          .fontSize(15)
          .height(36)
          .backgroundColor('#8A9AA6')
          .onClick(() => {
            this.showDiagnostics = false;
          })
        Text('诊断信息')
          .fontSize(20)
          .fontWeight(FontWeight.Medium)
          .fontColor('#17324D')
      }
      .width('100%')
      .margin({ top: 8, bottom: 8 })
      Column({ space: 4 }) {
        // INLINE rows (same B.1 fix as the home card): fieldRow's by-value
        // expression args never re-render, and a diagnostics page that froze
        // at （未运行） forever is the same defect.
        Row({ space: 8 }) {
          Text('状态').fontSize(13).fontColor('#8A9AA6').width(96)
          Text(this.published !== null ? this.published.state : '（未运行）')
            .fontSize(14).fontColor('#17324D').layoutWeight(1).maxLines(3)
        }.width('100%').padding({ top: 6, bottom: 6 })
        Row({ space: 8 }) {
          Text('信号').fontSize(13).fontColor('#8A9AA6').width(96)
          Text(this.published !== null
            ? (this.published.signal_registered ? 'registered' : 'not registered') : '—')
            .fontSize(14).fontColor('#17324D').layoutWeight(1).maxLines(3)
        }.width('100%').padding({ top: 6, bottom: 6 })
        Row({ space: 8 }) {
          Text('信号重连').fontSize(13).fontColor('#8A9AA6').width(96)
          Text(this.published !== null ? `${this.published.signal_reconnects}` : '—')
            .fontSize(14).fontColor('#17324D').layoutWeight(1).maxLines(3)
        }.width('100%').padding({ top: 6, bottom: 6 })
        Row({ space: 8 }) {
          Text('隧道').fontSize(13).fontColor('#8A9AA6').width(96)
          Text(this.published !== null
            ? (this.published.wg_ready ? 'ready' : 'not ready') : '—')
            .fontSize(14).fontColor('#17324D').layoutWeight(1).maxLines(3)
        }.width('100%').padding({ top: 6, bottom: 6 })
        Row({ space: 8 }) {
          Text('WG 会话').fontSize(13).fontColor('#8A9AA6').width(96)
          Text(this.published !== null ? `${this.published.wg_sessions}` : '—')
            .fontSize(14).fontColor('#17324D').layoutWeight(1).maxLines(3)
        }.width('100%').padding({ top: 6, bottom: 6 })
        Row({ space: 8 }) {
          Text('对端/路由').fontSize(13).fontColor('#8A9AA6').width(96)
          Text(this.published !== null
            ? `${this.published.peer_count} / ${this.published.route_count}` : '—')
            .fontSize(14).fontColor('#17324D').layoutWeight(1).maxLines(3)
        }.width('100%').padding({ top: 6, bottom: 6 })
        Row({ space: 8 }) {
          Text('管理重连').fontSize(13).fontColor('#8A9AA6').width(96)
          Text(this.published !== null && this.published.reconnects !== undefined
            ? `${this.published.reconnects}` : '—')
            .fontSize(14).fontColor('#17324D').layoutWeight(1).maxLines(3)
        }.width('100%').padding({ top: 6, bottom: 6 })
        Row({ space: 8 }) {
          Text('中继').fontSize(13).fontColor('#8A9AA6').width(96)
          Text(this.published !== null && this.published.relay_state !== undefined &&
            this.published.relay_state !== null ? this.published.relay_state : '—')
            .fontSize(14).fontColor('#17324D').layoutWeight(1).maxLines(3)
        }.width('100%').padding({ top: 6, bottom: 6 })
        Row({ space: 8 }) {
          Text('接口').fontSize(13).fontColor('#8A9AA6').width(96)
          Text(this.published === null ? '—' : (this.published.iface_present === undefined
            ? '未知' : (this.published.iface_present ? 'vpn-tun 在' : 'vpn-tun 不存在')))
            .fontSize(14).fontColor('#17324D').layoutWeight(1).maxLines(3)
        }.width('100%').padding({ top: 6, bottom: 6 })
        Row({ space: 8 }) {
          Text('最近错误').fontSize(13).fontColor('#8A9AA6').width(96)
          Text(this.published !== null ? this.published.last_error_class : '—')
            .fontSize(14).fontColor('#17324D').layoutWeight(1).maxLines(3)
        }.width('100%').padding({ top: 6, bottom: 6 })
        Row({ space: 8 }) {
          Text('请求 ID').fontSize(13).fontColor('#8A9AA6').width(96)
          Text(this.published !== null && this.published.request_id !== undefined
            ? this.published.request_id : '—')
            .fontSize(14).fontColor('#17324D').layoutWeight(1).maxLines(3)
        }.width('100%').padding({ top: 6, bottom: 6 })
      }
      .width('100%')
      .padding(16)
      .borderRadius(16)
      .backgroundColor('#FFFFFF')
      .alignItems(HorizontalAlign.Start)
    }
    .width('100%')
    .height('100%')
    .padding(20)
    .backgroundColor('#F2F6F8')
  }

  @Builder
  settingsTab() {
    Scroll() {
      Column({ space: 0 }) {
        Text('设置')
          .fontSize(24)
          .fontWeight(FontWeight.Bold)
          .fontColor('#17324D')
          .width('100%')
          .margin({ top: 8, bottom: 4 })
        if (this.showDiagnostics) {
          this.diagnosticsView()
        } else {
          this.sectionTitle('服务器配置')
          if (this.showSetupForm || !this.configured) {
            this.setupFormCard()
          } else {
            this.serverConfigCard()
          }
          this.sectionTitle('关于')
          Column() {
            this.fieldRow('版本', this.versionName.length > 0 ? this.versionName : '未知')
          }
          .width('100%')
          .padding(16)
          .borderRadius(16)
          .backgroundColor('#FFFFFF')
          .alignItems(HorizontalAlign.Start)
          this.sectionTitle('诊断')
          Row({ space: 8 }) {
            Text('诊断信息')
              .fontSize(16)
              .fontColor('#17324D')
              .layoutWeight(1)
            Text('›')
              .fontSize(18)
              .fontColor('#8A9AA6')
          }
          .width('100%')
          .padding(16)
          .borderRadius(16)
          .backgroundColor('#FFFFFF')
          .onClick(() => {
            this.showDiagnostics = true;
          })
        }
      }
      .width('100%')
      .padding(20)
    }
    .width('100%')
    .height('100%')
    .backgroundColor('#F2F6F8')
    .align(Alignment.Top)
    .scrollBar(BarState.Off)
  }

  build() {
    Column() {
      Tabs({ barPosition: BarPosition.End, index: this.activeTab }) {
        TabContent() {
          this.homeTab()
        }
        .tabBar('主页')

        TabContent() {
          this.peersTab()
        }
        .tabBar('对端')

        TabContent() {
          this.settingsTab()
        }
        .tabBar('设置')
      }
      .scrollable(true)
      .barMode(BarMode.Fixed)
      .onChange((index: number) => {
        this.activeTab = index;
        if (index !== 1) {
          // leaving the peers tab drops the drill-down selection
          this.selectedPeer = null;
        }
      })
      .width('100%')
      .layoutWeight(1)
    }
    .width('100%')
    .height('100%')
  }
}
