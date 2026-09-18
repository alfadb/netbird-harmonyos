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
//   B1-B6 (I5b §13.16): the two PATH beacons — `stage=onCreate-reached` at the
//      extension's earliest reachable lifecycle point (BEFORE the duplicate
//      onCreate early return) and `stage=watcher-started` at the status
//      watcher's start (after its existing once-guard, outside the tick) —
//      each emit EXACTLY one line per instance with the five-field
//      `requestId|stage|diag|is_evidence` shape and nothing else; the onCreate
//      beacon survives the async start flow; the watcher beacon is not
//      re-printed by a second start call nor by three REPLAYED REAL ticks. The
//      three-state reading matrix: beacons + arm file absent ⇒ exactly one
//      ARM_SKIPPED (I5a, through the REAL tick); beacons + zero ticks ⇒ the
//      beacons and NO arm line; neither beacon ⇒ no N2H_COLLECTOR_PATH line at
//      all (extension process never started). Source pins close the group.
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
//   C1-C6 (N2-H arm private channel, 2026-09-18): the arm ticket moved from
//      the app-invisible /data/local/tmp path into the app's OWN filesDir,
//      written by the REAL EntryAbility from the non-secret `n2h_arm_options`
//      want parameter and read by the REAL extension under the SAME basename
//      (`netbird-n2h-arm`) and the SAME filesDir mechanism as the credentials:
//      C1 the want payload lands byte-verbatim in filesDir, logged as
//      file-name + UTF-8 byte count with the payload never printed (C1b: a
//      multibyte payload pins the UTF-8 count against UTF-16 impersonation);
//      C2 the
//      extension's probe + read resolve <filesDir>/netbird-n2h-arm and the
//      EXISTING arm options (posctl_port / probe_wait_ms /
//      peer_b2a_probe_id) reach the core n2h_collector_arm unchanged; C3 a
//      launch WITHOUT the parameter deletes a leftover ticket (default: no
//      arm) while every credential/config/deviation file stays byte-identical;
//      C4 an invalid-JSON payload keeps the ORIGINAL convention
//      (ARM_OPTS_IGNORED + §5.1 defaults, honestly logged, still no fabricated
//      success); C5/C5b a failed OPEN and a failed/short WRITE after the
//      TRUNC-ing open each leave NO usable ticket (the latter removes exactly
//      the wiped diagnostic file, logged independently with codes) and the
//      extension then reports arm-file-absent (never a faked arm); C5c a
//      failed CLEANUP is logged as REMOVE_FAILED with its code — never claimed
//      as removed; C6 the per-instance
//      one-shot arm attempt is intact on the new channel (a second tick calls
//      the core again ZERO times). Source pins: the extension no longer
//      references /data/local/tmp/netbird-n2h-arm anywhere, the credential
//      dev-override channel is untouched.
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
// The fixture EntryAbility lives in its own source dir (arm private channel,
// C group): staged byte-identically like the five extension modules.
const REAL_ENTRY_DIR = join(HERE, '..', 'src', 'main', 'ets', 'entryability');

// The filesDir every arm-driven fixture instance shares — the string the B
// group already used; the fake fs (exclusion-generation-hooks.mjs) keys its
// in-memory map by the RESOLVED path, so the extension's real
// `<context.filesDir>/netbird-n2h-arm` join and the fixture EntryAbility's
// own writes land on one and the same file.
const FIXTURE_FILESDIR = '/tmp/n2h-i5b-fixture';
const ARM_TICKET_PATH = `${FIXTURE_FILESDIR}/netbird-n2h-arm`;
const ARM_TICKET_NAME = 'netbird-n2h-arm';

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

function freshAbility(mod: Record<string, unknown>,
  filesDir?: string): Record<string, unknown> {
  const ability = new (mod['default'] as new () => Record<string, unknown>)();
  ability['requestId'] = REQUEST_ID;
  // Arm-driven instances need the context the REAL lifecycle always provides;
  // the B-group lifecycle beacons deliberately keep the context ABSENT (their
  // "async start flow aborts on the fixture's absent context" regression).
  if (filesDir !== undefined) {
    ability['context'] = { filesDir: filesDir };
  }
  return ability;
}

// The live-session status the REAL arm gate reads (wg.device_up && wg.ready).
const WG_READY: Record<string, unknown> = { wg: { device_up: true, ready: true } };

// One watcher tick of the REAL arm hook.
function tickArm(ability: Record<string, unknown>, status: Record<string, unknown>): void {
  (ability['maybeArmN2hCollector'] as (s: unknown) => void)(status);
}

