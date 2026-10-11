/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Which login method the local configuration uses. Both modes keep the SAME
// on-disk shape upstream keeps: a device config (management URL + persistent
// WireGuard identity) plus this marker. There is no credential document and no
// token: the management JWT is used once, inside the login action, and dropped
// with the call (upstream client/internal/profilemanager/config.go:62-135 has
// no token field; client/android/login.go:181-186 stores only an account e-mail
// hint, which this shell does not use).

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
