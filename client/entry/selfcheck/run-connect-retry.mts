// Connect-retry re-entry self-check (2026-10-11, fix/retry) — host-side, no
// device, no network, no build.
//
// Device evidence (pc-connecting-diag/REPORT.md §4.1-§4.4, 0.0.24): after a
// connector abort the card correctly showed 连接失败; the retry then set
// requestSent=true, deleted the failure notice and called
// startVpnExtensionAbility against a STILL ALIVE extension process. The connect
// flow hangs off onCreate behind an unconditional instance latch and this SDK
// declaration has no onConnect, so nothing re-ran — no second status document,
// no second failure notice, no exit from 连接中….
//
// Principle of the sibling suites: the REAL production files are copied
// byte-for-byte into OS-temp fixtures (.ts) at run time and the REAL extension
// class is driven — the code under test is the shipped code, never a mirror.
// @ohos.* / libnetbird_core.so are functional stubs injected by
// exclusion-generation-hooks.mjs (in-memory fs keyed by path, recording hilog,
// fake DNS/fd/network-map/VpnConnection, counting connector start).
//
// Coverage:
//   R1 the aborted attempt really runs: the connector is started once and the
//      attempt ends with the failure notice the UI renders (the 0.0.24 shape)
//   R2 the entry writes the connect ack BEFORE the start flow can abort
//   R3 a repeated onCreate on the same live instance RE-RUNS the connect
//      sequence (the instance latch is gone; the gate is the connector run)
//   R4 the entry itself preserves the previous failure notice — nothing is
//      destroyed before the attempt demonstrably started
//   R5 a connector run that IS active still refuses the entry, and that refusal
//      is answered with an ack (the idempotent "already running" answer), never
//      with a silent no-op
//   P1-P8 wiring pins on the byte-identical Index.ets: the notice deletion moved
//      behind the ack, the stop-then-wait-then-start leg, the bounded
//      pending-start window on the existing 2 s tick, and the single start call
//      site
//
// Run: node client/entry/selfcheck/run-connect-retry.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./exclusion-generation-hooks.mjs', import.meta.url).href);

const HERE = dirname(fileURLToPath(import.meta.url));
const ETS = join(HERE, '..', 'src', 'main', 'ets');
const REAL_DIR = join(ETS, 'vpnextensionability');
const REAL_ROUTES_DIR = join(ETS, 'routes');
const G = globalThis as unknown as Record<string, unknown>;

const FILES_DIR = '/tmp/nb-connect-retry-fixture';
const ACK_PATH = `${FILES_DIR}/netbird-connect-ack`;
const NOTICE_PATH = `${FILES_DIR}/netbird-connect-failure`;
const MARKER_PATH = `${FILES_DIR}/netbird-diag-vpn.marker`;
const DEVICE_PATH = `${FILES_DIR}/netbird-device-config.json`;
const SETUP_KEY_PATH = `${FILES_DIR}/netbird-setup-key`;
const MGMT = 'http://mgmt.test:33073';

// The byte-identical staged set: the same table run-route-vpe.mts stages, minus
// nothing — the extension's whole import graph must link.
const STAGED = ['NetBirdVpnExtensionAbility', 'NetBirdConnector',
  'NetBirdVpnConfig', 'NetBirdRouteValidation', 'NetBirdEndpointExclusion',
  'NetBirdInterfaceStatsProbe', 'NetBirdPhysicalNetworkPolicy',
  'NetBirdIceSocketPool', 'NetBirdUiModel', 'NetBirdRecreatePolicy',
  'NetBirdDiagnosticsCore', 'NetBirdN2Stimulus', 'NetBirdN2AEvent', 'NetBirdN2Sandwich',
  'NetBirdN2ControlRequest', 'NetBirdN2CdPlan', 'NetBirdN2Ring'];
