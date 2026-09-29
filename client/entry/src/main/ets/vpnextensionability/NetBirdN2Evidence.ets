import { CaptureRouteOutcome, VpnTunnelConfig, withCaptureRoute } from './NetBirdVpnConfig';

export interface N2EvidenceOptions {
  trustedSelf: boolean;
  capture32: string[];
  outerDestinations: string[];
  hdcControlIp: string;
  allowHdcOverlap: boolean;
}

export interface N2EvidenceDecision {
  options?: N2EvidenceOptions;
  error: string;
}

// A missing key is inert. A present but malformed request must not start a VPN.
export function parseN2Evidence(raw: Object | undefined, blockedSelf: boolean,
  oldCapture: boolean, forceDefaultRoute: boolean,
  endpointSetting: Object | undefined): N2EvidenceDecision {
  if (raw === undefined) {
    return { error: '' };
  }
  if (raw === null || typeof raw !== 'object' || Array.isArray(raw)) {
    return { error: 'invalid-options' };
  }
  const input: Record<string, Object> = raw as Record<string, Object>;
  if (input['trusted_self'] !== true || !Array.isArray(input['capture_32']) ||
    (input['capture_32'] as Object[]).length === 0 ||
    (input['capture_32'] as Object[]).length > 32) {
    return { error: 'invalid-options' };
  }
  if (blockedSelf || oldCapture) {
    return { error: 'dev-switch-conflict' };
  }
  if (forceDefaultRoute) {
    return { error: 'force-default-route' };
  }
  if (endpointSetting === true) {
    return { error: 'endpoint-exclusions-conflict' };
  }
  const capture32: string[] = [];
  const items: Object[] = input['capture_32'] as Object[];
  for (let i = 0; i < items.length; i++) {
    const ip: Object = items[i];
    if (typeof ip !== 'string' || !/^(0|[1-9][0-9]{0,2})(\.(0|[1-9][0-9]{0,2})){3}$/.test(ip) ||
      ip.split('.').some((part: string): boolean => Number(part) > 255) || capture32.indexOf(ip) >= 0) {
      return { error: 'invalid-capture-ip' };
    }
    capture32.push(ip);
  }
  const rawOuter: Object | undefined = input['outer_destinations'];
  if (!Array.isArray(rawOuter) || rawOuter.length === 0 || rawOuter.length > 128) {
    return { error: 'invalid-outer-destinations' };
  }
  const outerDestinations: string[] = [];
  for (const item of rawOuter as Object[]) {
    if (typeof item !== 'string') { return { error: 'invalid-outer-destinations' }; }
    const slash: number = item.indexOf('/');
    const proto: string = item.substring(0, slash);
    const target: string = item.substring(slash + 1);
    if ((proto !== 'tcp' && proto !== 'udp') ||
      validateN2Control(capture32, target, 'outer') !== '' ||
      outerDestinations.indexOf(item) >= 0) {
      return { error: 'invalid-outer-destinations' };
    }
    outerDestinations.push(item);
  }
  const hdc: Object | undefined = input['hdc_control_ip'];
  if (hdc !== undefined && (typeof hdc !== 'string' ||
    !/^(0|[1-9][0-9]{0,2})(\.(0|[1-9][0-9]{0,2})){3}$/.test(hdc) ||
    hdc.split('.').some((part: string): boolean => Number(part) > 255))) {
    return { error: 'invalid-hdc-ip' };
  }
  const allow: Object | undefined = input['allow_hdc_overlap'];
  if (allow !== undefined && typeof allow !== 'boolean') {
    return { error: 'invalid-hdc-override' };
  }
  if (hdc !== undefined && capture32.indexOf(hdc as string) >= 0 && allow !== true) {
    return { error: 'hdc-overlap' };
  }
  return { error: '', options: { trustedSelf: true, capture32: capture32,
    outerDestinations: outerDestinations,
    hdcControlIp: typeof hdc === 'string' ? hdc : '', allowHdcOverlap: allow === true } };
}

export function validateN2Control(capture32: string[], target: string, id: string): string {
  const match = /^((?:0|[1-9][0-9]{0,2})(?:\.(?:0|[1-9][0-9]{0,2})){3}):([1-9][0-9]{0,4})$/.exec(target);
  if (match === null || match[1].split('.').some((part: string): boolean => Number(part) > 255) ||
    Number(match[2]) > 65535 || capture32.indexOf(match[1]) < 0) {
    return 'target-not-captured';
  }
  if (!/^[A-Za-z0-9_-]{1,64}$/.test(id)) {
    return 'invalid-marker';
  }
  return '';
}

export function withoutN2ExcludedRoutes(config: VpnTunnelConfig): VpnTunnelConfig {
  return {
    tunnelAddress: config.tunnelAddress,
    tunnelPrefixLength: config.tunnelPrefixLength,
    interfaceName: config.interfaceName,
    routes: config.routes.filter((route): boolean => !route.isExcludedRoute),
    dnsServers: config.dnsServers,
    mtu: config.mtu,
    peerEndpoint: config.peerEndpoint
  };
}

export function withN2EvidenceRoutes(config: VpnTunnelConfig, ips: string[]): VpnTunnelConfig {
  let next: VpnTunnelConfig = config;
  for (let i = 0; i < ips.length; i++) {
    const outcome: CaptureRouteOutcome = withCaptureRoute(next, `${ips[i]}:53`);
    if (!outcome.added) {
      throw new Error(`capture-route-${outcome.reason}`);
    }
    const existing = next.routes.find((route): boolean => route.destination === ips[i] &&
      route.prefixLength === 32 && route.isExcludedRoute === false);
    if (existing === undefined) {
      next = outcome.config;
    }
  }
  // N2 evidence DNS decision (a), 2026-09-28: the evidence generation declares
  // NO platform DNS server. The App's own outer-endpoint resolution does not
  // use it — initial management/signal names resolve BEFORE create() on the
  // physical network (resolveManagementIp) and every post-create /relay /STUN
  // lookup goes through the core's protected-DNS fd path (resolveProtectedIp,
  // `purpose=dns`), while the D row itself forces a protected query and never
  // the system resolver. Keeping `dnsServers` therefore buys the evidence run
  // nothing, but a declared resolver IP that is also a `capture_32`
  // destination (e.g. the Wi-Fi resolver, or 223.5.5.5 listed as the D-row
  // destination) would let the platform route the App's own resolver query
  // into the TUN, where the campaign-wide from-OS destination scan counts it
  // as an outer frame to a frozen destination — a false hit. (b) was rejected:
  // the DNS set is only final at create() (the core network-map snapshot is
  // applied after the ticket is armed), so an arm-time collision check over
  // the deployment default would not cover a management-advertised server.
  return {
    tunnelAddress: next.tunnelAddress,
    tunnelPrefixLength: next.tunnelPrefixLength,
    interfaceName: next.interfaceName,
    routes: next.routes,
    dnsServers: [],
    mtu: next.mtu,
    peerEndpoint: next.peerEndpoint
  };
}