// The arm-file state of the path-aware fs stub (exclusion-generation-hooks.mjs),
// in one place: the ticket is a REAL entry in the in-memory map keyed by the
// RESOLVED path (present=true puts `text` there, present=false removes it,
// present=undefined leaves the map untouched for a pre-seeded body), and
// accessError simulates the code an accessSync throw carries ('no-code' = a
// throw without a code). No file + no error = the boolean-false answer the
// 2026-09-18 device actually produced (accessError=none).
function setArmFile(present: boolean | undefined, accessError: unknown,
  text: string = ''): void {
  const files = (G['__NB_FS'] ?? {}) as Record<string, string>;
  const errs = (G['__NB_FS_ACCESS_ERROR'] ?? {}) as Record<string, unknown>;
  if (present === true) {
    files[ARM_TICKET_PATH] = text;
  } else if (present === false) {
    delete files[ARM_TICKET_PATH];
  }
  if (accessError === undefined) {
    delete errs[ARM_TICKET_PATH];
  } else {
    errs[ARM_TICKET_PATH] = accessError;
  }
  G['__NB_FS'] = files;
  G['__NB_FS_ACCESS_ERROR'] = errs;
  G['__NB_ARM_OPTS'] = undefined;
}

function seedFile(path: string, text: string): void {
  const files = (G['__NB_FS'] ?? {}) as Record<string, string>;
  files[path] = text;
  G['__NB_FS'] = files;
}

