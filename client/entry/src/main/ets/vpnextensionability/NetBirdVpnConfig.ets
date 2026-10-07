import vpnExtension from '@ohos.net.vpnExtension';
import { CoreNetworkConfig, CoreRouteEntry } from './NetBirdConnector';
import { ExcludedRouteEntry, mergeExcludedRouteEntries } from './NetBirdEndpointExclusion';

// Client-side VPN tunnel configuration model + platform VpnConfig mapper.
//
// Source of truth (do NOT invent other API shapes — project rule):
// - Default-route expression follows the on-device-verified MR4 row of
//   spikes/n1b-disc-phys-hap (commit e8e2cf9): RouteInfo.isDefaultRoute
//   (@ohos.net.connection.d.ts, @since 8) on destination 0.0.0.0/0 and
//   RouteInfo.isExcludedRoute (@since 20) on the host-LAN exclusion.
// - Route `interface` follows fa819bf: the platform VPN interface name
//   'vpn-tun' (wlan0 was proven to bypass the tunnel).
// - dnsAddresses / mtu / addresses are SDK VpnConfig fields (@since 11, mtu
//   range [576,1500]). dnsAddresses is SDK-declared but has NO on-device
//   verification in any spike — TODO(未验证): physical N2b proof pending.
//
// Config source is a local fixed constant; loadVpnTunnelConfig() is the only
// read entry point and the single replacement site once the control plane
// (N3) is legally cleared to deliver per-client configuration. No control
// plane protocol is implemented here.

export interface VpnRouteConfig {
  destination: string;
  prefixLength: number;
  gateway: string;
  hasGateway: boolean;
  isDefaultRoute: boolean;
  isExcludedRoute: boolean;
  // Provenance tag for the pre-create route truth log / fail-closed
  // validation (H1 diagnosis, docs/vpn-create-401-diagnosis-20260915.md
  // §3.2/§5): 'managed' is written ONLY by applyNetworkConfig for
  // core-snapshot-derived entries. Base-template and derived-exclusion
  // entries stay untagged (classified by shape in
  // NetBirdRouteValidation.routeEntrySource). Never mapped into the platform
  // VpnConfig — buildPlatformVpnConfig reads fixed fields only.
  source?: string;
}

export interface VpnTunnelConfig {
  // Tunnel interface address, e.g. '10.99.0.1' with prefixLength 32
  // (MR4-verified base shape).
  tunnelAddress: string;
  tunnelPrefixLength: number;
  // Platform VPN interface name used in every RouteInfo ('vpn-tun', fa819bf).
  interfaceName: string;
  // Ordered route list; must contain the default route (isDefaultRoute).
  routes: VpnRouteConfig[];
  // DNS servers declared to the platform (VpnConfig.dnsAddresses). An EMPTY
  // list means "declare none" (the mapper then omits the field): the N2
  // evidence generation uses that (NetBirdN2Evidence decision (a)).
  // TODO(未验证): placeholder until the control plane supplies real values;
  // on-device DNS behavior never verified in a spike.
  dnsServers: string[];
  // MTU in bytes, platform-accepted range [576,1500]; MR4 verified 1400.
  mtu: number;
  // WireGuard peer endpoint 'host:port'. Shell-side data only: it is logged
  // and reserved for the native data plane; the platform VpnConfig has no
  // endpoint field.
  // TODO(未验证): placeholder value; real endpoint arrives via control plane.
  peerEndpoint: string;
}

