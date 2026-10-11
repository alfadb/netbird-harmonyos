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
// - Every read is preceded by a cheap synchronous stat (fs.statSync, the same
//   synchronous fs half the callers already use, e.g. fs.accessSync) and an
//   unchanged document is answered from the previous successful read instead of
//   being opened, read and decoded again — the VPE polls the request on its
//   100 ms socket-supply tick. The probe compares the whole file identity (ino,
//   size, mtime, ctime plus the nanosecond stamps the platform reports) AND is
//   bypassed for any document whose whole-second mtime is not safely in the
//   past: two same-size writes inside one second can produce equal
//   second-granular metadata. The cache block below spells out why no write
//   this module makes can be missed, and a missing file still reads as null
//   (now without even allocating the bound buffer).
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
// The SDK's documented "no such file or directory" code: the ONE error that
// means "this document was never written" (both for open and for stat).
const NOT_FOUND_CODE: number = 13900002;

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

// ONE decoder for the whole module. The VPE polls the request document on its
// 100 ms socket-supply tick, so building a fresh TextDecoder for every poll was
// pure per-tick overhead. A whole-buffer decodeToString leaves no state behind
// (only a streamed decode would), so a single instance is safe to reuse.
const UTF8_DECODER: util.TextDecoder = util.TextDecoder.create('utf-8', { fatal: true });

function decodeUtf8(bytes: Uint8Array): string {
  try {
    return UTF8_DECODER.decodeToString(bytes);
  } catch (error) {
    throw new RoutePreferencesError(REASON_MALFORMED);
  }
}

// ---------------------------------------------------------------------------
// the cheap "unchanged document" probe (one stat, no read)
// ---------------------------------------------------------------------------
//
// Both documents are polled by a timer (the VPE reads the request on its 100 ms
// socket-supply tick, the page reads the result on its 2 s status tick) and are
// written only by the atomic temp + fsync + rename below. Without the probe
// every poll allocated the 16 KiB bound buffer and re-opened, re-read and
// re-decoded the whole document even when nothing had changed since the
// previous poll.
//
// Why the identity includes ino AND the nanosecond stamps: mtime is reported in
// whole seconds, so two same-size writes inside one second can produce equal
// size+mtime+ctime. `ino` alone already catches every write THIS module makes
// (the rename swaps the file), and the ns mtime/ctime (optional in the SDK
// declaration, present from API 15) catch a foreign writer that rewrites the
// file IN PLACE. On top of both, the cache is bypassed whenever the document's
// mtime second is not safely in the past — the conservative leg for a platform
// that reports no nanosecond stamps. What is left after all three is a foreign
// in-place rewrite, with the same byte count, inside the SECOND already
// recorded in the cache, on a platform without nanosecond stamps; no write this
// module performs can look like that.
interface DocumentStamp {
  ino: bigint;
  size: number;
  mtime: number;
  ctime: number;
  mtimeNs: bigint | undefined;
  ctimeNs: bigint | undefined;
}

interface CachedDocument {
  text: string;
  stamp: DocumentStamp;
}

// Keyed by the document's full path: the request and the result are independent
// documents and pathFor can never name anything else.
const documentCache: Map<string, CachedDocument> = new Map<string, CachedDocument>();

function sameStamp(a: DocumentStamp, b: DocumentStamp): boolean {
  return a.ino === b.ino && a.size === b.size &&
    a.mtime === b.mtime && a.ctime === b.ctime &&
    a.mtimeNs === b.mtimeNs && a.ctimeNs === b.ctimeNs;
}

// A document whose whole-second mtime is the CURRENT second (or later: a clock
// that stepped back must not make the cache look trustworthy either) is always
// read for real. Its second-granular metadata cannot separate two same-size
// writes inside one second, and the nanosecond stamps are exactly what the
// platform may not report. This costs one extra real read per second per
// document at most.
function stampIsFresh(stamp: DocumentStamp): boolean {
  return stamp.mtime >= Math.floor(Date.now() / 1000);
}

// The cheap probe. null = the file does not exist, which is the SAME answer the
// read below reports for a missing document; a real stat failure is unavailable,
// never a fabricated "missing".
function stampOf(path: string): DocumentStamp | null {
  try {
    const stat: fs.Stat = fs.statSync(path);
    return {
      ino: stat.ino, size: stat.size, mtime: stat.mtime, ctime: stat.ctime,
      mtimeNs: stat.mtimeNs, ctimeNs: stat.ctimeNs
    };
  } catch (error) {
    if (failureCode(error as Error) === NOT_FOUND_CODE) {
      return null;
    }
    throw unavailable();
  }
}

// Bounded whole-file read. null = the file does not exist; a real IO failure is
// unavailable (never reported as "missing"); oversized or non-UTF-8 content is
// malformed. An unchanged document is served from the last successful read: the
// returned TEXT is the same bytes that read produced, so every caller-visible
// result (including a parse error the caller then raises) is unchanged.
async function readTextBounded(path: string, maxBytes: number): Promise<string | null> {
  const stamp: DocumentStamp | null = stampOf(path);
  if (stamp === null) {
    documentCache.delete(path);
    return null;
  }
  const cached: CachedDocument | undefined = documentCache.get(path);
  if (cached !== undefined && sameStamp(cached.stamp, stamp) && !stampIsFresh(stamp)) {
    return cached.text;
  }
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
    if (failureCode(error as Error) === NOT_FOUND_CODE) {
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
  const text: string = decodeUtf8(new Uint8Array(buffer, 0, total));
  // The stamp was taken BEFORE the read: a write racing this read swaps the
  // inode, so the next probe cannot mistake the newer document for this text.
  documentCache.set(path, { text: text, stamp: stamp });
  return text;
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
