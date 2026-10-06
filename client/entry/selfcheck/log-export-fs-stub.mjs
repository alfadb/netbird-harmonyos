// PATH-AWARE in-memory @ohos.file.fs for run-log-export.mts ONLY (host-side
// fixture). Same principle as the other selfcheck hooks in this directory: the
// module under test is the REAL shipped file copied byte-for-byte to a .ts
// fixture, and only the platform seams are replaced. This stub is FUNCTIONAL,
// not empty: the async fs promises the export module uses (stat/open/read/
// write/close) are implemented over a Map, with byte-accurate positional
// writes, and every seam used by the suite is injectable.
//
// Faithfulness notes:
//   - fd bookkeeping is real enough to detect a leak: openTrace/closeTrace/
//     openNow/maxOpen let the suite assert that sources are opened ONE AT A
//     TIME and that every handle is closed (the module's finally blocks).
//   - write() returns the REAL byte count and can be capped per call
//     (state.writeMaxBytes) so the short-write retry loops are driven for
//     real rather than assumed.
//   - read() reads from the CURRENT position and never beyond EOF, like the
//     SDK's read() with no options (SDK 26 @ohos.file.fs.d.ts:2664/2744).
//   - errors are BusinessError-shaped ({code}) exactly as far as the module
//     under test reads them (one numeric field).
//   - state.onOpen lets the suite mutate the fs at the moment the module opens
//     a source, which is how a mid-export rotation is driven honestly.

/** abs path -> Buffer of the file's bytes. */
export const files = new Map();
/** abs path -> mtime (seconds). */
export const mtimes = new Map();
/** abs path -> error code thrown by stat() (one-shot). */
export const statErrors = new Map();
/** abs path -> error code thrown by open() (one-shot). */
export const openErrors = new Map();
/** abs path -> error code thrown by the NEXT read() of that file (one-shot). */
export const readErrors = new Map();
/** abs path -> error code thrown by the NEXT write() to that file (one-shot). */
export const writeErrors = new Map();
/** abs path -> error code thrown by close() of that file (one-shot). */
export const closeErrors = new Map();
/** open order (abs paths). */
export const openTrace = [];
/** close order (abs paths). */
export const closeTrace = [];
/** fds open right now. */
export const openNow = new Set();
/** high-water mark of simultaneous open fds. */
export let maxOpen = 0;
/** suite-controlled seams. */
export const state = {
  writeMaxBytes: undefined, // cap on ONE write() call's byte count
  onOpen: undefined          // (path) => void, run inside open()
};

const fds = new Map(); // fd -> { path, pos, base, parts }
let nextFd = 1;

export const OpenMode = {
  READ_ONLY: 0o0,
  WRITE_ONLY: 0o1,
  READ_WRITE: 0o2,
  CREATE: 0o100,
  TRUNC: 0o1000,
  APPEND: 0o2000
};

function businessError(code) {
  const error = new Error(`stub-fs-code-${code}`);
  error.code = code;
  return error;
}

export function reset() {
  files.clear();
  mtimes.clear();
  statErrors.clear();
  openErrors.clear();
  readErrors.clear();
  writeErrors.clear();
  closeErrors.clear();
  openTrace.length = 0;
  closeTrace.length = 0;
  openNow.clear();
  maxOpen = 0;
  fds.clear();
  nextFd = 1;
  state.writeMaxBytes = undefined;
  state.onOpen = undefined;
}

export function seedFile(path, text, mtimeSec) {
  files.set(path, Buffer.from(text, 'utf8'));
  mtimes.set(path, mtimeSec === undefined ? 1700000000 : mtimeSec);
}

export function seedBytes(path, buffer, mtimeSec) {
  files.set(path, Buffer.from(buffer));
  mtimes.set(path, mtimeSec === undefined ? 1700000000 : mtimeSec);
}

export function textOf(path) {
  const buffer = files.get(path);
  return buffer === undefined ? undefined : buffer.toString('utf8');
}

export function sizeOf(path) {
  const buffer = files.get(path);
  return buffer === undefined ? undefined : buffer.length;
}

