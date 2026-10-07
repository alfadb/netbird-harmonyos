/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Pure per-NetID route-selection state + the two public route documents
// (the persisted selection and the offline route catalog). ZERO imports: the
// host selfcheck stages this file byte-for-byte into a node fixture
// (client/entry/selfcheck/run-route-preferences.mts). File IO lives in
// ./NetBirdRouteStore (temp + fsync + rename), never here.
//
// ## Selection unit: the NetID (HA group), never the physical route
//
// A logical network is identified by its management NetID. Several advertised
// routes can carry the same NetID with different route id/peer/metric — the
// high-availability candidates of one network (live example pinned by
// docs/route-audit.md: two net_id=HostInternal rows for 192.168.50.0/24 with
// metric 200 and 100 on two peers). Toggling one NetID keeps every HA
// candidate of that network together; a route id or peer is never the key.
//
// ## Core parity (client/core/src/route_selector.rs, upstream-pinned)
//
// This module is the shell-side twin of the pure core `RouteSelector`. The
// rules intentionally copied one-for-one:
//
// - `deselect_all == true` blocks every NetID, including ones that do not
//   exist yet; otherwise a NetID is enabled unless it is in
//   `deselected_routes`.
// - `selected_routes` records explicit user intent (it distinguishes a pick
//   from the default and survives a reload) but it NEVER overrides a
//   conflicting `deselected_routes` entry — deselected wins, and a hand-written
//   conflict is preserved verbatim.
// - Enabling under `deselect_all` clears the flag and marks every KNOWN NetID
//   deselected except the requested one (core `select_routes` replace branch):
//   networks that only appear later are default-on again.
// - Enabling an unknown NetID is rejected with no mutation at all, and that
//   early return also leaves `deselect_all` set (a typo must not re-enable
//   everything).
// - Disabling is a no-op while `deselect_all` is set (even for an unknown
//   NetID); otherwise it removes the NetID from `selected_routes` and adds it
//   to `deselected_routes`.
// - Select-all clears both lists and the flag; deselect-all clears both lists
//   and sets the flag (so it also covers future routes).
//
// Deliberately NOT implemented in this v1 increment (see the increment scope):
// no DNS, no IPv6, and no exit-node manager behaviour — no automatic exit
// exclusivity, no `skip_auto_apply` reconciliation. An explicit 0.0.0.0/0 exit
// route is toggled by its NetID like any other IPv4 route; `is_exit` is only
// metadata so a later UI can put exits in a separate section.
//
// ## Exact NetID strings
//
// NetIDs are preserved byte-for-byte: no lowering, no trimming. An empty NetID
// is a legitimate group key (the core's `ManagedRoute.net_id` is simply empty
// when management sends no NetID), so this module never invents a prefix
// fallback.
//
// ## Persisted selection document (filesDir/netbird-route-selection.json)
//
// {"v":1,"management_url":"<url>","selection":{...}} — scoped to one
// management server; the URL is public configuration. A missing file means
// "no stored preference" (default all-on). A document for ANOTHER management
// URL is likewise treated as absent by the store. A malformed or
// wrong-version document is a STABLE ERROR, never a silent default — silently
// returning all-on would re-enable routes the user turned off and then
// overwrite the evidence on the next save.
//
// ## Public catalog document (filesDir/netbird-routes.json)
//
// {"v":1,"management_url":"<url>","updated_at_ms":<n>,"request_id":"<id>",
//  "catalog":<native getter contract>} where the catalog is exactly
// {available:boolean, serial:number, selection:{...},
//  networks:[{net_id:string,networks:string[],candidate_count:number,
//             enabled:boolean,is_exit:boolean}]}
// It is a plain public snapshot (route ids/prefixes are not secrets) written by
// the VPE later; the UI reads it offline (no native process calls) and uses
// updated_at_ms to mark a stale cache. Nothing here carries a token, a private
// key or a device-config write.

