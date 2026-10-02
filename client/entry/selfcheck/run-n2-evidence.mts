// Host-only selfcheck loads the shipped ArkTS files byte-for-byte.
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./route-check-hooks.mjs', import.meta.url).href);
const here = dirname(fileURLToPath(import.meta.url));
const real = join(here, '..', 'src', 'main', 'ets', 'vpnextensionability');
const dir = mkdtempSync(join(tmpdir(), 'nb-n2-evidence-'));
try {
  for (const name of ['NetBirdN2Evidence', 'NetBirdVpnConfig', 'NetBirdEndpointExclusion',
    'NetBirdN2Ring']) {
    writeFileSync(join(dir, `${name}.ts`), readFileSync(join(real, `${name}.ets`)));
  }
  writeFileSync(join(dir, 'NetBirdVpnConfig.dep.ts'),
    'export { withCaptureRoute, loadVpnTunnelConfig, buildPlatformVpnConfig } from "./NetBirdVpnConfig.ts";\n' +
    'export const CaptureRouteOutcome = undefined; export const VpnTunnelConfig = undefined;\n');
  writeFileSync(join(dir, 'NetBirdEndpointExclusion.dep.ts'),
    'export { mergeExcludedRouteEntries } from "./NetBirdEndpointExclusion.ts";\n' +
    'export const ExcludedRouteEntry = undefined;\n');
  const n2 = await import(pathToFileURL(join(dir, 'NetBirdN2Evidence.ts')).href);
  const cfg = await import(pathToFileURL(join(dir, 'NetBirdVpnConfig.ts')).href);
  const decide = (n2data: unknown, blocked = false, s6 = false, force = false,
    exclusions?: unknown) => n2.parseN2Evidence(n2data, blocked, s6, force, exclusions);
  const req = { trusted_self: true, capture_32: ['119.29.29.119'],
    outer_destinations: ['tcp/119.29.29.119:443', 'udp/119.29.29.119:53'],
    hdc_control_ip: '192.168.50.1' };
  const assert = (ok: boolean, label: string) => {
    if (!ok) throw new Error(label);
    console.log('ok ' + label);
  };
  assert(decide(undefined).options === undefined, 'default off');
  for (const [data, blocked, s6, force, exclusions] of [
    [req, true, false, false, undefined], [req, false, true, false, undefined],
    [req, false, false, true, undefined], [req, false, false, false, true],
    [{ ...req, hdc_control_ip: '119.29.29.119' }, false, false, false, undefined],
    [{ ...req, capture_32: ['119.29.29.999'] }, false, false, false, undefined],
    [{ ...req, outer_destinations: [] }, false, false, false, undefined],
    [{ ...req, outer_destinations: ['udp/119.29.29.120:53'] }, false, false, false, undefined],
    [{ ...req, outer_destinations: ['udp/119.29.29.119:53', 'udp/119.29.29.119:53'] }, false, false, false, undefined]
  ] as const) {
    assert(decide(data, blocked, s6, force, exclusions).options === undefined,
      'refuses conflict or invalid address');
  }
  const accepted = decide(req).options;
  assert(accepted?.capture32[0] === '119.29.29.119', 'valid arm');
  // The diagnostic ring cap (`ring_segment_bytes`, 2026-10-01): ABSENT keeps the
  // documented 4 MiB default; a present whole number inside the shrink-only
  // 64 KiB..4 MiB window rides the options verbatim; anything else refuses the
  // WHOLE evidence ticket by name (fail-closed — no VPN, no ring).
  assert(accepted?.ringSegmentBytes === 4 * 1024 * 1024, 'ring cap absent ⇒ 4 MiB default');
  assert(decide({ ...req, ring_segment_bytes: 64 * 1024 }).options?.ringSegmentBytes === 65536,
    'ring cap accepts the 64 KiB lower bound');
  assert(decide({ ...req, ring_segment_bytes: 256 * 1024 }).options?.ringSegmentBytes === 262144,
    'ring cap accepts a shrunk value verbatim');
  assert(decide({ ...req, ring_segment_bytes: 4 * 1024 * 1024 }).options?.ringSegmentBytes ===
    4 * 1024 * 1024, 'ring cap accepts exactly the 4 MiB default');
  for (const bad of [64 * 1024 - 1, 4 * 1024 * 1024 + 1, 0, -1, 1.5, '65536', null, true]) {
    const decision = decide({ ...req, ring_segment_bytes: bad });
    assert(decision.options === undefined && decision.error === 'invalid-ring-segment-bytes',
      'ring cap refuses ' + JSON.stringify(bad));
  }
  const base = n2.withoutN2ExcludedRoutes(cfg.loadVpnTunnelConfig());
  const routed = n2.withN2EvidenceRoutes(base, accepted.capture32);
  assert(routed.routes.some((r: {destination: string; prefixLength: number; isExcludedRoute: boolean}) =>
    r.destination === '119.29.29.119' && r.prefixLength === 32 && !r.isExcludedRoute),
  'non-excluded capture /32');
  assert(routed.routes.every((r: {isExcludedRoute: boolean}) => !r.isExcludedRoute),
    'base exclusion removed');
  const trusted = cfg.buildPlatformVpnConfig(routed, undefined, ['com.netbird.harmonyos']);
  assert(trusted.trustedApplications?.length === 1 && !('blockedApplications' in trusted),
    'trusted app only');
  const ordinary = cfg.buildPlatformVpnConfig(cfg.loadVpnTunnelConfig());
  assert(!('trustedApplications' in ordinary) && !('blockedApplications' in ordinary),
    'default config unchanged');
  // N2 evidence DNS decision (a): the evidence generation declares no platform
  // DNS server, so the submitted VpnConfig omits dnsAddresses entirely.
  assert(routed.dnsServers.length === 0, 'evidence config declares no platform DNS server');
  assert(!('dnsAddresses' in trusted), 'evidence VpnConfig omits dnsAddresses');
  assert(JSON.stringify(Object.keys(ordinary)) ===
    JSON.stringify(['addresses', 'routes', 'dnsAddresses', 'mtu']),
    'ordinary VpnConfig field order stays addresses/routes/dnsAddresses/mtu');
  assert(JSON.stringify(ordinary.dnsAddresses) === JSON.stringify(['8.8.8.8']),
    'ordinary config keeps the deployment DNS default');
  // The real create order is applyNetworkConfig(base, net) BEFORE the evidence
  // transform: a management-advertised nameserver must not reappear through
  // the core snapshot (this is why (b)'s arm-time collision check is fragile
  // — the DNS set is only final at create()).
  const coreNet = { available: true, address: '10.99.0.1', address_prefix_len: 32,
    routes: [], dns: { service_enable: true, servers: [{ ip: '223.5.5.5', port: 53 }] } };
  const refilled = cfg.applyNetworkConfig(base, coreNet);
  assert(refilled.dnsServers.length === 1 && refilled.dnsServers[0] === '223.5.5.5',
    'core snapshot does carry a platform DNS server');
  const evidence = n2.withN2EvidenceRoutes(refilled, accepted.capture32);
  assert(evidence.dnsServers.length === 0, 'evidence transform drops the core-snapshot DNS too');
  assert(!('dnsAddresses' in cfg.buildPlatformVpnConfig(evidence, undefined,
    ['com.netbird.harmonyos'])), 'submitted evidence VpnConfig carries no dnsAddresses');
  assert(JSON.stringify(n2.withN2EvidenceRoutes(routed, accepted.capture32)) === JSON.stringify(routed),
    'recreate route idempotence');
  assert(n2.validateN2Control(accepted.capture32, '119.29.29.119:53', 'event-pre') === '',
    'control accepts armed captured endpoint');
  assert(n2.validateN2Control(accepted.capture32, '119.29.29.120:53', 'event-pre') ===
    'target-not-captured', 'control rejects destination outside capture');
  assert(n2.validateN2Control(accepted.capture32, '119.29.29.119:65536', 'event-pre') ===
    'target-not-captured', 'control rejects invalid port');
  assert(n2.validateN2Control(accepted.capture32, '119.29.29.119:53', 'unsafe|marker') ===
    'invalid-marker', 'control rejects unsafe marker');
  let refused = false;
  try {
    n2.withN2EvidenceRoutes({ ...routed, routes: [...routed.routes,
      { destination: '119.29.29.119', prefixLength: 32, gateway: routed.tunnelAddress,
        hasGateway: true, isDefaultRoute: false, isExcludedRoute: true }] }, accepted.capture32);
  } catch {
    refused = true;
  }
  assert(refused, 'excluded /32 conflict refused even beside an existing capture route');
} finally {
  rmSync(dir, { recursive: true, force: true });
}
