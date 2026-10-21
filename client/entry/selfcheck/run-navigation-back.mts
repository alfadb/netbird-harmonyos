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
const peerView = sliceFrom(PAGE, 'peerDetailBody() {', 'peersTab() {');
// 路由选择 (the internal settings page): its builder and the settings entry.
const routeView = sliceFrom(PAGE, '@Builder\n  routeSelectionView() {',
  'private openRouteSelection(): void');
// 0.0.52: the settings entry is the 状态与路由 group's 路由选择 row (the two rows
// share one group now, so the slice starts at the group title).
const settingsRouteEntry = sliceFrom(PAGE, "this.sectionTitle('状态与路由')", 'private closeVisibleSubflow');
// 0.0.52: the ONE subpage title bar all three subpages render.
const titleBar = sliceFrom(PAGE, 'pageTitleBar(title: string) {', 'settingsDivider() {');

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
// 0.0.52: the three subpages render the ONE title bar whose back control (the system
// chevron symbol) closes through the SAME helper — no new path into the close logic,
// and the old '‹' text glyph and the filled 返回 button are both gone.
check('the three subpages share the ONE title bar, whose back control closes through ' +
  'the shared close helper',
  peerView.includes("this.pageTitleBar('对端详情')") &&
  detailView.includes("this.pageTitleBar('详细状态')") &&
  routeView.includes("this.pageTitleBar('路由选择')") &&
  (PAGE.match(/this\.pageTitleBar\('/g) ?? []).length === 3 &&
  titleBar.includes("SymbolGlyph($r('sys.symbol.chevron_left'))") &&
  titleBar.includes('this.closeVisibleSubflow();') &&
  !PAGE.includes("Text('‹')") &&
  !PAGE.includes("Button('返回')") &&
  systemBack.includes('return this.closeVisibleSubflow();'));
// 0.0.51: the settings subpages carry their own title in the back row, so the
// parent 「设置」 title must NOT render above them (no double title).
check('settings subpages show only their own title (no parent 设置 title above it)',
  (() => {
    const settings = PAGE.slice(PAGE.indexOf('settingsTab() {'),
      PAGE.indexOf('// ---- 路由选择 (设置 → 路由选择'));
    return settings.includes('if (!this.showDiagnostics && !this.routeSelectionOpen) {') &&
      settings.indexOf("Text('设置')") < settings.indexOf('if (this.showDiagnostics) {');
  })());
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
  // 0.0.52: the page's back control is the shared title bar, which closes through
  // closeVisibleSubflow (pinned above) — the route page itself no longer holds the
  // handler, so the check follows the ONE builder instead of a copy.
  routeView.includes("this.pageTitleBar('路由选择')") &&
  titleBar.includes('this.closeVisibleSubflow();') &&
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
