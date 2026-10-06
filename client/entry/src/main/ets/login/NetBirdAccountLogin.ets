/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors

import core from 'libnetbird_core.so';
import { buildAuthorizeUrl, checkRedirectUri, validateAuthCode } from './NetBirdAccountAuthCore';
import type { AccountAuthConfig, PkceSession } from './NetBirdAccountAuthCore';
import { createPkceSession } from './NetBirdAccountCrypto';
import { AccountTokenTransport } from './NetBirdAccountTransport';
import type { AccountCredentials } from './NetBirdAccountStoreCore';

export interface PreparedAccountLogin {
  authorizeUrl: string;
  expectedState: string;
  redirectUri: string;
}

interface NativeAccountApi {
  account_auth_discovery_start?: (request: string) => string;
  account_auth_discovery_poll?: (requestId: number) => string;
  account_auth_discovery_cancel?: (requestId: number) => string;
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

  async finish(code: string): Promise<AccountCredentials> {
    const config = this.config;
    const session = this.session;
    if (this.exchangeUsed) { throw fail('exchange_already_used'); }
    if (discoveryOwner !== this || !this.prepared || config === undefined || session === undefined) {
      throw fail('login_not_prepared');
    }
    if (!validateAuthCode(code).ok) { throw fail('code_invalid'); }
    this.exchangeUsed = true;
    const generation: number = this.generation;
    try {
      const result = await this.transport.exchangeAuthorizationCode(config, session, code);
      if (generation !== this.generation) { throw fail('cancelled'); }
      if (!result.ok) { throw fail(result.errorCode || 'token_exchange_failed'); }
      const tokens = result.tokens;
      const jwt: string = config.useIdToken ? tokens.idToken : tokens.accessToken;
      if (jwt.length === 0) { throw fail('token_invalid'); }
      return {
        managementUrl: this.managementUrl,
        clientId: config.clientId,
        tokenEndpoint: config.tokenEndpoint,
        useIdToken: config.useIdToken,
        jwt: jwt,
        refreshToken: tokens.refreshToken,
        expiresAtMs: tokens.expiresIn > 0 ? Date.now() + tokens.expiresIn * 1000 : 0
      };
    } catch (error) {
      if (generation !== this.generation) { throw fail('cancelled'); }
      throw error instanceof Error && error.message === 'busy' ? fail('busy') : fail('token_exchange_failed');
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
    if (id > 0) {
      const api = core as Object as NativeAccountApi;
      try { api.account_auth_discovery_cancel?.(id); } catch (_error) {}
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
