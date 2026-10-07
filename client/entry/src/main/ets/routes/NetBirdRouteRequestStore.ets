/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// File adapter for the live-route IPC pair (see NetBirdRouteRequest for the
// documents themselves):
//
//   filesDir/netbird-route-apply-request.json   UI  -> VPE
//   filesDir/netbird-route-apply-result.json    VPE -> UI
//
// Same minimal IO pattern as NetBirdRouteStore (private file, temp file,
// fsync, rename) and the same whitelist discipline: the path helper below can
// only ever name these two files plus their temp siblings, so this module can
// never touch the device config, the setup key, the account document, the
// route-selection document or the route catalog. Both documents are plain
// public configuration (an opaque id, a management URL, a state token and
// NetIDs are not secrets) and nothing here reads or writes credential material.
//
// Contract rules this adapter deliberately implements:
//
// - MISSING file -> null. An existing but malformed / wrong-version document is
//   a stable RoutePreferencesError (REASON_MALFORMED / REASON_VERSION) and a
//   real IO failure is REASON_UNAVAILABLE — never a fabricated "missing", which
//   would silently drop an apply the user asked for. The caller handles them.
// - Nothing is ever unlinked. The request is a RETAINED mailbox: the VPE
//   de-duplicates it with an in-memory lastProcessedId (its only live field) and
//   there is no TTL here, so a request written while the VPE was offline still
//   triggers a read of the CURRENT preference document on the next start.
// - Every read is a direct bounded read of the whole tiny file. There is NO
//   mtime/size "unchanged" shortcut: two atomic writes inside the same 100 ms
//   can produce equal byte counts and an equal mtime, and skipping the read
//   would then miss the newer request. The VPE's own busy guard bounds how
//   often that read happens.
// - The two files have independent writers that are serialized OUTSIDE this
//   module (the UI serializes its own request/result writes, the VPE serializes
//   the request read + result write); the atomic temp/fsync/rename write is what
//   keeps a concurrent reader from ever seeing a half-written document.
// - 16 KiB is the whole document bound (validated before the file is touched and
//   enforced again on read), which is ample for four fields or a route list.

import fs from '@ohos.file.fs';
import util from '@ohos.util';
import {
  REASON_MALFORMED, REASON_UNAVAILABLE, RoutePreferencesError
} from './NetBirdRoutePreferences';
import {
  parseRouteApplyRequestDocument, parseRouteApplyResultDocument,
  serializeRouteApplyRequestDocument, serializeRouteApplyResultDocument
} from './NetBirdRouteRequest';
import type { RouteApplyRequest, RouteApplyResult } from './NetBirdRouteRequest';

export const ROUTE_APPLY_REQUEST_FILE: string = 'netbird-route-apply-request.json';
export const ROUTE_APPLY_RESULT_FILE: string = 'netbird-route-apply-result.json';
export const MAX_ROUTE_APPLY_DOCUMENT_BYTES: number = 16 * 1024;
const TEMP_SUFFIX: string = '.tmp';

interface CodedFailure extends Error {
  code?: number;
}

function failureCode(error: Error): number {
  try {
    const code: number | undefined = (error as CodedFailure).code;
    return typeof code === 'number' && Number.isFinite(code) ? code : 0;
  } catch (ignored) {
    return 0;
  }
}

function unavailable(): RoutePreferencesError {
  return new RoutePreferencesError(REASON_UNAVAILABLE);
}

// Only the two IPC documents and their temp siblings can ever be addressed.
function pathFor(filesDir: string, basename: string): string {
  if (filesDir.length === 0 || filesDir.endsWith('/') ||
    basename !== ROUTE_APPLY_REQUEST_FILE && basename !== ROUTE_APPLY_RESULT_FILE &&
    basename !== ROUTE_APPLY_REQUEST_FILE + TEMP_SUFFIX &&
    basename !== ROUTE_APPLY_RESULT_FILE + TEMP_SUFFIX) {
    throw unavailable();
  }
  return `${filesDir}/${basename}`;
}

function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  const copy: Uint8Array = new Uint8Array(bytes.length);
  copy.set(bytes);
  return copy.buffer;
}

