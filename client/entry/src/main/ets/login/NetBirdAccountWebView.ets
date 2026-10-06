/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors

import { webview } from '@kit.ArkWeb';
import { CALLBACK_KIND_CODE, CALLBACK_KIND_ERROR, PkceSession, classifyCallback,
  matchCallbackUri } from './NetBirdAccountAuthCore';

@Component
export struct NetBirdAccountWebView {
  @Prop authorizeUrl: string = '';
  @Prop expectedState: string = '';
  @Prop redirectUri: string = '';
  onCode: (code: string) => void = (_code: string): void => {};
  onCancel: () => void = (): void => {};
  onFailure: (stableReason: string) => void = (_reason: string): void => {};

  @State private loading: boolean = true;
  @State private message: string = '';

  private controller: webview.WebviewController = new webview.WebviewController();
  private controllerAttached: boolean = false;
  private handled: boolean = false;

  private failOnce(reason: string, message: string): void {
    if (this.handled) {
      return;
    }
    this.handled = true;
    this.loading = false;
    this.message = message;
    this.onFailure(reason);
  }

  private cancelOnce(): void {
    if (this.handled) {
      return;
    }
    this.handled = true;
    this.loading = false;
    this.onCancel();
  }

  private handleMainFrameUrl(url: string): boolean {
    if (this.handled) {
      return true;
    }
    if (matchCallbackUri(this.redirectUri, url)) {
      const callbackSession: PkceSession = {
        state: this.expectedState,
        verifier: '',
        challenge: '',
        redirectUri: this.redirectUri
      };
      const result = classifyCallback(url, callbackSession);
      if (result.kind === CALLBACK_KIND_CODE) {
        this.handled = true;
        this.loading = false;
        this.onCode(result.code);
      } else if (result.kind === CALLBACK_KIND_ERROR && result.error === 'access_denied') {
        this.cancelOnce();
      } else {
        this.failOnce('callback_rejected', '登录回调无效，请返回后重试。');
      }
      return true;
    }

    const lowerUrl: string = url.toLowerCase();
    if (lowerUrl.startsWith('https://') || lowerUrl === 'about:blank') {
      return false;
    }
    this.failOnce('unsupported_redirect', '登录页面请求了不支持的跳转。');
    return true;
  }

  private onLoadNavigation(event: OnLoadInterceptEvent): boolean {
    if (!event.data.isMainFrame()) {
      return false;
    }
    return this.handleMainFrameUrl(event.data.getRequestUrl());
  }

  private onOverrideNavigation(request: WebResourceRequest): boolean {
    if (!request.isMainFrame()) {
      return false;
    }
    return this.handleMainFrameUrl(request.getRequestUrl());
  }

  private goBackOrCancel(): void {
    if (this.controllerAttached) {
      try {
        if (this.controller.accessBackward()) {
          this.controller.backward();
          return;
        }
      } catch (_error) {
        // The controller may detach while the page is closing.
      }
    }
    this.cancelOnce();
  }

  build() {
    Column() {
      Row() {
        Text('账户登录')
          .fontSize(20)
          .fontWeight(FontWeight.Medium)
          .layoutWeight(1)
        Button('取消')
          .onClick((): void => this.goBackOrCancel())
      }
      .width('100%')
      .padding({ left: 16, right: 16, top: 12, bottom: 12 })

      if (this.loading) {
        Row() {
          LoadingProgress()
            .width(20)
            .height(20)
          Text('正在加载登录页面…')
            .fontSize(14)
            .fontColor('#52615B')
            .margin({ left: 10 })
        }
        .width('100%')
        .justifyContent(FlexAlign.Center)
        .padding({ top: 8, bottom: 8 })
      }

      if (this.message.length > 0) {
        Text(this.message)
          .fontSize(14)
          .fontColor('#A33B32')
          .padding({ left: 16, right: 16, top: 8, bottom: 8 })
      }

      Web({ src: this.authorizeUrl, controller: this.controller })
        .javaScriptAccess(true)
        .domStorageAccess(true)
        .fileAccess(false)
        .databaseAccess(false)
        .multiWindowAccess(false)
        .onControllerAttached((): void => {
          this.controllerAttached = true;
        })
        .onPageBegin((): void => {
          this.loading = true;
        })
        .onPageEnd((): void => {
          this.loading = false;
        })
        .onLoadIntercept((event: OnLoadInterceptEvent): boolean => this.onLoadNavigation(event))
        .onOverrideUrlLoading((request: WebResourceRequest): boolean =>
          this.onOverrideNavigation(request))
        .onSslErrorEvent((event: SslErrorEvent): void => {
          event.handler.handleCancel();
          if (event.isMainFrame) {
            this.failOnce('tls_error', '安全连接失败，请检查网络后重试。');
          }
        })
        .width('100%')
        .layoutWeight(1)
    }
    .width('100%')
    .height('100%')
    .backgroundColor('#F7F9F8')
  }
}
