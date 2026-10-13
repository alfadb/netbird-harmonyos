// 2026-10-13 PC 托盘（0.0.29）— host-side checks.
//
// Three parts, all against shipped sources, no device, no build, no compiler:
//   A. the PURE model (tray/NetBirdTrayModel.ets), imported BYTE-IDENTICAL (the
//      same trick run-keep-alive.mts uses): the state vocabulary mapping, the
//      hover tip and its 128-char bound, the icon variant, the state-dependent
//      menu (and the fact that Connect and Disconnect are never in it together),
//      the menu signature, the closed menu-code set and the two gates.
//   B. the WIRING of the tray: the platform module is the ONLY importer of
//      @kit.DeskTopExtensionKit, it is reached ONLY through a dynamic import
//      (so a phone never evaluates it), the six platform calls and both
//      listeners are wired, the quick operation carries the empty abilityName
//      that makes the left click an event, the device/syscap gates run BEFORE the
//      load, the API-version guards exist, error 1010710004 is named, the
//      updates are coalesced and fail-soft, and NO StatusBarViewExtensionAbility
//      is registered (this iteration has no pop-up).
//   C. the QUIT flow and its wiring into the page and the ability: the bypass flag
//      is set first, disconnect → keep-alive → tray removal → terminateSelf is the
//      order, requestQuit() is the ONE way in, the page routes the tray's action
//      rows into the SAME requestStart/requestStop it already had and registers its
//      requestStop as the flow's disconnect step, the Settings entry runs the same
//      flow, and 0.0.31's unified exit reaches the flow from
//      EntryAbility.onPrepareToTerminate too (the system-generated tray 退出).
//
// Run: node client/entry/selfcheck/run-tray.mts
// Exit 0 = all assertions pass.

import { readFileSync, mkdtempSync, writeFileSync, rmSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const ets = join(here, '..', 'src', 'main', 'ets');
const MODEL = join(ets, 'tray', 'NetBirdTrayModel.ets');
const PLATFORM = join(ets, 'tray', 'NetBirdTrayPlatform.ets');
const SERVICE = join(ets, 'tray', 'NetBirdTrayService.ets');
const QUIT = join(ets, 'tray', 'NetBirdQuitFlow.ets');
const MODEL_TEXT = readFileSync(MODEL, 'utf8');
const PLATFORM_TEXT = readFileSync(PLATFORM, 'utf8');
const SERVICE_TEXT = readFileSync(SERVICE, 'utf8');
const QUIT_TEXT = readFileSync(QUIT, 'utf8');
const PAGE = readFileSync(join(ets, 'pages', 'Index.ets'), 'utf8');
const ABILITY = readFileSync(join(ets, 'entryability', 'EntryAbility.ets'), 'utf8');
const MODULE_JSON5 = readFileSync(join(here, '..', 'src', 'main', 'module.json5'), 'utf8');
const BUILD_PROFILE = readFileSync(join(here, '..', '..', 'build-profile.json5'), 'utf8');
const APP_JSON5 = readFileSync(join(here, '..', '..', 'AppScope', 'app.json5'), 'utf8');

let passed = 0;
let failed = 0;
function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else {
    failed++;
    if (detail !== undefined) { console.log(`     ${detail}`); }
  }
}
function sliceFrom(src: string, begin: string, end: string): string {
  const at = src.indexOf(begin);
  if (at < 0) { return ''; }
  const stop = src.indexOf(end, at);
  return stop < 0 ? src.slice(at) : src.slice(at, stop);
}
function etsFiles(root: string): string[] {
  const out: string[] = [];
  const walk = (dirPath: string): void => {
    for (const entry of readdirSync(dirPath, { withFileTypes: true })) {
      const path = join(dirPath, entry.name);
      if (entry.isDirectory()) { walk(path); } else if (entry.name.endsWith('.ets')) { out.push(path); }
    }
  };
  walk(root);
  return out;
}
// Comment lines removed: an import/API pin must answer "does this MODULE import
// it", not "does this module ever mention the name" (these files document their
// own platform boundary in their headers).
function codeOnly(src: string): string {
  return src.split('\n').filter((line: string): boolean => {
    const text = line.trim();
    return !(text.startsWith('//') || text.startsWith('/*') || text.startsWith('*'));
  }).join('\n');
}

