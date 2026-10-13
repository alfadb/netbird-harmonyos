/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// 2026-10-13 PC 托盘（0.0.29）；2026-10-13 统一退出（0.0.31）—— PURE LOGIC only.
//
// WHAT THIS FILE IS: every tray decision that can be made without a device —
// what the right-click menu contains for a given connection state, what the
// hover tip says, which icon variant the state calls for, and whether this
// device can have a status-bar icon at all. The platform calls live in the ONE
// adapter (tray/NetBirdTrayPlatform.ets, the only module that imports
// @kit.DeskTopExtensionKit) and the coalescing/dedupe logic lives in the ONE
// service (tray/NetBirdTrayService.ets); the host-side selfcheck imports THIS
// file byte-identical (client/entry/selfcheck/run-tray.mts), so it must stay
// plain TypeScript: NO @ohos import, no decorator, no enum, no platform call,
// no clock read.
//
// WHY THIS SHAPE — the upstream contract it mirrors. Upstream's desktop client
// (client/ui/tray.go) builds ONE tray menu whose rows are state-dependent:
//
//   buildMenu():            t.statusItem (state label) / Connect / Disconnect /
//                           profiles / Open / exit-node / Settings / About / Quit
//   relayoutMenu():         t.upItem.SetHidden(connected || connecting)
//                           t.downItem.SetHidden(!connected && !connecting)
//   handleQuit():           BeginShutdown() -> (unless KeepConnectedOnQuit)
//                           Connection.Down() -> app.Quit()
//   applyIcon()/iconForState(): connected vs not picks the icon
//   tray_click_windows.go:  LEFT click opens the window; right click = menu
//
// HarmonyOS gives the same four surfaces (see the tray API verification note,
// TRAY-API-VERIFY.md §1.2): a status-bar icon with a left-click EVENT, a
// right-click menu, a hover tip, and the app's own state. Three differences are
// structural and are encoded here rather than worked around:
//
//   1. A menu item CANNOT be hidden (statusBarManager.StatusBarMenuItem has no
//      visibility/enabled field — only title/menuAction/subMenu/menuCode/
//      options), so upstream's SetHidden becomes "do not put the row in the
//      rebuilt menu at all". trayMenuGroups() therefore returns the LEVEL-1 rows
//      for the CURRENT state, and the service republishes the whole menu when —
//      and only when — that set changes.
//   2. There is no status ROW: a level-1 item must carry a subMenu or a
//      menuAction (error 1010720001) and nothing can render it disabled, so the
//      state lives in the HOVER TIP, which is also the platform's own documented
//      place for it (StatusBarItem.hoverTips, 6.0.2(22)+).
//   3. The state TEXT is the page's own official label
//      (NetBirdUiModel.homeStateText: 已连接 / 连接中… / 正在断开… / 需要登录 /
//      未连接). This module never invents a second vocabulary — it only maps the
//      page's label onto a tray token, and an unrecognised label is treated as
//      the honest default (未连接).

// The menu codes delivered by statusBarManager's 'rightMenuClick' event. Each
// code is globally unique within the menu (error 1010710007) and is our own
// closed token set — never platform text.
export const TRAY_MENU_CONNECT: string = 'nb_tray_connect';
export const TRAY_MENU_DISCONNECT: string = 'nb_tray_disconnect';
export const TRAY_MENU_OPEN: string = 'nb_tray_open';
export const TRAY_MENU_QUIT: string = 'nb_tray_quit';

// The row titles: upstream's tray.menu.connect / .disconnect / .open / .quit.
// 0.0.31: the Quit row is 退出 — the plain label upstream uses, because quitting
// now ALWAYS disconnects first (there is no "keep the tunnel" variant left to
// distinguish it from; see tray/NetBirdQuitFlow.ets).
export const TRAY_TITLE_CONNECT: string = '连接';
export const TRAY_TITLE_DISCONNECT: string = '断开';
export const TRAY_TITLE_OPEN: string = '打开主窗口';
export const TRAY_TITLE_QUIT: string = '退出';

