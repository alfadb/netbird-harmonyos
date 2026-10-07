/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// The live-route IPC pair of the 路由选择页: the UI's APPLY REQUEST and the
// VPE's APPLY RESULT. Pure types + strict parse/format, no IO and no clock, so
// the shipped contract can be executed host-side
// (client/entry/selfcheck/run-route-request.mts). The byte-level file handling
// lives in NetBirdRouteRequestStore.
//
// ## Request document (filesDir/netbird-route-apply-request.json)
//
//   {"v":1,"id":"<ui id>","management_url":"<url>","created_at_ms":<n>}
//
// It is a NOTIFICATION, not a selection copy: the UI saves the existing
// preference document first (netbird-route-selection.json, scoped by management
// URL) and then atomically writes this request naming only WHICH scope changed.
// The reader (the VPE) loads the selection itself with the CURRENT management
// URL, so no selection is duplicated here and no session is bound: a request
// carries no route list, no session id and no credential.
//
// `id` is OPAQUE. The UI generates it and the VPE echoes it into the result;
// readers compare ids, and nothing may infer an order from two ids (a
// lexicographic "newer" guess would silently drop a newer request).
//
// ## Result document (filesDir/netbird-route-apply-result.json)
//
//   {"v":1,"id":"<echoed id>","state":"applying"|"applied"|"failed",
//    "mode":"no-os-change"|"recreated"|"stored-no-map"|"","error":"",
//    "routes":["<net id>",...],"at_ms":<n>}
//
// `state:"applied"` means the OS side really is at the requested state —
// either it was recreated (`mode:"recreated"`) or the requested state was
// already in force and nothing had to change (`mode:"no-os-change"`).
// `mode:"stored-no-map"` is deliberately NOT applied: the preference document
// was stored but no map was handed to the OS, so a reader must not report
// success — see routeApplyResultIsApplied, which is the ONE predicate for that
// rule so the UI cannot grow a second opinion.
//
// Validation is minimal and follows NetBirdRoutePreferences: field types and
// bounds only, no cross-field rules (the writer owns the combination it
// writes), and every shape error is the existing stable RoutePreferencesError
// vocabulary (REASON_MALFORMED / REASON_VERSION) — never a silent default.
// Nothing in these documents is a secret, so this module never logs a payload.

import {
  MAX_NET_ID_CHARS, MAX_REQUEST_ID_CHARS, MAX_URL_CHARS,
  REASON_MALFORMED, REASON_VERSION, RoutePreferencesError
} from './NetBirdRoutePreferences';

export const ROUTE_APPLY_REQUEST_VERSION: number = 1;
export const ROUTE_APPLY_RESULT_VERSION: number = 1;

// The three states a result may report. `applying` is the VPE's own in-flight
// marker; only `applied` claims the OS side is settled.
export const ROUTE_APPLY_STATE_APPLYING: string = 'applying';
export const ROUTE_APPLY_STATE_APPLIED: string = 'applied';
export const ROUTE_APPLY_STATE_FAILED: string = 'failed';

// How an `applied`/`failed` answer was reached. '' is the honest "not
// described" mode (e.g. the in-flight `applying` answer) and is always legal.
export const ROUTE_APPLY_MODE_NONE: string = '';
export const ROUTE_APPLY_MODE_NO_OS_CHANGE: string = 'no-os-change';
export const ROUTE_APPLY_MODE_RECREATED: string = 'recreated';
export const ROUTE_APPLY_MODE_STORED_NO_MAP: string = 'stored-no-map';

// The UI's apply request: an opaque id, the scope that changed, and when the
// UI wrote it. No route list and no session identity — see the header.
export interface RouteApplyRequest {
  id: string;
  management_url: string;
  created_at_ms: number;
}

// The VPE's apply result. `routes` holds the NetIDs the answer is about (a
// prefix is legal too — both are bounded by MAX_NET_ID_CHARS); it is
// canonicalized to sorted/unique on write, because the set of routes is what
// matters and a stable byte order keeps two writers' payloads comparable.
export interface RouteApplyResult {
  id: string;
  state: string;
  mode: string;
  error: string;
  routes: string[];
  at_ms: number;
}

// ---------------------------------------------------------------------------
// raw document views (runtime values are checked with typeof/Array.isArray;
// the declared types are only what a correct writer produces)
// ---------------------------------------------------------------------------

interface RawRouteApplyRequestDocument {
  v?: number;
  id?: string;
  management_url?: string;
  created_at_ms?: number;
}

interface RawRouteApplyResultDocument {
  v?: number;
  id?: string;
  state?: string;
  mode?: string;
  error?: string;
  routes?: string[] | null;
  at_ms?: number;
}

// ---------------------------------------------------------------------------
// checked fields (types + bounds only)
// ---------------------------------------------------------------------------

function malformed(): RoutePreferencesError {
  return new RoutePreferencesError(REASON_MALFORMED);
}

function hasWhitespaceOrControl(value: string): boolean {
  for (let i: number = 0; i < value.length; i++) {
    const code: number = value.charCodeAt(i);
    if (code <= 0x20 || code === 0x7f) {
      return true;
    }
  }
  return false;
}

// Opaque, but it must be a usable non-empty key: the result echoes it verbatim
// and both sides compare it exactly.
function checkedId(value: string | undefined): string {
  if (typeof value !== 'string' || value.length === 0 || value.length > MAX_REQUEST_ID_CHARS) {
    throw malformed();
  }
  return value;
}

