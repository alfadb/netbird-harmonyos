/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// The account login action (PKCE + the management registration it feeds).
//
// Upstream shape (client/android/client.go:194-215, client/internal/auth/
// auth.go:184-236, shared/management/client/grpc.go:641-656): the app runs the
// interactive SSO flow, then IMMEDIATELY performs the management login /
// registration with the fresh JWT — before the engine starts — and keeps no
// token anywhere (client/internal/profilemanager/config.go:62-135 has no token
// field). This controller is that action for the HarmonyOS shell: after the
// token exchange it hands the JWT to the core's `account_login_*` NAPI job
// (which dials with the SAME device identity / management URL the extension
// uses) and returns only the account facts the caller must persist — the
// management URL. The JWT never leaves this class and is never written to
// disk.

import core from 'libnetbird_core.so';
import { buildAuthorizeUrl, checkRedirectUri, validateAuthCode } from './NetBirdAccountAuthCore';
import type { AccountAuthConfig, PkceSession } from './NetBirdAccountAuthCore';
import { createPkceSession } from './NetBirdAccountCrypto';
import { AccountTokenTransport } from './NetBirdAccountTransport';

export interface PreparedAccountLogin {
  authorizeUrl: string;
  expectedState: string;
  redirectUri: string;
}

/// What a completed login action leaves for the shell: the management URL the
/// account belongs to. Deliberately NO token, no refresh token, no expiry —
/// upstream persists none of them.
export interface AccountLoginResult {
  managementUrl: string;
}

interface NativeAccountApi {
  account_auth_discovery_start?: (request: string) => string;
  account_auth_discovery_poll?: (requestId: number) => string;
  account_auth_discovery_cancel?: (requestId: number) => string;
  account_login_start?: (request: string) => string;
  account_login_poll?: (requestId: number) => string;
  account_login_cancel?: (requestId: number) => string;
}

interface DiscoveryMetadata {
  client_id: string;
  audience: string;
  authorization_endpoint: string;
  token_endpoint: string;
  scope: string;
  use_id_token: boolean;
  redirect_urls: string[];
  disable_prompt_login: boolean;
  login_flag: number;
}

interface DiscoveryReply {
  ok?: boolean;
  request_id?: number;
  state?: string;
  config?: DiscoveryMetadata;
}

interface LoginJobReply {
  ok?: boolean;
  request_id?: number;
  state?: string;
  outcome?: string;
  error?: string;
}

let discoveryOwner: AccountLoginController | undefined = undefined;
const POLL_INTERVAL_MS: number = 250;
const MAX_POLL_COUNT: number = 84;

function fail(reason: string): Error {
  return new Error(reason);
}

function parseReply(text: string): DiscoveryReply {
  if (text.length === 0 || text.length > 16384) { throw fail('discovery_failed'); }
  try {
    const value: Object = JSON.parse(text) as Object;
    if (value === null || typeof value !== 'object' || Array.isArray(value)) {
      throw fail('discovery_failed');
    }
    return value as DiscoveryReply;
  } catch (_error) {
    throw fail('discovery_failed');
  }
}

function mapMetadata(raw: DiscoveryMetadata): AccountAuthConfig {
  if (raw === null || typeof raw !== 'object' || typeof raw.client_id !== 'string' ||
    typeof raw.audience !== 'string' || typeof raw.authorization_endpoint !== 'string' ||
    typeof raw.token_endpoint !== 'string' || typeof raw.scope !== 'string' ||
    typeof raw.use_id_token !== 'boolean' || !Array.isArray(raw.redirect_urls) ||
    typeof raw.disable_prompt_login !== 'boolean' || typeof raw.login_flag !== 'number') {
    throw fail('metadata_invalid');
  }
  if (![0, 1, 2].includes(raw.login_flag)) { throw fail('metadata_invalid'); }
  let redirectUri: string = '';
  for (const candidate of raw.redirect_urls) {
    if (typeof candidate !== 'string') { continue; }
    const checked = checkRedirectUri(candidate);
    if (checked.ok) { redirectUri = checked.url; break; }
  }
  if (redirectUri.length === 0) { throw fail('redirect_unavailable'); }
  const scopes: string[] = raw.scope.trim().length === 0 ? [] : raw.scope.trim().split(/\s+/);
  return {
    clientId: raw.client_id,
    authorizationEndpoint: raw.authorization_endpoint,
    tokenEndpoint: raw.token_endpoint,
    redirectUri: redirectUri,
    scopes: scopes,
    audience: raw.audience.length > 0 ? raw.audience : undefined,
    useIdToken: raw.use_id_token
  };
}

export class AccountLoginController {
  private generation: number = 0;
  private requestId: number = 0;
  private config: AccountAuthConfig | undefined = undefined;
  private session: PkceSession | undefined = undefined;
  private prepared: boolean = false;
  private exchangeUsed: boolean = false;
  private polling: boolean = false;
  private pollTimer: number = -1;
  private wakePoll: (() => void) | undefined = undefined;
  private transport: AccountTokenTransport = new AccountTokenTransport();
  private managementUrl: string = '';
  private loginRequestId: number = 0;

