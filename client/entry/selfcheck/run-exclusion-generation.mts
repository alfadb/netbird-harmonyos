// N2-H I3a generation-marker self-check (host-side, no device, no network, no
// build). Preregistration v3.1 §2.1 C2 (1)/(2)/(2a)/(4) + R-1/R-2:
//   (1)  one VPN_EXCLUSION_GENERATION line per create(), BEFORE that create(),
//        n starting at 0 and +1 with no gap within one requestId;
//   (2)  every VPN_ROUTE_ENTRY row of that generation carries the same gen=<n>;
//   (2a) the announced routes=<r>/exclusions=<m> equal the rendered row set
//        (row count, index uniquely covering 0..r-1, isExcludedRoute=="true"
//        row count);
//   (4)  the acceptance anchor: kind=initial -> VPN_CREATE_RESOLVED,
//        kind=recreate -> VPN_RECREATE_RESOLVED|…|gen=<n>|accepted=true, both
//        AFTER the generation line and its rows;
//   R-2  the arm's evidence generation G (collector.exclusion_generation /
//        N2H_COLLECTOR_ARMED|gen=G / the last accepted generation line) comes
//        from ONE source and must be literally equal.
//
// Principle of run-relay-exclusion.mts, taken to the generation bookkeeping:
// the REAL production files are copied byte-for-byte into OS-temp fixtures
// (.ts) at run time and the REAL extension class is driven END-TO-END —
//   NetBirdVpnExtensionAbility.ets (+ real siblings NetBirdConnector /
//   NetBirdVpnConfig / NetBirdRouteValidation / NetBirdEndpointExclusion);
// @ohos.* / libnetbird_core.so are functional stubs injected by
// exclusion-generation-hooks.mjs (recording hilog, fake DNS table, fake fd
// probe, fake network map, fake VpnConnection). The fixtures are never
// rewritten.
//
// Coverage (task mapping):
//   G1 initial leg: the REAL logExclusionGeneration + logRouteTable emit the
//      exact marker literal and the gen-stamped rows of the submitted config
//   G2 initial acceptance: the REAL settleCreate emits
//      VPN_CREATE_RESOLVED|accepted=true and makes that gen the accepted one
//      (generation line + all rows BEFORE the anchor)
//   G3 recreate leg: the REAL performControlledRecreate drives a full
//      destroy -> create -> fd contract -> wg feed -> ACK cycle: the recreate
//      marker (kind=recreate), its rows, and the new acceptance anchor
//      VPN_RECREATE_RESOLVED|gen=1|accepted=true, with the pre-existing
//      VPN_RECREATE_SNAPSHOT / VPN_RECREATE_DONE lines unchanged
//   G4 a SECOND recreate: gen=2, and the whole generation sequence is 0,1,2
//      (+1, no gap, kinds initial/recreate/recreate)
//   G5 three-way R-2 consistency, driven through the REAL arm hook: the arm
//      option the core echoes into collector.exclusion_generation, the
//      N2H_COLLECTOR_ARMED|gen= field and the last ACCEPTED generation line
//      all carry the same number
//   G6 fail-closed honesty: a create() that is NOT accepted never becomes the
//      accepted generation, and the arm then reports NO generation (the key is
//      omitted) instead of fabricating one
//   G7 wiring pins on the byte-identical source (the pre-create call sites,
//      the two anchor literals, the untouched existing recreate lines and the
//      arm's single-source read)
//   A1-A6 (I5a §13.15): the arm hook's outcome is never silent — an absent or
//      unreadable arm control file emits exactly ONE
//      N2H_COLLECTOR_ARM_SKIPPED line (reason=arm-file-absent for a PROVED
//      absence: boolean false or BusinessError 13900002; reason=
//      arm-file-unreadable for every other error token, which does not prove
//      absence) carrying the raw code as accessError=, WITHOUT calling the core
//      arm; the one-shot latch silences every later tick of that instance (and
//      a file appearing later never arms it); the present-file NOT_ARMED (gate
//      closed, both field combinations) and ARMED (gate open) paths stay
//      byte-identical (regression).
//
// Run: node client/entry/selfcheck/run-exclusion-generation.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./exclusion-generation-hooks.mjs', import.meta.url).href);

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_DIR = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability');