// The scope key is compared exactly by the reader's own selection load, so it
// must be a usable URL string (public configuration, never a credential).
function checkedScopeUrl(value: string | undefined): string {
  if (typeof value !== 'string' || value.length === 0 || value.length > MAX_URL_CHARS ||
    hasWhitespaceOrControl(value)) {
    throw malformed();
  }
  return value;
}

// Epoch milliseconds from the caller's clock; this module owns no clock and
// never derives one. Negative and non-integer stamps are malformed.
function checkedTimestamp(value: number | undefined): number {
  if (typeof value !== 'number' || !Number.isInteger(value) || value < 0) {
    throw malformed();
  }
  return value;
}

function checkedState(value: string | undefined): string {
  if (value === ROUTE_APPLY_STATE_APPLYING || value === ROUTE_APPLY_STATE_APPLIED ||
    value === ROUTE_APPLY_STATE_FAILED) {
    return value;
  }
  throw malformed();
}

function checkedMode(value: string | undefined): string {
  if (value === ROUTE_APPLY_MODE_NONE || value === ROUTE_APPLY_MODE_NO_OS_CHANGE ||
    value === ROUTE_APPLY_MODE_RECREATED || value === ROUTE_APPLY_MODE_STORED_NO_MAP) {
    return value;
  }
  throw malformed();
}

// A free-text failure reason. It carries no bound of its own: the document's
// 16 KiB file bound is the only size guard, and truncating a native message
// would lose the only diagnosis the user gets.
function checkedError(value: string | undefined): string {
  if (typeof value !== 'string') {
    throw malformed();
  }
  return value;
}

function checkedRouteList(values: string[] | null | undefined): string[] {
  if (!Array.isArray(values)) {
    throw malformed();
  }
  const items: string[] = values as string[];
  const seen: Set<string> = new Set<string>();
  const out: string[] = [];
  for (const value of items) {
    if (typeof value !== 'string' || value.length === 0 || value.length > MAX_NET_ID_CHARS) {
      throw malformed();
    }
    if (!seen.has(value)) {
      seen.add(value);
      out.push(value);
    }
  }
  out.sort();
  return out;
}

// ---------------------------------------------------------------------------
// apply request (parse / serialize)
// ---------------------------------------------------------------------------

export function serializeRouteApplyRequestDocument(
  request: RouteApplyRequest | null | undefined): string {
  if (request === null || request === undefined || typeof request !== 'object' ||
    Array.isArray(request)) {
    throw malformed();
  }
  return JSON.stringify({
    v: ROUTE_APPLY_REQUEST_VERSION,
    id: checkedId(request.id),
    management_url: checkedScopeUrl(request.management_url),
    created_at_ms: checkedTimestamp(request.created_at_ms)
  });
}

// Strict parse. A wrong version and every shape error are stable errors
// (REASON_VERSION / REASON_MALFORMED); the caller decides what to do with a
// request it cannot read — this module never substitutes an id or a scope.
export function parseRouteApplyRequestDocument(text: string): RouteApplyRequest {
  let doc: RawRouteApplyRequestDocument | null = null;
  try {
    doc = JSON.parse(text) as RawRouteApplyRequestDocument | null;
  } catch (error) {
    throw malformed();
  }
  if (doc === null || typeof doc !== 'object' || Array.isArray(doc)) {
    throw malformed();
  }
  if (typeof doc.v !== 'number' || doc.v !== ROUTE_APPLY_REQUEST_VERSION) {
    throw new RoutePreferencesError(REASON_VERSION);
  }
  return {
    id: checkedId(doc.id),
    management_url: checkedScopeUrl(doc.management_url),
    created_at_ms: checkedTimestamp(doc.created_at_ms)
  };
}

// ---------------------------------------------------------------------------
// apply result (parse / serialize)
// ---------------------------------------------------------------------------

export function serializeRouteApplyResultDocument(
  result: RouteApplyResult | null | undefined): string {
  if (result === null || result === undefined || typeof result !== 'object' ||
    Array.isArray(result)) {
    throw malformed();
  }
  return JSON.stringify({
    v: ROUTE_APPLY_RESULT_VERSION,
    id: checkedId(result.id),
    state: checkedState(result.state),
    mode: checkedMode(result.mode),
    error: checkedError(result.error),
    routes: checkedRouteList(result.routes),
    at_ms: checkedTimestamp(result.at_ms)
  });
}

export function parseRouteApplyResultDocument(text: string): RouteApplyResult {
  let doc: RawRouteApplyResultDocument | null = null;
  try {
    doc = JSON.parse(text) as RawRouteApplyResultDocument | null;
  } catch (error) {
    throw malformed();
  }
  if (doc === null || typeof doc !== 'object' || Array.isArray(doc)) {
    throw malformed();
  }
  if (typeof doc.v !== 'number' || doc.v !== ROUTE_APPLY_RESULT_VERSION) {
    throw new RoutePreferencesError(REASON_VERSION);
  }
  return {
    id: checkedId(doc.id),
    state: checkedState(doc.state),
    mode: checkedMode(doc.mode),
    error: checkedError(doc.error),
    routes: checkedRouteList(doc.routes),
    at_ms: checkedTimestamp(doc.at_ms)
  };
}

// The ONE reading of "the OS side really is at the requested state". A
// `stored-no-map` answer stored the preference without handing a map to the OS,
// so it is NOT applied even though state says `applied`; the contradictory
// combination is read as not-applied rather than trusted, and every other
// state (applying / failed) is not applied either.
export function routeApplyResultIsApplied(result: RouteApplyResult): boolean {
  return result.state === ROUTE_APPLY_STATE_APPLIED &&
    result.mode !== ROUTE_APPLY_MODE_STORED_NO_MAP;
}