// ---- A. the pure model ------------------------------------------------------

const FIXTURES = mkdtempSync(join(tmpdir(), 'nb-tray-'));
const fixture = join(FIXTURES, 'NetBirdTrayModel.ts');
writeFileSync(fixture, readFileSync(MODEL)); // byte-identical copy of the real module

check('the model is a PURE module (no @ohos/@kit import, no platform call)',
  /^\s*import\s/m.test(codeOnly(MODEL_TEXT)) === false &&
  codeOnly(MODEL_TEXT).indexOf('@ohos') < 0 && codeOnly(MODEL_TEXT).indexOf('@kit') < 0);

try {
  const m: any = await import(pathToFileURL(fixture).href);

  // -- the closed sets --------------------------------------------------------
  const codes = [m.TRAY_MENU_CONNECT, m.TRAY_MENU_DISCONNECT, m.TRAY_MENU_OPEN, m.TRAY_MENU_QUIT];
  check('the four menu codes are non-empty and globally unique (error 1010710007)',
    codes.every((code: string): boolean => typeof code === 'string' && code.length > 0) &&
    new Set<string>(codes).size === 4);
  check('only OUR codes are known: anything else is refused, not acted on',
    codes.every((code: string): boolean => m.trayMenuCodeKnown(code) === true) &&
    m.trayMenuCodeKnown('') === false && m.trayMenuCodeKnown('leftClick') === false &&
    m.trayMenuCodeKnown('nb_tray_other') === false);

  // -- the state vocabulary (the page's own labels) ---------------------------
  check('the page\'s official labels map onto the four tray tokens',
    m.trayStateToken('已连接') === m.TRAY_STATE_CONNECTED &&
    m.trayStateToken('连接中…') === m.TRAY_STATE_TRANSITION &&
    m.trayStateToken('正在断开…') === m.TRAY_STATE_TRANSITION &&
    m.trayStateToken('需要登录') === m.TRAY_STATE_LOGIN_REQUIRED &&
    m.trayStateToken('未连接') === m.TRAY_STATE_DISCONNECTED);
  check('an unrecognised label is 未连接, never a guess',
    m.trayStateToken('') === m.TRAY_STATE_DISCONNECTED &&
    m.trayStateToken('连接失败') === m.TRAY_STATE_DISCONNECTED);

  // -- hover tips -------------------------------------------------------------
  check('the hover tip carries the app name and the state label',
    m.trayHoverTips('已连接') === `${m.TRAY_HOVER_PREFIX}已连接` &&
    m.trayHoverTips('未连接') === `${m.TRAY_HOVER_PREFIX}未连接`);
  check('an empty label falls back to 未连接 instead of an empty tip',
    m.trayHoverTips('') === `${m.TRAY_HOVER_PREFIX}未连接`);
  check('the hover tip is bounded to the platform\'s 128 characters (error 1010710005)',
    m.trayHoverTips('x'.repeat(400)).length === m.TRAY_HOVER_MAX_CHARS &&
    m.TRAY_HOVER_MAX_CHARS <= 128);

  // -- icon variant -----------------------------------------------------------
  check('the icon follows the connection state (upstream iconForState)',
    m.trayIconVariant('已连接') === m.TRAY_ICON_CONNECTED &&
    m.trayIconVariant('连接中…') === m.TRAY_ICON_IDLE &&
    m.trayIconVariant('需要登录') === m.TRAY_ICON_IDLE &&
    m.trayIconVariant('未连接') === m.TRAY_ICON_IDLE);

  // -- the menu ---------------------------------------------------------------
  const flat = (groups: any[][]): any[] => groups.reduce((all: any[], g: any[]): any[] =>
    all.concat(g), []);
  const rowsOf = (text: string): any[] => flat(m.trayMenuGroups(text));
  for (const [text, expected] of [['已连接', m.TRAY_MENU_DISCONNECT],
    ['连接中…', m.TRAY_MENU_DISCONNECT], ['正在断开…', m.TRAY_MENU_DISCONNECT],
    ['未连接', m.TRAY_MENU_CONNECT], ['需要登录', m.TRAY_MENU_CONNECT],
    ['', m.TRAY_MENU_CONNECT]] as [string, string][]) {
    const rows = rowsOf(text);
    const codesInMenu = rows.map((row: any): string => row.menuCode);
    check(`the menu for 「${text === '' ? '(no label)' : text}」 offers ${
      expected === m.TRAY_MENU_CONNECT ? '连接' : '断开'} and never both`,
      codesInMenu.indexOf(expected) >= 0 &&
      !(codesInMenu.indexOf(m.TRAY_MENU_CONNECT) >= 0 &&
        codesInMenu.indexOf(m.TRAY_MENU_DISCONNECT) >= 0) &&
      codesInMenu.indexOf(m.TRAY_MENU_OPEN) >= 0 &&
      codesInMenu.indexOf(m.TRAY_MENU_QUIT) >= 0,
      JSON.stringify(codesInMenu));
  }
  check('every state\'s menu stays inside the platform\'s 20 level-1 items',
    ['已连接', '连接中…', '需要登录', '未连接'].every((text: string): boolean =>
      rowsOf(text).length <= 20));
  check('the menu is grouped (upstream\'s separators around actions / Open / Quit)',
    m.trayMenuGroups('未连接').length === 3 &&
    m.trayMenuGroups('未连接')[0].length === 1 &&
    m.trayMenuGroups('未连接')[1][0].menuCode === m.TRAY_MENU_OPEN &&
    m.trayMenuGroups('未连接')[2][0].menuCode === m.TRAY_MENU_QUIT);
  check('row titles are the upstream menu labels (0.0.31: ONE quit row, 退出)',
    rowsOf('未连接')[0].title === '连接' &&
    rowsOf('已连接')[1].title === '打开主窗口' &&
    rowsOf('已连接')[2].title === '退出' &&
    MODEL_TEXT.indexOf('TRAY_TITLE_QUIT: string = \'退出\'') >= 0 &&
    MODEL_TEXT.indexOf('退出并断开') < 0);
  check('the menu signature is stable per state and changes across the boundary',
    m.trayMenuSignature('已连接') === m.trayMenuSignature('连接中…') &&
    m.trayMenuSignature('已连接') !== m.trayMenuSignature('未连接') &&
    m.trayMenuSignature('未连接') === m.trayMenuSignature('需要登录'));

  // -- the gates --------------------------------------------------------------
  check('only 2in1 is a candidate device (the syscap exists nowhere else)',
    m.trayDeviceSupported('2in1') === true && m.trayDeviceSupported('phone') === false &&
    m.trayDeviceSupported('tablet') === false && m.trayDeviceSupported('') === false);
  check('the platform\'s own capability answer is required on top of the device gate',
    m.traySupported('2in1', true) === true && m.traySupported('2in1', false) === false &&
    m.traySupported('phone', true) === false);
} catch (error) {
  check(`harness (model) — ${(error as Error).message}`, false);
}
rmSync(FIXTURES, { recursive: true, force: true });

