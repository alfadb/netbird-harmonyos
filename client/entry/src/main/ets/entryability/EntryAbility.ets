import AbilityConstant from '@ohos.app.ability.AbilityConstant';
import { Configuration } from '@ohos.app.ability.Configuration';
import ConfigurationConstant from '@ohos.app.ability.ConfigurationConstant';
import UIAbility from '@ohos.app.ability.UIAbility';
import Want from '@ohos.app.ability.Want';
import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
import resourceManager from '@ohos.resourceManager';
import util from '@ohos.util';
import window from '@ohos.window';
import { appendSystemEvent, event as diagnosticsEvent, start as startDiagnostics,
  stop as stopDiagnostics } from '../vpnextensionability/NetBirdDiagnostics';
import type { DiagFields } from '../vpnextensionability/NetBirdDiagnostics';
import { start as startSystemEvents,
  stop as stopSystemEvents } from '../vpnextensionability/NetBirdSystemEvents';
// 2026-10-13 PC 托盘（0.0.29）: the UI-process status-bar icon (2in1 only) and the
// shared explicit-quit flag. Both modules are fail-soft by construction — on a
// phone, on a ROM without the PC status-bar capability, or on a device where the
// kit's runtime module does not exist, startTray() answers false after one
// diagnostic line and nothing else in this ability changes.
import { startTray } from '../tray/NetBirdTrayService';
import { explicitQuitInProgress } from '../tray/NetBirdQuitFlow';
// 2026-10-10 后台保活（0.0.19 实验版，方案 A）：借用 DATA_TRANSFER 长时任务让整个应用
// 的进程不被挂起。长时任务只能由 UIAbility 申请，所以注册与注销都落在本 ability 的生命
// 周期上；平台调用全部在 NetBirdKeepAliveService 里，本文件只做接线。
import { KEEP_ALIVE_REVISION_SHUTDOWN, notifyKeepAliveResume, registerKeepAlive,
  stopKeepAlive,
  unregisterKeepAlive } from '../vpnextensionability/NetBirdKeepAliveService';
// 0.0.21（挂起长窗口后的恢复复查）：onForeground 是本进程"又开始跑了"的最早信号，它的
// reason token 与平台的 continuousTaskActive 一起构成 `ui_resume_check` 的闭集。
import { KEEP_ALIVE_RESUME_FOREGROUND } from '../vpnextensionability/NetBirdKeepAlivePolicy';

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';
const BUNDLE_NAME: string = 'cn.alfadb.netbird';

// Theme (2026-10-09): the base/dark color resources the window's system bars are
// derived from. They live in entry/src/main/resources/base/element/color.json
// (light) and .../dark/element/color.json (dark) and are resolved HERE BY NAME
// (resourceManager.getColorByNameSync, @since 10 — verified in the local SDK d.ts
// at .../ets/api/@ohos.resourceManager.d.ts), so the bars carry exactly the
// palette value the page widgets get for the same mode. Name lookup rather than
// $r(...).id deliberately: $r is an ArkUI declaration whose resource transform is
// a UI-syntax pass, and this file is a plain ability (no struct) — the name API
// needs no transform, and the names are checked against both resource files.
const BAR_STATUS_BACKGROUND_RESOURCE: string = 'nb_window_status_bar_background';
const BAR_NAVIGATION_BACKGROUND_RESOURCE: string = 'nb_window_navigation_bar_background';
const BAR_CONTENT_RESOURCE: string = 'nb_window_system_bar_content';

// Development-time credential delivery via want parameters.
//
// Why: on device the app sandbox cannot see /data/local/tmp (verified
// 2026-09-13: the VpnExtension read path fell back to filesDir and aborted
// with "missing file under filesDir: netbird-setup-key"). So the launcher
// injects the development config as want parameters instead:
//
//   aa start -b cn.alfadb.netbird -a EntryAbility \
//     --ps netbird_config '<json>' --ps netbird_setup_key '<key>'
//
// These values are written into this app's own filesDir, from where the
// existing credential reader picks them up. DEVELOPMENT ONLY: the files sit in
// plain application storage (not secure storage) and only exist because the
// operator explicitly launched the app with these parameters. Values are never
// logged (only file name + byte count).
const CONFIG_PARAM: string = 'netbird_config';
const SETUP_KEY_PARAM: string = 'netbird_setup_key';
const PROTECT_DEVIATION_PARAM: string = 'protectDeviation';
const CONFIG_FILE: string = 'netbird-device-config.json';
const SETUP_KEY_FILE: string = 'netbird-setup-key';
// Handed to the VpnExtensionAbility through filesDir rather than through its
// want: Index.buildWant() constructs the extension want itself and does not
// forward launcher parameters (device-verified 2026-09-14: the extension
// never saw --ps protectDeviation).
const PROTECT_DEVIATION_FILE: string = 'netbird-protect-deviation';
// N2-H arm private channel (2026-09-18): the non-secret diagnostic arm
// ticket. The `n2h_arm_options` parameter carries ONLY diagnostic window
// options (posctl_port / probe_wait_ms / peer_b2a_probe_id) — never
// setup-key / private-key material, which keep their dedicated parameters
// above. The value is written verbatim into this app's OWN filesDir under the
// SAME basename the VpnExtensionAbility reads: /data/local/tmp is invisible
// to the app sandbox (device 19:49-19:50 — accessSync answered boolean false
// while the shell listed the file), so the shared path is no longer used.
// A launch WITHOUT the parameter deletes a leftover ticket so a normal start
// cannot open the diagnostic window (default: no arm). Payload is never
// logged — file name + byte count on success, error CODE on failure.
const N2H_ARM_OPTIONS_PARAM: string = 'n2h_arm_options';
const N2H_ARM_OPTIONS_FILE: string = 'netbird-n2h-arm';
// N2 T5a「B 断流刺激」private channel (2026-09-28): the SAME non-secret
// want-parameter mechanism as `n2h_arm_options`, with its own payload file. The
// basename must match the extension's `N2_STIMULUS_FILE_NAME`
// (NetBirdN2Stimulus.ets) exactly — the two abilities share filesDir, not
// imports (the same literal-basename convention as the arm ticket above).
// An EXPLICIT parameter writes the one pending stimulus; every launch without
// it deletes a leftover so a normal start cannot fire a stale stimulus. The
// extension consumes (deletes) the file when it applies it, so one launch =
// one stimulus.
const N2_STIMULUS_PARAM: string = 'n2_stimulus';
const N2_STIMULUS_FILE: string = 'netbird-n2-stimulus';
// N2 T3 §3 positive-control private channel (2026-09-28): the SAME
// non-secret want-parameter mechanism as `n2_stimulus`, with its own payload
// file. The basename must match the extension's `N2_CONTROL_FILE_NAME`
// (NetBirdN2ControlRequest.ets) exactly — the two abilities share filesDir,
// not imports (the same literal-basename convention as the arm ticket and the
// stimulus above). ONE launch carries at most ONE pending request: a launch
// with BOTH `n2_stimulus` and `n2_control` writes NEITHER (logged as
// N2_CONTROL_COMBO_REJECTED) and removes both leftovers, and a launch with
// only one of them removes the other's leftover — so a stale request can
// never fire later and the executor's three separate calls (pre-control ->
// production event -> post-control) each carry exactly the request they mean.
// The payload is never logged.
const N2_CONTROL_PARAM: string = 'n2_control';
const N2_CONTROL_FILE: string = 'netbird-n2-control';
// N2 P4「A 列生产事件触发原语」(2026-09-28): the SAME non-secret
// want-parameter mechanism again, with its own payload file. The basename must
// match the extension's `N2_A_EVENT_FILE_NAME` (NetBirdN2AEvent.ets) exactly.
// The combination rule now spans THREE channels: a launch carrying more than
// one of {n2_stimulus, n2_control, n2_a_event} writes NEITHER and removes all
// leftovers (the two-channel refusal keeps its exact pre-existing line; a
// refusal involving the A-event channel adds `|a_event=`), and a launch with
// exactly one writes it while removing the other two leftovers. The payload is
// never logged.
const N2_A_EVENT_PARAM: string = 'n2_a_event';
const N2_A_EVENT_FILE: string = 'netbird-n2-a-event';
// N2 first batch C/D columns (2026-09-29): the SAME non-secret want-parameter
// mechanism again, with its own payload file. The basename must match the
// extension's `N2_CD_FILE_NAME` (NetBirdN2CdPlan.ets) exactly. This is the
// channel the HOST uses while it still has a live link (Wi-Fi) to hand the App
// one switching-sandwich plan (`{"column":"C","kind":"M","rep":1,"id":"…"}`),
// which the App then executes alone during the cellular phase.
const N2_CD_PLAN_PARAM: string = 'n2_cd_plan';
const N2_CD_PLAN_FILE: string = 'netbird-n2-cd-plan';
// N2 first batch C/D columns (2026-09-29): the ring FETCH fallback channel
// (`{"start":0,"count":64}`) — the extension re-emits a bounded window of the
// armed-only app-internal evidence ring on HiLog. The basename must match
// `N2_RING_DUMP_FILE_NAME` (NetBirdN2Ring.ets).
const N2_RING_DUMP_PARAM: string = 'n2_ring_dump';
const N2_RING_DUMP_FILE: string = 'netbird-n2-ring-dump';
// N2 first batch C/D columns (2026-09-29): the EXPLICIT host ACK channel
// (`{"session":"<arm token>","up_to_seq":<durable seq>}`) — the ONLY thing that
// may advance the ring's retrieved watermark, and therefore the only thing that
// can release the oldest segment to rotation. The host sends it only AFTER it
// has persisted the fetched ring bytes and verified the window is complete; the
// extension forwards the document verbatim and the CORE binds it to the current
// arm. The basename must match `N2_RING_ACK_FILE_NAME` (NetBirdN2Ring.ets).
const N2_RING_ACK_PARAM: string = 'n2_ring_ack';
const N2_RING_ACK_FILE: string = 'netbird-n2-ring-ack';

