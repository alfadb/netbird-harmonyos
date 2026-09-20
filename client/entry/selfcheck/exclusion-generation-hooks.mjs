// Loader-side resolve hook for run-exclusion-generation.mts ONLY (host-side
// fixture). Same principle as relay-exclusion-hooks.mjs / route-check-hooks.mjs:
// the REAL modules import @ohos.* / libnetbird_core.so which cannot resolve
// under plain node; this hook short-circuits ONLY those specifiers. No module
// under test is ever rewritten — the fixtures are byte-identical copies of the
// shipped .ets files.
//
// The stubs are FUNCTIONAL, not empty, because the fixture imports the REAL
// NetBirdVpnExtensionAbility.ets and drives its generation bookkeeping
// end-to-end (the initial leg's truth dump + acceptance anchor through the REAL
// settleCreate, the controlled recreate through the REAL
// performControlledRecreate, and the REAL arm hook maybeArmN2hCollector):
//   - @ohos.hilog  -> recording no-op formatter; every formatted line lands in
//                     globalThis.__NB_LOG, so the marker assertions run against
//                     the REAL hilog call sites (same recorder as
//                     relay-exclusion-hooks.mjs).
//   - @ohos.file.fs -> a PATH-AWARE in-memory filesystem (globalThis.__NB_FS,
//                     a {absPath: text} map) so the arm private channel
//                     (2026-09-18) is driven through the REAL filesDir join:
//                     the extension resolves <context.filesDir>/netbird-n2h-arm
//                     and the fixture EntryAbility resolves its own filesDir —
//                     same basename, same mechanism as the credentials. A
//                     per-path error table (__NB_FS_ACCESS_ERROR) keeps the
//                     three shapes the real @ohos.file.fs.accessSync can
//                     answer with (SDK 26.0.0.821 @ohos.file.fs.d.ts:309-336 —
//                     boolean false / BusinessError 13900002 for "not there",
//                     other codes for a probe that did not prove absence):
//                       path in __NB_FS                     -> accessSync true
//                       path not in __NB_FS                 -> accessSync false
//                       __NB_FS_ACCESS_ERROR[path]=<n>      -> throws BusinessError n
//                       __NB_FS_ACCESS_ERROR[path]='no-code' -> throws WITHOUT a code
//                     openSync honors __NB_FS_OPEN_ERROR[path] (open-failure
//                     tests) and writeSync honors __NB_FS_WRITE_ERROR[fd] (the
//                     TRUNC-wiped short/failed-write case: the entry is first
//                     emptied, then the error is thrown). writeSync returns
//                     the REAL UTF-8 byte count (Buffer.byteLength) — the
//                     contract @ohos.file.fs.writeSync documents — so the
//                     fixture EntryAbility's short-write detection is driven
//                     honestly. openSync returns {fd: path}; writeSync stores
//                     the text under that fd (== the path); unlinkSync deletes.
//   - @ohos.net.vpnExtension -> `createVpnConnection()` returns a fake
//                     connection whose create() resolves to
//                     globalThis.__NB_CREATE_FD (default 99) and records every
//                     submitted config in globalThis.__NB_CREATED_CONFIGS;
//                     destroy()/protect() resolve (counted in __NB_DESTROYED /
//                     __NB_PROTECTED). This is what lets the recreate leg run
//                     to completion on the host.
//   - libnetbird_core.so -> the NAPI seam of the paths under test, each reading
//                     a per-test global:
//                       __NB_FD_STATUS   {open,errno} for core.fd_status(fd)
//                       __NB_NETCFG      connector_network_config() snapshot
//                       __NB_STATUS      connector_status() document
//                       __NB_ARM_OPTS    the arm options JSON the shell sent
//                     connector_stop / connector_wg_socket_feed /
//                     connector_tun_fd_feed / connector_route_set_applied
//                     answer success shapes (the ACK lands in __NB_ACK).
//                     Any core function the driven paths do NOT call stays
//                     undefined: calling it is a loud TypeError, never a
//                     silently passing stub.
//   - @ohos.net.connection -> the four synchronous network APIs + the fake DNS
//                     table globalThis.__NB_DNS (the real derivation runs on
//                     the paths under test). Since the 2026-09-20
//                     network-context snapshot probe, the stub also carries
//                     getAppNetSync and is table-driven so the probe's tests
//                     can seed answers WITHOUT changing any default the older
//                     groups observe:
//                       __NB_NET_DEFAULT_ID  number|{code:n} for
//                                            getDefaultNetSync (default 0 ->
//                                            the historical {netId: 0})
//                       __NB_NET_APP_ID      number|{code:n} for
//                                            getAppNetSync (default 0)
//                       __NB_NET_ALL_IDS     number[] for getAllNetsSync
//                                            (default [] -> the historical
//                                            empty list)
//                       __NB_NET_CAPS_BY_ID  netId -> NetCapabilities shape,
//                                            or {error:n} to reject
//                                            (default: the historical
//                                            {networkCap: [], bearerTypes: []})
//                       __NB_NET_PROPS_BY_ID netId -> {routes}, or {error:n}
//                                            to reject (default: the
//                                            historical wlan0 response, now
//                                            with routes: [])
//                       __NB_NET_THROW_CODES methodName -> code|'no-code' to
//                                            throw (default unset: no-op)
//   - @ohos.net.statistics -> the per-NIC byte counters the 2026-09-18
//                     iface-bytes availability probe calls (SDK 26.0.0.821
//                     @ohos.net.statistics.d.ts:79/113, since 10, no
//                     permission on the iface methods). Functional stub keyed
//                     by NIC name: __NB_STATS_RX / __NB_STATS_TX map a name to
//                     a number (resolved value) or {code:n} (rejected
//                     BusinessError); an UNSET name rejects with the
//                     documented internal-error code 2100003 so an unexpected
//                     probe can never fake a value. Every call is recorded in
//                     __NB_STATS_CALLS as "method:nic" (append-only — the
//                     probe tests assert on length deltas).
//   - @ohos.app.ability.VpnExtensionAbility -> a bare base class so the real
//                     extension class can be `new`-ed on the host.
//   - @ohos.app.ability.UIAbility -> a bare base class so the real
//                     EntryAbility (arm private channel, C group) can be
//                     `new`-ed the same way.
//   - @ohos.util -> a REAL TextEncoder (Buffer-backed UTF-8): the fixture
//                     EntryAbility uses it for the write-completeness byte
//                     count (UTF-8 bytes, never UTF-16 code units).
// Everything else (@ohos.app.ability.Want / @ohos.app.ability.AbilityConstant /
// @ohos.window / @ohos.resourceManager) is dead code for the driven paths ->
// empty stubs.
//
// Sibling modules are NOT stubbed: the test stages the five REAL files
// byte-identically in one temp dir and writes <Name>.shim.ts bridges for the
// interface names node's type stripping erases (the same plumbing
// run-relay-exclusion.mts uses).

