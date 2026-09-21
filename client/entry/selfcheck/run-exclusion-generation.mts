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
//   A1-A6 (I5a §13.15 + 2026-09-18 real-device timing fix): the arm hook's
//      outcome is never silent — an absent or unreadable arm control file
//      emits exactly ONE N2H_COLLECTOR_ARM_SKIPPED line (reason=arm-file-absent
//      for a PROVED absence: boolean false or BusinessError 13900002; reason=
//      arm-file-unreadable for every other error token, which does not prove
//      absence) carrying the raw code as accessError=, WITHOUT calling the core
//      arm, and that probe consumes the instance's one-shot latch (a file
//      appearing later never arms it). With the ticket PRESENT but the session
//      gate closed the attempt is NOT consumed: one deduped
//      N2H_COLLECTOR_WAITING|attempted=false line (NOT_ARMED retired — the
//      0016 run showed the first tick eating the only attempt while WG was
//      still ramping), the SAME instance then arms exactly once on the first
//      gate-open tick with the payload unchanged, a tick after that repeats
//      nothing, destroyRequested still arms nothing, and the gate-open ARMED
//      path stays byte-identical (regression).
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
//   C8 (2026-09-18 peer_overlay_ip bridge): the parallel posctl-target fix's
//      optional non-secret `peer_overlay_ip` (the ACTUAL n2h-peer overlay
//      address; multi-peer maps no longer mean first()) is bridged with the
//      peer_b2a_probe_id verbatim discipline — present ⇒ in the native opts
//      JSON verbatim (existing fields unchanged), absent ⇒ the key stays
//      absent (no default injected), wrong-type (e.g. a JSON number) ⇒
//      forwarded AS THAT VALUE for the core's bare-IPv4 + peer-map check to
//      refuse; no marker field and no log literal changes.
//   C9 (2026-09-20 dns_nxdomain_echo bridge): v4 effective preregistration
//      §3 path① / §6.2 item 2's optional non-secret `dns_nxdomain_echo`
//      (the explicit enable of the dns-nxdomain-echo receipt method; default
//      OFF) is bridged with the SAME present-only + verbatim discipline —
//      present true/false ⇒ the key lands in the native opts JSON verbatim
//      (existing fields unchanged), absent ⇒ the key stays ABSENT (no `false`
//      default injected), wrong-type (JSON string/number) ⇒ forwarded AS THAT
//      VALUE for the core's strict boolean check to refuse; no marker field
//      and no log literal changes.
//   P1-P8 (2026-09-18 iface-bytes availability probe): the OFFICIAL
//      @ohos.net.statistics getIfaceRxBytes/getIfaceTxBytes (since 10, no
//      permission on the iface methods) is called ONLY from the arm SUCCESS
//      branch, exactly once per instance, as one four-promise round over
//      'vpn-tun' + the fixed negative-control name 'n2h-none0'.
//      P1 the four calls (negative control actually invoked); P2 the vpn-tun
//      line carries the resolved byte values, none-errors, digit-shaped
//      begin/end ms and the diag flags; P3 the unknown name's 0-answer is
//      recorded verbatim (never read as a proof about vpn-tun); P4 a
//      per-direction rejection leaves that byte field unavailable with the
//      raw code kept while the other direction records; P5 NO native ARM
//      success ⇒ NO probe (no ticket / gate closed / core answers not-ok);
//      P6 ticks after the arm repeat nothing (one-shot inherited) — the FIRST
//      post-arm tick carries exactly the WINDOW close edge, the tick after it
//      repeats nothing; P7 a
//      resolved non-finite/negative value is never printed and never turned
//      into 0 (invalid-value), a resolved 0 prints as 0; P8 source pins (call
//      site inside the ok-branch after the ARMED literal, byte-only probe
//      module — no /proc read, no getUid*/GET_NETWORK_STATS, no exception
//      message text — and module.json5 unchanged). The probe never touches
//      Evidence.counters/notes; no packet count exists anywhere.
//   W1-W6 (2026-09-20 iface-bytes WINDOW two-edge read, v4 preregistration
//      §4 (ii) C7 same-source-calibration prerequisite): the OPEN edge is the
//      arm SUCCESS branch's own vpn-tun reading, re-rendered on
//      N2H_IFACE_BYTES_WINDOW|phase=open from the SAME numbers (no second
//      read; pairRequestId == requestId); the CLOSE edge is ONE more vpn-tun
//      read issued on the FIRST arm-hook tick AFTER that open line — never
//      the arming tick (the latch flips only after the async open line),
//      never without an open edge, never twice (latch consumed at issue).
//      W1 open edge once + zero close before the next tick; W2 the close
//      edge: same requestId, fresh values, close.started >= open.finished,
//      exactly the two vpn-tun calls; W3 later ticks repeat nothing; W4 a
//      rejected API prints unavailable + the code and the flow continues;
//      W5 no ARM success (no ticket / gate closed / core not-ok) ⇒ zero
//      close reads; W6 source pins (no timer/loop in the window path, no
//      await added to the tick, the close hook rides the arm hook before its
//      one-shot return, the read issues only behind the open latch with a
//      last-resort catch, latch callback writes one boolean inside the
//      ok-branch, both reads strictly after the native arm call). C7 counting
//      input only — never Evidence.
//   NC1-NC5 (2026-09-20 network-context snapshot): the PUBLIC connection
//      getters (getDefaultNetSync/getAppNetSync/getAllNetsSync +
//      getNetCapabilitiesSync/getConnectionPropertiesSync, all Sync forms read
//      off the local SDK d.ts) are sampled ONCE per arm SUCCESS, in the same
//      branch as the iface probe. NC1 the armed line carries the seeded
//      default/app netIds, the getAllNets list verbatim and per-net
//      interface/bearerTypes/routes (dest/prefix/excl/gw tokens) with digit
//      begin/end; NC2 a rejected API (code) leaves that field unavailable
//      while every independent read still records; NC3 an empty handle world
//      records nets=0/detail=empty honestly; NC4 no ARM success (gate closed /
//      core not-ok) fires nothing and the armed instance never repeats; NC5
//      source pins (getter-only module — no bindSocket/setAppNet, no
//      .message, still byte-only; call order inside the ok-branch; aborted
//      guard literal).
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
// The 2026-09-18 iface-bytes availability probe marker (the P group below).
const PROBE_PREFIX = 'N2H_IFACE_BYTES_PROBE|';
// The 2026-09-20 iface-bytes WINDOW two-edge marker (the W group below).
const WINDOW_PREFIX = 'N2H_IFACE_BYTES_WINDOW|';
// The 2026-09-20 network-context snapshot marker (the NC group below).
const NETCTX_LINE = 'N2H_NET_CONTEXT|';

// The five real modules staged byte-identically (code under test = shipped
// code), and the erased interface names each shim must re-declare for the ESM
// link (same list as run-relay-exclusion.mts — see that file's hook comment).
// NetBirdInterfaceStatsProbe (2026-09-18 iface-bytes availability probe) is
// staged the same way: the REAL extension imports it, so the byte-identical
// fixture needs the byte-identical helper (no sibling imports of its own —
// no shim needed).
const STAGED = ['NetBirdVpnExtensionAbility', 'NetBirdConnector',
  'NetBirdVpnConfig', 'NetBirdRouteValidation', 'NetBirdEndpointExclusion',
  'NetBirdInterfaceStatsProbe'];
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

// The statistics stub's append-only call record ('method:nic' strings) and the
// per-NIC answer tables (exclusion-generation-hooks.mjs): a number resolves,
// {code:n} rejects, an UNSET name rejects 2100003 — an unexpected probe can
// never fake a value.
function statsCalls(): string[] {
  return (G['__NB_STATS_CALLS'] ?? []) as string[];
}

function seedStats(rx: Record<string, unknown>, tx: Record<string, unknown>): void {
  G['__NB_STATS_RX'] = rx;
  G['__NB_STATS_TX'] = tx;
}

// The network-context probe's connection tables (exclusion-generation-hooks.mjs):
// a number resolves, {code:n} rejects, an UNSET key answers the historical
// default (netId 0 / empty list / wlan0 without routes) — seeding one table
// never perturbs what the older groups observe. __NB_NET_THROW_CODES maps a
// method name to a BusinessError code (or 'no-code').
function seedNetCtx(tables: Record<string, unknown>): void {
  G['__NB_NET_DEFAULT_ID'] = undefined;
  G['__NB_NET_APP_ID'] = undefined;
  G['__NB_NET_ALL_IDS'] = undefined;
  G['__NB_NET_CAPS_BY_ID'] = undefined;
  G['__NB_NET_PROPS_BY_ID'] = undefined;
  G['__NB_NET_THROW_CODES'] = undefined;
  for (const key of Object.keys(tables)) {
    G[key] = tables[key];
  }
}

