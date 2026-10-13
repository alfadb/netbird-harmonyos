/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// 2026-10-13 PC 托盘（0.0.29）—— the app-facing tray module. This is the ONE
// module the rest of the app talks to: EntryAbility starts it, Index.ets pushes
// the current state into it and registers the listener for the menu's actions.
//
// WHAT LIVES HERE: the device/syscap gate, the LAZY load of the platform module
// (so a phone or an old ROM never even evaluates the status-bar import), the
// coalescing of state pushes into platform calls, the routing of the tray's two
// events (left click / right-menu item) and the once-per-process diagnostics.
// The platform calls themselves are in tray/NetBirdTrayPlatform.ets; the
// decisions they express (menu shape, tip text, icon variant) are in
// tray/NetBirdTrayModel.ets.
//
// THE UPSTREAM CONTRACT THIS MIRRORS (client/ui/tray.go,
// tray_click_windows.go): a persistent icon that shows the connection state, a
// LEFT click that opens the window, a RIGHT-click menu with Connect /
// Disconnect / Open / Quit, and Quit that disconnects first. Upstream keeps its
// window alive with DisableQuitOnLastWindowClosed and a
// WindowClosing hook; on HarmonyOS the same "X hides, the app keeps running"
// behaviour is EntryAbility's windowStageClose interceptor, which since 0.0.32
// hides the window (and its dock entry) with UIAbilityContext.hideAbility() and
// brings it back through showAbility() — the pair this module's openMainWindow()
// also drives — and this icon dies with the process: it does NOT keep the app alive
// (the borrowed continuous task does, see NetBirdKeepAliveService). A
// tray icon on a device whose process was killed is gone, and that is the
// platform's documented behaviour, not a defect.
//
// FAIL-SOFT, ALWAYS: every platform touch is wrapped, every refusal is one
// `ui_tray` diagnostic record (a repeated one is recorded once per process) and
// nothing here may ever break the VPN's own start/stop path. On a phone, on a
// tablet, on a ROM without the capability or with the kit missing entirely, the
// feature simply stays off: traySupported()/startTray() answer false, one line
// says why, and every later push is a no-op.
//
// THE STATE PUSH: Index.ets calls updateTrayState() from its EXISTING 2 s tick
// with the same official label its home card renders. The push is deduped (a
// signature per surface) and coalesced to one 500 ms window, because the status
// bar APIs answer 1010710003 when they are called too frequently.

import common from '@ohos.app.ability.common';
import deviceInfo from '@ohos.deviceInfo';
import hilog from '@ohos.hilog';
import Want from '@ohos.app.ability.Want';
import { event } from '../vpnextensionability/NetBirdDiagnostics';
import type { DiagFields } from '../vpnextensionability/NetBirdDiagnostics';
import { TRAY_CLICK_LEFT, TRAY_EVENT_ICON_CLICK, TRAY_EVENT_MENU_CLICK,
  TRAY_MENU_OPEN, TRAY_TEXT_DISCONNECTED, TRAY_UPDATE_DEBOUNCE_MS,
  trayDeviceSupported, trayHoverTips, trayIconVariant, trayMenuCodeKnown,
  trayMenuGroups, trayMenuSignature, traySupported } from './NetBirdTrayModel';
import type { TrayPlatformEvent } from './NetBirdTrayPlatform';

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';
const BUNDLE_NAME: string = 'cn.alfadb.netbird';
const ENTRY_ABILITY_NAME: string = 'EntryAbility';

// The ONE record name (lowercase, not an internal runtime name) and the values
// its public fields carry: 'stage' / 'result' / 'reason' / 'mode' are all on the
// public key set of the diagnostics writer.
const TRAY_EVENT_NAME: string = 'ui_tray';
const TRAY_STAGE_START: string = 'start';
const TRAY_STAGE_UPDATE: string = 'update';
const TRAY_STAGE_REMOVE: string = 'remove';
const TRAY_STAGE_CLICK: string = 'click';
const TRAY_STAGE_MENU: string = 'menu';
const TRAY_RESULT_OK: string = 'ok';
const TRAY_RESULT_ERROR: string = 'error';
const TRAY_RESULT_SKIPPED: string = 'skipped';

