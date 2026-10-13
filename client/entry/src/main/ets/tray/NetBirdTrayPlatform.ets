/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// 2026-10-13 PC 托盘（0.0.29）—— the ONE module that imports the PC status-bar
// (tray) API: @kit.DeskTopExtensionKit -> statusBarManager
// (@hms.pcService.statusBarManager, @syscap
// SystemCapability.PCService.StatusBarManager, @since 5.0.0(12), no permission
// and not a system API — see TRAY-API-VERIFY.md §1.2).
//
// WHY IT IS SEPARATE FROM THE SERVICE: this module is loaded ONLY through a
// dynamic import (tray/NetBirdTrayService.ets loadPlatform()), and only after
// the device gate AND canIUse(SystemCapability.PCService.StatusBarManager) both
// passed. The kit's runtime module does not exist on a device without the PC
// status-bar capability, and a STATIC import of it — evaluated when the module
// record loads, which for a statically imported module means when the app starts
// — would take the whole UI process down on such a device. Loading this file
// lazily is what turns "the API is missing" into a caught error instead of a
// crash: the service logs one line and the feature simply stays off.
//
// WHAT IT DOES: the raw platform calls, and nothing else. It receives plain data
// from the pure model (row specs, tip text) and hands plain data back (the two
// event kinds the platform delivers). Every judgement — whether to run at all,
// when to republish, what the state means — belongs to
// tray/NetBirdTrayModel.ets and tray/NetBirdTrayService.ets. The calls here are
// deliberately WITHOUT their own try/catch: the service is the single place that
// decides what a platform refusal means, so nothing is swallowed twice.
//
// THE CALLS, by their exact SDK signatures
// (sdk/default/hms/ets/api/@hms.pcService.statusBarManager.d.ts):
//   addToStatusBar(context, StatusBarItem): void                    :453
//   updateStatusBarMenu(context, StatusBarGroupMenu[]): void        :532
//   updateStatusBarIcon(context, StatusBarIcon): void               :613
//   updateStatusBarHoverTips(context, hoverTips): Promise<void>     :652 (6.0.2(22)+)
//   removeFromStatusBar(context): void                              :493
//   on('statusBarIconClick'|'rightMenuClick', Callback<EventData>)  :710, :740
//   off(...)                                                        :724, :754
//   isStatusBarCapabilitySupported(context): Promise<boolean>       :792 (26.0.0+)
// Errors the service must expect: 1010710001 (pixelmap too large), 1010710002
// (too many menu items), 1010710003 (called too frequently), 1010710004 (the
// icon cannot be removed while no window is in the foreground), 1010710005
// (string too long), 1010710007 (menuCode not unique), 1010720001 (a menu item
// with neither subMenu nor menuAction).

import { statusBarManager } from '@kit.DeskTopExtensionKit';
import common from '@ohos.app.ability.common';
import emitter from '@ohos.events.emitter';
import image from '@ohos.multimedia.image';
import { TRAY_APP_ICON_RESOURCE, TRAY_EVENT_ICON_CLICK, TRAY_EVENT_MENU_CLICK,
  TRAY_KEY_ICON_CLICK_TYPE, TRAY_KEY_MENU_CODE,
  TRAY_MODULE_NAME, TRAY_QUICK_HEIGHT_VP, TRAY_QUICK_TITLE, TrayMenuSpec }
  from './NetBirdTrayModel';

// The entry ability the tray's actions drive (the left-click event and the
// 打开主窗口 row both start it; statusBarManager can only start an in-app
// UIAbility).
const ENTRY_ABILITY_NAME: string = 'EntryAbility';
// 24 vp at a 2x density, i.e. the size the platform recommends. Decoding the
// packaged icon at a bounded size is also what keeps the PixelMap clear of
// error 1010710001.
const ICON_SIZE_PX: number = 48;

export interface TrayPlatformEvent {
  kind: string;
  menuCode: string;
}

export type TrayPlatformHandler = (event: TrayPlatformEvent) => void;

let handler: TrayPlatformHandler | undefined = undefined;
let listening: boolean = false;
let appIcon: image.PixelMap | undefined = undefined;

export function setPlatformHandler(next: TrayPlatformHandler | undefined): void {
  handler = next;
}

// Fail-soft by construction: this runs inside a platform callback, and a
// throwing listener must never escape into it.
function emitEvent(kind: string, menuCode: string): void {
  const target: TrayPlatformHandler | undefined = handler;
  if (target === undefined) {
    return;
  }
  const event: TrayPlatformEvent = { kind: kind, menuCode: menuCode };
  try {
    target(event);
  } catch (error) {
    // The service's own listener already reports what it could not do.
  }
}

function eventValue(data: emitter.EventData, key: string): string {
  const bag: Record<string, Object> | undefined = data.data;
  if (bag === undefined || bag === null) {
    return '';
  }
  const value: Object | undefined = bag[key];
  return typeof value === 'string' ? value : '';
}

function onIconClick(data: emitter.EventData): void {
  // data['iconClickType'] === 'leftClick' for a left-button click; the callback
  // is only ever delivered for a click on OUR icon.
  emitEvent(TRAY_EVENT_ICON_CLICK, eventValue(data, TRAY_KEY_ICON_CLICK_TYPE));
}

function onRightMenuClick(data: emitter.EventData): void {
  emitEvent(TRAY_EVENT_MENU_CLICK, eventValue(data, TRAY_KEY_MENU_CODE));
}

