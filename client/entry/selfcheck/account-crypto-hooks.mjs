// Loader stubs for run-account-crypto.mts only. Production code is imported
// byte-identically; only HarmonyOS SDK module resolution is replaced.
const CRYPTO = 'data:text/javascript,' + encodeURIComponent(
  'function settings() { return globalThis.__NB_ACCOUNT_CRYPTO; }\n' +
  'const api = {};\n' +
  'api.createRandom = () => ({ generateRandomSync(size) {' +
  ' const s = settings(); s.randomCalls++;' +
  ' if (s.failRandom) { throw new Error("raw random detail"); }' +
  ' if (size !== 32) { throw new Error("unexpected random size"); }' +
  ' const start = (s.randomCalls - 1) * 32;' +
  ' return { data: Uint8Array.from({ length: size }, (_, i) => (start + i) & 255) };' +
  '} });\n' +
  'api.createMd = (algorithm) => {' +
  ' if (algorithm !== "SHA256") { throw new Error("unexpected digest"); }' +
  ' let data = new Uint8Array(0);' +
  ' return {' +
  '  update(blob) { if (settings().failDigest) { throw new Error("raw digest detail"); }' +
  '   data = blob.data; return Promise.resolve(); },' +
  '  digest() { return Promise.resolve({ data: settings().sha256(data) }); }' +
  ' }; };\n' +
  'export default api;');
const UTIL = 'data:text/javascript,' + encodeURIComponent(
  'class Base64Helper { encodeToStringSync(bytes) {' +
  ' return Buffer.from(bytes).toString("base64"); } }' +
  'export default { Base64Helper };');
const STUBS = new Map([
  ['@ohos.security.cryptoFramework', CRYPTO],
  ['@ohos.util', UTIL]
]);

export function resolve(specifier, context, next) {
  if (STUBS.has(specifier)) {
    return { url: STUBS.get(specifier), shortCircuit: true };
  }
  if (specifier === './NetBirdAccountAuthCore' && context.parentURL !== undefined) {
    return { url: new URL('NetBirdAccountAuthCore.ts', new URL('.', context.parentURL)).href,
      shortCircuit: true };
  }
  return next(specifier, context);
}
