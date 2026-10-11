/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// File adapter for the 后台保活（实验）settings switch (0.0.19, 方案 A). Same
// minimal IO pattern as NetBirdRouteStore (private file,
// temp file, fsync, rename) with NO key material and NO crypto: the document is
// one public boolean ("may this app borrow a DATA_TRANSFER continuous task"),
// and the path whitelist below can only ever name that one file plus its temp
// sibling — this module can never touch the device config, the setup key or the
// route documents.
//
// SCOPE RULES
// - missing file, unreadable file, oversized file, corrupt JSON, an unknown
//   version, or a non-boolean `enabled` -> KEEP_ALIVE_DEFAULT_ENABLED (true).
//   The load NEVER throws: the switch's value may not be able to stop the app
//   from starting, and a read failure must not surface as an error dialog on a
//   settings page.
// - the write is atomic (temp + fsync + rename) and reports success as a
//   boolean instead of throwing: the caller (the settings toggle) has no error
//   surface for it, and a failed write must never break the switch's own
//   runtime effect. A failed write leaves the previous document untouched.

import fs from '@ohos.file.fs';
import util from '@ohos.util';

export const KEEP_ALIVE_FILE: string = 'netbird-keepalive.json';
export const KEEP_ALIVE_DOCUMENT_VERSION: number = 1;
// The document is `{"v":1,"enabled":true}` — 24 bytes. The bound is generous and
// exists only to refuse an absurd file instead of allocating for it.
export const MAX_KEEP_ALIVE_DOCUMENT_BYTES: number = 4096;
export const KEEP_ALIVE_DEFAULT_ENABLED: boolean = true;
const TEMP_SUFFIX: string = '.tmp';

interface StoredKeepAlive {
  v: number;
  enabled: boolean;
}

// The path whitelist: only this module's own document and its temp sibling.
function pathFor(filesDir: string, basename: string): string {
  if (filesDir.length === 0 || filesDir.endsWith('/') ||
    basename !== KEEP_ALIVE_FILE && basename !== KEEP_ALIVE_FILE + TEMP_SUFFIX) {
    throw new Error('keep-alive-store-unavailable');
  }
  return `${filesDir}/${basename}`;
}

function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  const copy: Uint8Array = new Uint8Array(bytes.length);
  copy.set(bytes);
  return copy.buffer;
}

function decodeUtf8(bytes: Uint8Array): string {
  return util.TextDecoder.create('utf-8', { fatal: true }).decodeToString(bytes);
}

// Bounded read; null for "nothing to read" (absent, unreadable, oversized or
// not UTF-8) — every one of those means the default value, never an error.
async function readTextBounded(path: string): Promise<string | null> {
  let file: fs.File | null = null;
  const buffer: ArrayBuffer = new ArrayBuffer(MAX_KEEP_ALIVE_DOCUMENT_BYTES + 1);
  let total: number = 0;
  try {
    file = await fs.open(path, fs.OpenMode.READ_ONLY);
    while (total < buffer.byteLength) {
      const count: number = await fs.read(file.fd, buffer,
        { offset: total, length: buffer.byteLength - total });
      if (count <= 0) {
        break;
      }
      total += count;
    }
  } catch (error) {
    return null;
  } finally {
    if (file !== null) {
      try {
        await fs.close(file);
      } catch (ignored) {
        // The primary result is preserved; a close failure carries no data.
      }
    }
  }
  if (total > MAX_KEEP_ALIVE_DOCUMENT_BYTES) {
    return null;
  }
  try {
    return decodeUtf8(new Uint8Array(buffer, 0, total));
  } catch (error) {
    return null;
  }
}

async function writeAtomicText(filesDir: string, text: string): Promise<void> {
  const path: string = pathFor(filesDir, KEEP_ALIVE_FILE);
  const tempPath: string = pathFor(filesDir, KEEP_ALIVE_FILE + TEMP_SUFFIX);
  const bytes: Uint8Array = new util.TextEncoder().encode(text);
  let file: fs.File | null = null;
  try {
    file = await fs.open(tempPath, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
    const written: number = await fs.write(file.fd, toArrayBuffer(bytes));
    if (written !== bytes.length) {
      throw new Error('keep-alive-store-short-write');
    }
    await fs.fsync(file.fd);
    await fs.close(file);
    file = null;
    await fs.rename(tempPath, path);
  } finally {
    if (file !== null) {
      try {
        await fs.close(file);
      } catch (ignored) {
        // Preserve the write failure; never expose SDK paths or messages.
      }
    }
    try {
      await fs.unlink(tempPath);
    } catch (ignored) {
      // A successful rename already consumed the temporary path.
    }
  }
}

// The switch's stored value. Never throws, never returns an error: anything
// that is not the exact, known, well-formed document is the default.
export async function loadKeepAliveEnabled(filesDir: string): Promise<boolean> {
  let text: string | null = null;
  try {
    text = await readTextBounded(pathFor(filesDir, KEEP_ALIVE_FILE));
  } catch (error) {
    return KEEP_ALIVE_DEFAULT_ENABLED;
  }
  if (text === null) {
    return KEEP_ALIVE_DEFAULT_ENABLED;
  }
  try {
    const doc: StoredKeepAlive = JSON.parse(text) as StoredKeepAlive;
    if (doc === null || doc === undefined) {
      return KEEP_ALIVE_DEFAULT_ENABLED;
    }
    if (doc.v !== KEEP_ALIVE_DOCUMENT_VERSION) {
      return KEEP_ALIVE_DEFAULT_ENABLED;
    }
    if (typeof doc.enabled !== 'boolean') {
      return KEEP_ALIVE_DEFAULT_ENABLED;
    }
    return doc.enabled;
  } catch (error) {
    return KEEP_ALIVE_DEFAULT_ENABLED;
  }
}

// Persist the switch. true = the document is on disk (temp + fsync + rename);
// false = nothing was written and the previous document is untouched.
export async function saveKeepAliveEnabled(filesDir: string,
  enabled: boolean): Promise<boolean> {
  const text: string =
    `{"v":${KEEP_ALIVE_DOCUMENT_VERSION},"enabled":${enabled ? 'true' : 'false'}}`;
  try {
    await writeAtomicText(filesDir, text);
    return true;
  } catch (error) {
    // A refused path (the whitelist above) and a device IO failure are the same
    // answer to the caller: the switch keeps its runtime effect, the file does
    // not change.
    return false;
  }
}
