/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// NetBirdAccountAuthCore — PURE LOGIC for the user-account OAuth 2.0 login:
// RFC 6749 authorization-code grant + RFC 7636 PKCE, driven through a
// WebView. No @ohos / platform imports: the host-side selfcheck imports
// this file byte-identical (client/entry/selfcheck/run-account-auth-core.mts),
// so everything here stays plain erasable TypeScript — no enums, no
// decorators, no imports, no async, no Math.random, no hashing, and no
// module-level mutable state (pure small functions only).
//
// WHY THIS EXISTS: the app gains a NEW account-login mode (a user account
// on a NetBird deployment) NEXT TO the existing setup-key mode, which stays
// untouched (setup/NetBirdSetupConfig.ets + pages/Index.ets keep the
// setup-key path). Endpoint discovery for the account flow is a separate
// later step; this module therefore takes every server address as a generic
// parameter and hardcodes NO server URLs.
//
// SPLIT OF DUTIES (the platform adapter = the later UI/WebView code):
//   - adapter: generate `state` and `code_verifier` from a CRYPTOGRAPHICALLY
//     SECURE random source and derive
//     code_challenge = BASE64URL(SHA256(verifier)) (no '=' padding).
//     This module NEVER generates randomness and NEVER hashes — it only
//     VALIDATES the RFC 7636 shapes (§4.1/§4.2) and the selfcheck pins the
//     RFC 7636 Appendix B known vector for the adapter to test against.
//   - adapter: navigate the WebView to the built authorize URL, observe
//     redirect navigations, and POST the built form bodies. This module
//     builds request bytes; the adapter owns the TLS connection.
//   - this module: endpoint scheme policy, redirect/callback exact
//     matching, callback classification (code / OAuth error / ignore /
//     reject / unsupported), form bodies, token reply parsing.
//
// SECURITY POSTURE (every rule below is pinned by the selfcheck):
//   - authorize/token endpoints MUST be https. The only relaxation is an
//     explicit dev-build flag (allowInsecureEndpoints=true) passed by the
//     CALLER as a build constant — never derived from user-editable config.
//   - redirect URIs: https anywhere; http ONLY for loopback hosts
//     (127.0.0.0/8, localhost, [::1]) per RFC 8252 §7.1; private-use app
//     schemes (netbird://...) per RFC 8252 §7.1 too. Callbacks are matched
//     on the EXACT normalized (scheme, host, port, path) of the configured
//     URI — never by prefix, suffix or substring.
//   - `state` is mandatory and must equal THIS session's state (exact
//     string) on success AND on OAuth error responses; anything else is
//     rejected and never exchanged. Missing or duplicate code/state params
//     are rejected as ambiguous.
//   - the implicit flow (tokens in the URI fragment) is UNSUPPORTED and
//     reported as such; fragments are never parsed for tokens.
//   - token replies are parsed as JSON ONLY (form-encoded or HTML replies
//     are rejected), bounded at MAX_TOKEN_REPLY_CHARS before parsing. The
//     projection honors useIdToken and NEVER silently substitutes id_token
//     for access_token or vice versa. JWTs stay opaque — never decoded:
//     a token is trusted only because it arrived over TLS from the token
//     endpoint named by config, for no other reason.
//   - SECRETS: the authorization code, the code_verifier and every token
//     are secret. This module performs NO logging and returns only stable,
//     bounded error codes (plus sanitized server error text for display) —
//     never a code, verifier or token value.
//
// ERROR STRINGS are stable ASCII codes for the UI to map to readable copy:
//   endpoint_* / redirect_* / scope_* / client_id_* / state_* / verifier_* /
//   challenge_* / code_* / refresh_token_* / duplicate_code /
//   duplicate_state / duplicate_error / state_missing / state_mismatch /
//   code_missing / code_and_error / fragment_response_unsupported /
//   not_parseable / not_the_callback / session_invalid / too_large /
//   not_json / not_json_object / token_error / missing_access_token /
//   missing_id_token / invalid_access_token / invalid_id_token /
//   invalid_refresh_token / invalid_token_type / unsupported_token_type /
//   invalid_expires_in.

// ---- bounds ---------------------------------------------------------------

// A callback/redirect URI longer than this is not ours / malformed.
export const MAX_CALLBACK_URI_CHARS: number = 2048;
// An authorize/token endpoint URL longer than this is rejected outright.
export const MAX_ENDPOINT_CHARS: number = 2048;
// Token endpoint replies are only parsed below this size (64 KiB — far
// above any real token bundle, far below any DoS-sized body).
export const MAX_TOKEN_REPLY_CHARS: number = 65536;
// RFC 7636 §4.1: code_verifier length is 43..128 unreserved characters.
export const VERIFIER_MIN_CHARS: number = 43;
export const VERIFIER_MAX_CHARS: number = 128;
// RFC 7636 §4.2 with S256: BASE64URL(SHA256(...)) is exactly 43 chars.
export const CHALLENGE_CHARS: number = 43;
// The authorization `code` the server round-trips: bounded opaque input.
export const MAX_CODE_CHARS: number = 1024;
// Outgoing secret form fields (refresh_token): bounded opaque input.
export const MAX_SECRET_FIELD_CHARS: number = 8192;
// Session state: adapter-generated; bounded opaque round-trip value.
export const MAX_STATE_CHARS: number = 256;
export const MAX_CLIENT_ID_CHARS: number = 256;
export const MAX_SCOPE_LIST_CHARS: number = 1024;
// Sanitized server error text kept for display.
export const MAX_ERROR_DETAIL_CHARS: number = 256;
// expires_in must be a positive JSON integer that fits int32 (napi boundary).
export const MAX_EXPIRES_IN: number = 2147483647;

