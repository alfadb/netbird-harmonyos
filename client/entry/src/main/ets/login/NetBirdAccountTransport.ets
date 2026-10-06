/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors

import http from '@ohos.net.http';
import type { AccountAuthConfig, PkceSession, TokenParseResult } from './NetBirdAccountAuthCore';
import {
  buildAuthorizationCodeForm,
  checkEndpointUrl,
  parseTokenResponse
} from './NetBirdAccountAuthCore';

const REQUEST_TIMEOUT_MS: number = 15000;
const MAX_REPLY_BYTES: number = 65536;

function failed(errorCode: string, errorDetail: string = ''): TokenParseResult {
  return { ok: false, errorCode: errorCode, errorDetail: errorDetail,
    tokens: { accessToken: '', idToken: '', refreshToken: '', expiresIn: 0 } };
}

export class AccountTokenTransport {
  private active: http.HttpRequest | undefined = undefined;
  private generation: number = 0;
  private cancelWait: (() => void) | undefined = undefined;
  private destroyed: boolean = false;

  async exchangeAuthorizationCode(config: AccountAuthConfig, session: PkceSession,
    code: string): Promise<TokenParseResult> {
    if (this.active !== undefined) {
      throw new Error('busy');
    }
    const endpoint = checkEndpointUrl(config.tokenEndpoint, true);
    if (!endpoint.ok) {
      return failed(endpoint.error);
    }
    const form = buildAuthorizationCodeForm(config, session, code);
    if (!form.ok) {
      return failed(form.error);
    }

    const request: http.HttpRequest = http.createHttp();
    const options: http.HttpRequestOptions = {
      method: http.RequestMethod.POST,
      header: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: form.body,
      expectDataType: http.HttpDataType.STRING,
      maxLimit: MAX_REPLY_BYTES,
      connectTimeout: REQUEST_TIMEOUT_MS,
      readTimeout: REQUEST_TIMEOUT_MS,
      maxRedirects: 0
    };
    const requestGeneration: number = this.generation;
    this.active = request;
    this.destroyed = false;
    const cancelled: Promise<null> = new Promise<null>((resolve) => {
      this.cancelWait = (): void => resolve(null);
    });
    try {
      const response = await Promise.race([
        request.request(endpoint.url, options),
        cancelled
      ]);
      if (requestGeneration !== this.generation || response === null) {
        return failed('cancelled');
      }
      if (typeof response.result !== 'string') {
        return failed('invalid_response');
      }
      if (response.responseCode < 200 || response.responseCode >= 300) {
        return failed('http_error', `HTTP ${response.responseCode}`);
      }
      return parseTokenResponse(response.result, config.useIdToken);
    } catch (error) {
      if (requestGeneration !== this.generation) {
        return failed('cancelled');
      }
      return failed('network_error');
    } finally {
      this.destroyRequest(request);
      if (this.active === request) {
        this.active = undefined;
        this.cancelWait = undefined;
      }
    }
  }

  cancel(): void {
    if (this.active === undefined) {
      return;
    }
    this.generation += 1;
    const resolveCancelled = this.cancelWait;
    this.cancelWait = undefined;
    this.destroyRequest(this.active);
    if (resolveCancelled !== undefined) {
      resolveCancelled();
    }
  }

  private destroyRequest(request: http.HttpRequest): void {
    if (!this.destroyed) {
      this.destroyed = true;
      try {
        request.destroy();
      } catch (_error) {
        // Teardown must not replace the transport result or interrupt cancellation.
      }
    }
  }
}