interface BusinessFailure extends Error {
  code?: number;
}

export default class EntryAbility extends UIAbility {
  // Theme (2026-10-09): the one WindowStage this ability loads its page into.
  // Kept only so onConfigurationUpdate — which receives no stage — can re-derive
  // the status/navigation bar colors after a system dark/light switch.
  private mainWindowStage: window.WindowStage | undefined = undefined;

  // 0.0.30 PC 托盘退出修复: did windowStage.on('windowStageClose') actually take?
  // This is the ONE fact that tells the two close routes apart in
  // onPrepareToTerminate() below. It is written in exactly one place (the success
  // arm of registerCloseToBackground) and read in exactly one place, so it can
  // never disagree with what the platform accepted.
  private closeToBackgroundRegistered: boolean = false;

  onCreate(want: Want, launchParam: AbilityConstant.LaunchParam): void {
    // Diagnostics (earliest safe point): a fresh UI process opens its own
    // observation run and its own marker before anything else in this callback
    // can fail or return. Role 'ui' writes only netbird-ui.log* and
    // netbird-diag-ui.marker; the helper never reads a want parameter, a
    // credential or a status document.
    startDiagnostics(this.context.filesDir, 'ui');
    // System-event watcher (same role-consistent process): the platform delivers
    // APP_CRASH / APP_FREEZE / RESOURCE_OVERLIMIT / APP_KILLED here, and every
    // already-projected, bounded line is persisted through the diagnostics
    // writer's validated ingress — NOT through event('x', {detail: line}),
    // whose 48-char value bound would cut an APP_KILLED reason off the record.
    // Subscribed after the writer is open so every line has a destination;
    // start() never throws and a platform refusal is just one more line.
    startSystemEvents('ui', (line: string): void => { appendSystemEvent(line); });
    // 2026-10-10 后台保活（实验，方案 A）：注册三个长时任务回调。此时诊断写入器已经
    // 打开，取消/挂起/激活事件（以及注册失败）都有落点。注册本身不改任何状态：要不要
    // 持有任务由纯策略按页面每 2 s 上报的事实决定，本调用只建立事件通道。
    registerKeepAlive(this.context);
    // 2026-10-13 PC 托盘（0.0.29）:接入状态栏图标（仅 2in1）。本调用只做设备/系统能力
    // 自检并把图标挂上去；连接状态由页面在它已有的 2 s 轮询里推进来，托盘本身不读任何
    // 状态文档、不新增定时器。手机/平板/旧 ROM 上它会自己关掉并只留一条诊断。
    startTray(this.context).catch((error: Error): void => {
      // startTray 自身已经把每一种失败都记成一条 ui_tray；这里只是不让它的 Promise 逃逸。
      hilog.warn(DOMAIN, TAG, 'ENTRY_TRAY_START_FAILED|code=%{public}s',
        String((error as BusinessFailure).code ?? 'none'));
    });
    hilog.info(DOMAIN, TAG, 'ENTRY_ONCREATE|bundle=%{public}s', want.bundleName ?? BUNDLE_NAME);
    this.recordLifecycle('onCreate');
    // LaunchParam, SDK fields by their exact names: `lastExitReason` is the
    // LastExitReason enum identifying how the PREVIOUS run of this UI process
    // terminated. Only that bounded enum value is recorded (numeric, or a
    // string enum token on a build that delivers one) — `lastExitMessage` is
    // deliberately NEVER read or logged, because the SDK documents it as a
    // free-form detailed message and this ability must not copy launcher/want
    // text into its own log.
    const lastExitReason: AbilityConstant.LastExitReason | undefined = launchParam.lastExitReason;
    if (lastExitReason !== undefined && lastExitReason !== null) {
      const exitFields: DiagFields = {};
      exitFields['reason'] = lastExitReason;
      diagnosticsEvent('ui_last_exit', exitFields);
    }
    this.removePreviousStatus();
    this.persistCredentialOverrides(want);
    // Theme (2026-10-09): pin THIS app's color mode to "follow the system" —
    // app-scoped only, no device-wide theme is touched. Placed after the
    // existing diagnostic / credential steps so those run exactly as before.
    this.followSystemColorMode();
  }

  // Lifecycle observations carry ONE whitelisted scalar field, `stage`, with
  // the exact callback name. No want/URL/setup-key text is ever passed. The
  // record is built as a typed Record plus a key write, the same shape the
  // diagnostics module uses for its own fields: ArkTS rejects a bare object
  // literal in this position (arkts-no-untyped-obj-literals).
  private recordLifecycle(stage: string): void {
    const fields: DiagFields = {};
    fields['stage'] = stage;
    diagnosticsEvent('ui_lifecycle', fields);
  }

  // A fresh UI process cannot infer liveness from a previous process's file:
  // force-stop leaves its last connected snapshot behind. A live extension
  // publishes again on its next tick.
  private removePreviousStatus(): void {
    const path: string = `${this.context.filesDir}/netbird-status.json`;
    try {
      if (fs.accessSync(path)) {
        fs.unlinkSync(path);
      }
    } catch (error) {
      hilog.warn(DOMAIN, TAG, 'ENTRY_STATUS_REMOVE_FAILED|code=%{public}s',
        String((error as BusinessFailure).code ?? 'none'));
    }
  }

  // ---------------------------------------------------------------------------
  // Theme (2026-10-09): automatic system dark/light following.
  //
  // Automatic only — this app offers no manual light/dark switch and writes no
  // device-wide theme; it simply stops overriding the system's color mode and
  // lets the base/dark resource set do the rest:
  //   * every page/widget color is a $r('app.color.nb_*') resource, resolved by
  //     the platform from entry/src/main/resources/base/element/color.json
  //     (light) or .../dark/element/color.json (dark);
  //   * the window's status/navigation bars are the ONE place the native API
  //     takes literal color strings instead of resources, so the same palette
  //     entries are resolved by name here (see BAR_*_RESOURCE above).
  // Every failure is contained: it is logged (code only, never a message
  // carrying platform text) and the affected surface is left as it was — a
  // fixed white is never forced.
  // ---------------------------------------------------------------------------

