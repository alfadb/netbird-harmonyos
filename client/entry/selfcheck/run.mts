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

  // ---- v3.1 dns branch order (preregistration §2-C1 dns / R-4) --------------
  // The ONE branch order: default network first; the all-nets filtered branch
  // ONLY when the default network cannot prove physical attribution; anything
  // else is a RECORDED gap (never a silent pick, never a post-hoc choice).
  const probe = (id: number, networkCap: number[], bearerTypes: number[],
    dnses: unknown[], capsOk = true, propertiesOk = true): unknown => ({
    handleId: id, capsOk: capsOk, networkCap: networkCap, bearerTypes: bearerTypes,
    propertiesOk: propertiesOk, dnses: dnses
  });
  const NOT_VPN: number = mod.NET_CAPABILITY_NOT_VPN;
  const VPN_BEARER: number = mod.BEARER_VPN;
  check('capability constants are the platform literals',
    NOT_VPN === 15 && VPN_BEARER === 4 && mod.DNS_RESOLVER_PORT_DEFAULT === 53);
  check('physical evidence requires the NOT_VPN capability EXPLICITLY',
    mod.isPhysicalNetEvidence(probe(101, [12, NOT_VPN], [1], [])) &&
    !mod.isPhysicalNetEvidence(probe(101, [12], [1], [])) &&
    !mod.isPhysicalNetEvidence(probe(0, [NOT_VPN], [1], [])) &&
    !mod.isPhysicalNetEvidence(probe(101, [NOT_VPN], [VPN_BEARER], [])));

  // D1: step 2 holds -> the DEFAULT branch, its resolvers, verbatim markers
  const d1 = mod.deriveDnsEndpointExclusions({
    defaultNet: probe(101, [12, NOT_VPN], [1],
      [{ address: '203.0.113.53', family: 1, port: 53 }, { address: '203.0.113.54' }]),
    allNets: [probe(202, [NOT_VPN], [1], [{ address: '198.51.100.9' }])],
    allNetsRead: true, apiError: ''
  });
  check('D1 default branch taken when step 2 holds', d1.branch === 'default' && !d1.gap);
  check('D1 resolvers are the default network\'s, IPv4 only, port defaulted',
    deepEq(d1.endpoints, [{ kind: 'dns', host: '203.0.113.53', port: 53 },
      { kind: 'dns', host: '203.0.113.54', port: 53 }]), JSON.stringify(d1));
  check('D1 notes carry the branch and the verbatim grounds',
    deepEq(d1.notes, ['dns-branch=default',
      'dns-physical-evidence=handle=101;networkCap-has-NOT_VPN=15;' +
      'bearerTypes-no-BEARER_VPN=4']), JSON.stringify(d1.notes));
  check('D1 the all-nets resolvers are NOT mixed in (no branch picking)',
    d1.endpoints.every((e: Record<string, unknown>): boolean => e['host'] !== '198.51.100.9'));

  // D2: step 2 does not hold (no NOT_VPN) -> the UNIQUE all-nets branch
  const d2 = mod.deriveDnsEndpointExclusions({
    defaultNet: probe(101, [12], [1], [{ address: '203.0.113.53' }]),
    allNets: [probe(202, [12], [1], [{ address: '192.0.2.1' }]),
      probe(303, [NOT_VPN], [1], [{ address: '198.51.100.9', family: 1, port: 5353 }])],
    allNetsRead: true, apiError: ''
  });
  check('D2 step 2 fails -> all-nets filtered branch', d2.branch === 'all-nets-filtered' &&
    !d2.gap);
  check('D2 only the qualifying handle\'s resolvers are used',
    deepEq(d2.endpoints, [{ kind: 'dns', host: '198.51.100.9', port: 5353 }]),
    JSON.stringify(d2));
  check('D2 notes name the branch and the qualifying handle verbatim',
    deepEq(d2.notes, ['dns-branch=all-nets-filtered',
      'dns-physical-evidence=handle=303;networkCap-has-NOT_VPN=15;' +
      'bearerTypes-no-BEARER_VPN=4']), JSON.stringify(d2.notes));

  // D2b: a VPN bearer type on the default handle disqualifies step 2 as well
  const d2b = mod.deriveDnsEndpointExclusions({
    defaultNet: probe(101, [NOT_VPN], [VPN_BEARER], [{ address: '203.0.113.53' }]),
    allNets: [probe(404, [NOT_VPN], [1], [{ address: '203.0.113.60' }])],
    allNetsRead: true, apiError: ''
  });
  check('D2b bearerTypes containing BEARER_VPN disqualifies the default network',
    d2b.branch === 'all-nets-filtered' &&
    deepEq(d2b.endpoints, [{ kind: 'dns', host: '203.0.113.60', port: 53 }]),
    JSON.stringify(d2b));

  // D3: neither path proves physical attribution -> gap (no branch record)
  const d3 = mod.deriveDnsEndpointExclusions({
    defaultNet: mod.emptyDnsNetProbe(), allNets: [], allNetsRead: true, apiError: ''
  });
  check('D3 both paths fail -> dns gap with no branch', d3.branch === 'none' && d3.gap &&
    d3.gapReason === 'no-physical-net' && d3.endpoints.length === 0, JSON.stringify(d3));
  check('D3 the gap is a verbatim note',
    deepEq(d3.notes, ['dns-branch=none', 'dns-physical-evidence=none',
      'dns-gap=no-physical-net']), JSON.stringify(d3.notes));

  // D4: empty resolver list on the chosen branch -> gap, branch still recorded
  const d4 = mod.deriveDnsEndpointExclusions({
    defaultNet: probe(101, [NOT_VPN], [1], []), allNets: [], allNetsRead: false, apiError: ''
  });
  check('D4 empty dnses -> dnses-empty gap with the branch recorded',
    d4.branch === 'default' && d4.gap && d4.gapReason === 'dnses-empty' &&
    deepEq(d4.notes, ['dns-branch=default',
      'dns-physical-evidence=handle=101;networkCap-has-NOT_VPN=15;' +
      'bearerTypes-no-BEARER_VPN=4', 'dns-gap=dnses-empty']), JSON.stringify(d4));

  // D5: R-3 is strictly IPv4 — an IPv6 resolver is a gap, never a /128 freeze
  const d5 = mod.deriveDnsEndpointExclusions({
    defaultNet: probe(101, [NOT_VPN], [1],
      [{ address: '2001:db8::53', family: 2 }, { address: '198.51.100.9' }]),
    allNets: [], allNetsRead: false, apiError: ''
  });
  check('D5 IPv6 resolver -> recorded gap, usable IPv4 resolver kept',
    d5.gap && d5.gapReason === 'dnses-not-ipv4' &&
    deepEq(d5.endpoints, [{ kind: 'dns', host: '198.51.100.9', port: 53 }]),
    JSON.stringify(d5));
  const d5b = mod.deriveDnsEndpointExclusions({
    defaultNet: probe(101, [NOT_VPN], [1], [{ address: '2001:db8::53', family: 2 }]),
    allNets: [], allNetsRead: false, apiError: ''
  });
  check('D5b IPv6-only -> gap with no endpoints',
    d5b.gap && d5b.gapReason === 'dnses-not-ipv4' && d5b.endpoints.length === 0,
    JSON.stringify(d5b));

  // D6: an API error is preserved verbatim (and outranks the other reasons)
  const d6 = mod.deriveDnsEndpointExclusions({
    defaultNet: mod.emptyDnsNetProbe(),
    allNets: [probe(505, [NOT_VPN], [1], [{ address: '198.51.100.9' }])],
    allNetsRead: true, apiError: 'api-error-code-201'
  });
  check('D6 api error -> gap token carried verbatim, resolvers still frozen',
    d6.gap && d6.gapReason === 'api-error-code-201' && d6.branch === 'all-nets-filtered' &&
    deepEq(d6.endpoints, [{ kind: 'dns', host: '198.51.100.9', port: 53 }]),
    JSON.stringify(d6));

  // D7: an unreadable properties leg is a gap too (never an invented list)
  const d7 = mod.deriveDnsEndpointExclusions({
    defaultNet: probe(101, [NOT_VPN], [1], [], true, false),
    allNets: [], allNetsRead: false, apiError: ''
  });
  check('D7 unread resolver properties -> api-error gap, no endpoints',
    d7.gap && d7.gapReason === 'api-error' && d7.endpoints.length === 0 &&
    d7.branch === 'default', JSON.stringify(d7));
  const d7b = mod.deriveDnsEndpointExclusions({
    defaultNet: mod.emptyDnsNetProbe(), allNets: [], allNetsRead: false, apiError: ''
  });
  check('D7b no qualifying network at all -> no-physical-net gap (a failed all-nets read ' +
    'is reported by the caller as apiError, see D6)',
    d7b.gap && d7b.gapReason === 'no-physical-net' && d7b.branch === 'none',
    JSON.stringify(d7b));
} catch (error) {
  failedCount++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(fixtureDir, { recursive: true, force: true });
}

console.log(`\n${passed} passed, ${failedCount} failed`);
process.exit(failedCount === 0 ? 0 : 1);
