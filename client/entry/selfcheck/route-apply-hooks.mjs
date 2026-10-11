// SDK-module loader fakes for run-route-apply.mts ONLY (host-side fixture).
// Same principle as route-preferences-hooks.mjs / route-request-hooks.mjs: the
// REAL production modules are staged byte-for-byte into one OS-temp directory,
// and only the @ohos.* platform modules and the extension-less relative
// specifiers those staged files import are resolved here.
//
// @ohos.file.fs is a FUNCTIONAL stub: it forwards every operation to the real
// node fs the test configures on globalThis.__NB_ROUTE_APPLY_TEST, so BOTH stores'
// stat/open/read/write/fsync/close/rename/unlink path and the atomic temp-file
// behaviour are exercised against real bytes. No device, no compiler.
// @ohos.util is the TextEncoder/TextDecoder pair backed by node's.
const FS = 'data:text/javascript,' + encodeURIComponent(
  'const handles = new Map(); let nextFd = 10;' +
  'const modes = { READ_ONLY: 0, READ_WRITE: 2, CREATE: 64, TRUNC: 512 };' +
  'const api = { OpenMode: modes,' +
  // The live-route request adapter's cheap "unchanged document" probe
  // (NetBirdRouteRequestStore): a REAL stat over the same node fs.
  ' statSync(path) { const s = globalThis.__NB_ROUTE_APPLY_TEST;' +
  ' try { const st = s.nodeFs.statSync(path, { bigint: true });' +
  // node's BigIntStats keeps mtime/ctime as Date objects, so the whole seconds
  // are derived from the exact nanosecond fields (bigint division truncates).
  ' return { ino: st.ino, size: Number(st.size),' +
  ' mtime: Number(st.mtimeNs / 1000000000n), ctime: Number(st.ctimeNs / 1000000000n),' +
  ' mtimeNs: st.mtimeNs, ctimeNs: st.ctimeNs }; }' +
  ' catch (e) { if (e.code === "ENOENT") throw { code: 13900002 }; throw e; } },' +
  ' async open(path, mode) { const s = globalThis.__NB_ROUTE_APPLY_TEST;' +
  ' if (s.readError && mode === 0) throw s.readError;' +
  ' const flags = mode === 0 ? "r" : "w+"; let h;' +
  ' try { h = await s.nodeFs.open(path, flags); } catch (e) {' +
  ' if (e.code === "ENOENT") throw { code: 13900002 }; throw e; }' +
  ' const fd = nextFd++; handles.set(fd, h); return { fd, h }; },' +
  ' async read(fd, arrayBuffer, opts) { const s = globalThis.__NB_ROUTE_APPLY_TEST;' +
  ' if (s.readError) throw s.readError; const h = handles.get(fd);' +
  ' const target = Buffer.from(arrayBuffer);' +
  ' const r = await h.read(target, opts.offset, opts.length, opts.offset); return r.bytesRead; },' +
  ' async write(fd, bytes) { const s = globalThis.__NB_ROUTE_APPLY_TEST;' +
  ' if (s.writeError) throw s.writeError; const h = handles.get(fd);' +
  ' const data = Buffer.from(bytes); const r = await h.write(data, 0, data.length, 0); return r.bytesWritten; },' +
  ' async fsync(fd) { await handles.get(fd).sync(); },' +
  ' async close(file) { handles.delete(file.fd); await file.h.close(); },' +
  ' async rename(from, to) { await globalThis.__NB_ROUTE_APPLY_TEST.nodeFs.rename(from, to); },' +
  ' async unlink(path) { try { await globalThis.__NB_ROUTE_APPLY_TEST.nodeFs.unlink(path); }' +
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
  './NetBirdRouteRequest',
  './NetBirdRouteStore'
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
