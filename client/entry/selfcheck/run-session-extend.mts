/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// 2026-10-13 会话延长 self-check (host-side, no device, no network).
//
// Two halves of the SAME feature:
//
//   1. the OFFER conditions and the texts — checked against the REAL
//      NetBirdUiModel.ets (copied byte-identical into a temp .ts fixture, the
//      run-ui-model.mts pattern), so "deadline > 0 → row", the ceil-rounded
//      labels and the Chinese wording are proven by OUTPUT, not by a source
//      pin. Upstream: `updateSessionRow` HomeFragment.java:901-912,
//      `formatSessionExpiry` :920-941, `ceilDiv` :943-945, the zh-rCN strings
//      app/src/main/res/values-zh-rCN/strings.xml:233-246.
//
//   2. the WIRING in Index.ets — the connect toggle's login-required branch
//      (upstream HomeFragment.java:193-200) and the home session row's tap
//      (fragment_home.xml:377-427 → MainActivity.extendSession :1068-1085).
//      These are source assertions: the page cannot be imported on the host
//      (it pulls @ohos.*).
//
// Run: node client/entry/selfcheck/run-session-extend.mts

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));

let passed = 0;
let failed = 0;
function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
  if (!condition && detail !== undefined) { console.log(`     ${detail}`); }
}

