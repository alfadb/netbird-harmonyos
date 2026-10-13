// Loader-side resolve hook for the host-side fixtures that stage and drive the
// REAL NetBirdVpnExtensionAbility.ets: run-exclusion-generation.mts and —
// through route-apply-vpe-hooks.mjs — run-route-apply-vpe.mts (the live-route
// apply leg). Same principle as relay-exclusion-hooks.mjs / route-check-hooks.mjs:
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
//                     destroy() resolves and is counted in __NB_DESTROYED;
//                     protect() is counted in __NB_PROTECTED, APPENDS to
//                     __NB_PROBE_TRACE when the fixture seeded it, and is
//                     driven by globalThis.__NB_PROTECT_MODE — 'resolved'
//                     (default) / 'rejected' (Promise.reject) / 'throw'
//                     (SYNCHRONOUS throw) / 'never' (a promise that never
//                     settles: the wedge shape the per-fd bounded box reports
//                     as a timeout).
//                     ADDITIVE (2026-10-07 startup-race increment):
//                     __NB_PROTECT_PARK_AT = n parks the n-th protect() call
//                     (1 = the outbound WG socket, 2 = the FIRST ICE pool
//                     socket) and publishes its resolver as
//                     __NB_PROTECT_PARKED = {fd, resolve, reject}, so a fixture
//                     can hold the REAL initial leg inside the ICE protect
//                     await and release it explicitly. Unset (every
//                     pre-existing group) = the historical 'resolved' path,
//                     unchanged; the four modes above still win when set.
//                     protectProcessNet() (S3 diag switch,
//                     API 22+, SDK d.ts:275) is driven by
//                     globalThis.__NB_PROCESS_PROTECT_MODE —
//                     'resolved' (default) / 'rejected' (Promise.reject) /
//                     'throw' (SYNCHRONOUS throw) / 'never' (a promise that
//                     never settles, i.e. the device-observed wedge) — and
//                     every call is counted in __NB_PROCESS_PROTECT_CALLS.
//                     This is what lets the recreate leg AND the settleCreate
//                     S3 call site run to completion on the host.
//   - libnetbird_core.so -> the NAPI seam of the paths under test, each reading
//                     a per-test global:
//                       __NB_FD_STATUS   {open,errno} for core.fd_status(fd)
//                       __NB_NETCFG      connector_network_config() snapshot
//                       __NB_STATUS      connector_status() document
//                       __NB_ARM_OPTS    the arm options JSON the shell sent
//                     Since 2026-09-26 the protect-probe fd hand-off's two
//                     exports are functional too (S7 group):
//                     n2h_probe_socket_open() and
//                     n2h_probe_socket_commit(fd,result), driven by
//                     __NB_PROBE_OPEN_ANSWER / __NB_PROBE_FD /
//                     __NB_PROBE_COMMIT_ANSWER, counted in
//                     __NB_PROBE_OPEN_CALLS / __NB_PROBE_COMMIT_CALLS and
//                     recorded in __NB_PROBE_COMMIT_ARGS; together with the
//                     FakeConnection's protect() they append the ORDERED
//                     __NB_PROBE_TRACE (open / protect:<fd> / commit:<fd>:<r> /
//                     arm) when the fixture seeds that array.
//                     connector_stop / connector_wg_socket_feed /
//                     connector_tun_fd_feed / connector_route_set_applied
//                     answer success shapes (the ACK lands in __NB_ACK).
//                     Since 2026-09-23 the three management-socket seams of
//                     the REAL startConnectorThenCreate — mgmt_socket_open
//                     (__NB_MGMT_FD, default 21), connector_start_with_socket
//                     and connector_socket_feed — are functional too, because
//                     the S1/S2 groups drive the initial leg end-to-end.
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
  // READ_ONLY is the route adapter\'s read mode (NetBirdRouteStore.readTextBounded);
  // additive — every pre-existing caller only uses READ_WRITE|CREATE|TRUNC.
  '  OpenMode: { READ_ONLY: 0, READ_WRITE: 3, CREATE: 8, TRUNC: 16 },\n' +
  // 2026-10-10 the live-route request adapter (NetBirdRouteRequestStore) probes
  // a document with fs.statSync before reading it. This in-memory map keeps no
  // timestamps, so the probe answers "as if written right now" (and reports the
  // documented 13900002 for a path that was never written): the adapter\'s
  // unchanged-shortcut therefore never engages in these fixtures and every poll
  // still reads the real bytes. That is the conservative direction — a document
  // is never answered from a cache while its metadata is unknown here.
  '  statSync: (path) => {\n' +
  '    const f = files();\n' +
  '    if (!Object.prototype.hasOwnProperty.call(f, path)) {\n' +
  '      const e = new Error("stat failed: " + path); e.code = 13900002; throw e;\n' +
  '    }\n' +
  '    const now = Date.now();\n' +
  '    const nowNs = BigInt(now) * BigInt(1000000);\n' +
  '    return { ino: BigInt(1), size: Buffer.byteLength(String(f[path]), "utf8"),\n' +
  '      mtime: Math.floor(now / 1000), ctime: Math.floor(now / 1000),\n' +
  '      mtimeNs: nowNs, ctimeNs: nowNs };\n' +
  '  },\n' +
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
  '  unlinkSync: (path) => { delete files()[path]; },\n' +
  // The SDK's SYNCHRONOUS temp+fsync+rename document pattern, kept functional
  // for any staged writer that uses it: renameSync moves the stored text
  // exactly like the real 13900002-less happy path, and throws the SDK's
  // documented "not there" code when the source is missing (a writer always
  // renames a temp it just wrote).
  '  renameSync: (from, to) => {\n' +
  '    const f = files();\n' +
  '    if (!Object.prototype.hasOwnProperty.call(f, from)) {\n' +
  '      const e = new Error("rename failed: " + from); e.code = 13900002; throw e;\n' +
  '    }\n' +
  '    f[to] = f[from]; delete f[from];\n' +
  '  },\n' +
  '  fsyncSync: () => {},\n' +
  // 2026-10-07 per-NetID route selection (NetBirdRouteStore.ets): the route
  // adapter uses the ASYNC half of the same SDK over the SAME in-memory map,
  // so the REAL store (staged byte-identically by the fixture) runs unchanged:
  // READ_ONLY open of a missing path throws the documented 13900002 (which the
  // store maps to "no document" = default all-on), __NB_FS_OPEN_ERROR[path]
  // still simulates a real open failure (mapped to "unavailable"), read serves
  // the stored text at buffer position 0 (the SDK semantics), write stores the
  // caller's UTF-8 bytes, and rename/unlink are the atomic-swap legs.
  '  async open(path, mode) {\n' +
  '    const code = (g().__NB_FS_OPEN_ERROR || {})[path];\n' +
  '    if (code !== undefined) {\n' +
  '      const e = new Error("open failed: " + path); e.code = code; throw e;\n' +
  '    }\n' +
  '    if (mode === 0 && !Object.prototype.hasOwnProperty.call(files(), path)) {\n' +
  '      const e = new Error("open failed: " + path); e.code = 13900002; throw e;\n' +
  '    }\n' +
  '    return { fd: path };\n' +
  '  },\n' +
  '  async read(fd, arrayBuffer, opts) {\n' +
  '    const text = files()[fd];\n' +
  '    if (text === undefined) {\n' +
  '      const e = new Error("read failed: " + fd); e.code = 13900002; throw e;\n' +
  '    }\n' +
  '    const bytes = Buffer.from(String(text), "utf8");\n' +
  '    const offset = (opts && typeof opts.offset === "number") ? opts.offset : 0;\n' +
  '    const length = (opts && typeof opts.length === "number") ? opts.length : bytes.length;\n' +
  '    if (offset >= bytes.length) { return 0; }\n' +
  '    const count = Math.min(length, bytes.length - offset);\n' +
  '    Buffer.from(arrayBuffer).set(bytes.subarray(offset, offset + count), 0);\n' +
  '    return count;\n' +
  '  },\n' +
  '  async write(fd, bytes) {\n' +
  '    const text = Buffer.from(bytes).toString("utf8");\n' +
  '    files()[fd] = text;\n' +
  '    return Buffer.byteLength(text, "utf8");\n' +
  '  },\n' +
  '  async fsync(fd) {},\n' +
  '  async close(file) {},\n' +
  '  async rename(from, to) {\n' +
  '    const f = files();\n' +
  '    if (!Object.prototype.hasOwnProperty.call(f, from)) {\n' +
  '      const e = new Error("rename failed: " + from); e.code = 13900002; throw e;\n' +
  '    }\n' +
  '    f[to] = f[from]; delete f[from];\n' +
  '  },\n' +
  '  async unlink(path) { delete files()[path]; }\n' +
  '};');