export const ROUTE_SELECTION_VERSION: number = 1;
export const ROUTE_CATALOG_VERSION: number = 1;
export const MAX_URL_CHARS: number = 2048;
export const MAX_NET_ID_CHARS: number = 512;
export const MAX_PREFIX_CHARS: number = 128;
export const MAX_REQUEST_ID_CHARS: number = 256;

export const REASON_MALFORMED: string = 'malformed';
export const REASON_VERSION: string = 'version';
export const REASON_UNKNOWN_ROUTE: string = 'unknown-route';
export const REASON_UNAVAILABLE: string = 'unavailable';

export const CATALOG_STATUS_OK: string = 'ok';
export const CATALOG_STATUS_MISSING: string = 'missing';
export const CATALOG_STATUS_SCOPE_MISMATCH: string = 'scope-mismatch';
export const CATALOG_STATUS_MALFORMED: string = 'malformed';
export const CATALOG_STATUS_VERSION: string = 'version';
export const CATALOG_STATUS_UNAVAILABLE: string = 'unavailable';

// The native core getter contract's selection object, verbatim (snake_case so
// the persisted document, the catalog and the core JSON need no translation).
export interface RouteSelection {
  selected_routes: string[];
  deselected_routes: string[];
  deselect_all: boolean;
}

// One logical network (one NetID) in the native getter's `networks` array.
// `networks` holds the distinct advertised prefixes of that NetID;
// `candidate_count` is the number of HA candidate rows behind it.
export interface RouteNetworkEntry {
  net_id: string;
  networks: string[];
  candidate_count: number;
  enabled: boolean;
  is_exit: boolean;
}

// The native getter contract itself, as stored under the wrapper's `catalog`.
export interface RouteCatalog {
  available: boolean;
  serial: number;
  selection: RouteSelection;
  networks: RouteNetworkEntry[];
}

// A parsed selection document: the scope URL plus the selection it scopes.
export interface StoredRouteSelection {
  management_url: string;
  selection: RouteSelection;
}

// A parsed catalog document: scope URL, cache stamp, writer request id, catalog.
export interface StoredRouteCatalog {
  management_url: string;
  updated_at_ms: number;
  request_id: string;
  catalog: RouteCatalog;
}

// Typed result of an offline catalog read. `status` is one of the
// CATALOG_STATUS_* tokens; `catalog` is non-null only for CATALOG_STATUS_OK.
// `updated_at_ms` (and `request_id`) are reported whenever the document was
// readable at all, so a UI can mark an old cache stale even when the scope does
// not match and a writer can correlate its last snapshot.
export interface RouteCatalogReadResult {
  status: string;
  management_url: string;
  updated_at_ms: number;
  request_id: string;
  catalog: RouteCatalog | null;
}

// Stable failure of a state mutation or of a document parse. `reason` is one of
// the REASON_* tokens; `net_ids` carries the rejected IDs for
// REASON_UNKNOWN_ROUTE.
export class RoutePreferencesError extends Error {
  readonly reason: string;
  readonly net_ids: string[];

  constructor(reason: string, netIds?: string[]) {
    super(reason);
    this.name = 'RoutePreferencesError';
    this.reason = reason;
    this.net_ids = netIds === undefined ? [] : netIds.slice();
  }
}

// ---------------------------------------------------------------------------
// raw document views (runtime values are checked with typeof/Array.isArray;
// the declared types are only what a correct writer produces)
// ---------------------------------------------------------------------------

interface RawRouteSelection {
  selected_routes?: string[] | null;
  deselected_routes?: string[] | null;
  deselect_all?: boolean;
}

interface RawRouteSelectionDocument {
  v?: number;
  management_url?: string;
  selection?: RawRouteSelection | null;
}

interface RawRouteNetworkEntry {
  net_id?: string;
  networks?: string[] | null;
  candidate_count?: number;
  enabled?: boolean;
  is_exit?: boolean;
}

