/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Live route apply at the LIVE VPE (2026-10-09 instant-apply increment) —
// host-side: no device, no network, no compiler.
//
// Same principle as run-route-vpe.mts / run-exclusion-generation.mts: the REAL production
// files are copied byte-for-byte into an OS-temp fixture (.ts) and DRIVEN.
// Nothing under test is rewritten or mirrored:
//   vpnextensionability/NetBirdVpnExtensionAbility.ets  (the integration)
//   vpnextensionability/NetBirdConnector.ets            (the REAL bridge:
//                                    connectorRouteSelectionSet /
//                                    connectorRouteSetApplied(routes,isRecreate,
//                                    kind?) — not a stub)
//   routes/NetBirdRouteRequestStore.ets                 (the REAL IPC adapter)
//   routes/NetBirdRouteRequest.ets                      (the REAL documents)
//   routes/NetBirdRouteStore.ets                        (the REAL selection store)
//   routes/NetBirdRoutePreferences.ets                  (the REAL pure rules)
// The loader seam is route-apply-vpe-hooks.mjs, which reuses the existing
// exclusion-generation fake FS (the async half of @ohos.file.fs + @ohos.util),
// so the REAL store's open/read/write/fsync/close/rename path runs unchanged.
// The two seams this increment itself consumes — the native live setter and the
// per-test ACK answer — are driven by wrapping the fixture's own core stubs, so
// the bridge's real parse/projection still runs over them.
//
// Coverage (task mapping):
//   L1  a request that changes the routes -> EXACTLY ONE controlled recreate,
//       same connector/WG/ICE/management session (no connector stop, no start,
//       no socket open, no socket feed), ACK kind 'user', and an 'applied' /
//       'recreated' answer carrying the requested set.
//   L2  >3 sequential user changes: every one of them recreates and every ACK
//       is kind 'user' (the automatic 3/30 budget is the CORE's business and is
//       never spent from here).
//   L3  an EMPTY selection recreates (the overlay graft still stands) and ACKs
//       `[]`; the initial leg ACKs an AVAILABLE map with an empty desired set as
//       `[]` with kind 'initial' instead of skipping — only a MISSING core
//       snapshot still skips.
//   L4  a native answer that proves desired == applied ("no OS change") answers
//       applied / no-os-change with NO destroy, create, feed or ACK.
//   L5  the busy latch serializes overlapping ticks, and a request queued behind
//       an in-flight recreate is processed after it — with the LATEST preference
//       document, never a copy carried by the request.
//   L6  duplicate id / foreign scope / untrustworthy preference document /
//       unreadable request: the first two are answered (never retried forever),
//       the last two leave the running connection and the documents untouched,
//       and an unreadable request is never answered under a guessed id.
//   L7  a teardown latched while destroy() / the settle wait / create() / the
//       interface verification is awaiting never reaches the TUN feed or the
//       ACK, and the fresh generation is torn down (no orphan TUN, no late ACK).
//   L8  the answer document appears only AFTER the ACK was accepted.
//   L9  the pre-existing automatic caller still ACKs kind 'auto' and the recreate
//       reports its real outcome token.
//   L10 the page's mailbox rides the EXISTING 100 ms socket-supply tick, and that
//       interval is stopped ONLY by the pre-existing teardown.
//   L11 the remaining native answers: no-map (stored-no-map, NOT applied), a
//       refusal (its own error token, connection untouched), an unreadable answer
//       (never applied) and a WG-seam refusal after the mutation (the existing
//       fail-closed teardown).
//   L12 the defer rule: a request arriving during an in-flight recreate or after
//       teardown is left unconsumed and unanswered, and is applied later; an
//       absent preference document means the all-on default.
//   L13 the Rust management worker may advance the NetworkMap between the setter
//       call and the recreate's OWN snapshot read (two distinct NAPI calls in one
//       JS turn): the platform config and the ACK carry the snapshot's set B, and
//       the answer reports that ACKed set B — never the setter's earlier set A.
//   SR  the startup race (2026-10-07): the initial leg is parked inside its FIRST
//       ICE pool protect; a mailbox tick in that window consumes nothing and opens
//       no rebuildable state; releasing it lets the leg really finish (pool fed +
//       initial ACK) and the retained request is then applied exactly once as
//       no-os-change; a user stop / connector death / refused outbound socket can
//       never be undone by a late completion.
//   P   source pins: one call site, the synchronous decision turn, the four
//       answer branches, the two teardown re-checks, no new timer, no direct FS
//       access from the consumer, no locally re-spelled state/mode literal.
//
// Run: node client/entry/selfcheck/run-route-apply-vpe.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./route-apply-vpe-hooks.mjs', import.meta.url).href);

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_DIR = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability');
const REAL_ROUTES_DIR = join(HERE, '..', 'src', 'main', 'ets', 'routes');
const G = globalThis as unknown as Record<string, unknown>;

const FILES_DIR = '/tmp/nb-route-apply-vpe-fixture';
const SELECTION_PATH = `${FILES_DIR}/netbird-route-selection.json`;
const REQUEST_PATH = `${FILES_DIR}/netbird-route-apply-request.json`;
const REQUEST_TEMP_PATH = `${REQUEST_PATH}.tmp`;
const RESULT_PATH = `${FILES_DIR}/netbird-route-apply-result.json`;
const RESULT_TEMP_PATH = `${RESULT_PATH}.tmp`;
const MGMT = 'http://mgmt.test:33073';
const MGMT_OTHER = 'http://other.test:33073';
const REQUEST_ID = 'req-route-apply-vpe';

// The byte-identical staged set (same table as run-route-vpe.mts) plus the two
// route modules this increment's value imports add (the IPC adapter and the
// pure document module it parses with).
const STAGED = ['NetBirdVpnExtensionAbility', 'NetBirdConnector',
  'NetBirdVpnConfig', 'NetBirdRouteValidation', 'NetBirdEndpointExclusion',
  'NetBirdInterfaceStatsProbe', 'NetBirdPhysicalNetworkPolicy',
  'NetBirdIceSocketPool', 'NetBirdUiModel', 'NetBirdRecreatePolicy',
  // 2026-10-09 (0.0.17) onDestroy-entry stop marker: the pure marker core the
  // REAL extension now imports by value for its synchronous write; zero
  // imports and no erased types, so no shim (staged byte-identically).
  'NetBirdDiagnosticsCore',
  'NetBirdN2Stimulus', 'NetBirdN2AEvent', 'NetBirdN2Sandwich',
  'NetBirdN2ControlRequest', 'NetBirdN2CdPlan', 'NetBirdN2Ring'];
const STAGED_ROUTES = ['NetBirdRouteStore', 'NetBirdRoutePreferences',
  'NetBirdRouteRequest', 'NetBirdRouteRequestStore'];
const SHIM_TYPES: Record<string, string[]> = {
  NetBirdConnector: ['ConnectorStatusResult', 'CoreNetworkConfig', 'CoreRouteEntry',
    'ConnectorStartResult', 'ConnectorLogoutResult', 'DeviceConfigFile', 'ManagementEndpoint',
    'NativeTcpSocketOpen', 'RelayAdvertisedView', 'RouteSetAppliedResult',
    'SocketFeedResult', 'SocketSupplyRequest', 'TunReleaseResult', 'WgFeedResult',
    // STAGE 2/3 (official-alignment audit §2.7): the shared ICE mux arm type.
    'IceMuxArmResult',
    'PhysicalBearerFacts', 'PhoneLogConfigureResult', 'PhoneLogSummary'],
  NetBirdUiModel: ['UiNetworkConfig', 'UiStatusDocument'],
  NetBirdEndpointExclusion: ['ExcludedRouteEntry', 'EndpointResolution',
    'ExclusionBuildResult', 'ExclusionEndpoint', 'RelayEndpoint',
    'DnsDerivationInput', 'DnsDerivationResult', 'DnsNetProbe',
    'DnsPlatformNetAddress',
    'StunDerivationResult', 'StunUriEntry', 'StunParseResult', 'StunEndpoint'],
  NetBirdVpnConfig: ['VpnRouteConfig', 'VpnTunnelConfig',
    'TunnelSubnetRouteOutcome', 'CaptureRouteOutcome'],
  NetBirdRouteValidation: ['RouteCheckResult', 'RouteViolation'],
  NetBirdN2Stimulus: ['N2StimulusDecision', 'N2StimulusReply'],
  NetBirdN2AEvent: ['N2AEventDecision', 'N2AEventRequest', 'N2AEventReply'],
  NetBirdN2Sandwich: ['N2SandwichPost'],
  NetBirdN2ControlRequest: ['N2ControlDecision', 'N2ControlRequest'],
  NetBirdN2CdPlan: ['N2CdPlanDecision', 'N2CdPlanRequest', 'N2BearerFacts'],
  NetBirdN2Ring: ['N2RingDumpDecision', 'N2RingDumpRequest',
    'N2RingAckDecision', 'N2RingAckRequest']
};