export const CODE_CHALLENGE_METHOD: string = 'S256';
export const TOKEN_REQUEST_CONTENT_TYPE: string = 'application/x-www-form-urlencoded';

// RFC 7636 Appendix B known vector — the adapter MUST reproduce this pair
// on-device (secure random + BASE64URL(SHA256)). The selfcheck proves the
// pair with host crypto and pins both shapes through this module.
export const RFC7636_TEST_VERIFIER: string = 'dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk';
export const RFC7636_TEST_CHALLENGE: string = 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM';

// ---- input types ----------------------------------------------------------

// Generic standard OAuth/OIDC client configuration. Filled by the endpoint
// discovery step (later work); this module hardcodes nothing.
export interface AccountAuthConfig {
  clientId: string;
  authorizationEndpoint: string;
  tokenEndpoint: string;
  redirectUri: string;
  scopes: string[];
  audience?: string;
  useIdToken: boolean;
}

// One login attempt's PKCE material, produced by the platform adapter
// (secure random + SHA256). state/redirectUri define THIS attempt only.
export interface PkceSession {
  state: string;
  verifier: string;
  challenge: string;
  redirectUri: string;
}

export interface FieldCheck {
  ok: boolean;
  error: string; // stable ASCII code ('' when ok)
}

export interface UrlCheck {
  ok: boolean;
  error: string;         // stable ASCII code ('' when ok)
  url: string;           // validated/normalized url ('' when !ok)
}

export interface AuthorizeUrlResult {
  ok: boolean;
  url: string;           // the URL to navigate to ('' when !ok)
  error: string;         // stable ASCII code ('' when ok)
}

export interface FormResult {
  ok: boolean;
  body: string;          // application/x-www-form-urlencoded body ('' when !ok)
  error: string;         // stable ASCII code ('' when ok)
}

// Callback classification. Action table for the adapter:
//   'code'       -> exchange (buildAuthorizationCodeForm + POST)
//   'error'      -> user-visible OAuth error / cancel; NEVER exchange
//   'ignored'    -> not our callback (e.g. intermediate page); keep waiting
//   'rejected'   -> abort this attempt; NEVER exchange (state/param problem)
//   'unsupported'-> abort (wrong flow, e.g. fragment tokens); NEVER exchange
export const CALLBACK_KIND_CODE: string = 'code';
export const CALLBACK_KIND_ERROR: string = 'error';
export const CALLBACK_KIND_IGNORED: string = 'ignored';
export const CALLBACK_KIND_REJECTED: string = 'rejected';
export const CALLBACK_KIND_UNSUPPORTED: string = 'unsupported';

export interface CallbackResult {
  kind: string;            // one of CALLBACK_KIND_*
  code: string;            // authorization code, ONLY on kind 'code'
  error: string;           // sanitized OAuth error code, ONLY on kind 'error'
  errorDescription: string; // sanitized server text, ONLY on kind 'error'
  detail: string;          // stable machine reason on ignored/rejected/unsupported
}

// Token projection. '' / 0 are the documented "absent" sentinels.
export interface AuthTokens {
  accessToken: string;
  idToken: string;
  refreshToken: string;
  expiresIn: number; // 0 = not provided; otherwise 1..MAX_EXPIRES_IN
}

export interface TokenParseResult {
  ok: boolean;
  errorCode: string;     // stable ASCII code ('' when ok)
  errorDetail: string;   // bounded sanitized server text (token_error only)
  tokens: AuthTokens;    // zero value when !ok
}

// ---- tiny encoding / decoding helpers (no URL/URLSearchParams needed) -----

const HEX_CHARS: string = '0123456789ABCDEF';

function hexDigitVal(charCode: number): number {
  if (charCode >= 48 && charCode <= 57) { return charCode - 48; }      // 0-9
  if (charCode >= 65 && charCode <= 70) { return charCode - 55; }      // A-F
  if (charCode >= 97 && charCode <= 102) { return charCode - 87; }     // a-f
  return -1;
}

// RFC 3986 §2.3 unreserved characters (by code point).
function isUnreservedCp(cp: number): boolean {
  if (cp > 0x7F) { return false; }
  return (cp >= 0x41 && cp <= 0x5A) || (cp >= 0x61 && cp <= 0x7A) ||
    (cp >= 0x30 && cp <= 0x39) ||
    cp === 0x2D || cp === 0x2E || cp === 0x5F || cp === 0x7E;
}

// Percent-encode one code point's UTF-8 bytes (uppercase hex).
function utf8PercentEncodeCp(cp: number): string {
  const bytes: number[] = [];
  if (cp <= 0x7F) {
    bytes.push(cp);
  } else if (cp <= 0x7FF) {
    bytes.push(0xC0 | (cp >> 6), 0x80 | (cp & 0x3F));
  } else if (cp <= 0xFFFF) {
    bytes.push(0xE0 | (cp >> 12), 0x80 | ((cp >> 6) & 0x3F), 0x80 | (cp & 0x3F));
  } else {
    bytes.push(0xF0 | (cp >> 18), 0x80 | ((cp >> 12) & 0x3F),
      0x80 | ((cp >> 6) & 0x3F), 0x80 | (cp & 0x3F));
  }
  let out: string = '';
  for (const b of bytes) {
    out += '%' + HEX_CHARS[b >> 4] + HEX_CHARS[b & 15];
  }
  return out;
}

