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
  // 2026-10-12: the login-action job (`account_login_*`) — what the controller
  // sends after the token exchange, what it gets back, and what it cancels.
  loginRequests: [], loginPolls: [], loginCancelled: [], loginOutcome: 'success',
  loginFailure: '', loginWait: false,
  // 2026-10-13: the session extend's management step (`account_extend_*`).
  extendRequests: [], extendPolls: [], extendCancelled: [],
  extendFailure: '', extendWait: false,
  extendReply: {
    state: 'done', outcome: 'success',
    session_expiry: 'set', session_expires_at_unix: 1_893_456_000
  },
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
  // 2026-10-13: the discovery request carries exactly three fields — the
  // management URL, the CA and the SessionExtend flag (upstream
  // `GetPKCEAuthorizationFlow(sessionExtend)`, shared/management/client/
  // grpc.go:744-760). The ordinary login/registration flow sends false.
  check('discovery receives only management URL, CA and the login flow flag',
    start.management_url === 'https://mgmt.example' && start.ca_pem.includes('CERTIFICATE') &&
    start.session_extend === false && Object.keys(start).length === 3);
  check('authorize URL has PKCE, exact chosen redirect, query response and login flag mapping',
    prepared.expectedState === 'secure-state' && prepared.redirectUri === 'https://client.example/callback' &&
    prepared.authorizeUrl.includes('response_mode=query') && prepared.authorizeUrl.includes('prompt=login') &&
    prepared.authorizeUrl.includes('scope=openid%20profile') && !prepared.authorizeUrl.includes('offline_access') &&
    !prepared.authorizeUrl.includes('client_secret'));
  let busy = false;
  try { await new AccountLoginController().begin('https://other.example', 'ca'); }
  catch (error) { busy = error instanceof Error && error.message === 'busy'; }
  check('a second controller cannot replace the active native discovery owner', busy);

  // 2026-10-12: `finish` is the WHOLE login action (upstream client/android/
  // client.go:194-215 → client/internal/auth/auth.go:184-236): exchange the
  // code, then hand the JWT to the core's `account_login_*` job together with
  // the connector config the caller built, and return ONLY the management URL.
  const CONFIG = '{"management_url":"https://mgmt.example","private_key":"KEY"}';
  const result = await controller.finish('callback-code', CONFIG);
  check('finish returns the management URL and NOTHING token-shaped',
    result.managementUrl === 'https://mgmt.example' &&
    Object.keys(result).length === 1 &&
    !JSON.stringify(result).includes('access-secret') &&
    !JSON.stringify(result).includes('refresh-secret'));
  check('the login job received the exchanged access JWT and the connector config',
    state.loginRequests.length === 1 &&
    state.loginRequests[0].config === CONFIG && state.loginRequests[0].jwt === 'access-secret');
  check('a successful login job is reported as success and cancels nothing',
    state.loginOutcome === 'success' && state.loginCancelled.length === 0);
  check('duplicate finish is rejected and code is exchanged once',
    await rejected(controller.finish('callback-code', CONFIG), 'exchange_already_used') &&
    state.exchanges.length === 1);

  state.metadata = { ...metadata, disable_prompt_login: true, login_flag: 2, use_id_token: true, scope: 'openid' };
  state.tokenResult.tokens.expiresIn = 0;
  const second = new AccountLoginController();
  const noPrompt = await second.begin('https://mgmt.example', 'ca');
  const idResult = await second.finish('another-code', CONFIG);
  check('disabled prompts stay omitted and use_id_token selects the id JWT for the job',
    !noPrompt.authorizeUrl.includes('prompt=') && !noPrompt.authorizeUrl.includes('max_age=') &&
    state.loginRequests[1].jwt === 'id-secret' && idResult.managementUrl === 'https://mgmt.example');

  // the management server refused the registration: the same 需要登录 verdict the
  // connect path reports, surfaced to the login form as a stable reason.
  state.tokenResult.tokens.expiresIn = 90;
  state.loginOutcome = 'needs-login';
  const refused = new AccountLoginController();
  await refused.begin('https://mgmt.example', 'ca');
  check('a refused registration surfaces the needs-login verdict',
    await rejected(refused.finish('refused-code', CONFIG), 'needs-login'));
  state.loginOutcome = 'success';

  // a transport-level login failure keeps its own stable token
  state.loginFailure = 'network-error';
  const offline = new AccountLoginController();
  await offline.begin('https://mgmt.example', 'ca');
  check('a login-job failure surfaces its own stable token',
    await rejected(offline.finish('offline-code', CONFIG), 'network-error'));
  state.loginFailure = '';

  state.metadata = { ...metadata, login_flag: 1 };
  const maxAge = new AccountLoginController();
  const maxAgePrepared = await maxAge.begin('https://mgmt.example', 'ca');
  maxAge.cancel();
  check('login flag one maps to max_age and prepared cancellation blocks finish',
    maxAgePrepared.authorizeUrl.includes('max_age=0') && !maxAgePrepared.authorizeUrl.includes('prompt=') &&
    await rejected(maxAge.finish('cancelled-code', CONFIG), 'login_not_prepared'));

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
  const pendingFinish = duringExchange.finish('exchange-code', CONFIG);
  duringExchange.cancel();
  check('cancel during token exchange destroys transport and never returns credentials',
    await rejected(pendingFinish, 'cancelled') && state.transportCancels > 0);
  delete (state as unknown as { exchangeWait?: Promise<Object> }).exchangeWait;
  delete (state as unknown as { cancelExchange?: () => void }).cancelExchange;

  // a cancel while the login job is in flight must cancel THAT job too (the
  // token must not outlive the action either way)
  state.loginWait = true;
  const duringJob = new AccountLoginController();
  await duringJob.begin('https://mgmt.example', 'ca');
  const pendingJob = duringJob.finish('job-code', CONFIG);
  await new Promise<void>((resolve) => setTimeout(resolve, 5));
  duringJob.cancel();
  check('cancel during the login job cancels the native job and rejects the finish',
    await rejected(pendingJob, 'cancelled') && state.loginCancelled.length > 0);
  state.loginWait = false;

  // ---------------------------------------------------------------------------
  // 2026-10-13: the interactive SESSION EXTEND (upstream
  // client/android/session.go:127-155 → engine_authsession.go:83-107). Same SSO
  // run, a DIFFERENT management commit: the fresh JWT goes to the core's
  // `account_extend_*` job, and the caller gets the server's own 3-state
  // deadline — no account fact, no config.
  // ---------------------------------------------------------------------------
  const extend = new AccountLoginController();
  const extendPrepared = await extend.begin('https://mgmt.example', 'ca', true);
  const extendStart = state.requests[state.requests.length - 1];
  check('the extend discovery marks SessionExtend and builds the same PKCE request',
    extendStart.session_extend === true && extendStart.management_url === 'https://mgmt.example' &&
    extendPrepared.authorizeUrl.includes('response_mode=query') &&
    extendPrepared.expectedState === 'secure-state');
  const extendResult = await extend.finishExtend('extend-code', CONFIG);
  check('the extend job received the exchanged JWT and the connector config',
    state.extendRequests.length === 1 && state.extendRequests[0].config === CONFIG &&
    state.extendRequests[0].jwt === 'access-secret');
  check('the extend result is the server 3-state and NOTHING token-shaped',
    extendResult.sessionExpiry === 'set' && extendResult.sessionExpiresAtS === 1_893_456_000 &&
    Object.keys(extendResult).length === 2 &&
    !JSON.stringify(extendResult).includes('access-secret') &&
    !JSON.stringify(extendResult).includes('refresh-secret'));
  check('a successful extend is reported as success and cancels nothing',
    state.extendCancelled.length === 0);
  check('duplicate extend is rejected and the code is exchanged once for it',
    await rejected(extend.finishExtend('extend-code', CONFIG), 'exchange_already_used'));
  // `use_id_token` selects the id JWT here too (the SAME rule as the login
  // action — upstream `tokenInfo.GetTokenToUse()` is shared by both).
  state.metadata = { ...metadata, use_id_token: true };
  const idExtend = new AccountLoginController();
  await idExtend.begin('https://mgmt.example', 'ca', true);
  await idExtend.finishExtend('id-extend-code', CONFIG);
  check('the extend sends the id JWT when the provider selects the id token',
    state.extendRequests[1].jwt === 'id-secret');
  state.metadata = metadata;
  // `unknown` (no deadline in the reply) leaves the caller's anchor alone: the
  // result carries no positive deadline.
  state.extendReply = { state: 'done', outcome: 'success', session_expiry: 'unknown' };
  const unknownExtend = new AccountLoginController();
  await unknownExtend.begin('https://mgmt.example', 'ca', true);
  const unknownResult = await unknownExtend.finishExtend('unknown-code', CONFIG);
  check('an unknown deadline is reported as unknown with no timestamp',
    unknownResult.sessionExpiry === 'unknown' && unknownResult.sessionExpiresAtS === 0);
  state.extendReply = {
    state: 'done', outcome: 'success',
    session_expiry: 'set', session_expires_at_unix: 1_893_456_000
  };
  // the management call's own failure keeps its stable class token.
  state.extendFailure = 'management-error';
  const refusedExtend = new AccountLoginController();
  await refusedExtend.begin('https://mgmt.example', 'ca', true);
  check('an extend refusal surfaces its own stable token',
    await rejected(refusedExtend.finishExtend('refused-extend', CONFIG), 'management-error'));
  state.extendFailure = '';
  // a malformed extend reply is refused, never interpreted.
  state.extendReply = { state: 'done', outcome: 'success', session_expiry: 7 };
  const malformedExtend = new AccountLoginController();
  await malformedExtend.begin('https://mgmt.example', 'ca', true);
  check('a malformed extend deadline field is refused',
    await rejected(malformedExtend.finishExtend('malformed-extend', CONFIG), 'extend_failed'));
  state.extendReply = {
    state: 'done', outcome: 'success',
    session_expiry: 'set', session_expires_at_unix: 1_893_456_000
  };
  // a cancel while the extend job is in flight cancels THAT job too.
  state.extendWait = true;
  const duringExtend = new AccountLoginController();
  await duringExtend.begin('https://mgmt.example', 'ca', true);
  const pendingExtend = duringExtend.finishExtend('job-extend', CONFIG);
  await new Promise<void>((resolve) => setTimeout(resolve, 5));
  duringExtend.cancel();
  check('cancel during the extend job cancels the native job and rejects the finish',
    await rejected(pendingExtend, 'cancelled') && state.extendCancelled.length > 0);
  state.extendWait = false;
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) process.exitCode = 1;
