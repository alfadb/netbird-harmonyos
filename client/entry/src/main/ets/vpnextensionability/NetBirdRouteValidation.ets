// Per-field validation of route entries handed to the platform VpnConfig
// (H1 diagnosis: docs/vpn-create-401-diagnosis-20260915.md §3.2/§4/§5).
//
// WHY: RouteInfo's base fields are REQUIRED with fixed types
// (@ohos.net.connection.d.ts, SDK 26.0.0.821 — re-verified in-tree):
//   interface      L2176 (string)      destination L2183 (LinkAddress)
//   gateway        L2190 (NetAddress)  hasGateway  L2198 (boolean)
//   isDefaultRoute L2209 (boolean)     isExcludedRoute? L2217 (optional,
//                  @since 20, default false)
//   LinkAddress.prefixLength L2239 (number; d.ts declares NO range)
//   NetAddress.family? L2267 (optional; 1 = IPv4, 2 = IPv6, default 1)
// The core snapshot reaches applyNetworkConfig through a bare JSON.parse
// (NetBirdConnector.ets connectorNetworkConfig) with NO field-level guard,
// so a missing `is_default` key becomes isDefaultRoute: undefined and a
// non-numeric prefix segment becomes prefixLength: NaN — both land on
// required platform positions and are the H1 candidate mechanism behind the
// device-side `create()` 401 Parameter error.
//
// CONTRACT (fail-closed):
// - validateRouteEntries is strictly read-only. It NEVER repairs, defaults
//   or drops an entry (no implicit-default fill-in: that would mask a core
//   side defect). The caller must NOT call connection.create() when
//   ok=false and must follow the existing abort style instead.
// - Only SDK-DECLARED constraints are enforced. No invented rules: the d.ts
//   declares no prefixLength range and no topology rules, and
//   platform-undocumented checks are explicitly out of scope (diagnosis
//   §2.4-2). `family` is only checked when an entry actually carries it.
// - kind tokens: 'missing' (absent or null), 'type' (present, wrong JS
//   type), 'nan' (number but NaN), 'range' (right type, outside the
//   declared domain: empty string, non-finite number, family not 1|2).
//
// PURE MODULE (zero imports, same discipline as NetBirdEndpointExclusion.ets):
// the selfcheck copies this real file byte-for-byte into a temp fixture and
// imports it under plain node.
//
// Field values are read through the FieldValue union and checked with
// typeof at RUN TIME: the declared TS types on VpnRouteConfig are exactly
// what a JSON.parse passthrough can violate, so this module must not trust
// them. (ArkTS forbids any/unknown — arkts-no-any-unknown — and the union
// spans the whole JSON value domain, so nothing narrower would be honest.)

// Runtime value domain of a route-entry field as it may arrive from the
// JSON.parse passthrough (JSON: string | number | boolean | null, plus the
// absent/undefined case).
type FieldValue = string | number | boolean | null | undefined;

// Structural view of one shell-side route entry (VpnRouteConfig in
// NetBirdVpnConfig.ets satisfies this shape).
export interface RouteEntryView {
  destination?: FieldValue;
  prefixLength?: FieldValue;
  gateway?: FieldValue;
  hasGateway?: FieldValue;
  isDefaultRoute?: FieldValue;
  isExcludedRoute?: FieldValue;
  // NetAddress.family passthrough. The shell-side VpnRouteConfig carries no
  // family field today (buildPlatformVpnConfig derives 1|2 from the
  // destination text), so production entries never set it; the check guards
  // the SDK structural contract should any source start carrying it.
  family?: FieldValue;
  // Provenance tag written by applyNetworkConfig for core-snapshot-derived
  // entries ('managed'). Base-template and derived-exclusion entries stay
  // untagged and are classified by shape (see routeEntrySource).
  source?: string;
}

// One field violation of one route entry. index = position in the final
// route list; index -1 = the config-level `interface` name (not an entry).
export interface RouteViolation {
  index: number;
  source: string;
  field: string;
  kind: string;
}

