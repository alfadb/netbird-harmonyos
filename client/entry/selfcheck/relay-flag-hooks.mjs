// Loader-side resolve hook for run-relay-flag.mts ONLY (host-side fixture).
// The real NetBirdConnector.ets imports @ohos.* modules and libnetbird_core.so
// which cannot resolve under plain node; this hook short-circuits ONLY those
// specifiers to empty stub modules so the real file can be imported as-is.
// No behavior under test is touched — the hook never rewrites the module.

const STUB = 'data:text/javascript,export%20default%20%7B%7D%3B';

const STUBBED = new Set([
  '@ohos.file.fs',
  '@ohos.hilog',
  '@ohos.net.connection',
  '@ohos.resourceManager',
  '@ohos.util',
  'libnetbird_core.so'
]);

export function resolve(specifier, context, next) {
  if (STUBBED.has(specifier)) {
    return { url: STUB, shortCircuit: true };
  }
  return next(specifier, context);
}