// Every probe log line is written several microtasks after the tick that
// armed it (four awaited statistics promises); one macrotask boundary drains
// the whole chain deterministically (the same pattern the B1 await uses).
async function drainProbe(): Promise<void> {
  await new Promise<void>((resolve: () => void): void => { setTimeout(resolve, 0); });
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

  // ② ③ (2026-09-18 real-device timing fix): with the ticket present but the
  // session gate closed the hook WAITS — one deduped WAITING line, no native
  // call, latch open — and the SAME instance arms exactly once on the first
  // later tick whose gate opens, ticket payload unchanged. NOT_ARMED is
  // retired: it consumed the only attempt while WG was still ramping (0016).
  const waitAbility = freshAbility(mod, FIXTURE_FILESDIR);
  const WAIT_TICKET = '{"posctl_port":41000,"probe_wait_ms":900}';
  setArmFile(true, undefined, WAIT_TICKET);
  resetLog();
  const callsAtWait = Number(G['__NB_ARM_CALLS'] ?? 0);
  tickArm(waitAbility, { wg: { device_up: false, ready: true } });
  check('A5 file present + gate closed emits ONE deduped WAITING line, no native call, ' +
    'latch open (NOT_ARMED retired)',
    linesWith('N2H_COLLECTOR_WAITING|').length === 1 &&
    lastLine('N2H_COLLECTOR_WAITING|') ===
    'N2H_COLLECTOR_WAITING|requestId=' + REQUEST_ID +
    '|reason=session-not-ready|deviceUp=false|ready=true|attempted=false|' +
    'diag=true|is_evidence=false' &&
    linesWith('N2H_COLLECTOR_NOT_ARMED|').length === 0 &&
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 0 &&
    Number(G['__NB_ARM_CALLS'] ?? 0) === callsAtWait &&
    waitAbility['n2hArmAttempted'] === false, JSON.stringify(logs()));
  tickArm(waitAbility, { wg: { device_up: true, ready: false } });
  check('A5 a second waiting tick (the 0016 shape deviceUp=true|ready=false) repeats NOTHING ' +
    '(dedup boolean: still the single first-tick line, zero native calls, latch open)',
    linesWith('N2H_COLLECTOR_WAITING|').length === 1 &&
    linesWith('N2H_COLLECTOR_WAITING|').every((l: string): boolean =>
      l.indexOf('|deviceUp=false|ready=true|attempted=false|') > 0) &&
    linesWith('N2H_COLLECTOR_WAITING|').every((l: string): boolean =>
      l.indexOf('|deviceUp=true|') < 0) &&
    Number(G['__NB_ARM_CALLS'] ?? 0) === callsAtWait &&
    waitAbility['n2hArmAttempted'] === false, JSON.stringify(logs()));
  waitAbility['n2hAcceptedExclusionGeneration'] = 0;
  tickArm(waitAbility, WG_READY);
  const waitOpts = JSON.parse(String(G['__NB_ARM_OPTS'] ?? '{}')) as Record<string, unknown>;
  check('A5 the first ready tick arms ONCE on the SAME instance, ticket payload unchanged, ' +
    'no extra WAITING line',
    Number(G['__NB_ARM_CALLS'] ?? 0) === callsAtWait + 1 &&
    waitOpts['posctl_port'] === 41000 && waitOpts['probe_wait_ms'] === 900 &&
    waitAbility['n2hArmAttempted'] === true &&
    linesWith('N2H_COLLECTOR_ARMED|').length === 1 &&
    linesWith('N2H_COLLECTOR_WAITING|').length === 1 &&
    lastLine('N2H_COLLECTOR_ARMED|') ===
    'N2H_COLLECTOR_ARMED|requestId=' + REQUEST_ID +
    '|gen=0|openedMono=4242|exclusions=0|posctlPort=41000|probeWaitMs=900|' +
    'peerB2aProbeId=none|diag=true|is_evidence=false', JSON.stringify(logs()));
  resetLog();
  tickArm(waitAbility, WG_READY);
  check('A5 a tick after the arm repeats nothing (one-shot latch, no second native call)',
    logs().length === 0 && Number(G['__NB_ARM_CALLS'] ?? 0) === callsAtWait + 1,
    JSON.stringify(logs()));
  const destroyedAbility = freshAbility(mod, FIXTURE_FILESDIR);
  destroyedAbility['destroyRequested'] = true;
  setArmFile(true, undefined); // present-but-empty ticket: A6 below reads the defaults
  resetLog();
  tickArm(destroyedAbility, WG_READY);
  check('A5 destroyRequested still arms nothing (unchanged guard: no line, no native call, ' +
    'latch stays open)',
    logs().length === 0 && Number(G['__NB_ARM_CALLS'] ?? 0) === callsAtWait + 1 &&
    destroyedAbility['n2hArmAttempted'] === false, JSON.stringify(logs()));
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

  // ---------------- C8 (2026-09-18): peer_overlay_ip verbatim bridging -----
  // The parallel posctl-target fix adds the optional non-secret
  // `peer_overlay_ip` to the arm file options: the ACTUAL n2h-peer overlay
  // address (multi-peer maps no longer mean "first()"). The shell NEVER
  // validates or repairs it — present ⇒ forwarded verbatim into the native
  // opts JSON, absent ⇒ the key stays absent (no default injected), wrong-type
  // ⇒ forwarded as that value and the CORE's strict bare-IPv4 + peer-map check
  // refuses it. No marker field and no log literal changes.
  const overlayAbility = freshAbility(mod, FIXTURE_FILESDIR);
  overlayAbility['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, '{"peer_overlay_ip":"10.99.0.7"}');
  resetLog();
  tickArm(overlayAbility, WG_READY);
  const overlayOpts = JSON.parse(String(G['__NB_ARM_OPTS'] ?? '{}')) as Record<string, unknown>;
  check('C8 a present peer_overlay_ip reaches the native opts JSON verbatim',
    overlayOpts['peer_overlay_ip'] === '10.99.0.7' &&
    String(G['__NB_ARM_OPTS']).indexOf('"peer_overlay_ip":"10.99.0.7"') > 0,
    String(G['__NB_ARM_OPTS']));
  check('C8 the existing arm fields ride unchanged next to the new key',
    overlayOpts['posctl_port'] === 40001 && overlayOpts['probe_wait_ms'] === 800,
    String(G['__NB_ARM_OPTS']));

  const noOverlayAbility = freshAbility(mod, FIXTURE_FILESDIR);
  noOverlayAbility['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, '{"posctl_port":41000}');
  resetLog();
  tickArm(noOverlayAbility, WG_READY);
  const noOverlayOpts =
    JSON.parse(String(G['__NB_ARM_OPTS'] ?? '{}')) as Record<string, unknown>;
  check('C8 an absent peer_overlay_ip stays absent (no key, no default injected)',
    noOverlayOpts['peer_overlay_ip'] === undefined &&
    String(G['__NB_ARM_OPTS']).indexOf('peer_overlay_ip') < 0,
    String(G['__NB_ARM_OPTS']));

  const badOverlayAbility = freshAbility(mod, FIXTURE_FILESDIR);
  badOverlayAbility['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, '{"peer_overlay_ip":123}');
  resetLog();
  tickArm(badOverlayAbility, WG_READY);
  const badOverlayOpts =
    JSON.parse(String(G['__NB_ARM_OPTS'] ?? '{}')) as Record<string, unknown>;
  check('C8 a wrong-type value is forwarded AS THAT VALUE (never dropped, never defaulted)',
    badOverlayOpts['peer_overlay_ip'] === 123 &&
    String(G['__NB_ARM_OPTS']).indexOf('"peer_overlay_ip":123') > 0,
    String(G['__NB_ARM_OPTS']));

  // ---------------- C9 (2026-09-20): dns_nxdomain_echo verbatim bridging ----
  // v4 effective preregistration §3 path① / §6.2 item 2: the optional
  // non-secret `dns_nxdomain_echo` enable for the device collector's dns
  // probe. The shell is a PURE FORWARDER — present (true OR false) ⇒ the key
  // lands in the native opts JSON verbatim (existing fields unchanged),
  // absent ⇒ the key stays ABSENT (no `false` default injected; the core's
  // documented OFF is what an omitted key means), wrong-type (JSON string /
  // number) ⇒ forwarded AS THAT VALUE for the core's strict boolean check to
  // refuse. No marker field and no log literal changes.
  const echoOnAbility = freshAbility(mod, FIXTURE_FILESDIR);
  echoOnAbility['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, '{"dns_nxdomain_echo":true}');
  resetLog();
  tickArm(echoOnAbility, WG_READY);
  const echoOnRaw = String(G['__NB_ARM_OPTS']);
  const echoOnOpts = JSON.parse(echoOnRaw) as Record<string, unknown>;
  check('C9 a present dns_nxdomain_echo=true reaches the native opts JSON verbatim',
    echoOnOpts['dns_nxdomain_echo'] === true &&
    echoOnRaw.indexOf('"dns_nxdomain_echo":true') > 0,
    echoOnRaw);
  check('C9 the existing arm fields ride unchanged next to dns_nxdomain_echo',
    echoOnOpts['posctl_port'] === 40001 && echoOnOpts['probe_wait_ms'] === 800,
    echoOnRaw);

  const echoOffAbility = freshAbility(mod, FIXTURE_FILESDIR);
  echoOffAbility['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, '{"dns_nxdomain_echo":false}');
  resetLog();
  tickArm(echoOffAbility, WG_READY);
  const echoOffRaw = String(G['__NB_ARM_OPTS']);
  const echoOffOpts = JSON.parse(echoOffRaw) as Record<string, unknown>;
  check('C9 an explicit false is forwarded as false (never dropped as "absent")',
    echoOffOpts['dns_nxdomain_echo'] === false &&
    echoOffRaw.indexOf('"dns_nxdomain_echo":false') > 0,
    echoOffRaw);

  const noEchoAbility = freshAbility(mod, FIXTURE_FILESDIR);
  noEchoAbility['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, '{"posctl_port":41000}');
  resetLog();
  tickArm(noEchoAbility, WG_READY);
  const noEchoRaw = String(G['__NB_ARM_OPTS']);
  const noEchoOpts = JSON.parse(noEchoRaw) as Record<string, unknown>;
  check('C9 an absent dns_nxdomain_echo stays absent (no key, no false injected)',
    noEchoOpts['dns_nxdomain_echo'] === undefined &&
    noEchoRaw.indexOf('dns_nxdomain_echo') < 0,
    noEchoRaw);

  const badEchoAbility = freshAbility(mod, FIXTURE_FILESDIR);
  badEchoAbility['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, '{"dns_nxdomain_echo":"true"}');
  resetLog();
  tickArm(badEchoAbility, WG_READY);
  const badEchoRaw = String(G['__NB_ARM_OPTS']);
  const badEchoOpts = JSON.parse(badEchoRaw) as Record<string, unknown>;
  check('C9 a wrong-type string is forwarded AS THAT VALUE (never coerced, never dropped)',
    badEchoOpts['dns_nxdomain_echo'] === 'true' &&
    badEchoRaw.indexOf('"dns_nxdomain_echo":"true"') > 0,
    badEchoRaw);

  const numEchoAbility = freshAbility(mod, FIXTURE_FILESDIR);
  numEchoAbility['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, '{"dns_nxdomain_echo":1}');
  resetLog();
  tickArm(numEchoAbility, WG_READY);
  const numEchoRaw = String(G['__NB_ARM_OPTS']);
  const numEchoOpts = JSON.parse(numEchoRaw) as Record<string, unknown>;
  check('C9 a wrong-type number is forwarded AS THAT VALUE (never coerced, never dropped)',
    numEchoOpts['dns_nxdomain_echo'] === 1 &&
    numEchoRaw.indexOf('"dns_nxdomain_echo":1') > 0,
    numEchoRaw);

  // ---------------- P (2026-09-18): the iface-bytes availability probe -----
  // The OFFICIAL per-NIC byte API (@ohos.net.statistics getIfaceRxBytes /
  // getIfaceTxBytes, since 10, NO permission on the iface methods) is the
  // availability question run 0017 left open: the app cannot read
  // /proc/net/dev (Permission denied, counters=[]). The probe is wired ONLY
  // into the arm SUCCESS branch — these checks pin exactly that: when it
  // fires, what it records (values / codes / begin-end / negative control),
  // what it must never do (fire without a native ARM success, turn an error
  // into 0, print a bogus value, run twice per instance), and the unchanged
  // literals around the insertion point. Bytes only: nothing here feeds
  // Evidence.counters, no packet count exists, no ARM reconciliation runs.
  await drainProbe(); // settle the probes the earlier groups' ARMED ticks armed
  resetLog();

  // P1: the REAL arm success branch triggers exactly ONE probe round.
  const probeAbility = freshAbility(mod, FIXTURE_FILESDIR);
  probeAbility['n2hAcceptedExclusionGeneration'] = 0;
  seedStats({ 'vpn-tun': 1234, 'n2h-none0': 0 },
    { 'vpn-tun': 5678, 'n2h-none0': 0 });
  setArmFile(true, undefined);
  const callsAtP1 = statsCalls().length;
  tickArm(probeAbility, WG_READY);
  await drainProbe();
  const p1Calls = statsCalls().slice(callsAtP1);
  check('P1 the arm SUCCESS branch triggers exactly ONE probe round: the four documented calls',
    linesWith(PROBE_PREFIX).length === 2 &&
    p1Calls.length === 4 &&
    p1Calls.indexOf('getIfaceRxBytes:vpn-tun') >= 0 &&
    p1Calls.indexOf('getIfaceTxBytes:vpn-tun') >= 0 &&
    p1Calls.indexOf('getIfaceRxBytes:n2h-none0') >= 0 &&
    p1Calls.indexOf('getIfaceTxBytes:n2h-none0') >= 0,
    JSON.stringify({ probeLines: linesWith(PROBE_PREFIX), calls: p1Calls }));

  // P2: the vpn-tun line — exact head (values + none-errors), digit-shaped
  // begin/end with finished >= started, the diag flags as the tail.
  const p1Vpn = linesWith(PROBE_PREFIX).filter((l: string): boolean =>
    fieldOf(l, 'interface') === 'vpn-tun');
  const p1VpnLine = p1Vpn.length === 1 ? p1Vpn[0] : '';
  const p1Started = fieldOf(p1VpnLine, 'startedUnixMs');
  const p1Finished = fieldOf(p1VpnLine, 'finishedUnixMs');
  check('P2 vpn-tun line: resolved values + none-errors + digit begin/end + diag flags',
    p1VpnLine === 'N2H_IFACE_BYTES_PROBE|requestId=' + REQUEST_ID +
    '|interface=vpn-tun|rxBytes=1234|txBytes=5678|rxError=none|txError=none|' +
    'startedUnixMs=' + p1Started + '|finishedUnixMs=' + p1Finished +
    '|diag=true|is_evidence=false' &&
    /^\d+$/.test(p1Started) && /^\d+$/.test(p1Finished) &&
    Number(p1Finished) >= Number(p1Started),
    p1VpnLine);

  // P3: the negative control line — the 0-answer of the nonexistent name is
  // recorded verbatim (a finding about the unknown-name shape, never read as
  // a proof that the vpn-tun value works).
  const p1Neg = linesWith(PROBE_PREFIX).filter((l: string): boolean =>
    fieldOf(l, 'interface') === 'n2h-none0');
  check('P3 the fixed nonexistent name IS sampled and its 0-answer recorded verbatim',
    p1Neg.length === 1 &&
    p1Neg[0] === 'N2H_IFACE_BYTES_PROBE|requestId=' + REQUEST_ID +
    '|interface=n2h-none0|rxBytes=0|txBytes=0|rxError=none|txError=none|' +
    'startedUnixMs=' + fieldOf(p1Neg[0], 'startedUnixMs') +
    '|finishedUnixMs=' + fieldOf(p1Neg[0], 'finishedUnixMs') +
    '|diag=true|is_evidence=false',
    p1Neg.join('\n'));

  // P4: per-direction honesty — a rejected call leaves its byte field
  // unavailable with the raw code kept; the OTHER direction still records;
  // the round still made its full four calls.
  const errAbility = freshAbility(mod, FIXTURE_FILESDIR);
  errAbility['n2hAcceptedExclusionGeneration'] = 0;
  seedStats({ 'vpn-tun': { code: 2103012 }, 'n2h-none0': 7 },
    { 'vpn-tun': 99, 'n2h-none0': { code: 2100002 } });
  setArmFile(true, undefined);
  const callsAtP4 = statsCalls().length;
  resetLog();
  tickArm(errAbility, WG_READY);
  await drainProbe();
  const p4Vpn = linesWith(PROBE_PREFIX).filter((l: string): boolean =>
    fieldOf(l, 'interface') === 'vpn-tun');
  const p4Neg = linesWith(PROBE_PREFIX).filter((l: string): boolean =>
    fieldOf(l, 'interface') === 'n2h-none0');
  check('P4 rx rejects -> rxBytes=unavailable with the code kept, tx still records',
    p4Vpn.length === 1 &&
    fieldOf(p4Vpn[0], 'rxBytes') === 'unavailable' &&
    fieldOf(p4Vpn[0], 'rxError') === '2103012' &&
    fieldOf(p4Vpn[0], 'txBytes') === '99' &&
    fieldOf(p4Vpn[0], 'txError') === 'none',
    p4Vpn.join('\n'));
  check('P4 tx rejects -> txBytes=unavailable with the code kept, rx still records',
    p4Neg.length === 1 &&
    fieldOf(p4Neg[0], 'rxBytes') === '7' &&
    fieldOf(p4Neg[0], 'rxError') === 'none' &&
    fieldOf(p4Neg[0], 'txBytes') === 'unavailable' &&
    fieldOf(p4Neg[0], 'txError') === '2100002',
    p4Neg.join('\n'));
  check('P4 the failed round still made its full four statistics calls',
    statsCalls().length - callsAtP4 === 4, String(statsCalls().length - callsAtP4));

  // P5: NO native ARM success ⇒ NO probe. Three shapes: the plain-start
  // default (no ticket), the ticket with the session gate still closed, and
  // the gate open but the core answering not-ok.
  const callsAtP5 = statsCalls().length;
  const noTicketAbility = freshAbility(mod, FIXTURE_FILESDIR);
  setArmFile(false, undefined);
  resetLog();
  tickArm(noTicketAbility, WG_READY);
  await drainProbe();
  check('P5a no arm ticket (the plain-start default) -> the ONE skip line, no probe call',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 1 &&
    linesWith(PROBE_PREFIX).length === 0 &&
    statsCalls().length === callsAtP5, JSON.stringify(logs()));

  const waitingAbility = freshAbility(mod, FIXTURE_FILESDIR);
  setArmFile(true, undefined, '');
  resetLog();
  tickArm(waitingAbility, { wg: { device_up: true, ready: false } });
  await drainProbe();
  check('P5b ticket present but the session gate closed -> one WAITING line, no probe call',
    linesWith('N2H_COLLECTOR_WAITING|').length === 1 &&
    linesWith(PROBE_PREFIX).length === 0 &&
    statsCalls().length === callsAtP5, JSON.stringify(logs()));

  const refusedAbility = freshAbility(mod, FIXTURE_FILESDIR);
  refusedAbility['n2hAcceptedExclusionGeneration'] = 0;
  G['__NB_ARM_ANSWER'] = '{"ok":false,"error":"forced-by-p5"}';
  setArmFile(true, undefined);
  resetLog();
  tickArm(refusedAbility, WG_READY);
  G['__NB_ARM_ANSWER'] = undefined;
  await drainProbe();
  check('P5c the core answers not-ok -> one ARM_FAILED line, no probe call (out.ok gates it)',
    linesWith('N2H_COLLECTOR_ARM_FAILED|').length === 1 &&
    linesWith('N2H_COLLECTOR_ARMED|').length === 0 &&
    linesWith(PROBE_PREFIX).length === 0 &&
    statsCalls().length === callsAtP5, JSON.stringify(logs()));

  // P6: the OPEN probe inherits the arm's one-shot semantics; the FIRST tick
  // after the arm carries exactly the WINDOW close edge (the W group pins its
  // shape — here only that it is the one extra thing, then silence): no new
  // PROBE line, exactly one close round (two statistics calls), and the tick
  // after that repeats nothing at all.
  const callsAtP6 = statsCalls().length;
  resetLog();
  tickArm(probeAbility, WG_READY);
  await drainProbe();
  const p6CloseCalls = statsCalls().length - callsAtP6;
  check('P6 first post-arm tick: no PROBE repeat, exactly the one WINDOW close round',
    linesWith(PROBE_PREFIX).length === 0 &&
    linesWith(WINDOW_PREFIX).length === 1 &&
    fieldOf(linesWith(WINDOW_PREFIX)[0], 'phase') === 'close' &&
    p6CloseCalls === 2,
    JSON.stringify({ window: linesWith(WINDOW_PREFIX), calls: p6CloseCalls }));
  resetLog();
  tickArm(probeAbility, WG_READY);
  await drainProbe();
  check('P6 the tick after the close edge repeats nothing: no line, no statistics call',
    statsCalls().length === callsAtP6 + p6CloseCalls && logs().length === 0,
    JSON.stringify(logs()));

  // P7: value honesty — a RESOLVED value that is not a finite nonnegative
  // byte count is never printed and never turned into 0 (token
  // invalid-value); a resolved 0 IS finite nonnegative and prints as 0.
  const honestAbility = freshAbility(mod, FIXTURE_FILESDIR);
  honestAbility['n2hAcceptedExclusionGeneration'] = 0;
  seedStats({ 'vpn-tun': -5, 'n2h-none0': 0 },
    { 'vpn-tun': NaN, 'n2h-none0': 0 });
  setArmFile(true, undefined);
  resetLog();
  tickArm(honestAbility, WG_READY);
  await drainProbe();
  const p7Lines = linesWith(PROBE_PREFIX);
  check('P7 negative/NaN answers -> unavailable + invalid-value (no 0-laundering)',
    p7Lines.length === 2 &&
    fieldOf(p7Lines[0], 'interface') === 'vpn-tun' &&
    fieldOf(p7Lines[0], 'rxBytes') === 'unavailable' &&
    fieldOf(p7Lines[0], 'rxError') === 'invalid-value' &&
    fieldOf(p7Lines[0], 'txBytes') === 'unavailable' &&
    fieldOf(p7Lines[0], 'txError') === 'invalid-value',
    JSON.stringify(p7Lines));
  check('P7 the negative control still records its honest zeros in the same round',
    p7Lines.length === 2 &&
    fieldOf(p7Lines[1], 'interface') === 'n2h-none0' &&
    fieldOf(p7Lines[1], 'rxBytes') === '0' &&
    fieldOf(p7Lines[1], 'txBytes') === '0' &&
    fieldOf(p7Lines[1], 'rxError') === 'none',
    JSON.stringify(p7Lines));

  // P8: wiring pins on the byte-identical sources — the call site sits INSIDE
  // the arm success branch (ok -> ARMED literal -> probe -> else), the probe
  // module carries the fixed names and stays byte-only (no /proc read, no
  // getUid*/permission surface, no message text), and module.json5 grew no
  // permission.
  const probeSource: string =
    readFileSync(join(dir, 'NetBirdInterfaceStatsProbe.ts')).toString();
  const okBranchAt = extSource.indexOf('if (answer.ok === true) {');
  const armedLit = extSource.indexOf(
    "'N2H_COLLECTOR_ARMED|requestId=%{public}s|gen=%{public}d|openedMono=%{public}d|'");
  const probeCall = extSource.indexOf('runInterfaceStatsProbe(this.requestId, (): void => {');
  const elseAt = extSource.indexOf('} else {', armedLit);
  check('P8 the probe call sits INSIDE the arm success branch (ok -> ARMED literal -> probe -> else)',
    okBranchAt > 0 && armedLit > okBranchAt && probeCall > armedLit && elseAt > probeCall &&
    extSource.indexOf(
      "import { runInterfaceStatsProbe, runNetworkContextProbe, runInterfaceStatsWindowClose } from\n  './NetBirdInterfaceStatsProbe';") > 0,
    JSON.stringify({ okBranchAt, armedLit, probeCall, elseAt }));
  check('P8 the probe module pins the two fixed names, the marker, and the byte-only scope',
    probeSource.indexOf("const PROBE_IFACE: string = 'vpn-tun';") > 0 &&
    probeSource.indexOf("const PROBE_NEGATIVE_IFACE: string = 'n2h-none0';") > 0 &&
    probeSource.indexOf(PROBE_PREFIX) > 0 &&
    probeSource.indexOf('diag=true|is_evidence=false') > 0 &&
    probeSource.indexOf('@ohos.file.fs') < 0 &&
    probeSource.indexOf('readTextSync') < 0 &&
    probeSource.indexOf('getUidRxBytes') < 0 &&
    probeSource.indexOf('.message') < 0,
    'probe module pin');
  check('P8 the aborted-sample guard prints codes/names only (never an exception message)',
    extSource.indexOf(
      "'N2H_IFACE_BYTES_PROBE_ABORTED|requestId=%{public}s|code=%{public}s|'") > 0 &&
    extSource.indexOf("'name=%{public}s|diag=true|is_evidence=false',") > 0,
    'aborted-guard pin');
  check('P8 no permission surface was added (module.json5 carries no GET_NETWORK_STATS)',
    readFileSync(join(HERE, '..', 'src', 'main', 'module.json5')).toString()
      .indexOf('GET_NETWORK_STATS') < 0,
    'module.json5 pin');

  // ---------------- W (2026-09-20): the iface-bytes WINDOW two-edge read ----
  // v4 preregistration §4 (ii), C7 same-source-calibration prerequisite: ONE
  // OPEN/CLOSE pair on the SAME official per-NIC byte API. The OPEN edge
  // re-renders the arm branch's own vpn-tun sample (SAME numbers — no second
  // read); the CLOSE edge is ONE fresh vpn-tun read issued on the first
  // arm-hook tick AFTER that open line — never the arming tick, never twice,
  // never without the open latch. Diagnostic-only: the readings are C7
  // counting input, never Evidence.counters/notes, never the judge. The same
  // field discipline as the probe line holds (codes only, never a message).
  const openAbility = freshAbility(mod, FIXTURE_FILESDIR);
  openAbility['n2hAcceptedExclusionGeneration'] = 0;
  seedStats({ 'vpn-tun': 2100, 'n2h-none0': 0 }, { 'vpn-tun': 3100, 'n2h-none0': 0 });
  setArmFile(true, undefined);
  const callsAtW1 = statsCalls().length;
  resetLog();
  tickArm(openAbility, WG_READY);
  await drainProbe();
  const w1Lines = linesWith(WINDOW_PREFIX);
  const w1Open = w1Lines.length === 1 ? w1Lines[0] : '';
  const w1ProbeVpn = linesWith(PROBE_PREFIX).filter((l: string): boolean =>
    fieldOf(l, 'interface') === 'vpn-tun');
  check('W1 the arming tick emits exactly the open edge, no close, and only the four probe calls',
    w1Lines.length === 1 &&
    fieldOf(w1Open, 'phase') === 'open' &&
    fieldOf(w1Open, 'requestId') === REQUEST_ID &&
    fieldOf(w1Open, 'pairRequestId') === REQUEST_ID &&
    fieldOf(w1Open, 'interface') === 'vpn-tun' &&
    fieldOf(w1Open, 'rxBytes') === '2100' && fieldOf(w1Open, 'txBytes') === '3100' &&
    fieldOf(w1Open, 'rxError') === 'none' && fieldOf(w1Open, 'txError') === 'none' &&
    /^\d+$/.test(fieldOf(w1Open, 'startedUnixMs')) &&
    /^\d+$/.test(fieldOf(w1Open, 'finishedUnixMs')) &&
    fieldOf(w1Open, 'diag') === 'true' && fieldOf(w1Open, 'is_evidence') === 'false' &&
    statsCalls().length - callsAtW1 === 4,
    JSON.stringify({ window: w1Lines, calls: statsCalls().slice(callsAtW1) }));
  check('W1 the open edge is the SAME sample as the PROBE line (no second read: identical numbers)',
    w1ProbeVpn.length === 1 &&
    fieldOf(w1ProbeVpn[0], 'startedUnixMs') === fieldOf(w1Open, 'startedUnixMs') &&
    fieldOf(w1ProbeVpn[0], 'finishedUnixMs') === fieldOf(w1Open, 'finishedUnixMs') &&
    fieldOf(w1ProbeVpn[0], 'rxBytes') === fieldOf(w1Open, 'rxBytes') &&
    fieldOf(w1ProbeVpn[0], 'txBytes') === fieldOf(w1Open, 'txBytes'),
    JSON.stringify({ probe: w1ProbeVpn, open: w1Open }));

  // W2: the CLOSE edge lands on the NEXT watcher tick — same requestId/pair,
  // FRESH values, exactly two vpn-tun calls, close.started >= open.finished.
  const callsAtW2 = statsCalls().length;
  resetLog();
  seedStats({ 'vpn-tun': 2400, 'n2h-none0': 0 }, { 'vpn-tun': 3600, 'n2h-none0': 0 });
  tickArm(openAbility, WG_READY);
  await drainProbe();
  const w2Lines = linesWith(WINDOW_PREFIX);
  const w2Close = w2Lines.length === 1 ? w2Lines[0] : '';
  const w2Calls = statsCalls().slice(callsAtW2);
  check('W2 the NEXT tick emits exactly the close edge: same requestId/pair, fresh values, two vpn-tun calls',
    w2Lines.length === 1 &&
    fieldOf(w2Close, 'phase') === 'close' &&
    fieldOf(w2Close, 'requestId') === REQUEST_ID &&
    fieldOf(w2Close, 'pairRequestId') === REQUEST_ID &&
    fieldOf(w2Close, 'interface') === 'vpn-tun' &&
    fieldOf(w2Close, 'rxBytes') === '2400' && fieldOf(w2Close, 'txBytes') === '3600' &&
    fieldOf(w2Close, 'rxError') === 'none' && fieldOf(w2Close, 'txError') === 'none' &&
    fieldOf(w2Close, 'diag') === 'true' && fieldOf(w2Close, 'is_evidence') === 'false' &&
    w2Calls.length === 2 &&
    w2Calls.indexOf('getIfaceRxBytes:vpn-tun') >= 0 &&
    w2Calls.indexOf('getIfaceTxBytes:vpn-tun') >= 0,
    JSON.stringify({ window: w2Lines, calls: w2Calls }));
  check('W2 the close edge is not earlier than the open edge (started >= open.finished)',
    Number(fieldOf(w2Close, 'startedUnixMs')) >= Number(fieldOf(w1Open, 'finishedUnixMs')),
    JSON.stringify({ openFinished: fieldOf(w1Open, 'finishedUnixMs'),
      closeStarted: fieldOf(w2Close, 'startedUnixMs') }));

  // W3: one-shot — the ticks after the close edge repeat nothing at all.
  const callsAtW3 = statsCalls().length;
  resetLog();
  tickArm(openAbility, WG_READY);
  tickArm(openAbility, WG_READY);
  await drainProbe();
  check('W3 ticks after the close edge repeat nothing: no WINDOW/PROBE line, no statistics call',
    logs().length === 0 && statsCalls().length === callsAtW3, JSON.stringify(logs()));

  // W4: a REJECTED close read prints unavailable + that direction's raw code,
  // never throws, never launders 0 — and the diagnostic flow continues (the
  // one-shot latch was consumed at issue; the next tick repeats nothing).
  const w4Ability = freshAbility(mod, FIXTURE_FILESDIR);
  w4Ability['n2hAcceptedExclusionGeneration'] = 0;
  seedStats({ 'vpn-tun': 10, 'n2h-none0': 0 }, { 'vpn-tun': 20, 'n2h-none0': 0 });
  setArmFile(true, undefined);
  resetLog();
  tickArm(w4Ability, WG_READY);
  await drainProbe();
  const w4Open = linesWith(WINDOW_PREFIX);
  seedStats({ 'vpn-tun': { code: 2103012 } }, { 'vpn-tun': { code: 2100002 } });
  const callsAtW4 = statsCalls().length;
  resetLog();
  tickArm(w4Ability, WG_READY);
  await drainProbe();
  const w4Lines = linesWith(WINDOW_PREFIX);
  const w4Close = w4Lines.length === 1 ? w4Lines[0] : '';
  check('W4 a rejected close read: unavailable + the raw code per direction, no 0-laundering, no throw',
    w4Open.length === 1 && fieldOf(w4Open[0], 'phase') === 'open' &&
    w4Lines.length === 1 && fieldOf(w4Close, 'phase') === 'close' &&
    fieldOf(w4Close, 'requestId') === REQUEST_ID &&
    fieldOf(w4Close, 'pairRequestId') === REQUEST_ID &&
    fieldOf(w4Close, 'rxBytes') === 'unavailable' &&
    fieldOf(w4Close, 'rxError') === '2103012' &&
    fieldOf(w4Close, 'txBytes') === 'unavailable' &&
    fieldOf(w4Close, 'txError') === '2100002' &&
    linesWith('N2H_IFACE_BYTES_WINDOW_CLOSE_ABORTED|').length === 0,
    JSON.stringify({ open: w4Open, close: w4Lines }));
  resetLog();
  tickArm(w4Ability, WG_READY);
  await drainProbe();
  check('W4 the diagnostic flow continues: the next tick repeats nothing (one-shot consumed at issue)',
    logs().length === 0 && statsCalls().length === callsAtW4 + 2,
    JSON.stringify({ logs: logs(), calls: statsCalls().slice(callsAtW4) }));

  // W5: NO native ARM success ⇒ zero close reads. Three shapes, mirroring P5.
  const w5Cases: Array<{ name: string; arm: () => void; status: Record<string, unknown> }> = [
    { name: 'W5a no arm ticket (the plain-start default)', status: WG_READY,
      arm: (): void => { setArmFile(false, undefined); } },
    { name: 'W5b ticket present but the session gate closed', status: { wg: { device_up: true, ready: false } },
      arm: (): void => { setArmFile(true, undefined, ''); } },
    { name: 'W5c the core answers not-ok', status: WG_READY,
      arm: (): void => {
        G['__NB_ARM_ANSWER'] = '{"ok":false,"error":"forced-by-w5"}';
        setArmFile(true, undefined);
      } }
  ];
  for (const w5Case of w5Cases) {
    const w5Ability = freshAbility(mod, FIXTURE_FILESDIR);
    w5Ability['n2hAcceptedExclusionGeneration'] = 0;
    w5Case.arm();
    const callsAtW5 = statsCalls().length;
    resetLog();
    tickArm(w5Ability, w5Case.status);
    await drainProbe();
    tickArm(w5Ability, w5Case.status);
    await drainProbe();
    G['__NB_ARM_ANSWER'] = undefined;
    check(w5Case.name + ' -> zero WINDOW lines, zero close read',
      linesWith(WINDOW_PREFIX).length === 0 &&
      linesWith(PROBE_PREFIX).length === 0 &&
      statsCalls().length === callsAtW5, JSON.stringify(logs()));
  }

  // W6: source pins on the byte-identical sources — the window path adds no
  // timer and no loop, the close read is fire-and-forget behind the open
  // latch, its hook rides the arm hook BEFORE the one-shot return, and the
  // latch callback writes one boolean inside the ok-branch.
  const armHookAt = extSource.indexOf('private maybeArmN2hCollector(');
  const closeHookAt = extSource.indexOf('this.maybeCloseInterfaceStatsWindow();');
  const armEarlyReturnAt = extSource.indexOf(
    'if (this.n2hArmAttempted || this.destroyRequested) {', armHookAt);
  // The window path only — the extension keeps its PRE-EXISTING watcher
  // interval, so the pin is scoped to the close latch method and the two
  // probe-module window functions, not to the whole extension.
  const w6CloseFnMatch = /private maybeCloseInterfaceStatsWindow\(\): void \{[\s\S]*?\n  \}/
    .exec(extSource);
  const w6CloseFn = w6CloseFnMatch === null ? '' : w6CloseFnMatch[0];
  const w6EmitFnMatch = /function emitWindowLine\([\s\S]*?\n\}/.exec(probeSource);
  const w6EmitFn = w6EmitFnMatch === null ? '' : w6EmitFnMatch[0];
  const w6CloseExportMatch = /export async function runInterfaceStatsWindowClose\([\s\S]*?\n\}/
    .exec(probeSource);
  const w6CloseExport = w6CloseExportMatch === null ? '' : w6CloseExportMatch[0];
  const w6NoTimerLoop = (text: string): boolean =>
    !/setInterval|setTimeout|while \(|for \(/.test(text);
  check('W6 the window path adds no timer and no loop (fire-and-forget, no await added)',
    w6NoTimerLoop(w6CloseFn) && w6NoTimerLoop(w6EmitFn) && w6NoTimerLoop(w6CloseExport) &&
    probeSource.indexOf('setInterval') < 0 && probeSource.indexOf('setTimeout') < 0 &&
    extSource.indexOf('await this.maybeCloseInterfaceStatsWindow') < 0 &&
    extSource.indexOf('await runInterfaceStatsWindowClose') < 0,
    JSON.stringify({ closeFn: w6CloseFn.length, emitFn: w6EmitFn.length,
      closeExport: w6CloseExport.length }));
  check('W6 the close hook rides the arm hook BEFORE its one-shot early return',
    armHookAt > 0 && closeHookAt > armHookAt && armEarlyReturnAt > closeHookAt,
    JSON.stringify({ armHookAt, closeHookAt, armEarlyReturnAt }));
  const w6GuardAt = extSource.indexOf(
    'if (!this.n2hIfaceWindowOpenSeen || this.n2hIfaceWindowCloseDone ||');
  const w6IssueAt = extSource.indexOf('runInterfaceStatsWindowClose(this.requestId).catch(');
  check('W6 the close read issues only behind the open latch, once (consumed at issue), with a last-resort catch',
    w6GuardAt > 0 && extSource.indexOf('this.n2hIfaceWindowCloseDone = true;') > w6GuardAt &&
    w6IssueAt > w6GuardAt && probeCall > 0,
    JSON.stringify({ w6GuardAt, w6IssueAt }));
  const latchCbAt = extSource.indexOf('this.n2hIfaceWindowOpenSeen = true;');
  check('W6 the open latch is one boolean written inside the ok-branch after the probe call',
    okBranchAt > 0 && armedLit > okBranchAt && probeCall > armedLit &&
    latchCbAt > probeCall && elseAt > latchCbAt, JSON.stringify({ okBranchAt, probeCall, latchCbAt, elseAt }));
  // "The read happens AFTER the native arm": the open edge sits in the
  // ok-branch past the `n2h_collector_arm(...)` call, and the close edge is
  // gated by the open latch that only that same branch sets (so the close read
  // cannot precede a successful native arm however the hook is scheduled).
  const armCallAt = extSource.indexOf('core.n2h_collector_arm(');
  check('W6 both window reads issue only AFTER the native arm call (open in the ok-branch, close behind the open latch)',
    armCallAt > 0 && probeCall > armCallAt && latchCbAt > armCallAt &&
    w6GuardAt > 0 && w6IssueAt > w6GuardAt,
    JSON.stringify({ armCallAt, probeCall, latchCbAt }));
  check('W6 both window edges are emitted by the byte-only probe module, diag-only',
    probeSource.indexOf(WINDOW_PREFIX) > 0 &&
    probeSource.indexOf("emitWindowLine(requestId, 'open', PROBE_IFACE, vpnSample)") > 0 &&
    probeSource.indexOf("emitWindowLine(requestId, 'close', PROBE_IFACE, sample)") > 0 &&
    probeSource.indexOf("'diag=true|is_evidence=false'") > 0,
    'window source pin');

  // ---------------- NC (2026-09-20): the network-context snapshot ----------
  // The PUBLIC connection getters are sampled ONCE per arm SUCCESS, in the
  // same branch as the iface probe. This is context for the parallel native
  // SO_MARK observation — NOT a routing verdict: a netId/interface/route list
  // is the system's metadata view, and nothing here decodes SO_MARK or claims
  // the socket's final path. Check shape only: values seeded into the stub
  // come out verbatim, rejections keep their code without blanking the other
  // reads, and empty worlds record empty.
  const netctxAbility = freshAbility(mod, FIXTURE_FILESDIR);
  netctxAbility['n2hAcceptedExclusionGeneration'] = 0;
  // Two nets, in the exact getAllNets order; net 101 carries two routes
  // (one without the since-20 isExcludedRoute key -> 'absent'), net 103 keeps
  // the stub default (wlan0, routes empty). The LAN-style values are stub
  // fixtures — they assert FIELD PLUMBING, never a real device's addresses.
  seedNetCtx({
    __NB_NET_DEFAULT_ID: 101,
    __NB_NET_APP_ID: 102,
    __NB_NET_ALL_IDS: [103, 101],
    __NB_NET_CAPS_BY_ID: {
      103: { networkCap: [], bearerTypes: [4] },
      101: { networkCap: [], bearerTypes: [1] }
    },
    __NB_NET_PROPS_BY_ID: {
      101: { routes: [
        { interface: 'wlan0',
          destination: { address: { address: '192.168.50.0', family: 1 }, prefixLength: 24 },
          gateway: { address: '192.168.50.1', family: 1 }, hasGateway: true,
          isDefaultRoute: false },
        { interface: 'wlan0',
          destination: { address: { address: '0.0.0.0', family: 1 }, prefixLength: 0 },
          gateway: { address: '192.168.50.1', family: 1 }, hasGateway: true,
          isDefaultRoute: true }
      ] }
    }
  });
  setArmFile(true, undefined);
  resetLog();
  tickArm(netctxAbility, WG_READY);
  await drainProbe();
  const nc1Lines = linesWith(NETCTX_LINE);
  const nc1 = nc1Lines.length === 1 ? nc1Lines[0] : '';
  check('NC1 the armed snapshot carries seeded default/app netIds, the list verbatim, ' +
    'per-net iface/bearer/routes, and digit begin/end',
    nc1Lines.length === 1 &&
    nc1 === 'N2H_NET_CONTEXT|requestId=' + REQUEST_ID +
    '|defaultNetId=101|defaultNetError=none|appNetId=102|appNetError=none|' +
    'nets=2|allError=none|truncated=false|netsDetail=' +
    'netId=103,iface=wlan0,bearer=4,capsError=none,propsError=none,routes=empty;' +
    'netId=101,iface=wlan0,bearer=1,capsError=none,propsError=none,routes=' +
    'dest~192.168.50.0~prefix~24~excl~absent~gw~192.168.50.1+' +
    'dest~0.0.0.0~prefix~0~excl~absent~gw~192.168.50.1' +
    '|startedUnixMs=' + fieldOf(nc1, 'startedUnixMs') +
    '|finishedUnixMs=' + fieldOf(nc1, 'finishedUnixMs') +
    '|diag=true|is_evidence=false' &&
    /^\d+$/.test(fieldOf(nc1, 'startedUnixMs')) &&
    /^\d+$/.test(fieldOf(nc1, 'finishedUnixMs')) &&
    Number(fieldOf(nc1, 'finishedUnixMs')) >= Number(fieldOf(nc1, 'startedUnixMs')),
    JSON.stringify(nc1Lines));

  // NC2: one rejected API keeps its code as 'unavailable' while every
  // independent read still records — appNet rejects 201, net 101's props
  // reject 201, and defaultNet / the full list / net 103's segment / net
  // 101's bearer all still come through.
  const nc2Ability = freshAbility(mod, FIXTURE_FILESDIR);
  nc2Ability['n2hAcceptedExclusionGeneration'] = 0;
  seedNetCtx({
    __NB_NET_DEFAULT_ID: 101,
    __NB_NET_THROW_CODES: { getAppNetSync: 201 },
    __NB_NET_ALL_IDS: [103, 101],
    __NB_NET_PROPS_BY_ID: { 101: { error: 201 } }
  });
  setArmFile(true, undefined);
  resetLog();
  tickArm(nc2Ability, WG_READY);
  await drainProbe();
  const nc2Lines = linesWith(NETCTX_LINE);
  const nc2 = nc2Lines.length === 1 ? nc2Lines[0] : '';
  check('NC2 a rejected API leaves its field unavailable with the code; the other ' +
    'independent reads still record',
    nc2Lines.length === 1 &&
    nc2 === 'N2H_NET_CONTEXT|requestId=' + REQUEST_ID +
    '|defaultNetId=101|defaultNetError=none|appNetId=unavailable|appNetError=201|' +
    'nets=2|allError=none|truncated=false|netsDetail=' +
    'netId=103,iface=wlan0,bearer=empty,capsError=none,propsError=none,routes=empty;' +
    'netId=101,iface=unavailable,bearer=empty,capsError=none,propsError=201,' +
    'routes=unavailable' +
    '|startedUnixMs=' + fieldOf(nc2, 'startedUnixMs') +
    '|finishedUnixMs=' + fieldOf(nc2, 'finishedUnixMs') +
    '|diag=true|is_evidence=false',
    JSON.stringify(nc2Lines));

  // NC3: the empty handle world — 0 / 0 / an empty list record verbatim as
  // '0'/'0'/nets=0/detail=empty. No physical/VPN selection story is invented.
  const nc3Ability = freshAbility(mod, FIXTURE_FILESDIR);
  nc3Ability['n2hAcceptedExclusionGeneration'] = 0;
  seedNetCtx({});
  setArmFile(true, undefined);
  resetLog();
  tickArm(nc3Ability, WG_READY);
  await drainProbe();
  const nc3Lines = linesWith(NETCTX_LINE);
  const nc3 = nc3Lines.length === 1 ? nc3Lines[0] : '';
  check('NC3 an empty handle world records 0/0/0 verbatim (detail=empty), no invented story',
    nc3Lines.length === 1 &&
    nc3.indexOf('N2H_NET_CONTEXT|requestId=' + REQUEST_ID +
      '|defaultNetId=0|defaultNetError=none|appNetId=0|appNetError=none|' +
      'nets=0|allError=none|truncated=false|netsDetail=empty|startedUnixMs=') === 0 &&
    nc3.endsWith('|diag=true|is_evidence=false'),
    JSON.stringify(nc3Lines));

  // NC4: only a real arm SUCCESS triggers the snapshot — a closed gate fires
  // nothing, a core not-ok answers nothing, and the armed instance never
  // repeats (one-shot inherited from the arm latch).
  const nc4Gate = freshAbility(mod, FIXTURE_FILESDIR);
  nc4Gate['n2hAcceptedExclusionGeneration'] = 0;
  seedNetCtx({ __NB_NET_ALL_IDS: [103] });
  setArmFile(true, undefined);
  resetLog();
  tickArm(nc4Gate, { wg: { device_up: true, ready: false } });
  const nc4CallsAtWait = linesWith(NETCTX_LINE).length;
  const nc4Refused = freshAbility(mod, FIXTURE_FILESDIR);
  nc4Refused['n2hAcceptedExclusionGeneration'] = 0;
  G['__NB_ARM_ANSWER'] = '{"ok":false,"error":"forced-by-nc4"}';
  tickArm(nc4Refused, WG_READY);
  G['__NB_ARM_ANSWER'] = undefined;
  await drainProbe();
  check('NC4 no ARM success fires nothing (gate closed, core not-ok) — zero snapshot lines',
    nc4CallsAtWait === 0 &&
    linesWith('N2H_COLLECTOR_ARM_FAILED|').length === 1 &&
    linesWith(NETCTX_LINE).length === 0 &&
    linesWith(PROBE_PREFIX).length === 0, JSON.stringify(logs()));
  resetLog();
  tickArm(netctxAbility, WG_READY);
  check('NC4 ticks after the arm repeat nothing (one-shot inherited)',
    logs().length === 0, JSON.stringify(logs()));

  // NC5: source pins — the snapshot stays getter-only metadata (no
  // bindSocket/setAppNet/setNet*, no exception message text), still shares the
  // byte-only module, rides the same ok-branch AFTER the iface probe, and the
  // last-resort aborted guard prints codes/names only.
  check('NC5 the snapshot is getter-only metadata in the shared probe module',
    probeSource.indexOf(NETCTX_LINE) > 0 &&
    probeSource.indexOf('bindSocket') < 0 &&
    probeSource.indexOf('setAppNet') < 0 &&
    probeSource.indexOf('setNet') < 0 &&
    probeSource.indexOf('.message') < 0 &&
    probeSource.indexOf('getIfaceRxBytes') > 0,
    'net-context module pin');
  const ncCall = extSource.indexOf('runNetworkContextProbe(this.requestId)');
  check('NC5 the snapshot call rides the same ok-branch, after the iface probe',
    ncCall > extSource.indexOf('runInterfaceStatsProbe(this.requestId, (): void => {') &&
    ncCall > armedLit && ncCall < elseAt,
    JSON.stringify({ armedLit, elseAt, ncCall }));
  check('NC5 the aborted guard prints codes/names only (never an exception message)',
    extSource.indexOf(
      "'N2H_NET_CONTEXT_ABORTED|requestId=%{public}s|code=%{public}s|'") > 0 &&
    extSource.indexOf("'name=%{public}s|diag=true|is_evidence=false',") > 0,
    'net-context aborted-guard pin');

  // ---------------- PB (2026-09-20): the posctl probe netId ----------------
  // The arm resolves the VPN network's netId via the OFFICIAL read-only
  // getters BEFORE the native arm and puts the non-secret integer into the
  // opts JSON ONLY when exactly one vpn-tun handle was found; every other
  // outcome keeps the key ABSENT (the probe stays byte-identical) and one
  // marker carries the reason + error CODE token. No bind ever happens in
  // the shell: the stub module has no bindSocket/setAppNet at all, so any
  // new call site would be a loud TypeError, and the source pins below keep
  // it structural.
  const PROBE_NET_MARKER = 'N2H_POSCTL_PROBE_NET_ID_UNAVAILABLE|';

  function armOptsJson(): Record<string, unknown> {
    const raw = G['__NB_ARM_OPTS'] as string | undefined;
    return raw === undefined ? {} : JSON.parse(raw) as Record<string, unknown>;
  }

  // PB1: found — one vpn-tun handle (net 147) lands in the opts JSON with
  // the other fields unchanged; the arm still answers ok exactly once.
  const pb1 = freshAbility(mod, FIXTURE_FILESDIR);
  pb1['n2hAcceptedExclusionGeneration'] = 0;
  seedNetCtx({
    __NB_NET_ALL_IDS: [147, 101],
    __NB_NET_PROPS_BY_ID: { 147: { interfaceName: 'vpn-tun' } }
  });
  setArmFile(true, undefined);
  resetLog();
  const pb1ArmCallsAt = Number(G['__NB_ARM_CALLS'] ?? 0);
  tickArm(pb1, WG_READY);
  await drainProbe();
  const pb1Opts = armOptsJson();
  check('PB1 the found vpn-tun netId rides the opts JSON verbatim, other fields unchanged',
    pb1Opts['probe_net_id'] === 147 &&
    pb1Opts['exclusion_generation'] === 0 &&
    typeof pb1Opts['posctl_port'] === 'number' && pb1Opts['posctl_port'] > 0 &&
    typeof pb1Opts['probe_wait_ms'] === 'number' && pb1Opts['probe_wait_ms'] > 0 &&
    Array.isArray(pb1Opts['shell_exclusions']),
    JSON.stringify(pb1Opts));
  check('PB1 the arm still answers ok exactly once and no unavailability marker fires',
    Number(G['__NB_ARM_CALLS']) === pb1ArmCallsAt + 1 &&
    linesWith('N2H_COLLECTOR_ARMED|').length === 1 &&
    linesWith(PROBE_NET_MARKER).length === 0,
    JSON.stringify(logs()));

  // PB2: not found — the default wlan0-only world keeps the key ABSENT and
  // records exactly ONE unavailability marker (reason only, code=none).
  const pb2 = freshAbility(mod, FIXTURE_FILESDIR);
  pb2['n2hAcceptedExclusionGeneration'] = 0;
  seedNetCtx({ __NB_NET_ALL_IDS: [103] });
  setArmFile(true, undefined);
  resetLog();
  tickArm(pb2, WG_READY);
  await drainProbe();
  const pb2Opts = armOptsJson();
  const pb2Markers = linesWith(PROBE_NET_MARKER);
  check('PB2 no vpn-tun handle keeps the key absent and prints ONE reason marker (not-found)',
    !('probe_net_id' in pb2Opts) &&
    (G['__NB_ARM_OPTS'] as string).indexOf('probe_net_id') < 0 &&
    pb2Markers.length === 1 &&
    pb2Markers[0].indexOf('reason=not-found|code=none|diag=true|is_evidence=false') > 0 &&
    linesWith('N2H_COLLECTOR_ARMED|').length === 1,
    JSON.stringify({ opts: pb2Opts, markers: pb2Markers }));

  // PB3: the list read rejects — key absent, ONE marker with the raw code.
  const pb3 = freshAbility(mod, FIXTURE_FILESDIR);
  pb3['n2hAcceptedExclusionGeneration'] = 0;
  seedNetCtx({ __NB_NET_THROW_CODES: { getAllNetsSync: 201 } });
  setArmFile(true, undefined);
  resetLog();
  tickArm(pb3, WG_READY);
  await drainProbe();
  const pb3Markers = linesWith(PROBE_NET_MARKER);
  check('PB3 a rejected getAllNetsSync keeps the key absent and prints the code (list-error)',
    !('probe_net_id' in armOptsJson()) &&
    pb3Markers.length === 1 &&
    pb3Markers[0].indexOf('reason=list-error|code=201|diag=true|is_evidence=false') > 0,
    JSON.stringify(pb3Markers));

  // PB4: a name collision — two vpn-tun handles refuse the resolution (never
  // a guess by list order); key absent, ONE marker reason=ambiguous.
  const pb4 = freshAbility(mod, FIXTURE_FILESDIR);
  pb4['n2hAcceptedExclusionGeneration'] = 0;
  seedNetCtx({
    __NB_NET_ALL_IDS: [147, 148],
    __NB_NET_PROPS_BY_ID: {
      147: { interfaceName: 'vpn-tun' },
      148: { interfaceName: 'vpn-tun' }
    }
  });
  setArmFile(true, undefined);
  resetLog();
  tickArm(pb4, WG_READY);
  await drainProbe();
  const pb4Markers = linesWith(PROBE_NET_MARKER);
  check('PB4 two vpn-tun handles are ambiguous: key absent, ONE marker, never a list-order pick',
    !('probe_net_id' in armOptsJson()) &&
    pb4Markers.length === 1 &&
    pb4Markers[0].indexOf('reason=ambiguous|code=none|') > 0,
    JSON.stringify(pb4Markers));

  // PB5: the vpn-tun handle's props reject — the handle is skipped, the
  // world answers not-found (the code stays with the marker, never a value).
  const pb5 = freshAbility(mod, FIXTURE_FILESDIR);
  pb5['n2hAcceptedExclusionGeneration'] = 0;
  seedNetCtx({
    __NB_NET_ALL_IDS: [147],
    __NB_NET_PROPS_BY_ID: { 147: { error: 201 } }
  });
  setArmFile(true, undefined);
  resetLog();
  tickArm(pb5, WG_READY);
  await drainProbe();
  const pb5Markers = linesWith(PROBE_NET_MARKER);
  check('PB5 an unreadable vpn-tun handle is skipped: key absent, ONE marker (not-found)',
    !('probe_net_id' in armOptsJson()) &&
    pb5Markers.length === 1 &&
    pb5Markers[0].indexOf('reason=not-found|code=none|') > 0,
    JSON.stringify(pb5Markers));

  // PB6: source pins — the shell NEVER binds: the byte-identical extension
  // source has no connection-module import at all (netId facts flow ONLY
  // through the sibling getter-only module, whose own NC5 pins already hold),
  // the resolution sits BEFORE the native arm call, the only opts write is
  // the found branch, and the marker prints codes only.
  check('PB6 the extension source has no connection-module import (facts via the getter-only sibling)',
    extSource.indexOf('@ohos.net.connection') < 0 &&
    extSource.indexOf('resolveVpnTunNetId') > 0,
    'extension netId pin');
  const pbResolveAt = extSource.indexOf('resolveVpnTunNetId()');
  const pbArmAt = extSource.indexOf('core.n2h_collector_arm(optsJson)');
  check('PB6 the resolution runs BEFORE the native arm call and writes only on found',
    pbResolveAt > 0 && pbArmAt > pbResolveAt &&
    extSource.indexOf('opts.probe_net_id = vpnResolution.netId;') > pbResolveAt &&
    extSource.indexOf("'N2H_POSCTL_PROBE_NET_ID_UNAVAILABLE|requestId=%{public}s|reason=%{public}s|'") > 0,
    JSON.stringify({ pbResolveAt, pbArmAt }));
  check('PB6 no permission surface was added (module.json5 requestPermissions untouched)',
    readFileSync(join(HERE, '..', 'src', 'main', 'module.json5')).toString()
      .indexOf('GET_NETWORK_STATS') < 0,
    'module.json5 pin');

  rmSync(dir, { recursive: true, force: true });
} catch (error) {
  failedCount++;
  console.log('FAIL harness — ' + (error as Error).message + '\n' +
    ((error as Error).stack ?? ''));
}

console.log('');
console.log(String(passed) + ' passed, ' + String(failedCount) + ' failed');
process.exit(failedCount === 0 ? 0 : 1);
