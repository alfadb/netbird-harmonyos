import vpnExtension from '@ohos.net.vpnExtension';

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
  // DNS servers declared to the platform (VpnConfig.dnsAddresses).
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
// route 2: IPv4 default 0.0.0.0/0 with isDefaultRoute: true;
// route 3: host-LAN exclusion 192.168.50.0/24 with isExcludedRoute: true
// (keeps the outer path — management/signal/WG endpoint traffic — out of our
// own tunnel; the exclusion network is deployment-specific and stays
// configurable).
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
    },
    {
      destination: '192.168.50.0',
      prefixLength: 24,
      gateway: '10.99.0.1',
      hasGateway: true,
      isDefaultRoute: false,
      isExcludedRoute: true
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

// Maps the client configuration onto the platform VpnConfig exactly in the
// spike-verified MR4 field shapes. Non-excluded routes omit isExcludedRoute
// (platform default false) to stay byte-shape identical to the verified MR4
// literals; excluded routes set it explicitly.
export function buildPlatformVpnConfig(config: VpnTunnelConfig): vpnExtension.VpnConfig {
  const address: vpnExtension.LinkAddress = {
    address: { address: config.tunnelAddress, family: 1 },
    prefixLength: config.tunnelPrefixLength
  };
  const routes: vpnExtension.RouteInfo[] = [];
  for (let i = 0; i < config.routes.length; i++) {
    const routeConfig: VpnRouteConfig = config.routes[i];
    const route: vpnExtension.RouteInfo = {
      interface: config.interfaceName,
      destination: {
        address: { address: routeConfig.destination, family: 1 },
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
  const vpnConfig: vpnExtension.VpnConfig = {
    addresses: [address],
    routes: routes,
    dnsAddresses: config.dnsServers,
    mtu: config.mtu
  };
  return vpnConfig;
}
