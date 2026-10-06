/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Focused host checks for account-mode credential selection at VPN start.
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

check('VPN start imports the production marker and encrypted-store APIs',
  ext.includes("from '../login/NetBirdAccountMode'") &&
  ext.includes("from '../login/NetBirdAccountStore'") &&
  ext.includes('loadAccountCredentials(filesDir, device.management_url)'));
check('missing marker keeps legacy Setup Key and dev override reads unchanged',
  start.includes('let authMode: string = ACCOUNT_MODE_SETUP_KEY') &&
  start.includes('if (fs.accessSync(modePath))') &&
  start.includes('setupKey = readCredentialFileText(filesDir, setupKeyFile)') &&
  start.includes('parseDeviceConfig(readCredentialFileText(filesDir, deviceFile))'));
check('account mode reads its device config only from filesDir and binds credentials to it',
  start.includes('parseDeviceConfig(readPrivateFileText(filesDir, deviceFile))') &&
  start.includes('loadAccountCredentials(filesDir, device.management_url)'));
check('account failure paths publish one stable login-required class without error text',
  start.includes("credentialFailure = 'account-login-required'") &&
  start.includes('this.writeConnectFailureForUi(credentialFailure)') &&
  start.includes("'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=credentials|errorClass=%{public}s'") &&
  !start.includes('error.message') && !start.includes('safeError(error as Error)'));
check('account credentials pass JWT only; legacy credentials retain setup_key',
  ext.includes("? { 'jwt': accountJwt } : { 'setup_key': setupKey }") &&
  !start.includes('refreshToken') && !start.includes('clearAccountCredentials'));
check('mode decision covers empty credentials and positive expiry while allowing unknown expiry',
  mode.includes("return 'account-login-required'") &&
  mode.includes('jwt.length === 0') &&
  mode.includes('isAccountCredentialExpired(expiresAtMs, nowMs)') &&
  mode.includes('expiresAtMs > 0'));

console.log(`RESULT passed=${passed} failed=${failed}`);
if (failed > 0) { process.exitCode = 1; }