interface RawRouteCatalog {
  available?: boolean;
  serial?: number;
  selection?: RawRouteSelection | null;
  networks?: RawRouteNetworkEntry[] | null;
}

interface RawRouteCatalogDocument {
  v?: number;
  management_url?: string;
  updated_at_ms?: number;
  request_id?: string;
  catalog?: RawRouteCatalog | null;
}

// ---------------------------------------------------------------------------
// pure helpers
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

// The scope key must be a usable URL string; it is public configuration, not a
// credential, and is compared exactly (no normalization) by the store.
function validateScopeUrl(managementUrl: string): void {
  if (managementUrl.length === 0 || managementUrl.length > MAX_URL_CHARS ||
    hasWhitespaceOrControl(managementUrl)) {
    throw malformed();
  }
}

// Ascending order, duplicates collapsed. JS sorts by UTF-16 code unit; for the
// ASCII route/NetID alphabet management emits this is the same order as the
// core's BTreeSet<String>.
function sortedUnique(values: string[]): string[] {
  const seen: Set<string> = new Set<string>();
  const out: string[] = [];
  for (const value of values) {
    if (!seen.has(value)) {
      seen.add(value);
      out.push(value);
    }
  }
  out.sort();
  return out;
}

// First-seen order, duplicates collapsed (the core's LogicalNetwork keeps
// prefixes in map order; only the identity of each prefix matters here).
function uniquePreservingOrder(values: string[]): string[] {
  const seen: Set<string> = new Set<string>();
  const out: string[] = [];
  for (const value of values) {
    if (!seen.has(value)) {
      seen.add(value);
      out.push(value);
    }
  }
  return out;
}

function arraysEqual(a: string[], b: string[]): boolean {
  if (a.length !== b.length) {
    return false;
  }
  for (let i: number = 0; i < a.length; i++) {
    if (a[i] !== b[i]) {
      return false;
    }
  }
  return true;
}

function withoutId(values: string[], netId: string): string[] {
  const out: string[] = [];
  for (const value of values) {
    if (value !== netId) {
      out.push(value);
    }
  }
  return out;
}

function requireKnownNetId(netId: string, knownNetIds: string[]): void {
  if (knownNetIds.indexOf(netId) < 0) {
    throw new RoutePreferencesError(REASON_UNKNOWN_ROUTE, [netId]);
  }
}

// ---------------------------------------------------------------------------
// state (pure; every mutator returns a FRESH state and never touches its input)
// ---------------------------------------------------------------------------

// A fresh selector: nothing explicit, everything default-on (future NetIDs too).
export function routeSelectionDefault(): RouteSelection {
  return { selected_routes: [], deselected_routes: [], deselect_all: false };
}

// Deep copy for a UI draft: editing/cancelling the draft can never mutate the
// state the page is currently showing.
export function routeSelectionClone(selection: RouteSelection): RouteSelection {
  return {
    selected_routes: selection.selected_routes.slice(),
    deselected_routes: selection.deselected_routes.slice(),
    deselect_all: selection.deselect_all
  };
}

export function routeSelectionEquals(a: RouteSelection, b: RouteSelection): boolean {
  return a.deselect_all === b.deselect_all &&
    arraysEqual(a.selected_routes, b.selected_routes) &&
    arraysEqual(a.deselected_routes, b.deselected_routes);
}

// Enabled = the flag is off and the NetID is not explicitly deselected. The
// explicit `selected_routes` entry does NOT override a deselected conflict.
export function routeSelectionIsEnabled(selection: RouteSelection, netId: string): boolean {
  if (selection.deselect_all === true) {
    return false;
  }
  return selection.deselected_routes.indexOf(netId) < 0;
}

// Whether this NetID was explicitly touched in either direction (core
// `has_user_selection`).
export function routeSelectionHasUserSelection(selection: RouteSelection, netId: string): boolean {
  return selection.selected_routes.indexOf(netId) >= 0 ||
    selection.deselected_routes.indexOf(netId) >= 0;
}