  async begin(managementUrl: string, caPem: string): Promise<PreparedAccountLogin> {
    if (discoveryOwner !== undefined || this.prepared || this.polling) { throw fail('busy'); }
    const api = core as Object as NativeAccountApi;
    if (typeof api.account_auth_discovery_start !== 'function' ||
      typeof api.account_auth_discovery_poll !== 'function' ||
      typeof api.account_auth_discovery_cancel !== 'function') { throw fail('discovery_unavailable'); }
    if (typeof managementUrl !== 'string' || managementUrl.length === 0 ||
      typeof caPem !== 'string' || caPem.trim().length === 0) { throw fail('invalid_input'); }
    discoveryOwner = this;
    const generation: number = ++this.generation;
    this.exchangeUsed = false;
    let nativeFinished: boolean = false;
    try {
      const start = parseReply(api.account_auth_discovery_start(JSON.stringify({
        management_url: managementUrl,
        ca_pem: caPem
      })));
      if (start.ok !== true || typeof start.request_id !== 'number' || start.request_id <= 0) {
        throw fail('discovery_failed');
      }
      this.requestId = start.request_id;
      this.polling = true;
      let rawConfig: DiscoveryMetadata | undefined = undefined;
      for (let attempt: number = 0; attempt <= MAX_POLL_COUNT; attempt++) {
        if (generation !== this.generation) { throw fail('cancelled'); }
        const poll = parseReply(api.account_auth_discovery_poll(start.request_id));
        if (poll.state === 'done') {
          nativeFinished = true;
          rawConfig = poll.config;
          break;
        }
        if (poll.state === 'failed' || poll.state === 'unknown') { throw fail('discovery_failed'); }
        if (poll.state !== 'pending' || attempt === MAX_POLL_COUNT) { throw fail('discovery_timeout'); }
        await this.delay(generation);
      }
      if (generation !== this.generation) { throw fail('cancelled'); }
      if (rawConfig === undefined) { throw fail('discovery_failed'); }
      const config = mapMetadata(rawConfig);
      const session = await createPkceSession(config.redirectUri);
      if (generation !== this.generation) { throw fail('cancelled'); }
      const built = buildAuthorizeUrl(config, session);
      if (!built.ok) { throw fail('metadata_invalid'); }
      let authorizeUrl: string = `${built.url}&response_mode=query`;
      if (!rawConfig.disable_prompt_login) {
        if (rawConfig.login_flag === 0) { authorizeUrl += '&prompt=login'; }
        else if (rawConfig.login_flag === 1) { authorizeUrl += '&max_age=0'; }
      }
      this.config = config;
      this.session = session;
      this.managementUrl = managementUrl;
      this.prepared = true;
      this.polling = false;
      this.requestId = 0;
      return { authorizeUrl: authorizeUrl, expectedState: session.state, redirectUri: session.redirectUri };
    } catch (error) {
      this.polling = false;
      if (generation === this.generation) { discoveryOwner = undefined; }
      throw error instanceof Error && ['busy', 'cancelled', 'discovery_timeout', 'discovery_failed',
        'discovery_unavailable', 'invalid_input', 'metadata_invalid', 'redirect_unavailable',
        'pkce_crypto_unavailable'].includes(error.message) ? error : fail('login_failed');
    } finally {
      if (!nativeFinished && this.requestId > 0) {
        try { api.account_auth_discovery_cancel?.(this.requestId); } catch (_error) {}
      }
      this.requestId = 0;
    }
  }

  // `configJson` is the SAME connector start config document the extension
  // hands to `connector_start_with_socket` (management URL + device private key
  // + trust roots + meta): the login action therefore registers exactly the
  // identity the VPN will later run with. Only its outcome token is returned.
  private async runManagementLogin(jwt: string, configJson: string): Promise<string> {
    const api = core as Object as NativeAccountApi;
    if (typeof api.account_login_start !== 'function' || typeof api.account_login_poll !== 'function' ||
      typeof api.account_login_cancel !== 'function') { throw fail('login_unavailable'); }
    const generation: number = this.generation;
    const start = parseLoginReply(api.account_login_start(JSON.stringify({
      config: configJson,
      jwt: jwt
    })));
    if (start.ok !== true || typeof start.request_id !== 'number' || start.request_id <= 0) {
      throw fail(typeof start.error === 'string' && start.error.length > 0 ? start.error : 'login_failed');
    }
    const requestId: number = start.request_id;
    this.loginRequestId = requestId;
    let finished: boolean = false;
    try {
      for (let attempt: number = 0; attempt <= MAX_POLL_COUNT; attempt++) {
        if (generation !== this.generation) { throw fail('cancelled'); }
        const poll = parseLoginReply(api.account_login_poll(requestId));
        if (poll.state === 'done') {
          finished = true;
          if (poll.outcome === 'success') { return 'success'; }
          if (poll.outcome === 'needs-login') { return 'needs-login'; }
          throw fail('login_failed');
        }
        if (poll.state === 'failed' || poll.state === 'unknown') {
          finished = true;
          throw fail(typeof poll.error === 'string' && poll.error.length > 0 ? poll.error : 'login_failed');
        }
        if (poll.state !== 'pending' || attempt === MAX_POLL_COUNT) { throw fail('login_timeout'); }
        await this.delay(generation);
      }
      throw fail('login_timeout');
    } finally {
      this.loginRequestId = 0;
      if (!finished) {
        try { api.account_login_cancel?.(requestId); } catch (_error) {}
      }
    }
  }