  // Pins THIS APP's color mode to "follow the system"
  // (COLOR_MODE_NOT_SET, which is also the platform default for an app that
  // never calls this API). App-scoped: setColorMode only affects this
  // application's own configuration.
  private followSystemColorMode(): void {
    try {
      this.context.getApplicationContext().setColorMode(
        ConfigurationConstant.ColorMode.COLOR_MODE_NOT_SET);
      hilog.info(DOMAIN, TAG, 'THEME_COLOR_MODE|value=not-set|note=follow-system-app-scope-only');
    } catch (error) {
      // Harmless by construction: an app that never calls setColorMode already
      // follows the system, so a failure here changes nothing — the resources
      // and the bars below still follow the system mode.
      hilog.warn(DOMAIN, TAG, 'THEME_COLOR_MODE_FAILED|code=%{public}s|note=follow-system-default-kept',
        String((error as BusinessFailure).code ?? 'none'));
    }
  }

  private refreshSystemBarTheme(): void {
    const windowStage: window.WindowStage | undefined = this.mainWindowStage;
    if (windowStage === undefined) {
      // The configuration changed before onWindowStageCreate stored the stage:
      // there is no window to theme yet, and the initial window load applies the
      // (already current) mode in its own callback.
      hilog.info(DOMAIN, TAG, 'THEME_BAR_SKIPPED|note=no-window-stage-yet');
      return;
    }
    this.applySystemBarTheme(windowStage);
  }

  private applySystemBarTheme(windowStage: window.WindowStage): void {
    let statusBarBackground: string = '';
    let navigationBarBackground: string = '';
    let barContent: string = '';
    try {
      const manager: resourceManager.ResourceManager = this.context.resourceManager;
      statusBarBackground = this.barColorString(
        manager.getColorByNameSync(BAR_STATUS_BACKGROUND_RESOURCE));
      navigationBarBackground = this.barColorString(
        manager.getColorByNameSync(BAR_NAVIGATION_BACKGROUND_RESOURCE));
      barContent = this.barColorString(manager.getColorByNameSync(BAR_CONTENT_RESOURCE));
    } catch (error) {
      hilog.warn(DOMAIN, TAG,
        'THEME_BAR_RESOLVE_FAILED|code=%{public}s|note=system-bar-colors-untouched',
        String((error as BusinessFailure).code ?? 'none'));
      return;
    }
    // A typed literal: ArkTS rejects a bare object literal in this position
    // (arkts-no-untyped-obj-literals), the same reason recordLifecycle builds
    // its record through a typed local.
    const properties: window.SystemBarProperties = {
      statusBarColor: statusBarBackground,
      statusBarContentColor: barContent,
      navigationBarColor: navigationBarBackground,
      navigationBarContentColor: barContent
    };
    try {
      const mainWindow: window.Window = windowStage.getMainWindowSync();
      mainWindow.setWindowSystemBarProperties(properties).then((): void => {
        hilog.info(DOMAIN, TAG,
          'THEME_BAR_APPLIED|status=%{public}s|navigation=%{public}s|content=%{public}s',
          statusBarBackground, navigationBarBackground, barContent);
      }).catch((error: Error): void => {
        hilog.warn(DOMAIN, TAG,
          'THEME_BAR_APPLY_FAILED|code=%{public}s|note=system-bar-colors-untouched',
          String((error as BusinessFailure).code ?? 'none'));
      });
    } catch (error) {
      hilog.warn(DOMAIN, TAG,
        'THEME_BAR_WINDOW_FAILED|code=%{public}s|note=system-bar-colors-untouched',
        String((error as BusinessFailure).code ?? 'none'));
    }
  }

  // The resource manager resolves the color against the app's current
  // configuration (an ARGB number); the native bar fields take an RGB string, so
  // only the alpha is dropped. The resolved resource is already the dark value
  // whenever the app is in dark mode — exactly the value the page widgets get.
  private barColorString(argb: number): string {
    let hex: string = (argb & 0xFFFFFF).toString(16);
    while (hex.length < 6) {
      hex = '0' + hex;
    }
    return '#' + hex.toUpperCase();
  }

  onNewWant(want: Want, _launchParam: AbilityConstant.LaunchParam): void {
    hilog.info(DOMAIN, TAG, 'ENTRY_ONNEWWANT|bundle=%{public}s', want.bundleName ?? BUNDLE_NAME);
    this.recordLifecycle('onNewWant');
    this.persistCredentialOverrides(want);
  }

  onWindowStageCreate(windowStage: window.WindowStage): void {
    // Theme (2026-10-09): remembered for onConfigurationUpdate, which is handed
    // no stage. Nothing else about this callback changes.
    this.mainWindowStage = windowStage;
    // 2026-10-13 关闭到后台（0.0.29）—— registered BEFORE loadContent so the X
    // button is covered from the first frame on.
    this.registerCloseToBackground(windowStage);
    windowStage.loadContent('pages/Index', (error) => {
      if (error.code) {
        hilog.error(DOMAIN, TAG, 'ENTRY_LOAD_REJECTED|code=%{public}d', error.code);
        return;
      }
      hilog.info(DOMAIN, TAG, 'ENTRY_PAGE_READY|bundle=%{public}s', BUNDLE_NAME);
      // Theme: initial window load — the bars are derived from the same
      // base/dark color resources the just-loaded page uses.
      this.applySystemBarTheme(windowStage);
    });
  }

  // ---------------------------------------------------------------------------
  // 2026-10-13 关闭到后台（0.0.29）—— "X hides the app, it does not quit it".
  //
  // THE UPSTREAM CONTRACT (client/ui/main.go:293-299, :382-388): the desktop
  // client sets DisableQuitOnLastWindowClosed (macOS:
  // ApplicationShouldTerminateAfterLastWindowClosed=false) and a WindowClosing
  // hook that simply forgets the main window — the app keeps running, the tunnel
  // is a separate daemon and is untouched. HarmonyOS has no "hide" for a main
  // window (no Window.hide()), so the equivalent is: REFUSE the close and
  // MINIMISE the window ourselves. The ability, the UI process, the page, its
  // 2 s tick, the continuous task, the notification and the :vpn extension all
  // stay exactly as they were; the app stays in the Dock and comes back when it
  // is clicked (onForeground / onPageShow).
  //
  // THE API: windowStage.on('windowStageClose', cb) — @ohos.window.d.ts:9610
  // (off :9630), API 14, SystemCapability.Window.SessionManager, no permission.
  // The callback's boolean return value is the contract: true = DO NOT close the
  // main window, false = close. The d.ts declares Callback<void> because the type
  // system has no room for the return value (documented in the SDK docs);
  // windowWillClose (same WindowStage, :6154) is the async form of the same
  // interception and is deliberately NOT registered — when both are registered
  // only windowWillClose is answered.
  //
  // REGISTRATION MAY FAIL: before HarmonyOS 6.1 the call answers error 801 on a
  // device that is not in free-window mode (and on a ROM without the syscap at
  // all), so it is try/caught and the failure is ONE diagnostic line. Whether it
  // took is remembered in closeToBackgroundRegistered, because that single fact
  // is what onPrepareToTerminate() below uses to tell the X route from a real
  // termination request. onPrepareToTerminate() (needs
  // ohos.permission.PREPARE_APP_TERMINATE, normal/system_grant, declared in
  // module.json5) is the FALLBACK for the device where this registration fails.
  // ---------------------------------------------------------------------------
  private registerCloseToBackground(windowStage: window.WindowStage): void {
    try {
      windowStage.on('windowStageClose', (): boolean => {
        // One line per path, in the existing diagnostic shape: `stage` is the
        // callback's own name.
        this.recordLifecycle('windowStageClose');
        this.minimizeOnClose(windowStage);
        // true = do not close the main window.
        return true;
      });
      // The registration was accepted: from here on the X button is this
      // callback's business and a termination request can mean only one thing
      // (see onPrepareToTerminate).
      this.closeToBackgroundRegistered = true;
      hilog.info(DOMAIN, TAG, 'UI_CLOSE_GUARD|mode=windowStageClose|result=registered');
    } catch (error) {
      // 801 on a non-free-window device, or the API missing on an old ROM: the
      // page keeps its pre-0.0.29 behaviour for the X button, and
      // onPrepareToTerminate() stays the fallback that must veto.
      const code: number | undefined = (error as BusinessFailure).code;
      hilog.warn(DOMAIN, TAG, 'UI_CLOSE_GUARD|mode=windowStageClose|result=error|code=%{public}s',
        String(code ?? 'none'));
      this.recordCloseGuard('windowStageClose-unavailable',
        code === undefined ? 'none' : String(code));
    }
  }

