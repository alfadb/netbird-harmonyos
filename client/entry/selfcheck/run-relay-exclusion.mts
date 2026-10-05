// N2-H "advertised relay exclusion" self-check (host-side, no device, no
// network, no build). Solves the N2-H circular dependency end-to-end on the
// host: the shell must include the relay endpoint in its exclusion set
// ALREADY while relay is disabled, from the management ADVERTISEMENT
// (core 95f6c02: status sibling section `relay_advertised`), and must keep
// the fail-closed / skip-with-log severity split.
//
// Principle of run.mts / run-relay-flag.mts / run-route-validation.mts,
// taken one level deeper: the REAL production files are copied byte-for-byte
// into OS-temp fixtures (.ts) at run time and the REAL extension class's
// deriveEndpointExclusions() is driven END-TO-END —
//   NetBirdVpnExtensionAbility.ets (+ real siblings NetBirdConnector /
//   NetBirdVpnConfig / NetBirdRouteValidation / NetBirdEndpointExclusion);
// @ohos.* / libnetbird_core.so are functional stubs injected by
// relay-exclusion-hooks.mjs (recording hilog, fake DNS table, fake
// connector_status JSON). The fixtures are never rewritten.
//
// Coverage (task mapping):
//   R1  advertised + relay disabled -> the relay host's /32 IS in the set,
//       fatal=false, relayRequired=false, relayAdvertised=true  (core case)
//   R2  advertised + enabled -> /32 included AND relayRequired=true
//   R3  not advertised -> no relay route, existing semantics unchanged
//   R4  relay endpoint resolution failure x disabled -> SKIP with the
//       VPN_ENDPOINT_EXCLUSION_SKIPPED marker, NOT fatal
//   R5  resolution failure x enabled -> fail-closed (FAIL_CLOSED marker)
//       (also: unparsable advertised url x enabled -> fail-closed)
//   R6  multiple advertised urls + duplicates (same IP as an existing
//       entry / same host two ports) -> dedup correct; merging with the
//       base entries reuses the real mergeExcludedRouteEntries
//   R7  lenient parse: section missing / flag type anomaly / urls not an
//       array / mixed elements / empty urls -> treated as (honestly) not
//       advertised, malformed flagged, never a throw
//
// Run: node client/entry/selfcheck/run-relay-exclusion.mts
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
// ESM link (see relay-exclusion-hooks.mjs). NetBirdInterfaceStatsProbe rides
// along: the REAL extension imports it (2026-09-18 iface-bytes probe), so the
// byte-identical fixture needs the byte-identical helper (no shim — the
// extension imports only its one exported function).
const STAGED = ['NetBirdVpnExtensionAbility', 'NetBirdConnector',
  'NetBirdVpnConfig', 'NetBirdRouteValidation', 'NetBirdEndpointExclusion',
  'NetBirdInterfaceStatsProbe', 'NetBirdPhysicalNetworkPolicy',
  // N5c demand-sized ICE pool (2026-10-02): the sizing pure module the REAL
  // extension imports; zero imports and no erased types, so no shim.
  'NetBirdIceSocketPool',
  // UI phase 1 (2026-10-05): the status-document builder pure module the REAL
  // extension imports; its two erased interfaces are re-declared by its shim.
  'NetBirdUiModel',
  // roam-1005-0014: the recreate lifecycle policy pure module the REAL
  // extension imports; zero imports and no erased types, so no shim.
  'NetBirdRecreatePolicy',
  // N2 T5a (2026-09-28): the stimulus pure module the REAL extension imports.
  'NetBirdN2Stimulus',
  // N2 P4/P5 (2026-09-28): the A-column trigger and sandwich pure modules.
  'NetBirdN2AEvent',
  'NetBirdN2Sandwich',
  // N2 T3 §3 (2026-09-28): the positive-control request pure module.
  'NetBirdN2ControlRequest',
  // N2 first batch C/D columns (2026-09-29): the switching-sandwich and
  // ring-dump pure modules the REAL extension imports.
  'NetBirdN2CdPlan',
  'NetBirdN2Ring'];