const EMPTY = 'data:text/javascript,export%20default%20%7B%7D%3B';

const HILOG = 'data:text/javascript,' + encodeURIComponent(
  'function fmt(f, a) { if (!a || a.length === 0) { return String(f); }\n' +
  '  let i = 0;\n' +
  '  return String(f).replace(/%\\{public\\}[sd]/g, () => (i < a.length ?' +
  ' String(a[i++]) : "")); }\n' +
  'const rec = (lvl) => (domain, tag, format, ...args) => {\n' +
  '  const g = globalThis;\n' +
  '  g.__NB_LOG = g.__NB_LOG || [];\n' +
  '  g.__NB_LOG.push("[" + lvl + "] " + fmt(format, args));\n' +
  '};\n' +
  'export default { debug: rec("D"), info: rec("I"), warn: rec("W"),' +
  ' error: rec("E") };');

const FS = 'data:text/javascript,' + encodeURIComponent(
  'function g() { return globalThis; }\n' +
  'function files() { if (!g().__NB_FS) { g().__NB_FS = {}; } return g().__NB_FS; }\n' +
  'function thrower(path, what) {\n' +
  '  const code = (g().__NB_FS_ACCESS_ERROR || {})[path];\n' +
  '  if (code === undefined) { return; }\n' +
  '  const e = new Error(what + " failed: " + path);\n' +
  '  if (code !== "no-code") { e.code = code; }\n' +
  '  throw e;\n' +
  '}\n' +
  'export default {\n' +
  '  OpenMode: { READ_WRITE: 3, CREATE: 8, TRUNC: 16 },\n' +
  '  accessSync: (path) => {\n' +
  '    thrower(path, "access");\n' +
  '    return Object.prototype.hasOwnProperty.call(files(), path);\n' +
  '  },\n' +
  '  readTextSync: (path) => {\n' +
  '    thrower(path, "read");\n' +
  '    if (!Object.prototype.hasOwnProperty.call(files(), path)) {\n' +
  '      const e = new Error("readText failed: " + path); e.code = 13900002;\n' +
  '      throw e;\n' +
  '    }\n' +
  '    return files()[path];\n' +
  '  },\n' +
  '  openSync: (path) => {\n' +
  '    const code = (g().__NB_FS_OPEN_ERROR || {})[path];\n' +
  '    if (code !== undefined) {\n' +
  '      const e = new Error("open failed: " + path); e.code = code; throw e;\n' +
  '    }\n' +
  '    return { fd: path };\n' +
  '  },\n' +
  '  writeSync: (fd, text) => {\n' +
  '    const err = (g().__NB_FS_WRITE_ERROR || {})[fd];\n' +
  '    if (err !== undefined) {\n' +
  '      files()[fd] = ""; // the open already TRUNCed; nothing got written\n' +
  '      const e = new Error("write failed: " + fd); e.code = err; throw e;\n' +
  '    }\n' +
  '    const bytes = Buffer.byteLength(String(text), "utf8");\n' +
  '    files()[fd] = String(text);\n' +
  '    return bytes;\n' +
  '  },\n' +
  '  closeSync: () => {},\n' +
  '  unlinkSync: (path) => { delete files()[path]; }\n' +
  '};');