// Global all-on: clears both lists and the flag, so later networks are on too.
// The UI label must say the switch includes future routes (a batch of the
// currently visible NetIDs is a different action: use the per-NetID mutators).
export function routeSelectionSelectAll(): RouteSelection {
  return routeSelectionDefault();
}

// Global all-off: clears both lists and sets the flag, so it also covers
// networks that do not exist yet.
export function routeSelectionDeselectAll(): RouteSelection {
  return { selected_routes: [], deselected_routes: [], deselect_all: true };
}

// Enable one logical network (core `select_routes(..., append = true)`).
// Unknown NetID: REASON_UNKNOWN_ROUTE with NO mutation. Under a global
// all-off this is the explicit exception that clears the flag and marks every
// known NetID deselected except the requested one.
export function routeSelectionEnable(selection: RouteSelection, netId: string,
  knownNetIds: string[]): RouteSelection {
  requireKnownNetId(netId, knownNetIds);
  const next: RouteSelection = routeSelectionClone(selection);
  if (next.deselect_all) {
    next.selected_routes = [];
    next.deselected_routes = sortedUnique(knownNetIds);
  }
  next.selected_routes = sortedUnique(next.selected_routes.concat([netId]));
  next.deselected_routes = withoutId(next.deselected_routes, netId);
  next.deselect_all = false;
  return next;
}

// Disable one logical network (core `deselect_routes`). A no-op while the
// global all-off flag is set — that check runs before validation, exactly like
// the core (an unknown ID under the flag is not an error). Unknown NetID
// otherwise: REASON_UNKNOWN_ROUTE with NO mutation.
export function routeSelectionDisable(selection: RouteSelection, netId: string,
  knownNetIds: string[]): RouteSelection {
  const next: RouteSelection = routeSelectionClone(selection);
  if (next.deselect_all) {
    return next;
  }
  requireKnownNetId(netId, knownNetIds);
  next.deselected_routes = sortedUnique(next.deselected_routes.concat([netId]));
  next.selected_routes = withoutId(next.selected_routes, netId);
  return next;
}

// The checkbox action: flip the current verdict. Under the global all-off flag
// every NetID reads disabled, so a toggle is the explicit enable exception.
export function routeSelectionToggle(selection: RouteSelection, netId: string,
  knownNetIds: string[]): RouteSelection {
  if (routeSelectionIsEnabled(selection, netId)) {
    return routeSelectionDisable(selection, netId, knownNetIds);
  }
  return routeSelectionEnable(selection, netId, knownNetIds);
}

// Every distinct NetID the cached catalog knows, sorted ascending (the `known`
// set the core's mutation APIs validate against).
export function routeSelectionKnownNetIds(catalog: RouteCatalog): string[] {
  const ids: string[] = [];
  for (const entry of catalog.networks) {
    ids.push(entry.net_id);
  }
  return sortedUnique(ids);
}

// The per-NetID rows a settings UI renders: catalog order, one row per NetID
// (same-NetID HA candidates already collapsed by the writer), `enabled`
// recomputed from the SELECTION (the catalog's own `enabled` is only the
// core's verdict at capture time), `is_exit` available for a separate section.
export function routeSelectionRows(selection: RouteSelection,
  catalog: RouteCatalog): RouteNetworkEntry[] {
  const rows: RouteNetworkEntry[] = [];
  for (const entry of catalog.networks) {
    rows.push({
      net_id: entry.net_id,
      networks: entry.networks.slice(),
      candidate_count: entry.candidate_count,
      enabled: routeSelectionIsEnabled(selection, entry.net_id),
      is_exit: entry.is_exit
    });
  }
  return rows;
}

// ---------------------------------------------------------------------------
// selection document (parse / serialize)
// ---------------------------------------------------------------------------

