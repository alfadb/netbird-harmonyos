// F1–F5 (v0.0.27 overnight-outage fix) — host-side checks, no device, no build.
//
// The directive is REPORT.md §5, whose evidence is §2–§4:
//   F1  the host-side socket-supply gate must be INSTANTANEOUS, not latched: a
//       supply is refused only while the platform CURRENTLY reports no usable
//       physical default network (REPORT.md §0.1/§2.4: the remembered
//       `networkDirty` latch stayed true for 5 h 29 m after ONE aborted
//       recovery and refused every management/signal/relay/DNS socket).
//   F2  the recovery must not depend on a new netId edge: it is re-invoked from
//       the EXISTING 5 s watcher tick while incomplete, and its 13 silent
//       `break`s now report a stage token (HiLog WARN + the diagnostics/tee
//       path, so the stage reaches netbird-vpn.log).
//   F3  a network switch must never empty the core's DNS list: the seam always
//       carries a resolved address (and the core no longer wipes on an empty
//       one — pinned in client/core/tests/network_change_keeps_dns_f3.rs).
//   F4  upstream's callback semantics (android-client NetworkChangeDetector
//       .java:211-230 / ConcreteNetworkAvailabilityListener.java:70-84): dedupe
//       on the network identity, ignore the first observation after subscribe,
//       treat a capability change on the default handle as a trigger.
//   F5  the extension owns network handling; the UI is not required.
//
// Same discipline as run-exclusion-generation.mts: the REAL production files
// are copied BYTE-IDENTICALLY into OS-temp fixtures and the REAL extension class
// is driven; @ohos.* / libnetbird_core.so are functional stubs injected by
// exclusion-generation-hooks.mjs. The fixture is never rewritten.
//
// Run: node client/entry/selfcheck/run-network-recovery.mts
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
const STAGED_ROUTES = ['NetBirdRouteStore', 'NetBirdRoutePreferences'];
const REAL_ENTRY_DIR = join(ETS, 'entryability');
const FIXTURE_FILESDIR = '/tmp/n2h-net-recovery-fixture';
const REQUEST_ID = 'req-net-rec';
// Mirrors the shipped constant (asserted equal below).
const RETRY_MIN_MS = 15000;

// The modules staged byte-identically (the same list run-exclusion-generation.mts
// stages — the fixture link surface of the whole extension).
const STAGED = ['NetBirdVpnExtensionAbility', 'NetBirdConnector',
  'NetBirdVpnConfig', 'NetBirdRouteValidation', 'NetBirdEndpointExclusion',
  'NetBirdInterfaceStatsProbe', 'NetBirdPhysicalNetworkPolicy',
  // 0.0.34 peer-status lag fix (REPORT §5 F1/F4): the publish-debounce policy
  // the REAL extension now imports by value — zero imports, no erased type
  // crossing the value import, staged byte-identically, no shim.
  'NetBirdStatusPolicy',
  'NetBirdUiModel', 'NetBirdRecreatePolicy', 'NetBirdDiagnosticsCore',
  'NetBirdN2Stimulus', 'NetBirdN2AEvent', 'NetBirdN2Sandwich',
  'NetBirdN2ControlRequest', 'NetBirdN2CdPlan', 'NetBirdN2Ring'];
const SHIM_TYPES: Record<string, string[]> = {
  NetBirdConnector: ['ConnectorStatusResult', 'CoreNetworkConfig', 'CoreRouteEntry',
    'ConnectorStartResult', 'ConnectorLogoutResult', 'DeviceConfigFile', 'ManagementEndpoint',
    'NativeTcpSocketOpen', 'RelayAdvertisedView', 'RouteSetAppliedResult',
    'SocketFeedResult', 'SocketSupplyRequest', 'TunReleaseResult', 'WgFeedResult',
    'IceMuxArmResult', 'PhysicalBearerFacts',
    'PhoneLogConfigureResult', 'PhoneLogSummary'],
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
  return logs().filter((l: string): boolean => l.indexOf(prefix) >= 0)
    .map((l: string): string => l.slice(l.indexOf(prefix)));
}
function lastLine(prefix: string): string {
  const hits = linesWith(prefix);
  return hits.length === 0 ? '' : hits[hits.length - 1];
}
function fieldOf(line: string, key: string): string {
  const hit = line.split('|').find((f: string): boolean => f.indexOf(key + '=') === 0);
  return hit === undefined ? '' : hit.slice(key.length + 1);
}
// The level the recording hilog stub prefixes each line with ('[W] ' etc.).
function levelLoggedWarn(prefix: string): boolean {
  return logs().some((l: string): boolean => l.indexOf('[W] ' + prefix) === 0);
}

function supplyOf(key: string): number {
  return Number(G['__NB_SUPPLY_' + key] ?? 0);
}
function resetSupply(): void {
  for (const key of ['WG_OPEN', 'WG_FEED', 'MGMT_OPEN', 'MGMT_FEED', 'ICE_OPEN', 'ICE_MUX_ARM',
    'DNS', 'DNS_RESOLVE', 'MGMT_ADDR']) {
    G['__NB_SUPPLY_' + key] = 0;
  }
}