export async function stat(path) {
  const injected = statErrors.get(path);
  if (injected !== undefined) {
    statErrors.delete(path);
    throw businessError(injected);
  }
  if (!files.has(path)) {
    throw businessError(13900002);
  }
  return { size: files.get(path).length, mtime: mtimes.get(path) };
}

export async function open(path, mode) {
  if (state.onOpen !== undefined) {
    state.onOpen(path);
  }
  const injected = openErrors.get(path);
  if (injected !== undefined) {
    openErrors.delete(path);
    throw businessError(injected);
  }
  const writable = (mode & (OpenMode.WRITE_ONLY | OpenMode.READ_WRITE)) !== 0;
  if (!files.has(path)) {
    if (writable || (mode & OpenMode.CREATE) !== 0) {
      files.set(path, Buffer.alloc(0));
      mtimes.set(path, 1700000000);
    } else {
      throw businessError(13900002);
    }
  }
  if (writable && (mode & OpenMode.TRUNC) !== 0) {
    files.set(path, Buffer.alloc(0));
  }
  const fd = nextFd;
  nextFd += 1;
  const entry = {
    path: path,
    pos: writable ? files.get(path).length : 0,
    base: writable ? files.get(path) : undefined,
    parts: writable ? [] : undefined
  };
  fds.set(fd, entry);
  openTrace.push(path);
  openNow.add(fd);
  if (openNow.size > maxOpen) {
    maxOpen = openNow.size;
  }
  return { fd: fd, path: path, name: path.split('/').pop() };
}

export async function read(fd, buffer, options) {
  const entry = fds.get(fd);
  if (entry === undefined) {
    throw businessError(13900008);
  }
  const injected = readErrors.get(entry.path);
  if (injected !== undefined) {
    readErrors.delete(entry.path);
    throw businessError(injected);
  }
  const data = files.get(entry.path) === undefined ? Buffer.alloc(0) : files.get(entry.path);
  const want = options !== undefined && options.length !== undefined ?
    options.length : buffer.byteLength;
  const available = data.length - entry.pos;
  const count = Math.min(want, available);
  if (count <= 0) {
    return 0;
  }
  data.copy(Buffer.from(buffer), 0, entry.pos, entry.pos + count);
  entry.pos += count;
  return count;
}

export async function write(fd, buffer, options) {
  const entry = fds.get(fd);
  if (entry === undefined) {
    throw businessError(13900008);
  }
  const injected = writeErrors.get(entry.path);
  if (injected !== undefined) {
    writeErrors.delete(entry.path);
    throw businessError(injected);
  }
  if (entry.parts === undefined) {
    entry.parts = [];
    entry.base = files.get(entry.path) === undefined ? Buffer.alloc(0) : files.get(entry.path);
    entry.pos = entry.base.length;
  }
  let payload = typeof buffer === 'string' ? Buffer.from(buffer, 'utf8') : Buffer.from(buffer);
  if (options !== undefined && options.length !== undefined) {
    payload = payload.subarray(0, options.length);
  }
  if (state.writeMaxBytes !== undefined && payload.length > state.writeMaxBytes) {
    payload = payload.subarray(0, state.writeMaxBytes);
  }
  entry.parts.push(Buffer.from(payload));
  entry.pos += payload.length;
  return payload.length;
}

export async function close(file) {
  const fd = typeof file === 'number' ? file : file.fd;
  const entry = fds.get(fd);
  if (entry === undefined) {
    throw businessError(13900008);
  }
  const injected = closeErrors.get(entry.path);
  if (injected !== undefined) {
    closeErrors.delete(entry.path);
    throw businessError(injected);
  }
  if (entry.parts !== undefined) {
    const head = entry.base === undefined ? Buffer.alloc(0) : entry.base;
    files.set(entry.path, Buffer.concat([head].concat(entry.parts)));
  }
  fds.delete(fd);
  openNow.delete(fd);
  closeTrace.push(entry.path);
}

export function assertNoLeak() {
  return openNow.size === 0 && openTrace.length === closeTrace.length;
}

// The module shape the real '@ohos.file.fs' presents to `import fs from ...`.
export default { OpenMode, stat, open, read, write, close };
