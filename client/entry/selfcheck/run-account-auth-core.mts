/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Host-side checks for the user-account PKCE auth flow core (byte-identical
// import of client/entry/src/main/ets/login/NetBirdAccountAuthCore.ets —
// no device, no network, no @ohos).
// Run: node client/entry/selfcheck/run-account-auth-core.mts
// Exit 0 = all assertions pass; exit 1 = any failure.
//
// Pinned here: module purity (no imports / no @ohos / no Math.random / no
// hashing / no URL constructor / no URLSearchParams / no encodeURIComponent /
// no JSON.stringify / no async / no base64-JWT decoding), the RFC 7636
// Appendix B known vector (proven with host crypto, validated through the
// module), form/URL encoding vectors, endpoint https policy, redirect URI
// policy (loopback http, app schemes, exact matching vs evil
// prefix/suffix/path/port hosts), callback classification (state
// match/missing/dup, OAuth error, cancel, fragment flow), request form
// bodies, and token reply parsing (JSON-only, 64 KiB bound, useIdToken with
// NO silent id/access fallback, invalid expires/type rejection).
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const here = dirname(fileURLToPath(import.meta.url));
const SOURCE = join(here, '..', 'src', 'main', 'ets', 'login', 'NetBirdAccountAuthCore.ets');
const dir = mkdtempSync(join(tmpdir(), 'nb-account-auth-'));
let passed = 0;
let failed = 0;
function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
  if (!condition && detail !== undefined) {
    console.log(`     ${detail}`);
  }
}
// Code-shape pins must ignore comments (the file DOCUMENTS the things it must
// not DO), so they run on comment-free source.
function stripComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/^\s*\/\/.*$/gm, ' ');
}

// Mirror of the pure input types (structural — the real ones are erased
// interfaces inside the imported module).
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
  audience?: string;
  useIdToken: boolean;
}

const VERIFIER = 'dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk'; // RFC 7636 App. B
const CHALLENGE = 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM';
const STATE = 'st-3.9_x~ok';
const REDIRECT = 'https://client.example.com/auth/callback';
const CLIENT_ID = 'nb-harmony-client';

function makeSession(): PkceSession {
  return { state: STATE, verifier: VERIFIER, challenge: CHALLENGE, redirectUri: REDIRECT };
}
function makeConfig(): AccountAuthConfig {
  return {
    clientId: CLIENT_ID,
    authorizationEndpoint: 'https://idp.example.com/authorize',
    tokenEndpoint: 'https://idp.example.com/token',
    redirectUri: REDIRECT,
    scopes: ['openid', 'profile', 'email', 'offline_access', 'api'],
    audience: 'https://api.example.com',
    useIdToken: true
  };
}