// MR4-verified default-route shape (e8e2cf9), expressed as configuration:
// route 1: tunnel subnet 10.99.0.0/24 (plain route);
// route 2: IPv4 default 0.0.0.0/0 with isDefaultRoute: true.
//
// The base host-LAN exclusion (previously the static route 3
// 192.168.50.0/24) is NOT part of this template anymore: it is DERIVED per
// create() from the physical default network's local IPv4 prefix
// (NetBirdEndpointExclusion.deriveLanExclusion +
// NetBirdConnector.physicalDefaultLanProbe; scenario-A roaming fix,
// LAN-ROUTE-ROAMING-ANALYSIS.md §4 step 1). Home Wi-Fi (local
// 192.168.50.x/24) reproduces exactly the retired static entry; cellular or
// an unusable probe yields NO LAN exclusion, so the managed route to the
// home subnet is not masked while away. The extension grafts it at assembly
// time (initial create AND every recreate leg) ahead of the frozen endpoint
// set via withEndpointExclusions — see NetBirdVpnExtensionAbility.
//
// N2-H (docs/native-nx-governance.md 追加登记; docs/n13-relay-increment-plan
// §3.4/§5): route 2 alone is NOT a sufficient outer-endpoint exclusion set.
// The remaining outer endpoints (management, signal, relay — all DDNS-able)
// are FROZEN and EXCLUDED per start at runtime: deriveEndpointExclusions
// (NetBirdVpnExtensionAbility) resolves the real endpoints shell-side and
// withEndpointExclusions below grafts the derived /32|/128 entries onto this
// base BEFORE create() (routes are fixed at create time). The N2 evidence
// mode strips every isExcludedRoute entry from this base
// (withoutN2ExcludedRoutes) and also goes without the derived LAN piece.
//
// N3-7: route 2 is a TEMPLATE entry only. The default-route safety gate
// (`gateDefaultRoutes` below + the core-side snapshot gate) decides whether
// it may actually be installed; installing 0.0.0.0/0 while the data plane
// cannot carry traffic is a traffic black hole, so the default is HOLD.
const DEFAULT_VPN_TUNNEL_CONFIG: VpnTunnelConfig = {
  tunnelAddress: '10.99.0.1',
  tunnelPrefixLength: 32,
  interfaceName: 'vpn-tun',
  routes: [
    {
      destination: '10.99.0.0',
      prefixLength: 24,
      gateway: '10.99.0.1',
      hasGateway: true,
      isDefaultRoute: false,
      isExcludedRoute: false
    },
    {
      destination: '0.0.0.0',
      prefixLength: 0,
      gateway: '10.99.0.1',
      hasGateway: true,
      isDefaultRoute: true,
      isExcludedRoute: false
    }
  ],
  dnsServers: ['8.8.8.8'],
  mtu: 1400,
  peerEndpoint: '192.0.2.10:51820'
};

// Single config read entry point. The control plane (N3, not implemented —
// legally gated) will replace this body; every consumer must go through it.
export function loadVpnTunnelConfig(): VpnTunnelConfig {
  return DEFAULT_VPN_TUNNEL_CONFIG;
}

