// N13-D1 relay_enabled pass-through self-check (host-side, no device, no
// network, no build).
//
// Same principle as run.mts: the REAL production module
// client/entry/src/main/ets/vpnextensionability/NetBirdConnector.ets is
// copied byte-for-byte into an OS-temp fixture (.ts) at run time — the code
// under test is the shipped code, never a hand-held mirror. The file imports
// @ohos.* / libnetbird_core.so, which plain node cannot resolve, so
// relay-flag-hooks.mjs short-circuits ONLY those specifiers to empty stubs
// (registered via node:module register, in-process synchronous hooks are
// unnecessary here).
//
// Asserts the relay_enabled branch of buildConnectorConfigJson +
// parseDeviceConfig (default-ON semantics, official client parity):
//   (1) missing/true  -> output JSON has 'relay_enabled': true
//   (2) false         -> output JSON has 'relay_enabled': false
//   (3) malformed (string/number/null) -> treated as the enabled default,
//       no throw
//
// Plus the decision B bridge increment (same byte-identical fixture):
//   (4) buildConnectorConfigJson 3rd optional arg `routeSelection`: omitted /
//       undefined / null stays BYTE-IDENTICAL to the frozen two-arg document;
//       provided is appended last as a real JSON OBJECT (never a string-encoded
//       document) — asserted byte-exactly, including the empty selector
//   (5) connectorRoutes(): the `connector_routes` getter decode against a mock
//       native export (relay-flag-hooks.mjs) — a valid document yields the
//       typed catalog with unknown fields dropped, `available:false` stays an
//       honest catalog, and every malformed document / native throw / missing
//       export is a STABLE null (never a throw, never a half-believed catalog).
//       @ohos.hilog stays an EMPTY stub here, so any log call on that path
//       would throw instead of passing silently: the decode logs no raw
//       payload and no field value.
//
// Plus the Core 5f route-ACK / runtime-setter increment (same byte-identical
// fixture, same mock core):
//   (6) connectorRouteSetApplied(routes, isRecreate, kind?): the legacy
//       two-argument native call stays BYTE-IDENTICAL — the recorded call has
//       EXACTLY two arguments and the same routes JSON — while a supplied kind
//       crosses as the THIRD argument. An EMPTY applied set crosses explicitly
//       as `{"routes":[]}` for kind 'user' (no wrapper-side short-circuit and
//       no fabricated placeholder route), and a native `ok:false` + `error`
//       answer is preserved verbatim.
//   (7) connectorRouteSelectionSet(selection): the typed selector crosses as
//       ONE argument, a real JSON object out of the three declared fields (an
//       extra caller key is dropped), a valid native answer decodes to the
//       typed RouteSelectionSetResult with unknown fields dropped, a legal
//       `ok:false` + error token is preserved (not null), and only a missing
//       export, a native throw or a malformed document answers a STABLE null.
//
// Run: node client/entry/selfcheck/run-relay-flag.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./relay-flag-hooks.mjs', import.meta.url).href);

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_DIR = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability');
const REAL = join(REAL_DIR, 'NetBirdConnector.ets');

const fixtureDir: string = mkdtempSync(join(tmpdir(), 'nb-connector-relay-'));
const fixture: string = join(fixtureDir, 'NetBirdConnector.ts');
writeFileSync(fixture, readFileSync(REAL)); // byte-identical copy of the real module
// The real module now VALUE-imports its sibling NetBirdEndpointExclusion (the
// v3.1 dns branch order), so the harness stages that file byte-identically as
// well and writes the linker dep file relay-flag-hooks.mjs redirects to: the
// real module plus shim consts for the interface names node's type stripping
// erases.
writeFileSync(join(fixtureDir, 'NetBirdEndpointExclusion.ts'),
  readFileSync(join(REAL_DIR, 'NetBirdEndpointExclusion.ets')));
