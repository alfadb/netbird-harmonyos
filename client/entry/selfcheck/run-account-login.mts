/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
// Host checks for login orchestration. No device, HTTP, or real authorization codes.
// Run: node client/entry/selfcheck/run-account-login.mts
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./account-login-hooks.mjs', import.meta.url).href);
const here = dirname(fileURLToPath(import.meta.url));
const loginDir = join(here, '..', 'src', 'main', 'ets', 'login');
const dir = mkdtempSync(join(tmpdir(), 'nb-account-login-'));
const metadata = {
  client_id: 'client-1', audience: '', authorization_endpoint: 'https://idp.example/authorize',
  token_endpoint: 'https://idp.example/token', scope: 'openid profile', use_id_token: false,
  redirect_urls: ['bad-uri', 'https://client.example/callback'],
  disable_prompt_login: false, login_flag: 0, client_secret: 'must-not-project'
};
const state = {
  requests: [], cancelled: [], pollIds: [], polls: [], exchanges: [], transportCancels: 0,
  requestId: 40, metadata, pollWait: false, tokenResult: {
    ok: true, errorCode: '', errorDetail: '',
    tokens: { accessToken: 'access-secret', idToken: 'id-secret', refreshToken: 'refresh-secret', expiresIn: 90 }
  }
};
(globalThis as unknown as { __NB_ACCOUNT_LOGIN: typeof state }).__NB_ACCOUNT_LOGIN = state;
let passed = 0;
let failed = 0;
function check(name: string, condition: boolean): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) passed++; else failed++;
}
async function rejected(promise: Promise<unknown>, expected: string): Promise<boolean> {
  try { await promise; return false; }
  catch (error) { return error instanceof Error && error.message === expected; }
}
async function freshModule(): Promise<{ AccountLoginController: new () => any }> {
  const file = join(dir, 'NetBirdAccountLogin.ts');
  const bytes = readFileSync(join(loginDir, 'NetBirdAccountLogin.ets'));
  writeFileSync(file, bytes);
  return await import(pathToFileURL(file).href) as { AccountLoginController: new () => any };
}
try {
  for (const file of ['NetBirdAccountLogin.ets', 'NetBirdAccountAuthCore.ets']) {
    const source = readFileSync(join(loginDir, file));
    writeFileSync(join(dir, file.replace('.ets', '.ts')), source);
    check(`${file} fixture copies source bytes`, readFileSync(join(dir, file.replace('.ets', '.ts'))).equals(source));
  }
  const { AccountLoginController } = await freshModule();
  const controller = new AccountLoginController();
  const prepared = await controller.begin('https://mgmt.example', '-----BEGIN CERTIFICATE-----\nCA\n-----END CERTIFICATE-----');
  const start = state.requests[state.requests.length - 1];
  check('discovery receives only management URL and required CA',
    start.management_url === 'https://mgmt.example' && start.ca_pem.includes('CERTIFICATE') &&
    Object.keys(start).length === 2);
  check('authorize URL has PKCE, exact chosen redirect, query response and login flag mapping',
    prepared.expectedState === 'secure-state' && prepared.redirectUri === 'https://client.example/callback' &&
    prepared.authorizeUrl.includes('response_mode=query') && prepared.authorizeUrl.includes('prompt=login') &&
    prepared.authorizeUrl.includes('scope=openid%20profile') && !prepared.authorizeUrl.includes('offline_access') &&
    !prepared.authorizeUrl.includes('client_secret'));
  let busy = false;
  try { await new AccountLoginController().begin('https://other.example', 'ca'); }
  catch (error) { busy = error instanceof Error && error.message === 'busy'; }
  check('a second controller cannot replace the active native discovery owner', busy);

  const credentials = await controller.finish('callback-code');
  check('finish selects access JWT and maps refresh token and positive expiry',
    credentials.jwt === 'access-secret' && credentials.refreshToken === 'refresh-secret' &&
    credentials.expiresAtMs > Date.now() && credentials.expiresAtMs <= Date.now() + 90000 &&
    credentials.managementUrl === 'https://mgmt.example');
  check('duplicate finish is rejected and code is exchanged once',
    await rejected(controller.finish('callback-code'), 'exchange_already_used') && state.exchanges.length === 1);

  state.metadata = { ...metadata, disable_prompt_login: true, login_flag: 2, use_id_token: true, scope: 'openid' };
  state.tokenResult.tokens.expiresIn = 0;
  const second = new AccountLoginController();
  const noPrompt = await second.begin('https://mgmt.example', 'ca');
  const idCredentials = await second.finish('another-code');
  check('disabled prompts stay omitted and use_id_token selects id JWT with no expiry',
    !noPrompt.authorizeUrl.includes('prompt=') && !noPrompt.authorizeUrl.includes('max_age=') &&
    idCredentials.jwt === 'id-secret' && idCredentials.expiresAtMs === 0);

  state.metadata = { ...metadata, login_flag: 1 };
  const maxAge = new AccountLoginController();
  const maxAgePrepared = await maxAge.begin('https://mgmt.example', 'ca');
  maxAge.cancel();
  check('login flag one maps to max_age and prepared cancellation blocks finish',
    maxAgePrepared.authorizeUrl.includes('max_age=0') && !maxAgePrepared.authorizeUrl.includes('prompt=') &&
    await rejected(maxAge.finish('cancelled-code'), 'login_not_prepared'));

  state.metadata = { ...metadata, login_flag: 8 };
  const invalidFlag = new AccountLoginController();
  check('unknown login flag fails closed after discovery completes',
    await rejected(invalidFlag.begin('https://mgmt.example', 'ca'), 'metadata_invalid'));

  state.metadata = metadata;
  state.polls.push({ state: 'failed', error: 'private discovery detail' });
  const failedDiscovery = new AccountLoginController();
  check('native failed state becomes a stable error and is cleaned up',
    await rejected(failedDiscovery.begin('https://mgmt.example', 'ca'), 'discovery_failed') &&
    state.cancelled.length > 0);

  state.pollWait = true;
  const polling = new AccountLoginController();
  const pendingBegin = polling.begin('https://mgmt.example', 'ca');
  polling.cancel();
  check('cancel during polling wakes promptly and rejects stale begin',
    await rejected(pendingBegin, 'cancelled') && state.pollWait && state.cancelled.length > 0);
  state.pollWait = false;

  let resolveExchange: ((result: Object) => void) | undefined;
  state.exchangeWait = new Promise<Object>((resolve) => { resolveExchange = resolve; });
  state.cancelExchange = (): void => resolveExchange?.(state.tokenResult);
  const duringExchange = new AccountLoginController();
  await duringExchange.begin('https://mgmt.example', 'ca');
  const pendingFinish = duringExchange.finish('exchange-code');
  duringExchange.cancel();
  check('cancel during token exchange destroys transport and never returns credentials',
    await rejected(pendingFinish, 'cancelled') && state.transportCancels > 0);
  delete (state as unknown as { exchangeWait?: Promise<Object> }).exchangeWait;
  delete (state as unknown as { cancelExchange?: () => void }).cancelExchange;
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) process.exitCode = 1;
