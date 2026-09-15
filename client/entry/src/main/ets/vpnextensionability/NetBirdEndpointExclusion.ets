// N2-H endpoint-derived route-exclusion set — PURE module (zero imports).
//
// Governance basis (docs/native-nx-governance.md 追加登记 2026-09-14): this
// platform cannot use the per-socket protect path (ohos.permission.MANAGE_VPN
// is gated and not appliable), so the approved substitute criterion N2-H
// applies: the outer endpoints (management, signal, relay, ICE/STUN, WG peer,
// DNS) must be FROZEN and EXCLUDED via host routes before VpnConnection.create
// (routes are fixed at create time); resolution failure / endpoint change is
// fail-closed. docs/n13-relay-increment-plan-20260914.md §3.4/§5 add the hard
// precondition: enabling relay (`relay_enabled`) REQUIRES the relay TCP
// endpoint to be part of that frozen exclusion set first.
//
// This module owns the testable core of that derivation:
//   - parseRelayUrl: rel:// / rels:// authority extraction, a structural
//     mirror of the core's own RelayUrl::parse (client/core/src/relay_client.rs)
//     so the shell freezes exactly the host:port the core will dial;
//   - buildExcludedRouteEntries: resolved endpoints -> /32 (IPv4) and /128
//     (IPv6) isExcludedRoute entries, deduplicated;
//   - isExclusionFailClosed: the N2-H severity rule (any resolution failure
//     while relay is required = no VPN);
//   - mergeExcludedRouteEntries: append derived entries WITHOUT disturbing the
//     existing base entries (tunnel subnet / default template / host-LAN
//     exclusion keep their semantics verbatim).
//
// Log discipline: callers log hostnames/ports/counts only — never tokens, and
// this module holds no credential-shaped data at all.
//
// Host self-check: client/entry/selfcheck runs the unit suite against a
// byte-identical mirror of this file (enforced at runtime) under node.

// Route-entry shape mirrored from VpnRouteConfig (NetBirdVpnConfig.ets —
// structurally identical; the extension maps entries onto VpnRouteConfig).
export interface ExcludedRouteEntry {
  destination: string;
  prefixLength: number;
  gateway: string;
  hasGateway: boolean;
  isDefaultRoute: boolean;
  isExcludedRoute: boolean;
}

// One outer endpoint that must be frozen + excluded. `kind` is a stable token:
// 'management' | 'signal' | 'relay'.
export interface ExclusionEndpoint {
  kind: string;
  host: string;
  port: number;
}

// Per-endpoint resolution outcome fed into the pure builder. `address` is the
// resolved IP ('' when the resolution failed). The resolution itself is an
// injectable dependency upstream (shell DNS / literals / test doubles).
export interface EndpointResolution {
  kind: string;
  host: string;
  ok: boolean;
  address: string;
}

export interface ExclusionBuildResult {
  routes: ExcludedRouteEntry[];
  resolvedCount: number;
  duplicatesRemoved: number;
  failed: EndpointResolution[];
}

// host:port parsed out of a managed relay URL.
export interface RelayEndpoint {
  host: string;
  port: number;
}

export type AddressClass = 'ipv4' | 'ipv6' | 'invalid';

// Classify a resolution result as a routable address literal. IPv4 = strict
// dotted-quad (digits only, 0-255 per octet); IPv6 = contains ':' (kept as
// text — the platform NetAddress takes the literal verbatim); anything else is
// invalid and the caller treats it as a FAILED resolution.
export function classifyAddress(address: string): AddressClass {
  if (address.length === 0) {
    return 'invalid';
  }
  if (address.indexOf(':') >= 0) {
    return 'ipv6';
  }
  const octets: string[] = address.split('.');
  if (octets.length !== 4) {
    return 'invalid';
  }
  for (let i = 0; i < octets.length; i++) {
    const octet: string = octets[i];
    if (octet.length === 0 || octet.length > 3) {
      return 'invalid';
    }
    for (let c = 0; c < octet.length; c++) {
      const code: number = octet.charCodeAt(c);
      if (code < 0x30 || code > 0x39) {
        return 'invalid';
      }
    }
    const value: number = Number(octet);
    if (value > 255) {
      return 'invalid';
    }
  }
  return 'ipv4';
}

