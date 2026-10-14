// SDK-module loader fakes for run-keep-alive.mts ONLY (host-side fixture).
// Same principle as route-apply-hooks.mjs / log-export-hooks.mjs: the REAL
// production module is staged byte-for-byte into one OS-temp directory and
// driven under plain node, and only the @ohos.* platform seams are resolved here.
//
//   - @ohos.file.fs -> a FUNCTIONAL stub over the real node fs the test configures
//                      on globalThis.__NB_KA, so the store's whole atomic path
//                      (open/read/write/fsync/close/rename/unlink and the missing
//                      file business code 13900002) is exercised against real
//                      bytes, in a real directory, with real fsync. The stub also
//                      records every path it is asked for, which is how the test
//                      proves the store touches nothing but its own document.
//   - @ohos.util    -> the TextEncoder/TextDecoder pair backed by node's.
//   - 0.0.21: the platform seams NetBirdKeepAliveService.ets imports
//     (backgroundTaskManager / notificationManager / wantAgent / hilog plus the
//     two type-only modules Want / common) are stubbed the same way. The
//     backgroundTaskManager stub KEEPS the registered callbacks, so the test can
//     drive the real suspend/active handlers with a plain object and check what
//     the real module recorded — the callback bodies under test are the shipped
//     ones, never a copy.
//
// No module under test is ever rewritten.

import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const FS = 'data:text/javascript,' + encodeURIComponent(
  'const handles = new Map(); let nextFd = 10;\n' +
  'const modes = { READ_ONLY: 0, READ_WRITE: 2, CREATE: 64, TRUNC: 512 };\n' +
  'const api = { OpenMode: modes,\n' +
  ' async open(path, mode) { const s = globalThis.__NB_KA; s.paths.push(path);\n' +
  '  const flags = mode === 0 ? "r" : "w+";\n' +
  '  try { const h = await s.nodeFs.open(path, flags); const fd = nextFd++;' +
  '   handles.set(fd, h); return { fd: fd }; }\n' +
  '  catch (e) { if (e.code === "ENOENT") throw { code: 13900002, message: "no such file" };' +
  '   throw e; } },\n' +
  ' async read(fd, arrayBuffer, opts) { const h = handles.get(fd);\n' +
  '  const target = Buffer.from(arrayBuffer);\n' +
  '  const r = await h.read(target, opts.offset, opts.length, opts.offset);\n' +
  '  return r.bytesRead; },\n' +
  ' async write(fd, bytes) { const s = globalThis.__NB_KA;\n' +
  '  if (s.writeError) { throw s.writeError; }\n' +
  '  const h = handles.get(fd); const data = Buffer.from(bytes);\n' +
  '  const r = await h.write(data, 0, data.length, 0);\n' +
  '  return s.shortWrite ? r.bytesWritten - 1 : r.bytesWritten; },\n' +
  ' async fsync(fd) { await handles.get(fd).sync(); },\n' +
  ' async close(file) { const h = handles.get(file.fd); handles.delete(file.fd);\n' +
  '  if (h !== undefined) { await h.close(); } },\n' +
  ' async rename(from, to) { await globalThis.__NB_KA.nodeFs.rename(from, to); },\n' +
  ' async unlink(path) { try { await globalThis.__NB_KA.nodeFs.unlink(path); }\n' +
  '  catch (e) { if (e.code === "ENOENT") throw { code: 13900002 }; throw e; } }\n' +
  '};\nexport default api;');

const UTIL = 'data:text/javascript,' + encodeURIComponent(
  'class TextEncoder { encode(s) { return new globalThis.TextEncoder().encode(s); } }\n' +
  'class Decoder { constructor(enc, opts) { this.d = new globalThis.TextDecoder(enc, opts); }\n' +
  ' decodeToString(b) { return this.d.decode(b); } }\n' +
  'export default { TextEncoder, TextDecoder: { create(enc, opts) {\n' +
  ' return new Decoder(enc, opts); } } };');