// The platform's own pre-flight answer. On a ROM older than 26.0.0 the check
// does not exist; "cannot answer" is NOT a yes, so it answers false and the
// service leaves the feature off.
export async function platformCapabilitySupported(context: common.Context): Promise<boolean> {
  if (typeof statusBarManager.isStatusBarCapabilitySupported !== 'function') {
    return false;
  }
  return await statusBarManager.isStatusBarCapabilitySupported(context);
}

// The packaged app icon, decoded ONCE per process (the PixelMap is reused for
// every later update; the ImageSource is released as soon as the decode is
// done).
async function appIconPixelMap(context: common.Context): Promise<image.PixelMap> {
  const cached: image.PixelMap | undefined = appIcon;
  if (cached !== undefined) {
    return cached;
  }
  const raw: Uint8Array = context.resourceManager.getMediaByNameSync(TRAY_APP_ICON_RESOURCE);
  const source: image.ImageSource = image.createImageSource(raw.buffer as ArrayBuffer);
  try {
    const size: image.Size = { width: ICON_SIZE_PX, height: ICON_SIZE_PX };
    const options: image.DecodingOptions = { desiredSize: size, editable: false };
    const decoded: image.PixelMap = await source.createPixelMap(options);
    appIcon = decoded;
    return decoded;
  } finally {
    source.release().catch((error: Error): void => {
      // Releasing the source only frees the encoded buffer; the decoded
      // PixelMap stays valid, so a failed release is harmless.
    });
  }
}

// FOLLOW-UP (recorded, not hidden): a status-bar icon should be a MONOCHROME
// 24 vp pair — pure white for dark wallpapers (white) and pure black for light
// ones (black). This iteration reuses the packaged colour app icon for BOTH
// fields, which is what the platform accepts today; the icons still switch by
// state through updateStatusBarIcon, they just carry the same pixels. Generating
// the real pair is a follow-up asset task.
async function statusBarIcon(context: common.Context): Promise<statusBarManager.StatusBarIcon> {
  const pixelMap: image.PixelMap = await appIconPixelMap(context);
  const icon: statusBarManager.StatusBarIcon = { white: pixelMap, black: pixelMap };
  return icon;
}

// The pure model's row specs -> the platform's menu tree. Every row is an EVENT
// row: notifyOnly + the item's own menuCode (the non-deprecated carrier since
// 6.1.1(24)) means the click comes back through 'rightMenuClick' and no UI is
// started implicitly. abilityName is still mandatory on the action object, and
// it names THIS app's ability — which is the documented scope of the field.
function menuTree(groups: TrayMenuSpec[][]): statusBarManager.StatusBarGroupMenu[] {
  const tree: statusBarManager.StatusBarGroupMenu[] = [];
  for (let g = 0; g < groups.length; g++) {
    const specs: TrayMenuSpec[] = groups[g];
    const rows: statusBarManager.StatusBarMenuItem[] = [];
    for (let i = 0; i < specs.length; i++) {
      const spec: TrayMenuSpec = specs[i];
      const action: statusBarManager.StatusBarMenuAction = {
        abilityName: ENTRY_ABILITY_NAME,
        moduleName: TRAY_MODULE_NAME,
        notifyOnly: true
      };
      const row: statusBarManager.StatusBarMenuItem = {
        title: spec.title, menuAction: action, menuCode: spec.menuCode
      };
      rows.push(row);
    }
    tree.push(rows);
  }
  return tree;
}

// The ONE add call. quickOperation.abilityName = '' is the documented "no
// pop-up, deliver a left-click event instead" (d.ts:396-398); hoverTips carries
// the current state text from the first moment.
export async function platformAdd(context: common.Context, groups: TrayMenuSpec[][],
  tips: string): Promise<void> {
  const icon: statusBarManager.StatusBarIcon = await statusBarIcon(context);
  const quick: statusBarManager.QuickOperation = {
    title: TRAY_QUICK_TITLE, height: TRAY_QUICK_HEIGHT_VP, abilityName: ''
  };
  const item: statusBarManager.StatusBarItem = {
    icons: icon, quickOperation: quick, statusBarGroupMenu: menuTree(groups), hoverTips: tips
  };
  statusBarManager.addToStatusBar(context, item);
}

export function platformUpdateMenu(context: common.Context, groups: TrayMenuSpec[][]): void {
  statusBarManager.updateStatusBarMenu(context, menuTree(groups));
}

export async function platformUpdateIcon(context: common.Context): Promise<void> {
  const icon: statusBarManager.StatusBarIcon = await statusBarIcon(context);
  statusBarManager.updateStatusBarIcon(context, icon);
}

// hoverTips arrived in 6.0.2(22): on an older ROM the call does not exist, and
// the tip simply keeps its add-time value.
export async function platformUpdateTips(context: common.Context, tips: string): Promise<void> {
  if (typeof statusBarManager.updateStatusBarHoverTips !== 'function') {
    return;
  }
  await statusBarManager.updateStatusBarHoverTips(context, tips);
}

// Note error 1010710004: the platform refuses to remove an icon while no window
// is in the foreground. The service reports exactly what happened; the icon also
// dies with the process, so a refused removal is never a correctness problem.
export function platformRemove(context: common.Context): void {
  statusBarManager.removeFromStatusBar(context);
}

export function platformListen(): void {
  if (listening) {
    return;
  }
  statusBarManager.on('statusBarIconClick', onIconClick);
  statusBarManager.on('rightMenuClick', onRightMenuClick);
  listening = true;
}

export function platformUnlisten(): void {
  if (!listening) {
    return;
  }
  try {
    statusBarManager.off('statusBarIconClick', onIconClick);
    statusBarManager.off('rightMenuClick', onRightMenuClick);
  } finally {
    // A failed off() must not leave the flag claiming listeners are gone.
    listening = false;
  }
}
