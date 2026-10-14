// Host-side checks for the responsive shell — DESIGN-HARMONY I1 (0.0.36) and I2
// (0.0.37). No device, no build.
//
// Run: node client/entry/selfcheck/run-harmony-layout.mts
//
// Two halves, both pinned against the byte-identical production sources:
//   * the DECISIONS in pages/NetBirdHarmonyLayout.ets — the official
//     `WidthBreakpoint` boundaries, the RAIL TIER TABLE (0.0.37: 240/280/320 vp
//     by window tier) with the pane-split invariant asserted at every tier
//     boundary, the pane decision, the xl indent limit, the STATE-DRIVEN DEFAULT
//     PANE rule (I2) and the visual scale / label-value row width policy;
//   * the WIRING in pages/Index.ets — that the narrow branch's Tabs is still
//     there, that the split branch exists and re-parents today's builders instead
//     of re-implementing them, that the layout never reads
//     deviceInfo.deviceType, that the wide pane has exactly ONE back authority,
//     that the pane's content is chosen by the resolved destination (not raw
//     activeTab), and that the rail's rows cannot break a word.
//
// run-peer-list-render.mts stays green next to this file: the peer list, its
// ForEach key (peerRowKey) and the reader wiring are not touched by I2 (the row
// gained padding/hover/keyboard but NO new rendered field, so the key is still
// exactly what peerRow renders).
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const ets = join(here, '..', 'src', 'main', 'ets');
const dir = mkdtempSync(join(tmpdir(), 'nb-harmony-layout-'));
const PAGE = readFileSync(join(ets, 'pages', 'Index.ets'), 'utf8');
let passed = 0;
let failed = 0;
function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
  if (!condition && detail !== undefined) {
    console.log(`     ${detail}`);
  }
}
try {
  // The pure module is imported byte-identical (only the extension changes).
  const layoutFixture = join(dir, 'NetBirdHarmonyLayout.ts');
  writeFileSync(layoutFixture, readFileSync(join(ets, 'pages', 'NetBirdHarmonyLayout.ets')));
  const m = await import(pathToFileURL(layoutFixture).href);

  // -------------------------------------------------------------------------
  // 1. the classifier: the official WidthBreakpoint boundaries, verbatim
  //    (xs (0,320) · sm [320,600) · md [600,840) · lg [840,1440) · xl [1440,∞))
  // -------------------------------------------------------------------------
  check('breakpoints: the boundaries are the SDK WidthBreakpoint numbers',
    m.HARMONY_BP_SM_VP === 320 && m.HARMONY_BP_MD_VP === 600 &&
    m.HARMONY_BP_LG_VP === 840 && m.HARMONY_BP_XL_VP === 1440);
  check('classifier: xs/sm boundary is 320 (319 → xs, 320 → sm)',
    m.widthBreakpointOf(319) === 'xs' && m.widthBreakpointOf(320) === 'sm');
  check('classifier: 599 → sm, 600 → md (the sm/md boundary)',
    m.widthBreakpointOf(599) === 'sm' && m.widthBreakpointOf(600) === 'md');
  check('classifier: 839 → md, 840 → lg (the split boundary)',
    m.widthBreakpointOf(839) === 'md' && m.widthBreakpointOf(840) === 'lg');
  check('classifier: 1439 → lg, 1440 → xl (the lg/xl boundary)',
    m.widthBreakpointOf(1439) === 'lg' && m.widthBreakpointOf(1440) === 'xl');
  check('classifier: a nonsense width falls back to xs (the narrow shell)', (() => {
    return m.widthBreakpointOf(Number.NaN) === 'xs' &&
      m.widthBreakpointOf(-1) === 'xs' && m.widthBreakpointOf(0) === 'xs';
  })());

  // -------------------------------------------------------------------------
  // 2. the RAIL TIER TABLE (0.0.37, I2 point 1): the rail follows the window in
  //    three steps — 240 vp at [840,1024), 280 vp at [1024,1440), 320 vp at
  //    [1440,∞) — and is monotone in the window width.
  // -------------------------------------------------------------------------
  check('rail: the three tier widths are 240 / 280 / 320',
    m.RAIL_WIDTH_COMPACT_VP === 240 && m.RAIL_WIDTH_MEDIUM_VP === 280 &&
    m.RAIL_WIDTH_ROOMY_VP === 320);
  check('rail: the tier boundaries are the split threshold and the official xl boundary',
    m.RAIL_TIER_MID_VP === 1024 && m.HARMONY_BP_XL_VP === 1440);
  check('rail: 840/1023 → compact 240 (the window-wide ⇒ roomier-rail table starts here)',
    m.railWidthFor(840) === 240 && m.railWidthFor(1023) === 240 &&
    m.railTierOf(840) === 'compact' && m.railTierOf(1023) === 'compact');
  check('rail: 1024/1439 → medium 280',
    m.railWidthFor(1024) === 280 && m.railWidthFor(1439) === 280 &&
    m.railTierOf(1024) === 'medium' && m.railTierOf(1439) === 'medium');
  check('rail: 1440/2560 → roomy 320',
    m.railWidthFor(1440) === 320 && m.railWidthFor(2560) === 320 &&
    m.railTierOf(1440) === 'roomy' && m.railTierOf(2560) === 'roomy');
  check('rail: below the split threshold the name/width stay total (compact 240)',
    m.railWidthFor(0) === 240 && m.railWidthFor(Number.NaN) === 240 &&
    m.railTierOf(839) === 'compact');
  check('rail: the width is MONOTONE in the window width (never shrinks as it grows)',
    (() => {
      let previous = 0;
      for (let w = 800; w <= 2600; w += 1) {
        const rail = m.railWidthFor(w);
        if (rail < previous) { return false; }
        previous = rail;
      }
      return true;
    })());

  // -------------------------------------------------------------------------
  // 3. THE SPLIT INVARIANT at every tier boundary: the pane must stay >= the
  //    framework's 600 vp NavigationMode.Auto rule at 840/1023/1024/1439/1440
  //    (the rail jumps 40 vp at 1024 and at 1440, and the pane gives exactly
  //    that up — it must still be far above 600).
  // -------------------------------------------------------------------------
  check('width: the compact rail + Auto split = the lg boundary (240 + 600 = 840)',
    m.RAIL_WIDTH_COMPACT_VP + m.NAV_AUTO_SPLIT_VP === m.HARMONY_BP_LG_VP);
  check('width: 840 → pane 600, exactly the Auto threshold (no rounding gap)',
    m.splitPaneWidthVp(840) === 600 && m.splitPaneWidthVp(840) >= m.NAV_AUTO_SPLIT_VP);
  check('width: 1023 → pane 783 (the last compact window)',
    m.splitPaneWidthVp(1023) === 783 && m.paneSplitValid(1023));
  check('width: 1024 → pane 744 (the rail grows 40, the pane gives 40 up, still valid)',
    m.splitPaneWidthVp(1024) === 744 && m.paneSplitValid(1024));
  check('width: 1439 → pane 1159 (the last medium window)',
    m.splitPaneWidthVp(1439) === 1159 && m.paneSplitValid(1439));
  check('width: 1440 → pane 1120 (the rail grows 40 again, still valid)',
    m.splitPaneWidthVp(1440) === 1120 && m.paneSplitValid(1440));
  check('width: EVERY width in the split shell keeps the pane >= 600 (the invariant)',
    (() => {
      for (let w = m.HARMONY_BP_LG_VP; w <= 4000; w += 1) {
        if (!m.paneSplitValid(w)) { return false; }
        if (m.splitPaneWidthVp(w) < m.NAV_AUTO_SPLIT_VP) { return false; }
      }
      return true;
    })());
  check('width: the pane Navigation keeps the framework defaults it is calibrated on',
    m.NAV_BAR_WIDTH_VP === 240 && m.MIN_CONTENT_WIDTH_VP === 360);
  check('width: the effective split threshold is the lg boundary, not a second number',
    m.splitThresholdVp() === 840 && m.splitThresholdVp() === m.HARMONY_BP_LG_VP);
  check('indent: the xl pane is limited to 1200 and below xl it is unconstrained',
    m.CONTENT_MAX_WIDTH_VP === 1200 && m.contentMaxWidthVp(1440) === 1200 &&
    m.contentMaxWidthVp(2560) === 1200 && m.contentMaxWidthVp(1439) === 0 &&
    m.contentMaxWidthVp(840) === 0);

  // -------------------------------------------------------------------------
  // 4. the pane decision at and around the effective threshold
  // -------------------------------------------------------------------------
  check('pane: 839 → single, 840 → split (the effective threshold is exact)',
    m.paneLayoutOf(839) === 'single' && m.paneLayoutOf(840) === 'split');
  check('pane: the whole md tier keeps today\'s shell (600 → single, 839 → single)',
    m.paneLayoutOf(600) === 'single' && m.paneLayoutOf(720) === 'single');
  check('pane: lg and xl are split (840, 1440, 2560 → split)',
    m.paneLayoutOf(840) === 'split' && m.paneLayoutOf(1440) === 'split' &&
    m.paneLayoutOf(2560) === 'split');
  check('pane: the threshold boundary is off-by-one safe',
    m.paneLayoutOf(m.splitThresholdVp()) === 'split' &&
    m.paneLayoutOf(m.splitThresholdVp() - 1) === 'single');
  check('pane: a nonsense width keeps the narrow shell (never a half-built split)',
    m.paneLayoutOf(Number.NaN) === 'single' && m.paneLayoutOf(0) === 'single');
  check('pane: the arrangement names are the two the page branches on',
    m.PANE_SINGLE === 'single' && m.PANE_SPLIT === 'split');

  // -------------------------------------------------------------------------
  // 5. THE STATE-DRIVEN DEFAULT PANE (0.0.37, I2 point 2) — the pure rule. The
  //    destination numbers are activeTab's numbers: 0 guide, 1 对端, 2 设置.
  // -------------------------------------------------------------------------
  const facts = (activeTab, connected, needsLogin) =>
    ({ activeTab: activeTab, connected: connected, needsLogin: needsLogin });
  check('default: needs-login (not authenticated) with no choice → the GUIDE pane',
    m.widePaneDestination(facts(0, false, true)) === 0 && m.PANE_DEST_GUIDE === 0);
  check('default: 未连接 with no choice → the GUIDE pane',
    m.widePaneDestination(facts(0, false, false)) === 0);
  check('default: connected with no choice → the 对端 list',
    m.widePaneDestination(facts(0, true, false)) === 1 && m.PANE_DEST_PEERS === 1);
  check('default: a needs-login flag does NOT beat a full tunnel (connected wins the default)',
    m.widePaneDestination(facts(0, true, true)) === 1);
  check('default: the guide kind is the login guide exactly when needs-login is set',
    m.wideGuideKind(facts(0, false, true)) === m.GUIDE_LOGIN &&
    m.wideGuideKind(facts(0, false, false)) === m.GUIDE_CONNECT &&
    m.GUIDE_LOGIN === 'login' && m.GUIDE_CONNECT === 'connect');
  check('explicit: 对端 (activeTab 1) sticks for the session whatever the state does',
    m.widePaneDestination(facts(1, false, false)) === 1 &&
    m.widePaneDestination(facts(1, true, false)) === 1 &&
    m.widePaneDestination(facts(1, false, true)) === 1);
  check('explicit: 设置 (activeTab 2) sticks for the session whatever the state does',
    m.widePaneDestination(facts(2, false, false)) === 2 &&
    m.widePaneDestination(facts(2, true, false)) === 2 &&
    m.widePaneDestination(facts(2, false, true)) === 2);
  check('fresh start: activeTab 0 before any choice IS the default-applies case',
    m.widePaneDestination(facts(0, false, false)) === 0 &&
    m.widePaneDestination(facts(0, true, false)) === 1);

  // -------------------------------------------------------------------------
  // 6. the visual scale and the label/value width policy (0.0.37, I2 point 3)
  // -------------------------------------------------------------------------
  check('type: app 22 > page 20 > section 16 > body 15 >= caption 14 (PC floor)',
    m.TYPE_APP_TITLE_VP === 22 && m.TYPE_PAGE_TITLE_VP === 20 &&
    m.TYPE_SECTION_TITLE_VP === 16 && m.TYPE_BODY_VP === 15 && m.TYPE_CAPTION_VP === 14);
  check('type: body is inside the 14–16 body band and the caption meets both floors',
    m.TYPE_BODY_VP >= 14 && m.TYPE_BODY_VP <= 16 && m.TYPE_CAPTION_VP >= 14 &&
    m.TYPE_PAGE_TITLE_VP >= 20 && m.TYPE_PAGE_TITLE_VP <= 22);
  check('spacing: the rail paddings are on the 8 vp grid (16 / 16)',
    m.RAIL_PAGE_PADDING_VP === 16 && m.RAIL_CARD_PADDING_VP === 16 &&
    m.RAIL_PAGE_PADDING_VP % 8 === 0 && m.RAIL_CARD_PADDING_VP % 8 === 0);
  check('label/value: the value column is the rail minus both paddings, at every tier',
    m.railInfoValueWidthVp(840) === 240 - 2 * 16 - 2 * 16 &&
    m.railInfoValueWidthVp(1024) === 280 - 2 * 16 - 2 * 16 &&
    m.railInfoValueWidthVp(1440) === 320 - 2 * 16 - 2 * 16);
  check('label/value: the narrowest value column still clears the no-break floor',
    m.railInfoValueWidthVp(840) >= m.RAIL_INFO_MIN_VALUE_VP &&
    m.RAIL_INFO_MIN_VALUE_VP >= 160,
    `compact value column=${m.railInfoValueWidthVp(840)} floor=${m.RAIL_INFO_MIN_VALUE_VP}`);
  check('label/value: the value column never shrinks as the window grows',
    m.railInfoValueWidthVp(840) <= m.railInfoValueWidthVp(1024) &&
    m.railInfoValueWidthVp(1024) <= m.railInfoValueWidthVp(1440));

  // -------------------------------------------------------------------------
  // 7. the narrow branch is still the narrow branch (textual, on Index.ets)
  // -------------------------------------------------------------------------
  check('Index: today\'s narrow Tabs is still there, unchanged',
    PAGE.includes('Tabs({ barPosition: BarPosition.End, index: this.activeTab })') &&
    PAGE.includes(".barMode(BarMode.Fixed)") &&
    PAGE.includes("// leaving the peers tab drops the drill-down selection"));
  check('Index: the layout never asks what the DEVICE is (no deviceInfo.deviceType)',
    !PAGE.includes('deviceInfo.deviceType'));
  check('Index: the width is observed, not assumed (seed + root onAreaChange)',
    PAGE.includes('this.seedWidthVp();') &&
    PAGE.includes('private onRootAreaChange(newArea: Area): void') &&
    (PAGE.match(/\.onAreaChange\(/g) ?? []).length === 2);
  check('Index: the classifier decides the shell, and the split branch is rendered',
    PAGE.includes('if (paneLayoutOf(this.widthVp) === \'split\') {') &&
    PAGE.includes('this.wideShell()') && PAGE.includes('this.connectionRail()') &&
    PAGE.includes('this.contentPane()'));

  // -------------------------------------------------------------------------
  // 8. the RESPONSIVE RAIL is wired to the pure tier table (I2 point 1)
  // -------------------------------------------------------------------------
  check('rail: the rail width is the pure table\'s answer, not a constant',
    PAGE.includes('.width(this.railWidth())') &&
    PAGE.includes('private railWidth(): number {') &&
    PAGE.includes('return railWidthFor(this.widthVp);') &&
    !PAGE.includes('.width(RAIL_WIDTH_VP)'));
  check('rail: the page imports the tier table + the state/scale decisions from the module',
    PAGE.includes('railWidthFor') && PAGE.includes('PANE_DEST_GUIDE') &&
    PAGE.includes('widePaneDestination') && PAGE.includes('contentMaxWidthVp') &&
    PAGE.includes("from './NetBirdHarmonyLayout';"));
  check('rail: the nav block is 48 vp rows with a selected accent bar and group caption',
    PAGE.includes(".height(48)") && PAGE.includes('.width(3)') &&
    PAGE.includes("Text('导航')") && PAGE.includes('PANE_DEST_PEERS') &&
    PAGE.includes('PANE_DEST_SETTINGS'));
  check('rail: pointer + keyboard affordances on the PC (hover, focus, tab order, Enter)',
    (PAGE.match(/\.onHover\(/g) ?? []).length === 2 &&
    (PAGE.match(/\.tabIndex\(/g) ?? []).length === 2 &&
    PAGE.includes('.hoverEffect(HoverEffect.Highlight)') &&
    PAGE.includes('private isActivationKey(event: KeyEvent): boolean') &&
    PAGE.includes("from '@ohos.multimodalInput.keyCode'") &&
    (PAGE.match(/\.onKeyEvent\(/g) ?? []).length >= 3);
  check('rail: the indent limit is applied to the pane, not to the rail',
    PAGE.includes('private paneConstraint(): ConstraintSizeOptions {') &&
    PAGE.includes('.constraintSize(this.paneConstraint())') &&
    PAGE.includes("return { maxWidth: max > 0 ? max : '100%' };") &&
    PAGE.includes('const max: number = contentMaxWidthVp(this.widthVp);'));
  check('rail: the round connect control is kept, sized to the rail on the wide branch',
    PAGE.includes('private connectButtonSize(): number {') &&
    PAGE.includes('return this.wideLayout() ? 104 : 132;') &&
    PAGE.includes('.width(this.connectButtonSize())') && (() => {
      // Scoped to the rail's own block: the 后台保活 card elsewhere on the page
      // legitimately uses a Toggle (a binary setting), the CONNECT control must not.
      const rail = PAGE.slice(PAGE.indexOf('connectionRail() {'),
        PAGE.indexOf('railDestinations() {'));
      return rail.includes('this.homeTab()') && !rail.includes('Toggle(') &&
        !rail.includes('ToggleType.Switch');
    })());

  // -------------------------------------------------------------------------
  // 9. the wide pane re-parents today's builders — it does not re-implement them
  //    (one render site per body, so nothing can drift)
  // -------------------------------------------------------------------------
  check('pane: the rail hosts TODAY\'S home body',
    PAGE.includes('this.homeTab()') &&
    PAGE.indexOf('this.homeTab()') > PAGE.indexOf('connectionRail() {'));
  check('pane: the navBar hosts TODAY\'S peer list body and the detail is a NavDestination',
    PAGE.includes('Navigation(this.peersNavStack) {') &&
    PAGE.includes('this.peerListBody()') &&
    PAGE.includes('.navDestination(this.peerPaneDestination)') &&
    PAGE.includes('NavDestination() {') &&
    PAGE.includes('this.peerDetailBody()'));
  check('pane: the peer list is rendered from ONE ForEach site (no second key to drift)',
    (PAGE.match(/ForEach\(this\.visiblePeers\(\)/g) ?? []).length === 1 &&
    (PAGE.match(/\}, \(peer: UiPeerEntry\) => peerRowKey\(peer\)\)/g) ?? []).length === 1);
  check('pane: the 设置 body is hosted in its own single-column Navigation',
    PAGE.includes('Navigation(this.settingsNavStack) {') &&
    PAGE.includes('this.settingsTab()') &&
    PAGE.includes('.mode(NavigationMode.Stack)'));
  check('pane: 设置 does NOT hide the navigation page (hideNavBar(true) would blank it)',
    PAGE.includes('.hideNavBar(false)') && !PAGE.includes('.hideNavBar(true)'));
  check('pane: the pane Navigation splits on the framework\'s own rule',
    PAGE.includes('.mode(NavigationMode.Auto)') &&
    PAGE.includes('.navBarWidth(NAV_BAR_WIDTH_VP)') &&
    PAGE.includes('.minContentWidth(MIN_CONTENT_WIDTH_VP)'));
  check('pane: the content area is never blank (a base route, ensured on appear)',
    PAGE.includes("const PEER_PANE_HOME_ROUTE: string = 'peerPaneHome';") &&
    PAGE.includes("const PEER_DETAIL_ROUTE: string = 'peerDetail';") &&
    PAGE.includes('private ensurePeerPaneBaseRoute(): void {') &&
    PAGE.includes('.onAppear((): void => {') &&
    PAGE.includes('this.ensurePeerPaneBaseRoute();'));

  // -------------------------------------------------------------------------
  // 10. the STATE-DRIVEN DEFAULT is what the pane renders (I2 point 2) — the
  //     wiring, and the removal of the I1 placeholder line
  // -------------------------------------------------------------------------
  check('default: the pane content is chosen by the resolved destination, in the rule\'s order',
    PAGE.includes('private wideDestination(): number {') &&
    PAGE.includes('if (this.wideDestination() === PANE_DEST_PEERS) {') &&
    PAGE.includes('} else if (this.wideDestination() === PANE_DEST_SETTINGS) {') &&
    PAGE.indexOf('if (this.wideDestination() === PANE_DEST_PEERS) {') <
      PAGE.indexOf('} else if (this.wideDestination() === PANE_DEST_SETTINGS) {'));
  check('default: the I1 placeholder pane is gone, replaced by the state guide',
    !PAGE.includes('wideHintPane') &&
    !PAGE.includes("Text('选择左侧的「对端」或「设置」')") &&
    PAGE.includes('this.wideGuidePane()') &&
    PAGE.includes('\n  wideGuidePane() {'));
  check('default: the guide renders the live state and the recorded failure reason',
    PAGE.includes('Text(this.configured ? this.stateText() : \'尚未配置服务器\')') &&
    PAGE.includes('this.lastErrorLine() !== this.stateText()') &&
    PAGE.includes('.fill(this.stateColor())'));
  check('default: the needs-login guide reuses the EXISTING connect-login flow',
    PAGE.includes("if (this.wideGuideKind() === GUIDE_LOGIN) {") &&
    PAGE.includes("Button(this.accountLoginBusy ? '正在登录…' : '账户登录')") &&
    PAGE.includes('this.beginConnectLogin();') &&
    PAGE.includes("Button('去设置')"));
  check('default: the peer pane is the RESOLVED one wherever it is gated',
    PAGE.includes('private peerPaneVisible(): boolean {') &&
    PAGE.includes('if (this.peerPaneVisible()) {') &&
    PAGE.includes('if (!this.peerPaneVisible()) {'));
  check('default: the empty peer pane is a real block with ONE relevant action',
    PAGE.includes('struct PaneEmptyState {') &&
    PAGE.includes("title: 'VPN 未连接'") &&
    PAGE.includes("tone: 'warning'") &&
    PAGE.includes("actionLabel: this.peerQuery.length > 0 ? '清除搜索' : ''") &&
    PAGE.includes("actionLabel: '去连接'"));
  check('default: the list header is stacked with a one-line capped count',
    (() => {
      const from = PAGE.indexOf("Text('对端')");
      const header = PAGE.slice(from, PAGE.indexOf('TextInput({ text: this.peerQuery', from));
      return from > 0 && header.includes('已连接') && header.includes('.maxLines(1)') &&
        header.includes('textOverflow');
    })());

  // -------------------------------------------------------------------------
  // 11. the label/value rows are STACKED (I2 point 3 first bullet) — the I1
  //     device defect was a fixed 96 vp label column squeezing "0/0 已连接"
  //     into 已连/接
  // -------------------------------------------------------------------------
  check('rows: the rail\'s three info rows are the stacked RailInfoRow component',
    (PAGE.match(/RailInfoRow\(\{/g) ?? []).length === 3 &&
    PAGE.includes('struct RailInfoRow {'));
  check('rows: that component paints the label ABOVE the value, each full width',
    (() => {
      const body = PAGE.slice(PAGE.indexOf('struct RailInfoRow {'),
        PAGE.indexOf('// 0.0.37 (I2) — THE PANE\'S EMPTY / ERROR STATE'));
      return body.includes("Text(this.label)") &&
        body.indexOf("Text(this.label)") < body.indexOf('Text(this.value ? this.value') &&
        (body.match(/\.width\('100%'\)/g) ?? []).length >= 3;
    })());
  check('rows: the peer row keeps the same rendered fields (peerRowKey must not drift)',
    PAGE.includes('Text(peer.name)') &&
    PAGE.includes("Text(peer.ip.length > 0 ? peer.ip : '无 IP')") &&
    PAGE.includes('connectionLabel(peer.connection)') &&
    PAGE.includes('peerStatusText(peer.status)') &&
    !PAGE.includes('last_handshake_text'));

  // -------------------------------------------------------------------------
  // 12. ONE back authority on the wide branch (k3 correction 4 / §E R6), and the
  //     rail's destination rule (k3 correction 2, extended by I2)
  // -------------------------------------------------------------------------
  const closeHandler = PAGE.slice(PAGE.indexOf('private closeVisibleSubflow(): boolean'),
    PAGE.indexOf('onBackPress(): boolean'));
  check('back: the pane route is closed inside the ONE close handler',
    closeHandler.includes('if (this.closeWidePaneRoute()) {') &&
    PAGE.includes('onBackPress(): boolean {\n    return this.closeVisibleSubflow();\n  }'));
  check('back: the pane authority is behind the account/editor actions and ahead of the page closes',
    closeHandler.indexOf("if (action === 'cancel-editor')") <
      closeHandler.indexOf('this.closeWidePaneRoute()') &&
    closeHandler.indexOf('this.closeWidePaneRoute()') <
      closeHandler.indexOf("action === 'close-diagnostics'"));
  check('back: the narrow peer fact is preserved AND the resolved pane is added to it',
    closeHandler.includes('const peerDetailOpen: boolean = (this.activeTab === 1 && this.selectedPeer !== null) ||') &&
    closeHandler.includes('(this.peerPaneVisible() && this.selectedPeer !== null);') &&
    closeHandler.includes('peerDetailOpen,'));
  check('back: only the VISIBLE destination\'s stack is consulted, and only for a real sub-view',
    PAGE.includes('this.peerPaneVisible() && this.selectedPeer !== null && this.peersNavStack.size() > 0') &&
    PAGE.includes('this.wideDestination() === PANE_DEST_SETTINGS && this.settingsNavStack.size() > 0'));
  check('back: the wide branch has its own stack per destination (one Navigation each)',
    PAGE.includes('private peersNavStack: NavPathStack = new NavPathStack();') &&
    PAGE.includes('private settingsNavStack: NavPathStack = new NavPathStack();'));
  check('rail: two destination items only, and activeTab===0 means "no choice yet"',
    PAGE.includes('private selectWideDestination(index: number): void') &&
    PAGE.includes('this.wideDestination() === PANE_DEST_PEERS'));
} catch (error) {
  failed++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