// The listener the PAGE owns: it receives the menu code of every action row
// (Connect / Disconnect / Quit). 打开主窗口 never reaches it — the tray opens the
// window itself, exactly like the left click.
export type TrayActionListener = (menuCode: string) => void;

interface BusinessFailure extends Error {
  code?: number;
}

interface TrayServiceState {
  context: common.UIAbilityContext | undefined;
  // The platform accepted addToStatusBar: updates and removal are meaningful.
  started: boolean;
  // The feature is off for this process (device, syscap, missing kit or a
  // refused capability check) — one line was recorded, nothing is retried.
  unavailable: boolean;
  startInFlight: boolean;
  listener: TrayActionListener | undefined;
  // What the icon currently carries, per surface. A failed platform call still
  // advances its own entry: retrying a rate-limited API on every 2 s tick is
  // worse than a stale tip, and the next state TRANSITION re-arms the surface.
  tips: string;
  iconVariant: string;
  menuSignature: string;
  pendingText: string;
  pendingReady: boolean;
  timer: number;
  // "Same error, one record per process": the 2 s tick must not repeat itself.
  reported: Set<string>;
}

const state: TrayServiceState = {
  context: undefined,
  started: false,
  unavailable: false,
  startInFlight: false,
  listener: undefined,
  tips: '',
  iconVariant: '',
  menuSignature: '',
  pendingText: '',
  pendingReady: false,
  timer: -1,
  reported: new Set<string>()
};

// The lazily loaded platform module. `undefined` until something really needs
// it; a failed load is remembered by never trying again in this process.
let platform: typeof import('./NetBirdTrayPlatform') | undefined = undefined;

function failureCode(error: Error): number {
  try {
    const code: number | undefined = (error as BusinessFailure).code;
    return typeof code === 'number' && Number.isFinite(code) ? code : 0;
  } catch (ignored) {
    return 0;
  }
}

// The `reason` of a failed platform call: the platform's own numeric cause when
// it gave one, an honest closed token when it did not.
function failureToken(error: Error): string {
  const code: number = failureCode(error);
  return code === 0 ? 'error' : String(code);
}

function record(stage: string, result: string, reason: string): void {
  const key: string = `${stage}:${result}:${reason}`;
  if (result !== TRAY_RESULT_OK) {
    if (state.reported.has(key)) {
      return;
    }
    state.reported.add(key);
  }
  const fields: DiagFields = {};
  fields['stage'] = stage;
  fields['result'] = result;
  fields['reason'] = reason;
  event(TRAY_EVENT_NAME, fields);
}

// The ONLY load of the status-bar kit's module. A device without the PC
// status-bar capability has no such runtime module; the rejection is caught
// HERE, so it costs one diagnostic line instead of the app.
async function loadPlatform(): Promise<typeof import('./NetBirdTrayPlatform') | undefined> {
  const loaded: typeof import('./NetBirdTrayPlatform') | undefined = platform;
  if (loaded !== undefined) {
    return loaded;
  }
  try {
    const module: typeof import('./NetBirdTrayPlatform') =
      await import('./NetBirdTrayPlatform');
    platform = module;
    return module;
  } catch (error) {
    hilog.warn(DOMAIN, TAG, 'UI_TRAY_PLATFORM_MISSING|code=%{public}s|note=feature-off',
      failureToken(error as Error));
    record(TRAY_STAGE_START, TRAY_RESULT_SKIPPED, 'platform-module-missing');
    return undefined;
  }
}

// The device gate. Read through the platform's own deviceInfo; a device that
// cannot answer the question has not said yes.
function currentDeviceType(): string {
  try {
    const value: string = deviceInfo.deviceType;
    return typeof value === 'string' ? value : '';
  } catch (error) {
    return '';
  }
}

