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
//   - stripDialPort: 'IPv4:port' dial shape -> bare IPv4 literal (regression
//     guard: an exclusion `address` must never carry a port — the create-401
//     of 2026-09-15 froze 'IP:port' as an ipv6 /128 family=2 entry);
//   - buildExcludedRouteEntries: resolved endpoints -> /32 (IPv4) and /128
//     (IPv6) isExcludedRoute entries, deduplicated;
//   - isExclusionFailClosed: the N2-H severity rule (any resolution failure
//     while relay is required = no VPN);
//   - mergeExcludedRouteEntries: append derived entries WITHOUT disturbing the
//     existing base entries (tunnel subnet / default template / host-LAN
//     exclusion keep their semantics verbatim);
//   - deriveDnsEndpointExclusions: the v3.1 dns freeze caliber's ONE branch
//     order (preregistration §2-C1 dns / R-4) — default network first, the
//     all-nets filtered branch ONLY when the default network cannot prove
//     physical attribution, and a recorded gap (never a silent pick) when
//     neither path can.
//   - parseStunUri / deriveStunEndpointExclusions: the N2-H I2 stun caliber
//     (design §13.13) — `netbird_config.stuns[].uri` as the tail of
//     `connector_network_config()`, parsed with the core's OWN `stun:` rules
//     (default port 3478), `turn:` refused by name (no turn path in this
//     build) and a recorded gap when nothing was announced.
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
// 'management' | 'signal' | 'relay' | 'stun' | 'dns' (the core's
// EndpointKind::name() tokens — the arm forwards them verbatim).
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

// Strict dotted-quad IPv4 literal check (digits only, 0-255 per octet) —
// shared by classifyAddress and stripDialPort (same shape as
// NetBirdConnector.isIPv4Literal; this module stays zero-import).
function isIPv4LiteralStr(value: string): boolean {
  const octets: string[] = value.split('.');
  if (octets.length !== 4) {
    return false;
  }
  for (let i = 0; i < octets.length; i++) {
    const octet: string = octets[i];
    if (octet.length === 0 || octet.length > 3) {
      return false;
    }
    for (let c = 0; c < octet.length; c++) {
      const code: number = octet.charCodeAt(c);
      if (code < 0x30 || code > 0x39) {
        return false;
      }
    }
    if (Number(octet) > 255) {
      return false;
    }
  }
  return true;
}

// Strip a trailing ':port' from the 'IPv4:port' DIAL shape (e.g.
// '203.0.113.7:443' as resolveManagementIp returns it for the connector
// dial). Exclusion entries freeze BARE address literals only: a port in the
// address field is not an IPv6 literal, yet the pre-fix 'contains ":" ->
// ipv6' rule rendered exactly that shape as a /128 family=2 entry the
// platform rejects at VpnConnection.create (the create-401 of 2026-09-15).
// Matches EXACTLY ONE ':' with a strict dotted-quad prefix and pure-digit
// suffix; genuine IPv6 literals (multiple ':') and bracket forms ('[v6]',
// '[v6]:port') are returned UNTOUCHED — never mis-stripped.
export function stripDialPort(address: string): string {
  const colon: number = address.indexOf(':');
  if (colon < 0 || address.indexOf(':', colon + 1) >= 0) {
    return address; // no colon, or more than one — never an IPv4:port shape
  }
  const port: string = address.slice(colon + 1);
  if (port.length === 0 || port.length > 5) {
    return address;
  }
  for (let c = 0; c < port.length; c++) {
    const code: number = port.charCodeAt(c);
    if (code < 0x30 || code > 0x39) {
      return address;
    }
  }
  if (!isIPv4LiteralStr(address.slice(0, colon))) {
    return address;
  }
  return address.slice(0, colon);
}

