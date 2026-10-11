/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Host-side checks for the auth-mode marker (which login method the local
// configuration uses). The account credential / expiry policy this suite used
// to drive is GONE with the account store: upstream keeps no token
// (client/internal/profilemanager/config.go:62-135), so there is no stored
// credential left to expire.
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
  check('the module carries no credential/token policy any more',
    bytes.toString('utf8').includes('export function parseAuthMode') &&
    !bytes.toString('utf8').includes('decideAccountCredentialMode') &&
    !bytes.toString('utf8').includes('isAccountCredentialExpired') &&
    !/export (const|function) .*(jwt|token)/i.test(bytes.toString('utf8')));
  console.log(`RESULT passed=${passed} failed=${failed}`);
  if (failed > 0) { process.exitCode = 1; }
} finally {
  rmSync(dir, { recursive: true, force: true });
}
