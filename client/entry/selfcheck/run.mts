// N2-H pure-module self-check (host-side, no device, no network).
//
// The production module client/entry/src/main/ets/vpnextensionability/
// NetBirdEndpointExclusion.ets is a zero-import pure module. node (v22.7+,
// type stripping) cannot load the .ets extension directly, so this harness
// copies the REAL file byte-for-byte into an OS-temp fixture (.ts) at run
// time — the code under test is the shipped code, never a hand-held mirror —
// and runs the unit suite against it.
//
// Run: node client/entry/selfcheck/run.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability',
  'NetBirdEndpointExclusion.ets');

const fixtureDir: string = mkdtempSync(join(tmpdir(), 'nb-endpoint-exclusion-'));
const fixture: string = join(fixtureDir, 'NetBirdEndpointExclusion.ts');
writeFileSync(fixture, readFileSync(REAL)); // byte-identical copy of the real module

let passed: number = 0;
let failedCount: number = 0;

function check(name: string, condition: boolean, detail?: string): void {
  if (condition) {
    passed++;
    console.log(`ok   ${name}`);
  } else {
    failedCount++;
    console.log(`FAIL ${name}${detail !== undefined ? ` — ${detail}` : ''}`);
  }
}

function deepEq(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

try {
  const mod = await import(pathToFileURL(fixture).href);

  // ---- classifyAddress -----------------------------------------------------
  check('classify ipv4', mod.classifyAddress('192.168.50.12') === 'ipv4');
  check('classify ipv4 zero-octet', mod.classifyAddress('0.0.0.0') === 'ipv4');
  check('classify ipv6', mod.classifyAddress('2001:db8::1') === 'ipv6');
  check('classify ipv6 loopback', mod.classifyAddress('::1') === 'ipv6');
  check('classify empty is invalid', mod.classifyAddress('') === 'invalid');
  check('classify hostname is invalid', mod.classifyAddress('home.alfadb.cn') === 'invalid');
  check('classify out-of-range octet is invalid', mod.classifyAddress('256.1.1.1') === 'invalid');
  check('classify five octets is invalid', mod.classifyAddress('1.2.3.4.5') === 'invalid');
  check('classify exponent form is invalid', mod.classifyAddress('1e2.0.0.1') === 'invalid');
  check('classify signed octet is invalid', mod.classifyAddress('1.2.3.-4') === 'invalid');

  // ---- parseRelayUrl (mirror of core RelayUrl::parse) ----------------------
  const prod = mod.parseRelayUrl('rels://home.alfadb.cn:28443');
  check('rels host:port', deepEq(prod, { host: 'home.alfadb.cn', port: 28443 }));
  const prodRel = mod.parseRelayUrl('rel://home.alfadb.cn:28443');
  check('rel host:port', deepEq(prodRel, { host: 'home.alfadb.cn', port: 28443 }));
  check('rel default port 80', deepEq(mod.parseRelayUrl('rel://relay.example'),
    { host: 'relay.example', port: 80 }));
  check('rels default port 443', deepEq(mod.parseRelayUrl('rels://relay.example'),
    { host: 'relay.example', port: 443 }));
  check('scheme is case-insensitive', deepEq(mod.parseRelayUrl('RELS://h.example:443'),
    { host: 'h.example', port: 443 }));
  check('bracketed ipv6 with port', deepEq(mod.parseRelayUrl('rels://[2001:db8::1]:28443'),
    { host: '2001:db8::1', port: 28443 }));
  check('bracketed ipv6 default port', deepEq(mod.parseRelayUrl('rels://[2001:db8::1]'),
    { host: '2001:db8::1', port: 443 }));
  check('reject other scheme', mod.parseRelayUrl('https://relay.example:443') === null);
  check('reject path', mod.parseRelayUrl('rels://relay.example/relay') === null);
  check('reject query', mod.parseRelayUrl('rels://relay.example?token=x') === null);
  check('reject userinfo', mod.parseRelayUrl('rels://user@relay.example') === null);
  check('reject empty host', mod.parseRelayUrl('rels://') === null);
  check('reject port zero', mod.parseRelayUrl('rels://relay.example:0') === null);
  check('reject port over 16 bit', mod.parseRelayUrl('rels://relay.example:65536') === null);
  check('reject non-numeric port', mod.parseRelayUrl('rels://relay.example:abc') === null);
  check('reject non-ascii host', mod.parseRelayUrl('rels://rel\u00e4y.example') === null);
  check('reject missing separator', mod.parseRelayUrl('relay.example:28443') === null);

  // ---- buildExcludedRouteEntries --------------------------------------------
  const gw = '10.99.0.1';
  const multi = mod.buildExcludedRouteEntries([
    { kind: 'management', host: 'api.netcenter.alfadb.cn', ok: true, address: '203.0.113.7' },
    { kind: 'signal', host: 'signal.netcenter.alfadb.cn', ok: true, address: '203.0.113.9' }
  ], gw);
  check('multi-host yields two routes', multi.routes.length === 2 && multi.resolvedCount === 2,
    JSON.stringify(multi));
  check('multi-host route shape', deepEq(multi.routes[0], {
    destination: '203.0.113.7', prefixLength: 32, gateway: gw, hasGateway: true,
    isDefaultRoute: false, isExcludedRoute: true
  }));
  check('multi-host no failures', multi.failed.length === 0 && multi.duplicatesRemoved === 0);

  const fam = mod.buildExcludedRouteEntries([
    { kind: 'relay', host: 'home.alfadb.cn', ok: true, address: '198.51.100.4' },
    { kind: 'relay', host: 'v6.example', ok: true, address: '2001:db8::10' }
  ], gw);
  check('ipv4 gets /32', fam.routes[0].prefixLength === 32);
  check('ipv6 gets /128', fam.routes[1].prefixLength === 128 &&
    fam.routes[1].destination === '2001:db8::10');

  const dedup = mod.buildExcludedRouteEntries([
    { kind: 'management', host: 'api.example', ok: true, address: '203.0.113.7' },
    { kind: 'signal', host: 'api.example', ok: true, address: '203.0.113.7' }
  ], gw);
  check('same ip deduped across hosts', dedup.routes.length === 1 &&
    dedup.duplicatesRemoved === 1 && dedup.resolvedCount === 1);

  const withFail = mod.buildExcludedRouteEntries([
    { kind: 'relay', host: 'home.alfadb.cn', ok: false, address: '' },
    { kind: 'signal', host: 'signal.example', ok: true, address: '203.0.113.9' }
  ], gw);
  check('failed resolution excluded from routes', withFail.routes.length === 1);
  check('failed resolution reported', withFail.failed.length === 1 &&
    withFail.failed[0].host === 'home.alfadb.cn');

  const badShape = mod.buildExcludedRouteEntries([
    { kind: 'relay', host: 'home.alfadb.cn', ok: true, address: 'not-an-ip' }
  ], gw);
  check('non-literal address treated as failed', badShape.routes.length === 0 &&
    badShape.failed.length === 1);

  // ---- isExclusionFailClosed -------------------------------------------------
  const failRes = mod.buildExcludedRouteEntries(
    [{ kind: 'relay', host: 'home.alfadb.cn', ok: false, address: '' }], gw);
  const okRes = mod.buildExcludedRouteEntries(
    [{ kind: 'relay', host: 'home.alfadb.cn', ok: true, address: '198.51.100.4' }], gw);
  check('fail-closed when relay required + failed', mod.isExclusionFailClosed(failRes, true));
  check('skip allowed when relay not required', !mod.isExclusionFailClosed(failRes, false));
  check('no fail-closed when nothing failed', !mod.isExclusionFailClosed(okRes, true));

  // ---- mergeExcludedRouteEntries (LAN coexistence) ----------------------------
  const lanEntry = {
    destination: '192.168.50.0', prefixLength: 24, gateway: gw, hasGateway: true,
    isDefaultRoute: false, isExcludedRoute: true
  };
  const defaultEntry = {
    destination: '0.0.0.0', prefixLength: 0, gateway: gw, hasGateway: true,
    isDefaultRoute: true, isExcludedRoute: false
  };
  const derived = mod.buildExcludedRouteEntries([
    { kind: 'management', host: 'api.example', ok: true, address: '203.0.113.7' },
    { kind: 'relay', host: 'home.example', ok: true, address: '198.51.100.4' }
  ], gw).routes;
  const merged = mod.mergeExcludedRouteEntries([lanEntry, defaultEntry], derived);
  check('LAN exclusion kept verbatim', deepEq(merged[0], lanEntry));
  check('non-excluded entries untouched', deepEq(merged[1], defaultEntry));
  check('derived appended after existing', merged.length === 4 &&
    merged[2].destination === '203.0.113.7' && merged[3].destination === '198.51.100.4');
  const noDup = mod.mergeExcludedRouteEntries(
    [lanEntry, { destination: '203.0.113.7', prefixLength: 32, gateway: gw, hasGateway: true,
      isDefaultRoute: false, isExcludedRoute: true }], derived);
  check('derived duplicate of existing skipped', noDup.length === 3);
} catch (error) {
  failedCount++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(fixtureDir, { recursive: true, force: true });
}

console.log(`\n${passed} passed, ${failedCount} failed`);
process.exit(failedCount === 0 ? 0 : 1);