// ---- B. the tray wiring -----------------------------------------------------

const allEts = etsFiles(ets);
const importersOf = (specifier: string): string[] =>
  allEts.filter((path: string): boolean =>
    codeOnly(readFileSync(path, 'utf8')).indexOf(specifier) >= 0);

check('the platform module is the ONLY importer of the PC status-bar kit',
  JSON.stringify(importersOf('@kit.DeskTopExtensionKit')) === JSON.stringify([PLATFORM]) &&
  importersOf('@hms.pcService.statusBarManager').length === 0,
  JSON.stringify([importersOf('@kit.DeskTopExtensionKit'),
    importersOf('@hms.pcService.statusBarManager')]));
check('the kit is reached ONLY through a dynamic import (a phone never evaluates it)',
  SERVICE_TEXT.indexOf("await import('./NetBirdTrayPlatform')") >= 0 &&
  SERVICE_TEXT.indexOf('@kit.DeskTopExtensionKit') < 0 &&
  SERVICE_TEXT.indexOf('@hms.pcService') < 0 &&
  PLATFORM_TEXT.indexOf('import { statusBarManager } from') >= 0);
check('the device gate and the syscap gate both run BEFORE the platform is loaded',
  SERVICE_TEXT.indexOf('trayDeviceSupported(deviceType)') >= 0 &&
  SERVICE_TEXT.indexOf('trayDeviceSupported(deviceType)') <
    SERVICE_TEXT.indexOf("await loadPlatform()") &&
  SERVICE_TEXT.indexOf("canIUse('SystemCapability.PCService.StatusBarManager')") <
    SERVICE_TEXT.indexOf("await loadPlatform()"));
