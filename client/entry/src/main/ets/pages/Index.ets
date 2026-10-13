import Want from '@ohos.app.ability.Want';
import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
import cryptoFramework from '@ohos.security.cryptoFramework';
import util from '@ohos.util';
import bundleManager from '@ohos.bundle.bundleManager';
import vpnExtension from '@ohos.net.vpnExtension';
import common from '@ohos.app.ability.common';
import { ConnectFailureNotice, PublishedStatus, readConnectFailure, readPublishedStatus,
  removeConnectFailure, sleep } from '../vpnextensionability/NetBirdConnector';
import { currentPublishedStatus } from '../vpnextensionability/NetBirdStatusPolicy';
// 2026-10-08 扩展被系统回收后的自动重连: the WHOLE decision (every gate + the
// anti-loop state machine) is a PURE module — the host check
// selfcheck/run-reconnect-policy.mts imports it byte-identical. This page only
// gathers the facts (the status document it just read, the extension's marker
// file, its own latches) and acts through requestStart(), the same single start
// path the button uses.
// 0.0.20: two facts were added to that list. (a) `keepAliveActive` — a borrowed
// keep-alive task is held, which is why the 2 s poll still runs in the background
// and therefore the ONE thing that opens the foreground gate there; the record
// then says trigger=background. (b) the kill event naming the marker's pid is
// treated as "the document is worthless" (criterion ② relaxed), so the reconnect
// does not have to wait ~10 s for the document to age out.
import { ReconnectDecision, ReconnectInput, ReconnectPolicyState,
  RECONNECT_ACTION_LIMIT, RECONNECT_ACTION_RECONNECT, RECONNECT_ACTION_VERIFY_FAILED,
  RECONNECT_MARKER_OPEN, RECONNECT_MAX_ATTEMPTS,
  RECONNECT_RESULT_FAILED, RECONNECT_RESULT_LIMIT, RECONNECT_RESULT_RECONNECTED,
  RECONNECT_RESULT_SKIPPED, RECONNECT_TRIGGER_BACKGROUND, RECONNECT_TRIGGER_FOREGROUND,
  decideReconnect, reconnectInitialState, reconnectOnActive,
  reconnectOnStartRejected } from '../vpnextensionability/NetBirdReconnectPolicy';
// 2026-10-08 自动重连判据⑦: the ONE proof that the extension process really is
// gone. The status document going stale cannot be it (the system suspends the
// background extension for minutes while it stays alive), so the UI process's
// own APP_KILLED for this app's `:vpn` process is read through a read-only getter
// — process memory only, never persisted, never re-derived here.
import { lastKilledVpnProcess } from '../vpnextensionability/NetBirdSystemEvents';
import type { ProcessExitRecord } from '../vpnextensionability/NetBirdSystemEventsCore';
import { UiPeerEntry, UiPeerIceDetail, UiPeerIceEndpoint, NEEDS_LOGIN_CLASS,
  NEEDS_LOGIN_TEXT, connectFailureText,
  connectionLabel, filterPeers, homeStateText,
  lastErrorText, lastHandshakeText, onlinePeerCount, peerStatusText,
  sortPeers, vpnActiveState,
  SESSION_EXTENDED_TEXT, sessionExtendFailureText, sessionRowDeadlineS,
  sessionRowView } from '../vpnextensionability/NetBirdUiModel';
// 2026-10-11 retry re-entry (fix/retry) / 2026-10-12 upstream tap handling: the
// pure connect-entry + pending-action model. This page only gathers the facts
// (the extension's own stop evidence, its own latches) and acts through
// requestStart()/requestStop(); every verdict is the pure module's, host-checked
// byte-identically by run-ui-model.mts and run-connect-retry.mts.
import { CONNECT_LEG_AFTER_STOP, EXTENSION_STOP_POLL_MS,
  EXTENSION_STOP_WAIT_MS, PENDING_ACTION_TIMEOUT_MS, PENDING_TARGET_CONNECTED,
  PENDING_TARGET_DISCONNECTED, PENDING_TARGET_NONE, connectStartLeg, decideTransition,
  engineReportState } from '../vpnextensionability/NetBirdUiModel';
import type { TransitionDecision, TransitionFacts }
  from '../vpnextensionability/NetBirdUiModel';
import { DEFAULT_MANAGEMENT_URL, SETUP_CONFIG_FILE, SETUP_KEY_FILE,
  buildDeviceConfigJson, buildSetupKeyFileText, checkManagementUrl, checkSetupKey,
  readStoredManagementUrl, readStoredPrivateKey, readStoredRelayEnabled,
  KeyCheck, UrlCheck } from '../setup/NetBirdSetupConfig';
import { NetBirdAccountWebView } from '../login/NetBirdAccountWebView';
import { AccountLoginController } from '../login/NetBirdAccountLogin';
import type { AccountLoginResult, PreparedAccountLogin,
  SessionExtendResult } from '../login/NetBirdAccountLogin';
import { ACCOUNT_MODE_ACCOUNT, ACCOUNT_MODE_FILE_NAME, ACCOUNT_MODE_SETUP_KEY,
  parseAuthMode } from '../login/NetBirdAccountMode';
import { buildConnectorConfigJson, readPackagedCaRootsPem } from '../vpnextensionability/NetBirdConnector';
import type { DeviceConfigFile } from '../vpnextensionability/NetBirdConnector';
// 诊断（2026-10-05 日志导出增量）：本页只发安全事件（事件名 + 计数），绝不带
// Setup Key / 服务器地址 / 配置内容；日志导出的文件系统与上限逻辑全在
// NetBirdLogExport 里，本页只取结果并如实渲染。
import { DiagFields, event } from '../vpnextensionability/NetBirdDiagnostics';
// The extension's lifecycle marker (shared filesDir, written by the VPN role):
// state=open means that process never reached its stop callback — the ONE fact
// that separates "the system reclaimed it" from "the user stopped it". Read
// through the diagnostics core's own allowlisted path/parser; never re-derived
// here.
import { MARKER_BASENAME_VPN, Marker, parseMarker,
  recordPath } from '../vpnextensionability/NetBirdDiagnosticsCore';
import { exportDiagnostics, LogExportFileReport, LogExportResult,
  LOG_EXPORT_OUTCOME_CANCELLED, LOG_EXPORT_OUTCOME_EXPORTED,
  LOG_EXPORT_OUTCOME_FAILED, LOG_EXPORT_OUTCOME_NO_LOGS } from '../vpnextensionability/NetBirdLogExport';
// 路由选择 (2026-10-09, 即时生效增量): per-NetID route selection. Every switch
// action (one NetID / 全部启用 / 全部停用) saves the management-URL-scoped
// preference IMMEDIATELY and then writes the VPE's doorbell (an atomic request
// document) — the page never stops, starts or re-logs-in a connection for a
// route change. The page reads the public route catalog OFFLINE (a plain
// document, never a native connector getter — the UI process has no connector)
// and polls the VPE's apply RESULT document on its existing 2 s tick. All four
// documents live in the shared filesDir and are read/written through the
// routes/ stores.
import { RouteCatalog, RouteCatalogReadResult, RouteNetworkEntry, RouteSelection,
  CATALOG_STATUS_MISSING, CATALOG_STATUS_OK, CATALOG_STATUS_SCOPE_MISMATCH,
  CATALOG_STATUS_UNAVAILABLE, CATALOG_STATUS_VERSION, REASON_UNKNOWN_ROUTE,
  RoutePreferencesError,
  routeSelectionClone, routeSelectionDefault, routeSelectionDeselectAll,
  routeSelectionIsEnabled, routeSelectionKnownNetIds, routeSelectionRows,
  routeSelectionSelectAll, routeSelectionToggle } from '../routes/NetBirdRoutePreferences';
import { loadRouteSelection, readRouteCatalog,
  saveRouteSelection } from '../routes/NetBirdRouteStore';
// The live-route IPC pair: the request is ONLY a notification (no selection copy
// and no session binding — the VPE loads the CURRENT saved preference itself),
// and `routeApplyResultIsApplied` is the ONE reading of "the OS side really is at
// the requested state". The page never grows a second opinion about it.
import { RouteApplyRequest, RouteApplyResult,
  routeApplyResultIsApplied } from '../routes/NetBirdRouteRequest';
import { readRouteApplyResult,
  writeRouteApplyRequest } from '../routes/NetBirdRouteRequestStore';
import { ROUTE_TONE_ERROR, RouteApplyView, routeApplyRequestId, routeApplyStatus,
  routeApplyStatusText, routeApplyStatusTone, routeCandidateText, routeNetName,
  routePrefixesText } from '../routes/NetBirdRouteApplyPolicy';
// 2026-10-10 后台保活（0.0.19 实验版，方案 A）：借用系统的 DATA_TRANSFER 长时任务让
// 整个应用的进程不被挂起（鸿蒙没有 VPN 类型的长时任务，另一款鸿蒙 VPN 客户端也是这么
// 借的）。整机制裁留在纯逻辑模块 NetBirdKeepAlivePolicy（宿主 selfcheck 逐字节导入），
// 平台调用留在唯一适配层 NetBirdKeepAliveService（唯一 import backgroundTaskManager
// 的地方），开关值留在 NetBirdKeepAliveStore。本页只在已有的 2 s 轮询里收集事实、执行
// 决定，绝不新增定时器、不新增状态文档读取，也绝不让这条支线影响 VPN 的连接/断开。
import { KeepAliveCancelEvent, KeepAliveDecision, KeepAliveFacts, KeepAlivePolicyState,
  KeepAliveStatus,
  KEEP_ALIVE_ACTION_START, KEEP_ALIVE_ACTION_STOP, KEEP_ALIVE_NOTIFICATION_RECONNECTING,
  KEEP_ALIVE_REASON_GRACE, KEEP_ALIVE_REASON_LOGOUT, KEEP_ALIVE_REASON_USER_STOP,
  KEEP_ALIVE_REASON_VPN_ACTIVE, KEEP_ALIVE_RESUME_GAP,
  decideKeepAlive, keepAliveInitialState, keepAliveOnCancelled, keepAliveOnLogout,
  keepAliveOnRefreshed, keepAliveOnStartResolved, keepAliveOnUserConnect,
  keepAliveOnUserStop, keepAliveUserStopRecent, keepAliveWithTask, keepAlivePollGapMs,
  keepAliveResumedAfterGap,
  shouldRefresh } from '../vpnextensionability/NetBirdKeepAlivePolicy';
import { keepAliveTaskPresent, refreshKeepAlive, refreshKeepAliveText,
  setKeepAliveResumeListener, startKeepAlive,
  stopKeepAlive, takeKeepAliveCancels } from '../vpnextensionability/NetBirdKeepAliveService';
import { KEEP_ALIVE_DEFAULT_ENABLED, loadKeepAliveEnabled,
  saveKeepAliveEnabled } from '../vpnextensionability/NetBirdKeepAliveStore';
// 2026-10-13 PC 托盘（0.0.29）；2026-10-13 统一退出（0.0.31）: the status-bar icon's
// menu actions and the state it shows. 连接/断开 ride exactly the two methods the
// page's own toggle uses (requestStart / requestStop) — the tray is a second
// ENTRY POINT, never a second control path; 打开主窗口 is handled inside the tray
// service (startAbility); 退出 runs the shared quit flow, which disconnects
// first, exactly like upstream's tray handleQuit(). This page also OWNS the one
// vpnExtension stop call site (requestStop) and registers it as the quit flow's
// disconnect step, so the system-generated tray 退出 (which arrives at
// EntryAbility.onPrepareToTerminate) tears down the VPN the same way. On a phone
// none of the tray part is ever live: the service answers false once and its
// state push is a no-op.
import { TRAY_MENU_CONNECT, TRAY_MENU_DISCONNECT, TRAY_MENU_QUIT }
  from '../tray/NetBirdTrayModel';
import { setTrayActionListener, updateTrayState } from '../tray/NetBirdTrayService';
import { QUIT_REASON_SETTINGS, QUIT_REASON_TRAY, requestQuit, setQuitDisconnect }
  from '../tray/NetBirdQuitFlow';

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';
const BUNDLE_NAME: string = 'cn.alfadb.netbird';
const EXTENSION_ABILITY: string = 'NetBirdVpnExtensionAbility';
const STATUS_POLL_MS: number = 2000;
// 2026-10-12 upstream tap handling: the recorded connect-failure verdict is shown
// as a Toast carrying the engine's own message (upstream Toast.LENGTH_LONG,
// MainActivity.java:1030-1036) exactly ONCE per verdict; the persistent channels
// stay the card's official state and the red error line.
const FAILURE_TOAST_MS: number = 4000;
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

export function navigationBackDecision(accountWebView: boolean, accountBusy: boolean,
  accountCommitting: boolean, configuredEditor: boolean, diagnostics: boolean,
  peerDetail: boolean, routeDetail: boolean): string {
  if (accountWebView) { return 'account-webview'; }
  if (accountBusy) { return accountCommitting ? 'consume' : 'cancel-account'; }
  if (configuredEditor) { return 'cancel-editor'; }
  if (diagnostics) { return 'close-diagnostics'; }
  if (peerDetail) { return 'close-peer'; }
  // 路由选择: an internal SETTINGS page — a back press closes it back to 设置
  // (the tab it was opened from), never to the desktop and never over the
  // editor/diagnostics/peer handlers above.
  if (routeDetail) { return 'close-route'; }
  return 'exit';
}

interface BusinessFailure extends Error {
  code?: number;
}

// The extension's `netbird-logout-result` answer body.
interface LogoutResultFile {
  ok: boolean;
  reason?: string;
  ticket_ms: number;
}

// ---------------------------------------------------------------------------
// 2026-10-07 详细状态页 (the client-side counterpart of `netbird status -d`):
// the published document carries the client-detail sections as loosely typed
// Objects (NetBirdConnector.PublishedStatus), so these local mirrors are read
// LENIENTLY: every field is type-checked at the read site, and an ABSENT
// section/field renders the honest 未知 — never a fabricated zero, 直连,
// 「已连接」 or 「未启用」. An older document (written before this increment)
// simply omits the sections and the same 未知 wording applies.
//
// Timestamp units follow the document contract exactly:
//   peers[].last_handshake_s  = AGE in seconds since the handshake (relative)
//   session_expires_at_s      = absolute Unix SECONDS (epoch)
//   updated_at_ms             = absolute Unix MILLISECONDS (epoch)
// ---------------------------------------------------------------------------

interface DetailManagementView {
  scheme: string;
  host: string;
  port: number;
}

interface DetailDnsView {
  ip: string;
  port: number;
}

interface DetailRouteView {
  network: string;
  is_default: boolean;
}

interface DetailDefaultRouteView {
  allowed: boolean;
  reason: string;
}

interface DetailNetworkView {
  available: boolean;
  prefix: string | null;
  interface_dns: string | null;
  dns_servers: DetailDnsView[];
  routes: DetailRouteView[];
  default_route: DetailDefaultRouteView | null;
  signal_uri: string | null;
}

interface DetailTrafficView {
  tx_packets: number;
  tx_bytes: number;
  rx_packets: number;
  rx_bytes_to_tun: number;
  dropped_no_route: number;
  decrypt_errors: number;
}

interface DetailRelayView {
  enabled: boolean;
  state: string;
  current_url: string | null;
  urls: string[];
  reconnects: number;
  transport_bytes: number;
  token_valid: boolean;
  last_error_class: string | null;
}

// One label/value line of the detail screen.
interface DetailField {
  label: string;
  value: string;
}

// Wording for an absent section/field: the document either predates the field
// or the core never delivered it — BOTH are 未知, never a default value.
const UNKNOWN_DOC: string = '未知（状态文档未提供）';
const UNKNOWN_CORE: string = '未知（核心未上报）';
// The route row lists at most this many routes inline; past it the detail page
// switches to one viewable row per route (nothing is silently hidden).
const ROUTE_SUMMARY_LIMIT: number = 8;
// Line cap for a DetailRow that carries a whole multi-line block (the per-peer
// status / ICE blocks). The SDK accepts [0, INT32_MAX]; 0 shows nothing and
// INT32_MAX risks layout arithmetic overflow, so this stays a large finite cap:
// far past anything a phone screen (inside the page's outer Scroll) can render,
// which is what "show the full block" means in practice.
const DETAIL_BLOCK_MAX_LINES: number = 1000;

// 2026-10-13: WHAT the one ArkWeb PKCE surface was opened FOR. The SSO run is
// the same in all three cases (upstream `runOAuthFlow`,
// client/android/login.go:216-232), so there is ONE webview/callback pair; the
// intent decides what the exchanged JWT is handed to afterwards:
//   setup   → the management Login/registration, then persist + connect
//             (`auth.login(...)` before `connectClient.Run`, client.go:194-215)
//   connect → the SAME management Login, then connect — the connect toggle's
//             login-required branch, upstream HomeFragment.java:193-200 where
//             `switchConnection(true)` makes the engine start run the
//             interactive SSO
//   extend  → the management ExtendAuthSession and nothing else
//             (`client/android/session.go:127-155`)
const ACCOUNT_INTENT_SETUP: string = 'setup';
const ACCOUNT_INTENT_CONNECT: string = 'connect';
const ACCOUNT_INTENT_EXTEND: string = 'extend';
// Upstream's extend toasts: LENGTH_SHORT on success and LENGTH_LONG on failure
// (MainActivity.java:1073-1086). Same 4 s as the connect-failure toast.
const SESSION_TOAST_MS: number = 4000;

// Lenient section readers: undefined/null (absent section or an explicit null)
// return null; anything else is asserted to the local mirror and every field
// read is guarded individually below. Never throws.
function readNetworkView(raw: Object | null | undefined): DetailNetworkView | null {
  if (raw === undefined || raw === null) {
    return null;
  }
  return raw as DetailNetworkView;
}

function readTrafficView(raw: Object | null | undefined): DetailTrafficView | null {
  if (raw === undefined || raw === null) {
    return null;
  }
  return raw as DetailTrafficView;
}

function readRelayView(raw: Object | null | undefined): DetailRelayView | null {
  if (raw === undefined || raw === null) {
    return null;
  }
  return raw as DetailRelayView;
}

// 2026-10-10 后台保活（实验）: the live-view body's four numbers, taken from the
// SAME status document the 2 s tick already read (never a second read). A
// missing or absurd counter stays -1 = 未知 and is rendered as such by the pure
// policy — a counter that could not be read is never shown as 0.
function keepAliveStatusOf(status: PublishedStatus): KeepAliveStatus {
  const peers: UiPeerEntry[] =
    status.peers !== undefined && status.peers !== null ? status.peers : [];
  const traffic: DetailTrafficView | null = readTrafficView(status.wg_traffic);
  const txBytes: number = traffic !== null && Number.isFinite(traffic.tx_bytes)
    ? traffic.tx_bytes : -1;
  const rxBytes: number = traffic !== null && Number.isFinite(traffic.rx_bytes_to_tun)
    ? traffic.rx_bytes_to_tun : -1;
  const keepAliveStatus: KeepAliveStatus = {
    onlinePeers: onlinePeerCount(peers),
    peerCount: peers.length,
    txBytes: txBytes,
    rxBytes: rxBytes
  };
  return keepAliveStatus;
}

// The management authority section: scheme + host + port only (the published
// document never carries the configured URL string). null when absent OR when
// the object carries no host — an endpoint without a host is not renderable
// and must not print "://".
function readManagementView(raw: Object | null | undefined): DetailManagementView | null {
  if (raw === undefined || raw === null) {
    return null;
  }
  const view: DetailManagementView = raw as DetailManagementView;
  if (typeof view.host !== 'string' || view.host.length === 0) {
    return null;
  }
  return view;
}

// Human-readable byte size for the aggregate counters (binary units). A
// non-finite/negative value is 未知, not "0 B".
function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) {
    return '未知';
  }
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units: string[] = ['KB', 'MB', 'GB', 'TB', 'PB'];
  let value: number = bytes / 1024;
  let index: number = 0;
  while (value >= 1024 && index < units.length - 1) {
    value = value / 1024;
    index++;
  }
  return `${Math.round(value * 10) / 10} ${units[index]}`;
}

// Formatted size + the exact counter value (the diagnostics page is where the
// byte-reconciliation numbers must stay readable verbatim).
function byteText(bytes: number): string {
  return `${formatBytes(bytes)}（${bytes} 字节）`;
}

function pad2(value: number): string {
  return value < 10 ? `0${value}` : `${value}`;
}

