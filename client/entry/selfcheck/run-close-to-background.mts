// 2026-10-13 关闭到后台 + PC 长时任务类型（0.0.29）— host-side checks.
//
// Three parts, all against shipped sources, no device, no build, no compiler:
//   A. CLOSE-TO-BACKGROUND: EntryAbility registers windowStageClose inside
//      onWindowStageCreate and BEFORE loadContent (so the X button is covered
//      from the first frame), the callback returns true and minimises the main
//      window, both paths are try/caught and each leaves one diagnostic line —
//      and the callback does NOT stop the VPN, the task or the ability (the X is
//      a hide, not an exit).
//   B. THE FALLBACK GUARD: onPrepareToTerminate() vetoes a termination EXCEPT
//      while this process is running its own explicit quit, and the permission
//      the callback needs is declared in module.json5.
//   C. KEEP-ALIVE MODE SELECTION: the pure decision (keepAliveModeFor /
//      isKeepAliveMode, imported BYTE-IDENTICAL) plus the adapter wiring — the
//      mode is chosen AT REQUEST TIME from deviceInfo.deviceType, both literals
//      are declared, and the claim path recognises either of them. The
//      behavioural half (what the adapter actually asks the platform for, on a
//      2in1 and on a phone) is driven through the shipped request path in
//      run-keep-alive.mts, against the same staged adapter.
//
// Run: node client/entry/selfcheck/run-close-to-background.mts
// Exit 0 = all assertions pass.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const ets = join(here, '..', 'src', 'main', 'ets');
const POLICY = join(ets, 'vpnextensionability', 'NetBirdKeepAlivePolicy.ets');
const SERVICE = join(ets, 'vpnextensionability', 'NetBirdKeepAliveService.ets');
const ABILITY = join(ets, 'entryability', 'EntryAbility.ets');
const POLICY_TEXT = readFileSync(POLICY, 'utf8');
const SERVICE_TEXT = readFileSync(SERVICE, 'utf8');
const ABILITY_TEXT = readFileSync(ABILITY, 'utf8');
const MODULE_JSON5 = readFileSync(join(here, '..', 'src', 'main', 'module.json5'), 'utf8');

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

// ---- A. close to background -------------------------------------------------

const stageBody = sliceFrom(ABILITY_TEXT, 'onWindowStageCreate(windowStage: window.WindowStage): void {',
  'onForeground(): void {');
const guardBody = sliceFrom(ABILITY_TEXT, 'private registerCloseToBackground(',
  'private minimizeOnClose(');
const minimizeBody = sliceFrom(ABILITY_TEXT, 'private minimizeOnClose(', 'private recordCloseGuard(');
const closeCallback = sliceFrom(ABILITY_TEXT, "windowStage.on('windowStageClose', (): boolean => {",
  '});');

check('the interceptor is registered inside onWindowStageCreate, before loadContent',
  stageBody.indexOf('this.registerCloseToBackground(windowStage);') >= 0 &&
  stageBody.indexOf('this.registerCloseToBackground(windowStage);') <
    stageBody.indexOf("windowStage.loadContent('pages/Index'"));
check('it is windowStageClose (API 14, no permission) and its callback refuses the close',
  ABILITY_TEXT.indexOf("windowStage.on('windowStageClose', (): boolean => {") >= 0 &&
  closeCallback.indexOf('this.minimizeOnClose(windowStage);') >= 0 &&
  closeCallback.indexOf('return true;') > closeCallback.indexOf('this.minimizeOnClose(windowStage);'));
check('the async twin is deliberately NOT registered (only one of the two is answered)',
  ABILITY_TEXT.indexOf("on('windowWillClose'") < 0);
check('the X is turned into a MINIMISE of the main window',
  minimizeBody.indexOf('windowStage.getMainWindowSync().minimize()') >= 0);
check('a refused registration (801 / no syscap) is caught, logged and does not throw',
  guardBody.indexOf('try {') >= 0 && guardBody.indexOf('catch (error)') > guardBody.indexOf('try {') &&
  guardBody.indexOf('801') >= 0);
check('a refused minimise is contained too (the window simply stays open)',
  minimizeBody.indexOf('.catch((error: Error): void => {') >= 0 &&
  minimizeBody.indexOf('note=window-stays-open') >= 0);
check('one diagnostic line per path, in the existing shapes',
  ABILITY_TEXT.indexOf("this.recordLifecycle('windowStageClose');") >= 0 &&
  ABILITY_TEXT.indexOf("'UI_CLOSE_GUARD|mode=windowStageClose|result=registered'") >= 0 &&
  ABILITY_TEXT.indexOf("'UI_CLOSE_MINIMIZE|result=ok|note=background-kept'") >= 0 &&
  ABILITY_TEXT.indexOf("diagnosticsEvent('ui_close_guard', fields);") >= 0);
check('the X stops NOTHING: no VPN stop, no keep-alive stop, no terminateSelf in it',
  closeCallback.indexOf('stopKeepAlive') < 0 && closeCallback.indexOf('vpnExtension') < 0 &&
  closeCallback.indexOf('terminateSelf') < 0 &&
  closeCallback.indexOf('stopBackgroundRunning') < 0);

