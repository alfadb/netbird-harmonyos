/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Small PURE view policy for the 路由选择页 (设置 → 路由选择) of the INSTANT-apply
// increment: a switch action saves the scoped preference and rings a doorbell
// for the live VPE, and the connection itself is never stopped, started or
// re-logged-in. The page owns every side effect — the atomic preference write,
// the atomic apply-request write and the offline result read — and this module
// owns only what the page may SAY and the id/label tokens it renders, so the
// shipped wording and the honesty rule below can be executed host-side
// (client/entry/selfcheck/run-route-apply.mts) instead of being asserted from a
// hand-held mirror.
//
// The rules that carry the whole honesty argument:
//
// 1. 已生效 is claimed ONLY from a result document whose opaque id equals the
//    NEWEST request id this page wrote. An older generation's result, an absent
//    result and the catalog's own `enabled` verdict are never turned into
//    已生效: the catalog is the management's view at capture time, not proof
//    that the OS side changed.
// 2. The predicate for "the OS side really is at the requested state" is NOT
//    duplicated here: the page passes the answer of `routeApplyResultIsApplied`
//    (NetBirdRouteRequest) into the view, which stays the ONE reading of the
//    `stored-no-map` rule.
// 3. No fresh published document (offline) never claims 已生效: the preference
//    IS saved and the request IS written, and the honest wording is
//    「将在下次连接时生效」. Nothing here stops, starts or re-logs-in anything.
//
// The stale/matching reading of the stop-ack document moved out with the stop
// leg itself: this increment has no stop, no ACK and no restart at all, so
// nothing of that retired helper is imported here.
//
// No DNS, no IPv6 and no exit-node exclusivity anywhere: an exit route is a
// normal IPv4 NetID here, and the global all-off switch is the ONLY "all" verb.
//
// The result STATE tokens are taken from the IPC contract itself, so a token can
// never drift from the document the page actually reads. This module still owns
// no file IO, no clock and no UI resource.

import { ROUTE_APPLY_STATE_APPLIED,
  ROUTE_APPLY_STATE_FAILED } from './NetBirdRouteRequest';

// ---------------------------------------------------------------------------
// the one status the page may render
// ---------------------------------------------------------------------------

// idle           — nothing has been asked in this page session and no answer of
//                  ours exists: say nothing.
// saving         — the preference write (or the request write behind it) is in
//                  flight.
// pending        — the request is written and the session is live, but no
//                  matching answer has arrived yet.
// applied        — a matching result says the OS side really is at the state.
// next-connect   — saved, but this connection will not pick it up (offline, or a
//                  stored-no-map answer): the NEXT connection applies it.
// failed         — a matching result reports a failure: say so, with its own
//                  error text when there is one.
// save-failed    — the preference write itself failed: nothing was persisted,
//                  nothing was notified and the connection is untouched.
// notify-failed  — the preference IS saved but the doorbell could not be written:
//                  the live session was not told, the next connection applies it.
export const ROUTE_STATUS_IDLE: string = 'idle';
export const ROUTE_STATUS_SAVING: string = 'saving';
export const ROUTE_STATUS_PENDING: string = 'pending';
export const ROUTE_STATUS_APPLIED: string = 'applied';
export const ROUTE_STATUS_NEXT_CONNECT: string = 'next-connect';
export const ROUTE_STATUS_FAILED: string = 'failed';
export const ROUTE_STATUS_SAVE_FAILED: string = 'save-failed';
export const ROUTE_STATUS_NOTIFY_FAILED: string = 'notify-failed';

// Tone tokens the page maps onto palette colours (never a colour itself, so
// this module stays free of UI resources).
export const ROUTE_TONE_OK: string = 'ok';
export const ROUTE_TONE_WARN: string = 'warn';
export const ROUTE_TONE_ERROR: string = 'error';
export const ROUTE_TONE_NONE: string = '';

// What the page knows at render time. `result_applied` is the answer of
// `routeApplyResultIsApplied` for the result document on disk (false when there
// is none or when it belongs to another request); `result_state` is that
// document's own state token, so a `stored-no-map` answer — which parses as
// state `applied` but is NOT applied — can be worded as 下次连接 without a
// second opinion about what "applied" means.
export interface RouteApplyView {
  latest_request_id: string;
  result_id: string;
  result_state: string;
  result_applied: boolean;
  has_live_document: boolean;
  saving: boolean;
  save_failed: boolean;
  notify_failed: boolean;
}

