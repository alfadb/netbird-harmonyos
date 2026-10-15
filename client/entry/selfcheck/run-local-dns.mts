// Run: node client/entry/selfcheck/run-local-dns.mts (no HAP/device).
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { register } from 'node:module';
import assert from 'node:assert/strict';
register(new URL('./local-dns-hooks.mjs', import.meta.url));
const real = new URL('../src/main/ets/vpnextensionability/', import.meta.url);
const dir = mkdtempSync(join(tmpdir(), 'nb-local-dns-'));
let count = 0;
function check(name: string, value: boolean): void {
  assert.ok(value, name); count++; console.log(`ok ${name}`);
}
try {
  writeFileSync(join(dir, 'NetBirdVpnConfig.ts'), readFileSync(new URL('NetBirdVpnConfig.ets', real)));
  writeFileSync(join(dir, 'NetBirdEndpointExclusion.ts'),
    readFileSync(new URL('NetBirdEndpointExclusion.ets', real), 'utf8') + '\nexport const ExcludedRouteEntry = {};\n');
  const m = await import(join(dir, 'NetBirdVpnConfig.ts'));
  const base = structuredClone(m.loadVpnTunnelConfig());
  base.routes.push({ destination: '100.108.0.0', prefixLength: 16,
    gateway: base.tunnelAddress, hasGateway: true, isDefaultRoute: false, isExcludedRoute: true });
  const before = JSON.stringify(base);
  const ip = '100.108.255.254';
  const net = { available: true, address: '100.108.12.3', address_prefix_len: 16,
    routes: [{ network: '192.168.1.0/24', is_default: false }],
    dns: { service_enable: false, servers: [{ ip: '9.9.9.9', port: 5353 }],
      local_resolver_ip: ip, local_resolver_ready: true,
      search_domains: [' Example.COM. ', 'example.com', 'NETBIRD.INTERNAL.', '', '*.bad', '-bad.test'],
      custom_zone_count: 1, record_count: 2, nameserver_group_count: 3 } };
  const apply = (n = net) => m.buildPlatformVpnConfig(m.withTunnelSubnetRoute(
    m.gateDefaultRoutes(m.applyNetworkConfig(base, n), false)).config);
  const platform = apply();
  check('ready overrides flattened upstreams even service disabled', JSON.stringify(platform.dnsAddresses) === JSON.stringify([ip]));
  check('upstream non-53 port remains core metadata', net.dns.servers[0].port === 5353 && !platform.dnsAddresses[0].includes(':'));
  check('canonical deduplicated search domains', JSON.stringify(platform.searchDomains) === JSON.stringify(['example.com', 'netbird.internal']));
  const hosts = platform.routes.filter((r) => r.destination.address.address === ip && r.destination.prefixLength === 32);
  check('one exact IPv4 host route enters TUN', hosts.length === 1 && hosts[0].interface === 'vpn-tun' && !hosts[0].isExcludedRoute && hosts[0].destination.address.family === 1);
  check('broader LAN exclusion preserved', platform.routes.some((r) => r.destination.prefixLength === 16 && r.isExcludedRoute));
  check('base remains unchanged', before === JSON.stringify(base));
  const local = m.applyNetworkConfig(base, net);
  check('capture-route copy preserves search domains', JSON.stringify(m.withCaptureRoute(local, '203.0.113.9:53').config.searchDomains) === JSON.stringify(local.searchDomains));
  check('exclusion copy preserves search domains', JSON.stringify(m.withEndpointExclusions(local, [{ destination: '198.51.100.1', prefixLength: 32, gateway: base.tunnelAddress, hasGateway: true, isDefaultRoute: false, isExcludedRoute: true }]).searchDomains) === JSON.stringify(local.searchDomains));
  const conflictBase = structuredClone(base);
  conflictBase.routes.push({ destination: ip, prefixLength: 32, gateway: base.tunnelAddress,
    hasGateway: true, isDefaultRoute: false, isExcludedRoute: true });
  const conflict = m.applyNetworkConfig(conflictBase, net).routes.filter((r) => r.destination === ip && r.prefixLength === 32);
  check('exact excluded duplicate becomes one TUN host route', conflict.length === 1 && !conflict[0].isExcludedRoute);
  const invalid = structuredClone(net); invalid.dns.local_resolver_ip = `${ip}:53`;
  check('platform rejects resolver IP with port without flattening fallback', JSON.stringify(apply(invalid).dnsAddresses) === JSON.stringify(base.dnsServers));
  const dup = structuredClone(net);
  dup.routes.push({ network: `${ip}/32`, is_default: false }, { network: `${ip}/32`, is_default: false });
  check('duplicate host routes collapse', apply(dup).routes.filter((r) => r.destination.address.address === ip && r.destination.prefixLength === 32).length === 1);
  const waiting = structuredClone(net); waiting.dns.local_resolver_ready = false;
  const held = apply(waiting);
  check('not-ready retains base DNS', JSON.stringify(held.dnsAddresses) === JSON.stringify(base.dnsServers));
  check('not-ready never creates virtual host route', !held.routes.some((r) => r.destination.address.address === ip));
  const legacy = structuredClone(net); delete legacy.dns.local_resolver_ready;
  delete legacy.dns.local_resolver_ip;
  check('old schema keeps flattened IP compatibility', JSON.stringify(apply(legacy).dnsAddresses) === JSON.stringify(['9.9.9.9']));
  const rules = structuredClone(net); rules.dns.servers = [{ ip: '1.1.1.1', port: 1053 }];
  rules.dns.record_count = 99; rules.dns.custom_zone_count = 8;
  check('same runtime address with changed rules has identical platform config', JSON.stringify(apply(rules)) === JSON.stringify(platform));
  const search = structuredClone(net); search.dns.search_domains = ['other.internal'];
  check('search changes are visible in platform config', JSON.stringify(apply(search)) !== JSON.stringify(platform));
  const changed = structuredClone(net); changed.dns.local_resolver_ip = '100.109.255.254';
  check('resolver IP changes are visible', JSON.stringify(apply(changed)) !== JSON.stringify(platform));
  const ability = readFileSync(new URL('NetBirdVpnExtensionAbility.ets', real), 'utf8');
  check('initial uses shared merger and platform mapper', ability.includes('net !== null ? applyNetworkConfig(base, net) : base') && ability.includes("buildPlatformVpnConfig(merged, this.diagBlockedApplicationList('initial')"));
  check('recreate uses shared merger and platform mapper', ability.includes('let merged: VpnTunnelConfig = applyNetworkConfig(base, net);') && ability.includes("buildPlatformVpnConfig(merged, this.diagBlockedApplicationList('recreate')"));
  check('local DNS route stays out of management ACK', ability.includes('net.routes.map((r: CoreRouteEntry): string => r.network)'));
  console.log(`${count} local DNS checks passed`);
} finally { rmSync(dir, { recursive: true, force: true }); }