function fileOf(path: string): string | undefined {
  return ((G['__NB_FS'] ?? {}) as Record<string, string>)[path];
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
  // The fixture EntryAbility (arm private channel, C group): byte-identical
  // real file, no sibling imports, no shim needed.
  writeFileSync(join(dir, 'EntryAbility.ts'),
    readFileSync(join(REAL_ENTRY_DIR, 'EntryAbility.ets')));
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
  const ability = freshAbility(mod, FIXTURE_FILESDIR);
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
  setArmFile(true, undefined);
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
  const rejectAbility = freshAbility(mod, FIXTURE_FILESDIR);
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
  setArmFile(true, undefined);
  resetLog();
  (rejectAbility['maybeArmN2hCollector'] as (s: unknown) => void)(
    { wg: { device_up: true, ready: true } });
  const armedR = lastLine('N2H_COLLECTOR_ARMED|');
  const optsR = JSON.parse(String(G['__NB_ARM_OPTS'] ?? '{}')) as Record<string, unknown>;
  check('G6 with no accepted generation the arm omits the key (never fabricates one)',
    optsR['exclusion_generation'] === undefined && fieldOf(armedR, 'gen') === '-1',
    JSON.stringify(optsR) + ' ' + armedR);

  // ---------------- A (I5a §13.15): the arm check is never a silent branch --
  // ① the arm file probe THROWS the documented ENOENT code: the hook must
  //    leave ONE grep-able skip line naming the reason and the raw code, and
  //    must NOT call the core arm. The ticket body seeded in the fs map proves
  //    nothing about the file rides the marker (the probe threw before any
  //    read). The path on the line is the RESOLVED filesDir join — the same
  //    basename the fixture EntryAbility writes (C group).
  const absentAbility = freshAbility(mod, FIXTURE_FILESDIR);
  setArmFile(true, 13900002, 'SENTINEL-ARM-BODY');
  resetLog();
  tickArm(absentAbility, WG_READY);
  const skipLine = lastLine('N2H_COLLECTOR_ARM_SKIPPED|');
  check('A1 a missing arm file emits exactly ONE skip marker',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 1, JSON.stringify(logs()));
  check('A1 skip marker literal (reason=arm-file-absent + the raw access code + the resolved ' +
    'filesDir path)',
    skipLine === 'N2H_COLLECTOR_ARM_SKIPPED|requestId=' + REQUEST_ID +
    '|reason=arm-file-absent|path=' + ARM_TICKET_PATH + '|accessError=13900002' +
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

  // The API can also ANSWER "not found" as a boolean — the shape the 2026-09-18
  // device run actually produced (accessError=none; NOT a proven ENOENT). The
  // reason stays the same and the difference is visible in accessError.
  const falseAbility = freshAbility(mod, FIXTURE_FILESDIR);
  setArmFile(false, undefined);
  resetLog();
  tickArm(falseAbility, WG_READY);
  const falseLine = lastLine('N2H_COLLECTOR_ARM_SKIPPED|');
  check('A3 the boolean not-found answer is the SAME reason with accessError=none',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 1 &&
    falseLine === 'N2H_COLLECTOR_ARM_SKIPPED|requestId=' + REQUEST_ID +
    '|reason=arm-file-absent|path=' + ARM_TICKET_PATH + '|accessError=none' +
    '|attempted=true|diag=true|is_evidence=false', falseLine);

  // An error code that does NOT prove absence must be a DIFFERENT reason — the
  // two cases may never collapse into one (13900012 = permission denied).
  const unreadableAbility = freshAbility(mod, FIXTURE_FILESDIR);
  setArmFile(false, 13900012);
  resetLog();
  tickArm(unreadableAbility, WG_READY);
  const unreadableLine = lastLine('N2H_COLLECTOR_ARM_SKIPPED|');
  check('A4 a non-ENOENT error is reason=arm-file-unreadable with its code kept',
    unreadableLine === 'N2H_COLLECTOR_ARM_SKIPPED|requestId=' + REQUEST_ID +
    '|reason=arm-file-unreadable|path=' + ARM_TICKET_PATH + '|accessError=13900012' +
    '|attempted=true|diag=true|is_evidence=false', unreadableLine);
  const noCodeAbility = freshAbility(mod, FIXTURE_FILESDIR);
  setArmFile(false, 'no-code');
  resetLog();
  tickArm(noCodeAbility, WG_READY);
  const noCodeLine = lastLine('N2H_COLLECTOR_ARM_SKIPPED|');
  check('A4 an error thrown WITHOUT a code never claims absence (accessError=no-code)',
    noCodeLine === 'N2H_COLLECTOR_ARM_SKIPPED|requestId=' + REQUEST_ID +
    '|reason=arm-file-unreadable|path=' + ARM_TICKET_PATH + '|accessError=no-code' +
    '|attempted=true|diag=true|is_evidence=false', noCodeLine);

  // ② ③ regressions: the file present keeps the EXISTING two branches verbatim
  // (and prints no skip line).
  const notReadyAbility = freshAbility(mod, FIXTURE_FILESDIR);
  setArmFile(true, undefined);
  resetLog();
  tickArm(notReadyAbility, { wg: { device_up: false, ready: true } });
  check('A5 file present + gate closed keeps the existing NOT_ARMED literal (regression)',
    linesWith('N2H_COLLECTOR_NOT_ARMED|').length === 1 &&
    lastLine('N2H_COLLECTOR_NOT_ARMED|') ===
    'N2H_COLLECTOR_NOT_ARMED|requestId=' + REQUEST_ID +
    '|reason=session-not-ready|deviceUp=false|ready=true|diag=true|is_evidence=false' &&
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 0, JSON.stringify(logs()));
  const notReadyAbility2 = freshAbility(mod, FIXTURE_FILESDIR);
  resetLog();
  tickArm(notReadyAbility2, { wg: { device_up: true, ready: false } });
  check('A5 the arm gate still requires BOTH device_up and ready (regression)',
    lastLine('N2H_COLLECTOR_NOT_ARMED|') ===
    'N2H_COLLECTOR_NOT_ARMED|requestId=' + REQUEST_ID +
    '|reason=session-not-ready|deviceUp=true|ready=false|diag=true|is_evidence=false' &&
    G['__NB_ARM_OPTS'] === undefined, JSON.stringify(logs()));
  const readyAbility = freshAbility(mod, FIXTURE_FILESDIR);
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

  // ---------------- B (I5b §13.16): the two PATH beacons + 三态矩阵 ---------
  // The watcher's interval is CAPTURED, never scheduled: the B group replays
  // REAL ticks by hand, which is precisely the "the watcher was started but
  // never ticked" form the device dry-run could not distinguish from "the hook
  // ran and skipped". No real timer is left pending (the G legs never reach the
  // watcher: their protect gate fails closed first on the absent wg_fwd_open).
  const capturedTicks: Array<() => void> = [];
  G['setInterval'] = (handler: () => void): number => {
    capturedTicks.push(handler);
    return 7001;
  };
  // The live-session document the REAL tick polls: wg up + ready, not terminal,
  // no recreate ⇒ the tick reaches the arm hook on every replay.
  const watcherStatus: Record<string, unknown> = {
    running: true, state: 'connected', peer_count: 1, route_count: 2,
    reconnects: 0, terminal: false, wg: { device_up: true, ready: true }
  };
  const pathLines = (stage: string): string[] => linesWith('N2H_COLLECTOR_PATH|')
    .filter((l: string): boolean => fieldOf(l, 'stage') === stage);

  // B1: the onCreate beacon through the REAL onCreate (normal path, its async
  // start flow aborts on the fixture's absent context — caught by onCreate).
  const bornAbility = freshAbility(mod);
  resetLog();
  (bornAbility['onCreate'] as (w: unknown) => void)(
    { bundleName: 'com.n2h.fixture', parameters: { requestId: REQUEST_ID } });
  const onCreateBeacons = pathLines('onCreate-reached');
  const onCreateLine: string = onCreateBeacons.length > 0 ? onCreateBeacons[0] : '';
  check('B1 onCreate emits exactly ONE stage=onCreate-reached beacon',
    onCreateBeacons.length === 1, JSON.stringify(linesWith('N2H_COLLECTOR_PATH|')));
  check('B1 onCreate beacon literal (5 fields: requestId|stage|diag|is_evidence)',
    onCreateLine === 'N2H_COLLECTOR_PATH|requestId=' + REQUEST_ID +
    '|stage=onCreate-reached|diag=true|is_evidence=false' &&
    onCreateLine.split('|').length === 5, onCreateLine);
  check('B1 the beacon carries the requestId resolved from the want (never the default)',
    fieldOf(onCreateLine, 'requestId') === REQUEST_ID &&
    fieldOf(onCreateLine, 'requestId') !== 'missing', onCreateLine);
  await new Promise<void>((resolve: () => void): void => { setTimeout(resolve, 0); });
  check('B1 the async start flow of that onCreate never re-prints the beacon',
    pathLines('onCreate-reached').length === 1, JSON.stringify(logs()));

  // B2: the beacon precedes EVERY early return — the earliest reachable point.
  const dupAbility = freshAbility(mod);
  dupAbility['connectionCreationAttempted'] = true;
  resetLog();
  (dupAbility['onCreate'] as (w: unknown) => void)(
    { bundleName: 'com.n2h.fixture', parameters: { requestId: REQUEST_ID } });
  check('B2 the beacon is at the entry: it precedes the duplicate-onCreate return',
    linesWith('N2H_COLLECTOR_PATH|').length === 1 &&
    lastLine('VPN_CREATE_REJECTED|') === 'VPN_CREATE_REJECTED|requestId=' + REQUEST_ID +
    '|phase=duplicate-onCreate|summary=connection-already-attempted', JSON.stringify(logs()));

  // B3: the watcher beacon — one line per instance, never per tick, through the
  // REAL start method + REPLAYED REAL ticks (matrix ①: beacons + absent file).
  const tickAbility = freshAbility(mod, FIXTURE_FILESDIR);
  G['__NB_STATUS'] = watcherStatus;
  setArmFile(false, undefined); // the device form: boolean false (accessError=none)
  resetLog();
  (tickAbility['startStatusAndNetcfgWatcher'] as () => void)();
  const watcherBeaconLine: string = pathLines('watcher-started').length > 0
    ? pathLines('watcher-started')[0] : '';
  check('B3 the watcher start emits exactly ONE stage=watcher-started beacon',
    pathLines('watcher-started').length === 1 && capturedTicks.length === 1,
    JSON.stringify({ beacons: pathLines('watcher-started').length,
      ticks: capturedTicks.length }));
  check('B3 watcher beacon literal (5 fields: requestId|stage|diag|is_evidence)',
    watcherBeaconLine === 'N2H_COLLECTOR_PATH|requestId=' + REQUEST_ID +
    '|stage=watcher-started|diag=true|is_evidence=false' &&
    watcherBeaconLine.split('|').length === 5, watcherBeaconLine);
  (tickAbility['startStatusAndNetcfgWatcher'] as () => void)();
  check('B3 a second start call re-prints nothing (the existing once-guard, no new latch)',
    pathLines('watcher-started').length === 1 && capturedTicks.length === 1,
    JSON.stringify({ beacons: pathLines('watcher-started').length,
      ticks: capturedTicks.length }));
  for (let i = 0; i < 3; i++) {
    capturedTicks[0]();
  }
  check('B3 three REPLAYED real ticks keep ONE watcher beacon (never per-tick spam)',
    pathLines('watcher-started').length === 1, JSON.stringify(linesWith('N2H_COLLECTOR_PATH|')));
  check('B3 matrix ①: beacons + arm file absent ⇒ exactly ONE ARM_SKIPPED (I5a regression)',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 1 &&
    lastLine('N2H_COLLECTOR_ARM_SKIPPED|') === 'N2H_COLLECTOR_ARM_SKIPPED|requestId=' +
    REQUEST_ID + '|reason=arm-file-absent|path=' + ARM_TICKET_PATH + '|' +
    'accessError=none|attempted=true|diag=true|is_evidence=false',
    JSON.stringify(linesWith('N2H_COLLECTOR_ARM_SKIPPED|')));

  // B4: matrix ② — the extension AND the watcher were reached, but the watcher
  // never ticked: both beacons, and NOT ONE arm line.
  const idleAbility = freshAbility(mod, FIXTURE_FILESDIR);
  setArmFile(false, undefined);
  resetLog();
  (idleAbility['onCreate'] as (w: unknown) => void)(
    { bundleName: 'com.n2h.fixture', parameters: { requestId: REQUEST_ID } });
  (idleAbility['startStatusAndNetcfgWatcher'] as () => void)();
  check('B4 matrix ②: beacons present + ZERO ticks ⇒ both beacons and NO arm line',
    JSON.stringify(linesWith('N2H_COLLECTOR_PATH|')
      .map((l: string): string => fieldOf(l, 'stage'))) ===
    JSON.stringify(['onCreate-reached', 'watcher-started']) &&
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 0 &&
    linesWith('N2H_COLLECTOR_NOT_ARMED|').length === 0 &&
    linesWith('N2H_COLLECTOR_ARMED|').length === 0, JSON.stringify(logs()));

  // B5: matrix ③ — the extension process never started: no beacon at all.
  const silentAbility = freshAbility(mod);
  resetLog();
  check('B5 matrix ③: an instance that received no lifecycle call emits NO beacon',
    silentAbility['requestId'] === REQUEST_ID && linesWith('N2H_COLLECTOR_PATH|').length === 0 &&
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 0, JSON.stringify(logs()));

  // B6: source pins on the byte-identical fixture (positions + single copies).
  const onCreatePathAt = extSource.indexOf(
    "'N2H_COLLECTOR_PATH|requestId=%{public}s|stage=onCreate-reached|diag=true|is_evidence=false'");
  const duplicateRejectAt = extSource.indexOf(
    "'VPN_CREATE_REJECTED|requestId=%{public}s|phase=duplicate-onCreate|summary=connection-already-attempted'");
  check('B6 the onCreate beacon is pinned BEFORE the duplicate-onCreate early return',
    onCreatePathAt > 0 && onCreatePathAt < duplicateRejectAt,
    JSON.stringify({ onCreatePathAt, duplicateRejectAt }));
  const watcherPathAt = extSource.indexOf(
    "'N2H_COLLECTOR_PATH|requestId=%{public}s|stage=watcher-started|diag=true|is_evidence=false'");
  const watcherGuardAt = extSource.indexOf(
    'if (this.watcherTimer >= 0 || this.destroyRequested) {');
  const intervalAt = extSource.indexOf('}, STATUS_POLL_MS);');
  check('B6 the watcher beacon is pinned AFTER the once-guard and the interval creation',
    watcherPathAt > 0 && watcherPathAt > watcherGuardAt && watcherPathAt > intervalAt,
    JSON.stringify({ watcherPathAt, watcherGuardAt, intervalAt }));
  check('B6 each beacon literal exists exactly ONCE in the shipped source (no extra copy)',
    extSource.split('stage=onCreate-reached').length === 2 &&
    extSource.split('stage=watcher-started').length === 2, 'beacon literal pin');

  // ---------------- C (N2-H arm private channel, 2026-09-18): the REAL
  // EntryAbility want -> filesDir ticket -> REAL extension read -> native arm.
  // Both fixtures are the SHIPPED files; the path-aware fake fs keys its map
  // by the RESOLVED path, so the two processes meet on ONE file — exactly the
  // cross-process filesDir mechanism the device already validated for the
  // credentials.
  const entryMod = await import(pathToFileURL(join(dir, 'EntryAbility.ts')).href) as
    Record<string, unknown>;
  const ARM_PAYLOAD = '{"posctl_port":41000,"probe_wait_ms":900,' +
    '"peer_b2a_probe_id":"N2H-posctl-fixture"}';
  const newEntry = (): Record<string, unknown> => {
    const entry = new (entryMod['default'] as new () => Record<string, unknown>)();
    entry['context'] = { filesDir: FIXTURE_FILESDIR };
    return entry;
  };
  const resetFs = (): void => {
    G['__NB_FS'] = {};
    G['__NB_FS_ACCESS_ERROR'] = {};
    G['__NB_FS_OPEN_ERROR'] = {};
    G['__NB_FS_WRITE_ERROR'] = {};
    G['__NB_ARM_OPTS'] = undefined;
  };

  // C1: the explicit non-secret want lands byte-verbatim in filesDir, logged
  // as file name + byte count, payload never printed. The count is the payload's
  // real UTF-8 byte length (never the UTF-16 code-unit string length).
  resetFs();
  const writeEntry = newEntry();
  resetLog();
  (writeEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'n2h_arm_options': ARM_PAYLOAD } }, {});
  check('C1 the want payload is written VERBATIM to <filesDir>/netbird-n2h-arm',
    fileOf(ARM_TICKET_PATH) === ARM_PAYLOAD, JSON.stringify(fileOf(ARM_TICKET_PATH)));
  check('C1 the write is logged as file name + UTF-8 byte count only',
    linesWith('N2H_ARM_TICKET_WRITTEN|').length === 1 &&
    lastLine('N2H_ARM_TICKET_WRITTEN|') === 'N2H_ARM_TICKET_WRITTEN|file=' + ARM_TICKET_NAME +
    '|bytes=' + String(Buffer.byteLength(ARM_PAYLOAD, 'utf8')) +
    '|note=diagnostic-only-non-secret',
    linesWith('N2H_ARM_TICKET_WRITTEN|').join('\n'));
  check('C1 the payload never reaches the log',
    logs().join('\n').indexOf('N2H-posctl-fixture') < 0, JSON.stringify(logs()));

  // C1b: the logged byte count is UTF-8 bytes — a multibyte payload exposes any
  // UTF-16 code-unit impersonation (2 chars cost 6 UTF-8 bytes, not 2).
  const UNI_PAYLOAD = '{"peer_b2a_probe_id":"N2H-λ-超"}';
  resetLog();
  (writeEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'n2h_arm_options': UNI_PAYLOAD } }, {});
  check('C1b the byte count is UTF-8 (multibyte payload), file byte-verbatim',
    fileOf(ARM_TICKET_PATH) === UNI_PAYLOAD &&
    lastLine('N2H_ARM_TICKET_WRITTEN|').indexOf('|bytes=' +
      String(Buffer.byteLength(UNI_PAYLOAD, 'utf8')) + '|') > 0 &&
    Buffer.byteLength(UNI_PAYLOAD, 'utf8') !== UNI_PAYLOAD.length,
    lastLine('N2H_ARM_TICKET_WRITTEN|') + ' vs len=' + UNI_PAYLOAD.length);
  seedFile(ARM_TICKET_PATH, ARM_PAYLOAD); // C2 reads the C1 payload, not C1b's

  // C2: the REAL extension probes + reads the SAME basename under the SAME
  // filesDir and forwards the EXISTING arm options to the native arm.
  const channelAbility = freshAbility(mod, FIXTURE_FILESDIR);
  channelAbility['n2hAcceptedExclusionGeneration'] = 0;
  resetLog();
  tickArm(channelAbility, WG_READY);
  const channelOpts = JSON.parse(String(G['__NB_ARM_OPTS'] ?? '{}')) as Record<string, unknown>;
  check('C2 the extension reads the filesDir ticket and sends the existing arm options to native',
    channelOpts['posctl_port'] === 41000 && channelOpts['probe_wait_ms'] === 900 &&
    channelOpts['peer_b2a_probe_id'] === 'N2H-posctl-fixture',
    JSON.stringify(channelOpts));
  check('C2 the ARMED marker carries the announced peer and the non-default knobs',
    lastLine('N2H_COLLECTOR_ARMED|') === 'N2H_COLLECTOR_ARMED|requestId=' + REQUEST_ID +
    '|gen=0|openedMono=4242|exclusions=0|posctlPort=41000|probeWaitMs=900|' +
    'peerB2aProbeId=N2H-posctl-fixture|diag=true|is_evidence=false',
    lastLine('N2H_COLLECTOR_ARMED|'));
  check('C2 a present ticket produces NO skip line',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 0, JSON.stringify(logs()));

  // C3: default no arm — a launch WITHOUT the parameter deletes a leftover
  // ticket; credential/config/deviation files stay byte-identical.
  const SETUP_PATH = `${FIXTURE_FILESDIR}/netbird-setup-key`;
  const DEVCFG_PATH = `${FIXTURE_FILESDIR}/netbird-device-config.json`;
  const DEVIATION_PATH = `${FIXTURE_FILESDIR}/netbird-protect-deviation`;
  seedFile(ARM_TICKET_PATH, '{"posctl_port":41000}'); // leftover diagnostic round
  seedFile(SETUP_PATH, 'setup-key-body-untouched');
  seedFile(DEVCFG_PATH, '{"management_url":"http://m","private_key":"pk-body"}');
  seedFile(DEVIATION_PATH, 'route-exclusion');
  const normalEntry = newEntry();
  resetLog();
  (normalEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird' }, {}); // NO parameters at all
  check('C3 a parameter-less launch deletes the leftover ticket (default: no arm)',
    fileOf(ARM_TICKET_PATH) === undefined &&
    linesWith('N2H_ARM_TICKET_REMOVED|').length === 1 &&
    lastLine('N2H_ARM_TICKET_REMOVED|') === 'N2H_ARM_TICKET_REMOVED|file=' + ARM_TICKET_NAME +
    '|note=default-no-arm', JSON.stringify(logs()));
  check('C3 the credential/config/deviation files are NOT touched by the cleanup',
    fileOf(SETUP_PATH) === 'setup-key-body-untouched' &&
    fileOf(DEVCFG_PATH) === '{"management_url":"http://m","private_key":"pk-body"}' &&
    fileOf(DEVIATION_PATH) === 'route-exclusion', 'private files pin');
  const cleanAbility = freshAbility(mod, FIXTURE_FILESDIR);
  G['__NB_ARM_OPTS'] = undefined;
  resetLog();
  tickArm(cleanAbility, WG_READY);
  check('C3 after the cleanup the extension reports arm-file-absent and never arms',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 1 &&
    lastLine('N2H_COLLECTOR_ARM_SKIPPED|').indexOf('|reason=arm-file-absent|') > 0 &&
    G['__NB_ARM_OPTS'] === undefined, JSON.stringify(logs()));
  // A credentials-bearing launch WITHOUT the arm param behaves the same. (Its
  // credential writes follow the PRE-EXISTING flow verbatim — including
  // netbird_config overwriting netbird-device-config.json.)
  const credEntry = newEntry();
  resetLog();
  (credEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'netbird_setup_key': 'K2', 'netbird_config': '{"a":1}' } }, {});
  check('C3 a credential launch without the arm param also clears the ticket, credentials intact',
    fileOf(ARM_TICKET_PATH) === undefined && fileOf(SETUP_PATH) === 'K2\n' &&
    fileOf(DEVCFG_PATH) === '{"a":1}' && fileOf(DEVIATION_PATH) === 'route-exclusion',
    JSON.stringify({ setup: fileOf(SETUP_PATH), devcfg: fileOf(DEVCFG_PATH),
      arm: fileOf(ARM_TICKET_PATH) }));

  // C4: an invalid-JSON payload keeps the ORIGINAL convention — the file is
  // written verbatim, the extension logs ARM_OPTS_IGNORED and falls back to
  // the §5.1 defaults; the ARMED marker shows the defaults (a REAL arming
  // under defaults, never the refused options smuggled through as applied).
  resetFs();
  const badEntry = newEntry();
  (badEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird', parameters: { 'n2h_arm_options': '{not-json' } }, {});
  check('C4 an invalid-JSON payload is written verbatim (the extension owns the parse)',
    fileOf(ARM_TICKET_PATH) === '{not-json', JSON.stringify(fileOf(ARM_TICKET_PATH)));
  const badAbility = freshAbility(mod, FIXTURE_FILESDIR);
  badAbility['n2hAcceptedExclusionGeneration'] = 0;
  resetLog();
  tickArm(badAbility, WG_READY);
  check('C4 invalid JSON keeps the original convention: ARM_OPTS_IGNORED + defaults, still armed',
    linesWith('N2H_COLLECTOR_ARM_OPTS_IGNORED|').length === 1 &&
    fieldOf(lastLine('N2H_COLLECTOR_ARM_OPTS_IGNORED|'), 'action') === 'defaults' &&
    lastLine('N2H_COLLECTOR_ARMED|').indexOf('|posctlPort=40001|probeWaitMs=800|') > 0 &&
    lastLine('N2H_COLLECTOR_ARMED|').indexOf('peerB2aProbeId=none|') > 0,
    JSON.stringify(logs()));

  // C5: a failed OPEN leaves NO ticket and is logged with its error CODE —
  // the extension then sees arm-file-absent; nobody fakes an arm.
  resetFs();
  G['__NB_FS_OPEN_ERROR'] = { [ARM_TICKET_PATH]: 13900042 };
  const failEntry = newEntry();
  resetLog();
  (failEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird', parameters: { 'n2h_arm_options': ARM_PAYLOAD } }, {});
  check('C5 a failed open leaves NO ticket and logs the error CODE (never the payload)',
    fileOf(ARM_TICKET_PATH) === undefined &&
    lastLine('N2H_ARM_TICKET_WRITE_FAILED|') === 'N2H_ARM_TICKET_WRITE_FAILED|file=' +
    ARM_TICKET_NAME + '|code=13900042|note=no-ticket-no-arm' &&
    linesWith('N2H_ARM_TICKET_REMOVED|').length === 0,
    linesWith('N2H_ARM_TICKET_WRITE_FAILED|').join('\n') + '\n' +
    linesWith('N2H_ARM_TICKET_REMOVED|').join('\n'));
  const failAbility = freshAbility(mod, FIXTURE_FILESDIR);
  resetLog();
  tickArm(failAbility, WG_READY);
  check('C5 after a failed open the extension reports arm-file-absent (no faked arm)',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 1 &&
    lastLine('N2H_COLLECTOR_ARM_SKIPPED|').indexOf('|reason=arm-file-absent|') > 0 &&
    G['__NB_ARM_OPTS'] === undefined, JSON.stringify(logs()));

  // C5b: the dangerous shape — the OPEN succeeded (its TRUNC wiped the
  // previous round's ticket) and the WRITE then threw: the on-disk file is an
  // empty ticket the extension would parse into the §5.1 defaults and ARM.
  // The failure path must REMOVE exactly this one diagnostic ticket (logged
  // independently), never touch the credentials, and the extension must then
  // SKIP with no native arm call.
  resetFs();
  seedFile(ARM_TICKET_PATH, '{"posctl_port":41000}'); // the previous ticket, now TRUNC-wiped
  seedFile(SETUP_PATH, 'setup-key-body-untouched');
  seedFile(DEVCFG_PATH, '{"management_url":"http://m","private_key":"pk-body"}');
  seedFile(DEVIATION_PATH, 'route-exclusion');
  G['__NB_FS_WRITE_ERROR'] = { [ARM_TICKET_PATH]: 13900001 };
  const halfEntry = newEntry();
  resetLog();
  (halfEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird', parameters: { 'n2h_arm_options': ARM_PAYLOAD } }, {});
  check('C5b a failed write after TRUNC removes the empty ticket (it must not survive)',
    fileOf(ARM_TICKET_PATH) === undefined, JSON.stringify(fileOf(ARM_TICKET_PATH)));
  check('C5b the write failure and the cleanup are logged independently with codes',
    lastLine('N2H_ARM_TICKET_WRITE_FAILED|') === 'N2H_ARM_TICKET_WRITE_FAILED|file=' +
    ARM_TICKET_NAME + '|code=13900001|note=no-ticket-no-arm' &&
    lastLine('N2H_ARM_TICKET_REMOVED|') === 'N2H_ARM_TICKET_REMOVED|file=' + ARM_TICKET_NAME +
    '|note=failed-write-cleanup', JSON.stringify(logs()));
  const halfAbility = freshAbility(mod, FIXTURE_FILESDIR);
  G['__NB_ARM_OPTS'] = undefined;
  resetLog();
  tickArm(halfAbility, WG_READY);
  check('C5b the extension then SKIPS (arm-file-absent) and never calls the native arm',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 1 &&
    lastLine('N2H_COLLECTOR_ARM_SKIPPED|').indexOf('|reason=arm-file-absent|') > 0 &&
    G['__NB_ARM_OPTS'] === undefined, JSON.stringify(logs()));
  check('C5b the credential/config/deviation files are byte-identical',
    fileOf(SETUP_PATH) === 'setup-key-body-untouched' &&
    fileOf(DEVCFG_PATH) === '{"management_url":"http://m","private_key":"pk-body"}' &&
    fileOf(DEVIATION_PATH) === 'route-exclusion', 'private files pin');

  // C5c: if even the CLEANUP fails, the surviving (truncated) file is reported
  // as what it is — REMOVE_FAILED with its code — and never as removed.
  resetFs();
  seedFile(ARM_TICKET_PATH, '{"posctl_port":41000}');
  G['__NB_FS_WRITE_ERROR'] = { [ARM_TICKET_PATH]: 13900001 };
  G['__NB_FS_ACCESS_ERROR'] = { [ARM_TICKET_PATH]: 13900042 };
  const stuckEntry = newEntry();
  resetLog();
  (stuckEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird', parameters: { 'n2h_arm_options': ARM_PAYLOAD } }, {});
  check('C5c a failed cleanup keeps the truncated file and logs REMOVE_FAILED (no false removal)',
    fileOf(ARM_TICKET_PATH) === '' &&
    lastLine('N2H_ARM_TICKET_REMOVE_FAILED|') === 'N2H_ARM_TICKET_REMOVE_FAILED|file=' +
    ARM_TICKET_NAME + '|code=13900042|note=failed-write-cleanup' &&
    linesWith('N2H_ARM_TICKET_REMOVED|').length === 0, JSON.stringify(logs()));

  // C6: the per-instance ONE-SHOT arm attempt is intact on the new channel —
  // the second tick of the same instance never calls the native arm again.
  resetFs();
  setArmFile(true, undefined, ARM_PAYLOAD);
  const latchAbility = freshAbility(mod, FIXTURE_FILESDIR);
  latchAbility['n2hAcceptedExclusionGeneration'] = 0;
  resetLog();
  tickArm(latchAbility, WG_READY);
  const callsAfterFirst: number = Number(G['__NB_ARM_CALLS'] ?? 0);
  check('C6 the first tick with a ticket calls the native arm exactly once',
    callsAfterFirst >= 1 && linesWith('N2H_COLLECTOR_ARMED|').length === 1,
    JSON.stringify(logs()));
  resetLog();
  tickArm(latchAbility, WG_READY);
  tickArm(latchAbility, WG_READY);
  check('C6 the SECOND tick of the same instance repeats NOTHING (no second native call)',
    logs().length === 0 && Number(G['__NB_ARM_CALLS'] ?? 0) === callsAfterFirst,
    JSON.stringify({ logs: logs(), calls: G['__NB_ARM_CALLS'] }));

  // C7: source pins — the extension has NO app-invisible arm path left, the
  // channel rides the SAME filesDir mechanism as the credentials, and the
  // credential dev-override channel is untouched.
  const entrySource: string = readFileSync(join(dir, 'EntryAbility.ts')).toString();
  check('C7 the extension no longer references the app-invisible /data/local/tmp arm path',
    extSource.indexOf('/data/local/tmp/netbird-n2h-arm') < 0 &&
    extSource.indexOf("const N2H_ARM_FILE_NAME: string = 'netbird-n2h-arm';") > 0 &&
    extSource.indexOf('${this.context.filesDir}/${N2H_ARM_FILE_NAME}') > 0,
    'arm path pin');
  check('C7 the EntryAbility carries the non-secret param, the same basename, and the cleanup',
    entrySource.indexOf("'n2h_arm_options'") > 0 &&
    entrySource.indexOf("'netbird-n2h-arm'") > 0 &&
    entrySource.indexOf("this.removeArmTicket('default-no-arm')") > 0 &&
    entrySource.indexOf("this.removeArmTicket('failed-write-cleanup')") > 0 &&
    entrySource.indexOf('short-write:') > 0 &&
    entrySource.indexOf('new util.TextEncoder().encode(text).byteLength') > 0 &&
    entrySource.indexOf('N2H_ARM_TICKET_WRITTEN') > 0,
    'entry channel pin');
  check('C7 the credential dev-override channel is untouched',
    readFileSync(join(dir, 'NetBirdConnector.ts')).toString()
      .indexOf('DEV_CREDENTIAL_DIR') > 0,
    'credential override pin');

  rmSync(dir, { recursive: true, force: true });
} catch (error) {
  failedCount++;
  console.log('FAIL harness — ' + (error as Error).message + '\n' +
    ((error as Error).stack ?? ''));
}

console.log('');
console.log(String(passed) + ' passed, ' + String(failedCount) + ' failed');
process.exit(failedCount === 0 ? 0 : 1);