  // The X really is a "hide to background": minimise the main window. Failure is
  // contained (the window simply stays open, which is the pre-0.0.29 behaviour)
  // and reported with the platform's own code.
  private minimizeOnClose(windowStage: window.WindowStage): void {
    try {
      windowStage.getMainWindowSync().minimize().then((): void => {
        hilog.info(DOMAIN, TAG, 'UI_CLOSE_MINIMIZE|result=ok|note=background-kept');
        this.recordCloseGuard('minimize', 'ok');
      }).catch((error: Error): void => {
        hilog.warn(DOMAIN, TAG, 'UI_CLOSE_MINIMIZE|result=error|code=%{public}s',
          String((error as BusinessFailure).code ?? 'none'));
        this.recordCloseGuard('minimize', String((error as BusinessFailure).code ?? 'none'));
      });
    } catch (error) {
      hilog.warn(DOMAIN, TAG, 'UI_CLOSE_MINIMIZE|result=error|code=%{public}s|note=window-stays-open',
        String((error as BusinessFailure).code ?? 'none'));
      this.recordCloseGuard('minimize', String((error as BusinessFailure).code ?? 'none'));
    }
  }

  // `ui_close_guard`: which close path ran and what it answered. Both fields are
  // bounded tokens (a callback name, `ok` or the platform's numeric code) — never
  // platform text.
  private recordCloseGuard(path: string, result: string): void {
    const fields: DiagFields = {};
    fields['stage'] = path;
    fields['result'] = result;
    diagnosticsEvent('ui_close_guard', fields);
  }

  // 2026-10-13 关闭到后台（0.0.29）；2026-10-13 系统托盘退出修复（0.0.30）—— the guard
  // that separates "the user closed the window" from "the user asked the app to
  // exit", and the only place that knows whether a termination is OUR OWN.
  //
  // onPrepareToTerminate(): @ohos.app.ability.UIAbility.d.ts:681, API 10,
  // @permission ohos.permission.PREPARE_APP_TERMINATE (normal / system_grant —
  // granted at install, no ACL), executed on 2in1 only. The SDK documents it as
  // "the user clicks the close button or exits from the dock / system tray", and
  // returning true CANCELS the termination.
  //
  // DEVICE EVIDENCE (0.0.29, PC, netbird-ui.log, UI pid 47704 — the user's test):
  //   * 15:45:40.884 / 15:45:49.209 / 15:45:57.054 — three clicks on the
  //     SYSTEM-GENERATED tray 退出 each produced `ui_close_guard result=veto
  //     stage=onPrepareToTerminate` and NOTHING else: no `windowStageClose`
  //     record, no onDestroy. The app stayed up, which is exactly the bug.
  //   * the same process had registered windowStageClose successfully (the log
  //     contains no `windowStageClose-unavailable` record), and in the earlier
  //     pid 41730 the X button produced `windowStageClose` + `minimize ok` and
  //     never onPrepareToTerminate.
  //   => on this ROM the two routes are disjoint and each fires ONE callback:
  //      the X reaches windowStageClose; the tray 退出 reaches
  //      onPrepareToTerminate. (The SDK's own note — "if ... a listener for
  //      window.WindowStage.on('windowStageClose') [is registered], this callback
  //      function is not executed" — holds for the X route only here; the system
  //      tray route still arrives.) So a termination request that arrives WHILE
  //      windowStageClose is registered cannot be the X: it is the user asking
  //      the app to exit, and 0.0.29 was vetoing it. Do not do that again.
  //
  // WHAT IT ANSWERS HERE:
  //   * false (allow) while THIS process is running the explicit quit flow (tray
  //     退出并断开 / 设置 → 退出应用) — the app asked to leave, so the guard must
  //     not fight it. By that point the flow has already disconnected the VPN,
  //     released the continuous task and removed the tray icon
  //     (tray/NetBirdQuitFlow.ets).
  //   * false (allow) when closeToBackgroundRegistered is true: the X is already
  //     intercepted above, so this request is the system's auto-generated tray
  //     退出 (or a dock close) and it MUST go through — plain exit, with today's
  //     onDestroy cleanup (release the continuous task, close the writer; the
  //     status-bar icon dies with the process, and the :vpn extension process
  //     keeps the tunnel). A user who wants the VPN gone uses 退出并断开.
  //   * true (cancel) ONLY when the registration above FAILED (801 / no syscap):
  //     there the X arrives HERE, and vetoing is what keeps "close = background"
  //     on such a device. This is the fallback's whole reason to exist.
  //     NOTE the deliberate asymmetry with PLAIN SYSTEM EXIT (force-stop, OOM,
  //     device shutdown): those paths do not consult this callback at all, and
  //     their semantics stay pre-0.0.29 — onDestroy's existing cleanup runs and
  //     the VPN extension process (with a live tunnel) may survive the UI process.
  onPrepareToTerminate(): boolean {
    if (explicitQuitInProgress()) {
      hilog.info(DOMAIN, TAG, 'UI_TERMINATE_GUARD|result=allow|reason=explicit-quit');
      this.recordCloseGuard('onPrepareToTerminate', 'allow-explicit-quit');
      return false;
    }
    if (this.closeToBackgroundRegistered) {
      // 0.0.30: the X is covered by windowStageClose, so this is a REAL exit
      // request (the system tray 退出 row / a dock close). Let it terminate.
      hilog.info(DOMAIN, TAG, 'UI_TERMINATE_GUARD|result=allow|reason=system-exit');
      this.recordCloseGuard('onPrepareToTerminate', 'allow-system-exit');
      return false;
    }
    hilog.info(DOMAIN, TAG, 'UI_TERMINATE_GUARD|result=veto|note=close-is-background');
    this.recordCloseGuard('onPrepareToTerminate', 'veto');
    return true;
  }

  onForeground(): void {
    this.recordLifecycle('onForeground');
    // 0.0.21（挂起长窗口后的恢复复查）：本进程又跑起来了——真机上被平台挂起的那一段可以
    // 长达 7362 s（0.0.20：17:08 VPN 被相机清理后 1 h16 m 无人重连，直到用户点开应用）。
    // 这里只转发这一个事实；重读状态文档、重新判重连仍是页面自己的事（本文件不读任何状态
    // 文档）。页面还没注册监听者（冷启动的这一刻）时它就是个空操作。
    notifyKeepAliveResume(KEEP_ALIVE_RESUME_FOREGROUND);
  }

  onBackground(): void {
    this.recordLifecycle('onBackground');
  }

  // 2026-10-08 memory-pressure OBSERVATION (observation only — this callback
  // must NOT release UI components or any other memory; the SDK explicitly
  // warns that releasing UI components here can block the main thread). The
  // ONE field is the bounded AbilityConstant.MemoryLevel enum (0..4 on API 26),
  // never a free-form message. UIAbility extends Ability, which declares
  // onMemoryLevel, so this IS a real lifecycle override.
  onMemoryLevel(level: AbilityConstant.MemoryLevel): void {
    const fields: DiagFields = {};
    fields['stage'] = 'platform';
    fields['state'] = String(level);
    diagnosticsEvent('ui_memory_level', fields);
  }