// Safe URL encoding for every parameter this module emits: percent-encodes
// each UTF-8 byte that is not RFC 3986-unreserved (space becomes %20).
// Self-contained on purpose — no URL constructor, no URLSearchParams, no
// encodeURIComponent: identical behavior on host and device.
export function formUrlEncode(value: string): string {
  let out: string = '';
  let i: number = 0;
  while (i < value.length) {
    const codePoint: number | undefined = value.codePointAt(i);
    if (codePoint === undefined) {
      return out;
    }
    const cp: number = codePoint;
    if (isUnreservedCp(cp)) {
      out += String.fromCharCode(cp);
    } else {
      out += utf8PercentEncodeCp(cp);
    }
    i += cp > 0xFFFF ? 2 : 1;
  }
  return out;
}

// Decode UTF-8 bytes to text; invalid sequences become U+FFFD.
function decodeUtf8Bytes(bytes: number[]): string {
  let out: string = '';
  let i: number = 0;
  while (i < bytes.length) {
    const b0: number = bytes[i];
    let len: number = 0;
    let cp: number = 0;
    if (b0 < 0x80) {
      out += String.fromCharCode(b0);
      i += 1;
      continue;
    } else if (b0 >= 0xC2 && b0 <= 0xDF) {
      len = 2; cp = b0 & 0x1F;
    } else if (b0 >= 0xE0 && b0 <= 0xEF) {
      len = 3; cp = b0 & 0x0F;
    } else if (b0 >= 0xF0 && b0 <= 0xF4) {
      len = 4; cp = b0 & 0x07;
    } else {
      out += '\uFFFD';
      i += 1;
      continue;
    }
    let ok: boolean = i + len <= bytes.length;
    for (let k: number = 1; ok && k < len; k++) {
      const b: number = bytes[i + k];
      if (b < 0x80 || b > 0xBF) { ok = false; break; }
      cp = (cp << 6) | (b & 0x3F);
    }
    const overlong: boolean = (len === 2 && cp < 0x80) || (len === 3 && cp < 0x800) ||
      (len === 4 && cp < 0x10000);
    if (!ok || overlong || cp > 0x10FFFF || (len === 3 && cp >= 0xD800 && cp <= 0xDFFF)) {
      out += '\uFFFD';
      i += 1;
      continue;
    }
    if (cp > 0xFFFF) {
      const v: number = cp - 0x10000;
      out += String.fromCharCode(0xD800 + (v >> 10), 0xDC00 + (v & 0x3FF));
    } else {
      out += String.fromCharCode(cp);
    }
    i += len;
  }
  return out;
}

// Decode one query component per RFC 3986: %XX sequences decode as UTF-8,
// malformed escapes stay literal. '+' stays a LITERAL plus (this is a URI
// query, not an HTML form body — servers percent-encode real spaces), so a
// code value can never silently mutate into something containing a space.
function decodeQueryComponent(raw: string): string {
  let out: string = '';
  let pending: number[] = [];
  const flush = (): void => {
    if (pending.length > 0) {
      out += decodeUtf8Bytes(pending);
      pending = [];
    }
  };
  let i: number = 0;
  while (i < raw.length) {
    const ch: string = raw.charAt(i);
    if (ch === '%') {
      const hi: number = i + 1 < raw.length ? hexDigitVal(raw.charCodeAt(i + 1)) : -1;
      const lo: number = i + 2 < raw.length ? hexDigitVal(raw.charCodeAt(i + 2)) : -1;
      if (hi >= 0 && lo >= 0) {
        pending.push((hi << 4) | lo);
        i += 3;
      } else {
        flush();
        out += '%';
        i += 1;
      }
    } else {
      flush();
      out += ch;
      i += 1;
    }
  }
  flush();
  return out;
}

// All query pairs in order, keys and values decoded. Duplicates are KEPT so
// the caller can reject ambiguity (duplicate code/state/error) explicitly.
function parseQueryPairs(query: string): string[][] {
  const pairs: string[][] = [];
  if (query.length === 0) { return pairs; }
  const parts: string[] = query.split('&');
  for (const part of parts) {
    if (part.length === 0) { continue; }
    const eq: number = part.indexOf('=');
    if (eq < 0) {
      pairs.push([decodeQueryComponent(part), '']);
    } else {
      pairs.push([decodeQueryComponent(part.slice(0, eq)),
        decodeQueryComponent(part.slice(eq + 1))]);
    }
  }
  return pairs;
}

// ---- URI parsing / policy --------------------------------------------------

interface ParsedUri {
  valid: boolean;
  scheme: string;    // lowercased ('' when !valid)
  host: string;      // lowercased, bracketed IPv6 kept ('' for no-authority)
  port: string;      // numeric string, default ports elided ('' = none)
  path: string;      // normalized ('/' when the URI carries no path)
  query: string;     // raw (undecoded) query, leading '?' stripped
  hasFragment: boolean;
}

const SCHEME_SHAPE: RegExp = /^[A-Za-z][A-Za-z0-9+.\-]*$/;
const HOST_SHAPE: RegExp = /^[a-z0-9.\-]+$/;
const IPV6_SHAPE: RegExp = /^\[[0-9A-Fa-f:.]+\]$/;
// Whitespace, controls, DEL and C1 have no place in a URI we act on.
const URI_FORBIDDEN_CHARS: RegExp = /[\u0000-\u0020\u007F-\u009F]/;