// A missing key is the core's "empty" default; a present wrong value (including
// null) is a loud malformed error.
function parseNetIdArray(value: string[] | null | undefined): string[] {
  if (value === undefined) {
    return [];
  }
  if (!Array.isArray(value)) {
    throw malformed();
  }
  const items: string[] = value as string[];
  const out: string[] = [];
  for (const item of items) {
    if (typeof item !== 'string' || item.length > MAX_NET_ID_CHARS) {
      throw malformed();
    }
    out.push(item);
  }
  return sortedUnique(out);
}

function parseOptionalBoolean(value: boolean | undefined): boolean {
  if (value === undefined) {
    return false;
  }
  if (typeof value !== 'boolean') {
    throw malformed();
  }
  return value;
}

function parsePrefixArray(value: string[] | null | undefined): string[] {
  if (!Array.isArray(value)) {
    throw malformed();
  }
  const items: string[] = value as string[];
  const out: string[] = [];
  for (const item of items) {
    if (typeof item !== 'string' || item.length > MAX_PREFIX_CHARS) {
      throw malformed();
    }
    out.push(item);
  }
  return uniquePreservingOrder(out);
}

function parseRouteSelection(value: RawRouteSelection | null | undefined): RouteSelection {
  if (value === null || value === undefined || typeof value !== 'object' || Array.isArray(value)) {
    throw malformed();
  }
  return {
    selected_routes: parseNetIdArray(value.selected_routes),
    deselected_routes: parseNetIdArray(value.deselected_routes),
    deselect_all: parseOptionalBoolean(value.deselect_all)
  };
}

// Canonical form (sorted, deduplicated, validated): the persisted bytes are
// independent of mutation order. Throws REASON_MALFORMED on a bad shape or a
// null/absent selection.
function canonicalRouteSelection(selection: RouteSelection | null | undefined): RouteSelection {
  if (selection === null || selection === undefined || typeof selection !== 'object' ||
    Array.isArray(selection)) {
    throw malformed();
  }
  if (typeof selection.deselect_all !== 'boolean') {
    throw malformed();
  }
  return {
    selected_routes: sortedUnique(checkedNetIdList(selection.selected_routes)),
    deselected_routes: sortedUnique(checkedNetIdList(selection.deselected_routes)),
    deselect_all: selection.deselect_all
  };
}

function checkedNetIdList(values: string[]): string[] {
  if (!Array.isArray(values)) {
    throw malformed();
  }
  const items: string[] = values as string[];
  const out: string[] = [];
  for (const value of items) {
    if (typeof value !== 'string' || value.length > MAX_NET_ID_CHARS) {
      throw malformed();
    }
    out.push(value);
  }
  return out;
}

function checkedPrefixList(values: string[]): string[] {
  if (!Array.isArray(values)) {
    throw malformed();
  }
  const items: string[] = values as string[];
  const out: string[] = [];
  for (const value of items) {
    if (typeof value !== 'string' || value.length > MAX_PREFIX_CHARS) {
      throw malformed();
    }
    out.push(value);
  }
  return out;
}

export function serializeRouteSelectionDocument(managementUrl: string,
  selection: RouteSelection): string {
  validateScopeUrl(managementUrl);
  const canonical: RouteSelection = canonicalRouteSelection(selection);
  const nestedSelection: RouteSelection = {
    selected_routes: canonical.selected_routes,
    deselected_routes: canonical.deselected_routes,
    deselect_all: canonical.deselect_all
  };
  return JSON.stringify({
    v: ROUTE_SELECTION_VERSION,
    management_url: managementUrl,
    selection: nestedSelection
  });
}

