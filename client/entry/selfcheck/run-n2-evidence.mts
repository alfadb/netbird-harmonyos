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
  for (const name of ['NetBirdN2Evidence', 'NetBirdVpnConfig', 'NetBirdEndpointExclusion']) {
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
  const req = { trusted_self: true, capture_32: ['119.29.29.119'], hdc_control_ip: '192.168.50.1' };
  const assert = (ok: boolean, label: string) => {
    if (!ok) throw new Error(label);
    console.log('ok ' + label);
  };
  assert(decide(undefined).options === undefined, 'default off');
  for (const [data, blocked, s6, force, exclusions] of [
    [req, true, false, false, undefined], [req, false, true, false, undefined],
    [req, false, false, true, undefined], [req, false, false, false, true],
    [{ ...req, hdc_control_ip: '119.29.29.119' }, false, false, false, undefined],
    [{ ...req, capture_32: ['119.29.29.999'] }, false, false, false, undefined]
  ] as const) {
    assert(decide(data, blocked, s6, force, exclusions).options === undefined,
      'refuses conflict or invalid address');
  }
  const accepted = decide(req).options;
  assert(accepted?.capture32[0] === '119.29.29.119', 'valid arm');
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
  assert(JSON.stringify(n2.withN2EvidenceRoutes(routed, accepted.capture32)) === JSON.stringify(routed),
    'recreate route idempotence');
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