// The ONE status decision. Order matters: a failed preference write outranks
// everything (nothing on disk changed), then the in-flight write, then the
// doorbell that could not be written, then a matching failure answer (honest
// even while offline — the extension did report it), then the offline wording,
// then a matching applied answer, and only then pending.
export function routeApplyStatus(view: RouteApplyView): string {
  if (view.save_failed) {
    return ROUTE_STATUS_SAVE_FAILED;
  }
  if (view.saving) {
    return ROUTE_STATUS_SAVING;
  }
  if (view.notify_failed) {
    return ROUTE_STATUS_NOTIFY_FAILED;
  }
  if (view.latest_request_id.length === 0) {
    return ROUTE_STATUS_IDLE;
  }
  const matching: boolean = view.result_id.length > 0 &&
    view.result_id === view.latest_request_id;
  if (matching && view.result_state === ROUTE_APPLY_STATE_FAILED) {
    return ROUTE_STATUS_FAILED;
  }
  if (!view.has_live_document) {
    // Offline: the preference and the request are on disk, so the next
    // connection applies them — but this page must not claim 已生效.
    return ROUTE_STATUS_NEXT_CONNECT;
  }
  if (matching && view.result_applied) {
    return ROUTE_STATUS_APPLIED;
  }
  if (matching && view.result_state === ROUTE_APPLY_STATE_APPLIED) {
    // state says applied, but this answer stored the preference without handing
    // a map to the OS (the request module's stored-no-map mode): next connect.
    return ROUTE_STATUS_NEXT_CONNECT;
  }
  return ROUTE_STATUS_PENDING;
}

export function routeApplyStatusText(status: string, error: string): string {
  if (status === ROUTE_STATUS_SAVING) {
    return '正在保存…';
  }
  if (status === ROUTE_STATUS_PENDING) {
    return '已保存，正在应用…';
  }
  if (status === ROUTE_STATUS_APPLIED) {
    return '已生效。';
  }
  if (status === ROUTE_STATUS_NEXT_CONNECT) {
    return '已保存，将在下次连接时生效。';
  }
  if (status === ROUTE_STATUS_FAILED) {
    return error.length > 0 ? `已保存，但应用失败：${error}` : '已保存，但应用失败。';
  }
  if (status === ROUTE_STATUS_SAVE_FAILED) {
    return '保存失败，路由选择未生效；连接状态未改变。';
  }
  if (status === ROUTE_STATUS_NOTIFY_FAILED) {
    return '已保存；未能通知当前连接，将在下次连接时生效。';
  }
  return '';
}

export function routeApplyStatusTone(status: string): string {
  if (status === ROUTE_STATUS_APPLIED) {
    return ROUTE_TONE_OK;
  }
  if (status === ROUTE_STATUS_FAILED || status === ROUTE_STATUS_SAVE_FAILED) {
    return ROUTE_TONE_ERROR;
  }
  if (status === ROUTE_STATUS_SAVING || status === ROUTE_STATUS_PENDING ||
    status === ROUTE_STATUS_NEXT_CONNECT || status === ROUTE_STATUS_NOTIFY_FAILED) {
    return ROUTE_TONE_WARN;
  }
  return ROUTE_TONE_NONE;
}

// A fresh, unique id for EVERY completed desired save: the counter is what makes
// two saves inside the same millisecond distinct, and the stamp is what makes
// two runs of the page distinct. The id is opaque — nothing may order two ids.
export function routeApplyRequestId(seq: number, nowMs: number): string {
  return `ui-route-${nowMs}-${seq}`;
}

// ---------------------------------------------------------------------------
// one NetID row's display text (pure, so the page cannot grow a second shape)
// ---------------------------------------------------------------------------

// The HA group's NetID; an empty NetID is a legitimate group key (management
// sent none), never a fabricated prefix fallback.
export function routeNetName(netId: string): string {
  return netId.length > 0 ? netId : '（管理端未提供网络标识）';
}

// Every advertised prefix of the ONE logical network, in catalog order — this
// is the subnet column. A NetID with no advertised prefix is said so plainly.
export function routePrefixesText(prefixes: string[]): string {
  return prefixes.length > 0 ? prefixes.join('、') : '未提供网段';
}

// The HA candidate count behind the single NetID row. The row is NEVER built
// from a route id, a peer key or a metric — those are candidates, not identity.
export function routeCandidateText(candidateCount: number): string {
  return `节点数 ${candidateCount}`;
}