const SHIM_TYPES: Record<string, string[]> = {
  NetBirdConnector: ['ConnectorStatusResult', 'CoreNetworkConfig', 'CoreRouteEntry',
    'ConnectorStartResult', 'ConnectorLogoutResult', 'DeviceConfigFile', 'ManagementEndpoint',
    'NativeTcpSocketOpen', 'RelayAdvertisedView', 'RouteSetAppliedResult',
    'SocketFeedResult', 'SocketSupplyRequest', 'TunReleaseResult', 'WgFeedResult',
    // N2 first batch C/D columns: the erased bearer-facts shape.
    'PhysicalBearerFacts'],
  // UI phase 1: the erased input/document shapes buildUiStatusDocument names.
  NetBirdUiModel: ['UiNetworkConfig', 'UiStatusDocument'],
  NetBirdEndpointExclusion: ['ExcludedRouteEntry', 'EndpointResolution',
    'ExclusionBuildResult', 'ExclusionEndpoint', 'RelayEndpoint',
    // v3.1 dns branch order: the erased interface names the real
    // NetBirdConnector / NetBirdVpnExtensionAbility import alongside the values
    'DnsDerivationInput', 'DnsDerivationResult', 'DnsNetProbe',
    'DnsPlatformNetAddress',
    // N2-H I2 stun caliber (§13.13): same erased names, same reason
    'StunDerivationResult', 'StunUriEntry', 'StunParseResult', 'StunEndpoint'],
  NetBirdVpnConfig: ['VpnRouteConfig', 'VpnTunnelConfig',
    // S1 diag switch (2026-09-23): the erased outcome interface the real
    // extension imports alongside withTunnelSubnetRoute
    'TunnelSubnetRouteOutcome',
    // S6 diag capture route (2026-09-26): the second erased outcome interface
    // the real extension now imports alongside withCaptureRoute
    'CaptureRouteOutcome'],
  NetBirdRouteValidation: ['RouteCheckResult', 'RouteViolation'],
  // N2 T5a: the erased stimulus request/reply shapes the extension names.
  NetBirdN2Stimulus: ['N2StimulusDecision', 'N2StimulusReply'],
  // N2 P4/P5: the erased trigger and sandwich shapes the extension names.
  NetBirdN2AEvent: ['N2AEventDecision', 'N2AEventRequest', 'N2AEventReply'],
  NetBirdN2Sandwich: ['N2SandwichPost'],
  // N2 T3 §3: the erased positive-control request/decision shapes.
  NetBirdN2ControlRequest: ['N2ControlDecision', 'N2ControlRequest'],
  // N2 first batch C/D columns (2026-09-29): the erased shapes of the
  // switching-sandwich and ring-dump pure modules.
  NetBirdN2CdPlan: ['N2CdPlanDecision', 'N2CdPlanRequest', 'N2BearerFacts'],
  NetBirdN2Ring: ['N2RingDumpDecision', 'N2RingDumpRequest',
    // 2026-09-29 (explicit host ACK): the erased ACK request/decision shapes.
    'N2RingAckDecision', 'N2RingAckRequest']
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

// v3.1 C1 dns (R-4): the platform-network tables the functional
// @ohos.net.connection stub reads (see relay-exclusion-hooks.mjs).
function setNetTables(defaultId: number, all: number[],
  caps: Record<string, unknown>, dnses: Record<string, unknown>,
  throwing: string[] = []): void {
  G['__NB_NET_DEFAULT_ID'] = defaultId;
  G['__NB_NET_ALL'] = all;
  G['__NB_NET_CAPS'] = caps;
  G['__NB_NET_DNSES'] = dnses;
  G['__NB_NET_THROW'] = throwing;
}

const NOT_VPN: number = 15;
const VPN_BEARER: number = 4;
const WIFI: number = 1;

function physicalCaps(): Record<string, unknown> {
  return { networkCap: [12, NOT_VPN], bearerTypes: [WIFI] };
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

// One full derivation against the REAL extension class (private method —
// reachable at runtime; type stripping erases the modifier, not the method).
// `stuns` is the 5th (N2-H I2) argument: the announced
// `connector_network_config().stuns` list; a call site that omits it (the pre-I2
// shape) passes undefined, which must stay an honest gap.
async function derive(ability: Record<string, unknown>,
  mgmtDialIp: string, signal: unknown, stuns?: unknown): Promise<{ fatal: boolean; routes:
  Array<Record<string, unknown>> }> {
  const outcome = await ability['deriveEndpointExclusions'](
    { host: 'mgmt.example', port: 443, ip: '' }, mgmtDialIp, signal, '10.99.0.1', stuns) as
    { fatal: boolean; routes: Array<Record<string, unknown>> };
  return outcome;
}

function freshAbility(mod: Record<string, unknown>): Record<string, unknown> {
  return new (mod['default'] as new () => Record<string, unknown>)();
}

function routeFor(routes: Array<Record<string, unknown>>, destination: string,
  prefix: number): Record<string, unknown> | undefined {
  return routes.find((r: Record<string, unknown>): boolean =>
    r['destination'] === destination && r['prefixLength'] === prefix);
}

try {
  const dir = mkdtempSync(join(tmpdir(), 'nb-relay-exclusion-'));
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

  const mod = await import(pathToFileURL(join(dir, 'NetBirdVpnExtensionAbility.ts')).href) as
    Record<string, unknown>;
  const exclusionPure = await import(
    pathToFileURL(join(dir, 'NetBirdEndpointExclusion.ts')).href) as Record<string, unknown>;
  const connector = await import(pathToFileURL(join(dir, 'NetBirdConnector.ts')).href) as
    { physicalDefaultNetId: () => number; forgetPhysicalDefaultDns: () => void };
  const dns = (address: string): Array<Record<string, unknown>> =>
    [{ address: address, family: 1, port: 53 }];
  setNetTables(101, [101, 202], { 101: physicalCaps(), 202: physicalCaps() },
    { 101: dns('192.0.2.53'), 202: dns('198.51.100.53') });
  check('physical default selection prefers a qualifying default',
    connector.physicalDefaultNetId() === 101);
  connector.forgetPhysicalDefaultDns();
  setNetTables(900, [101, 202], { 900: { networkCap: [], bearerTypes: [VPN_BEARER] },
    101: physicalCaps(), 202: physicalCaps() },
  { 101: dns('192.0.2.53'), 202: dns('198.51.100.53') });
  check('physical fallback refuses ambiguous candidates', connector.physicalDefaultNetId() === 0);
  setNetTables(900, [101, 202], { 900: { networkCap: [], bearerTypes: [VPN_BEARER] },
    101: physicalCaps(), 202: physicalCaps() }, { 202: dns('198.51.100.53') });
  check('physical fallback selects the single resolver-bearing network',
    connector.physicalDefaultNetId() === 202);
  connector.forgetPhysicalDefaultDns();
  setNetTables(0, [], {}, {});

  const relaySection = (urls: string[]): Record<string, unknown> =>
    ({ enabled: urls.length > 0, state: urls.length > 0 ? 'disconnected' : 'disabled',
      urls: urls, reconnects: 0, frames_tx: 0, frames_rx: 0, transport_bytes: 0,
      token_valid: false, last_error_class: null });

  const status = (relayUrls: string[], advertised: unknown): Record<string, unknown> => {
    const doc: Record<string, unknown> = { running: true, state: 'started',
      relay: relaySection(relayUrls) };
    if (advertised !== null) {
      doc['relay_advertised'] = advertised;
    }
    return doc;
  };

  // ---------------- R1: advertised + relay_enabled=false (THE core case) ----
  setStatus(status([], { advertised: true,
    advertised_urls: ['rels://relay.example:28443'] }));
  setDns({ 'mgmt.example': '203.0.113.7', 'relay.example': '198.51.100.4' });
  G['__NB_LOG'] = [];
  const r1 = await derive(freshAbility(mod), '203.0.113.7', null);
  const relayRoute1 = routeFor(r1.routes, '198.51.100.4', 32);
  check('R1 advertised + disabled -> relay host /32 IS excluded (not abort)',
    r1.fatal === false && relayRoute1 !== undefined &&
    relayRoute1['isExcludedRoute'] === true && relayRoute1['hasGateway'] === true &&
    relayRoute1['gateway'] === '10.99.0.1', JSON.stringify(r1));
  const r1line = linesWith('VPN_ENDPOINT_EXCLUSIONS|').join('');
  check('R1 summary log carries advertisement fields',
    linesWith('VPN_ENDPOINT_EXCLUSIONS|').length === 1 &&
    r1line.indexOf('|relayRequired=false|') > 0 &&
    r1line.indexOf('|relayAdvertised=true|') > 0 &&
    r1line.endsWith('|relayHosts=1'), r1line);
  check('R1 no fail-closed / malformed markers',
    linesWith('VPN_ENDPOINT_EXCLUSION_FAIL_CLOSED|').length === 0 &&
    linesWith('VPN_RELAY_ADVERTISED_MALFORMED|').length === 0, logs().join('\n'));

  // ---------------- R2: advertised + enabled --------------------------------
  setStatus(status(['rels://relay.example:28443'], { advertised: true,
    advertised_urls: ['rels://relay.example:28443'] }));
  setDns({ 'mgmt.example': '203.0.113.7', 'relay.example': '198.51.100.4' });
  G['__NB_LOG'] = [];
  const r2 = await derive(freshAbility(mod), '203.0.113.7', null);
  check('R2 advertised + enabled -> /32 included and relayRequired=true',
    r2.fatal === false && routeFor(r2.routes, '198.51.100.4', 32) !== undefined &&
    linesWith('VPN_ENDPOINT_EXCLUSIONS|')[0].indexOf('|relayRequired=true|') > 0 &&
    linesWith('VPN_ENDPOINT_EXCLUSIONS|')[0].indexOf('|relayAdvertised=true|') > 0,
    JSON.stringify(r2) + '\n' + linesWith('VPN_ENDPOINT_EXCLUSIONS|').join('\n'));

  // ---------------- R3: not advertised --------------------------------------
  setStatus(status([], null)); // pre-95f6c02 core: no relay_advertised section
  setDns({ 'mgmt.example': '203.0.113.7' });
  G['__NB_LOG'] = [];
  const r3 = await derive(freshAbility(mod), '203.0.113.7', null);
  check('R3 not advertised -> no relay route, semantics unchanged',
    r3.fatal === false && routeFor(r3.routes, '198.51.100.4', 32) === undefined &&
    routeFor(r3.routes, '203.0.113.7', 32) !== undefined, JSON.stringify(r3));
  const r3line = linesWith('VPN_ENDPOINT_EXCLUSIONS|')[0];
  check('R3 summary says not advertised / no relay hosts',
    r3line.indexOf('|relayAdvertised=false|') > 0 &&
    r3line.endsWith('|relayHosts=0'), r3line);
  check('R3 grep-able not-advertised hint',
    linesWith('VPN_RELAY_ADVERTISED_NONE|').length === 1, logs().join('\n'));

  // ---------------- R4: resolution failure x DISABLED -> skip, not abort ----
  setStatus(status([], { advertised: true,
    advertised_urls: ['rels://relay.example:28443'] }));
  setDns({ 'mgmt.example': '203.0.113.7' }); // relay.example intentionally unresolvable
  G['__NB_LOG'] = [];
  const r4 = await derive(freshAbility(mod), '203.0.113.7', null);
  const skipped4 = linesWith('VPN_ENDPOINT_EXCLUSION_SKIPPED|');
  check('R4 resolution failure x disabled -> NOT fatal, route skipped',
    r4.fatal === false && routeFor(r4.routes, '198.51.100.4', 32) === undefined &&
    routeFor(r4.routes, '203.0.113.7', 32) !== undefined, JSON.stringify(r4));
  check('R4 SKIPPED marker carries kind/host/relayRequired/action',
    skipped4.length === 1 && skipped4[0].indexOf('|kind=relay|') > 0 &&
    skipped4[0].indexOf('|host=relay.example|') > 0 &&
    skipped4[0].indexOf('|relayRequired=false|') > 0 &&
    skipped4[0].endsWith('|action=skip-with-log'), skipped4.join('\n'));
  check('R4 no FAIL_CLOSED marker',
    linesWith('VPN_ENDPOINT_EXCLUSION_FAIL_CLOSED|').length === 0, logs().join('\n'));

  // ---------------- R5: resolution failure x ENABLED -> fail-closed ---------
  setStatus(status(['rels://relay.example:28443'], { advertised: true,
    advertised_urls: ['rels://relay.example:28443'] }));
  setDns({ 'mgmt.example': '203.0.113.7' }); // relay resolution fails
  G['__NB_LOG'] = [];
  const r5 = await derive(freshAbility(mod), '203.0.113.7', null);
  check('R5 resolution failure x enabled -> fail-closed (fatal)',
    r5.fatal === true && routeFor(r5.routes, '198.51.100.4', 32) === undefined,
    JSON.stringify(r5));
  const skipped5 = linesWith('VPN_ENDPOINT_EXCLUSION_SKIPPED|');
  check('R5 SKIPPED action=fail-closed + FAIL_CLOSED marker',
    skipped5.length === 1 && skipped5[0].endsWith('|action=fail-closed') &&
    linesWith('VPN_ENDPOINT_EXCLUSION_FAIL_CLOSED|').length === 1, logs().join('\n'));

  // R5b: UNPARSABLE advertised url x enabled -> fail-closed too (the core
  // would refuse the url typed; an unfreezable relay endpoint must abort
  // exactly like an unresolved one when relay is being enabled).
  setStatus(status(['rels://relay.example:28443'], { advertised: true,
    advertised_urls: ['https://relay.example:443'] }));
  setDns({ 'mgmt.example': '203.0.113.7', 'relay.example': '198.51.100.4' });
  G['__NB_LOG'] = [];
  const r5b = await derive(freshAbility(mod), '203.0.113.7', null);
  check('R5b unparsable advertised url x enabled -> fail-closed',
    r5b.fatal === true &&
    linesWith('VPN_ENDPOINT_EXCLUSION_URL_MALFORMED|').length === 1 &&
    linesWith('VPN_ENDPOINT_EXCLUSION_FAIL_CLOSED|').length === 1, logs().join('\n'));

  // ---------------- R6: dedup across multiple advertised urls ---------------
  // relay.example advertises on two ports AND resolves onto the management
  // dial IP; a second advertised host stays distinct.
  setStatus(status([], { advertised: true, advertised_urls: [
    'rel://relay.example', 'rels://relay.example:28443', 'rels://relay2.example:443'] }));
  setDns({ 'mgmt.example': '203.0.113.7', 'relay.example': '203.0.113.7',
    'relay2.example': '198.51.100.4' });
  G['__NB_LOG'] = [];
  const r6 = await derive(freshAbility(mod), '203.0.113.7', null);
  const r6line = linesWith('VPN_ENDPOINT_EXCLUSIONS|')[0];
  check('R6 same-IP collapse: only 2 routes (mgmt/relay shared IP + 2nd host)',
    r6.fatal === false && r6.routes.length === 2 &&
    routeFor(r6.routes, '203.0.113.7', 32) !== undefined &&
    routeFor(r6.routes, '198.51.100.4', 32) !== undefined,
    JSON.stringify(r6));
  check('R6 duplicates counted (mgmt fresh resolve + 2 relay ports)',
    r6line.indexOf('|duplicates=3|') > 0, r6line);
  check('R6 relayHosts counts distinct hosts only',
    r6line.endsWith('|relayHosts=2'), r6line);
  // Merging with the base entries reuses the REAL mergeExcludedRouteEntries:
  // LAN + default entries verbatim, derived appended once, no duplicates.
  const lanEntry = { destination: '192.168.50.0', prefixLength: 24, gateway: '10.99.0.1',
    hasGateway: true, isDefaultRoute: false, isExcludedRoute: true };
  const defaultEntry = { destination: '0.0.0.0', prefixLength: 0, gateway: '10.99.0.1',
    hasGateway: true, isDefaultRoute: true, isExcludedRoute: false };
  const merged6 = (exclusionPure['mergeExcludedRouteEntries'] as
    (existing: unknown[], derived: unknown[]) => unknown[])([lanEntry, defaultEntry], r6.routes);
  check('R6 real merge: base entries verbatim + derived appended once',
    merged6.length === 4 && merged6[0] === lanEntry && merged6[1] === defaultEntry &&
    (merged6[2] as Record<string, unknown>)['destination'] === '203.0.113.7' &&
    (merged6[3] as Record<string, unknown>)['destination'] === '198.51.100.4',
    JSON.stringify(merged6));
  const merged6dup = (exclusionPure['mergeExcludedRouteEntries'] as
    (existing: unknown[], derived: unknown[]) => unknown[])([
      lanEntry, { destination: '198.51.100.4', prefixLength: 32, gateway: '10.99.0.1',
        hasGateway: true, isDefaultRoute: false, isExcludedRoute: true }], r6.routes);
  check('R6 real merge: derived duplicate of existing skipped',
    merged6dup.length === 3, JSON.stringify(merged6dup));

  // ---------------- R7: lenient parse of the advertised section -------------
  const connectorMod = await import(
    pathToFileURL(join(dir, 'NetBirdConnector.ts')).href) as Record<string, unknown>;
  const parse = connectorMod['parseRelayAdvertised'] as
    (s: unknown) => Record<string, unknown>;

  const v7a = parse(status([], null)); // section absent
  check('R7a section absent -> not advertised, not malformed, no throw',
    v7a['advertised'] === false && v7a['malformed'] === false &&
    JSON.stringify(v7a['urls']) === '[]', JSON.stringify(v7a));

  const v7b = parse(status([], { advertised: 'true' }));
  check('R7b flag type anomaly -> malformed + not advertised',
    v7b['advertised'] === false && v7b['malformed'] === true, JSON.stringify(v7b));

  const v7c = parse(status([], { advertised: true, advertised_urls: 'rels://relay.example' }));
  check('R7c urls not an array -> malformed + not advertised',
    v7c['advertised'] === false && v7c['malformed'] === true, JSON.stringify(v7c));

  const v7d = parse(status([], { advertised: true,
    advertised_urls: ['rels://relay.example:28443', 7, null] }));
  check('R7d mixed elements -> valid strings kept, anomaly flagged',
    v7d['advertised'] === true && v7d['malformed'] === true &&
    JSON.stringify(v7d['urls']) === '["rels://relay.example:28443"]', JSON.stringify(v7d));

  const v7e = parse(status([], { advertised: true, advertised_urls: [] }));
  check('R7e empty advertised urls -> honest not-advertised (no anomaly)',
    v7e['advertised'] === false && v7e['malformed'] === false, JSON.stringify(v7e));

  // End-to-end leniency: malformed section -> treated as not advertised, the
  // derivation completes WITHOUT a throw and logs the grep-able hint.
  setStatus(status([], { advertised: 3, advertised_urls: 'nope' }));
  setDns({ 'mgmt.example': '203.0.113.7' });
  G['__NB_LOG'] = [];
  const r7 = await derive(freshAbility(mod), '203.0.113.7', null);
  check('R7f end-to-end malformed section -> not advertised, no relay route, no throw',
    r7.fatal === false && routeFor(r7.routes, '198.51.100.4', 32) === undefined &&
    linesWith('VPN_RELAY_ADVERTISED_MALFORMED|').length === 1 &&
    linesWith('VPN_ENDPOINT_EXCLUSIONS|')[0].indexOf('|relayAdvertised=false|') > 0,
    JSON.stringify(r7) + '\n' + logs().join('\n'));

  // ---------------- D1…D5: v3.1 C1 dns branch order (R-4) -------------------
  // The REAL extension derives the dns endpoints through the REAL connector
  // glue, driven by the platform tables above — the branch order, the routes
  // and the verbatim branch record are the shipped behavior, not a mirror.
  setStatus(status([], null));
  setDns({ 'mgmt.example': '203.0.113.7' });
  setNetTables(101, [101], { 101: physicalCaps() },
    { 101: [{ address: '192.168.7.1', family: 1, port: 53 }] });
  G['__NB_LOG'] = [];
  const d1 = await derive(freshAbility(mod), '203.0.113.7', null);
  check('D1 default branch -> the physical resolver IS route-excluded (/32)',
    d1.fatal === false && routeFor(d1.routes, '192.168.7.1', 32) !== undefined &&
    routeFor(d1.routes, '192.168.7.1', 32)!['isExcludedRoute'] === true, JSON.stringify(d1));
  const d1marker = linesWith('N2H_DNS_BRANCH|');
  check('D1 marker carries the branch + the verbatim physical grounds',
    d1marker.length === 1 && d1marker[0].indexOf('|dns-branch=default|') > 0 &&
    d1marker[0].indexOf(
      '|dns-physical-evidence=handle=101;networkCap-has-NOT_VPN=15;' +
      'bearerTypes-no-BEARER_VPN=4|') > 0 &&
    d1marker[0].indexOf('|dnsResolvers=1|') > 0 &&
    d1marker[0].indexOf('|dnsGap=none|') > 0, d1marker.join('\n'));
  // the same report is what the ARM hands to the core: the retained shell
  // exclusion set carries the dns endpoint (kind/host/port/resolved)
  const d1ability = freshAbility(mod);
  setNetTables(101, [101], { 101: physicalCaps() },
    { 101: [{ address: '192.168.7.1', family: 1, port: 53 }] });
  G['__NB_LOG'] = [];
  await derive(d1ability, '203.0.113.7', null);
  const d1shell = (d1ability['n2hExclusionEndpoints'] as Array<Record<string, unknown>>)
    .filter((e: Record<string, unknown>): boolean => e['kind'] === 'dns');
  const d1report = d1ability['n2hDnsReport'] as Record<string, unknown>;
  check('D1 the dns endpoint rides the shell exclusion set handed to the arm',
    d1shell.length === 1 && d1shell[0]['host'] === '192.168.7.1' && d1shell[0]['port'] === 53,
    JSON.stringify(d1shell));
  check('D1 the retained arm report is the branch record (no gap_reason)',
    d1report['branch'] === 'default' && d1report['gap'] === false &&
    d1report['physicalEvidence'] ===
    'handle=101;networkCap-has-NOT_VPN=15;bearerTypes-no-BEARER_VPN=4',
    JSON.stringify(d1report));

  // D2: step 2 does not hold (default handle has no NOT_VPN) -> all-nets
  setNetTables(101, [101, 202], { 101: { networkCap: [12], bearerTypes: [WIFI] },
    202: physicalCaps() }, { 101: [{ address: '203.0.113.99', family: 1, port: 53 }],
    202: [{ address: '198.51.100.9', family: 1, port: 53 }] });
  G['__NB_LOG'] = [];
  const d2 = await derive(freshAbility(mod), '203.0.113.7', null);
  const d2marker = linesWith('N2H_DNS_BRANCH|');
  check('D2 step 2 fails -> all-nets branch, only its resolver is excluded',
    d2.fatal === false && routeFor(d2.routes, '198.51.100.9', 32) !== undefined &&
    routeFor(d2.routes, '203.0.113.99', 32) === undefined &&
    d2marker[0].indexOf('|dns-branch=all-nets-filtered|') > 0 &&
    d2marker[0].indexOf('|dns-physical-evidence=handle=202;') > 0,
    JSON.stringify(d2) + '\n' + d2marker.join('\n'));

  // D3: neither path proves physical attribution -> dns GAP (no route, no
  // fabricated branch)
  setNetTables(0, [], {}, {});
  G['__NB_LOG'] = [];
  const d3 = await derive(freshAbility(mod), '203.0.113.7', null);
  const d3marker = linesWith('N2H_DNS_BRANCH|');
  check('D3 no physical network -> dns gap, no dns route, no throw',
    d3.fatal === false && d3.routes.every((r: Record<string, unknown>): boolean =>
      r['destination'] !== '192.168.7.1') &&
    d3marker[0].indexOf('|dns-branch=none|') > 0 &&
    d3marker[0].indexOf('|dns-physical-evidence=none|') > 0 &&
    d3marker[0].indexOf('|dnsGap=no-physical-net|') > 0, d3marker.join('\n'));

  // D4: evidence-by-value — the default network qualifies AND the all-nets set
  // carries another physical resolver: the DEFAULT branch wins, because the
  // order is fixed before any data is seen (no post-hoc branch picking)
  setNetTables(101, [101, 202], { 101: physicalCaps(), 202: physicalCaps() },
    { 101: [{ address: '192.168.7.1', family: 1, port: 53 }],
      202: [{ address: '198.51.100.9', family: 1, port: 53 }] });
  G['__NB_LOG'] = [];
  const d4 = await derive(freshAbility(mod), '203.0.113.7', null);
  const d4marker = linesWith('N2H_DNS_BRANCH|');
  check('D4 default branch wins even when all-nets could yield more (fixed order)',
    routeFor(d4.routes, '192.168.7.1', 32) !== undefined &&
    routeFor(d4.routes, '198.51.100.9', 32) === undefined &&
    d4marker[0].indexOf('|dns-branch=default|') > 0, JSON.stringify(d4) + '\n' + d4marker.join('\n'));

  // D5: a platform API error is recorded verbatim as the gap token, and the
  // surviving branch's resolvers are still frozen
  setNetTables(101, [202], { 202: physicalCaps() },
    { 202: [{ address: '198.51.100.9', family: 1, port: 53 }] }, ['getDefaultNetSync']);
  G['__NB_LOG'] = [];
  const d5 = await derive(freshAbility(mod), '203.0.113.7', null);
  const d5marker = linesWith('N2H_DNS_BRANCH|');
  check('D5 API error -> api-error-code token, all-nets resolvers still excluded',
    d5.fatal === false && routeFor(d5.routes, '198.51.100.9', 32) !== undefined &&
    d5marker[0].indexOf('|dns-branch=all-nets-filtered|') > 0 &&
    d5marker[0].indexOf('|dnsGap=api-error-code-2100002|') > 0,
    JSON.stringify(d5) + '\n' + d5marker.join('\n'));

  // Reset the platform tables: the cases below must start from "no default
  // network" exactly like the pre-dns fixture did.
  setNetTables(0, [], {}, {});

  // ---------- S1…S6: N2-H I2 stun caliber (design §13.13) -------------------
  // The announced `netbird_config.stuns[].uri` list reaches the shell in the
  // tail of `connector_network_config()` (core, asserted in the cargo suite)
  // and is the 5th argument of the REAL derivation. Nothing here is a mirror:
  // the REAL parse, the REAL endpoint class, the REAL /32 route entries and
  // the REAL retained shell-exclusion set are the shipped behavior.
  setStatus(status([], null));
  setDns({ 'mgmt.example': '203.0.113.7', 'stun.example': '198.51.100.7' });
  G['__NB_LOG'] = [];
  const s1 = await derive(freshAbility(mod), '203.0.113.7', null,
    [{ uri: 'stun:stun.example:3478' }]);
  const s1route = routeFor(s1.routes, '198.51.100.7', 32);
  check('S1 announced stun host -> /32 excluded route (isExcludedRoute), not fatal',
    s1.fatal === false && s1route !== undefined && s1route['isExcludedRoute'] === true &&
    s1route['hasGateway'] === true && s1route['gateway'] === '10.99.0.1' &&
    s1route['isDefaultRoute'] === false, JSON.stringify(s1));
  const s1marker = linesWith('N2H_STUN_ENDPOINTS|');
  check('S1 marker carries the verbatim stun tokens (counts + no gap)',
    s1marker.length === 1 && s1marker[0].indexOf('|stuns=1|') > 0 &&
    s1marker[0].indexOf('|usable=1|') > 0 && s1marker[0].indexOf('|rejected=0|') > 0 &&
    s1marker[0].indexOf('|turnRejected=0|') > 0 &&
    s1marker[0].indexOf('|stunGap=none|') > 0 &&
    s1marker[0].indexOf('|diag=true|is_evidence=false') > 0, s1marker.join('\n'));
  check('S1 the stun endpoint enters the VPN_ENDPOINT_EXCLUSIONS summary count',
    linesWith('VPN_ENDPOINT_EXCLUSIONS|')[0].indexOf('|hosts=2|') > 0 &&
    linesWith('VPN_ENDPOINT_EXCLUSIONS|')[0].indexOf('|resolved=2|') > 0,
    linesWith('VPN_ENDPOINT_EXCLUSIONS|')[0]);
  check('S1 no rejection marker for a usable announcement',
    linesWith('N2H_STUN_URI_REJECTED|').length === 0, logs().join('\n'));

  // S2: the stun endpoint is RETAINED in the shell exclusion set the arm hands
  // to the core as `shell-exclusion` (kind 'stun' is a core EndpointKind token)
  const s2ability = freshAbility(mod);
  setDns({ 'mgmt.example': '203.0.113.7', 'stun.example': '198.51.100.7' });
  G['__NB_LOG'] = [];
  await derive(s2ability, '203.0.113.7', null, [{ uri: 'stun:stun.example:3478' }]);
  const s2shell = (s2ability['n2hExclusionEndpoints'] as Array<Record<string, unknown>>)
    .filter((e: Record<string, unknown>): boolean => e['kind'] === 'stun');
  const s2res = (s2ability['n2hExclusionResolutions'] as Array<Record<string, unknown>>)
    .filter((e: Record<string, unknown>): boolean => e['kind'] === 'stun');
  check('S2 the stun endpoint rides the retained shell exclusion set (arm source)',
    s2shell.length === 1 && s2shell[0]['host'] === 'stun.example' &&
    s2shell[0]['port'] === 3478 && s2res.length === 1 &&
    s2res[0]['ok'] === true && s2res[0]['address'] === '198.51.100.7',
    JSON.stringify(s2shell) + JSON.stringify(s2res));

  // S3: an IPv4 literal short-circuits DNS — the host is absent from the fake
  // resolver table on purpose, so a route for it can only come from the
  // literal branch, and the omitted port defaults to 3478
  const s3ability = freshAbility(mod);
  setDns({ 'mgmt.example': '203.0.113.7' });
  G['__NB_LOG'] = [];
  const s3 = await derive(s3ability, '203.0.113.7', null, [{ uri: 'stun:203.0.113.44' }]);
  const s3shell = (s3ability['n2hExclusionEndpoints'] as Array<Record<string, unknown>>)
    .filter((e: Record<string, unknown>): boolean => e['kind'] === 'stun');
  check('S3 IPv4 literal stun host: /32 route with NO DNS leg, default port 3478',
    s3.fatal === false && routeFor(s3.routes, '203.0.113.44', 32) !== undefined &&
    s3shell.length === 1 && s3shell[0]['port'] === 3478 &&
    linesWith('VPN_ENDPOINT_EXCLUSION_SKIPPED|').length === 0, JSON.stringify(s3));

  // S4: `turn:` is refused BY NAME and generates NO route — even though the
  // fake resolver WOULD answer for that host (a resolved turn host would show
  // up as a route if the shell had frozen it)
  setDns({ 'mgmt.example': '203.0.113.7', 'turn.example': '198.51.100.8' });
  G['__NB_LOG'] = [];
  const s4 = await derive(freshAbility(mod), '203.0.113.7', null,
    [{ uri: 'turn:turn.example:3478' }]);
  const s4marker = linesWith('N2H_STUN_ENDPOINTS|');
  const s4rejected = linesWith('N2H_STUN_URI_REJECTED|');
  check('S4 turn: announced -> NO turn route at all',
    s4.fatal === false && routeFor(s4.routes, '198.51.100.8', 32) === undefined &&
    s4.routes.length === 1, JSON.stringify(s4));
  check('S4 the named turn reason is recorded, counted, and gap-flagged',
    s4rejected.length === 1 &&
    s4rejected[0].indexOf('|reason=turn-rejected:no-turn-path-in-this-build|') > 0 &&
    s4marker.length === 1 && s4marker[0].indexOf('|usable=0|') > 0 &&
    s4marker[0].indexOf('|turnRejected=1|') > 0 &&
    s4marker[0].indexOf('|stunGap=unusable-uris|') > 0,
    s4rejected.join('\n') + '\n' + s4marker.join('\n'));

  // S5: a missing / empty announcement is an honest gap (never an invented
  // endpoint); the pre-I2 call shape (argument omitted => undefined) behaves
  // identically
  setDns({ 'mgmt.example': '203.0.113.7' });
  for (const [label, announced] of [
    ['undefined (pre-I2 call shape)', undefined],
    ['null', null],
    ['empty array', []]
  ] as Array<[string, unknown]>) {
    G['__NB_LOG'] = [];
    const s5 = await derive(freshAbility(mod), '203.0.113.7', null, announced);
    const s5marker = linesWith('N2H_STUN_ENDPOINTS|');
    check('S5 stuns ' + label + ' -> no stun route, stunGap=no-stuns, no throw',
      s5.fatal === false && s5.routes.length === 1 &&
      s5marker.length === 1 && s5marker[0].indexOf('|stuns=0|') > 0 &&
      s5marker[0].indexOf('|usable=0|') > 0 && s5marker[0].indexOf('|rejected=0|') > 0 &&
      s5marker[0].indexOf('|stunGap=no-stuns|') > 0,
      JSON.stringify(s5) + '\n' + s5marker.join('\n'));
  }

  // S6: a type-anomalous announcement element is reported (malformed-entry),
  // and the usable element in the same list still freezes
  setDns({ 'mgmt.example': '203.0.113.7', 'stun.example': '198.51.100.7' });
  G['__NB_LOG'] = [];
  const s6 = await derive(freshAbility(mod), '203.0.113.7', null,
    [{ uri: 7 }, { uri: 'stun:stun.example:3478' }]);
  const s6marker = linesWith('N2H_STUN_ENDPOINTS|');
  check('S6 anomalous element reported, usable sibling still /32-excluded',
    s6.fatal === false && routeFor(s6.routes, '198.51.100.7', 32) !== undefined &&
    linesWith('N2H_STUN_URI_REJECTED|').length === 1 &&
    linesWith('N2H_STUN_URI_REJECTED|')[0].indexOf('|reason=malformed-entry|') > 0 &&
    s6marker[0].indexOf('|stuns=2|') > 0 && s6marker[0].indexOf('|usable=1|') > 0 &&
    s6marker[0].indexOf('|rejected=1|') > 0 &&
    s6marker[0].indexOf('|stunGap=unusable-uris|') > 0,
    JSON.stringify(s6) + '\n' + s6marker.join('\n'));

  // The last hop cannot be driven host-side (it needs the native arm), so it
  // is pinned TEXTUALLY on the byte-identical shipped source — the precedent
  // is run-exclusion-addr-shape.mts's A5 wiring pins: the branch record the
  // pure module returned (asserted above) is the value the arm forwards
  // verbatim to the core, which renders it into the evidence `notes[]`.
  const extSource: string = readFileSync(join(dir, 'NetBirdVpnExtensionAbility.ts')).toString();
  check('arm wiring: the retained dns report is attached to the arm options verbatim',
    extSource.indexOf('opts.dns = report;') > 0 &&
    extSource.indexOf('branch: this.n2hDnsReport.branch,') > 0 &&
    extSource.indexOf('physical_evidence: this.n2hDnsReport.physicalEvidence') > 0 &&
    extSource.indexOf('report.gap_reason = this.n2hDnsReport.gapReason;') > 0,
    'arm forwarding pin');
  check('arm wiring: the marker format carries the verbatim token names',
    extSource.indexOf("'N2H_DNS_BRANCH|requestId=%{public}s|dns-branch=%{public}s|'") > 0 &&
    extSource.indexOf("'dns-physical-evidence=%{public}s|dnsResolvers=%{public}d|dnsGap=%{public}s|'")
      > 0, 'marker pin');

  // N2-H I2 wiring pins (same textual-precedent discipline, same reason): the
  // announced stun list is read from the SAME snapshot the caller already holds
  // (`net.stuns`, the tail of connector_network_config()) and forwarded as the
  // derivation's 5th argument; the marker formats carry the verbatim tokens the
  // checks above assert on the real derivation.
  check('arm wiring: the announced stuns ride net.stuns into the derivation call',
    extSource.indexOf('baseTemplate.tunnelAddress, net !== null ? net.stuns : null)') > 0,
    'stun wiring pin');
  check('arm wiring: the stun marker formats carry the verbatim token names',
    extSource.indexOf("'N2H_STUN_ENDPOINTS|requestId=%{public}s|stuns=%{public}d|usable=%{public}d|'")
      > 0 &&
    extSource.indexOf("'rejected=%{public}d|turnRejected=%{public}d|stunGap=%{public}s|'") > 0 &&
    extSource.indexOf("'N2H_STUN_URI_REJECTED|requestId=%{public}s|index=%{public}d|reason=%{public}s|'")
      > 0, 'stun marker pin');

  // Log discipline: no token/credential-shaped material in the relay logs
  // (URLs are host:port only; assert the summary line never embeds a scheme).
  check('log discipline: EXCLUSIONS line carries no url scheme material',
    linesWith('VPN_ENDPOINT_EXCLUSIONS|').every((l: string): boolean =>
      l.indexOf('://') < 0 && l.indexOf('token') < 0), linesWith('VPN_ENDPOINT_EXCLUSIONS|').join('\n'));

  rmSync(dir, { recursive: true, force: true });
} catch (error) {
  failedCount++;
  console.log('FAIL harness — ' + (error as Error).message + '\n' +
    ((error as Error).stack ?? ''));
}

console.log('');
console.log(String(passed) + ' passed, ' + String(failedCount) + ' failed');
process.exit(failedCount === 0 ? 0 : 1);