// 0.0.21: the real adapter registers its three long-task callbacks here; the test
// emits through the SAME callbacks the app would get from the platform.
// 0.0.29: startBackgroundRunning also RECORDS the requested background modes, so
// the device-dependent type choice (taskKeeping on 2in1, dataTransfer elsewhere)
// is asserted against the shipped request path instead of a source grep.
const BTM = 'data:text/javascript,' + encodeURIComponent(
  'const s = () => globalThis.__NB_KA;\n' +
  'const api = {\n' +
  ' on(type, cb) { s().btm[type] = cb; },\n' +
  ' off(type, cb) { if (s().btm[type] === cb) { delete s().btm[type]; } },\n' +
  ' async getAllContinuousTasks() { return s().btmTasks; },\n' +
  ' async startBackgroundRunning(context, modes) { s().btmRequestedModes = modes;\n' +
  '  return s().btmStartResult; },\n' +
  ' async stopBackgroundRunning() { s().btmStopped = s().btmStopped + 1; },\n' +
  ' ContinuousTaskSuspendReason: { SYSTEM_SUSPEND_SYSTEM_LOAD_WARNING: 12 }\n' +
  '};\nexport default api;');

// 0.0.34: `published` records every request the SHIPPED adapter actually handed
// to the platform (so "no live-view publish in taskKeeping mode" is asserted on
// the real request path), and `publishError` makes the platform refuse one the
// way the PC's notification service did (202 / ANS 110002), which is how the
// refresh backoff and its per-level logging are driven.
const NOTIFICATION = 'data:text/javascript,' + encodeURIComponent(
  'const s = () => globalThis.__NB_KA;\n' +
  'const api = {\n' +
  ' SlotType: { LIVE_VIEW: 4 },\n' +
  ' ContentType: { NOTIFICATION_CONTENT_SYSTEM_LIVE_VIEW: 8 },\n' +
  ' async isNotificationEnabled() { return true; },\n' +
  ' async requestEnableNotification() { },\n' +
  ' async publish(request) { const st = s();\n' +
  '  if (st.publishError) { throw st.publishError; }\n' +
  '  st.published.push(request); }\n' +
  '};\nexport default api;');

const WANT_AGENT = 'data:text/javascript,' + encodeURIComponent(
  'const api = {\n' +
  ' WantAgentFlags: { UPDATE_PRESENT_FLAG: 2 },\n' +
  ' OperationType: { START_ABILITY: 1 },\n' +
  ' async getWantAgent() { return {}; }\n' +
  '};\nexport default api;');

// 0.0.29: the device type decides which borrowed background mode is requested, so
// the test drives it instead of hard-coding one. Read on EVERY access: the test
// switches the device between two calls.
const DEVICE_INFO = 'data:text/javascript,' + encodeURIComponent(
  'const api = {\n' +
  ' get deviceType() { return globalThis.__NB_KA.deviceType; }\n' +
  '};\nexport default api;');

// Type-only import targets (node keeps the statement, so they must resolve) and
// the logging seam: every line is kept for the test to inspect, nothing is printed.
const EMPTY = 'data:text/javascript,export default {};';
const HILOG = 'data:text/javascript,' + encodeURIComponent(
  'const s = () => globalThis.__NB_KA;\n' +
  'const push = (level, domain, tag, format, ...args) => {\n' +
  ' s().hilog.push({ level: level, tag: tag, format: format, args: args }); };\n' +
  'export default { info: (...a) => push("info", ...a), warn: (...a) => push("warn", ...a),\n' +
  ' error: (...a) => push("error", ...a), debug: (...a) => push("debug", ...a) };');

const STUBS = new Map([
  ['@ohos.file.fs', FS],
  ['@ohos.util', UTIL],
  ['@ohos.resourceschedule.backgroundTaskManager', BTM],
  ['@ohos.notificationManager', NOTIFICATION],
  ['@ohos.app.ability.wantAgent', WANT_AGENT],
  ['@ohos.deviceInfo', DEVICE_INFO],
  ['@ohos.app.ability.Want', EMPTY],
  ['@ohos.app.ability.common', EMPTY],
  ['@ohos.hilog', HILOG]
]);

// The staged production modules import their siblings the way ArkTS does — with
// no file extension ('./NetBirdKeepAlivePolicy', './NetBirdDiagnostics'), which
// node's ESM resolver would refuse. The staged directory is the only place this
// rule applies, and the module under test is still imported byte-for-byte.
export function resolve(specifier, context, next) {
  const url = STUBS.get(specifier);
  if (url !== undefined) {
    return { url: url, shortCircuit: true };
  }
  if (specifier.startsWith('.') && typeof context.parentURL === 'string' &&
    context.parentURL.startsWith('file:')) {
    try {
      const candidate = join(dirname(fileURLToPath(context.parentURL)), specifier + '.ts');
      if (existsSync(candidate)) {
        return { url: pathToFileURL(candidate).href, shortCircuit: true };
      }
    } catch (ignored) {
      // fall through to the default resolver
    }
  }
  return next(specifier, context);
}
