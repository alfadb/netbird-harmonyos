/* SPDX-License-Identifier: AGPL-3.0-or-later
 * Copyright (C) 2026 NetBird HarmonyOS contributors
 */
// 2026-10-13 显式退出（0.0.29）→ 统一退出（0.0.31）—— the ONE exit flow, shared by
// every exit path this app can act on. All of them run the SAME teardown, in the
// SAME order:
//
//     arm the flag → requestStop (disconnect) → grace → stopKeepAlive
//     → remove tray → terminateSelf()
//
// THE THREE ENTRY POINTS (one flow, one order, one reason token each):
//   * the tray menu's 退出 row → the page → requestQuit(QUIT_REASON_TRAY);
//   * 设置 → 应用 → 退出应用 → requestQuit(QUIT_REASON_SETTINGS);
//   * the SYSTEM-GENERATED tray 退出 (and any other dock/system termination
//     request) → EntryAbility.onPrepareToTerminate(), which must answer
//     SYNCHRONOUSLY: it cancels the request ONCE (shouldVetoSystemExit below,
//     the bounded safeguard) and calls requestQuit(QUIT_REASON_SYSTEM) — the
//     flow's own terminateSelf() is then the app's exit.
//
// WHY THERE IS NO "exit but keep the tunnel" ANY MORE (product decision,
// 0.0.31): HarmonyOS couples the VPN's lifetime to the launching app — the
// platform stops the VPN when the app process exits — so keeping the tunnel
// after exit was never reliable, and upstream desktop's documented default is
// disconnect-on-quit anyway (preferences/store.go: KeepConnectedOnQuit defaults
// to false; client/ui/tray.go handleQuit()):
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
// 0.0.29/0.0.30 had TWO exits: this flow, and a plain system exit that left the
// :vpn extension (and a live tunnel) behind. 0.0.31 has one. The tray menu
// reflects it: upstream's tray.go has exactly one Quit row and so does ours
// (退出).
//
// THE ONE PATH THE APP CANNOT ACT ON, stated honestly: a force-stop, an OOM
// kill, a device shutdown or any platform process kill never reaches a callback
// of ours (onPrepareToTerminate is not consulted on those paths). onDestroy runs
// its existing best-effort cleanup and the platform takes the VPN down with the
// process — which is exactly WHY the platform's coupling is the design
// constraint here rather than something to work around.
//
// WHY THE DISCONNECT COMES FROM THE PAGE: the ONE
// vpnExtension.stopVpnExtensionAbility call site stays in pages/Index.ets
// (requestStop), which registers that closure once through setQuitDisconnect().
// This module never talks to vpnExtension, and keeping the closure a plain
// callback is also what keeps the connector (and the native core module) out of
// EntryAbility's dependency graph.
//
// THE FLAG: explicitQuitRequested is set FIRST — before the disconnect, before
// any await — because onPrepareToTerminate() reads it. Once OUR flow is running,
// every termination request is answered "allow": that request is the one our own
// terminateSelf() is producing, and vetoing it would leave the app half-exited.
// The flag is process-scoped and one-way: this process is on its way out.

import common from '@ohos.app.ability.common';
import hilog from '@ohos.hilog';
import { event } from '../vpnextensionability/NetBirdDiagnostics';
import type { DiagFields } from '../vpnextensionability/NetBirdDiagnostics';
import { KEEP_ALIVE_REVISION_SHUTDOWN,
  stopKeepAlive } from '../vpnextensionability/NetBirdKeepAliveService';
import { SYSTEM_EXIT_VETO_NONE, systemExitVetoDecision } from './NetBirdQuitPolicy';
import type { SystemExitVetoDecision } from './NetBirdQuitPolicy';
import { removeTray } from './NetBirdTrayService';

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';
const BUNDLE_NAME: string = 'cn.alfadb.netbird';

interface BusinessFailure extends Error {
  code?: number;
}

// The entry points' reason tokens, a closed set of OUR OWN — never platform or
// user text.
export const QUIT_REASON_TRAY: string = 'tray-quit';
export const QUIT_REASON_SETTINGS: string = 'settings-quit';
export const QUIT_REASON_SYSTEM: string = 'system-quit';

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
let systemExitVetoAtMs: number = SYSTEM_EXIT_VETO_NONE;
let quitDisconnect: QuitDisconnect | undefined = undefined;

// Read by EntryAbility.onPrepareToTerminate(): true only while OUR quit is
// running, so the platform's own termination requests keep their default
// (allowed) meaning.
export function explicitQuitInProgress(): boolean {
  return explicitQuitRequested;
}

// The page registers its requestStop() here, once per page lifetime (and clears
// it in aboutToDisappear, the same discipline the tray's action listener has).
// While the page is gone there is nothing to disconnect FROM this side — the one
// extension-stop call site lives there — so the flow says so in one log line
// instead of inventing a second call site.
export function setQuitDisconnect(disconnect: QuitDisconnect | undefined): void {
  quitDisconnect = disconnect;
}

// THE synchronous answer onPrepareToTerminate() needs: true = cancel THIS
// termination so the flow can finish the teardown, false = let the platform
// terminate. The rule (first request of the process is cancelled once, a second
// one inside the window is let through, our own quit is always let through)
// lives in NetBirdQuitPolicy, byte-identical-tested on the host; this function
// only carries the state it returns.
export function shouldVetoSystemExit(nowMs: number): boolean {
  const decision: SystemExitVetoDecision = systemExitVetoDecision(systemExitVetoAtMs,
    explicitQuitRequested, nowMs);
  systemExitVetoAtMs = decision.vetoedAtMs;
  return decision.veto;
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

// The ONE way into the flow. Synchronous on purpose: the first thing
// performExplicitQuit does is arm the flag and issue the disconnect, both before
// its first await — which is what lets onPrepareToTerminate() cancel a system
// exit and return in the same call, with the flag already set.
export function requestQuit(context: common.UIAbilityContext, reason: string): void {
  const disconnect: QuitDisconnect | undefined = quitDisconnect;
  performExplicitQuit(context, reason, (): void => {
    if (disconnect === undefined) {
      hilog.warn(DOMAIN, TAG, 'UI_QUIT_DISCONNECT_UNAVAILABLE|reason=%{public}s', reason);
      return;
    }
    disconnect();
  }).catch((error: Error): void => {
    // performExplicitQuit records every step it could not complete; this only
    // keeps a rejected promise from escaping.
    hilog.error(DOMAIN, TAG, 'UI_QUIT_FAILED|reason=%{public}s|code=%{public}s',
      reason, String((error as BusinessFailure).code ?? 'none'));
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
  //    no component is left. On the system-exit path this is the exit the veto
  //    in step 0 bought time for — and the flag armed above is what makes the
  //    platform's own termination request pass instead of being cancelled.
  try {
    await context.terminateSelf();
  } catch (error) {
    hilog.error(DOMAIN, TAG, 'UI_QUIT_TERMINATE_FAILED|reason=%{public}s|code=%{public}s',
      reason, String((error as BusinessFailure).code ?? 'none'));
    record('error', reason);
  }
}
