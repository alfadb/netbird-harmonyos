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
const drop = ext.slice(ext.indexOf('private async dropStoredAccountJwt('),
  ext.indexOf('private writeConnectFailureForUi('));
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
check('mode decision blocks only a missing store or an elapsed stored token',
  mode.includes("return 'account-login-required'") &&
  mode.includes('if (!hasCredentials)') &&
  mode.includes('jwt.length === 0') &&
  mode.includes('isAccountCredentialExpired(expiresAtMs, nowMs)') &&
  mode.includes('expiresAtMs > 0'));

// 2026-10-11 (account auth fix): the stored management JWT is a ONE-SHOT
// identity proof (a second Login with it answers "JWT already used"), so the
// extension drops it — once the login it was offered to COMPLETED, and once the
// core reports the login-required class (token already used / peer session
// expired). The rewrite touches ONLY the `jwt` field and goes through the
// encrypted account store; the token never reaches a log line.
check('a completed account login drops the spent one-shot JWT',
  ext.includes('if (net !== null && authMode === ACCOUNT_MODE_ACCOUNT && accountJwt.length > 0)') &&
  ext.includes("await this.dropStoredAccountJwt('login-completed');"));
check('the core account-login-required class drops the stored JWT before the failure notice',
  ext.includes("if (failureClass === 'account-login-required')") &&
  ext.includes("await this.dropStoredAccountJwt('login-required');") &&
  ext.indexOf("await this.dropStoredAccountJwt('login-required');") <
    ext.indexOf('this.writeConnectFailureForUi(failureClass);'));
check('the drop rewrites only the jwt field through the encrypted account store',
  drop.includes('await loadAccountCredentials(this.context.filesDir)') &&
  drop.includes('await saveAccountCredentials(this.context.filesDir, cleared)') &&
  drop.includes("jwt: '',") && drop.includes('managementUrl: stored.managementUrl') &&
  drop.includes('clientId: stored.clientId') && drop.includes('tokenEndpoint: stored.tokenEndpoint') &&
  drop.includes('refreshToken: stored.refreshToken') &&
  !drop.includes('fs.write') && !drop.includes('fs.openSync') &&
  ext.includes("import { loadAccountCredentials, saveAccountCredentials } from '../login/NetBirdAccountStore'"));
check('the drop is best-effort and never logs the token',
  drop.includes("'VPN_ACCOUNT_JWT_DROPPED|requestId=%{public}s|reason=%{public}s|field=jwt'") &&
  drop.includes("'VPN_ACCOUNT_JWT_DROP_FAILED|requestId=%{public}s|reason=%{public}s|code=%{public}s'") &&
  drop.includes('stored.jwt.length === 0'));

console.log(`RESULT passed=${passed} failed=${failed}`);
if (failed > 0) { process.exitCode = 1; }