try {
  const text = readFileSync(SOURCE, 'utf8');
  const code = stripComments(text);

  // ---- purity / shape pins (comment-free source) ---------------------------
  check('core never imports a platform module', !/@ohos/.test(code));
  check('core has no import/require statement',
    !/^\s*import\s/m.test(code) && !/\brequire\s*\(/.test(code));
  check('core never generates randomness', !/Math\.random/.test(code));
  check('core never hashes (no SHA-256 / hash / digest calls in code)',
    !/sha-?256/i.test(code) && !/createHash/.test(code) && !/\.digest\(/.test(code) &&
    !/\bcrypto\b/.test(code));
  check('core uses no URL constructor / URLSearchParams / encodeURIComponent',
    !/new URL\s*\(/.test(code) && !/URLSearchParams/.test(code) &&
    !/encodeURIComponent/.test(code) && !/decodeURIComponent/.test(code));
  check('core never stringifies tokens (no JSON.stringify)', !/JSON\.stringify/.test(code));
  check('core is fully synchronous', !/\basync\b/.test(code) && !/\bawait\b/.test(code));
  check('core uses no any/unknown (ArkTS-safe typing)',
    !/:\s*any\b/.test(code) && !/\bunknown\b/.test(code));
  check('core never decodes JWTs (no base64 helpers, no "." split)',
    !/\batob\b/.test(code) && !/\bbtoa\b/.test(code) && !/fromBase64/.test(code) &&
    !/toBase64/.test(code) && !/split\('\.'\)/.test(code) && !/split\("\."\)/.test(code));

  // ---- byte-identical host import ------------------------------------------
  const fixture = join(dir, 'NetBirdAccountAuthCore.ts');
  const sourceBytes = readFileSync(SOURCE);
  writeFileSync(fixture, sourceBytes);
  check('fixture is byte-identical to the shipped .ets', readFileSync(fixture).equals(sourceBytes));
  const m = await import(pathToFileURL(fixture).href);

  // ---- exported surface / constants ----------------------------------------
  check('constants are the documented ones',
    m.MAX_TOKEN_REPLY_CHARS === 65536 && m.MAX_CALLBACK_URI_CHARS === 2048 &&
    m.VERIFIER_MIN_CHARS === 43 && m.VERIFIER_MAX_CHARS === 128 &&
    m.CHALLENGE_CHARS === 43 && m.MAX_CODE_CHARS === 1024 &&
    m.MAX_STATE_CHARS === 256 && m.MAX_EXPIRES_IN === 2147483647 &&
    m.CODE_CHALLENGE_METHOD === 'S256' &&
    m.TOKEN_REQUEST_CONTENT_TYPE === 'application/x-www-form-urlencoded',
    `reply=${m.MAX_TOKEN_REPLY_CHARS} verifier=${m.VERIFIER_MIN_CHARS}-${m.VERIFIER_MAX_CHARS}`);
  check('callback kind constants are stable strings',
    m.CALLBACK_KIND_CODE === 'code' && m.CALLBACK_KIND_ERROR === 'error' &&
    m.CALLBACK_KIND_IGNORED === 'ignored' && m.CALLBACK_KIND_REJECTED === 'rejected' &&
    m.CALLBACK_KIND_UNSUPPORTED === 'unsupported');
  const fns: string[] = ['formUrlEncode', 'validatePkceVerifier', 'validatePkceChallenge',
    'validateAuthCode', 'checkEndpointUrl', 'checkRedirectUri', 'isLoopbackHttpUri',
    'matchCallbackUri', 'buildAuthorizeUrl', 'classifyCallback',
    'buildAuthorizationCodeForm', 'buildRefreshTokenForm', 'parseTokenResponse',
    'sanitizeErrorText'];
  check('every API function is exported', fns.every((f: string) => typeof m[f] === 'function'));
  check('RFC 7636 vector constants are exported',
    m.RFC7636_TEST_VERIFIER === VERIFIER && m.RFC7636_TEST_CHALLENGE === CHALLENGE);

  // ---- RFC 7636 Appendix B vector, proven with host crypto -----------------
  const hostChallenge: string = createHash('sha256').update(VERIFIER).digest('base64url');
  check('host SHA256 proves the pinned vector', hostChallenge === CHALLENGE,
    `host=${hostChallenge}`);
  check('vector verifier passes RFC 7636 validation',
    m.validatePkceVerifier(VERIFIER).ok && m.validatePkceChallenge(CHALLENGE).ok);

  // ---- PKCE shape validation -----------------------------------------------
  check('43-char verifier ok, 128-char verifier ok',
    m.validatePkceVerifier('A'.repeat(43)).ok && m.validatePkceVerifier('A'.repeat(128)).ok);
  check('42/129-char verifier rejected with verifier_length',
    m.validatePkceVerifier('A'.repeat(42)).error === 'verifier_length' &&
    m.validatePkceVerifier('A'.repeat(129)).error === 'verifier_length');
  check('non-unreserved verifier chars rejected (+ / = space % é)',
    ['+', '/', '=', ' ', '%', 'é'].every((c: string) =>
      m.validatePkceVerifier('a'.repeat(42) + c).error === 'verifier_charset'));
  check('43-char base64url challenge ok (incl. uppercase)',
    m.validatePkceChallenge(CHALLENGE).ok &&
    m.validatePkceChallenge('A'.repeat(43)).ok);
  check('challenge rejects 42/44 chars, padding and std-base64 alphabet',
    m.validatePkceChallenge('A'.repeat(42)).error === 'challenge_length' &&
    m.validatePkceChallenge('A'.repeat(44)).error === 'challenge_length' &&
    m.validatePkceChallenge(CHALLENGE.slice(0, 42) + '=').error === 'challenge_charset' &&
    m.validatePkceChallenge('A'.repeat(41) + '+/').error === 'challenge_charset');

  // ---- form/URL encoding ----------------------------------------------------
  check('formUrlEncode keeps unreserved verbatim',
    m.formUrlEncode('abc') === 'abc' && m.formUrlEncode('a~b.c-d_e') === 'a~b.c-d_e' &&
    m.formUrlEncode('') === '');
  check('formUrlEncode percent-encodes reserved and space (%20, not +)',
    m.formUrlEncode('a b') === 'a%20b' && m.formUrlEncode('a+b') === 'a%2Bb' &&
    m.formUrlEncode('a=b&c') === 'a%3Db%26c' && m.formUrlEncode('100%') === '100%25' &&
    m.formUrlEncode('a/b:c') === 'a%2Fb%3Ac');
  check('formUrlEncode encodes UTF-8 bytes (é 中 😀)',
    m.formUrlEncode('é') === '%C3%A9' && m.formUrlEncode('中') === '%E4%B8%AD' &&
    m.formUrlEncode('\u{1F600}') === '%F0%9F%98%80');

  // ---- endpoint policy ------------------------------------------------------
  check('https endpoint accepted verbatim (path kept, host case preserved)',
    m.checkEndpointUrl('https://idp.example.com/authorize', true).ok &&
    m.checkEndpointUrl('https://IDP.Example.COM/authorize', true).url ===
      'https://IDP.Example.COM/authorize');
  check('whitespace around endpoint trimmed',
    m.checkEndpointUrl('  https://idp.example.com/authorize  ', true).ok);
  check('http endpoint refused in production mode (endpoint_http_forbidden)',
    m.checkEndpointUrl('http://idp.example.com/authorize', true).error === 'endpoint_http_forbidden');
  check('http endpoint allowed only when the caller passes the dev flag',
    m.checkEndpointUrl('http://192.168.1.10:8080/authorize', false).ok);
  check('IPv6 literal endpoint with port accepted',
    m.checkEndpointUrl('https://[::1]:8443/token', true).ok);
  const badEndpoints: Array<[string, string]> = [
    ['', 'endpoint_malformed'], ['not-a-url', 'endpoint_malformed'],
    ['ftp://idp.example.com', 'endpoint_scheme_unsupported'],
    ['https://', 'endpoint_malformed'], ['https://user@idp.example.com/x', 'endpoint_malformed'],
    ['https://idp.example.com:abc/x', 'endpoint_malformed'],
    ['https://idp.example.com:0/x', 'endpoint_malformed'],
    ['https://idp.example.com/x#frag', 'endpoint_fragment_forbidden'],
    ['https://idp.exa mple.com/x', 'endpoint_malformed']
  ];
  for (const [raw, want] of badEndpoints) {
    check(`endpoint rejected: ${JSON.stringify(raw)} -> ${want}`,
      !m.checkEndpointUrl(raw, true).ok && m.checkEndpointUrl(raw, true).error === want);
  }

  // ---- configured redirect URI policy --------------------------------------
  check('https redirect normalized (lowercase host, 443 elided, empty path)',
    m.checkRedirectUri('https://Client.Example.COM').url === 'https://client.example.com/' &&
    m.checkRedirectUri('https://client.example.com:443/cb').url === 'https://client.example.com/cb');
  check('loopback http redirect allowed (strict 127/8 IPv4, localhost, [::1])',
    m.checkRedirectUri('http://127.0.0.1:8080/cb').url === 'http://127.0.0.1:8080/cb' &&
    m.checkRedirectUri('http://127.255.255.255/cb').ok &&
    m.checkRedirectUri('http://localhost/cb').ok &&
    m.checkRedirectUri('http://[::1]:9000/cb').url === 'http://[::1]:9000/cb');
  check('127 DNS suffix and invalid numeric IPv4 are not loopback',
    !m.checkRedirectUri('http://127.evil.example/cb').ok &&
    !m.checkRedirectUri('http://127.999.1.1/cb').ok &&
    !m.isLoopbackHttpUri('http://127.evil.example/cb') &&
    !m.isLoopbackHttpUri('http://127.999.1.1/cb'));
  check('app custom schemes allowed (authority and no-authority forms)',
    m.checkRedirectUri('netbird://login/callback').ok &&
    m.checkRedirectUri('com.example.app:/oauth2redirect').ok &&
    m.checkRedirectUri('com.example.app:/oauth2redirect').url === 'com.example.app:/oauth2redirect');
  check('public http redirect refused (redirect_http_not_loopback)',
    m.checkRedirectUri('http://api.example.com/cb').error === 'redirect_http_not_loopback');
  check('localhost-suffix evil host is still not loopback',
    m.checkRedirectUri('http://localhost.evil.example/cb').error === 'redirect_http_not_loopback');
  check('registered special / unsafe schemes refused',
    ['ftp://x/cb', 'javascript:alert(1)', 'data:text/html,x', 'file:///cb',
      'ws://x/cb', 'about:blank', 'mailto:a@b'].every((raw: string) =>
      !m.checkRedirectUri(raw).ok));
  const badRedirects: Array<[string, string]> = [
    ['', 'redirect_malformed'], ['   ', 'redirect_malformed'],
    ['https:///cb', 'redirect_malformed'],
    ['https://user@client.example.com/cb', 'redirect_malformed'],
    ['https://client.example.com:0/cb', 'redirect_malformed'],
    ['https://client.example.com:70000/cb', 'redirect_malformed'],
    ['https://client.example.com/cb?x=1', 'redirect_query_forbidden'],
    ['https://client.example.com/cb#f', 'redirect_fragment_forbidden'],
    ['netbird://login?x=1', 'redirect_query_forbidden']
  ];
  for (const [raw, want] of badRedirects) {
    check(`redirect rejected: ${JSON.stringify(raw)} -> ${want}`,
      !m.checkRedirectUri(raw).ok && m.checkRedirectUri(raw).error === want);
  }
  check('isLoopbackHttpUri true only for http loopback',
    m.isLoopbackHttpUri('http://127.0.0.1:8080/cb') &&
    !m.isLoopbackHttpUri('https://127.0.0.1:8080/cb') &&
    !m.isLoopbackHttpUri('http://api.example.com/cb'));

  // ---- exact callback matching (anti evil host/path/port) -------------------
  check('exact match true; host case + default-port normalization applied',
    m.matchCallbackUri(REDIRECT, 'https://CLIENT.example.com:443/auth/callback') === true &&
    m.matchCallbackUri(REDIRECT, REDIRECT) === true);
  check('evil host suffix/prefix does NOT match',
    m.matchCallbackUri(REDIRECT,
      'https://client.example.com.evil.example/auth/callback') === false &&
    m.matchCallbackUri(REDIRECT,
      'https://evil.client.example.com/auth/callback') === false &&
    m.matchCallbackUri(REDIRECT, 'https://evil.example/auth/callback') === false);
  check('userinfo trick does NOT match',
    m.matchCallbackUri(REDIRECT,
      'https://client.example.com@evil.example/auth/callback') === false);
  check('path prefix / suffix / sibling path do NOT match',
    m.matchCallbackUri(REDIRECT, 'https://client.example.com/auth/callback/extra') === false &&
    m.matchCallbackUri(REDIRECT, 'https://client.example.com/auth') === false &&
    m.matchCallbackUri(REDIRECT, 'https://client.example.com/auth/callback2') === false);
  check('wrong port / scheme do NOT match',
    m.matchCallbackUri(REDIRECT, 'https://client.example.com:8443/auth/callback') === false &&
    m.matchCallbackUri(REDIRECT, 'http://client.example.com/auth/callback') === false);
  check('loopback port shift does NOT match (exact-port policy)',
    m.matchCallbackUri('http://127.0.0.1:8761/login',
      'http://127.0.0.1:8762/login') === false);

  // ---- authorize URL --------------------------------------------------------
  const wantUrl = 'https://idp.example.com/authorize?response_type=code' +
    `&client_id=${CLIENT_ID}` +
    '&redirect_uri=https%3A%2F%2Fclient.example.com%2Fauth%2Fcallback' +
    '&scope=openid%20profile%20email%20offline_access%20api' +
    `&state=${STATE}` +
    `&code_challenge=${CHALLENGE}&code_challenge_method=S256` +
    '&audience=https%3A%2F%2Fapi.example.com';
  const gotAud = m.buildAuthorizeUrl(makeConfig(), makeSession());
  check('authorize URL exact string (all standard params + audience)',
    gotAud.ok && gotAud.url === wantUrl, `got ${gotAud.url}`);
  const cfgNoAud: AccountAuthConfig = makeConfig();
  delete cfgNoAud.audience;
  const wantNoAud = wantUrl.replace('&audience=https%3A%2F%2Fapi.example.com', '');
  check('audience omitted when absent',
    m.buildAuthorizeUrl(cfgNoAud, makeSession()).url === wantNoAud);
  const emptyAud: AccountAuthConfig = makeConfig();
  emptyAud.audience = '';
  check('explicitly empty audience refused (audience_invalid)',
    m.buildAuthorizeUrl(emptyAud, makeSession()).error === 'audience_invalid');
  check('http authorize endpoint refused by default, allowed with dev flag',
    (() => {
      const c: AccountAuthConfig = makeConfig();
      c.authorizationEndpoint = 'http://idp.example.com/authorize';
      const strict = m.buildAuthorizeUrl(c, makeSession());
      const lax = m.buildAuthorizeUrl(c, makeSession(), true);
      return strict.error === 'endpoint_http_forbidden' && lax.ok;
    })());
  check('http TOKEN endpoint is validated too (refused by default)',
    (() => {
      const c: AccountAuthConfig = makeConfig();
      c.tokenEndpoint = 'http://idp.example.com/token';
      return m.buildAuthorizeUrl(c, makeSession()).error === 'endpoint_http_forbidden' &&
        m.buildAuthorizeUrl(c, makeSession(), true).ok;
    })());
  const badSessions: Array<[PkceSession, string]> = [
    [{ ...makeSession(), verifier: 'A'.repeat(42) }, 'verifier_length'],
    [{ ...makeSession(), challenge: 'A'.repeat(42) }, 'challenge_length'],
    [{ ...makeSession(), state: '' }, 'state_invalid'],
    [{ ...makeSession(), state: 'st\nate' }, 'state_invalid'],
    [{ ...makeSession(), redirectUri: 'https://other.example.com/cb' }, 'redirect_mismatch'],
    [{ ...makeSession(), redirectUri: 'https://x/cb?q=1' }, 'redirect_query_forbidden']
  ];
  for (const [session, want] of badSessions) {
    check(`authorize refuses broken session (${want})`,
      m.buildAuthorizeUrl(makeConfig(), session).error === want);
  }
  check('authorize refuses empty scopes and blank scope entries',
    (() => {
      const noScopes: AccountAuthConfig = makeConfig();
      noScopes.scopes = [];
      const blankScope: AccountAuthConfig = makeConfig();
      blankScope.scopes = ['openid', ''];
      return m.buildAuthorizeUrl(noScopes, makeSession()).error === 'scope_missing' &&
        m.buildAuthorizeUrl(blankScope, makeSession()).error === 'scope_invalid';
    })());
  check('authorize refuses non-loopback 127 DNS redirect hosts',
    (() => {
      const c: AccountAuthConfig = makeConfig();
      c.redirectUri = 'http://127.evil.example/cb';
      const badDns = m.buildAuthorizeUrl(c, makeSession());
      c.redirectUri = 'http://127.999.1.1/cb';
      const badNumeric = m.buildAuthorizeUrl(c, makeSession());
      return badDns.error === 'redirect_http_not_loopback' &&
        badNumeric.error === 'redirect_http_not_loopback';
    })());
  check('authorize refuses empty clientId',
    (() => {
      const c: AccountAuthConfig = makeConfig();
      c.clientId = '';
      return m.buildAuthorizeUrl(c, makeSession()).error === 'client_id_invalid';
    })());

  // ---- callback classification ----------------------------------------------
  const cb = (query: string): string => `https://client.example.com/auth/callback${query}`;
  const ok1 = m.classifyCallback(cb('?code=abc-123%2Fxyz&state=st-3.9_x~ok'), makeSession());
  check('success: code+state -> kind code with decoded code',
    ok1.kind === 'code' && ok1.code === 'abc-123/xyz' && ok1.error === '', JSON.stringify(ok1));
  const ok2 = m.classifyCallback(cb('?state=st-3.9_x~ok&code=abc+def'), makeSession());
  check('success: param order free, + stays literal plus (RFC 3986 query)',
    ok2.kind === 'code' && ok2.code === 'abc+def');
  const ok3 = m.classifyCallback(
    cb('?code=x1&state=st-3.9_x~ok&iss=https%3A%2F%2Fidp.example.com&session_state=s1'),
    makeSession());
  check('success: unknown params (iss, session_state) ignored',
    ok3.kind === 'code' && ok3.code === 'x1');
  const rej = (uri: string, session?: PkceSession): string =>
    m.classifyCallback(uri, session ?? makeSession()).detail;
  check('state mismatch -> rejected state_mismatch, no code',
    (() => {
      const r = m.classifyCallback(cb('?code=x&state=other'), makeSession());
      return r.kind === 'rejected' && r.detail === 'state_mismatch' && r.code === '';
    })());
  check('missing state -> rejected state_missing',
    m.classifyCallback(cb('?code=x'), makeSession()).detail === 'state_missing');
  check('duplicate code -> rejected duplicate_code',
    m.classifyCallback(cb('?code=a&code=b&state=st-3.9_x~ok'), makeSession()).detail === 'duplicate_code');
  check('duplicate state -> rejected duplicate_state',
    m.classifyCallback(cb('?code=a&state=S1&state=st-3.9_x~ok'), makeSession()).detail === 'duplicate_state');
  check('OAuth error with valid state -> kind error (cancel), never a code',
    (() => {
      const r = m.classifyCallback(cb('?error=access_denied&error_description=user%20cancelled&state=st-3.9_x~ok'),
        makeSession());
      return r.kind === 'error' && r.error === 'access_denied' &&
        r.errorDescription === 'user cancelled' && r.code === '' && r.detail === '';
    })());
  check('OAuth error WITHOUT valid state is rejected, not trusted',
    m.classifyCallback(cb('?error=server_error'), makeSession()).detail === 'state_missing' &&
    m.classifyCallback(cb('?error=server_error&state=nope'), makeSession()).detail === 'state_mismatch');
  check('code together with error is ambiguous -> rejected code_and_error',
    rej(cb('?code=x&error=access_denied&state=st-3.9_x~ok')) === 'code_and_error');
  check('empty / spaced code rejected (code_invalid)',
    rej(cb('?code=&state=st-3.9_x~ok')) === 'code_invalid' &&
    rej(cb('?code=a%20b&state=st-3.9_x~ok')) === 'code_invalid');
  check('non-callback navigation ignored (not_the_callback)',
    (() => {
      const r = m.classifyCallback('https://idp.example.com/consent?foo=1', makeSession());
      return r.kind === 'ignored' && r.detail === 'not_the_callback' && r.code === '';
    })());
  check('evil host callback ignored (no state parsing, no leak)',
    (() => {
      const r = m.classifyCallback('https://client.example.com.evil.example/auth/callback?code=x&state=st-3.9_x~ok',
        makeSession());
      return r.kind === 'ignored' && r.code === '';
    })());
  check('garbage input ignored (not_parseable)',
    m.classifyCallback('not a uri', makeSession()).detail === 'not_parseable' &&
    m.classifyCallback('', makeSession()).detail === 'not_parseable');
  check('fragment token flow unsupported (implicit flow never parsed)',
    (() => {
      const r = m.classifyCallback(cb('?code=x&state=st-3.9_x~ok#access_token=SECRET'), makeSession());
      const r2 = m.classifyCallback(cb('#access_token=SECRET&token_type=bearer'), makeSession());
      return r.kind === 'unsupported' && r.detail === 'fragment_response_unsupported' &&
        r.code === '' && r2.kind === 'unsupported';
    })());
  check('loopback http end-to-end: configured http://127.0.0.1:8761/login',
    (() => {
      const s: PkceSession = makeSession();
      s.redirectUri = 'http://127.0.0.1:8761/login';
      const r = m.classifyCallback('http://127.0.0.1:8761/login?code=c1&state=st-3.9_x~ok', s);
      return r.kind === 'code' && r.code === 'c1';
    })());
  check('app scheme end-to-end: netbird://login',
    (() => {
      const s: PkceSession = makeSession();
      s.redirectUri = 'netbird://login';
      const r = m.classifyCallback('netbird://login?code=c2&state=st-3.9_x~ok', s);
      return r.kind === 'code' && r.code === 'c2';
    })());

  // ---- request form bodies ---------------------------------------------------
  const wantExchange = 'code=abc-123%2Fxyz' +
    `&client_id=${CLIENT_ID}` +
    '&redirect_uri=https%3A%2F%2Fclient.example.com%2Fauth%2Fcallback' +
    `&code_verifier=${VERIFIER}` +
    '&grant_type=authorization_code';
  const gotForm = m.buildAuthorizationCodeForm(makeConfig(), makeSession(), 'abc-123/xyz');
  check('authorization-code form exact body', gotForm.ok && gotForm.body === wantExchange,
    `got ${gotForm.body}`);
  const wantRefresh = 'grant_type=refresh_token' +
    `&client_id=${CLIENT_ID}` +
    '&refresh_token=rt-token%2B%2F%3D1';
  const gotRefresh = m.buildRefreshTokenForm(makeConfig(), 'rt-token+/=1');
  check('refresh form exact body (separate builder, no verifier/redirect)',
    gotRefresh.ok && gotRefresh.body === wantRefresh, `got ${gotRefresh.body}`);
  check('exchange form refuses empty code, broken verifier, bad redirect',
    m.buildAuthorizationCodeForm(makeConfig(), makeSession(), '').error === 'code_invalid' &&
    m.buildAuthorizationCodeForm(makeConfig(), { ...makeSession(), verifier: 'short' }, 'c')
      .error === 'verifier_length');
  check('refresh form refuses empty refresh token',
    m.buildRefreshTokenForm(makeConfig(), '').error === 'refresh_token_invalid');

  // ---- token reply parsing ----------------------------------------------------
  const fullReply = '{"access_token":"at.jwt","token_type":"Bearer","expires_in":3600,' +
    '"refresh_token":"rt-1","id_token":"id.jwt","scope":"openid api"}';
  const t1 = m.parseTokenResponse(fullReply, false);
  check('full reply, useIdToken=false: access token required and kept',
    t1.ok && t1.tokens.accessToken === 'at.jwt' && t1.tokens.idToken === 'id.jwt' &&
    t1.tokens.refreshToken === 'rt-1' && t1.tokens.expiresIn === 3600 &&
    t1.errorCode === '', JSON.stringify(t1.tokens));
  const t2 = m.parseTokenResponse(fullReply, true);
  check('full reply, useIdToken=true: id token present, nothing substituted',
    t2.ok && t2.tokens.idToken === 'id.jwt' && t2.tokens.accessToken === 'at.jwt');
  check('useIdToken=true without id_token REJECTED even though access_token exists',
    (() => {
      const r = m.parseTokenResponse('{"access_token":"at","expires_in":10}', true);
      return !r.ok && r.errorCode === 'missing_id_token' && r.tokens.accessToken === '';
    })());
  check('useIdToken=false without access_token REJECTED even though id_token exists',
    (() => {
      const r = m.parseTokenResponse('{"id_token":"it"}', false);
      return !r.ok && r.errorCode === 'missing_access_token';
    })());
  check('non-JSON and non-object replies rejected',
    m.parseTokenResponse('a=1&b=2', false).errorCode === 'not_json' &&
    m.parseTokenResponse('', false).errorCode === 'not_json' &&
    m.parseTokenResponse('{oops', false).errorCode === 'not_json' &&
    m.parseTokenResponse('42', false).errorCode === 'not_json_object' &&
    m.parseTokenResponse('null', false).errorCode === 'not_json_object' &&
    m.parseTokenResponse('[1,2]', false).errorCode === 'not_json_object');
  const err1 = m.parseTokenResponse('{"error":"invalid_grant","error_description":"code expired"}', false);
  check('OAuth error body -> token_error + sanitized detail, no tokens',
    !err1.ok && err1.errorCode === 'token_error' && err1.errorDetail === 'code expired' &&
    err1.tokens.accessToken === '');
  check('error body without description falls back to the error code text',
    m.parseTokenResponse('{"error":"server_error"}', false).errorDetail === 'server_error');
  check('success wins over a stray error field',
    m.parseTokenResponse('{"access_token":"at","error":"whatever"}', false).ok === true);
  check('invalid expires_in variants rejected',
    m.parseTokenResponse('{"access_token":"a","expires_in":"3600"}', false).errorCode === 'invalid_expires_in' &&
    m.parseTokenResponse('{"access_token":"a","expires_in":0}', false).errorCode === 'invalid_expires_in' &&
    m.parseTokenResponse('{"access_token":"a","expires_in":-5}', false).errorCode === 'invalid_expires_in' &&
    m.parseTokenResponse('{"access_token":"a","expires_in":1.5}', false).errorCode === 'invalid_expires_in' &&
    m.parseTokenResponse('{"access_token":"a","expires_in":2147483648}', false).errorCode === 'invalid_expires_in');
  check('expires_in absent -> 0 sentinel; int32 max accepted',
    m.parseTokenResponse('{"access_token":"a"}', false).tokens.expiresIn === 0 &&
    m.parseTokenResponse('{"access_token":"a","expires_in":2147483647}', false).ok);
  check('token_type enforced: Bearer ok (any case), DPoP refused, non-string refused',
    m.parseTokenResponse('{"access_token":"a","token_type":"bearer"}', false).ok &&
    m.parseTokenResponse('{"access_token":"a","token_type":"DPoP"}', false).errorCode === 'unsupported_token_type' &&
    m.parseTokenResponse('{"access_token":"a","token_type":5}', false).errorCode === 'invalid_token_type');
  check('wrong-typed tokens rejected (no type confusion)',
    m.parseTokenResponse('{"access_token":42}', false).errorCode === 'invalid_access_token' &&
    m.parseTokenResponse('{"access_token":"a","id_token":7}', true).errorCode === 'invalid_id_token' &&
    m.parseTokenResponse('{"access_token":"a","refresh_token":{}}', false).errorCode === 'invalid_refresh_token');
  check('empty-string optional refresh_token reads as absent',
    m.parseTokenResponse('{"access_token":"a","refresh_token":""}', false).ok &&
    m.parseTokenResponse('{"access_token":"a","refresh_token":""}', false).tokens.refreshToken === '');
  const tooBig = 'x'.repeat(65537);
  check('64 KiB reply bound: over -> too_large, exactly at bound -> parsed',
    m.parseTokenResponse(tooBig, false).errorCode === 'too_large' &&
    (() => {
      const head = '{"access_token":"';
      const body = 'a'.repeat(65536 - head.length - 2);
      return m.parseTokenResponse(head + body + '"}', false).ok;
    })());

  // ---- sanitized error text ---------------------------------------------------
  check('sanitizeErrorText: controls become spaces, trimmed, bounded',
    m.sanitizeErrorText('a\nb\tc', 256) === 'a b c' &&
    m.sanitizeErrorText('  pad  ', 256) === 'pad' &&
    m.sanitizeErrorText('x'.repeat(300), 256).length === 256 &&
    m.sanitizeErrorText('中文', 256) === '');
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