// Parse a managed relay URL into host + port — structural mirror of
// RelayUrl::parse (client/core/src/relay_client.rs): rel:// => default 80,
// rels:// => default 443, bracketed IPv6 literals, ASCII authority only;
// path/query/fragment/userinfo/control chars and any other scheme are
// REFUSED (null) exactly like the core's typed failure — never guessed.
export function parseRelayUrl(url: string): RelayEndpoint | null {
  const sep: number = url.indexOf('://');
  if (sep <= 0) {
    return null;
  }
  const scheme: string = url.slice(0, sep).toLowerCase();
  if (scheme !== 'rel' && scheme !== 'rels') {
    return null;
  }
  const rest: string = url.slice(sep + 3);
  if (rest.length === 0) {
    return null;
  }
  for (let i = 0; i < rest.length; i++) {
    const code: number = rest.charCodeAt(i);
    if (code <= 0x20 || code >= 0x7f) {
      return null;
    }
  }
  if (rest.indexOf('/') >= 0 || rest.indexOf('?') >= 0 || rest.indexOf('#') >= 0 ||
    rest.indexOf('@') >= 0) {
    return null;
  }
  let host: string = '';
  let portStr: string | null = null;
  if (rest.slice(0, 1) === '[') {
    // Bracketed IPv6 literal: [h:h:h][:port]
    const close: number = rest.indexOf(']');
    if (close < 0) {
      return null;
    }
    host = rest.slice(1, close);
    const after: string = rest.slice(close + 1);
    if (after.slice(0, 1) === ':') {
      portStr = after.slice(1);
    } else if (after.length !== 0) {
      return null;
    }
  } else {
    const colon: number = rest.lastIndexOf(':');
    if (colon >= 0) {
      host = rest.slice(0, colon);
      portStr = rest.slice(colon + 1);
    } else {
      host = rest;
    }
  }
  if (host.length === 0) {
    return null;
  }
  let port: number;
  if (portStr === null) {
    port = scheme === 'rels' ? 443 : 80;
  } else {
    if (portStr.length === 0 || portStr.length > 5) {
      return null;
    }
    for (let c = 0; c < portStr.length; c++) {
      const code: number = portStr.charCodeAt(c);
      if (code < 0x30 || code > 0x39) {
        return null;
      }
    }
    port = Number(portStr);
    if (port <= 0 || port > 65535) {
      return null;
    }
  }
  return { host: host, port: port };
}

// PURE: endpoint resolutions -> deduplicated /32|/128 excluded-route entries.
// Gateway convention matches the existing base exclusions (tunnel address,
// hasGateway: true). Unresolved or non-literal addresses land in `failed`
// (never silently dropped); duplicates collapse with duplicatesRemoved.
export function buildExcludedRouteEntries(resolutions: EndpointResolution[],
  gateway: string): ExclusionBuildResult {
  const routes: ExcludedRouteEntry[] = [];
  const seen: string[] = [];
  const failed: EndpointResolution[] = [];
  let resolvedCount: number = 0;
  let duplicatesRemoved: number = 0;
  for (let i = 0; i < resolutions.length; i++) {
    const resolution: EndpointResolution = resolutions[i];
    if (!resolution.ok) {
      failed.push(resolution);
      continue;
    }
    const addressClass: AddressClass = classifyAddress(resolution.address);
    let prefix: number = -1;
    if (addressClass === 'ipv4') {
      prefix = 32;
    } else if (addressClass === 'ipv6') {
      prefix = 128;
    }
    if (prefix < 0) {
      failed.push(resolution);
      continue;
    }
    if (seen.indexOf(resolution.address) >= 0) {
      duplicatesRemoved++;
      continue;
    }
    seen.push(resolution.address);
    routes.push({
      destination: resolution.address,
      prefixLength: prefix,
      gateway: gateway,
      hasGateway: true,
      isDefaultRoute: false,
      isExcludedRoute: true
    });
    resolvedCount++;
  }
  return { routes: routes, resolvedCount: resolvedCount, duplicatesRemoved: duplicatesRemoved,
    failed: failed };
}

// PURE: the N2-H severity rule. With relay in the required freeze set, ANY
// failed resolution aborts the start (fail-closed — a missing exclusion on a
// relay-enabled deployment means an outer endpoint could enter our own
// tunnel). Without relay, failures are tolerable and the caller must log the
// skip reason instead.
export function isExclusionFailClosed(result: ExclusionBuildResult, relayRequired: boolean):
  boolean {
  return relayRequired && result.failed.length > 0;
}

// PURE: append derived exclusion entries onto an existing route list. Every
// existing entry is kept VERBATIM (the host-LAN exclusion keeps its exact
// semantics); a derived entry is appended once and skipped when an existing
// entry already covers the same destination+prefix.
export function mergeExcludedRouteEntries(existing: ExcludedRouteEntry[],
  derived: ExcludedRouteEntry[]): ExcludedRouteEntry[] {
  const merged: ExcludedRouteEntry[] = existing.slice();
  for (let i = 0; i < derived.length; i++) {
    const candidate: ExcludedRouteEntry = derived[i];
    let duplicate: boolean = false;
    for (let j = 0; j < merged.length; j++) {
      if (merged[j].destination === candidate.destination &&
        merged[j].prefixLength === candidate.prefixLength) {
        duplicate = true;
        break;
      }
    }
    if (!duplicate) {
      merged.push(candidate);
    }
  }
  return merged;
}
