/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Encrypted credential file adapter. The AssetStoreKit backend holds only the
// 32-byte key; the complete credential document is encrypted before file IO.

import fs from '@ohos.file.fs';
import cryptoFramework from '@ohos.security.cryptoFramework';
import util from '@ohos.util';
import { AccountKeyStoreError, loadOrCreateAccountKey } from './NetBirdAccountKeyStore';
import {
  AccountStoreError, MAX_PAYLOAD_BYTES, parseCredentials, REASON_MALFORMED,
  REASON_MISMATCH, REASON_UNAVAILABLE, serializeCredentials
} from './NetBirdAccountStoreCore';
import type { AccountCredentials } from './NetBirdAccountStoreCore';

export const ACCOUNT_CREDENTIALS_FILE: string = 'netbird-account.enc';
const TEMP_FILE: string = 'netbird-account.enc.tmp';
const FORMAT_VERSION: number = 1;
const IV_BYTES: number = 12;
const TAG_BYTES: number = 16;
const MAX_ENVELOPE_BYTES: number = 128 * 1024;
const AAD: Uint8Array = new Uint8Array([
  0x6e, 0x65, 0x74, 0x62, 0x69, 0x72, 0x64, 0x2d, 0x61, 0x63, 0x63, 0x6f, 0x75, 0x6e, 0x74,
  0x2d, 0x65, 0x6e, 0x63, 0x2d, 0x76, 0x31, 0x00, 0x6e, 0x65, 0x74, 0x62, 0x69, 0x72, 0x64,
  0x2d, 0x61, 0x63, 0x63, 0x6f, 0x75, 0x6e, 0x74, 0x2d, 0x6b, 0x65, 0x79, 0x2d, 0x76, 0x31
]);

interface Envelope {
  v: number;
  iv: string;
  ciphertext: string;
  tag: string;
}

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

function stableError(reason: string): AccountStoreError {
  return new AccountStoreError(reason);
}

function pathFor(filesDir: string, basename: string): string {
  if (filesDir.length === 0 || filesDir.endsWith('/') || basename !== ACCOUNT_CREDENTIALS_FILE &&
    basename !== TEMP_FILE) {
    throw stableError(REASON_UNAVAILABLE);
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
    throw stableError(REASON_MALFORMED);
  }
}

function base64(bytes: Uint8Array): string {
  return new util.Base64Helper().encodeToStringSync(bytes);
}

function unbase64(value: string): Uint8Array {
  try {
    return new util.Base64Helper().decodeSync(value);
  } catch (error) {
    throw stableError(REASON_MALFORMED);
  }
}

async function secureRandom(length: number): Promise<Uint8Array> {
  try {
    const result: cryptoFramework.DataBlob = await cryptoFramework.createRandom().generateRandom(length);
    if (!(result.data instanceof Uint8Array) || result.data.length !== length) {
      throw stableError(REASON_UNAVAILABLE);
    }
    return new Uint8Array(result.data);
  } catch (error) {
    if (error instanceof AccountStoreError) {
      throw error;
    }
    throw stableError(REASON_UNAVAILABLE);
  }
}

async function cipherKey(): Promise<cryptoFramework.SymKey> {
  let raw: Uint8Array;
  try {
    raw = await loadOrCreateAccountKey();
  } catch (error) {
    const reason: string = error instanceof AccountKeyStoreError &&
      error.reason === 'unsupported' ? 'unsupported' : REASON_UNAVAILABLE;
    throw stableError(reason);
  }
  if (!(raw instanceof Uint8Array) || raw.length !== 32) {
    throw stableError(REASON_UNAVAILABLE);
  }
  try {
    return await cryptoFramework.createSymKeyGenerator('AES256').convertKey({ data: raw });
  } catch (error) {
    throw stableError(REASON_UNAVAILABLE);
  }
}

async function encrypt(plaintext: Uint8Array): Promise<Envelope> {
  const iv: Uint8Array = await secureRandom(IV_BYTES);
  const key: cryptoFramework.SymKey = await cipherKey();
  try {
    const cipher: cryptoFramework.Cipher = cryptoFramework.createCipher('AES256|GCM|NoPadding');
    const params: cryptoFramework.GcmParamsSpec = {
      algName: 'GCM', iv: { data: iv }, aad: { data: AAD }, authTag: { data: new Uint8Array(TAG_BYTES) }
    };
    await cipher.init(cryptoFramework.CryptoMode.ENCRYPT_MODE, key, params);
    const result: cryptoFramework.DataBlob = await cipher.doFinal({ data: plaintext });
    if (!(result.data instanceof Uint8Array) || result.data.length !== plaintext.length + TAG_BYTES) {
      throw stableError(REASON_UNAVAILABLE);
    }
    return {
      v: FORMAT_VERSION,
      iv: base64(iv),
      ciphertext: base64(result.data.slice(0, result.data.length - TAG_BYTES)),
      tag: base64(result.data.slice(result.data.length - TAG_BYTES))
    };
  } catch (error) {
    if (error instanceof AccountStoreError) {
      throw error;
    }
    throw stableError(REASON_UNAVAILABLE);
  }
}