// The state vocabulary of the page (NetBirdUiModel.homeStateText) — the ONLY
// labels this module recognises. Anything else is 未连接 (never a guess).
export const TRAY_TEXT_CONNECTED: string = '已连接';
export const TRAY_TEXT_CONNECTING: string = '连接中…';
export const TRAY_TEXT_DISCONNECTING: string = '正在断开…';
export const TRAY_TEXT_LOGIN_REQUIRED: string = '需要登录';
export const TRAY_TEXT_DISCONNECTED: string = '未连接';

// The tray's own state tokens (derived from the label above).
export const TRAY_STATE_CONNECTED: string = 'connected';
export const TRAY_STATE_TRANSITION: string = 'transition';
export const TRAY_STATE_LOGIN_REQUIRED: string = 'login-required';
export const TRAY_STATE_DISCONNECTED: string = 'disconnected';

// Icon variants. Upstream applyIcon()/iconForState() picks by connection state;
// our two variants exist as tokens (and as the mapping the service drives) but
// both currently resolve to the packaged app icon — generating the proper
// monochrome 24vp white/black pair is a FOLLOW-UP, and it must not change this
// file's contract.
export const TRAY_ICON_CONNECTED: string = 'connected';
export const TRAY_ICON_IDLE: string = 'idle';

// StatusBarItem.hoverTips: 1..128 characters (error 1010710005 above that).
export const TRAY_HOVER_PREFIX: string = 'NetBird VPN · ';
export const TRAY_HOVER_MAX_CHARS: number = 128;

// QuickOperation is MANDATORY even though this iteration renders no pop-up: the
// empty abilityName is the documented way to say "left click is an event"
// (statusBarManager.d.ts:396-398 — "If an empty string is passed, the click
// service can be processed by listening for the statusBarIconClick events"), and
// a pop-up height must still be positive.
export const TRAY_QUICK_TITLE: string = 'NetBird';
export const TRAY_QUICK_HEIGHT_VP: number = 40;

// The module that owns the ability the tray starts (menuAction.moduleName).
export const TRAY_MODULE_NAME: string = 'entry';

// The packaged icon, resolved BY NAME (the same resource-by-name style
// EntryAbility uses for its system-bar colors — no $r transform in a plain
// module). It is declared for the ability as $media:app_icon.
export const TRAY_APP_ICON_RESOURCE: string = 'app_icon';

// The event kinds the platform delivers: data['iconClickType'] === 'leftClick'
// for the left button, data['menuCode'] for a right-clicked item
// (statusBarManager.d.ts:710, :740). The platform adapter normalises the raw
// payload into these two tokens, which is what the service routes on.
export const TRAY_EVENT_ICON_CLICK: string = 'icon-click';
export const TRAY_EVENT_MENU_CLICK: string = 'menu-click';
export const TRAY_CLICK_LEFT: string = 'leftClick';
export const TRAY_KEY_ICON_CLICK_TYPE: string = 'iconClickType';
export const TRAY_KEY_MENU_CODE: string = 'menuCode';

// The only device with the status-bar capability: SystemCapability.
// PCService.StatusBarManager exists on 2in1 alone (device-define/2in1-hmos.json).
export const TRAY_DEVICE_2IN1: string = '2in1';

// Every platform call is coalesced to this one window: the tray APIs answer
// "1010710003 - The API is being called too frequently" and the tray API
// verification note recommends ~500 ms of debounce per state change.
export const TRAY_UPDATE_DEBOUNCE_MS: number = 500;

// One level-1 row of the rebuilt menu: the title the platform renders and the
// code it will hand back through 'rightMenuClick'.
export interface TrayMenuSpec {
  title: string;
  menuCode: string;
}

// The page's label -> this module's closed state token. An unknown label (a
// future page text, an empty document) is the honest default.
export function trayStateToken(stateText: string): string {
  if (stateText === TRAY_TEXT_CONNECTED) {
    return TRAY_STATE_CONNECTED;
  }
  if (stateText === TRAY_TEXT_CONNECTING || stateText === TRAY_TEXT_DISCONNECTING) {
    return TRAY_STATE_TRANSITION;
  }
  if (stateText === TRAY_TEXT_LOGIN_REQUIRED) {
    return TRAY_STATE_LOGIN_REQUIRED;
  }
  return TRAY_STATE_DISCONNECTED;
}

