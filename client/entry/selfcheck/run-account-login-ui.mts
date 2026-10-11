// UI integration checks for the account-login entry in Index.ets.
// Run: node client/entry/selfcheck/run-account-login-ui.mts

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const PAGE = readFileSync(join(HERE, '..', 'src', 'main', 'ets', 'pages', 'Index.ets'), 'utf8');

let passed = 0;
let failed = 0;
function check(name: string, condition: boolean): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
}
function sliceFrom(src: string, begin: string, end: string): string {
  const at = src.indexOf(begin);
  if (at < 0) { return ''; }
  const stop = src.indexOf(end, at);
  return stop < 0 ? src.slice(at) : src.slice(at, stop);
}

const begin = sliceFrom(PAGE, 'private async beginAccountLogin()', 'private async finishAccountLogin(');
const finish = sliceFrom(PAGE, 'private async finishAccountLogin(', 'private cancelAccountLogin(');
const cancel = sliceFrom(PAGE, 'private cancelAccountLogin(', '// Device identity for the config file');
const setupSave = sliceFrom(PAGE, 'private saveSetup(): void', 'private beginConfigurationEdit(): void');
const cancelEdit = sliceFrom(PAGE, 'private cancelConfigurationEdit(): void', 'private selectAuthMode(');
const configForm = sliceFrom(PAGE, 'setupFormCard() {', 'serverConfigCard() {');
const configSummary = sliceFrom(PAGE, 'serverConfigCard() {', 'logoutAction() {');
const webView = sliceFrom(PAGE, '  build() {\n    if (this.accountLoginOpen)', '\n  }\n}');
const accountErrorText = sliceFrom(PAGE, 'private accountStoreFailureSuffix(',
  'private async beginAccountLogin()');

check('configuration summary separates server address and login method and has one edit entry',
  configSummary.includes("Text('服务器地址')") && configSummary.includes("Text('登录方式')") &&
  configSummary.includes("Button('更改配置')") && !configSummary.includes('认证方式：'));
check('form offers two equal auth choices and only renders the selected Setup Key field',
  configForm.includes("Button('账户登录')") && configForm.includes("Button('Setup Key')") &&
  configForm.includes('if (this.authMode === ACCOUNT_MODE_SETUP_KEY)') &&
  configForm.includes("'登录并连接'") && configForm.includes("'保存并连接'"));
check('embedded WebView receives exactly its exported callback properties',
  webView.includes('NetBirdAccountWebView({') && webView.includes('authorizeUrl: this.accountAuthorizeUrl') &&
  webView.includes('expectedState: this.accountExpectedState') &&
  webView.includes('redirectUri: this.accountRedirectUri') && webView.includes('onCode:') &&
  webView.includes('onCancel:') && webView.includes('onFailure:'));
check('controller builds login state from checked management URL and bundled CA roots',
  begin.includes('checkManagementUrl(this.urlInput)') &&
  begin.includes('readPackagedCaRootsPem(getContext(this).resourceManager)') &&
  begin.includes('this.accountLoginController.begin(urlCheck.url, caPem)'));
check('account commit starts VPN only after credentials, config, and auth mode persist',
  finish.indexOf('await saveAccountCredentials(') >= 0 &&
  finish.indexOf('await saveAccountCredentials(') < finish.indexOf('writeSetupFile(SETUP_CONFIG_FILE') &&
  finish.indexOf('writeSetupFile(SETUP_CONFIG_FILE') < finish.indexOf('writeSetupFile(ACCOUNT_MODE_FILE_NAME') &&
  finish.indexOf('writeSetupFile(ACCOUNT_MODE_FILE_NAME') < finish.indexOf('this.requestStart();') &&
  finish.split('this.requestStart();').length === 2);
check('Setup Key save starts VPN once only after every save succeeds',
  setupSave.indexOf('if (!this.writeSetupFile(SETUP_CONFIG_FILE') >= 0 &&
  setupSave.includes("this.formError = '保存失败，请重试';\n      return;\n    }\n    // Drop the secret") &&
  setupSave.split('this.requestStart();').length === 2);
check('canceling a configured edit restores persisted mode and URL without writing files',
  cancelEdit.includes('this.refreshSetupState();') && cancelEdit.includes('this.urlInput = this.configuredUrl;') &&
  cancelEdit.includes('this.keyInput = \'\';') && !cancelEdit.includes('writeSetupFile(') &&
  !cancelEdit.includes('this.configured = false'));
check('configured state is derived from stored credentials, not editor visibility',
  PAGE.includes('this.configured = storedMode === ACCOUNT_MODE_ACCOUNT') &&
  !PAGE.includes('this.configured = false'));
