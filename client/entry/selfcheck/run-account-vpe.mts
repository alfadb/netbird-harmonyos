/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Focused host checks for the credential shape at VPN start (2026-10-12: the
// connect is token-less — upstream `loginToManagement`, client/internal/
// connect.go:774-796; only setup-key mode carries a credential, and it rides
// the core's `Register` second step).
// Run: node client/entry/selfcheck/run-account-vpe.mts

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ETS = join(HERE, '..', 'src', 'main', 'ets');
const ext = readFileSync(join(ETS, 'vpnextensionability', 'NetBirdVpnExtensionAbility.ets'), 'utf8');
const mode = readFileSync(join(ETS, 'login', 'NetBirdAccountMode.ets'), 'utf8');
const start = ext.slice(ext.indexOf('    let setupKey: string ='), ext.indexOf('    if (this.destroyRequested)',
  ext.indexOf('    let setupKey: string =')));
let passed = 0;
let failed = 0;
function check(name: string, ok: boolean): void {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}`);
  if (ok) { passed++; } else { failed++; }
}

check('VPN start reads the auth-mode marker without any credential store',
  ext.includes("from '../login/NetBirdAccountMode'") &&
  !ext.includes('NetBirdAccountStore') && !ext.includes('AccountCredentials'));
check('missing marker keeps legacy Setup Key and dev override reads unchanged',
  start.includes('let authMode: string = ACCOUNT_MODE_SETUP_KEY') &&
  start.includes('if (fs.accessSync(modePath))') &&
  start.includes('setupKey = readCredentialFileText(filesDir, setupKeyFile)') &&
  start.includes('parseDeviceConfig(readCredentialFileText(filesDir, deviceFile))'));
check('account mode reads its device config only from filesDir (identity + management URL)',
  start.includes('parseDeviceConfig(readPrivateFileText(filesDir, deviceFile))') &&
  start.includes("credentialFailure = 'needs-login'") &&
  !start.includes('loadAccountCredentials') &&
  !start.includes('decideAccountCredentialMode'));
check('account failure paths publish one stable needs-login class without error text',
  start.includes('this.writeConnectFailureForUi(credentialFailure)') &&
  start.includes("'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=credentials|errorClass=%{public}s'") &&
  !start.includes('error.message') && !start.includes('safeError(error as Error)'));
check('the connect is token-less in account mode; legacy mode keeps setup_key',
  ext.includes("? {} : { 'setup_key': setupKey }") &&
  !ext.includes("'jwt'") && !ext.includes('refreshToken'));

// 2026-10-12: the stored-JWT path is GONE — no account document, no drop
// workaround, no silent session renewal. The core holds no token, so the only
// login-required fact is the server's own `needs-login` class, published
// verbatim.
check('no account token store, no drop workaround, no renewal in the extension',
  !ext.includes('dropStoredAccountJwt') && !ext.includes('VPN_ACCOUNT_JWT_DROPPED') &&
  !ext.includes('clearAccountCredentials') && !ext.includes('saveAccountCredentials') &&
  !ext.includes('extend_auth_session') && !ext.includes('ExtendAuthSession') &&
  !ext.includes('session_renew'));
check('the core needs-login class travels to the UI untouched',
  ext.includes("'needs-login'") && ext.includes('this.writeConnectFailureForUi(failureClass);'));
check('the mode marker module carries no credential decision any more',
  mode.includes('export function parseAuthMode') &&
  !mode.includes('decideAccountCredentialMode') && !mode.includes('isAccountCredentialExpired'));

console.log(`RESULT passed=${passed} failed=${failed}`);
if (failed > 0) { process.exitCode = 1; }