// ---- B. the fallback guard --------------------------------------------------

const terminateBody = sliceFrom(ABILITY_TEXT, 'onPrepareToTerminate(): boolean {',
  'private persistCredentialOverrides(');
check('onPrepareToTerminate vetoes by default and allows only OUR OWN quit',
  terminateBody.indexOf('if (explicitQuitInProgress()) {') >= 0 &&
  terminateBody.indexOf('if (explicitQuitInProgress()) {') < terminateBody.indexOf('return false;') &&
  terminateBody.indexOf('return false;') < terminateBody.indexOf('return true;') &&
  ABILITY_TEXT.indexOf("'UI_TERMINATE_GUARD|result=veto|note=close-is-background'") >= 0);
check('the permission the fallback needs is declared, inside requestPermissions',
  MODULE_JSON5.indexOf("name: 'ohos.permission.PREPARE_APP_TERMINATE'") >= 0 &&
  MODULE_JSON5.indexOf('requestPermissions') <
    MODULE_JSON5.indexOf('PREPARE_APP_TERMINATE'));
check('the veto is documented against the platform rule (windowStageClose wins)',
  ABILITY_TEXT.indexOf('once windowStageClose IS') >= 0 &&
  ABILITY_TEXT.indexOf('registered, onPrepareToTerminate is no longer called') >= 0);

// ---- C. the borrowed background mode follows the device ---------------------

const FIXTURES = mkdtempSync(join(tmpdir(), 'nb-close-bg-'));
writeFileSync(join(FIXTURES, 'NetBirdKeepAlivePolicy.ts'), POLICY_TEXT); // byte-identical

try {
  const p: any = await import(pathToFileURL(join(FIXTURES, 'NetBirdKeepAlivePolicy.ts')).href);
  check('2in1 borrows taskKeeping (the PC-only, ACL-free type)',
    p.keepAliveModeFor('2in1') === 'taskKeeping' &&
    p.KEEP_ALIVE_MODE_TASK_KEEPING === 'taskKeeping');
  check('every other device keeps borrowing dataTransfer',
    p.keepAliveModeFor('phone') === 'dataTransfer' &&
    p.keepAliveModeFor('tablet') === 'dataTransfer' &&
    p.keepAliveModeFor('') === 'dataTransfer' &&
    p.KEEP_ALIVE_MODE_DATA_TRANSFER === 'dataTransfer');
  check('the claim path knows BOTH types and nothing else',
    p.isKeepAliveMode('taskKeeping') === true && p.isKeepAliveMode('dataTransfer') === true &&
    p.isKeepAliveMode('TASKKEEPING') === true && p.isKeepAliveMode('audioPlayback') === false &&
    p.isKeepAliveMode('') === false);
} catch (error) {
  check(`harness (policy) — ${(error as Error).message}`, false);
}
rmSync(FIXTURES, { recursive: true, force: true });

check('the adapter resolves the mode AT REQUEST TIME from the device type',
  SERVICE_TEXT.indexOf("import deviceInfo from '@ohos.deviceInfo';") >= 0 &&
  SERVICE_TEXT.indexOf('return keepAliveModeFor(deviceInfo.deviceType);') >= 0 &&
  SERVICE_TEXT.indexOf('const mode: string = requestedBackgroundMode();') >= 0 &&
  SERVICE_TEXT.indexOf('await backgroundTaskManager.startBackgroundRunning(context, [mode], agent)') >= 0);
check('a device that cannot report its type falls back to the shipped type',
  SERVICE_TEXT.indexOf('const KEEP_ALIVE_MODE_FALLBACK: string = KEEP_ALIVE_MODE_DATA_TRANSFER;') >= 0 &&
  SERVICE_TEXT.indexOf('return KEEP_ALIVE_MODE_FALLBACK;') > SERVICE_TEXT.indexOf('catch (error)'));
check('the claim path recognises either known type (no duplicate request)',
  SERVICE_TEXT.indexOf('if (!isKeepAliveTask(task))') >= 0 &&
  SERVICE_TEXT.indexOf('if (isKeepAliveMode(modes[i]))') >= 0 &&
  SERVICE_TEXT.indexOf('isDataTransferTask') < 0);
check('module.json5 declares BOTH borrowed types for the UIAbility',
  /name: 'EntryAbility'[\s\S]*?backgroundModes: \[[\s\S]*?'dataTransfer'[\s\S]*?'taskKeeping'[\s\S]*?\]/
    .test(MODULE_JSON5));
check('the notification content is untouched by the type change',
  SERVICE_TEXT.indexOf('keepAliveNotificationText(status)') >= 0 &&
  SERVICE_TEXT.indexOf('KEEP_ALIVE_NOTIFICATION_TITLE') >= 0 &&
  SERVICE_TEXT.indexOf('NOTIFICATION_CONTENT_SYSTEM_LIVE_VIEW') >= 0);

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