// Real UTF-8 encoding for the fixture EntryAbility's completeness check (the
// platform util.TextEncoder contract: encode() returns the UTF-8 bytes).
const UTIL = 'data:text/javascript,' + encodeURIComponent(
  'class TextEncoder {\n' +
  '  encode(input) { return Uint8Array.from(Buffer.from(String(input ?? ""), "utf8")); }\n' +
  '}\n' +
  'export default { TextEncoder: TextEncoder };');

const VPN_EXT = 'data:text/javascript,' + encodeURIComponent(
  'function g() { return globalThis; }\n' +
  'class FakeConnection {\n' +
  '  create(config) {\n' +
  '    g().__NB_CREATED_CONFIGS = g().__NB_CREATED_CONFIGS || [];\n' +
  '    g().__NB_CREATED_CONFIGS.push(config);\n' +
  '    return Promise.resolve(typeof g().__NB_CREATE_FD === "number" ?' +
  ' g().__NB_CREATE_FD : 99);\n' +
  '  }\n' +
  '  destroy() {\n' +
  '    g().__NB_DESTROYED = (g().__NB_DESTROYED || 0) + 1;\n' +
  '    return Promise.resolve();\n' +
  '  }\n' +
  '  protect(fd) {\n' +
  '    g().__NB_PROTECTED = (g().__NB_PROTECTED || 0) + 1;\n' +
  '    return Promise.resolve();\n' +
  '  }\n' +
  '}\n' +
  'export default {\n' +
  '  createVpnConnection: () => {\n' +
  '    g().__NB_CONNECTIONS = (g().__NB_CONNECTIONS || 0) + 1;\n' +
  '    return new FakeConnection();\n' +
  '  },\n' +
  '  VpnConnection: FakeConnection\n' +
  '};');

