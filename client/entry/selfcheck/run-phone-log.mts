/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// phone_log surface + VPN-extension diagnostics wiring — host-side self-check
// (no device, no network, no HAP build).
//
// Same principle as run-relay-status-log.mts: the REAL production module
// client/entry/src/main/ets/vpnextensionability/NetBirdConnector.ets is copied
// byte-for-byte into an OS-temp fixture (.ts) at run time — the code under test
// is the shipped code, never a hand-held mirror — and phone-log-hooks.mjs
// short-circuits ONLY the unresolvable specifiers (@ohos.*, libnetbird_core.so)
// to stubs. The native phone_log_* surface is driven through
// globalThis.__NB_PHONE_LOG, so the wrapper's real branches (ok / error code /
// malformed reply / platform throw / missing export) are exercised.
//
// Pinned here:
//   (1) the four typed defensive wrappers: verbatim argument hand-off, error
//       codes instead of exceptions, `native-unavailable` for an absent export,
//       null for an unreadable status document — and NO path/session token in
//       any reply this side declares or returns;
//   (2) the once-per-minute summary: dropped/pending/failed rendering, the
//       ok/degraded/stopped/unavailable verdicts, explicit `na` for an
//       unreadable counter, and no raw path/session leakage;
//   (2b) the 2026-10-08 Rust-heap `alloc_stats` bridge: the four-counter
//       document is shape-checked and handed over verbatim, an absent export /
//       throw / empty / garbled reply is `undefined` (never a throw), and the
//       REAL pure core turns the text into KiB + counters or four `na` keys;
//   (3) the record shape against the REAL DiagnosticsCore whitelist: every
//       milestone name/field key is accepted, and the phone_log_status detail
//       survives buildRecord intact;
//   (4) the extension WIRING as source pins (the extension cannot be imported
//       under node — the same slicing approach run-logout-flow.mts /
//       run-lan-roam.mts use): start order (native tee -> diagnostics ->
//       system events), the single shared sink, the late Diagnostics.stop()
//       AFTER destroyOnce, the system-event watcher stopped in onDestroy, the
//       process-lifetime writer (phone_log_shutdown is never called) and the
//       60 s sample riding the existing watcher tick (no second timer).
//
// Run: node client/entry/selfcheck/run-phone-log.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./phone-log-hooks.mjs', import.meta.url).href);

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_DIR = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability');
const REAL = join(REAL_DIR, 'NetBirdConnector.ets');
const REAL_CORE = join(REAL_DIR, 'NetBirdDiagnosticsCore.ets');
const REAL_EXT = join(REAL_DIR, 'NetBirdVpnExtensionAbility.ets');

const fixtureDir: string = mkdtempSync(join(tmpdir(), 'nb-phone-log-'));
const fixture: string = join(fixtureDir, 'NetBirdConnector.ts');
writeFileSync(fixture, readFileSync(REAL)); // byte-identical copy of the real module
// the same sibling staging as run-relay-status-log.mts: the real module
// value-imports NetBirdEndpointExclusion, so the fixture dir needs its
// byte-identical copy plus the linker dep file phone-log-hooks.mjs redirects
// './NetBirdEndpointExclusion' to (node's type stripping erases the type-only
// interface names, hence the shim consts).
writeFileSync(join(fixtureDir, 'NetBirdEndpointExclusion.ts'),
  readFileSync(join(REAL_DIR, 'NetBirdEndpointExclusion.ets')));
writeFileSync(join(fixtureDir, 'NetBirdEndpointExclusion.dep.ts'),
  'export * from \'./NetBirdEndpointExclusion.ts\';\n' +
  'export const DnsDerivationInput = undefined;\n' +
  'export const DnsDerivationResult = undefined;\n' +
  'export const DnsNetProbe = undefined;\n' +
  'export const DnsPlatformNetAddress = undefined;\n' +
  'export const StunUriEntry = undefined;\n');
// the pure diagnostics core (no imports) drives the record/whitelist checks
const coreFixture: string = join(fixtureDir, 'NetBirdDiagnosticsCore.ts');
writeFileSync(coreFixture, readFileSync(REAL_CORE));

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

function sliceFrom(src: string, begin: string, end: string): string {
  const at = src.indexOf(begin);
  if (at < 0) { return ''; }
  const stop = src.indexOf(end, at + begin.length);
  return stop < 0 ? src.slice(at) : src.slice(at, stop);
}

// The native stub's state, read at CALL time by phone-log-hooks.mjs.
interface PhoneLogFake {
  absent: boolean;
  configure: (path: string) => string;
  append: (line: string) => string;
  status: () => string;
  shutdown: () => string;
  // 006: the connector_resource_status() document (same fake, same absent
  // latch — a missing export is the `unsupported` branch of both surfaces).
  resource: () => string;
  // 2026-10-08: the Rust-heap alloc_stats() document (same fake/latch again).
  alloc: () => string;
}

const fake: PhoneLogFake = {
  absent: false,
  configure: (): string => '{"ok":true}',
  append: (): string => '{"ok":true}',
  status: (): string => '{"component":"phone_log"}',
  shutdown: (): string => '{"ok":true}',
  resource: (): string => '{"ok":true}',
  alloc: (): string => '{"bytes":0,"peak":0,"allocs":0,"frees":0}'
};
const seen: { configure: string[]; append: string[] } = { configure: [], append: [] };
(globalThis as unknown as { __NB_PHONE_LOG: PhoneLogFake }).__NB_PHONE_LOG = fake;

const statusDoc = (over: Record<string, unknown>): string => JSON.stringify({
  component: 'phone_log', enabled: true, running: true, offered: 10, accepted: 9,
  written: 9, dropped: 0, rate_suppressed: 0, failed: 0, pending: 0, rotations: 0,
  last_error: null, ...over
});

// The EXACT native schema of connector_resource_status() (connector.rs
// `ResourceSnapshot::to_json`), overridable per test.
const resourceDoc = (over: Record<string, unknown>): string => JSON.stringify({
  ok: true,
  connector: { present: true, generation: 3, running: true, state: 'connected',
    starts_total: 4, stops_total: 1 },
  ice: { available: true, sessions_present: 2, sessions_live: 2, sessions_stopped: 0,
    live_local_sockets: 5 },
  ice_sockets: { available: true, queued: 7, taken: 12 },
  socket_supply: { installed: true, pending: 0, queued: 1 },
  relay_carrier: { available: true, attached_lanes: 2, pending_opens: 1, foreign_clients: 0 },
  dns_lookups: { pending_receivers: 0 },
  wg: { available: false, reason: 'not-collected' },
  ...over
});

// The 22 scalar leaves the event must carry, in the two records the shell
// emits: NOTHING may be missing (a > MAX_LINE_CHARS record would have been
// cut with `~` and silently lost its tail).
const RESOURCE_LEAVES: string[] = [
  'conn_present', 'conn_gen', 'conn_running', 'conn_state', 'conn_starts', 'conn_stops',
  'ice_available', 'ice_sessions', 'ice_live', 'ice_stopped', 'ice_fds',
  'icesock_ready', 'icesock_queued', 'icesock_taken',
  'supply_installed', 'supply_pending', 'supply_queued',
  'relay_ready', 'relay_lanes', 'relay_opens', 'relay_foreign', 'dns_pending'
];