// The live session's network map: one managed /24, the default route still held
// by the data-plane gate (so the recreate's config really is rebuilt).
const NETCFG = JSON.stringify({
  available: true,
  serial: 7,
  address: '100.108.144.165',
  address_prefix_len: 16,
  routes: [{ network: '10.10.10.0/24', is_default: false }],
  dns: { service_enable: true, servers: [{ ip: '9.9.9.9', port: 53 }] },
  default_route: { allowed: false, reason: 'data-plane-not-ready' },
  stuns: []
});
// The same map with an EMPTY desired set (the user turned every network off).
const NETCFG_EMPTY = JSON.stringify({
  available: true,
  serial: 8,
  address: '100.108.144.165',
  address_prefix_len: 16,
  routes: [],
  dns: { service_enable: true, servers: [{ ip: '9.9.9.9', port: 53 }] },
  default_route: { allowed: false, reason: 'data-plane-not-ready' },
  stuns: []
});

const SELECTION_OFF = {
  selected_routes: [], deselected_routes: ['HostInternal'], deselect_all: false
};
const SELECTION_OTHER = {
  selected_routes: [], deselected_routes: ['corp'], deselect_all: false
};
const SELECTION_EMPTY = {
  selected_routes: [], deselected_routes: [], deselect_all: true
};
const DESIRED = ['10.10.10.0/24'];

// L13: the NetworkMap the Rust management worker hands over AFTER it answered
// the setter call. It is a SEPARATE `connector_network_config()` read (the
// recreate leg's own snapshot), so it may legally differ from the setter's
// `desired_routes` even though the two calls sit in one JS turn.
const NETCFG_ADVANCED = JSON.stringify({
  available: true,
  serial: 8,
  address: '100.108.144.165',
  address_prefix_len: 16,
  routes: [{ network: '10.20.20.0/24', is_default: false }],
  dns: { service_enable: true, servers: [{ ip: '9.9.9.9', port: 53 }] },
  default_route: { allowed: false, reason: 'data-plane-not-ready' },
  stuns: []
});
const DESIRED_ADVANCED = ['10.20.20.0/24'];

// The REAL serializers/readers of the staged route modules (bound inside the
// try below, after the fixture is staged). The documents the fixture writes are
// therefore the SHIPPED bytes, never hand-rolled JSON.
let serializeRequest: (request: Record<string, unknown>) => string = (): string => '';
let serializeSelection: (url: string, selection: Record<string, unknown>) => string =
  (): string => '';
let parseResultDocument: (text: string) => Record<string, unknown> =
  (): Record<string, unknown> => ({});
let resultIsApplied: (result: Record<string, unknown>) => boolean = (): boolean => false;

// The per-test injection knobs of the two seams this increment consumes (see
// the patch block inside the try). They are module-scope so the patched stub
// functions can close over them.
let flipOnCreate = false;
let flipOnDestroy = false;
let armProbeFlipAtDestroy = false;
let armProbeFlipAtCreate = false;
let holdCreate = false;
let releaseCreate: ((fd: number) => void) | null = null;
let currentAbility: Record<string, unknown> | null = null;

let passed = 0;
let failed = 0;
let fixtureDir = '';

function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
  if (!condition && detail !== undefined) { console.log(`     ${detail}`); }
}

function files(): Record<string, string> {
  return (G['__NB_FS'] ?? {}) as Record<string, string>;
}

function fileOf(path: string): string | undefined {
  return files()[path];
}

function seedFile(path: string, text: string): void {
  const map = files();
  map[path] = text;
  G['__NB_FS'] = map;
}

function logs(): string[] {
  return (G['__NB_LOG'] as string[]) ?? [];
}

// The recording hilog stub prefixes each line with '[<lvl>] '; strip it so the
// assertions match the REAL format strings verbatim.
function linesWith(prefix: string): string[] {
  return logs().filter((line: string): boolean => line.indexOf(prefix) >= 0);
}

function count(name: string): number {
  const value = G[name];
  return typeof value === 'number' ? value : 0;
}

function delay(ms: number): Promise<void> {
  return new Promise<void>((resolve: () => void): void => { setTimeout(resolve, ms); });
}

function answer(payload: Record<string, unknown>): void {
  G['__NB_SELECTION_SET_ANSWER'] = JSON.stringify(payload);
}

function setterAnswer(available: boolean, changed: boolean, desired: string[],
  recreateNeeded: boolean): Record<string, unknown> {
  return {
    ok: true, available: available, changed: changed,
    desired_routes: desired, recreate_needed: recreateNeeded
  };
}

// One fresh world per scenario: empty in-memory fs + fresh core answers +
// released injection knobs.
function resetWorld(): void {
  G['__NB_FS'] = {};
  G['__NB_FS_OPEN_ERROR'] = {};
  G['__NB_LOG'] = [];
  G['__NB_NETCFG'] = NETCFG;
  G['__NB_STATUS'] = { relay: { urls: [] } };
  G['__NB_FD_STATUS'] = { open: true, errno: 0 };
  G['__NB_ROUTES'] = undefined;
  G['__NB_SELECTION_SET_CALLS'] = 0;
  G['__NB_SELECTION_SET_ARGS'] = undefined;
  G['__NB_SELECTION_SET_ANSWER'] = undefined;
  G['__NB_ACK_CALLS'] = 0;
  G['__NB_ACK'] = undefined;
  G['__NB_RESULT_AT_ACK'] = undefined;
  G['__NB_CREATE_CALLS'] = 0;
  G['__NB_CREATE_CONFIGS'] = [];
  G['__NB_DESTROY_CALLS'] = 0;
  G['__NB_NEW_CONN_DESTROYED'] = false;
  G['__NB_TUN_FEED_CALLS'] = 0;
  G['__NB_STOP_CALLS'] = 0;
  G['__NB_START_CALLS'] = 0;
  G['__NB_SUPPLY_MGMT_OPEN'] = 0;
  G['__NB_SUPPLY_WG_OPEN'] = 0;
  G['__NB_SUPPLY_ICE_OPEN'] = 0;
  G['__NB_SUPPLY_MGMT_FEED'] = 0;
  G['__NB_SUPPLY_ICE_FEED'] = 0;
  // STAGE 2/3: the WG feed and the ONE shared-mux arm are supply seams too, and
  // the SR group asserts them EXACTLY (fed once, armed once, the retired pool at
  // zero) — so they belong to the per-world reset like every other seam counter.
  G['__NB_SUPPLY_WG_FEED'] = 0;
  G['__NB_SUPPLY_ICE_MUX_ARM'] = 0;
  G['__NB_SUPPLY_POLL'] = 0;
  G['__NB_IFACE_PROBES'] = 0;
  G['__NB_IFACE_ANSWER'] = undefined;
  G['__NB_FLIP_ON_PROBE'] = false;
  // The startup-race knobs of the shared vpnExtension stub
  // (exclusion-generation-hooks.mjs): no parked protect, no forced outbound-open
  // answer, and a FRESH protect-call counter may leak into the next world (the
  // park matches the n-th call of THIS world).
  G['__NB_PROTECT_PARK_AT'] = undefined;
  G['__NB_PROTECT_PARKED'] = undefined;
  G['__NB_PROTECTED'] = 0;
  G['__NB_WG_FD'] = undefined;
  flipOnCreate = false;
  flipOnDestroy = false;
  armProbeFlipAtDestroy = false;
  armProbeFlipAtCreate = false;
  holdCreate = false;
  releaseCreate = null;
  currentAbility = null;
}

function makeAbility(mod: Record<string, unknown>): Record<string, unknown> {
  const ability = new (mod['default'] as new () => Record<string, unknown>)();
  ability['requestId'] = REQUEST_ID;
  ability['context'] = { filesDir: FILES_DIR };
  ability['managementUrl'] = MGMT;
  currentAbility = ability;
  return ability;
}

// A LIVE, settled generation: a running connection, a settled create and a
// valid TUN fd — the shape every apply requires. `createPromise` is the initial
// leg's outcome (resolved, destroyRequired) exactly as production has it, so the
// pre-existing fail-closed teardown really destroys the current generation.
function settledWorld(mod: Record<string, unknown>,
  createConnection: () => unknown): Record<string, unknown> {
  const ability = makeAbility(mod);
  ability['connection'] = createConnection();
  ability['createPromise'] = Promise.resolve({
    destroyRequired: true, accepted: true, reason: 'fixture-initial-created'
  });
  ability['createSettled'] = true;
  ability['tunFd'] = 42;
  ability['fdConfirmedOpenAfterCreate'] = true;
  return ability;
}

function writeRequestDoc(id: string, scope: string = MGMT): void {
  seedFile(REQUEST_PATH, serializeRequest({
    id: id, management_url: scope, created_at_ms: 1
  }));
}

function result(): Record<string, unknown> | undefined {
  const text = fileOf(RESULT_PATH);
  return text === undefined ? undefined : parseResultDocument(text);
}

function poll(ability: Record<string, unknown>): void {
  (ability['maybePollRouteApplyRequest'] as () => void)();
}

async function waitIdle(ability: Record<string, unknown>, maxMs: number = 12000): Promise<void> {
  const deadline: number = Date.now() + maxMs;
  while (Date.now() < deadline) {
    await delay(10);
    if (ability['routeRequestBusy'] === false) { break; }
  }
  // let the fire-and-forget teardown legs (failRecreate -> destroyOnce) land
  await delay(30);
}

async function applyAndSettle(ability: Record<string, unknown>): Promise<void> {
  poll(ability);
  await waitIdle(ability);
}

// Park the in-flight recreate inside the fake create() so a test can observe the
// document/session state while the apply really is in flight.
async function waitParked(maxMs: number = 8000): Promise<void> {
  const deadline: number = Date.now() + maxMs;
  while (Date.now() < deadline) {
    await delay(10);
    if (releaseCreate !== null) { return; }
  }
}