// N3-6: overlay a core.connector_network_config() snapshot onto the base
// local config. Applied ONCE, at start, before connection.create() — the
// platform VpnConfig (addresses/routes/dnsAddresses) is FIXED at create()
// time and VpnConnection has no route/DNS update API, so later map changes
// CANNOT be applied at runtime (the extension logs them as explicitly
// NOT applied — see NetBirdVpnExtensionAbility watch loop). Empty/absent
// snapshot fields fall back to the base deployment values. The base config's
// isExcludedRoute entries are kept (the dynamically derived host-LAN entry
// the extension grafted at assembly time, plus the frozen outer-endpoint
// /32|/128 set) — the outer path (management/WG endpoint traffic) must stay
// out of our own tunnel regardless of what the map routes. The ROUTE list is
// the one field with a narrower fallback: an explicit snapshot whose effective
// route list is empty does NOT restore the base 0.0.0.0/0 template entry (see
// the route-disable safety note in the body) — it keeps only the base
// exclusions and non-default entries.
export function applyNetworkConfig(base: VpnTunnelConfig, net: CoreNetworkConfig): VpnTunnelConfig {
  const address: string =
    net.address !== undefined && net.address !== null && net.address.length > 0
      ? net.address
      : base.tunnelAddress;
  const prefixLength: number =
    net.address_prefix_len !== undefined && net.address_prefix_len !== null
      ? net.address_prefix_len
      : base.tunnelPrefixLength;

  const routes: VpnRouteConfig[] = [];
  const managed: CoreRouteEntry[] = net.routes !== undefined && net.routes !== null
    ? net.routes
    : [];
  for (let i = 0; i < managed.length; i++) {
    const parts: string[] = managed[i].network.split('/');
    if (parts.length !== 2) {
      // core renders canonical 'a.b.c.d/prefix'; anything else is a
      // contract violation — skip it loudly rather than guess
      continue;
    }
    routes.push({
      destination: parts[0],
      prefixLength: Number(parts[1]),
      gateway: address,
      hasGateway: true,
      isDefaultRoute: managed[i].is_default,
      isExcludedRoute: false,
      // Provenance only (route truth log / H1 validation): marks the entry
      // as core-snapshot-derived. Values are untouched — the platform shape
      // produced by buildPlatformVpnConfig is unchanged.
      source: 'managed'
    });
  }
  for (let i = 0; i < base.routes.length; i++) {
    if (base.routes[i].isExcludedRoute) {
      routes.push(base.routes[i]);
    }
  }
  // Route-disable safety (2026-10-07): an EXPLICIT network config (net !==
  // null — the caller passes `base` verbatim when there is no snapshot at all)
  // whose effective route list came out EMPTY — every managed route disabled
  // or absent AND no base exclusion carried — must NOT be answered with the
  // base template's 0.0.0.0/0. `routes.length > 0 ? routes : base.routes`
  // restored it, so "no routes in the map" (the route-disable feature's whole
  // point) still submitted the default route: all traffic captured by the
  // tunnel, the exact opposite of the user's setting (and a black hole when
  // the data plane cannot carry it). The fallback keeps the base exclusions
  // (already carried above) and the base NON-default entries (the
  // tunnel-subnet template), and drops every isDefaultRoute entry. The
  // no-snapshot (net === null) legacy path never reaches this function, so it
  // keeps its base.routes verbatim — unchanged.
  if (routes.length === 0) {
    for (let i = 0; i < base.routes.length; i++) {
      if (!base.routes[i].isDefaultRoute) {
        routes.push(base.routes[i]);
      }
    }
  }

  const dnsServers: string[] = [];
  if (net.dns !== undefined && net.dns !== null) {
    for (let i = 0; i < net.dns.servers.length; i++) {
      dnsServers.push(net.dns.servers[i].ip);
    }
  }

  return {
    tunnelAddress: address,
    tunnelPrefixLength: prefixLength,
    interfaceName: base.interfaceName,
    routes: routes,
    dnsServers: dnsServers.length > 0 ? dnsServers : base.dnsServers,
    mtu: base.mtu,
    peerEndpoint: base.peerEndpoint
  };
}

// N2-H: graft the endpoint-derived exclusion entries onto a tunnel config's
// route list BEFORE create(). Existing entries are kept verbatim (the
// host-LAN exclusion and the default-route template keep their semantics);
// derived /32|/128 entries are appended once, deduplicated against the
// existing set by destination+prefix (pure merge — unit-tested in
// client/entry/selfcheck via the NetBirdEndpointExclusion module).
// applyNetworkConfig carries every isExcludedRoute entry of the grafted base
// into the merged config, on the create() path AND through the fallback
// (no-network-map) path alike.
export function withEndpointExclusions(base: VpnTunnelConfig,
  derived: ExcludedRouteEntry[]): VpnTunnelConfig {
  if (derived.length === 0) {
    return base;
  }
  return {
    tunnelAddress: base.tunnelAddress,
    tunnelPrefixLength: base.tunnelPrefixLength,
    interfaceName: base.interfaceName,
    routes: mergeExcludedRouteEntries(base.routes, derived),
    dnsServers: base.dnsServers,
    mtu: base.mtu,
    peerEndpoint: base.peerEndpoint
  };
}

// N3-7 default-route safety gate (shell half — the deciding half lives in
// core, see ShellNetworkConfig::default_route_decision): while the data
// plane cannot carry traffic, 0.0.0.0/0 must NOT enter the platform
// VpnConfig, or every flow (including our own management dial) is sucked
// into a dead tunnel = traffic black hole. `allowed=false` strips every
// isDefaultRoute entry from the merged config; the caller logs the core
// reason token (e.g. `default-route-held:data-plane-not-ready`).
export function gateDefaultRoutes(config: VpnTunnelConfig, allowed: boolean): VpnTunnelConfig {
  if (allowed) {
    return config;
  }
  const held: VpnRouteConfig[] = [];
  for (let i = 0; i < config.routes.length; i++) {
    if (!config.routes[i].isDefaultRoute) {
      held.push(config.routes[i]);
    }
  }
  return {
    tunnelAddress: config.tunnelAddress,
    tunnelPrefixLength: config.tunnelPrefixLength,
    interfaceName: config.interfaceName,
    routes: held,
    dnsServers: config.dnsServers,
    mtu: config.mtu,
    peerEndpoint: config.peerEndpoint
  };
}