const CONNECTION = 'data:text/javascript,' + encodeURIComponent(
  'function netG() { return globalThis; }\n' +
  'function netThrow(name) {\n' +
  '  const c = (netG().__NB_NET_THROW_CODES || {})[name];\n' +
  '  if (c === undefined) { return; }\n' +
  '  const e = new Error("net-api:" + name);\n' +
  '  if (c !== "no-code") { e.code = c; }\n' +
  '  throw e;\n' +
  '}\n' +
  'function netHandleOf(v) {\n' +
  '  if (v !== null && typeof v === "object") {\n' +
  '    const e = new Error("net rejected"); e.code = v.code; throw e;\n' +
  '  }\n' +
  '  return { netId: Number(v === undefined ? 0 : v) };\n' +
  '}\n' +
  'function byNetId(name, handle) {\n' +
  '  return (netG()[name] || {})[handle ? handle.netId : 0];\n' +
  '}\n' +
  'export default {\n' +
  '  getAddressesByName: (host) => {\n' +
  '    const t = netG().__NB_DNS || {};\n' +
  '    const v = t[host];\n' +
  '    return v !== undefined && v !== null\n' +
  '      ? Promise.resolve([{ address: v }])\n' +
  '      : Promise.reject(new Error("dns-failed"));\n' +
  '  },\n' +
  '  getDefaultNetSync: () => {\n' +
  '    netThrow("getDefaultNetSync");\n' +
  '    return netHandleOf(netG().__NB_NET_DEFAULT_ID);\n' +
  '  },\n' +
  '  getAppNetSync: () => {\n' +
  '    netThrow("getAppNetSync");\n' +
  '    return netHandleOf(netG().__NB_NET_APP_ID);\n' +
  '  },\n' +
  '  getNetCapabilitiesSync: (handle) => {\n' +
  '    netThrow("getNetCapabilitiesSync");\n' +
  '    const c = byNetId("__NB_NET_CAPS_BY_ID", handle);\n' +
  '    if (c !== undefined && c !== null && c.error !== undefined) {\n' +
  '      const e = new Error("caps rejected"); e.code = c.error; throw e;\n' +
  '    }\n' +
  '    return c === undefined || c === null ? { networkCap: [], bearerTypes: [] } : c;\n' +
  '  },\n' +
  '  getConnectionPropertiesSync: (handle) => {\n' +
  '    netThrow("getConnectionPropertiesSync");\n' +
  '    netG().__NB_NET_PROPS_CALLS = (netG().__NB_NET_PROPS_CALLS || 0) + 1;\n' +
  '    const p = byNetId("__NB_NET_PROPS_BY_ID", handle);\n' +
  '    if (p !== undefined && p !== null && p.error !== undefined) {\n' +
  '      const e = new Error("props rejected"); e.code = p.error; throw e;\n' +
  '    }\n' +
  '    return { interfaceName: (p && p.interfaceName !== undefined && p.interfaceName !== null) ? p.interfaceName : "wlan0", dnses: [], routes: (p && p.routes) || [] };\n' +
  '  },\n' +
  '  getAllNetsSync: () => {\n' +
  '    netThrow("getAllNetsSync");\n' +
  '    netG().__NB_NET_GETALL_CALLS = (netG().__NB_NET_GETALL_CALLS || 0) + 1;\n' +
  '    const all = netG().__NB_NET_ALL_IDS || [];\n' +
  '    const out = [];\n' +
  '    for (let i = 0; i < all.length; i++) { out.push({ netId: all[i] }); }\n' +
  '    return out;\n' +
  '  }\n' +
  '};');

// The per-NIC byte counters (see the header comment): a number resolves,
// {code:n} rejects, an unset NIC rejects 2100003; every call is recorded.
const STATISTICS = 'data:text/javascript,' + encodeURIComponent(
  'function g() { return globalThis; }\n' +
  'function pick(table, method, nic) {\n' +
  '  const calls = g().__NB_STATS_CALLS || (g().__NB_STATS_CALLS = []);\n' +
  '  calls.push(method + ":" + nic);\n' +
  '  const v = (g()[table] || {})[nic];\n' +
  '  if (v === undefined) {\n' +
  '    const e = new Error("statistics unset: " + nic); e.code = 2100003;\n' +
  '    return Promise.reject(e);\n' +
  '  }\n' +
  '  if (v !== null && typeof v === "object") {\n' +
  '    const e = new Error("statistics rejected: " + nic); e.code = v.code;\n' +
  '    return Promise.reject(e);\n' +
  '  }\n' +
  '  return Promise.resolve(Number(v));\n' +
  '}\n' +
  'export default {\n' +
  '  getIfaceRxBytes: (nic) => pick("__NB_STATS_RX", "getIfaceRxBytes", nic),\n' +
  '  getIfaceTxBytes: (nic) => pick("__NB_STATS_TX", "getIfaceTxBytes", nic)\n' +
  '};');