// Classify a resolution result as a routable address literal. IPv4 = strict
// dotted-quad (digits only, 0-255 per octet), also recognized in the
// 'IPv4:port' dial shape (family is IPv4 — pair with stripDialPort before
// rendering a destination; DEFENSIVE: this is the exact branch that
// mis-classified the port-carrying management address as ipv6 pre-fix);
// IPv6 = contains ':' and is NOT the IPv4:port shape (kept as text — the
// platform NetAddress takes the literal verbatim; multi-':' and bracketed
// forms always fall through here untouched); anything else is invalid and
// the caller treats it as a FAILED resolution.
export function classifyAddress(address: string): AddressClass {
  if (address.length === 0) {
    return 'invalid';
  }
  if (stripDialPort(address) !== address) {
    return 'ipv4'; // 'a.b.c.d:port' dial shape: IPv4 family, port stripped
  }
  if (address.indexOf(':') >= 0) {
    return 'ipv6';
  }
  if (!isIPv4LiteralStr(address)) {
    return 'invalid';
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
    // DEFENSIVE regression guard (pairs with the stripDialPort call at the
    // management derivation site in NetBirdVpnExtensionAbility): `address`
    // must be a BARE literal — never 'IP:port'. Stripping here too keeps
    // every caller safe: the old direct classification re-created the
    // ipv6 /128 family=2 entry the platform rejects at create-time.
    const address: string = stripDialPort(resolution.address);
    const addressClass: AddressClass = classifyAddress(address);
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
    if (seen.indexOf(address) >= 0) {
      duplicatesRemoved++;
      continue;
    }
    seen.push(address);
    routes.push({
      destination: address,
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

// ---------------------------------------------------------------------------
// N2-H v3.1 dns freeze: the UNIQUE platform-default-network branch order
// ---------------------------------------------------------------------------
//
// Preregistration §2-C1 (dns caliber, v3.1-corrected) / R-4: the dns freeze
// set's SHELL half is the PLATFORM DEFAULT (PHYSICAL) NETWORK's resolver
// list — not the tunnel's own DNS. The branch order is pinned BEFORE
// measurement and is the only one allowed:
//
//   step 1  take the default handle                      (getDefaultNetSync)
//   step 2  require handle != 0, its networkCap to carry NET_CAPABILITY_NOT_VPN
//           EXPLICITLY and its bearerTypes NOT to carry BEARER_VPN; when that
//           holds, take THIS handle's dnses[]            (step 2 held)
//   step 3  ONLY when step 2 did not hold, take the all-nets branch: every
//           handle of getAllNetsSync() is filtered by the SAME evidence
//           predicate, and the survivors' dnses[] are used
//   step 4  an error, an empty value or an unprovable physical attribution on
//           either path is a dns REQUIRED-class GAP (never a silent pick, and
//           never a re-choice of branch after seeing the data)
//
// The chosen branch and the verbatim grounds are the module's return value:
// the caller logs them (hilog) AND hands them to the core, which renders them
// into the evidence document's `notes[]` — a missing record is a dns gap for
// the judge. Nothing here is a "best effort": no branch is ever picked
// because it happens to yield more resolvers.
//
// The caller (NetBirdConnector) only COLLECTS facts with `@ohos.net.connection`
// and keeps step 3 from being called when step 2 held; the decision below
// re-applies the order to whatever it was handed, so a fact bundle that
// carries both is still resolved to the default branch.

/// `NET_CAPABILITY_NOT_VPN` (`@ohos.net.connection.d.ts` NetCap): the explicit
/// "this network is not a VPN" capability the dns evidence requires.
export const NET_CAPABILITY_NOT_VPN: number = 15;
/// `BEARER_VPN` (NetBearType): a network carrying it can never prove physical
/// (tunnel-outside) attribution.
export const BEARER_VPN: number = 4;
/// `NetAddress.family` values (1 = IPv4, 2 = IPv6; the platform default is 1).
export const NET_FAMILY_IPV4: number = 1;
export const NET_FAMILY_IPV6: number = 2;
/// A resolver's port when the platform reports none (`NetAddress.port` defaults
/// to 0 in the SDK): DNS is port 53 by convention, and the freeze needs a
/// `host:port` (v3.1 R-3 rejects a port-less value as a gap).
export const DNS_RESOLVER_PORT_DEFAULT: number = 53;

/// The three frozen `dns-branch` values (exactly one per derivation).
export const DNS_BRANCH_DEFAULT: string = 'default';
export const DNS_BRANCH_ALL_NETS_FILTERED: string = 'all-nets-filtered';
export const DNS_BRANCH_NONE: string = 'none';
/// The `dns-physical-evidence` value when no branch could be established.
export const DNS_PHYSICAL_EVIDENCE_NONE: string = 'none';

/// The recorded gap reasons (`dns-gap=<token>`), fixed order of precedence:
/// an API error first, then "no network could prove physical attribution",
/// then "the chosen path carried no resolvers", then "nothing usable as a
/// strict IPv4 resolver" (v3.1 R-3: the freeze set is strictly IPv4).
export const DNS_GAP_API_ERROR: string = 'api-error';
export const DNS_GAP_NO_PHYSICAL_NET: string = 'no-physical-net';
export const DNS_GAP_DNSES_EMPTY: string = 'dnses-empty';
export const DNS_GAP_DNSES_NOT_IPV4: string = 'dnses-not-ipv4';

// One resolver address as the platform reported it (`connection.NetAddress`;
// the fields are read defensively — the SDK documents family/port defaults).
export interface DnsPlatformNetAddress {
  address: string;
  family?: number;
  port?: number;
}

// Everything the branch order needs to know about ONE platform network. The
// caller fills it from `getNetCapabilitiesSync` / `getConnectionPropertiesSync`
// and never decides anything itself.
export interface DnsNetProbe {
  // `netId` of the handle; 0 means "no usable handle" (the SDK documents 0 as
  // "no default network"), which can never prove physical attribution.
  handleId: number;
  // false = the capabilities read THREW: no evidence, hence no attribution.
  capsOk: boolean;
  networkCap: number[];
  bearerTypes: number[];
  // false = the properties read THREW (or was never attempted because the
  // evidence predicate failed): `dnses` must then not be trusted.
  propertiesOk: boolean;
  dnses: DnsPlatformNetAddress[];
}

// The fact bundle the caller hands in. `allNets` stays EMPTY unless step 2
// did not hold (the caller does not even call `getAllNetsSync()` otherwise);
// `allNetsRead=false` means that call threw.
export interface DnsDerivationInput {
  defaultNet: DnsNetProbe;
  allNets: DnsNetProbe[];
  allNetsRead: boolean;
  // '' = no platform API call threw. Otherwise the first error token
  // (`api-error-code-<n>` / `api-error`), which makes the derivation a gap.
  apiError: string;
}

export interface DnsDerivationResult {
  // The resolver endpoints to freeze AND exclude (kind 'dns'), deduplicated
  // by address, in platform order.
  endpoints: ExclusionEndpoint[];
  // The verbatim notes: `dns-branch=…`, `dns-physical-evidence=…`, and
  // `dns-gap=…` when the derivation recorded a gap.
  notes: string[];
  branch: string;
  physicalEvidence: string;
  gap: boolean;
  gapReason: string;
}

// A probe with no evidence at all (the shape every early-exit path starts
// from).
export function emptyDnsNetProbe(): DnsNetProbe {
  return { handleId: 0, capsOk: false, networkCap: [], bearerTypes: [], propertiesOk: false,
    dnses: [] };
}

// PURE: the v3.1 physical-attribution predicate. It requires BOTH halves
// explicitly: the NOT_VPN capability must be PRESENT (absence is not proof)
// and no VPN bearer type may be present (a network that is simultaneously a
// VPN can never be called physical).
export function isPhysicalNetEvidence(probe: DnsNetProbe): boolean {
  if (probe.handleId === 0 || !probe.capsOk) {
    return false;
  }
  if (probe.networkCap.indexOf(NET_CAPABILITY_NOT_VPN) < 0) {
    return false;
  }
  if (probe.bearerTypes.indexOf(BEARER_VPN) >= 0) {
    return false;
  }
  return true;
}

// PURE: the verbatim grounds of one branch (R-4's `dns-physical-evidence`).
function dnsPhysicalEvidence(handles: number[]): string {
  const ids: string[] = [];
  for (let i = 0; i < handles.length; i++) {
    ids.push(String(handles[i]));
  }
  const label: string = handles.length === 1 ? 'handle=' : 'handles=';
  return label + ids.join(',') + ';networkCap-has-NOT_VPN=' + String(NET_CAPABILITY_NOT_VPN) +
    ';bearerTypes-no-BEARER_VPN=' + String(BEARER_VPN);
}

// PURE: one network's dnses -> strict IPv4 resolver endpoints. An entry that
// is not a usable IPv4 resolver (IPv6 family per v3.1 R-3, a non-literal
// address, an empty address) is REPORTED (`sawNonIpv4`) instead of skipped
// silently; duplicates collapse.
function collectDnsResolvers(dnses: DnsPlatformNetAddress[],
  out: ExclusionEndpoint[]): boolean {
  let sawNonIpv4: boolean = false;
  for (let i = 0; i < dnses.length; i++) {
    const entry: DnsPlatformNetAddress = dnses[i];
    const rawAddress: string = entry.address !== undefined && entry.address !== null
      ? entry.address
      : '';
    const family: number = entry.family !== undefined && entry.family !== null
      ? entry.family
      : NET_FAMILY_IPV4;
    const bare: string = stripDialPort(rawAddress);
    if (family !== NET_FAMILY_IPV4 || classifyAddress(bare) !== 'ipv4') {
      sawNonIpv4 = true;
      continue;
    }
    let duplicate: boolean = false;
    for (let j = 0; j < out.length; j++) {
      if (out[j].host === bare) {
        duplicate = true;
        break;
      }
    }
    if (duplicate) {
      continue;
    }
    const reportedPort: number = entry.port !== undefined && entry.port !== null ? entry.port : 0;
    const port: number = reportedPort > 0 && reportedPort <= 65535
      ? reportedPort
      : DNS_RESOLVER_PORT_DEFAULT;
    out.push({ kind: 'dns', host: bare, port: port });
  }
  return sawNonIpv4;
}

// PURE: the UNIQUE branch order (preregistration §2-C1 dns / R-4). See the
// section header: step 2 decides whether step 3 may even be looked at, and no
// path is ever chosen on the basis of what it returned.
export function deriveDnsEndpointExclusions(input: DnsDerivationInput): DnsDerivationResult {
  const endpoints: ExclusionEndpoint[] = [];
  const notes: string[] = [];
  let branch: string = DNS_BRANCH_NONE;
  let physicalEvidence: string = DNS_PHYSICAL_EVIDENCE_NONE;
  let sawNonIpv4: boolean = false;
  let dnsesEmpty: boolean = true;
  let propertiesUnread: boolean = false;

  if (isPhysicalNetEvidence(input.defaultNet)) {
    // step 2 held: the default network IS the physical network. This is the
    // ONLY branch whose resolvers are read, whatever `allNets` carries.
    branch = DNS_BRANCH_DEFAULT;
    physicalEvidence = dnsPhysicalEvidence([input.defaultNet.handleId]);
    if (!input.defaultNet.propertiesOk) {
      propertiesUnread = true;
    } else {
      dnsesEmpty = input.defaultNet.dnses.length === 0;
      sawNonIpv4 = collectDnsResolvers(input.defaultNet.dnses, endpoints);
    }
  } else {
    // step 3 — reachable ONLY because step 2 did not hold
    const qualifying: DnsNetProbe[] = [];
    for (let i = 0; i < input.allNets.length; i++) {
      if (isPhysicalNetEvidence(input.allNets[i])) {
        qualifying.push(input.allNets[i]);
      }
    }
    if (input.allNetsRead && qualifying.length > 0) {
      branch = DNS_BRANCH_ALL_NETS_FILTERED;
      const handles: number[] = [];
      for (let i = 0; i < qualifying.length; i++) {
        handles.push(qualifying[i].handleId);
      }
      physicalEvidence = dnsPhysicalEvidence(handles);
      let entries: number = 0;
      for (let i = 0; i < qualifying.length; i++) {
        if (!qualifying[i].propertiesOk) {
          propertiesUnread = true;
          continue;
        }
        entries += qualifying[i].dnses.length;
        if (collectDnsResolvers(qualifying[i].dnses, endpoints)) {
          sawNonIpv4 = true;
        }
      }
      dnsesEmpty = entries === 0;
    } else {
      // neither branch produced a network that can prove physical
      // attribution: no branch token, no ground, and a recorded gap
      branch = DNS_BRANCH_NONE;
      physicalEvidence = DNS_PHYSICAL_EVIDENCE_NONE;
    }
  }

  // step 4: the gap decision, in the fixed precedence order
  let gapReason: string = '';
  if (input.apiError.length > 0) {
    gapReason = input.apiError;
  } else if (branch === DNS_BRANCH_NONE) {
    gapReason = DNS_GAP_NO_PHYSICAL_NET;
  } else if (propertiesUnread) {
    gapReason = DNS_GAP_API_ERROR;
  } else if (endpoints.length === 0) {
    gapReason = dnsesEmpty ? DNS_GAP_DNSES_EMPTY : DNS_GAP_DNSES_NOT_IPV4;
  } else if (sawNonIpv4) {
    // usable IPv4 resolvers exist, but the platform also reported at least one
    // entry that cannot enter a strictly-IPv4 freeze set (v3.1 R-3): recorded
    // as a gap rather than dropped silently
    gapReason = DNS_GAP_DNSES_NOT_IPV4;
  }

  notes.push('dns-branch=' + branch);
  notes.push('dns-physical-evidence=' + physicalEvidence);
  if (gapReason.length > 0) {
    notes.push('dns-gap=' + gapReason);
  }
  return { endpoints: endpoints, notes: notes, branch: branch,
    physicalEvidence: physicalEvidence, gap: gapReason.length > 0,
    gapReason: gapReason };
}

// ---------------------------------------------------------------------------
// N2-H I2 stun freeze: `netbird_config.stuns[].uri` -> /32 excluded endpoints
// ---------------------------------------------------------------------------
//
// The stun class is one of the seven the governance freeze requires
// (docs/native-nx-governance.md 追加登记; preregistration §2-C1 lists stun as
// a REQUIRED kind), and until this increment the shell had no channel to it:
// the core gathers against `netbird_config.stuns[]` internally, and
// `connector_network_config()` did not render it. Core now tail-appends
// `stuns:[{uri}]` to that read-only snapshot (design §13.13) and this section
// turns those URIs into the exclusion set's stun endpoints.
//
// PARSE MIRROR: `parseStunUri` mirrors the core's own `parse_stun_uri`
// (client/core/src/ice.rs:100-143) for the `stun:` branch — first-`:` scheme
// split, case-insensitive `stun` scheme, LAST-`:` host/port split, default
// port 3478, `u16` port (leading `+` accepted, 0 and >65535 refused), empty
// host refused, bracketed IPv6 refused — so the shell freezes exactly the
// `host:port` core gathers against. `turn:` is refused with the NAMED reason
// [`TURN_REJECTED_REASON`] and NO turn route is ever generated: this build has
// no turn path at all (`set_stuns` rejects `turn:`; the collector records the
// absent kind with the same named reason). One ADDITIONAL shell-side
// restriction (recorded, never silent): a host that still carries a `:` after
// the split (`stun:a:b:3478` — core would accept host `a:b`) is refused as
// [`STUN_REJECT_UNUSABLE_HOST`] instead of being rendered as a bogus /128
// route entry — that is the create-401 failure class this module already
// guards elsewhere, and core's own gather could not resolve such a host
// either.
//
// A missing/empty/type-anomalous `stuns` section, and every URI this module
// refuses, are reported as a GAP token — never an invented endpoint, never a
// silent drop. The gap is diagnostic (hilog marker) in this increment: the
// fail-closed widening of the five required kinds and the arm-opts contract
// (§5.1) are separate increments.

/// The `stun:` default port (pion's default, mirrored from `ice.rs:137`).
export const STUN_DEFAULT_PORT: number = 3478;

/// The named reason a `turn:` URI is refused: this build has no turn path
/// (core: `n2h_collector::TURN_REJECTED_REASON`, `peer_conn.rs` `set_stuns`
/// rejects `turn:`). No turn endpoint is ever frozen or excluded.
export const TURN_REJECTED_REASON: string = 'turn-rejected:no-turn-path-in-this-build';

/// Per-URI refusal tokens (stable, greppable; mirror the core parser's
/// distinct rejection branches).
export const STUN_REJECT_NO_SCHEME: string = 'no-stun-scheme';
export const STUN_REJECT_BAD_PORT: string = 'bad-port';
export const STUN_REJECT_PORT_ZERO: string = 'port-zero';
export const STUN_REJECT_IPV6: string = 'ipv6-rejected';
export const STUN_REJECT_NO_HOST: string = 'no-host';
export const STUN_REJECT_UNUSABLE_HOST: string = 'unusable-host';
export const STUN_REJECT_MALFORMED_ENTRY: string = 'malformed-entry';

/// Gap tokens: nothing announced at all, versus announced-but-unusable. Both
/// are honest gaps (`stuns` missing on the wire is NOT "no stun endpoint
/// exists" — it is "the shell was not told", which the core's own freeze then
/// covers or records as its own reasoned absence).
export const STUN_GAP_NO_STUNS: string = 'no-stuns';
export const STUN_GAP_UNUSABLE_URIS: string = 'unusable-uris';

// host:port parsed out of a `stun:` URI.
export interface StunEndpoint {
  host: string;
  port: number;
}

// The parse outcome: `endpoint` is null exactly when `reason` is non-empty.
export interface StunParseResult {
  endpoint: StunEndpoint | null;
  reason: string;
}

// One `connector_network_config()` tail element (`stuns:[{uri}]`).
export interface StunUriEntry {
  uri: string;
}

export interface StunDerivationResult {
  // The stun endpoints to freeze AND exclude (kind 'stun'), deduplicated by
  // host:port, in announcement order.
  endpoints: ExclusionEndpoint[];
  // One refusal token per refused element, in announcement order (never a
  // silent drop) — `TURN_REJECTED_REASON` for a `turn:` URI.
  rejected: string[];
  turnRejected: number;
  // How many elements the announced list carried (0 when the section is
  // missing or not an array-shaped list) — reported by the caller instead of
  // it reaching into the raw input.
  announced: number;
  gap: boolean;
  gapReason: string;
}

// PURE: one refusal outcome (`endpoint` null ⇔ `reason` non-empty).
function stunReject(reason: string): StunParseResult {
  return { endpoint: null, reason: reason };
}

// PURE: the `stun:` branch mirror of the core parser (see the section header).
export function parseStunUri(uri: string): StunParseResult {
  const text: string = uri.trim();
  const sep: number = text.indexOf(':');
  if (sep < 0) {
    return stunReject(STUN_REJECT_NO_SCHEME);
  }
  const scheme: string = text.slice(0, sep).toLowerCase();
  if (scheme !== 'stun') {
    // `turn:` carries its NAMED reason; any other scheme is refused the same
    // way core's `lacks a stun: scheme` branch does
    return stunReject(scheme === 'turn' ? TURN_REJECTED_REASON : STUN_REJECT_NO_SCHEME);
  }
  const rest: string = text.slice(sep + 1);
  let host: string = rest;
  let port: number = STUN_DEFAULT_PORT;
  const last: number = rest.lastIndexOf(':');
  if (last >= 0) {
    const hostPart: string = rest.slice(0, last);
    if (hostPart.slice(0, 1) === '[') {
      return stunReject(STUN_REJECT_IPV6); // bracketed IPv6 literal (core: udp4-only)
    }
    const portPart: string = rest.slice(last + 1);
    // core parses with `str::parse::<u16>()`: digits with an OPTIONAL leading
    // '+' — mirrored exactly, because a stricter shell parse would omit an
    // endpoint the core actually gathers against
    let digits: string = portPart;
    if (digits.slice(0, 1) === '+') {
      digits = digits.slice(1);
    }
    if (digits.length === 0 || digits.length > 5) {
      return stunReject(STUN_REJECT_BAD_PORT);
    }
    for (let i = 0; i < digits.length; i++) {
      const code: number = digits.charCodeAt(i);
      if (code < 0x30 || code > 0x39) {
        return stunReject(STUN_REJECT_BAD_PORT);
      }
    }
    port = Number(digits);
    if (port > 65535) {
      return stunReject(STUN_REJECT_BAD_PORT);
    }
    if (port === 0) {
      return stunReject(STUN_REJECT_PORT_ZERO);
    }
    host = hostPart;
  }
  if (host.length === 0) {
    return stunReject(STUN_REJECT_NO_HOST);
  }
  if (host.indexOf(':') >= 0) {
    // shell-side extra guard (see the section header): a colon-bearing host is
    // a route entry the platform rejects, never a real stun endpoint
    return stunReject(STUN_REJECT_UNUSABLE_HOST);
  }
  return { endpoint: { host: host, port: port }, reason: '' };
}

// PURE: the announced `stuns[]` entries -> the stun class of the exclusion set
// (kind 'stun'). Missing / empty / anomalously-typed input and every refused
// URI become a recorded gap token; nothing is ever invented or silently
// dropped.
export function deriveStunEndpointExclusions(stuns: StunUriEntry[] | null | undefined):
  StunDerivationResult {
  const endpoints: ExclusionEndpoint[] = [];
  const rejected: string[] = [];
  let turnRejected: number = 0;
  const entries: StunUriEntry[] | null | undefined = stuns;
  if (entries === null || entries === undefined || entries.length === 0) {
    return { endpoints: endpoints, rejected: rejected, turnRejected: 0, announced: 0,
      gap: true, gapReason: STUN_GAP_NO_STUNS };
  }
  // the announced count is read HERE (never by the caller): a wrong-typed
  // section has no length, and that must not leak an `undefined` into a log line
  const announced: number = typeof entries.length === 'number' ? entries.length : 0;
  for (let i = 0; i < entries.length; i++) {
    const entry: StunUriEntry = entries[i];
    const uri: string = entry !== null && entry !== undefined && typeof entry.uri === 'string'
      ? entry.uri
      : '';
    if (uri.length === 0) {
      rejected.push(STUN_REJECT_MALFORMED_ENTRY);
      continue;
    }
    const parsed: StunParseResult = parseStunUri(uri);
    const endpoint: StunEndpoint | null = parsed.endpoint;
    if (endpoint === null) {
      rejected.push(parsed.reason);
      if (parsed.reason === TURN_REJECTED_REASON) {
        turnRejected++;
      }
      continue;
    }
    let duplicate: boolean = false;
    for (let j = 0; j < endpoints.length; j++) {
      if (endpoints[j].host === endpoint.host && endpoints[j].port === endpoint.port) {
        duplicate = true;
        break;
      }
    }
    if (duplicate) {
      continue;
    }
    endpoints.push({ kind: 'stun', host: endpoint.host, port: endpoint.port });
  }
  let gapReason: string = '';
  if (rejected.length > 0) {
    gapReason = STUN_GAP_UNUSABLE_URIS;
  } else if (endpoints.length === 0) {
    // no usable entry and nothing to name as refused (e.g. a section of the
    // wrong type): still an honest gap
    gapReason = STUN_GAP_NO_STUNS;
  }
  return { endpoints: endpoints, rejected: rejected, turnRejected: turnRejected,
    announced: announced, gap: gapReason.length > 0, gapReason: gapReason };
}