// The five real modules staged byte-identically (code under test = shipped
// code), and the erased interface names each shim must re-declare for the ESM
// link (same list as run-relay-exclusion.mts — see that file's hook comment).
const STAGED = ['NetBirdVpnExtensionAbility', 'NetBirdConnector',
  'NetBirdVpnConfig', 'NetBirdRouteValidation', 'NetBirdEndpointExclusion'];
const SHIM_TYPES: Record<string, string[]> = {
  NetBirdConnector: ['ConnectorStatusResult', 'CoreNetworkConfig', 'CoreRouteEntry',
    'ConnectorStartResult', 'DeviceConfigFile', 'ManagementEndpoint',
    'NativeTcpSocketOpen', 'RelayAdvertisedView', 'RouteSetAppliedResult',
    'SocketFeedResult', 'WgFeedResult'],
  NetBirdEndpointExclusion: ['ExcludedRouteEntry', 'EndpointResolution',
    'ExclusionBuildResult', 'ExclusionEndpoint', 'RelayEndpoint',
    'DnsDerivationInput', 'DnsDerivationResult', 'DnsNetProbe',
    'DnsPlatformNetAddress',
    'StunDerivationResult', 'StunUriEntry', 'StunParseResult', 'StunEndpoint'],
  NetBirdVpnConfig: ['VpnRouteConfig', 'VpnTunnelConfig'],
  NetBirdRouteValidation: ['RouteCheckResult', 'RouteViolation']
};

const REQUEST_ID = 'req-i3a';

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

function logs(): string[] {
  return (G['__NB_LOG'] as string[]) ?? [];
}

function resetLog(): void {
  G['__NB_LOG'] = [];
}

function linesWith(prefix: string): string[] {
  // The recording stub prefixes each line with '[<lvl>] ' — strip it so the
  // assertions match the REAL hilog format strings verbatim.
  return logs().filter((l: string): boolean => l.indexOf(prefix) >= 0)
    .map((l: string): string => l.slice(l.indexOf(prefix)));
}

function lastLine(prefix: string): string {
  const hits = linesWith(prefix);
  return hits.length === 0 ? '' : hits[hits.length - 1];
}

// One `key=value` field of a pipe-delimited marker line ('' when absent).
function fieldOf(line: string, key: string): string {
  const hit = line.split('|').find((f: string): boolean => f.indexOf(key + '=') === 0);
  return hit === undefined ? '' : hit.slice(key.length + 1);
}

function freshAbility(mod: Record<string, unknown>): Record<string, unknown> {
  const ability = new (mod['default'] as new () => Record<string, unknown>)();
  ability['requestId'] = REQUEST_ID;
  return ability;
}

// The live-session status the REAL arm gate reads (wg.device_up && wg.ready).
const WG_READY: Record<string, unknown> = { wg: { device_up: true, ready: true } };

// One watcher tick of the REAL arm hook.
function tickArm(ability: Record<string, unknown>, status: Record<string, unknown>): void {
  (ability['maybeArmN2hCollector'] as (s: unknown) => void)(status);
}

// The arm-file state globals of the fs stub, in one place: PRESENT true/false =
// the boolean answer the API gives; ACCESS_ERROR = the code a throw carries (a
// non-number token ⇒ the throw has NO code); both undefined = throw 13900002
// (a missing file as the device sees it).
function setArmFile(present: boolean | undefined, accessError: unknown): void {
  G['__NB_ARM_PRESENT'] = present;
  G['__NB_ARM_ACCESS_ERROR'] = accessError;
  G['__NB_ARM_TEXT'] = '';
  G['__NB_ARM_OPTS'] = undefined;
}

// The desired-route-set snapshot the REAL recreate leg consumes: two managed
// routes (one of them the default route the gate releases) — the exclusion
// entries come from the frozen set grafted by the leg itself.
function netcfgJson(): string {
  return JSON.stringify({
    available: true,
    serial: 9,
    address: '10.99.0.1',
    address_prefix_len: 32,
    routes: [
      { network: '172.16.0.0/12', is_default: false },
      { network: '0.0.0.0/0', is_default: true }
    ],
    dns: { service_enable: true, servers: [{ ip: '8.8.8.8', port: 53 }] },
    default_route: { allowed: true, reason: 'wg-tunnel-ready' },
    stuns: []
  });
}