function releaseParked(): void {
  const release: ((fd: number) => void) | null = releaseCreate;
  if (release !== null) { release(99); }
}

function flipTeardown(): void {
  const ability: Record<string, unknown> | null = currentAbility;
  if (ability !== null) { ability['destroyRequested'] = true; }
}

// ---------------------------------------------------------------------------
// The startup-race world: the REAL initial leg, driven through the REAL
// settleCreate and the REAL protect chain of the fixture's FakeConnection
// (exclusion-generation-hooks.mjs). protect() call 1 is the outbound WG socket
// (the fail-closed gate), calls 2..1+ICE_SOCKET_POOL_MIN are the ICE pool. The
// fixture parks one of them (__NB_PROTECT_PARK_AT) and releases it explicitly,
// so the window between "outbound gate passed / WG fed" and "ICE pool fed +
// initial ACK" is observable and steerable.
// ---------------------------------------------------------------------------
function parkProtectAt(n: number): void {
  G['__NB_PROTECT_PARK_AT'] = n;
  G['__NB_PROTECT_PARKED'] = undefined;
}

function parkedProtect(): Record<string, unknown> | null {
  const parked: unknown = G['__NB_PROTECT_PARKED'];
  return parked === undefined || parked === null ? null : parked as Record<string, unknown>;
}

async function waitProtectParked(maxMs: number = 8000): Promise<void> {
  const deadline: number = Date.now() + maxMs;
  while (Date.now() < deadline) {
    await delay(10);
    if (parkedProtect() !== null) { return; }
  }
}

function releaseParkedProtect(): void {
  const parked: Record<string, unknown> | null = parkedProtect();
  G['__NB_PROTECT_PARK_AT'] = undefined; // every later protect resolves as always
  G['__NB_PROTECT_PARKED'] = undefined;
  if (parked !== null) { (parked['resolve'] as () => void)(); }
}

// A REAL initial leg in progress: the connection whose protect chain the leg
// drives, plus the route set its initial ACK will report. `settleCreate` is the
// production entry the create() continuation calls (`rawCreate.then`), and
// `createPromise` is the value that continuation stores — the same promise the
// pre-existing fail-closed teardown awaits before destroying the generation.
function settlingWorld(mod: Record<string, unknown>,
  createConnection: () => unknown, appliedRoutes: string[]): Record<string, unknown> {
  const ability = makeAbility(mod);
  ability['connection'] = createConnection();
  ability['pendingAppliedRoutes'] = appliedRoutes;
  return ability;
}

function startInitialSettle(ability: Record<string, unknown>): Promise<unknown> {
  const settle: Promise<unknown> = (ability['settleCreate'] as
    (fd: number, gen: number) => Promise<unknown>)(42, 0);
  ability['createPromise'] = settle;
  return settle;
}

// The ONE rebuildable gate every rebuild / live-apply path reads: a live
// connection AND a settled initial generation AND a valid TUN fd (the same
// three terms the recreate precondition and the live-apply consumer spell
// out). The create-REJECTED terminal keeps `createSettled` true with
// tunFd = -1, so the tests below assert THIS composite — never "every flag
// false".
function rebuildableGate(ability: Record<string, unknown>): boolean {
  return ability['connection'] !== undefined && ability['createSettled'] === true &&
    typeof ability['tunFd'] === 'number' && (ability['tunFd'] as number) >= 0;
}

