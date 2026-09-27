import { CaptureRouteOutcome, VpnTunnelConfig, withCaptureRoute } from './NetBirdVpnConfig';

export interface N2EvidenceOptions {
  trustedSelf: boolean;
  capture32: string[];
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
    hdcControlIp: typeof hdc === 'string' ? hdc : '', allowHdcOverlap: allow === true } };
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
  return next;
}
