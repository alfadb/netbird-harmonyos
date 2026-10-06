/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Platform adapter for the random AES-256 key used by a later encrypted-file
// credential store. AssetStoreKit is for short secrets (<1024 bytes), so only
// this 32-byte key belongs here; credential JSON belongs in a separate encrypted
// file. This module never logs, serializes, or returns the key as text.
//
// SDK 26: @kit.AssetStoreKit exports @ohos.security.asset. Async query/add/remove
// are Promise-based; query throws 24000002 for a missing alias and add throws
// 24000003 for a duplicate. DEVICE_FIRST_UNLOCKED permits background access
// after first unlock; AuthType.NONE adds no per-query interactive auth.

import asset from '@ohos.security.asset';
import cryptoFramework from '@ohos.security.cryptoFramework';

export const ACCOUNT_KEY_ALIAS: string = 'netbird-account-key-v1';
export const ACCOUNT_KEY_BYTES: number = 32;

const ERROR_NOT_FOUND: number = 24000002;
const ERROR_DUPLICATED: number = 24000003;
const ERROR_UNSUPPORTED: number = 24000017;
const ERROR_NOT_SYSTEM_APPLICATION: number = 202;

interface CodedFailure extends Error {
  code?: number;
}

export class AccountKeyStoreError extends Error {
  readonly reason: string;
  readonly code: number;

  constructor(reason: string, code: number) {
    super(reason);
    this.name = 'AccountKeyStoreError';
    this.reason = reason;
    this.code = code;
  }
}

function errorCode(error: Object): number {
  try {
    const code: number | undefined = (error as CodedFailure).code;
    return typeof code === 'number' && Number.isFinite(code) ? code : 0;
  } catch (ignored) {
    return 0;
  }
}

function failure(error: Object): AccountKeyStoreError {
  const code: number = errorCode(error);
  const reason: string = code === ERROR_UNSUPPORTED || code === ERROR_NOT_SYSTEM_APPLICATION
    ? 'unsupported' : 'unavailable';
  return new AccountKeyStoreError(reason, code);
}

function aliasBytes(): Uint8Array {
  const bytes: Uint8Array = new Uint8Array(ACCOUNT_KEY_ALIAS.length);
  for (let i: number = 0; i < ACCOUNT_KEY_ALIAS.length; i++) {
    bytes[i] = ACCOUNT_KEY_ALIAS.charCodeAt(i);
  }
  return bytes;
}

function queryAttributes(): asset.AssetMap {
  const query: asset.AssetMap = new Map<asset.Tag, asset.Value>();
  query.set(asset.Tag.ALIAS, aliasBytes());
  query.set(asset.Tag.RETURN_TYPE, asset.ReturnType.ALL);
  query.set(asset.Tag.RETURN_LIMIT, 1);
  return query;
}

function addAttributes(key: Uint8Array): asset.AssetMap {
  const attributes: asset.AssetMap = new Map<asset.Tag, asset.Value>();
  attributes.set(asset.Tag.ALIAS, aliasBytes());
  attributes.set(asset.Tag.SECRET, key);
  attributes.set(asset.Tag.ACCESSIBILITY, asset.Accessibility.DEVICE_FIRST_UNLOCKED);
  attributes.set(asset.Tag.AUTH_TYPE, asset.AuthType.NONE);
  attributes.set(asset.Tag.SYNC_TYPE, asset.SyncType.NEVER);
  attributes.set(asset.Tag.CONFLICT_RESOLUTION, asset.ConflictResolution.THROW_ERROR);
  return attributes;
}

function storedKey(results: Array<asset.AssetMap>): Uint8Array {
  if (results.length === 0) {
    throw new AccountKeyStoreError('malformed', 0);
  }
  const value: asset.Value | undefined = results[0].get(asset.Tag.SECRET);
  if (!(value instanceof Uint8Array) || value.length !== ACCOUNT_KEY_BYTES) {
    throw new AccountKeyStoreError('malformed', 0);
  }
  return new Uint8Array(value);
}

async function generateKey(): Promise<Uint8Array> {
  try {
    const blob: cryptoFramework.DataBlob =
      await cryptoFramework.createRandom().generateRandom(ACCOUNT_KEY_BYTES);
    if (!(blob.data instanceof Uint8Array) || blob.data.length !== ACCOUNT_KEY_BYTES) {
      throw new AccountKeyStoreError('malformed', 0);
    }
    return new Uint8Array(blob.data);
  } catch (error) {
    if (error instanceof AccountKeyStoreError) {
      throw error;
    }
    throw failure(error as Object);
  }
}

export async function loadAccountKey(): Promise<Uint8Array | null> {
  let results: Array<asset.AssetMap>;
  try {
    results = await asset.query(queryAttributes());
  } catch (error) {
    const code: number = errorCode(error as Object);
    if (code === ERROR_NOT_FOUND) {
      return null;
    }
    throw failure(error as Object);
  }
  return storedKey(results);
}

export async function loadOrCreateAccountKey(): Promise<Uint8Array> {
  const existing: Uint8Array | null = await loadAccountKey();
  if (existing !== null) {
    return existing;
  }

  const key: Uint8Array = await generateKey();
  try {
    await asset.add(addAttributes(key));
    return new Uint8Array(key);
  } catch (error) {
    const code: number = errorCode(error as Object);
    if (code !== ERROR_DUPLICATED) {
      throw failure(error as Object);
    }
    const winner: Uint8Array | null = await loadAccountKey();
    if (winner === null) {
      throw new AccountKeyStoreError('unavailable', ERROR_NOT_FOUND);
    }
    return winner;
  }
}

export async function clearAccountKey(): Promise<void> {
  const query: asset.AssetMap = new Map<asset.Tag, asset.Value>();
  query.set(asset.Tag.ALIAS, aliasBytes());
  try {
    await asset.remove(query);
  } catch (error) {
    const code: number = errorCode(error as Object);
    if (code === ERROR_NOT_FOUND) {
      return;
    }
    throw failure(error as Object);
  }
}
