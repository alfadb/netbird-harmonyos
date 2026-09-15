// Exclusion address-shape self-check (host-side, no device, no network, no
// build). Regression for the create-401 single-variable fix: the management
// dial resolution ('IP:port' — resolveManagementIp's return shape, consumed
// verbatim by the connector start) was passed UNSTRIPPED into the endpoint
// exclusion derivation, so classifyAddress's 'contains ":" -> ipv6' rule
// froze it as a /128 family=2 route entry the platform rejects at
// VpnConnection.create (401 Parameter error, ms after VPN_CREATE_BEGIN).
// Fix contract under test:
//   - source: the derivation call site passes stripDialPort(connectAddr) —
//     the exclusion `address` field is a BARE IP, same contract as the
//     signal branch's dialIp (addr.slice(0, addr.lastIndexOf(':'))),
//   - defense: NetBirdEndpointExclusion recognizes the 'IPv4:port' dial
//     shape (stripDialPort), so a future unstripped caller can never
//     re-render a /128 family=2 entry; genuine IPv6 literals (multiple ':')
//     and bracket forms ('[v6]', '[v6]:port') are never touched.
// All IPs are RFC 5737 documentation values — no real endpoints, no
// credentials.
//
// Principle of run.mts / run-relay-exclusion.mts: the REAL production files
// are copied byte-for-byte into OS-temp fixtures (.ts) at run time — the
// code under test is the shipped code, never a hand-held mirror:
//   A1  REAL NetBirdVpnExtensionAbility.deriveEndpointExclusions() fed the
//       pre-fix 'IP:port' dial shape + REAL NetBirdVpnConfig platform mapper
//   A2  REAL NetBirdEndpointExclusion stripDialPort + classifyAddress
//   A3  REAL buildExcludedRouteEntries end-to-end
//   A4  REAL NetBirdConnector resolveManagementIp (dial contract unchanged)
//   A5  wiring pins against the REAL shipped sources (the start flow cannot
//       be driven host-side — sockets/native — so the call site is pinned
//       textually on the byte-identical source, deterministically)
// @ohos.* / libnetbird_core.so are functional stubs injected by
// relay-exclusion-hooks.mjs (recording hilog, fake DNS table, fake
// connector_status JSON). Fixtures are never rewritten.
//
// Run: node client/entry/selfcheck/run-exclusion-addr-shape.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./relay-exclusion-hooks.mjs', import.meta.url).href);

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_DIR = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability');

// The five real modules staged byte-identically (code under test = shipped
// code), and the erased interface names each shim must re-declare for the
// ESM link (see relay-exclusion-hooks.mjs) — same staging as
// run-relay-exclusion.mts.
const STAGED = ['NetBirdVpnExtensionAbility', 'NetBirdConnector',
  'NetBirdVpnConfig', 'NetBirdRouteValidation', 'NetBirdEndpointExclusion'];
const SHIM_TYPES: Record<string, string[]> = {
  NetBirdConnector: ['ConnectorStatusResult', 'CoreNetworkConfig', 'CoreRouteEntry',
    'ConnectorStartResult', 'DeviceConfigFile', 'ManagementEndpoint',
    'NativeTcpSocketOpen', 'RelayAdvertisedView', 'RouteSetAppliedResult',
    'SocketFeedResult', 'WgFeedResult'],
  NetBirdEndpointExclusion: ['ExcludedRouteEntry', 'EndpointResolution',
    'ExclusionBuildResult', 'ExclusionEndpoint', 'RelayEndpoint'],
  NetBirdVpnConfig: ['VpnRouteConfig', 'VpnTunnelConfig'],
  NetBirdRouteValidation: ['RouteCheckResult', 'RouteViolation']
};

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

const G: Record<string, unknown> = globalThis as unknown as Record<string, unknown>;

function setStatus(status: unknown): void {
  G['__NB_FAKE_STATUS'] = JSON.stringify(status);
}

function setDns(table: Record<string, string>): void {
  G['__NB_DNS'] = table;
}

function logs(): string[] {
  return (G['__NB_LOG'] as string[]) ?? [];
}

function linesWith(prefix: string): string[] {
  // The recording stub prefixes each line with '[<lvl>] ' — strip it so the
  // assertions match the REAL hilog format strings verbatim.
  return logs().filter((l: string): boolean => l.indexOf(prefix) >= 0)
    .map((l: string): string => l.slice(l.indexOf(prefix)));
}