// Real UTF-8 encoding/decoding for the fixture (the platform util.TextEncoder /
// util.TextDecoder contract): the route adapter encodes the documents it writes
// and decodes the ones it reads with the SAME module, fatal mode included.
const UTIL = 'data:text/javascript,' + encodeURIComponent(
  'class TextEncoder {\n' +
  '  encode(input) { return Uint8Array.from(Buffer.from(String(input ?? ""), "utf8")); }\n' +
  '}\n' +
  'class Decoder {\n' +
  '  constructor(encoding, options) { this.d = new globalThis.TextDecoder(encoding, options); }\n' +
  '  decodeToString(bytes) { return this.d.decode(bytes); }\n' +
  '}\n' +
  'export default { TextEncoder: TextEncoder,\n' +
  '  TextDecoder: { create(encoding, options) { return new Decoder(encoding, options); } } };');

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
  // 2026-09-26 (S7 protect-probe hand-off): an OPTIONAL ordered call trace
  // (globalThis.__NB_PROBE_TRACE) the fixture seeds when it wants the
  // open -> protect -> commit -> arm order as one observable sequence. Unset
  // (the default for every pre-existing group) = not recorded at all.
  '    if (g().__NB_PROBE_TRACE !== undefined) {\n' +
  '      g().__NB_PROBE_TRACE.push("protect:" + fd);\n' +
  '    }\n' +
  '    const mode = g().__NB_PROTECT_MODE || "resolved";\n' +
  '    if (mode === "never") { return new Promise(() => {}); }\n' +
  // S7: the two failure shapes the dedicated probe wrapper maps to
  // unprotected (reject) / error (SYNCHRONOUS throw). Additive: the default
  // stays "resolved", so no pre-existing group changes shape.
  '    if (mode === "rejected") { return Promise.reject(new Error("protect-rejected")); }\n' +
  '    if (mode === "throw") { throw new Error("protect-throw"); }\n' +
  // 2026-10-07 startup-race increment (ADDITIVE): park the n-th protect call
  // and publish its resolver. The executor below runs synchronously, so
  // __NB_PROTECT_PARKED is observable the moment protect() returns. Unset =
  // byte-for-byte the historical resolved path.
  '    const parkAt = g().__NB_PROTECT_PARK_AT;\n' +
  '    if (typeof parkAt === "number" && parkAt > 0 && g().__NB_PROTECTED === parkAt) {\n' +
  '      return new Promise((resolve, reject) => {\n' +
  '        g().__NB_PROTECT_PARKED = { fd: fd, resolve: resolve, reject: reject };\n' +
  '      });\n' +
  '    }\n' +
  '    return Promise.resolve();\n' +
  '  }\n' +
  '  protectProcessNet() {\n' +
  '    g().__NB_PROCESS_PROTECT_CALLS = (g().__NB_PROCESS_PROTECT_CALLS || 0) + 1;\n' +
  '    const mode = g().__NB_PROCESS_PROTECT_MODE || "resolved";\n' +
  '    if (mode === "throw") { throw new Error("process-protect-throw"); }\n' +
  '    if (mode === "rejected") { return Promise.reject(new Error("process-protect-rejected")); }\n' +
  '    if (mode === "never") { return new Promise(() => {}); }\n' +
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
  // N2 first batch C/D columns (2026-09-29): `linkAddresses` is the raw
  // platform fact the pure `classifyN2Bearer` records as `local=`; an unseeded
  // netId answers none (never an invented address). `dnses` is passed through
  // for the same reason: `physicalDefaultNetId()` only calls a network physical
  // when it has a usable IPv4 resolver, so a fixture that wants a MEASURED
  // bearer must seed one — an unseeded network still answers [].
  '    return { interfaceName: (p && p.interfaceName !== undefined && p.interfaceName !== null) ? p.interfaceName : "wlan0", dnses: (p && p.dnses) || [], routes: (p && p.routes) || [], linkAddresses: (p && p.linkAddresses) || [] };\n' +
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
  // 2026-09-30 (formal-switch production-recovery gate): the CORE-side wall
  // clock the C/D default answers read. A fixture may advance it
  // (`__NB_CD_CORE_NOW`) instead of sleeping 3 s, which is how "the D column's
  // DWELL_MIN_MS was satisfied" is expressed deterministically.
  'function coreNow() {\n' +
  '  return typeof g().__NB_CD_CORE_NOW === "number" ? g().__NB_CD_CORE_NOW : Date.now();\n' +
  '}\n' +
  'export default {\n' +
  '  n2_socket_trace_arm: () => JSON.stringify({ ok: true }),\n' +
  '  n2_socket_trace_protect: () => JSON.stringify({ ok: true }),\n' +
  // N2 T5a「B 断流刺激」(2026-09-28): the shell's armed-only stimulus entry.
  // Functional and recording like the other seams — every call is counted and
  // the answer is overridable per test (`__NB_STIMULUS_ANSWER`), so a fixture
  // can drive the REAL poll path and assert the request that reached the core.
  // Default answer = the honest not-armed refusal, never a fabricated applied.
  '  n2_stimulus: (kind, id) => {\n' +
  '    g().__NB_STIMULUS_CALLS = (g().__NB_STIMULUS_CALLS || 0) + 1;\n' +
  '    g().__NB_STIMULUS_LAST = { kind: kind, id: id };\n' +
  '    if (typeof g().__NB_STIMULUS_ANSWER === "string") { return g().__NB_STIMULUS_ANSWER; }\n' +
  '    return JSON.stringify({ ok: false, result: "rejected", reason: "not-armed", target_fd: -1 });\n' +
  '  },\n' +
  '  fd_status: () => j(g().__NB_FD_STATUS, { open: false, errno: 9 }),\n' +
  '  connector_status: () => j(g().__NB_STATUS, {}),\n' +
  '  connector_network_config: () =>\n' +
  '    j(g().__NB_NETCFG, { available: false, reason: "no-network-map" }),\n' +
  // 2026-10-07 route catalog getter (decision B): the native `connector_routes`
  // document the VPE mirrors into netbird-routes.json. Functional and
  // overridable like the two seams above: a STRING answer is returned verbatim
  // (the malformed-document test), an object is stringified, and the DEFAULT is
  // the honest native "no route catalog" state (available:false) — so the
  // pre-existing suites never publish a catalog and change no shape.
  '  connector_routes: () =>\n' +
  '    j(g().__NB_ROUTES, { available: false, serial: 0,\n' +
  '      selection: { selected_routes: [], deselected_routes: [], deselect_all: false },\n' +
  '      networks: [] }),\n' +
  // S1/S2 initial-leg drive (2026-09-23): the REAL startConnectorThenCreate is
  // driven end-to-end for the two config switches, so the three management
  // socket seams it walks are functional here. All three are additive — no
  // pre-existing group reached them (the arm/config groups drive the private
  // build/arm methods, never the start flow), so nothing else changes shape.
  '  mgmt_socket_open: () => {\n' +
  '    g().__NB_SUPPLY_MGMT_OPEN = (g().__NB_SUPPLY_MGMT_OPEN || 0) + 1;\n' +
  '    return JSON.stringify({ fd: typeof g().__NB_MGMT_FD === "number" ? g().__NB_MGMT_FD : 21,\n' +
  '      bind_rc: 0, bind_errno: 0 });\n' +
  '  },\n' +
  '  connector_start_with_socket: (fd, configJson, setupKeyJson, addrJson) => {\n' +
  // 2026-10-07 route selection: the config JSON the shell actually handed the
  // native start is recorded so a fixture can assert the startup selection (and
  // its ABSENCE) on the real call, not on a mirror. Additive: the answer is
  // unchanged.
  '    g().__NB_START_CALLS = (g().__NB_START_CALLS || 0) + 1;\n' +
  '    g().__NB_START_LAST = { fd: fd, config: configJson, creds: setupKeyJson,\n' +
  '      addr: addrJson };\n' +
  '    return JSON.stringify({ started: true, state: "running" });\n' +
  '  },\n' +
  '  connector_socket_feed: () => {\n' +
  '    g().__NB_SUPPLY_MGMT_FEED = (g().__NB_SUPPLY_MGMT_FEED || 0) + 1;\n' +
  '    return JSON.stringify({ ok: true, queued: 1 });\n' +
  '  },\n' +
  '  connector_signal_socket_feed: () => {\n' +
  '    g().__NB_SUPPLY_SIGNAL_FEED = (g().__NB_SUPPLY_SIGNAL_FEED || 0) + 1;\n' +
  '    return JSON.stringify({ ok: true, queued: 1 });\n' +
  '  },\n' +
  '  connector_stop: () => JSON.stringify({ already_stopped: true, state: "stopped" }),\n' +
  // Explicit logout (2026-10-01): these suites never write a `netbird-logout`
  // ticket, so the poll never reaches the seam; the export exists so an
  // accidental call fails loudly with the honest no-connector refusal
  // instead of a TypeError.
  '  connector_logout: () => JSON.stringify({ ok: false, error: "no-connector",\n' +
  '    reason: "not-running" }),\n' +
  // N2 first batch C/D columns (2026-09-29): the observer's switch path calls
  // the connector's "the physical network changed" seam; the N19/N22 fixtures
  // drive `physicalNetworkAvailable` directly, so it must exist and be counted.
  '  connector_physical_network_changed: (addressJson) => {\n' +
  '    g().__NB_PHYSICAL_CHANGED = (g().__NB_PHYSICAL_CHANGED || 0) + 1;\n' +
  // F3 (v0.0.27 outage REPORT.md §5): the ARGUMENT is recorded verbatim. The
  // pre-F3 shell sent `''` here on every switch, which made the core empty its
  // DNS + STUN-DNS lists; `run-network-recovery.mts` asserts the seam now always
  // carries a resolved address.
  '    g().__NB_PHYSICAL_CHANGED_ARG = String(addressJson);\n' +
  // 2026-09-30: the ordered trace the gate's own sequence is judged on (the
  // core cleanup must precede the new network's pre control).
  '    g().__NB_ORDER = (g().__NB_ORDER || []).concat(["physical-changed"]);\n' +
  '    return JSON.stringify({ ok: true });\n' +
  '  },\n' +
  // 2026-09-30 (formal-switch production-recovery gate): every seam that hands
  // the core a production socket is now COUNTED (`__NB_SUPPLY_*`, one counter
  // per entry), so a fixture can assert "the gate let NOTHING through" as a
  // call count on the REAL shell path instead of static text. The answers stay
  // the happy path so an ungated drive behaves exactly as before.
  '  wg_fwd_open: () => {\n' +
  '    g().__NB_SUPPLY_WG_OPEN = (g().__NB_SUPPLY_WG_OPEN || 0) + 1;\n' +
  '    return JSON.stringify({ fd: typeof g().__NB_WG_FD === "number" ? g().__NB_WG_FD : 61,\n' +
  '      bind_rc: 0, bind_errno: 0 });\n' +
  '  },\n' +
  '  ice_socket_open: () => {\n' +
  '    g().__NB_SUPPLY_ICE_OPEN = (g().__NB_SUPPLY_ICE_OPEN || 0) + 1;\n' +
  '    return JSON.stringify({ fd: typeof g().__NB_ICE_FD === "number" ? g().__NB_ICE_FD : 62,\n' +
  '      bind_rc: 0, bind_errno: 0 });\n' +
  '  },\n' +
  // STAGE 2/3 (official-alignment audit §2.7): the ONE shared mux — since stage 3
  // built OVER the already-protected WG outer socket and started there, so the
  // seam takes no fd and opens nothing (`{ok,port,replaced,reader}`). Counted like
  // every other supply seam so a fixture can assert "nothing was let through".
  '  connector_ice_mux_arm: () => {\n' +
  '    g().__NB_SUPPLY_ICE_MUX_ARM = (g().__NB_SUPPLY_ICE_MUX_ARM || 0) + 1;\n' +
  '    return JSON.stringify({ ok: true,\n' +
  '      port: typeof g().__NB_ICE_MUX_PORT === "number" ? g().__NB_ICE_MUX_PORT : 41000,\n' +
  '      replaced: false, reader: true });\n' +
  '  },\n' +
  '  socket_supply_dns_servers: () => {\n' +
  '    g().__NB_SUPPLY_DNS = (g().__NB_SUPPLY_DNS || 0) + 1;\n' +
  '    return JSON.stringify({ ok: true });\n' +
  '  },\n' +
  '  connector_management_address_update: () => {\n' +
  '    g().__NB_SUPPLY_MGMT_ADDR = (g().__NB_SUPPLY_MGMT_ADDR || 0) + 1;\n' +
  '    return JSON.stringify({ ok: true });\n' +
  '  },\n' +
  // The on-demand supply channel. Since 0.0.27 it is EVENT-DRIVEN: the shell
  // registers `socket_supply_subscribe(cb)` and this stub captures that callback
  // in `g().__NB_SUPPLY_WAKE`, so a fixture can fire the core's enqueue wake by
  // hand (the only path that services a request before the 5 s safety net).
  // `socket_supply_poll` answers `__NB_SOCKET_SUPPLY_QUEUE` element by element
  // first (a queue a fixture fills to prove that ONE wake drains MANY requests)
  // and then the single `__NB_SOCKET_SUPPLY_REQUEST` (the pre-0.0.27 shape, kept
  // so every pre-existing fixture is unchanged); default = nothing pending. The
  // reply is recorded so "refused with which reason" is observable at the core
  // seam too.
  '  socket_supply_poll: () => {\n' +
  '    g().__NB_SUPPLY_POLL = (g().__NB_SUPPLY_POLL || 0) + 1;\n' +
  '    const queue = g().__NB_SOCKET_SUPPLY_QUEUE;\n' +
  '    if (Array.isArray(queue) && queue.length > 0) { return JSON.stringify(queue.shift()); }\n' +
  '    return JSON.stringify(g().__NB_SOCKET_SUPPLY_REQUEST ||\n' +
  '      { id: "", kind: "", purpose: "" });\n' +
  '  },\n' +
  '  socket_supply_subscribe: (wake) => {\n' +
  '    g().__NB_SUPPLY_WAKE = wake;\n' +
  '    g().__NB_SUPPLY_SUBSCRIBE = (g().__NB_SUPPLY_SUBSCRIBE || 0) + 1;\n' +
  '    return JSON.stringify({ ok: true });\n' +
  '  },\n' +
  '  socket_supply_unsubscribe: () => {\n' +
  '    g().__NB_SUPPLY_WAKE = undefined;\n' +
  '    g().__NB_SUPPLY_UNSUBSCRIBE = (g().__NB_SUPPLY_UNSUBSCRIBE || 0) + 1;\n' +
  '    return JSON.stringify({ ok: true });\n' +
  '  },\n' +
  '  socket_supply_reply: (id, fd, result) => {\n' +
  '    g().__NB_SUPPLY_REPLY = (g().__NB_SUPPLY_REPLY || 0) + 1;\n' +
  '    g().__NB_SUPPLY_REPLY_IDS = (g().__NB_SUPPLY_REPLY_IDS || []).concat([id]);\n' +
  '    g().__NB_SUPPLY_REPLY_LAST = { id: id, fd: fd, result: result };\n' +
  '    return JSON.stringify({ ok: result === "protected" });\n' +
  '  },\n' +
  // The shell-side protected-DNS resolution the recovery awaits. Parked by
  // default on the fixture answer (`__NB_DNS_POLL`), so a fixture can hold a
  // REAL in-flight recovery at this await and release it later.
  '  connector_protected_dns_start: () => {\n' +
  '    g().__NB_SUPPLY_DNS_RESOLVE = (g().__NB_SUPPLY_DNS_RESOLVE || 0) + 1;\n' +
  '    return JSON.stringify({ ok: true, id: 1, pending: true });\n' +
  '  },\n' +
  '  connector_protected_dns_poll: () => JSON.stringify(g().__NB_DNS_POLL ||\n' +
  '    { ok: true, pending: false, ip: "203.0.113.7" }),\n' +
  '  socket_supply_shutdown: () => JSON.stringify({ ok: true }),\n' +
  '  socket_supply_close_fd: () => JSON.stringify({ ok: true }),\n' +
  '  connector_wg_socket_feed: () => {\n' +
  '    g().__NB_SUPPLY_WG_FEED = (g().__NB_SUPPLY_WG_FEED || 0) + 1;\n' +
  '    g().__NB_ORDER = (g().__NB_ORDER || []).concat(["recover-feed"]);\n' +
  '    return JSON.stringify({ ok: true, device_up: true });\n' +
  '  },\n' +
  '  connector_tun_fd_feed: () => JSON.stringify({ ok: true, device_up: true }),\n' +
  // roam-1005-0124: the recreate leg's pre-destroy TUN release. Counted and
  // overridable per test like the other seams; the default is the happy
  // path (a live device's dup closed).
  '  connector_tun_release: () => {\n' +
  '    g().__NB_TUN_RELEASE_CALLS = (g().__NB_TUN_RELEASE_CALLS || 0) + 1;\n' +
  '    if (typeof g().__NB_TUN_RELEASE_ANSWER === "string") { return g().__NB_TUN_RELEASE_ANSWER; }\n' +
  '    return JSON.stringify({ ok: true, released: true, fd: 63, device_up: true });\n' +
  '  },\n' +
  '  connector_route_set_applied: (payload, isRecreate) => {\n' +
  '    g().__NB_ACK = { payload: payload, isRecreate: isRecreate };\n' +
  '    return JSON.stringify({ ok: true, recreate: { count: 1 } });\n' +
  '  },\n' +
  '  n2h_collector_arm: (optsJson) => {\n' +
  '    if (g().__NB_PROBE_TRACE !== undefined) { g().__NB_PROBE_TRACE.push("arm"); }\n' +
  '    g().__NB_ARM_CALLS = (g().__NB_ARM_CALLS || 0) + 1;\n' +
  '    g().__NB_ARM_OPTS = optsJson;\n' +
  '    if (typeof g().__NB_ARM_ANSWER === "string") { return g().__NB_ARM_ANSWER; }\n' +
  '    return JSON.stringify({ ok: true, armed: true, opened_mono: 4242 });\n' +
  '  },\n' +
  // 2026-09-26 (S7 protect-probe fd hand-off, plan §4/§6 item 5): the two NAPI
  // exports the shell's half of the hand-off calls, with the ice_socket_open
  // JSON shapes. Per-test globals, all defaulted to the happy path:
  //   __NB_PROBE_OPEN_ANSWER   string|unset  -> an explicit open answer
  //                                            (e.g. {ok:false,error:…,errno:…})
  //                                            or a malformed body
  //   __NB_PROBE_FD            number        -> the fd open answers (default 71)
  //   __NB_PROBE_COMMIT_ANSWER string|unset  -> an explicit commit answer
  //   __NB_PROBE_TRACE         array|unset   -> the ordered call trace the
  //                                            fixture seeds ('open' /
  //                                            'protect:<fd>' / 'commit:<fd>:
  //                                            <result>' / 'arm')
  //   __NB_PROBE_OPEN_CALLS / __NB_PROBE_COMMIT_CALLS / __NB_PROBE_COMMIT_ARGS
  //                          -> the counters/arguments, so "no open" and "the
  //                             commit got the very fd open returned" are
  //                             behavioural assertions.
  '  n2h_probe_socket_open: () => {\n' +
  '    if (g().__NB_PROBE_TRACE !== undefined) { g().__NB_PROBE_TRACE.push("open"); }\n' +
  '    g().__NB_PROBE_OPEN_CALLS = (g().__NB_PROBE_OPEN_CALLS || 0) + 1;\n' +
  '    if (typeof g().__NB_PROBE_OPEN_ANSWER === "string") { return g().__NB_PROBE_OPEN_ANSWER; }\n' +
  '    return JSON.stringify({ ok: true,\n' +
  '      fd: typeof g().__NB_PROBE_FD === "number" ? g().__NB_PROBE_FD : 71,\n' +
  '      handoff_id: "N2H-protect-0123456789abcdef01234567",\n' +
  '      bind_rc: 0, bind_errno: 0 });\n' +
  '  },\n' +
  '  n2h_probe_socket_commit: (fd, result) => {\n' +
  '    if (g().__NB_PROBE_TRACE !== undefined) {\n' +
  '      g().__NB_PROBE_TRACE.push("commit:" + fd + ":" + result);\n' +
  '    }\n' +
  '    g().__NB_PROBE_COMMIT_CALLS = (g().__NB_PROBE_COMMIT_CALLS || 0) + 1;\n' +
  '    g().__NB_PROBE_COMMIT_ARGS = (g().__NB_PROBE_COMMIT_ARGS || []).concat([[fd, result]]);\n' +
  '    if (typeof g().__NB_PROBE_COMMIT_ANSWER === "string") {\n' +
  '      return g().__NB_PROBE_COMMIT_ANSWER;\n' +
  '    }\n' +
  '    return JSON.stringify({ ok: true, fd: fd,\n' +
  '      handoff_id: "N2H-protect-0123456789abcdef01234567", protect_result: result });\n' +
  '  },\n' +
  // N2 P4 (2026-09-28): the A-column trigger entry. Functional and recording
  // like the other seams: every call is counted and the request lands in
  // __NB_A_EVENT_LAST; __NB_A_EVENT_ANSWER overrides the answer. The default
  // is the honest not-armed refusal, never a fabricated dispatch.
  '  n2_a_event: (kind, id) => {\n' +
  '    g().__NB_A_EVENT_CALLS = (g().__NB_A_EVENT_CALLS || 0) + 1;\n' +
  '    g().__NB_A_EVENT_LAST = { kind: kind, id: id };\n' +
  '    if (typeof g().__NB_A_EVENT_ANSWER === "string") { return g().__NB_A_EVENT_ANSWER; }\n' +
  '    return JSON.stringify({ ok: false, result: "rejected", reason: "not-armed",\n' +
  '      kind: kind, fd: -1, dup_fd: -1 });\n' +
  '  },\n' +
  // N2 P5 (2026-09-28): the automatic sandwich orchestration seams. `pre`
  // defaults to the honest refusal (an unarmed/unattached session), `post`
  // records the shell's verdict and `poll` answers "nothing ready".
  '  n2_sandwich_pre: (kind, marker) => {\n' +
  '    g().__NB_SANDWICH_PRE_CALLS = (g().__NB_SANDWICH_PRE_CALLS || 0) + 1;\n' +
  '    g().__NB_SANDWICH_PRE_LAST = { kind: kind, marker: marker };\n' +
  '    if (typeof g().__NB_SANDWICH_PRE_ANSWER === "string") {\n' +
  '      return g().__NB_SANDWICH_PRE_ANSWER;\n' +
  '    }\n' +
  '    return JSON.stringify({ ok: false, result: "rejected", reason: "not-armed",\n' +
  '      kind: kind, marker: marker });\n' +
  '  },\n' +
  '  n2_sandwich_post: (kind, marker, result, reason) => {\n' +
  '    g().__NB_SANDWICH_POST_CALLS = (g().__NB_SANDWICH_POST_CALLS || 0) + 1;\n' +
  '    g().__NB_SANDWICH_POST_LAST = { kind: kind, marker: marker, result: result,\n' +
  '      reason: reason };\n' +
  '    return JSON.stringify({ ok: true, result: result, reason: reason, kind: kind,\n' +
  '      marker: marker });\n' +
  '  },\n' +
  '  n2_sandwich_poll: () => {\n' +
  '    g().__NB_SANDWICH_POLL_CALLS = (g().__NB_SANDWICH_POLL_CALLS || 0) + 1;\n' +
  '    if (typeof g().__NB_SANDWICH_POLL_ANSWER === "string") {\n' +
  '      return g().__NB_SANDWICH_POLL_ANSWER;\n' +
  '    }\n' +
  '    return JSON.stringify({ ok: true, posts: [] });\n' +
  '  },\n' +
  // N2 first batch C/D columns (2026-09-29): the switching-sandwich seams and
  // the armed-only evidence ring. Functional and recording like the other N2
  // seams, so a fixture can drive the REAL plan/switch/step path and assert
  // exactly what reached the core; every answer is overridable per test and the
  // defaults are the honest refusals (never a fabricated acceptance).
  '  n2_cd_plan: (id, column, kind, rep, target, netBefore, localBefore, startVerdict) => {\n' +
  '    g().__NB_CD_PLAN_CALLS = (g().__NB_CD_PLAN_CALLS || 0) + 1;\n' +
  '    g().__NB_CD_PLAN_LAST = { id: id, column: column, kind: kind, rep: rep, target: target,\n' +
  '      netBefore: netBefore, localBefore: localBefore, startVerdict: startVerdict };\n' +
  // The MINIMAL core-side plan mirror the default answers below consult, so a
  // default `switched` can never be handed back for a state the real core would
  // refuse (D before its preparation recovery, or before DWELL_MIN_MS). It
  // records only what the shell reports; an explicit `__NB_CD_*_ANSWER` always
  // wins and is never second-guessed.
  '    g().__NB_CD_CORE = { id: id, column: column,\n' +
  '      phase: column === "D" ? "await-start-bearer" : "ready", readyMs: 0 };\n' +
  '    if (typeof g().__NB_CD_PLAN_ANSWER === "string") { return g().__NB_CD_PLAN_ANSWER; }\n' +
  '    return JSON.stringify({ ok: false, result: "rejected", reason: "not-armed", id: id,\n' +
  '      pre_marker: id + "-pre", post_marker: id + "-post", line: "" });\n' +
  '  },\n' +
  '  n2_cd_pre: (id, result, reason) => {\n' +
  '    g().__NB_CD_PRE_CALLS = (g().__NB_CD_PRE_CALLS || 0) + 1;\n' +
  '    g().__NB_CD_PRE_LAST = { id: id, result: result, reason: reason };\n' +
  '    if (typeof g().__NB_CD_PRE_ANSWER === "string") { return g().__NB_CD_PRE_ANSWER; }\n' +
  '    return JSON.stringify({ ok: true, result: result, reason: reason, id: id, line: "" });\n' +
  '  },\n' +
  // D column: the Wi-Fi -> cellular preparation hop and its cellular-side
  // recovery. Recorded like the rest; the defaults are the honest recorded
  // answers of the core's own state machine.
  '  n2_cd_prepare: (id, verdict, netAfter, localAfter) => {\n' +
  '    g().__NB_CD_PREPARE_CALLS = (g().__NB_CD_PREPARE_CALLS || 0) + 1;\n' +
  '    g().__NB_CD_PREPARE_LAST = { id: id, verdict: verdict, netAfter: netAfter,\n' +
  '      localAfter: localAfter };\n' +
  '    if (typeof g().__NB_CD_PREPARE_ANSWER === "string") { return g().__NB_CD_PREPARE_ANSWER; }\n' +
  // A non-empty shell verdict is a terminal refusal in the real core; the
  // mirror follows the report only on the honest path.
  '    if (verdict !== "") {\n' +
  '      return JSON.stringify({ ok: false, result: "unsat", reason: verdict, id: id, line: "" });\n' +
  '    }\n' +
  '    const st = g().__NB_CD_CORE;\n' +
  '    if (st && st.id === id) { st.phase = "preparation"; }\n' +
  '    return JSON.stringify({ ok: true, result: "prepared", reason: "not-a-d-measurement",\n' +
  '      id: id, line: "" });\n' +
  '  },\n' +
  '  n2_cd_ready: (id) => {\n' +
  '    g().__NB_CD_READY_CALLS = (g().__NB_CD_READY_CALLS || 0) + 1;\n' +
  '    g().__NB_CD_READY_LAST = { id: id };\n' +
  '    if (typeof g().__NB_CD_READY_ANSWER === "string") { return g().__NB_CD_READY_ANSWER; }\n' +
  '    const st = g().__NB_CD_CORE;\n' +
  '    if (!st || st.id !== id || st.phase !== "preparation") {\n' +
  '      return JSON.stringify({ ok: false, result: "rejected", reason: "recovery-not-owed",\n' +
  '        id: id, line: "" });\n' +
  '    }\n' +
  '    st.phase = "ready";\n' +
  '    st.readyMs = coreNow();\n' +
  '    return JSON.stringify({ ok: true, result: "ready", reason: "preparation-recovered",\n' +
  '      id: id, line: "" });\n' +
  '  },\n' +
  // The identity baseline the switch report carries. Default = the honest
  // "no live registration of this kind" (every field -1 / 0), so a test that
  // wants a measured baseline must say so with `__NB_CD_IDENTITY_ANSWER`.
  '  n2_cd_identity: (kind) => {\n' +
  '    g().__NB_CD_IDENTITY_CALLS = (g().__NB_CD_IDENTITY_CALLS || 0) + 1;\n' +
  '    g().__NB_CD_IDENTITY_LAST = { kind: kind };\n' +
  '    g().__NB_ORDER = (g().__NB_ORDER || []).concat(["identity"]);\n' +
  '    if (typeof g().__NB_CD_IDENTITY_ANSWER === "string") {\n' +
  '      return g().__NB_CD_IDENTITY_ANSWER;\n' +
  '    }\n' +
  '    return JSON.stringify({ ok: false, fd: -1, dup_fd: -1, generation: -1, net_id: 0 });\n' +
  '  },\n' +
  // The IDENTIFIED switch report (the five-argument `n2_cd_switch` was removed):
  // the identity triple rides the same call.
  '  n2_cd_switch_identified: (id, verdict, netAfter, localAfter, netId, fd, dupFd, generation) => {\n' +
  '    g().__NB_CD_SWITCH_CALLS = (g().__NB_CD_SWITCH_CALLS || 0) + 1;\n' +
  '    g().__NB_CD_SWITCH_LAST = { id: id, verdict: verdict, netAfter: netAfter,\n' +
  '      localAfter: localAfter, netId: netId, fd: fd, dupFd: dupFd, generation: generation };\n' +
  '    if (typeof g().__NB_CD_SWITCH_ANSWER === "string") { return g().__NB_CD_SWITCH_ANSWER; }\n' +
  // The default answer follows the real core's own preconditions, so a fixture
  // can never "switch" a D plan the core would refuse (see `__NB_CD_CORE`).
  '    const st = g().__NB_CD_CORE;\n' +
  '    if (verdict !== "") {\n' +
  '      return JSON.stringify({ ok: false, result: "unsat", reason: verdict, id: id, line: "" });\n' +
  '    }\n' +
  '    if (st && st.id === id) {\n' +
  '      if (st.phase === "await-start-bearer") {\n' +
  '        return JSON.stringify({ ok: false, result: "unsat", reason: "prepare-hop-required",\n' +
  '          id: id, line: "" });\n' +
  '      }\n' +
  '      if (st.phase === "preparation") {\n' +
  '        return JSON.stringify({ ok: false, result: "unsat",\n' +
  '          reason: "switch-before-preparation-complete", id: id, line: "" });\n' +
  '      }\n' +
  '      if (st.column === "D" && coreNow() - st.readyMs < 3000) {\n' +
  '        return JSON.stringify({ ok: false, result: "unsat", reason: "dwell-min-not-reached",\n' +
  '          id: id, line: "" });\n' +
  '      }\n' +
  '      st.phase = "switched";\n' +
  '    }\n' +
  '    return JSON.stringify({ ok: true, result: "switched", reason: "ok", id: id, line: "" });\n' +
  '  },\n' +
  '  n2_cd_post: (id, result, reason) => {\n' +
  '    g().__NB_CD_POST_CALLS = (g().__NB_CD_POST_CALLS || 0) + 1;\n' +
  '    g().__NB_CD_POST_LAST = { id: id, result: result, reason: reason };\n' +
  '    if (typeof g().__NB_CD_POST_ANSWER === "string") { return g().__NB_CD_POST_ANSWER; }\n' +
  '    return JSON.stringify({ ok: true, result: result, reason: reason, id: id, line: "" });\n' +
  '  },\n' +
  // The answer may be a string (one fixed step) or an ARRAY (a queue: the
  // n-th poll answers the n-th element, later polls fall back to the idle
  // default). A queue is what makes the REAL switch-instant chain
  // (step=pre -> step=recover -> step=receipt) deterministic.
  '  n2_cd_poll: () => {\n' +
  '    g().__NB_CD_POLL_CALLS = (g().__NB_CD_POLL_CALLS || 0) + 1;\n' +
  '    if (typeof g().__NB_CD_POLL_ANSWER === "string") { return g().__NB_CD_POLL_ANSWER; }\n' +
  '    const queue = g().__NB_CD_POLL_ANSWER_QUEUE;\n' +
  '    if (Array.isArray(queue) && queue.length > 0) { return String(queue.shift()); }\n' +
  '    return JSON.stringify({ ok: true, step: "idle", phase: "", id: "", kind: "",\n' +
  '      column: "", fd: -1, dup_fd: -1, remote: "unknown", marker: "", result: "", line: "" });\n' +
  '  },\n' +
  '  n2_ring_configure: (path, maxBytes, segments) => {\n' +
  '    g().__NB_RING_CONFIG = { path: path, maxBytes: maxBytes, segments: segments };\n' +
  '    if (typeof g().__NB_RING_CONFIGURE_ANSWER === "string") {\n' +
  '      return g().__NB_RING_CONFIGURE_ANSWER;\n' +
  '    }\n' +
  '    return JSON.stringify({ ok: true });\n' +
  '  },\n' +
  '  n2_ring_append: (line) => {\n' +
  '    g().__NB_RING = (g().__NB_RING || []).concat([String(line)]);\n' +
  '    return JSON.stringify({ ok: true, enabled: true });\n' +
  '  },\n' +
  '  n2_ring_disable: () => {\n' +
  '    g().__NB_RING_DISABLED = (g().__NB_RING_DISABLED || 0) + 1;\n' +
  '    return JSON.stringify({ ok: true });\n' +
  '  },\n' +
  '  n2_ring_status: () => {\n' +
  '    g().__NB_RING_STATUS_CALLS = (g().__NB_RING_STATUS_CALLS || 0) + 1;\n' +
  '    return JSON.stringify({ ok: true, enabled: true,\n' +
  '      line: "N2_RING_STATS|file=netbird-n2-evidence-ring.log|enabled=true|bytes=0|" +\n' +
  '        "max_bytes=4194304|segments=3|writes=1|failures=0|rotations=0|last_error=none|" +\n' +
  '        "mono_ms=1|diag=true|is_evidence=false" });\n' +
  '  },\n' +
  // The B4 retrieval surface: the CORE owns the file and all the seq
  // pagination. The defaults are the honest "no ring configured" answers; the
  // manifest and the ACK are bare calls the shell makes around the dump.
  '  n2_ring_manifest: () => {\n' +
  '    g().__NB_RING_MANIFEST_CALLS = (g().__NB_RING_MANIFEST_CALLS || 0) + 1;\n' +
  '    if (typeof g().__NB_RING_MANIFEST_ANSWER === "string") {\n' +
  '      return g().__NB_RING_MANIFEST_ANSWER;\n' +
  '    }\n' +
  '    return JSON.stringify({ ok: true, line: "" });\n' +
  '  },\n' +
  '  n2_ring_dump: (request) => {\n' +
  '    g().__NB_RING_DUMP_CALLS = (g().__NB_RING_DUMP_CALLS || 0) + 1;\n' +
  '    g().__NB_RING_DUMP_LAST = { request: request };\n' +
  '    if (typeof g().__NB_RING_DUMP_ANSWER === "string") {\n' +
  '      return g().__NB_RING_DUMP_ANSWER;\n' +
  '    }\n' +
  '    return JSON.stringify({ ok: false, result: "rejected", reason: "not-configured",\n' +
  '      from_seq: 0, to_seq: 0, count: 0, emitted: 0, lines: [], summary: "" });\n' +
  '  },\n' +
  '  n2_ring_ack: (request) => {\n' +
  '    g().__NB_RING_ACK_CALLS = (g().__NB_RING_ACK_CALLS || 0) + 1;\n' +
  '    g().__NB_RING_ACK_LAST = { request: request };\n' +
  '    if (g().__NB_CORE_ACK_THROW === true) { throw new Error("ack-seam-throw"); }\n' +
  '    if (typeof g().__NB_RING_ACK_ANSWER === "string") { return g().__NB_RING_ACK_ANSWER; }\n' +
  '    return JSON.stringify({ ok: false, result: "rejected", reason: "not-armed",\n' +
  '      session: "", up_to_seq: 0, acked_seq: 0, previous_acked_seq: 0, advanced: false });\n' +
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
  // Theme (2026-10-09): the REAL EntryAbility now imports these two SDK
  // modules for its app-scoped "follow the system" color mode, and they must
  // resolve as MODULES (the ability imports the first by NAME and the second
  // by DEFAULT), so neither can be left out of the map:
  //   * @ohos.app.ability.Configuration is an interface-only SDK module
  //     (26.0.0.821 <sdk>/default/openharmony/ets/api/
  //     @ohos.app.ability.Configuration.d.ts:30 `export interface
  //     Configuration`) — it carries no runtime value at all. The named export
  //     below is the same erased-shape placeholder the sibling shims use; it
  //     is a NAMED export because the ability writes `{ Configuration }` and
  //     node's strip-only TS loader keeps that import statement (it does no
  //     type analysis), so a missing name would fail the ESM link.
  //   * @ohos.app.ability.ConfigurationConstant is a REAL runtime namespace
  //     (same dir, @ohos.app.ability.ConfigurationConstant.d.ts:28-187,
  //     `export default ConfigurationConstant` with the ColorMode enum at
  //     :39-67). Its enum values are the SDK's verbatim: an empty stub would
  //     make ColorMode.COLOR_MODE_NOT_SET a TypeError that EntryAbility's own
  //     followSystemColorMode() try/catch swallows, silently turning the
  //     tested onCreate into its failure branch instead of the real call.
  '@ohos.app.ability.Configuration': 'data:text/javascript,' + encodeURIComponent(
    'export const Configuration = undefined;\n'),
  '@ohos.app.ability.ConfigurationConstant': 'data:text/javascript,' + encodeURIComponent(
    'export default { ColorMode: { COLOR_MODE_NOT_SET: -1, COLOR_MODE_DARK: 0,\n' +
    '  COLOR_MODE_LIGHT: 1 } };\n'),
  '@ohos.app.ability.Want': EMPTY,
  '@ohos.window': EMPTY,
  '@ohos.file.fs': FS,
  '@ohos.hilog': HILOG,
  // 2026-10-09 (0.0.17) onDestroy-entry stop marker: the REAL extension now
  // imports `process` for the pid it stamps into the marker (the same
  // `process.pid` NetBirdDiagnostics.ets writes), so the specifier must resolve
  // for the byte-identical extension fixture to link. The value is a FIXED
  // host pid: no driven path depends on it being real, and the marker round-trip
  // check reads the pid back out of the written line.
  '@ohos.process': 'data:text/javascript,' + encodeURIComponent(
    'export default { pid: 4242 };\n'),
  '@ohos.net.connection': CONNECTION,
  '@ohos.net.statistics': STATISTICS,
  '@ohos.net.vpnExtension': VPN_EXT,
  '@ohos.resourceManager': EMPTY,
  '@ohos.util': UTIL,
  'libnetbird_core.so': CORE,
  '../login/NetBirdAccountMode': 'data:text/javascript,' + encodeURIComponent(
    'export const ACCOUNT_MODE_ACCOUNT = "account";\n' +
    'export const ACCOUNT_MODE_FILE_NAME = "netbird-auth-mode";\n' +
    'export const ACCOUNT_MODE_SETUP_KEY = "setup-key";\n' +
    'export const parseAuthMode = (text) => !text || !text.trim() || text.trim() === "setup-key"' +
    ' ? "setup-key" : text.trim() === "account" ? "account" : "invalid";'),
  // 2026-10-10 后台保活（0.0.19, 方案 A）: the REAL EntryAbility now registers the
  // borrowed DATA_TRANSFER continuous task on its onCreate/onDestroy, so this
  // specifier must resolve for the byte-identical ability fixture to link. The
  // feature belongs to the UI process's own adapter (NetBirdKeepAliveService —
  // the only module that imports backgroundTaskManager / notificationManager /
  // wantAgent) and takes NO part in route-exclusion generation; it also needs
  // platform IO this host fixture cannot run, so the names are no-ops here. The
  // decision machine is driven end-to-end by run-keep-alive.mts, never mirrored.
  '../vpnextensionability/NetBirdKeepAliveService': 'data:text/javascript,' + encodeURIComponent(
    'export const KEEP_ALIVE_REVISION_SHUTDOWN = -1;\n' +
    'export const registerKeepAlive = () => undefined;\n' +
    'export const unregisterKeepAlive = () => undefined;\n' +
    'export const stopKeepAlive = async () => false;\n' +
    'export const startKeepAlive = async () => false;\n' +
    'export const refreshKeepAlive = async () => false;\n' +
    'export const keepAliveTaskPresent = () => false;\n' +
    'export const takeKeepAliveCancels = () => [];\n' +
    // 0.0.21（挂起长窗口后的恢复复查）: the REAL EntryAbility now reports its
    // onForeground through this ONE no-op seam too, so the name must exist for
    // the byte-identical ability fixture to link. It carries no route semantics
    // and this suite never drives the page's re-check (that wiring is
    // host-checked by run-keep-alive.mts), hence a no-op here.
    'export const notifyKeepAliveResume = () => undefined;'),
  // 0.0.21: the REAL EntryAbility names its resume reason token from the pure
  // keep-alive policy module (`KEEP_ALIVE_RESUME_FOREGROUND`), so this specifier
  // must resolve as well. The stub carries the token's shipped value verbatim —
  // the ability only passes it straight back into the seam above.
  '../vpnextensionability/NetBirdKeepAlivePolicy': 'data:text/javascript,' + encodeURIComponent(
    'export const KEEP_ALIVE_RESUME_FOREGROUND = "foreground";\n'),
  // 2026-10-09 live route apply: the REAL extension now imports the live-route
  // IPC file adapter (and the token vocabulary of its pure document module) by
  // value, so both specifiers must resolve for these fixtures to link. This
  // suite never drives the apply consumer — no request document is written here
  // and its 100 ms tick is either captured or too short-lived to fire — so the
  // answer is the truthful "no request", and a result write that IS reached
  // fails LOUDLY instead of pretending an answer was published. The real
  // adapter and its documents are driven end-to-end by run-route-apply-vpe.mts
  // (the store itself by run-route-request.mts), never mirrored here.
  '../routes/NetBirdRouteRequestStore': 'data:text/javascript,' + encodeURIComponent(
    'export const readRouteApplyRequest = async () => null;\n' +
    'export const writeRouteApplyResult = async () => {\n' +
    '  throw new Error("route-apply-result-not-staged-in-this-fixture");\n' +
    '};'),
  '../routes/NetBirdRouteRequest': 'data:text/javascript,' + encodeURIComponent(
    'export const ROUTE_APPLY_STATE_APPLYING = "applying";\n' +
    'export const ROUTE_APPLY_STATE_APPLIED = "applied";\n' +
    'export const ROUTE_APPLY_STATE_FAILED = "failed";\n' +
    'export const ROUTE_APPLY_MODE_NONE = "";\n' +
    'export const ROUTE_APPLY_MODE_NO_OS_CHANGE = "no-os-change";\n' +
    'export const ROUTE_APPLY_MODE_RECREATED = "recreated";\n' +
    'export const ROUTE_APPLY_MODE_STORED_NO_MAP = "stored-no-map";')
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
  './NetBirdInterfaceStatsProbe': 'NetBirdInterfaceStatsProbe.ts',
  './NetBirdPhysicalNetworkPolicy': 'NetBirdPhysicalNetworkPolicy.ts',
  // UI phase 1 (2026-10-05): the status-document builder the extension
  // imports by value; its two erased interfaces are re-declared by the shim.
  './NetBirdUiModel': 'NetBirdUiModel.shim.ts',
  // roam-1005-0014: the recreate lifecycle policy pure module the REAL
  // extension imports; zero imports and no erased types — plain extension
  // resolution like NetBirdInterfaceStatsProbe above.
  './NetBirdRecreatePolicy': 'NetBirdRecreatePolicy.ts',
  // 2026-10-09 (0.0.17): the REAL extension now names the pure marker core by
  // value (buildMarker / markerBasename / recordPath / MARKER_STATE_STOPPING)
  // for its synchronous onDestroy-entry write, so the fixture stages that file
  // byte-identically next to the extension (zero imports, no erased types, no
  // shim needed — the same discipline as NetBirdRecreatePolicy above).
  './NetBirdDiagnosticsCore': 'NetBirdDiagnosticsCore.ts',
  // N2 T5a (2026-09-28): the stimulus module is staged byte-identically; the
  // extension imports it by an extensionless specifier and names its erased
  // interfaces (N2StimulusDecision / N2StimulusReply), so it resolves to the
  // shim that re-declares them.
  './NetBirdN2Stimulus': 'NetBirdN2Stimulus.shim.ts',
  // N2 T3 §3 (2026-09-28): the positive-control request module is staged the
  // same way; its erased N2ControlDecision / N2ControlRequest interfaces are
  // re-declared by its shim.
  './NetBirdN2ControlRequest': 'NetBirdN2ControlRequest.shim.ts',
  // N2 P4 (2026-09-28): the A-column trigger pure module, staged and shimmed
  // like the stimulus module above.
  './NetBirdN2AEvent': 'NetBirdN2AEvent.shim.ts',
  // N2 P5 (2026-09-28): the sandwich pure module (its one erased interface
  // N2SandwichPost is re-declared by the shim).
  './NetBirdN2Sandwich': 'NetBirdN2Sandwich.shim.ts',
  // N2 first batch C/D columns (2026-09-29): the switching-sandwich pure module
  // (erased shapes N2CdPlanDecision / N2CdPlanRequest / N2BearerFacts) and the
  // ring pure module (N2RingDumpDecision / N2RingDumpRequest).
  './NetBirdN2CdPlan': 'NetBirdN2CdPlan.shim.ts',
  './NetBirdN2Ring': 'NetBirdN2Ring.shim.ts',
  // Diagnostics wiring (2026-10-08): the REAL extension and the REAL
  // EntryAbility now import the diagnostics writer (start/event/stop, plus
  // appendSystemEvent for the UI's watcher sink) and the system-event watcher
  // (start/stop). Neither module takes any part in route-exclusion generation —
  // their platform IO (hidebug / hiAppEvent) is exactly what this host fixture
  // cannot run — so each specifier resolves to a no-op bridge staged next to
  // the byte-identical fixtures. The two modules' own behaviour is host-checked
  // by run-diagnostics.mts and run-system-events.mts, never here.
  './NetBirdDiagnostics': 'NetBirdDiagnostics.shim.ts',
  './NetBirdSystemEvents': 'NetBirdSystemEvents.shim.ts',
  '../vpnextensionability/NetBirdDiagnostics': 'NetBirdDiagnostics.shim.ts',
  '../vpnextensionability/NetBirdSystemEvents': 'NetBirdSystemEvents.shim.ts',
  // 2026-10-07 per-NetID route selection (decision B): the REAL extension now
  // imports the file adapter by value, so the fixture stages
  // NetBirdRouteStore.ets byte-identically next to the extension and this entry
  // points the VPE's '../routes/NetBirdRouteStore' at that copy. The adapter's
  // own value import of the pure document module is extensionless, so it gets
  // the same treatment (resolved relative to the staged adapter) — staging the
  // pure module byte-identically is what keeps THIS hook a resolver, never a
  // re-implementation, of the route rules.
  '../routes/NetBirdRouteStore': 'NetBirdRouteStore.ts',
  './NetBirdRoutePreferences': 'NetBirdRoutePreferences.ts',
  // 2026-10-13 (0.0.29): the REAL EntryAbility now starts the PC status-bar tray
  // (2in1 only) and reads the shared explicit-quit flag. Neither takes any part in
  // route-exclusion generation, and the tray's platform module needs the PC-only
  // status-bar kit this host fixture cannot load, so both ability-side specifiers
  // are bridged by no-op shims carrying the same named exports.
  '../tray/NetBirdTrayService': 'tray-NetBirdTrayService.shim.ts',
  '../tray/NetBirdQuitFlow': 'tray-NetBirdQuitFlow.shim.ts'
};

