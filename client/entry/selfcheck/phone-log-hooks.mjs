// Loader-side resolve hook for run-phone-log.mts ONLY (host-side fixture).
//
// The real NetBirdConnector.ets imports @ohos.* modules and
// libnetbird_core.so, which plain node cannot resolve. This hook
// short-circuits ONLY those specifiers so the real file can be imported as-is
// (no behavior under test is rewritten) and redirects the real value import of
// './NetBirdEndpointExclusion' to the byte-identical sibling the harness stages
// next to the fixture (the same plumbing relay-flag-hooks.mjs uses — the
// connector value-imports the derivation logic, and node's type stripping
// erases the type-only names, so the staged dep file re-exports them as shims).
//
// libnetbird_core.so is a FUNCTIONAL stub for the phone_log surface only: each
// export reads globalThis.__NB_PHONE_LOG at CALL time, so ONE imported fixture
// can be driven through every reply shape (ok / error code / malformed reply /
// platform throw / surface absent):
//
//   globalThis.__NB_PHONE_LOG = {
//     absent: false,                 // true = behave like a core built with no
//                                    // phone_log_* export: every call throws a
//                                    // TypeError, the shape the wrapper maps to
//                                    // `native-unavailable`
//     configure: (path) => '{"ok":true,...}',
//     append: (line) => '{"ok":true}',
//     status: () => '{"component":"phone_log",...}',
//     shutdown: () => '{"ok":true,"stopped":true,...}',
//     resource: () => '{"ok":true,"connector":{...},...}',
//     alloc: () => '{"bytes":n,"peak":n,"allocs":n,"frees":n}'
//   }
//
// `resource` is the 006 `connector_resource_status()` document and `alloc` is
// the Rust-heap `alloc_stats()` document (same fake, so the same `absent`
// latch drives the missing-export branch of BOTH surfaces).
//
// The connector module never imports @ohos.net.connection at call time in these
// tests, so the empty stub is enough for it to link.

const STUB = 'data:text/javascript,export%20default%20%7B%7D%3B';

const CORE = 'data:text/javascript,' + encodeURIComponent(
  'function g() { return globalThis.__NB_PHONE_LOG || {}; }\n' +
  'function absent() { return g().absent === true; }\n' +
  'function boom(name) { throw new TypeError(name + " is not a function"); }\n' +
  'const core = {};\n' +
  'core.phone_log_configure = (p) => { if (absent()) { boom("phone_log_configure"); }' +
  ' return g().configure(p); };\n' +
  'core.phone_log_append = (l) => { if (absent()) { boom("phone_log_append"); }' +
  ' return g().append(l); };\n' +
  'core.phone_log_status = () => { if (absent()) { boom("phone_log_status"); }' +
  ' return g().status(); };\n' +
  'core.phone_log_shutdown = () => { if (absent()) { boom("phone_log_shutdown"); }' +
  ' return g().shutdown(); };\n' +
  'core.connector_resource_status = () => { if (absent()) { boom("connector_resource_status"); }' +
  ' return g().resource(); };\n' +
  'core.alloc_stats = () => { if (absent()) { boom("alloc_stats"); }' +
  ' return g().alloc(); };\n' +
  'export default core;');

const STUBBED = new Map([
  ['@ohos.file.fs', STUB],
  ['@ohos.hilog', STUB],
  ['@ohos.net.connection', STUB],
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