function parseDefaultPortElision(scheme: string, port: string): string {
  if (port.length === 0) { return ''; }
  const value: number = Number(port);
  if (scheme === 'http' && value === 80) { return ''; }
  if (scheme === 'https' && value === 443) { return ''; }
  return String(value);
}

// Minimal hand parser: exactly what exact callback matching needs. No URL
// constructor / URLSearchParams — not guaranteed on the device runtime.
function parseUri(raw: string): ParsedUri {
  const bad: ParsedUri = { valid: false, scheme: '', host: '', port: '', path: '',
    query: '', hasFragment: false };
  if (raw.length === 0 || raw.length > MAX_CALLBACK_URI_CHARS) { return bad; }
  if (URI_FORBIDDEN_CHARS.test(raw)) { return bad; }
  const colonAt: number = raw.indexOf(':');
  if (colonAt <= 0) { return bad; }
  const rawScheme: string = raw.slice(0, colonAt);
  if (!SCHEME_SHAPE.test(rawScheme)) { return bad; }
  const scheme: string = rawScheme.toLowerCase();
  const rest: string = raw.slice(colonAt + 1);
  // cut body / query / fragment apart (first '?' and '#')
  const queryAt: number = rest.indexOf('?');
  const fragAt: number = rest.indexOf('#');
  let cut: number = rest.length;
  if (queryAt >= 0 && queryAt < cut) { cut = queryAt; }
  if (fragAt >= 0 && fragAt < cut) { cut = fragAt; }
  const body: string = rest.slice(0, cut);
  // A query only exists before a fragment; text after '#' is never a query.
  const query: string = queryAt >= 0 && (fragAt < 0 || queryAt < fragAt)
    ? rest.slice(queryAt + 1, fragAt >= 0 ? fragAt : rest.length)
    : '';
  const result: ParsedUri = { valid: true, scheme: scheme, host: '', port: '',
    path: '/', query: query, hasFragment: fragAt >= 0 };
  if (body.startsWith('//')) {
    const authorityStart: number = 2;
    let authorityEnd: number = body.length;
    for (let i: number = authorityStart; i < body.length; i++) {
      if (body.charAt(i) === '/') { authorityEnd = i; break; }
    }
    const authority: string = body.slice(authorityStart, authorityEnd);
    const path: string = body.slice(authorityEnd);
    if (authority.length === 0) { return bad; }
    if (authority.indexOf('@') >= 0) { return bad; } // userinfo is never ours
    let host: string = '';
    let portRaw: string = '';
    if (authority.startsWith('[')) {
      const closeAt: number = authority.indexOf(']');
      if (closeAt < 0) { return bad; }
      host = authority.slice(0, closeAt + 1);
      portRaw = authority.slice(closeAt + 1);
      if (portRaw.length > 0 && portRaw.charAt(0) !== ':') { return bad; }
      if (portRaw.startsWith(':')) { portRaw = portRaw.slice(1); }
    } else {
      const colonHost: number = authority.indexOf(':');
      if (colonHost >= 0) {
        host = authority.slice(0, colonHost);
        portRaw = authority.slice(colonHost + 1);
      } else {
        host = authority;
      }
    }
    host = host.toLowerCase(); // normalize BEFORE charset validation + matching
    if (!IPV6_SHAPE.test(host) && (host.length === 0 || !HOST_SHAPE.test(host))) {
      return bad;
    }
    if (portRaw.length > 0) {
      if (!/^[0-9]+$/.test(portRaw)) { return bad; }
      const value: number = Number(portRaw);
      if (value < 1 || value > 65535) { return bad; }
    }
    result.host = host;
    result.port = parseDefaultPortElision(scheme, portRaw);
    result.path = path.length === 0 ? '/' : path;
  } else {
    // no-authority form (private-use scheme, e.g. netbird:/oauth2redirect)
    if (scheme === 'http' || scheme === 'https') { return bad; }
    result.path = body.length === 0 ? '/' : body;
  }
  return result;
}

function isLoopbackHost(host: string): boolean {
  if (host === 'localhost' || host === '[::1]') { return true; }
  const values: number[] = [];
  let start: number = 0;
  for (let i: number = 0; i <= host.length; i++) {
    if (i < host.length && host.charAt(i) !== '.') { continue; }
    const octet: string = host.slice(start, i);
    if (!/^[0-9]+$/.test(octet)) { return false; }
    const value: number = Number(octet);
    if (value > 255) { return false; }
    values.push(value);
    start = i + 1;
  }
  return values.length === 4 && values[0] === 127;
}

// Loopback http is the one plain-http redirect shape this flow allows
// (RFC 8252 §7.1); exposed for the adapter's dev-tooling checks.
export function isLoopbackHttpUri(raw: string): boolean {
  const parsed: ParsedUri = parseUri(raw);
  return parsed.valid && parsed.scheme === 'http' && isLoopbackHost(parsed.host);
}

// Validate the CONFIGURED redirect URI (exact match target). Policy:
//   - https anywhere; http only loopback; private-use app schemes allowed
//   - no query, no fragment (the callback's query carries code/state; a
//     configured query would only blur exact matching)
//   - no userinfo, no whitespace/controls, valid numeric port
const FORBIDDEN_REDIRECT_SCHEMES: RegExp =
  /^(javascript|data|blob|file|ftp|ws|wss|about|chrome|tel|mailto|android-app|content)$/;

