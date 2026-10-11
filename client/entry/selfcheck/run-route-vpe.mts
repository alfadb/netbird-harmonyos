/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Per-NetID route selection at VPN start + public route-catalog publication
// (2026-10-07, decision B) — host-side: no device, no network, no compiler.
//
// Same principle as run-route-apply-vpe.mts / run-exclusion-generation.mts: the REAL
// production files are copied byte-for-byte into an OS-temp fixture (.ts) and
// DRIVEN. Nothing under test is rewritten or mirrored:
//   NetBirdVpnExtensionAbility.ets  (the integration under test)
//   NetBirdConnector.ets            (the REAL buildConnectorConfigJson /
//                                    connectorRoutes bridge — not a stub)
//   NetBirdRouteStore.ets           (the REAL file adapter)
//   NetBirdRoutePreferences.ets     (the REAL pure document rules)
// The loader hook (exclusion-generation-hooks.mjs) answers only the @ohos
// platform seam; its @ohos.file.fs stub gained the ASYNC half of the same
// in-memory filesystem so the real adapter's open/read/write/fsync/close/
// rename/unlink path runs unchanged, and its libnetbird_core.so stub gained the
// FUNCTIONAL connector_routes getter. No route function is stubbed.
//
// Coverage (task mapping):
//   S  startup: the stored selection for the CONFIGURED management server is
//      loaded ONCE, before any protected socket / connector start, and the
//      REAL start config carries it as a JSON OBJECT (never a string).
//   S  missing document and a document scoped to ANOTHER server -> the third
//      argument is omitted and the start config is BYTE-IDENTICAL to the
//      pre-existing two-argument JSON.
//   S  an existing but untrustworthy document (malformed / wrong version /
//      unreadable) -> a CONTROLLED start failure with a stable class token:
//      the connector is never started, the document is never rewritten, and no
//      raw document text reaches a log.
//   C  catalog publication on the status tick: the REAL connector_routes
//      getter is mirrored through the REAL store — scope/session/stamp, the
//      disabled HA row retained verbatim, an empty-but-available catalog still
//      published, and a readable document round-tripped by the REAL pure
//      reader.
//   C  null / available:false / structurally malformed native documents publish
//      NOTHING: the last known offline catalog file is left untouched.
//   C  teardown / overlap guards: destroyRequested, connectorDeathHandled, a
//      dead watcher, an in-flight write and an empty management scope each
//      prevent the write; the in-flight latch is released on BOTH the resolve
//      and the reject path (a failed write never wedges publication).
//   P  source pins: ONE start call site, the load before it, exactly TWO
//      publish call sites (the status tick), no new timer, and no fabricated
//      "applied" field.
//
// Run: node client/entry/selfcheck/run-route-vpe.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./exclusion-generation-hooks.mjs', import.meta.url).href);

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_DIR = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability');
const REAL_ROUTES_DIR = join(HERE, '..', 'src', 'main', 'ets', 'routes');
const G = globalThis as unknown as Record<string, unknown>;

const FILES_DIR = '/tmp/nb-route-vpe-fixture';
const SELECTION_PATH = `${FILES_DIR}/netbird-route-selection.json`;
const CATALOG_PATH = `${FILES_DIR}/netbird-routes.json`;
const CATALOG_TEMP_PATH = `${CATALOG_PATH}.tmp`;
const NOTICE_PATH = `${FILES_DIR}/netbird-connect-failure`;
const DEVICE_PATH = `${FILES_DIR}/netbird-device-config.json`;
const SETUP_KEY_PATH = `${FILES_DIR}/netbird-setup-key`;
const MGMT = 'http://mgmt.test:33073';
const MGMT_OTHER = 'http://other.test:33073';
const REQUEST_ID = 'req-route-vpe';