// Strict parse of the persisted selection document. Wrong version and any shape
// error are stable errors (REASON_VERSION / REASON_MALFORMED) — the caller must
// not fall back to a default that would silently re-enable routes.
export function parseRouteSelectionDocument(text: string): StoredRouteSelection {
  let doc: RawRouteSelectionDocument | null = null;
  try {
    doc = JSON.parse(text) as RawRouteSelectionDocument | null;
  } catch (error) {
    throw malformed();
  }
  if (doc === null || typeof doc !== 'object' || Array.isArray(doc)) {
    throw malformed();
  }
  if (typeof doc.v !== 'number' || doc.v !== ROUTE_SELECTION_VERSION) {
    throw new RoutePreferencesError(REASON_VERSION);
  }
  if (typeof doc.management_url !== 'string') {
    throw malformed();
  }
  validateScopeUrl(doc.management_url);
  return {
    management_url: doc.management_url,
    selection: parseRouteSelection(doc.selection)
  };
}

// ---------------------------------------------------------------------------
// catalog document (parse / serialize)
// ---------------------------------------------------------------------------

function parseSerial(value: number | undefined): number {
  if (typeof value !== 'number' || !Number.isInteger(value) || value < 0) {
    throw malformed();
  }
  return value;
}

function parseCandidateCount(value: number | undefined): number {
  if (typeof value !== 'number' || !Number.isInteger(value) || value < 1) {
    throw malformed();
  }
  return value;
}

function parseRouteCatalog(value: RawRouteCatalog | null | undefined): RouteCatalog {
  if (value === null || value === undefined || typeof value !== 'object' || Array.isArray(value)) {
    throw malformed();
  }
  if (typeof value.available !== 'boolean' || !Array.isArray(value.networks)) {
    throw malformed();
  }
  const rawNetworks: (RawRouteNetworkEntry | null | undefined)[] =
    value.networks as (RawRouteNetworkEntry | null | undefined)[];
  const networks: RouteNetworkEntry[] = [];
  const seen: Set<string> = new Set<string>();
  for (const entry of rawNetworks) {
    if (entry === null || entry === undefined || typeof entry !== 'object' || Array.isArray(entry)) {
      throw malformed();
    }
    if (typeof entry.net_id !== 'string' || entry.net_id.length > MAX_NET_ID_CHARS) {
      throw malformed();
    }
    if (seen.has(entry.net_id)) {
      throw malformed();
    }
    seen.add(entry.net_id);
    if (typeof entry.enabled !== 'boolean' || typeof entry.is_exit !== 'boolean') {
      throw malformed();
    }
    networks.push({
      net_id: entry.net_id,
      networks: parsePrefixArray(entry.networks),
      candidate_count: parseCandidateCount(entry.candidate_count),
      enabled: entry.enabled,
      is_exit: entry.is_exit
    });
  }
  return {
    available: value.available,
    serial: parseSerial(value.serial),
    selection: parseRouteSelection(value.selection),
    networks: networks
  };
}

// Validate + canonicalize a catalog before it is persisted. Throws
// REASON_MALFORMED on any wrong field, so a broken writer cannot replace a good
// cache with a bad one.
function canonicalRouteCatalog(catalog: RouteCatalog | null | undefined): RouteCatalog {
  if (catalog === null || catalog === undefined || typeof catalog !== 'object' ||
    Array.isArray(catalog)) {
    throw malformed();
  }
  if (typeof catalog.available !== 'boolean' || !Array.isArray(catalog.networks)) {
    throw malformed();
  }
  const rawNetworks: (RouteNetworkEntry | null | undefined)[] =
    catalog.networks as (RouteNetworkEntry | null | undefined)[];
  const networks: RouteNetworkEntry[] = [];
  const seen: Set<string> = new Set<string>();
  for (const entry of rawNetworks) {
    if (entry === null || entry === undefined || typeof entry !== 'object' || Array.isArray(entry)) {
      throw malformed();
    }
    if (typeof entry.net_id !== 'string' || entry.net_id.length > MAX_NET_ID_CHARS) {
      throw malformed();
    }
    if (seen.has(entry.net_id)) {
      throw malformed();
    }
    seen.add(entry.net_id);
    if (typeof entry.enabled !== 'boolean' || typeof entry.is_exit !== 'boolean') {
      throw malformed();
    }
    networks.push({
      net_id: entry.net_id,
      networks: uniquePreservingOrder(checkedPrefixList(entry.networks)),
      candidate_count: parseCandidateCount(entry.candidate_count),
      enabled: entry.enabled,
      is_exit: entry.is_exit
    });
  }
  return {
    available: catalog.available,
    serial: parseSerial(catalog.serial),
    selection: canonicalRouteSelection(catalog.selection),
    networks: networks
  };
}