// ---------------------------------------------------------------------------
// The tunnel's own subnet as an explicit managed route (DEFAULT behavior since
// 2026-09-26; formerly the opt-in S1 diagnostic switch).
//
// Huawei support (docs/n2h-huawei-support-replies-20260923.md §3.1(a), row
// 3.1(a)) states that whether a packet enters the VPN is decided ONLY by
// matching VpnConfig.routes (+ the trusted/blocked application lists) — there
// is NO implicit "the VPN app's own traffic / the tunnel subnet is handled
// anyway" rule. The on-device session of 0021 and the AUTH 0031 R0/R1 runs
// confirmed both directions: with the tunnel subnet absent from the submitted
// route list (0021: only 192.168.50.0/24 + the /32 exclusions, never
// 100.108.0.0/16, while the kernel main table had `vpn-tun 100.108.0.0/16`),
// a packet sent to a tunnel address from an unbound socket leaves over the
// physical network; with the entry present, it enters the tunnel. This helper
// derives exactly that one route from the tunnel address + prefix already
// present in the config (100.108.144.165/16 -> 100.108.0.0/16).
//
// Pure function, no side effects, never throws: the caller logs the outcome
// (`VPN_TUNNEL_SUBNET_ROUTE|…`) and the returned config is the only input to
// the downstream route logs / validation gate. It can only ADD a non-default,
// non-excluded route — it never removes, re-orders or relaxes an existing
// entry, and it is never a gate pass.
//
// The entry is skipped (with a reason token, never silently) when:
//   - the tunnel address is not a bare IPv4 literal (IPv6 / hostname /
//     'IPv4:port' dial shape / empty)   -> 'not-ipv4' | 'invalid-address',
//   - the prefix is 32 (a host route for the tunnel address itself, not a
//     subnet)                            -> 'prefix-32',
//   - the prefix is 0 or out of [1,32] (0.0.0.0/0 would be a default route,
//     which this entry's contract forbids) -> 'prefix-out-of-range',
//   - an entry with the same destination AND prefix is already present
//     (the tunnel network may already arrive from the core snapshot)
//                                        -> 'duplicate'.
export interface TunnelSubnetRouteOutcome {
  config: VpnTunnelConfig;
  added: boolean;
  // The computed network address, '' when the address/prefix could not yield
  // one (log truth only — never used to build a route in that case).
  network: string;
  // 'none' when added; otherwise one of the skip tokens above.
  reason: string;
}

// Strict dotted-quad parse (identical acceptance to the endpoint module's
// private isIPv4LiteralStr: 4 octets, 1-3 digits each, value <= 255).
// Returns [] for anything else. Local copy because that helper is not
// exported and this module must not grow an import cycle.
function parseIpv4Octets(address: string): number[] {
  const parts: string[] = address.split('.');
  if (parts.length !== 4) {
    return [];
  }
  const octets: number[] = [];
  for (let i = 0; i < parts.length; i++) {
    const part: string = parts[i];
    if (part.length === 0 || part.length > 3) {
      return [];
    }
    for (let c = 0; c < part.length; c++) {
      const code: number = part.charCodeAt(c);
      if (code < 0x30 || code > 0x39) {
        return [];
      }
    }
    const value: number = Number(part);
    if (value > 255) {
      return [];
    }
    octets.push(value);
  }
  return octets;
}

// Network address of `octets` under `prefix` (0..32), dotted-quad rendered.
function ipv4NetworkAddress(octets: number[], prefix: number): string {
  const value: number =
    ((octets[0] << 24) | (octets[1] << 16) | (octets[2] << 8) | octets[3]) >>> 0;
  const mask: number = prefix === 0 ? 0 : (0xffffffff << (32 - prefix)) >>> 0;
  const network: number = (value & mask) >>> 0;
  return `${(network >>> 24) & 0xff}.${(network >>> 16) & 0xff}.` +
    `${(network >>> 8) & 0xff}.${network & 0xff}`;
}

