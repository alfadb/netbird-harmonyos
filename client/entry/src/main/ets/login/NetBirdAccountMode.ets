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

export function decideAccountCredentialMode(mode: string, hasCredentials: boolean, jwt: string,
  expiresAtMs: number, nowMs: number): string {
  if (mode === ACCOUNT_MODE_SETUP_KEY) {
    return ACCOUNT_MODE_SETUP_KEY;
  }
  if (mode !== ACCOUNT_MODE_ACCOUNT) {
    return ACCOUNT_MODE_INVALID;
  }
  if (!hasCredentials || jwt.length === 0 || isAccountCredentialExpired(expiresAtMs, nowMs)) {
    return 'account-login-required';
  }
  return ACCOUNT_MODE_ACCOUNT;
}
