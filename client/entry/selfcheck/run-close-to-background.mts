// 2026-10-13 关闭到后台 + PC 长时任务类型（0.0.29）— host-side checks.
//
// Three parts, all against shipped sources, no device, no build, no compiler:
//   A. CLOSE-TO-BACKGROUND: EntryAbility registers windowStageClose inside
//      onWindowStageCreate and BEFORE loadContent (so the X button is covered
//      from the first frame), the callback returns true and minimises the main
//      window, both paths are try/caught and each leaves one diagnostic line —
//      and the callback does NOT stop the VPN, the task or the ability (the X is
//      a hide, not an exit).
//   B. THE TERMINATE GUARD (0.0.30 → 0.0.31): onPrepareToTerminate() answers
//      synchronously — it allows OUR OWN quit (the one-way flag), cancels the
//      FIRST system exit request of the process and starts the unified quit flow
//      right there (tray/NetBirdQuitFlow.requestQuit → disconnect first), and
//      vetoes ONLY on the device where the windowStageClose registration failed
//      (the fallback's whole reason to exist); the permission the callback needs
//      is declared in module.json5.
//   D. THE VETO-ONCE SAFEGUARD (0.0.31): the pure decision behind that answer
//      (tray/NetBirdQuitPolicy.ets), imported BYTE-IDENTICAL and driven at its
//      exact boundaries — first request cancelled once, a second attempt inside
//      the window let through (an app that refuses to exit is the failure mode),
//      our own quit always let through.
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
  'onForeground(): void {');
check('onPrepareToTerminate allows OUR OWN quit first (the app asked to leave)',
  terminateBody.indexOf('if (explicitQuitInProgress()) {') >= 0 &&
  terminateBody.indexOf('if (explicitQuitInProgress()) {') <
    terminateBody.indexOf("'allow-explicit-quit'") &&
  terminateBody.indexOf("'allow-explicit-quit'") <
    terminateBody.indexOf('if (this.closeToBackgroundRegistered) {') &&
  terminateBody.indexOf('return false;') < terminateBody.indexOf('return true;'));
check('0.0.31: a real system exit cancels ONCE and starts the FULL quit flow',
  terminateBody.indexOf('if (shouldVetoSystemExit(Date.now())) {') >= 0 &&
  terminateBody.indexOf('requestQuit(this.context, QUIT_REASON_SYSTEM);') >
    terminateBody.indexOf('if (shouldVetoSystemExit(Date.now())) {') &&
  terminateBody.indexOf('requestQuit(this.context, QUIT_REASON_SYSTEM);') <
    terminateBody.indexOf('return true;') &&
  terminateBody.indexOf("'UI_TERMINATE_GUARD|result=veto|reason=system-exit-quit'") >= 0 &&
  terminateBody.indexOf("'veto-system-exit-quit'") >= 0);
check('the answer is synchronous: no await anywhere in the guard',
  terminateBody.indexOf('await ') < 0 && terminateBody.indexOf('async ') < 0 &&
  terminateBody.indexOf('requestQuit(this.context, QUIT_REASON_SYSTEM);') <
    terminateBody.indexOf('return true;'));
check('a request that must not be cancelled is let through (second attempt / ours)',
  terminateBody.indexOf("'UI_TERMINATE_GUARD|result=allow|reason=system-exit'") >
    terminateBody.indexOf('requestQuit(this.context, QUIT_REASON_SYSTEM);') &&
  terminateBody.indexOf("'allow-system-exit'") >
    terminateBody.indexOf('requestQuit(this.context, QUIT_REASON_SYSTEM);'));
check('the veto survives ONLY as the last arm (the device where the registration failed)',
  (terminateBody.match(/return true;/g) ?? []).length === 2 &&
  terminateBody.indexOf("'UI_TERMINATE_GUARD|result=veto|note=close-is-background'") >
    terminateBody.indexOf('if (this.closeToBackgroundRegistered) {') &&
  terminateBody.indexOf("'UI_TERMINATE_GUARD|result=veto|note=close-is-background'") <
    terminateBody.lastIndexOf('return true;'));
check('the guard does the CLEANUP nowhere: the flow owns every step',
  terminateBody.indexOf('stopKeepAlive') < 0 && terminateBody.indexOf('removeTray') < 0 &&
  terminateBody.indexOf('vpnExtension') < 0 && terminateBody.indexOf('terminateSelf') < 0 &&
  ABILITY_TEXT.indexOf("from '../tray/NetBirdQuitFlow';") >= 0);
