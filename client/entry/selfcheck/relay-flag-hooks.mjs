// Loader-side resolve hook for run-relay-flag.mts / run-relay-status-log.mts
// ONLY (host-side fixture). The real NetBirdConnector.ets imports @ohos.*
// modules and libnetbird_core.so which cannot resolve under plain node; this
// hook short-circuits ONLY those specifiers to stub modules so the real file
// can be imported as-is. No behavior under test is touched — the hook never
// rewrites the module.
//
// @ohos.net.connection is a FUNCTIONAL stub, not an empty module: the real
// module's v3.1 dns caliber (derivePlatformDnsEndpoints) calls the four
// synchronous network APIs, and the sibling module NetBirdEndpointExclusion
// (a real value import) must link. The four APIs read per-test tables on
// globalThis so a harness can drive the REAL branch order:
//   globalThis.__NB_NET_DEFAULT_ID / __NB_NET_ALL / __NB_NET_CAPS /
//   __NB_NET_DNSES / __NB_NET_THROW (see relay-exclusion-hooks.mjs).
// The './NetBirdEndpointExclusion' sibling is redirected to the dep file the
// harness stages next to the byte-identical fixture: it re-exports the REAL
// module plus shim consts for the interface names node's type stripping
// erases (the same plumbing run-route-validation.mts uses).

// libnetbird_core.so is a FUNCTIONAL stub for the connector_routes surface
// ONLY (the same call-time pattern as phone-log-hooks.mjs): the fake reads
// globalThis.__NB_CORE_ROUTES at CALL time, so ONE imported fixture can be
// driven through every decode branch —
//
//   globalThis.__NB_CORE_ROUTES = {
//     absent: false,         // true = export missing: the call throws the
//                            // TypeError a core without the symbol produces
//     throw: false,          // true = platform/native throw at call time
//     payload: '<json text>' // the raw connector_routes() document
//   }
//
// `undefined`/`null` global = the surface is ABSENT (throws), which is the
// real shape of an older core. No other core export is provided on purpose:
// none is exercised by run-relay-flag.mts / run-relay-status-log.mts, and
// @ohos.hilog stays the EMPTY stub so a stray log call from the wrapper
// cannot pass silently (it would throw and surface as a failing assertion).
//
// Core 5f adds the two route-ACK surfaces, with the SAME call-time pattern and
// the SAME absent/throw/payload answer table:
//   globalThis.__NB_CORE_ACK = { absent?, throw?, payload? }
//     -> connector_route_set_applied(routesJson, isRecreate[, kind])
//   globalThis.__NB_CORE_SELECTION_SET = { absent?, throw?, payload? }
//     -> connector_route_selection_set(selectionJson)
// Every call to either surface is RECORDED verbatim (the raw arguments array)
// under globalThis.__NB_CORE_CALLS[name], so a test can prove what actually
// crossed the NAPI boundary — in particular that the legacy 2-argument ACK
// passes EXACTLY two arguments and no third one.
const CORE = 'data:text/javascript,' + encodeURIComponent(
  'function g() { return globalThis.__NB_CORE_ROUTES; }\n' +
  'function recCall(name, args) {\n' +
  '  const all = globalThis.__NB_CORE_CALLS || (globalThis.__NB_CORE_CALLS = {});\n' +
  '  const list = all[name] || (all[name] = []);\n' +
  '  list.push(Array.prototype.slice.call(args));\n' +
  '}\n' +
  'function answer(name, table) {\n' +
  '  if (table === undefined || table === null || table.absent === true) {\n' +
  '    throw new TypeError(name + " is not a function");\n' +
  '  }\n' +
  '  if (table.throw === true) { throw new Error("native " + name + " failure"); }\n' +
  '  return table.payload;\n' +
  '}\n' +
  'const core = {};\n' +
  'core.connector_routes = () => {\n' +
  '  const m = g();\n' +
  '  if (m === undefined || m === null || m.absent === true) {\n' +
  '    throw new TypeError("connector_routes is not a function");\n' +
  '  }\n' +
  '  if (m.throw === true) { throw new Error("native connector_routes failure"); }\n' +
  '  return m.payload;\n' +
  '};\n' +
  'core.connector_route_set_applied = function () {\n' +
  '  recCall("connector_route_set_applied", arguments);\n' +
  '  return answer("connector_route_set_applied", globalThis.__NB_CORE_ACK);\n' +
  '};\n' +
  'core.connector_route_selection_set = function () {\n' +
  '  recCall("connector_route_selection_set", arguments);\n' +
  '  return answer("connector_route_selection_set", globalThis.__NB_CORE_SELECTION_SET);\n' +
  '};\n' +
  'export default core;');

