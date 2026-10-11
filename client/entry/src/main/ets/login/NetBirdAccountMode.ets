/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors

export const ACCOUNT_MODE_FILE_NAME: string = 'netbird-auth-mode';
export const ACCOUNT_MODE_ACCOUNT: string = 'account';
export const ACCOUNT_MODE_SETUP_KEY: string = 'setup-key';
export const ACCOUNT_MODE_INVALID: string = 'invalid';

// A missing/empty marker preserves the legacy Setup Key flow. Unknown explicit
// values must not make an account session fall through to Setup Key.
export function parseAuthMode(text: string): string {
  const mode: string = text.trim();
  if (mode.length === 0 || mode === ACCOUNT_MODE_SETUP_KEY) {
    return ACCOUNT_MODE_SETUP_KEY;
  }
  if (mode === ACCOUNT_MODE_ACCOUNT) {
    return ACCOUNT_MODE_ACCOUNT;
  }
  return ACCOUNT_MODE_INVALID;
}

export function isAccountCredentialExpired(expiresAtMs: number, nowMs: number): boolean {
  return expiresAtMs > 0 && expiresAtMs <= nowMs;
}

// 2026-10-11 (account auth fix): account mode starts a connect even when the
// stored JWT is EMPTY. The management JWT is a one-shot identity proof — after
// it was used (or rejected as already used) the extension drops it while the
// rest of the account store stays, and a reconnect then has to identify the
// peer by its WireGuard public key alone (upstream: a normal reconnect sends
// Login with no JWT and no setup key; the core tries exactly that first). The
// still-relevant gates: no account document at all, or an EXPIRED stored token
// -> ask for 账户登录 instead of starting a connect that cannot register.
export function decideAccountCredentialMode(mode: string, hasCredentials: boolean, jwt: string,
  expiresAtMs: number, nowMs: number): string {
  if (mode === ACCOUNT_MODE_SETUP_KEY) {
    return ACCOUNT_MODE_SETUP_KEY;
  }
  if (mode !== ACCOUNT_MODE_ACCOUNT) {
    return ACCOUNT_MODE_INVALID;
  }
  if (!hasCredentials) {
    return 'account-login-required';
  }
  if (jwt.length === 0) {
    return ACCOUNT_MODE_ACCOUNT;
  }
  if (isAccountCredentialExpired(expiresAtMs, nowMs)) {
    return 'account-login-required';
  }
  return ACCOUNT_MODE_ACCOUNT;
}