check('active/connecting VPN disables configuration entry and reports the reason before submit',
  configSummary.includes('.enabled(!this.accountModeSwitchBlocked())') &&
  configSummary.includes('断开 VPN 后可更改配置') &&
  configForm.includes('.enabled(!this.accountModeSwitchBlocked())') &&
  setupSave.includes('if (this.accountModeSwitchBlocked())'));
check('device identity is reused and Setup Key files are not removed when switching modes',
  finish.includes('this.devicePrivateKey()') && finish.includes('this.deviceRelayEnabled()') &&
  !finish.includes('removeSetupFile(SETUP_KEY_FILE)'));
check('cancel invalidates discovery or token exchange and cancels the private controller',
  cancel.includes('this.accountLoginGeneration++') && cancel.includes('this.accountLoginController.cancel()') &&
  PAGE.includes('const credentials: AccountCredentials = await this.accountLoginController.finish(code);') &&
  PAGE.indexOf('await this.accountLoginController.finish(code);') < PAGE.indexOf('this.accountLoginCommitting = true;') &&
  PAGE.includes("Button('取消登录')") &&
  PAGE.includes('if (!this.accountLoginCommitting) {\n      this.cancelAccountLogin(false);'));
check('auth mode cannot be switched while VPN is connecting or active',
  PAGE.includes('return this.requestSent || (this.published !== null && vpnActiveState(this.published.state));') &&
  setupSave.includes("this.formError = '断开 VPN 后可更改配置';"));
check('UI state contains no persisted or transient token material',
  !/@State\s+(?:private\s+)?\w+\s*:\s*(?:AccountCredentials|.*(?:jwt|refreshToken|verifier|token))/i.test(PAGE));
check('explicit logout clears encrypted account credentials after the existing logout flow',
  PAGE.includes('clearAccountCredentials(getContext(this).filesDir).then(') &&
  PAGE.includes('this.removeSetupFile(ACCOUNT_MODE_FILE_NAME);'));
// 0.0.22: run the two REAL helpers (type annotations stripped) against a coded
// failure, so "step + platform code reaches the sentence" is proven by output,
// not by a source pin.
const errorTextSource = accountErrorText
  .replace('private accountStoreFailureSuffix(', 'function accountStoreFailureSuffix(')
  .replace('private accountLoginErrorText(', 'function accountLoginErrorText(')
  .replace(/this\.accountStoreFailureSuffix/g, 'accountStoreFailureSuffix')
  .replace(/: Error \| string/g, '')
  .replace(/: string/g, '');
class FakeAccountStoreError extends Error {
  constructor(reason: string, step: string, code: number, detail: string) {
    super(reason);
    (this as any).reason = reason;
    (this as any).step = step;
    (this as any).code = code;
    (this as any).detail = detail;
  }
}
const errorText = new Function('AccountStoreError',
  `${errorTextSource}; return { accountLoginErrorText };`)(FakeAccountStoreError) as {
    accountLoginErrorText: (error: Error | string) => string;
  };
function coded(reason: string, step: string, code: number): Error {
  return new FakeAccountStoreError(reason, step, code, 'PLATFORM_RAW_SENTINEL');
}

check('a coded secure-storage failure renders step and platform code in the user message',
  errorText.accountLoginErrorText(coded('unavailable', 'asset.add', 24000001)) ===
    '安全存储不可用，无法保存账户凭据。（asset.add 错误码 24000001）' &&
  errorText.accountLoginErrorText(coded('unsupported', 'asset.query', 202)) ===
    '安全存储不可用，无法保存账户凭据。（asset.query 错误码 202）' &&
  errorText.accountLoginErrorText(coded('malformed', 'file.write', 0)) ===
    '账户登录失败，请重试。（file.write 错误码 0）' &&
  !errorText.accountLoginErrorText(coded('unavailable', 'asset.add', 24000001))
    .includes('PLATFORM_RAW_SENTINEL'));
check('an uncoded store failure and the non-store reasons keep the exact old sentences',
  errorText.accountLoginErrorText(coded('unavailable', '', 0)) === '安全存储不可用，无法保存账户凭据。' &&
  errorText.accountLoginErrorText('discovery_timeout') === '连接管理服务器超时，请检查网络后重试。' &&
  errorText.accountLoginErrorText('cancelled') === '' &&
  errorText.accountLoginErrorText(new Error('device_key_unavailable')) === '账户登录失败，请重试。');
check('the same failure is logged once with reason, step, code and the platform text only',
  finish.split('ACCOUNT_STORE_FAILED').length === 2 &&
  finish.includes("'ACCOUNT_STORE_FAILED|reason=%{public}s|step=%{public}s|code=%{public}d|message=%{public}s'") &&
  finish.includes('failure.reason, failure.step, failure.code, failure.detail') &&
  !finish.includes('failure.message'));

console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
