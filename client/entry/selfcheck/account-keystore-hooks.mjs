// Node loader stubs for the byte-identical AccountKeyStore fixture only.
// Fakes are read at call time so the harness can drive missing, duplicate, and
// platform-error cases without touching device assets or random sources.
const ASSET = 'data:text/javascript,export%20default%20globalThis.__NB_KEY_ASSET;';
const CRYPTO = 'data:text/javascript,export%20default%20globalThis.__NB_KEY_CRYPTO;';

export function resolve(specifier, context, next) {
  if (specifier === '@ohos.security.asset') {
    return { url: ASSET, shortCircuit: true };
  }
  if (specifier === '@ohos.security.cryptoFramework') {
    return { url: CRYPTO, shortCircuit: true };
  }
  return next(specifier, context);
}
