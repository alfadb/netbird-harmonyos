// Host-side navigation back checks for Index.ets; no device, network, or build.
// Run: node client/entry/selfcheck/run-navigation-back.mts

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

const helperMatch = PAGE.match(/export function navigationBackDecision\([\s\S]*?\n}/);
if (helperMatch === null) { throw new Error('navigationBackDecision missing'); }
const helperSource = helperMatch[0].replace(/: boolean/g, '').replace(/: string/g, '').replace(/^export /, '');
const navigationBackDecision = new Function(`${helperSource}; return navigationBackDecision;`)() as
  (webView: boolean, busy: boolean, committing: boolean, editor: boolean,
    diagnostics: boolean, peer: boolean) => string;
const closeHandler = sliceFrom(PAGE, 'private closeVisibleSubflow(): boolean', 'onBackPress(): boolean');
const systemBack = sliceFrom(PAGE, 'onBackPress(): boolean', 'build() {');
const homeDetailEntry = sliceFrom(PAGE, "UI_DETAIL_STATUS_OPENED|from=home", '.width(100%)');
const settingsDetailEntry = sliceFrom(PAGE, "this.detailStatusOriginTab = 2;", '});');
const detailView = sliceFrom(PAGE, 'detailStatusView() {', '@Builder\n  settingsTab()');
const peerView = sliceFrom(PAGE, 'peersTab() {', '// ---- 设置');
// 路由选择 (the internal settings page): its builder and the settings entry.
const routeView = sliceFrom(PAGE, '@Builder\n  routeSelectionView() {',
  'private openRouteSelection(): void');
const settingsRouteEntry = sliceFrom(PAGE, "this.sectionTitle('路由选择')", 'private closeVisibleSubflow');

check('peer detail consumes one back, then root returns normal exit',
  navigationBackDecision(false, false, false, false, false, true) === 'close-peer' &&
  navigationBackDecision(false, false, false, false, false, false) === 'exit');
check('stale peer selection on another tab does not consume back',
  PAGE.includes('this.activeTab === 1 && this.selectedPeer !== null') &&
  navigationBackDecision(false, false, false, false, false, false) === 'exit');
check('home detailed status returns to home and settings detailed status to settings',
  homeDetailEntry.includes('this.detailStatusOriginTab = 0;') &&
  settingsDetailEntry.includes('this.detailStatusOriginTab = 2;') &&
  closeHandler.includes('this.activeTab = this.detailStatusOriginTab;'));
check('detailed status close is ordered before peer detail and follows actual tab visibility',
  closeHandler.includes('this.activeTab === 2 && this.showDiagnostics') &&
  closeHandler.indexOf("action === 'close-diagnostics'") < closeHandler.indexOf("action === 'close-peer'") &&
  closeHandler.includes('this.activeTab === 1 && this.selectedPeer !== null'));
check('WebView cancellation is first and reuses the existing cancellation callback',
  navigationBackDecision(true, true, false, false, false, true) === 'account-webview' &&
  closeHandler.indexOf("action === 'account-webview'") < closeHandler.indexOf("action === 'cancel-account'") &&
  closeHandler.includes('this.cancelAccountLogin(false);'));
check('cancellable account work cancels, while commit consumes back without cancellation',
  navigationBackDecision(false, true, false, false, false, false) === 'cancel-account' &&
  navigationBackDecision(false, true, true, false, false, false) === 'consume' &&
  closeHandler.includes("if (action === 'consume') {\n      return true;"));
check('configured edit cancels before detail; unconfigured first-run form remains root',
  navigationBackDecision(false, false, false, true, true, false) === 'cancel-editor' &&
  closeHandler.includes('this.activeTab === 2 && !this.showDiagnostics && this.configured && this.showSetupForm') &&
  navigationBackDecision(false, false, false, false, false, false) === 'exit');
check('visible peer and detailed-status buttons share the same close helper',
  peerView.includes('this.closeVisibleSubflow();') && detailView.includes('this.closeVisibleSubflow();') &&
  systemBack.includes('return this.closeVisibleSubflow();'));
check('root back is not trapped', systemBack.includes('return this.closeVisibleSubflow();') &&
  closeHandler.includes('return false;'));
// ---- 路由选择 (2026-10-09): an internal SETTINGS page ------------------------
check('route selection detail consumes one back returning to settings, then root exits normally',
  navigationBackDecision(false, false, false, false, false, false, true) === 'close-route' &&
  navigationBackDecision(false, false, false, false, false, false, false) === 'exit' &&
  closeHandler.includes('this.routeSelectionOpen = false;'));
check('route selection close is a settings page close: it gates on the visible settings tab and discards nothing',
  closeHandler.includes('this.activeTab === 2 && !this.showDiagnostics && this.routeSelectionOpen') &&
  closeHandler.indexOf('this.routeSelectionOpen = false;') > 0 &&
  closeHandler.indexOf('discardRouteDraft') < 0 &&
  routeView.includes('this.closeVisibleSubflow();') &&
  settingsRouteEntry.includes('this.openRouteSelection();'));
check('route selection detail never preempts the editor, diagnostics or peer handlers',
  navigationBackDecision(false, false, false, true, false, false, true) === 'cancel-editor' &&
  navigationBackDecision(false, false, false, false, true, false, true) === 'close-diagnostics' &&
  navigationBackDecision(false, false, false, false, false, true, true) === 'close-peer' &&
  closeHandler.indexOf("action === 'close-diagnostics'") < closeHandler.indexOf("action === 'close-route'") &&
  closeHandler.indexOf("action === 'close-peer'") < closeHandler.indexOf("action === 'close-route'"));
check('back from the route page touches no connection: every change was saved when the switch moved',
  closeHandler.indexOf('stopVpnExtensionAbility') < 0 &&
  closeHandler.indexOf('requestStart') < 0 && closeHandler.indexOf('requestStop') < 0 &&
  closeHandler.indexOf('routeApplyBusy') < 0 &&
  routeView.indexOf("Button('放弃更改')") < 0 && routeView.indexOf("Button('保存") < 0);

console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