export interface RouteCheckResult {
  ok: boolean;
  violations: RouteViolation[];
}

// Stable provenance class for the truth log and the violation markers:
// 'managed' (core snapshot), 'exclusion' (base LAN / derived endpoint
// freeze), 'default' (base default-route template), 'base' (other
// base-template entries, reachable via the no-network-map fallback).
// Classification follows the SDK-declared meaning of the flags; a flag whose
// runtime TYPE is corrupt may classify as 'base' — the per-field truth dump
// carries the real values alongside.
export function routeEntrySource(entry: RouteEntryView): string {
  if (entry.source === 'managed') {
    return 'managed';
  }
  if (entry.isExcludedRoute === true) {
    return 'exclusion';
  }
  if (entry.isDefaultRoute === true) {
    return 'default';
  }
  return 'base';
}

function requiredStringKind(value: FieldValue): string | null {
  if (value === undefined || value === null) {
    return 'missing';
  }
  if (typeof value !== 'string') {
    return 'type';
  }
  return value.length === 0 ? 'range' : null;
}

function requiredNumberKind(value: FieldValue): string | null {
  if (value === undefined || value === null) {
    return 'missing';
  }
  if (typeof value !== 'number') {
    return 'type';
  }
  if (Number.isNaN(value)) {
    return 'nan';
  }
  return Number.isFinite(value) ? null : 'range';
}

function requiredBooleanKind(value: FieldValue): string | null {
  if (value === undefined || value === null) {
    return 'missing';
  }
  return typeof value === 'boolean' ? null : 'type';
}

// isExcludedRoute is SDK-optional (@since 20, default false): absent is the
// normal platform-accepted shape, only a present non-boolean violates.
function optionalBooleanKind(value: FieldValue): string | null {
  if (value === undefined || value === null) {
    return null;
  }
  return typeof value === 'boolean' ? null : 'type';
}

// NetAddress.family is optional with default 1 (d.ts L2267); when present it
// must be exactly 1 or 2.
function optionalFamilyKind(value: FieldValue): string | null {
  if (value === undefined || value === null) {
    return null;
  }
  if (typeof value !== 'number') {
    return 'type';
  }
  return value === 1 || value === 2 ? null : 'range';
}

function collect(violations: RouteViolation[], index: number, source: string,
  field: string, kind: string | null): void {
  if (kind !== null) {
    const violation: RouteViolation =
      { index: index, source: source, field: field, kind: kind };
    violations.push(violation);
  }
}

// Validate every route entry against the SDK-declared RouteInfo contract.
// Returns ALL violations found (a single violating entry fails the WHOLE
// config — callers must not create with a filtered/sanitized subset).
export function validateRouteEntries(routes: RouteEntryView[],
  interfaceName: string): RouteCheckResult {
  const violations: RouteViolation[] = [];
  const interfaceKind: string | null = requiredStringKind(interfaceName);
  if (interfaceKind !== null) {
    const configViolation: RouteViolation =
      { index: -1, source: 'config', field: 'interface', kind: interfaceKind };
    violations.push(configViolation);
  }
  for (let i = 0; i < routes.length; i++) {
    const entry: RouteEntryView = routes[i];
    const source: string = routeEntrySource(entry);
    collect(violations, i, source, 'destination', requiredStringKind(entry.destination));
    collect(violations, i, source, 'prefixLength', requiredNumberKind(entry.prefixLength));
    collect(violations, i, source, 'gateway', requiredStringKind(entry.gateway));
    collect(violations, i, source, 'hasGateway', requiredBooleanKind(entry.hasGateway));
    collect(violations, i, source, 'isDefaultRoute', requiredBooleanKind(entry.isDefaultRoute));
    collect(violations, i, source, 'isExcludedRoute', optionalBooleanKind(entry.isExcludedRoute));
    collect(violations, i, source, 'family', optionalFamilyKind(entry.family));
  }
  const result: RouteCheckResult = { ok: violations.length === 0, violations: violations };
  return result;
}

