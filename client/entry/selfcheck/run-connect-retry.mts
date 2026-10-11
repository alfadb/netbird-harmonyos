// Connect-retry re-entry self-check (2026-10-11 fix/retry; retargeted 2026-10-12
// onto the official tap handling) — host-side, no device, no network, no build.
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
//   R2 the engine's OWN state report (the status document) is written BEFORE the
//      start flow can abort — the mechanism that replaced the deleted ack file
//   R3 a repeated onCreate on the same live instance RE-RUNS the connect
//      sequence (the instance latch is gone; the gate is the connector run)
//   R4 the entry itself preserves the previous failure notice — nothing is
//      destroyed before the attempt demonstrably started
//   R5 a connector run that IS active still refuses the entry, and that refusal
//      is answered by the ACTIVE run's own document, never by a silent no-op and
//      never by a shell-invented evidence file
//   P1-P12 wiring pins on the byte-identical Index.ets / extension source: the
//      notice supersession moved behind the engine's own report, the
//      stop-then-wait-then-start leg, the official 7 s pending action with NO
//      failure text on expiry, the 2 s force-cancel window, and the single start
//      call site
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
const STATUS_PATH = `${FILES_DIR}/netbird-status.json`;
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
  'NetBirdUiModel', 'NetBirdRecreatePolicy',
  'NetBirdDiagnosticsCore', 'NetBirdN2Stimulus', 'NetBirdN2AEvent', 'NetBirdN2Sandwich',
  'NetBirdN2ControlRequest', 'NetBirdN2CdPlan', 'NetBirdN2Ring'];
const STAGED_ROUTES = ['NetBirdRouteStore', 'NetBirdRoutePreferences'];
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