export function checkRedirectUri(raw: string): UrlCheck {
  const parsed: ParsedUri = parseUri(raw);
  if (!parsed.valid) {
    return { ok: false, error: 'redirect_malformed', url: '' };
  }
  if (parsed.hasFragment) {
    return { ok: false, error: 'redirect_fragment_forbidden', url: '' };
  }
  if (parsed.query.length > 0) {
    return { ok: false, error: 'redirect_query_forbidden', url: '' };
  }
  if (parsed.scheme === 'http') {
    if (!isLoopbackHost(parsed.host)) {
      return { ok: false, error: 'redirect_http_not_loopback', url: '' };
    }
  } else if (parsed.scheme === 'https') {
    // any host
  } else if (FORBIDDEN_REDIRECT_SCHEMES.test(parsed.scheme)) {
    return { ok: false, error: 'redirect_scheme_forbidden', url: '' };
  } else if (parsed.host.length === 0 && parsed.path === '/') {
    return { ok: false, error: 'redirect_malformed', url: '' };
  }
  const port: string = parsed.port.length > 0 ? `:${parsed.port}` : '';
  const url: string = parsed.host.length === 0
    ? `${parsed.scheme}:${parsed.path}`
    : `${parsed.scheme}://${parsed.host}${port}${parsed.path}`;
  return { ok: true, error: '', url: url };
}

// Exact match of the normalized (scheme, host, port, path) — NEVER a
// prefix/suffix/substring comparison. A callback that only looks like ours
// (evil host, extra path, wrong port/scheme) does not match.
export function matchCallbackUri(configuredUri: string, callbackUri: string): boolean {
  const configured: ParsedUri = parseUri(configuredUri);
  const callback: ParsedUri = parseUri(callbackUri);
  if (!configured.valid || !callback.valid) { return false; }
  return configured.scheme === callback.scheme &&
    configured.host === callback.host &&
    configured.port === callback.port &&
    configured.path === callback.path;
}

// Validate an authorize/token endpoint URL. requireHttps is the production
// default; an explicit dev build may pass false (custom-HTTP management /
// dev IdP) — the CALLER owns that decision, user config never relaxes it.
// The path (and any query) is kept verbatim; '#' is refused.
export function checkEndpointUrl(raw: string, requireHttps: boolean): UrlCheck {
  if (raw.length === 0 || raw.length > MAX_ENDPOINT_CHARS) {
    return { ok: false, error: 'endpoint_malformed', url: '' };
  }
  const value: string = raw.trim();
  if (value.length === 0 || URI_FORBIDDEN_CHARS.test(value)) {
    return { ok: false, error: 'endpoint_malformed', url: '' };
  }
  const colonAt: number = value.indexOf(':');
  if (colonAt <= 0) { return { ok: false, error: 'endpoint_malformed', url: '' }; }
  const scheme: string = value.slice(0, colonAt).toLowerCase();
  if (scheme !== 'https' && scheme !== 'http') {
    return { ok: false, error: 'endpoint_scheme_unsupported', url: '' };
  }
  if (requireHttps && scheme !== 'https') {
    return { ok: false, error: 'endpoint_http_forbidden', url: '' };
  }
  const rest: string = value.slice(colonAt + 1);
  if (!rest.startsWith('//')) {
    return { ok: false, error: 'endpoint_malformed', url: '' };
  }
  if (rest.indexOf('#') >= 0) {
    return { ok: false, error: 'endpoint_fragment_forbidden', url: '' };
  }
  const afterSlashes: string = rest.slice(2);
  const slashAt: number = afterSlashes.indexOf('/');
  const queryAt: number = afterSlashes.indexOf('?');
  let authorityEnd: number = afterSlashes.length;
  if (slashAt >= 0 && slashAt < authorityEnd) { authorityEnd = slashAt; }
  if (queryAt >= 0 && queryAt < authorityEnd) { authorityEnd = queryAt; }
  const authority: string = afterSlashes.slice(0, authorityEnd);
  if (authority.length === 0 || authority.indexOf('@') >= 0) {
    return { ok: false, error: 'endpoint_malformed', url: '' };
  }
  let host: string = authority;
  let portRaw: string = '';
  const bracketClose: number = host.indexOf(']');
  if (bracketClose >= 0) {
    // bracketed IPv6 literal: any port colon lives AFTER the ']'
    const tail: string = host.slice(bracketClose + 1);
    if (tail.length > 0) {
      if (tail.charAt(0) !== ':') { return { ok: false, error: 'endpoint_malformed', url: '' }; }
      portRaw = tail.slice(1);
    }
    host = authority.slice(0, bracketClose + 1);
  } else {
    const colonHost: number = authority.lastIndexOf(':');
    if (colonHost >= 0) {
      host = authority.slice(0, colonHost);
      portRaw = authority.slice(colonHost + 1);
    }
  }
  if (portRaw.length > 0) {
    if (!/^[0-9]+$/.test(portRaw)) {
      return { ok: false, error: 'endpoint_malformed', url: '' };
    }
    const portValue: number = Number(portRaw);
    if (portValue < 1 || portValue > 65535) {
      return { ok: false, error: 'endpoint_malformed', url: '' };
    }
  }
  const hostNorm: string = host.toLowerCase(); // validate case-insensitively
  if (hostNorm.length === 0 || (!IPV6_SHAPE.test(hostNorm) && !HOST_SHAPE.test(hostNorm))) {
    return { ok: false, error: 'endpoint_malformed', url: '' };
  }
  return { ok: true, error: '', url: value };
}

// ---- PKCE input validation (shapes only; NO random, NO hashing) -----------