export function withTunnelSubnetRoute(config: VpnTunnelConfig): TunnelSubnetRouteOutcome {
  const skipped = (network: string, reason: string): TunnelSubnetRouteOutcome => {
    return { config: config, added: false, network: network, reason: reason };
  };
  const address: string = config.tunnelAddress;
  const prefix: number = config.tunnelPrefixLength;
  // A 'IPv4:port' dial shape or a real IPv6 literal both carry ':' — neither
  // is a tunnel ADDRESS; the split below would misread the first as a subnet.
  if (address.indexOf(':') >= 0) {
    return skipped('', 'not-ipv4');
  }
  const octets: number[] = parseIpv4Octets(address);
  if (octets.length !== 4) {
    return skipped('', 'invalid-address');
  }
  if (!Number.isInteger(prefix) || prefix > 32 || prefix < 1) {
    return skipped('', 'prefix-out-of-range');
  }
  if (prefix === 32) {
    return skipped(ipv4NetworkAddress(octets, prefix), 'prefix-32');
  }
  const network: string = ipv4NetworkAddress(octets, prefix);
  for (let i = 0; i < config.routes.length; i++) {
    if (config.routes[i].destination === network &&
      config.routes[i].prefixLength === prefix) {
      return skipped(network, 'duplicate');
    }
  }
  const route: VpnRouteConfig = {
    destination: network,
    prefixLength: prefix,
    // Same gateway/interface shape as every managed route (applyNetworkConfig)
    // and the MR4-verified literals: explicitly non-default, non-excluded.
    gateway: address,
    hasGateway: true,
    isDefaultRoute: false,
    isExcludedRoute: false
  };
  const routes: VpnRouteConfig[] = config.routes.slice();
  routes.push(route);
  const withRoute: VpnTunnelConfig = {
    tunnelAddress: config.tunnelAddress,
    tunnelPrefixLength: config.tunnelPrefixLength,
    interfaceName: config.interfaceName,
    routes: routes,
    dnsServers: config.dnsServers,
    mtu: config.mtu,
    peerEndpoint: config.peerEndpoint
  };
  return { config: withRoute, added: true, network: network, reason: 'none' };
}

// ---------------------------------------------------------------------------
// S6 diagnostic capture route (DEFAULT OFF; protect-bypass plan
// n2h-kit/protect-bypass-plan-20260926.md §3.1 + §6 item 1②).
//
// The bypass experiment needs ONE destination that is captured by the tunnel
// WITHOUT being masked by an endpoint exclusion, so the shell appends a single
// explicit non-excluded `<target>/32` route for the arm ticket's
// `protect_probe.target` while the `diag_capture_route` switch is on. Exactly
// one destination is affected (no default route, no exclusion set edit), which
// is what lets `/proc/net/route` decide whether the capture really landed.
//
// Pure function, no side effects, never throws: the caller logs the outcome
// (`N2H_DIAG_CAPTURE_ROUTE|…`) and the returned config is the only input to the
// downstream route logs / validation gate. It can only ADD one non-default,
// non-excluded host route — it never removes, re-orders or relaxes an existing
// entry, and it is never a gate pass.
//
// The entry is skipped (with a reason token, never silently) when:
//   - no string target was carried (absent `protect_probe` / wrong type)
//                                                          -> 'no-target',
//   - the target is not the core's strict `IPv4:port` acceptance (bare
//     dotted-quad without leading zeros + decimal 1..=65535 port without a
//     leading zero — the SAME rules `parse_protect_target` applies, so a
//     target the core would refuse never gets a capture route here)
//                                                          -> 'invalid-target',
//   - the target IP is ALREADY inside an isExcludedRoute entry (the derived
//     endpoint /32|/128 freezes AND the base LAN /24 alike — a /24 exclusion
//     puts the IP in the exclusion set just as much as a /32 does)
//                                                          -> 'target-excluded'
//     (the anti-masking precondition: an excluded target would re-create
//     exactly the masking this experiment exists to remove, and a longer-prefix
//     non-excluded /32 would silently shadow that exclusion).
export interface CaptureRouteOutcome {
  config: VpnTunnelConfig;
  added: boolean;
  // The bare target IPv4 the /32 was built from — '' when none could be
  // derived (log truth only; never used to build a route in that case).
  target: string;
  // 'none' when added; otherwise one of the skip tokens above.
  reason: string;
}

