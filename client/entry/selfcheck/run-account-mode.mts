/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Host-side checks for account-mode marker parsing and token-expiry policy.
// Run: node client/entry/selfcheck/run-account-mode.mts
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const SOURCE = join(here, '..', 'src', 'main', 'ets', 'login', 'NetBirdAccountMode.ets');
const dir = mkdtempSync(join(tmpdir(), 'nb-account-mode-'));
let passed = 0;
let failed = 0;
function check(name: string, condition: boolean): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
}

try {
  const bytes = readFileSync(SOURCE);
  const fixture = join(dir, 'NetBirdAccountMode.ts');
  writeFileSync(fixture, bytes);
  const mode = await import(pathToFileURL(fixture).href);
  check('marker filename and mode values are stable',
    mode.ACCOUNT_MODE_FILE_NAME === 'netbird-auth-mode' &&
    mode.ACCOUNT_MODE_ACCOUNT === 'account' && mode.ACCOUNT_MODE_SETUP_KEY === 'setup-key');
  check('missing or empty marker preserves legacy Setup Key mode',
    mode.parseAuthMode('') === 'setup-key' && mode.parseAuthMode('  ') === 'setup-key');
  check('literal marker values select their exact mode',
    mode.parseAuthMode('account') === 'account' && mode.parseAuthMode('setup-key') === 'setup-key');
  check('unknown explicit marker is rejected, not treated as Setup Key',
    mode.parseAuthMode('Account') === 'invalid' && mode.parseAuthMode('account-v2') === 'invalid');
  check('zero/unknown expiry is server-validated; elapsed positive expiry is rejected',
    !mode.isAccountCredentialExpired(0, 1000) &&
    !mode.isAccountCredentialExpired(-1, 1000) &&
    mode.isAccountCredentialExpired(999, 1000) &&
    !mode.isAccountCredentialExpired(1001, 1000));
  check('legacy mode decision never requires account credentials',
    mode.decideAccountCredentialMode('setup-key', false, '', 0, 1000) === 'setup-key');
  check('valid account JWT is accepted with unknown or future expiry',
    mode.decideAccountCredentialMode('account', true, 'jwt', 0, 1000) === 'account' &&
    mode.decideAccountCredentialMode('account', true, 'jwt', 1001, 1000) === 'account');
  check('missing credentials and an elapsed stored token require account login',
    mode.decideAccountCredentialMode('account', false, '', 0, 1000) === 'account-login-required' &&
    mode.decideAccountCredentialMode('account', true, 'jwt', 1000, 1000) === 'account-login-required' &&
    mode.decideAccountCredentialMode('account', false, 'jwt', 1001, 1000) === 'account-login-required');
  // 2026-10-11 (account auth fix): the stored management JWT is a ONE-SHOT
  // identity proof, dropped by the extension once it was used. An account store
  // with an empty JWT is therefore a normal, usable state: the next connect
  // logs in by WireGuard public key alone (the core tries a JWT-less Login
  // first), and the mode decision must not block it.
  check('an empty stored JWT is a valid public-key-only account session',
    mode.decideAccountCredentialMode('account', true, '', 0, 1000) === 'account' &&
    mode.decideAccountCredentialMode('account', true, '', 1000, 1000) === 'account');
  check('invalid explicit mode is not classified as legacy or account',
    mode.decideAccountCredentialMode('invalid', true, 'jwt', 0, 1000) === 'invalid');
  console.log(`RESULT passed=${passed} failed=${failed}`);
  if (failed > 0) { process.exitCode = 1; }
} finally {
  rmSync(dir, { recursive: true, force: true });
}