const VERIFIER_SHAPE: RegExp = /^[A-Za-z0-9\-._~]+$/;   // RFC 7636 §4.1
const CHALLENGE_SHAPE: RegExp = /^[A-Za-z0-9\-_]+$/;    // base64url alphabet

export function validatePkceVerifier(verifier: string): FieldCheck {
  if (verifier.length < VERIFIER_MIN_CHARS || verifier.length > VERIFIER_MAX_CHARS) {
    return { ok: false, error: 'verifier_length' };
  }
  if (!VERIFIER_SHAPE.test(verifier)) {
    return { ok: false, error: 'verifier_charset' };
  }
  return { ok: true, error: '' };
}

// S256 challenge: exactly CHALLENGE_CHARS base64url chars, no '=' padding.
export function validatePkceChallenge(challenge: string): FieldCheck {
  if (challenge.length !== CHALLENGE_CHARS) {
    return { ok: false, error: 'challenge_length' };
  }
  if (!CHALLENGE_SHAPE.test(challenge)) {
    return { ok: false, error: 'challenge_charset' };
  }
  return { ok: true, error: '' };
}

// Opaque server-round-tripped value: bounded, no whitespace/controls.
function checkOpaqueField(value: string, maxChars: number, errorCode: string): FieldCheck {
  if (value.length === 0 || value.length > maxChars) {
    return { ok: false, error: errorCode };
  }
  if (URI_FORBIDDEN_CHARS.test(value)) {
    return { ok: false, error: errorCode };
  }
  return { ok: true, error: '' };
}

export function validateAuthCode(code: string): FieldCheck {
  if (code.length === 0 || code.length > MAX_CODE_CHARS) {
    return { ok: false, error: 'code_invalid' };
  }
  for (let i: number = 0; i < code.length; i++) {
    const c: number = code.charCodeAt(i);
    if (c < 0x21 || c > 0x7E) { return { ok: false, error: 'code_invalid' }; }
  }
  return { ok: true, error: '' };
}

function validateState(state: string): FieldCheck {
  return checkOpaqueField(state, MAX_STATE_CHARS, 'state_invalid');
}

function validateClientId(clientId: string): FieldCheck {
  return checkOpaqueField(clientId, MAX_CLIENT_ID_CHARS, 'client_id_invalid');
}

function validateRefreshToken(token: string): FieldCheck {
  if (token.length === 0 || token.length > MAX_SECRET_FIELD_CHARS) {
    return { ok: false, error: 'refresh_token_invalid' };
  }
  for (let i: number = 0; i < token.length; i++) {
    const c: number = token.charCodeAt(i);
    if (c < 0x21 || c > 0x7E) { return { ok: false, error: 'refresh_token_invalid' }; }
  }
  return { ok: true, error: '' };
}

// ---- authorize URL ---------------------------------------------------------

// Build the standard authorization request URL:
//   ?response_type=code&client_id=...&redirect_uri=...&scope=...&state=...
//     &code_challenge=...&code_challenge_method=S256[&audience=...]
// (fixed parameter order; space-joined scopes; safe percent-encoding).
// allowInsecureEndpoints is a DEV-BUILD escape hatch for custom-HTTP
// endpoints — a caller constant, never user config.
export function buildAuthorizeUrl(config: AccountAuthConfig, session: PkceSession,
  allowInsecureEndpoints: boolean = false): AuthorizeUrlResult {
  const authorize: UrlCheck = checkEndpointUrl(config.authorizationEndpoint, !allowInsecureEndpoints);
  if (!authorize.ok) {
    return { ok: false, url: '', error: authorize.error };
  }
  const token: UrlCheck = checkEndpointUrl(config.tokenEndpoint, !allowInsecureEndpoints);
  if (!token.ok) {
    return { ok: false, url: '', error: token.error };
  }
  const clientId: FieldCheck = validateClientId(config.clientId);
  if (!clientId.ok) { return { ok: false, url: '', error: clientId.error }; }
  const redirect: UrlCheck = checkRedirectUri(config.redirectUri);
  if (!redirect.ok) { return { ok: false, url: '', error: redirect.error }; }
  const sessionRedirect: UrlCheck = checkRedirectUri(session.redirectUri);
  if (!sessionRedirect.ok) { return { ok: false, url: '', error: sessionRedirect.error }; }
  if (redirect.url !== sessionRedirect.url) {
    return { ok: false, url: '', error: 'redirect_mismatch' };
  }
  const state: FieldCheck = validateState(session.state);
  if (!state.ok) { return { ok: false, url: '', error: state.error }; }
  const verifier: FieldCheck = validatePkceVerifier(session.verifier);
  if (!verifier.ok) { return { ok: false, url: '', error: verifier.error }; }
  const challenge: FieldCheck = validatePkceChallenge(session.challenge);
  if (!challenge.ok) { return { ok: false, url: '', error: challenge.error }; }
  if (config.scopes.length === 0) {
    return { ok: false, url: '', error: 'scope_missing' };
  }
  for (const scope of config.scopes) {
    if (scope.length === 0 || URI_FORBIDDEN_CHARS.test(scope)) {
      return { ok: false, url: '', error: 'scope_invalid' };
    }
  }
  const scopeList: string = config.scopes.join(' ');
  if (scopeList.length > MAX_SCOPE_LIST_CHARS) {
    return { ok: false, url: '', error: 'scope_invalid' };
  }
  const hasAudience: boolean = config.audience !== undefined && config.audience.length > 0;
  if (config.audience !== undefined && !hasAudience) {
    return { ok: false, url: '', error: 'audience_invalid' };
  }
  const pieces: string[] = [];
  pieces.push('response_type=code');
  pieces.push(`client_id=${formUrlEncode(config.clientId)}`);
  pieces.push(`redirect_uri=${formUrlEncode(session.redirectUri)}`);
  pieces.push(`scope=${formUrlEncode(scopeList)}`);
  pieces.push(`state=${formUrlEncode(session.state)}`);
  pieces.push(`code_challenge=${formUrlEncode(session.challenge)}`);
  pieces.push(`code_challenge_method=${CODE_CHALLENGE_METHOD}`);
  if (hasAudience) {
    pieces.push(`audience=${formUrlEncode(config.audience as string)}`);
  }
  return { ok: true, error: '', url: `${authorize.url}?${pieces.join('&')}` };
}