check('the platform\'s own pre-flight check is asked, and a ROM that cannot answer is a NO',
  SERVICE_TEXT.indexOf('platformCapabilitySupported(context)') >= 0 &&
  PLATFORM_TEXT.indexOf("typeof statusBarManager.isStatusBarCapabilitySupported !== 'function'") >= 0);
check('every status-bar call the tray needs is wired',
  PLATFORM_TEXT.indexOf('statusBarManager.addToStatusBar(context, item)') >= 0 &&
  PLATFORM_TEXT.indexOf('statusBarManager.updateStatusBarMenu(context, menuTree(groups))') >= 0 &&
  PLATFORM_TEXT.indexOf('statusBarManager.updateStatusBarIcon(context, icon)') >= 0 &&
  PLATFORM_TEXT.indexOf('statusBarManager.updateStatusBarHoverTips(context, tips)') >= 0 &&
  PLATFORM_TEXT.indexOf('statusBarManager.removeFromStatusBar(context)') >= 0);
check('both icon events are listened to and both are removed again',
  PLATFORM_TEXT.indexOf("statusBarManager.on('statusBarIconClick', onIconClick)") >= 0 &&
  PLATFORM_TEXT.indexOf("statusBarManager.on('rightMenuClick', onRightMenuClick)") >= 0 &&
  PLATFORM_TEXT.indexOf("statusBarManager.off('statusBarIconClick', onIconClick)") >= 0 &&
  PLATFORM_TEXT.indexOf("statusBarManager.off('rightMenuClick', onRightMenuClick)") >= 0);
check('the left click is an EVENT, not a pop-up (quickOperation.abilityName = \'\')',
  PLATFORM_TEXT.indexOf("title: TRAY_QUICK_TITLE, height: TRAY_QUICK_HEIGHT_VP, abilityName: ''") >= 0 &&
  MODEL_TEXT.indexOf('TRAY_QUICK_HEIGHT_VP: number = 40') >= 0);
check('the item carries the menu tree and the hover tip from the first moment',
  PLATFORM_TEXT.indexOf('icons: icon, quickOperation: quick, statusBarGroupMenu: menuTree(groups), hoverTips: tips') >= 0);
check('menu rows are event rows: notifyOnly + the item\'s own menuCode',
  PLATFORM_TEXT.indexOf('notifyOnly: true') >= 0 &&
  PLATFORM_TEXT.indexOf('title: spec.title, menuAction: action, menuCode: spec.menuCode') >= 0);
check('the hover-tip API (6.0.2(22)+) is version-guarded before it is called',
  PLATFORM_TEXT.indexOf("typeof statusBarManager.updateStatusBarHoverTips !== 'function'") >= 0);
