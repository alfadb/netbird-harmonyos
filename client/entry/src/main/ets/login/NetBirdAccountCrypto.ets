/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// WebView account-login PKCE adapter. The auth core remains platform-free;
// this module owns only secure random generation and S256 derivation.
import cryptoFramework from '@ohos.security.cryptoFramework';
import util from '@ohos.util';
import { checkRedirectUri } from './NetBirdAccountAuthCore';
import type { PkceSession } from './NetBirdAccountAuthCore';

const RANDOM_BYTES: number = 32;
function bytesToHex(bytes: Uint8Array): string {
  const digits: string = '0123456789abcdef';
  let out: string = '';
  for (let i: number = 0; i < bytes.length; i++) {
    const value: number = bytes[i];
    out += digits.charAt(value >> 4) + digits.charAt(value & 15);
  }
  return out;
}

function base64Url(bytes: Uint8Array): string {
  const standard: string = new util.Base64Helper().encodeToStringSync(bytes);
  return standard.replace(/=/g, '').split('+').join('-').split('/').join('_');
}

function asciiBytes(value: string): Uint8Array {
  const bytes: Uint8Array = new Uint8Array(value.length);
  for (let i: number = 0; i < value.length; i++) {
    const code: number = value.charCodeAt(i);
    if (code > 0x7F) {
      throw new Error('pkce_verifier_invalid');
    }
    bytes[i] = code;
  }
  return bytes;
}

// Kept public so the RFC 7636 Appendix B vector can verify the device digest
// path without requiring an on-device random source.
export async function computePkceChallenge(verifier: string): Promise<string> {
  try {
    const md: cryptoFramework.Md = cryptoFramework.createMd('SHA256');
    await md.update({ data: asciiBytes(verifier) });
    const digest: cryptoFramework.DataBlob = await md.digest();
    return base64Url(digest.data);
  } catch (error) {
    if (error instanceof Error && error.message === 'pkce_verifier_invalid') {
      throw error;
    }
    throw new Error('pkce_crypto_unavailable');
  }
}

export async function createPkceSession(redirectUri: string): Promise<PkceSession> {
  const redirect = checkRedirectUri(redirectUri);
  if (!redirect.ok) {
    throw new Error(redirect.error);
  }
  try {
    const random = cryptoFramework.createRandom();
    const verifierBytes: cryptoFramework.DataBlob = random.generateRandomSync(RANDOM_BYTES);
    const stateBytes: cryptoFramework.DataBlob = random.generateRandomSync(RANDOM_BYTES);
    const verifier: string = base64Url(verifierBytes.data);
    const state: string = bytesToHex(stateBytes.data);
    const challenge: string = await computePkceChallenge(verifier);
    return { state: state, verifier: verifier, challenge: challenge, redirectUri: redirect.url };
  } catch (error) {
    if (error instanceof Error && error.message === 'pkce_crypto_unavailable') {
      throw error;
    }
    throw new Error('pkce_crypto_unavailable');
  }
}
