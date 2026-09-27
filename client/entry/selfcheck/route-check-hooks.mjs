// Loader-side resolve hook for run-route-validation.mts ONLY (host-side
// fixture). Same principle as relay-flag-hooks.mjs: the real modules import
// @ohos.* / libnetbird_core.so which cannot resolve under plain node; this
// hook short-circuits ONLY those specifiers to empty stub modules.
//
// NetBirdVpnConfig.ets (the module under test here) additionally imports
// TYPE-ONLY names from real siblings ('CoreNetworkConfig', 'CoreRouteEntry',
// 'ExcludedRouteEntry'). After node type-stripping erases the interfaces,
// the ESM linker would reject those named imports, so:
//   './NetBirdConnector'         -> shim data: module exporting the two type
//                                   names (the import is type-positions only
//                                   in the module under test; no runtime
//                                   value is ever needed from it)
//   './NetBirdEndpointExclusion' -> NetBirdEndpointExclusion.dep.ts, a tiny
//                                   harness file that re-exports the REAL
//                                   byte-identical fixture (mergeExcluded-
//                                   RouteEntries is called at runtime) plus
//                                   a shim const for the erased interface
//                                   name. The fixture file itself stays
//                                   byte-identical; no behavior under test
//                                   is ever rewritten.
const STUB = 'data:text/javascript,export%20default%20%7B%7D%3B';

// @ohos.net.connection is a FUNCTIONAL stub (not the empty one): the real
// NetBirdConnector's v3.1 dns caliber calls four synchronous network APIs,
// and they must answer with the per-test tables on globalThis instead of
// crashing the fixture (see relay-exclusion-hooks.mjs for the table names).
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
  ['@ohos.net.vpnExtension', STUB],
  ['@ohos.resourceManager', STUB],
  ['@ohos.util', STUB],
  ['libnetbird_core.so', STUB]
]);

export function resolve(specifier, context, next) {
  if (STUBBED.has(specifier)) {
    return { url: STUBBED.get(specifier), shortCircuit: true };
  }
  if (context.parentURL === undefined) {
    return next(specifier, context);
  }
  const dir = new URL('.', context.parentURL);
  if (specifier === './NetBirdVpnConfig') {
    return { url: new URL('NetBirdVpnConfig.dep.ts', dir).href, shortCircuit: true };
  }
  if (specifier === './NetBirdConnector') {
    return {
      url: 'data:text/javascript,' + encodeURIComponent(
        'export const CoreNetworkConfig = undefined;\n' +
        'export const CoreRouteEntry = undefined;\n'),
      shortCircuit: true
    };
  }
  if (specifier === './NetBirdEndpointExclusion') {
    return {
      url: new URL('NetBirdEndpointExclusion.dep.ts', dir).href,
      shortCircuit: true
    };
  }
  return next(specifier, context);
}