try {
  fixtureDir = mkdtempSync(join(tmpdir(), 'nb-route-apply-vpe-'));
  const dir: string = fixtureDir;
  for (const name of STAGED) {
    writeFileSync(join(dir, name + '.ts'),
      readFileSync(join(REAL_DIR, name + '.ets'))); // byte-identical real file
  }
  for (const name of STAGED_ROUTES) {
    writeFileSync(join(dir, name + '.ts'),
      readFileSync(join(REAL_ROUTES_DIR, name + '.ets'))); // byte-identical real file
  }
  for (const name of Object.keys(SHIM_TYPES)) {
    writeFileSync(join(dir, name + '.shim.ts'),
      'export * from \'./' + name + '.ts\';\n' +
      SHIM_TYPES[name].map((t: string): string => 'export const ' + t + ' = undefined;')
        .join('\n') + '\n');
  }
  writeFileSync(join(dir, 'NetBirdDiagnostics.shim.ts'),
    'export const start = () => undefined;\n' +
    'export const event = () => false;\n' +
    'export const appendSystemEvent = () => false;\n' +
    'export const stop = () => undefined;\n');
  writeFileSync(join(dir, 'NetBirdSystemEvents.shim.ts'),
    'export const start = () => false;\n' +
    'export const stop = () => undefined;\n' +
    'export const isRunning = () => false;\n');

  const mod = await import(pathToFileURL(join(dir, 'NetBirdVpnExtensionAbility.ts')).href) as
    Record<string, unknown>;
  const requests = await import(pathToFileURL(join(dir, 'NetBirdRouteRequest.ts')).href) as
    Record<string, unknown>;
  const prefs = await import(pathToFileURL(join(dir, 'NetBirdRoutePreferences.ts')).href) as
    Record<string, unknown>;
  const coreStub = ((await import('libnetbird_core.so')) as Record<string, unknown>)['default'] as
    Record<string, unknown>;
  const vpnNs = ((await import('@ohos.net.vpnExtension')) as Record<string, unknown>)['default'] as
    Record<string, unknown>;
  const statsNs = ((await import('@ohos.net.statistics')) as Record<string, unknown>)['default'] as
    Record<string, unknown>;
  const extSource: string = readFileSync(
    join(REAL_DIR, 'NetBirdVpnExtensionAbility.ets'), 'utf8');

  serializeRequest = requests['serializeRouteApplyRequestDocument'] as
    (request: Record<string, unknown>) => string;
  serializeSelection = prefs['serializeRouteSelectionDocument'] as
    (url: string, selection: Record<string, unknown>) => string;
  parseResultDocument = requests['parseRouteApplyResultDocument'] as
    (text: string) => Record<string, unknown>;
  resultIsApplied = requests['routeApplyResultIsApplied'] as
    (result: Record<string, unknown>) => boolean;

  // -------------------------------------------------------------------------
  // The two seams THIS increment consumes, driven through the fixture's own
  // core stub (the REAL connector bridge still parses/projects every answer):
  //   1. connector_route_selection_set — the live setter. Per-test answer via
  //      __NB_SELECTION_SET_ANSWER (a STRING is returned verbatim, which is also
  //      how "the native answered garbage" = the bridge's null is driven).
  //   2. connector_route_set_applied — wrapped to record the ACK KIND, the
  //      payload and whether an answer document already existed at ACK time.
  // Plus counters for the seams that must stay untouched (stop / start / TUN
  // feed) and a per-instance destroy marker so "the FRESH generation was torn
  // down" is observable.
  // -------------------------------------------------------------------------
  coreStub['connector_route_selection_set'] = (selectionJson: string): string => {
    G['__NB_SELECTION_SET_CALLS'] = count('__NB_SELECTION_SET_CALLS') + 1;
    G['__NB_SELECTION_SET_ARGS'] = JSON.parse(selectionJson) as Record<string, unknown>;
    const explicit = G['__NB_SELECTION_SET_ANSWER'];
    if (typeof explicit === 'string') { return explicit; }
    return JSON.stringify(setterAnswer(true, true, DESIRED, true));
  };
  const originalAck = coreStub['connector_route_set_applied'] as
    (payload: string, isRecreate: boolean, kind?: string) => string;
  // The pristine setter seam, kept so L13 can drive the worker's map advance
  // through a per-test override and then restore the shared stub.
  const defaultSelectionSetter = coreStub['connector_route_selection_set'] as
    (selectionJson: string) => string;
  coreStub['connector_route_set_applied'] =
    (payload: string, isRecreate: boolean, kind?: string): string => {
      G['__NB_ACK_CALLS'] = count('__NB_ACK_CALLS') + 1;
      // Whether an answer document already existed AT ACK TIME. Captured before
      // the call, because the shared stub answers through the same seams.
      G['__NB_RESULT_AT_ACK'] =
        Object.prototype.hasOwnProperty.call(files(), RESULT_PATH);
      const answer: string = originalAck(payload, isRecreate, kind);
      // The shared stub records its own {payload,isRecreate} shape, so the
      // kind-aware record is written AFTER it (the bridge hands the KIND as the
      // third argument; keeping it is the whole point of this wrapper).
      G['__NB_ACK'] = {
        payload: JSON.parse(payload) as Record<string, unknown>,
        isRecreate: isRecreate,
        kind: kind
      };
      return answer;
    };
  const originalStop = coreStub['connector_stop'] as () => string;
  coreStub['connector_stop'] = (): string => {
    G['__NB_STOP_CALLS'] = count('__NB_STOP_CALLS') + 1;
    return originalStop();
  };
  const originalTunFeed = coreStub['connector_tun_fd_feed'] as (fd: number) => string;
  coreStub['connector_tun_fd_feed'] = (fd: number): string => {
    G['__NB_TUN_FEED_CALLS'] = count('__NB_TUN_FEED_CALLS') + 1;
    return originalTunFeed(fd);
  };
  const connectionProto = (vpnNs['VpnConnection'] as { prototype: Record<string, unknown> })
    .prototype;
  const originalCreate = connectionProto['create'] as (config: unknown) => Promise<number>;
  const originalDestroy = connectionProto['destroy'] as () => Promise<void>;
  connectionProto['create'] = function (this: unknown, config: unknown): Promise<number> {
    G['__NB_CREATE_CALLS'] = count('__NB_CREATE_CALLS') + 1;
    G['__NB_CREATE_CONFIGS'] = ((G['__NB_CREATE_CONFIGS'] as unknown[]) ?? []).concat([config]);
    G['__NB_NEW_CONN'] = this;
    if (flipOnCreate) { flipTeardown(); }
    if (armProbeFlipAtCreate) { G['__NB_FLIP_ON_PROBE'] = true; }
    if (holdCreate) {
      return new Promise<number>((resolve: (fd: number) => void): void => {
        releaseCreate = resolve;
      });
    }
    return originalCreate.call(this, config);
  };
  connectionProto['destroy'] = function (this: unknown): Promise<void> {
    G['__NB_DESTROY_CALLS'] = count('__NB_DESTROY_CALLS') + 1;
    if (G['__NB_NEW_CONN'] === this) { G['__NB_NEW_CONN_DESTROYED'] = true; }
    if (flipOnDestroy) { flipTeardown(); }
    if (armProbeFlipAtDestroy) { G['__NB_FLIP_ON_PROBE'] = true; }
    return originalDestroy.call(this);
  };
  statsNs['getIfaceRxBytes'] = async (): Promise<number> => {
    G['__NB_IFACE_PROBES'] = count('__NB_IFACE_PROBES') + 1;
    if (G['__NB_FLIP_ON_PROBE'] === true) {
      G['__NB_FLIP_ON_PROBE'] = false;
      flipTeardown();
    }
    const answerText = G['__NB_IFACE_ANSWER'];
    if (answerText === 'present') { return 0; }
    // 'absent' = the documented "no such interface" code; anything else is the
    // shared fixture's own "probe could not prove absence" (lenient 'unknown'),
    // which is also the default so a happy recreate leaves the settle loop at
    // its floor exactly like every pre-existing drive does.
    const error = new Error('iface-probe');
    (error as Error & { code?: string }).code =
      answerText === 'absent' ? '2103012' : '2100003';
    throw error;
  };

  const createConnection = (): unknown =>
    (vpnNs['createVpnConnection'] as (context: unknown) => unknown)(null);

  // ---- L1: a route change applies through ONE recreate, session untouched ---
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  answer(setterAnswer(true, true, DESIRED, true));
  writeRequestDoc('ui-route-1');
  {
    const ability = settledWorld(mod, createConnection);
    await applyAndSettle(ability);
    const doc = result();
    check('L1a the answer claims applied/recreated with the requested set, and the ONE ' +
      'predicate reads it as applied',
      doc !== undefined && doc['id'] === 'ui-route-1' && doc['state'] === 'applied' &&
      doc['mode'] === 'recreated' && doc['error'] === '' &&
      JSON.stringify(doc['routes']) === JSON.stringify(DESIRED) &&
      resultIsApplied(doc) === true, JSON.stringify(doc));
    check('L1b exactly ONE recreate and the same connector / WG / ICE / management session ' +
      '(no stop, no start, no socket open, no socket feed)',
      count('__NB_CREATE_CALLS') === 1 && count('__NB_TUN_FEED_CALLS') === 1 &&
      count('__NB_STOP_CALLS') === 0 && count('__NB_START_CALLS') === 0 &&
      count('__NB_SUPPLY_MGMT_OPEN') === 0 && count('__NB_SUPPLY_WG_OPEN') === 0 &&
      count('__NB_SUPPLY_ICE_OPEN') === 0 && count('__NB_SUPPLY_MGMT_FEED') === 0 &&
      count('__NB_SUPPLY_ICE_FEED') === 0,
      JSON.stringify({ create: count('__NB_CREATE_CALLS'),
        tunFeed: count('__NB_TUN_FEED_CALLS'), stop: count('__NB_STOP_CALLS'),
        start: count('__NB_START_CALLS') }));
    const ack = (G['__NB_ACK'] ?? {}) as Record<string, unknown>;
    check('L1c the native setter received the LATEST preference selection and the recreate ' +
      'ACKed it as isRecreate=true / kind=user',
      JSON.stringify(G['__NB_SELECTION_SET_ARGS']) === JSON.stringify(SELECTION_OFF) &&
      ack['isRecreate'] === true && ack['kind'] === 'user' &&
      JSON.stringify(ack['payload']) === JSON.stringify({ routes: DESIRED }),
      JSON.stringify({ selection: G['__NB_SELECTION_SET_ARGS'], ack: ack }));
    check('L1d the recreate really submitted a rebuilt platform config',
      (G['__NB_CREATE_CONFIGS'] as unknown[]).length === 1);
  }

  // ---- L13: the map advances between the setter and the recreate's OWN read -
  // The setter answer and the recreate's `connector_network_config()` read are
  // TWO distinct native calls. "No await between them" constrains only the
  // shell's watcher tick: the Rust management worker is a separate thread and
  // may land a newer NetworkMap inside that window. The recreate already ACKs
  // the set ITS OWN snapshot holds; the answer must report THAT set, never the
  // setter's earlier desired list.
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  coreStub['connector_route_selection_set'] = (selectionJson: string): string => {
    G['__NB_SELECTION_SET_CALLS'] = count('__NB_SELECTION_SET_CALLS') + 1;
    G['__NB_SELECTION_SET_ARGS'] = JSON.parse(selectionJson) as Record<string, unknown>;
    // the worker's newer map lands right after this answer, still inside the
    // same JS turn, and BEFORE the recreate's own snapshot read
    G['__NB_NETCFG'] = NETCFG_ADVANCED;
    return JSON.stringify(setterAnswer(true, true, DESIRED, true));
  };
  writeRequestDoc('ui-route-advanced');
  {
    const ability = settledWorld(mod, createConnection);
    await applyAndSettle(ability);
    const doc = result();
    const ack = (G['__NB_ACK'] ?? {}) as Record<string, unknown>;
    const configs = (G['__NB_CREATE_CONFIGS'] as Record<string, unknown>[]) ?? [];
    const submitted: Record<string, unknown>[] = configs.length === 1
      ? ((configs[0]['routes'] as Record<string, unknown>[]) ?? []) : [];
    const hasRoute = (destination: string, prefix: number): boolean =>
      submitted.some((route: Record<string, unknown>): boolean => {
        // the REAL platform RouteInfo shape the staged builder emits:
        // { interface, destination: { address: { address, family }, prefixLength }, … }
        const target = route['destination'] as Record<string, unknown> | undefined;
        const addr = target === undefined ? undefined
          : (target['address'] as Record<string, unknown> | undefined);
        return target !== undefined && addr !== undefined &&
          target['prefixLength'] === prefix && addr['address'] === destination;
      });
    check('L13a the recreate submits the set its OWN snapshot holds (B, plus the grafted ' +
      'tunnel-subnet overlay) and ACKs exactly that — not the setter\'s earlier set A',
      count('__NB_CREATE_CALLS') === 1 && count('__NB_TUN_FEED_CALLS') === 1 &&
      hasRoute('10.20.20.0', 24) && hasRoute('100.108.0.0', 16) &&
      !hasRoute('10.10.10.0', 24) &&
      ack['isRecreate'] === true && ack['kind'] === 'user' &&
      JSON.stringify(ack['payload']) === JSON.stringify({ routes: DESIRED_ADVANCED }),
      JSON.stringify({ ack: ack, submitted: submitted }));
    check('L13b the answer reports the ACKed set B, never the setter\'s earlier desired A',
      doc !== undefined && doc['state'] === 'applied' && doc['mode'] === 'recreated' &&
      doc['error'] === '' &&
      JSON.stringify(doc['routes']) === JSON.stringify(DESIRED_ADVANCED) &&
      resultIsApplied(doc) === true, JSON.stringify(doc));
  }
  coreStub['connector_route_selection_set'] = defaultSelectionSetter;

  // ---- L2: more than three sequential user changes, every ACK kind user -----
  resetWorld();
  answer(setterAnswer(true, true, DESIRED, true));
  {
    const ability = settledWorld(mod, createConnection);
    const ids: string[] = ['ui-route-s1', 'ui-route-s2', 'ui-route-s3', 'ui-route-s4'];
    const seen: Record<string, unknown>[] = [];
    for (const id of ids) {
      seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
      writeRequestDoc(id);
      await applyAndSettle(ability);
      const doc = result();
      if (doc !== undefined) { seen.push(doc); }
    }
    check('L2a four sequential user changes each produced their own applied/recreated answer',
      seen.length === 4 &&
      seen.every((doc: Record<string, unknown>): boolean =>
        doc['state'] === 'applied' && doc['mode'] === 'recreated') &&
      JSON.stringify(seen.map((doc: Record<string, unknown>): unknown => doc['id'])) ===
        JSON.stringify(ids),
      JSON.stringify(seen));
    check('L2b every one of the four ACKs is kind user (the automatic budget stays the core\'s)',
      count('__NB_ACK_CALLS') === 4 && count('__NB_CREATE_CALLS') === 4 &&
      count('__NB_STOP_CALLS') === 0 && count('__NB_START_CALLS') === 0);
  }

  // ---- L3: an empty selection still recreates, and ACKs [] -------------------
  resetWorld();
  G['__NB_NETCFG'] = NETCFG_EMPTY;
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_EMPTY));
  answer(setterAnswer(true, true, [], true));
  writeRequestDoc('ui-route-empty');
  {
    const ability = settledWorld(mod, createConnection);
    await applyAndSettle(ability);
    const doc = result();
    const ack = (G['__NB_ACK'] ?? {}) as Record<string, unknown>;
    check('L3a an EMPTY desired set rebuilds (overlay graft still there) and ACKs [] ' +
      'without a fabricated route',
      doc !== undefined && doc['state'] === 'applied' && doc['mode'] === 'recreated' &&
      JSON.stringify(doc['routes']) === JSON.stringify([]) &&
      count('__NB_CREATE_CALLS') === 1 && count('__NB_TUN_FEED_CALLS') === 1 &&
      JSON.stringify(ack['payload']) === JSON.stringify({ routes: [] }),
      JSON.stringify({ doc: doc, ack: ack }));
    // the initial leg: an AVAILABLE map with an EMPTY desired set is ACKed as []
    G['__NB_ACK_CALLS'] = 0;
    G['__NB_ACK'] = undefined;
    const initialAck = makeAbility(mod);
    initialAck['pendingAppliedRoutes'] = [];
    (initialAck['ackAppliedRoutesInitial'] as () => void)();
    const firstAck = (G['__NB_ACK'] ?? {}) as Record<string, unknown>;
    check('L3b the initial leg ACKs an AVAILABLE empty desired set as [] with kind initial ' +
      '(instead of skipping it)',
      count('__NB_ACK_CALLS') === 1 && firstAck['isRecreate'] === false &&
      firstAck['kind'] === 'initial' &&
      JSON.stringify(firstAck['payload']) === JSON.stringify({ routes: [] }),
      JSON.stringify(firstAck));
    // the initial leg: NO core snapshot at all is still skipped, never faked
    G['__NB_ACK_CALLS'] = 0;
    G['__NB_ACK'] = undefined;
    const noSnapshot = makeAbility(mod);
    noSnapshot['pendingAppliedRoutes'] = null;
    (noSnapshot['ackAppliedRoutesInitial'] as () => void)();
    check('L3c a MISSING core snapshot is still skipped (no fabricated [] applied set)',
      count('__NB_ACK_CALLS') === 0 &&
      linesWith('VPN_ROUTE_SET_ACK_SKIPPED|').length === 1);
  }

  // ---- L4: native proves "desired == applied" -> NO OS change ----------------
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  answer(setterAnswer(true, true, DESIRED, false));
  writeRequestDoc('ui-route-noop');
  {
    const ability = settledWorld(mod, createConnection);
    await applyAndSettle(ability);
    const doc = result();
    check('L4 the native proof "the applied set already equals the desired set" answers ' +
      'applied / no-os-change with NO destroy, create, feed or ACK',
      doc !== undefined && doc['state'] === 'applied' && doc['mode'] === 'no-os-change' &&
      JSON.stringify(doc['routes']) === JSON.stringify(DESIRED) &&
      resultIsApplied(doc) === true && count('__NB_CREATE_CALLS') === 0 &&
      count('__NB_DESTROY_CALLS') === 0 && count('__NB_TUN_FEED_CALLS') === 0 &&
      count('__NB_ACK_CALLS') === 0,
      JSON.stringify({ doc: doc, create: count('__NB_CREATE_CALLS'),
        destroy: count('__NB_DESTROY_CALLS') }));
  }

  // ---- L5: the busy latch, the queue and the LATEST preference document -----
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  answer(setterAnswer(true, true, DESIRED, true));
  writeRequestDoc('ui-route-A');
  holdCreate = true;
  {
    const ability = settledWorld(mod, createConnection);
    poll(ability);
    await waitParked();
    const callsWhileBusy: number = count('__NB_SELECTION_SET_CALLS');
    poll(ability); // a second 100 ms tick while the apply really is in flight
    await delay(30);
    check('L5a the busy latch serializes overlapping ticks (an in-flight apply is never ' +
      'entered twice)',
      count('__NB_SELECTION_SET_CALLS') === callsWhileBusy &&
      ability['routeRequestBusy'] === true && count('__NB_CREATE_CALLS') === 1,
      JSON.stringify({ whileBusy: callsWhileBusy, now: count('__NB_SELECTION_SET_CALLS') }));
    // the page saves a NEWER selection and rings again while the recreate runs
    seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OTHER));
    writeRequestDoc('ui-route-B');
    releaseParked();
    await waitIdle(ability);
    const first = result();
    check('L5b the first apply still answers for its OWN id',
      first !== undefined && first['id'] === 'ui-route-A' &&
      first['state'] === 'applied' && first['mode'] === 'recreated',
      JSON.stringify(first));
    holdCreate = false;
    await applyAndSettle(ability);
    const second = result();
    check('L5c the NEWEST queued id is processed after the current recreate, and the LATEST ' +
      'preference document is what the native setter receives',
      second !== undefined && second['id'] === 'ui-route-B' &&
      second['state'] === 'applied' && second['mode'] === 'recreated' &&
      JSON.stringify(G['__NB_SELECTION_SET_ARGS']) === JSON.stringify(SELECTION_OTHER) &&
      count('__NB_CREATE_CALLS') === 2,
      JSON.stringify({ second: second, selection: G['__NB_SELECTION_SET_ARGS'] }));
  }

  // ---- L6: duplicate / foreign scope / untrustworthy documents ---------------
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  answer(setterAnswer(true, true, DESIRED, true));
  writeRequestDoc('ui-route-dup');
  {
    const ability = settledWorld(mod, createConnection);
    await applyAndSettle(ability);
    const answerBytes = fileOf(RESULT_PATH);
    const acksAfterFirst = count('__NB_ACK_CALLS');
    const createsAfterFirst = count('__NB_CREATE_CALLS');
    await applyAndSettle(ability); // the retained mailbox is re-read: SAME id
    check('L6a a duplicate (the retained request still names a consumed id) is a no-op: ' +
      'no second recreate and no second answer document',
      fileOf(RESULT_PATH) === answerBytes &&
      count('__NB_ACK_CALLS') === acksAfterFirst &&
      count('__NB_CREATE_CALLS') === createsAfterFirst &&
      answerBytes !== undefined,
      JSON.stringify({ same: fileOf(RESULT_PATH) === answerBytes,
        acks: count('__NB_ACK_CALLS') }));
  }

  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  answer(setterAnswer(true, true, DESIRED, true));
  writeRequestDoc('ui-route-scope', MGMT_OTHER);
  {
    const ability = settledWorld(mod, createConnection);
    await applyAndSettle(ability);
    const doc = result();
    check('L6b a request naming ANOTHER server scope is answered honestly as failed / ' +
      'scope-mismatch, with no setter call and no recreate',
      doc !== undefined && doc['state'] === 'failed' && doc['error'] === 'scope-mismatch' &&
      resultIsApplied(doc) === false && count('__NB_SELECTION_SET_CALLS') === 0 &&
      count('__NB_CREATE_CALLS') === 0 && count('__NB_STOP_CALLS') === 0,
      JSON.stringify(doc));
  }

  resetWorld();
  seedFile(SELECTION_PATH,
    '{"v":1,"management_url":"http://mgmt.test:33073","selection":{"deselect_all":"nope"}}');
  answer(setterAnswer(true, true, DESIRED, true));
  writeRequestDoc('ui-route-badprefs');
  {
    const ability = settledWorld(mod, createConnection);
    const before = fileOf(SELECTION_PATH);
    await applyAndSettle(ability);
    const doc = result();
    check('L6c an untrustworthy preference document fails the apply with its stable class ' +
      'token, leaves the document byte-identical and the running VPN untouched',
      doc !== undefined && doc['state'] === 'failed' &&
      doc['error'] === 'route-selection-malformed' && resultIsApplied(doc) === false &&
      count('__NB_SELECTION_SET_CALLS') === 0 && count('__NB_CREATE_CALLS') === 0 &&
      count('__NB_DESTROY_CALLS') === 0 && count('__NB_STOP_CALLS') === 0 &&
      count('__NB_START_CALLS') === 0 && fileOf(SELECTION_PATH) === before,
      JSON.stringify({ doc: doc, same: fileOf(SELECTION_PATH) === before }));
  }

  resetWorld();
  seedFile(REQUEST_PATH, '{"v":1,"id":"ui-route-x"'); // truncated: unreadable
  answer(setterAnswer(true, true, DESIRED, true));
  {
    const ability = settledWorld(mod, createConnection);
    await applyAndSettle(ability);
    await applyAndSettle(ability);
    check('L6d an unreadable request is never answered under a guessed id, is never unlinked, ' +
      'and its class is logged once (not once per tick)',
      fileOf(RESULT_PATH) === undefined &&
      fileOf(REQUEST_PATH) === '{"v":1,"id":"ui-route-x"' &&
      count('__NB_SELECTION_SET_CALLS') === 0 &&
      linesWith('VPN_ROUTE_APPLY_READ_FAIL|').length === 1,
      JSON.stringify({ result: fileOf(RESULT_PATH), request: fileOf(REQUEST_PATH),
        lines: linesWith('VPN_ROUTE_APPLY_READ_FAIL|') }));
  }

  // ---- L7: a teardown latched inside each await never feeds or ACKs ---------
  const stopStages: Array<[string, () => void]> = [
    ['destroy', (): void => { flipOnDestroy = true; }],
    ['settle', (): void => { armProbeFlipAtDestroy = true; }],
    ['create', (): void => { flipOnCreate = true; }],
    ['verify', (): void => { armProbeFlipAtCreate = true; }]
  ];
  for (const [stage, arm] of stopStages) {
    resetWorld();
    seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
    answer(setterAnswer(true, true, DESIRED, true));
    writeRequestDoc(`ui-route-stop-${stage}`);
    arm();
    const ability = settledWorld(mod, createConnection);
    await applyAndSettle(ability);
    const doc = result();
    const reachedCreate: number = count('__NB_CREATE_CALLS');
    const freshDestroyed: boolean = G['__NB_NEW_CONN_DESTROYED'] === true;
    check(`L7(${stage}) a teardown latched inside the ${stage} await aborts BEFORE the TUN feed ` +
      'and the ACK, and — once a fresh generation exists — that generation is torn down ' +
      '(no orphan TUN, no late ACK)',
      count('__NB_TUN_FEED_CALLS') === 0 && count('__NB_ACK_CALLS') === 0 &&
      doc !== undefined && doc['state'] === 'failed' &&
      String(doc['error']).indexOf('recreate-') === 0 &&
      ((stage === 'create' || stage === 'verify')
        ? freshDestroyed === true && reachedCreate === 1
        : reachedCreate === 0),
      JSON.stringify({ doc: doc, create: reachedCreate, freshDestroyed: freshDestroyed,
        tunFeed: count('__NB_TUN_FEED_CALLS'), ack: count('__NB_ACK_CALLS') }));
  }

  // ---- L8: the answer document exists only AFTER the ACK ---------------------
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  answer(setterAnswer(true, true, DESIRED, true));
  writeRequestDoc('ui-route-ack-first');
  holdCreate = true;
  {
    const ability = settledWorld(mod, createConnection);
    poll(ability);
    await waitParked();
    check('L8a no answer document exists while the recreate is in flight',
      fileOf(RESULT_PATH) === undefined && fileOf(RESULT_TEMP_PATH) === undefined);
    releaseParked();
    await waitIdle(ability);
    const doc = result();
    check('L8b the ACK was accepted BEFORE the answer document was written, and only then ' +
      'does the answer claim applied',
      G['__NB_RESULT_AT_ACK'] === false && doc !== undefined &&
      doc['state'] === 'applied' && doc['mode'] === 'recreated' &&
      doc['id'] === 'ui-route-ack-first',
      JSON.stringify({ atAck: G['__NB_RESULT_AT_ACK'], doc: doc }));
  }

  // ---- L9: the pre-existing automatic caller keeps kind 'auto' ---------------
  resetWorld();
  {
    const ability = settledWorld(mod, createConnection);
    const outcome: string = await (ability['performControlledRecreate'] as
      (reason: string) => Promise<string>)('desired-route-set-change');
    const ack = (G['__NB_ACK'] ?? {}) as Record<string, unknown>;
    check('L9 the automatic caller still ACKs isRecreate=true / kind=auto and the recreate ' +
      'reports its real outcome token',
      ack['isRecreate'] === true && ack['kind'] === 'auto' && outcome === 'ok',
      JSON.stringify({ ack: ack, outcome: outcome }));
  }

  // ---- L10: the EXISTING 100 ms supply tick carries the mailbox --------------
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  answer(setterAnswer(true, true, DESIRED, true));
  writeRequestDoc('ui-route-timer');
  {
    const ability = settledWorld(mod, createConnection);
    (ability['startSocketSupply'] as () => void)();
    const deadline: number = Date.now() + 8000;
    while (Date.now() < deadline && fileOf(RESULT_PATH) === undefined) {
      await delay(10);
    }
    const doc = result();
    check('L10a the page mailbox is picked up by the EXISTING 100 ms socket-supply tick ' +
      '(no new timer), which keeps serving the native socket poll',
      doc !== undefined && doc['id'] === 'ui-route-timer' && doc['state'] === 'applied' &&
      count('__NB_SUPPLY_POLL') > 0,
      JSON.stringify({ doc: doc, supplyPolls: count('__NB_SUPPLY_POLL') }));
    check('L10b the supply interval is STILL running after an apply (only teardown stops it)',
      ability['socketSupplyTimer'] !== undefined && ability['socketSupplyTimer'] !== -1,
      String(ability['socketSupplyTimer']));
    (ability['stopSocketSupply'] as () => void)();
    check('L10c and it is stopped by the pre-existing teardown',
      ability['socketSupplyTimer'] === -1);
  }

  // ---- L11: the remaining native answers -------------------------------------
  // (a) available:false — the selection was STORED, no map reached the OS.
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  answer({ ok: true, available: false, changed: false, desired_routes: [],
    recreate_needed: false });
  writeRequestDoc('ui-route-nomap');
  {
    const ability = settledWorld(mod, createConnection);
    await applyAndSettle(ability);
    const doc = result();
    check('L11a an answer without an applied map is written as stored-no-map, and the ONE ' +
      'predicate refuses to read it as applied (the page words it 下次连接, never 已生效)',
      doc !== undefined && doc['state'] === 'applied' && doc['mode'] === 'stored-no-map' &&
      resultIsApplied(doc) === false && count('__NB_SELECTION_SET_CALLS') === 1 &&
      count('__NB_CREATE_CALLS') === 0 && count('__NB_DESTROY_CALLS') === 0 &&
      count('__NB_ACK_CALLS') === 0,
      JSON.stringify({ doc: doc, applied: resultIsApplied(doc ?? {}) }));
  }
  // (b) the native refused WITHOUT mutating (selection-invalid / no-connector).
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  answer({ ok: false, error: 'selection-invalid', available: false, changed: false,
    desired_routes: [], recreate_needed: false });
  writeRequestDoc('ui-route-refused');
  {
    const ability = settledWorld(mod, createConnection);
    await applyAndSettle(ability);
    const doc = result();
    check('L11b a native refusal is answered as failed with its OWN error token, and the ' +
      'running connection is untouched',
      doc !== undefined && doc['state'] === 'failed' &&
      doc['error'] === 'selection-invalid' && resultIsApplied(doc) === false &&
      count('__NB_CREATE_CALLS') === 0 && count('__NB_DESTROY_CALLS') === 0 &&
      count('__NB_STOP_CALLS') === 0 && count('__NB_ACK_CALLS') === 0,
      JSON.stringify(doc));
  }
  // (c) an unreadable native answer (a malformed document is the bridge's null).
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  G['__NB_SELECTION_SET_ANSWER'] = '{not-json';
  writeRequestDoc('ui-route-unreadable');
  {
    const ability = settledWorld(mod, createConnection);
    await applyAndSettle(ability);
    const doc = result();
    check('L11c an unreadable native answer never claims applied and rebuilds nothing',
      doc !== undefined && doc['state'] === 'failed' &&
      doc['error'] === 'selection-set-unreadable' && resultIsApplied(doc) === false &&
      count('__NB_CREATE_CALLS') === 0 && count('__NB_DESTROY_CALLS') === 0,
      JSON.stringify(doc));
  }
  // (d) wg-apply-failed: the mutation DID happen and the data plane refused it —
  //     the EXISTING fail-closed teardown runs.
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  answer({ ok: false, error: 'wg-apply-failed', available: true, changed: true,
    desired_routes: DESIRED, recreate_needed: true });
  writeRequestDoc('ui-route-wgfail');
  {
    const ability = settledWorld(mod, createConnection);
    await applyAndSettle(ability);
    const doc = result();
    check('L11d a WG-seam refusal after the mutation runs the EXISTING fail-closed teardown ' +
      '(connector stopped, generation destroyed) and answers failed',
      doc !== undefined && doc['state'] === 'failed' && doc['error'] === 'wg-apply-failed' &&
      count('__NB_STOP_CALLS') === 1 && count('__NB_DESTROY_CALLS') === 1 &&
      count('__NB_CREATE_CALLS') === 0 && count('__NB_ACK_CALLS') === 0 &&
      ability['destroyPromise'] !== undefined,
      JSON.stringify({ doc: doc, stop: count('__NB_STOP_CALLS'),
        destroy: count('__NB_DESTROY_CALLS') }));
  }

  // ---- L12: the defer rule and the all-on default ----------------------------
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  answer(setterAnswer(true, true, DESIRED, true));
  writeRequestDoc('ui-route-defer');
  {
    const ability = settledWorld(mod, createConnection);
    ability['recreateInFlight'] = true; // an automatic / LAN recreate is running
    await applyAndSettle(ability);
    check('L12a a request that arrives while a recreate is in flight is left UNCONSUMED and ' +
      'unanswered (no setter call, no guessed answer)',
      fileOf(RESULT_PATH) === undefined && count('__NB_SELECTION_SET_CALLS') === 0 &&
      ability['lastRouteApplyId'] === '');
    ability['recreateInFlight'] = false; // the automatic recreate finished
    await applyAndSettle(ability);
    const doc = result();
    check('L12b and the same retained request IS applied by a later tick (the newest queued id ' +
      'after the current recreate)',
      doc !== undefined && doc['id'] === 'ui-route-defer' && doc['state'] === 'applied' &&
      doc['mode'] === 'recreated' && count('__NB_CREATE_CALLS') === 1,
      JSON.stringify(doc));
  }
  resetWorld();
  answer(setterAnswer(true, true, DESIRED, false));
  writeRequestDoc('ui-route-default');
  {
    const ability = settledWorld(mod, createConnection);
    await applyAndSettle(ability);
    const doc = result();
    check('L12c an ABSENT preference document means the pre-existing all-on default reaches ' +
      'the native setter',
      JSON.stringify(G['__NB_SELECTION_SET_ARGS']) ===
        JSON.stringify({ selected_routes: [], deselected_routes: [], deselect_all: false }) &&
      doc !== undefined && doc['state'] === 'applied' && doc['mode'] === 'no-os-change',
      JSON.stringify({ selection: G['__NB_SELECTION_SET_ARGS'], doc: doc }));
  }
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  answer(setterAnswer(true, true, DESIRED, true));
  writeRequestDoc('ui-route-teardown');
  {
    const ability = settledWorld(mod, createConnection);
    ability['destroyRequested'] = true; // teardown latched
    await applyAndSettle(ability);
    check('L12d a teardown-latched instance attempts nothing and answers nothing',
      fileOf(RESULT_PATH) === undefined && count('__NB_SELECTION_SET_CALLS') === 0 &&
      count('__NB_CREATE_CALLS') === 0);
  }

  // ---- SR: the startup race -------------------------------------------------
  // The generation may only be declared settled — and so only become
  // rebuildable — once the initial leg really completed: the ONE protected
  // socket (the WG outer socket, which since stage 3 also carries the ICE mux),
  // the WG feed, the mux arm and the initial route-set ACK. Parked at that ONE
  // protect, a mailbox tick arriving in the window must consume NOTHING and
  // rebuild NOTHING.
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  // The desired set is the one the initial ACK will record as applied, so the
  // retained request needs no OS change once the leg is settled.
  answer(setterAnswer(true, true, DESIRED, false));
  writeRequestDoc('ui-route-sr-parked');
  parkProtectAt(1);
  {
    const ability = settlingWorld(mod, createConnection, DESIRED);
    const settle = startInitialSettle(ability);
    await waitProtectParked();
    const parked = parkedProtect();
    await applyAndSettle(ability);
    check('SR1 the initial leg is really parked at the ONE protect that gates both ' +
      'planes (nothing fed yet, no mux armed, the generation not settled)',
      parked !== null && parked['fd'] === 61 && ability['outboundProtected'] === false &&
      ability['tunFd'] === 42 && ability['createSettled'] === false &&
      count('__NB_SUPPLY_WG_FEED') === 0 && count('__NB_SUPPLY_ICE_MUX_ARM') === 0,
      JSON.stringify({ parked: parked === null ? null : parked['fd'],
        outboundProtected: ability['outboundProtected'], tunFd: ability['tunFd'],
        wgFeed: count('__NB_SUPPLY_WG_FEED'), muxArm: count('__NB_SUPPLY_ICE_MUX_ARM'),
        createSettled: ability['createSettled'] }));
    check('SR2 a mailbox tick inside that window consumes NOTHING and opens no ' +
      'rebuildable state: no setter call, no answer document, no recreate, no ' +
      'destroy, no feed, and the rebuildable gate is still shut',
      ability['lastRouteApplyId'] === '' && rebuildableGate(ability) === false &&
      count('__NB_SELECTION_SET_CALLS') === 0 && fileOf(RESULT_PATH) === undefined &&
      count('__NB_CREATE_CALLS') === 0 && count('__NB_DESTROY_CALLS') === 0 &&
      count('__NB_SUPPLY_WG_FEED') === 0 && count('__NB_SUPPLY_ICE_MUX_ARM') === 0,
      JSON.stringify({ consumed: ability['lastRouteApplyId'],
        setter: count('__NB_SELECTION_SET_CALLS'), result: fileOf(RESULT_PATH),
        create: count('__NB_CREATE_CALLS'), destroy: count('__NB_DESTROY_CALLS'),
        wgFeed: count('__NB_SUPPLY_WG_FEED'), muxArm: count('__NB_SUPPLY_ICE_MUX_ARM') }));

    releaseParkedProtect();
    const outcome = (await settle) as Record<string, unknown>;
    const initialAck = (G['__NB_ACK'] ?? {}) as Record<string, unknown>;
    check('SR3 releasing the ONE protect lets the REAL initial leg finish — fd ' +
      'contract accepted, WG fed, the mux armed OVER that very socket (no second ' +
      'open / bind / protect for ICE), the initial ACK out as kind initial — and ' +
      'only NOW is the generation settled',
      outcome['accepted'] === true && ability['createSettled'] === true &&
      rebuildableGate(ability) === true && count('__NB_ACK_CALLS') === 1 &&
      initialAck['isRecreate'] === false && initialAck['kind'] === 'initial' &&
      JSON.stringify(initialAck['payload']) === JSON.stringify({ routes: DESIRED }) &&
      count('__NB_SUPPLY_WG_FEED') === 1 && count('__NB_SUPPLY_ICE_MUX_ARM') === 1 &&
      count('__NB_SUPPLY_ICE_OPEN') === 0 && count('__NB_SUPPLY_ICE_FEED') === 0,
      JSON.stringify({ outcome: outcome, createSettled: ability['createSettled'],
        wgFeed: count('__NB_SUPPLY_WG_FEED'), muxArm: count('__NB_SUPPLY_ICE_MUX_ARM'),
        iceOpen: count('__NB_SUPPLY_ICE_OPEN'), iceFeed: count('__NB_SUPPLY_ICE_FEED'),
        ack: initialAck }));
    await applyAndSettle(ability);
    const doc = result();
    const bytes = fileOf(RESULT_PATH);
    check('SR4 the SAME retained request is then applied EXACTLY ONCE as applied / ' +
      'no-os-change (its desired set is the one the initial ACK just recorded)',
      doc !== undefined && doc['id'] === 'ui-route-sr-parked' && doc['state'] === 'applied' &&
      doc['mode'] === 'no-os-change' && resultIsApplied(doc) === true &&
      JSON.stringify(doc['routes']) === JSON.stringify(DESIRED) &&
      ability['lastRouteApplyId'] === 'ui-route-sr-parked' &&
      count('__NB_SELECTION_SET_CALLS') === 1 && count('__NB_ACK_CALLS') === 1 &&
      count('__NB_CREATE_CALLS') === 0 && count('__NB_DESTROY_CALLS') === 0,
      JSON.stringify({ doc: doc, setter: count('__NB_SELECTION_SET_CALLS') }));
    await applyAndSettle(ability);
    check('SR5 the same id is never processed twice (no second setter call, no second ' +
      'answer, no recreate)',
      count('__NB_SELECTION_SET_CALLS') === 1 && fileOf(RESULT_PATH) === bytes &&
      count('__NB_CREATE_CALLS') === 0 && count('__NB_ACK_CALLS') === 1,
      JSON.stringify({ setter: count('__NB_SELECTION_SET_CALLS'),
        rewritten: fileOf(RESULT_PATH) !== bytes }));
    (ability['stopStatusAndNetcfgWatcher'] as () => void)();
  }

  // (a) the user stops inside the ONE protect await: the leg fails closed — the
  //     socket never opens, no feed, no mux arm, no ACK, the generation is
  //     destroyed — and its late completion declares nothing settled.
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  answer(setterAnswer(true, true, DESIRED, false));
  writeRequestDoc('ui-route-sr-stop');
  parkProtectAt(1);
  {
    const ability = settlingWorld(mod, createConnection, DESIRED);
    const settle = startInitialSettle(ability);
    await waitProtectParked();
    ability['destroyRequested'] = true;
    releaseParkedProtect();
    const outcome = (await settle) as Record<string, unknown>;
    await delay(30); // let the fire-and-forget fail-closed teardown land
    check('SR6 a user stop latched inside the protect await is fail-closed AND never ' +
      'settles: no initial ACK, nothing fed, the generation destroyed, the ' +
      'rebuildable gate shut',
      outcome['accepted'] === true && outcome['reason'] === 'protect-fail-closed' &&
      count('__NB_ACK_CALLS') === 0 && count('__NB_DESTROY_CALLS') === 1 &&
      count('__NB_SUPPLY_WG_FEED') === 0 && count('__NB_SUPPLY_ICE_MUX_ARM') === 0 &&
      ability['createSettled'] === false && rebuildableGate(ability) === false,
      JSON.stringify({ outcome: outcome, ack: count('__NB_ACK_CALLS'),
        destroy: count('__NB_DESTROY_CALLS'), createSettled: ability['createSettled'],
        gate: rebuildableGate(ability) }));
    await applyAndSettle(ability);
    check('SR7 ... and its retained request is never consumed and never answered',
      fileOf(RESULT_PATH) === undefined && count('__NB_SELECTION_SET_CALLS') === 0 &&
      ability['lastRouteApplyId'] === '');
    (ability['stopStatusAndNetcfgWatcher'] as () => void)();
  }

  // (b) the connector dies while the ONE protect is still pending: because
  //     connectorDeathHandled is NOT one of the feed guards, the leg really does
  //     run to its end (the WG socket is fed, the mux armed over it, the initial
  //     ACK goes out) — the LATE completion is what must still refuse to settle.
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  answer(setterAnswer(true, true, DESIRED, false));
  writeRequestDoc('ui-route-sr-death');
  parkProtectAt(1);
  {
    const ability = settlingWorld(mod, createConnection, DESIRED);
    const settle = startInitialSettle(ability);
    await waitProtectParked();
    ability['connectorDeathHandled'] = true;
    releaseParkedProtect();
    const outcome = (await settle) as Record<string, unknown>;
    await delay(30);
    check('SR8 a connector death latched inside the protect await: the leg still runs ' +
      'to its end (WG fed + mux armed + initial ACK) but the late completion does ' +
      'NOT open the rebuildable gate',
      outcome['accepted'] === true && count('__NB_ACK_CALLS') === 1 &&
      count('__NB_DESTROY_CALLS') === 0 && count('__NB_SUPPLY_WG_FEED') === 1 &&
      count('__NB_SUPPLY_ICE_MUX_ARM') === 1 &&
      ability['createSettled'] === false && rebuildableGate(ability) === false,
      JSON.stringify({ outcome: outcome, ack: count('__NB_ACK_CALLS'),
        wgFeed: count('__NB_SUPPLY_WG_FEED'), muxArm: count('__NB_SUPPLY_ICE_MUX_ARM'),
        createSettled: ability['createSettled'], gate: rebuildableGate(ability) }));
    await applyAndSettle(ability);
    check('SR9 ... and its retained request is never consumed and never answered',
      fileOf(RESULT_PATH) === undefined && count('__NB_SELECTION_SET_CALLS') === 0 &&
      ability['lastRouteApplyId'] === '');
    (ability['stopStatusAndNetcfgWatcher'] as () => void)();
  }

  // (c) the fail-closed gate itself: the outbound socket never opens, so the leg
  //     dies before any feed. The fd was already published (tunFd = 42 >= 0) and
  //     the connection exists, so the individual flags prove nothing — the
  //     COMPOSITE rebuildable gate is what stays shut (the same reason the
  //     create-REJECTED terminal, createSettled = true with tunFd = -1, is only
  //     safe as part of that composite).
  resetWorld();
  seedFile(SELECTION_PATH, serializeSelection(MGMT, SELECTION_OFF));
  answer(setterAnswer(true, true, DESIRED, false));
  writeRequestDoc('ui-route-sr-nofeed');
  G['__NB_WG_FD'] = -1; // core.wg_fwd_open answers "no socket"
  {
    const ability = settlingWorld(mod, createConnection, DESIRED);
    const settle = startInitialSettle(ability);
    const outcome = (await settle) as Record<string, unknown>;
    await delay(30);
    check('SR10 an initial leg that never fed anything neither ACKs nor settles, even ' +
      'though tunFd is already published at 42',
      outcome['destroyRequired'] === true && outcome['accepted'] === true &&
      outcome['reason'] === 'protect-fail-closed' && count('__NB_ACK_CALLS') === 0 &&
      count('__NB_DESTROY_CALLS') === 1 && ability['tunFd'] === 42 &&
      ability['createSettled'] === false && rebuildableGate(ability) === false,
      JSON.stringify({ outcome: outcome, tunFd: ability['tunFd'],
        createSettled: ability['createSettled'], gate: rebuildableGate(ability) }));
    (ability['stopStatusAndNetcfgWatcher'] as () => void)();
  }

  // ---- P: source pins on the byte-identical extension ------------------------
  const consumer = ((): string => {
    const at: number = extSource.indexOf('private maybePollRouteApplyRequest(): void {');
    const stop: number = extSource.indexOf('private async feedSpareProtectedSocket(');
    return at < 0 || stop < 0 ? '' : extSource.slice(at, stop);
  })();
  check('P1 the mailbox rides the ONE existing 100 ms tick: exactly one call site, inside ' +
    'startSocketSupply, and the consumer adds no timer of its own',
    extSource.split('this.maybePollRouteApplyRequest();').length === 2 &&
    extSource.indexOf('this.maybePollRouteApplyRequest();') >
      extSource.indexOf('this.socketSupplyTimer = setInterval(') &&
    consumer.length > 0 && consumer.indexOf('setInterval(') < 0 &&
    consumer.indexOf('setTimeout(') < 0 && consumer.indexOf('commonEvent') < 0 &&
    consumer.indexOf('startVpnExtensionAbility(') < 0);
  const guardAt: number = consumer.indexOf(
    'if (this.destroyRequested || this.connectorDeathHandled || this.recreateInFlight ||');
  const setterAt: number = consumer.indexOf('connectorRouteSelectionSet(desired)');
  const recreateAt: number = consumer.indexOf('await this.performControlledRecreate(');
  check('P2 the decision turn is synchronous: the guard re-check, the native setter and the ' +
    'recreate call follow each other with NO await in between',
    guardAt > 0 && guardAt < setterAt && setterAt < recreateAt &&
    consumer.slice(guardAt, setterAt).indexOf('await') < 0 &&
    consumer.slice(guardAt, setterAt).indexOf('loadRouteSelection(') < 0,
    `${guardAt}/${setterAt}/${recreateAt}`);
  check('P3 only the user path passes the second argument, and the three pre-existing ' +
    'automatic call sites are unchanged',
    extSource.split('performControlledRecreate(').length === 6 &&
    extSource.indexOf('this.performControlledRecreate(decision.reason).then(') > 0 &&
    extSource.indexOf('this.performControlledRecreate(status.recreate.reason).catch(') > 0 &&
    extSource.indexOf('this.performControlledRecreate(`vpn-iface-lost:${source}`).catch(') > 0 &&
    extSource.indexOf("`route-apply:${requestId}`, 'user', acked);") > 0);
  check('P4 the ACK kind is explicit at BOTH legs (initial / the recreate leg\'s ackKind)',
    extSource.includes("connectorRouteSetApplied(routes, false, 'initial')") &&
    extSource.includes('connectorRouteSetApplied(routeNetworks, true, ackKind)'));
  check('P5 the two NEW teardown re-checks sit after the create and after the interface ' +
    'verification (four in total, the two pre-existing ones unchanged)',
    extSource.split("throw new Error('recreate-interrupted-by-teardown');").length === 5 &&
    extSource.indexOf("throw new Error('recreate-interrupted-by-teardown');",
      extSource.indexOf('await this.createConnectionBounded(conn, config);')) > 0 &&
    extSource.indexOf("throw new Error('recreate-interrupted-by-teardown');",
      extSource.indexOf('await this.verifyRecreatedIface(attempt, RECREATE_IFACE_MAX_ATTEMPTS);')) > 0);
  check('P6 the consumer never touches the supply interval, and that interval is still ' +
    'stopped from exactly the two pre-existing teardown sites',
    extSource.split('this.stopSocketSupply();').length === 3 &&
    consumer.indexOf('stopSocketSupply') < 0 && consumer.indexOf('clearInterval') < 0 &&
    consumer.indexOf('socketSupplyShutdown') < 0);
  check('P7 the consumer reads and writes the two IPC documents ONLY through the store ' +
    'adapter (no direct fs)',
    consumer.indexOf('readRouteApplyRequest(') > 0 &&
    consumer.indexOf('writeRouteApplyResult(') > 0 &&
    consumer.indexOf('fs.') < 0 && consumer.indexOf('readTextBounded') < 0);
  check('P8 every answer uses the contract tokens, and the consumer never writes an ' +
    '"applying" marker or a locally re-spelled state literal',
    consumer.indexOf('ROUTE_APPLY_STATE_APPLIED') > 0 &&
    consumer.indexOf('ROUTE_APPLY_STATE_FAILED') > 0 &&
    consumer.indexOf('ROUTE_APPLY_MODE_STORED_NO_MAP') > 0 &&
    consumer.indexOf('ROUTE_APPLY_MODE_RECREATED') > 0 &&
    consumer.indexOf('ROUTE_APPLY_MODE_NO_OS_CHANGE') > 0 &&
    consumer.indexOf('ROUTE_APPLY_STATE_APPLYING') < 0 &&
    consumer.indexOf("'applied'") < 0 && consumer.indexOf("'failed'") < 0);
  check('P9 the four native answers each have their own branch (unreadable / refusal / ' +
    'no-map / no-os-change) before the recreate branch',
    consumer.indexOf('if (setter === null)') > 0 &&
    consumer.indexOf('if (!setter.ok)') > 0 &&
    consumer.indexOf('if (!setter.available)') > 0 &&
    consumer.indexOf('if (!setter.recreate_needed)') > 0 &&
    consumer.indexOf('if (setter === null)') < consumer.indexOf('await this.performControlledRecreate(') &&
    consumer.indexOf('if (!setter.recreate_needed)') < consumer.indexOf('await this.performControlledRecreate('));
  check('P10 the initial-leg ACK keeps its empty-set rule: the stash no longer filters an ' +
    'empty desired set out, and the skip branch is the null snapshot only',
    extSource.indexOf('net !== null && net.routes !== undefined && net.routes !== null') > 0 &&
    extSource.indexOf('net.routes.length > 0') < 0 &&
    extSource.indexOf('if (routes === null) {') > 0);
} catch (error) {
  failed++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  if (fixtureDir.length > 0) {
    rmSync(fixtureDir, { recursive: true, force: true });
  }
}

console.log(`${passed} passed, ${failed} failed`);
// The driven legs arm real intervals; the fixture is done, so exit explicitly
// (same as the sibling end-to-end harnesses).
process.exit(failed > 0 ? 1 : 0);