// ---- callback classification ----------------------------------------------

// Classify one WebView redirect navigation against THIS session. Only the
// exact callback shape carrying a valid state is ever allowed to produce a
// code; everything else is ignored / rejected / reported unsupported, and
// nothing here returns a code unless every check passed.
export function classifyCallback(callbackUri: string, session: PkceSession): CallbackResult {
  const rejected: (detail: string) => CallbackResult = (detail: string): CallbackResult => {
    return { kind: CALLBACK_KIND_REJECTED, code: '', error: '', errorDescription: '', detail: detail };
  };
  const ignored: (detail: string) => CallbackResult = (detail: string): CallbackResult => {
    return { kind: CALLBACK_KIND_IGNORED, code: '', error: '', errorDescription: '', detail: detail };
  };
  if (session.state.length === 0) { return rejected('session_invalid'); }
  const configured: ParsedUri = parseUri(session.redirectUri);
  if (!configured.valid) { return rejected('session_invalid'); }
  const callback: ParsedUri = parseUri(callbackUri);
  if (!callback.valid) { return ignored('not_parseable'); }
  const matches: boolean = configured.scheme === callback.scheme &&
    configured.host === callback.host &&
    configured.port === callback.port &&
    configured.path === callback.path;
  if (!matches) { return ignored('not_the_callback'); }
  if (callback.hasFragment) {
    // Implicit flow (tokens in the fragment) is unsupported — never parsed.
    return { kind: CALLBACK_KIND_UNSUPPORTED, code: '', error: '', errorDescription: '',
      detail: 'fragment_response_unsupported' };
  }
  const pairs: string[][] = parseQueryPairs(callback.query);
  let codeValue: string = '';
  let codeCount: number = 0;
  let stateValue: string = '';
  let stateCount: number = 0;
  let errorValue: string = '';
  let errorCount: number = 0;
  let errorDescription: string = '';
  for (const pair of pairs) {
    const key: string = pair[0];
    if (key === 'code') { codeCount += 1; codeValue = pair[1]; }
    else if (key === 'state') { stateCount += 1; stateValue = pair[1]; }
    else if (key === 'error') {
      errorCount += 1;
      errorValue = pair[1];
    } else if (key === 'error_description') {
      errorDescription = pair[1];
    }
    // unknown params (iss, session_state, ...) are ignored on purpose
  }
  if (codeCount > 1) { return rejected('duplicate_code'); }
  if (stateCount > 1) { return rejected('duplicate_state'); }
  if (errorCount > 1) { return rejected('duplicate_error'); }
  if (stateCount === 0) { return rejected('state_missing'); }
  if (stateValue !== session.state) { return rejected('state_mismatch'); }
  if (errorCount === 1) {
    if (codeCount === 1) { return rejected('code_and_error'); }
    return { kind: CALLBACK_KIND_ERROR, code: '', error: sanitizeErrorText(errorValue, 128),
      errorDescription: sanitizeErrorText(errorDescription, MAX_ERROR_DETAIL_CHARS), detail: '' };
  }
  if (codeCount === 0) { return rejected('code_missing'); }
  const codeCheck: FieldCheck = validateAuthCode(codeValue);
  if (!codeCheck.ok) { return rejected('code_invalid'); }
  return { kind: CALLBACK_KIND_CODE, code: codeValue, error: '', errorDescription: '', detail: '' };
}

// ---- token request form bodies ---------------------------------------------

// RFC 6749 §4.1.3 exchange body (fixed order, urlencoded):
//   code=...&client_id=...&redirect_uri=...&code_verifier=...&grant_type=authorization_code
export function buildAuthorizationCodeForm(config: AccountAuthConfig, session: PkceSession,
  code: string): FormResult {
  const clientId: FieldCheck = validateClientId(config.clientId);
  if (!clientId.ok) { return { ok: false, body: '', error: clientId.error }; }
  const redirect: UrlCheck = checkRedirectUri(session.redirectUri);
  if (!redirect.ok) { return { ok: false, body: '', error: redirect.error }; }
  const verifier: FieldCheck = validatePkceVerifier(session.verifier);
  if (!verifier.ok) { return { ok: false, body: '', error: verifier.error }; }
  const codeCheck: FieldCheck = validateAuthCode(code);
  if (!codeCheck.ok) { return { ok: false, body: '', error: codeCheck.error }; }
  const body: string = `code=${formUrlEncode(code)}` +
    `&client_id=${formUrlEncode(config.clientId)}` +
    `&redirect_uri=${formUrlEncode(session.redirectUri)}` +
    `&code_verifier=${formUrlEncode(session.verifier)}` +
    `&grant_type=authorization_code`;
  return { ok: true, body: body, error: '' };
}

