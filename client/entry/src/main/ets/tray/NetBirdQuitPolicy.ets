/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// 2026-10-13 统一退出（0.0.31）—— PURE LOGIC only.
//
// WHAT THIS FILE IS: the ONE decision that cannot ask the platform anything —
// what onPrepareToTerminate() must ANSWER to the system's own 退出, given two
// facts and a clock. No import, no platform call, no module state; the caller
// carries the returned state. The host-side selfcheck imports this file
// byte-identical (client/entry/selfcheck/run-close-to-background.mts), so it must
// stay plain TypeScript.
//
// WHY THE ANSWER IS NOT A SIMPLE "true" OR "false". onPrepareToTerminate() is
// synchronous (true = CANCEL this termination) but the unified quit is not: it
// disconnects the VPN, waits a bounded grace, releases the borrowed continuous
// task, removes the status-bar icon and only then calls terminateSelf()
// (tray/NetBirdQuitFlow.ets). So the first system exit request of a process is
// CANCELLED once and the flow is started right there; the flow's own
// terminateSelf() is the exit, and by then the flow has set its one-way flag, so
// every later request is answered "allow" (see NetBirdQuitFlow's
// explicitQuitRequested arm below).
//
// THE BOUNDED SAFEGUARD (why vetoedAtMs exists at all): an app that answered
// "cancel" forever would be an app the user cannot exit. Two independent exits
// from that state:
//   * explicitQuitRequested — our own flow is already running, so this request
//     IS our exit and must never be cancelled.
//   * the window — only the FIRST request of a process is cancelled; a second
//     one within SYSTEM_EXIT_VETO_WINDOW_MS is let through (a user who clicks
//     退出 twice, or a platform that retries, is never stuck), and a request
//     much later is treated as a FRESH one (the earlier flow evidently never
//     reached terminateSelf, e.g. the page was already gone).
export const SYSTEM_EXIT_VETO_WINDOW_MS: number = 5000;

// The "no veto recorded yet" state: vetoedAtMs is -1 until the first request.
export const SYSTEM_EXIT_VETO_NONE: number = -1;

// What the caller must do, and the state to carry into the next call.
export interface SystemExitVetoDecision {
  veto: boolean;
  vetoedAtMs: number;
}

// THE decision. Pure: same facts + same clock = same answer.
export function systemExitVetoDecision(vetoedAtMs: number, explicitQuitRequested: boolean,
  nowMs: number): SystemExitVetoDecision {
  if (explicitQuitRequested) {
    // Our own quit is running: this termination request is the one ITS
    // terminateSelf() is producing (or the platform echoing it). Let it pass.
    const allowed: SystemExitVetoDecision = { veto: false, vetoedAtMs: vetoedAtMs };
    return allowed;
  }
  if (vetoedAtMs === SYSTEM_EXIT_VETO_NONE) {
    // First system exit request of this process: cancel it ONCE so the caller
    // can start the unified flow synchronously.
    const first: SystemExitVetoDecision = { veto: true, vetoedAtMs: nowMs };
    return first;
  }
  if (nowMs - vetoedAtMs < SYSTEM_EXIT_VETO_WINDOW_MS) {
    // A second attempt right behind the first: the flow has already issued the
    // disconnect, and the user (or the platform) asked again — let it through.
    const again: SystemExitVetoDecision = { veto: false, vetoedAtMs: vetoedAtMs };
    return again;
  }
  // Long after the first: a fresh request, treated exactly like the first one.
  const fresh: SystemExitVetoDecision = { veto: true, vetoedAtMs: nowMs };
  return fresh;
}