export function serializeRouteCatalogDocument(managementUrl: string, catalog: RouteCatalog,
  requestId: string, updatedAtMs: number): string {
  validateScopeUrl(managementUrl);
  if (typeof requestId !== 'string' || requestId.length > MAX_REQUEST_ID_CHARS) {
    throw malformed();
  }
  if (typeof updatedAtMs !== 'number' || !Number.isFinite(updatedAtMs) || updatedAtMs < 0) {
    throw malformed();
  }
  const canonical: RouteCatalog = canonicalRouteCatalog(catalog);
  const networks: RouteNetworkEntry[] = [];
  for (const entry of canonical.networks) {
    networks.push({
      net_id: entry.net_id,
      networks: entry.networks,
      candidate_count: entry.candidate_count,
      enabled: entry.enabled,
      is_exit: entry.is_exit
    });
  }
  const nestedSelection: RouteSelection = {
    selected_routes: canonical.selection.selected_routes,
    deselected_routes: canonical.selection.deselected_routes,
    deselect_all: canonical.selection.deselect_all
  };
  const nestedCatalog: RouteCatalog = {
    available: canonical.available,
    serial: canonical.serial,
    selection: nestedSelection,
    networks: networks
  };
  return JSON.stringify({
    v: ROUTE_CATALOG_VERSION,
    management_url: managementUrl,
    updated_at_ms: updatedAtMs,
    request_id: requestId,
    catalog: nestedCatalog
  });
}

// Strict parse of the public catalog document. Same rule as the selection
// document: a wrong version or a malformed shape is a stable error, never a
// silently empty catalog.
export function parseRouteCatalogDocument(text: string): StoredRouteCatalog {
  let doc: RawRouteCatalogDocument | null = null;
  try {
    doc = JSON.parse(text) as RawRouteCatalogDocument | null;
  } catch (error) {
    throw malformed();
  }
  if (doc === null || typeof doc !== 'object' || Array.isArray(doc)) {
    throw malformed();
  }
  if (typeof doc.v !== 'number' || doc.v !== ROUTE_CATALOG_VERSION) {
    throw new RoutePreferencesError(REASON_VERSION);
  }
  if (typeof doc.management_url !== 'string') {
    throw malformed();
  }
  validateScopeUrl(doc.management_url);
  if (typeof doc.updated_at_ms !== 'number' || !Number.isFinite(doc.updated_at_ms) ||
    doc.updated_at_ms < 0) {
    throw malformed();
  }
  if (typeof doc.request_id !== 'string' || doc.request_id.length > MAX_REQUEST_ID_CHARS) {
    throw malformed();
  }
  return {
    management_url: doc.management_url,
    updated_at_ms: doc.updated_at_ms,
    request_id: doc.request_id,
    catalog: parseRouteCatalog(doc.catalog)
  };
}

// Cache-age test for the UI: a non-positive or future stamp is stale as well
// (a clock jump must not make an old catalog look fresh).
export function routeCatalogIsStale(updatedAtMs: number, nowMs: number, maxAgeMs: number): boolean {
  if (!Number.isFinite(updatedAtMs) || updatedAtMs <= 0) {
    return true;
  }
  if (!Number.isFinite(nowMs) || nowMs < updatedAtMs) {
    return true;
  }
  if (!Number.isFinite(maxAgeMs) || maxAgeMs < 0) {
    return true;
  }
  return nowMs - updatedAtMs > maxAgeMs;
}