function freshAbility(mod: Record<string, unknown>): Record<string, unknown> {
  return new (mod['default'] as new () => Record<string, unknown>)();
}

function routeFor(routes: Array<Record<string, unknown>>, destination: string,
  prefix: number): Record<string, unknown> | undefined {
  return routes.find((r: Record<string, unknown>): boolean =>
    r['destination'] === destination && r['prefixLength'] === prefix);
}

// One full derivation against the REAL extension class (private method —
// reachable at runtime; type stripping erases the modifier, not the method).
// mgmtDialIp is fed EXACTLY as the production call site receives it from
// resolveManagementIp (the 'IP:port' dial shape).
async function derive(ability: Record<string, unknown>,
  mgmtDialIp: string): Promise<{ fatal: boolean; routes: Array<Record<string, unknown>> }> {
  const outcome = await ability['deriveEndpointExclusions'](
    { host: 'mgmt.example', port: 443, ip: '' }, mgmtDialIp, null, '10.99.0.1') as
    { fatal: boolean; routes: Array<Record<string, unknown>> };
  return outcome;
}

try {
  const dir = mkdtempSync(join(tmpdir(), 'nb-exclusion-addr-shape-'));
  for (const name of STAGED) {
    writeFileSync(join(dir, name + '.ts'),
      readFileSync(join(REAL_DIR, name + '.ets'))); // byte-identical real file
  }
  for (const name of Object.keys(SHIM_TYPES)) {
    writeFileSync(join(dir, name + '.shim.ts'),
      'export * from \'./' + name + '.ts\';\n' +
      SHIM_TYPES[name].map((t: string): string => 'export const ' + t + ' = undefined;')
        .join('\n') + '\n');
  }

  // Pure module first (zero imports — loads standalone), then connector and
  // config via their shims, then the extension (which links all siblings).
  const exclusionPure = await import(
    pathToFileURL(join(dir, 'NetBirdEndpointExclusion.ts')).href) as Record<string, unknown>;
  const connectorMod = await import(
    pathToFileURL(join(dir, 'NetBirdConnector.shim.ts')).href) as Record<string, unknown>;
  const configMod = await import(
    pathToFileURL(join(dir, 'NetBirdVpnConfig.shim.ts')).href) as Record<string, unknown>;

  // ---------------- A2: pure strip/classify matrix --------------------------
  const strip = exclusionPure['stripDialPort'] as (a: string) => string;
  const classify = exclusionPure['classifyAddress'] as (a: string) => string;

  // The exact pre-fix bug: 'IP:port' was classified ipv6 (contains ':').
  check('A2 classifyAddress(IPv4:port) === ipv4 (pre-fix wrongly ipv6 — THE bug)',
    classify('203.0.113.7:443') === 'ipv4',
    'got ' + String(classify('203.0.113.7:443')));
  check('A2 stripDialPort(IPv4:port) -> bare IPv4',
    strip('203.0.113.7:443') === '203.0.113.7',
    'got ' + String(strip('203.0.113.7:443')));
  check('A2 stripDialPort(bare IPv4) unchanged',
    strip('203.0.113.7') === '203.0.113.7');
  check('A2 stripDialPort never touches an IPv6 literal (multi-: guard)',
    strip('2001:db8::1') === '2001:db8::1',
    'got ' + String(strip('2001:db8::1')));
  check('A2 stripDialPort never touches the [v6]:port bracket form',
    strip('[2001:db8::1]:443') === '[2001:db8::1]:443',
    'got ' + String(strip('[2001:db8::1]:443')));
  check('A2 stripDialPort ignores a non-digit port suffix',
    strip('203.0.113.7:https') === '203.0.113.7:https');
  check('A2 classifyAddress(bare IPv4) === ipv4 (unaffected)',
    classify('203.0.113.7') === 'ipv4');
  check('A2 classifyAddress(IPv6 literal) === ipv6 (not stripped/misjudged)',
    classify('2001:db8::1') === 'ipv6');
  check('A2 classifyAddress([v6]:port) === ipv6 (bracket form never the IPv4:port rule)',
    classify('[2001:db8::1]:443') === 'ipv6',
    'got ' + String(classify('[2001:db8::1]:443')));

  // ---------------- A3: builder end-to-end (defense in the real path) -------
  const build = exclusionPure['buildExcludedRouteEntries'] as
    (resolutions: unknown[], gateway: string) =>
      { routes: Array<Record<string, unknown>>; resolvedCount: number;
        duplicatesRemoved: number; failed: unknown[] };
  const gw = '10.99.0.1';

  const b1 = build([{ kind: 'management', host: 'mgmt.example', ok: true,
    address: '203.0.113.7:443' }], gw);
  check('A3 builder: IPv4:port resolution -> BARE /32 route, nothing failed',
    b1.routes.length === 1 && b1.resolvedCount === 1 && b1.failed.length === 0 &&
    b1.routes[0]['destination'] === '203.0.113.7' &&
    b1.routes[0]['prefixLength'] === 32 &&
    b1.routes[0]['isExcludedRoute'] === true, JSON.stringify(b1));

  const b2 = build([{ kind: 'relay', host: 'v6.example', ok: true,
    address: '2001:db8::1' }], gw);
  check('A3 builder: IPv6 literal stays /128, destination verbatim (not stripped)',
    b2.routes.length === 1 && b2.routes[0]['destination'] === '2001:db8::1' &&
    b2.routes[0]['prefixLength'] === 128, JSON.stringify(b2));

  const b3 = build([{ kind: 'management', host: 'mgmt.example', ok: true,
    address: '203.0.113.7' }], gw);
  check('A3 builder: bare IPv4 -> /32 (existing behavior unaffected)',
    b3.routes.length === 1 && b3.routes[0]['destination'] === '203.0.113.7' &&
    b3.routes[0]['prefixLength'] === 32, JSON.stringify(b3));

  const b4 = build([{ kind: 'relay', host: 'v6.example', ok: true,
    address: '[2001:db8::1]:443' }], gw);
  check('A3 builder: [v6]:port stays v6-classified /128 verbatim ' +
    '(documented status-quo semantics; never a false IPv4 strip)',
    b4.routes.length === 1 && b4.routes[0]['destination'] === '[2001:db8::1]:443' &&
    b4.routes[0]['prefixLength'] === 128, JSON.stringify(b4));

  const b5 = build([
    { kind: 'management', host: 'mgmt.example', ok: true, address: '203.0.113.7:443' },
    { kind: 'signal', host: 'mgmt.example', ok: true, address: '203.0.113.7' }
  ], gw);
  check('A3 builder: port-shape and bare same IP dedup to ONE route',
    b5.routes.length === 1 && b5.duplicatesRemoved === 1 &&
    b5.routes[0]['destination'] === '203.0.113.7', JSON.stringify(b5));

  // ---------------- A4: resolveManagementIp dial contract unchanged ---------
  // Both call sites that need the PORT (connector start dial at
  // NetBirdVpnExtensionAbility connectorStartWithSocket, signal feed in
  // provisionSignalSocket) consume this return verbatim — the fix strips
  // ONLY at the exclusion derivation call site.
  const resolveMgmt = connectorMod['resolveManagementIp'] as
    (endpoint: unknown, timeoutMs: number) => Promise<string>;
  const literalAddr = await resolveMgmt({ host: 'mgmt.example', port: 443,
    ip: '203.0.113.7' }, 100);
  check('A4 resolveManagementIp literal branch keeps the dial port',
    literalAddr === '203.0.113.7:443', 'got ' + literalAddr);
  setDns({ 'dns.example': '198.51.100.4' });
  const dnsAddr = await resolveMgmt({ host: 'dns.example', port: 8443, ip: '' }, 1000);
  check('A4 resolveManagementIp DNS branch keeps the dial port',
    dnsAddr === '198.51.100.4:8443', 'got ' + dnsAddr);

  // ---------------- A1: REGRESSION through the REAL derivation --------------
  // Fed exactly what production passed pre-fix: the unstripped dial address.
  setStatus({ running: true, state: 'started' }); // no relay section: quiet path
  setDns({ 'mgmt.example': '203.0.113.7' });
  G['__NB_LOG'] = [];
  const extMod = await import(
    pathToFileURL(join(dir, 'NetBirdVpnExtensionAbility.ts')).href) as Record<string, unknown>;
  const x1 = await derive(freshAbility(extMod), '203.0.113.7:443');
  const mgmtRoute1 = routeFor(x1.routes, '203.0.113.7', 32);
  check('A1 dial-shape mgmt resolution freezes BARE /32 exclusion, not fatal',
    x1.fatal === false && mgmtRoute1 !== undefined &&
    mgmtRoute1['isExcludedRoute'] === true && mgmtRoute1['hasGateway'] === true &&
    mgmtRoute1['gateway'] === '10.99.0.1', JSON.stringify(x1));
  check('A1 NO /128 or port-carrying entry survives (pre-fix: dest=IP:port prefix=128)',
    x1.routes.length === 1 &&
    x1.routes.every((r: Record<string, unknown>): boolean =>
      r['prefixLength'] === 32 &&
      String(r['destination']).indexOf(':') < 0), JSON.stringify(x1));
  const x1line = linesWith('VPN_ENDPOINT_EXCLUSIONS|').join('');
  check('A1 summary collapses dial+fresh into one entry (duplicates=1|routes=1)',
    linesWith('VPN_ENDPOINT_EXCLUSIONS|').length === 1 &&
    x1line.indexOf('|duplicates=1|') > 0 && x1line.indexOf('|routes=1|') > 0, x1line);
  // Platform mapping: family rides destination shape (NetBirdVpnConfig) —
  // a bare destination must render family=1 everywhere, no family=2 route.
  const platform = configMod['buildPlatformVpnConfig'] as (c: unknown) =>
    { routes: Array<Record<string, unknown>> };
  const mergedPlatform = platform(configMod['withEndpointExclusions'](
    configMod['loadVpnTunnelConfig'](), x1.routes));
  const mgmtPlatformRoute = mergedPlatform.routes.find(
    (r: Record<string, unknown>): boolean => {
      const dest = r['destination'] as Record<string, unknown>;
      const addr = dest['address'] as Record<string, unknown>;
      return addr['address'] === '203.0.113.7' && dest['prefixLength'] === 32;
    });
  check('A1 platform VpnConfig: mgmt entry family=1 /32, NO family=2 route anywhere',
    mgmtPlatformRoute !== undefined &&
    (mgmtPlatformRoute['destination'] as Record<string, unknown>)['address'] !== undefined &&
    ((mgmtPlatformRoute['destination'] as Record<string, unknown>)['address'] as
      Record<string, unknown>)['family'] === 1 &&
    (mgmtPlatformRoute['isExcludedRoute'] as boolean) === true &&
    mergedPlatform.routes.every((r: Record<string, unknown>): boolean => {
      const addr = (r['destination'] as Record<string, unknown>)['address'] as
        Record<string, unknown>;
      return addr['family'] === 1 && String(addr['address']).indexOf(':') < 0;
    }), JSON.stringify(mergedPlatform.routes));

  // ---------------- A5: wiring pins on the REAL shipped sources -------------
  const extSource: string =
    readFileSync(join(REAL_DIR, 'NetBirdVpnExtensionAbility.ets'), 'utf8');
  const exclSource: string =
    readFileSync(join(REAL_DIR, 'NetBirdEndpointExclusion.ets'), 'utf8');
  const cfgSource: string =
    readFileSync(join(REAL_DIR, 'NetBirdVpnConfig.ets'), 'utf8');
  check('A5 derivation call site passes stripDialPort(connectAddr); old shape gone',
    extSource.indexOf('stripDialPort(connectAddr)') >= 0 &&
    extSource.indexOf('mgmtEndpoint, connectAddr,') < 0,
    extSource.slice(extSource.indexOf('deriveEndpointExclusions(') - 200,
      extSource.indexOf('deriveEndpointExclusions(') + 200));
  check('A5 fix-site comment states the bare-IP contract (对照 signal 分支)',
    extSource.indexOf('对照 signal 分支') >= 0 &&
    extSource.indexOf('BARE IP') >= 0);
  check('A5 signal-branch strip still in place (parity reference)',
    extSource.indexOf('dialIp: addr.slice(0, addr.lastIndexOf(\':\'))') >= 0);
  check('A5 builder-side defense present in the real exclusion module',
    exclSource.indexOf('stripDialPort(resolution.address)') >= 0 &&
    exclSource.indexOf('export function stripDialPort') >= 0);
  check('A5 NetBirdVpnConfig family rule untouched (bare dest -> family 1)',
    cfgSource.indexOf("destination.indexOf(':') >= 0 ? 2 : 1") >= 0);

  rmSync(dir, { recursive: true, force: true });
} catch (error) {
  failedCount++;
  console.log('FAIL harness — ' + (error as Error).message + '\n' +
    ((error as Error).stack ?? ''));
}

console.log('');
console.log(String(passed) + ' passed, ' + String(failedCount) + ' failed');
process.exit(failedCount === 0 ? 0 : 1);