// The core's `parse_ipv4` acceptance (config.rs:408): 4 dot-separated parts,
// 1-3 digits each, value <= 255, and NO leading zero (so '010.1.1.1' and the
// short forms are refused on both sides alike).
function isStrictTargetIpv4(address: string): boolean {
  const octets: number[] = parseIpv4Octets(address);
  if (octets.length !== 4) {
    return false;
  }
  const parts: string[] = address.split('.');
  for (let i = 0; i < parts.length; i++) {
    if (parts[i].length > 1 && parts[i].charCodeAt(0) === 0x30) {
      return false;
    }
  }
  return true;
}

// The core's port acceptance (parse_protect_target): decimal digits only,
// 1..=65535, no leading zero.
function isStrictTargetPort(port: string): boolean {
  if (port.length === 0 || port.length > 5) {
    return false;
  }
  if (port.length > 1 && port.charCodeAt(0) === 0x30) {
    return false;
  }
  for (let i = 0; i < port.length; i++) {
    const code: number = port.charCodeAt(i);
    if (code < 0x30 || code > 0x39) {
      return false;
    }
  }
  const value: number = Number(port);
  return value >= 1 && value <= 65535;
}

// Is `address` (a bare IPv4) inside `route`'s destination/prefix? Only the
// two IPv4 shapes can contain an IPv4 target: a destination carrying ':' is an
// IPv6 freeze and never matches, and a non-IPv4 destination is not a route this
// helper can reason about (false = "not proven excluded", so the caller can
// still refuse earlier on its own shape checks).
function ipv4InRoute(address: string, route: VpnRouteConfig): boolean {
  const prefix: number = route.prefixLength;
  if (!Number.isInteger(prefix) || prefix < 0 || prefix > 32) {
    return false;
  }
  const octets: number[] = parseIpv4Octets(address);
  const destOctets: number[] = parseIpv4Octets(route.destination);
  if (octets.length !== 4 || destOctets.length !== 4) {
    return false;
  }
  const target: number =
    ((octets[0] << 24) | (octets[1] << 16) | (octets[2] << 8) | octets[3]) >>> 0;
  const dest: number =
    ((destOctets[0] << 24) | (destOctets[1] << 16) | (destOctets[2] << 8) | destOctets[3]) >>> 0;
  const mask: number = prefix === 0 ? 0 : (0xffffffff << (32 - prefix)) >>> 0;
  return ((target & mask) >>> 0) === ((dest & mask) >>> 0);
}

export function withCaptureRoute(config: VpnTunnelConfig, target: string): CaptureRouteOutcome {
  const skipped = (ip: string, reason: string): CaptureRouteOutcome => {
    return { config: config, added: false, target: ip, reason: reason };
  };
  if (typeof target !== 'string' || target.length === 0) {
    return skipped('', 'no-target');
  }
  const sep: number = target.lastIndexOf(':');
  if (sep <= 0 || sep === target.length - 1) {
    return skipped('', 'invalid-target');
  }
  const address: string = target.slice(0, sep);
  if (!isStrictTargetIpv4(address) || !isStrictTargetPort(target.slice(sep + 1))) {
    return skipped('', 'invalid-target');
  }
  for (let i = 0; i < config.routes.length; i++) {
    const route: VpnRouteConfig = config.routes[i];
    if (route.isExcludedRoute === true && ipv4InRoute(address, route)) {
      return skipped(address, 'target-excluded');
    }
  }
  const route: VpnRouteConfig = {
    destination: address,
    prefixLength: 32,
    // Same gateway/interface shape as every managed route (applyNetworkConfig)
    // and the tunnel-subnet entry: explicitly non-default, non-excluded.
    gateway: config.tunnelAddress,
    hasGateway: true,
    isDefaultRoute: false,
    isExcludedRoute: false
  };
  const routes: VpnRouteConfig[] = config.routes.slice();
  routes.push(route);
  const withRoute: VpnTunnelConfig = {
    tunnelAddress: config.tunnelAddress,
    tunnelPrefixLength: config.tunnelPrefixLength,
    interfaceName: config.interfaceName,
    routes: routes,
    dnsServers: config.dnsServers,
    mtu: config.mtu,
    peerEndpoint: config.peerEndpoint
  };
  return { config: withRoute, added: true, target: address, reason: 'none' };
}