  // Theme (2026-10-09): the system dark/light switch is delivered here, and the
  // ONLY field read is the bounded Configuration.colorMode enum — no free-form
  // configuration text is logged or stored. The page itself needs no handling:
  // every widget resolves its $r('app.color.nb_*') against the base/dark
  // resource set. The window's status/navigation bar colors are literal strings
  // in the native API, so they are re-derived on every configuration update.
  onConfigurationUpdate(newConfig: Configuration): void {
    const colorMode: ConfigurationConstant.ColorMode | undefined = newConfig.colorMode;
    hilog.info(DOMAIN, TAG, 'THEME_CONFIG_UPDATE|colorMode=%{public}s|note=system-bar-refresh',
      colorMode === undefined ? 'undefined' : String(colorMode));
    this.refreshSystemBarTheme();
  }

  // The ONLY stop() call site: the real ability-destroy callback. A page's
  // aboutToDisappear / onPageHide fires while this process is still alive and
  // is NOT a process exit, so it must never mark the run clean. The resulting
  // `marker state=clean` means THIS callback was observed to run and the run's
  // records were flushed — an observation of lifecycle cleanup, NOT proof that
  // the process exited (a force-stop or OOM kill leaves the marker open, and
  // the platform may end the process once onDestroy returns, before the
  // helper's own async marker write lands).
  onDestroy(): void {
    this.recordLifecycle('onDestroy');
    // 2026-10-10 后台保活（实验，方案 A）：先摘掉回调（我们自己的释放不能被读成平台的
    // 取消），再尽力释放任务。stopBackgroundRunning 是异步的，进程可能在它落地前就结束
    // ——这不是缺陷：进程退出本身就会结束这个应用的长时任务，这里只是把「有序退出」也
    // 写清楚，且失败只字不留地放行。
    unregisterKeepAlive();
    stopKeepAlive(this.context, KEEP_ALIVE_REVISION_SHUTDOWN, 'shutdown')
      .catch((_error: Error): void => {
        // Best-effort teardown; the platform releases the task with the process.
      });
    // The watcher goes FIRST, while the writer is still open: its stop() emits
    // the sys_watcher_stopped line (and any suppressed-count report) through the
    // sink, and the diagnostics stop() below flushes that queue. Stopping the
    // writer first would make those last lines unwritable.
    stopSystemEvents();
    stopDiagnostics();
  }

  private persistCredentialOverrides(want: Want): void {
    const params: Record<string, Object> | undefined = want.parameters;
    if (params !== undefined && params !== null) {
      const config: Object | undefined = params[CONFIG_PARAM];
      const setupKey: Object | undefined = params[SETUP_KEY_PARAM];
      const deviation: Object | undefined = params[PROTECT_DEVIATION_PARAM];
      if (typeof config === 'string' && config.length > 0) {
        this.writeFile(CONFIG_FILE, config);
      }
      if (typeof setupKey === 'string' && setupKey.length > 0) {
        this.writeFile(SETUP_KEY_FILE, `${setupKey}\n`);
      }
      if (typeof deviation === 'string' && deviation === 'route-exclusion') {
        this.writeFile(PROTECT_DEVIATION_FILE, `${deviation}\n`);
      } else {
        this.removeProtectDeviation();
      }
    } else {
      this.removeProtectDeviation();
    }
    // N2-H arm private channel: an EXPLICIT non-secret parameter writes the
    // ticket. Whether a launch WITHOUT it clears a leftover is decided further
    // below, AFTER the N2 request parameters are read (N2 T7a item 7: a launch
    // that carries one request but no arm ticket must NOT disarm the running
    // session). The credential writes above are untouched.
    const armOptions: Object | undefined =
      params === undefined || params === null ? undefined : params[N2H_ARM_OPTIONS_PARAM];
    // N2 T5a / T3 / P4 private channels: same discipline as the arm ticket
    // above. ONE launch carries at most ONE pending request (stimulus,
    // positive control or A-column trigger): an explicit non-secret parameter
    // writes it, a launch without it removes that channel's leftover, and a
    // launch carrying MORE THAN ONE writes none of them (the combination is
    // refused, not ordered) while removing every leftover. The payloads are
    // never logged.
    const stimulus: Object | undefined =
      params === undefined || params === null ? undefined : params[N2_STIMULUS_PARAM];
    const control: Object | undefined =
      params === undefined || params === null ? undefined : params[N2_CONTROL_PARAM];
    const aEvent: Object | undefined =
      params === undefined || params === null ? undefined : params[N2_A_EVENT_PARAM];
    const cdPlan: Object | undefined =
      params === undefined || params === null ? undefined : params[N2_CD_PLAN_PARAM];
    const ringDump: Object | undefined =
      params === undefined || params === null ? undefined : params[N2_RING_DUMP_PARAM];
    const ringAck: Object | undefined =
      params === undefined || params === null ? undefined : params[N2_RING_ACK_PARAM];
    const stimulusText: string = typeof stimulus === 'string' ? stimulus : '';
    const controlText: string = typeof control === 'string' ? control : '';
    const aEventText: string = typeof aEvent === 'string' ? aEvent : '';
    const cdPlanText: string = typeof cdPlan === 'string' ? cdPlan : '';
    const ringDumpText: string = typeof ringDump === 'string' ? ringDump : '';
    const ringAckText: string = typeof ringAck === 'string' ? ringAck : '';
    // N2 T7a (item 7) — arm ticket protection. The documented operator flow is
    // "aa start WITH the arm ticket, then one launch per request"; a launch
    // that carries an N2 request parameter but NO arm ticket must therefore
    // KEEP the existing ticket instead of deleting it (deleting it would leave
    // the already-running extension armed but a later extension restart
    // unarmed — the executor's "arm once, then send" shape would silently lose
    // its arm). A launch with NO N2 request parameter keeps the original
    // default-no-arm rule, so a stale ticket still cannot survive an unrelated
    // launch. An explicit ticket always wins and is written verbatim.
    if (typeof armOptions === 'string' && armOptions.length > 0) {
      this.writeArmTicketFile(armOptions);
    } else if (stimulusText.length > 0 || controlText.length > 0 || aEventText.length > 0 ||
      cdPlanText.length > 0 || ringDumpText.length > 0 || ringAckText.length > 0) {
      hilog.info(DOMAIN, TAG,
        'N2H_ARM_TICKET_KEPT|file=%{public}s|note=request-launch-without-arm-ticket|' +
        'diag=true|is_evidence=false',
        N2H_ARM_OPTIONS_FILE);
    } else {
      this.removeArmTicket('default-no-arm');
    }
    // N2 first batch C/D columns (2026-09-29): the one-pending-request-per-launch
    // rule now spans SIX channels. The pre-existing refusals (stimulus+control,
    // and every two-of-three combination involving the A-event channel) keep
    // their exact line shapes; a combination that involves one of the THREE NEW
    // channels (cd_plan, ring_dump, ring_ack) and is not already covered emits
    // the generic `pending=` shape naming exactly the file of each channel that
    // is actually present, in the fixed order stimulus, control, a_event,
    // cd_plan, ring_dump, ring_ack.
    const pendingCount: number = (stimulusText.length > 0 ? 1 : 0) +
      (controlText.length > 0 ? 1 : 0) + (aEventText.length > 0 ? 1 : 0) +
      (cdPlanText.length > 0 ? 1 : 0) + (ringDumpText.length > 0 ? 1 : 0) +
      (ringAckText.length > 0 ? 1 : 0);
    const removeEveryPendingRequest = (note: string): void => {
      this.removeStimulusFile(note);
      this.removeControlFile(note);
      this.removeAEventFile(note);
      this.removeCdPlanFile(note);
      this.removeRingDumpFile(note);
      this.removeRingAckFile(note);
    };
    if (stimulusText.length > 0 && controlText.length > 0) {
      // The pre-existing two-channel refusal keeps its exact line; a THIRD
      // pending request (the A-event trigger) extends the same refusal with
      // `|a_event=` — the combination rule now spans three channels.
      if (aEventText.length > 0) {
        hilog.warn(DOMAIN, TAG,
          'N2_CONTROL_COMBO_REJECTED|stimulus=%{public}s|control=%{public}s|' +
          'a_event=%{public}s|note=one-pending-request-per-launch|diag=true|is_evidence=false',
          N2_STIMULUS_FILE, N2_CONTROL_FILE, N2_A_EVENT_FILE);
      } else {
        hilog.warn(DOMAIN, TAG,
          'N2_CONTROL_COMBO_REJECTED|stimulus=%{public}s|control=%{public}s|' +
          'note=one-pending-request-per-launch|diag=true|is_evidence=false',
          N2_STIMULUS_FILE, N2_CONTROL_FILE);
      }
      this.removeStimulusFile('combo-rejected');
      this.removeControlFile('combo-rejected');
      this.removeAEventFile('combo-rejected');
      this.removeCdPlanFile('combo-rejected');
      this.removeRingDumpFile('combo-rejected');
      this.removeRingAckFile('combo-rejected');
    } else if (stimulusText.length > 0 && aEventText.length > 0) {
      hilog.warn(DOMAIN, TAG,
        'N2_CONTROL_COMBO_REJECTED|stimulus=%{public}s|control=%{public}s|' +
        'a_event=%{public}s|note=one-pending-request-per-launch|diag=true|is_evidence=false',
        N2_STIMULUS_FILE, N2_CONTROL_FILE, N2_A_EVENT_FILE);
      this.removeStimulusFile('combo-rejected');
      this.removeControlFile('combo-rejected');
      this.removeAEventFile('combo-rejected');
      this.removeCdPlanFile('combo-rejected');
      this.removeRingDumpFile('combo-rejected');
      this.removeRingAckFile('combo-rejected');
    } else if (controlText.length > 0 && aEventText.length > 0) {
      hilog.warn(DOMAIN, TAG,
        'N2_CONTROL_COMBO_REJECTED|stimulus=%{public}s|control=%{public}s|' +
        'a_event=%{public}s|note=one-pending-request-per-launch|diag=true|is_evidence=false',
        N2_STIMULUS_FILE, N2_CONTROL_FILE, N2_A_EVENT_FILE);
      this.removeStimulusFile('combo-rejected');
      this.removeControlFile('combo-rejected');
      this.removeAEventFile('combo-rejected');
      this.removeCdPlanFile('combo-rejected');
      this.removeRingDumpFile('combo-rejected');
      this.removeRingAckFile('combo-rejected');
    } else if (pendingCount > 1) {
      // A combination the pre-existing branches do not cover. It always
      // involves `n2_cd_plan`, `n2_ring_dump` or `n2_ring_ack` (those are the
      // only channels left once the three branches above did not match), so the
      // line names exactly the pending channels by file.
      let pending: string = '';
      if (stimulusText.length > 0) { pending += (pending.length > 0 ? ',' : '') + N2_STIMULUS_FILE; }
      if (controlText.length > 0) { pending += (pending.length > 0 ? ',' : '') + N2_CONTROL_FILE; }
      if (aEventText.length > 0) { pending += (pending.length > 0 ? ',' : '') + N2_A_EVENT_FILE; }
      if (cdPlanText.length > 0) { pending += (pending.length > 0 ? ',' : '') + N2_CD_PLAN_FILE; }
      if (ringDumpText.length > 0) { pending += (pending.length > 0 ? ',' : '') + N2_RING_DUMP_FILE; }
      if (ringAckText.length > 0) { pending += (pending.length > 0 ? ',' : '') + N2_RING_ACK_FILE; }
      hilog.warn(DOMAIN, TAG,
        'N2_CONTROL_COMBO_REJECTED|pending=%{public}s|' +
        'note=one-pending-request-per-launch|diag=true|is_evidence=false',
        pending);
      removeEveryPendingRequest('combo-rejected');
    } else if (stimulusText.length > 0) {
      this.writeStimulusFile(stimulusText);
      this.removeControlFile('default-no-control');
      this.removeAEventFile('default-no-a-event');
      this.removeCdPlanFile('default-no-cd-plan');
      this.removeRingDumpFile('default-no-ring-dump');
      this.removeRingAckFile('default-no-ring-ack');
    } else if (controlText.length > 0) {
      this.writeControlFile(controlText);
      this.removeStimulusFile('default-no-stimulus');
      this.removeAEventFile('default-no-a-event');
      this.removeCdPlanFile('default-no-cd-plan');
      this.removeRingDumpFile('default-no-ring-dump');
      this.removeRingAckFile('default-no-ring-ack');
    } else if (aEventText.length > 0) {
      this.writeAEventFile(aEventText);
      this.removeStimulusFile('default-no-stimulus');
      this.removeControlFile('default-no-control');
      this.removeCdPlanFile('default-no-cd-plan');
      this.removeRingDumpFile('default-no-ring-dump');
      this.removeRingAckFile('default-no-ring-ack');
    } else if (cdPlanText.length > 0) {
      this.writeCdPlanFile(cdPlanText);
      this.removeStimulusFile('default-no-stimulus');
      this.removeControlFile('default-no-control');
      this.removeAEventFile('default-no-a-event');
      this.removeRingDumpFile('default-no-ring-dump');
      this.removeRingAckFile('default-no-ring-ack');
    } else if (ringDumpText.length > 0) {
      this.writeRingDumpFile(ringDumpText);
      this.removeStimulusFile('default-no-stimulus');
      this.removeControlFile('default-no-control');
      this.removeAEventFile('default-no-a-event');
      this.removeCdPlanFile('default-no-cd-plan');
      this.removeRingAckFile('default-no-ring-ack');
    } else if (ringAckText.length > 0) {
      this.writeRingAckFile(ringAckText);
      this.removeStimulusFile('default-no-stimulus');
      this.removeControlFile('default-no-control');
      this.removeAEventFile('default-no-a-event');
      this.removeCdPlanFile('default-no-cd-plan');
      this.removeRingDumpFile('default-no-ring-dump');
    } else {
      this.removeStimulusFile('default-no-stimulus');
      this.removeControlFile('default-no-control');
      this.removeAEventFile('default-no-a-event');
      this.removeCdPlanFile('default-no-cd-plan');
      this.removeRingDumpFile('default-no-ring-dump');
      this.removeRingAckFile('default-no-ring-ack');
    }
  }