check('the flag is set only where the platform ACCEPTED the registration',
  guardBody.indexOf('this.closeToBackgroundRegistered = true;') >
    guardBody.indexOf("windowStage.on('windowStageClose'") &&
  guardBody.indexOf('this.closeToBackgroundRegistered = true;') < guardBody.indexOf('catch (error)'));
check('the flag starts false, is written once and read once',
  ABILITY_TEXT.indexOf('private closeToBackgroundRegistered: boolean = false;') >= 0 &&
  (ABILITY_TEXT.match(/this\.closeToBackgroundRegistered/g) ?? []).length === 2);
check('the device evidence that drove the fix is recorded in the code',
  ABILITY_TEXT.indexOf('DEVICE EVIDENCE (0.0.29, PC, netbird-ui.log, UI pid 47704') >= 0 &&
  ABILITY_TEXT.indexOf('holds for the X route only here') >= 0 &&
  ABILITY_TEXT.indexOf('windowStageClose-unavailable') >= 0);
check('the permission the fallback needs is declared, inside requestPermissions',
  MODULE_JSON5.indexOf("name: 'ohos.permission.PREPARE_APP_TERMINATE'") >= 0 &&
  MODULE_JSON5.indexOf('requestPermissions') <
    MODULE_JSON5.indexOf('PREPARE_APP_TERMINATE'));

// ---- D. the veto-once safeguard (pure decision) -----------------------------

const QUIT_POLICY = join(ets, 'tray', 'NetBirdQuitPolicy.ets');
const QUIT_POLICY_TEXT = readFileSync(QUIT_POLICY, 'utf8');
const POLICY_FIXTURES = mkdtempSync(join(tmpdir(), 'nb-quit-policy-'));
writeFileSync(join(POLICY_FIXTURES, 'NetBirdQuitPolicy.ts'), QUIT_POLICY_TEXT);

check('the quit policy is a PURE module (no import, no platform call, no clock)',
  /^\s*import\s/m.test(QUIT_POLICY_TEXT) === false &&
  QUIT_POLICY_TEXT.indexOf('@ohos') < 0 && QUIT_POLICY_TEXT.indexOf('@kit') < 0 &&
  QUIT_POLICY_TEXT.indexOf('Date.now') < 0);

try {
  const q: any = await import(pathToFileURL(join(POLICY_FIXTURES, 'NetBirdQuitPolicy.ts')).href);
  const W: number = q.SYSTEM_EXIT_VETO_WINDOW_MS;
  const NONE: number = q.SYSTEM_EXIT_VETO_NONE;
  const decide = (vetoedAtMs: number, explicit: boolean, nowMs: number): any =>
    q.systemExitVetoDecision(vetoedAtMs, explicit, nowMs);

  check('the window is the bounded handful of seconds the guard promises',
    W === 5000 && NONE === -1);
  check('the FIRST system exit of a process is cancelled ONCE, and the time is kept',
    decide(NONE, false, 1000).veto === true &&
    decide(NONE, false, 1000).vetoedAtMs === 1000);
  check('a SECOND attempt inside the window is let through (the user is never stuck)',
    decide(1000, false, 1001).veto === false &&
    decide(1000, false, 1001).vetoedAtMs === 1000 &&
    decide(1000, false, 5999).veto === false);
  check('the boundary is exact: at W ms the request counts as a FRESH one',
    decide(1000, false, 1000 + W - 1).veto === false &&
    decide(1000, false, 1000 + W).veto === true &&
    decide(1000, false, 1000 + W).vetoedAtMs === 1000 + W);
  check('our OWN quit always passes, whatever the clock says',
    decide(NONE, true, 1000).veto === false &&
    decide(1000, true, 1001).veto === false &&
    decide(1000, true, 999999).veto === false &&
    decide(NONE, true, 1000).vetoedAtMs === NONE);
  check('it is pure: the caller carries the state, the decision invents none',
    JSON.stringify(decide(NONE, false, 1000)) === JSON.stringify(decide(NONE, false, 1000)) &&
    decide(1000, false, 1001).vetoedAtMs === 1000 &&
    QUIT_POLICY_TEXT.indexOf('export function systemExitVetoDecision(') >= 0);
} catch (error) {
  check(`harness (quit policy) — ${(error as Error).message}`, false);
}
rmSync(POLICY_FIXTURES, { recursive: true, force: true });

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
