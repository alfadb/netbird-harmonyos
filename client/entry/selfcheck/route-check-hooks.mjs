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

const STUBBED = new Set([
  '@ohos.file.fs',
  '@ohos.hilog',
  '@ohos.net.connection',
  '@ohos.net.vpnExtension',
  '@ohos.resourceManager',
  '@ohos.util',
  'libnetbird_core.so'
]);

export function resolve(specifier, context, next) {
  if (STUBBED.has(specifier)) {
    return { url: STUB, shortCircuit: true };
  }
  if (context.parentURL === undefined) {
    return next(specifier, context);
  }
  const dir = new URL('.', context.parentURL);
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
