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
// ESM link (see relay-exclusion-hooks.mjs).
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

// One full derivation against the REAL extension class (private method —
// reachable at runtime; type stripping erases the modifier, not the method).
async function derive(ability: Record<string, unknown>,
  mgmtDialIp: string, signal: unknown): Promise<{ fatal: boolean; routes:
  Array<Record<string, unknown>> }> {
  const outcome = await ability['deriveEndpointExclusions'](
    { host: 'mgmt.example', port: 443, ip: '' }, mgmtDialIp, signal, '10.99.0.1') as
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