const STAGED_ROUTES = ['NetBirdRouteStore', 'NetBirdRoutePreferences'];
const SHIM_TYPES: Record<string, string[]> = {
  NetBirdConnector: ['ConnectorStatusResult', 'CoreNetworkConfig', 'CoreRouteEntry',
    'ConnectorStartResult', 'ConnectorLogoutResult', 'DeviceConfigFile', 'ManagementEndpoint',
    'NativeTcpSocketOpen', 'RelayAdvertisedView', 'RouteSetAppliedResult',
    'SocketFeedResult', 'SocketSupplyRequest', 'TunReleaseResult', 'WgFeedResult',
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

// The map the driven leg sees on its FIRST poll: available, no signal uri. The
// real leg therefore starts the connector, waits once, then stops it and writes
// the failure notice — exactly the 0.0.24 device shape, in one tick.
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

function startCalls(): number {
  return typeof G['__NB_START_CALLS'] === 'number' ? G['__NB_START_CALLS'] as number : 0;
}

function logs(): string {
  const list = (G['__NB_LOG'] as string[]) ?? [];
  return list.join('\n');
}

function resetWorld(): void {
  G['__NB_FS'] = {};
  G['__NB_FS_OPEN_ERROR'] = {};
  G['__NB_LOG'] = [];
  G['__NB_DNS'] = { 'mgmt.test': '203.0.113.7' };
  G['__NB_NETCFG'] = NETCFG_NO_SIGNAL;
  G['__NB_STATUS'] = {};
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

function seedCredentials(): void {
  seedFile(SETUP_KEY_PATH, 'setup-key-body');
  seedFile(DEVICE_PATH, JSON.stringify({ management_url: MGMT, private_key: 'pk-body' }));
}

function drain(): Promise<void> {
  return new Promise<void>((resolve: () => void): void => { setTimeout(resolve, 0); });
}

function ack(): Record<string, unknown> | undefined {
  const text = fileOf(ACK_PATH);
  if (text === undefined) { return undefined; }
  try {
    return JSON.parse(text) as Record<string, unknown>;
  } catch (error) {
    return undefined;
  }
}

function notice(): Record<string, unknown> | undefined {
  const text = fileOf(NOTICE_PATH);
  if (text === undefined) { return undefined; }
  try {
    return JSON.parse(text) as Record<string, unknown>;
  } catch (error) {
    return undefined;
  }
}

try {
  fixtureDir = mkdtempSync(join(tmpdir(), 'nb-connect-retry-'));
  const dir = fixtureDir;
  for (const name of STAGED) {
    writeFileSync(join(dir, name + '.ts'), readFileSync(join(REAL_DIR, name + '.ets')));
  }
  for (const name of STAGED_ROUTES) {
    writeFileSync(join(dir, name + '.ts'), readFileSync(join(REAL_ROUTES_DIR, name + '.ets')));
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
  const ui = await import(pathToFileURL(join(dir, 'NetBirdUiModel.ts')).href) as
    Record<string, unknown>;
  const extSource: string = readFileSync(join(REAL_DIR, 'NetBirdVpnExtensionAbility.ets'), 'utf8');
  const pageSource: string = readFileSync(join(ETS, 'pages', 'Index.ets'), 'utf8');

  const makeAbility = (): Record<string, unknown> => {
    const ability = new (mod['default'] as new () => Record<string, unknown>)();
    ability['context'] = { filesDir: FILES_DIR };
    return ability;
  };
  const enter = (ability: Record<string, unknown>, requestId: string): void => {
    (ability['onCreate'] as (w: unknown) => void)({ parameters: { 'requestId': requestId } });
  };

  // ---- R1/R2: the aborted attempt runs and acks before it can abort --------
  resetWorld();
  seedCredentials();
  const ability = makeAbility();
  enter(ability, 'ui-1');
  check('R2 the entry writes the connect ack synchronously (before any await)',
    ack() !== undefined && ack()?.['request_id'] === 'ui-1' &&
    ack()?.['stage'] === ui['CONNECT_ACK_STAGE_STARTED'],
    `ack=${JSON.stringify(ack())}`);
  check('R2 the ack carries a positive at_ms',
    typeof ack()?.['at_ms'] === 'number' && (ack()?.['at_ms'] as number) > 0);
  await drain();
  await drain();
  check('R1 the connect sequence really ran (one connector start)',
    startCalls() === 1, `startCalls=${startCalls()}`);
  check('R1 the failed attempt leaves the notice the UI renders',
    notice() !== undefined && notice()?.['error_class'] === 'no-signal-uri',
    `notice=${JSON.stringify(notice())}`);

  // ---- R4/R3: the entry preserves the notice, and a repeated onCreate on the
  // SAME (still alive) instance re-runs the whole connect sequence -------------
  seedFile(NOTICE_PATH, '{"at_ms":111,"error_class":"auth"}');
  const before = startCalls();
  enter(ability, 'ui-2');
  check('R4 the new entry destroys nothing: the previous notice is still there',
    notice()?.['at_ms'] === 111 && notice()?.['error_class'] === 'auth',
    `notice=${JSON.stringify(notice())}`);
  check('R3 the repeated entry is ack-ed for THIS request',
    ack()?.['request_id'] === 'ui-2' && ack()?.['stage'] === ui['CONNECT_ACK_STAGE_STARTED'],
    `ack=${JSON.stringify(ack())}`);
  await drain();
  await drain();
  check('R3 the repeated entry really re-runs the connect sequence (the old ' +
    'instance latch no longer swallows it)',
    startCalls() === before + 1, `before=${before} after=${startCalls()}`);
  check('R3 re-running overwrites the stale verdict with THIS attempt\'s own',
    notice() !== undefined && notice()?.['error_class'] === 'no-signal-uri' &&
    (notice()?.['at_ms'] as number) > 111,
    `notice=${JSON.stringify(notice())}`);
  check('R3 the duplicate-onCreate rejection literal is gone from the source',
    !extSource.includes('duplicate-onCreate') &&
    !extSource.includes('connectionCreationAttempted'));

  // ---- R5: a connector run that IS active still refuses the entry -----------
  G['__NB_STATUS'] = { running: true, state: 'connecting', peer_count: 0 };
  const running = startCalls();
  enter(ability, 'ui-3');
  check('R5 a live connector run refuses the entry (no second connect sequence)',
    startCalls() === running, `running=${running} after=${startCalls()}`);
  check('R5 the refusal is still ANSWERED (idempotent "already running" ack)',
    ack()?.['request_id'] === 'ui-3' && ack()?.['stage'] === ui['CONNECT_ACK_STAGE_RUNNING'],
    `ack=${JSON.stringify(ack())}`);
  check('R5 the refusal is named as a connector run, not an instance latch',
    logs().includes('phase=connect-run-active') &&
    !logs().includes('connection-already-attempted'));
  check('R5 the probe answers null -> entry proceeds (never a silent block)',
    typeof mod['default'] === 'function' &&
    extSource.includes('const status: ConnectorStatusResult | null = pollStatusOrNull();'));

  // ---- P1-P8: wiring pins on the page (byte-identical source) ---------------
  const requestStart: string = pageSource.slice(pageSource.indexOf('private requestStart('),
    pageSource.indexOf('private startVpnExtension('));
  const startLeg: string = pageSource.slice(pageSource.indexOf('private startVpnExtension('),
    pageSource.indexOf('private extensionGone('));
  const updatePending: string = pageSource.slice(
    pageSource.indexOf('private updatePendingStart('),
    pageSource.indexOf('private readConnectAckAtMs('));
  const timeoutLeg: string = pageSource.slice(
    pageSource.indexOf('private pendingStartTimedOut('),
    pageSource.indexOf('private requestStop('));
  check('P1 the request no longer deletes the failure notice up front',
    requestStart.length > 0 && !requestStart.includes('removeConnectFailure('));
  check('P2 the request arms the bounded pending-start window and resets its ack latch',
    requestStart.includes('this.pendingStartDeadlineMs = this.lastStartAtMs + PENDING_START_TIMEOUT_MS') &&
    requestStart.includes('this.pendingStartAcked = false'));
  check('P3 the request decides the start leg on the extension\'s own stop evidence',
    requestStart.includes('connectStartLeg(this.extensionGone())') &&
    requestStart.includes('CONNECT_LEG_AFTER_STOP') &&
    requestStart.includes('this.stopVpnExtensionThenStart(fromAutoReconnect)'));
  check('P4 the stop leg is bounded by its own clock and never waits on the ' +
    'platform promise',
    pageSource.includes('vpnExtension.stopVpnExtensionAbility(this.buildWant()).catch(') &&
    pageSource.includes('const deadline: number = Date.now() + EXTENSION_STOP_WAIT_MS;') &&
    pageSource.includes('await sleep(EXTENSION_STOP_POLL_MS);'));
  check('P5 the page has exactly ONE extension-start call site, inside the leg helper',
    (pageSource.match(/startVpnExtensionAbility\(/g) ?? []).length === 1 &&
    startLeg.includes('vpnExtension.startVpnExtensionAbility(this.buildWant())') &&
    !requestStart.includes('startVpnExtensionAbility('));
  check('P6 the notice is superseded only once the ack proves the attempt started',
    updatePending.includes('decidePendingStart(facts)') &&
    updatePending.includes('PENDING_START_ACKNOWLEDGED') &&
    updatePending.includes('removeConnectFailure(getContext(this).filesDir)'));
  check('P7 the timeout releases the latch and renders the failure',
    updatePending.includes('PENDING_START_TIMEOUT') &&
    timeoutLeg.includes('this.requestSent = false;') &&
    timeoutLeg.includes('readConnectFailure(getContext(this).filesDir)') &&
    timeoutLeg.includes('this.actionError ='));
  check('P8 the bookkeeping rides the EXISTING tick (no second timer)',
    (pageSource.match(/this\.updatePendingStart\(live\);/g) ?? []).length === 1 &&
    pageSource.includes('ackAtMs = this.readConnectAckAtMs();'));
  check('P11 an outcome closes the window for good (a later disconnect can never ' +
    'fire this timeout over a session that really ran)',
    requestStart.includes('this.pendingStartOutcome = false;') &&
    updatePending.includes('this.pendingStartOutcome = true;') &&
    updatePending.includes("'outcomeSeen': this.pendingStartOutcome"));
  check('P9 the extension gate reads the connector run, not an instance latch',
    extSource.includes('const connectorRunning: boolean | null = this.connectorRunState();') &&
    extSource.includes('connectEntryDecision(connectorRunning) === CONNECT_ENTRY_REJECT_RUNNING') &&
    extSource.indexOf('connectEntryDecision(connectorRunning)') <
      extSource.indexOf('this.writeConnectAckForUi(CONNECT_ACK_STAGE_STARTED)'));
  check('P10 the ack write is the one shared document, opened TRUNC and closed',
    extSource.includes('buildConnectAttemptAck(this.requestId, Date.now(), stage)') &&
    extSource.includes('fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC'));
} finally {
  if (fixtureDir.length > 0) {
    rmSync(fixtureDir, { recursive: true, force: true });
  }
}
console.log(`${passed} passed, ${failed} failed`);
// The driven start legs arm the extension's own supply interval; the fixture is
// done, so exit explicitly (same as the sibling end-to-end harnesses).
process.exit(failed > 0 ? 1 : 0);