  private removeProtectDeviation(): void {
    const path: string = `${this.context.filesDir}/${PROTECT_DEVIATION_FILE}`;
    try {
      if (!fs.accessSync(path)) { return; }
      fs.unlinkSync(path);
      hilog.info(DOMAIN, TAG, 'PROTECT_DEVIATION_REMOVED|file=%{public}s', PROTECT_DEVIATION_FILE);
    } catch (error) {
      hilog.error(DOMAIN, TAG, 'PROTECT_DEVIATION_REMOVE_FAILED|file=%{public}s|code=%{public}s',
        PROTECT_DEVIATION_FILE, String((error as BusinessFailure).code ?? 'none'));
    }
  }

  private writeFile(name: string, text: string): void {
    const path: string = `${this.context.filesDir}/${name}`;
    let file: fs.File | undefined = undefined;
    try {
      file = fs.openSync(path, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      fs.writeSync(file.fd, text);
      hilog.warn(DOMAIN, TAG,
        'CRED_WANT_OVERRIDE|file=%{public}s|bytes=%{public}d|note=development-only-not-secure-storage',
        name, text.length);
    } catch (error) {
      const message: string = (error as Error).message ?? 'unknown';
      hilog.error(DOMAIN, TAG, 'CRED_WANT_OVERRIDE_FAILED|file=%{public}s|message=%{public}s',
        name, message.slice(0, 80));
    } finally {
      if (file !== undefined) {
        fs.closeSync(file);
      }
    }
  }

  // N2-H arm private channel: the SAME writeFile style as the credentials
  // (open trunc + write + close in this app's own filesDir) with its own
  // markers, and the honesty rule in BOTH failure shapes:
  //   - openSync throws → nothing was created, no ticket, log the error CODE.
  //   - writeSync throws or writes SHORT after a successful open — the open's
  //     TRUNC has already wiped any previous ticket — then the on-disk file is
  //     an empty/partial ticket that the extension would parse into the
  //     §5.1 defaults and ARM. So the failure path REMOVES exactly this one
  //     diagnostic ticket after closing the fd (removeArmTicket below logs the
  //     removal — or its own failure — independently with its error CODE; a
  //     removal failure is never reported as a removal).
  // Completeness: fs.writeSync returns the number of bytes ACTUALLY written
  // and, for a string buffer, those are UTF-8 bytes (SDK contract). The
  // expected count is the payload's real UTF-8 byte length via the platform
  // TextEncoder — never the UTF-16 code-unit `text.length`. The success log
  // reports the same UTF-8 byte count. Payload is never logged.
  private writeArmTicketFile(text: string): void {
    const path: string = `${this.context.filesDir}/${N2H_ARM_OPTIONS_FILE}`;
    let file: fs.File | undefined = undefined;
    let writeFailed: boolean = false;
    let failureCode: string = 'none';
    let expectedBytes: number = 0;
    try {
      file = fs.openSync(path, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      const written: number = fs.writeSync(file.fd, text);
      expectedBytes = new util.TextEncoder().encode(text).byteLength;
      if (written !== expectedBytes) {
        writeFailed = true;
        failureCode = `short-write:${written}of${expectedBytes}`;
      }
    } catch (error) {
      writeFailed = true;
      failureCode = String((error as BusinessFailure).code ?? 'none');
    } finally {
      if (file !== undefined) {
        try {
          fs.closeSync(file);
        } catch (closeError) {
          // The ticket's fate is already decided by the write outcome below;
          // a close failure is its own visible event, never swallowed.
          hilog.error(DOMAIN, TAG,
            'N2H_ARM_TICKET_CLOSE_FAILED|file=%{public}s|code=%{public}s',
            N2H_ARM_OPTIONS_FILE, String((closeError as BusinessFailure).code ?? 'none'));
        }
      }
    }
    if (!writeFailed) {
      hilog.warn(DOMAIN, TAG,
        'N2H_ARM_TICKET_WRITTEN|file=%{public}s|bytes=%{public}d|note=diagnostic-only-non-secret',
        N2H_ARM_OPTIONS_FILE, expectedBytes);
      return;
    }
    hilog.error(DOMAIN, TAG,
      'N2H_ARM_TICKET_WRITE_FAILED|file=%{public}s|code=%{public}s|note=no-ticket-no-arm',
      N2H_ARM_OPTIONS_FILE, failureCode);
    this.removeArmTicket('failed-write-cleanup');
  }

  // N2-H arm private channel: delete THE ONE diagnostic-generated ticket file
  // (basename above) — from the default no-arm launch path AND from a failed
  // ticket write (the open's TRUNC would otherwise leave an empty file the
  // extension could arm on). Credential / config / deviation files are NEVER
  // touched here. A missing ticket is the normal quiet case; a failed removal
  // is logged with its error CODE and the note — the surviving (or wiped)
  // file is reported as what it is, never as removed.
  private removeArmTicket(note: string): void {
    const path: string = `${this.context.filesDir}/${N2H_ARM_OPTIONS_FILE}`;
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      fs.unlinkSync(path);
      hilog.info(DOMAIN, TAG,
        'N2H_ARM_TICKET_REMOVED|file=%{public}s|note=%{public}s', N2H_ARM_OPTIONS_FILE, note);
    } catch (error) {
      const code: string = String((error as BusinessFailure).code ?? 'none');
      hilog.error(DOMAIN, TAG,
        'N2H_ARM_TICKET_REMOVE_FAILED|file=%{public}s|code=%{public}s|note=%{public}s',
        N2H_ARM_OPTIONS_FILE, code, note);
    }
  }

  // N2 T5a「B 断流刺激」private channel (2026-09-28). Same write honesty as
  // writeArmTicketFile above: the open's TRUNC already wiped any previous
  // request, so a failed/short WRITE must not leave an empty or partial file
  // that the extension could parse into a stimulus — the failure path removes
  // exactly this one file (logged independently, with its own error code; a
  // removal failure is never reported as a removal). The payload is never
  // logged, and `bytes=` is the real UTF-8 byte count (never the UTF-16
  // code-unit `text.length`).
  private writeStimulusFile(text: string): void {
    const path: string = `${this.context.filesDir}/${N2_STIMULUS_FILE}`;
    let file: fs.File | undefined = undefined;
    let writeFailed: boolean = false;
    let failureCode: string = 'none';
    let expectedBytes: number = 0;
    try {
      file = fs.openSync(path, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      const written: number = fs.writeSync(file.fd, text);
      expectedBytes = new util.TextEncoder().encode(text).byteLength;
      if (written !== expectedBytes) {
        writeFailed = true;
        failureCode = `short-write:${written}of${expectedBytes}`;
      }
    } catch (error) {
      writeFailed = true;
      failureCode = String((error as BusinessFailure).code ?? 'none');
    } finally {
      if (file !== undefined) {
        try {
          fs.closeSync(file);
        } catch (closeError) {
          hilog.error(DOMAIN, TAG,
            'N2_STIMULUS_FILE_CLOSE_FAILED|file=%{public}s|code=%{public}s',
            N2_STIMULUS_FILE, String((closeError as BusinessFailure).code ?? 'none'));
        }
      }
    }
    if (!writeFailed) {
      hilog.warn(DOMAIN, TAG,
        'N2_STIMULUS_FILE_WRITTEN|file=%{public}s|bytes=%{public}d|note=diagnostic-only-non-secret',
        N2_STIMULUS_FILE, expectedBytes);
      return;
    }
    hilog.error(DOMAIN, TAG,
      'N2_STIMULUS_FILE_WRITE_FAILED|file=%{public}s|code=%{public}s|note=no-stimulus',
      N2_STIMULUS_FILE, failureCode);
    this.removeStimulusFile('failed-write-cleanup');
  }

  // Delete THE ONE stimulus request file (basename above) — from the default
  // no-stimulus launch path, from the combination refusal, and from a failed
  // write. Credential / config / deviation / arm-ticket / control-request files
  // are NEVER touched here.
  private removeStimulusFile(note: string): void {
    const path: string = `${this.context.filesDir}/${N2_STIMULUS_FILE}`;
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      fs.unlinkSync(path);
      hilog.info(DOMAIN, TAG,
        'N2_STIMULUS_FILE_REMOVED|file=%{public}s|note=%{public}s', N2_STIMULUS_FILE, note);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'N2_STIMULUS_FILE_REMOVE_FAILED|file=%{public}s|code=%{public}s|note=%{public}s',
        N2_STIMULUS_FILE, String((error as BusinessFailure).code ?? 'none'), note);
    }
  }

