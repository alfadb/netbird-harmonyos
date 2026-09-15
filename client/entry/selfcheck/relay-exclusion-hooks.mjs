// Loader-side resolve hook for run-relay-exclusion.mts ONLY (host-side
// fixture). Same principle as relay-flag-hooks.mjs / route-check-hooks.mjs:
// the real modules import @ohos.* / libnetbird_core.so which cannot resolve
// under plain node; this hook short-circuits ONLY those specifiers.
//
// The stubs are FUNCTIONAL, not empty, because the fixture imports the REAL
// NetBirdVpnExtensionAbility.ets and drives its private
// deriveEndpointExclusions() end-to-end:
//   - @ohos.hilog      -> recording no-op formatter; every formatted line
//                       lands in globalThis.__NB_LOG (log-marker assertions
//                       run against the REAL hilog call sites)
//   - @ohos.net.connection -> getAddressesByName reads the per-test fake DNS
//                       table globalThis.__NB_DNS (deterministic, offline;
//                       a missing host rejects exactly like the platform)
//   - libnetbird_core.so -> connector_status() returns the per-test fake
//                       status JSON globalThis.__NB_FAKE_STATUS
//   - @ohos.app.ability.VpnExtensionAbility -> a bare base class so the real
//                       extension class can be `new`-ed on the host
// Everything else (@ohos.file.fs / .util / .resourceManager / .net.vpnExtension
// / @ohos.app.ability.Want) is dead code for the tested path -> empty stubs.
//
// Sibling modules are NOT stubbed: the test stages the five REAL files
// byte-identically in one temp dir. Type-only named imports (erased
// interfaces that would fail the ESM link) are bridged by the test-written
// <Name>.shim.ts files: `export * from './<Name>.ts'` (the real byte-identical
// fixture) plus undefined consts for the erased interface names. The hook
// redirects ONLY the extension './<Name>' specifiers to those shims; the
// fixtures themselves are never rewritten.

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

const CONNECTION = 'data:text/javascript,' + encodeURIComponent(
  'export default {\n' +
  '  getAddressesByName: (host) => {\n' +
  '    const t = globalThis.__NB_DNS || {};\n' +
  '    const v = t[host];\n' +
  '    return v !== undefined && v !== null\n' +
  '      ? Promise.resolve([{ address: v }])\n' +
  '      : Promise.reject(new Error("dns-failed"));\n' +
  '  }\n' +
  '};');

const CORE = 'data:text/javascript,' + encodeURIComponent(
  'export default {\n' +
  '  connector_status: () => {\n' +
  '    const s = globalThis.__NB_FAKE_STATUS;\n' +
  '    return typeof s === "string" ? s : JSON.stringify(s === undefined ? {} : s);\n' +
  '  }\n' +
  '};');

const VPN_ABILITY = 'data:text/javascript,' + encodeURIComponent(
  'export default class VpnExtensionAbility {}');

const STUBS = {
  '@ohos.app.ability.VpnExtensionAbility': VPN_ABILITY,
  '@ohos.app.ability.Want': EMPTY,
  '@ohos.file.fs': EMPTY,
  '@ohos.hilog': HILOG,
  '@ohos.net.connection': CONNECTION,
  '@ohos.net.vpnExtension': EMPTY,
  '@ohos.resourceManager': EMPTY,
  '@ohos.util': EMPTY,
  'libnetbird_core.so': CORE
};

// Sibling modules of the extension: the ESM link would fail on the erased
// type-only named imports, so the hook redirects them to the shim files the
// test writes next to the byte-identical fixtures.
const SHIMS = {
  './NetBirdConnector': 'NetBirdConnector.shim.ts',
  './NetBirdEndpointExclusion': 'NetBirdEndpointExclusion.shim.ts',
  './NetBirdRouteValidation': 'NetBirdRouteValidation.shim.ts',
  './NetBirdVpnConfig': 'NetBirdVpnConfig.shim.ts'
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