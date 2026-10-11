/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// File adapter for the per-NetID route-selection document and the public route
// catalog. Same minimal IO pattern as NetBirdKeepAliveStore (private file, temp
// file, fsync, rename) but with NO key material and NO crypto: both documents
// are plain public configuration (a management URL and route NetIDs/prefixes
// are not secrets). The path whitelist below can only ever name these two
// files plus their temp siblings, so this module can never touch the device
// config or the setup key.
//
// Scope rules (NetBirdRoutePreferences owns the parsing):
// - missing selection file / a selection for ANOTHER management URL -> null
//   (the caller uses the default all-on state);
// - an existing but malformed or wrong-version selection file -> a stable
//   RoutePreferencesError, never a silent default, and the file is never
//   rewritten by a read;
// - the catalog is read offline for the UI (no native process call) and its
//   updated_at_ms is reported so the UI can mark the cache stale.

import fs from '@ohos.file.fs';
import util from '@ohos.util';
import {
  CATALOG_STATUS_MALFORMED, CATALOG_STATUS_MISSING, CATALOG_STATUS_OK,
  CATALOG_STATUS_SCOPE_MISMATCH, CATALOG_STATUS_UNAVAILABLE, CATALOG_STATUS_VERSION,
  REASON_MALFORMED, REASON_UNAVAILABLE, REASON_VERSION, RoutePreferencesError,
  parseRouteCatalogDocument, parseRouteSelectionDocument, serializeRouteCatalogDocument,
  serializeRouteSelectionDocument
} from './NetBirdRoutePreferences';
import type {
  RouteCatalog, RouteCatalogReadResult, RouteSelection, StoredRouteCatalog, StoredRouteSelection
} from './NetBirdRoutePreferences';

export const ROUTE_SELECTION_FILE: string = 'netbird-route-selection.json';
export const ROUTE_CATALOG_FILE: string = 'netbird-routes.json';
export const MAX_SELECTION_DOCUMENT_BYTES: number = 64 * 1024;
export const MAX_CATALOG_DOCUMENT_BYTES: number = 512 * 1024;
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

// Only the two route documents and their temp siblings can ever be addressed.
function pathFor(filesDir: string, basename: string): string {
  if (filesDir.length === 0 || filesDir.endsWith('/') ||
    basename !== ROUTE_SELECTION_FILE && basename !== ROUTE_CATALOG_FILE &&
    basename !== ROUTE_SELECTION_FILE + TEMP_SUFFIX && basename !== ROUTE_CATALOG_FILE + TEMP_SUFFIX) {
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

// Bounded read. null = the file does not exist; a real IO failure is
// unavailable (never reported as "missing", which would silently reset the
// user's selection); oversized or non-UTF-8 content is malformed.
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

function catalogResult(status: string, managementUrl: string, updatedAtMs: number,
  requestId: string, catalog: RouteCatalog | null): RouteCatalogReadResult {
  return {
    status: status,
    management_url: managementUrl,
    updated_at_ms: updatedAtMs,
    request_id: requestId,
    catalog: catalog
  };
}

function catalogFailureStatus(reason: string): string {
  return reason === REASON_VERSION ? CATALOG_STATUS_VERSION : CATALOG_STATUS_MALFORMED;
}

// The stored selection for THIS management server, or null when there is none
// (missing file, or a document scoped to another server). Throws
// REASON_MALFORMED / REASON_VERSION / REASON_UNAVAILABLE for an existing
// document that cannot be trusted — the caller must not substitute the
// default all-on state, because the next save would overwrite the evidence.
export async function loadRouteSelection(filesDir: string,
  managementUrl: string): Promise<RouteSelection | null> {
  const text: string | null =
    await readTextBounded(pathFor(filesDir, ROUTE_SELECTION_FILE), MAX_SELECTION_DOCUMENT_BYTES);
  if (text === null) {
    return null;
  }
  const stored: StoredRouteSelection = parseRouteSelectionDocument(text);
  if (stored.management_url !== managementUrl) {
    return null;
  }
  return stored.selection;
}

// Persist the selection for THIS management server. The document is validated
// and serialized BEFORE the file is touched, so an invalid state can never
// replace a good one; the write itself is temp + fsync + rename.
export async function saveRouteSelection(filesDir: string, managementUrl: string,
  selection: RouteSelection): Promise<void> {
  const text: string = serializeRouteSelectionDocument(managementUrl, selection);
  await writeAtomicText(filesDir, ROUTE_SELECTION_FILE, text);
}

// Offline catalog read for the UI. Never throws and never starts a native
// process: the status token tells the UI what it has, and catalog is non-null
// only for CATALOG_STATUS_OK.
export async function readRouteCatalog(filesDir: string,
  managementUrl: string): Promise<RouteCatalogReadResult> {
  let text: string | null = null;
  try {
    text = await readTextBounded(pathFor(filesDir, ROUTE_CATALOG_FILE), MAX_CATALOG_DOCUMENT_BYTES);
  } catch (error) {
    const readMalformed: boolean = error instanceof RoutePreferencesError &&
      error.reason === REASON_MALFORMED;
    return catalogResult(readMalformed ? CATALOG_STATUS_MALFORMED : CATALOG_STATUS_UNAVAILABLE, '', 0, '', null);
  }
  if (text === null) {
    return catalogResult(CATALOG_STATUS_MISSING, '', 0, '', null);
  }
  let stored: StoredRouteCatalog | null = null;
  let failureStatus: string = '';
  try {
    stored = parseRouteCatalogDocument(text);
  } catch (error) {
    failureStatus = error instanceof RoutePreferencesError
      ? catalogFailureStatus(error.reason)
      : CATALOG_STATUS_MALFORMED;
  }
  if (stored === null) {
    return catalogResult(failureStatus === '' ? CATALOG_STATUS_MALFORMED : failureStatus, '', 0, '', null);
  }
  if (stored.management_url !== managementUrl) {
    return catalogResult(CATALOG_STATUS_SCOPE_MISMATCH, stored.management_url, stored.updated_at_ms,
      stored.request_id, null);
  }
  return catalogResult(CATALOG_STATUS_OK, stored.management_url, stored.updated_at_ms,
    stored.request_id, stored.catalog);
}

// Catalog writer for the VPE. `requestId` is the writer's own request id and
// `updatedAtMs` is passed in (no clock inside the store, so the caller owns the
// stamp and host tests are deterministic). The catalog is validated before the
// file is touched; nothing here reads or writes credentials.
export async function writeRouteCatalog(filesDir: string, managementUrl: string,
  catalog: RouteCatalog, requestId: string, updatedAtMs: number): Promise<void> {
  const text: string = serializeRouteCatalogDocument(managementUrl, catalog, requestId, updatedAtMs);
  await writeAtomicText(filesDir, ROUTE_CATALOG_FILE, text);
}