function status(): Record<string, unknown> | undefined {
  const text = fileOf(STATUS_PATH);
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
  const uiSource: string = readFileSync(join(REAL_DIR, 'NetBirdUiModel.ets'), 'utf8');
  const pageSource: string = readFileSync(join(ETS, 'pages', 'Index.ets'), 'utf8');

  const makeAbility = (): Record<string, unknown> => {
    const ability = new (mod['default'] as new () => Record<string, unknown>)();
    ability['context'] = { filesDir: FILES_DIR };
    return ability;
  };
  const enter = (ability: Record<string, unknown>, requestId: string): void => {
    (ability['onCreate'] as (w: unknown) => void)({ parameters: { 'requestId': requestId } });
  };

  // ---- R1/R2: the aborted attempt runs and reports before it can abort -----
  resetWorld();
  seedCredentials();
  // The connector's own live status. `running: false` is what the ENTRY gate must
  // see to let this leg through (a run in flight is refused); the `connecting`
  // token is what the core reports the instant `connector_start` answers
  // (`{"started":true,"state":"connecting"}`, client/core/src/state.rs
  // ConnState::as_str) — the stub's status is static, so one fixture carries both
  // halves of the same start.
  G['__NB_STATUS'] = { running: false, state: 'connecting', peer_count: 0,
    route_count: 0, reconnects: 0 };
  const ability = makeAbility();
  enter(ability, 'ui-1');
  check('R2 no evidence file is invented up front (the ack document is gone)',
    fileOf(ACK_PATH) === undefined);
  await drain();
  await drain();
  check('R1 the connect sequence really ran (one connector start)',
    startCalls() === 1, `startCalls=${startCalls()}`);
  // The engine's OWN state report: the status document the UI's pending action reads
  // (the mechanism that replaced the ack file). It is written from the live core
  // status as soon as the run started — BEFORE the start flow reaches its abort.
  check('R2 the engine\'s OWN state report is written for THIS request',
    status() !== undefined && status()?.['state'] === 'connecting' &&
    status()?.['request_id'] === 'ui-1',
    `status=${JSON.stringify(status())}`);
  check('R2 the report carries a positive updated_at_ms',
    typeof status()?.['updated_at_ms'] === 'number' && (status()?.['updated_at_ms'] as number) > 0);
  check('R2 the report precedes the attempt\'s abort (reported first, then failed)',
    logs().includes('VPN_ENGINE_REPORT|') &&
    logs().includes('requestId=ui-1') &&
    logs().indexOf('VPN_ENGINE_REPORT|') < logs().indexOf('VPN_CONNECT_FAILURE_NOTICE|'),
    `logs=${logs().split('\n').filter((l: string): boolean => l.includes('VPN_')).join(' / ')}`);
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
  await drain();
  await drain();
  check('R3 the repeated entry re-reports the engine state for THIS request',
    status()?.['request_id'] === 'ui-2' && status()?.['state'] === 'connecting',
    `status=${JSON.stringify(status())}`);
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
  check('R5 the refusal needs no evidence document of its own (the active run ' +
    'is already reporting its state, and the ack file is gone)',
    fileOf(ACK_PATH) === undefined && !extSource.includes('writeConnectAckForUi'));
  check('R5 the refusal is named as a connector run, not an instance latch',
    logs().includes('phase=connect-run-active') &&
    !logs().includes('connection-already-attempted'));
  check('R5 the probe answers null -> entry proceeds (never a silent block)',
    typeof mod['default'] === 'function' &&
    extSource.includes('const status: ConnectorStatusResult | null = pollStatusOrNull();'));

  // ---- P1-P12: wiring pins on the page / extension (byte-identical source) --
  const requestStart: string = pageSource.slice(pageSource.indexOf('private requestStart('),
    pageSource.indexOf('private startVpnExtension('));
  const startLeg: string = pageSource.slice(pageSource.indexOf('private startVpnExtension('),
    pageSource.indexOf('private extensionGone('));
  const updatePending: string = pageSource.slice(
    pageSource.indexOf('private updatePendingAction('),
    pageSource.indexOf('private supersedePreviousVerdict('));
  const supersede: string = pageSource.slice(
    pageSource.indexOf('private supersedePreviousVerdict('),
    pageSource.indexOf('private pendingActionExpired('));
  const expiredLeg: string = pageSource.slice(
    pageSource.indexOf('private pendingActionExpired('),
    pageSource.indexOf('private forceCancelConnection('));
  const forceCancel: string = pageSource.slice(
    pageSource.indexOf('private forceCancelConnection('),
    pageSource.indexOf('private requestStop('));
  check('P1 the request no longer deletes the failure notice up front',
    requestStart.length > 0 && !requestStart.includes('removeConnectFailure('));
  check('P2 the request arms the OFFICIAL 7 s pending action for the connect target',
    requestStart.includes('this.pendingActionTarget = PENDING_TARGET_CONNECTED') &&
    requestStart.includes('this.pendingActionDeadlineMs = this.lastStartAtMs + ' +
      'PENDING_ACTION_TIMEOUT_MS') &&
    requestStart.includes('this.forceCancelArmedAtMs = 0') &&
    requestStart.includes('this.noticeSuperseded = false'));
  check('P2 the 30 s attempt timeout is gone (no timeout of the connecting state)',
    !pageSource.includes('PENDING_START_TIMEOUT_MS') &&
    !pageSource.includes('pendingStartTimedOut'));
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
  check('P6 the notice is superseded only once THIS attempt\'s own engine report exists',
    updatePending.includes('decideTransition(facts)') &&
    updatePending.includes('if (live !== null && !this.noticeSuperseded)') &&
    supersede.includes('removeConnectFailure(getContext(this).filesDir)') &&
    supersede.includes('notice.at_ms >= this.lastStartAtMs - 2000'));
  check('P7 the expiry falls back to the last reported state and renders NO failure text',
    updatePending.includes('decision.expired') &&
    expiredLeg.includes('this.requestSent = false;') &&
    expiredLeg.includes('note=fallback-to-last-reported-state') &&
    !expiredLeg.includes('connectFailureText(') &&
    !expiredLeg.includes('this.actionError ='));
  check('P8 the bookkeeping rides the EXISTING tick (no second timer) and reads no ack',
    (pageSource.match(/this\.updatePendingAction\(live\);/g) ?? []).length === 1 &&
    !pageSource.includes('readConnectAckAtMs') &&
    !pageSource.includes('netbird-connect-ack'));
  check('P9 the extension gate reads the connector run, not an instance latch',
    extSource.includes('const connectorRunning: boolean | null = this.connectorRunState();') &&
    extSource.includes('connectEntryDecision(connectorRunning) === CONNECT_ENTRY_REJECT_RUNNING') &&
    !extSource.includes('CONNECT_ACK_STAGE_STARTED'));
  check('P10 the extension reports the engine\'s own state from the START leg, before ' +
    'the watcher exists',
    extSource.includes('this.publishEngineStateReport(true);') &&
    extSource.includes('if (this.watcherTimer < 0 && !allowBeforeWatcher)') &&
    extSource.indexOf('this.publishEngineStateReport(true);') <
      extSource.indexOf('const net: CoreNetworkConfig | null = await this.waitNetworkConfigBounded();') &&
    extSource.includes("this.publishStatusForUi(status, 'unknown', true)") &&
    extSource.includes('this.publishEngineStateReport();'));
  check('P11 the force-cancel window: 2 s delay, toggle gated on it, tap aborts the ' +
    'transition instead of starting a new connect',
    pageSource.includes('return this.stopSent || this.transitionPainted;') &&
    pageSource.includes('if (this.transitionPainted) {') &&
    pageSource.includes('return this.forceCancelOpen;') &&
    pageSource.includes('if (this.forceCancelOpen) {') &&
    pageSource.includes('this.forceCancelConnection();') &&
    forceCancel.includes('this.requestStop();'));
  check('P12 the official state set replaced our invented failure labels',
    typeof ui['engineReportState'] === 'function' &&
    uiSource.includes("export const PENDING_ACTION_TIMEOUT_MS: number = 7000;") &&
    uiSource.includes("export const FORCE_CANCEL_DELAY_MS: number = 2000;") &&
    uiSource.includes("return '需要登录';") &&
    !uiSource.includes("return '连接失败';") &&
    !uiSource.includes("'无法连接服务器'") &&
    !uiSource.includes('PENDING_START_TIMEOUT'));
  check('P13 a transition the page KNOWS is over releases the tap latch at once ' +
    '(the platform refusal, the resolved stop, the engine\'s end)',
    pageSource.includes('private clearPendingAction(): void {') &&
    pageSource.includes('this.pendingActionTarget = PENDING_TARGET_NONE;\n' +
      '    this.pendingActionDeadlineMs = 0;\n    this.updatePendingAction(null);') &&
    pageSource.includes('this.clearPendingAction();') &&
    (pageSource.match(/this\.clearPendingAction\(\);/g) ?? []).length === 3);
} finally {
  if (fixtureDir.length > 0) {
    rmSync(fixtureDir, { recursive: true, force: true });
  }
}
console.log(`${passed} passed, ${failed} failed`);
// The driven start legs arm the extension's own supply interval; the fixture is
// done, so exit explicitly (same as the sibling end-to-end harnesses).
process.exit(failed > 0 ? 1 : 0);
