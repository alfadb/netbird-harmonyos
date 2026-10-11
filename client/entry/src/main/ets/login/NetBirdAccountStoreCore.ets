/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Pure credential document validation and parsing. Platform encryption and
// private-file IO live in NetBirdAccountStore.ets.

export const PAYLOAD_VERSION: number = 1;
export const MAX_PAYLOAD_BYTES: number = 64 * 1024;
export const MAX_URL_CHARS: number = 2048;
export const MAX_TOKEN_CHARS: number = 16 * 1024;
export const MAX_CLIENT_ID_CHARS: number = 512;
export const REASON_UNAVAILABLE: string = 'unavailable';
export const REASON_UNSUPPORTED: string = 'unsupported';
export const REASON_MALFORMED: string = 'malformed';
export const REASON_MISMATCH: string = 'mismatch';

export interface AccountCredentials {
  managementUrl: string;
  clientId: string;
  tokenEndpoint: string;
  useIdToken: boolean;
  jwt: string;
  refreshToken: string;
  expiresAtMs: number;
}

export class AccountStoreError extends Error {
  readonly reason: string;
  // Which step failed, the platform BusinessError code (0 = the failing error
  // carried no numeric code) and the platform error's own message text. The
  // stable `message` stays the reason, so callers never see raw SDK text; only
  // the login UI's diagnostics suffix and the one hilog line read these three.
  readonly step: string;
  readonly code: number;
  readonly detail: string;

  constructor(reason: string, step?: string, code?: number, detail?: string) {
    super(reason);
    this.name = 'AccountStoreError';
    this.reason = reason;
    this.step = step ?? '';
    this.code = code ?? 0;
    this.detail = detail ?? '';
  }
}

function hasWhitespaceOrControl(value: string): boolean {
  for (let i: number = 0; i < value.length; i++) {
    const code: number = value.charCodeAt(i);
    if (code <= 0x20 || code === 0x7f) {
      return true;
    }
  }
  return false;
}

export function isHttpsEndpoint(value: string): boolean {
  if (value.length === 0 || value.length > MAX_URL_CHARS || !value.startsWith('https://') ||
    hasWhitespaceOrControl(value)) {
    return false;
  }
  const rest: string = value.substring(8);
  let cut: number = rest.length;
  for (const separator of ['/', '?', '#']) {
    const at: number = rest.indexOf(separator);
    if (at >= 0 && at < cut) {
      cut = at;
    }
  }
  const authority: string = rest.substring(0, cut);
  if (authority.length === 0 || authority.indexOf('@') >= 0) {
    return false;
  }
  const colonAt: number = authority.indexOf(':');
  if (colonAt >= 0) {
    const host: string = authority.substring(0, colonAt);
    const port: string = authority.substring(colonAt + 1);
    if (host.length === 0 || !/^[0-9]+$/.test(port)) {
      return false;
    }
    const portNumber: number = Number(port);
    if (portNumber < 1 || portNumber > 65535) {
      return false;
    }
  }
  return true;
}

export function validateCredentials(credentials: AccountCredentials): void {
  if (!isHttpsEndpoint(credentials.managementUrl) || !isHttpsEndpoint(credentials.tokenEndpoint)) {
    throw new AccountStoreError(REASON_MALFORMED);
  }
  if (credentials.clientId.length === 0 || credentials.clientId.length > MAX_CLIENT_ID_CHARS ||
    hasWhitespaceOrControl(credentials.clientId)) {
    throw new AccountStoreError(REASON_MALFORMED);
  }
  // 2026-10-11 (account auth fix): `jwt` may be EMPTY. The stored JWT is a
  // one-shot identity proof; once the connector reports a completed login (or
  // the token is rejected as already used) the extension rewrites this
  // document with jwt: '' so the dead token can never be resent. The
  // management URL / client id / token endpoint / refresh token stay, so
  // account mode remains selected and the next connect logs in by WireGuard
  // public key alone. A non-empty token is still length-checked and
  // whitespace-free.
  if (typeof credentials.useIdToken !== 'boolean' ||
    credentials.jwt.length > MAX_TOKEN_CHARS || hasWhitespaceOrControl(credentials.jwt) ||
    credentials.refreshToken.length > MAX_TOKEN_CHARS || hasWhitespaceOrControl(credentials.refreshToken)) {
    throw new AccountStoreError(REASON_MALFORMED);
  }
  if (typeof credentials.expiresAtMs !== 'number' || !Number.isFinite(credentials.expiresAtMs)) {
    throw new AccountStoreError(REASON_MALFORMED);
  }
}

export function serializeCredentials(credentials: AccountCredentials): string {
  validateCredentials(credentials);
  return JSON.stringify({
    v: PAYLOAD_VERSION,
    managementUrl: credentials.managementUrl,
    clientId: credentials.clientId,
    tokenEndpoint: credentials.tokenEndpoint,
    useIdToken: credentials.useIdToken,
    jwt: credentials.jwt,
    refreshToken: credentials.refreshToken,
    expiresAtMs: credentials.expiresAtMs
  });
}

interface StoredAccountDoc {
  v?: number;
  managementUrl?: string;
  clientId?: string;
  tokenEndpoint?: string;
  useIdToken?: boolean;
  jwt?: string;
  refreshToken?: string;
  expiresAtMs?: number;
}

export function parseCredentials(text: string): AccountCredentials {
  try {
    if (text.length === 0 || text.length > MAX_PAYLOAD_BYTES) {
      throw new AccountStoreError(REASON_MALFORMED);
    }
    const doc: StoredAccountDoc | null = JSON.parse(text) as StoredAccountDoc | null;
    if (doc === null || doc.v !== PAYLOAD_VERSION || typeof doc.managementUrl !== 'string' ||
      typeof doc.clientId !== 'string' || typeof doc.tokenEndpoint !== 'string' ||
      typeof doc.useIdToken !== 'boolean' || typeof doc.jwt !== 'string' ||
      typeof doc.refreshToken !== 'string' || typeof doc.expiresAtMs !== 'number') {
      throw new AccountStoreError(REASON_MALFORMED);
    }
    const credentials: AccountCredentials = {
      managementUrl: doc.managementUrl,
      clientId: doc.clientId,
      tokenEndpoint: doc.tokenEndpoint,
      useIdToken: doc.useIdToken,
      jwt: doc.jwt,
      refreshToken: doc.refreshToken,
      expiresAtMs: doc.expiresAtMs
    };
    validateCredentials(credentials);
    return credentials;
  } catch (error) {
    if (error instanceof AccountStoreError) {
      throw error;
    }
    throw new AccountStoreError(REASON_MALFORMED);
  }
}
