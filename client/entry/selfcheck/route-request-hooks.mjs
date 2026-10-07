// SDK-module loader fakes for run-route-request.mts ONLY (host-side fixture).
// Same principle as route-preferences-hooks.mjs: the real production modules are
// staged byte-for-byte, and only the @ohos.* platform modules and the
// extension-less relative specifiers the staged files import are resolved here.
//
// @ohos.file.fs is a FUNCTIONAL stub: it forwards every operation to the real
// node fs the test configures on globalThis.__NB_ROUTE_REQUEST_TEST, so the
// store's open/read/write/fsync/close/rename/unlink path and the atomic
// temp-file behaviour are exercised against real bytes. No device, no compiler.
// @ohos.util is the TextEncoder/TextDecoder pair backed by node's.
const FS = 'data:text/javascript,' + encodeURIComponent(
  'const handles = new Map(); let nextFd = 10;' +
  'const modes = { READ_ONLY: 0, READ_WRITE: 2, CREATE: 64, TRUNC: 512 };' +
  'const api = { OpenMode: modes,' +
  ' async open(path, mode) { const s = globalThis.__NB_ROUTE_REQUEST_TEST;' +
  ' if (s.readError && mode === 0) throw s.readError;' +
  ' const flags = mode === 0 ? "r" : "w+"; let h;' +
  ' try { h = await s.nodeFs.open(path, flags); } catch (e) {' +
  ' if (e.code === "ENOENT") throw { code: 13900002 }; throw e; }' +
  ' const fd = nextFd++; handles.set(fd, h); return { fd, h }; },' +
  ' async read(fd, arrayBuffer, opts) { const s = globalThis.__NB_ROUTE_REQUEST_TEST;' +
  ' if (s.readError) throw s.readError; const h = handles.get(fd);' +
  ' const target = Buffer.from(arrayBuffer);' +
  ' const r = await h.read(target, opts.offset, opts.length, opts.offset); return r.bytesRead; },' +
  ' async write(fd, bytes) { const s = globalThis.__NB_ROUTE_REQUEST_TEST;' +
  ' if (s.writeError) throw s.writeError; const h = handles.get(fd);' +
  ' const data = Buffer.from(bytes); const r = await h.write(data, 0, data.length, 0); return r.bytesWritten; },' +
  ' async fsync(fd) { await handles.get(fd).sync(); },' +
  ' async close(file) { handles.delete(file.fd); await file.h.close(); },' +
  ' async rename(from, to) { await globalThis.__NB_ROUTE_REQUEST_TEST.nodeFs.rename(from, to); },' +
  ' async unlink(path) { try { await globalThis.__NB_ROUTE_REQUEST_TEST.nodeFs.unlink(path); }' +
  ' catch (e) { if (e.code === "ENOENT") throw { code: 13900002 }; throw e; } }' +
  '}; export default api;');
const UTIL = 'data:text/javascript,' + encodeURIComponent(
  'class TextEncoder { encode(s) { return new globalThis.TextEncoder().encode(s); } }' +
  'class Decoder { constructor(enc, opts) { this.d = new globalThis.TextDecoder(enc, opts); } ' +
  ' decodeToString(b) { return this.d.decode(b); } }' +
  'export default { TextEncoder, TextDecoder: { create(enc, opts) { return new Decoder(enc, opts); } } };');
const STUBS = new Map([
  ['@ohos.file.fs', FS],
  ['@ohos.util', UTIL]
]);
// The staged production modules import each other WITHOUT an extension.
const RELATIVE = new Set([
  './NetBirdRoutePreferences',
  './NetBirdRouteRequest'
]);

export function resolve(specifier, context, next) {
  if (STUBS.has(specifier)) return { url: STUBS.get(specifier), shortCircuit: true };
  if (RELATIVE.has(specifier) && context.parentURL !== undefined) {
    return {
      url: new URL(specifier.slice(2) + '.ts', new URL('.', context.parentURL)).href,
      shortCircuit: true
    };
  }
  return next(specifier, context);
}