function sleepMs(ms: number): Promise<void> {
  return new Promise<void>((resolve: () => void): void => { setTimeout(resolve, ms); });
}
// One macrotask boundary drains the shell's own promise chain deterministically.
async function drainProbe(): Promise<void> {
  await new Promise<void>((resolve: () => void): void => { setTimeout(resolve, 0); });
}

// The network-context probe's connection tables (exclusion-generation-hooks.mjs).
function seedNetCtx(tables: Record<string, unknown>): void {
  for (const key of ['__NB_NET_DEFAULT_ID', '__NB_NET_APP_ID', '__NB_NET_ALL_IDS',
    '__NB_NET_CAPS_BY_ID', '__NB_NET_PROPS_BY_ID', '__NB_NET_THROW_CODES']) {
    G[key] = undefined;
  }
  for (const key of Object.keys(tables)) {
    G[key] = tables[key];
  }
}
function WiFiLink(address: string): Record<string, unknown> {
  return {
    dnses: [{ address: '119.29.29.29', family: 1 }],
    linkAddresses: [{ address: { address: address, family: 1 } }]
  };
}
function seedWifi(netId: number, address: string): void {
  seedNetCtx({ __NB_NET_DEFAULT_ID: netId, __NB_NET_ALL_IDS: [netId],
    __NB_NET_CAPS_BY_ID: { [netId]: { networkCap: [15], bearerTypes: [1] } },
    __NB_NET_PROPS_BY_ID: { [netId]: WiFiLink(address) } });
}
function seedCellular(netId: number, address: string): void {
  // bearerTypes [0] = cellular, networkCap [15] = the NOT_VPN bit the physical
  // evidence probe requires (the same caps run-exclusion-generation.mts seeds
  // for its cellular link).
  seedNetCtx({ __NB_NET_DEFAULT_ID: netId, __NB_NET_ALL_IDS: [netId],
    __NB_NET_CAPS_BY_ID: { [netId]: { networkCap: [15], bearerTypes: [0] } },
    __NB_NET_PROPS_BY_ID: { [netId]: WiFiLink(address) } });
}

function freshAbility(mod: Record<string, unknown>): Record<string, unknown> {
  const ability = new (mod['default'] as new () => Record<string, unknown>)();
  ability['requestId'] = REQUEST_ID;
  ability['context'] = { filesDir: FIXTURE_FILESDIR };
  return ability;
}

// The ability wired the way the create() flow leaves it: an accepted generation
// with a live connection, the frozen management endpoint, and the address the
// start leg resolved (F3's `managementDialAddr`).
function recoveryAbility(mod: Record<string, unknown>, netId: number,
  FakeConnection: new () => Record<string, unknown>,
  dialAddr: string = '203.0.113.9:33073'): Record<string, unknown> {
  const ability = freshAbility(mod);
  ability['networkRecoveryReady'] = true;
  ability['networkNetId'] = netId;
  ability['connection'] = new FakeConnection();
  ability['managementEndpoint'] = { host: 'mgmt.test', port: 33073, ip: '' };
  ability['managementDialAddr'] = dialAddr;
  ability['signalUri'] = '';
  ability['createSettled'] = true;
  ability['tunFd'] = 42;
  return ability;
}

// The REAL watcher tick body, replayed by hand: starts the REAL watcher with the
// interval CAPTURED (no real timer is left pending) and returns the tick closure.
function watcherTick(ability: Record<string, unknown>): () => void {
  const captured: Array<() => void> = [];
  const previous = G['setInterval'];
  G['setInterval'] = (handler: () => void): number => {
    captured.push(handler);
    return 7300 + captured.length;
  };
  (ability['startStatusAndNetcfgWatcher'] as () => void)();
  G['setInterval'] = previous;
  return captured[captured.length - 1];
}

const WATCHER_STATUS: Record<string, unknown> = {
  running: true, state: 'connected', peer_count: 1, route_count: 2,
  reconnects: 0, terminal: false, wg: { device_up: true, ready: true }
};

// N5 (0.0.34): park the n-th protect() of the fixture's FakeConnection (the
// helper run-route-apply-vpe.mts uses) so the window between "protect issued"
// and "protect settled" is steerable — the ONLY window in which the protected
// socket's generation can move under the await.
function parkProtectAt(n: number): void {
  G['__NB_PROTECT_PARK_AT'] = n;
  G['__NB_PROTECT_PARKED'] = undefined;
}
function parkedProtect(): Record<string, unknown> | null {
  const parked: unknown = G['__NB_PROTECT_PARKED'];
  return parked === undefined || parked === null ? null : parked as Record<string, unknown>;
}
async function waitProtectParked(): Promise<void> {
  for (let i = 0; i < 400; i++) {
    await sleepMs(10);
    if (parkedProtect() !== null) { return; }
  }
}
function releaseParkedProtect(): void {
  const parked: Record<string, unknown> | null = parkedProtect();
  G['__NB_PROTECT_PARK_AT'] = undefined; // every later protect resolves as always
  G['__NB_PROTECT_PARKED'] = undefined;
  if (parked !== null) { (parked['resolve'] as () => void)(); }
}