function encodeUtf8(text: string): Uint8Array {
  return new util.TextEncoder().encode(text);
}

function decodeUtf8(bytes: Uint8Array): string {
  try {
    return util.TextDecoder.create('utf-8', { fatal: true }).decodeToString(bytes);
  } catch (error) {
    throw new RoutePreferencesError(REASON_MALFORMED);
  }
}

// Bounded whole-file read. null = the file does not exist; a real IO failure is
// unavailable (never reported as "missing"); oversized or non-UTF-8 content is
// malformed.
async function readTextBounded(path: string, maxBytes: number): Promise<string | null> {
  let file: fs.File | null = null;
  const buffer: ArrayBuffer = new ArrayBuffer(maxBytes + 1);
  let total: number = 0;
  try {
    file = await fs.open(path, fs.OpenMode.READ_ONLY);
    while (total < buffer.byteLength) {
      const count: number = await fs.read(file.fd, buffer, { offset: total, length: buffer.byteLength - total });
      if (count <= 0) {
        break;
      }
      total += count;
    }
  } catch (error) {
    if (failureCode(error as Error) === 13900002) {
      return null;
    }
    throw unavailable();
  } finally {
    if (file !== null) {
      try {
        await fs.close(file);
      } catch (ignored) {
        // The primary result is preserved; a close failure carries no data.
      }
    }
  }
  if (total > maxBytes) {
    throw new RoutePreferencesError(REASON_MALFORMED);
  }
  return decodeUtf8(new Uint8Array(buffer, 0, total));
}

async function writeAtomicText(filesDir: string, basename: string, text: string): Promise<void> {
  const path: string = pathFor(filesDir, basename);
  const tempPath: string = pathFor(filesDir, basename + TEMP_SUFFIX);
  const bytes: Uint8Array = encodeUtf8(text);
  let file: fs.File | null = null;
  try {
    file = await fs.open(tempPath, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
    const written: number = await fs.write(file.fd, toArrayBuffer(bytes));
    if (written !== bytes.length) {
      throw unavailable();
    }
    await fs.fsync(file.fd);
    await fs.close(file);
    file = null;
    await fs.rename(tempPath, path);
  } catch (error) {
    if (error instanceof RoutePreferencesError) {
      throw error;
    }
    throw unavailable();
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

// The retained apply request, or null when the UI never wrote one. The document
// is NOT filtered by any management URL here: the reader loads the preference
// document with the CURRENT management URL itself, which is exactly why a
// request carries no scope-filtered selection copy.
export async function readRouteApplyRequest(filesDir: string): Promise<RouteApplyRequest | null> {
  const text: string | null = await readTextBounded(
    pathFor(filesDir, ROUTE_APPLY_REQUEST_FILE), MAX_ROUTE_APPLY_DOCUMENT_BYTES);
  if (text === null) {
    return null;
  }
  return parseRouteApplyRequestDocument(text);
}

// Publish the apply request. The document is validated and serialized BEFORE
// the file is touched, so an invalid request can never replace a good one; the
// write itself is temp + fsync + rename and never unlinks the file.
export async function writeRouteApplyRequest(filesDir: string,
  request: RouteApplyRequest): Promise<void> {
  const text: string = serializeRouteApplyRequestDocument(request);
  await writeAtomicText(filesDir, ROUTE_APPLY_REQUEST_FILE, text);
}

// The VPE's latest apply result, or null when it never wrote one. A reader
// compares the result's opaque id with its own request id; nothing here orders
// two results.
export async function readRouteApplyResult(filesDir: string): Promise<RouteApplyResult | null> {
  const text: string | null = await readTextBounded(
    pathFor(filesDir, ROUTE_APPLY_RESULT_FILE), MAX_ROUTE_APPLY_DOCUMENT_BYTES);
  if (text === null) {
    return null;
  }
  return parseRouteApplyResultDocument(text);
}

export async function writeRouteApplyResult(filesDir: string,
  result: RouteApplyResult): Promise<void> {
  const text: string = serializeRouteApplyResultDocument(result);
  await writeAtomicText(filesDir, ROUTE_APPLY_RESULT_FILE, text);
}