  // The login action: exchange the authorization code for the JWT, register
  // with it through the core, and return ONLY the account facts to persist.
  // `configJson` is built by the caller from the very same device identity the
  // extension will dial with.
  async finish(code: string, configJson: string): Promise<AccountLoginResult> {
    const config = this.config;
    const session = this.session;
    if (this.exchangeUsed) { throw fail('exchange_already_used'); }
    if (discoveryOwner !== this || !this.prepared || config === undefined || session === undefined) {
      throw fail('login_not_prepared');
    }
    if (!validateAuthCode(code).ok) { throw fail('code_invalid'); }
    if (typeof configJson !== 'string' || configJson.length === 0) { throw fail('invalid_input'); }
    this.exchangeUsed = true;
    const generation: number = this.generation;
    try {
      const result = await this.transport.exchangeAuthorizationCode(config, session, code);
      if (generation !== this.generation) { throw fail('cancelled'); }
      if (!result.ok) { throw fail(result.errorCode || 'token_exchange_failed'); }
      const tokens = result.tokens;
      const jwt: string = config.useIdToken ? tokens.idToken : tokens.accessToken;
      if (jwt.length === 0) { throw fail('token_invalid'); }
      // 2026-10-12: the JWT is a LOCAL. It goes straight to the core's login
      // action and dies with this call frame — never into a file, never into
      // UI state, never into a later connect.
      const outcome: string = await this.runManagementLogin(jwt, configJson);
      if (generation !== this.generation) { throw fail('cancelled'); }
      if (outcome !== 'success') { throw fail(outcome); }
      return { managementUrl: this.managementUrl };
    } catch (error) {
      if (generation !== this.generation) { throw fail('cancelled'); }
      throw error instanceof Error ? error : fail('login_failed');
    } finally {
      if (generation === this.generation) { this.clearPrepared(); }
    }
  }

  cancel(): void {
    this.generation += 1;
    this.transport.cancel();
    this.stopDelay();
    const id: number = this.requestId;
    this.requestId = 0;
    const loginId: number = this.loginRequestId;
    this.loginRequestId = 0;
    const api = core as Object as NativeAccountApi;
    if (id > 0) {
      try { api.account_auth_discovery_cancel?.(id); } catch (_error) {}
    }
    if (loginId > 0) {
      try { api.account_login_cancel?.(loginId); } catch (_error) {}
    }
    this.clearPrepared();
    this.polling = false;
    if (discoveryOwner === this) { discoveryOwner = undefined; }
  }

  private delay(generation: number): Promise<void> {
    return new Promise<void>((resolve: () => void): void => {
      this.wakePoll = (): void => resolve();
      this.pollTimer = setTimeout((): void => {
        this.pollTimer = -1;
        this.wakePoll = undefined;
        if (generation === this.generation) { resolve(); }
      }, POLL_INTERVAL_MS) as number;
    });
  }

  private stopDelay(): void {
    if (this.pollTimer >= 0) {
      clearTimeout(this.pollTimer);
      this.pollTimer = -1;
    }
    const wake = this.wakePoll;
    this.wakePoll = undefined;
    if (wake !== undefined) { wake(); }
  }

  private clearPrepared(): void {
    this.prepared = false;
    this.config = undefined;
    this.session = undefined;
    this.managementUrl = '';
    if (discoveryOwner === this) { discoveryOwner = undefined; }
  }
}

// The login-action reply is a stable-token document: `{ok,request_id}` for the
// start, `{state,outcome}` / `{state,error}` for the poll. Only stable tokens
// cross this boundary — never a server message.
function parseLoginReply(text: string): LoginJobReply {
  if (text.length === 0 || text.length > 16384) { throw fail('login_failed'); }
  let value: Object;
  try {
    value = JSON.parse(text) as Object;
  } catch (_error) {
    throw fail('login_failed');
  }
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw fail('login_failed');
  }
  const reply: LoginJobReply = value as LoginJobReply;
  if (reply.error !== undefined && typeof reply.error !== 'string') { throw fail('login_failed'); }
  if (reply.outcome !== undefined && typeof reply.outcome !== 'string') { throw fail('login_failed'); }
  return reply;
}