// startTray() is called ONCE per UI process, from EntryAbility.onCreate.
export async function startTray(context: common.UIAbilityContext): Promise<boolean> {
  state.context = context;
  if (state.unavailable || state.started) {
    return state.started;
  }
  if (state.startInFlight) {
    return false;
  }
  const deviceType: string = currentDeviceType();
  if (!trayDeviceSupported(deviceType)) {
    // Phone / tablet / anything else: the capability does not exist there, so
    // not even the platform module is loaded.
    record(TRAY_STAGE_START, TRAY_RESULT_SKIPPED, 'device-not-2in1');
    state.unavailable = true;
    return false;
  }
  if (!canIUse('SystemCapability.PCService.StatusBarManager')) {
    record(TRAY_STAGE_START, TRAY_RESULT_SKIPPED, 'syscap-absent');
    state.unavailable = true;
    return false;
  }
  state.startInFlight = true;
  try {
    const module: typeof import('./NetBirdTrayPlatform') | undefined = await loadPlatform();
    if (module === undefined) {
      state.unavailable = true;
      return false;
    }
    let capability: boolean = false;
    try {
      capability = await module.platformCapabilitySupported(context);
    } catch (error) {
      record(TRAY_STAGE_START, TRAY_RESULT_ERROR, failureToken(error as Error));
      state.unavailable = true;
      return false;
    }
    if (!traySupported(deviceType, capability)) {
      record(TRAY_STAGE_START, TRAY_RESULT_SKIPPED, 'capability-unsupported');
      state.unavailable = true;
      return false;
    }
    // The item the icon is created with: the state the page has not pushed yet
    // reads as 未连接 (the platform's default hover text would be the module
    // label instead).
    const text: string = state.pendingReady ? state.pendingText : TRAY_TEXT_DISCONNECTED;
    const tips: string = trayHoverTips(text);
    await module.platformAdd(context, trayMenuGroups(text), tips);
    module.setPlatformHandler(onPlatformEvent);
    module.platformListen();
    state.started = true;
    state.tips = tips;
    state.iconVariant = trayIconVariant(text);
    state.menuSignature = trayMenuSignature(text);
    // The state the page pushed while the platform was answering: publish it
    // now through the same coalesced path.
    if (state.pendingReady) {
      scheduleFlush();
    }
    hilog.info(DOMAIN, TAG, 'UI_TRAY_STARTED|bundle=%{public}s|device=%{public}s',
      BUNDLE_NAME, deviceType);
    record(TRAY_STAGE_START, TRAY_RESULT_OK, deviceType);
    return true;
  } catch (error) {
    // 1010710001/1010710002/1010710005/1010710007/1010720001 and any missing
    // API land here: one line, the feature stays off, nothing else changes.
    hilog.warn(DOMAIN, TAG, 'UI_TRAY_START_FAILED|code=%{public}s|note=feature-off',
      failureToken(error as Error));
    record(TRAY_STAGE_START, TRAY_RESULT_ERROR, failureToken(error as Error));
    state.unavailable = true;
    return false;
  } finally {
    state.startInFlight = false;
  }
}

// The page's registration: the tray menu's action rows hand their menu code to
// this listener, which routes it into the SAME connect/stop/quit paths the
// page's own buttons use.
export function setTrayActionListener(listener: TrayActionListener | undefined): void {
  state.listener = listener;
}

// Called from the page's EXISTING 2 s tick with the same official label the home
// card renders (Index.ets stateText()). Cheap and idempotent: it stores the text
// and, when the tray is live, arms ONE 500 ms flush.
export function updateTrayState(stateText: string): void {
  state.pendingText = stateText;
  state.pendingReady = true;
  if (!state.started) {
    return;
  }
  scheduleFlush();
}

