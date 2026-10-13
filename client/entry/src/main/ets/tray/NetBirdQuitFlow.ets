/* SPDX-License-Identifier: AGPL-3.0-or-later
 * Copyright (C) 2026 NetBird HarmonyOS contributors
 */
// 2026-10-13 显式退出（0.0.29）—— the ONE explicit-quit flow, shared by the PC
// tray's 退出并断开 row and by the Settings entry (设置 → 应用 → 退出应用).
//
// WHY IT EXISTS AT ALL: once the window's X is intercepted (see EntryAbility's
// windowStageClose handler) the app no longer has a plain "close the window =
// quit" path, and the system's own tray 退出 item cannot be customised. Upstream
// models the same situation with a dedicated Quit action
// (client/ui/tray.go handleQuit()):
//
//     func (t *Tray) handleQuit() {
//         services.BeginShutdown()
//         ...
//         if t.svc.Preferences == nil || !t.svc.Preferences.Get().KeepConnectedOnQuit {
//             ctx, cancel := context.WithTimeout(context.Background(), quitDownTimeout)
//             if err := t.svc.Connection.Down(ctx); err != nil { log.Errorf(...) }
//         }
//         t.app.Quit()
//     }
//
// i.e. QUIT DISCONNECTS THE VPN — that is the documented default
// (preferences/store.go KeepConnectedOnQuit defaults to false), and this app has
// no such preference, so the default is the only behaviour. The order is
// upstream's too: stop the business first (with a bounded wait), then leave.
//
// THE TWO EXITS THIS APP NOW HAS, and how they differ:
//   * EXPLICIT QUIT (this file): disconnect the VPN, release the borrowed
//     continuous task, remove the status-bar icon, then terminateSelf() so the
//     ordinary onDestroy cleanup runs. The VPN really goes down.
//   * PLAIN SYSTEM EXIT (the auto-generated tray 退出 item, a Dock close, a
//     force-stop, an OOM kill): NOTHING extra happens. onDestroy's existing
//     cleanup runs (unregister the task callbacks, best-effort stopBackgroundRunning,
//     stop the diagnostics); the VPN extension process and its tunnel are NOT
//     touched, so a still-running tunnel may survive the UI process. That is the
//     pre-0.0.29 semantics and it is deliberately unchanged.
//
// THE BYPASS FLAG: EntryAbility.onPrepareToTerminate() vetoes termination by
// default (the fallback for a device where windowStageClose cannot be
// registered). That veto must never block OUR OWN exit, so the flag below is set
// FIRST — before the disconnect, before any await — and the ability reads it.
// The flag is process-scoped and one-way: this process is on its way out.

import common from '@ohos.app.ability.common';
import hilog from '@ohos.hilog';
import { event } from '../vpnextensionability/NetBirdDiagnostics';
import type { DiagFields } from '../vpnextensionability/NetBirdDiagnostics';
import { KEEP_ALIVE_REVISION_SHUTDOWN,
  stopKeepAlive } from '../vpnextensionability/NetBirdKeepAliveService';
import { removeTray } from './NetBirdTrayService';

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';
const BUNDLE_NAME: string = 'cn.alfadb.netbird';

interface BusinessFailure extends Error {
  code?: number;
}

// The two entry points. The reason is a closed token of OUR OWN, never platform
// or user text.
export const QUIT_REASON_TRAY: string = 'tray-quit';
export const QUIT_REASON_SETTINGS: string = 'settings-quit';

// How long the disconnect gets before the process is torn down. Upstream allows
// 5 s for Connection.Down (quitDownTimeout); the platform call here is a local
// stopVpnExtensionAbility that resolves in tens of milliseconds, so this is the
// small bounded wait that lets it land — not a retry budget.
export const QUIT_DISCONNECT_GRACE_MS: number = 1500;

// What the caller supplies to end the business: the page's own requestStop(),
// so the ONE extension-stop call site stays in the page (this module never talks
// to vpnExtension).
export type QuitDisconnect = () => void;

let explicitQuitRequested: boolean = false;
let quitInFlight: boolean = false;

// Read by EntryAbility.onPrepareToTerminate(): true only while OUR explicit quit
// is running, so the platform's own termination requests keep their default
// (vetoed) meaning.
export function explicitQuitInProgress(): boolean {
  return explicitQuitRequested;
}

function record(result: string, reason: string): void {
  const fields: DiagFields = {};
  fields['result'] = result;
  fields['reason'] = reason;
  event('ui_quit', fields);
}

// The same 4-line helper NetBirdConnector exports, kept LOCAL on purpose: the
// quit flow is imported by EntryAbility (for the flag), and pulling the
// connector — and with it the native core module — into the ability's
// dependency graph for one sleep() would be a bad trade.
function sleep(ms: number): Promise<void> {
  return new Promise<void>((resolve: () => void) => {
    setTimeout(resolve, ms);
  });
}

// The flow. Every step is fail-soft and ordered: nothing here may leave the app
// alive-but-half-quit, and nothing may block the exit.
export async function performExplicitQuit(context: common.UIAbilityContext, reason: string,
  disconnect: QuitDisconnect): Promise<void> {
  if (quitInFlight) {
    return;
  }
  quitInFlight = true;
  explicitQuitRequested = true;
  hilog.info(DOMAIN, TAG, 'UI_QUIT_REQUEST|bundle=%{public}s|reason=%{public}s',
    BUNDLE_NAME, reason);
  // 1. The business: upstream's Connection.Down() before app.Quit(). The page's
  //    requestStop() is a no-op when nothing was requested (already
  //    disconnected), which is exactly right.
  try {
    disconnect();
  } catch (error) {
    hilog.warn(DOMAIN, TAG, 'UI_QUIT_DISCONNECT_FAILED|reason=%{public}s|code=%{public}s',
      reason, String((error as BusinessFailure).code ?? 'none'));
  }
  // 2. Let that stop land before this process goes away.
  try {
    await sleep(QUIT_DISCONNECT_GRACE_MS);
  } catch (error) {
    // A timer failure must not stop the exit.
  }
  // 3. Release the borrowed continuous task (KEEP_ALIVE_REVISION_SHUTDOWN = no
  //    session left, so an in-flight creation is withdrawn rather than adopted).
  try {
    await stopKeepAlive(context, KEEP_ALIVE_REVISION_SHUTDOWN, reason);
  } catch (error) {
    hilog.warn(DOMAIN, TAG, 'UI_QUIT_KEEPALIVE_FAILED|reason=%{public}s|code=%{public}s',
      reason, String((error as BusinessFailure).code ?? 'none'));
  }
  // 4. Remove the status-bar icon. Best effort: the platform refuses the removal
  //    while no window is in the foreground (error 1010710004) and the icon dies
  //    with this process anyway, so a refusal is logged and never blocks the
  //    exit. removeTray() never throws.
  await removeTray(reason);
  record('ok', reason);
  // 5. The ordinary ability teardown: terminateSelf() destroys this UIAbility,
  //    onDestroy runs its usual cleanup, and the platform ends the process once
  //    no component is left. The onPrepareToTerminate veto is already bypassed.
  try {
    await context.terminateSelf();
  } catch (error) {
    hilog.error(DOMAIN, TAG, 'UI_QUIT_TERMINATE_FAILED|reason=%{public}s|code=%{public}s',
      reason, String((error as BusinessFailure).code ?? 'none'));
    record('error', reason);
  }
}
