/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
// Host-side checks for the byte-identical account token transport. No network/device.
// Run: node client/entry/selfcheck/run-account-transport.mts
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./account-transport-hooks.mjs', import.meta.url).href);

const here = dirname(fileURLToPath(import.meta.url));
const transportSource = join(here, '..', 'src', 'main', 'ets', 'login', 'NetBirdAccountTransport.ets');
const coreSource = join(here, '..', 'src', 'main', 'ets', 'login', 'NetBirdAccountAuthCore.ets');
const dir = mkdtempSync(join(tmpdir(), 'nb-account-transport-'));
let passed = 0;
let failed = 0;
function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
  if (!condition && detail !== undefined) { console.log(`     ${detail}`); }
}
interface FakeHttpRequestOptions {
  method?: string;
  header?: Object;
  body?: string | Object | ArrayBuffer;
  extraData?: string | Object | ArrayBuffer;
  expectDataType?: number;
  maxLimit?: number;
  connectTimeout?: number;
  readTimeout?: number;
  maxRedirects?: number;
}
interface FakeHttpRequest {
  url: string;
  options: FakeHttpRequestOptions;
  destroyCount: number;
}
interface FakeHttpState {
  requests: FakeHttpRequest[];
  reply?: Object;
  reject?: boolean;
  destroyThrows?: boolean;
}
interface PkceSession {
  state: string;
  verifier: string;
  challenge: string;
  redirectUri: string;
}
interface AccountAuthConfig {
  clientId: string;
  authorizationEndpoint: string;
  tokenEndpoint: string;
  redirectUri: string;
  scopes: string[];
  useIdToken: boolean;
}
const globalState = globalThis as Object as { __NB_HTTP: FakeHttpState };
const state: FakeHttpState = { requests: [], reply: {
  responseCode: 200, result: '{"access_token":"access-1","refresh_token":"refresh-1"}'
} };
globalState.__NB_HTTP = state;
const session: PkceSession = {
  state: 'state-1', verifier: 'A'.repeat(43), challenge: 'B'.repeat(43),
  redirectUri: 'https://app.example/callback'
};
const config: AccountAuthConfig = {
  clientId: 'client-1', authorizationEndpoint: 'https://idp.example/authorize',
  tokenEndpoint: 'https://idp.example/token', redirectUri: session.redirectUri,
  scopes: ['openid'], useIdToken: false
};

try {
  const transportFixture = join(dir, 'NetBirdAccountTransport.ts');
  const coreFixture = join(dir, 'NetBirdAccountAuthCore.ts');
  const transportBytes = readFileSync(transportSource);
  writeFileSync(transportFixture, transportBytes);
  writeFileSync(coreFixture, readFileSync(coreSource));
  check('transport fixture is byte-identical to the shipped .ets',
    readFileSync(transportFixture).equals(transportBytes));
  const { AccountTokenTransport } = await import(pathToFileURL(transportFixture).href);

  const transport = new AccountTokenTransport();
  const success = await transport.exchangeAuthorizationCode(config, session, 'code/1');
  const sent = state.requests[0];
  const opts = sent.options;
  check('POST sends the API 26 body field, not extraData, and never code in URL',
    success.ok && sent.url === config.tokenEndpoint && opts.extraData === undefined &&
    opts.body === 'code=code%2F1&client_id=client-1&redirect_uri=https%3A%2F%2Fapp.example%2Fcallback' +
      `&code_verifier=${'A'.repeat(43)}&grant_type=authorization_code`);
  check('request uses form content type, string response and strict bounds',
    opts.method === 'POST' &&
    (opts.header as Record<string, string>)['Content-Type'] === 'application/x-www-form-urlencoded' &&
    opts.expectDataType === 0 && opts.maxLimit === 65536 &&
    opts.connectTimeout === 15000 && opts.readTimeout === 15000 && opts.maxRedirects === 0);
  check('parsed access/refresh tokens are returned from the core parser',
    success.ok && success.tokens.accessToken === 'access-1' &&
    success.tokens.refreshToken === 'refresh-1');
  check('request is destroyed once after completion', sent.destroyCount === 1);

  const createdBeforeHttpsReject = state.requests.length;
  const httpConfig: AccountAuthConfig = { ...config, tokenEndpoint: 'http://idp.example/token' };
  const rejected = await transport.exchangeAuthorizationCode(httpConfig, session, 'code-2');
  check('non-HTTPS token endpoint is refused before creating an HTTP request',
    rejected.errorCode === 'endpoint_http_forbidden' && state.requests.length === createdBeforeHttpsReject);

  state.reply = { responseCode: 302, result: '<html>secret body</html>' };
  const redirect = await transport.exchangeAuthorizationCode(config, session, 'code-3');
  check('redirect response is an HTTP error; no redirect is followed or body exposed',
    redirect.errorCode === 'http_error' && redirect.errorDetail === 'HTTP 302' &&
    state.requests[state.requests.length - 1].options !== undefined);

  state.reply = { responseCode: 200, result: 'x'.repeat(65537) };
  const oversized = await transport.exchangeAuthorizationCode(config, session, 'code-4');
  check('oversized body is rejected by bounded core parser', oversized.errorCode === 'too_large');

  let rejectLate: ((reason: Error) => void) | undefined;
  state.reject = false;
  state.reply = new Promise<Object>((resolve, reject) => {
    rejectLate = (reason: Error): void => reject(reason);
  });
  const cancellable = new AccountTokenTransport();
  const pending = cancellable.exchangeAuthorizationCode(config, session, 'code-5');
  const cancelRequest = state.requests[state.requests.length - 1];
  let busyRejected = false;
  try {
    await cancellable.exchangeAuthorizationCode(config, session, 'code-6');
  } catch (error) {
    busyRejected = error instanceof Error && error.message === 'busy';
  }
  check('a second concurrent request rejects as busy', busyRejected && state.requests.length === 4);
  state.destroyThrows = true;
  let cancelThrew = false;
  try {
    cancellable.cancel();
  } catch (_error) {
    cancelThrew = true;
  }
  const cancelled = await pending;
  check('destroy failure cannot escape cancel or replace its stable result',
    !cancelThrew && cancelled.errorCode === 'cancelled' && cancelRequest.destroyCount === 1);
  state.destroyThrows = false;
  if (rejectLate !== undefined) {
    rejectLate(new Error('late private platform error'));
  }
  check('a late rejection cannot change the cancellation result',
    cancelled.errorCode === 'cancelled' && cancelled.tokens.accessToken === '');

  state.reply = undefined;
  state.reject = true;
  state.destroyThrows = true;
  const networkFailure = await transport.exchangeAuthorizationCode(config, session, 'code-7');
  check('transport and destroy failures are normalized without exposing platform text',
    networkFailure.errorCode === 'network_error' && networkFailure.errorDetail === '');
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