function scheduleFlush(): void {
  if (state.timer >= 0) {
    return;
  }
  state.timer = setTimeout((): void => {
    state.timer = -1;
    flushTray();
  }, TRAY_UPDATE_DEBOUNCE_MS);
}

// Republish ONLY the surfaces whose value really changed. The menu is a whole
// tree on every updateStatusBarMenu call, hence the signature.
function flushTray(): void {
  const module: typeof import('./NetBirdTrayPlatform') | undefined = platform;
  const context: common.UIAbilityContext | undefined = state.context;
  if (module === undefined || context === undefined || !state.started) {
    return;
  }
  const text: string = state.pendingReady ? state.pendingText : TRAY_TEXT_DISCONNECTED;
  state.pendingReady = false;
  const signature: string = trayMenuSignature(text);
  if (signature !== state.menuSignature) {
    state.menuSignature = signature;
    try {
      module.platformUpdateMenu(context, trayMenuGroups(text));
      record(TRAY_STAGE_UPDATE, TRAY_RESULT_OK, 'menu');
    } catch (error) {
      record(TRAY_STAGE_UPDATE, TRAY_RESULT_ERROR, failureToken(error as Error));
    }
  }
  const variant: string = trayIconVariant(text);
  if (variant !== state.iconVariant) {
    state.iconVariant = variant;
    module.platformUpdateIcon(context).then((): void => {
      record(TRAY_STAGE_UPDATE, TRAY_RESULT_OK, 'icon');
    }).catch((error: Error): void => {
      record(TRAY_STAGE_UPDATE, TRAY_RESULT_ERROR, failureToken(error));
    });
  }
  const tips: string = trayHoverTips(text);
  if (tips !== state.tips) {
    state.tips = tips;
    module.platformUpdateTips(context, tips).then((): void => {
      record(TRAY_STAGE_UPDATE, TRAY_RESULT_OK, 'tips');
    }).catch((error: Error): void => {
      record(TRAY_STAGE_UPDATE, TRAY_RESULT_ERROR, failureToken(error));
    });
  }
}

// The tray's two events. The left click and the 打开主窗口 row both OPEN THE
// WINDOW, which is upstream's Windows behaviour (tray_click_windows.go: onClick
// -> ShowWindow) and the only thing the platform allows besides an event.
function onPlatformEvent(platformEvent: TrayPlatformEvent): void {
  if (platformEvent.kind === TRAY_EVENT_ICON_CLICK) {
    // The platform documents exactly one click kind here ('leftClick'); anything
    // else is not copied into a record.
    record(TRAY_STAGE_CLICK, TRAY_RESULT_OK, platformEvent.menuCode === TRAY_CLICK_LEFT
      ? TRAY_CLICK_LEFT : 'unknown-click');
    openMainWindow();
    return;
  }
  if (platformEvent.kind !== TRAY_EVENT_MENU_CLICK) {
    return;
  }
  if (!trayMenuCodeKnown(platformEvent.menuCode)) {
    record(TRAY_STAGE_MENU, TRAY_RESULT_SKIPPED, 'unknown-menu-code');
    return;
  }
  if (platformEvent.menuCode === TRAY_MENU_OPEN) {
    record(TRAY_STAGE_MENU, TRAY_RESULT_OK, TRAY_MENU_OPEN);
    openMainWindow();
    return;
  }
  const listener: TrayActionListener | undefined = state.listener;
  if (listener === undefined) {
    // The window (and with it the page) is gone: the tray can still show the
    // state, but the connect/stop/quit actions belong to the page, so they are
    // honestly reported as not delivered instead of being improvised here.
    record(TRAY_STAGE_MENU, TRAY_RESULT_SKIPPED, 'no-page-listener');
    return;
  }
  record(TRAY_STAGE_MENU, TRAY_RESULT_OK, platformEvent.menuCode);
  try {
    listener(platformEvent.menuCode);
  } catch (error) {
    record(TRAY_STAGE_MENU, TRAY_RESULT_ERROR, failureToken(error as Error));
  }
}