const STUB = 'data:text/javascript,export%20default%20%7B%7D%3B';

const CONNECTION = 'data:text/javascript,' + encodeURIComponent(
  'function netG() { return globalThis; }\n' +
  'function netThrowing(name) {\n' +
  '  const t = netG().__NB_NET_THROW;\n' +
  '  return Array.isArray(t) && t.indexOf(name) >= 0;\n' +
  '}\n' +
  'function netBoom(name) {\n' +
  '  const e = new Error("net-api:" + name);\n' +
  '  e.code = 2100002;\n' +
  '  throw e;\n' +
  '}\n' +
  'function netCaps(id) {\n' +
  '  const t = netG().__NB_NET_CAPS || {};\n' +
  '  const c = t[id];\n' +
  '  return c === undefined || c === null ? { networkCap: [], bearerTypes: [] } : c;\n' +
  '}\n' +
  'function netDnses(id) {\n' +
  '  const t = netG().__NB_NET_DNSES || {};\n' +
  '  const d = t[id];\n' +
  '  return d === undefined || d === null ? [] : d;\n' +
  '}\n' +
  'export default {\n' +
  '  getAddressesByName: (host) => Promise.reject(new Error("dns-failed")),\n' +
  '  getDefaultNetSync: () => {\n' +
  '    if (netThrowing("getDefaultNetSync")) { netBoom("getDefaultNetSync"); }\n' +
  '    return { netId: netG().__NB_NET_DEFAULT_ID || 0 };\n' +
  '  },\n' +
  '  getNetCapabilitiesSync: (handle) => {\n' +
  '    if (netThrowing("getNetCapabilitiesSync")) { netBoom("getNetCapabilitiesSync"); }\n' +
  '    return netCaps(handle.netId);\n' +
  '  },\n' +
  '  getConnectionPropertiesSync: (handle) => {\n' +
  '    if (netThrowing("getConnectionPropertiesSync")) { netBoom("getConnectionPropertiesSync"); }\n' +
  '    return { interfaceName: "wlan0", dnses: netDnses(handle.netId) };\n' +
  '  },\n' +
  '  getAllNetsSync: () => {\n' +
  '    if (netThrowing("getAllNetsSync")) { netBoom("getAllNetsSync"); }\n' +
  '    const all = netG().__NB_NET_ALL || [];\n' +
  '    const out = [];\n' +
  '    for (let i = 0; i < all.length; i++) { out.push({ netId: all[i] }); }\n' +
  '    return out;\n' +
  '  }\n' +
  '};');

const STUBBED = new Map([
  ['@ohos.file.fs', STUB],
  ['@ohos.hilog', STUB],
  ['@ohos.net.connection', CONNECTION],
  ['@ohos.resourceManager', STUB],
  ['@ohos.util', STUB],
  ['libnetbird_core.so', CORE]
]);

export function resolve(specifier, context, next) {
  if (STUBBED.has(specifier)) {
    return { url: STUBBED.get(specifier), shortCircuit: true };
  }
  if (specifier === './NetBirdEndpointExclusion' && context.parentURL !== undefined) {
    // staged by the harness next to the byte-identical NetBirdConnector fixture
    const dir = new URL('.', context.parentURL);
    return { url: new URL('NetBirdEndpointExclusion.dep.ts', dir).href, shortCircuit: true };
  }
  return next(specifier, context);
}