const fixtureDir: string = mkdtempSync(join(tmpdir(), 'nb-session-extend-'));
try {
  // ---- 1. the pure module, byte-identical -----------------------------------
  const source = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability',
    'NetBirdUiModel.ets');
  const fixture: string = join(fixtureDir, 'NetBirdUiModel.ts');
  writeFileSync(fixture, readFileSync(source));
  const m = await import(pathToFileURL(fixture).href);

  const MIN_MS: number = 60 * 1000;
  const HOUR_MS: number = 60 * MIN_MS;
  const DAY_MS: number = 24 * HOUR_MS;
  // nowMs = 0 makes the deadline itself the remaining time in milliseconds.
  const row = (deadlineMs: number): Record<string, unknown> =>
    m.sessionRowView(deadlineMs / 1000, 0) as Record<string, unknown>;

  // The official Chinese wording: upstream's OWN zh-rCN translation, carried
  // verbatim (values-zh-rCN/strings.xml:233-246).
  check('the row constants are upstream zh-rCN verbatim',
    m.SESSION_BANNER_HINT === '点按以延长会话' &&
    m.SESSION_EXTENDED_TEXT === '会话已延长' &&
    m.SESSION_EXPIRED_TEXT === '会话已过期' &&
    m.SESSION_EXPIRES_SOON_TEXT === '会话将在不到一分钟后过期',
    JSON.stringify([m.SESSION_BANNER_HINT, m.SESSION_EXTENDED_TEXT, m.SESSION_EXPIRED_TEXT,
      m.SESSION_EXPIRES_SOON_TEXT]));

  // `updateSessionRow` (:901-912): deadline <= 0 → the row is GONE.
  for (const bad of [undefined, 0, -1, NaN]) {
    const view = m.sessionRowView(bad, 0);
    check(`no row for deadline ${String(bad)}`,
      view.offer === false && view.text === '' && view.hint === '', JSON.stringify(view));
  }
  check('no row when the clock is unusable',
    m.sessionRowView(1_893_456_000, NaN).offer === false);

  // `formatSessionExpiry` (:920-941), branch by branch. The expired label needs
  // a POSITIVE deadline (the row's own visibility rule) whose instant has
  // passed: the wall clock is ahead of it.
  const expired = m.sessionRowView(100, 100 * 1000);
  check('expired: a past deadline keeps the row and says so',
    expired.offer === true && expired.text === m.SESSION_EXPIRED_TEXT &&
    expired.hint === m.SESSION_BANNER_HINT, JSON.stringify(expired));
  check('sub-minute tail keeps its own wording',
    row(1_000).text === m.SESSION_EXPIRES_SOON_TEXT &&
    row(MIN_MS - 1).text === m.SESSION_EXPIRES_SOON_TEXT);
  check('whole minutes round UP (ceilDiv)',
    row(MIN_MS).text === '会话将在 1 分钟后过期' &&
    row(2 * MIN_MS).text === '会话将在 2 分钟后过期' &&
    row(2 * MIN_MS + 1).text === '会话将在 3 分钟后过期' &&
    row(59 * MIN_MS).text === '会话将在 59 分钟后过期');
  check('past the minute band the hour label takes over',
    row(59 * MIN_MS + 1).text === '会话将在 1 小时后过期' &&
    row(23 * HOUR_MS).text === '会话将在 23 小时后过期');
  check('past the hour band the day label takes over',
    row(23 * HOUR_MS + 1).text === '会话将在 1 天后过期' &&
    row(3 * DAY_MS).text === '会话将在 3 天后过期');
  check('every offered row carries the constant official hint',
    [row(MIN_MS), row(HOUR_MS), row(DAY_MS), row(1000)].every(
      (view: Record<string, unknown>): boolean => view.hint === m.SESSION_BANNER_HINT));

  // The one adaptation: the LATER of the connector's published anchor and the
  // server's answer to an extend this page ran (the UI process has no connector
  // to write the anchor into — see the module comment).
  check('the row deadline is the later known observation',
    m.sessionRowDeadlineS(1_000, 2_000) === 2_000 &&
    m.sessionRowDeadlineS(2_000, 1_000) === 2_000);
  check('an unknown side contributes nothing',
    m.sessionRowDeadlineS(undefined, 2_000) === 2_000 &&
    m.sessionRowDeadlineS(2_000, 0) === 2_000 &&
    m.sessionRowDeadlineS(undefined, 0) === 0 &&
    m.sessionRowDeadlineS(NaN, NaN) === 0 &&
    m.sessionRowDeadlineS(-5, -5) === 0);
  check('a published deadline alone still offers the row',
    m.sessionRowView(m.sessionRowDeadlineS(1_893_456_000, 0), 0).offer === true);

  // `session_extend_failed` (strings.xml:297 → zh-rCN:234): interruption is not
  // a message; every class token renders its own sentence.
  check('a cancelled extend has no message (upstream keeps the tunnel, no toast)',
    m.sessionExtendFailureText('cancelled') === '');
  check('the failure prefix is the official zh-rCN wording',
    m.sessionExtendFailureText('timeout').startsWith('延长会话失败：') &&
    m.sessionExtendFailureText('network-error').startsWith('延长会话失败：') &&
    m.sessionExtendFailureText('timeout').includes('超时') &&
    m.sessionExtendFailureText('network-error').includes('网络') &&
    m.sessionExtendFailureText('management-error').includes('管理服务器拒绝了本次延长'));
  check('an unknown token is visible, never silently dropped',
    m.sessionExtendFailureText('bogus-token') === '延长会话失败：未知错误（bogus-token）');

  // ---- 2. the Index.ets wiring ---------------------------------------------
  const PAGE: string = readFileSync(join(HERE, '..', 'src', 'main', 'ets', 'pages', 'Index.ets'),
    'utf8');
  function sliceFrom(src: string, begin: string, end: string): string {
    const at = src.indexOf(begin);
    if (at < 0) { return ''; }
    const stop = src.indexOf(end, at);
    return stop < 0 ? src.slice(at) : src.slice(at, stop);
  }

  const toggle = sliceFrom(PAGE, 'Button(this.toggleLabel())', '// 状态卡片：');
  check('the connect tap runs the interactive login when the account needs one',
    toggle.includes('this.loginRequired() && this.authMode === ACCOUNT_MODE_ACCOUNT') &&
    toggle.includes('this.beginConnectLogin();') &&
    toggle.indexOf('this.beginConnectLogin();') < toggle.lastIndexOf('this.requestStart();') &&
    toggle.includes('} else {') && toggle.includes('this.requestStart();'),
    toggle);
  check('Setup Key peers keep the plain connect (no account SSO for them)',
    toggle.includes("this.authMode === ACCOUNT_MODE_ACCOUNT") &&
    !toggle.includes('ACCOUNT_MODE_SETUP_KEY'));

  const connectLogin = sliceFrom(PAGE, 'private async beginConnectLogin()',
    '// Device identity for the config file');
  const connectCommit = sliceFrom(PAGE, 'private async finishConnectLogin(',
    'private async finishSessionExtend(');
  check('the connect-intent login uses the stored URL and no extend flag',
    connectLogin.includes('checkManagementUrl(this.configuredUrl)') &&
    connectLogin.includes('ACCOUNT_INTENT_CONNECT, this.configuredUrl, false') &&
    connectLogin.includes('this.requestStart();'),
    connectLogin);
  check('the connect commit runs the SAME login action and then connects',
    connectCommit.includes('this.loginConnectorConfig(this.accountLoginTargetUrl)') &&
    connectCommit.includes('this.accountLoginController.finish(code, connectorConfig)') &&
    connectCommit.indexOf('this.accountLoginController.finish(code, connectorConfig)') <
      connectCommit.indexOf('this.requestStart();') &&
    connectCommit.split('this.requestStart();').length === 2 &&
    !connectCommit.includes('writeSetupFile('));

  const extendBegin = sliceFrom(PAGE, 'private async beginSessionExtend()',
    'private async beginConnectLogin()');
  const extendCommit = sliceFrom(PAGE, 'private async finishSessionExtend(',
    '// The connector start config both new intents dial with');
  check('the session row tap opens the extend SSO against the stored URL',
    extendBegin.includes('ACCOUNT_INTENT_EXTEND, this.configuredUrl, true') &&
    extendBegin.includes('this.sessionExtendBusy') &&
    !extendBegin.includes('accountModeSwitchBlocked'),
    extendBegin);
  check('the extend commit calls the core extend job and leaves the tunnel alone',
    extendCommit.includes('this.accountLoginController.finishExtend(code, connectorConfig)') &&
    !extendCommit.includes('requestStart') && !extendCommit.includes('requestStop') &&
    !extendCommit.includes('writeSetupFile(') && !extendCommit.includes('removeConnectFailure'),
    extendCommit);
  check('only a POSITIVE deadline moves the displayed anchor',
    extendCommit.includes('if (result.sessionExpiresAtS > 0)') &&
    extendCommit.includes('this.sessionExtendAtS = result.sessionExpiresAtS;'));
  check('the extend outcomes are upstream\'s two toasts',
    extendCommit.includes('this.toastSessionExtend(SESSION_EXTENDED_TEXT)') &&
    extendCommit.includes('this.toastSessionExtend(sessionExtendFailureText(reason))'));

  const sessionRowSource = sliceFrom(PAGE, 'if (this.sessionRowOffer())', '// ---- 对端');
  check('the home session row renders the offer text and hint and taps the extend',
    sessionRowSource.includes('Text(this.sessionRowText())') &&
    sessionRowSource.includes('Text(this.sessionRowHint())') &&
    sessionRowSource.includes('this.beginSessionExtend();') &&
    sessionRowSource.includes('.enabled(!this.sessionExtendBusy && !this.accountLoginBusy)'),
    sessionRowSource);
  check('the row follows the core\'s own published deadline token',
    PAGE.includes('this.published === null') &&
    PAGE.includes('typeof status.session_expires_at_s === \'number\'') &&
    PAGE.includes('sessionRowDeadlineS(atS, this.sessionExtendAtS)'));
  check('the row is absent while no connector document is live',
    sliceFrom(PAGE, 'private sessionRowOffer()', 'private sessionRowText()')
      .includes('if (this.published === null) {\n      return false;'));

  const dispatch = sliceFrom(PAGE, 'private async handleAccountLoginCode(',
    'private async finishConnectLogin(');
  check('the ONE ArkWeb callback dispatches on the flow\'s intent',
    dispatch.includes('ACCOUNT_INTENT_EXTEND') && dispatch.includes('await this.finishSessionExtend(code);') &&
    dispatch.includes('ACCOUNT_INTENT_CONNECT') && dispatch.includes('await this.finishConnectLogin(code);') &&
    dispatch.includes('await this.finishAccountLogin(code);') &&
    PAGE.includes('this.handleAccountLoginCode(code);'),
    dispatch);
  check('no token ever enters the page\'s state',
    !PAGE.includes("'jwt'") && !PAGE.includes('jwtToken') && !PAGE.includes('refreshToken') &&
    !/@State\s+(?:private\s+)?\w+\s*:\s*.*(?:jwt|verifier)/i.test(PAGE));
} catch (error) {
  failed++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(fixtureDir, { recursive: true, force: true });
}

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
