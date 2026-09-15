// Route-entry validation + pre-create truth log self-check (host-side, no
// device, no network, no build). H1 diagnosis:
// docs/vpn-create-401-diagnosis-20260915.md §3.2/§4/§5.
//
// Same principle as run.mts / run-relay-flag.mts: the REAL production
// modules are copied byte-for-byte into OS-temp fixtures (.ts) at run time —
// the code under test is the shipped code, never a hand-held mirror:
//   Part A  NetBirdRouteValidation.ets   (pure module, zero imports)
//   Part B  NetBirdVpnConfig.ets         (+ real siblings NetBirdConnector /
//           NetBirdEndpointExclusion; @ohos.* / libnetbird_core.so are
//           short-circuited to stubs by route-check-hooks.mjs — the tested
//           functions never touch them)
//
// Coverage (task mapping):
//   T1  valid entries -> validator ok + platform VpnConfig byte-identical to
//       the hand-derived baseline (per-field + key-set assertions); no
//       fail-closed. Pins "valid path unchanged" incl. the new `source`
//       provenance tag never leaking into the platform object.
//   T2  isDefaultRoute missing -> fail-closed, marker field=isDefaultRoute
//       kind=missing (incl. END-TO-END via a core snapshot JSON.parsed
//       WITHOUT the is_default key).
//   T3  prefixLength NaN -> kind=nan (end-to-end via network 'a.b.c.d/xyz');
//       Infinity -> range; string -> type; null -> missing.
//   T4  gateway missing -> kind=missing.
//   T5  family=3 -> kind=range.
//   T6  one valid + one violating entry -> the WHOLE config fails
//       (ok=false, nothing dropped/skipped: the merged list is untouched).
//   T7  truth-log field completeness: VPN_ROUTE_TABLE header + per-entry
//       VPN_ROUTE_ENTRY lines carry index/source/dest/prefix/family/
//       isDefaultRoute/isExcludedRoute/hasGateway/interface; raw undefined/
//       NaN stay visible; formatRouteViolation exact marker shape.
//
// Run: node client/entry/selfcheck/run-route-validation.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./route-check-hooks.mjs', import.meta.url).href);

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_DIR = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability');

let passed: number = 0;
let failedCount: number = 0;

function check(name: string, condition: boolean, detail?: string): void {
  if (condition) {
    passed++;
    console.log('ok   ' + name);
  } else {
    failedCount++;
    console.log('FAIL ' + name + (detail !== undefined ? ' — ' + detail : ''));
  }
}