writeFileSync(join(fixtureDir, 'NetBirdEndpointExclusion.dep.ts'),
  'export * from \'./NetBirdEndpointExclusion.ts\';\n' +
  'export const DnsDerivationInput = undefined;\n' +
  'export const DnsDerivationResult = undefined;\n' +
  'export const DnsNetProbe = undefined;\n' +
  'export const DnsPlatformNetAddress = undefined;\n' +
  // N2-H I2 stun caliber (§13.13): another erased type-only import of the real module
  'export const StunUriEntry = undefined;\n');

let passed: number = 0;
let failedCount: number = 0;

function check(name: string, condition: boolean, detail?: string): void {
  if (condition) {
    passed++;
    console.log(`ok   ${name}`);
  } else {
    failedCount++;
    console.log(`FAIL ${name}${detail !== undefined ? ` — ${detail}` : ''}`);
  }
}

// Minimal device config; management_url/private_key are dummy placeholders —
// no real credentials in source (credential discipline).
function device(overrides: Record<string, unknown>): Record<string, unknown> {
  return {
    management_url: 'https://mgmt.example:443',
    private_key: 'DEVICE_PRIVATE_KEY_PLACEHOLDER',
    ...overrides
  };
}

try {
  const mod = await import(pathToFileURL(fixture).href);

  // (1) missing relay_enabled -> enabled default written explicitly
  const absent = JSON.parse(mod.buildConnectorConfigJson(device({}) as never,
    false)) as Record<string, unknown>;
  check('missing flag -> relay_enabled is true',
    absent['relay_enabled'] === true, JSON.stringify(absent));

  // (1b) explicit false -> the only opt-out
  const off = JSON.parse(mod.buildConnectorConfigJson(
    device({ relay_enabled: false }) as never, false)) as Record<string, unknown>;
  check('false flag -> relay_enabled is false',
    off['relay_enabled'] === false, JSON.stringify(off));

  // (2) true -> relay_enabled: true written
  const on = JSON.parse(mod.buildConnectorConfigJson(
    device({ relay_enabled: true }) as never, false)) as Record<string, unknown>;
  check('true flag -> relay_enabled is true',
    on['relay_enabled'] === true, JSON.stringify(on));

  // (2b) other config keys untouched by the flag
  check('true flag keeps management_url',
    on['management_url'] === 'https://mgmt.example:443', JSON.stringify(on));

  // (3) malformed values -> treated as the enabled default, no throw
  for (const bad of ['false', 0, null]) {
    let threw: boolean = false;
    let parsed: Record<string, unknown> = {};
    try {
      parsed = JSON.parse(mod.buildConnectorConfigJson(
        device({ relay_enabled: bad }) as never, false)) as Record<string, unknown>;
    } catch (error) {
      threw = true;
    }
    check(`malformed flag (${JSON.stringify(bad)}) -> no throw and enabled default`,
      !threw && parsed['relay_enabled'] === true, JSON.stringify(parsed));
  }

  // parseDeviceConfig normalizes the flag the same way (no throw on garbage):
  // undefined = enabled default, false = the only opt-out.
  const normOff = mod.parseDeviceConfig(
    JSON.stringify(device({ relay_enabled: 'no' }))) as Record<string, unknown>;
  check('parseDeviceConfig: malformed flag normalized to enabled default',
    normOff['relay_enabled'] === undefined, JSON.stringify(normOff));
  const normTrue = mod.parseDeviceConfig(
    JSON.stringify(device({ relay_enabled: true }))) as Record<string, unknown>;
  check('parseDeviceConfig: true normalized to enabled default',
    normTrue['relay_enabled'] === undefined, JSON.stringify(normTrue));
  const normFalse = mod.parseDeviceConfig(
    JSON.stringify(device({ relay_enabled: false }))) as Record<string, unknown>;
  check('parseDeviceConfig: false kept as the explicit opt-out',
    normFalse['relay_enabled'] === false, JSON.stringify(normFalse));

  // -------------------------------------------------------------------------
  // (4) decision B: `route_selection` — the 3rd OPTIONAL parameter of
  // buildConnectorConfigJson. The omission default must stay byte-identical.
  // -------------------------------------------------------------------------
  const DEFAULT_BYTES: string =
    '{"management_url":"https://mgmt.example:443",' +
    '"private_key":"DEVICE_PRIVATE_KEY_PLACEHOLDER","hostname":"netbird-ohos",' +
    '"os_name":"harmonyos","os_version":"unknown","netbird_version":"0.1.0",' +
    '"relay_enabled":true}';
  const twoArg: string = mod.buildConnectorConfigJson(device({}) as never, false);
  check('route_selection omitted: two-arg bytes frozen (no drift)',
    twoArg === DEFAULT_BYTES, twoArg);
  check('route_selection omitted: undefined 3rd arg byte-identical',
    mod.buildConnectorConfigJson(device({}) as never, false, undefined) === twoArg);
  check('route_selection omitted: null 3rd arg byte-identical',
    mod.buildConnectorConfigJson(device({}) as never, false, null) === twoArg);
  check('route_selection omitted: no key written at all',
    (JSON.parse(twoArg) as Record<string, unknown>)['route_selection'] === undefined);

  // specified -> appended LAST, as a real JSON object, nothing else touched
  const oneNet = { selected_routes: ['HostInternal'], deselected_routes: [], deselect_all: false };
  const withOne: string = mod.buildConnectorConfigJson(device({}) as never, false, oneNet as never);
  const WITH_ONE_BYTES: string = DEFAULT_BYTES.slice(0, -1) +
    ',"route_selection":{"selected_routes":["HostInternal"],"deselected_routes":[],' +
    '"deselect_all":false}}';
  check('route_selection specified: exact bytes / key order',
    withOne === WITH_ONE_BYTES, withOne);
  check('route_selection specified: object form, escaped string absent',
    withOne.indexOf('"route_selection":{') >= 0 && withOne.indexOf('\\"') < 0, withOne);
  check('route_selection specified: typed roundtrip',
    JSON.stringify((JSON.parse(withOne) as Record<string, unknown>)['route_selection']) ===
      JSON.stringify(oneNet));

  // the EMPTY selector is valid and is written like any other selection
  const emptySel = { selected_routes: [], deselected_routes: [], deselect_all: false };
  const withEmpty: string =
    mod.buildConnectorConfigJson(device({}) as never, false, emptySel as never);
  const WITH_EMPTY_BYTES: string = DEFAULT_BYTES.slice(0, -1) +
    ',"route_selection":{"selected_routes":[],"deselected_routes":[],"deselect_all":false}}';
  check('route_selection empty selector: valid, exact bytes, object form',
    withEmpty === WITH_EMPTY_BYTES && withEmpty.indexOf('"route_selection":{') >= 0, withEmpty);

  const offWithSel: string = mod.buildConnectorConfigJson(
    device({ relay_enabled: false }) as never, false, emptySel as never);
  check('route_selection additive: relay opt-out and every other key untouched',
    offWithSel === '{"management_url":"https://mgmt.example:443",' +
      '"private_key":"DEVICE_PRIVATE_KEY_PLACEHOLDER","hostname":"netbird-ohos",' +
      '"os_name":"harmonyos","os_version":"unknown","netbird_version":"0.1.0",' +
      '"relay_enabled":false,"route_selection":{"selected_routes":[],' +
      '"deselected_routes":[],"deselect_all":false}}', offWithSel);

  // -------------------------------------------------------------------------
  // (5) decision B: connectorRoutes() — the connector_routes() getter decode.
  // The mock native export lives in relay-flag-hooks.mjs and reads
  // globalThis.__NB_CORE_ROUTES at call time.
  // -------------------------------------------------------------------------
  interface RoutesFake {
    absent?: boolean;
    throw?: boolean;
    payload?: unknown;
  }
  const setRoutes = (fake: RoutesFake | undefined): void => {
    (globalThis as unknown as { __NB_CORE_ROUTES: RoutesFake | undefined }).__NB_CORE_ROUTES =
      fake;
  };

  setRoutes(undefined);
  const absentFirst = mod.connectorRoutes();
  const absentSecond = mod.connectorRoutes();
  check('connectorRoutes: export absent -> null (no throw, stable)',
    absentFirst === null && absentSecond === null,
    `${JSON.stringify(absentFirst)}/${JSON.stringify(absentSecond)}`);
  setRoutes({ absent: true });
  check('connectorRoutes: absent latch -> null', mod.connectorRoutes() === null);
  setRoutes({ throw: true });
  check('connectorRoutes: native throw -> null', mod.connectorRoutes() === null);

  const validDoc = {
    available: true,
    serial: 42,
    selection: { selected_routes: [], deselected_routes: ['HostInternal'], deselect_all: false },
    networks: [
      { net_id: 'HostInternal', networks: ['192.168.50.0/24'], candidate_count: 2,
        enabled: false, is_exit: false, unknown_row_key: 'dropped' },
      { net_id: '', networks: [], candidate_count: 1, enabled: true, is_exit: true }
    ],
    unknown_top_key: 'dropped'
  };
  const expectedCatalog = {
    available: true,
    serial: 42,
    selection: { selected_routes: [], deselected_routes: ['HostInternal'], deselect_all: false },
    networks: [
      { net_id: 'HostInternal', networks: ['192.168.50.0/24'], candidate_count: 2,
        enabled: false, is_exit: false },
      { net_id: '', networks: [], candidate_count: 1, enabled: true, is_exit: true }
    ]
  };
  setRoutes({ payload: JSON.stringify(validDoc) });
  const decoded = mod.connectorRoutes();
  check('connectorRoutes: valid document -> typed catalog, unknown fields dropped',
    JSON.stringify(decoded) === JSON.stringify(expectedCatalog), JSON.stringify(decoded));
  check('connectorRoutes: valid document -> not a throw and not null',
    decoded !== null && decoded !== undefined);

  const emptyDoc = { available: false, serial: 0, selection: emptySel, networks: [] };
  setRoutes({ payload: JSON.stringify(emptyDoc) });
  const decodedEmpty = mod.connectorRoutes();
  check('connectorRoutes: available:false stays an honest catalog, never null',
    decodedEmpty !== null && JSON.stringify(decodedEmpty) === JSON.stringify(
      { available: false, serial: 0,
        selection: { selected_routes: [], deselected_routes: [], deselect_all: false },
        networks: [] }), JSON.stringify(decodedEmpty));

  // every malformed shape is a STABLE null (refuse the WHOLE document)
  const malformedCases: [string, unknown][] = [
    ['not JSON', 'routes-not-json'],
    ['JSON null', 'null'],
    ['JSON array', '[]'],
    ['payload not a string', 42],
    ['available not boolean', JSON.stringify({ ...emptyDoc, available: 'yes' })],
    ['serial fractional', JSON.stringify({ ...emptyDoc, serial: 1.5 })],
    ['serial negative', JSON.stringify({ ...emptyDoc, serial: -1 })],
    ['selection missing', JSON.stringify({ available: true, serial: 1, networks: [] })],
    ['selection not an object', JSON.stringify({ available: true, serial: 1,
      selection: [], networks: [] })],
    ['selection lists not arrays', JSON.stringify({ available: true, serial: 1,
      selection: { selected_routes: 'x', deselected_routes: [], deselect_all: false },
      networks: [] })],
    ['selection element not a string', JSON.stringify({ available: true, serial: 1,
      selection: { selected_routes: [7], deselected_routes: [], deselect_all: false },
      networks: [] })],
    ['deselect_all not boolean', JSON.stringify({ available: true, serial: 1,
      selection: { selected_routes: [], deselected_routes: [], deselect_all: 'no' },
      networks: [] })],
    ['networks not an array', JSON.stringify({ ...emptyDoc, networks: {} })],
    ['network row not an object', JSON.stringify({ ...emptyDoc, networks: ['x'] })],
    ['net_id not a string', JSON.stringify({ ...emptyDoc, networks: [
      { net_id: 1, networks: [], candidate_count: 1, enabled: true, is_exit: false }] })],
    ['prefixes not strings', JSON.stringify({ ...emptyDoc, networks: [
      { net_id: 'a', networks: [null], candidate_count: 1, enabled: true, is_exit: false }] })],
    ['candidate_count fractional', JSON.stringify({ ...emptyDoc, networks: [
      { net_id: 'a', networks: [], candidate_count: 1.5, enabled: true, is_exit: false }] })],
    ['candidate_count negative', JSON.stringify({ ...emptyDoc, networks: [
      { net_id: 'a', networks: [], candidate_count: -1, enabled: true, is_exit: false }] })],
    ['enabled not boolean', JSON.stringify({ ...emptyDoc, networks: [
      { net_id: 'a', networks: [], candidate_count: 1, enabled: 1, is_exit: false }] })],
    ['is_exit missing', JSON.stringify({ ...emptyDoc, networks: [
      { net_id: 'a', networks: [], candidate_count: 1, enabled: true }] })]
  ];
  for (const [name, payload] of malformedCases) {
    setRoutes({ payload });
    const first = mod.connectorRoutes();
    const second = mod.connectorRoutes();
    check(`connectorRoutes: malformed (${name}) -> stable null`,
      first === null && second === null, JSON.stringify(first));
  }
  setRoutes(undefined);

  // -------------------------------------------------------------------------
  // (6) Core 5f: connectorRouteSetApplied(routes, isRecreate, kind?) — the
  // legacy two-argument native call must stay BYTE-IDENTICAL (exactly two
  // arguments and the same routes JSON), the kind-aware call must cross with
  // the kind as the THIRD argument, and an EMPTY applied set must cross
  // explicitly (no wrapper-side short-circuit, no placeholder route).
  // relay-flag-hooks.mjs records every call's raw arguments under
  // globalThis.__NB_CORE_CALLS.
  // -------------------------------------------------------------------------
  const G = globalThis as unknown as Record<string, unknown>;

  interface AckFake {
    absent?: boolean;
    throw?: boolean;
    payload?: unknown;
  }
  const setAck = (fake: AckFake | undefined): void => { G['__NB_CORE_ACK'] = fake; };
  const callsOf = (name: string): unknown[][] => {
    const all = G['__NB_CORE_CALLS'] as Record<string, unknown[][]> | undefined;
    const list = all === undefined ? undefined : all[name];
    return list === undefined ? [] : list;
  };
  const HAPPY_ACK: string = JSON.stringify({ ok: true, recreate: { count: 1 } });

  G['__NB_CORE_CALLS'] = {};
  setAck({ payload: HAPPY_ACK });
  const initialAck = mod.connectorRouteSetApplied(['0.0.0.0/0', '172.16.0.0/12'], false);
  const initialCall: unknown[] = callsOf('connector_route_set_applied')[0];
  check('connectorRouteSetApplied: legacy call passes EXACTLY two arguments',
    callsOf('connector_route_set_applied').length === 1 && initialCall.length === 2,
    JSON.stringify(initialCall));
  check('connectorRouteSetApplied: legacy 2-arg payload bytes frozen (routes JSON + boolean)',
    initialCall[0] === '{"routes":["0.0.0.0/0","172.16.0.0/12"]}' && initialCall[1] === false,
    JSON.stringify(initialCall));
  check('connectorRouteSetApplied: legacy 2-arg answer decoded verbatim',
    JSON.stringify(initialAck) === HAPPY_ACK, JSON.stringify(initialAck));

  const recreateAck = mod.connectorRouteSetApplied(['10.0.0.0/8'], true);
  const recreateCall: unknown[] = callsOf('connector_route_set_applied')[1];
  check('connectorRouteSetApplied: legacy recreate call keeps the boolean 2nd argument',
    recreateCall.length === 2 && recreateCall[0] === '{"routes":["10.0.0.0/8"]}' &&
      recreateCall[1] === true && recreateAck.ok === true, JSON.stringify(recreateCall));

  const undefinedKindAck = mod.connectorRouteSetApplied(['10.0.0.0/8'], false, undefined);
  const undefinedKindCall: unknown[] = callsOf('connector_route_set_applied')[2];
  check('connectorRouteSetApplied: explicit undefined kind still calls the legacy 2-arg entry',
    undefinedKindCall.length === 2 && undefinedKindAck.ok === true,
    JSON.stringify(undefinedKindCall));

  const userAck = mod.connectorRouteSetApplied(['0.0.0.0/0'], false, 'user');
  const userCall: unknown[] = callsOf('connector_route_set_applied')[3];
  check('connectorRouteSetApplied: kind user crosses as the THIRD argument',
    userCall.length === 3 && userCall[0] === '{"routes":["0.0.0.0/0"]}' &&
      userCall[1] === false && userCall[2] === 'user' && userAck.ok === true,
    JSON.stringify(userCall));

  const emptyUserAck = mod.connectorRouteSetApplied([], false, 'user');
  const emptyUserCall: unknown[] = callsOf('connector_route_set_applied')[4];
  const emptyUserPayload: string = emptyUserCall[0] as string;
  check('connectorRouteSetApplied: EMPTY routes + kind user pass through as {"routes":[]}',
    emptyUserCall.length === 3 && emptyUserPayload === '{"routes":[]}' &&
      emptyUserCall[2] === 'user' && emptyUserAck.ok === true, JSON.stringify(emptyUserCall));
  check('connectorRouteSetApplied: empty routes never fabricate a placeholder route',
    emptyUserPayload.indexOf('0.0.0.0/0') < 0, emptyUserPayload);

  setAck({ payload: JSON.stringify({ ok: false, error: 'route-set-invalid' }) });
  const refusedAck = mod.connectorRouteSetApplied([], false, 'user');
  check('connectorRouteSetApplied: native ok:false + error preserved verbatim',
    refusedAck.ok === false && refusedAck.error === 'route-set-invalid',
    JSON.stringify(refusedAck));

  // -------------------------------------------------------------------------
  // (7) Core 5f: connectorRouteSelectionSet(selection) — the runtime setter.
  // One argument, the typed selector serialized as a real JSON object out of
  // the three declared fields; a valid native answer decodes to the typed
  // result; a legal ok:false + error token is preserved; only a missing/absent
  // export, a native throw or a malformed document answers null.
  // -------------------------------------------------------------------------
  interface SelectionSetFake {
    absent?: boolean;
    throw?: boolean;
    payload?: unknown;
  }
  const setSelectionSet = (fake: SelectionSetFake | undefined): void => {
    G['__NB_CORE_SELECTION_SET'] = fake;
  };
  const ONE_SELECTION = { selected_routes: ['HostInternal'], deselected_routes: [],
    deselect_all: false };
  const ONE_SELECTION_JSON: string = '{"selected_routes":["HostInternal"],' +
    '"deselected_routes":[],"deselect_all":false}';

  setSelectionSet(undefined);
  const setAbsentFirst = mod.connectorRouteSelectionSet(ONE_SELECTION as never);
  const setAbsentSecond = mod.connectorRouteSelectionSet(ONE_SELECTION as never);
  check('connectorRouteSelectionSet: export absent -> null (no throw, stable)',
    setAbsentFirst === null && setAbsentSecond === null,
    `${JSON.stringify(setAbsentFirst)}/${JSON.stringify(setAbsentSecond)}`);
  setSelectionSet({ absent: true });
  check('connectorRouteSelectionSet: absent latch -> null',
    mod.connectorRouteSelectionSet(ONE_SELECTION as never) === null);
  setSelectionSet({ throw: true });
  check('connectorRouteSelectionSet: native throw -> null',
    mod.connectorRouteSelectionSet(ONE_SELECTION as never) === null);

  const HAPPY_SET: string = JSON.stringify({ ok: true, available: true, changed: true,
    desired_routes: ['192.168.50.0/24'], recreate_needed: false });
  G['__NB_CORE_CALLS'] = {};
  setSelectionSet({ payload: HAPPY_SET });
  const decodedSet = mod.connectorRouteSelectionSet(ONE_SELECTION as never);
  const setCalls: unknown[][] = callsOf('connector_route_selection_set');
  const setCall: unknown[] = setCalls[0];
  check('connectorRouteSelectionSet: ONE argument, the typed selector as a real JSON object',
    setCalls.length === 1 && setCall.length === 1 && setCall[0] === ONE_SELECTION_JSON,
    JSON.stringify(setCall));
  check('connectorRouteSelectionSet: object form, never a string-encoded document',
    (setCall[0] as string).charAt(0) === '{' && (setCall[0] as string).indexOf('\\"') < 0,
    setCall[0] as string);
  check('connectorRouteSelectionSet: valid answer -> typed result, extra native field dropped',
    decodedSet !== null && JSON.stringify(decodedSet) === HAPPY_SET,
    JSON.stringify(decodedSet));

  const withExtraSet = mod.connectorRouteSelectionSet(
    { selected_routes: ['HostInternal'], deselected_routes: [], deselect_all: false,
      unknown_caller_key: 'dropped' } as never);
  const extraSetCall: unknown[] = callsOf('connector_route_selection_set')[1];
  check('connectorRouteSelectionSet: extra caller key never crosses the bridge',
    extraSetCall[0] === ONE_SELECTION_JSON && withExtraSet !== null,
    JSON.stringify(extraSetCall));

  for (const token of ['no-connector', 'selection-invalid', 'wg-apply-failed']) {
    setSelectionSet({ payload: JSON.stringify({ ok: false, error: token, available: true,
      changed: false, desired_routes: [], recreate_needed: true }) });
    const refusedSet = mod.connectorRouteSelectionSet(ONE_SELECTION as never);
    check(`connectorRouteSelectionSet: ok:false + error ${token} preserved (not null)`,
      refusedSet !== null && refusedSet.ok === false && refusedSet.error === token &&
        refusedSet.available === true && refusedSet.changed === false &&
        refusedSet.desired_routes.length === 0 && refusedSet.recreate_needed === true,
      JSON.stringify(refusedSet));
  }

  const malformedSetCases: [string, unknown][] = [
    ['not JSON', 'selection-not-json'],
    ['JSON null', 'null'],
    ['JSON array', '[]'],
    ['payload not a string', 42],
    ['ok not boolean', JSON.stringify({ ok: 'yes', available: true, changed: false,
      desired_routes: [], recreate_needed: false })],
    ['available not boolean', JSON.stringify({ ok: true, available: 1, changed: false,
      desired_routes: [], recreate_needed: false })],
    ['changed not boolean', JSON.stringify({ ok: true, available: true, changed: null,
      desired_routes: [], recreate_needed: false })],
    ['recreate_needed not boolean', JSON.stringify({ ok: true, available: true, changed: false,
      desired_routes: [], recreate_needed: 'no' })],
    ['desired_routes not an array', JSON.stringify({ ok: true, available: true, changed: false,
      desired_routes: {}, recreate_needed: false })],
    ['desired_routes element not a string', JSON.stringify({ ok: true, available: true,
      changed: false, desired_routes: [7], recreate_needed: false })],
    ['ok:false without an error token', JSON.stringify({ ok: false, available: true,
      changed: false, desired_routes: [], recreate_needed: true })],
    ['ok:false with an unknown error token', JSON.stringify({ ok: false, error: 'boom',
      available: true, changed: false, desired_routes: [], recreate_needed: true })]
  ];
  for (const [name, payload] of malformedSetCases) {
    setSelectionSet({ payload });
    const first = mod.connectorRouteSelectionSet(ONE_SELECTION as never);
    const second = mod.connectorRouteSelectionSet(ONE_SELECTION as never);
    check(`connectorRouteSelectionSet: malformed (${name}) -> stable null`,
      first === null && second === null, JSON.stringify(first));
  }
  setSelectionSet(undefined);
  setAck(undefined);
} catch (error) {
  failedCount++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(fixtureDir, { recursive: true, force: true });
}

console.log(`\n${passed} passed, ${failedCount} failed`);
process.exit(failedCount === 0 ? 0 : 1);