try {
  const m = await import(pathToFileURL(fixture).href) as Record<string, any>;
  const core = await import(pathToFileURL(coreFixture).href) as Record<string, any>;
  const extText: string = readFileSync(REAL_EXT, 'utf8');
  const connectorText: string = readFileSync(REAL, 'utf8');

  // =========================================================================
  // (1) the four typed defensive wrappers
  // =========================================================================
  check('the wrapper surface exists with its declared types',
    typeof m.phoneLogConfigure === 'function' && typeof m.phoneLogAppend === 'function' &&
    typeof m.phoneLogStatus === 'function' && typeof m.phoneLogShutdown === 'function' &&
    connectorText.includes('export function phoneLogConfigure(path: string): PhoneLogConfigureResult') &&
    connectorText.includes('export function phoneLogAppend(line: string): PhoneLogAppendResult') &&
    connectorText.includes('export function phoneLogStatus(): PhoneLogStatusResult | null') &&
    connectorText.includes('export function phoneLogShutdown(): PhoneLogShutdownResult'));

  // -- configure: install, idempotent repeat, error code, anomalies ---------
  fake.configure = (path: string): string => {
    seen.configure.push(path);
    return JSON.stringify({
      ok: true, configured: true, idempotent: false, enabled: true, running: true,
      session: 'SESS-SENTINEL-1', path: path, queue_capacity: 256
    });
  };
  const first = m.phoneLogConfigure('/data/files/netbird-vpn.log');
  check('configure: a real install answers ok with the native state',
    first.ok === true && first.idempotent === false && first.enabled === true &&
    first.running === true, JSON.stringify(first));
  check('configure: the path is handed to native verbatim',
    seen.configure.length === 1 && seen.configure[0] === '/data/files/netbird-vpn.log',
    JSON.stringify(seen.configure));
  check('configure: the reply this side declares carries no session token and no path',
    JSON.stringify(first).indexOf('SESS-SENTINEL-1') < 0 &&
    JSON.stringify(first).indexOf('/data/files') < 0, JSON.stringify(first));
  fake.configure = (path: string): string => {
    seen.configure.push(path);
    return JSON.stringify({ ok: true, idempotent: true, enabled: true, running: true,
      session: 'SESS-SENTINEL-2', path: path });
  };
  const again = m.phoneLogConfigure('/data/files/netbird-vpn.log');
  check('configure: a same-path repeat is the idempotent answer (no second writer)',
    again.ok === true && again.idempotent === true &&
    JSON.stringify(again).indexOf('SESS-SENTINEL-2') < 0, JSON.stringify(again));
  fake.configure = (): string => '{"ok":false,"error":"shutdown-pending"}';
  const refused = m.phoneLogConfigure('/data/files/netbird-vpn.log');
  check('configure: a native refusal is a code, never a throw',
    refused.ok === false && refused.error === 'shutdown-pending', JSON.stringify(refused));
  fake.configure = (): string => '{"ok":false,"error":"invalid-path"}';
  check('configure: invalid-path is passed through as a stable code',
    m.phoneLogConfigure('no-slash.log').error === 'invalid-path');

  // -- append: accepted / not-configured / absent / malformed / platform ----
  fake.append = (line: string): string => { seen.append.push(line); return '{"ok":true}'; };
  const appended = m.phoneLogAppend('ts=x pid=1 role=vpn event=diag_start\n');
  check('append: hand-off answers ok and passes the line verbatim',
    appended.ok === true && seen.append.length === 1 &&
    seen.append[0] === 'ts=x pid=1 role=vpn event=diag_start\n', JSON.stringify(seen.append));
  fake.append = (): string => '{"ok":false,"error":"not-configured"}';
  const notConfigured = m.phoneLogAppend('x');
  check('append: an unconfigured tee is refused by code (never buffered)',
    notConfigured.ok === false && notConfigured.error === 'not-configured');
  fake.append = (): string => 'not-json';
  check('append: a non-JSON reply is a native-throw, not a throw at the caller',
    m.phoneLogAppend('x').error === 'native-throw');
  fake.append = (): string => '{"ok":"yes"}';
  check('append: a JSON reply whose ok is not a boolean is malformed-reply',
    m.phoneLogAppend('x').error === 'malformed-reply');
  fake.append = (): string => { const e = new Error('platform'); (e as any).code = 201; throw e; };
  check('append: a platform throw becomes platform-error-<code>',
    m.phoneLogAppend('x').error === 'platform-error-201');
  fake.append = (): string => { throw new Error('plain'); };
  check('append: a plain throw becomes native-throw',
    m.phoneLogAppend('x').error === 'native-throw');

  // -- status: document / absent component / absent export / throw ----------
  const leakyStatus = statusDoc({});
  const leakyParsed = JSON.parse(leakyStatus);
  leakyParsed.session = 'SESS-SENTINEL-4';
  leakyParsed.path = '/data/files/PATH-SENTINEL-4';
  fake.status = (): string => JSON.stringify(leakyParsed);
  const status = m.phoneLogStatus();
  check('status: the declared counters are returned',
    status !== null && status.component === 'phone_log' && status.dropped === 0 &&
    status.pending === 0 && status.failed === 0);
  check('status: the raw session token and path never enter the returned object',
    status !== null && JSON.stringify(status).indexOf('SENTINEL-4') < 0,
    JSON.stringify(status));
  fake.status = (): string => '{"enabled":true}';
  check('status: a document without the component identity is null (never invented)',
    m.phoneLogStatus() === null);
  fake.status = (): string => '{"component":7}';
  check('status: a non-string component is null', m.phoneLogStatus() === null);
  fake.status = (): string => { throw new Error('no native'); };
  check('status: a throw is null', m.phoneLogStatus() === null);

  // -- shutdown: shape only (the extension never calls it) ------------------
  fake.shutdown = (): string => '{"ok":true,"stopped":true,"running":false,"pending":0}';
  const shutdown = m.phoneLogShutdown();
  check('shutdown: the bounded stop reply is typed (ok/stopped/running/pending)',
    shutdown.ok === true && shutdown.stopped === true && shutdown.running === false &&
    shutdown.pending === 0);

  // -- absent native surface: every wrapper answers instead of throwing -----
  fake.absent = true;
  let threw: boolean = false;
  let absentConfigure = '';
  let absentAppend = '';
  let absentStatus: unknown = 'unset';
  let absentShutdown = '';
  try {
    absentConfigure = m.phoneLogConfigure('/data/files/netbird-vpn.log').error;
    absentAppend = m.phoneLogAppend('x').error;
    absentStatus = m.phoneLogStatus();
    absentShutdown = m.phoneLogShutdown().error;
  } catch (error) {
    threw = true;
  }
  fake.absent = false;
  check('absent export: no wrapper throws',
    threw === false);
  check('absent export: configure/append/shutdown answer native-unavailable',
    absentConfigure === 'native-unavailable' && absentAppend === 'native-unavailable' &&
    absentShutdown === 'native-unavailable',
    `${absentConfigure} / ${absentAppend} / ${absentShutdown}`);
  check('absent export: status answers null', absentStatus === null);
  const beforeEmpty = seen.configure.length;
  check('an empty configure path is refused before any native call',
    m.phoneLogConfigure('').error === 'empty-path' &&
    seen.configure.length === beforeEmpty);

  // =========================================================================
  // (1b) the 006 native resource snapshot bridge
  //   (connector_resource_status -> typed snapshot -> flat `na`-safe leaves)
  // =========================================================================
  check('the bridge surface exists with its declared types',
    typeof m.connectorResourceStatus === 'function' && typeof m.nativeResourceSample === 'function' &&
    connectorText.includes(
      'export function connectorResourceStatus(): ConnectorResourceStatus | null') &&
    connectorText.includes(
      'export function nativeResourceSample(status: ConnectorResourceStatus | null): NativeResourceSample'));

  // -- valid: the exact schema, leaf for leaf ------------------------------
  fake.resource = (): string => resourceDoc({});
  const snap = m.connectorResourceStatus();
  check('resource: a valid document is projected leaf for leaf',
    snap !== null && snap.ok === true &&
    snap.connector.present === true && snap.connector.generation === 3 &&
    snap.connector.running === true && snap.connector.state === 'connected' &&
    snap.connector.starts_total === 4 && snap.connector.stops_total === 1 &&
    snap.ice.available === true && snap.ice.sessions_present === 2 &&
    snap.ice.sessions_live === 2 && snap.ice.sessions_stopped === 0 &&
    snap.ice.live_local_sockets === 5 &&
    snap.ice_sockets.queued === 7 && snap.ice_sockets.taken === 12 &&
    snap.socket_supply.installed === true && snap.socket_supply.pending === 0 &&
    snap.socket_supply.queued === 1 &&
    snap.relay_carrier.attached_lanes === 2 && snap.relay_carrier.pending_opens === 1 &&
    snap.relay_carrier.foreign_clients === 0 &&
    snap.dns_lookups.pending_receivers === 0, JSON.stringify(snap));

  // -- arbitrary extra fields are IGNORED (never carried through) ----------
  const extraDoc = JSON.parse(resourceDoc({ peer_key: 'SENTINEL-KEY', relay_url: 'rels://SENTINEL' }));
  extraDoc.connector.secret = 'SENTINEL-SECRET';
  extraDoc.connector.fd = 4242;
  fake.resource = (): string => JSON.stringify(extraDoc);
  const extraSnap = m.connectorResourceStatus();
  check('resource: an arbitrary extra field never enters the projected snapshot',
    extraSnap !== null && JSON.stringify(extraSnap).indexOf('SENTINEL') < 0 &&
    JSON.stringify(extraSnap).indexOf('4242') < 0, JSON.stringify(extraSnap));

  // -- a busy/absent leaf renders `na`; false and 0 stay distinct ----------
  fake.resource = (): string => resourceDoc({
    connector: { present: false, generation: null, running: false, state: null,
      starts_total: 0, stops_total: 0 },
    ice: { available: false, sessions_present: 0, sessions_live: 0, sessions_stopped: 0,
      live_local_sockets: 0 },
    ice_sockets: { available: false, queued: null, taken: null },
    socket_supply: { installed: false, pending: 0, queued: 0 },
    relay_carrier: { available: false, attached_lanes: 0, pending_opens: 0, foreign_clients: 0 },
    dns_lookups: { pending_receivers: null }
  });
  const busySample = m.nativeResourceSample(m.connectorResourceStatus());
  check('resource: a busy/absent leaf renders na while false and 0 stay themselves',
    busySample.conn_present === 'false' && busySample.conn_gen === 'na' &&
    busySample.conn_running === 'false' && busySample.conn_state === 'na' &&
    busySample.conn_starts === '0' && busySample.conn_stops === '0' &&
    busySample.ice_available === 'false' && busySample.ice_sessions === '0' &&
    busySample.icesock_ready === 'false' && busySample.icesock_queued === 'na' &&
    busySample.icesock_taken === 'na' && busySample.supply_installed === 'false' &&
    busySample.supply_pending === '0' && busySample.relay_ready === 'false' &&
    busySample.relay_lanes === '0' && busySample.dns_pending === 'na',
    JSON.stringify(busySample));

  // -- valid values render as their own decimal text ------------------------
  const resourceSample = m.nativeResourceSample(snap);
  check('resource: a valid snapshot renders every leaf as its own text',
    resourceSample.conn_present === 'true' && resourceSample.conn_gen === '3' &&
    resourceSample.conn_running === 'true' && resourceSample.conn_state === 'connected' &&
    resourceSample.conn_starts === '4' && resourceSample.ice_sessions === '2' &&
    resourceSample.ice_fds === '5' && resourceSample.icesock_queued === '7' &&
    resourceSample.icesock_taken === '12' && resourceSample.supply_queued === '1' &&
    resourceSample.relay_lanes === '2' && resourceSample.relay_opens === '1' &&
    resourceSample.relay_foreign === '0' && resourceSample.dns_pending === '0',
    JSON.stringify(resourceSample));
  check('resource: an unreadable snapshot is ALL na (never a row of zeros)',
    (() => {
      const na = m.nativeResourceSample(null);
      const keys = Object.keys(na);
      return keys.length === 22 && keys.every((k: string) => na[k] === 'na');
    })());

  // -- malformed documents are null, never a throw --------------------------
  fake.resource = (): string => 'not-json';
  check('resource: unparseable JSON is null', m.connectorResourceStatus() === null);
  fake.resource = (): string => 'null';
  check('resource: a null root is null', m.connectorResourceStatus() === null);
  fake.resource = (): string => '[]';
  check('resource: a non-object root is null', m.connectorResourceStatus() === null);
  fake.resource = (): string => '{"ok":"yes"}';
  check('resource: a non-boolean ok is null', m.connectorResourceStatus() === null);
  fake.resource = (): string => '{"ok":true}';
  check('resource: a document missing its sections is null (not half-believed)',
    m.connectorResourceStatus() === null);
  fake.resource = (): string => resourceDoc({ connector: null });
  check('resource: a null section is null', m.connectorResourceStatus() === null);
  fake.resource = (): string => { throw new Error('platform'); };
  check('resource: a platform throw is null', m.connectorResourceStatus() === null);

  // -- a type-anomalous leaf degrades to na, the document still reads -------
  fake.resource = (): string => resourceDoc({
    connector: { present: 'maybe', generation: -3, running: true, state: '',
      starts_total: 4, stops_total: 1 },
    ice: { available: true, sessions_present: 'lots', sessions_live: 2, sessions_stopped: 0,
      live_local_sockets: 5 }
  });
  const anomalous = m.nativeResourceSample(m.connectorResourceStatus());
  check('resource: a type-anomalous leaf is na, never a fabricated 0 or false',
    anomalous.conn_present === 'na' && anomalous.conn_gen === 'na' &&
    anomalous.conn_state === 'na' && anomalous.ice_sessions === 'na' &&
    anomalous.conn_starts === '4' && anomalous.conn_running === 'true',
    JSON.stringify(anomalous));

  // -- the absent export branch (same latch as phone_log) -------------------
  fake.absent = true;
  let resThrew: boolean = false;
  let absentSnap: unknown = 'unset';
  try {
    absentSnap = m.connectorResourceStatus();
  } catch (error) {
    resThrew = true;
  }
  fake.absent = false;
  check('resource: an absent export is null and never throws',
    resThrew === false && absentSnap === null);

  // =========================================================================
  // (1c) the 2026-10-08 Rust-heap alloc_stats bridge
  //   (alloc_stats -> shape check -> verbatim text -> the REAL pure-core parse)
  // =========================================================================
  check('the Rust-heap bridge surface exists with its declared type',
    typeof m.nativeAllocStats === 'function' &&
    connectorText.includes('export function nativeAllocStats(): string | undefined'));
  fake.alloc = (): string => '{"bytes":1048576,"peak":2097152,"allocs":77,"frees":12}';
  const allocText = m.nativeAllocStats();
  check('alloc: a valid four-counter document is handed over verbatim',
    allocText === '{"bytes":1048576,"peak":2097152,"allocs":77,"frees":12}',
    String(allocText));
  check('alloc: the REAL pure core turns that text into KiB + raw counters',
    core.buildRustAllocFields(allocText).rust_heap_kb === '1024' &&
    core.buildRustAllocFields(allocText).rust_heap_peak_kb === '2048' &&
    core.buildRustAllocFields(allocText).rust_allocs === '77' &&
    core.buildRustAllocFields(allocText).rust_frees === '12');
  fake.alloc = (): string => 'not-json';
  check('alloc: an unparseable reply is undefined (the sampler writes na)',
    m.nativeAllocStats() === undefined);
  fake.alloc = (): string => '[]';
  check('alloc: a non-object reply is undefined', m.nativeAllocStats() === undefined);
  fake.alloc = (): string => '{"bytes":"1","peak":2,"allocs":3,"frees":4}';
  check('alloc: a counter that is not a number is undefined (no fabricated 0)',
    m.nativeAllocStats() === undefined);
  fake.alloc = (): string => '{"bytes":1,"peak":2,"allocs":3,"frees":-4}';
  check('alloc: a negative counter is undefined', m.nativeAllocStats() === undefined);
  fake.alloc = (): string => '{"bytes":1,"peak":2,"allocs":3}';
  check('alloc: a document missing a counter is undefined (not half-believed)',
    m.nativeAllocStats() === undefined);
  fake.alloc = (): string => '';
  check('alloc: an empty reply is undefined', m.nativeAllocStats() === undefined);
  fake.alloc = (): string => { throw new Error('platform'); };
  check('alloc: a platform throw is undefined, never a throw at the caller',
    m.nativeAllocStats() === undefined);
  fake.absent = true;
  let allocThrew: boolean = false;
  let absentAlloc: unknown = 'unset';
  try {
    absentAlloc = m.nativeAllocStats();
  } catch (error) {
    allocThrew = true;
  }
  fake.absent = false;
  check('alloc: an absent export is undefined and never throws',
    allocThrew === false && absentAlloc === undefined);
  check('alloc: the pure core renders an undefined reader as four na keys',
    core.buildRustAllocFields(null).rust_heap_kb === 'na' &&
    core.buildRustAllocFields(null).rust_heap_peak_kb === 'na' &&
    core.buildRustAllocFields(null).rust_allocs === 'na' &&
    core.buildRustAllocFields(null).rust_frees === 'na');

  // =========================================================================
  // (2) the once-per-minute summary
  // =========================================================================
  const okSummary = m.phoneLogSummary(JSON.parse(statusDoc({})));
  check('summary: a healthy writer is ok with the three counters',
    okSummary.state === 'ok' &&
    okSummary.detail === 'dropped=0,pending=0,failed=0', JSON.stringify(okSummary));
  const droppedSummary = m.phoneLogSummary(JSON.parse(statusDoc({ dropped: 146 })));
  check('summary: counted drops degrade the verdict',
    droppedSummary.state === 'degraded' &&
    droppedSummary.detail === 'dropped=146,pending=0,failed=0', JSON.stringify(droppedSummary));
  const failedSummary = m.phoneLogSummary(JSON.parse(statusDoc({ failed: 2, pending: 7 })));
  check('summary: write failures degrade and pending is reported separately',
    failedSummary.state === 'degraded' &&
    failedSummary.detail === 'dropped=0,pending=7,failed=2', JSON.stringify(failedSummary));
  const stoppedSummary = m.phoneLogSummary(JSON.parse(statusDoc({ running: false })));
  check('summary: a stopped writer is stopped (nothing is being recorded)',
    stoppedSummary.state === 'stopped');
  const disabledSummary = m.phoneLogSummary(JSON.parse(statusDoc({ enabled: false })));
  check('summary: a disabled tee is stopped', disabledSummary.state === 'stopped');
  const noDoc = m.phoneLogSummary(null);
  check('summary: no status document is unavailable with explicit na counters',
    noDoc.state === 'unavailable' &&
    noDoc.detail === 'dropped=na,pending=na,failed=na', JSON.stringify(noDoc));
  const anomaly = m.phoneLogSummary(JSON.parse(statusDoc({
    dropped: 'lots', pending: -3, failed: null
  })) as any);
  check('summary: type-anomalous counters render na, never a fabricated 0',
    anomaly.detail === 'dropped=na,pending=na,failed=na', JSON.stringify(anomaly));
  const secretDoc = JSON.parse(statusDoc({}));
  secretDoc.session = 'SESS-SENTINEL-3';
  secretDoc.path = '/data/files/SECRET-PATH-SENTINEL';
  const secretSummary = m.phoneLogSummary(secretDoc);
  check('summary: the raw path and session token never reach the summary',
    JSON.stringify(secretSummary).indexOf('SENTINEL') < 0 &&
    secretSummary.detail === 'dropped=0,pending=0,failed=0', JSON.stringify(secretSummary));
  check('the declared status interface omits path and session (cannot be printed by accident)',
    (() => {
      const block = sliceFrom(connectorText, 'export interface PhoneLogStatusResult {', '\n}');
      return block.length > 0 && block.indexOf('session') < 0 && block.indexOf('path') < 0;
    })());

  // =========================================================================
  // (3) the record shape against the REAL diagnostics whitelist
  // =========================================================================
  // Every shell milestone this increment wires, with the keys its call site
  // uses. Names/keys must pass the pure core's allowlists or the record would
  // silently lose the field (or the whole event).
  const milestones: Array<[string, string[]]> = [
    ['vpn_log_tee', ['stage', 'result', 'detail']],
    ['vpn_ext_start', ['stage', 'net']],
    ['vpn_lifecycle_stop', ['stage', 'state']],
    ['vpn_destroy_done', ['result', 'stage', 'trigger']],
    ['vpn_recreate_begin', ['reason']],
    ['vpn_recreate_done', ['result', 'count']],
    ['vpn_recreate_failed', ['stage', 'result']],
    ['vpn_net_switch', ['net', 'mode', 'trigger']],
    ['vpn_net_recovered', ['net']],
    ['vpn_net_recovery_failed', ['stage', 'result']],
    ['vpn_core_dead', ['reason', 'result']],
    ['vpn_start_failed', ['stage', 'reason']],
    // Fix A: at most three management DNS aborts per startup, retaining the
    // attempt ordinal and numeric platform code without a raw error message.
    ['vpn_connector_aborted', ['stage', 'detail', 'count', 'code', 'reason']],
    ['phone_log_status', ['stage', 'state', 'detail']],
    // 2026-10-08 memory-pressure observation (bounded enum token only)
    ['vpn_memory_level', ['stage', 'state']],
    // 006 bridge surface
    ['vpn_destroy_phase', ['phase', 'stage']],
    // 2026-10-13 device report §C/G1: the per-request protected-socket supply
    // outcome — the shell-side refusal reason exists nowhere else on this ROM.
    ['socket_supply', ['purpose', 'result', 'reason', 'ms']],
    // 2026-10-13 device report §D: the effective tunnel MTU and its provenance
    // (default vs network-map), once per config build.
    ['vpn_tunnel_mtu', ['stage', 'mtu', 'source']],
    // 0.0.50 recovery leg: the awaited step's outcome (ok/timeout, one record
    // per step) and the retry's silent early-return reason (once per epoch).
    ['vpn_recovery_stage', ['stage', 'result']],
    ['vpn_recovery_skip', ['reason']],
    // 0.0.34 peer-status lag fix (peer-status-lag REPORT §3.5): the extension's
    // publication stamp + the core's edge revision + the two peer counts the F5
    // label change moves — one record per REAL write (the debounce above folds
    // the unchanged heartbeat ticks away, so this never becomes a 5 s timer).
    ['vpn_status_publish', ['detail', 'updated_at_ms', 'status_revision',
      'peers_online', 'peers_relay']],
    ['native_resource_sample', ['stage', 'seq', 'conn_present', 'conn_gen', 'conn_running',
      'conn_state', 'conn_starts', 'conn_stops', 'ice_available', 'ice_sessions', 'ice_live',
      'ice_stopped', 'ice_fds', 'icesock_ready', 'icesock_queued', 'icesock_taken',
      'supply_installed', 'supply_pending', 'supply_queued', 'relay_ready', 'relay_lanes',
      'relay_opens', 'relay_foreign', 'dns_pending']]
  ];
  // 0.0.26 gap 2: the VPN_* lifecycle markers mirrored into the app log by
  // `diagLifecycleMarker` — the event name is the marker lowercased (never a
  // literal in the source, hence its own list) and every record carries the
  // verbatim UPPERCASE marker in `detail`.
  const mirroredMarkers: string[] = [
    'VPN_PROTECT_BEGIN', 'VPN_PROTECT_RESOLVED', 'VPN_PROTECT_FAIL_CLOSED',
    'VPN_ICE_MUX_ARMED', 'VPN_ICE_MUX_FAIL', 'VPN_CREATE_BEGIN',
    'VPN_CREATE_RESOLVED', 'VPN_CREATE_REJECTED', 'VPN_CREATE_INVALID_FD',
    'VPN_CONNECTION_CREATED', 'VPN_START_FLOW_REJECTED', 'VPN_DESTROY_BEGIN',
    'VPN_DESTROY_RESOLVED', 'VPN_DESTROY_REJECTED', 'VPN_DESTROY_SKIPPED',
    // F2 (v0.0.27 outage REPORT.md §5): the physical-recovery abort stage. It was
    // the ONE shell fact that capture had to DERIVE — every abort was a silent
    // `break` (13 of them) — so the stage token now travels the same mirrored
    // path as the markers above and lands in netbird-vpn.log as
    // `detail=VPN_NETWORK_RECOVERY_ABORT:stage=<token>`.
    'VPN_NETWORK_RECOVERY_ABORT'
  ];
  const badMirrored = mirroredMarkers
    .filter((mk: string) => core.isPublicEventName(mk.toLowerCase()) !== true);
  check('every mirrored lifecycle marker has a legal public event name',
    badMirrored.length === 0 && core.PUBLIC_FIELD_KEYS.indexOf('detail') >= 0,
    badMirrored.join(','));
  const badNames: string[] = milestones
    .filter((mm: [string, string[]]) => core.isPublicEventName(mm[0]) !== true)
    .map((mm: [string, string[]]) => mm[0]);
  check('every milestone name passes the diagnostics public-name rule',
    badNames.length === 0, badNames.join(','));
  const badKeys: string[] = [];
  for (const mm of milestones) {
    for (const key of mm[1]) {
      if (core.PUBLIC_FIELD_KEYS.indexOf(key) < 0) { badKeys.push(`${mm[0]}.${key}`); }
    }
  }
  check('every milestone field key is on the diagnostics public whitelist',
    badKeys.length === 0, badKeys.join(','));
  const ts = '2026-10-08T00:00:00.000Z';
  const sampleFields = m.phoneLogSummary(JSON.parse(statusDoc({ dropped: 146, pending: 3, failed: 1 })));
  const record = core.buildRecord(ts, 42, 'vpn', 'phone_log_status',
    { stage: 'native', state: sampleFields.state, detail: sampleFields.detail }, false);
  check('the phone_log_status record is exact, ordered and unterminated by redaction',
    record === 'ts=2026-10-08T00:00:00.000Z pid=42 role=vpn event=phone_log_status ' +
      'detail=dropped=146,pending=3,failed=1 stage=native state=degraded\n', record);
  check('the summary record is far below the per-record line bound',
    record.length <= core.MAX_LINE_CHARS);

  // The REAL bridge output, through the REAL renderer and the REAL record
  // builder: the two parts the shell emits from ONE 60 s tick.
  fake.resource = (): string => resourceDoc({});
  const tickSample = m.nativeResourceSample(m.connectorResourceStatus());
  const partA: Record<string, string | number> = {
    'stage': 'connector', 'seq': 1,
    'conn_present': tickSample.conn_present, 'conn_gen': tickSample.conn_gen,
    'conn_running': tickSample.conn_running, 'conn_state': tickSample.conn_state,
    'conn_starts': tickSample.conn_starts, 'conn_stops': tickSample.conn_stops,
    'ice_available': tickSample.ice_available, 'ice_sessions': tickSample.ice_sessions,
    'ice_live': tickSample.ice_live, 'ice_stopped': tickSample.ice_stopped,
    'ice_fds': tickSample.ice_fds
  };
  const partB: Record<string, string | number> = {
    'stage': 'transport', 'seq': 2,
    'icesock_ready': tickSample.icesock_ready, 'icesock_queued': tickSample.icesock_queued,
    'icesock_taken': tickSample.icesock_taken, 'supply_installed': tickSample.supply_installed,
    'supply_pending': tickSample.supply_pending, 'supply_queued': tickSample.supply_queued,
    'relay_ready': tickSample.relay_ready, 'relay_lanes': tickSample.relay_lanes,
    'relay_opens': tickSample.relay_opens, 'relay_foreign': tickSample.relay_foreign,
    'dns_pending': tickSample.dns_pending
  };
  const recordA = core.buildRecord(ts, 42, 'vpn', 'native_resource_sample', partA, false);
  const recordB = core.buildRecord(ts, 42, 'vpn', 'native_resource_sample', partB, false);
  check('each native_resource_sample part is bounded and unterminated (nothing cut)',
    recordA.length < core.MAX_LINE_CHARS && recordB.length < core.MAX_LINE_CHARS &&
    recordA.indexOf('~') < 0 && recordB.indexOf('~') < 0 &&
    Object.keys(partA).length <= 16 && Object.keys(partB).length <= 16,
    `${recordA.length}/${recordB.length}`);
  const emittedKeys: string[] = Object.keys(partA).concat(Object.keys(partB));
  check('every scalar leaf of the native schema is emitted across the two parts',
    RESOURCE_LEAVES.every((k: string) => emittedKeys.indexOf(k) >= 0) &&
    RESOURCE_LEAVES.length === 22 && emittedKeys.length === 22 + 4,
    RESOURCE_LEAVES.filter((k: string) => emittedKeys.indexOf(k) < 0).join(','));
  check('the two parts correlate by the record envelope and are labelled stage/seq',
    recordA.indexOf('role=vpn event=native_resource_sample') >= 0 &&
    recordB.indexOf('role=vpn event=native_resource_sample') >= 0 &&
    recordA.indexOf(' seq=1 stage=connector') >= 0 &&
    recordB.indexOf(' seq=2 stage=transport') >= 0);

  // =========================================================================
  // (4) the extension wiring (source pins — the extension is not importable)
  // =========================================================================
  const tee = sliceFrom(extText, 'private startDiagnosticsLogTee(): void {',
    'private diagSink(line: string): void {');
  const sink = sliceFrom(extText, 'private diagSink(line: string): void {',
    'private diagMilestone(name: string');
  const sample = sliceFrom(extText, 'private samplePhoneLogStatus(): void {',
    'private watchPhysicalNetwork(): void {');
  const resourceMethod = sliceFrom(extText, 'private sampleNativeResourceStatus(): void {',
    'private watchPhysicalNetwork(): void {');
  const tick = sliceFrom(extText, 'this.watcherTimer = setInterval((): void => {',
    '}, STATUS_POLL_MS);');
  const onDestroy = sliceFrom(extText, 'onDestroy(): void {',
    'private captureFdStatus(): FdSnapshot {');
  const death = sliceFrom(extText, 'private handleConnectorDeath(',
    'private async performControlledRecreate(');
  const recreate = sliceFrom(extText, 'private async performControlledRecreate(',
    'private destroyConnectionBounded(');
  const failRecreate = sliceFrom(extText, 'private failRecreate(stage: string, error: Error): void {',
    'private startSocketSupply(): void {');
  const runDestroy = sliceFrom(extText, 'private async runDestroyOnce(): Promise<void> {',
    'private auditOutboundSocketAfterDestroy(): void {');
  const connectFailure = sliceFrom(extText, 'private writeConnectFailureForUi(errorClass: string): void {',
    'private destroyOnce(trigger: string): Promise<void> {');
  const networkSwitch = sliceFrom(extText,
    'private physicalNetworkAvailable(event: string, eventNetId: number, netId: number): void {',
    'private checkLanRoamRecreate(): void {');
  const recoveryFail = sliceFrom(extText, 'private n2NetworkRecoveryFail(error: Error): void {',
    'private async recoverPhysicalNetwork(): Promise<void> {');
  const recovery = sliceFrom(extText, 'private async recoverPhysicalNetwork(): Promise<void> {',
    'private async startConnectorThenCreate(');
  // 2026-10-13 observability increment (device report §C/§D): the two new
  // persisted records and the methods that carry them.
  const supply = sliceFrom(extText, 'private async fulfillSocketRequest(',
    'private stopSocketSupply(): void {');
  const mtuMethod = sliceFrom(extText, 'private logTunnelMtu(',
    'private logConfigApplied(');

  check('every slice for the wiring pins is non-empty',
    tee.length > 0 && sink.length > 0 && sample.length > 0 && tick.length > 0 &&
    onDestroy.length > 0 && death.length > 0 && recreate.length > 0 &&
    failRecreate.length > 0 && runDestroy.length > 0 && connectFailure.length > 0 &&
    networkSwitch.length > 0 && recoveryFail.length > 0 && recovery.length > 0 &&
    resourceMethod.length > 0 && supply.length > 0 && mtuMethod.length > 0,
    `${tee.length}/${sink.length}/${sample.length}/${tick.length}/${onDestroy.length}`);

  check('the extension imports the diagnostics helper and the system-event watcher',
    extText.includes("from './NetBirdDiagnostics'") &&
    extText.includes("from './NetBirdSystemEvents'"));
  check('the phone_log / resource interfaces are imported TYPE-ONLY (erased: no shim needed)',
    extText.includes(
      'import type { PhoneLogConfigureResult, PhoneLogSummary, NativeResourceSample }') &&
    extText.includes("from './NetBirdConnector';") &&
    !extText.includes('PhoneLogConfigureResult, PhoneLogSummary, phoneLogAppend'));
  check('the tee is skipped (not crashed) when the lifecycle context has no filesDir',
    tee.includes('filesDir = this.context.filesDir;') &&
    tee.includes('if (filesDir.length === 0)') &&
    tee.indexOf('if (filesDir.length === 0)') < tee.indexOf('phoneLogConfigure('));
  check('the tee target is filesDir + the native-vpn log name',
    extText.includes('const VPN_LOG_FILE_NAME: string = \'netbird-vpn.log\';') &&
    tee.includes('${filesDir}/${VPN_LOG_FILE_NAME}'));
  check('onCreate arms the diagnostics BEFORE the connection is created',
    extText.indexOf('this.startDiagnosticsLogTee();') >
      extText.indexOf('this.connectionCreationAttempted = true;') &&
    extText.indexOf('this.startDiagnosticsLogTee();') <
      extText.indexOf('vpnExtension.createVpnConnection(this.context)'));
  check('start order: native tee -> diagnostics -> system events -> tee record',
    tee.indexOf('phoneLogConfigure(') >= 0 &&
    tee.indexOf('phoneLogConfigure(') < tee.indexOf('startDiagnostics(filesDir') &&
    tee.indexOf('startDiagnostics(filesDir') < tee.indexOf('startSystemEvents(') &&
    tee.indexOf('startSystemEvents(') < tee.indexOf("diagMilestone('vpn_log_tee'"));
  check('both modules are started with the role vpn and the SAME safe sink',
    tee.includes("startDiagnostics(filesDir, 'vpn'") &&
    tee.includes("startSystemEvents('vpn'") &&
    tee.split('this.diagSink(line)').length === 3);
  check('the vpn role injects the Rust-heap alloc reader into the diagnostics start',
    tee.includes('(): string | undefined => nativeAllocStats()') &&
    connectorText.includes('export function nativeAllocStats(): string | undefined') &&
    extText.split('nativeAllocStats(').length === 2,
    'the reader must be the 4th argument of the ONE startDiagnostics call');
  check('the platform memory-pressure level is recorded, with no memory released',
    extText.includes('onMemoryLevel(level: AbilityConstant.MemoryLevel): void') &&
    extText.includes("import AbilityConstant from '@ohos.app.ability.AbilityConstant';") &&
    extText.includes("const milestoneFields: DiagFields = { 'stage': 'platform', 'state': String(level) };") &&
    extText.includes("diagMilestone('vpn_memory_level'") &&
    !/onMemoryLevel[\s\S]{0,400}?(free|release|trim|evict|clear)\(/.test(
      sliceFrom(extText, 'onMemoryLevel(level: AbilityConstant.MemoryLevel): void {',
        'onCreate(want: Want): void {')));
  check('the shared sink appends through the phone_log wrapper and never double-forwards to hilog',
    sink.includes('phoneLogAppend(line)') && sink.indexOf('hilog') < 0 &&
    sink.includes('try {') && sink.includes('catch (error)'));
  check('the tee is configured once per process (latch) and a failed configure is recorded',
    tee.includes('if (!this.logTeeConfigured)') &&
    tee.includes("'result': configured.ok ? 'ok' : 'fail'") &&
    extText.includes('private logTeeConfigured: boolean = false;'));
  check('a refused tee also stays visible on hilog (the record itself would be lost)',
    tee.includes('VPN_LOG_TEE_FAILED') &&
    tee.indexOf('VPN_LOG_TEE_FAILED') < tee.indexOf('startDiagnostics(filesDir'));
  check('the vpn role never writes a second log file of its own',
    extText.indexOf('netbird-ui.log') < 0 && extText.indexOf('netbird-vpn.log\'.1') < 0);

  // watcher: 60 s gate, one native read per window, no extra timer
  check('the writer-health sample rides the existing watcher tick, first statement',
    tick.indexOf('this.samplePhoneLogStatus();') >= 0 &&
    tick.indexOf('this.samplePhoneLogStatus();') < tick.indexOf('this.maybeCloseInterfaceStatsWindow();') &&
    tick.indexOf('phoneLogAppend') < 0 && tick.indexOf('startDiagnostics') < 0);
  check('the sample is gated to one native status read per 60 s (no per-tick write storm)',
    extText.includes('const PHONE_LOG_STATUS_SAMPLE_MS: number = 60000;') &&
    sample.includes('if (now - this.lastPhoneLogStatusAtMs < PHONE_LOG_STATUS_SAMPLE_MS)') &&
    sample.includes('phoneLogSummary(phoneLogStatus())') &&
    sample.includes("diagMilestone('phone_log_status'") &&
    sample.indexOf('setInterval') < 0 &&
    extText.split('phoneLogStatus(').length === 2);
  check('the native resource sample rides the SAME 60 s gate (no new interval, no 5 s path)',
    sample.includes('this.sampleNativeResourceStatus();') &&
    sample.indexOf('this.sampleNativeResourceStatus();') >
      sample.indexOf("diagMilestone('phone_log_status'") &&
    sample.indexOf('this.sampleNativeResourceStatus();') >
      sample.indexOf('if (now - this.lastPhoneLogStatusAtMs'),
    'the resource call must sit after the gate, inside samplePhoneLogStatus');
  check('the resource read is one native call behind the gate, in its own bounded method',
    resourceMethod.length > 0 && resourceMethod.indexOf('setInterval') < 0 &&
    resourceMethod.includes('nativeResourceSample(connectorResourceStatus())') &&
    extText.split('connectorResourceStatus(').length === 2 &&
    extText.split('nativeResourceSample(').length === 2);
  // STAGE 4 (official-alignment audit §2.7): the retired ICE socket pool took
  // its own 4 s top-up timer with it. 0.0.27 then replaced the 100 ms
  // socket-supply tick: the supply is event-driven now (the core's own enqueue
  // wake) with a 5 s safety net, and the route-apply mailbox that tick used to
  // carry got its own 500 ms interval. The reviewed count is therefore 5 —
  // watcher (5 s), N2 iface burst (500 ms, armed window only), N2 ring stats
  // (60 s, configured only), supply safety net (5 s), mailbox (500 ms) — and
  // NONE of them belongs to the phone-log resource bridge.
  check('the resource bridge adds NO timer of its own to the extension (its five are: ' +
    'watcher, N2 burst, N2 ring, supply safety net, mailbox poll)',
    extText.split('setInterval(').length === 6 &&
    sample.indexOf('setInterval') < 0 && resourceMethod.indexOf('setInterval') < 0);
  check('the diagnostics helper is started exactly once in the whole extension (no re-arm timer)',
    extText.split('startDiagnostics(').length === 2 &&
    extText.split('startSystemEvents(').length === 2);

  // stop ordering
  check('onDestroy records the stop, stops the system-event watcher, then stops the connector',
    onDestroy.includes("diagMilestone('vpn_lifecycle_stop'") &&
    onDestroy.indexOf("diagMilestone('vpn_lifecycle_stop'") < onDestroy.indexOf('stopSystemEvents();') &&
    onDestroy.indexOf('stopSystemEvents();') < onDestroy.indexOf('connectorStop();') &&
    onDestroy.indexOf('connectorStop();') < onDestroy.indexOf("this.destroyOnce('onDestroy')"));
  check('Diagnostics.stop is LATE: only after destroyOnce settles, never before',
    onDestroy.indexOf('stopDiagnostics();') > onDestroy.indexOf("this.destroyOnce('onDestroy')") &&
    onDestroy.indexOf('stopDiagnostics();') > onDestroy.indexOf('core.n2_ring_disable();') &&
    onDestroy.indexOf('stopDiagnostics();') > onDestroy.indexOf('.then((): void => {'));
  check('the teardown outcome is recorded from inside runDestroyOnce (ok and fail)',
    runDestroy.split("diagMilestone('vpn_destroy_done'").length === 3 &&
    runDestroy.includes("'result': 'ok'") && runDestroy.includes("'result': 'fail'"));
  check('the destroy phase is observational: requested -> settled/failed, no clean marking',
    onDestroy.split("diagMilestone('vpn_destroy_phase'").length === 4 &&
    onDestroy.indexOf("'phase': 'requested'") <
      onDestroy.indexOf("this.destroyOnce('onDestroy')") &&
    onDestroy.indexOf("'phase': 'failed'") > onDestroy.indexOf('.catch(') &&
    onDestroy.indexOf("'phase': 'settled'") > onDestroy.indexOf('.then(') &&
    onDestroy.indexOf('stopDiagnostics();') > onDestroy.indexOf("'phase': 'settled'") &&
    onDestroy.indexOf("'phase': 'settled'") > onDestroy.indexOf("'phase': 'failed'"),
    'the phase record must bracket the destroy leg without touching the marker');
  check('the phase record carries fixed tokens only (no error summary, no path)',
    (() => {
      const declarations: string[] = [
        "const destroyRequestedFields: DiagFields = { 'phase': 'requested', 'stage': 'destroy' };",
        "const destroyFailedFields: DiagFields = { 'phase': 'failed', 'stage': 'destroy' };",
        "const destroySettledFields: DiagFields = { 'phase': 'settled', 'stage': 'destroy' };"
      ];
      return declarations.every((d: string) => onDestroy.indexOf(d) >= 0);
    })());
  check('the native writer is NEVER shut down (process lifetime, no shutdown race)',
    extText.indexOf('phoneLogShutdown') < 0 &&
    connectorText.includes('export function phoneLogShutdown(): PhoneLogShutdownResult'));

  // milestone wiring
  check('core death (fatal auth / retry budget exhausted / poll limit) is recorded',
    death.includes("diagMilestone('vpn_core_dead'"));
  check('the startup-abort funnel records the same failure class the UI notice carries',
    connectFailure.includes("diagMilestone('vpn_start_failed'") &&
    connectFailure.indexOf("diagMilestone('vpn_start_failed'") <
      connectFailure.indexOf('fs.openSync(path'));
  check('the recreate start/end pair is recorded (begin + done/fail)',
    recreate.includes("diagMilestone('vpn_recreate_begin'") &&
    recreate.includes("diagMilestone('vpn_recreate_done'") &&
    failRecreate.includes("diagMilestone('vpn_recreate_failed'"));
  check('the network loss/switch and the recovery edges are recorded',
    networkSwitch.includes("diagMilestone('vpn_net_switch'") &&
    networkSwitch.indexOf("diagMilestone('vpn_net_switch'") > networkSwitch.indexOf('if (!switched) { return; }') &&
    recovery.includes("diagMilestone('vpn_net_recovered'") &&
    recoveryFail.includes("diagMilestone('vpn_net_recovery_failed'"));
  check('the recovery record carries no error summary (fixed tokens only)',
    (() => {
      const start: number = recoveryFail.indexOf('const milestoneFields: DiagFields =');
      const record: string = start < 0 ? '' : recoveryFail.slice(start);
      return record.length > 0 && record.indexOf('safeError') < 0 &&
        record.includes("'stage': 'physical-recovery'") &&
        record.includes("diagMilestone('vpn_net_recovery_failed'");
    })(), recoveryFail);
  // 2026-10-13 device report §C/G1: every supply request records its outcome on
  // the PERSISTED tee, inside the reply's finally (so no early return, refusal
  // or exception path can skip it) — purpose, result, refusal reason, elapsed.
  // The hilog line stays exactly where it was (this ROM keeps no I-level
  // app-domain line, which is why the record is the fix).
  check('every supply request records its outcome on the persisted tee',
    supply.length > 0 &&
    supply.includes("diagMilestone('socket_supply'") &&
    supply.indexOf("diagMilestone('socket_supply'") > supply.indexOf('socketSupplyReply(request.id') &&
    supply.indexOf("diagMilestone('socket_supply'") < supply.indexOf('socketSupplyCloseFd(fd)') &&
    supply.includes("'purpose': request.purpose ?? 'unknown'") &&
    supply.includes("'result': result") &&
    supply.includes("'reason': reason") &&
    supply.includes("'ms': Date.now() - t0") &&
    supply.indexOf('VPN_SOCKET_SUPPLY|id=') < supply.indexOf("diagMilestone('socket_supply'"));
  check('the supply record carries no fd, no path and no id',
    (() => {
      const start: number = supply.indexOf('const supplyFields: DiagFields =');
      const bag: string = start < 0 ? '' : supply.slice(start, supply.indexOf('};', start));
      return bag.length > 0 && bag.indexOf('request.id') < 0 && bag.indexOf('fd') < 0 &&
        bag.indexOf('path') < 0 && bag.indexOf('reply') < 0;
    })(), supply);
  // 2026-10-13 device report §D: the effective MTU of the config that is really
  // submitted (`merged.mtu`, whose source is selectTunnelMtu's answer) is
  // recorded once per config build — the initial create leg and the recreate
  // leg — with the provenance token from the PURE source helper.
  check('the effective tunnel MTU is recorded once per config build, with its source',
    mtuMethod.length > 0 &&
    mtuMethod.includes('tunnelMtuSource(baseMtu, announced)') &&
    mtuMethod.includes("diagMilestone('vpn_tunnel_mtu'") &&
    mtuMethod.indexOf('this.appliedTunnelMtu = effectiveMtu;') > 0 &&
    extText.includes(
      "this.logTunnelMtu(baseTemplate.mtu, net !== null ? net.mtu : null, merged.mtu, 'create');") &&
    extText.includes("this.logTunnelMtu(recreateTemplate.mtu, net.mtu, merged.mtu, 'recreate');") &&
    extText.includes('withTunnelSubnetRoute, tunnelMtuSource } from \'./NetBirdVpnConfig\';'));
  check('the MTU record names the value submitted to the platform, not a default',
    (() => {
      const start: number = mtuMethod.indexOf('const milestoneFields: DiagFields =');
      const bag: string = start < 0 ? '' : mtuMethod.slice(start, mtuMethod.indexOf('};', start));
      return bag.includes("'mtu': effectiveMtu") && bag.includes("'source': source") &&
        bag.includes("'stage': leg") && bag.indexOf('1280') < 0 && bag.indexOf('baseMtu') < 0;
    })(), mtuMethod);
  check('the published status document carries the same applied MTU',
    extText.includes('this.managementEndpoint, this.appliedTunnelMtu);') &&
    extText.includes('private appliedTunnelMtu: number | undefined = undefined;'));
  check('every wired milestone name is the exact token the record pins above',
    milestones.every((mm: [string, string[]]) =>
      extText.includes(`'${mm[0]}'`)));
  check('the wired milestone surface is exactly the 22 reviewed names / 27 helper calls',
    milestones.length === 22 && extText.split('this.diagMilestone(').length === 28);
  check('every mirrored lifecycle marker is really wired into the extension',
    mirroredMarkers.every((mk: string) =>
      extText.includes(`this.diagLifecycleMarker('${mk}'`)));
  check('the milestones are recorded through the shared non-throwing helper',
    extText.includes('private diagMilestone(name: string, fields: DiagFields): void') &&
    sliceFrom(extText, 'private diagMilestone(name: string',
      'private samplePhoneLogStatus(): void {').includes('diagEvent(name, fields);'));
  check('every field bag is declared as DiagFields (ArkTS rejects a bare literal argument)',
    extText.split(': DiagFields = ').length === extText.split('this.diagMilestone(').length &&
    !/this\.diagMilestone\([^)]*\{ /.test(extText));
  check('DiagFields is imported type-only from the diagnostics writer',
    extText.includes("import type { DiagFields } from './NetBirdDiagnostics';"));
} catch (error) {
  failedCount++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(fixtureDir, { recursive: true, force: true });
}

console.log(`\n${passed} passed, ${failedCount} failed`);
process.exit(failedCount === 0 ? 0 : 1);