check('the documented 1010710004 refusal is named, not swallowed as a generic error',
  SERVICE_TEXT.indexOf('1010710004') >= 0 && SERVICE_TEXT.indexOf('no-foreground-window') >= 0 &&
  SERVICE_TEXT.indexOf('icon-dies-with-process') >= 0);
check('the state push is coalesced into ONE debounced call (no second timer)',
  SERVICE_TEXT.indexOf('TRAY_UPDATE_DEBOUNCE_MS') >= 0 &&
  SERVICE_TEXT.indexOf('state.timer = setTimeout(') >= 0 &&
  SERVICE_TEXT.indexOf('setInterval(') < 0 &&
  MODEL_TEXT.indexOf('TRAY_UPDATE_DEBOUNCE_MS: number = 500') >= 0);
check('state updates are deduped per surface (a 2 s tick must not republish)',
  SERVICE_TEXT.indexOf('if (signature !== state.menuSignature)') >= 0 &&
  SERVICE_TEXT.indexOf('if (variant !== state.iconVariant)') >= 0 &&
  SERVICE_TEXT.indexOf('if (tips !== state.tips)') >= 0);
check('every platform touch on the service side is wrapped (fail-soft)',
  (SERVICE_TEXT.match(/try \{/g) ?? []).length >= 6 &&
  (SERVICE_TEXT.match(/catch \(error/g) ?? []).length >= 6);
check('a device without the capability degrades to one line and no work',
  SERVICE_TEXT.indexOf("record(TRAY_STAGE_START, TRAY_RESULT_SKIPPED, 'device-not-2in1')") >= 0 &&
  SERVICE_TEXT.indexOf("record(TRAY_STAGE_START, TRAY_RESULT_SKIPPED, 'syscap-absent')") >= 0 &&
  SERVICE_TEXT.indexOf('feature simply stays off') >= 0);
check('the tray never claims to keep the app alive (the continuous task does)',
  SERVICE_TEXT.indexOf('does NOT keep the app alive') >= 0);
check('the monochrome 24vp icon pair is recorded as a FOLLOW-UP, not hidden',
  PLATFORM_TEXT.indexOf('FOLLOW-UP') >= 0 &&
  PLATFORM_TEXT.toUpperCase().indexOf('MONOCHROME') >= 0 &&
  PLATFORM_TEXT.indexOf('getMediaByNameSync(TRAY_APP_ICON_RESOURCE)') >= 0);
check('no StatusBarViewExtensionAbility is registered in this iteration',
  MODULE_JSON5.indexOf('statusBarView') < 0 &&
  allEts.every((path: string): boolean =>
    readFileSync(path, 'utf8').indexOf('StatusBarViewExtensionAbility') < 0));
check('the page owns the tray listener and drops it when the page goes away',
  PAGE.indexOf('setTrayActionListener((menuCode: string): void => {') >= 0 &&
  PAGE.indexOf('setTrayActionListener(undefined);') >= 0 &&
  PAGE.indexOf('setTrayActionListener((menuCode: string): void => {') <
    PAGE.indexOf('setTrayActionListener(undefined);'));
check('the state push rides the page\'s EXISTING 2 s tick with the card\'s own label',
  PAGE.indexOf('updateTrayState(this.stateText());') >= 0 &&
  PAGE.split('updateTrayState(this.stateText());').length === 2 &&
  sliceFrom(PAGE, 'private pollConnectorStatus(): void {', 'private onResumeSignal')
    .indexOf('updateTrayState(this.stateText());') >= 0);
check('the ability starts the tray once and never lets its promise escape',
  ABILITY.indexOf('startTray(this.context)') >= 0 &&
  ABILITY.indexOf('startTray(this.context).catch(') >= 0);

// ---- C. the explicit quit flow ---------------------------------------------

const trayAction = sliceFrom(PAGE, 'private onTrayAction(menuCode: string): void {',
  'private beginExplicitQuit(reason: string): void {');
check('the tray\'s action rows ride the page\'s OWN connect/stop paths',
  trayAction.indexOf('TRAY_MENU_CONNECT') >= 0 &&
  trayAction.indexOf('TRAY_MENU_CONNECT') < trayAction.indexOf('this.requestStart();') &&
  trayAction.indexOf('TRAY_MENU_DISCONNECT') >= 0 &&
  trayAction.indexOf('TRAY_MENU_DISCONNECT') < trayAction.indexOf('this.requestStop();') &&
  !trayAction.includes('vpnExtension.'));
check('退出 runs the shared quit flow, 打开主窗口 does not reach the page',
  trayAction.indexOf('TRAY_MENU_QUIT') >= 0 &&
  trayAction.indexOf('this.beginExplicitQuit(QUIT_REASON_TRAY);') >
    trayAction.indexOf('TRAY_MENU_QUIT') && !trayAction.includes('TRAY_MENU_OPEN'));
check('the Settings entry runs the SAME flow (设置 → 应用 → 退出应用)',
  PAGE.indexOf("Text('退出应用（断开 VPN）')") >= 0 &&
  PAGE.indexOf('this.beginExplicitQuit(QUIT_REASON_SETTINGS);') >= 0);
check('one quit at a time: the page latches it before doing anything',
  PAGE.indexOf('private quitInFlight: boolean = false;') >= 0 &&
  sliceFrom(PAGE, 'private beginExplicitQuit(reason: string): void {', 'private diagEvent(')
    .indexOf('this.quitInFlight = true;') >= 0);
check('the quit flow is shared, not duplicated: the page never reaches into it',
  (PAGE.match(/performExplicitQuit\(/g) ?? []).length === 0 &&
  PAGE.indexOf('requestQuit(host, reason);') >= 0 &&
  QUIT_TEXT.indexOf('export async function performExplicitQuit(') >= 0 &&
  QUIT_TEXT.indexOf('performExplicitQuit(context, reason, (): void => {') >= 0);
check('the page owns the ONE extension-stop call site and registers it with the flow',
  PAGE.indexOf('setQuitDisconnect((): void => {') >= 0 &&
  PAGE.indexOf('this.requestStop();') > PAGE.indexOf('setQuitDisconnect((): void => {') &&
  PAGE.indexOf('setQuitDisconnect(undefined);') >
    PAGE.indexOf('setQuitDisconnect((): void => {') &&
  PAGE.indexOf('setQuitDisconnect(undefined);') > PAGE.indexOf('setTrayActionListener(undefined);') &&
  QUIT_TEXT.indexOf('setQuitDisconnect(disconnect: QuitDisconnect | undefined): void') >= 0 &&
  QUIT_TEXT.indexOf('UI_QUIT_DISCONNECT_UNAVAILABLE') >= 0);
check('the bypass flag is set FIRST, before the disconnect and before any await',
  QUIT_TEXT.indexOf('explicitQuitRequested = true;') >= 0 &&
  QUIT_TEXT.indexOf('explicitQuitRequested = true;') <
    QUIT_TEXT.indexOf('disconnect();', QUIT_TEXT.indexOf('export async function performExplicitQuit(')) &&
  QUIT_TEXT.indexOf('explicitQuitRequested = true;') <
    QUIT_TEXT.indexOf('await sleep(', QUIT_TEXT.indexOf('export async function performExplicitQuit(')));
check('the exit order is upstream\'s: disconnect → keep-alive → icon → terminate',
  QUIT_TEXT.indexOf('disconnect();') < QUIT_TEXT.indexOf('stopKeepAlive(context') &&
  QUIT_TEXT.indexOf('stopKeepAlive(context') < QUIT_TEXT.indexOf('await removeTray(reason)') &&
  QUIT_TEXT.indexOf('await removeTray(reason)') < QUIT_TEXT.indexOf('await context.terminateSelf()'));
check('the borrowed task is released as SHUTDOWN (no session left)',
  QUIT_TEXT.indexOf('KEEP_ALIVE_REVISION_SHUTDOWN') >= 0 &&
  QUIT_TEXT.indexOf('stopKeepAlive(context, KEEP_ALIVE_REVISION_SHUTDOWN, reason)') >= 0);
check('the one path the app cannot act on is documented, not hidden',
  QUIT_TEXT.indexOf('THE ONE PATH THE APP CANNOT ACT ON') >= 0 &&
  QUIT_TEXT.indexOf('platform stops the VPN when the app process exits') >= 0 &&
  QUIT_TEXT.indexOf('onDestroy') >= 0 &&
  ABILITY.indexOf('PLAIN PLATFORM KILL') >= 0 &&
  ABILITY.indexOf('no code of ours to run') >= 0);
check('the keep-alive and tray steps of the quit cannot block the exit',
  (QUIT_TEXT.match(/try \{/g) ?? []).length >= 4 &&
  QUIT_TEXT.indexOf('await removeTray(reason);') > QUIT_TEXT.indexOf('catch (error'));
check('the ability reads the bypass flag and starts the flow from the quit module',
  ABILITY.indexOf("from '../tray/NetBirdQuitFlow';") >= 0 &&
  ABILITY.indexOf('import { explicitQuitInProgress,') >= 0 &&
  ABILITY.indexOf('explicitQuitInProgress()') >= 0 &&
  ABILITY.indexOf('requestQuit(this.context, QUIT_REASON_SYSTEM);') >= 0 &&
  ABILITY.indexOf('QUIT_REASON_SYSTEM') < ABILITY.indexOf('requestQuit(this.context, QUIT_REASON_SYSTEM);'));
check('the version is 31 / 0.0.31 in both profiles',
  /versionCode: 31/.test(BUILD_PROFILE) && /versionName: '0\.0\.31'/.test(BUILD_PROFILE) &&
  /versionCode: 31/.test(APP_JSON5) && /versionName: '0\.0\.31'/.test(APP_JSON5));
check('0.0.31: the system tray 退出 takes the SAME full-quit path (veto once, then quit)',
  ABILITY.indexOf('private closeToBackgroundRegistered: boolean = false;') >= 0 &&
  ABILITY.indexOf('this.closeToBackgroundRegistered = true;') >= 0 &&
  ABILITY.indexOf('if (this.closeToBackgroundRegistered) {') >= 0 &&
  ABILITY.indexOf('if (shouldVetoSystemExit(Date.now())) {') >= 0 &&
  ABILITY.indexOf("'UI_TERMINATE_GUARD|result=veto|reason=system-exit-quit'") >= 0 &&
  ABILITY.indexOf("'veto-system-exit-quit'") >= 0 &&
  ABILITY.indexOf("'UI_TERMINATE_GUARD|result=allow|reason=system-exit'") >= 0);
check('the veto-once decision is a pure module, carried by the flow (not re-derived)',
  QUIT_TEXT.indexOf("from './NetBirdQuitPolicy'") >= 0 &&
  QUIT_TEXT.indexOf('systemExitVetoDecision(systemExitVetoAtMs,') >= 0 &&
  QUIT_TEXT.indexOf('explicitQuitRequested, nowMs);') >= 0 &&
  QUIT_TEXT.indexOf('systemExitVetoAtMs: number = SYSTEM_EXIT_VETO_NONE;') >= 0 &&
  QUIT_TEXT.indexOf('SYSTEM_EXIT_VETO_WINDOW_MS: number =') < 0 &&
  readFileSync(join(ets, 'tray', 'NetBirdQuitPolicy.ets'), 'utf8')
    .indexOf('SYSTEM_EXIT_VETO_WINDOW_MS: number = 5000') >= 0);

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