// The hover tip: the app's name and the OFFICIAL state label, bounded to the
// platform's 128 characters.
export function trayHoverTips(stateText: string): string {
  const label: string = stateText.length > 0 ? stateText : TRAY_TEXT_DISCONNECTED;
  const tips: string = TRAY_HOVER_PREFIX + label;
  if (tips.length <= TRAY_HOVER_MAX_CHARS) {
    return tips;
  }
  return tips.substring(0, TRAY_HOVER_MAX_CHARS);
}

// Upstream iconForState(): connected vs not. 需要登录 is NOT connected — the
// tunnel is down and the label says why.
export function trayIconVariant(stateText: string): string {
  return trayStateToken(stateText) === TRAY_STATE_CONNECTED
    ? TRAY_ICON_CONNECTED : TRAY_ICON_IDLE;
}

// Upstream relayoutMenu(): Connect is hidden while a tunnel runs or is being
// established, Disconnect is hidden when there is nothing to disconnect. The
// rows are returned as SEPARATED GROUPS (each group renders as its own section),
// mirroring upstream's separators around Connect/Disconnect, Open and Quit.
// Level-1 items stay far below the platform's 20-item limit.
export function trayMenuGroups(stateText: string): TrayMenuSpec[][] {
  const token: string = trayStateToken(stateText);
  const active: boolean = token === TRAY_STATE_CONNECTED || token === TRAY_STATE_TRANSITION;
  const actions: TrayMenuSpec[] = [];
  if (active) {
    const disconnect: TrayMenuSpec = {
      title: TRAY_TITLE_DISCONNECT, menuCode: TRAY_MENU_DISCONNECT
    };
    actions.push(disconnect);
  } else {
    const connect: TrayMenuSpec = { title: TRAY_TITLE_CONNECT, menuCode: TRAY_MENU_CONNECT };
    actions.push(connect);
  }
  const open: TrayMenuSpec = { title: TRAY_TITLE_OPEN, menuCode: TRAY_MENU_OPEN };
  const quit: TrayMenuSpec = { title: TRAY_TITLE_QUIT, menuCode: TRAY_MENU_QUIT };
  const groups: TrayMenuSpec[][] = [actions, [open], [quit]];
  return groups;
}

// Is this one of OUR menu codes? The 'rightMenuClick' payload crosses a process
// boundary, so a code outside this closed set is not an action — it is text we
// will not act on and will not copy into a record.
export function trayMenuCodeKnown(menuCode: string): boolean {
  return menuCode === TRAY_MENU_CONNECT || menuCode === TRAY_MENU_DISCONNECT ||
    menuCode === TRAY_MENU_OPEN || menuCode === TRAY_MENU_QUIT;
}

// A stable string for the CURRENT menu, so the service republishes the menu only
// when the row set really changed (updateStatusBarMenu is a whole-tree push and
// the tray APIs are rate-limited).
export function trayMenuSignature(stateText: string): string {
  const groups: TrayMenuSpec[][] = trayMenuGroups(stateText);
  let signature: string = '';
  for (let g = 0; g < groups.length; g++) {
    if (g > 0) {
      signature += '/';
    }
    const group: TrayMenuSpec[] = groups[g];
    for (let i = 0; i < group.length; i++) {
      if (i > 0) {
        signature += '|';
      }
      signature += `${group[i].menuCode}:${group[i].title}`;
    }
  }
  return signature;
}

// The cheap device gate: the status-bar capability is 2in1-only, so nothing is
// loaded (not even the platform module) anywhere else.
export function trayDeviceSupported(deviceType: string): boolean {
  return deviceType === TRAY_DEVICE_2IN1;
}

// The full gate: the device AND the platform's own pre-flight answer
// (isStatusBarCapabilitySupported, 26.0.0+). A ROM that cannot answer the
// question has not said yes — so the feature stays off.
export function traySupported(deviceType: string, capabilitySupported: boolean): boolean {
  return trayDeviceSupported(deviceType) && capabilitySupported === true;
}