// startAbility on our own EntryAbility: a hot start when the ability is still
// alive (it is — the icon only exists while the process does), so the page's
// onNewWant path runs and the window comes back to the foreground.
//
// 0.0.32 关闭到托盘: a window hidden by hideAbility() is NOT brought back by a
// hot start alone — its session stays invisible — so the CONTEXT is asked to
// show it first (UIAbilityContext.showAbility(), the twin of the hide the X
// runs). This is a "show", not a rebuild: the same window, page and state come
// back. The call is made unconditionally because this module cannot see the
// ability's hide flag; when nothing is hidden it answers either nothing or an
// error (801 on a device without the capability, 16000050) that is logged and
// IGNORED — the hot start below is what restores a merely-backgrounded window,
// exactly as before.
function openMainWindow(): void {
  const context: common.UIAbilityContext | undefined = state.context;
  if (context === undefined) {
    record(TRAY_STAGE_CLICK, TRAY_RESULT_SKIPPED, 'no-context');
    return;
  }
  try {
    context.showAbility().then((): void => {
      hilog.info(DOMAIN, TAG, 'UI_TRAY_OPEN|result=show|bundle=%{public}s', BUNDLE_NAME);
      record(TRAY_STAGE_CLICK, TRAY_RESULT_OK, 'show');
    }).catch((error: Error): void => {
      record(TRAY_STAGE_CLICK, TRAY_RESULT_ERROR, failureToken(error));
    });
  } catch (error) {
    record(TRAY_STAGE_CLICK, TRAY_RESULT_ERROR, failureToken(error as Error));
  }
  const want: Want = { bundleName: BUNDLE_NAME, abilityName: ENTRY_ABILITY_NAME };
  try {
    context.startAbility(want).then((): void => {
      hilog.info(DOMAIN, TAG, 'UI_TRAY_OPEN|result=start|bundle=%{public}s', BUNDLE_NAME);
    }).catch((error: Error): void => {
      record(TRAY_STAGE_CLICK, TRAY_RESULT_ERROR, failureToken(error));
    });
  } catch (error) {
    record(TRAY_STAGE_CLICK, TRAY_RESULT_ERROR, failureToken(error as Error));
  }
}

// removeFromStatusBar on the explicit-quit path. Best effort by design: the
// platform refuses the removal while no window is in the foreground
// (1010710004), and the icon dies with the process anyway — so a refusal is
// reported for what it is and never blocks the quit.
export async function removeTray(reason: string): Promise<void> {
  const module: typeof import('./NetBirdTrayPlatform') | undefined = platform;
  const context: common.UIAbilityContext | undefined = state.context;
  if (module === undefined || context === undefined || !state.started) {
    return;
  }
  if (state.timer >= 0) {
    clearTimeout(state.timer);
    state.timer = -1;
  }
  try {
    module.platformUnlisten();
  } catch (error) {
    record(TRAY_STAGE_REMOVE, TRAY_RESULT_ERROR, failureToken(error as Error));
  }
  try {
    module.platformRemove(context);
    hilog.info(DOMAIN, TAG, 'UI_TRAY_REMOVED|reason=%{public}s', reason);
    record(TRAY_STAGE_REMOVE, TRAY_RESULT_OK, reason);
  } catch (error) {
    const code: number = failureCode(error as Error);
    hilog.warn(DOMAIN, TAG, 'UI_TRAY_REMOVE_FAILED|code=%{public}s|reason=%{public}s|' +
      'note=icon-dies-with-process', failureToken(error as Error), reason);
    record(TRAY_STAGE_REMOVE, TRAY_RESULT_ERROR, code === 1010710004
      ? 'no-foreground-window' : failureToken(error as Error));
  }
  state.started = false;
}

// Read-only, for the diagnostics/selfcheck story: is a status-bar icon up?
export function trayStarted(): boolean {
  return state.started;
}