const CORE = 'data:text/javascript,' + encodeURIComponent(
  'function g() { return globalThis; }\n' +
  'function j(v, fallback) {\n' +
  '  if (typeof v === "string") { return v; }\n' +
  '  return JSON.stringify(v === undefined ? fallback : v);\n' +
  '}\n' +
  'export default {\n' +
  '  fd_status: () => j(g().__NB_FD_STATUS, { open: false, errno: 9 }),\n' +
  '  connector_status: () => j(g().__NB_STATUS, {}),\n' +
  '  connector_network_config: () =>\n' +
  '    j(g().__NB_NETCFG, { available: false, reason: "no-network-map" }),\n' +
  '  connector_stop: () => JSON.stringify({ already_stopped: true, state: "stopped" }),\n' +
  '  connector_wg_socket_feed: () => JSON.stringify({ ok: true, device_up: true }),\n' +
  '  connector_tun_fd_feed: () => JSON.stringify({ ok: true, device_up: true }),\n' +
  '  connector_route_set_applied: (payload, isRecreate) => {\n' +
  '    g().__NB_ACK = { payload: payload, isRecreate: isRecreate };\n' +
  '    return JSON.stringify({ ok: true, recreate: { count: 1 } });\n' +
  '  },\n' +
  '  n2h_collector_arm: (optsJson) => {\n' +
  '    g().__NB_ARM_CALLS = (g().__NB_ARM_CALLS || 0) + 1;\n' +
  '    g().__NB_ARM_OPTS = optsJson;\n' +
  '    if (typeof g().__NB_ARM_ANSWER === "string") { return g().__NB_ARM_ANSWER; }\n' +
  '    return JSON.stringify({ ok: true, armed: true, opened_mono: 4242 });\n' +
  '  }\n' +
  '};');

const STUBS = {
  '@ohos.app.ability.VpnExtensionAbility': 'data:text/javascript,' + encodeURIComponent(
    'export default class VpnExtensionAbility {}'),
  // The fixture EntryAbility (arm private channel): a bare base class so the
  // real UIAbility subclass can be `new`-ed on the host; its `context` (with
  // filesDir) is assigned by the test before onCreate.
  '@ohos.app.ability.UIAbility': 'data:text/javascript,' + encodeURIComponent(
    'export default class UIAbility {}'),
  '@ohos.app.ability.AbilityConstant': EMPTY,
  '@ohos.app.ability.Want': EMPTY,
  '@ohos.window': EMPTY,
  '@ohos.file.fs': FS,
  '@ohos.hilog': HILOG,
  '@ohos.net.connection': CONNECTION,
  '@ohos.net.statistics': STATISTICS,
  '@ohos.net.vpnExtension': VPN_EXT,
  '@ohos.resourceManager': EMPTY,
  '@ohos.util': UTIL,
  'libnetbird_core.so': CORE
};

// Sibling modules of the extension: the ESM link would fail on the erased
// type-only named imports, so the hook redirects them to the shim files the
// test writes next to the byte-identical fixtures. NetBirdInterfaceStatsProbe
// has no erased types (the extension imports only its one exported function),
// so its entry is plain extension resolution for the extensionless specifier.
const SHIMS = {
  './NetBirdConnector': 'NetBirdConnector.shim.ts',
  './NetBirdEndpointExclusion': 'NetBirdEndpointExclusion.shim.ts',
  './NetBirdRouteValidation': 'NetBirdRouteValidation.shim.ts',
  './NetBirdVpnConfig': 'NetBirdVpnConfig.shim.ts',
  './NetBirdInterfaceStatsProbe': 'NetBirdInterfaceStatsProbe.ts'
};

export function resolve(specifier, context, next) {
  if (Object.prototype.hasOwnProperty.call(STUBS, specifier)) {
    return { url: STUBS[specifier], shortCircuit: true };
  }
  if (Object.prototype.hasOwnProperty.call(SHIMS, specifier) &&
    context.parentURL !== undefined) {
    const dir = new URL('.', context.parentURL);
    return { url: new URL(SHIMS[specifier], dir).href, shortCircuit: true };
  }
  return next(specifier, context);
}
