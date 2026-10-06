// SDK-module loader fakes for run-account-store.mts; production adapter bytes
// remain unchanged and encryption uses Node's real AES-256-GCM implementation.
const ASSET = 'data:text/javascript,' + encodeURIComponent(
  'const api = {' +
  ' Tag: { SECRET: 0x03000001, ALIAS: 0x03000002, ACCESSIBILITY: 0x02000003,' +
  ' AUTH_TYPE: 0x02000005, SYNC_TYPE: 0x02000010, RETURN_TYPE: 0x02000040,' +
  ' RETURN_LIMIT: 0x02000041, CONFLICT_RESOLUTION: 0x02000044 },' +
  ' ReturnType: { ALL: 0 }, Accessibility: { DEVICE_FIRST_UNLOCKED: 1 }, AuthType: { NONE: 0 },' +
  ' SyncType: { NEVER: 0 }, ConflictResolution: { THROW_ERROR: 1 },' +
  ' async query(q) { const s = globalThis.__NB_STORE_TEST;' +
  ' if (s.keyError) throw s.keyError; const alias = s.alias(q);' +
  ' if (!s.key || s.aliasName !== alias) throw { code: 24000002 };' +
  ' return [new Map([[0x03000001, new Uint8Array(s.key)]])]; },' +
  ' async add(a) { const s = globalThis.__NB_STORE_TEST; s.aliasName = s.alias(a);' +
  ' s.key = new Uint8Array(a.get(0x03000001)); }' +
  '}; export default api;');
const CRYPTO = 'data:text/javascript,' + encodeURIComponent(
  'import { createCipheriv, createDecipheriv, randomBytes } from "node:crypto";' +
  'function blob(data) { return { data: new Uint8Array(data) }; }' +
  'const api = {' +
  ' CryptoMode: { ENCRYPT_MODE: 0, DECRYPT_MODE: 1 },' +
  ' createRandom() { return { async generateRandom(n) { return blob(randomBytes(n)); } }; },' +
  ' createSymKeyGenerator(name) { if (name !== "AES256") throw Error("algorithm");' +
  ' return { async convertKey(input) { return { bytes: new Uint8Array(input.data) }; } }; },' +
  ' createCipher(name) { if (name !== "AES256|GCM|NoPadding") throw Error("cipher");' +
  ' let c; let encrypting = false; return {' +
  ' async init(mode, key, p) { const iv = p.iv.data; const aad = p.aad.data; encrypting = mode === 0;' +
  ' if (p.authTag.data.length !== 16) throw Error("invalid GCM auth tag length");' +
   ' if (encrypting) { c = createCipheriv("aes-256-gcm", key.bytes, iv); c.setAAD(aad); }' +
  ' else { c = createDecipheriv("aes-256-gcm", key.bytes, iv); c.setAAD(aad); c.setAuthTag(p.authTag.data); } },' +
  ' async doFinal(input) { const out = Buffer.concat([c.update(input.data), c.final()]);' +
  ' return blob(encrypting ? Buffer.concat([out, c.getAuthTag()]) : out); }' +
  ' }; } }; export default api;');
const FS = 'data:text/javascript,' + encodeURIComponent(
  'const handles = new Map(); let nextFd = 10;' +
  'const modes = { READ_ONLY: 0, READ_WRITE: 2, CREATE: 64, TRUNC: 512 };' +
  'const api = { OpenMode: modes,' +
  ' async open(path, mode) { const s = globalThis.__NB_STORE_TEST;' +
  ' if (s.readError && mode === 0) throw s.readError;' +
  ' const flags = mode === 0 ? "r" : "w+"; let h;' +
  ' try { h = await s.nodeFs.open(path, flags); } catch (e) {' +
  ' if (e.code === "ENOENT") throw { code: 13900002 }; throw e; }' +
  ' const fd = nextFd++; handles.set(fd, h); return { fd, h }; },' +
  ' async read(fd, arrayBuffer, opts) { const s = globalThis.__NB_STORE_TEST;' +
  ' if (s.readError) throw s.readError; const h = handles.get(fd);' +
  ' const target = Buffer.from(arrayBuffer); const r = await h.read(target, opts.offset, opts.length, opts.offset); return r.bytesRead; },' +
  ' async write(fd, bytes) { const s = globalThis.__NB_STORE_TEST;' +
  ' if (s.writeError) throw s.writeError; const h = handles.get(fd);' +
  ' const data = Buffer.from(bytes); const r = await h.write(data, 0, data.length, 0); return r.bytesWritten; },' +
  ' async fsync(fd) { await handles.get(fd).sync(); },' +
  ' async close(file) { handles.delete(file.fd); await file.h.close(); },' +
  ' async rename(from, to) { await globalThis.__NB_STORE_TEST.nodeFs.rename(from, to); },' +
  ' async unlink(path) { try { await globalThis.__NB_STORE_TEST.nodeFs.unlink(path); }' +
  ' catch (e) { if (e.code === "ENOENT") throw { code: 13900002 }; throw e; } }' +
  '}; export default api;');
const UTIL = 'data:text/javascript,' + encodeURIComponent(
  'class Base64Helper { encodeToStringSync(b) { return Buffer.from(b).toString("base64"); }' +
  ' decodeSync(s) { return new Uint8Array(Buffer.from(s, "base64")); } }' +
  'class TextEncoder { encode(s) { return new globalThis.TextEncoder().encode(s); } }' +
  'class Decoder { constructor(enc, opts) { this.d = new globalThis.TextDecoder(enc, opts); }' +
  ' decodeToString(b) { return this.d.decode(b); } }' +
  'export default { Base64Helper, TextEncoder, TextDecoder: { create(enc, opts) { return new Decoder(enc, opts); } } };');
const STUBS = new Map([
  ['@ohos.security.asset', ASSET],
  ['@ohos.security.cryptoFramework', CRYPTO],
  ['@ohos.file.fs', FS],
  ['@ohos.util', UTIL]
]);

export function resolve(specifier, context, next) {
  if (STUBS.has(specifier)) return { url: STUBS.get(specifier), shortCircuit: true };
  if (specifier === './NetBirdAccountKeyStore' && context.parentURL !== undefined) {
    return { url: new URL('NetBirdAccountKeyStore.ts', new URL('.', context.parentURL)).href, shortCircuit: true };
  }
  if (specifier === './NetBirdAccountStoreCore' && context.parentURL !== undefined) {
    return { url: new URL('NetBirdAccountStoreCore.ts', new URL('.', context.parentURL)).href, shortCircuit: true };
  }
  return next(specifier, context);
}