  // N2 T3 §3 positive-control private channel (2026-09-28). The SAME write
  // honesty as writeStimulusFile above: the open's TRUNC already wiped any
  // previous request, so a failed/short WRITE must not leave an empty or
  // partial file the extension could parse into a control — the failure path
  // removes exactly this one file (logged independently, with its own error
  // code; a removal failure is never reported as a removal). The payload is
  // never logged, and `bytes=` is the real UTF-8 byte count (never the UTF-16
  // code-unit `text.length`).
  private writeControlFile(text: string): void {
    const path: string = `${this.context.filesDir}/${N2_CONTROL_FILE}`;
    let file: fs.File | undefined = undefined;
    let writeFailed: boolean = false;
    let failureCode: string = 'none';
    let expectedBytes: number = 0;
    try {
      file = fs.openSync(path, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      const written: number = fs.writeSync(file.fd, text);
      expectedBytes = new util.TextEncoder().encode(text).byteLength;
      if (written !== expectedBytes) {
        writeFailed = true;
        failureCode = `short-write:${written}of${expectedBytes}`;
      }
    } catch (error) {
      writeFailed = true;
      failureCode = String((error as BusinessFailure).code ?? 'none');
    } finally {
      if (file !== undefined) {
        try {
          fs.closeSync(file);
        } catch (closeError) {
          hilog.error(DOMAIN, TAG,
            'N2_CONTROL_FILE_CLOSE_FAILED|file=%{public}s|code=%{public}s',
            N2_CONTROL_FILE, String((closeError as BusinessFailure).code ?? 'none'));
        }
      }
    }
    if (!writeFailed) {
      hilog.warn(DOMAIN, TAG,
        'N2_CONTROL_FILE_WRITTEN|file=%{public}s|bytes=%{public}d|note=diagnostic-only-non-secret',
        N2_CONTROL_FILE, expectedBytes);
      return;
    }
    hilog.error(DOMAIN, TAG,
      'N2_CONTROL_FILE_WRITE_FAILED|file=%{public}s|code=%{public}s|note=no-control',
      N2_CONTROL_FILE, failureCode);
    this.removeControlFile('failed-write-cleanup');
  }

  // Delete THE ONE positive-control request file (basename above) — from the
  // default no-control launch path, from the combination refusal, and from a
  // failed write. Credential / config / deviation / arm-ticket / stimulus /
  // A-event files are NEVER touched here.
  private removeControlFile(note: string): void {
    const path: string = `${this.context.filesDir}/${N2_CONTROL_FILE}`;
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      fs.unlinkSync(path);
      hilog.info(DOMAIN, TAG,
        'N2_CONTROL_FILE_REMOVED|file=%{public}s|note=%{public}s', N2_CONTROL_FILE, note);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'N2_CONTROL_FILE_REMOVE_FAILED|file=%{public}s|code=%{public}s|note=%{public}s',
        N2_CONTROL_FILE, String((error as BusinessFailure).code ?? 'none'), note);
    }
  }

  // N2 P4「A 列生产事件触发原语」private channel (2026-09-28). The SAME write
  // honesty as writeStimulusFile / writeControlFile above: the open's TRUNC
  // already wiped any previous request, so a failed/short WRITE must not leave
  // an empty or partial file the extension could parse into a trigger — the
  // failure path removes exactly this one file (logged independently, with its
  // own error code; a removal failure is never reported as a removal). The
  // payload is never logged, and `bytes=` is the real UTF-8 byte count.
  private writeAEventFile(text: string): void {
    const path: string = `${this.context.filesDir}/${N2_A_EVENT_FILE}`;
    let file: fs.File | undefined = undefined;
    let writeFailed: boolean = false;
    let failureCode: string = 'none';
    let expectedBytes: number = 0;
    try {
      file = fs.openSync(path, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      const written: number = fs.writeSync(file.fd, text);
      expectedBytes = new util.TextEncoder().encode(text).byteLength;
      if (written !== expectedBytes) {
        writeFailed = true;
        failureCode = `short-write:${written}of${expectedBytes}`;
      }
    } catch (error) {
      writeFailed = true;
      failureCode = String((error as BusinessFailure).code ?? 'none');
    } finally {
      if (file !== undefined) {
        try {
          fs.closeSync(file);
        } catch (closeError) {
          hilog.error(DOMAIN, TAG,
            'N2_A_EVENT_FILE_CLOSE_FAILED|file=%{public}s|code=%{public}s',
            N2_A_EVENT_FILE, String((closeError as BusinessFailure).code ?? 'none'));
        }
      }
    }
    if (!writeFailed) {
      hilog.warn(DOMAIN, TAG,
        'N2_A_EVENT_FILE_WRITTEN|file=%{public}s|bytes=%{public}d|note=diagnostic-only-non-secret',
        N2_A_EVENT_FILE, expectedBytes);
      return;
    }
    hilog.error(DOMAIN, TAG,
      'N2_A_EVENT_FILE_WRITE_FAILED|file=%{public}s|code=%{public}s|note=no-a-event',
      N2_A_EVENT_FILE, failureCode);
    this.removeAEventFile('failed-write-cleanup');
  }

  // Delete THE ONE A-event request file (basename above) — from the default
  // no-a-event launch path, from the combination refusal, and from a failed
  // write. Credential / config / deviation / arm-ticket / stimulus / control
  // files are NEVER touched here.
  private removeAEventFile(note: string): void {
    const path: string = `${this.context.filesDir}/${N2_A_EVENT_FILE}`;
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      fs.unlinkSync(path);
      hilog.info(DOMAIN, TAG,
        'N2_A_EVENT_FILE_REMOVED|file=%{public}s|note=%{public}s', N2_A_EVENT_FILE, note);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'N2_A_EVENT_FILE_REMOVE_FAILED|file=%{public}s|code=%{public}s|note=%{public}s',
        N2_A_EVENT_FILE, String((error as BusinessFailure).code ?? 'none'), note);
    }
  }

  // N2 first batch C/D columns (2026-09-29): the three NEW request channels
  // (`n2_cd_plan`, `n2_ring_dump`, `n2_ring_ack`). ONE parameterized helper
  // carries the SAME write
  // honesty and removal discipline as the three channels above — open TRUNC +
  // write + close, a failed or SHORT write removes exactly this one file (the
  // open's TRUNC would otherwise leave an empty payload the extension could
  // parse), and the payload is never logged: only the file name, the real
  // UTF-8 byte count and an error CODE. `marker` is the channel's own log
  // marker prefix (`N2_CD_PLAN_FILE` / `N2_RING_DUMP_FILE` / `N2_RING_ACK_FILE`).
  private writeN2RequestFile(marker: string, name: string, note: string, text: string): void {
    const path: string = `${this.context.filesDir}/${name}`;
    let file: fs.File | undefined = undefined;
    let writeFailed: boolean = false;
    let failureCode: string = 'none';
    let expectedBytes: number = 0;
    try {
      file = fs.openSync(path, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      const written: number = fs.writeSync(file.fd, text);
      expectedBytes = new util.TextEncoder().encode(text).byteLength;
      if (written !== expectedBytes) {
        writeFailed = true;
        failureCode = `short-write:${written}of${expectedBytes}`;
      }
    } catch (error) {
      writeFailed = true;
      failureCode = String((error as BusinessFailure).code ?? 'none');
    } finally {
      if (file !== undefined) {
        try {
          fs.closeSync(file);
        } catch (closeError) {
          hilog.error(DOMAIN, TAG, marker + '_CLOSE_FAILED|file=%{public}s|code=%{public}s',
            name, String((closeError as BusinessFailure).code ?? 'none'));
        }
      }
    }
    if (!writeFailed) {
      hilog.warn(DOMAIN, TAG,
        marker + '_WRITTEN|file=%{public}s|bytes=%{public}d|note=diagnostic-only-non-secret',
        name, expectedBytes);
      return;
    }
    hilog.error(DOMAIN, TAG, marker + '_WRITE_FAILED|file=%{public}s|code=%{public}s|note=' + note,
      name, failureCode);
    this.removeN2RequestFile(marker, name, 'failed-write-cleanup');
  }

  private removeN2RequestFile(marker: string, name: string, note: string): void {
    const path: string = `${this.context.filesDir}/${name}`;
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      fs.unlinkSync(path);
      hilog.info(DOMAIN, TAG,
        marker + '_REMOVED|file=%{public}s|note=%{public}s', name, note);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        marker + '_REMOVE_FAILED|file=%{public}s|code=%{public}s|note=%{public}s',
        name, String((error as BusinessFailure).code ?? 'none'), note);
    }
  }

  private writeCdPlanFile(text: string): void {
    this.writeN2RequestFile('N2_CD_PLAN_FILE', N2_CD_PLAN_FILE, 'no-cd-plan', text);
  }

  private removeCdPlanFile(note: string): void {
    this.removeN2RequestFile('N2_CD_PLAN_FILE', N2_CD_PLAN_FILE, note);
  }

  private writeRingDumpFile(text: string): void {
    this.writeN2RequestFile('N2_RING_DUMP_FILE', N2_RING_DUMP_FILE, 'no-ring-dump', text);
  }

  private removeRingDumpFile(note: string): void {
    this.removeN2RequestFile('N2_RING_DUMP_FILE', N2_RING_DUMP_FILE, note);
  }

  private writeRingAckFile(text: string): void {
    this.writeN2RequestFile('N2_RING_ACK_FILE', N2_RING_ACK_FILE, 'no-ring-ack', text);
  }

  private removeRingAckFile(note: string): void {
    this.removeN2RequestFile('N2_RING_ACK_FILE', N2_RING_ACK_FILE, note);
  }
}