export function resolve(specifier, context, next) {
  if (specifier === './NetBirdN2Control') {
    // N2 T3 §3 (2026-09-28): the positive-control sender the REAL extension now
    // calls from its armed-only `pollN2Control` (the control channel's missing
    // caller). FUNCTIONAL and recording like the other seams: every call is
    // counted and the arguments land in __NB_CONTROL_LAST, so a fixture can
    // drive the REAL poll path and assert exactly what reached the core. The
    // default is the armed happy path (returns void, like the real module);
    // __NB_CONTROL_MODE='throw' with __NB_CONTROL_ERROR makes it throw the
    // shell/core refusal a real send would raise, and an undefined `armed`
    // throws the real module's own `n2-not-armed`.
    return { url: 'data:text/javascript,' + encodeURIComponent(
      'export const N2ControlProtocol = undefined;\n' +
      'export const sendN2Control = (armed, target, protocol, id) => {\n' +
      '  const g = globalThis;\n' +
      '  g.__NB_CONTROL_CALLS = (g.__NB_CONTROL_CALLS || 0) + 1;\n' +
      '  g.__NB_CONTROL_LAST = { armed: armed, target: target, protocol: protocol, id: id };\n' +
      // N2 first batch C/D columns (2026-09-29): the ORDER of the two legs of
      // one plan's sandwich is what the pre/post separation is judged on, so the
      // ids are also recorded as an ordered list.
      '  g.__NB_CONTROL_IDS = (g.__NB_CONTROL_IDS || []).concat([String(id)]);\n' +
      // 2026-09-30: the gate's ordering trace — the new network's control must
      // leave AFTER the core\'s own old-connection cleanup.
      '  g.__NB_ORDER = (g.__NB_ORDER || []).concat(["control:" + String(id)]);\n' +
      '  if (armed === undefined) { throw new Error("n2-not-armed"); }\n' +
      '  if ((g.__NB_CONTROL_MODE || "sent") === "throw") {\n' +
      '    throw new Error(g.__NB_CONTROL_ERROR || "n2-control-send-failed");\n' +
      '  }\n' +
      '  return undefined;\n' +
      '};\n' +
      'export const sendAndWaitN2Control = async (armed, target, protocol, id) => {\n' +
      '  sendN2Control(armed, target, protocol, id);\n' +
      '  return undefined;\n' +
      '};'), shortCircuit: true };
  }
  if (specifier === './NetBirdN2Evidence') {
    return { url: 'data:text/javascript,' + encodeURIComponent(
      'export const N2EvidenceOptions = undefined; export const N2EvidenceDecision = undefined; ' +
      'export const parseN2Evidence = () => ({ options: undefined }); ' +
      'export const withN2EvidenceRoutes = (config) => config; ' +
      'export const withoutN2ExcludedRoutes = (config) => config;'), shortCircuit: true };
  }
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