async function decrypt(envelope: Envelope): Promise<Uint8Array> {
  const iv: Uint8Array = unbase64(envelope.iv);
  const ciphertext: Uint8Array = unbase64(envelope.ciphertext);
  const tag: Uint8Array = unbase64(envelope.tag);
  if (iv.length !== IV_BYTES || tag.length !== TAG_BYTES || ciphertext.length === 0 ||
    ciphertext.length > MAX_PAYLOAD_BYTES) {
    throw stableError(REASON_MALFORMED);
  }
  const key: cryptoFramework.SymKey = await cipherKey();
  try {
    const cipher: cryptoFramework.Cipher = cryptoFramework.createCipher('AES256|GCM|NoPadding');
    const params: cryptoFramework.GcmParamsSpec = {
      algName: 'GCM', iv: { data: iv }, aad: { data: AAD }, authTag: { data: tag }
    };
    await cipher.init(cryptoFramework.CryptoMode.DECRYPT_MODE, key, params);
    const result: cryptoFramework.DataBlob = await cipher.doFinal({ data: ciphertext });
    if (!(result.data instanceof Uint8Array) || result.data.length > MAX_PAYLOAD_BYTES) {
      throw stableError(REASON_MALFORMED);
    }
    return new Uint8Array(result.data);
  } catch (error) {
    if (error instanceof AccountStoreError) {
      throw error;
    }
    throw stableError(REASON_MALFORMED);
  }
}

function parseEnvelope(bytes: Uint8Array): Envelope {
  try {
    if (bytes.length === 0 || bytes.length > MAX_ENVELOPE_BYTES) {
      throw stableError(REASON_MALFORMED);
    }
    const value: Envelope | null = JSON.parse(decodeUtf8(bytes)) as Envelope | null;
    if (value === null || value.v !== FORMAT_VERSION || typeof value.iv !== 'string' ||
      typeof value.ciphertext !== 'string' || typeof value.tag !== 'string') {
      throw stableError(REASON_MALFORMED);
    }
    return value;
  } catch (error) {
    if (error instanceof AccountStoreError) {
      throw error;
    }
    throw stableError(REASON_MALFORMED);
  }
}

async function readBounded(path: string): Promise<Uint8Array | null> {
  let file: fs.File | null = null;
  try {
    file = await fs.open(path, fs.OpenMode.READ_ONLY);
    const buffer: ArrayBuffer = new ArrayBuffer(MAX_ENVELOPE_BYTES + 1);
    let total: number = 0;
    while (total < buffer.byteLength) {
      const count: number = await fs.read(file.fd, buffer, { offset: total, length: buffer.byteLength - total });
      if (count <= 0) {
        break;
      }
      total += count;
    }
    if (total > MAX_ENVELOPE_BYTES) {
      throw stableError(REASON_MALFORMED);
    }
    const bytes: Uint8Array = new Uint8Array(buffer, 0, total);
    return new Uint8Array(bytes);
  } catch (error) {
    if (failureCode(error as Error) === 13900002) {
      return null;
    }
    if (error instanceof AccountStoreError) {
      throw error;
    }
    throw stableError(REASON_UNAVAILABLE);
  } finally {
    if (file !== null) {
      try {
        await fs.close(file);
      } catch (error) {
        // The primary result is preserved; a close failure carries no file data.
      }
    }
  }
}

async function writeAtomic(path: string, bytes: Uint8Array): Promise<void> {
  const directory: string = path.substring(0, path.lastIndexOf('/'));
  const tempPath: string = pathFor(directory, TEMP_FILE);
  let file: fs.File | null = null;
  try {
    file = await fs.open(tempPath, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
    const written: number = await fs.write(file.fd, toArrayBuffer(bytes));
    if (written !== bytes.length) {
      throw stableError(REASON_UNAVAILABLE);
    }
    await fs.fsync(file.fd);
    await fs.close(file);
    file = null;
    await fs.rename(tempPath, path);
  } catch (error) {
    if (error instanceof AccountStoreError) {
      throw error;
    }
    throw stableError(REASON_UNAVAILABLE);
  } finally {
    if (file !== null) {
      try {
        await fs.close(file);
      } catch (error) {
        // Preserve the write failure; never expose SDK paths or messages.
      }
    }
    try {
      await fs.unlink(tempPath);
    } catch (error) {
      // A successful rename already consumed the temporary path.
    }
  }
}

export async function saveAccountCredentials(filesDir: string, credentials: AccountCredentials): Promise<void> {
  const payload: Uint8Array = encodeUtf8(serializeCredentials(credentials));
  if (payload.length === 0 || payload.length > MAX_PAYLOAD_BYTES) {
    throw stableError(REASON_MALFORMED);
  }
  const envelope: Envelope = await encrypt(payload);
  const bytes: Uint8Array = encodeUtf8(JSON.stringify(envelope));
  if (bytes.length === 0 || bytes.length > MAX_ENVELOPE_BYTES) {
    throw stableError(REASON_MALFORMED);
  }
  await writeAtomic(pathFor(filesDir, ACCOUNT_CREDENTIALS_FILE), bytes);
}

export async function loadAccountCredentials(filesDir: string,
  expectedManagementUrl?: string): Promise<AccountCredentials | null> {
  const bytes: Uint8Array | null = await readBounded(pathFor(filesDir, ACCOUNT_CREDENTIALS_FILE));
  if (bytes === null) {
    return null;
  }
  const envelope: Envelope = parseEnvelope(bytes);
  const plaintext: Uint8Array = await decrypt(envelope);
  const credentials: AccountCredentials = parseCredentials(decodeUtf8(plaintext));
  if (expectedManagementUrl !== undefined && credentials.managementUrl !== expectedManagementUrl) {
    throw stableError(REASON_MISMATCH);
  }
  return credentials;
}

export async function clearAccountCredentials(filesDir: string): Promise<void> {
  const path: string = pathFor(filesDir, ACCOUNT_CREDENTIALS_FILE);
  try {
    await fs.unlink(path);
  } catch (error) {
    if (failureCode(error as Error) !== 13900002) {
      throw stableError(REASON_UNAVAILABLE);
    }
  }
}