// Absolute Unix SECONDS (epoch) rendered in the device's local zone. This is
// NOT the handshake field: session_expires_at_s is a deadline, whereas
// peers[].last_handshake_s is an age and is rendered by lastHandshakeText.
function formatEpochSeconds(epochS: number): string {
  const date: Date = new Date(epochS * 1000);
  return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())} ` +
    `${pad2(date.getHours())}:${pad2(date.getMinutes())}:${pad2(date.getSeconds())}`;
}

// Remaining time until an absolute Unix-SECONDS deadline ('已到期' once past).
function remainingText(epochS: number): string {
  const seconds: number = Math.floor(epochS - Date.now() / 1000);
  if (seconds <= 0) {
    return '已到期';
  }
  const days: number = Math.floor(seconds / 86400);
  const hours: number = Math.floor((seconds % 86400) / 3600);
  const minutes: number = Math.floor((seconds % 3600) / 60);
  if (days > 0) {
    return `剩余 ${days} 天 ${hours} 小时`;
  }
  if (hours > 0) {
    return `剩余 ${hours} 小时 ${minutes} 分`;
  }
  if (minutes > 0) {
    return `剩余 ${minutes} 分`;
  }
  return `剩余 ${seconds} 秒`;
}

// Relative age of a millisecond epoch stamp (the published document itself).
function secondsAgoText(epochMs: number): string {
  const seconds: number = Math.max(0, Math.floor((Date.now() - epochMs) / 1000));
  if (seconds < 60) {
    return `${seconds} 秒前`;
  }
  const minutes: number = Math.floor(seconds / 60);
  if (minutes < 60) {
    return `${minutes} 分钟前`;
  }
  return `${Math.floor(minutes / 60)} 小时前`;
}

// 运行时长 (2026-10-07): the CONNECTOR PROCESS's own age, computed from the
// core's `started_at_unix` (Unix SECONDS, passed through the published
// document, which carries it only for a positive integer stamp). It is a
// process clock — never a management connection verdict (that stays 状态 /
// 信号连接). An absent stamp is 未知, exactly like every other field the
// document did not carry.
function uptimeText(startedAtUnix: number | undefined): string {
  if (typeof startedAtUnix !== 'number' || !Number.isInteger(startedAtUnix) ||
    startedAtUnix <= 0) {
    return UNKNOWN_DOC;
  }
  const seconds: number = Math.floor(Date.now() / 1000) - startedAtUnix;
  return `已运行 ${durationText(seconds)}（核心启动于 ${formatEpochSeconds(startedAtUnix)}）`;
}

// Human duration for a second count. A sub-minute age — including a
// clock-skewed negative one — reads 不足 1 分钟, never a negative duration.
function durationText(seconds: number): string {
  if (seconds < 60) {
    return '不足 1 分钟';
  }
  const days: number = Math.floor(seconds / 86400);
  const hours: number = Math.floor((seconds % 86400) / 3600);
  const minutes: number = Math.floor((seconds % 3600) / 60);
  if (days > 0) {
    return `${days} 天 ${hours} 小时`;
  }
  if (hours > 0) {
    return `${hours} 小时 ${minutes} 分`;
  }
  return `${minutes} 分钟`;
}

// The relay client's runtime state token (core relay_state_token contract:
// disconnected | dialing | handshaking | authenticating | ready |
// reconnecting | dead) rendered in Chinese; an unknown token prints verbatim
// instead of being guessed into a plausible state.
function relayStateText(state: string): string {
  if (typeof state !== 'string' || state.length === 0) {
    return UNKNOWN_DOC;
  }
  if (state === 'ready') {
    return '已就绪（中继会话可用）';
  }
  if (state === 'dialing') {
    return '拨号中';
  }
  if (state === 'handshaking') {
    return '握手中';
  }
  if (state === 'authenticating') {
    return '认证中';
  }
  if (state === 'reconnecting') {
    return '重连中';
  }
  if (state === 'disconnected') {
    return '未连接';
  }
  if (state === 'dead') {
    return '已停止';
  }
  return state;
}

// One counter rendered as "N 包" — 未知 when the document did not carry a
// number (never "undefined 包").
function packetsText(packets: number): string {
  return typeof packets === 'number' ? `${packets} 包` : UNKNOWN_DOC;
}

// 对端 bearer line. carrier_known=false means the core's WG session view did
// not carry this peer (map-only peer): the two bearer booleans are then NOT
// observations, so the honest answer is 未知 — never 直连 and never "未使用
// 中继".
function peerCarrierText(peer: UiPeerEntry): string {
  if (!peer.carrier_known) {
    return '未知（核心未上报该对端的会话信息）';
  }
  if (peer.on_carrier) {
    return peer.carrier_attached ? '经中继承载（已附着）' : '经中继承载（未附着）';
  }
  return '未使用中继';
}

// 对端 status line: management-online is NOT the same fact as "this device
// has a tunnel to the peer" — the three-state distinction is spelled out.
function peerStatusDetailText(peer: UiPeerEntry): string {
  if (peer.status === 'connected') {
    return '已连接（本机隧道已建立）';
  }
  if (peer.status === 'idle') {
    return '空闲（管理端在线，本机暂无隧道会话）';
  }
  return '离线（管理端标记为离线）';
}

// 中继通道地址 (2026-10-07): the peer's OWN actual ATTACHED relay lane from
// the core's live attached-lane table (UiPeerEntry.actual_relay_url) — the
// address that lane really rides, never the LOCAL relay session
// (UiStatusDocument.relay_url) and never a merely configured relay. Three
// states, read apart:
//   actual_relay_known !== true (key absent / lane table not delivered) -> 未知;
//   known true, value null -> the table WAS delivered and NO lane is attached
//     to this peer: an observation about the lane table only. It does NOT say
//     the peer is 直连 and does NOT say no relay transport exists — the
//     selected bearer stays the connection/on_carrier verdict (承载 row);
//   known true, value string -> the attached lane URL verbatim. An attached
//     lane can coexist with a direct path as a standby, so an address here
//     never means "this peer is being relayed right now".
function peerRelayText(peer: UiPeerEntry): string {
  if (peer.actual_relay_known !== true) {
    return '未知（核心未上报该对端的中继通道）';
  }
  const url: string | null | undefined = peer.actual_relay_url;
  if (typeof url === 'string' && url.length > 0) {
    return url;
  }
  if (url === null || url === undefined) {
    return '无已附着的中继通道（不代表直连）';
  }
  return '未知（核心未上报该对端的中继通道）';
}

// One per-peer payload byte counter (core wg_peers[] tx_bytes / rx_bytes: the
// peer's PLAINTEXT inner-IP bytes, cumulative since THAT peer's tunnel was
// created). The published peer view carries the number when the core delivered
// one and OMITS the key for a map-only peer, so a missing key is 未知 while a
// delivered 0 stays a real 0 B. These are NOT the aggregate 流量统计 rows:
// 发送（加密后）there counts ENCAPSULATED wire bytes — the two must never share
// a label.
function peerPayloadBytesText(peer: UiPeerEntry, sent: boolean): string {
  const value: number | undefined = sent ? peer.tx_bytes : peer.rx_bytes;
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) {
    return UNKNOWN_CORE;
  }
  return byteText(value);
}

// ---- 2026-10-08 per-peer ICE 明细 (core `ice_peers`) -----------------------
// Display helpers for UiPeerEntry.ice. `ice` ABSENT means the core delivered no
// readable ICE detail for this peer: ICE 未知 (an observation, never a default
// state and never a placeholder endpoint). A PRESENT detail with null endpoints
// means no candidate pair was selected YET: 尚未选中 (never candidate #1).
//
// ICE is a CANDIDATE-PATH view only. It never overrides the WG-session verdict
// (状态 / 连接方式 / 承载 / 中继通道地址), and "ICE 已连接" never means the
// current traffic is direct — `reachable` is the core's own usability flag, and
// a `disconnected` peer keeps its retained pair with reachable=false, so those
// endpoints are labelled as the PREVIOUS selection instead of a live path.

function iceStateText(state: string): string {
  if (typeof state !== 'string' || state.length === 0) {
    return '未知（核心未给出状态）';
  }
  if (state === 'idle') {
    return '空闲（idle）';
  }
  if (state === 'gathering') {
    return '收集中（gathering）';
  }
  if (state === 'checking') {
    return '连通性检查中（checking）';
  }
  if (state === 'connected') {
    return '已连接（connected）';
  }
  if (state === 'disconnected') {
    return '已断开（disconnected）';
  }
  if (state === 'failed') {
    return '失败（failed）';
  }
  return state;
}

// 主导 = this device controls the session, 响应 = the peer does. Only the
// core's own boolean is a role; null (no session established, or not
// delivered) is 未知 — never a default role.
function iceRoleText(controlling: boolean | null): string {
  if (controlling === true) {
    return '主导（本机为控制方）';
  }
  if (controlling === false) {
    return '响应（对端为控制方）';
  }
  return '未知（核心未给出角色）';
}

// The selected-pair candidate type in Chinese, token kept in parentheses; an
// unexpected token prints verbatim instead of being guessed into a type.
function iceCandidateTypeText(type: string): string {
  if (typeof type !== 'string' || type.length === 0) {
    return '未知类型';
  }
  if (type === 'host') {
    return '主机（host）';
  }
  if (type === 'srflx') {
    return '服务器反射（srflx）';
  }
  if (type === 'prflx') {
    return '对端反射（prflx）';
  }
  if (type === 'relay') {
    return '中继（relay）';
  }
  return type;
}

// How to read a selected pair the detail carries: a reachable detail is the
// CURRENT selection; a disconnected/failed one may retain its last pair, which
// is then labelled as the previous selection (never a live path); anything
// else non-reachable is 已选中但当前不可用.
function iceSelectionPrefixText(ice: UiPeerIceDetail): string {
  if (ice.reachable === true) {
    return '当前选中';
  }
  if (ice.state === 'disconnected' || ice.state === 'failed') {
    return '上次选中（连接已断开，可能已失效）';
  }
  return '已选中（当前不可用）';
}

// One selected-pair endpoint: candidate type + address:port. null/absent is
// 尚未选中 — an observation about THIS detail (it was delivered), never 未知
// and never the first candidate standing in for a missing selection.
function iceEndpointText(endpoint: UiPeerIceEndpoint | null | undefined,
  ice: UiPeerIceDetail): string {
  if (endpoint === null || endpoint === undefined) {
    return '尚未选中';
  }
  const address: string = typeof endpoint.address === 'string' && endpoint.address.length > 0
    ? endpoint.address : '地址未知';
  const port: string = typeof endpoint.port === 'number' ? `${endpoint.port}` : '端口未知';
  return `${iceSelectionPrefixText(ice)}：${iceCandidateTypeText(endpoint.type)} ${address}:${port}`;
}

// The core's usability verdict for this peer's ICE path (Connected AND the WG
// endpoint was applied). It is NOT the WG connection verdict — that stays
// 状态 / 连接方式 / 承载 / 中继通道地址.
function iceReachText(ice: UiPeerIceDetail): string {
  if (ice.reachable === true) {
    return '可达（ICE 已连接且 WG 端点已应用）';
  }
  if (ice.endpoint_applied === true) {
    return '不可达（WG 端点曾应用，当前不可用）';
  }
  return '不可达（尚未应用 WG 端点）';
}

function iceErrorText(ice: UiPeerIceDetail): string {
  const errorClass: string | null = ice.last_error_class;
  return typeof errorClass === 'string' && errorClass.length > 0 ? errorClass : '无';
}

// The per-peer ICE block of the detailed-status screen (6 lines, one block per
// peer). An absent detail collapses to the single 未知 line.
function peerIceBlockText(peer: UiPeerEntry): string {
  const ice: UiPeerIceDetail | undefined = peer.ice;
  if (ice === undefined || ice === null) {
    return '未知（核心未上报该对端的 ICE 详情）';
  }
  const lines: string[] = [];
  lines.push(`ICE 状态：${iceStateText(ice.state)}`);
  lines.push(`ICE 角色：${iceRoleText(ice.controlling)}`);
  lines.push(`ICE 本端：${iceEndpointText(ice.selected_local, ice)}`);
  lines.push(`ICE 对端：${iceEndpointText(ice.selected_remote, ice)}`);
  lines.push(`ICE 可达性：${iceReachText(ice)}`);
  lines.push(`ICE 最近错误：${iceErrorText(ice)}`);
  return lines.join('\n');
}

// The per-peer block of the detail screen: endpoint / local egress / overlay
// networks, each with its honest unavailable wording. The 中继通道地址 line is
// the peer's OWN actual attached lane (peerRelayText), kept apart from the
// LOCAL relay session URL — the local relay URL is never presented as the
// peer's.
function peerBlockText(peer: UiPeerEntry): string {
  const lines: string[] = [];
  lines.push(`状态：${peerStatusDetailText(peer)}`);
  lines.push(`连接方式：${peer.online ? connectionLabel(peer.connection) : '未知（无本机会话）'}`);
  lines.push(`NetBird IP：${peer.ip.length > 0 ? peer.ip : '未知'}`);
  lines.push(`对端端点：${peer.endpoint.length > 0 ? peer.endpoint : UNKNOWN_CORE}`);
  lines.push(`本机出口：${peer.egress_local.length > 0 ? peer.egress_local : UNKNOWN_CORE}`);
  lines.push(`对端网络：${peer.allowed_ips.length > 0 ? peer.allowed_ips.join('、') : UNKNOWN_CORE}`);
  lines.push(`承载：${peerCarrierText(peer)}`);
  lines.push(`中继通道地址：${peerRelayText(peer)}`);
  lines.push(`最近握手：${lastHandshakeText(peer.last_handshake_s)}`);
  lines.push(`发送（明文）：${peerPayloadBytesText(peer, true)}`);
  lines.push(`接收（明文）：${peerPayloadBytesText(peer, false)}`);
  return lines.join('\n');
}

// The configured resolver set in management order (priority order, verbatim).
// Port 0 means the core reported no port — the bare IP is then printed rather
// than a fabricated :0.
function dnsServersText(network: DetailNetworkView): string {
  if (network.dns_servers === undefined || network.dns_servers === null ||
    network.dns_servers.length === 0) {
    return '未配置';
  }
  const parts: string[] = [];
  for (let i = 0; i < network.dns_servers.length; i++) {
    const server: DetailDnsView = network.dns_servers[i];
    if (server === undefined || server === null || typeof server.ip !== 'string' ||
      server.ip.length === 0) {
      continue;
    }
    parts.push(typeof server.port === 'number' && server.port > 0
      ? `${server.ip}:${server.port}` : server.ip);
  }
  return parts.length > 0 ? parts.join('、') : '未配置';
}

// The managed route set as exported (network.routes). A held 0.0.0.0/0 is
// ABSENT here BY CONTRACT — it is reported by the default-route gate below,
// never invented into this list. Up to ROUTE_SUMMARY_LIMIT routes are listed
// inline; past that the row carries only the count and networkFields() renders
// EVERY route as its own row — the detail page must not silently hide routes
// beyond the old 8-entry summary.
function routeLine(route: DetailRouteView): string {
  return route.is_default === true ? `${route.network}（默认路由）` : route.network;
}

function routesText(network: DetailNetworkView): string {
  if (network.routes === undefined || network.routes === null ||
    network.routes.length === 0) {
    return '未下发';
  }
  const total: number = network.routes.length;
  if (total > ROUTE_SUMMARY_LIMIT) {
    return `共 ${total} 条（下列逐条列出）`;
  }
  const listed: string[] = [];
  for (let i = 0; i < total; i++) {
    const route: DetailRouteView = network.routes[i];
    if (route === undefined || route === null || typeof route.network !== 'string' ||
      route.network.length === 0) {
      continue;
    }
    listed.push(routeLine(route));
  }
  return listed.length > 0 ? `${listed.join('、')}（共 ${total} 条）` : `${total} 条（核心未给出网络）`;
}

// The default-route gate verdict: allowed=true means 0.0.0.0/0 was really
// handed to the tunnel; allowed=false means it is HELD (the core's reason
// token says why). A missing verdict is 未知 — never "held" or "allowed".
function defaultRouteText(network: DetailNetworkView): string {
  const gate: DetailDefaultRouteView | null = network.default_route;
  if (gate === null || gate === undefined) {
    return UNKNOWN_CORE;
  }
  if (gate.allowed === true) {
    return '已下发（0.0.0.0/0 默认路由）';
  }
  const reason: string = typeof gate.reason === 'string' && gate.reason.length > 0
    ? gate.reason : '核心未给出原因';
  return `已保留未下发（原因：${reason}）`;
}

// One label/value row of the detail screens. A child @Component with @Prop —
// NOT the old @Builder(fieldRow): a @Builder's by-value expression arguments
// are frozen at mount in ArkUI (the B.1 device defect), while @Prop re-syncs
// from the parent every time the observed published state re-renders.
@Component
struct DetailRow {
  @Prop label: string = '';
  @Prop value: string = '';
  // 2026-10-08: parameterized line cap. 8 is right for the single-value rows
  // (a wrapped endpoint/error line), but the per-peer status rows carry a
  // WHOLE multi-line block (peerBlockText/peerIceBlockText: 11 / 6 logical
  // lines plus wrapping), which .maxLines(8) truncated mid-block — the tail
  // 最近握手 / 发送（明文）/ 接收（明文） lines never rendered. Block callers
  // raise this (DETAIL_BLOCK_MAX_LINES); the detail page's outer Scroll carries
  // the extra height, so nothing is clipped elsewhere.
  @Prop maxLines: number = 8;

  build() {
    Row({ space: 8 }) {
      Text(this.label)
        .fontSize(13)
        .fontColor($r('app.color.nb_text_muted'))
        .width(96)
      // Truthiness (not .length): a value that slipped through as null or
      // undefined renders '—' instead of throwing inside the render pass.
      Text(this.value ? this.value : '—')
        .fontSize(14)
        .fontColor($r('app.color.nb_text_primary'))
        .layoutWeight(1)
        .maxLines(this.maxLines)
        .textOverflow({ overflow: TextOverflow.Ellipsis })
    }
    .width('100%')
    .alignItems(VerticalAlign.Top)
    .padding({ top: 6, bottom: 6 })
  }
}

// One collapsible section header. `expanded` and `summary` are @Prop, so the
// arrow and the live summary follow the polled state; tapping calls back into
// the page's own @State toggle.
@Component
struct DetailSectionHeader {
  @Prop title: string = '';
  @Prop summary: string = '';
  @Prop expanded: boolean = false;
  onToggle: () => void = (): void => {
  };

  build() {
    Row({ space: 8 }) {
      Text(this.title ? this.title : '')
        .fontSize(15)
        .fontWeight(FontWeight.Medium)
        .fontColor($r('app.color.nb_text_secondary'))
      Text(this.summary ? this.summary : '')
        .fontSize(12)
        .fontColor($r('app.color.nb_text_muted'))
        .layoutWeight(1)
        .maxLines(1)
        .textAlign(TextAlign.End)
        .textOverflow({ overflow: TextOverflow.Ellipsis })
      Text(this.expanded ? '▾' : '▸')
        .fontSize(14)
        .fontColor($r('app.color.nb_text_muted'))
    }
    .width('100%')
    .padding({ top: 12, bottom: 8 })
    .onClick(() => {
      this.onToggle();
    })
  }
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
  // 2026-10-13 PC 托盘（0.0.29）: one explicit quit at a time. Not @State — it
  // paints nothing; it only keeps a repeated 退出点击 from issuing a second
  // disconnect while the first quit flow is running.
  private quitInFlight: boolean = false;
  @State actionError: string = '';
  // roam-1005-0014 A.3: shown when the status document VANISHES while the
  // last live snapshot was in an active state and no user stop is in flight
  // — the extension fail-closed (interface lost) or died; either way the
  // tunnel is down and the user must reconnect explicitly. Cleared by a new
  // start request.
  @State disconnectNotice: string = '';
  // 2026-10-06 登录失败可见性: the error class of the persistent
  // connect-failure notice (extension aborted before the VPN existed /
  // connector ended terminally — e.g. login fatal auth, retry budget
  // exhausted). It is the engine's MESSAGE, not a state: the card keeps the
  // official label (未连接, or 需要登录 for the login class) and the reason rides
  // the error line + the one Toast; cleared by a fresh start.
  @State connectFailureClass: string = '';
  // ---- navigation ----
  @State activeTab: number = 0;
  @State detailStatusOriginTab: number = 2;
  @State selectedPeer: UiPeerEntry | null = null;
  @State peerQuery: string = '';
  @State showDiagnostics: boolean = false;
  @State versionName: string = '';
  @State versionCode: number = -1;
  // 详细状态页 section toggles (all open by default: the detailed screen is
  // the place where status must NOT be hidden; collapsing is the user's
  // choice, and every header keeps carrying a live summary).
  @State detailOverviewOpen: boolean = true;
  @State detailNetworkOpen: boolean = true;
  @State detailTrafficOpen: boolean = true;
  @State detailRelayOpen: boolean = true;
  @State detailPeersOpen: boolean = true;
  // ---- first-run setup / login card (settings tab). `configured` mirrors
  // persisted credentials; opening the editor never changes that fact.
  @State configured: boolean = false;
  @State configuredUrl: string = '';
  @State keyStored: boolean = false;
  @State showSetupForm: boolean = false;
  @State urlInput: string = DEFAULT_MANAGEMENT_URL;
  @State keyInput: string = '';
  @State formError: string = '';
  @State formHint: string = '';
  @State authMode: string = ACCOUNT_MODE_SETUP_KEY;
  @State accountLoginOpen: boolean = false;
  @State accountLoginBusy: boolean = false;
  @State accountLoginCommitting: boolean = false;
  @State accountLoginError: string = '';
  @State accountAuthorizeUrl: string = '';
  @State accountExpectedState: string = '';
  @State accountRedirectUri: string = '';
  // 2026-10-13 会话延长: the home session row's own facts. `sessionExtendAtS` is
  // the server's answer to the LAST successful extend in this page (0 = none);
  // `sessionExtendBusy` latches the row against a second tap while the SSO
  // surface is coming up. Both are @State: the row's visibility and label are
  // re-rendered from them.
  @State sessionExtendAtS: number = 0;
  @State sessionExtendBusy: boolean = false;
  private accountLoginController: AccountLoginController = new AccountLoginController();
  private accountLoginGeneration: number = 0;
  // WHAT the open SSO surface is for, and the management URL it was opened
  // against (captured at `begin` time: the form behind the webview must not be
  // able to move the target of an in-flight flow).
  private accountLoginIntent: string = ACCOUNT_INTENT_SETUP;
  private accountLoginTargetUrl: string = '';
  private requestId: string = `ui-${Date.now()}`;
  private observedPublishedStatus: boolean = false;
  // The state of the last LIVE document (drives the disappearance notice:
  // active → null without a user stop = the extension fail-closed / died).
  private lastLiveState: string = '';
  private lastUserStopAtMs: number = 0;
  // 0.0.28: the page's OWN stop-then-start re-entry leg
  // (stopVpnExtensionThenStart) arms THIS latch — never the user-stop one above.
  // Until 0.0.28 that leg wrote `lastUserStopAtMs`, and the keep-alive policy
  // reads that as "the user just asked for a disconnect": the tick 1-3 s after
  // every automatic reconnect withdrew the borrowed task as
  // `ui_bg_task stage=stop reason=user-stop`, so the NEXT extension kill had
  // nothing to recover it until the app came back to the foreground (v027b
  // device log: 90.5 min and 28.0 min with no tunnel). One use only: suppressing
  // the false "VPN 已断开" notice when the document that disappears is our own
  // stop, exactly what the old line was for.
  private lastRestartLegAtMs: number = 0;
  // When the current/last start request was sent — gates the persistent
  // failure notice to THIS attempt (a leftover from an earlier attempt is
  // never shown as the live verdict).
  private lastStartAtMs: number = 0;
  // 2026-10-12 upstream tap handling (ALIGNMENT-AUDIT §1.3 R5): upstream bounds
  // the TAP, not the attempt. `pendingActionTarget` is the requested engine state
  // of the last tap (upstream's pendingTarget: HomeFragment.java:198-200, :623-630) and the
  // card says 连接中…/正在断开… from it until the engine reports (or 7 s pass).
  // @State because it is what re-paints the toggle when the 7 s window closes.
  @State pendingActionTarget: string = PENDING_TARGET_NONE;
  // 0 = not armed. Armed at the tap with PENDING_ACTION_TIMEOUT_MS; never
  // extended (upstream posts one Runnable at beginPendingAction).
  private pendingActionDeadlineMs: number = 0;
  // The force-cancel window (upstream armForceCancel/openForceCancelWindow):
  // while a transition is painted the toggle is disabled, and 2 s into it the
  // toggle re-enables so a tap force-cancels (stop) the stuck transition.
  // @State: opening the window must repaint the toggle WITHOUT any other state
  // changing (a start with no report changes nothing else for seconds).
  @State forceCancelOpen: boolean = false;
  private forceCancelArmedAtMs: number = 0;
  // upstream isTransitioning(): a transition is being painted, so the toggle is
  // disabled until the force-cancel window opens. Written by the tick's pure
  // decideTransition() — the tap handler reads it, so the rule lives in ONE place.
  private transitionPainted: boolean = false;
  // The connect-failure notice already superseded by THIS attempt's own engine
  // report (the start evidence that replaced the deleted ack document). One read
  // per request, never one per tick.
  private noticeSuperseded: boolean = false;
  // at_ms of the recorded verdict already shown as a Toast (0 = none) — ONE toast
  // per verdict, never one per 2 s tick.
  private failureToastedAtMs: number = 0;
  // Explicit logout in flight (ticket written): masks the extension's
  // teardown-driven status-document disappearance so the logout the user
  // just asked for is not reported as an unexpected "VPN 已断开".
  private lastLogoutAtMs: number = 0;
  private logoutTicketMs: number = 0;
  private pollTimer: number = -1;
  // 2026-10-08 扩展被系统回收后的自动重连: the anti-loop state of the pure
  // NetBirdReconnectPolicy (attempts / last attempt / verification window), ONE
  // per UI process, disarmed again by an ACTIVE status document.
  private reconnectState: ReconnectPolicyState = reconnectInitialState();
  // Foreground latch for the SAME feature: the poll is a page timer, so it does
  // not run while the page is hidden — 0.0.20: unless the borrowed keep-alive task
  // keeps the whole app running, which the pure policy receives as
  // `keepAliveActive` (a held task is the ONE thing that opens the gate in the
  // background; every other criterion still has to pass).
  private pageVisible: boolean = false;
  // 0.0.20: the tunnel was seen ACTIVE at least once in this process, and the
  // moment it was last seen leaving the active states (0 = never). The keep-alive
  // grace is measured from that moment, so this page is the only clock it needs.
  private vpnActiveSeen: boolean = false;
  private vpnInactiveSinceMs: number = 0;
  // 0.0.28（保活跟随用户意图，而不是连接器的瞬时状态）：用户「VPN 开着」的意图锁存器。
  // 一旦本进程真的见过一次在跑的隧道就是 true，直到有事实结束这个意图才回落：真正的用户
  // 断开（requestStop）、登出（performLogout）、后台保活开关被关掉（onKeepAliveToggle）、
  // 引擎自己报出的死亡状态（failed/disconnected/closed/stopped 的活文档），或一条「这次
  // 连接根本没起来」的失败裁决（connect-failure 通知，needs-login 属于其中）。
  // 它故意跨过一切 TRANSIENT 状态：connecting / reconnecting / netLost / 扩展被挂起或被杀
  // ——那正是借来的长时任务必须活下来的窗口（见 0.0.28 真机数据：PC 在 11:32:08 进入
  // reconnecting，随后机器睡去、扩展不再发布文档，第一个越过 2 min 宽限的节拍就在 11:48:34
  // 把任务撤销成 vpn-inactive，而网络要到 8 h 10 m 后才回来）。上游 Android 的前台服务就是
  // 按这个界活的：只要引擎还在跑，服务就一直在前台（android-client@147bb4f VPNService）。
  // vpnActiveSeen 不能当这个事实用：它见过就永不回落，用户断开后也还是 true。
  private vpnIntentOn: boolean = false;
  // 0.0.21（挂起长窗口后的恢复复查）：上一拍的时刻（0 = 本进程还没拍过）。平台可以把整个
  // UI 进程挂起很久（0.0.20 真机上这次是 7362 s：2 s 轮询与 30 s 实况窗整段停摆，VPN 被
  // 相机清理后 1 h16 m 无人重连），因此"两拍相隔异常远"本身就是"进程刚回来"的证据。
  private lastPollAtMs: number = 0;
  // 单飞：同一时刻只允许一次轮询/复查在跑（0.0.21 起复查可以从平台回调里插进来）。已经在
  // 跑的那一拍本身就是它要的复查，直接跳过，绝不叠加第二次读。
  private pollRunning: boolean = false;
  // 平台回调 / onForeground 送来的恢复信号先落在这里，由紧接着的那一刻的那一拍记录成
  // `ui_resume_check`（空串 = 这一拍不是被恢复信号叫醒的）。
  private pendingResumeReason: string = '';
  // ONE `ui_auto_reconnect` record per SKIP REASON per process: the 2 s tick
  // would otherwise repeat the same "not this time" line forever.
  private reconnectReported: Set<string> = new Set<string>();
  // at_ms of the connect-failure notice that matches the current attempt (read
  // together with the rendered class in refreshConnectFailure). 0 = none. The
  // auto-reconnect never starts over a verdict NEWER than the extension marker.
  private connectFailureAtMs: number = 0;
  // 2026-10-10 后台保活（实验，方案 A）：ONE policy state per UI process, plus the
  // switch value the page renders. The state machine itself is the pure
  // NetBirdKeepAlivePolicy; the platform adapter owns "a task exists right now"
  // and this page mirrors that fact into the state before every decision.
  @State bgKeepAliveEnabled: boolean = KEEP_ALIVE_DEFAULT_ENABLED;
  private bgKeepAliveState: KeepAlivePolicyState = keepAliveInitialState();
  // ---- 诊断日志导出（设置 → 诊断日志）--------------------------------------
  // ONE user action at a time: the busy latch disables the button AND is
  // re-checked at the handler, so a double tap cannot start two exports.
  // tone/headline/body are the honest result text — a cap cut, a skipped file
  // or a source read error renders 部分导出, never 导出成功. The picked URI is
  // shown to the user only: it is NEVER hilog-ed (user data).
  @State logExportBusy: boolean = false;
  @State logExportTone: string = '';
  @State logExportHeadline: string = '';
  @State logExportBody: string = '';
  @State logExportUri: string = '';
  // Accepted (not latch-rejected) user connect/retry/disconnect actions of THIS
  // session: the only number the page ever puts into a diagnostic record.
  private diagActionCount: number = 0;
  // ---- 路由选择（设置 → 路由选择）---------------------------------------------
  // 2026-10-07 即时生效增量: there is no draft and no 保存/放弃 button any more.
  // Every switch action writes the scoped preference ATOMICALLY and then rings
  // the VPE's doorbell (an atomic request document), online and offline alike;
  // the live VPE loads the CURRENT preference itself and applies it in place.
  // The page never stops/starts/re-logs-in a connection for a route change.
  @State routeSelectionOpen: boolean = false;
  @State routeDraft: RouteSelection = routeSelectionDefault();
  @State routeLoadError: boolean = false;
  @State routeLoadErrorText: string = '';
  @State routeCatalog: RouteCatalog | null = null;
  @State routeCatalogStatus: string = '';
  @State routeCatalogUrl: string = '';
  @State routeCatalogUpdatedAtMs: number = 0;
  // 最近一次请求的 id：只有结果文档里的 id 与它相同，页面才把那份答案当作「自己
  // 这次请求」的状态；上一代请求的答案一律忽略，目录里的 enabled 更不能冒充
  // 「已生效」。
  @State routeRequestId: string = '';
  @State routeResultId: string = '';
  @State routeResultState: string = '';
  @State routeResultApplied: boolean = false;
  @State routeResultError: string = '';
  @State routeSaving: boolean = false;
  @State routeSaveFailed: boolean = false;
  @State routeNotifyFailed: boolean = false;
  @State routeActionError: string = '';
  // 用户是否已经动过开关：任何更早发出的读取答案（含上一次进入时发出、这次才落地的
  // 读取）都不得覆盖他的期望。重新进入时只有在没有在途写入、也没有在途读取——磁盘
  // 一定不旧——才会把它归零。
  private routeUserEdited: boolean = false;
  private routeCatalogReading: boolean = false;
  private routeResultReading: boolean = false;
  private routeSelectionLoading: boolean = false;
  // 串行写入（latest-wins）：同一时刻只有一次在途写入 + 一个「最新期望」槽位。
  // 快速连点只会替换槽位，绝不会让两次写入同时用同一个 .tmp，也不会让旧写入
  // 覆盖当前意图。
  private routeWriteBusy: boolean = false;
  private routeWritePending: RouteSelection | null = null;
  private routeRequestSeq: number = 0;

  aboutToAppear(): void {
    this.refreshSetupState();
    // First open without credentials: guide straight to configuration
    // (设置 tab owns the first-run form since phase 1).
    if (!this.configured) {
      this.activeTab = 2;
    }
    this.readVersionInfo();
    this.loadKeepAliveSwitch();
    // 0.0.21（挂起长窗口后的恢复复查）：本页是唯一读状态文档的人，所以"进程又跑起来了"
    // 的信号只有一个落点——这里。两条来源（平台说长时任务重新活跃 / UIAbility
    // onForeground）都经适配层转发过来；注册在第一次轮询之前，免得冷启动时漏掉一次。
    setKeepAliveResumeListener((reason: string): void => {
      this.onResumeSignal(reason);
    });
    // 2026-10-13 PC 托盘（0.0.29）: the tray menu's action rows land HERE, and only
    // here — the page owns the connect/stop path, so the tray's 连接/断开/退出
    // 并断开 become exactly the calls the page's own controls make. Registered
    // before the first poll; on a phone the registration is a harmless no-op
    // (there is no tray to deliver anything).
    setTrayActionListener((menuCode: string): void => {
      this.onTrayAction(menuCode);
    });
    // 2026-10-13 统一退出（0.0.31）: the quit flow's disconnect step IS this page's
    // requestStop() — the ONE vpnExtension-stop call site stays here, and the flow
    // never talks to the extension itself. Registered next to the tray listener
    // (same lifetime discipline), so the system-generated tray 退出, the tray menu
    // and 设置 → 退出应用 all end the VPN the same way.
    setQuitDisconnect((): void => {
      this.requestStop();
    });
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
    // 本页没了，恢复信号就没有落点：摘掉监听者（适配层从此不再持有本页）。
    setKeepAliveResumeListener(undefined);
    // 2026-10-13 PC 托盘（0.0.29）: same rule for the tray's action listener — the
    // icon may outlive this page (it belongs to the process), but the
    // connect/stop/quit actions must not point at a page that is gone.
    setTrayActionListener(undefined);
    // 2026-10-13 统一退出（0.0.31）: and the same for the quit flow's disconnect
    // step (the flow reports a missing one honestly instead of inventing a second
    // extension-stop call site).
    setQuitDisconnect(undefined);
    if (!this.accountLoginCommitting) {
      this.cancelAccountLogin(false);
    }
  }

  // 2026-10-08 扩展被系统回收后的自动重连: the page IS the foreground signal —
  // onPageShow fires when the page becomes visible again (including the return
  // from the background) and onPageHide when it stops being visible, so the
  // auto-reconnect can never be fired by a tick that ran in the background.
  // Nothing else is triggered here: the 2 s poll below keeps doing its work.
  onPageShow(): void {
    this.pageVisible = true;
  }

  onPageHide(): void {
    this.pageVisible = false;
  }

  private readVersionInfo(): void {
    try {
      const info: bundleManager.BundleInfo = bundleManager.getBundleInfoForSelfSync(
        bundleManager.BundleFlag.GET_BUNDLE_INFO_DEFAULT);
      this.versionName = info.versionName !== undefined && info.versionName !== null
        ? info.versionName : '';
      this.versionCode = info.versionCode;
    } catch (error) {
      this.versionName = '';
      this.versionCode = -1;
    }
  }

  // 0.0.21（挂起长窗口后的恢复复查）：这是唯一入口，恢复信号和 2 s 定时器都走它。单飞保护：
  // 一次只允许一拍照常跑——正在跑的那一拍就是恢复信号要的那次复查，所以直接跳过，不排队、
  // 不叠加、不新增定时器。
  private pollConnectorStatus(): void {
    if (this.pollRunning) {
      return;
    }
    this.pollRunning = true;
    try {
      this.pollConnectorStatusPass();
      // 2026-10-13 PC 托盘（0.0.29）: the SAME tick pushes the home card's official
      // state label (stateText()) to the status-bar icon. No second timer, no
      // second status read: the adapter dedupes by surface and coalesces to one
      // 500 ms window, and on a phone it returns immediately.
      updateTrayState(this.stateText());
    } finally {
      // finally: 一拍里任何一处抛错都不能把单飞锁留在 true 上（那会让页面从此不再轮询）。
      this.pollRunning = false;
    }
  }

  // 0.0.21：恢复信号（平台说长时任务重新活跃 / UIAbility onForeground）只有一个作用——
  // 立刻把状态文档重读一次，而不是等下一拍（真机上被挂起的那一段可以长达 7362 s）。
  // 冷启动第一拍之前不处理：那时还没有"上一拍"可言，紧接着的首次轮询本身就是最新读数。
  private onResumeSignal(reason: string): void {
    if (this.pollRunning) {
      return;
    }
    if (this.lastPollAtMs <= 0) {
      return;
    }
    if (this.pendingResumeReason.length === 0) {
      this.pendingResumeReason = reason;
    }
    this.pollConnectorStatus();
  }

  // 0.0.21：一条 `ui_resume_check`——`reason` 是发现这次恢复的信号（闭集：active / gap /
  // foreground），`count` 是两次轮询之间的秒数。两个键都在既有公开键集内，事件名是小写
  // token（不是内部事件名），没有路径、没有文档内容、没有自由文本。
  private diagResumeCheck(reason: string, gapMs: number): void {
    const fields: DiagFields = {};
    fields['reason'] = reason;
    fields['count'] = Math.round(gapMs / 1000);
    event('ui_resume_check', fields);
  }

  private pollConnectorStatusPass(): void {
    // 0.0.21（挂起长窗口后的恢复复查）：这一拍距上一拍多久？间隔异常大 = 这个进程有一段时间
    // 根本没在跑（平台挂起了整个 UI 进程），于是这一拍就是"恢复后的第一次复查"。记录一条
    // `ui_resume_check`（`reason` 是发现它的信号，`count` 是间隔秒数），随后照常走完这一拍
    // ——状态文档会在这同一拍里被重读，重连分支也随之重新判定。一次恢复只记一条：读完即把
    // 本拍时刻写回，下一拍（2 s 后）的间隔就恢复正常，不会重复触发。
    //
    // 恢复后的 10 s 内本来就要多复查几次（APP_KILLED 可能在恢复瞬间才投递，第一次复查时文档
    // 已过期却还没有终止事件，按既有策略会 skip 成 vpn-not-confirmed-dead——这是安全行为）。
    // 这里不为它新增定时器：页面自己的 2 s 轮询在 10 s 内就是 5 拍，每拍都重读文档、每拍都会
    // 重新判一次重连（节流与上限仍在纯策略里），所以"多做几次复查"就是既有节拍本身。
    const nowMs: number = Date.now();
    const gapMs: number = keepAlivePollGapMs(this.lastPollAtMs, nowMs);
    this.lastPollAtMs = nowMs;
    const resumeReason: string = this.pendingResumeReason;
    this.pendingResumeReason = '';
    if (keepAliveResumedAfterGap(gapMs)) {
      this.diagResumeCheck(resumeReason.length > 0 ? resumeReason : KEEP_ALIVE_RESUME_GAP, gapMs);
    }
    // 路由选择: the route detail page rides THIS existing 2 s tick — there is
    // no second always-on timer. Both reads are offline (plain public documents,
    // never a native connector getter): the catalog never touches the desired
    // selection, so a refresh can never discard an edit in progress, and the
    // result read only ever reports the answer whose id is the newest request
    // this page wrote.
    if (this.routeSelectionOpen) {
      this.refreshRouteCatalog();
      this.refreshRouteApplyResult();
    }
    // 2026-10-08 扩展被系统回收后的自动重连: latch the page's own start/stop
    // flags BEFORE the branch below releases them — a start or stop the user
    // has in flight must veto the automatic reconnect on this very tick.
    const requestInFlight: boolean = this.requestSent || this.stopSent;
    // The connector runs in the VpnExtension process: read the document that
    // process publishes into the shared filesDir (a NAPI call here would read
    // an empty slot and show a dead "disconnected" state). A document older
    // than two publish ticks counts as NOT running (B.3) — the page then
    // shows 未连接 instead of passing off a stale snapshot as live state.
    const status: PublishedStatus | null = currentPublishedStatus(
      readPublishedStatus(getContext(this).filesDir), Date.now());
    // 0.0.20 判据②放宽: a kill event naming the pid that wrote the open marker
    // proves the extension is gone, so there is nothing left to wait for — the
    // document simply has not aged out of its freshness window yet (~10 s). When
    // that proof exists the document is treated as ABSENT for every consumer
    // below (the tunnel really is down, the keep-alive sees no live tunnel, the
    // reconnect branch is entered). The decision itself is untouched: criterion
    // ⑦ asks for exactly this proof, so asking earlier can never mean "start a
    // VPN on a possibly live extension".
    const extensionDead: boolean = this.extensionConfirmedDead();
    const live: PublishedStatus | null = extensionDead ? null : status;
    // 2026-10-12 upstream tap handling: the pending action's own bookkeeping on
    // THIS existing tick — no second timer, no second status read. It releases
    // the tap latch when the engine reports the requested state (or the connect
    // demonstrably failed) and 7 s after the tap at the latest, and it supersedes
    // the previous failure notice once the engine's own report proves the new
    // attempt started.
    this.updatePendingAction(live);
    // 2026-10-10 后台保活（实验，方案 A）: the borrowed continuous task rides THIS
    // tick — no second timer, no second document read. It runs on both branches
    // (a live document is the ONLY thing that may hold the task, so "no fresh
    // document" must reach the policy as the fact it is).
    this.syncKeepAlive(live);
    if (live === null) {
      if (this.observedPublishedStatus) {
        // the document disappeared = the extension is gone (stopped or
        // destroyed): release the request latches for an explicit retry
        // roam-1005-0014 A.3: if the last live state was ACTIVE and neither the
        // user nor the page's own restart leg just stopped the extension, the
        // tunnel ended without the user asking — surface the reconnect notice
        // (the extension's fail-closed path removed the document after its
        // VPN_RECREATE_VERIFY failure). 0.0.28: the restart-leg latch is checked
        // on its own; it is a different fact from a user stop and must never be
        // read as one (see lastRestartLegAtMs).
        if (vpnActiveState(this.lastLiveState) && !this.stopSent &&
          Date.now() - this.lastUserStopAtMs > 8000 &&
          Date.now() - this.lastRestartLegAtMs > 8000 &&
          Date.now() - this.lastLogoutAtMs > 25000) {
          this.disconnectNotice = 'VPN 已断开，请重新连接';
        }
        this.requestSent = false;
        this.stopSent = false;
        // 2026-10-12 upstream tap handling: the engine's end IS an observable
        // outcome (upstream onEngineStopped → onEngineState(DISCONNECTED) settles the
        // tap latch), so the latch is released here rather than at the 7 s lapse.
        this.clearPendingAction();
        this.requestId = `ui-${Date.now()}`;
        this.observedPublishedStatus = false;
        this.lastLiveState = '';
      }
      this.published = null;
      // 2026-10-06 登录失败可见性: no live document AND a matching failure
      // notice = the attempt ended in a recorded failure (login fatal /
      // retry exhausted / startup abort) — the card shows the official
      // Disconnected label (需要登录 for the login class) and the reason goes to
      // the error line + its one Toast.
      this.refreshConnectFailure();
      // 2026-10-08 扩展被系统回收后的自动重连: this branch is criterion "the status
      // document is not fresh" — NOT "the VPN really is not there": the system
      // suspends the background extension for up to ~94 s with no status write and
      // no log line, and the document expires to null after ~10 s of that. The
      // decision, including the "the process really is gone" proof (criterion ⑦),
      // is the pure policy's.
      this.runAutoReconnect(requestInFlight);
      return;
    }
    this.observedPublishedStatus = true;
    this.lastLiveState = live.state;
    if (vpnActiveState(live.state)) {
      this.requestSent = true;
      // 0.0.28: a tunnel is RUNNING in this process — the user's VPN-on intent
      // holds from here on, across every transient loss, until one of the
      // ending facts below fires (see vpnIntentOn).
      this.vpnIntentOn = true;
      // a live session means any recorded failure belongs to a PREVIOUS
      // attempt — never render it over a running connection
      this.connectFailureClass = '';
      this.connectFailureAtMs = 0;
      // …including the page's own request-level error line: a running tunnel
      // (the attempt DID start after all — the pending-start window is not
      // proof of anything) supersedes it, exactly like the verdict above.
      this.actionError = '';
      // 2026-10-08 扩展被系统回收后的自动重连 / 0.0.20: the tunnel is up again —
      // the outstanding attempt has succeeded (its verification window closes
      // now), and the attempt counter is forgiven ONLY once this active run has
      // lasted RECONNECT_ACTIVE_RESET_MS: with the keep-alive task the UI process
      // outlives many reclaims, so a tunnel that dies again a few seconds later
      // must stay on the same chain of failures.
      this.reconnectState = reconnectOnActive(this.reconnectState, Date.now());
      if (live.request_id !== undefined && live.request_id.length > 0) {
        this.requestId = live.request_id;
      }
    } else {
      // A live document in a dead state (failed/disconnected/closed/stopped):
      // the tunnel is not coming back on its own — re-arm the switch.
      this.requestSent = false;
      // 0.0.28: the ENGINE itself reported that it is not running any more
      // (needs-login / a terminal death leaves exactly this document before its
      // process goes). That is a terminal stop, not a transient loss: the
      // keep-alive must not outlive the business.
      this.vpnIntentOn = false;
      this.stopSent = false;
      this.requestId = `ui-${Date.now()}`;
    }
    this.published = live;
  }

  // 0.0.20 判据②放宽: is the extension process proven gone while the status
  // document may still look live? The proof is the UI process's own APP_KILLED for
  // this app's `:vpn` process, and it only counts when it names the pid that wrote
  // the currently open marker — the same comparison criterion ⑦ makes inside the
  // pure policy (a marker rewritten by a newer process has a different pid, so a
  // fresh tunnel is never mistaken for the dead one). The marker is read ONLY when
  // such a kill record exists in this process: no kill record, no read, and the
  // live branch keeps its old cost. Any failure to read it means "cannot confirm",
  // which can only make this page more careful, never less.
  private extensionConfirmedDead(): boolean {
    const killed: ProcessExitRecord | null = lastKilledVpnProcess();
    if (killed === null) {
      return false;
    }
    try {
      const markerPath: string = recordPath(getContext(this).filesDir, MARKER_BASENAME_VPN);
      if (markerPath.length === 0 || !fs.accessSync(markerPath)) {
        return false;
      }
      const marker: Marker | null = parseMarker(fs.readTextSync(markerPath));
      return marker !== null && marker.state === RECONNECT_MARKER_OPEN &&
        marker.pid > 0 && marker.pid === killed.pid;
    } catch (error) {
      return false;
    }
  }

  // 2026-10-06 登录失败可见性: read the extension's persistent
  // connect-failure notice. Honored only when it belongs to the
  // current/last start attempt (at_ms not older than the request we sent,
  // 2 s clock margin) and is not stale (readConnectFailure age-bounds it).
  // A matching notice also releases the start latch — the attempt is over,
  // the user may retry or go fix the settings.
  private refreshConnectFailure(): void {
    const notice: ConnectFailureNotice | null =
      readConnectFailure(getContext(this).filesDir);
    if (notice !== null && notice.at_ms >= this.lastStartAtMs - 2000) {
      this.connectFailureClass = notice.error_class;
      // 2026-10-08 扩展被系统回收后的自动重连: the SAME read answers the policy's
      // "is there a verdict NEWER than the extension marker?" gate — one file
      // read, never a second one per tick.
      this.connectFailureAtMs = notice.at_ms;
      this.requestSent = false;
      // 0.0.28: a notice that belongs to THIS attempt is the extension's own
      // verdict that the VPN never reached a live tunnel (missing credentials,
      // a refused start, the network-map box expiring, a terminal core death —
      // needs-login among them). There is no business to keep a task for, and
      // the automatic reconnect refuses to start over it too
      // (NetBirdReconnectPolicy gate (5)), so the user's VPN-on intent ends
      // here as well: "the extension is gone and no reconnect is intended".
      this.vpnIntentOn = false;
      this.notifyFailureOnce(notice);
    } else {
      if (this.connectFailureClass.length > 0) {
        this.connectFailureClass = '';
      }
      this.connectFailureAtMs = 0;
    }
  }

  // 官方 Android 客户端把引擎自己的消息当成一次 Toast 显示
  // (MainActivity.java:1030-1036: `Toast.makeText(…, msg, LENGTH_LONG)`)：它是
  // TRANSIENT 的，持久通道是状态卡的官方状态（未连接 / 需要登录）加下面那行红字。
  // 这里用最接近的 HarmonyOS 等价物 promptAction.showToast（本页 UIContext）；被拒
  // 只 fail-soft —— 同样的文案已经在那行红字里，绝不因为一个提示失败而丢信息。
  // ONE toast per recorded verdict (at_ms is the notice's own identity).
  private notifyFailureOnce(notice: ConnectFailureNotice): void {
    if (notice.at_ms === this.failureToastedAtMs) {
      return;
    }
    this.failureToastedAtMs = notice.at_ms;
    try {
      this.getUIContext().getPromptAction().showToast({
        message: connectFailureText(notice.error_class), duration: FAILURE_TOAST_MS
      });
      hilog.info(DOMAIN, TAG, 'UI_FAILURE_TOAST|bundle=%{public}s|errorClass=%{public}s',
        BUNDLE_NAME, notice.error_class);
    } catch (error) {
      hilog.warn(DOMAIN, TAG, 'UI_FAILURE_TOAST_REJECTED|bundle=%{public}s|code=%{public}s',
        BUNDLE_NAME, String((error as BusinessFailure).code ?? 'none'));
    }
  }

  // ---------------------------------------------------------------------------
  // 2026-10-08 扩展被系统回收后的自动重连（增量）
  //
  // The system reclaims the VPN extension (memory pressure, an OOM kill) while
  // the app is in the background; the tunnel dies with it and the extension
  // never reaches its stop callback. Back in the foreground the page sees the
  // status document expire to null and, up to now, only told the user to
  // reconnect by hand. This is the ONE automatic retry — and it is made only once
  // the process that wrote the marker is CONFIRMED dead, because a suspended
  // extension looks identical from the status document alone.
  //
  // Called from exactly ONE place: the poll branch where the freshly read
  // document is null (expired or absent) — that is the "no fresh document" fact,
  // NOT "the process is gone": a suspended background extension writes nothing
  // for minutes while it is still alive. The gates (including criterion ⑦, the
  // UI process's own APP_KILLED for the pid that wrote the marker) and the
  // anti-loop live in the pure NetBirdReconnectPolicy module; this method only
  // reads the facts, acts, and records ONE safe event.
  // ---------------------------------------------------------------------------

  private runAutoReconnect(requestInFlight: boolean): void {
    const markerPath: string = recordPath(getContext(this).filesDir, MARKER_BASENAME_VPN);
    let markerState: string = '';
    let markerAtMs: number = 0;
    let markerPid: number = 0;
    try {
      if (markerPath.length > 0 && fs.accessSync(markerPath)) {
        const marker: Marker | null = parseMarker(fs.readTextSync(markerPath));
        if (marker !== null) {
          markerState = marker.state;
          // The pid that wrote the marker: the process criterion ⑦ must find in
          // the UI process's kill record. parseMarker() only returns a marker
          // whose pid it could read, and an unusable value stays 0 = unreadable,
          // which can only make the policy MORE careful, never less.
          markerPid = marker.pid;
          // The marker's own ISO stamp, for the "no verdict NEWER than the
          // marker" gate. An unparseable stamp stays 0 = unknown, which can only
          // make the policy MORE careful, never less.
          const parsedTs: number = Date.parse(marker.ts);
          markerAtMs = Number.isNaN(parsedTs) ? 0 : parsedTs;
        }
      }
    } catch (error) {
      // Unreadable marker = "we cannot tell": the policy treats it as a clean
      // end. A reconnect is never guessed out of a file we did not understand.
      markerState = '';
      markerAtMs = 0;
      markerPid = 0;
    }
    // Criterion ⑦: the platform's own verdict on this app's `:vpn` process, as
    // this UI process received it (APP_KILLED is delivered on the next launch).
    // null = no such event in this process = not confirmed dead.
    const killedVpn: ProcessExitRecord | null = lastKilledVpnProcess();
    const facts: ReconnectInput = {
      nowMs: Date.now(),
      foreground: this.pageVisible,
      // 0.0.20: a held keep-alive task is the fact that lets a BACKGROUND tick ask
      // at all (the pure policy opens gate (2) for it and records
      // trigger=background). The adapter owns "a task exists right now"; the page
      // mirrors it into this policy state every tick, so the value read here is
      // the same fact the keep-alive decision just used.
      keepAliveActive: this.bgKeepAliveState.taskPresent,
      markerState: markerState,
      markerAtMs: markerAtMs,
      markerPid: markerPid,
      startOrStopInFlight: requestInFlight,
      lastUserStopAtMs: this.lastUserStopAtMs,
      lastLogoutAtMs: this.lastLogoutAtMs,
      configured: this.configured,
      connectFailureAtMs: this.connectFailureAtMs,
      killedVpnPid: killedVpn === null ? null : killedVpn.pid
    };
    const decision: ReconnectDecision = decideReconnect(this.reconnectState, facts);
    this.reconnectState = decision.state;
    if (decision.action === RECONNECT_ACTION_RECONNECT) {
      // The decision is recorded BEFORE the action: the log then says why the
      // page started a VPN, not that it merely tried. The trigger is the
      // decision's own (foreground / background), never re-derived here.
      this.diagAutoReconnect(decision.trigger, RECONNECT_RESULT_RECONNECTED,
        decision.reason, decision.attempt);
      hilog.warn(DOMAIN, TAG,
        'UI_AUTO_RECONNECT|trigger=%{public}s|reason=%{public}s|attempt=%{public}d' +
        '|note=extension-reclaimed',
        decision.trigger, decision.reason, decision.attempt);
      // The SAME single start path the button uses: requestStart() owns the one
      // extension-start call site in this file and its own diagnostic action
      // record, so the automatic retry adds no second way to start the VPN. The
      // flag is what makes a REFUSED start a named auto-reconnect failure (the
      // platform's error code goes into the diagnostic and the attempt stops
      // waiting for its 30 s verification window).
      this.requestStart(true);
      // requestStart() clears the notice (a fresh attempt starts from a clean
      // slate); this is the ONE case where the user must SEE that the retry is
      // already under way instead of being asked for it. 0.0.20: in the background
      // nobody can see it, and the spec of this background path is "diagnostics
      // only" — no notice is written into a page that is not on screen.
      if (this.pageVisible) {
        this.disconnectNotice = '正在自动重新连接…';
      }
      return;
    }
    if (decision.action === RECONNECT_ACTION_VERIFY_FAILED) {
      this.diagAutoReconnectOnce(decision.trigger, RECONNECT_RESULT_FAILED,
        decision.reason, decision.attempt);
      hilog.warn(DOMAIN, TAG,
        'UI_AUTO_RECONNECT_FAILED|trigger=%{public}s|reason=%{public}s|attempt=%{public}d' +
        '|note=no-active-document',
        decision.trigger, decision.reason, decision.attempt);
      return;
    }
    if (decision.action === RECONNECT_ACTION_LIMIT) {
      this.diagAutoReconnectOnce(decision.trigger, RECONNECT_RESULT_LIMIT,
        decision.reason, decision.attempt);
      return;
    }
    this.diagAutoReconnectOnce(decision.trigger, RECONNECT_RESULT_SKIPPED,
      decision.reason, decision.attempt);
  }

  // The ONE diagnostic-record exit of the auto-reconnect: a closed-set trigger,
  // result and reason token plus the attempt count. No marker text, no path, no
  // URL, no credential, no free-form message. A `reason` that is not one of the
  // policy's tokens carries the platform's own numeric error code of a refused
  // start (or 'none' when it gave none) — a number is not free text.
  private diagAutoReconnect(trigger: string, result: string, reason: string,
    count: number): void {
    const fields: DiagFields = {};
    fields['trigger'] = trigger;
    fields['result'] = result;
    fields['reason'] = reason;
    fields['count'] = count;
    event('ui_auto_reconnect', fields);
  }

  // A SKIP is a standing condition, not an event: on the 2 s tick it would be
  // repeated forever, so each reason writes at most one record per process.
  private diagAutoReconnectOnce(trigger: string, result: string, reason: string,
    count: number): void {
    if (this.reconnectReported.has(reason)) {
      return;
    }
    this.reconnectReported.add(reason);
    this.diagAutoReconnect(trigger, result, reason, count);
  }

  // ---------------------------------------------------------------------------
  // 2026-10-10 后台保活（0.0.19 实验版，方案 A）
  //
  // The borrowed DATA_TRANSFER continuous task rides the EXISTING 2 s tick: no
  // second timer, no second status read. Every tick the page (a) folds the
  // platform's cancellation events into the ONE pure policy state, (b) mirrors
  // the adapter's "a task exists right now" fact, (c) asks the policy what to do
  // and (d) carries that out through the adapter — whose every call is fail-soft
  // and can never break the VPN's own start/stop path.
  //
  // The switch's value, this state and the page's own latches are the only page
  // inputs; everything else is a fact about a document the tick already read.
  // `syncKeepAlive` is called from exactly ONE place: pollConnectorStatus.
  // ---------------------------------------------------------------------------

  private syncKeepAlive(status: PublishedStatus | null): void {
    const nowMs: number = Date.now();
    // The platform's verdicts first: a user dismissal, an illegal-type verdict
    // and a backoff all change what the policy is allowed to do next.
    const cancels: KeepAliveCancelEvent[] = takeKeepAliveCancels();
    for (let i = 0; i < cancels.length; i++) {
      this.bgKeepAliveState = keepAliveOnCancelled(this.bgKeepAliveState,
        cancels[i].reason, cancels[i].detailedReason, nowMs);
    }
    this.bgKeepAliveState = keepAliveWithTask(this.bgKeepAliveState, keepAliveTaskPresent());
    const vpnActive: boolean = status !== null && vpnActiveState(status.state);
    // 0.0.20: the keep-alive grace is measured from the moment the tunnel was last
    // seen LEAVING the active states, not from whichever tick first noticed — the
    // page's own cadence must not stretch the window. The stamp only starts once
    // the tunnel was actually seen active in this process: an app that opens on a
    // dead tunnel has no "reconnect in progress" to protect, and its task (if a
    // previous session left one) still goes at once.
    if (vpnActive) {
      this.vpnActiveSeen = true;
      this.vpnInactiveSinceMs = 0;
    } else if (this.vpnActiveSeen && this.vpnInactiveSinceMs === 0) {
      this.vpnInactiveSinceMs = nowMs;
    }
    const facts: KeepAliveFacts = {
      nowMs: nowMs,
      enabled: this.bgKeepAliveEnabled,
      vpnActive: vpnActive,
      // 0.0.28: the user still wants this VPN — a tunnel ran in this process and
      // none of the ending facts has fired since (see vpnIntentOn). This is what
      // keeps the borrowed task alive across a transient loss, however long the
      // connector takes; the pure policy owns the decision.
      intentOn: this.vpnIntentOn,
      // The page's own user-stop guard (8 s), the same one the disconnect notice
      // uses: a stop the user just asked for must never be answered with a task.
      // 0.0.28: the derivation is the pure policy's, so "which latch counts as a
      // user stop" is host-checkable — the page's own restart leg is not one.
      userStopRecent: keepAliveUserStopRecent(this.stopSent, this.lastUserStopAtMs, nowMs),
      // An explicit logout in flight / just resolved (25 s), or no credentials at
      // all — there is nothing to keep alive for.
      loggedOut: this.lastLogoutAtMs > 0 && nowMs - this.lastLogoutAtMs <= 25000 ||
        !this.configured,
      // The page's VPN start is out and no live tunnel exists yet: the task is
      // asked for only once there is a tunnel to keep alive.
      startInFlight: this.requestSent && !vpnActive,
      // 0.0.20: how long ago the tunnel went inactive (0 = never seen active), and
      // whether the automatic reconnect has already spent its attempt budget —
      // both are facts of OTHER state machines, handed over as-is.
      vpnInactiveSinceMs: this.vpnInactiveSinceMs,
      reconnectGaveUp: this.reconnectState.attempts >= RECONNECT_MAX_ATTEMPTS
    };
    const decision: KeepAliveDecision = decideKeepAlive(this.bgKeepAliveState, facts);
    this.bgKeepAliveState = decision.state;
    const revision: number = this.bgKeepAliveState.revision;
    if (decision.action === KEEP_ALIVE_ACTION_START) {
      // vpnActive is true here, so the document exists: the live view's first
      // body comes from THAT document, never from a second read.
      if (status === null) {
        return;
      }
      const host: common.Context | undefined = this.getUIContext().getHostContext();
      if (host === undefined || host === null) {
        return;
      }
      startKeepAlive(host as common.UIAbilityContext, revision, decision.reason,
        keepAliveStatusOf(status)).then((ok: boolean): void => {
        this.bgKeepAliveState = keepAliveOnStartResolved(this.bgKeepAliveState, revision, ok,
          Date.now());
      }).catch((error: Error): void => {
        this.logKeepAliveFailure(error);
        this.bgKeepAliveState = keepAliveOnStartResolved(this.bgKeepAliveState, revision, false,
          Date.now());
      });
      return;
    }
    if (decision.action === KEEP_ALIVE_ACTION_STOP) {
      // A no-op for the adapter when it holds nothing (the 2 s tick asks again and
      // again while the tunnel is down) — no record, no churn.
      this.stopKeepAliveNow(decision.reason);
      return;
    }
    // Held and 30 s have passed: republish the live view (the system cancels a
    // DATA_TRANSFER task whose progress is not updated). The throttle lives in
    // the pure policy; the adapter never decides on its own. 0.0.20: inside the
    // reconnect grace there may be NO document to report (the tunnel is down), so
    // the body is the policy's "正在重新连接…" instead of the frozen last traffic
    // snapshot — a live view that never changes for minutes would also invite
    // SYSTEM_CANCEL_DATA_TRANSFER_NOT_UPDATE.
    if (shouldRefresh(this.bgKeepAliveState, nowMs)) {
      const host: common.Context | undefined = this.getUIContext().getHostContext();
      if (host === undefined || host === null) {
        return;
      }
      if (status !== null && vpnActiveState(status.state)) {
        refreshKeepAlive(host as common.UIAbilityContext, keepAliveStatusOf(status), revision,
          KEEP_ALIVE_REASON_VPN_ACTIVE).then((ok: boolean): void => {
          if (ok) {
            this.bgKeepAliveState = keepAliveOnRefreshed(this.bgKeepAliveState, Date.now());
          }
        }).catch((error: Error): void => {
          this.logKeepAliveFailure(error);
        });
      } else if (decision.reason === KEEP_ALIVE_REASON_GRACE) {
        refreshKeepAliveText(host as common.UIAbilityContext,
          KEEP_ALIVE_NOTIFICATION_RECONNECTING, revision,
          KEEP_ALIVE_REASON_GRACE).then((ok: boolean): void => {
          if (ok) {
            this.bgKeepAliveState = keepAliveOnRefreshed(this.bgKeepAliveState, Date.now());
          }
        }).catch((error: Error): void => {
          this.logKeepAliveFailure(error);
        });
      }
    }
  }

  // The ONE stop exit of the page: the adapter is silent when there is nothing to
  // release, so this is cheap enough to be asked on every 2 s tick.
  private stopKeepAliveNow(reason: string): void {
    const host: common.Context | undefined = this.getUIContext().getHostContext();
    if (host === undefined || host === null) {
      return;
    }
    stopKeepAlive(host as common.UIAbilityContext, this.bgKeepAliveState.revision, reason)
      .catch((error: Error): void => {
        this.logKeepAliveFailure(error);
      });
  }

  // The adapter is fail-soft by construction; this line exists so that a promise
  // it should never reject cannot become an unhandled rejection.
  private logKeepAliveFailure(error: Error): void {
    hilog.warn(DOMAIN, TAG, 'UI_KEEP_ALIVE_ADAPTER_FAILED|code=%{public}s',
      String((error as BusinessFailure).code ?? 'none'));
  }

  // The switch's stored value: read ONCE when the page opens. A missing or
  // corrupt document is the default (true), never an error.
  private loadKeepAliveSwitch(): void {
    loadKeepAliveEnabled(getContext(this).filesDir).then((enabled: boolean): void => {
      this.bgKeepAliveEnabled = enabled;
    }).catch((_error: Error): void => {
      this.bgKeepAliveEnabled = KEEP_ALIVE_DEFAULT_ENABLED;
    });
  }

  // The switch writes through IMMEDIATELY (like every other switch on this page)
  // and then asks the policy again on the spot: off means the task goes now, not
  // in two seconds. Turning it back on is an explicit user request, so it lifts a
  // previous dismissal exactly as a manual connect does.
  private onKeepAliveToggle(isOn: boolean): void {
    this.bgKeepAliveEnabled = isOn;
    if (isOn) {
      this.bgKeepAliveState = keepAliveOnUserConnect(this.bgKeepAliveState);
    } else {
      // 0.0.28: the switch is one of the user's own instruments — turning the
      // background keep-alive off ends the VPN-on intent too, so a task that is
      // somehow still held cannot be kept by it. (The `enabled` fact already
      // releases the task on this same call; this latch makes the intent honest.)
      this.vpnIntentOn = false;
    }
    saveKeepAliveEnabled(getContext(this).filesDir, isOn).then((ok: boolean): void => {
      if (!ok) {
        hilog.warn(DOMAIN, TAG, 'UI_KEEP_ALIVE_STORE_FAILED|enabled=%{public}s', String(isOn));
      }
    }).catch((_error: Error): void => {
      hilog.warn(DOMAIN, TAG, 'UI_KEEP_ALIVE_STORE_FAILED|enabled=%{public}s', String(isOn));
    });
    this.syncKeepAlive(this.published);
  }

  // ---- derived view state -------------------------------------------------

  private peersList(): UiPeerEntry[] {
    return this.published !== null && this.published.peers !== undefined &&
      this.published.peers !== null ? this.published.peers : [];
  }

  private visiblePeers(): UiPeerEntry[] {
    return sortPeers(filterPeers(this.peersList(), this.peerQuery));
  }

  // 详情页中继地址 (2026-10-06): the effective relay URL of the live (or
  // last live) LOCAL session from the published document — '' when not known.
  // A peer's own attached lane is a DIFFERENT fact (peerRelayText), never this.
  private relayUrlText(): string {
    return this.published !== null && this.published.relay_url !== undefined &&
      this.published.relay_url !== null ? this.published.relay_url : '';
  }

  // ---- 详细状态页 derivations (all evaluated at render time on the polled
  //      document — the same proven-fresh pattern as stateText/homeAddress) --

  private networkView(): DetailNetworkView | null {
    return this.published !== null ? readNetworkView(this.published.network) : null;
  }

  private trafficView(): DetailTrafficView | null {
    return this.published !== null ? readTrafficView(this.published.wg_traffic) : null;
  }

  private relayView(): DetailRelayView | null {
    return this.published !== null ? readRelayView(this.published.relay) : null;
  }

  private managementView(): DetailManagementView | null {
    return this.published !== null ? readManagementView(this.published.management) : null;
  }

  private versionText(): string {
    const name: string = this.versionName.length > 0 ? this.versionName : '未知';
    const code: string = this.versionCode >= 0 ? this.versionCode.toString() : '未知';
    return `${name}（版本号 ${code}）`;
  }

  // Management authority the connector was started against: scheme + host +
  // port from the published document. NO connectivity verdict is derived from
  // its presence — an endpoint is shown exactly as an endpoint.
  private managementText(): string {
    const endpoint: DetailManagementView | null = this.managementView();
    if (endpoint === null) {
      return UNKNOWN_DOC;
    }
    return `${endpoint.scheme}://${endpoint.host}:${endpoint.port}`;
  }

  // The signal dial target as configured (netbird_config.signal). It is NOT
  // the registration verdict — that is the 信号连接 row (signal_registered).
  private signalUriText(): string {
    const network: DetailNetworkView | null = this.networkView();
    if (network === null || network.signal_uri === null || network.signal_uri.length === 0) {
      return UNKNOWN_CORE;
    }
    return network.signal_uri;
  }

  // Session deadline: the core's own token ('unknown' | 'disabled' | 'set')
  // plus, only when it is 'set', the absolute Unix-SECONDS deadline.
  private sessionExpiryText(): string {
    const status: PublishedStatus | null = this.published;
    if (status === null) {
      return UNKNOWN_DOC;
    }
    const token: string =
      typeof status.session_expiry === 'string' ? status.session_expiry : '';
    const atS: number = typeof status.session_expires_at_s === 'number' &&
      status.session_expires_at_s > 0 ? status.session_expires_at_s : 0;
    if (atS > 0) {
      return `已设置 ${formatEpochSeconds(atS)}（${remainingText(atS)}）`;
    }
    if (token === 'set') {
      return '已设置（核心未提供到期时间）';
    }
    if (token === 'disabled') {
      return '未启用（管理端未下发会话期限）';
    }
    if (token === 'unknown') {
      return '未知（尚未取得会话期限）';
    }
    return UNKNOWN_DOC;
  }

  // Age of the published document itself (updated_at_ms is epoch MILLISECONDS
  // — not the epoch-seconds deadline above).
  private statusAgeText(): string {
    const status: PublishedStatus | null = this.published;
    if (status === null || typeof status.updated_at_ms !== 'number') {
      return UNKNOWN_DOC;
    }
    return secondsAgoText(status.updated_at_ms);
  }

  private lastErrorDetailText(): string {
    const status: PublishedStatus | null = this.published;
    if (status === null) {
      return '未知（状态文档不可用）';
    }
    const errorClass: string = status.last_error_class !== undefined &&
      status.last_error_class !== null ? status.last_error_class : '';
    if (errorClass.length === 0 || errorClass === 'none') {
      return '无';
    }
    const hint: string = lastErrorText(errorClass);
    return hint.length > 0 ? `${errorClass}：${hint}` : errorClass;
  }

  // ---- 详细状态页 rows ------------------------------------------------------

  private overviewFields(): DetailField[] {
    const rows: DetailField[] = [];
    const status: PublishedStatus | null = this.published;
    rows.push({ label: '状态', value: this.stateText() });
    if (status === null) {
      rows.push({ label: '核心状态', value: '未知（VPN 未运行或状态文档已过期）' });
    } else {
      rows.push({ label: '核心状态', value: status.state });
    }
    rows.push({ label: '运行状态', value: this.overviewRunningText() });
    rows.push({
      label: '运行时长',
      value: uptimeText(status !== null ? status.started_at_unix : undefined)
    });
    rows.push({
      label: '隧道数据面',
      value: status === null ? UNKNOWN_DOC : (status.wg_ready ? '已就绪' : '未就绪')
    });
    rows.push({
      label: 'WG 会话',
      value: status === null ? UNKNOWN_DOC : `${status.wg_sessions}`
    });
    rows.push({
      label: '对端/路由',
      value: status === null ? UNKNOWN_DOC : `${status.peer_count} / ${status.route_count}`
    });
    rows.push({ label: '管理服务器', value: this.managementText() });
    rows.push({
      label: '管理重连',
      value: status !== null && typeof status.reconnects === 'number'
        ? `${status.reconnects}` : UNKNOWN_DOC
    });
    rows.push({ label: '信号连接', value: this.signalText() });
    rows.push({
      label: '信号重连',
      value: status !== null && typeof status.signal_reconnects === 'number'
        ? `${status.signal_reconnects}` : UNKNOWN_DOC
    });
    rows.push({ label: '信号地址', value: this.signalUriText() });
    rows.push({ label: '会话期限', value: this.sessionExpiryText() });
    rows.push({ label: '最近错误', value: this.lastErrorDetailText() });
    rows.push({ label: '客户端版本', value: this.versionText() });
    rows.push({ label: '状态更新', value: this.statusAgeText() });
    rows.push({
      label: '请求 ID',
      value: status !== null && typeof status.request_id === 'string' &&
        status.request_id.length > 0 ? status.request_id : UNKNOWN_DOC
    });
    return rows;
  }

  private overviewRunningText(): string {
    const status: PublishedStatus | null = this.published;
    if (status === null) {
      return '未运行（无状态文档）';
    }
    const iface: string = status.iface_present === undefined
      ? '未知（接口探测无结果）'
      : (status.iface_present ? 'vpn-tun 存在' : 'vpn-tun 不存在');
    const errorText: string = this.lastErrorLine();
    if (errorText.length > 0) {
      return `vpn-tun：${iface}；${errorText}`;
    }
    return `vpn-tun：${iface}`;
  }

  // 信号连接: the registration verdict (signal_registered) — the dial target
  // lives in 信号地址.
  private signalText(): string {
    const status: PublishedStatus | null = this.published;
    if (status === null) {
      return UNKNOWN_DOC;
    }
    return status.signal_registered ? '已连接（registered）' : '未连接';
  }

  private networkFields(): DetailField[] {
    const rows: DetailField[] = [];
    const status: PublishedStatus | null = this.published;
    if (status === null) {
      rows.push({ label: '网络地图', value: '未知（VPN 未运行）' });
      return rows;
    }
    const network: DetailNetworkView | null = this.networkView();
    if (network === null) {
      // Older document: the network map section is absent, but the legacy
      // address/fqdn fields may still carry the last known own identity.
      rows.push({ label: '网络地图', value: '未知（核心未上报网络配置）' });
      rows.push({ label: '叠加地址', value: this.legacyAddressText() });
      rows.push({ label: '本机域名', value: this.ownFqdnText() });
      return rows;
    }
    rows.push({ label: '网络地图', value: network.available ? '可用' : '不可用' });
    rows.push({ label: '叠加地址', value: this.networkPrefixText(network) });
    rows.push({ label: '本机域名', value: this.ownFqdnText() });
    rows.push({
      label: '接口 DNS',
      value: typeof network.interface_dns === 'string' && network.interface_dns.length > 0
        ? network.interface_dns : '未配置'
    });
    rows.push({ label: 'DNS 服务器', value: dnsServersText(network) });
    rows.push({ label: '路由', value: routesText(network) });
    // 2026-10-07 follow-up: past the compact summary every managed route gets
    // its OWN row, so the full list stays viewable (the old form silently
    // dropped routes past 8). The ≤8 case keeps the previous rendering.
    const routes: DetailRouteView[] = network.routes;
    if (routes !== undefined && routes !== null && routes.length > ROUTE_SUMMARY_LIMIT) {
      for (let i = 0; i < routes.length; i++) {
        const route: DetailRouteView = routes[i];
        if (route === undefined || route === null || typeof route.network !== 'string' ||
          route.network.length === 0) {
          continue;
        }
        rows.push({ label: `路由 ${i + 1}`, value: routeLine(route) });
      }
    }
    rows.push({ label: '默认路由', value: defaultRouteText(network) });
    return rows;
  }

  // Own overlay address WITH the prefix the core assigned (network.prefix);
  // the legacy bare address is the fallback and is marked as prefix-less.
  private networkPrefixText(network: DetailNetworkView): string {
    if (typeof network.prefix === 'string' && network.prefix.length > 0) {
      return network.prefix;
    }
    const legacy: string = this.legacyAddressText();
    if (legacy.length > 0) {
      return `${legacy}（核心未提供前缀）`;
    }
    return UNKNOWN_CORE;
  }

  private legacyAddressText(): string {
    const status: PublishedStatus | null = this.published;
    if (status === null || status.address === undefined || status.address === null ||
      status.address.length === 0) {
      return '';
    }
    return status.address;
  }

  private ownFqdnText(): string {
    const status: PublishedStatus | null = this.published;
    if (status === null || status.fqdn === undefined || status.fqdn === null ||
      status.fqdn.length === 0) {
      return '未知';
    }
    return status.fqdn;
  }

  private trafficFields(): DetailField[] {
    const rows: DetailField[] = [];
    const traffic: DetailTrafficView | null = this.trafficView();
    if (traffic === null) {
      rows.push({ label: '流量统计', value: '未知（核心未上报 wg 计数）' });
      return rows;
    }
    rows.push({
      label: '发送（加密后）',
      value: `${byteText(traffic.tx_bytes)} · ${packetsText(traffic.tx_packets)}`
    });
    rows.push({
      label: '接收（交付 TUN）',
      value: `${byteText(traffic.rx_bytes_to_tun)} · ${packetsText(traffic.rx_packets)}`
    });
    rows.push({ label: '无路由丢弃', value: packetsText(traffic.dropped_no_route) });
    rows.push({ label: '解密失败', value: packetsText(traffic.decrypt_errors) });
    rows.push({
      label: '说明',
      value: '计数器为累计值（自设备启动以来）；发送为加密后的传输字节，接收为交付 TUN 的明文字节'
    });
    return rows;
  }

  private relayFields(): DetailField[] {
    const rows: DetailField[] = [];
    const status: PublishedStatus | null = this.published;
    if (status === null) {
      rows.push({ label: '中继', value: '未知（VPN 未运行）' });
      return rows;
    }
    const relay: DetailRelayView | null = this.relayView();
    if (relay === null) {
      rows.push({ label: '中继', value: '未知（状态文档无中继段）' });
      if (status.relay_state !== undefined && status.relay_state !== null) {
        rows.push({ label: '中继状态', value: relayStateText(status.relay_state) });
      }
      return rows;
    }
    rows.push({ label: '中继开关', value: relay.enabled === true ? '已启用' : '未启用' });
    rows.push({ label: '中继状态', value: relayStateText(relay.state) });
    rows.push({
      label: '当前中继地址',
      value: typeof relay.current_url === 'string' && relay.current_url.length > 0
        ? relay.current_url : '未知（尚未建立中继会话）'
    });
    rows.push({
      label: '已配置地址',
      value: relay.urls !== undefined && relay.urls !== null && relay.urls.length > 0
        ? relay.urls.join('、') : '无'
    });
    rows.push({
      label: '中继重连',
      value: typeof relay.reconnects === 'number' ? `${relay.reconnects}` : UNKNOWN_DOC
    });
    rows.push({
      label: '中继流量',
      value: typeof relay.transport_bytes === 'number'
        ? byteText(relay.transport_bytes) : UNKNOWN_DOC
    });
    rows.push({ label: '中继令牌', value: relay.token_valid === true ? '有效' : '无效或未知' });
    rows.push({
      label: '中继最近错误',
      value: typeof relay.last_error_class === 'string' && relay.last_error_class.length > 0
        ? relay.last_error_class : '无'
    });
    rows.push({
      label: '说明',
      value: '当前中继地址是本机中继会话的地址；各对端自己实际附着的中继通道地址，' +
        '见各对端的「中继通道地址」行'
    });
    return rows;
  }

  private peerFields(): DetailField[] {
    const rows: DetailField[] = [];
    if (this.published === null) {
      rows.push({ label: '对端', value: '未知（VPN 未运行）' });
      return rows;
    }
    const list: UiPeerEntry[] = this.peersList();
    if (list.length === 0) {
      rows.push({ label: '对端', value: '无（管理端未下发对端列表）' });
      return rows;
    }
    for (let i = 0; i < list.length; i++) {
      rows.push({ label: `${i + 1}. ${list[i].name}`, value: peerBlockText(list[i]) });
      // 2026-10-08: the ICE detail gets its OWN per-peer block row (the WG
      // block above keeps its exact content) — candidate paths only; the note
      // under this section states that the WG rows stay the actual bearer.
      rows.push({
        label: `${i + 1}. ${list[i].name} · ICE`,
        value: peerIceBlockText(list[i])
      });
    }
    return rows;
  }

  // ---- section summaries (the collapsed header still reports live state) ----

  private overviewSummary(): string {
    const status: PublishedStatus | null = this.published;
    if (status === null) {
      return this.stateText();
    }
    return `${this.stateText()} · 信号${status.signal_registered ? '已连接' : '未连接'}`;
  }

  private networkSummary(): string {
    const network: DetailNetworkView | null = this.networkView();
    if (network === null) {
      return this.legacyAddressText().length > 0 ? this.legacyAddressText() : '未知';
    }
    return this.networkPrefixText(network);
  }

  private trafficSummary(): string {
    const traffic: DetailTrafficView | null = this.trafficView();
    if (traffic === null) {
      return '未知';
    }
    return `发 ${formatBytes(traffic.tx_bytes)} / 收 ${formatBytes(traffic.rx_bytes_to_tun)}`;
  }

  private relaySummary(): string {
    const relay: DetailRelayView | null = this.relayView();
    if (relay === null) {
      const status: PublishedStatus | null = this.published;
      if (status !== null && status.relay_state !== undefined && status.relay_state !== null) {
        return relayStateText(status.relay_state);
      }
      return '未知';
    }
    if (relay.enabled !== true) {
      return '未启用';
    }
    return typeof relay.current_url === 'string' && relay.current_url.length > 0
      ? relay.current_url : relayStateText(relay.state);
  }

  private peersSummary(): string {
    return `${onlinePeerCount(this.peersList())}/${this.peersList().length} 已连接`;
  }

  // Live view of the drilled-down peer: re-read from the polled peers[] by
  // public key so the detail page keeps up with the status document; the
  // tapped snapshot is the fallback once the peer left the map.
  private selectedPeerLive(): UiPeerEntry | null {
    const selected: UiPeerEntry | null = this.selectedPeer;
    if (selected === null) {
      return null;
    }
    const list: UiPeerEntry[] = this.peersList();
    for (let i = 0; i < list.length; i++) {
      if (list[i].pub_key === selected.pub_key) {
        return list[i];
      }
    }
    return selected;
  }

  private selectedPeerName(): string {
    const peer: UiPeerEntry | null = this.selectedPeerLive();
    return peer !== null ? peer.name : '';
  }

  // 主题 (2026-10-09): the three-state peer status mapped to the SHARED
  // base/dark color resources. NetBirdUiModel stays a pure Node-loadable module
  // and keeps its own literal hex for the same three states (its host selfcheck
  // pins them); the page is where a $r resource can exist, so the status token
  // is translated to a resource HERE rather than teaching the model about ArkUI.
  private peerStatusResource(status: string): ResourceColor {
    if (status === 'connected') {
      return $r('app.color.nb_status_success');
    }
    if (status === 'idle') {
      return $r('app.color.nb_text_muted');
    }
    return $r('app.color.nb_border');
  }

  private selectedPeerColor(): ResourceColor {
    const peer: UiPeerEntry | null = this.selectedPeerLive();
    return peer !== null
      ? this.peerStatusResource(peer.status)
      : $r('app.color.nb_text_muted');
  }

  private selectedPeerFields(): DetailField[] {
    const rows: DetailField[] = [];
    const peer: UiPeerEntry | null = this.selectedPeerLive();
    if (peer === null) {
      return rows;
    }
    rows.push({
      label: '域名',
      value: peer.fqdn !== null && peer.fqdn.length > 0 ? peer.fqdn : '未知'
    });
    rows.push({ label: 'NetBird IP', value: peer.ip.length > 0 ? peer.ip : '未知' });
    rows.push({ label: '状态', value: peerStatusDetailText(peer) });
    rows.push({
      label: '连接方式',
      value: peer.online ? connectionLabel(peer.connection) : '未知（无本机会话）'
    });
    rows.push({ label: '最近握手', value: lastHandshakeText(peer.last_handshake_s) });
    rows.push({ label: '发送（明文）', value: peerPayloadBytesText(peer, true) });
    rows.push({ label: '接收（明文）', value: peerPayloadBytesText(peer, false) });
    rows.push({
      label: '对端端点',
      value: peer.endpoint.length > 0 ? peer.endpoint : UNKNOWN_CORE
    });
    rows.push({
      label: '本机出口',
      value: peer.egress_local.length > 0 ? peer.egress_local : UNKNOWN_CORE
    });
    rows.push({
      label: '对端网络',
      value: peer.allowed_ips.length > 0 ? peer.allowed_ips.join('、') : UNKNOWN_CORE
    });
    rows.push({ label: '承载', value: peerCarrierText(peer) });
    rows.push({ label: '中继通道地址', value: peerRelayText(peer) });
    // 2026-10-08 per-peer ICE 明细: candidate path only — the actual bearer
    // stays the 连接方式 / 承载 / 中继通道地址 rows above.
    const ice: UiPeerIceDetail | undefined = peer.ice;
    if (ice === undefined || ice === null) {
      rows.push({ label: 'ICE', value: '未知（核心未上报该对端的 ICE 详情）' });
    } else {
      rows.push({ label: 'ICE 状态', value: iceStateText(ice.state) });
      rows.push({ label: 'ICE 角色', value: iceRoleText(ice.controlling) });
      rows.push({ label: 'ICE 本端', value: iceEndpointText(ice.selected_local, ice) });
      rows.push({ label: 'ICE 对端', value: iceEndpointText(ice.selected_remote, ice) });
      rows.push({ label: 'ICE 可达性', value: iceReachText(ice) });
      rows.push({ label: 'ICE 最近错误', value: iceErrorText(ice) });
      rows.push({
        label: 'ICE 说明',
        value: 'ICE 只描述候选路径与端点应用情况；实际承载以「连接方式 / 承载 / 中继通道地址」' +
          '为准，ICE 已连接不代表流量直连'
      });
    }
    rows.push({
      label: '本机中继会话',
      value: this.relayUrlText().length > 0
        ? this.relayUrlText() : '无（本机未建立中继会话）'
    });
    rows.push({ label: '公钥', value: peer.pub_key });
    return rows;
  }

  private stateText(): string {
    const state: string = this.published !== null ? this.published.state : '';
    // 已连接 requires the core state connected AND the own overlay IP (the
    // network map arrived) AND the vpn-tun interface actually existing
    // (roam-1005-0014: the interface was removed while the core kept
    // self-reporting connected); every other report — including a recorded
    // connect failure — renders the OFFICIAL label for that state (see
    // NetBirdUiModel.homeStateText). The failure itself is the message, not a
    // state: it rides the error line and the toast (upstream: a Toast with the
    // engine's own text, MainActivity.java:1030-1036).
    const hasOverlayIp: boolean = this.published !== null &&
      this.published.address !== undefined && this.published.address !== null &&
      this.published.address.length > 0;
    const ifacePresent: boolean = this.published === null ||
      this.published.iface_present !== false;
    return homeStateText(state,
      this.pendingActionTarget === PENDING_TARGET_CONNECTED, this.stopSent,
      hasOverlayIp, ifacePresent, this.loginRequired());
  }

  // upstream's sticky `loginRequired` flag (HomeFragment.java:114, :873-883):
  // "Login required" replaces the Disconnected label, because a plain 未连接 would
  // suggest that a reconnect is all that is needed. Our carrier for it is the SAME
  // class token the extension persists when the core reports the login verdict
  // (NetBirdVpnExtensionAbility credentialFailure = NEEDS_LOGIN_CLASS), so the
  // official state set speaks the core's own `needs-login` class — not the
  // pre-integration shell-side `account-login-required` string, which the core
  // never emits.
  private loginRequired(): boolean {
    return this.connectFailureClass === NEEDS_LOGIN_CLASS;
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
    return `${onlinePeerCount(this.peersList())}/${this.peersList().length} 已连接`;
  }

  // 主题 (2026-10-09): the home status verdict's colour, from the shared
  // base/dark resources. The ResourceColor return type follows .fontColor /
  // Circle().fill (both ResourceColor) — a `string` return could not carry a
  // resource reference without a type error.
  private stateColor(): ResourceColor {
    const text: string = this.stateText();
    if (text === '已连接') {
      return $r('app.color.nb_status_success');
    }
    if (text === '连接中…' || text === '正在断开…') {
      return $r('app.color.nb_status_warning');
    }
    if (text === '需要登录') {
      return $r('app.color.nb_status_error');
    }
    return $r('app.color.nb_text_secondary');
  }

  private toggleOn(): boolean {
    return this.published !== null && vpnActiveState(this.published.state);
  }

  // 2026-10-12 upstream tap handling: the neutral fill marks "a transition is being
  // painted" (upstream dims and pulses the disabled switch, HomeFragment.java:442-460).
  private toggleBusy(): boolean {
    return this.stopSent || this.transitionPainted;
  }

  private toggleLabel(): string {
    if (this.stopSent) {
      return '正在断开…';
    }
    if (this.toggleOn()) {
      return '断开';
    }
    return this.transitionPainted ? '连接中…' : '连接';
  }

  // 主题 (2026-10-09): the big connect/stop toggle's fill. 已连接 keeps the
  // success role, the busy state the neutral-button role, idle the primary
  // role — every one from the base/dark resources, so the circle follows the
  // system mode without any cached hex.
  private toggleColor(): ResourceColor {
    if (this.toggleOn()) {
      return $r('app.color.nb_status_success');
    }
    if (this.toggleBusy()) {
      return $r('app.color.nb_neutral_button_background');
    }
    return $r('app.color.nb_primary');
  }

  // 2026-10-12 upstream tap handling (HomeFragment.java:403-436, :550-556): while a
  // transition is painted the toggle is DISABLED, and 2000 ms into it the toggle
  // re-enables (setToggle(..., canForceCancel, …)) so a stuck transition can still
  // be aborted by the user; once the engine settles, the toggle is always enabled.
  private toggleEnabled(): boolean {
    if (!this.configured) {
      return false;
    }
    if (this.transitionPainted) {
      return this.forceCancelOpen;
    }
    return true;
  }

  private lastErrorLine(): string {
    if (this.actionError.length > 0) {
      return this.actionError;
    }
    // 2026-10-06 登录失败可见性: a recorded terminal failure carries its own
    // Chinese reason — it beats the generic disconnect notice (and there is
    // no live document to read a class from).
    if (this.published === null) {
      // 2026-10-12: the core's needs-login class (upstream StatusNeedsLogin —
      // the management server refused this peer, its session expired or it is
      // not registered). The official label is the whole message.
      if (this.connectFailureClass === NEEDS_LOGIN_CLASS) {
        return NEEDS_LOGIN_TEXT;
      }
      if (this.connectFailureClass.length > 0) {
        return connectFailureText(this.connectFailureClass);
      }
      // A vanished document after an active state: the reconnect notice (the
      // extension fail-closed or died mid-connection).
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
    let storedMode: string = ACCOUNT_MODE_SETUP_KEY;
    try {
      const modePath: string = `${filesDir}/${ACCOUNT_MODE_FILE_NAME}`;
      if (fs.accessSync(modePath)) {
        storedMode = parseAuthMode(fs.readTextSync(modePath));
      }
    } catch (error) {
      storedMode = ACCOUNT_MODE_SETUP_KEY;
    }
    // 2026-10-12: account mode has NO credential document any more — upstream
    // stores no token (client/internal/profilemanager/config.go:62-135). What
    // makes an account session "configured" is the device config (management
    // URL + WireGuard identity) plus the account marker; whether the peer still
    // has to log in is a SERVER verdict, reported as 需要登录.
    this.authMode = storedMode;
    this.configured = storedMode === ACCOUNT_MODE_ACCOUNT
      ? url.length > 0 : url.length > 0 && keyStored;
    this.configuredUrl = url;
    this.keyStored = keyStored;
  }

  // Save the form: validate (Chinese, human-readable errors), then write the
  // two credential files in the EntryAbility.writeFile shape. Only the file
  // name + UTF-8 byte count are ever logged — never the setup key nor the
  // private key.
  private saveSetup(): void {
    if (this.accountModeSwitchBlocked()) {
      this.formError = '断开 VPN 后可更改配置';
      return;
    }
    if (this.authMode !== ACCOUNT_MODE_SETUP_KEY) {
      return;
    }
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
    const configText: string = buildDeviceConfigJson(urlCheck.url, privateKey,
      this.deviceRelayEnabled());
    const keyText: string = buildSetupKeyFileText(this.keyInput);
    if (!this.writeSetupFile(SETUP_CONFIG_FILE, configText) ||
      !this.writeSetupFile(SETUP_KEY_FILE, keyText) ||
      !this.writeSetupFile(ACCOUNT_MODE_FILE_NAME, `${ACCOUNT_MODE_SETUP_KEY}\n`)) {
      this.formError = '保存失败，请重试';
      return;
    }
    // Drop the secret from UI state as soon as it is on disk.
    this.keyInput = '';
    this.showSetupForm = false;
    this.refreshSetupState();
    this.formHint = '配置已保存，正在连接 VPN。';
    this.requestStart();
  }

  private beginConfigurationEdit(): void {
    if (this.accountModeSwitchBlocked()) {
      return;
    }
    this.urlInput = this.configuredUrl;
    this.keyInput = '';
    this.formError = '';
    this.formHint = '';
    this.accountLoginError = '';
    this.showSetupForm = true;
  }

  private cancelConfigurationEdit(): void {
    if (!this.configured || this.accountLoginCommitting) {
      return;
    }
    this.cancelAccountLogin(false);
    this.refreshSetupState();
    this.urlInput = this.configuredUrl;
    this.keyInput = '';
    this.formError = '';
    this.formHint = '';
    this.accountLoginError = '';
    this.showSetupForm = false;
  }

  private selectAuthMode(mode: string): void {
    if (this.accountModeSwitchBlocked() || this.accountLoginBusy) {
      return;
    }
    this.authMode = mode;
    this.keyInput = '';
    this.formError = '';
    this.formHint = '';
    this.accountLoginError = '';
  }

  private accountModeSwitchBlocked(): boolean {
    return this.requestSent || (this.published !== null && vpnActiveState(this.published.state));
  }

  // 2026-10-12: the account store is gone, so the 0.0.22 secure-storage
  // diagnostics went with it. What remains are the stable reason tokens the
  // login action (NetBirdAccountLogin) reports: discovery / crypto /
  // management-login failures.
  private accountLoginErrorText(error: Error | string): string {
    const reason: string = typeof error === 'string' ? error : error.message;
    if (reason === 'cancelled') { return ''; }
    if (reason === 'discovery_unavailable' || reason === 'login_unavailable') {
      return '账户登录暂不可用，请更新应用后重试。';
    }
    if (reason === 'discovery_timeout' || reason === 'login_timeout') {
      return '连接管理服务器超时，请检查网络后重试。';
    }
    if (reason === 'network-error') { return '网络不可用或服务器无法访问，请检查网络后重试。'; }
    // the management server did not accept this account: the same
    // 「需要登录」 the connect path reports — the account is not logged in.
    if (reason === 'needs-login') { return NEEDS_LOGIN_TEXT; }
    return '账户登录失败，请重试。';
  }

  // 2026-10-13: the 账户登录 button's entry — the ordinary login/registration
  // flow (Setup-Key users never come here).
  private async beginAccountLogin(): Promise<void> {
    if (this.accountLoginBusy || this.accountModeSwitchBlocked()) {
      if (this.accountModeSwitchBlocked()) {
        this.accountLoginError = '断开 VPN 后可切换登录方式';
      }
      return;
    }
    const urlCheck: UrlCheck = checkManagementUrl(this.urlInput);
    if (!urlCheck.ok) {
      this.formError = urlCheck.error;
      return;
    }
    await this.openAccountLogin(ACCOUNT_INTENT_SETUP, urlCheck.url, false);
  }

  // The ONE ArkWeb PKCE surface opener, shared by all three intents (upstream
  // runs the same `runOAuthFlow` for the login, the engine's login-required
  // start and the extend: client/android/login.go:216-232). Only the discovery's
  // SessionExtend flag differs, and `targetUrl` is captured HERE so an in-flight
  // flow cannot be retargeted by the form behind it.
  private async openAccountLogin(intent: string, targetUrl: string,
    sessionExtend: boolean): Promise<void> {
    if (this.accountLoginBusy) {
      return;
    }
    const urlCheck: UrlCheck = checkManagementUrl(targetUrl);
    if (!urlCheck.ok) {
      this.reportAccountLoginFailure(intent, 'invalid_input');
      return;
    }
    this.accountLoginIntent = intent;
    this.accountLoginTargetUrl = urlCheck.url;
    this.accountLoginError = '';
    if (intent === ACCOUNT_INTENT_SETUP) {
      this.formError = '';
      this.formHint = '';
    }
    this.accountLoginBusy = true;
    const generation: number = ++this.accountLoginGeneration;
    try {
      const caPem: string = readPackagedCaRootsPem(getContext(this).resourceManager);
      const prepared: PreparedAccountLogin =
        await this.accountLoginController.begin(urlCheck.url, caPem, sessionExtend);
      if (generation !== this.accountLoginGeneration) {
        return;
      }
      this.accountAuthorizeUrl = prepared.authorizeUrl;
      this.accountExpectedState = prepared.expectedState;
      this.accountRedirectUri = prepared.redirectUri;
      this.accountLoginOpen = true;
    } catch (error) {
      if (generation === this.accountLoginGeneration) {
        this.reportAccountLoginFailure(intent, error instanceof Error ? error : 'login_failed');
      }
    } finally {
      if (generation === this.accountLoginGeneration) {
        this.accountLoginBusy = false;
      }
    }
  }

  // A failure BEFORE the SSO surface exists has no single home: the setup form
  // owns its own error line, the connect intent owns the home card's error line
  // and the extend owns upstream's toast (the row itself is the persistent
  // channel). All three render the SAME stable-token sentences.
  private reportAccountLoginFailure(intent: string, error: Error | string): void {
    const reason: string = typeof error === 'string' ? error : error.message;
    if (intent === ACCOUNT_INTENT_EXTEND) {
      this.toastSessionExtend(sessionExtendFailureText(reason));
      return;
    }
    const text: string = this.accountLoginErrorText(reason);
    if (intent === ACCOUNT_INTENT_CONNECT) {
      this.actionError = text;
      return;
    }
    this.accountLoginError = text;
  }

  private async finishAccountLogin(code: string): Promise<void> {
    if (this.accountLoginBusy || this.accountModeSwitchBlocked()) {
      this.cancelAccountLogin(true);
      return;
    }
    const generation: number = this.accountLoginGeneration;
    this.accountLoginBusy = true;
    this.accountLoginOpen = false;
    try {
      // The device identity and the management URL the extension will dial:
      // the login action registers EXACTLY this peer (and with the packaged
      // trust roots the extension uses for https).
      const privateKey: string = this.devicePrivateKey();
      if (privateKey.length === 0) {
        throw new Error('device_key_unavailable');
      }
      const caPem: string = readPackagedCaRootsPem(getContext(this).resourceManager);
      const device: DeviceConfigFile = {
        management_url: this.accountManagementUrl(),
        private_key: privateKey,
        ca_pem: caPem,
        relay_enabled: this.deviceRelayEnabled()
      };
      const connectorConfig: string = buildConnectorConfigJson(device, false, null);
      this.accountLoginCommitting = true;
      // The login action: exchange the code, register the peer with the JWT
      // through the core, and keep NOTHING token-shaped (upstream stores no
      // token). It also runs BEFORE the VPN starts, exactly like upstream's
      // `auth.login(...)` before `connectClient.Run`.
      const result: AccountLoginResult =
        await this.accountLoginController.finish(code, connectorConfig);
      if (generation !== this.accountLoginGeneration) {
        return;
      }
      const configText: string = buildDeviceConfigJson(result.managementUrl, privateKey,
        this.deviceRelayEnabled());
      if (!this.writeSetupFile(SETUP_CONFIG_FILE, configText)) {
        throw new Error('configuration_save_failed');
      }
      if (!this.writeSetupFile(ACCOUNT_MODE_FILE_NAME, `${ACCOUNT_MODE_ACCOUNT}\n`)) {
        throw new Error('mode_save_failed');
      }
      this.urlInput = result.managementUrl;
      this.showSetupForm = false;
      this.accountLoginError = '';
      this.formHint = '账户已登录，正在连接 VPN。';
      this.refreshSetupState();
      this.requestStart();
    } catch (error) {
      if (generation === this.accountLoginGeneration) {
        this.accountLoginError = this.accountLoginErrorText(error instanceof Error ? error : 'login_failed');
      }
    } finally {
      if (generation === this.accountLoginGeneration) {
        this.accountLoginBusy = false;
        this.accountLoginCommitting = false;
      }
    }
  }

  // The management URL this login targets: the checked form input when the
  // form is open, else the stored device config. '' when neither is readable
  // (the login then fails closed as invalid input).
  private accountManagementUrl(): string {
    const checked: UrlCheck = checkManagementUrl(this.urlInput);
    if (checked.ok) {
      return checked.url;
    }
    try {
      const configPath: string = `${getContext(this).filesDir}/${SETUP_CONFIG_FILE}`;
      if (fs.accessSync(configPath)) {
        return readStoredManagementUrl(fs.readTextSync(configPath));
      }
    } catch (error) {
      return '';
    }
    return '';
  }

  private cancelAccountLogin(showError: boolean): void {
    if (this.accountLoginCommitting) {
      return;
    }
    this.accountLoginGeneration++;
    this.accountLoginController.cancel();
    this.accountLoginBusy = false;
    this.accountLoginOpen = false;
    this.accountAuthorizeUrl = '';
    this.accountExpectedState = '';
    this.accountRedirectUri = '';
    if (!showError) {
      this.accountLoginError = '';
    }
  }

  // ---------------------------------------------------------------------------
  // 2026-10-13: the two NEW intents of the ONE SSO surface.
  //
  // (a) login-required connect. Upstream's connect toggle does not have a
  //     separate sign-in step: the tap requests CONNECTED and the engine start
  //     itself runs the interactive SSO, which is why the branch only clears the
  //     sticky flag (HomeFragment.java:193-200, and `onLoginRequired`
  //     :873-882 paints 需要登录). In this split the engine runs in the
  //     VPN-extension process, which has no UI surface to show an IdP page on,
  //     so the UI process runs the SAME interactive login action the 账户登录
  //     button runs and connects only once it succeeded — same JWT, same
  //     management Login/registration call, same order (login before the tunnel).
  //     Setup-Key users are untouched: their 需要登录 is answered by the core's
  //     own setup-key login inside the extension, and no account SSO applies.
  //
  // (b) session extend. Upstream `MainActivity.extendSession()`: interactive SSO
  //     with `sessionExtend=true`, then the engine's `ExtendAuthSession` — the
  //     tunnel untouched, no resync (:1068-1085, session.go:127-155). Here the
  //     management call runs in the UI process against the management endpoint
  //     the extension itself dials, with the same device identity.
  // ---------------------------------------------------------------------------

  // The ONE code callback of the ArkWeb surface: the SSO run is identical for
  // every intent, so only the commit differs.
  private async handleAccountLoginCode(code: string): Promise<void> {
    if (this.accountLoginIntent === ACCOUNT_INTENT_EXTEND) {
      await this.finishSessionExtend(code);
      return;
    }
    if (this.accountLoginIntent === ACCOUNT_INTENT_CONNECT) {
      await this.finishConnectLogin(code);
      return;
    }
    await this.finishAccountLogin(code);
  }

  // (a) The connect intent's commit: the same management login action, then the
  // connect the user asked for. NOTHING is persisted — the device config and the
  // account marker already exist (they are what makes this peer configured and
  // this branch reachable), exactly like upstream's engine start, which re-runs
  // `auth.login(...)` against the config it already has (client.go:194-215).
  private async finishConnectLogin(code: string): Promise<void> {
    if (this.accountLoginBusy) {
      this.cancelAccountLogin(true);
      return;
    }
    const generation: number = this.accountLoginGeneration;
    this.accountLoginBusy = true;
    this.accountLoginOpen = false;
    try {
      const connectorConfig: string = this.loginConnectorConfig(this.accountLoginTargetUrl);
      this.accountLoginCommitting = true;
      const result: AccountLoginResult =
        await this.accountLoginController.finish(code, connectorConfig);
      if (generation !== this.accountLoginGeneration) {
        return;
      }
      // The verdict that sent the user here is answered: the peer is registered
      // again, so this attempt starts from a clean card.
      this.actionError = '';
      this.connectFailureClass = '';
      this.connectFailureAtMs = 0;
      hilog.info(DOMAIN, TAG, 'UI_CONNECT_LOGIN_OK|bundle=%{public}s|url=%{public}s',
        BUNDLE_NAME, result.managementUrl);
      this.requestStart();
    } catch (error) {
      if (generation === this.accountLoginGeneration) {
        this.actionError = this.accountLoginErrorText(error instanceof Error ? error : 'login_failed');
      }
    } finally {
      if (generation === this.accountLoginGeneration) {
        this.accountLoginBusy = false;
        this.accountLoginCommitting = false;
      }
    }
  }

  // (b) The extend intent's commit: the fresh JWT goes to the management
  // ExtendAuthSession through the core's one-shot job and nowhere else; the
  // server's own 3-state answer is the only result. The tunnel is never touched
  // (no stop, no start, no re-login of the peer).
  private async finishSessionExtend(code: string): Promise<void> {
    if (this.accountLoginBusy) {
      this.cancelAccountLogin(true);
      return;
    }
    const generation: number = this.accountLoginGeneration;
    this.accountLoginBusy = true;
    this.accountLoginOpen = false;
    try {
      const connectorConfig: string = this.loginConnectorConfig(this.accountLoginTargetUrl);
      this.accountLoginCommitting = true;
      const result: SessionExtendResult =
        await this.accountLoginController.finishExtend(code, connectorConfig);
      if (generation !== this.accountLoginGeneration) {
        return;
      }
      // `unknown` (the reply carried no deadline) keeps the anchor the row was
      // already showing — upstream `ApplySessionDeadline`'s nil no-op.
      if (result.sessionExpiresAtS > 0) {
        this.sessionExtendAtS = result.sessionExpiresAtS;
      }
      hilog.info(DOMAIN, TAG, 'UI_SESSION_EXTEND_OK|bundle=%{public}s|expiry=%{public}s',
        BUNDLE_NAME, result.sessionExpiry);
      this.toastSessionExtend(SESSION_EXTENDED_TEXT);
    } catch (error) {
      if (generation === this.accountLoginGeneration) {
        const reason: string = error instanceof Error ? error.message : 'extend_failed';
        hilog.warn(DOMAIN, TAG, 'UI_SESSION_EXTEND_FAILED|bundle=%{public}s|reason=%{public}s',
          BUNDLE_NAME, reason);
        this.toastSessionExtend(sessionExtendFailureText(reason));
      }
    } finally {
      if (generation === this.accountLoginGeneration) {
        this.accountLoginBusy = false;
        this.accountLoginCommitting = false;
      }
    }
  }

  // The connector start config both new intents dial with: the SAME document
  // shape the extension hands to `connector_start_with_socket` and the setup
  // login action hands to `account_login_*` — management URL + this device's
  // WireGuard identity + the packaged trust roots + the relay switch. It is the
  // identity the peer is already registered under; nothing here is written to
  // disk or logged.
  private loginConnectorConfig(managementUrl: string): string {
    const privateKey: string = this.devicePrivateKey();
    if (privateKey.length === 0) {
      throw new Error('device_key_unavailable');
    }
    const caPem: string = readPackagedCaRootsPem(getContext(this).resourceManager);
    const device: DeviceConfigFile = {
      management_url: managementUrl,
      private_key: privateKey,
      ca_pem: caPem,
      relay_enabled: this.deviceRelayEnabled()
    };
    return buildConnectorConfigJson(device, false, null);
  }

  // The extend's two outcomes are upstream's two Toasts (MainActivity.java:
  // 1073-1086: LENGTH_SHORT "Session extended", LENGTH_LONG "Failed to extend
  // session: %s"), in the official zh-rCN wording. Fail-soft like the connect
  // toast: the state row itself is the persistent channel.
  private toastSessionExtend(message: string): void {
    if (message.length === 0) {
      return;
    }
    try {
      this.getUIContext().getPromptAction().showToast({
        message: message, duration: SESSION_TOAST_MS
      });
      hilog.info(DOMAIN, TAG, 'UI_SESSION_EXTEND_TOAST|bundle=%{public}s', BUNDLE_NAME);
    } catch (error) {
      hilog.warn(DOMAIN, TAG, 'UI_SESSION_EXTEND_TOAST_REJECTED|bundle=%{public}s|code=%{public}s',
        BUNDLE_NAME, String((error as BusinessFailure).code ?? 'none'));
    }
  }

  // 主页会话行: the deadline the row renders. Upstream has ONE writer (the
  // engine's status recorder) and one reader (the fragment field). Here the
  // connector lives in the extension process, so the published document is the
  // connector's own anchor and `sessionExtendAtS` is the server's answer to an
  // extend this page ran; the LATER of the two is shown, so a stale published
  // value can never drag the display back after a successful extend and the
  // published document takes over as soon as the connector re-anchors from the
  // server (its next SyncResponse carries sessionExpiresAt,
  // management/internals/shared/grpc/conversion.go:211-219).
  private sessionDeadlineS(): number {
    const status: PublishedStatus | null = this.published;
    const atS: number | undefined = status !== null &&
      typeof status.session_expires_at_s === 'number' ? status.session_expires_at_s : undefined;
    return sessionRowDeadlineS(atS, this.sessionExtendAtS);
  }

  // Upstream `updateSessionRow()` (:901-912): the row exists exactly while a
  // positive deadline is known, and is GONE otherwise — no lead threshold.
  // A stopped engine has no deadline to report at all (the fragment re-seeds
  // from `accessor.sessionExpiresAt()` on resume, :883-898, which answers 0
  // once the service is unbound), so no live connector document = no row.
  private sessionRowOffer(): boolean {
    if (this.published === null) {
      return false;
    }
    return sessionRowView(this.sessionDeadlineS(), Date.now()).offer;
  }

  // Upstream `formatSessionExpiry()` (:920-941), verbatim rules.
  private sessionRowText(): string {
    return sessionRowView(this.sessionDeadlineS(), Date.now()).text;
  }

  private sessionRowHint(): string {
    return sessionRowView(this.sessionDeadlineS(), Date.now()).hint;
  }

  // The row's tap (upstream `sessionExpiryRow.setOnClickListener` →
  // `serviceAccessor.extendSession()`, HomeFragment.java:219-223): the
  // interactive SSO with the extend flag, then the management ExtendAuthSession.
  // Allowed while the tunnel is UP — that is the point of an extend — so the
  // setup form's "断开 VPN 后可更改" gate deliberately does not apply here.
  private async beginSessionExtend(): Promise<void> {
    if (this.sessionExtendBusy || this.accountLoginBusy) {
      return;
    }
    this.sessionExtendBusy = true;
    try {
      await this.openAccountLogin(ACCOUNT_INTENT_EXTEND, this.configuredUrl, true);
    } finally {
      this.sessionExtendBusy = false;
    }
  }

  // (a) The login-required connect tap: the ordinary account login (the SAME
  // action the 账户登录 button runs — `sessionExtend=false`), against the stored
  // management URL. If that URL is unusable there is nothing to sign in
  // against, so the tap falls through to the plain connect and reports exactly
  // the failure it would have reported anyway (never a silent no-op).
  private async beginConnectLogin(): Promise<void> {
    if (this.accountLoginBusy) {
      return;
    }
    if (!checkManagementUrl(this.configuredUrl).ok) {
      this.requestStart();
      return;
    }
    await this.openAccountLogin(ACCOUNT_INTENT_CONNECT, this.configuredUrl, false);
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

  // Relay switch carried across reconfigure: read the stored config's
  // relay_enabled BEFORE the file is overwritten — false only on the exact
  // explicit opt-out; a missing/unreadable value keeps the enabled default
  // (a fresh configure writes true via the parameter default).
  private deviceRelayEnabled(): boolean {
    try {
      const configPath: string = `${getContext(this).filesDir}/${SETUP_CONFIG_FILE}`;
      if (fs.accessSync(configPath)) {
        return readStoredRelayEnabled(fs.readTextSync(configPath));
      }
    } catch (error) {
      // unreadable/missing config = enabled default below
    }
    return true;
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
    const vpnLive: boolean = this.logoutRemoteActive();
    AlertDialog.show({
      title: vpnLive ? '注销确认' : '清除配置确认',
      message: vpnLive
        ? '断开连接并从管理端移除本设备，同时清除本机配置（含设备私钥）。确定继续？'
        : '仅清除本机配置（含设备私钥），不会从管理端移除设备记录。确定继续？',
      primaryButton: {
        value: '取消',
        action: (): void => {
        }
      },
      secondaryButton: {
        value: vpnLive ? '移除并清除' : '清除本机配置',
        fontColor: $r('app.color.nb_status_error'),
        action: (): void => {
          this.performLogout();
        }
      }
    });
  }

  private performLogout(): void {
    const vpnLive: boolean = this.logoutRemoteActive();
    // 0.0.28: an explicit logout ends the user's VPN-on intent whether or not a
    // tunnel is still live (the keep-alive fact `loggedOut` only guards the
    // 25 s around it; this latch is what must not survive it).
    this.vpnIntentOn = false;
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
    // 2026-10-10 后台保活（实验，方案 A）: a logout ends the app session — the
    // borrowed task goes with it, and every session-scoped platform verdict
    // (including an illegal-type stop) is forgotten with it.
    this.bgKeepAliveState = keepAliveOnLogout(this.bgKeepAliveState);
    this.stopKeepAliveNow(KEEP_ALIVE_REASON_LOGOUT);
    this.removeSetupFile(SETUP_CONFIG_FILE);
    this.removeSetupFile(SETUP_KEY_FILE);
    this.removeSetupFile(ACCOUNT_MODE_FILE_NAME);
    // a ticket that was never consumed (extension gone before its tick)
    this.removeSetupFile(LOGOUT_FILE_NAME);
    this.urlInput = DEFAULT_MANAGEMENT_URL;
    this.keyInput = '';
    this.formError = '';
    this.formHint = '';
    this.showSetupForm = false;
    // 2026-10-12: nothing token-shaped is stored locally any more (upstream
    // stores no token), so the local half of the logout is exactly these file
    // removals — there is no encrypted credential document to clear.
    this.refreshSetupState();
    this.formHint = serverLogoutConfirmed
      ? '已注销：本机凭据已删除，管理端已删除本设备。'
      : '已删除本机凭据。如需从管理端移除本设备，请在管理后台删除。';
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

  // `fromAutoReconnect`: 0.0.20 — this start was decided by the automatic
  // reconnect, so a REFUSAL is that attempt's own named failure and has to reach
  // the diagnostics and the reconnect policy (the user's own connect already has
  // its visible error text and its ui_connect record). Everything else about this
  // path is unchanged, and the extension-start call site below is still the ONLY
  // one in this file.
  private requestStart(fromAutoReconnect: boolean = false): void {
    if (this.requestSent) {
      // Request already sent this attempt: no repeat click, no re-issue.
      return;
    }
    // 诊断记录（用户「连接 / 重试」动作的集中处理点）：只记安全事件名 + 计数。
    // 重试 = 上一次尝试留下了可见失败（失败通知或动作错误）；否则是普通连接。
    this.diagActionCount = this.diagActionCount + 1;
    this.diagEvent(this.connectFailureClass.length > 0 || this.actionError.length > 0
      ? 'ui_connect_retry' : 'ui_connect', this.diagActionCount);
    this.requestSent = true;
    // 2026-10-10 后台保活（实验，方案 A）: the user asked for a connection (or the
    // automatic reconnect did) — this is the ONE thing that lifts a previous
    // dismissal of the borrowed task and starts its failure budget over. Taken
    // here, before the platform call: the request is what clears it, and a
    // request the platform then rejects is answered by the next tick's
    // "no live tunnel" stop anyway.
    this.bgKeepAliveState = keepAliveOnUserConnect(this.bgKeepAliveState);
    this.actionError = '';
    this.disconnectNotice = '';
    // 2026-10-06 登录失败可见性: the rendered verdict is cleared for a fresh
    // attempt, but the NOTICE FILE is deliberately left in place (2026-10-11,
    // fix/retry): it is the last recorded verdict, and deleting it in advance is
    // exactly what left the 0.0.24 card on 连接中… forever (pc-connecting-diag
    // REPORT §4.2). updatePendingAction() removes it — and only then — once this
    // attempt's own engine report proves the attempt really started.
    this.connectFailureClass = '';
    this.lastStartAtMs = Date.now();
    // 2026-10-12 upstream tap handling (beginPendingAction, HomeFragment.java:198-200, :623-630
    // + :620-628): the tap's requested state and its ONE 7 s Runnable. A tap that
    // requests connect and gets no answer falls back to the last reported real
    // state (NetBirdUiModel.decideTransition) — the attempt itself is never timed
    // out, because upstream never times out the connecting state or the engine.
    this.pendingActionTarget = PENDING_TARGET_CONNECTED;
    this.pendingActionDeadlineMs = this.lastStartAtMs + PENDING_ACTION_TIMEOUT_MS;
    // paintTransition(CONNECTING) arms the force-cancel window from zero.
    this.forceCancelArmedAtMs = 0;
    this.forceCancelOpen = false;
    this.noticeSuperseded = false;
    hilog.info(DOMAIN, TAG, 'UI_START_REQUEST|bundle=%{public}s|ability=%{public}s|requestId=%{public}s',
      BUNDLE_NAME, EXTENSION_ABILITY, this.requestId);
    // 2026-10-11 retry re-entry (fix/retry): a repeated startVpnExtensionAbility
    // against a LIVE extension instance is NOT a re-entry — the platform hands
    // `want.parameters` over "when the VPN is started for the FIRST time"
    // (@ohos.net.vpnExtension.d.ts:78-79) and VpnExtensionAbility declares no
    // onConnect — so when the extension's own stop evidence is missing the retry
    // drives ONE stop and waits (bounded) for the instance to be gone before it
    // starts. Without this the 0.0.24 retry produced no new onCreate at all.
    if (connectStartLeg(this.extensionGone()) === CONNECT_LEG_AFTER_STOP) {
      hilog.info(DOMAIN, TAG,
        'UI_START_RESTART|bundle=%{public}s|requestId=%{public}s|reason=extension-may-still-be-alive',
        BUNDLE_NAME, this.requestId);
      this.stopVpnExtensionThenStart(fromAutoReconnect);
      return;
    }
    this.startVpnExtension(fromAutoReconnect);
  }

  // The ONE extension-start call site of this page.
  private startVpnExtension(fromAutoReconnect: boolean): void {
    vpnExtension.startVpnExtensionAbility(this.buildWant()).then(() => {
      // The extension accepted the start; the published status document now
      // drives everything the home tab shows.
      hilog.info(DOMAIN, TAG, 'UI_START_RESOLVED|bundle=%{public}s|requestId=%{public}s',
        BUNDLE_NAME, this.requestId);
    }).catch((error: Error) => {
      const code: string = String((error as BusinessFailure).code ?? 'none');
      this.actionError = `连接请求未成功（错误码 ${code}），请重试`;
      this.requestSent = false;
      // 2026-10-12 upstream tap handling: the platform refused the start up front,
      // i.e. the engine will never report the requested state — the same outcome
      // shouldSuppressPaint clears the latch for (HomeFragment.java:501-522).
      // Without this the tap latch would keep the card on 连接中… for 7 s.
      this.clearPendingAction();
      hilog.error(DOMAIN, TAG, 'UI_START_REJECTED|bundle=%{public}s|code=%{public}s|requestId=%{public}s',
        BUNDLE_NAME, code, this.requestId);
      this.requestId = `ui-${Date.now()}`;
      // 0.0.20: the automatic reconnect's start was refused by the platform (this is
      // the background case the device has not yet answered: whether the system
      // allows a VPN start from a background UI process). Record it as this
      // attempt's failure — the platform's own numeric error code (a closed
      // token, 'none', when it gave none), on the public field set — and fold it
      // into the reconnect policy: the attempt was already counted, so nothing is
      // counted twice; its 30 s verification window simply ends now instead of
      // waiting for an answer that is already known to be "no". The 60 s interval
      // still gates the next attempt, and the page's own error text is untouched.
      if (fromAutoReconnect) {
        this.reconnectState = reconnectOnStartRejected(this.reconnectState);
        this.diagAutoReconnect(this.pageVisible
          ? RECONNECT_TRIGGER_FOREGROUND : RECONNECT_TRIGGER_BACKGROUND,
          RECONNECT_RESULT_FAILED, code, this.reconnectState.attempts);
      }
    });
  }

  // 2026-10-11 retry re-entry (fix/retry): is the extension instance provably
  // gone? The extension writes its `stopping` marker SYNCHRONOUSLY as the FIRST
  // statement of onDestroy (NetBirdVpnExtensionAbility.writeStopMarkerSync), so
  // the marker is the extension's own stop acknowledgement — the one fact the
  // UI process can read about that process without a platform getter. No marker
  // at all (or an unusable one) = no diagnostics session ever opened = nothing
  // alive to stop. A marker still `open` is NOT proof of life either (a killed
  // process leaves it behind), which is why the start leg waits under a bounded
  // clock instead of trusting this answer on its own.
  private extensionGone(): boolean {
    try {
      const markerPath: string = recordPath(getContext(this).filesDir, MARKER_BASENAME_VPN);
      if (markerPath.length === 0 || !fs.accessSync(markerPath)) {
        return true;
      }
      const marker: Marker | null = parseMarker(fs.readTextSync(markerPath));
      if (marker === null) {
        return true;
      }
      return marker.state !== RECONNECT_MARKER_OPEN;
    } catch (error) {
      return true;
    }
  }

  // 2026-10-11 retry re-entry (platform adaptation, ALIGNMENT-AUDIT R4): ONE stop
  // leg, then the start. The platform's stop promise is best-effort (the extension
  // may already be gone) and is deliberately NOT what the start waits on: the wait
  // below polls the extension's own stop evidence, so a stop call that never answers
  // can never keep the user's retry from starting. Still needed exactly because a
  // repeated start against a LIVE instance is not a re-entry on this platform.
  private stopVpnExtensionThenStart(fromAutoReconnect: boolean): void {
    // 0.0.28: the status document disappearing a moment from now is THIS stop, not
    // a fail-closed disconnect — but it is NOT a user stop either. Until 0.0.28
    // this line wrote `lastUserStopAtMs` (the latch requestStop() sets), and the
    // keep-alive policy reads that latch as "the user just asked for a
    // disconnect": decideKeepAlive() answers STOP/user-stop BEFORE every other
    // fact, so the borrowed task was withdrawn on the next 2 s tick — 1-3 s after
    // every automatic reconnect, which is exactly when the following kill became
    // unrecoverable (v027b: ui_auto_reconnect at 10:46:58 / 14:46:39 / 16:39:30,
    // stage=stop reason=user-stop at 10:47:00 / 14:46:40 / 16:39:32). The page's
    // own restart-leg latch carries the suppression instead; only requestStop()'s
    // own leg writes the user-stop one.
    this.lastRestartLegAtMs = Date.now();
    vpnExtension.stopVpnExtensionAbility(this.buildWant()).catch((_error: Error) => {
      // nothing to stop (already gone / never started): the wait leg decides.
    });
    this.waitExtensionGoneThenStart(fromAutoReconnect);
  }

  private async waitExtensionGoneThenStart(fromAutoReconnect: boolean): Promise<void> {
    // The exit condition is the REAL signal: the extension's own stop evidence —
    // the `stopping` marker onDestroy writes synchronously as its first statement —
    // or the UI process's own proof that the pid which wrote the marker was killed
    // (extensionConfirmedDead). Upstream needs no wait at all because stop and start
    // are two methods on one binder inside one process, sequenced under a single lock
    // by restartPending (VPNService.java:311-320; EngineRunner.java:216-232, :397-404);
    // on HarmonyOS the instance must be GONE before a repeated
    // startVpnExtensionAbility can re-enter (want.parameters is a first-start channel
    // and VpnExtensionAbility has no onConnect), so the stop has to be observed. The
    // bounded clock is only the platform fuse: a KILLED extension leaves its marker
    // `open` and will never write the stop evidence, and the user's retry must not
    // hang on it.
    const deadline: number = Date.now() + EXTENSION_STOP_WAIT_MS;
    while (Date.now() < deadline) {
      if (this.extensionConfirmedDead() || this.extensionGone()) {
        break;
      }
      await sleep(EXTENSION_STOP_POLL_MS);
    }
    if (!this.requestSent || this.stopSent) {
      // the user asked to disconnect while the stop leg was in flight: that stop
      // is the last word and no start follows it. The same rule upstream keeps by
      // clearing restartPending inside stop() (EngineRunner.java:375-381), taken
      // here under the page's own request latches.
      return;
    }
    this.startVpnExtension(fromAutoReconnect);
  }

  // 2026-10-12 upstream tap handling: the ONE bookkeeping step of a pending
  // action, on the EXISTING 2 s tick (no second timer, no second status read). It
  // does three things and nothing else:
  //   * runs the pure decideTransition() (upstream shouldSuppressPaint +
  //     expirePendingAction + the force-cancel window) and stores its answer;
  //   * supersedes the PREVIOUS attempt's failure notice — and only then — once
  //     the engine's own report proves that this attempt really started (the
  //     published status document is the cross-process form of the official
  //     client's engine state callback); the notice is never deleted before that;
  //   * on the 7 s expiry releases the latch so the card falls back to the last
  //     reported real state and the user has a working control again.
  private updatePendingAction(live: PublishedStatus | null): void {
    const nowMs: number = Date.now();
    const report: string = engineReportState(live !== null ? live.state : '');
    const targetBefore: string = this.pendingActionTarget;
    const facts: TransitionFacts = {
      'target': this.pendingActionTarget,
      'report': report,
      'deadlineMs': this.pendingActionDeadlineMs,
      'forceCancelArmedAtMs': this.forceCancelArmedAtMs,
      'forceCancelOpen': this.forceCancelOpen,
      'nowMs': nowMs
    };
    const decision: TransitionDecision = decideTransition(facts);
    this.pendingActionTarget = decision.target;
    this.pendingActionDeadlineMs = decision.deadlineMs;
    this.forceCancelArmedAtMs = decision.forceCancelArmedAtMs;
    this.forceCancelOpen = decision.forceCancelOpen;
    this.transitionPainted = decision.transition;
    if (live !== null && !this.noticeSuperseded) {
      this.noticeSuperseded = true;
      this.supersedePreviousVerdict();
    }
    if (decision.settled) {
      hilog.info(DOMAIN, TAG,
        'UI_PENDING_SETTLED|bundle=%{public}s|requestId=%{public}s|state=%{public}s',
        BUNDLE_NAME, this.requestId, report);
      return;
    }
    if (decision.expired) {
      this.pendingActionExpired(targetBefore);
    }
  }

  // The PREVIOUS attempt's recorded verdict is superseded once this attempt's own
  // engine report proves it started. A notice belonging to THIS attempt (written at
  // or after the request, 2 s clock margin — the same rule refreshConnectFailure
  // reads it with) is its live verdict and stays.
  private supersedePreviousVerdict(): void {
    const notice: ConnectFailureNotice | null = readConnectFailure(getContext(this).filesDir);
    if (notice === null || notice.at_ms >= this.lastStartAtMs - 2000) {
      return;
    }
    removeConnectFailure(getContext(this).filesDir);
    hilog.info(DOMAIN, TAG,
      'UI_CONNECT_STARTED|bundle=%{public}s|requestId=%{public}s|note=previous-verdict-superseded',
      BUNDLE_NAME, this.requestId);
  }

  // 2026-10-12 upstream expirePendingAction (HomeFragment.java:640-647): the
  // engine never reached the requested state inside the 7 s window, so fall back
  // to painting whatever it last reported instead of staying latched forever.
  // Upstream renders NO failure text here and neither do we: the preserved notice
  // is read with its own freshness rule, so a verdict from a PREVIOUS attempt is
  // never passed off as this one's, and no verdict is invented. A start that
  // produced nothing releases its latch — the tap must really retry (upstream can
  // rely on the in-process `engineIsRunning` guard; our counterpart is the
  // extension's own entry gate, which stays). A stop that produced nothing keeps
  // its latch: that stop is still the last word the user asked for, and
  // requestStop() clears it when the platform answers.
  private pendingActionExpired(target: string): void {
    if (target === PENDING_TARGET_DISCONNECTED) {
      hilog.warn(DOMAIN, TAG,
        'UI_PENDING_EXPIRED|bundle=%{public}s|requestId=%{public}s|afterMs=%{public}d|' +
        'target=stop|note=fallback-to-last-reported-state',
        BUNDLE_NAME, this.requestId, PENDING_ACTION_TIMEOUT_MS);
      return;
    }
    this.requestSent = false;
    this.disconnectNotice = '';
    hilog.warn(DOMAIN, TAG,
      'UI_PENDING_EXPIRED|bundle=%{public}s|requestId=%{public}s|afterMs=%{public}d|' +
      'target=connect|note=fallback-to-last-reported-state',
      BUNDLE_NAME, this.requestId, PENDING_ACTION_TIMEOUT_MS);
    this.requestId = `ui-${Date.now()}`;
  }

  // 2026-10-12 upstream tap handling: the transition is over as far as this page can
  // know it (the platform refused the start, or the stop promise resolved), so the
  // tap latch is released at once — upstream's clearPendingAction() for a report that
  // settles the target — instead of leaving the toggle latched for the rest of the
  // window. Nothing else is touched: no verdict is written and no document read.
  private clearPendingAction(): void {
    this.pendingActionTarget = PENDING_TARGET_NONE;
    this.pendingActionDeadlineMs = 0;
    this.updatePendingAction(null);
  }

  // 2026-10-12 upstream forceDisconnect (HomeFragment.java:579-584): abort a
  // transition that outlived the force-cancel delay — stop the engine, whatever it
  // was doing. Stopping also cancels a connect that is parked on an interactive
  // login the user abandoned. The window restarts, so even a stuck disconnect
  // keeps offering the tap again after the delay.
  private forceCancelConnection(): void {
    if (this.stopSent) {
      // already stopping: nothing to issue twice, the window just restarts.
      this.forceCancelArmedAtMs = 0;
      this.forceCancelOpen = false;
      return;
    }
    hilog.info(DOMAIN, TAG,
      'UI_FORCE_CANCEL|bundle=%{public}s|requestId=%{public}s|note=transition-aborted',
      BUNDLE_NAME, this.requestId);
    this.requestStop();
  }

  private requestStop(): void {
    if (!this.requestSent || this.stopSent) {
      // Stop is only meaningful after a start request; one attempt, no retry.
      return;
    }
    // 诊断记录（用户「断开」动作，同一集中处理点）：只记安全事件名 + 计数。
    this.diagActionCount = this.diagActionCount + 1;
    this.diagEvent('ui_disconnect', this.diagActionCount);
    this.stopSent = true;
    // 2026-10-12 upstream tap handling: a stop request is the same pending action
    // as a connect tap — beginPendingAction(DISCONNECTED) + paintTransition(
    // DISCONNECTING) (HomeFragment.java:190-192, :579-584), so the requested state
    // is DISCONNECTED and the force-cancel window restarts from zero.
    this.pendingActionTarget = PENDING_TARGET_DISCONNECTED;
    this.pendingActionDeadlineMs = Date.now() + PENDING_ACTION_TIMEOUT_MS;
    this.forceCancelArmedAtMs = 0;
    this.forceCancelOpen = false;
    // 2026-10-10 后台保活（实验，方案 A）: the connection this borrowed task
    // belonged to is over — the task goes with it, and the next connection starts
    // with a fresh budget. The revision bump discards a creation the platform has
    // not answered yet (the adapter withdraws that task rather than adopting it).
    this.bgKeepAliveState = keepAliveOnUserStop(this.bgKeepAliveState);
    this.stopKeepAliveNow(KEEP_ALIVE_REASON_USER_STOP);
    // 0.0.28: the user themselves ended this connection — the VPN-on intent goes
    // with it (the 8 s user-stop guard answers the next ticks; this latch is what
    // keeps the task from being held again once that guard has lapsed).
    this.vpnIntentOn = false;
    hilog.info(DOMAIN, TAG, 'UI_STOP_REQUEST|bundle=%{public}s|requestId=%{public}s',
      BUNDLE_NAME, this.requestId);
    vpnExtension.stopVpnExtensionAbility(this.buildWant()).then(() => {
      this.requestSent = false;
      this.stopSent = false;
      // 2026-10-12 upstream tap handling: the request reached its target, so the tap
      // latch is released right here instead of on the next 2 s tick (upstream's
      // onEngineStopped → onEngineState(DISCONNECTED) closes the window).
      this.clearPendingAction();
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

  // ---- 2026-10-13 PC 托盘（0.0.29）------------------------------------------
  //
  // The tray menu's action rows, routed into the SAME calls the page's own
  // controls make. The menu codes are the pure module's closed set; anything else
  // is already refused by the tray service, so the page only has to act.
  private onTrayAction(menuCode: string): void {
    hilog.info(DOMAIN, TAG, 'UI_TRAY_ACTION|bundle=%{public}s|menuCode=%{public}s',
      BUNDLE_NAME, menuCode);
    if (menuCode === TRAY_MENU_CONNECT) {
      // Exactly the home card's connect path (a no-op when a request is already
      // in flight, which is the same behaviour the button has).
      this.requestStart();
      return;
    }
    if (menuCode === TRAY_MENU_DISCONNECT) {
      this.requestStop();
      return;
    }
    if (menuCode === TRAY_MENU_QUIT) {
      this.beginExplicitQuit(QUIT_REASON_TRAY);
    }
    // 打开主窗口 never arrives here: the tray service opens the window itself.
  }

  // The EXPLICIT exit — the tray's 退出 row and 设置 → 应用 → 退出应用. See
  // ../tray/NetBirdQuitFlow.ets for the upstream contract this mirrors
  // (client/ui/tray.go handleQuit(): disconnect first by default, then quit) and
  // for the third entry point that shares it (the system-generated tray 退出, via
  // EntryAbility.onPrepareToTerminate → requestQuit(QUIT_REASON_SYSTEM)).
  private beginExplicitQuit(reason: string): void {
    if (this.quitInFlight) {
      // One quit, no second: a repeated click must not issue a second disconnect.
      return;
    }
    this.quitInFlight = true;
    const host: common.UIAbilityContext = getContext(this) as common.UIAbilityContext;
    // The disconnect is the page's own requestStop(), registered with the flow in
    // aboutToAppear (setQuitDisconnect) — the ONE extension-stop call site stays in
    // this file. requestStop() is itself a no-op when nothing was requested (the
    // tunnel is already down), which is exactly the desired "disconnect on quit"
    // semantics. requestQuit() returns before the flow's first await, so the flow
    // is already armed and the stop already issued when this returns.
    requestQuit(host, reason);
  }

  // ---- 诊断日志导出（设置 → 诊断日志）-------------------------------------
  //
  // ONE user action: the system file picker chooses a destination and
  // ../vpnextensionability/NetBirdLogExport copies this app's own logs there.
  // That module owns the fixed candidate allowlist, the caps and the per-file
  // report; the page only (a) hands over the host UIAbilityContext,
  // (b) latches ONE export at a time and (c) renders the returned result
  // WITHOUT inflation: a cap cut, a skipped file or a source read error is
  // 部分导出, never 导出成功. The picked URI is shown to the user and is NEVER
  // hilog-ed (only a boolean "has uri" flag goes to the log).

  // The ONE diagnostic-record exit of this page: a safe event name plus the
  // session's action count. No caller-supplied value, no config, no key.
  private diagEvent(name: string, count: number): void {
    const fields: DiagFields = { 'count': count };
    event(name, fields);
  }

  private startLogExport(): void {
    if (this.logExportBusy) {
      return; // busy: no second export (the button is disabled too)
    }
    const host: common.Context | undefined = this.getUIContext().getHostContext();
    if (host === undefined || host === null) {
      this.logExportTone = 'error';
      this.logExportHeadline = '导出失败：当前页面拿不到应用上下文';
      this.logExportBody = '';
      this.logExportUri = '';
      hilog.error(DOMAIN, TAG, 'UI_LOG_EXPORT_NO_CONTEXT');
      return;
    }
    this.logExportBusy = true;
    this.logExportTone = 'info';
    this.logExportHeadline = '正在导出…（请在系统窗口里选择保存位置）';
    this.logExportBody = '';
    this.logExportUri = '';
    hilog.info(DOMAIN, TAG, 'UI_LOG_EXPORT_STARTED');
    exportDiagnostics(host as common.UIAbilityContext).then((result: LogExportResult): void => {
      this.logExportBusy = false;
      this.renderLogExportResult(result);
    }).catch((error: Error): void => {
      // exportDiagnostics folds every failure into its result and never
      // throws; this net only keeps the busy latch from sticking forever.
      this.logExportBusy = false;
      this.logExportTone = 'error';
      this.logExportHeadline = '导出失败：日志导出内部错误';
      this.logExportBody = '';
      this.logExportUri = '';
      hilog.error(DOMAIN, TAG, 'UI_LOG_EXPORT_THREW|code=%{public}s',
        String((error as BusinessFailure).code ?? 'none'));
    });
  }

  // Honest rendering of the module's result: the outcome decides the headline,
  // the per-file reports decide whether that headline may say 完成 at all, and
  // every line carries the bytes that really came across.
  private renderLogExportResult(result: LogExportResult): void {
    let copiedFiles: number = 0;
    let problemFiles: number = 0;
    let skippedFiles: number = 0;
    const fileLines: string[] = [];
    for (let i = 0; i < result.files.length; i++) {
      const report: LogExportFileReport = result.files[i];
      if (report === undefined || report === null || !report.present) {
        continue;
      }
      if (report.errorCode !== undefined) {
        problemFiles++;
        // A source can fail AFTER some bytes moved: the copied count is then
        // real content in the document, so it is stated instead of "nothing".
        fileLines.push(report.bytes > 0
          ? `${report.name}：读取失败（错误码 ${report.errorCode}），已复制 ${byteText(report.bytes)} 后中断`
          : `${report.name}：读取失败（错误码 ${report.errorCode}），该文件未能导出`);
      } else if (report.skipped) {
        problemFiles++;
        skippedFiles++;
        fileLines.push(`${report.name}：达到总量上限，未导出`);
      } else if (report.truncated) {
        problemFiles++;
        copiedFiles++;
        fileLines.push(`${report.name}：${byteText(report.bytes)}（已截断：达到导出上限，该文件还有更多内容）`);
      } else {
        copiedFiles++;
        fileLines.push(`${report.name}：${byteText(report.bytes)}`);
      }
    }

    const lines: string[] = [];
    if (result.outcome === LOG_EXPORT_OUTCOME_EXPORTED) {
      if (problemFiles > 0) {
        this.logExportTone = 'warn';
        this.logExportHeadline = copiedFiles > 0
          ? `部分导出：${copiedFiles} 个日志文件已写入，共 ${byteText(result.bytes)}；有截断/跳过/读取失败`
          : `部分导出：没有日志文件写入内容（共复制 ${byteText(result.bytes)}）；有截断/跳过/读取失败`;
      } else {
        this.logExportTone = 'ok';
        this.logExportHeadline = `导出完成：${copiedFiles} 个日志文件，共 ${byteText(result.bytes)}`;
      }
      if (fileLines.length > 0) {
        lines.push('本次存在的日志文件（候选共 8 个）：');
        for (let i = 0; i < fileLines.length; i++) {
          lines.push(`· ${fileLines[i]}`);
        }
      } else {
        lines.push('没有任何候选日志文件可读。');
      }
      if (skippedFiles > 0) {
        lines.push('达到总量上限：未复制的排在后面（顺序为界面日志族在前，族内由旧到新）。');
      } else if (result.totalCapHit) {
        lines.push('总量上限刚好用尽，没有整段跳过的日志文件。');
      }
    } else if (result.outcome === LOG_EXPORT_OUTCOME_CANCELLED) {
      this.logExportTone = 'info';
      this.logExportHeadline = '已取消：没有选择保存位置，未写入任何文件';
      lines.push('日志仍完整留在本机。');
    } else if (result.outcome === LOG_EXPORT_OUTCOME_NO_LOGS) {
      this.logExportTone = 'info';
      this.logExportHeadline = '没有可导出的日志：本机暂无日志文件';
      lines.push('界面日志与 VPN 日志都还没有产生（连接并使用一段时间后就会有）。');
    } else if (result.outcome === LOG_EXPORT_OUTCOME_FAILED) {
      this.logExportTone = 'error';
      const code: string = result.code !== undefined ? String(result.code) : '未知';
      this.logExportHeadline = result.uri !== undefined && result.uri.length > 0
        ? `导出失败（错误码 ${code}）：目标文档可能只写入了一部分`
        : `导出失败（错误码 ${code}）：未写入文件`;
      for (let i = 0; i < fileLines.length; i++) {
        lines.push(`· ${fileLines[i]}`);
      }
    } else {
      this.logExportTone = 'warn';
      this.logExportHeadline = `导出结束：未识别的结果（${result.outcome}）`;
    }
    this.logExportBody = lines.join('\n');
    this.logExportUri = result.uri !== undefined && result.uri.length > 0 ? result.uri : '';
    // Numeric/boolean facts only — the URI itself never reaches hilog.
    hilog.info(DOMAIN, TAG,
      'UI_LOG_EXPORT_RESULT|outcome=%{public}s|bytes=%{public}d|files=%{public}d|problems=%{public}d|capHit=%{public}s|hasUri=%{public}s',
      result.outcome, result.bytes, copiedFiles, problemFiles,
      result.totalCapHit ? 'true' : 'false',
      this.logExportUri.length > 0 ? 'true' : 'false');
  }

  // 主题 (2026-10-09): the log-export tone colour from the shared base/dark
  // resources (ok/warn/error/info → success/warning/error/primary).
  private logExportToneColor(): ResourceColor {
    if (this.logExportTone === 'ok') {
      return $r('app.color.nb_status_success');
    }
    if (this.logExportTone === 'warn') {
      return $r('app.color.nb_status_warning');
    }
    if (this.logExportTone === 'error') {
      return $r('app.color.nb_status_error');
    }
    return $r('app.color.nb_primary');
  }

  // ---- shared small widgets -------------------------------------------------

  @Builder
  sectionTitle(title: string) {
    Text(title)
      .fontSize(15)
      .fontWeight(FontWeight.Medium)
      .fontColor($r('app.color.nb_text_secondary'))
      .width('100%')
      .margin({ top: 18, bottom: 6 })
  }

  // 主题 (2026-10-09): the dot takes a resolved colour (Resource or literal) —
  // ResourceColor is exactly what Circle().fill accepts, so a $r reference from
  // stateColor()/peerStatusResource() passes without a string/Resource mismatch.
  @Builder
  statusDot(color: ResourceColor, size: number) {
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
          .fontColor($r('app.color.nb_text_primary'))
          .margin({ top: 12 })
        if (!this.configured) {
          Column({ space: 10 }) {
            Text('尚未配置服务器')
              .fontSize(17)
              .fontWeight(FontWeight.Medium)
              .fontColor($r('app.color.nb_status_warning'))
            Text('请先在「设置」中填写管理服务器地址与 Setup Key，完成后再连接。')
              .fontSize(14)
              .fontColor($r('app.color.nb_text_secondary'))
              .textAlign(TextAlign.Center)
            Button('去设置')
              .fontSize(16)
              .height(40)
              .backgroundColor($r('app.color.nb_primary'))
              .fontColor($r('app.color.nb_on_primary'))
              .onClick(() => {
                this.activeTab = 2;
              })
          }
          .width('100%')
          .padding(16)
          .borderRadius(16)
          .backgroundColor($r('app.color.nb_card_background'))
          .alignItems(HorizontalAlign.Center)
        }
        // 大号连接开关：单一按钮切换连接/断开（含连接中状态）。
        // 主题 (2026-10-09): the label is the EXPLICIT on-primary role — the
        // button previously relied on the platform default font color, which
        // cannot follow the base/dark split (nb_on_primary flips to a dark ink
        // in dark mode because nb_primary flips to a light blue there).
        Button(this.toggleLabel())
          .fontSize(22)
          .fontWeight(FontWeight.Medium)
          .fontColor($r('app.color.nb_on_primary'))
          .width(132)
          .height(132)
          .borderRadius(66)
          .backgroundColor(this.toggleColor())
          .enabled(this.toggleEnabled())
          .onClick(() => {
            // 2026-10-12 upstream tap handling (HomeFragment.java:164-201): a tap
            // during a painted transition does NOT start a new connect — it aborts
            // the stuck transition (force-cancel = stop) once the 2 s window opened.
            if (this.transitionPainted) {
              if (this.forceCancelOpen) {
                this.forceCancelConnection();
              }
              return;
            }
            if (this.toggleOn()) {
              hilog.info(DOMAIN, TAG, 'UI_STOP_BUTTON_CLICKED|bundle=%{public}s', BUNDLE_NAME);
              this.requestStop();
            } else if (this.loginRequired() && this.authMode === ACCOUNT_MODE_ACCOUNT) {
              // 2026-10-13 (upstream HomeFragment.java:193-200): the connect tap is
              // also the sign-in entry. Upstream hands the interactive SSO to the
              // engine start; here the engine has no UI surface, so the UI runs the
              // SAME interactive account login and connects once it succeeded (see
              // finishConnectLogin). Setup-Key peers keep the plain connect below.
              hilog.info(DOMAIN, TAG, 'UI_START_BUTTON_CLICKED|bundle=%{public}s|loginRequired=true',
                BUNDLE_NAME);
              this.beginConnectLogin();
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
              .fontColor($r('app.color.nb_text_muted'))
              .width(96)
            Text(this.homeAddressText().length > 0 ? this.homeAddressText() : '—')
              .fontSize(14)
              .fontColor($r('app.color.nb_text_primary'))
              .layoutWeight(1)
              .maxLines(3)
              .textOverflow({ overflow: TextOverflow.Ellipsis })
          }
          .width('100%')
          .padding({ top: 6, bottom: 6 })
          Row({ space: 8 }) {
            Text('域名')
              .fontSize(13)
              .fontColor($r('app.color.nb_text_muted'))
              .width(96)
            Text(this.homeFqdnText().length > 0 ? this.homeFqdnText() : '—')
              .fontSize(14)
              .fontColor($r('app.color.nb_text_primary'))
              .layoutWeight(1)
              .maxLines(3)
              .textOverflow({ overflow: TextOverflow.Ellipsis })
          }
          .width('100%')
          .padding({ top: 6, bottom: 6 })
          Row({ space: 8 }) {
            Text('对端')
              .fontSize(13)
              .fontColor($r('app.color.nb_text_muted'))
              .width(96)
            Text(this.homePeersText())
              .fontSize(14)
              .fontColor($r('app.color.nb_text_primary'))
              .layoutWeight(1)
              .maxLines(3)
              .textOverflow({ overflow: TextOverflow.Ellipsis })
          }
          .width('100%')
          .padding({ top: 6, bottom: 6 })
          if (this.lastErrorLine().length > 0) {
            Text(this.lastErrorLine())
              .fontSize(13)
              .fontColor($r('app.color.nb_status_error'))
              .width('100%')
              .margin({ top: 6 })
              .maxLines(3)
          }
          // 2026-10-07: discoverable entry to the detailed status screen
          // (netbird status -d equivalent) straight from the main card — the
          // one-line rows above stay concise, the detail is one tap away.
          Row({ space: 8 }) {
            Text('详细状态')
              .fontSize(14)
              .fontColor($r('app.color.nb_text_primary'))
              .layoutWeight(1)
            Text('端点 / 中继 / 流量 / DNS / 路由 / 会话 ›')
              .fontSize(12)
              .fontColor($r('app.color.nb_text_muted'))
              .maxLines(1)
              .textOverflow({ overflow: TextOverflow.Ellipsis })
          }
          .width('100%')
          .padding({ top: 10, bottom: 2 })
          .onClick(() => {
            hilog.info(DOMAIN, TAG, 'UI_DETAIL_STATUS_OPENED|from=home');
            this.detailStatusOriginTab = 0;
            this.showDiagnostics = true;
            this.activeTab = 2;
          })
          // 2026-10-13 会话延长: the session row — the Android `session_expiry_row`
          // (fragment_home.xml:377-427). It exists exactly while a positive
          // deadline is known (HomeFragment.updateSessionRow :901-912 — no lead
          // threshold), shows `formatSessionExpiry`'s label with the constant
          // `session_banner_hint`, and the whole row is the tap target that runs
          // the interactive extend (HomeFragment.java:219-223 →
          // MainActivity.extendSession :1068-1085).
          if (this.sessionRowOffer()) {
            Row({ space: 8 }) {
              Column({ space: 2 }) {
                Text(this.sessionRowText())
                  .fontSize(15)
                  .fontWeight(FontWeight.Medium)
                  .fontColor($r('app.color.nb_text_primary'))
                  .width('100%')
                  .maxLines(2)
                  .textOverflow({ overflow: TextOverflow.Ellipsis })
                Text(this.sessionRowHint())
                  .fontSize(13)
                  .fontColor($r('app.color.nb_text_muted'))
                  .width('100%')
                  .maxLines(2)
              }
              .layoutWeight(1)
              .alignItems(HorizontalAlign.Start)
              Text('›')
                .fontSize(18)
                .fontColor($r('app.color.nb_text_muted'))
            }
            .width('100%')
            .padding({ top: 10, bottom: 2 })
            .enabled(!this.sessionExtendBusy && !this.accountLoginBusy)
            .onClick((): void => {
              hilog.info(DOMAIN, TAG, 'UI_SESSION_ROW_CLICKED|bundle=%{public}s', BUNDLE_NAME);
              this.beginSessionExtend();
            })
          }
        }
        .width('100%')
        .padding(16)
        .borderRadius(16)
        .backgroundColor($r('app.color.nb_card_background'))
        .alignItems(HorizontalAlign.Start)
      }
      .width('100%')
      .padding(20)
    }
    .width('100%')
    .height('100%')
    .backgroundColor($r('app.color.nb_page_background'))
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
          .fontColor($r('app.color.nb_text_primary'))
          .maxLines(1)
          .textOverflow({ overflow: TextOverflow.Ellipsis })
        Text(peer.ip.length > 0 ? peer.ip : '无 IP')
          .fontSize(13)
          .fontColor($r('app.color.nb_text_muted'))
      }
      .layoutWeight(1)
      .alignItems(HorizontalAlign.Start)
      Row({ space: 6 }) {
        if (peer.online) {
          // 主题 (2026-10-09): the 直连/经中继 chip takes its status role from
          // the base/dark resources — same role for the ink and the tint.
          Text(connectionLabel(peer.connection))
            .fontSize(12)
            .fontColor(peer.connection === 'direct'
              ? $r('app.color.nb_status_success')
              : $r('app.color.nb_status_warning'))
            .padding({ left: 8, right: 8, top: 3, bottom: 3 })
            .borderRadius(10)
            .backgroundColor(peer.connection === 'direct'
              ? $r('app.color.nb_status_success_background')
              : $r('app.color.nb_status_warning_background'))
        }
        // 三态状态（2026-10-06）: 已连接=绿、空闲=灰（管理端在线但本机
        // 暂无 WG 会话，不叫离线）、离线=浅灰（管理端 offlinePeers）。
        Text(peerStatusText(peer.status))
          .fontSize(12)
          .fontColor(this.peerStatusResource(peer.status))
        this.statusDot(this.peerStatusResource(peer.status), 10)
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
            .backgroundColor($r('app.color.nb_neutral_button_background'))
            .fontColor($r('app.color.nb_on_primary'))
            .onClick(() => {
              this.closeVisibleSubflow();
            })
          Text('对端详情')
            .fontSize(20)
            .fontWeight(FontWeight.Medium)
            .fontColor($r('app.color.nb_text_primary'))
        }
        .width('100%')
        .margin({ top: 8 })
        // 2026-10-08: the detail card now holds ~16 rows and no longer fits a
        // phone screen, so it lives in its own Scroll (the 返回/标题 row above
        // stays OUTSIDE the Scroll and therefore always reachable). layoutWeight
        // makes the Scroll take exactly the height left over inside the 100%
        // column, instead of growing past it and clipping the tail rows.
        Scroll() {
          Column({ space: 4 }) {
            Row({ space: 10 }) {
              this.statusDot(this.selectedPeerColor(), 12)
              Text(this.selectedPeerName())
                .fontSize(19)
                .fontWeight(FontWeight.Medium)
                .fontColor($r('app.color.nb_text_primary'))
                .layoutWeight(1)
                .maxLines(2)
                .textOverflow({ overflow: TextOverflow.Ellipsis })
            }
            .margin({ bottom: 6 })
            // 2026-10-07 client details: the rows are DetailRow @Prop children
            // driven by selectedPeerFields() (re-read from the polled peers[] by
            // public key), so the endpoint/egress/networks and their honest
            // 未知 states follow the status document instead of freezing at the
            // tap-time snapshot (the old fieldRow @Builder did freeze).
            ForEach(this.selectedPeerFields(), (field: DetailField) => {
              DetailRow({ label: field.label, value: field.value })
            }, (field: DetailField) => `${field.label}=${field.value}`)
          }
          .width('100%')
          .padding(16)
          .borderRadius(16)
          .backgroundColor($r('app.color.nb_card_background'))
          .alignItems(HorizontalAlign.Start)
        }
        .width('100%')
        .layoutWeight(1)
        .align(Alignment.Top)
        .scrollBar(BarState.Off)
      }
      .width('100%')
      .height('100%')
      .padding(20)
      .backgroundColor($r('app.color.nb_page_background'))
    } else {
      Column({ space: 12 }) {
        Row({ space: 10 }) {
          Text('对端')
            .fontSize(24)
            .fontWeight(FontWeight.Bold)
            .fontColor($r('app.color.nb_text_primary'))
            .layoutWeight(1)
          Text(`${onlinePeerCount(this.peersList())}/${this.peersList().length} 已连接`)
            .fontSize(14)
            .fontColor($r('app.color.nb_text_secondary'))
        }
        .width('100%')
        .margin({ top: 8 })
        TextInput({ text: this.peerQuery, placeholder: '搜索对端名称或 IP' })
          .fontSize(15)
          .height(42)
          .width('100%')
          .backgroundColor($r('app.color.nb_card_background'))
          .onChange((value: string): void => {
            this.peerQuery = value;
          })
        if (this.published === null) {
          Text('VPN 未连接，对端信息在连接后可用')
            .fontSize(14)
            .fontColor($r('app.color.nb_text_muted'))
            .margin({ top: 32 })
        } else if (this.visiblePeers().length === 0) {
          Text(this.peerQuery.length > 0 ? '没有匹配的对端' : '暂无对端')
            .fontSize(14)
            .fontColor($r('app.color.nb_text_muted'))
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
          .backgroundColor($r('app.color.nb_card_background'))
          .borderRadius(16)
          .padding({ left: 16, right: 16 })
          .scrollBar(BarState.Off)
        }
      }
      .width('100%')
      .height('100%')
      .padding(20)
      .backgroundColor($r('app.color.nb_page_background'))
    }
  }

  // ---- 设置 -------------------------------------------------------------------

  @Builder
  setupFormCard() {
    Column({ space: 12 }) {
      Text(this.configured ? '更改服务器配置' : '服务器配置')
        .fontSize(16)
        .fontWeight(FontWeight.Medium)
        .fontColor($r('app.color.nb_text_primary'))
        .width('100%')
      TextInput({ text: this.urlInput, placeholder: '服务器地址（https://…）' })
        .fontSize(15)
        .height(44)
        .width('100%')
        .backgroundColor($r('app.color.nb_input_background'))
        .enabled(!this.accountModeSwitchBlocked())
        .onChange((value: string): void => {
          this.urlInput = value;
        })
      Row({ space: 12 }) {
        // 主题 (2026-10-09): mode buttons — selected carries the primary role
        // with the EXPLICIT on-primary ink, unselected the card role with the
        // primary ink; the outline is the primary role in both states.
        Button('账户登录')
          .fontSize(14)
          .height(40)
          .layoutWeight(1)
          .backgroundColor(this.authMode === ACCOUNT_MODE_ACCOUNT
            ? $r('app.color.nb_primary') : $r('app.color.nb_card_background'))
          .fontColor(this.authMode === ACCOUNT_MODE_ACCOUNT
            ? $r('app.color.nb_on_primary') : $r('app.color.nb_primary'))
          .border({ width: 1, color: $r('app.color.nb_primary') })
          .enabled(!this.accountModeSwitchBlocked() && !this.accountLoginBusy)
          .onClick(() => this.selectAuthMode(ACCOUNT_MODE_ACCOUNT))
        Button('Setup Key')
          .fontSize(14)
          .height(40)
          .layoutWeight(1)
          .backgroundColor(this.authMode === ACCOUNT_MODE_SETUP_KEY
            ? $r('app.color.nb_primary') : $r('app.color.nb_card_background'))
          .fontColor(this.authMode === ACCOUNT_MODE_SETUP_KEY
            ? $r('app.color.nb_on_primary') : $r('app.color.nb_primary'))
          .border({ width: 1, color: $r('app.color.nb_primary') })
          .enabled(!this.accountModeSwitchBlocked() && !this.accountLoginBusy)
          .onClick(() => this.selectAuthMode(ACCOUNT_MODE_SETUP_KEY))
      }
      .width('100%')
      if (this.authMode === ACCOUNT_MODE_SETUP_KEY) {
        TextInput({ text: this.keyInput, placeholder: 'Setup Key' })
          .fontSize(15)
          .height(44)
          .width('100%')
          .backgroundColor($r('app.color.nb_input_background'))
          .enabled(!this.accountModeSwitchBlocked())
          .type(InputType.Password)
          .onChange((value: string): void => {
            this.keyInput = value;
          })
      } else if (this.accountLoginBusy) {
        Text(this.accountLoginCommitting ? '正在保存账户凭据…' : '正在验证账户…')
          .fontSize(13)
          .fontColor($r('app.color.nb_text_tertiary'))
          .width('100%')
        if (!this.accountLoginCommitting) {
          Button('取消登录')
            .fontSize(14)
            .height(36)
            .backgroundColor($r('app.color.nb_neutral_button_background'))
            .fontColor($r('app.color.nb_on_primary'))
            .onClick((): void => this.cancelAccountLogin(false))
        }
      }
      if (this.accountModeSwitchBlocked()) {
        Text('断开 VPN 后可更改配置')
          .fontSize(13)
          .fontColor($r('app.color.nb_status_warning'))
          .width('100%')
      }
      if (this.accountLoginError.length > 0) {
        Text(this.accountLoginError)
          .fontSize(13)
          .fontColor($r('app.color.nb_status_error'))
          .width('100%')
      }
      if (this.formError.length > 0) {
        Text(this.formError)
          .fontSize(13)
          .fontColor($r('app.color.nb_status_error'))
          .width('100%')
      }
      if (this.formHint.length > 0) {
        Text(this.formHint)
          .fontSize(13)
          .fontColor($r('app.color.nb_status_warning'))
          .width('100%')
          .maxLines(2)
      }
      Button(this.authMode === ACCOUNT_MODE_ACCOUNT
        ? (this.accountLoginBusy ? '正在登录…' : '登录并连接')
        : '保存并连接')
        .fontSize(15)
        .height(42)
        .width('100%')
        .backgroundColor($r('app.color.nb_primary'))
        .fontColor($r('app.color.nb_on_primary'))
        .enabled(!this.accountLoginBusy && !this.accountModeSwitchBlocked())
        .onClick(() => {
          hilog.info(DOMAIN, TAG, 'UI_SETUP_SAVE_CLICKED|configured=%{public}s',
            String(this.configured));
          if (this.authMode === ACCOUNT_MODE_ACCOUNT) {
            this.beginAccountLogin();
          } else {
            this.saveSetup();
          }
        })
      if (this.configured) {
        Button('取消')
          .fontSize(14)
          .height(36)
          .width('100%')
          .backgroundColor($r('app.color.nb_card_background'))
          .fontColor($r('app.color.nb_text_primary'))
          .border({ width: 1, color: $r('app.color.nb_border') })
          .enabled(!this.accountLoginCommitting)
          .onClick(() => this.cancelConfigurationEdit())
      }
    }
    .width('100%')
    .padding(16)
    .borderRadius(8)
    .backgroundColor($r('app.color.nb_card_background'))
    .alignItems(HorizontalAlign.Start)
  }

  @Builder
  serverConfigCard() {
    Column({ space: 10 }) {
      Text('当前配置')
        .fontSize(16)
        .fontWeight(FontWeight.Medium)
        .fontColor($r('app.color.nb_text_primary'))
        .width('100%')
      Text('服务器地址')
        .fontSize(12)
        .fontColor($r('app.color.nb_text_muted'))
        .width('100%')
      Text(this.configuredUrl)
        .fontSize(14)
        .fontColor($r('app.color.nb_text_secondary'))
        .width('100%')
        .maxLines(3)
        .textOverflow({ overflow: TextOverflow.Ellipsis })
      Text('登录方式')
        .fontSize(12)
        .fontColor($r('app.color.nb_text_muted'))
        .width('100%')
      Text(this.authMode === ACCOUNT_MODE_ACCOUNT ? '账户登录' : 'Setup Key')
        .fontSize(14)
        .fontColor($r('app.color.nb_text_secondary'))
        .width('100%')
      if (this.authMode === ACCOUNT_MODE_ACCOUNT) {
        Text('账户凭据过期时，可重新登录。')
          .fontSize(13)
          .fontColor($r('app.color.nb_text_tertiary'))
          .width('100%')
      }
      Button('更改配置')
        .fontSize(15)
        .height(40)
        .width('100%')
        .backgroundColor($r('app.color.nb_primary'))
        .fontColor($r('app.color.nb_on_primary'))
        .enabled(!this.accountModeSwitchBlocked())
        .onClick(() => this.beginConfigurationEdit())
      if (this.accountModeSwitchBlocked()) {
        Text('断开 VPN 后可更改配置')
          .fontSize(13)
          .fontColor($r('app.color.nb_status_warning'))
          .width('100%')
      }
    }
    .width('100%')
    .padding(16)
    .borderRadius(8)
    .backgroundColor($r('app.color.nb_card_background'))
    .alignItems(HorizontalAlign.Start)
  }

  @Builder
  logoutAction() {
    Text(this.logoutRemoteActive() ? '注销并从管理端移除本设备' : '清除本机配置')
      .fontSize(14)
      .fontColor($r('app.color.nb_status_error'))
      .width('100%')
      .padding({ left: 8, right: 8, top: 8, bottom: 8 })
      .onClick(() => {
        hilog.info(DOMAIN, TAG, 'UI_SETUP_LOGOUT_CLICKED');
        this.requestLogout();
      })
  }

  private logoutRemoteActive(): boolean {
    return this.published !== null && vpnActiveState(this.published.state);
  }

  // 诊断日志 (settings tab, 2026-10-05): the local-only log families, their
  // rotation, and the ONE export action. The wording below must stay tied to
  // the real writers: the UI writer keeps 4 x 512 KiB ≈ 2 MiB (the diagnostics
  // module's own cap), the native/VPN writer keeps 4 x 2 MiB ≈ 8 MiB, so the
  // local total is about 10 MiB; the newest segment replaces the oldest. The
  // export only COPIES: each file is read up to 2 MiB and the whole export up
  // to 12 MiB, and a cut is marked in the document (and in the result below).
  // A killed process can lose the records its writer had not flushed yet.
  @Builder
  diagnosticsLogCard() {
    Column({ space: 10 }) {
      Text('日志只写在本机应用目录，不会自动上传。')
        .fontSize(13)
        .fontColor($r('app.color.nb_text_secondary'))
        .width('100%')
      Text('滚动保留：界面日志 4 段 × 512 KiB ≈ 2 MiB，VPN/原生日志 4 段 × 2 MiB ≈ 8 MiB，' +
        '合计约 10 MiB；写满后最旧的段被覆盖，另外每 60 秒记录一次本进程内存采样。')
        .fontSize(13)
        .fontColor($r('app.color.nb_text_secondary'))
        .width('100%')
      Text('导出只做复制（不改动本机日志）：导出期间日志仍在写，每个文件最多取 2 MiB、总量最多 12 MiB，' +
        '超出部分在文件里标记为截断；系统强制结束进程时，还没落盘的日志尾部可能丢失。')
        .fontSize(13)
        .fontColor($r('app.color.nb_text_secondary'))
        .width('100%')
      Button(this.logExportBusy ? '正在导出…' : '导出日志')
        .fontSize(15)
        .height(38)
        .width('100%')
        .backgroundColor(this.logExportBusy
          ? $r('app.color.nb_neutral_button_background') : $r('app.color.nb_primary'))
        .fontColor($r('app.color.nb_on_primary'))
        .enabled(!this.logExportBusy)
        .onClick(() => {
          // The picked URI is user data: only this marker is logged.
          hilog.info(DOMAIN, TAG, 'UI_LOG_EXPORT_BUTTON_CLICKED');
          this.startLogExport();
        })
      if (this.logExportHeadline.length > 0) {
        Text(this.logExportHeadline)
          .fontSize(14)
          .fontWeight(FontWeight.Medium)
          .fontColor(this.logExportToneColor())
          .width('100%')
      }
      if (this.logExportBody.length > 0) {
        Text(this.logExportBody)
          .fontSize(12)
          .fontColor($r('app.color.nb_text_secondary'))
          .width('100%')
      }
      if (this.logExportUri.length > 0) {
        Text(`目标文档：${this.logExportUri}`)
          .fontSize(12)
          .fontColor($r('app.color.nb_text_muted'))
          .width('100%')
      }
    }
    .width('100%')
    .padding(16)
    .borderRadius(16)
    .backgroundColor($r('app.color.nb_card_background'))
    .alignItems(HorizontalAlign.Start)
  }

  // ---- 详细状态 (netbird status -d equivalent) ------------------------------
  //
  // Five collapsible sections, each one driven by a *method call* on the
  // polled published document (never a frozen @Builder value argument) and
  // rendered through DetailRow @Prop children. Content lives in the settings
  // scroll: the page grows, the outer Scroll scrolls (no fixed height).
  // Reachable from the home card (详细状态 ›) and from 设置 → 详细状态.

  @Builder
  detailStatusView() {
    Column({ space: 4 }) {
      Row({ space: 12 }) {
        Button('返回')
          .fontSize(15)
          .height(36)
          .backgroundColor($r('app.color.nb_neutral_button_background'))
          .fontColor($r('app.color.nb_on_primary'))
          .onClick(() => {
            this.closeVisibleSubflow();
          })
        Text('详细状态')
          .fontSize(20)
          .fontWeight(FontWeight.Medium)
          .fontColor($r('app.color.nb_text_primary'))
      }
      .width('100%')
      .margin({ top: 8, bottom: 8 })

      if (this.published === null) {
        Column({ space: 4 }) {
          Text('当前没有可用的状态文档（VPN 未运行，或状态已过期）。')
            .fontSize(13)
            .fontColor($r('app.color.nb_status_warning'))
            .width('100%')
          if (this.lastErrorLine().length > 0) {
            Text(this.lastErrorLine())
              .fontSize(13)
              .fontColor($r('app.color.nb_status_error'))
              .width('100%')
              .maxLines(3)
          }
        }
        .width('100%')
        .padding(12)
        .borderRadius(12)
        .backgroundColor($r('app.color.nb_status_warning_background'))
        .alignItems(HorizontalAlign.Start)
      }

      // ---- 连接概览 ----
      DetailSectionHeader({
        title: '连接概览',
        summary: this.overviewSummary(),
        expanded: this.detailOverviewOpen,
        onToggle: () => {
          this.detailOverviewOpen = !this.detailOverviewOpen;
        }
      })
      if (this.detailOverviewOpen) {
        Column({ space: 0 }) {
          ForEach(this.overviewFields(), (field: DetailField) => {
            DetailRow({ label: field.label, value: field.value })
          }, (field: DetailField) => `ov|${field.label}=${field.value}`)
        }
        .width('100%')
        .padding({ left: 16, right: 16, top: 4, bottom: 12 })
        .borderRadius(16)
        .backgroundColor($r('app.color.nb_card_background'))
      }

      // ---- 本机网络 ----
      DetailSectionHeader({
        title: '本机网络（地址 / DNS / 路由）',
        summary: this.networkSummary(),
        expanded: this.detailNetworkOpen,
        onToggle: () => {
          this.detailNetworkOpen = !this.detailNetworkOpen;
        }
      })
      if (this.detailNetworkOpen) {
        Column({ space: 0 }) {
          ForEach(this.networkFields(), (field: DetailField) => {
            DetailRow({ label: field.label, value: field.value })
          }, (field: DetailField) => `nw|${field.label}=${field.value}`)
        }
        .width('100%')
        .padding({ left: 16, right: 16, top: 4, bottom: 12 })
        .borderRadius(16)
        .backgroundColor($r('app.color.nb_card_background'))
      }

      // ---- 流量统计 ----
      DetailSectionHeader({
        title: '流量统计',
        summary: this.trafficSummary(),
        expanded: this.detailTrafficOpen,
        onToggle: () => {
          this.detailTrafficOpen = !this.detailTrafficOpen;
        }
      })
      if (this.detailTrafficOpen) {
        Column({ space: 0 }) {
          ForEach(this.trafficFields(), (field: DetailField) => {
            DetailRow({ label: field.label, value: field.value })
          }, (field: DetailField) => `tf|${field.label}=${field.value}`)
        }
        .width('100%')
        .padding({ left: 16, right: 16, top: 4, bottom: 12 })
        .borderRadius(16)
        .backgroundColor($r('app.color.nb_card_background'))
      }

      // ---- 中继 ----
      DetailSectionHeader({
        title: '中继',
        summary: this.relaySummary(),
        expanded: this.detailRelayOpen,
        onToggle: () => {
          this.detailRelayOpen = !this.detailRelayOpen;
        }
      })
      if (this.detailRelayOpen) {
        Column({ space: 0 }) {
          ForEach(this.relayFields(), (field: DetailField) => {
            DetailRow({ label: field.label, value: field.value })
          }, (field: DetailField) => `rl|${field.label}=${field.value}`)
        }
        .width('100%')
        .padding({ left: 16, right: 16, top: 4, bottom: 12 })
        .borderRadius(16)
        .backgroundColor($r('app.color.nb_card_background'))
      }

      // ---- 对端明细 ----
      DetailSectionHeader({
        title: '对端明细',
        summary: this.peersSummary(),
        expanded: this.detailPeersOpen,
        onToggle: () => {
          this.detailPeersOpen = !this.detailPeersOpen;
        }
      })
      if (this.detailPeersOpen) {
        Column({ space: 0 }) {
          // 2026-10-08: every row here carries a WHOLE per-peer multi-line block
          // (WG status block = 11 logical lines incl. 发送/接收（明文）, ICE block
          // = 6), so the default 8-line cap truncated the tail of the WG block.
          // The outer Scroll absorbs the extra height; other sections keep 8.
          ForEach(this.peerFields(), (field: DetailField) => {
            DetailRow({ label: field.label, value: field.value, maxLines: DETAIL_BLOCK_MAX_LINES })
          }, (field: DetailField) => `pe|${field.label}=${field.value}`)
          // 2026-10-07: the per-peer 发送/接收 lines above count the peer's
          // PLAINTEXT inner-IP payload bytes — the aggregate 流量统计 发送 row
          // counts ENCAPSULATED wire bytes, so the two must not be read as one.
          Text('对端发送/接收为明文 IP 载荷字节（自该对端隧道建立以来累计）；' +
            '「流量统计」的发送计加密后的传输字节，口径不同。')
            .fontSize(12)
            .fontColor($r('app.color.nb_text_muted'))
            .width('100%')
            .padding({ top: 8 })
          // 2026-10-08: the per-peer ICE blocks are candidate-path details —
          // the WG-session rows above (状态 / 连接方式 / 承载 / 中继通道地址)
          // stay the actual bearer, and ICE 已连接 never means 流量直连.
          Text('ICE 明细只描述候选路径与端点应用情况（断开后仍可能保留上次选中的地址）；' +
            '各对端实际承载以其「状态 / 连接方式 / 承载 / 中继通道地址」为准，' +
            'ICE 已连接不代表当前流量直连。')
            .fontSize(12)
            .fontColor($r('app.color.nb_text_muted'))
            .width('100%')
            .padding({ top: 8 })
          Row({ space: 8 }) {
            Text('点按「对端」标签页可查看并搜索单个对端')
              .fontSize(12)
              .fontColor($r('app.color.nb_text_muted'))
              .layoutWeight(1)
            Text('前往 ›')
              .fontSize(13)
              .fontColor($r('app.color.nb_text_primary'))
          }
          .width('100%')
          .padding({ top: 10, bottom: 2 })
          .onClick(() => {
            this.showDiagnostics = false;
            this.activeTab = 1;
          })
        }
        .width('100%')
        .padding({ left: 16, right: 16, top: 4, bottom: 12 })
        .borderRadius(16)
        .backgroundColor($r('app.color.nb_card_background'))
      }
    }
    .width('100%')
    .padding({ bottom: 24 })
    .alignItems(HorizontalAlign.Start)
  }

  @Builder
  settingsTab() {
    Scroll() {
      Column({ space: 0 }) {
        Text('设置')
          .fontSize(24)
          .fontWeight(FontWeight.Bold)
          .fontColor($r('app.color.nb_text_primary'))
          .width('100%')
          .margin({ top: 8, bottom: 4 })
        if (this.showDiagnostics) {
          this.detailStatusView()
        } else if (this.routeSelectionOpen) {
          // 路由选择: an internal SETTINGS page (back returns to 设置).
          this.routeSelectionView()
        } else {
          this.sectionTitle('服务器配置')
          if (this.showSetupForm || !this.configured) {
            this.setupFormCard()
          } else {
            this.serverConfigCard()
            this.logoutAction()
          }
          // 2026-10-10 后台保活（实验，方案 A）: always visible — a user who wants
          // this off must be able to say so without credentials in place.
          this.sectionTitle('后台保活（实验）')
          this.keepAliveCard()
          this.sectionTitle('关于')
          Column() {
            DetailRow({ label: '版本', value: this.versionName.length > 0 ? this.versionName : '未知' })
            DetailRow({ label: '版本号', value: this.versionCode >= 0 ? this.versionCode.toString() : '未知' })
          }
          .width('100%')
          .padding({ left: 16, right: 16, top: 10, bottom: 10 })
          .borderRadius(16)
          .backgroundColor($r('app.color.nb_card_background'))
          .alignItems(HorizontalAlign.Start)
          this.sectionTitle('诊断日志')
          this.diagnosticsLogCard()
          this.sectionTitle('详细状态')
          Row({ space: 8 }) {
            Text('详细状态')
              .fontSize(16)
              .fontColor($r('app.color.nb_text_primary'))
              .layoutWeight(1)
            Text('›')
              .fontSize(18)
              .fontColor($r('app.color.nb_text_muted'))
          }
          .width('100%')
          .padding(16)
          .borderRadius(16)
          .backgroundColor($r('app.color.nb_card_background'))
          .onClick(() => {
            this.detailStatusOriginTab = 2;
            this.showDiagnostics = true;
          })
          this.sectionTitle('路由选择')
          Row({ space: 8 }) {
            Text('路由选择')
              .fontSize(16)
              .fontColor($r('app.color.nb_text_primary'))
              .layoutWeight(1)
            Text(this.routeSummaryText())
              .fontSize(12)
              .fontColor($r('app.color.nb_text_muted'))
              .maxLines(1)
              .textOverflow({ overflow: TextOverflow.Ellipsis })
            Text('›')
              .fontSize(18)
              .fontColor($r('app.color.nb_text_muted'))
          }
          .width('100%')
          .padding(16)
          .borderRadius(16)
          .backgroundColor($r('app.color.nb_card_background'))
          .onClick(() => {
            this.openRouteSelection();
          })
          // 2026-10-13 PC 托盘（0.0.29）；2026-10-13 统一退出（0.0.31）: the explicit
          // exit, the same flow the tray's 退出 row and the system-generated tray
          // 退出 run. It is deliberately NOT "close the window" — this app's window
          // X hides to the background and keeps the tunnel up (see
          // EntryAbility.registerCloseToBackground), so the only way to really
          // leave AND disconnect must be visible in the app itself.
          this.sectionTitle('应用')
          Row({ space: 8 }) {
            Text('退出应用（断开 VPN）')
              .fontSize(16)
              .fontColor($r('app.color.nb_text_primary'))
              .layoutWeight(1)
            Text('›')
              .fontSize(18)
              .fontColor($r('app.color.nb_text_muted'))
          }
          .width('100%')
          .padding(16)
          .borderRadius(16)
          .backgroundColor($r('app.color.nb_card_background'))
          .onClick(() => {
            this.beginExplicitQuit(QUIT_REASON_SETTINGS);
          })
          Text('关闭窗口只是隐藏到后台，连接保持；「退出应用」和状态栏图标菜单里的「退出」都会先断开 VPN 并停止后台保活，再结束应用。')
            .fontSize(12)
            .fontColor($r('app.color.nb_text_muted'))
            .width('100%')
            .margin({ top: 6 })
        }
      }
      .width('100%')
      .padding(20)
    }
    .width('100%')
    .height('100%')
    .backgroundColor($r('app.color.nb_page_background'))
    .align(Alignment.Top)
    .scrollBar(BarState.Off)
  }

  // ---- 路由选择 (设置 → 路由选择, internal page) ------------------------------
  //
  // The page is a SETTINGS page: 返回 and the system back both close it back to
  // 设置 (closeVisibleSubflow), never to the desktop. Every colour comes from
  // the shared base/dark palette; empty/error states are plain Chinese.
  //
  // 即时生效: every switch writes through at once, so there is no 保存 / 放弃更改
  // button and no draft to discard. The switch is bound to the LATEST DESIRED
  // selection, which a catalog refresh never touches.

  @Builder
  routeRow(entry: RouteNetworkEntry) {
    Row({ space: 12 }) {
      Column({ space: 2 }) {
        Text(routeNetName(entry.net_id))
          .fontSize(15)
          .fontColor($r('app.color.nb_text_primary'))
          .maxLines(1)
          .textOverflow({ overflow: TextOverflow.Ellipsis })
        Text(routePrefixesText(entry.networks))
          .fontSize(12)
          .fontColor($r('app.color.nb_text_secondary'))
          .maxLines(2)
          .textOverflow({ overflow: TextOverflow.Ellipsis })
        Text(routeCandidateText(entry.candidate_count))
          .fontSize(12)
          .fontColor($r('app.color.nb_text_muted'))
      }
      .layoutWeight(1)
      .alignItems(HorizontalAlign.Start)
      Toggle({ type: ToggleType.Switch,
        isOn: routeSelectionIsEnabled(this.routeDraft, entry.net_id) })
        .selectedColor($r('app.color.nb_primary'))
        .enabled(!this.routeLoadError)
        .onChange((isOn: boolean): void => {
          // The callback carries the NEW switch value: apply the change only on
          // a REAL change. A re-emitted value the desired selection already has
          // (re-render, duplicate platform event, a programmatic callback) must
          // not blind-flip this network — the guard runs BEFORE any flip.
          if (isOn !== routeSelectionIsEnabled(this.routeDraft, entry.net_id)) {
            this.routeToggle(entry.net_id);
          }
        })
    }
    .width('100%')
    .padding({ top: 10, bottom: 10 })
  }

  @Builder
  routeSelectionView() {
    Column({ space: 10 }) {
      Row({ space: 12 }) {
        Button('返回')
          .fontSize(15)
          .height(36)
          .backgroundColor($r('app.color.nb_neutral_button_background'))
          .fontColor($r('app.color.nb_on_primary'))
          .onClick(() => {
            this.closeVisibleSubflow();
          })
        Text('路由选择')
          .fontSize(20)
          .fontWeight(FontWeight.Medium)
          .fontColor($r('app.color.nb_text_primary'))
      }
      .width('100%')
      .margin({ top: 8 })

      Text('按网络（NetID）选择要走隧道的 IPv4 路由；同一网络的高可用候选始终一起开关。' +
        '改动立即保存并通知当前连接，离线时下次连接生效；切换瞬间可能短暂中断数据。' +
        '当前版本不处理 DNS / IPv6，也不自动调整出口节点。')
        .fontSize(13)
        .fontColor($r('app.color.nb_text_secondary'))
        .width('100%')
      Text(`管理服务器：${this.configuredUrl}`)
        .fontSize(12)
        .fontColor($r('app.color.nb_text_muted'))
        .width('100%')
        .maxLines(2)
        .textOverflow({ overflow: TextOverflow.Ellipsis })
      if (this.routeCatalogNote().length > 0) {
        Text(this.routeCatalogNote())
          .fontSize(12)
          .fontColor($r('app.color.nb_text_muted'))
          .width('100%')
      }

      Row({ space: 12 }) {
        Button('全部启用')
          .fontSize(14)
          .height(38)
          .layoutWeight(1)
          .backgroundColor($r('app.color.nb_card_background'))
          .fontColor($r('app.color.nb_text_primary'))
          .border({ width: 1, color: $r('app.color.nb_border') })
          .enabled(!this.routeLoadError)
          .onClick(() => this.routeEnableAll())
        Button('全部停用')
          .fontSize(14)
          .height(38)
          .layoutWeight(1)
          .backgroundColor($r('app.color.nb_card_background'))
          .fontColor($r('app.color.nb_text_primary'))
          .border({ width: 1, color: $r('app.color.nb_border') })
          .enabled(!this.routeLoadError)
          .onClick(() => this.routeDisableAll())
      }
      .width('100%')
      Text('全部停用也适用于以后新增的路由')
        .fontSize(12)
        .fontColor($r('app.color.nb_text_muted'))
        .width('100%')

      if (this.routeLoadError) {
        Text(this.routeLoadErrorText)
          .fontSize(13)
          .fontColor($r('app.color.nb_status_error'))
          .width('100%')
      }

      if (this.routeCatalog === null) {
        Text(this.routeEmptyText())
          .fontSize(14)
          .fontColor($r('app.color.nb_text_muted'))
          .width('100%')
          .margin({ top: 20 })
      } else if (this.routeRows().length === 0) {
        Text(this.routeNoRowsText())
          .fontSize(14)
          .fontColor($r('app.color.nb_text_muted'))
          .width('100%')
          .margin({ top: 20 })
      } else {
        this.sectionTitle('普通路由')
        Column({ space: 0 }) {
          ForEach(this.routeSectionRows(false), (entry: RouteNetworkEntry) => {
            this.routeRow(entry)
          }, (entry: RouteNetworkEntry) => `rt|${entry.net_id}`)
        }
        .width('100%')
        .padding({ left: 16, right: 16, top: 4, bottom: 4 })
        .borderRadius(16)
        .backgroundColor($r('app.color.nb_card_background'))
        if (this.routeSectionRows(true).length > 0) {
          this.sectionTitle('出口节点')
          Column({ space: 0 }) {
            ForEach(this.routeSectionRows(true), (entry: RouteNetworkEntry) => {
              this.routeRow(entry)
            }, (entry: RouteNetworkEntry) => `rx|${entry.net_id}`)
          }
          .width('100%')
          .padding({ left: 16, right: 16, top: 4, bottom: 4 })
          .borderRadius(16)
          .backgroundColor($r('app.color.nb_card_background'))
        }
      }

      // 改动已经在开关动作里落盘并通知，因此这里只有一行如实的进度/结果，没有
      // 「保存」/「放弃更改」按钮，也没有任何停止/启动动作。
      if (this.routeStatusText().length > 0) {
        Text(this.routeStatusText())
          .fontSize(13)
          .fontColor(this.routeStatusColor())
          .width('100%')
      }
    }
    .width('100%')
    .padding({ bottom: 24 })
    .alignItems(HorizontalAlign.Start)
  }

  // ---- 路由选择页的数据模型（页面方法）----------------------------------------
  //
  // One row per management NetID (the HA group), never per route id/peer: the
  // catalog already collapses the same-NetID candidates into one entry, and the
  // row shows that network's subnet(s) plus its 节点数 (candidate_count). The
  // list is whatever the extension last wrote — while no live document exists
  // it is explicitly labelled 上次连接获取 and never claimed as 已生效.

  private openRouteSelection(): void {
    hilog.info(DOMAIN, TAG, 'UI_ROUTE_SELECTION_OPENED');
    this.routeSelectionOpen = true;
    this.routeActionError = '';
    this.routeSaveFailed = false;
    this.routeLoadError = false;
    this.routeLoadErrorText = '';
    // 重新进入不等于「用户没动过」：只要还有在途写入或在途读取，这次打开的存储文档就
    // 一定比内存里的期望旧，绝不能把编辑保护归零——旧答案会盖掉用户刚移动的开关。
    if (!this.routeWriteBusy && !this.routeSelectionLoading) {
      this.routeUserEdited = false;
    }
    // routeNotifyFailed is deliberately NOT cleared here: it belongs to the request
    // this page already wrote (the doorbell failed), not to one visit of the page.
    // Clearing it on re-entry would show 正在应用… forever without a notification.
    this.loadRouteSelection();
    this.refreshRouteCatalog();
    // A result already on disk is only adopted if its id matches the newest
    // request this page knows about; the id is learned from the request the
    // page itself wrote, so an idle page claims nothing.
    this.refreshRouteApplyResult();
  }

  // The persisted selection for THIS management server — the SAVED URL
  // (`configuredUrl`), never the unsaved editor field. A missing file (or a
  // document scoped to another server) is the default all-on state; a malformed
  // or wrong-version document is a LOUD error state, never a silent default
  // that the next save would overwrite.
  private loadRouteSelection(): void {
    const managementUrl: string = this.configuredUrl;
    if (managementUrl.length === 0 || this.routeSelectionLoading || this.routeWriteBusy) {
      // An in-flight WRITE means the in-memory desired selection is both newer
      // than the disk and the thing the user is looking at: reading the file can
      // only produce an older answer, so re-entry must not even ask for one.
      return;
    }
    this.routeSelectionLoading = true;
    loadRouteSelection(getContext(this).filesDir, managementUrl)
      .then((stored: RouteSelection | null): void => {
        this.routeSelectionLoading = false;
        if (this.routeUserEdited) {
          // a switch was already moved while this read was in flight: the stored
          // document is older than the user's wish and must not clobber it
          return;
        }
        this.routeDraft = stored === null
          ? routeSelectionDefault() : routeSelectionClone(stored);
        this.routeLoadError = false;
        this.routeLoadErrorText = '';
      })
      .catch((_error: Error): void => {
        this.routeSelectionLoading = false;
        this.routeLoadError = true;
        this.routeLoadErrorText =
          '路由选择记录无法读取，已按默认（全部启用）显示；为避免覆盖该记录，暂时不能修改。';
        this.routeDraft = routeSelectionDefault();
      });
  }

  // Offline catalog read (no native call), driven by the page's EXISTING 2 s
  // poll while the page is open plus one immediate read on open. It updates the
  // list/status/stamp only — the LATEST DESIRED selection is never touched here,
  // so a refresh can never discard a change the user just made.
  private refreshRouteCatalog(): void {
    const managementUrl: string = this.configuredUrl;
    if (managementUrl.length === 0 || this.routeCatalogReading) {
      return;
    }
    this.routeCatalogReading = true;
    readRouteCatalog(getContext(this).filesDir, managementUrl)
      .then((result: RouteCatalogReadResult): void => {
        this.routeCatalogReading = false;
        this.routeCatalogStatus = result.status;
        this.routeCatalogUrl = result.management_url;
        this.routeCatalogUpdatedAtMs = result.updated_at_ms;
        this.routeCatalog = result.catalog;
      })
      .catch((_error: Error): void => {
        // readRouteCatalog answers a status token instead of throwing; this is
        // only the defensive net — nothing is ever claimed from a failed read.
        this.routeCatalogReading = false;
        this.routeCatalogStatus = CATALOG_STATUS_UNAVAILABLE;
        this.routeCatalogUrl = '';
        this.routeCatalogUpdatedAtMs = 0;
        this.routeCatalog = null;
      });
  }

  // The VPE's apply RESULT, read on the same existing 2 s tick. ONLY a document
  // whose opaque id equals the newest request id THIS page wrote is adopted: an
  // older generation's answer, an absent document and the catalog's own
  // `enabled` verdict are never turned into 已生效. A read problem is simply
  // "we do not know yet" and changes nothing.
  private refreshRouteApplyResult(): void {
    const managementUrl: string = this.configuredUrl;
    if (managementUrl.length === 0 || this.routeResultReading) {
      return;
    }
    this.routeResultReading = true;
    readRouteApplyResult(getContext(this).filesDir)
      .then((result: RouteApplyResult | null): void => {
        this.routeResultReading = false;
        if (result === null || this.routeRequestId.length === 0 ||
          result.id !== this.routeRequestId) {
          return;
        }
        this.routeResultId = result.id;
        this.routeResultState = result.state;
        // The ONE reading of "the OS side really is at the requested state": a
        // `stored-no-map` answer stores the preference without handing a map to
        // the OS, so it is NOT applied even though its state says `applied`.
        this.routeResultApplied = routeApplyResultIsApplied(result);
        this.routeResultError = result.error;
      })
      .catch((_error: Error): void => {
        this.routeResultReading = false;
      });
  }

  private routeRows(): RouteNetworkEntry[] {
    const catalog: RouteCatalog | null = this.routeCatalog;
    if (catalog === null) {
      return [];
    }
    return routeSelectionRows(this.routeDraft, catalog);
  }

  // Catalog order, split into the ordinary list and the exit-node list. A
  // section with no entry is simply not rendered (no empty header).
  private routeSectionRows(isExit: boolean): RouteNetworkEntry[] {
    const rows: RouteNetworkEntry[] = [];
    for (const row of this.routeRows()) {
      if (row.is_exit === isExit) {
        rows.push(row);
      }
    }
    return rows;
  }

  private routeEnabledCount(): number {
    let enabled: number = 0;
    for (const row of this.routeRows()) {
      if (row.enabled) {
        enabled++;
      }
    }
    return enabled;
  }

  private routeSummaryText(): string {
    if (this.routeCatalog === null) {
      return '连接后可用';
    }
    return `已启用 ${this.routeEnabledCount()}/${this.routeRows().length}`;
  }

  // The freshness wording. A live document means the catalog belongs to the
  // current session; without one the list is the LAST session's view and is
  // labelled exactly that. The stored preference alone is never called 已生效.
  private routeCatalogNote(): string {
    if (this.routeCatalogStatus !== CATALOG_STATUS_OK || this.routeCatalog === null) {
      return '';
    }
    if (this.published !== null) {
      return '以下为当前连接下发的路由目录。';
    }
    return `上次连接获取（${secondsAgoText(this.routeCatalogUpdatedAtMs)}）；` +
      '当前未连接，选择将在下次连接时应用。';
  }

  private routeEmptyText(): string {
    if (this.routeCatalogStatus.length === 0) {
      return '正在读取路由目录…';
    }
    if (this.routeCatalogStatus === CATALOG_STATUS_MISSING) {
      return '尚无路由目录：连接成功后本机才会写入。';
    }
    if (this.routeCatalogStatus === CATALOG_STATUS_SCOPE_MISMATCH) {
      return this.routeCatalogUrl.length > 0
        ? `本机的路由目录属于其他管理服务器（${this.routeCatalogUrl}），未显示。`
        : '本机的路由目录属于其他管理服务器，未显示。';
    }
    if (this.routeCatalogStatus === CATALOG_STATUS_VERSION ||
      this.routeCatalogStatus === CATALOG_STATUS_UNAVAILABLE) {
      return '路由目录暂时无法读取，请重新连接后再试。';
    }
    return '路由目录无法解析，请重新连接以刷新。';
  }

  private routeNoRowsText(): string {
    if (this.routeCatalog !== null && this.routeCatalog.available === false) {
      return '管理端未下发路由目录。';
    }
    return '当前连接没有下发任何路由。';
  }

  // The ONE entry point of every switch: 单行 NetID 开关 / 全部启用 / 全部停用.
  // The desired selection is shown IMMEDIATELY, then persisted atomically, and
  // only a settled write rings the VPE's doorbell. Nothing here stops, starts or
  // re-logs-in a connection — online or offline.
  private routeApplyAction(selection: RouteSelection): void {
    if (this.routeLoadError || this.configuredUrl.length === 0) {
      // The page never replaces a preference document it could not read, and it
      // cannot scope a write without a configured server: say so, change nothing.
      this.routeActionError = this.routeLoadError
        ? this.routeLoadErrorText : '尚未配置管理服务器，无法保存路由选择。';
      return;
    }
    this.routeActionError = '';
    this.routeSaveFailed = false;
    this.routeUserEdited = true;
    this.routeDraft = selection;
    this.routePersist(selection);
  }

  private routeToggle(netId: string): void {
    const catalog: RouteCatalog | null = this.routeCatalog;
    if (this.routeLoadError || catalog === null) {
      return;
    }
    try {
      this.routeApplyAction(routeSelectionToggle(this.routeDraft, netId,
        routeSelectionKnownNetIds(catalog)));
    } catch (error) {
      if (!(error instanceof RoutePreferencesError) || error.reason !== REASON_UNKNOWN_ROUTE) {
        // Not the known-network check: never dress another failure up as
        // 「该网络已不在目录中」.
        throw error as Error;
      }
      // The row's network is no longer in the (just refreshed) catalog: no
      // write, no doorbell, and an honest explanation instead of a blind flip.
      this.routeActionError = '该网络已不在当前路由目录中，请重新连接后重试。';
    }
  }

  // 全部启用 / 全部停用 are GLOBAL verbs: they cover routes that do not exist
  // yet (the label under the stop button says exactly that). They are never a
  // batch over the currently visible NetIDs, and they write through just as
  // immediately as a single switch.
  private routeEnableAll(): void {
    this.routeApplyAction(routeSelectionSelectAll());
  }

  private routeDisableAll(): void {
    this.routeApplyAction(routeSelectionDeselectAll());
  }

  // 串行写入 (latest-wins). At most ONE write is in flight; a newer wish simply
  // replaces the pending slot, so two fast taps can never drive two atomic writes
  // through the same `.tmp` path, and an older write can never overwrite the
  // current intent. No new timer and no new framework: the recursion below is
  // driven only by the settled promises of the writes themselves.
  private routePersist(selection: RouteSelection): void {
    this.routeWritePending = routeSelectionClone(selection);
    if (this.routeWriteBusy) {
      return;
    }
    this.routeWriteBusy = true;
    this.routePumpWrites();
  }

  private routePumpWrites(): void {
    const desired: RouteSelection | null = this.routeWritePending;
    if (desired === null) {
      this.routeWriteBusy = false;
      this.routeSaving = false;
      return;
    }
    this.routeWritePending = null;
    this.routeSaving = true;
    saveRouteSelection(getContext(this).filesDir, this.configuredUrl, desired)
      .then((): void => {
        if (this.routeWritePending !== null) {
          // A newer wish arrived while this write was in flight: it is the one
          // that must reach the disk and the doorbell, so this settled write
          // does NOT ring for a state that is already superseded.
          this.routePumpWrites();
          return;
        }
        this.routeSendApplyRequest().then((): void => {
          this.routePumpWrites();
        });
      })
      .catch((_error: Error): void => {
        // The preference write FAILED: nothing was persisted, so nothing is
        // notified and the running connection is left exactly as it is.
        this.routeWritePending = null;
        this.routeWriteBusy = false;
        this.routeSaving = false;
        this.routeSaveFailed = true;
        hilog.error(DOMAIN, TAG, 'UI_ROUTE_SAVE_FAILED');
      });
  }

  // The doorbell. It carries ONLY an opaque id, the scope URL and the write
  // stamp: no selection copy and no session identity — the live VPE loads the
  // CURRENT saved preference itself. Every completed desired save gets its own
  // id, so a result can only ever be matched to the request it answers.
  private routeSendApplyRequest(): Promise<void> {
    this.routeRequestSeq = this.routeRequestSeq + 1;
    const id: string = routeApplyRequestId(this.routeRequestSeq, Date.now());
    this.routeRequestId = id;
    // A fresh request: any answer already on disk belongs to an older
    // generation and must not be rendered as this one's state.
    this.routeResultId = '';
    this.routeResultState = '';
    this.routeResultApplied = false;
    this.routeResultError = '';
    this.routeNotifyFailed = false;
    const request: RouteApplyRequest = {
      id: id,
      management_url: this.configuredUrl,
      created_at_ms: Date.now()
    };
    hilog.info(DOMAIN, TAG, 'UI_ROUTE_APPLY_REQUEST|id=%{public}s', id);
    return writeRouteApplyRequest(getContext(this).filesDir, request)
      .catch((_error: Error): void => {
        // The preference IS saved (the next connection applies it), but the live
        // session was NOT told: say so plainly instead of claiming 已生效.
        this.routeNotifyFailed = true;
        hilog.error(DOMAIN, TAG, 'UI_ROUTE_APPLY_NOTIFY_FAILED');
      });
  }

  private routeApplyView(): RouteApplyView {
    return {
      latest_request_id: this.routeRequestId,
      result_id: this.routeResultId,
      result_state: this.routeResultState,
      result_applied: this.routeResultApplied,
      has_live_document: this.published !== null,
      saving: this.routeSaving,
      save_failed: this.routeSaveFailed,
      notify_failed: this.routeNotifyFailed
    };
  }

  private routeStatusText(): string {
    if (this.routeActionError.length > 0) {
      return this.routeActionError;
    }
    return routeApplyStatusText(routeApplyStatus(this.routeApplyView()), this.routeResultError);
  }

  private routeStatusTone(): string {
    if (this.routeActionError.length > 0) {
      return ROUTE_TONE_ERROR;
    }
    return routeApplyStatusTone(routeApplyStatus(this.routeApplyView()));
  }

  private routeStatusColor(): ResourceColor {
    const tone: string = this.routeStatusTone();
    if (tone === 'ok') {
      return $r('app.color.nb_status_success');
    }
    if (tone === 'warn') {
      return $r('app.color.nb_status_warning');
    }
    if (tone === ROUTE_TONE_ERROR) {
      return $r('app.color.nb_status_error');
    }
    return $r('app.color.nb_text_secondary');
  }

  private closeVisibleSubflow(): boolean {
    const action: string = navigationBackDecision(
      this.accountLoginOpen,
      this.accountLoginBusy,
      this.accountLoginCommitting,
      this.activeTab === 2 && !this.showDiagnostics && this.configured && this.showSetupForm,
      this.activeTab === 2 && this.showDiagnostics,
      this.activeTab === 1 && this.selectedPeer !== null,
      this.activeTab === 2 && !this.showDiagnostics && this.routeSelectionOpen);
    if (action === 'account-webview') {
      this.cancelAccountLogin(false);
      return true;
    }
    if (action === 'cancel-account') {
      this.cancelAccountLogin(false);
      return true;
    }
    if (action === 'consume') {
      return true;
    }
    if (action === 'cancel-editor') {
      this.cancelConfigurationEdit();
      return true;
    }
    if (action === 'close-diagnostics') {
      this.showDiagnostics = false;
      this.activeTab = this.detailStatusOriginTab;
      return true;
    }
    if (action === 'close-peer') {
      this.selectedPeer = null;
      return true;
    }
    if (action === 'close-route') {
      // 路由选择: an internal 设置 page. Back simply returns to 设置 — every change
      // was saved the moment the switch moved, so there is no draft to discard
      // and no connection operation to issue.
      this.routeSelectionOpen = false;
      return true;
    }
    return false;
  }

  onBackPress(): boolean {
    return this.closeVisibleSubflow();
  }

  // ---- 后台保活（实验，方案 A）-------------------------------------------------
  //
  // ONE switch, written through at once, with the page's own one-line explanation
  // of what it does and of the visible price (a permanent notification). The row
  // carries no diagnostic and no status: everything the switch changes is decided
  // by the pure policy and reported by the adapter.

  @Builder
  keepAliveCard() {
    Row({ space: 12 }) {
      Column({ space: 2 }) {
        Text('后台保活（实验）')
          .fontSize(16)
          .fontColor($r('app.color.nb_text_primary'))
        Text('通过系统长时任务保持 VPN 在后台运行，通知栏会常驻流量通知')
          .fontSize(12)
          .fontColor($r('app.color.nb_text_muted'))
      }
      .layoutWeight(1)
      .alignItems(HorizontalAlign.Start)
      Toggle({ type: ToggleType.Switch, isOn: this.bgKeepAliveEnabled })
        .selectedColor($r('app.color.nb_primary'))
        .onChange((isOn: boolean): void => {
          // The callback carries the NEW switch value: act only on a REAL change,
          // so a re-emitted value (re-render, duplicate platform event, a
          // programmatic callback) cannot flip the switch under the user.
          if (isOn !== this.bgKeepAliveEnabled) {
            this.onKeepAliveToggle(isOn);
          }
        })
    }
    .width('100%')
    .padding(16)
    .borderRadius(16)
    .backgroundColor($r('app.color.nb_card_background'))
  }

  build() {
    if (this.accountLoginOpen) {
      NetBirdAccountWebView({
        authorizeUrl: this.accountAuthorizeUrl,
        expectedState: this.accountExpectedState,
        redirectUri: this.accountRedirectUri,
        onCode: (code: string): void => {
          this.handleAccountLoginCode(code);
        },
        onCancel: (): void => {
          this.cancelAccountLogin(false);
        },
        onFailure: (reason: string): void => {
          this.cancelAccountLogin(false);
          this.accountLoginError = this.accountLoginErrorText(reason);
        }
      })
    } else {
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
}
