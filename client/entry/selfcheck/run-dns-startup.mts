// Management DNS startup recovery: byte-identical production VPE and Connector,
// existing platform/native shims, injected DNS and timers; no real long sleeps.
import { readFileSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { register } from 'node:module';

register(new URL('./exclusion-generation-hooks.mjs', import.meta.url).href);
const HERE = dirname(fileURLToPath(import.meta.url));
const ETS = join(HERE, '..', 'src', 'main', 'ets');
// All generated artifacts stay inside this worktree.
mkdirSync(join(HERE, 'build'), { recursive: true });
const dir = mkdtempSync(join(HERE, 'build', 'dns-startup-'));
const G = globalThis as unknown as Record<string, any>;
const STAGED = ['NetBirdVpnExtensionAbility', 'NetBirdConnector', 'NetBirdVpnConfig',
  'NetBirdRouteValidation', 'NetBirdEndpointExclusion', 'NetBirdInterfaceStatsProbe',
  'NetBirdPhysicalNetworkPolicy', 'NetBirdStatusPolicy', 'NetBirdUiModel',
  'NetBirdRecreatePolicy', 'NetBirdDiagnosticsCore', 'NetBirdN2Stimulus',
  'NetBirdN2AEvent', 'NetBirdN2Sandwich', 'NetBirdN2ControlRequest',
  'NetBirdN2CdPlan', 'NetBirdN2Ring'];
const TYPES: Record<string, string[]> = {
  NetBirdConnector: ['ConnectorStatusResult', 'CoreNetworkConfig', 'CoreRouteEntry',
    'ConnectorStartResult', 'ConnectorLogoutResult', 'DeviceConfigFile', 'ManagementEndpoint',
    'NativeTcpSocketOpen', 'RelayAdvertisedView', 'RouteSetAppliedResult', 'SocketFeedResult',
    'SocketSupplyRequest', 'TunReleaseResult', 'WgFeedResult', 'IceMuxArmResult',
    'PhysicalBearerFacts', 'PhoneLogConfigureResult', 'PhoneLogSummary'],
  NetBirdUiModel: ['UiNetworkConfig', 'UiStatusDocument'],
  NetBirdEndpointExclusion: ['ExcludedRouteEntry', 'EndpointResolution',
    'ExclusionBuildResult', 'ExclusionEndpoint', 'RelayEndpoint', 'DnsDerivationInput',
    'DnsDerivationResult', 'DnsNetProbe', 'DnsPlatformNetAddress', 'StunDerivationResult',
    'StunUriEntry', 'StunParseResult', 'StunEndpoint'],
  NetBirdVpnConfig: ['VpnRouteConfig', 'VpnTunnelConfig', 'TunnelSubnetRouteOutcome',
    'CaptureRouteOutcome'],
  NetBirdRouteValidation: ['RouteCheckResult', 'RouteViolation'],
  NetBirdN2Stimulus: ['N2StimulusDecision', 'N2StimulusReply'],
  NetBirdN2AEvent: ['N2AEventDecision', 'N2AEventRequest', 'N2AEventReply'],
  NetBirdN2Sandwich: ['N2SandwichPost'],
  NetBirdN2ControlRequest: ['N2ControlDecision', 'N2ControlRequest'],
  NetBirdN2CdPlan: ['N2CdPlanDecision', 'N2CdPlanRequest', 'N2BearerFacts'],
  NetBirdN2Ring: ['N2RingDumpDecision', 'N2RingDumpRequest', 'N2RingAckDecision', 'N2RingAckRequest']
};
let passed = 0;
function check(label: string, ok: boolean): void {
  if (!ok) { throw new Error(label); }
  passed++;
  console.log('ok   ' + label);
}
const realNow = Date.now;
const realTimeout = globalThis.setTimeout;
const realClearTimeout = globalThis.clearTimeout;
const realInterval = globalThis.setInterval;
const realClearInterval = globalThis.clearInterval;
let now = 1000000;
let nextId = 0;
const timers = new Map<number, { at: number, fn: () => void }>();
const waits: number[] = [];
async function flush(): Promise<void> {
  for (let i = 0; i < 50; i++) { await Promise.resolve(); }
}
function fireNext(): void {
  const first = [...timers.entries()].sort((a, b) => a[1].at - b[1].at)[0];
  if (!first) { throw new Error('startup stalled without a timer'); }
  timers.delete(first[0]);
  now = first[1].at;
  first[1].fn();
}
try {
  for (const name of STAGED) {
    writeFileSync(join(dir, name + '.ts'), readFileSync(join(ETS, 'vpnextensionability', name + '.ets')));
  }
  for (const name of ['NetBirdRouteStore', 'NetBirdRoutePreferences']) {
    writeFileSync(join(dir, name + '.ts'), readFileSync(join(ETS, 'routes', name + '.ets')));
  }
  for (const [name, types] of Object.entries(TYPES)) {
    writeFileSync(join(dir, name + '.shim.ts'), `export * from './${name}.ts';\n` +
      types.map(t => `export const ${t} = undefined;`).join('\n'));
  }
  writeFileSync(join(dir, 'NetBirdDiagnostics.shim.ts'),
    'export const event = (name, fields) => { globalThis.__DNS_EVENTS.push({name, fields}); return true; };\n' +
    'export const start = () => undefined; export const stop = () => undefined;\n' +
    'export const appendSystemEvent = () => false;\n');
  writeFileSync(join(dir, 'NetBirdSystemEvents.shim.ts'),
    'export const start = () => false; export const stop = () => undefined;\n' +
    'export const isRunning = () => false;\n');
  const mod = await import(pathToFileURL(join(dir, 'NetBirdVpnExtensionAbility.ts')).href);
  const bridge = await import(pathToFileURL(join(dir, 'NetBirdConnector.ts')).href);
  const ui = await import(pathToFileURL(join(dir, 'NetBirdUiModel.ts')).href);
  const diag = await import(pathToFileURL(join(dir, 'NetBirdDiagnosticsCore.ts')).href);
  const platform = (await import('@ohos.net.connection')).default;
  const vpn = (await import('@ohos.net.vpnExtension')).default;
  Date.now = () => now;
  G.setTimeout = (fn: () => void, ms: number) => {
    waits.push(ms);
    const id = ++nextId;
    timers.set(id, { at: now + ms, fn });
    return id;
  };
  G.clearTimeout = (id: number) => timers.delete(id);
  G.setInterval = () => ++nextId;
  G.clearInterval = () => undefined;
  const filesDir = join(HERE, 'build', 'virtual-files');
  const noticePath = `${filesDir}/netbird-connect-failure`;
  let calls: number[] = [];
  function reset(url = 'http://mgmt.test:33073'): any {
    now = 1000000; timers.clear(); waits.length = 0; calls = [];
    G.__NB_FS = {
      [`${filesDir}/netbird-setup-key`]: 'fixture-setup-key',
      [`${filesDir}/netbird-device-config.json`]: JSON.stringify({ management_url: url, private_key: 'fixture-key' })
    };
    G.__NB_LOG = []; G.__DNS_EVENTS = []; G.__NB_FS_OPEN_ERROR = {};
    G.__NB_START_CALLS = 0; G.__NB_SUPPLY_MGMT_OPEN = 0; G.__NB_PROTECTED = [];
    G.__NB_STATUS = { running: false, state: 'connecting', peer_count: 0, route_count: 0 };
    G.__NB_NETCFG = { available: true, serial: 1, address: '100.64.0.1',
      address_prefix_len: 16, routes: [], stuns: [] };
    const ability = new mod.default();
    ability.context = { filesDir };
    ability.requestId = 'req-dns';
    ability.connection = vpn.createVpnConnection({});
    return ability;
  }
  function inject(answer: (n: number) => any): void {
    platform.getAddressesByName = () => { calls.push(now); return answer(calls.length); };
  }
  function networkError(): Error {
    return Object.assign(new Error('Failed to connect to the service'), { code: 2100002 });
  }
  async function run(ability: any, afterFlush?: () => void): Promise<void> {
    let done = false;
    const task = ability.startConnectorThenCreate({ parameters: {} }, ability.connection)
      .finally(() => { done = true; });
    for (let step = 0; step < 200 && !done; step++) {
      await flush();
      if (done) { break; }
      afterFlush?.();
      fireNext();
    }
    check('startup resolves within injected timer budget', done);
    await task;
  }
  check('exported retry cap is three', mod.MANAGEMENT_DNS_MAX_ATTEMPTS === 3);
  check('exported retry delays are 3s and 15s',
    mod.MANAGEMENT_DNS_RETRY_FIRST_MS === 3000 && mod.MANAGEMENT_DNS_RETRY_SECOND_MS === 15000);
  for (const successAt of [1, 2, 3]) {
    const ability = reset();
    inject(n => n < successAt ? Promise.reject(networkError()) : Promise.resolve([{ address: '203.0.113.7' }]));
    await run(ability);
    check(`success on attempt ${successAt}: exact DNS calls`, calls.length === successAt);
    check(`success on attempt ${successAt}: each attempt logs its ordinal`,
      G.__NB_LOG.filter((line: string) => line.includes('VPN_CONNECTOR_DNS|')).length === successAt);
    check(`success on attempt ${successAt}: real management socket/protect/connector chain`,
      G.__NB_SUPPLY_MGMT_OPEN >= 1 && G.__NB_PROTECTED.length >= 1 && G.__NB_START_CALLS === 1);
    check(`success on attempt ${successAt}: real continuation reaches later signal gate`,
      bridge.readConnectFailure(filesDir)?.error_class === 'no-signal-uri');
    if (successAt > 1) { check('first backoff is exactly 3s', calls[1] - calls[0] === 3000); }
    if (successAt > 2) { check('second backoff is exactly 15s', calls[2] - calls[1] === 15000); }
    ability.onDestroy(); await flush();
  }
  for (const failure of ['reject', 'throw', 'empty', 'timeout']) {
    const ability = reset();
    inject(() => {
      if (failure === 'throw') { throw networkError(); }
      if (failure === 'empty') { return Promise.resolve([]); }
      if (failure === 'timeout') { return new Promise(() => {}); }
      return Promise.reject(networkError());
    });
    await run(ability);
    check(`${failure}: attempts are bounded at three`, calls.length === 3);
    check(`${failure}: exhausted path opens no management socket and starts no connector`,
      G.__NB_SUPPLY_MGMT_OPEN === 0 && G.__NB_START_CALLS === 0);
    check(`${failure}: real failure notice reader sees network class`,
      bridge.readConnectFailure(filesDir)?.error_class === 'network');
    check(`${failure}: existing UI class renders actionable text`,
      ui.connectFailureText('network') === '网络不可用或服务器无法访问，请检查网络');
    check(`${failure}: one persistent final failure and no teardown`,
      G.__DNS_EVENTS.filter((e: any) => e.name === 'vpn_start_failed').length === 1 &&
      !ability.destroyRequested && ability.destroyPromise === undefined && ability.watcherTimer < 0);
    const aborts = G.__NB_LOG.filter((line: string) => line.includes('VPN_CONNECTOR_ABORTED|') && line.includes('stage=dns'));
    check(`${failure}: request and attempt sequence retained`, aborts.length === 3 &&
      aborts.every((line: string, i: number) => line.includes('requestId=req-dns') && line.includes(`attempt=${i + 1}`)));
    const records = G.__DNS_EVENTS.filter((e: any) => e.name === 'vpn_connector_aborted')
      .map((e: any) => diag.buildRecord(new Date(now).toISOString(), 4242, 'vpn', e.name, e.fields, false));
    check(`${failure}: production diagnostic encoder preserves stage, request, count, code`,
      records.length === 3 && records.every((line: string, i: number) =>
        line.includes('stage=dns') && line.includes('detail=req-dns') &&
        line.includes(`count=${i + 1}`) && line.includes(
          `code=${failure === 'reject' || failure === 'throw' ? '2100002' : 'none'}`)));
    check(`${failure}: retry polling uses only short injected timers`,
      waits.filter(ms => ms !== 5000).every(ms => ms > 0 && ms <= 500));
    ability.onDestroy(); await flush();
  }
  for (const badUrl of ['malformed', 'http://:443']) {
    const ability = reset(badUrl);
    inject(() => { throw new Error('DNS must not run for a malformed endpoint'); });
    await run(ability);
    check('endpoint hard error does not resolve or back off', calls.length === 0 && waits.length === 0);
    check('endpoint hard error publishes existing parse class',
      bridge.readConnectFailure(filesDir)?.error_class === 'parse' && G.__NB_START_CALLS === 0);
  }
  for (const stop of ['onDestroy', 'destroyRequested', 'destroyOnce']) {
    const ability = reset();
    inject(() => Promise.reject(networkError()));
    let stopped = false;
    await run(ability, () => {
      if (stopped) { return; }
      stopped = true;
      if (stop === 'onDestroy') { ability.onDestroy(); }
      else if (stop === 'destroyOnce') { void ability.destroyOnce('fixture'); }
      else { ability.destroyRequested = true; }
    });
    check(`${stop}: waiting startup exits after one short poll`, now - calls[0] <= 500);
    check(`${stop}: no retry, failure notice, socket, or connector after cancellation`,
      calls.length === 1 && G.__NB_FS[noticePath] === undefined &&
      G.__NB_SUPPLY_MGMT_OPEN === 0 && G.__NB_START_CALLS === 0 && timers.size === 0);
  }
  for (const lateResult of ['success', 'failure']) {
    const ability = reset();
    let resolveDns: any; let rejectDns: any;
    inject(() => new Promise((resolve, reject) => { resolveDns = resolve; rejectDns = reject; }));
    const task = ability.startConnectorThenCreate({ parameters: {} }, ability.connection);
    await flush();
    ability.onDestroy();
    if (lateResult === 'success') { resolveDns([{ address: '203.0.113.7' }]); }
    else { rejectDns(networkError()); }
    await task;
    check(`destroy during DNS then ${lateResult}: no late startup or notice`,
      calls.length === 1 && G.__NB_SUPPLY_MGMT_OPEN === 0 &&
      G.__NB_START_CALLS === 0 && G.__NB_FS[noticePath] === undefined && timers.size === 0);
  }
  console.log(`${passed} passed, 0 failed`);
} finally {
  Date.now = realNow;
  globalThis.setTimeout = realTimeout;
  globalThis.clearTimeout = realClearTimeout;
  globalThis.setInterval = realInterval;
  globalThis.clearInterval = realClearInterval;
  if (dirname(dir) !== join(HERE, 'build')) { throw new Error('unexpected fixture path'); }
  rmSync(dir, { recursive: true, force: true });
}