// Render one field value EXACTLY as it sits at runtime — undefined/NaN must
// stay visible (that visibility is the point of the pre-create dump), never
// defaulted. Strings are sanitized for the pipe-delimited log format the
// same way NetBirdVpnExtensionAbility.clean() sanitizes free text. A value
// that lied about its declared type harder than the JSON domain (e.g. a
// nested object) still renders as its runtime typeof.
function fieldText(value: FieldValue): string {
  if (typeof value === 'string') {
    return value.replace(/\|/g, '/').replace(/\r/g, ' ').replace(/\n/g, ' ')
      .slice(0, 160);
  }
  if (value === undefined || value === null || typeof value === 'number' ||
    typeof value === 'boolean') {
    return String(value);
  }
  return typeof value;
}

function safeText(value: string): string {
  return value.replace(/\|/g, '/').replace(/\r/g, ' ').replace(/\n/g, ' ')
    .slice(0, 160);
}

// isExcludedRoute is SDK-optional (default false) and the platform builder
// omits the key when falsy — absent renders as the EFFECTIVE value 'false'.
function effectiveExcludedText(value: FieldValue): string {
  if (value === undefined || value === null) {
    return 'false';
  }
  return fieldText(value);
}

// The pre-create per-field truth dump (diagnosis §5 T0 / §6-4): one header
// line plus one line per entry with every SDK-relevant field verbatim.
// Returned as separate lines so the caller emits one hilog line each — a
// single joined line would risk hilog truncation toward the route bound.
// Field names and route values only; never tokens or credentials.
export function formatRouteTable(requestId: string, interfaceName: string,
  routes: RouteEntryView[]): string[] {
  let excluded: number = 0;
  for (let i = 0; i < routes.length; i++) {
    if (routes[i].isExcludedRoute) {
      excluded++;
    }
  }
  const lines: string[] = [];
  lines.push('VPN_ROUTE_TABLE|requestId=' + safeText(requestId) +
    '|count=' + String(routes.length) + '|excluded=' + String(excluded));
  for (let i = 0; i < routes.length; i++) {
    const entry: RouteEntryView = routes[i];
    // Same derivation rule as buildPlatformVpnConfig (NetBirdVpnConfig.ets):
    // destination family 2 iff the address text contains ':', else 1; the
    // gateway family is the constant 1 there. A non-text destination (a
    // violation) renders 'invalid'.
    const family: string = typeof entry.destination === 'string'
      ? (entry.destination.indexOf(':') >= 0 ? '2' : '1')
      : 'invalid';
    lines.push('VPN_ROUTE_ENTRY|requestId=' + safeText(requestId) +
      '|index=' + String(i) +
      '|source=' + routeEntrySource(entry) +
      '|interface=' + fieldText(interfaceName) +
      '|dest=' + fieldText(entry.destination) +
      '|prefix=' + fieldText(entry.prefixLength) +
      '|family=' + family +
      '|isDefaultRoute=' + fieldText(entry.isDefaultRoute) +
      '|isExcludedRoute=' + effectiveExcludedText(entry.isExcludedRoute) +
      '|hasGateway=' + fieldText(entry.hasGateway));
  }
  return lines;
}

// Fail-closed marker line, one per violating field — stable, greppable token
// VPN_ROUTE_ENTRY_INVALID carrying index/source/field/kind. Raw values live
// in the VPN_ROUTE_TABLE dump emitted alongside; nothing here can carry
// tokens or credentials.
export function formatRouteViolation(requestId: string,
  violation: RouteViolation): string {
  return 'VPN_ROUTE_ENTRY_INVALID|requestId=' + safeText(requestId) +
    '|index=' + String(violation.index) +
    '|source=' + violation.source +
    '|field=' + violation.field +
    '|kind=' + violation.kind;
}