// The byte-identical staged set (same table as run-route-apply-vpe.mts) plus the two
// route modules that live under ets/routes/.
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
const STAGED_ROUTES = ['NetBirdRouteStore', 'NetBirdRoutePreferences'];
const SHIM_TYPES: Record<string, string[]> = {
  NetBirdConnector: ['ConnectorStatusResult', 'CoreNetworkConfig', 'CoreRouteEntry',
    'ConnectorStartResult', 'ConnectorLogoutResult', 'DeviceConfigFile', 'ManagementEndpoint',
    'NativeTcpSocketOpen', 'RelayAdvertisedView', 'RouteSetAppliedResult',
    'SocketFeedResult', 'SocketSupplyRequest', 'TunReleaseResult', 'WgFeedResult',
    // STAGE 2 (official-alignment audit §2.7): the shared ICE mux feed types.
    'IceMuxOpen', 'IceMuxFeedResult',
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

// The initial network map the driven leg sees: available on the FIRST poll (no
// wait), one managed /24, no default route released, and NO signal uri — after
// the start call the real leg stops the connector and returns, which keeps the
// drive short while still exercising the whole pre-start gate + start call.
const NETCFG_NO_SIGNAL = JSON.stringify({
  available: true,
  serial: 5,
  address: '100.108.144.165',
  address_prefix_len: 16,
  routes: [{ network: '10.10.10.0/24', is_default: false }],
  dns: { service_enable: true, servers: [{ ip: '9.9.9.9', port: 53 }] },
  default_route: { allowed: false, reason: 'data-plane-not-ready' },
  stuns: []
});

const SELECTION_HOST_INTERNAL = {
  selected_routes: [],
  deselected_routes: ['HostInternal'],
  deselect_all: false
};

// The live HA shape from docs/route-audit.md: one NetID with two candidates and
// `enabled:false` (the user disabled it), plus one enabled plain subnet.
const HA_CATALOG = {
  available: true,
  serial: 302,
  selection: SELECTION_HOST_INTERNAL,
  networks: [
    { net_id: 'HostInternal', networks: ['100.108.0.0/16'], candidate_count: 2,
      enabled: false, is_exit: false },
    { net_id: 'corp', networks: ['10.10.10.0/24'], candidate_count: 1,
      enabled: true, is_exit: false }
  ]
};
const EMPTY_CATALOG = {
  available: true,
  serial: 0,
  selection: { selected_routes: [], deselected_routes: [], deselect_all: false },
  networks: []
};
const UNAVAILABLE_CATALOG = {
  available: false,
  serial: 0,
  selection: { selected_routes: [], deselected_routes: [], deselect_all: false },
  networks: []
};
const OLD_CATALOG_DOC = '{"v":1,"management_url":"http://old.test:33073","updated_at_ms":1,' +
  '"request_id":"req-old","catalog":{"available":true,"serial":1,' +
  '"selection":{"selected_routes":[],"deselected_routes":[],"deselect_all":false},' +
  '"networks":[]}}';

// The scenario tables (declared here, iterated inside the try below).
const untrustworthy: string[][] = [
  ['S4a malformed', 'route-selection-malformed', '{"v":1,"management_url":"'],
  ['S4b wrong version', 'route-selection-version',
    JSON.stringify({ v: 99, management_url: MGMT, selection: SELECTION_HOST_INTERNAL })],
  ['S4c wrong shape', 'route-selection-malformed',
    JSON.stringify({ v: 1, management_url: MGMT, selection: { deselect_all: 'nope' } })]
];
const unavailableAnswers: unknown[][] = [
  ['C3a available:false', UNAVAILABLE_CATALOG],
  ['C3b unparseable document', '{not-json'],
  ['C3c structurally malformed document',
    { available: true, serial: 1, selection: { selected_routes: [], deselected_routes: [],
      deselect_all: false }, networks: [{ net_id: 'x', networks: 7, candidate_count: 1,
      enabled: true, is_exit: false }] }]
];
const guardCases: unknown[][] = [
  ['C4a destroyRequested', (a: Record<string, unknown>): void => { a['destroyRequested'] = true; }],
  ['C4b connectorDeathHandled',
    (a: Record<string, unknown>): void => { a['connectorDeathHandled'] = true; }],
  ['C4c dead watcher', (a: Record<string, unknown>): void => { a['watcherTimer'] = -1; }],
  ['C4d write already in flight',
    (a: Record<string, unknown>): void => { a['routeCatalogPublishInFlight'] = true; }],
  ['C4e no management scope',
    (a: Record<string, unknown>): void => { a['managementUrl'] = ''; }]
];

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

function openErrors(): Record<string, unknown> {
  return (G['__NB_FS_OPEN_ERROR'] ?? {}) as Record<string, unknown>;
}

// One fresh world per scenario: empty in-memory fs + fresh core answers.
function resetWorld(): void {
  G['__NB_FS'] = {};
  G['__NB_FS_OPEN_ERROR'] = {};
  G['__NB_LOG'] = [];
  G['__NB_DNS'] = { 'mgmt.test': '203.0.113.7', 'other.test': '203.0.113.8' };
  G['__NB_NETCFG'] = NETCFG_NO_SIGNAL;
  G['__NB_STATUS'] = { relay: { urls: [] } };
  G['__NB_FD_STATUS'] = { open: true, errno: 0 };
  G['__NB_MGMT_FD'] = 21;
  G['__NB_CREATED_CONFIGS'] = [];
  G['__NB_START_LAST'] = undefined;
  G['__NB_START_CALLS'] = 0;
  G['__NB_SUPPLY_MGMT_OPEN'] = 0;
  G['__NB_ROUTES'] = undefined;
  G['__NB_NET_DEFAULT_ID'] = undefined;
  G['__NB_NET_APP_ID'] = undefined;
  G['__NB_NET_ALL_IDS'] = undefined;
  G['__NB_NET_CAPS_BY_ID'] = undefined;
  G['__NB_NET_PROPS_BY_ID'] = undefined;
  G['__NB_NET_THROW_CODES'] = undefined;
}

function logs(): string[] {
  return (G['__NB_LOG'] as string[]) ?? [];
}

function startConfig(): Record<string, unknown> | undefined {
  const last = G['__NB_START_LAST'] as Record<string, unknown> | undefined;
  if (last === undefined) { return undefined; }
  try {
    return JSON.parse(String(last['config'])) as Record<string, unknown>;
  } catch (error) {
    return undefined;
  }
}

function mgmtOpenCount(): number {
  return typeof G['__NB_SUPPLY_MGMT_OPEN'] === 'number' ? G['__NB_SUPPLY_MGMT_OPEN'] as number : 0;
}

function drain(): Promise<void> {
  return new Promise<void>((resolve: () => void): void => { setTimeout(resolve, 0); });
}

function makeAbility(mod: Record<string, unknown>, requestId: string = REQUEST_ID):
  Record<string, unknown> {
  const ability = new (mod['default'] as new () => Record<string, unknown>)();
  ability['requestId'] = requestId;
  ability['context'] = { filesDir: FILES_DIR };
  return ability;
}

// Seed the credentials the REAL start flow reads, then drive the REAL
// startConnectorThenCreate end-to-end. `managementUrl` is the device config's
// own URL (the scope the selection must be loaded against).
function seedCredentials(managementUrl: string): void {
  seedFile(SETUP_KEY_PATH, 'setup-key-body');
  seedFile(DEVICE_PATH, JSON.stringify({
    management_url: managementUrl,
    private_key: 'pk-body'
  }));
}

async function driveStart(ability: Record<string, unknown>,
  connection: Record<string, unknown>): Promise<void> {
  ability['connection'] = connection;
  await (ability['startConnectorThenCreate'] as
    (w: unknown, c: unknown) => Promise<void>)({ parameters: {} }, connection);
  await drain();
  await drain();
}

function selectionDocument(managementUrl: string,
  selection: Record<string, unknown>): string {
  return JSON.stringify({ v: 1, management_url: managementUrl, selection: selection });
}

function publish(ability: Record<string, unknown>): void {
  (ability['publishRouteCatalogForUi'] as () => void)();
}

try {
  fixtureDir = mkdtempSync(join(tmpdir(), 'nb-route-vpe-'));
  const dir = fixtureDir;
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
  const bridge = await import(pathToFileURL(join(dir, 'NetBirdConnector.ts')).href) as
    Record<string, unknown>;
  const prefs = await import(pathToFileURL(join(dir, 'NetBirdRoutePreferences.ts')).href) as
    Record<string, unknown>;
  const vpnNs = (await import('@ohos.net.vpnExtension') as Record<string, unknown>)['default'] as
    Record<string, unknown>;
  const FakeConnection = vpnNs['VpnConnection'] as new () => Record<string, unknown>;
  const extSource: string = readFileSync(join(REAL_DIR, 'NetBirdVpnExtensionAbility.ets'), 'utf8');

  const buildBaseline = bridge['buildConnectorConfigJson'] as
    (device: unknown, force: boolean) => string;
  const parseDevice = bridge['parseDeviceConfig'] as (text: string) => unknown;
  const parseCatalogDoc = prefs['parseRouteCatalogDocument'] as
    (text: string) => Record<string, unknown>;

  // ---- S1: the matching stored selection reaches the REAL start call --------
  resetWorld();
  seedCredentials(MGMT);
  seedFile(SELECTION_PATH, selectionDocument(MGMT, SELECTION_HOST_INTERNAL));
  {
    const ability = makeAbility(mod);
    await driveStart(ability, new FakeConnection());
    const config = startConfig();
    check('S1a the real start call happened and its config carries the stored selection object',
      config !== undefined &&
      JSON.stringify(config['route_selection']) === JSON.stringify(SELECTION_HOST_INTERNAL),
      JSON.stringify({ config: config, last: G['__NB_START_LAST'] }));
    check('S1b the selection is a real JSON OBJECT in the config, never a string-encoded document',
      config !== undefined && typeof config['route_selection'] === 'object' &&
      typeof config['route_selection'] !== 'string');
    check('S1c the rest of the start config is unchanged (management_url / private_key present)',
      config !== undefined && config['management_url'] === MGMT &&
      config['private_key'] === 'pk-body' && config['relay_enabled'] === true);
  }

  // ---- S2: missing document -> baseline start JSON (field omitted) ---------
  resetWorld();
  seedCredentials(MGMT);
  {
    const ability = makeAbility(mod);
    await driveStart(ability, new FakeConnection());
    const config = startConfig();
    const device = parseDevice(fileOf(DEVICE_PATH) ?? '');
    check('S2a a MISSING selection document starts with NO route_selection key',
      config !== undefined && !Object.prototype.hasOwnProperty.call(config, 'route_selection'),
      JSON.stringify(config));
    check('S2b that start config is BYTE-IDENTICAL to the pre-existing two-argument JSON',
      config !== undefined && JSON.stringify(config) === buildBaseline(device, false),
      JSON.stringify({ got: JSON.stringify(config), want: buildBaseline(device, false) }));
  }

  // ---- S3: a document scoped to ANOTHER server is not this server's ---------
  resetWorld();
  seedCredentials(MGMT);
  seedFile(SELECTION_PATH, selectionDocument(MGMT_OTHER, SELECTION_HOST_INTERNAL));
  {
    const ability = makeAbility(mod);
    await driveStart(ability, new FakeConnection());
    const config = startConfig();
    check('S3a a selection scoped to another server is ignored (baseline, field omitted)',
      config !== undefined && !Object.prototype.hasOwnProperty.call(config, 'route_selection'),
      JSON.stringify(config));
  }
  resetWorld();
  seedCredentials(MGMT_OTHER);
  seedFile(SELECTION_PATH, selectionDocument(MGMT, SELECTION_HOST_INTERNAL));
  {
    const ability = makeAbility(mod);
    await driveStart(ability, new FakeConnection());
    const config = startConfig();
    check('S3b the scope is the ACTUAL configured management url from the device config',
      config !== undefined && !Object.prototype.hasOwnProperty.call(config, 'route_selection') &&
      config['management_url'] === MGMT_OTHER,
      JSON.stringify(config));
  }

  // ---- S4: an untrustworthy document blocks the start (never all-on) -------
  for (const [label, token, doc] of untrustworthy) {
    resetWorld();
    seedCredentials(MGMT);
    seedFile(SELECTION_PATH, doc);
    const ability = makeAbility(mod);
    await driveStart(ability, new FakeConnection());
    const config = startConfig();
    const invalid = logs().filter((l: string): boolean =>
      l.indexOf('VPN_ROUTE_SELECTION_INVALID') >= 0);
    const notice = fileOf(NOTICE_PATH);
    check(`${label} document blocks the start with the stable class token`,
      config === undefined && mgmtOpenCount() === 0 && invalid.length === 1 &&
      invalid[0].indexOf(`errorClass=${token}`) >= 0 &&
      notice !== undefined && notice.indexOf(`"error_class":"${token}"`) >= 0,
      JSON.stringify({ config: config, invalid: invalid, notice: notice,
        mgmtOpen: mgmtOpenCount() }));
    check(`${label}: the untrustworthy document is NOT rewritten and no raw text is logged`,
      fileOf(SELECTION_PATH) === doc &&
      logs().every((l: string): boolean => l.indexOf(doc) < 0 && l.indexOf('{') < 0),
      JSON.stringify({ file: fileOf(SELECTION_PATH), logs: logs() }));
  }

  // ---- S5: an unreadable (IO) selection document is also a controlled stop --
  resetWorld();
  seedCredentials(MGMT);
  seedFile(SELECTION_PATH, selectionDocument(MGMT, SELECTION_HOST_INTERNAL));
  openErrors()[SELECTION_PATH] = 13900003;
  {
    const ability = makeAbility(mod);
    await driveStart(ability, new FakeConnection());
    const invalid = logs().filter((l: string): boolean =>
      l.indexOf('VPN_ROUTE_SELECTION_INVALID') >= 0);
    check('S5 an unreadable selection document fails the start as unavailable, never all-on',
      startConfig() === undefined && mgmtOpenCount() === 0 && invalid.length === 1 &&
      invalid[0].indexOf('errorClass=route-selection-unavailable') >= 0,
      JSON.stringify({ invalid: invalid }));
  }

  // ---- C1: the status tick publishes the real catalog document -------------
  resetWorld();
  {
    const t0: number = Date.now();
    const ability = makeAbility(mod);
    ability['managementUrl'] = MGMT;
    ability['watcherTimer'] = 4242;
    G['__NB_ROUTES'] = HA_CATALOG;
    publish(ability);
    await drain();
    await drain();
    const text = fileOf(CATALOG_PATH);
    let doc: Record<string, unknown> | undefined = undefined;
    try { doc = text === undefined ? undefined : JSON.parse(text) as Record<string, unknown>; }
    catch (error) { doc = undefined; }
    const catalog = doc === undefined ? undefined : doc['catalog'] as Record<string, unknown>;
    const rows = catalog === undefined ? [] : (catalog['networks'] ?? []) as
      Array<Record<string, unknown>>;
    const ha = rows.find((r: Record<string, unknown>): boolean =>
      r['net_id'] === 'HostInternal');
    check('C1a the catalog document is written with the real scope/session/stamp',
      doc !== undefined && doc['v'] === 1 && doc['management_url'] === MGMT &&
      doc['request_id'] === REQUEST_ID && Number.isInteger(doc['updated_at_ms']) &&
      (doc['updated_at_ms'] as number) >= t0 && (doc['updated_at_ms'] as number) <= Date.now(),
      JSON.stringify({ doc: doc }));
    check('C1b the disabled HA row is retained verbatim (enabled:false, both candidates)',
      catalog !== undefined && catalog['available'] === true && catalog['serial'] === 302 &&
      rows.length === 2 && ha !== undefined && ha['enabled'] === false &&
      ha['candidate_count'] === 2 &&
      JSON.stringify(ha['networks']) === JSON.stringify(['100.108.0.0/16']),
      JSON.stringify({ catalog: catalog }));
    check('C1c no fabricated "applied" field anywhere in the document',
      text !== undefined && text.indexOf('applied') < 0);
    check('C1d the atomic temp staging name was consumed (no half document left)',
      fileOf(CATALOG_TEMP_PATH) === undefined);
    check('C1e the REAL pure reader accepts that exact document for that exact session',
      text !== undefined &&
      (() => {
        const stored = parseCatalogDoc(text);
        return stored['management_url'] === MGMT && stored['request_id'] === REQUEST_ID &&
          (stored['catalog'] as Record<string, unknown>)['serial'] === 302;
      })());
  }

  // ---- C2: a valid EMPTY catalog is still published ------------------------
  resetWorld();
  {
    const ability = makeAbility(mod);
    ability['managementUrl'] = MGMT;
    ability['watcherTimer'] = 4242;
    G['__NB_ROUTES'] = EMPTY_CATALOG;
    publish(ability);
    await drain();
    await drain();
    const text = fileOf(CATALOG_PATH);
    const doc = text === undefined ? undefined : JSON.parse(text) as Record<string, unknown>;
    const catalog = doc === undefined ? undefined : doc['catalog'] as Record<string, unknown>;
    check('C2 an empty-but-available catalog IS published (the offline list stays current)',
      catalog !== undefined && catalog['available'] === true &&
      JSON.stringify(catalog['networks']) === JSON.stringify([]),
      JSON.stringify({ text: text }));
  }

  // ---- C3: null / available:false / malformed native docs leave the cache ---
  for (const [label, answer] of unavailableAnswers) {
    resetWorld();
    seedFile(CATALOG_PATH, OLD_CATALOG_DOC);
    const ability = makeAbility(mod);
    ability['managementUrl'] = MGMT;
    ability['watcherTimer'] = 4242;
    G['__NB_ROUTES'] = answer;
    publish(ability);
    await drain();
    await drain();
    check(`${label} publishes NOTHING and the last known offline catalog is retained`,
      fileOf(CATALOG_PATH) === OLD_CATALOG_DOC &&
      fileOf(CATALOG_TEMP_PATH) === undefined,
      JSON.stringify({ file: fileOf(CATALOG_PATH) }));
  }

  // ---- C4: teardown / overlap / scope guards -------------------------------
  for (const [label, arm] of guardCases) {
    resetWorld();
    seedFile(CATALOG_PATH, OLD_CATALOG_DOC);
    const ability = makeAbility(mod);
    ability['managementUrl'] = MGMT;
    ability['watcherTimer'] = 4242;
    arm(ability);
    G['__NB_ROUTES'] = HA_CATALOG;
    publish(ability);
    await drain();
    await drain();
    check(`${label} prevents the write (a stale/duplicate publish never lands)`,
      fileOf(CATALOG_PATH) === OLD_CATALOG_DOC && fileOf(CATALOG_TEMP_PATH) === undefined,
      JSON.stringify({ file: fileOf(CATALOG_PATH) }));
  }

  // ---- C5: the in-flight latch is released on BOTH completion paths --------
  resetWorld();
  {
    const ability = makeAbility(mod);
    ability['managementUrl'] = MGMT;
    ability['watcherTimer'] = 4242;
    openErrors()[CATALOG_TEMP_PATH] = 13900003;
    G['__NB_ROUTES'] = HA_CATALOG;
    publish(ability);
    await drain();
    await drain();
    const releasedAfterReject = ability['routeCatalogPublishInFlight'] === false;
    const failedLog = logs().some((l: string): boolean =>
      l.indexOf('VPN_ROUTE_CATALOG_PUBLISH_FAIL') >= 0);
    const fileAfterReject = fileOf(CATALOG_PATH);
    delete openErrors()[CATALOG_TEMP_PATH];
    publish(ability);
    await drain();
    await drain();
    check('C5a a FAILED write is logged as a class and releases the in-flight latch',
      releasedAfterReject && failedLog && fileAfterReject === undefined,
      JSON.stringify({ released: releasedAfterReject, failedLog: failedLog,
        fileAfterReject: fileAfterReject }));
    check('C5b the next tick publishes again (the latch never wedges publication)',
      fileOf(CATALOG_PATH) !== undefined &&
      ability['routeCatalogPublishInFlight'] === false,
      JSON.stringify({ file: fileOf(CATALOG_PATH) }));
  }

  // ---- P: source pins on the byte-identical extension -----------------------
  const slice = (start: string, end: string): string => {
    const at = extSource.indexOf(start);
    if (at < 0) { return ''; }
    const stop = extSource.indexOf(end, at);
    return stop < 0 ? extSource.slice(at) : extSource.slice(at, stop);
  };
  const startLeg = slice('private async startConnectorThenCreate(',
    '// Packaged public CA roots');
  const publishCode = slice('private publishRouteCatalogForUi(): void {',
    'private async feedOneIceSocket(');
  check('P1 exactly ONE connector start call site exists',
    extSource.split('connectorStartWithSocket(').length === 2);
  check('P2 the start call passes the ACTUAL selection object (never JSON.stringify)',
    extSource.includes('buildConnectorConfigJson(device, forceDefaultRoute, routeSelection)') &&
    !extSource.includes('JSON.stringify(routeSelection)'));
  const legAt: number = extSource.indexOf('private async startConnectorThenCreate(');
  const loadAt: number = extSource.indexOf('await loadRouteSelection(');
  const legOpenAt: number = extSource.indexOf('const opened = mgmtSocketOpen();', legAt);
  const legStartAt: number = extSource.indexOf('connectorStartWithSocket(mgmtFd', legAt);
  // 2026-10-09 live route apply (run-route-apply-vpe.mts): the byte-identical
  // extension gained its SECOND (and only other) selection load — inside the
  // apply consumer, which re-reads the LATEST preference document for the
  // running session's management URL when the page's request mailbox rings.
  const applyLoadAt: number = extSource.indexOf(
    'await loadRouteSelection(filesDir, this.managementUrl)');
  check('P3 the selection is loaded at the start leg before the socket open/start, and the ' +
    'ONLY other load is the live-apply consumer of the request mailbox',
    extSource.split('loadRouteSelection(').length === 3 && loadAt > legAt &&
    legOpenAt > loadAt && legStartAt > loadAt &&
    applyLoadAt > extSource.indexOf('private async consumeRouteApplyRequest('),
    `${loadAt}/${legAt}/${legOpenAt}/${legStartAt}/${applyLoadAt}`);
  check('P4 the load uses the management url of the CONFIGURED device and the shared filesDir',
    startLeg.includes('await loadRouteSelection(filesDir, device.management_url)'));
  check('P5 an untrustworthy document is a controlled start failure with a class token',
    startLeg.includes('VPN_ROUTE_SELECTION_INVALID') &&
    startLeg.includes('this.writeConnectFailureForUi(selectionFailure)') &&
    startLeg.includes('return;') &&
    !startLeg.includes('JSON.stringify(error') && !startLeg.includes('error.message'));
  check('P6 NO live prefs watch / recreate-budget change was added',
    !extSource.includes('watchRouteSelection') &&
    !extSource.includes('writeRouteSelection') &&
    !extSource.includes('recreateBudget'));
  check('P7 exactly TWO publish call sites — both on the status-tick callback',
    extSource.split('this.publishRouteCatalogForUi(').length === 3 &&
    extSource.split('this.publishRouteCatalogForUi();').length === 3);
  check('P8 the publish is guarded by the teardown latches BEFORE the write',
    publishCode.length > 0 &&
    publishCode.includes('this.destroyRequested || this.connectorDeathHandled') &&
    publishCode.includes('this.watcherTimer < 0') &&
    publishCode.indexOf('writeRouteCatalog(') > publishCode.indexOf('this.watcherTimer < 0'));
  check('P9 only a readable, available catalog is published (null/false keep the cache)',
    publishCode.includes('catalog === null || catalog.available !== true') &&
    publishCode.includes('connectorRoutes()'));
  check('P10 the write is the real store call with the actual session, scope and clock',
    publishCode.includes('writeRouteCatalog(this.context.filesDir, managementUrl, catalog, ' +
      'requestId, Date.now())') &&
    publishCode.includes('const requestId: string = this.requestId;'));
  check('P11 the in-flight latch is released on the resolve AND the reject path',
    publishCode.split('this.routeCatalogPublishInFlight = false;').length === 3);
  check('P12 the new code adds NO timer and NO fabricated "applied" field',
    publishCode.indexOf('setInterval(') < 0 && publishCode.indexOf('setTimeout(') < 0 &&
    publishCode.indexOf('applied') < 0);
} catch (error) {
  failed++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  if (fixtureDir.length > 0) {
    rmSync(fixtureDir, { recursive: true, force: true });
  }
}

console.log(`${passed} passed, ${failed} failed`);
// The driven start legs arm the extension's own supply interval; the fixture is
// done, so exit explicitly (same as the sibling end-to-end harnesses).
process.exit(failed > 0 ? 1 : 0);