function deepEq(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function fixtureDir(prefix: string): string {
  return mkdtempSync(join(tmpdir(), prefix));
}

// Copy one real .ets as a byte-identical .ts fixture.
function stage(dir: string, name: string): void {
  writeFileSync(join(dir, name + '.ts'), readFileSync(join(REAL_DIR, name + '.ets')));
}

try {
  // ============================ Part A: pure validator =====================
  const dirA = fixtureDir('nb-route-validation-');
  stage(dirA, 'NetBirdRouteValidation');
  const v = await import(pathToFileURL(join(dirA, 'NetBirdRouteValidation.ts')).href);

  const iface = 'vpn-tun';
  const plain = { destination: '10.99.0.0', prefixLength: 24, gateway: '10.99.0.1',
    hasGateway: true, isDefaultRoute: false, isExcludedRoute: false };
  const def = { destination: '0.0.0.0', prefixLength: 0, gateway: '10.99.0.1',
    hasGateway: true, isDefaultRoute: true, isExcludedRoute: false };
  const lan = { destination: '192.168.50.0', prefixLength: 24, gateway: '10.99.0.1',
    hasGateway: true, isDefaultRoute: false, isExcludedRoute: true };

  // T1: valid entries (MR4 template shapes) -> ok, zero violations, no mutation
  const validSet = [
    { ...plain }, { ...def }, { ...lan },
    { destination: '172.16.0.0', prefixLength: 12, gateway: '10.99.0.1',
      hasGateway: true, isDefaultRoute: false, isExcludedRoute: false,
      source: 'managed' }
  ];
  const beforeJson = JSON.stringify(validSet);
  const okResult = v.validateRouteEntries(validSet, iface);
  check('T1 valid entries -> ok', okResult.ok === true && okResult.violations.length === 0,
    JSON.stringify(okResult));
  check('T1 validator is read-only (no mutation/repair)',
    JSON.stringify(validSet) === beforeJson, JSON.stringify(validSet));
  check('T1 source classification managed/exclusion/default/base',
    v.routeEntrySource(validSet[3]) === 'managed' && v.routeEntrySource(lan) === 'exclusion' &&
    v.routeEntrySource(def) === 'default' && v.routeEntrySource(plain) === 'base');

  // T2: isDefaultRoute missing (the H1 passthrough shape) -> missing
  const noDefault = [{ destination: '172.16.10.0', prefixLength: 24,
    gateway: '10.99.0.1', hasGateway: true, isExcludedRoute: false,
    source: 'managed' }];
  const r2 = v.validateRouteEntries(noDefault, iface);
  check('T2 missing isDefaultRoute -> fail-closed', r2.ok === false, JSON.stringify(r2));
  check('T2 marker index/source/field/kind',
    deepEq(r2.violations[0],
      { index: 0, source: 'managed', field: 'isDefaultRoute', kind: 'missing' }),
    JSON.stringify(r2.violations));
  const baseNoDefault = [{ destination: '10.20.0.0', prefixLength: 16,
    gateway: '10.99.0.1', hasGateway: true, isExcludedRoute: false }];
  const r2b = v.validateRouteEntries(baseNoDefault, iface);
  check('T2 untagged entry classifies source=base',
    r2b.ok === false && r2b.violations[0].source === 'base', JSON.stringify(r2b));

  // T3: prefixLength NaN / Infinity / string / null
  const r3nan = v.validateRouteEntries(
    [{ ...plain, prefixLength: NaN }], iface);
  check('T3 prefixLength NaN -> kind=nan',
    r3nan.ok === false && r3nan.violations[0].field === 'prefixLength' &&
    r3nan.violations[0].kind === 'nan', JSON.stringify(r3nan));
  const r3inf = v.validateRouteEntries(
    [{ ...plain, prefixLength: Infinity }], iface);
  check('T3 prefixLength Infinity -> kind=range',
    r3inf.violations[0].kind === 'range', JSON.stringify(r3inf));
  const r3str = v.validateRouteEntries(
    [{ ...plain, prefixLength: '24' }], iface);
  check('T3 prefixLength string -> kind=type',
    r3str.violations[0].kind === 'type', JSON.stringify(r3str));
  const r3null = v.validateRouteEntries(
    [{ ...plain, prefixLength: null }], iface);
  check('T3 prefixLength null -> kind=missing',
    r3null.violations[0].kind === 'missing', JSON.stringify(r3null));

  // T4: gateway missing / wrong type / empty
  const r4 = v.validateRouteEntries([{ ...plain, gateway: undefined }], iface);
  check('T4 gateway missing -> kind=missing',
    r4.ok === false && r4.violations[0].field === 'gateway' &&
    r4.violations[0].kind === 'missing', JSON.stringify(r4));
  const r4t = v.validateRouteEntries([{ ...plain, gateway: 5 }], iface);
  check('T4 gateway non-string -> kind=type',
    r4t.violations[0].field === 'gateway' && r4t.violations[0].kind === 'type',
    JSON.stringify(r4t));
  const r4e = v.validateRouteEntries([{ ...plain, gateway: '' }], iface);
  check('T4 gateway empty string -> kind=range',
    r4e.violations[0].kind === 'range', JSON.stringify(r4e));

  // T5: family, when present, must be 1 or 2
  const r5 = v.validateRouteEntries([{ ...plain, family: 3 }], iface);
  check('T5 family=3 -> kind=range',
    r5.ok === false && r5.violations[0].field === 'family' &&
    r5.violations[0].kind === 'range', JSON.stringify(r5));
  const r5t = v.validateRouteEntries([{ ...plain, family: '1' }], iface);
  check('T5 family string -> kind=type',
    r5t.violations[0].field === 'family' && r5t.violations[0].kind === 'type',
    JSON.stringify(r5t));
  const r5ok = v.validateRouteEntries([{ ...plain, family: 2 }], iface);
  check('T5 family 1|2 accepted, absent accepted',
    r5ok.ok === true && v.validateRouteEntries([plain], iface).ok === true,
    JSON.stringify(r5ok));

  // T6: mixed valid + violating -> WHOLE config fails (no silent skip)
  const mixed = [plain, { destination: '10.60.0.0', prefixLength: 24,
    gateway: '10.99.0.1', hasGateway: true, isExcludedRoute: false,
    source: 'managed' }];
  const r6 = v.validateRouteEntries(mixed, iface);
  check('T6 mixed config fails as a whole (ok=false)',
    r6.ok === false, JSON.stringify(r6));
  check('T6 violation points at the offending entry only',
    r6.violations.length === 1 && r6.violations[0].index === 1 &&
    r6.violations[0].field === 'isDefaultRoute' && r6.violations[0].kind === 'missing',
    JSON.stringify(r6.violations));
  check('T6 entries untouched (no drop/repair of the invalid one)',
    mixed.length === 2 && JSON.stringify(mixed[1]).indexOf('isDefaultRoute') === -1,
    JSON.stringify(mixed));

  // extras: optional isExcludedRoute + required destination + config interface
  const rOpt = v.validateRouteEntries([{ ...plain, isExcludedRoute: undefined }], iface);
  check('optional isExcludedRoute absent accepted',
    rOpt.ok === true, JSON.stringify(rOpt));
  const rOptBad = v.validateRouteEntries([{ ...plain, isExcludedRoute: 'yes' }], iface);
  check('optional isExcludedRoute non-boolean -> type',
    rOptBad.violations[0].field === 'isExcludedRoute' &&
    rOptBad.violations[0].kind === 'type', JSON.stringify(rOptBad));
  const rDest = v.validateRouteEntries([{ ...plain, destination: '' }], iface);
  check('empty destination -> range',
    rDest.violations[0].field === 'destination' && rDest.violations[0].kind === 'range',
    JSON.stringify(rDest));
  const rIface = v.validateRouteEntries([plain], '');
  check('empty interface name -> config-level violation index=-1',
    rIface.ok === false && deepEq(rIface.violations[0],
      { index: -1, source: 'config', field: 'interface', kind: 'range' }),
    JSON.stringify(rIface));

  // T7: truth-log completeness + raw-truth rendering + exact marker shape
  const t7Entries = [
    { ...def }, { ...lan },
    { destination: '172.16.10.0', prefixLength: NaN, gateway: '10.99.0.1',
      hasGateway: true, isExcludedRoute: false, source: 'managed' }
  ];
  const lines: string[] = v.formatRouteTable('req-1', iface, t7Entries);
  check('T7 header line shape',
    lines.length === 4 && lines[0] === 'VPN_ROUTE_TABLE|requestId=req-1|count=3|excluded=1',
    JSON.stringify(lines));
  const all = lines.join('\n');
  check('T7 every entry line carries all documented fields',
    lines.slice(1).every((l: string): boolean =>
      l.indexOf('VPN_ROUTE_ENTRY|requestId=req-1|') === 0 &&
      l.indexOf('|index=') > 0 && l.indexOf('|source=') > 0 &&
      l.indexOf('|interface=vpn-tun|') > 0 && l.indexOf('|dest=') > 0 &&
      l.indexOf('|prefix=') > 0 && l.indexOf('|family=') > 0 &&
      l.indexOf('|isDefaultRoute=') > 0 && l.indexOf('|isExcludedRoute=') > 0 &&
      l.indexOf('|hasGateway=') > 0),
    all);
  check('T7 raw truth stays visible (NaN/undefined not defaulted)',
    lines[3].indexOf('|prefix=NaN|') > 0 &&
    lines[3].indexOf('|isDefaultRoute=undefined|') > 0, lines[3]);
  check('T7 source classes rendered',
    lines[1].indexOf('|source=default|') > 0 &&
    lines[2].indexOf('|source=exclusion|') > 0 &&
    lines[3].indexOf('|source=managed|') > 0, all);
  check('T7 absent isExcludedRoute renders effective false',
    lines[1].indexOf('|isExcludedRoute=false|') > 0 &&
    lines[3].indexOf('|isExcludedRoute=false|') > 0, all);
  const dirty = v.formatRouteTable('req|2', 'vpn|tun',
    [{ ...plain, destination: '10.0.0.0|faked=1' }]);
  check('T7 pipe sanitization (no log-format injection)',
    dirty[0].indexOf('requestId=req/2') > 0 &&
    dirty[1].indexOf('|dest=10.0.0.0/faked=1|') > 0 &&
    dirty[1].indexOf('|interface=vpn/tun|') > 0, JSON.stringify(dirty));
  check('T7 violation marker exact shape',
    v.formatRouteViolation('req-1',
      { index: 2, source: 'managed', field: 'prefixLength', kind: 'nan' }) ===
    'VPN_ROUTE_ENTRY_INVALID|requestId=req-1|index=2|source=managed|' +
    'field=prefixLength|kind=nan');

  // =============== Part B: real mapper pipeline over real siblings =========
  const dirB = fixtureDir('nb-vpnconfig-mapper-');
  stage(dirB, 'NetBirdVpnConfig');
  stage(dirB, 'NetBirdEndpointExclusion');
  // linker plumbing (see exclusion-dep.stub.ts + route-check-hooks.mjs):
  // re-exports the real fixture + shims the erased interface name.
  writeFileSync(join(dirB, 'NetBirdEndpointExclusion.dep.ts'),
    readFileSync(join(HERE, 'exclusion-dep.stub.ts')));
  const cfg = await import(pathToFileURL(join(dirB, 'NetBirdVpnConfig.ts')).href);

  // T1 (mapper): valid snapshot -> platform VpnConfig byte-identical baseline
  const derived32 = { destination: '203.0.113.7', prefixLength: 32,
    gateway: '10.99.0.1', hasGateway: true, isDefaultRoute: false,
    isExcludedRoute: true };
  const base = cfg.withEndpointExclusions(cfg.loadVpnTunnelConfig(), [derived32]);
  const snapshot = JSON.parse('{"available":true,"serial":7,"address":"10.99.0.1",' +
    '"address_prefix_len":32,"routes":[' +
    '{"network":"172.16.0.0/12","is_default":false},' +
    '{"network":"0.0.0.0/0","is_default":true}],' +
    '"dns":{"service_enable":true,"servers":[{"ip":"8.8.8.8","port":53}]}}') as never;
  const merged = cfg.applyNetworkConfig(base, snapshot);
  check('T1 merged route list matches baseline (managed tagged, exclusions kept)',
    deepEq(merged.routes, [
      { destination: '172.16.0.0', prefixLength: 12, gateway: '10.99.0.1',
        hasGateway: true, isDefaultRoute: false, isExcludedRoute: false,
        source: 'managed' },
      { destination: '0.0.0.0', prefixLength: 0, gateway: '10.99.0.1',
        hasGateway: true, isDefaultRoute: true, isExcludedRoute: false,
        source: 'managed' },
      { destination: '192.168.50.0', prefixLength: 24, gateway: '10.99.0.1',
        hasGateway: true, isDefaultRoute: false, isExcludedRoute: true },
      { destination: '203.0.113.7', prefixLength: 32, gateway: '10.99.0.1',
        hasGateway: true, isDefaultRoute: false, isExcludedRoute: true }
    ]), JSON.stringify(merged.routes));
  const platform = cfg.buildPlatformVpnConfig(merged);
  const expectedPlatform =
    '{"addresses":[{"address":{"address":"10.99.0.1","family":1},"prefixLength":32}],' +
    '"routes":[' +
    '{"interface":"vpn-tun","destination":{"address":{"address":"172.16.0.0","family":1},' +
    '"prefixLength":12},"gateway":{"address":"10.99.0.1","family":1},' +
    '"hasGateway":true,"isDefaultRoute":false},' +
    '{"interface":"vpn-tun","destination":{"address":{"address":"0.0.0.0","family":1},' +
    '"prefixLength":0},"gateway":{"address":"10.99.0.1","family":1},' +
    '"hasGateway":true,"isDefaultRoute":true},' +
    '{"interface":"vpn-tun","destination":{"address":{"address":"192.168.50.0","family":1},' +
    '"prefixLength":24},"gateway":{"address":"10.99.0.1","family":1},' +
    '"hasGateway":true,"isDefaultRoute":false,"isExcludedRoute":true},' +
    '{"interface":"vpn-tun","destination":{"address":{"address":"203.0.113.7","family":1},' +
    '"prefixLength":32},"gateway":{"address":"10.99.0.1","family":1},' +
    '"hasGateway":true,"isDefaultRoute":false,"isExcludedRoute":true}],' +
    '"dnsAddresses":["8.8.8.8"],"mtu":1400}';
  check('T1 platform VpnConfig byte-identical to baseline',
    JSON.stringify(platform) === expectedPlatform, JSON.stringify(platform));
  check('T1 provenance tag never leaks into platform routes',
    platform.routes.every((r: Record<string, unknown>): boolean =>
      !('source' in r) && Object.keys(r).length === (r['isExcludedRoute'] !== undefined ? 6 : 5)),
    JSON.stringify(platform.routes.map((r: Record<string, unknown>): string[] =>
      Object.keys(r))));

  // T2/T3 end-to-end: core snapshot JSON passthrough -> validator verdict
  const snapMissing = JSON.parse('{"available":true,"address":"10.99.0.1",' +
    '"address_prefix_len":32,"routes":[{"network":"172.16.10.0/24"}],"dns":null}') as never;
  const mergedMissing = cfg.applyNetworkConfig(base, snapMissing);
  const vMissing = v.validateRouteEntries(mergedMissing.routes, mergedMissing.interfaceName);
  check('T2 end-to-end: snapshot without is_default -> isDefaultRoute missing, fail-closed',
    vMissing.ok === false && deepEq(vMissing.violations,
      [{ index: 0, source: 'managed', field: 'isDefaultRoute', kind: 'missing' }]),
    JSON.stringify(vMissing));
  const snapBadPrefix = JSON.parse('{"available":true,"routes":' +
    '[{"network":"10.9.9.0/xyz","is_default":false}]}') as never;
  const mergedBad = cfg.applyNetworkConfig(base, snapBadPrefix);
  const vBad = v.validateRouteEntries(mergedBad.routes, mergedBad.interfaceName);
  check('T3 end-to-end: non-numeric prefix segment -> prefixLength NaN, kind=nan',
    vBad.ok === false && vBad.violations.length === 1 &&
    vBad.violations[0].field === 'prefixLength' && vBad.violations[0].kind === 'nan',
    JSON.stringify(vBad));

  // T6 end-to-end: valid + violating in ONE snapshot -> whole config rejected
  const snapMixed = JSON.parse('{"available":true,"routes":[' +
    '{"network":"10.50.0.0/16","is_default":false},' +
    '{"network":"10.60.0.0/24"}]}') as never;
  const mergedMixed = cfg.applyNetworkConfig(base, snapMixed);
  const vMixed = v.validateRouteEntries(mergedMixed.routes, mergedMixed.interfaceName);
  check('T6 end-to-end: mixed snapshot fails whole config (ok=false)',
    vMixed.ok === false && vMixed.violations.length === 1 &&
    vMixed.violations[0].index === 1, JSON.stringify(vMixed));
  check('T6 end-to-end: no entry silently skipped (list kept intact for the dump)',
    mergedMixed.routes.length === 4 &&
    mergedMixed.routes[0].destination === '10.50.0.0' &&
    mergedMixed.routes[1].destination === '10.60.0.0',
    JSON.stringify(mergedMixed.routes));

  // fallback path unchanged: no snapshot -> base EXCLUDED entries kept
  // verbatim (existing behavior: managed list empty leaves only the base
  // isExcludedRoute entries — diagnosis §6-3), everything untagged.
  const base2 = cfg.withEndpointExclusions(cfg.loadVpnTunnelConfig(), []);
  const mergedFallback = cfg.applyNetworkConfig(base2,
    JSON.parse('{"available":false}') as never);
  check('fallback keeps base exclusion entries verbatim (same object references)',
    mergedFallback.routes.length === 1 &&
    mergedFallback.routes[0] === base2.routes[2],
    JSON.stringify(mergedFallback.routes));
  check('fallback classification exclusion',
    v.routeEntrySource(mergedFallback.routes[0]) === 'exclusion',
    JSON.stringify(mergedFallback.routes));
  const gated = cfg.gateDefaultRoutes(merged, false);
  check('gateDefaultRoutes strips only isDefaultRoute entries; rest stays valid',
    gated.routes.length === 3 && v.validateRouteEntries(gated.routes, gated.interfaceName).ok === true,
    JSON.stringify(gated.routes));
} catch (error) {
  failedCount++;
  console.log('FAIL harness — ' + (error as Error).message);
}

console.log('');
console.log(String(passed) + ' passed, ' + String(failedCount) + ' failed');
process.exit(failedCount === 0 ? 0 : 1);