try {
  const dir = mkdtempSync(join(tmpdir(), 'nb-net-recovery-'));
  for (const name of STAGED) {
    writeFileSync(join(dir, name + '.ts'), readFileSync(join(REAL_DIR, name + '.ets')));
  }
  for (const name of STAGED_ROUTES) {
    writeFileSync(join(dir, name + '.ts'), readFileSync(join(REAL_ROUTES_DIR, name + '.ets')));
  }
  writeFileSync(join(dir, 'EntryAbility.ts'),
    readFileSync(join(REAL_ENTRY_DIR, 'EntryAbility.ets')));
  for (const name of Object.keys(SHIM_TYPES)) {
    writeFileSync(join(dir, name + '.shim.ts'),
      'export * from \'./' + name + '.ts\';\n' +
      SHIM_TYPES[name].map((t: string): string => 'export const ' + t + ' = undefined;')
        .join('\n') + '\n');
  }
  // The two platform-IO siblings the REAL extension imports by value are bridged
  // with no-op modules (host-checked by their own suites, never here).
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
  const policy = await import(pathToFileURL(join(dir, 'NetBirdPhysicalNetworkPolicy.ts')).href) as
    Record<string, unknown>;
  const fakeVpnNs = (await import('@ohos.net.vpnExtension') as Record<string, unknown>)['default'] as
    Record<string, unknown>;
  const FakeConnection = fakeVpnNs['VpnConnection'] as new () => Record<string, unknown>;

  const extText = readFileSync(join(REAL_DIR, 'NetBirdVpnExtensionAbility.ets'), 'utf8');
  const policyText = readFileSync(join(REAL_DIR, 'NetBirdPhysicalNetworkPolicy.ets'), 'utf8');
  const coreText = readFileSync(join(HERE, '..', '..', 'core', 'src', 'connector.rs'), 'utf8');
  const uiText = readFileSync(join(ETS, 'pages', 'Index.ets'), 'utf8');

  // -------------------------------------------------------------------------
  // A. F1 pure policy: the gate has no memory (byte-identical shipped module).
  // -------------------------------------------------------------------------
  const gate = {
    networkUnusableNow: false, networkRecoveryReady: true,
    destroyRequested: false, destroyInFlight: false, recreateInFlight: false,
    hasConnection: true, connectionUnchanged: true,
    protectDeviation: false, productionGateBlocked: false
  };
  const reasonOf = policy['socketSupplyRejectionReason'] as (g: unknown) => string;
  check('A1 the outage shape (usable network, settled create, a switch happened earlier) is SERVED',
    reasonOf(gate) === '', reasonOf(gate));
  check('A2 an unusable network at THIS request is refused with a stage token',
    reasonOf({ ...gate, networkUnusableNow: true }) === 'network-unusable');
  check('A3 before the first create the network rule does not bite',
    reasonOf({ ...gate, networkUnusableNow: true, networkRecoveryReady: false }) === '');
  check('A4 no policy input can express "a switch happened earlier"',
    !policyText.includes('networkDirty: boolean') && !policyText.includes('isDns: boolean') &&
    policyText.indexOf('gate.networkDirty') < 0 && policyText.indexOf('gate.isDns') < 0);
  const capSig = policy['capabilitySignature'] as (c: number[]) => string;
  check('A5 the capability signature is order-insensitive and dedupes the empty set',
    capSig([15, 1]) === capSig([1, 15]) && capSig([15, 1]) === '1,15' &&
    capSig([]) === '');

  // -------------------------------------------------------------------------
  // B. F1/F3: the latch is gone end to end, through the REAL extension.
  // The device shape: Wi-Fi 101 -> cellular 314, and the recovery's first step
  // (the protected resolution of the management host) fails with the EPERM the
  // capture holds at 01:14:35.833/.848 -> an EMPTY ip.
  // -------------------------------------------------------------------------
  seedCellular(314, '10.41.63.59');
  G['__NB_STATUS'] = WATCHER_STATUS;
  G['__NB_DNS_POLL'] = { ok: true, pending: false, ip: '' };
  resetSupply();
  resetLog();
  const live = recoveryAbility(mod, 101, FakeConnection);
  (live['physicalNetworkAvailable'] as (e: string, en: number, n: number) => void)(
    'netAvailable', 314, 314);
  await sleepMs(300);
  const abortLines = linesWith('VPN_NETWORK_RECOVERY_ABORT|');
  check('B1 the switch is measured and the recovery aborts on the protected-DNS resolve',
    live['networkDirty'] === true && live['networkNetId'] === 314 && abortLines.length === 1 &&
    abortLines[0].indexOf('|stage=protected-dns|') > 0 &&
    fieldOf(abortLines[0], 'netId') === '314',
    JSON.stringify({ dirty: live['networkDirty'], lines: abortLines }));
  check('B2 the abort is a hilog WARN line (this ROM keeps no I-level domain line)',
    levelLoggedWarn('VPN_NETWORK_RECOVERY_ABORT|'),
    JSON.stringify(logs().filter((l: string): boolean =>
      l.indexOf('VPN_NETWORK_RECOVERY_ABORT') >= 0)));
  // Pre-F1 this was `reason=network-dirty` for management AND dns — the exact
  // 5 h 29 m refusal. The network is usable NOW, so both must be SERVED.
  resetSupply();
  resetLog();
  await (live['fulfillSocketRequest'] as (r: unknown) => Promise<void>)(
    { id: 'outage-m', kind: 'tcp', purpose: 'management' });
  await (live['fulfillSocketRequest'] as (r: unknown) => Promise<void>)(
    { id: 'outage-d', kind: 'udp', purpose: 'dns' });
  await (live['fulfillSocketRequest'] as (r: unknown) => Promise<void>)(
    { id: 'outage-dtcp', kind: 'tcp', purpose: 'dns' });
  const outageSupply = linesWith('VPN_SOCKET_SUPPLY|');
  check('B3 with the latch still SET, a usable network serves management and DNS UDP/TCP',
    outageSupply.length === 3 &&
    outageSupply.every((l: string): boolean => fieldOf(l, 'protect') === 'protected') &&
    outageSupply.every((l: string): boolean => fieldOf(l, 'reason') === '') &&
    supplyOf('MGMT_OPEN') === 2 && supplyOf('ICE_OPEN') === 1,
    JSON.stringify({ lines: outageSupply, mgmt: supplyOf('MGMT_OPEN'), dns: supplyOf('ICE_OPEN') }));
  const protectedAtInfo = outageSupply.length === 3 && outageSupply.every((l: string): boolean =>
    logs().some((raw: string): boolean => raw.indexOf('[I] VPN_SOCKET_SUPPLY|') === 0));
  // A REFUSAL, on the other hand, must survive a W-level capture — on this ROM
  // the I-level A02900 lines never reach the buffer, which is exactly why
  // REPORT.md §2.4 had to DERIVE the reason instead of reading it.
  const refused = recoveryAbility(mod, 314, FakeConnection);
  refused['destroyRequested'] = true;
  resetLog();
  await (refused['fulfillSocketRequest'] as (r: unknown) => Promise<void>)(
    { id: 'refused-m', kind: 'tcp', purpose: 'management' });
  check('B4 a refusal is logged at WARN level; a protected supply stays at INFO',
    logs().some((raw: string): boolean => raw.indexOf('[W] VPN_SOCKET_SUPPLY|') === 0 &&
      raw.indexOf('|reason=destroy|') > 0) && protectedAtInfo,
    JSON.stringify(logs().filter((l: string): boolean => l.indexOf('VPN_SOCKET_SUPPLY') >= 0)));

  // -------------------------------------------------------------------------
  // C. F2: the recovery gets an owner — the EXISTING 5 s watcher tick retries it
  // with NO new network event, and stops once it succeeded.
  // -------------------------------------------------------------------------
  live['lanRoamArmed'] = false; // this group measures the recovery, not the roam leg
  const tick = watcherTick(live);
  resetSupply();
  resetLog();
  G['__NB_DNS_POLL'] = { ok: true, pending: false, ip: '203.0.113.7' };
  tick();
  await sleepMs(400);
  const recoveredC1 = lastLine('VPN_NETWORK_RECOVERED|');
  check('C1 one watcher tick re-invokes the aborted recovery and it SUCCEEDS (no new netId edge)',
    recoveredC1.indexOf('netId=314') > 0 &&
    live['networkDirty'] === false && supplyOf('WG_FEED') === 1 &&
    supplyOf('MGMT_ADDR') === 1,
    JSON.stringify({ recovered: recoveredC1,
      dirty: live['networkDirty'], wg: supplyOf('WG_FEED') }));
  resetSupply();
  resetLog();
  tick();
  await sleepMs(200);
  check('C2 the retry stops once the recovery completed (no work in the next tick)',
    supplyOf('WG_OPEN') === 0 && linesWith('VPN_NETWORK_RECOVERED|').length === 0);

  // C3: a PERSISTENTLY failing recovery is retried, but not once per 5 s tick —
  // the shipped window bounds it (and so bounds the abort log).
  seedCellular(314, '10.41.63.59');
  G['__NB_DNS_POLL'] = { ok: true, pending: false, ip: '' };
  resetSupply();
  resetLog();
  const stuck = recoveryAbility(mod, 101, FakeConnection);
  stuck['lanRoamArmed'] = false;
  (stuck['physicalNetworkAvailable'] as (e: string, en: number, n: number) => void)(
    'netAvailable', 314, 314);
  await sleepMs(300);
  const afterSwitch = linesWith('VPN_NETWORK_RECOVERY_ABORT|').length;
  const stuckTick = watcherTick(stuck);
  resetLog();
  stuckTick();
  await sleepMs(300);
  const firstRetry = linesWith('VPN_NETWORK_RECOVERY_ABORT|').length;
  stuckTick();
  await sleepMs(300);
  const immediateSecond = linesWith('VPN_NETWORK_RECOVERY_ABORT|').length;
  stuck['recoveryRetryAtMs'] = Date.now() - (RETRY_MIN_MS + 5000); // the window slid
  stuckTick();
  await sleepMs(300);
  const afterWindow = linesWith('VPN_NETWORK_RECOVERY_ABORT|').length;
  check('C3 the retry is bounded: attempted, then refused inside the window, then attempted again',
    afterSwitch === 1 && firstRetry === 1 && immediateSecond === 1 && afterWindow === 2,
    JSON.stringify({ afterSwitch, firstRetry, immediateSecond, afterWindow }));
  check('C4 the bounding window is the shipped constant and it is a timestamp, not a count',
    extText.includes('const PHYSICAL_RECOVERY_RETRY_MIN_MS: number = 15000;') &&
    extText.includes('now - this.recoveryRetryAtMs < PHYSICAL_RECOVERY_RETRY_MIN_MS'));

  // -------------------------------------------------------------------------
  // D. F2 visibility: every abort of the chase names its stage, and none of the
  // 13 silent `break`s can come back. The stage vocabulary is closed.
  // -------------------------------------------------------------------------
  const recoveryBody = extText.slice(
    extText.indexOf('private async recoverPhysicalNetwork(): Promise<void> {'),
    extText.indexOf('private async startConnectorThenCreate('));
  const STAGES = ['no-connection', 'recreate-in-flight', 'network-lost', 'no-management-endpoint',
    'physical-dns', 'protected-dns', 'wg-socket-open', 'wg-protect', 'wg-feed',
    'management-address', 'dns-feed', 'ice-mux', 'signal-resolve', 'signal-socket-open',
    'signal-feed', 'spare-socket'];
  const missingStages = STAGES.filter((s: string): boolean =>
    !recoveryBody.includes("'" + s + "'"));
  check('D1 every recovery abort stage token is named in the chase (the closed vocabulary)',
    missingStages.length === 0, missingStages.join(','));
  // No `break` may leave the chase without naming its stage: the ONLY one that
  // may is the N2 production gate's own (deliberate) break.
  const bodyLines = recoveryBody.split('\n');
  const silent: string[] = [];
  let breakCount = 0;
  for (let i = 0; i < bodyLines.length; i++) {
    if (!/\bbreak;/.test(bodyLines[i])) { continue; }
    breakCount++;
    const gatedBreak = bodyLines[i].includes('if (this.n2CdProductionBlocked(attemptEpoch)) { break; }');
    const reported = (bodyLines[i - 1] ?? '').indexOf('this.recoveryAbort(') > 0 ||
      (bodyLines[i - 2] ?? '').indexOf('this.recoveryAbort(') > 0 ||
      (bodyLines[i - 3] ?? '').indexOf('this.recoveryAbort(') > 0;
    if (!gatedBreak && !reported) { silent.push(bodyLines[i].trim()); }
  }
  check('D2 no silent `break` survives: 13 instrumented aborts + the N2 gate break',
    breakCount === 14 && silent.length === 0 &&
    recoveryBody.includes('if (this.n2CdProductionBlocked(attemptEpoch)) { break; }') &&
    (recoveryBody.match(/this\.recoveryAbort\(/g) ?? []).length === 13,
    JSON.stringify({ breakCount, silent }));
  check('D3 the abort is persisted through the diagnostics/tee path, not only printed',
    extText.includes('private recoveryAbort(stage: string, netId: number, epoch: number): void {') &&
    extText.includes("this.diagLifecycleMarker('VPN_NETWORK_RECOVERY_ABORT', `stage=${stage}`)") &&
    extText.includes("`VPN_NETWORK_RECOVERY_ABORT|stage=${stage}|netId=${String(netId)}|`"));
  check('D4 `networkDirty` is still the recovery bookkeeping and is cleared only by a COMPLETE leg',
    (recoveryBody.match(/this\.networkDirty = false;/g) ?? []).length === 1 &&
    recoveryBody.indexOf('this.networkDirty = false;') >
    recoveryBody.indexOf("this.recoveryAbort('spare-socket'"));
  check('D5 the watcher tick owns the retry, before the status read and every early return',
    extText.indexOf('this.retryPhysicalNetworkRecovery();') >
    extText.indexOf('this.maybeCloseInterfaceStatsWindow();') &&
    extText.indexOf('this.retryPhysicalNetworkRecovery();') <
    extText.indexOf('const status: ConnectorStatusResult | null = pollStatusOrNull();'));
  check('D6 the retry can never overlap, gate, or fire onto a gone network',
    extText.includes('this.recreateInFlight || !this.networkRecoveryReady) {') &&
    extText.includes('this.n2CdProductionBlocked() || physicalDefaultNetId() === 0') &&
    extText.includes('if (!this.recoveryRetryArmed && !unhealthy) {'));
  // 0.0.34 N1 (protected-DNS outage REPORT.md §5): the retry is HEALTH-gated —
  // the edge latch is a reason to try, never the only one — and the socket gate
  // stays the instantaneous platform probe (0.0.28 F1 is NOT regressed).
  check('D7 the retry gate is health-gated (N1) and the supply gate stays instantaneous',
    extText.includes('private connectorNeedsRecovery(): boolean {') &&
    extText.includes("return this.connectorStateNow === 'reconnecting' || this.connectorStateNow === 'failed';") &&
    extText.includes('this.noteConnectorHealth(status);') &&
    extText.includes('const unhealthy: boolean = this.connectorNeedsRecovery() && this.connectorDownSinceMs > 0 &&') &&
    extText.includes('networkUnusableNow: physicalDefaultNetId() === 0') &&
    !policyText.includes('recoveryRetryArmed') && !policyText.includes('gate.recoveryRetryArmed'));

  // -------------------------------------------------------------------------
  // E. F4: upstream's callback semantics, as far as the HarmonyOS API allows.
  // -------------------------------------------------------------------------
  // (i) the first observation after subscribe is not a real transition.
  seedCellular(314, '10.41.63.59');
  resetSupply();
  resetLog();
  const firstSeen = recoveryAbility(mod, 101, FakeConnection);
  firstSeen['networkFirstObservation'] = true;
  (firstSeen['physicalNetworkAvailable'] as (e: string, en: number, n: number) => void)(
    'netAvailable', 314, 314);
  check('E1 the first observation after subscribe is not a switch (upstream: "not a real transition")',
    firstSeen['networkDirty'] === false && firstSeen['networkEpoch'] === 1 &&
    lastLine('VPN_NETWORK_EVENT|').indexOf('|switched=false|action=unchanged') > 0,
    lastLine('VPN_NETWORK_EVENT|'));
  check('E2 it consumed the latch: the NEXT observation is a switch',
    (() => {
      (firstSeen['physicalNetworkAvailable'] as (e: string, en: number, n: number) => void)(
        'netAvailable', 314, 314);
      return firstSeen['networkFirstObservation'] === false &&
        lastLine('VPN_NETWORK_EVENT|').indexOf('|switched=true|action=redial') > 0;
    })(), lastLine('VPN_NETWORK_EVENT|'));

  // (ii) a capability change on the DEFAULT handle is a trigger even when the
  // netId did not move, deduped on the capability set; it reports the change to
  // the core and touches NO epoch / latch / recovery.
  seedCellular(314, '10.41.63.59');
  const capAbility = recoveryAbility(mod, 314, FakeConnection);
  capAbility['networkCapSignature'] = '12';
  capAbility['pendingCapSignature'] = '12,15';
  G['__NB_PHYSICAL_CHANGED'] = 0;
  G['__NB_PHYSICAL_CHANGED_ARG'] = undefined;
  resetSupply();
  resetLog();
  (capAbility['physicalNetworkAvailable'] as (e: string, en: number, n: number) => void)(
    'netCapabilitiesChange', 314, 314);
  const capLine = lastLine('VPN_NETWORK_EVENT|');
  check('E3 a capability change on the default handle IS a trigger (same netId, new capability set)',
    Number(G['__NB_PHYSICAL_CHANGED'] ?? 0) === 1 &&
    capLine.indexOf('|switched=false|action=capability') > 0 &&
    capAbility['networkDirty'] === false && capAbility['networkEpoch'] === 1,
    JSON.stringify({ changed: G['__NB_PHYSICAL_CHANGED'], line: capLine }));
  check('E4 the same capability set again is deduped (no repeat notification)',
    (() => {
      (capAbility['physicalNetworkAvailable'] as (e: string, en: number, n: number) => void)(
        'netCapabilitiesChange', 314, 314);
      return Number(G['__NB_PHYSICAL_CHANGED'] ?? 0) === 1 &&
        lastLine('VPN_NETWORK_EVENT|').indexOf('|action=unchanged') > 0;
    })(), lastLine('VPN_NETWORK_EVENT|'));
  check('E5 the subscription carries the capability set and arms the first-observation latch',
    extText.includes("this.schedulePhysicalNetworkCheck('netCapabilitiesChange', info.netHandle.netId,") &&
    extText.includes('capabilitySignature(capabilityNumbers(info.netCap.networkCap))') &&
    extText.includes('this.networkFirstObservation = true;') &&
    extText.includes('isPhysicalNetworkSwitch(this.networkNetId, netId)'));
  check('E6 the difference from upstream is documented (no handle id beyond netId)',
    extText.includes('HarmonyOS exposes no handle id beyond the') ||
    extText.includes('HarmonyOS exposes no handle id beyond'));

  // -------------------------------------------------------------------------
  // F. F3: the core's DNS list is never emptied by a network change.
  // -------------------------------------------------------------------------
  const argRaw = String(G['__NB_PHYSICAL_CHANGED_ARG'] ?? '');
  check('F1 the core network-change seam carries a RESOLVED address (recorded from E3)',
    argRaw.indexOf('203.0.113.9:33073') > 0 && argRaw !== '""' &&
    argRaw !== 'undefined', argRaw);
  check('F2 the empty-address call is gone from the shell (one seam, always addressed)',
    !extText.includes("connectorPhysicalNetworkChanged('')") &&
    extText.includes('connectorPhysicalNetworkChanged(this.managementDialAddr)') &&
    (extText.match(/connectorPhysicalNetworkChanged\(/g) ?? []).length === 1);
  check('F3 the address is remembered at the start leg and refreshed by the recovery',
    extText.includes('this.managementDialAddr = connectAddr;') &&
    extText.includes('this.managementDialAddr = addr;'));
  check('F4 the core no longer wipes the resolver lists inside physical_network_changed',
    !coreText.includes('if management_addr.is_none() { supply.set_dns_servers(Vec::new()); }') &&
    !coreText.includes('} else { Vec::new() };'));

  // -------------------------------------------------------------------------
  // G. F5: the extension owns network handling; the UI is not required.
  // -------------------------------------------------------------------------
  check('G1 the observer, the decision and the recovery all live in the extension',
    extText.includes('private watchPhysicalNetwork(): void {') &&
    extText.includes('private physicalNetworkAvailable(event: string, eventNetId: number, netId: number): void {') &&
    extText.includes('private async recoverPhysicalNetwork(): Promise<void> {'));
  check('G2 the UI page owns NO part of the network path (no watch, no gate, no recovery)',
    !uiText.includes('createNetConnection') && !uiText.includes('physicalNetworkAvailable') &&
    !uiText.includes('recoverPhysicalNetwork') &&
    !uiText.includes('connectorPhysicalNetworkChanged') &&
    !uiText.includes('connectorWgSocketFeed'));
  check('G3 the switch/recovery milestones above were produced by the EXTENSION instance, with the ' +
    'UI page module never imported by this harness',
    recoveredC1.indexOf('netId=314') > 0 && !uiText.includes('recoverPhysicalNetwork') &&
    !uiText.includes('watchPhysicalNetwork'));

  // -------------------------------------------------------------------------
  // H. N1 (0.0.34, protected-DNS outage REPORT.md §5): ONE empty resolution
  // must not end the recovery. Window 1's 79 aborts each re-armed and retried;
  // window 3 (18 m 13 s) never even tried because the edge latch was clear.
  // -------------------------------------------------------------------------
  seedCellular(514, '10.41.63.59');
  resetSupply();
  // A recovery started by an earlier group can still be in flight (its
  // resolver await is a 100 ms poll), so let it land before the log is read.
  await sleepMs(300);
  resetLog();
  G['__NB_DNS_POLL'] = { ok: true, pending: false, ip: '' };
  const n1 = recoveryAbility(mod, 101, FakeConnection);
  n1['lanRoamArmed'] = false;
  (n1['physicalNetworkAvailable'] as (e: string, en: number, n: number) => void)(
    'netAvailable', 514, 514);
  await sleepMs(300);
  const h1Aborts = linesWith('VPN_NETWORK_RECOVERY_ABORT|')
    .filter((l: string): boolean => fieldOf(l, 'netId') === '514');
  check('H1 an empty resolution aborts ONE attempt and names it (attempt=1, elapsedMs present)',
    h1Aborts.length === 1 && h1Aborts[0].indexOf('|stage=protected-dns|') > 0 &&
    fieldOf(h1Aborts[0], 'attempt') === '1' && Number(fieldOf(h1Aborts[0], 'elapsedMs')) >= 0,
    JSON.stringify(h1Aborts));
  check('H2 the failure RE-ARMS the retry: a failed recovery is not the last one',
    n1['recoveryRetryArmed'] === true && n1['networkDirty'] === true,
    JSON.stringify({ armed: n1['recoveryRetryArmed'], dirty: n1['networkDirty'] }));
  const hTick = watcherTick(n1);
  G['__NB_DNS_POLL'] = { ok: true, pending: false, ip: '203.0.113.7' };
  resetSupply();
  resetLog();
  hTick();
  await sleepMs(400);
  const hRecovered = lastLine('VPN_NETWORK_RECOVERED|');
  check('H3 the NEXT tick retries and recovers: the empty resolution did not end the outage',
    hRecovered.indexOf('netId=514') > 0 && fieldOf(hRecovered, 'attempt') === '2' &&
    Number(fieldOf(hRecovered, 'elapsedMs')) >= 0 && n1['recoveryRetryArmed'] === false &&
    supplyOf('WG_FEED') === 1,
    JSON.stringify({ recovered: hRecovered, armed: n1['recoveryRetryArmed'],
      wg: supplyOf('WG_FEED') }));

  // The window-3 shape: the previous recovery COMPLETED (so the edge latch is
  // clear) and the connector sits in `reconnecting`. The health gate alone must
  // still retry — this is the 18 m 13 s that had ZERO attempts.
  seedCellular(514, '10.41.63.59');
  resetSupply();
  resetLog();
  G['__NB_DNS_POLL'] = { ok: true, pending: false, ip: '203.0.113.7' };
  const health = recoveryAbility(mod, 514, FakeConnection);
  health['lanRoamArmed'] = false;
  const healthTick = watcherTick(health);
  health['networkDirty'] = false;
  health['recoveryRetryArmed'] = false;
  health['connectorStateNow'] = 'reconnecting';
  health['connectorDownSinceMs'] = Date.now() - (RETRY_MIN_MS + 5000);
  healthTick();
  await sleepMs(400);
  check('H4 the health gate retries with NO edge latch (window 3: zero attempts -> one recovery)',
    lastLine('VPN_NETWORK_RECOVERED|').indexOf('netId=514') > 0 && supplyOf('WG_FEED') === 1,
    JSON.stringify({ recovered: lastLine('VPN_NETWORK_RECOVERED|'), wg: supplyOf('WG_FEED') }));

  // N1(ii): "connected and traffic flows" resolves the retry CHEAPLY — the
  // attempt opens no socket, resolves nothing and re-arms nothing.
  resetSupply();
  resetLog();
  health['networkDirty'] = false;
  health['recoveryRetryArmed'] = true;
  health['connectorStateNow'] = 'connected';
  health['connectorFlowingNow'] = true;
  health['connectorDownSinceMs'] = -1;
  health['recoveryRetryAtMs'] = 0;
  healthTick();
  await sleepMs(200);
  const flowingLine = lastLine('VPN_NETWORK_RECOVERY_ATTEMPT|');
  check('H5 a connected, flowing tunnel resolves the retry cheaply (no socket, no resolve, no rebuild)',
    flowingLine.indexOf('|result=already-flowing') > 0 && supplyOf('WG_OPEN') === 0 &&
    supplyOf('DNS_RESOLVE') === 0 && health['recoveryRetryArmed'] === false,
    JSON.stringify({ flowing: flowingLine, wg: supplyOf('WG_OPEN'), dns: supplyOf('DNS_RESOLVE') }));

  // -------------------------------------------------------------------------
  // I. N5 (0.0.34): the protect mark is OBSERVED at handoff. The SDK has no
  // read-back API (only protect()/protectProcessNet()/bindSocket() — all
  // write-side), so the observable half is: the generation the protect was
  // issued at, read back against the live one, and a re-protect when it moved.
  // -------------------------------------------------------------------------
  seedCellular(514, '10.41.63.59');
  resetSupply();
  resetLog();
  G['__NB_DNS_POLL'] = { ok: true, pending: false, ip: '203.0.113.7' };
  G['__NB_PROTECTED'] = 0;
  const marked = recoveryAbility(mod, 514, FakeConnection);
  marked['networkEpoch'] = 7;
  parkProtectAt(1);
  const markRequest = (marked['fulfillSocketRequest'] as (r: unknown) => Promise<void>)(
    { id: 'mark-d', kind: 'udp', purpose: 'dns' });
  await waitProtectParked();
  marked['networkEpoch'] = 8; // the platform moved the generation under the await
  releaseParkedProtect();
  await markRequest;
  const markLine = lastLine('VPN_PROTECT_MARK|');
  const replyLast = (G['__NB_SUPPLY_REPLY_LAST'] ?? {}) as Record<string, unknown>;
  check('I1 a generation that moved under the await is RE-PROTECTED before handoff',
    markLine.indexOf('|mark=reprotect-ok') > 0 && markLine.indexOf('|protectGeneration=7') > 0 &&
    markLine.indexOf('|liveGeneration=8') > 0 && markLine.indexOf('|readback=none') > 0 &&
    Number(G['__NB_PROTECTED'] ?? 0) === 2 && replyLast['result'] === 'protected',
    JSON.stringify({ mark: markLine, protects: G['__NB_PROTECTED'], reply: replyLast }));

  seedCellular(514, '10.41.63.59');
  resetSupply();
  resetLog();
  G['__NB_DNS_POLL'] = { ok: true, pending: false, ip: '203.0.113.7' };
  G['__NB_PROTECTED'] = 0;
  const steady = recoveryAbility(mod, 514, FakeConnection);
  steady['networkEpoch'] = 5;
  await (steady['fulfillSocketRequest'] as (r: unknown) => Promise<void>)(
    { id: 'mark-e', kind: 'udp', purpose: 'dns' });
  const steadyLine = lastLine('VPN_PROTECT_MARK|');
  check('I2 an unmoved generation is handed over as-is: one protect, mark=generation-match',
    steadyLine.indexOf('|mark=generation-match') > 0 && Number(G['__NB_PROTECTED'] ?? 0) === 1,
    JSON.stringify({ mark: steadyLine, protects: G['__NB_PROTECTED'] }));
  check('I3 the write-only SDK surface is cited where the observable half lives',
    extText.includes('@ohos.net.vpnExtension.d.ts:244') &&
    extText.includes('@ohos.net.connection.d.ts:1710') && extText.includes('readback=none'));

  // -------------------------------------------------------------------------
  // J. N2/N3 (0.0.34): the core-side chain the shell now relies on. The
  // behavioural tests live in `client/core/src/protected_dns.rs`; here only the
  // invariants a shell fixture can read are pinned.
  // -------------------------------------------------------------------------
  const pdnsText = readFileSync(join(HERE, '..', '..', 'core', 'src', 'protected_dns.rs'), 'utf8');
  const iceText = readFileSync(join(HERE, '..', '..', 'core', 'src', 'ice.rs'), 'utf8');
  check('J1 the chain is ordered protected -> protected-retry -> os-resolver -> last-known-good',
    pdnsText.indexOf('source=protected-retry') > 0 && pdnsText.indexOf('source=os-resolver') > 0 &&
    pdnsText.indexOf('source=last-known-good') > 0 &&
    pdnsText.indexOf('source=protected-retry') < pdnsText.indexOf('source=os-resolver') &&
    pdnsText.indexOf('source=os-resolver') < pdnsText.indexOf('source=last-known-good') &&
    pdnsText.indexOf('source=none') > pdnsText.indexOf('source=last-known-good'));
  check('J2 a syscall-level failure names the call AND keeps the raw OS error (N3)',
    pdnsText.includes('fn syscall_error(call: &str, error: io::Error) -> io::Error {') &&
    pdnsText.includes('dns:{call}-failed errno={errno} ({error})'));
  check('J3 the OS resolver is no longer a hard OHOS error (N2b)',
    !iceText.includes('return Err(fail("stun:protected-dns-required".into()));'));
} catch (error) {
  failedCount++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(join(tmpdir(), 'nb-net-recovery-'), { recursive: true, force: true });
}

console.log('');
console.log(`${passed} passed, ${failedCount} failed`);
process.exit(failedCount === 0 ? 0 : 1);