// Drive one full recreate leg through the REAL performControlledRecreate.
// Per-leg counters are reset so every assertion below is about THIS leg.
async function driveRecreate(ability: Record<string, unknown>): Promise<void> {
  G['__NB_NETCFG'] = netcfgJson();
  G['__NB_FD_STATUS'] = { open: true, errno: 0 };
  G['__NB_CREATED_CONFIGS'] = [];
  G['__NB_OLD_DESTROYED'] = 0;
  // the OLD generation's connection (its destroy() is the recreate's step 2)
  ability['connection'] = {
    destroy: (): Promise<void> => {
      G['__NB_OLD_DESTROYED'] = (G['__NB_OLD_DESTROYED'] as number) + 1;
      return Promise.resolve();
    }
  };
  ability['createSettled'] = true;
  ability['tunFd'] = 42;
  // the data-plane feeds are preconditions of step 5, not of steps 1-4
  ability['outboundSocketFd'] = 7;
  ability['outboundProtected'] = true;
  await (ability['performControlledRecreate'] as (reason: string) => Promise<void>)(
    'desired-route-set-change');
}

try {
  const dir = mkdtempSync(join(tmpdir(), 'nb-exclusion-generation-'));
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
  const cfg = await import(pathToFileURL(join(dir, 'NetBirdVpnConfig.ts')).href) as
    Record<string, unknown>;

  // The merged config of the INITIAL generation, built by the REAL mapper:
  // base template + two frozen endpoint /32 exclusions, the core snapshot
  // applied, default route HELD (the evidence-round discipline). 4 rows, 3 of
  // them excluded.
  const derived = [
    { destination: '203.0.113.7', prefixLength: 32, gateway: '10.99.0.1',
      hasGateway: true, isDefaultRoute: false, isExcludedRoute: true },
    { destination: '198.51.100.4', prefixLength: 32, gateway: '10.99.0.1',
      hasGateway: true, isDefaultRoute: false, isExcludedRoute: true }
  ];
  const withExcl = (cfg['withEndpointExclusions'] as
    (base: unknown, e: unknown[]) => Record<string, unknown>)(
    cfg['loadVpnTunnelConfig'](), derived);
  const merged = (cfg['applyNetworkConfig'] as
    (base: unknown, snap: unknown) => Record<string, unknown>)(
    withExcl, JSON.parse(netcfgJson()) as never);
  const initialCfg = (cfg['gateDefaultRoutes'] as
    (c: unknown, allowed: boolean) => Record<string, unknown>)(merged, false);

  // ---------------- G1: the initial generation marker + its rows -----------
  const ability = freshAbility(mod);
  resetLog();
  const gen0 = (ability['logExclusionGeneration'] as
    (kind: string, routes: unknown[], k: number) => number)(
    'initial', initialCfg['routes'] as unknown[], 0);
  (ability['logRouteTable'] as (c: unknown, gen: number) => void)(initialCfg, gen0);
  const marker0 = linesWith('VPN_EXCLUSION_GENERATION|');
  check('G1 exactly ONE generation marker per create()',
    marker0.length === 1 && gen0 === 0, JSON.stringify(marker0));
  check('G1 marker literal (requestId|gen|kind|exclusions|wgPeerEndpoints|routes)',
    marker0[0] === 'VPN_EXCLUSION_GENERATION|requestId=' + REQUEST_ID +
    '|gen=0|kind=initial|exclusions=3|wgPeerEndpoints=0|routes=4', marker0.join('\n'));
  const rows0 = linesWith('VPN_ROUTE_ENTRY|');
  check('G1 row count == the announced routes= (§2.1 (2a)①)',
    rows0.length === 4 && String(rows0.length) === fieldOf(marker0[0], 'routes'),
    JSON.stringify(rows0));
  check('G1 index uniquely covers 0..r-1 (§2.1 (2a)②)',
    JSON.stringify(rows0.map((l: string): string => fieldOf(l, 'index'))) ===
    JSON.stringify(['0', '1', '2', '3']), JSON.stringify(rows0));
  check('G1 excluded rows == the announced exclusions= (§2.1 (2a)③)',
    rows0.filter((l: string): boolean => fieldOf(l, 'isExcludedRoute') === 'true').length === 3 &&
    String(rows0.filter((l: string): boolean =>
      fieldOf(l, 'isExcludedRoute') === 'true').length) === fieldOf(marker0[0], 'exclusions'),
    JSON.stringify(rows0));
  check('G1 every row carries the marker\'s gen (§2.1 (2))',
    rows0.every((l: string): boolean => fieldOf(l, 'gen') === '0'), JSON.stringify(rows0));
  check('G1 the initial generation holds no default route (§2.1 (6) precondition)',
    rows0.every((l: string): boolean => fieldOf(l, 'isDefaultRoute') !== 'true') &&
    fieldOf(marker0[0], 'kind') === 'initial', JSON.stringify(rows0));

  // ---------------- G2: the initial acceptance anchor ----------------------
  G['__NB_FD_STATUS'] = { open: true, errno: 0 };
  const outcome = await (ability['settleCreate'] as
    (fd: number, gen: number) => Promise<Record<string, unknown>>)(42, gen0);
  const anchor0 = lastLine('VPN_CREATE_RESOLVED|');
  check('G2 initial acceptance anchor is the existing VPN_CREATE_RESOLVED|accepted=true',
    anchor0 === 'VPN_CREATE_RESOLVED|requestId=' + REQUEST_ID +
    '|fd=42|accepted=true|marker=CREATE_ACCEPTED' && outcome['accepted'] === true,
    anchor0 + ' ' + JSON.stringify(outcome));
  check('G2 the anchor makes THAT generation the accepted one (single source)',
    ability['n2hAcceptedExclusionGeneration'] === 0 &&
    ability['n2hExclusionGeneration'] === 0,
    String(ability['n2hAcceptedExclusionGeneration']));
  const logText = logs().join('\n');
  check('G2 marker + all rows precede the anchor (§2.1 (4) ordering)',
    logText.indexOf(marker0[0]) >= 0 &&
    logText.indexOf(marker0[0]) < logText.indexOf(rows0[rows0.length - 1]) &&
    logText.indexOf(rows0[rows0.length - 1]) < logText.indexOf(anchor0),
    JSON.stringify(logs()));

  // ---------------- G3: a full recreate leg through the REAL code ----------
  // the frozen exclusion set the start flow retains (this is what the recreate
  // leg grafts onto the rebuilt config)
  ability['frozenEndpointExclusions'] = derived;
  const connectionsBefore = Number(G['__NB_CONNECTIONS'] ?? 0);
  resetLog();
  await driveRecreate(ability);
  const marker1 = linesWith('VPN_EXCLUSION_GENERATION|');
  check('G3 the recreate leg announces exactly one generation, kind=recreate',
    marker1.length === 1 && fieldOf(marker1[0], 'kind') === 'recreate' &&
    fieldOf(marker1[0], 'gen') === '1', marker1.join('\n'));
  check('G3 recreate marker counts match its OWN submitted config (5 rows / 3 excluded)',
    marker1[0] === 'VPN_EXCLUSION_GENERATION|requestId=' + REQUEST_ID +
    '|gen=1|kind=recreate|exclusions=3|wgPeerEndpoints=0|routes=5', marker1.join('\n'));
  const rows1 = linesWith('VPN_ROUTE_ENTRY|');
  check('G3 recreate rows: count/index/excluded match the marker, all gen=1',
    rows1.length === 5 &&
    JSON.stringify(rows1.map((l: string): string => fieldOf(l, 'index'))) ===
    JSON.stringify(['0', '1', '2', '3', '4']) &&
    rows1.filter((l: string): boolean => fieldOf(l, 'isExcludedRoute') === 'true').length === 3 &&
    rows1.every((l: string): boolean => fieldOf(l, 'gen') === '1'), JSON.stringify(rows1));
  const recreateAnchor = lastLine('VPN_RECREATE_RESOLVED|');
  check('G3 recreate acceptance anchor literal (§2.1 (4))',
    recreateAnchor === 'VPN_RECREATE_RESOLVED|requestId=' + REQUEST_ID +
    '|gen=1|accepted=true', recreateAnchor);
  check('G3 the recreate anchor makes gen 1 the accepted generation',
    ability['n2hAcceptedExclusionGeneration'] === 1 &&
    ability['n2hExclusionGeneration'] === 1,
    String(ability['n2hAcceptedExclusionGeneration']));
  const recreateLog = logs().join('\n');
  check('G3 generation marker + all rows precede the recreate anchor',
    recreateLog.indexOf(marker1[0]) < recreateLog.indexOf(rows1[rows1.length - 1]) &&
    recreateLog.indexOf(rows1[rows1.length - 1]) < recreateLog.indexOf(recreateAnchor),
    JSON.stringify(logs()));
  check('G3 the pre-existing recreate lines coexist unchanged',
    linesWith('VPN_RECREATE_SNAPSHOT|').length === 1 &&
    fieldOf(lastLine('VPN_RECREATE_SNAPSHOT|'), 'routes') === '5' &&
    fieldOf(lastLine('VPN_RECREATE_SNAPSHOT|'), 'frozenEndpointExclusions') === '2' &&
    fieldOf(lastLine('VPN_RECREATE_DONE|'), 'routes') === '5' &&
    fieldOf(lastLine('VPN_RECREATE_DONE|'), 'defaultRoutes') === '1',
    linesWith('VPN_RECREATE_SNAPSHOT|').join('\n') + '\n' +
    linesWith('VPN_RECREATE_DONE|').join('\n'));
  check('G3 the leg really ran to completion (fresh connection, destroy, ACK isRecreate=true)',
    (G['__NB_CREATED_CONFIGS'] as unknown[]).length === 1 &&
    G['__NB_OLD_DESTROYED'] === 1 &&
    Number(G['__NB_CONNECTIONS'] ?? 0) === connectionsBefore + 1 &&
    (G['__NB_ACK'] as Record<string, unknown>)['isRecreate'] === true,
    JSON.stringify({ created: (G['__NB_CREATED_CONFIGS'] as unknown[]).length,
      destroyed: G['__NB_OLD_DESTROYED'], connections: G['__NB_CONNECTIONS'],
      ack: G['__NB_ACK'] }));

  // ---------------- G4: a SECOND recreate -> gen=2, sequence 0,1,2 ---------
  const connectionsBefore2 = Number(G['__NB_CONNECTIONS'] ?? 0);
  resetLog();
  await driveRecreate(ability);
  const marker2 = linesWith('VPN_EXCLUSION_GENERATION|');
  check('G4 the second recreate is generation 2 (monotonic +1, no gap)',
    marker2.length === 1 && fieldOf(marker2[0], 'gen') === '2' &&
    fieldOf(marker2[0], 'kind') === 'recreate' &&
    (G['__NB_CREATED_CONFIGS'] as unknown[]).length === 1 &&
    G['__NB_OLD_DESTROYED'] === 1 &&
    Number(G['__NB_CONNECTIONS'] ?? 0) === connectionsBefore2 + 1,
    marker2.join('\n'));
  check('G4 its anchor is gen=2 and it becomes the accepted generation',
    lastLine('VPN_RECREATE_RESOLVED|') ===
    'VPN_RECREATE_RESOLVED|requestId=' + REQUEST_ID + '|gen=2|accepted=true' &&
    ability['n2hAcceptedExclusionGeneration'] === 2,
    lastLine('VPN_RECREATE_RESOLVED|'));
  check('G4 all three generations form the contiguous sequence 0,1,2 with kinds',
    String(gen0) + ',' + fieldOf(marker1[0], 'gen') + ',' + fieldOf(marker2[0], 'gen') ===
    '0,1,2' &&
    [fieldOf(marker0[0], 'kind'), fieldOf(marker1[0], 'kind'),
      fieldOf(marker2[0], 'kind')].join(',') === 'initial,recreate,recreate',
    [marker0[0], marker1[0], marker2[0]].join('\n'));

  // ---------------- G5: three-way R-2 consistency via the REAL arm hook ----
  G['__NB_ARM_PRESENT'] = true;
  G['__NB_ARM_TEXT'] = '';
  resetLog();
  (ability['maybeArmN2hCollector'] as (s: unknown) => void)(
    { wg: { device_up: true, ready: true } });
  const armed = lastLine('N2H_COLLECTOR_ARMED|');
  const armOpts = JSON.parse(String(G['__NB_ARM_OPTS'] ?? '{}')) as Record<string, unknown>;
  const armedGen = fieldOf(armed, 'gen');
  check('G5 the arm carries the accepted generation as collector.exclusion_generation',
    armOpts['exclusion_generation'] === 2, JSON.stringify(armOpts));
  check('G5 the ARMED marker carries the same gen= field (R-2②)',
    armedGen === '2' && armed.indexOf('|gen=2|') > 0, armed);
  check('G5 the LAST ACCEPTED generation line carries the same gen (R-2③ three-way)',
    armedGen === String(armOpts['exclusion_generation']) &&
    armedGen === fieldOf(marker2[0], 'gen') &&
    armedGen === String(ability['n2hAcceptedExclusionGeneration']),
    JSON.stringify({ armed: armedGen, armOpts: armOpts['exclusion_generation'],
      marker: fieldOf(marker2[0], 'gen') }));
  check('G5 the ARMED marker keeps its existing fields (gen= is an insertion)',
    armed.indexOf('|openedMono=4242|exclusions=') > 0 &&
    armed.indexOf('|posctlPort=40001|probeWaitMs=800|peerB2aProbeId=none|') > 0 &&
    armed.endsWith('|diag=true|is_evidence=false'), armed);

  // ---------------- G6: a NOT-accepted create() reports no generation ------
  const rejectAbility = freshAbility(mod);
  resetLog();
  const genR = (rejectAbility['logExclusionGeneration'] as
    (kind: string, routes: unknown[], k: number) => number)(
    'initial', initialCfg['routes'] as unknown[], 0);
  G['__NB_FD_STATUS'] = { open: false, errno: 9 };
  const rejected = await (rejectAbility['settleCreate'] as
    (fd: number, gen: number) => Promise<Record<string, unknown>>)(7, genR);
  check('G6 a create() that is not accepted is NOT the accepted generation',
    rejected['accepted'] === false &&
    lastLine('VPN_CREATE_RESOLVED|').indexOf('|accepted=false|marker=CREATE_INVALID') > 0 &&
    rejectAbility['n2hAcceptedExclusionGeneration'] === -1,
    lastLine('VPN_CREATE_RESOLVED|') + ' ' + String(
      rejectAbility['n2hAcceptedExclusionGeneration']));
  G['__NB_ARM_PRESENT'] = true;
  G['__NB_ARM_TEXT'] = '';
  G['__NB_ARM_OPTS'] = undefined;
  resetLog();
  (rejectAbility['maybeArmN2hCollector'] as (s: unknown) => void)(
    { wg: { device_up: true, ready: true } });
  const armedR = lastLine('N2H_COLLECTOR_ARMED|');
  const optsR = JSON.parse(String(G['__NB_ARM_OPTS'] ?? '{}')) as Record<string, unknown>;
  check('G6 with no accepted generation the arm omits the key (never fabricates one)',
    optsR['exclusion_generation'] === undefined && fieldOf(armedR, 'gen') === '-1',
    JSON.stringify(optsR) + ' ' + armedR);

  // ---------------- A (I5a §13.15): the arm check is never a silent branch --
  // ① the arm file is NOT there (the probe throws the documented ENOENT code,
  //    which is exactly what the 2026-09-18 device dry-run hit): the hook must
  //    leave ONE grep-able skip line naming the reason and the raw code, and
  //    must NOT call the core arm. The file body sentinel proves nothing about
  //    the file rides the marker.
  const absentAbility = freshAbility(mod);
  setArmFile(undefined, undefined);
  G['__NB_ARM_TEXT'] = 'SENTINEL-ARM-BODY';
  resetLog();
  tickArm(absentAbility, WG_READY);
  const skipLine = lastLine('N2H_COLLECTOR_ARM_SKIPPED|');
  check('A1 a missing arm file emits exactly ONE skip marker',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 1, JSON.stringify(logs()));
  check('A1 skip marker literal (reason=arm-file-absent + the raw access code)',
    skipLine === 'N2H_COLLECTOR_ARM_SKIPPED|requestId=' + REQUEST_ID +
    '|reason=arm-file-absent|path=/data/local/tmp/netbird-n2h-arm|accessError=13900002' +
    '|attempted=true|diag=true|is_evidence=false', skipLine);
  check('A1 the skip does NOT call the core arm (no ARMED / NOT_ARMED / no opts)',
    G['__NB_ARM_OPTS'] === undefined && linesWith('N2H_COLLECTOR_ARMED|').length === 0 &&
    linesWith('N2H_COLLECTOR_NOT_ARMED|').length === 0, JSON.stringify(logs()));
  check('A1 the attempt latch is visible on the line AND on the single latch field',
    fieldOf(skipLine, 'attempted') === 'true' && absentAbility['n2hArmAttempted'] === true,
    String(absentAbility['n2hArmAttempted']));
  check('A1 the skip line carries no arm-file body (nothing was read)',
    logs().join('\n').indexOf('SENTINEL-ARM-BODY') < 0, JSON.stringify(logs()));

  // ④ the one-shot latch: the ATTEMPT happens once per instance — the skip is
  //    printed on the first tick and never repeated, including a file that
  //    appears later on that same instance.
  resetLog();
  tickArm(absentAbility, WG_READY);
  check('A2 a second tick on the same instance repeats nothing (one-shot latch)',
    logs().length === 0, JSON.stringify(logs()));
  setArmFile(true, undefined);
  resetLog();
  tickArm(absentAbility, WG_READY);
  check('A2 a file appearing later never arms that instance (ONE latch, no second counter)',
    logs().length === 0 && G['__NB_ARM_OPTS'] === undefined &&
    absentAbility['n2hArmAttempted'] === true, JSON.stringify(logs()));

  // The API can also ANSWER "not found" as a boolean; the reason stays the same
  // and the difference is visible in accessError (none = no error was thrown).
  const falseAbility = freshAbility(mod);
  setArmFile(false, undefined);
  resetLog();
  tickArm(falseAbility, WG_READY);
  const falseLine = lastLine('N2H_COLLECTOR_ARM_SKIPPED|');
  check('A3 the boolean not-found answer is the SAME reason with accessError=none',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 1 &&
    falseLine === 'N2H_COLLECTOR_ARM_SKIPPED|requestId=' + REQUEST_ID +
    '|reason=arm-file-absent|path=/data/local/tmp/netbird-n2h-arm|accessError=none' +
    '|attempted=true|diag=true|is_evidence=false', falseLine);

  // An error code that does NOT prove absence must be a DIFFERENT reason — the
  // two cases may never collapse into one (13900012 = permission denied).
  const unreadableAbility = freshAbility(mod);
  setArmFile(undefined, 13900012);
  resetLog();
  tickArm(unreadableAbility, WG_READY);
  const unreadableLine = lastLine('N2H_COLLECTOR_ARM_SKIPPED|');
  check('A4 a non-ENOENT error is reason=arm-file-unreadable with its code kept',
    unreadableLine === 'N2H_COLLECTOR_ARM_SKIPPED|requestId=' + REQUEST_ID +
    '|reason=arm-file-unreadable|path=/data/local/tmp/netbird-n2h-arm|accessError=13900012' +
    '|attempted=true|diag=true|is_evidence=false', unreadableLine);
  const noCodeAbility = freshAbility(mod);
  setArmFile(undefined, 'no-code');
  resetLog();
  tickArm(noCodeAbility, WG_READY);
  const noCodeLine = lastLine('N2H_COLLECTOR_ARM_SKIPPED|');
  check('A4 an error thrown WITHOUT a code never claims absence (accessError=no-code)',
    noCodeLine === 'N2H_COLLECTOR_ARM_SKIPPED|requestId=' + REQUEST_ID +
    '|reason=arm-file-unreadable|path=/data/local/tmp/netbird-n2h-arm|accessError=no-code' +
    '|attempted=true|diag=true|is_evidence=false', noCodeLine);

  // ② ③ regressions: the file present keeps the EXISTING two branches verbatim
  // (and prints no skip line).
  const notReadyAbility = freshAbility(mod);
  setArmFile(true, undefined);
  resetLog();
  tickArm(notReadyAbility, { wg: { device_up: false, ready: true } });
  check('A5 file present + gate closed keeps the existing NOT_ARMED literal (regression)',
    linesWith('N2H_COLLECTOR_NOT_ARMED|').length === 1 &&
    lastLine('N2H_COLLECTOR_NOT_ARMED|') ===
    'N2H_COLLECTOR_NOT_ARMED|requestId=' + REQUEST_ID +
    '|reason=session-not-ready|deviceUp=false|ready=true|diag=true|is_evidence=false' &&
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 0, JSON.stringify(logs()));
  const notReadyAbility2 = freshAbility(mod);
  resetLog();
  tickArm(notReadyAbility2, { wg: { device_up: true, ready: false } });
  check('A5 the arm gate still requires BOTH device_up and ready (regression)',
    lastLine('N2H_COLLECTOR_NOT_ARMED|') ===
    'N2H_COLLECTOR_NOT_ARMED|requestId=' + REQUEST_ID +
    '|reason=session-not-ready|deviceUp=true|ready=false|diag=true|is_evidence=false' &&
    G['__NB_ARM_OPTS'] === undefined, JSON.stringify(logs()));
  const readyAbility = freshAbility(mod);
  readyAbility['n2hAcceptedExclusionGeneration'] = 0;
  resetLog();
  tickArm(readyAbility, WG_READY);
  check('A6 file present + gate open still ARMs, existing literal unchanged (regression)',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 0 &&
    lastLine('N2H_COLLECTOR_ARMED|') ===
    'N2H_COLLECTOR_ARMED|requestId=' + REQUEST_ID +
    '|gen=0|openedMono=4242|exclusions=0|posctlPort=40001|probeWaitMs=800|' +
    'peerB2aProbeId=none|diag=true|is_evidence=false', JSON.stringify(logs()));

  // ---------------- G7: wiring pins on the byte-identical source -----------
  const extSource: string = readFileSync(join(dir, 'NetBirdVpnExtensionAbility.ts')).toString();
  const initialCall = extSource.indexOf(
    'this.logExclusionGeneration(EXCLUSION_GENERATION_INITIAL, merged.routes, 0);');
  const initialDump = extSource.indexOf('this.logRouteTable(merged, gen);');
  const initialCreate = extSource.indexOf('const rawCreate: Promise<number> = connection.create(config);');
  const initialAnchor = extSource.indexOf('return this.settleCreate(fd, gen);');
  check('G7 initial leg: marker -> rows -> create() -> anchor, in that order',
    initialCall > 0 && initialCall < initialDump && initialCall < initialCreate &&
    initialCreate < initialAnchor, JSON.stringify({ initialCall, initialDump,
      initialCreate, initialAnchor }));
  const recreateCall = extSource.indexOf(
    'this.logExclusionGeneration(EXCLUSION_GENERATION_RECREATE, merged.routes, 0);');
  const recreateDump = extSource.indexOf('this.logRouteTable(merged, gen);', initialDump + 1);
  const recreateCreate = extSource.indexOf(
    'const fd: number = await this.createConnectionBounded(conn, config);');
  const recreateAnchorAt = extSource.indexOf(
    "'VPN_RECREATE_RESOLVED|requestId=%{public}s|gen=%{public}d|accepted=true'");
  check('G7 recreate leg: marker -> rows -> create() -> VPN_RECREATE_RESOLVED, in that order',
    recreateCall > 0 && recreateCall < recreateDump && recreateCall < recreateCreate &&
    recreateCreate < recreateAnchorAt, JSON.stringify({ recreateCall, recreateDump,
      recreateCreate, recreateAnchorAt }));
  check('G7 the initial acceptance anchor literal is UNCHANGED (no gen= added)',
    extSource.indexOf(
      "'VPN_CREATE_RESOLVED|requestId=%{public}s|fd=%{public}s|accepted=%{public}s|marker=%{public}s'")
    > 0, 'anchor pin');
  check('G7 the pre-existing recreate marker literals are UNCHANGED',
    extSource.indexOf(
      "'VPN_RECREATE_SNAPSHOT|requestId=%{public}s|serial=%{public}s|routes=%{public}d|'") > 0 &&
    extSource.indexOf(
      "'VPN_RECREATE_DONE|requestId=%{public}s|tunFd=%{public}d|routes=%{public}d|'") > 0,
    'recreate line pin');
  check('G7 the arm reads its generation from ONE field (R-2 same-source)',
    extSource.indexOf(
      'const exclusionGeneration: number = this.n2hAcceptedExclusionGeneration;') > 0 &&
    extSource.indexOf('opts.exclusion_generation = exclusionGeneration;') > 0 &&
    extSource.indexOf(
      "'N2H_COLLECTOR_ARMED|requestId=%{public}s|gen=%{public}d|openedMono=%{public}d|'") > 0,
    'arm wiring pin');
  check('G7 the wgPeerEndpoints field is passed at BOTH legs (value 0 until I3b)',
    extSource.indexOf('merged.routes, 0);') > 0 &&
    extSource.split('merged.routes, 0);').length === 3, 'wg_peer field pin');

  rmSync(dir, { recursive: true, force: true });
} catch (error) {
  failedCount++;
  console.log('FAIL harness — ' + (error as Error).message + '\n' +
    ((error as Error).stack ?? ''));
}

console.log('');
console.log(String(passed) + ' passed, ' + String(failedCount) + ' failed');
process.exit(failedCount === 0 ? 0 : 1);