// Maps the client configuration onto the platform VpnConfig exactly in the
// spike-verified MR4 field shapes. Non-excluded routes omit isExcludedRoute
// (platform default false) to stay byte-shape identical to the verified MR4
// literals; excluded routes set it explicitly.
//
// S2 DIAGNOSTIC SWITCH (default OFF): `blockedApplications` is written ONLY
// when the caller passes a non-empty list — an absent switch leaves the field
// ABSENT (never an empty array), which keeps the submitted document
// byte-shape identical to the pre-switch mapper output. `trustedApplications`
// is never set (the two lists are mutually exclusive per the SDK); the
// Huawei-supported use is the SELF bundle name, see the caller
// (NetBirdVpnExtensionAbility.diagBlockedApplicationList).
export function buildPlatformVpnConfig(config: VpnTunnelConfig,
  blockedApplications?: string[], trustedApplications?: string[]): vpnExtension.VpnConfig {
  if (blockedApplications !== undefined && blockedApplications.length > 0 &&
    trustedApplications !== undefined && trustedApplications.length > 0) {
    throw new Error('trusted-and-blocked-applications-conflict');
  }
  const address: vpnExtension.LinkAddress = {
    address: { address: config.tunnelAddress, family: 1 },
    prefixLength: config.tunnelPrefixLength
  };
  const routes: vpnExtension.RouteInfo[] = [];
  for (let i = 0; i < config.routes.length; i++) {
    const routeConfig: VpnRouteConfig = config.routes[i];
    // N2-H: an IPv6 exclusion destination (a /128 freeze of a v6 endpoint)
    // must carry family 2 (NetAddress family: 1 = IPv4 default, 2 = IPv6);
    // every existing route shape stays family 1.
    const destinationFamily: number = routeConfig.destination.indexOf(':') >= 0 ? 2 : 1;
    const route: vpnExtension.RouteInfo = {
      interface: config.interfaceName,
      destination: {
        address: { address: routeConfig.destination, family: destinationFamily },
        prefixLength: routeConfig.prefixLength
      },
      gateway: { address: routeConfig.gateway, family: 1 },
      hasGateway: routeConfig.hasGateway,
      isDefaultRoute: routeConfig.isDefaultRoute
    };
    if (routeConfig.isExcludedRoute) {
      route.isExcludedRoute = true;
    }
    routes.push(route);
  }
  // Field insertion order is the submitted-document shape the host checks
  // pin (addresses, routes, dnsAddresses, mtu, …): declare the two mandatory
  // fields, then add `dnsAddresses` only when the config actually carries a
  // server. The N2 evidence generation carries none (NetBirdN2Evidence
  // decision (a)), so the field stays ABSENT there instead of submitting an
  // empty array; every ordinary config keeps at least the deployment default
  // (`8.8.8.8`), so its byte shape is unchanged.
  const vpnConfig: vpnExtension.VpnConfig = {
    addresses: [address],
    routes: routes
  };
  if (config.dnsServers.length > 0) {
    vpnConfig.dnsAddresses = config.dnsServers;
  }
  vpnConfig.mtu = config.mtu;
  // S2 (see the doc comment): present ONLY when a non-empty list was passed.
  if (blockedApplications !== undefined && blockedApplications.length > 0) {
    vpnConfig.blockedApplications = blockedApplications;
  }
  if (trustedApplications !== undefined && trustedApplications.length > 0) {
    vpnConfig.trustedApplications = trustedApplications;
  }
  return vpnConfig;
}