// RFC 6749 §6 refresh body (separate builder; no redirect_uri / verifier):
//   grant_type=refresh_token&client_id=...&refresh_token=...
export function buildRefreshTokenForm(config: AccountAuthConfig,
  refreshToken: string): FormResult {
  const clientId: FieldCheck = validateClientId(config.clientId);
  if (!clientId.ok) { return { ok: false, body: '', error: clientId.error }; }
  const token: FieldCheck = validateRefreshToken(refreshToken);
  if (!token.ok) { return { ok: false, body: '', error: token.error }; }
  const body: string = `grant_type=refresh_token` +
    `&client_id=${formUrlEncode(config.clientId)}` +
    `&refresh_token=${formUrlEncode(refreshToken)}`;
  return { ok: true, body: body, error: '' };
}

// ---- token reply parsing ----------------------------------------------------

// Bounded printable-ASCII text for display: anything else becomes a space.
export function sanitizeErrorText(raw: string, maxChars: number): string {
  let out: string = '';
  for (let i: number = 0; i < raw.length && out.length < maxChars; i++) {
    const c: number = raw.charCodeAt(i);
    out += c >= 0x20 && c <= 0x7E ? String.fromCharCode(c) : ' ';
  }
  return out.trim();
}

function zeroTokens(): AuthTokens {
  return { accessToken: '', idToken: '', refreshToken: '', expiresIn: 0 };
}

// Parse a token endpoint reply (JSON only, bounded). useIdToken decides
// which token is REQUIRED (id_token vs access_token); the missing one is a
// hard error — there is never a silent id/access substitution. JWTs are
// not decoded; trust comes solely from the TLS-protected token endpoint.
export function parseTokenResponse(replyText: string,
  useIdToken: boolean): TokenParseResult {
  if (replyText.length === 0) {
    return { ok: false, errorCode: 'not_json', errorDetail: '', tokens: zeroTokens() };
  }
  if (replyText.length > MAX_TOKEN_REPLY_CHARS) {
    return { ok: false, errorCode: 'too_large', errorDetail: '', tokens: zeroTokens() };
  }
  let root: Object;
  try {
    root = JSON.parse(replyText) as Object;
  } catch (error) {
    return { ok: false, errorCode: 'not_json', errorDetail: '', tokens: zeroTokens() };
  }
  if (root === null || typeof root !== 'object' || Array.isArray(root)) {
    return { ok: false, errorCode: 'not_json_object', errorDetail: '', tokens: zeroTokens() };
  }
  const rec: Record<string, Object> = root as Record<string, Object>;

  // typed field readers — a present-but-wrong-typed field is a hard error
  const readToken = (key: string, invalidCode: string): string | TokenParseResult => {
    const value: Object = rec[key];
    if (value === undefined) { return ''; }
    if (typeof value !== 'string') {
      return { ok: false, errorCode: invalidCode, errorDetail: '', tokens: zeroTokens() };
    }
    return value;
  };
  const fail: (errorCode: string, errorDetail: string) => TokenParseResult =
    (errorCode: string, errorDetail: string): TokenParseResult => {
      return { ok: false, errorCode: errorCode, errorDetail: errorDetail, tokens: zeroTokens() };
    };

  const serverError: Object = rec['error'];
  const serverErrorText: string = typeof serverError === 'string' ? serverError : '';

  const accessTokenOr: string | TokenParseResult =
    readToken('access_token', 'invalid_access_token');
  if (typeof accessTokenOr !== 'string') { return accessTokenOr; }
  const idTokenOr: string | TokenParseResult = readToken('id_token', 'invalid_id_token');
  if (typeof idTokenOr !== 'string') { return idTokenOr; }
  const refreshTokenOr: string | TokenParseResult =
    readToken('refresh_token', 'invalid_refresh_token');
  if (typeof refreshTokenOr !== 'string') { return refreshTokenOr; }

  const accessToken: string = accessTokenOr;
  const idToken: string = idTokenOr;
  const refreshToken: string = refreshTokenOr;
  const requiredPresent: boolean = useIdToken ? idToken.length > 0 : accessToken.length > 0;
  if (!requiredPresent) {
    if (serverErrorText.length > 0) {
      const description: Object = rec['error_description'];
      const descriptionText: string =
        typeof description === 'string' && description.length > 0
          ? description as string
          : serverErrorText;
      return fail('token_error', sanitizeErrorText(descriptionText, MAX_ERROR_DETAIL_CHARS));
    }
    return fail(useIdToken ? 'missing_id_token' : 'missing_access_token', '');
  }

  const tokenType: Object = rec['token_type'];
  if (tokenType !== undefined) {
    if (typeof tokenType !== 'string') { return fail('invalid_token_type', ''); }
    if (tokenType.toLowerCase() !== 'bearer') { return fail('unsupported_token_type', ''); }
  }

  let expiresIn: number = 0;
  const rawExpires: Object = rec['expires_in'];
  if (rawExpires !== undefined) {
    if (typeof rawExpires !== 'number' || !Number.isInteger(rawExpires) ||
      rawExpires < 1 || rawExpires > MAX_EXPIRES_IN) {
      return fail('invalid_expires_in', '');
    }
    expiresIn = rawExpires;
  }

  const tokens: AuthTokens = {
    accessToken: accessToken,
    idToken: idToken,
    refreshToken: refreshToken,
    expiresIn: expiresIn
  };
  return { ok: true, errorCode: '', errorDetail: '', tokens: tokens };
}
