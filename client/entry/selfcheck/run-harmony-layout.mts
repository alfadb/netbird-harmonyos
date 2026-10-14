// Host-side checks for the responsive shell — DESIGN-HARMONY I1 (0.0.36), I2
// (0.0.37), I3 (0.0.38) and I4 (0.0.39). No device, no build.
//
// Run: node client/entry/selfcheck/run-harmony-layout.mts
//
// Two halves, both pinned against the byte-identical production sources:
//   * the DECISIONS in pages/NetBirdHarmonyLayout.ets — the official
//     `WidthBreakpoint` boundaries, the RAIL TIER TABLE (0.0.37: 240/280/320 vp
//     by window tier) with the pane-split invariant asserted at every tier
//     boundary, the pane decision, the xl indent limit, the STATE-DRIVEN DEFAULT
//     PANE rule (I2) and the visual scale / label-value row width policy;
//     0.0.38 (I3) adds the PEER LIST COLUMN width (with "the IP never wraps" and
//     "a real name fits" as arithmetic), the two-line row's geometry, the 连接概览
//     pane's ROW POLICY (which rows exist for which document), the peer detail's
//     DEFAULT/ADVANCED classifier, the settings column clamp and the rail's
//     connection-region cap;
//     0.0.39 (I4) makes the list column a TIER TABLE with its own per-tier
//     reservation of the detail pane, states the name floor as "a 28-character
//     fqdn at 14 vp" and sweeps EVERY window of the shell against it, replaces the
//     overview policy with the review's five-item pane (hero / tiles / relay /
//     offline / conditional error, plus the nothing-to-show row), replaces the
//     detail's default/advanced split with FOUR named sections + the 域名
//     duplicate rule, and pins the disc's fill/ring split, the rail's one text
//     axis, the rail seam, the settings 680 cap / 240 button and the typography
//     floor;
//   * the WIRING in pages/Index.ets — that the narrow branch's Tabs is still
//     there, that the split branch exists and re-parents today's builders instead
//     of re-implementing them, that the layout never reads
//     deviceInfo.deviceType, that the wide pane has exactly ONE back authority,
//     that the pane's content is chosen by the resolved destination (not raw
//     activeTab), and that the rail's rows cannot break a word.
//
// run-peer-list-render.mts stays green next to this file: the peer row still
// renders EXACTLY the fields peerRowKey carries (name / ip / online / connection /
// status — the IP moved to its own line, and I4's 离线对端 last-seen text is the
// one field that JOINED the key). The DELIBERATE updates this file needed are
// marked in place.
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
  // 0.0.40 (I5) §3: the peer-row KEY is executed here too, because the selected
  // state had to join it — the same byte-identical fixture run-peer-list-render.mts
  // uses.
  const modelFixture = join(dir, 'NetBirdUiModel.ts');
  writeFileSync(modelFixture,
    readFileSync(join(ets, 'vpnextensionability', 'NetBirdUiModel.ets')));
  const ui = await import(pathToFileURL(modelFixture).href);

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
  // 0.0.40 (I5) DELIBERATE UPDATE (review §6): the 导航 CAPTION is gone — it named a
  // two-row group and the 1 vp boundary above already separates the regions. The
  // property this check exists to protect is unchanged: two 48 vp rows, each with a
  // 3 vp selected accent bar, driven by the resolved destination. The row height is
  // now the module's own number, which is also what the connection region's height
  // budget measures its ceiling against (RAIL_DESTINATIONS_HEIGHT_VP).
  check('rail: the nav block is 48 vp rows with a selected accent bar and NO caption',
    m.RAIL_NAV_ROW_HEIGHT_VP === 48 && PAGE.includes('.height(RAIL_NAV_ROW_HEIGHT_VP)') &&
    PAGE.includes('.width(RAIL_NAV_BAR_WIDTH_VP)') &&
    !PAGE.includes("Text('导航')") && PAGE.includes('PANE_DEST_PEERS') &&
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
  // 0.0.39 (I4): the peer-row KEY now serves TWO sites — the list's own ForEach and
  // the 连接概览 pane's 离线对端 rows — and both go through the same peerRowKey, so
  // there is still exactly one place a rendered field has to be declared. The list
  // itself is still one site.
  // 0.0.40 (I5) DELIBERATE UPDATE (§3): the LIST site now passes the row's selected
  // state into the key as peerRowKey's second argument — the selected marker/tint is
  // a rendered field, and a key that ignored it would keep the node of the row the
  // user just left selected. The 连接概览 rows are not selectable and keep the
  // one-argument call (i.e. selected=false).
  check('pane: the list is ONE ForEach site and EVERY peer row keys on peerRowKey',
    (PAGE.match(/ForEach\(this\.visiblePeers\(\)/g) ?? []).length === 1 &&
    (PAGE.match(/\}, \(peer: UiPeerEntry\) => peerRowKey\(peer\)\)/g) ?? []).length === 1 &&
    (PAGE.match(/\}, \(peer: UiPeerEntry\) => peerRowKey\(peer, this\.peerRowSelected\(peer\)\)\)/g)
      ?? []).length === 1);
  check('pane: the 设置 body is hosted in its own single-column Navigation',
    PAGE.includes('Navigation(this.settingsNavStack) {') &&
    PAGE.includes('this.settingsTab()') &&
    PAGE.includes('.mode(NavigationMode.Stack)'));
  check('pane: 设置 does NOT hide the navigation page (hideNavBar(true) would blank it)',
    PAGE.includes('.hideNavBar(false)') && !PAGE.includes('.hideNavBar(true)'));
  check('pane: the pane Navigation splits on the framework\'s own rule',
    PAGE.includes('.mode(NavigationMode.Auto)') &&
    // 0.0.38 (I3) DELIBERATE UPDATE: the navBar is no longer the framework's
    // 240 vp default — I3's C widens the list column to what the pane can spare
    // above this same minContentWidth. The Auto rule this check protects is
    // unchanged, and §13 below asserts the NEW width still honours it at every
    // window of the shell.
    // 0.0.39 (I4) DELIBERATE UPDATE: the reservation itself is TIERED now (320 at
    // the compact tier, 360 above it) — the framework derives the real navBarWidth
    // from it, so §13 asserts the tiered value is what keeps the split honest.
    PAGE.includes('.navBarWidth(this.peerListNavBarWidth())') &&
    PAGE.includes('.minContentWidth(this.peerContentMinWidth())'));
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
  // 0.0.40 (I5) DELIBERATE UPDATE (review §1 + §6): the rail's rows are now THREE
  // RailInfoRow sites — NetBird IP always, 域名 / 对端 on the phone only (the PC rail
  // defers them to the overview pane and the list's own header) — and the 流量 row is
  // no longer a RailInfoRow at all: it is a hand-written label + two stacked value
  // lines, because the review's §1 requires the two counters to be unable to
  // ellipsise and a single RailInfoRow value line cannot promise that. The property
  // this check exists to protect is unchanged: every rail info row is the STACKED
  // label-above-value shape, never a fixed-label-column Row, so no value can be
  // squeezed into a mid-word break.
  check('rows: the rail\'s three RailInfoRows are the stacked component, phone-only pair',
    (PAGE.match(/RailInfoRow\(\{/g) ?? []).length === 3 &&
    PAGE.includes('struct RailInfoRow {') &&
    PAGE.includes('if (!this.wideLayout()) {') &&
    PAGE.includes('if (this.wideLayout() && this.railTrafficRowVisible()) {'));
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

  // -------------------------------------------------------------------------
  // 13. 0.0.39 (I4) P0.2 — THE TIERED PEER LIST COLUMN and its per-tier
  //     reservation of the detail pane (the review's §P0.1 arithmetic)
  // -------------------------------------------------------------------------
  const pane = (w: number) => m.splitPaneWidthVp(w);
  check('peer list: the column is a TIER TABLE — one cap per rail tier (360/440/480)',
    m.PEER_LIST_NAV_BAR_CAP_COMPACT_VP === 360 &&
    m.PEER_LIST_NAV_BAR_CAP_MEDIUM_VP === 440 &&
    m.PEER_LIST_NAV_BAR_CAP_ROOMY_VP === 480 &&
    m.peerListNavBarCapVp(840) === 360 && m.peerListNavBarCapVp(1023) === 360 &&
    m.peerListNavBarCapVp(1024) === 440 && m.peerListNavBarCapVp(1439) === 440 &&
    m.peerListNavBarCapVp(1440) === 480 && m.peerListNavBarCapVp(4000) === 480);
  check('peer list: the compact tier reserves 320 for the detail, the rest 360',
    m.PEER_LIST_CONTENT_MIN_COMPACT_VP === 320 && m.MIN_CONTENT_WIDTH_VP === 360 &&
    m.peerContentMinWidthVp(840) === 320 && m.peerContentMinWidthVp(1023) === 320 &&
    m.peerContentMinWidthVp(1024) === 360 && m.peerContentMinWidthVp(1440) === 360);
  check('peer list: 840 vp splits the 600 vp pane 280 list / 320 detail (not 240/360)',
    pane(840) === 600 && m.peerListNavBarWidthFor(840) === 280 &&
    m.peerListNavBarWidthFor(840) + m.peerContentMinWidthVp(840) === pane(840));
  check('peer list: it takes what the pane can spare, capped, at every tier',
    m.peerListNavBarWidthFor(1023) === 360 && m.peerListNavBarWidthFor(1024) === 384 &&
    m.peerListNavBarWidthFor(1250) === 440 && m.peerListNavBarWidthFor(1440) === 480 &&
    m.peerListNavBarWidthFor(4000) === 480);
  check('peer list: THE INVARIANT — column + the tier\'s reservation fits the pane, always',
    (() => {
      for (let w = m.HARMONY_BP_LG_VP; w <= 4000; w += 1) {
        const bar: number = m.peerListNavBarWidthFor(w);
        if (bar < m.NAV_BAR_WIDTH_VP) { return false; }
        if (bar + m.peerContentMinWidthVp(w) > m.splitPaneWidthVp(w)) { return false; }
        if (m.splitPaneWidthVp(w) < m.NAV_AUTO_SPLIT_VP) { return false; }
      }
      return true;
    })());
  check('peer list: the column never shrinks as the window grows',
    (() => {
      let previous = m.peerListNavBarWidthFor(m.HARMONY_BP_LG_VP - 1);
      for (let w = m.HARMONY_BP_LG_VP; w <= 4000; w += 1) {
        const bar = m.peerListNavBarWidthFor(w);
        if (bar < previous) { return false; }
        previous = bar;
      }
      return true;
    })());
  check('peer list: a nonsense pane/window width answers the framework default, never NaN',
    m.peerListNavBarWidthVp(Number.NaN, Number.NaN) === 240 &&
    m.peerListNavBarWidthVp(0, 0) === 240 && m.peerListNavBarWidthFor(Number.NaN) === 240 &&
    m.peerListNavBarWidthFor(0) === 240);

  // -------------------------------------------------------------------------
  // 14. 0.0.39 (I4) P0.2 — THE TWO-LINE ROW: the IP can never wrap and a real
  //     28-character fqdn fits at EVERY wide tier (the floor the review states)
  // -------------------------------------------------------------------------
  check('peer row: the row is the 64 vp two-line form, its chrome budgeted per tier',
    m.PEER_ROW_MIN_HEIGHT_VP === 64 && m.PEER_ROW_LIST_PADDING_VP === 12 &&
    m.PEER_ROW_META_WIDE_VP === 112 && m.PEER_ROW_META_COMPACT_VP === 18 &&
    m.PEER_ROW_L2_META_COMPACT_VP === 88 && m.PEER_ROW_IP_MIN_WIDTH_VP === 120);
  check('peer row: the name floor is a 28-character fqdn at 14 vp (28 × 7 = 196)',
    m.PEER_ROW_FQDN_CHARS === 28 && m.PEER_ROW_NAME_CHAR_VP === 7 &&
    m.PEER_ROW_NAME_MIN_WIDTH_VP === 196);
  // 0.0.40 (I5) DELIBERATE UPDATE (§3): the selected row's 3 vp left marker is part
  // of every row's budget (it is always reserved, transparent when unselected, so
  // selecting a peer cannot shift the text), which is why it is subtracted below —
  // and it is the SAME 3 vp the rail's destination rows carry.
  check('peer row: the selected marker is the rail\'s own 3 vp bar, in the text budget',
    m.PEER_ROW_SELECT_BAR_VP === 3 && m.PEER_ROW_SELECT_BAR_VP === m.RAIL_NAV_BAR_WIDTH_VP);
  check('peer row: line 2 (the IP) owns the row minus page+card padding+select marker',
    m.peerRowTextWidthVp(280) === 280 - 2 * 16 - 2 * 12 - m.PEER_ROW_SELECT_BAR_VP &&
    m.peerRowTextWidthVp(480) === 480 - 2 * 16 - 2 * 12 - m.PEER_ROW_SELECT_BAR_VP);
  check('peer row: THE IP NEVER WRAPS — line 2 clears the dotted-quad floor in the whole range',
    (() => {
      for (let bar = m.NAV_BAR_WIDTH_VP; bar <= m.PEER_LIST_NAV_BAR_CAP_ROOMY_VP; bar += 1) {
        if (m.peerRowTextWidthVp(bar) < m.PEER_ROW_IP_MIN_WIDTH_VP) { return false; }
      }
      return true;
    })());
  check('peer row: the compact tier CONDENSES line 1 (chip + status ride line 2)',
    m.peerRowCondensed(840) && m.peerRowCondensed(1023) && !m.peerRowCondensed(1024) &&
    !m.peerRowCondensed(1440) && !m.peerRowCondensed(600) && !m.peerRowCondensed(320) &&
    m.peerRowMetaWidthVp(840) === 18 && m.peerRowMetaWidthVp(1024) === 112);
  check('peer row: THE 28-CHARACTER FQDN FITS AT EVERY WINDOW OF THE SHELL',
    (() => {
      for (let w = m.HARMONY_BP_LG_VP; w <= 4000; w += 1) {
        if (m.peerRowNameWidthFor(w) < m.PEER_ROW_NAME_MIN_WIDTH_VP) { return false; }
      }
      return true;
    })(),
    `min at 840 = ${m.peerRowNameWidthFor(840)} floor = ${m.PEER_ROW_NAME_MIN_WIDTH_VP}`);
  check('peer row: the name column at every tier bottom and the measured I3 window',
    m.peerRowNameWidthFor(840) === 280 - 2 * 16 - 2 * 12 - m.PEER_ROW_SELECT_BAR_VP - 18 &&
    m.peerRowNameWidthFor(1024) === 384 - 2 * 16 - 2 * 12 - m.PEER_ROW_SELECT_BAR_VP - 112 &&
    m.peerRowNameWidthFor(1250) === 440 - 2 * 16 - 2 * 12 - m.PEER_ROW_SELECT_BAR_VP - 112 &&
    m.peerRowNameWidthFor(1440) === 480 - 2 * 16 - 2 * 12 - m.PEER_ROW_SELECT_BAR_VP - 112,
    `840→${m.peerRowNameWidthFor(840)} 1024→${m.peerRowNameWidthFor(1024)} ` +
    `1250→${m.peerRowNameWidthFor(1250)} 1440→${m.peerRowNameWidthFor(1440)}`);
  check('peer row: the compact line 2 still holds the IP, the chip AND the status text',
    (() => {
      for (let w = m.HARMONY_BP_LG_VP; w <= 1023; w += 1) {
        const text: number = m.peerRowTextWidthVp(m.peerListNavBarWidthFor(w));
        if (text < m.PEER_ROW_IP_MIN_WIDTH_VP + m.PEER_ROW_L2_META_COMPACT_VP) { return false; }
      }
      return true;
    })());
  check('peer search: capped, never wider than the column, degenerate width is 0',
    m.PEER_SEARCH_MAX_WIDTH_VP === 320 && m.peerSearchWidthVp(480) === 320 &&
    m.peerSearchWidthVp(280) === 248 && m.peerSearchWidthVp(0) === 0 &&
    m.peerSearchWidthVp(Number.NaN) === 0);

  // -------------------------------------------------------------------------
  // 15. 0.0.39 (I4) P0.4 — THE 连接概览 PANE's ROW POLICY (which rows EXIST)
  // -------------------------------------------------------------------------
  const ov = (over: object) => Object.assign(
    { hasAddress: true, hasFqdn: true, peerCount: 3, offlineCount: 2,
      hasRelayInUse: true, hasError: false }, over);
  check('overview: the full document gets hero + tiles + relay + offline, in ONE order',
    JSON.stringify(m.overviewRowKeys(ov({}))) === JSON.stringify(
      ['address', 'fqdn', 'connected', 'direct', 'relay', 'relayInUse', 'offline']));
  check('overview: the recorded error is the LAST row, and only when one exists',
    (() => {
      const withError = m.overviewRowKeys(ov({ hasError: true }));
      return withError[withError.length - 1] === 'error' &&
        m.overviewRowKeys(ov({ hasError: false })).indexOf('error') < 0;
    })());
  check('overview: no own address / no fqdn ⇒ those rows are omitted',
    m.overviewRowKeys(ov({ hasAddress: false })).indexOf('address') < 0 &&
    m.overviewRowKeys(ov({ hasFqdn: false })).indexOf('fqdn') < 0);
  check('overview: no peers at all ⇒ NO tiles (never an invented zero), identity stays',
    JSON.stringify(m.overviewRowKeys(ov({ peerCount: 0, offlineCount: 0 }))) === JSON.stringify(
      ['address', 'fqdn', 'relayInUse']));
  check('overview: no relay in use / nobody offline ⇒ those rows are omitted',
    m.overviewRowKeys(ov({ hasRelayInUse: false })).indexOf('relayInUse') < 0 &&
    m.overviewRowKeys(ov({ offlineCount: 0 })).indexOf('offline') < 0);
  check('overview: nothing at all to show ⇒ the ONE honest row, never an empty pane',
    JSON.stringify(m.overviewRowKeys({ hasAddress: false, hasFqdn: false, peerCount: 0,
      offlineCount: 0, hasRelayInUse: false, hasError: false })) === JSON.stringify(['none']));
  check('overview: there is NO 连接时长 row anywhere (the document carries no such stamp)',
    JSON.stringify(m.overviewRowKeys(ov({}))).indexOf('duration') < 0 &&
    !PAGE.includes("label: '连接时长'") && !PAGE.includes("Text('连接时长')") &&
    !PAGE.includes("'连接时长'"));
  check('overview: the offline list is capped at 5 and the tile geometry is the review\'s',
    m.OVERVIEW_OFFLINE_MAX_ROWS === 5 && m.OVERVIEW_TILE_HEIGHT_VP === 96 &&
    m.TYPE_STAT_VALUE_VP === 24);

  // -------------------------------------------------------------------------
  // 16. 0.0.39 (I4) P1.7 — THE PEER DETAIL's FOUR SECTIONS
  // -------------------------------------------------------------------------
  check('detail: the 连接 section is the session the user came for',
    JSON.stringify(m.PEER_DETAIL_CONNECTION_LABELS) === JSON.stringify(
      ['域名', 'NetBird IP', '状态', '连接方式', '最近握手', '发送', '接收']));
  check('detail: 路径 is the endpoints and the bearer',
    JSON.stringify(m.PEER_DETAIL_PATH_LABELS) === JSON.stringify(
      ['对端端点', '本机出口', '对端网络', '承载', '中继通道地址', '本机中继会话']));
  check('detail: ICE is the six candidate-path rows (说明 is a caption, not a row)',
    JSON.stringify(m.PEER_DETAIL_ICE_LABELS) === JSON.stringify(
      ['ICE', 'ICE 状态', 'ICE 角色', 'ICE 本端', 'ICE 对端', 'ICE 可达性', 'ICE 最近错误']) &&
    m.PEER_DETAIL_ICE_LABELS.indexOf('ICE 说明') < 0);
  check('detail: 标识 is the public key alone',
    JSON.stringify(m.PEER_DETAIL_IDENTITY_LABELS) === JSON.stringify(['公钥']));
  check('detail: the classifier maps every label to exactly ONE of the four sections',
    m.peerDetailSectionOf('状态') === m.PEER_DETAIL_CONNECTION &&
    m.peerDetailSectionOf('域名') === m.PEER_DETAIL_CONNECTION &&
    m.peerDetailSectionOf('发送') === m.PEER_DETAIL_CONNECTION &&
    m.peerDetailSectionOf('承载') === m.PEER_DETAIL_PATH &&
    m.peerDetailSectionOf('对端端点') === m.PEER_DETAIL_PATH &&
    m.peerDetailSectionOf('中继通道地址') === m.PEER_DETAIL_PATH &&
    m.peerDetailSectionOf('ICE 状态') === m.PEER_DETAIL_ICE &&
    m.peerDetailSectionOf('ICE 最近错误') === m.PEER_DETAIL_ICE &&
    m.peerDetailSectionOf('公钥') === m.PEER_DETAIL_IDENTITY);
  check('detail: an UNCLASSIFIED label falls into the COLLAPSED 路径 section',
    m.peerDetailSectionOf('某个新行') === m.PEER_DETAIL_PATH &&
    m.peerDetailSectionOf('说明') === m.PEER_DETAIL_PATH);
  check('detail: the four label lists are disjoint (no row can render in two sections)',
    (() => {
      const lists = [m.PEER_DETAIL_CONNECTION_LABELS, m.PEER_DETAIL_PATH_LABELS,
        m.PEER_DETAIL_ICE_LABELS, m.PEER_DETAIL_IDENTITY_LABELS];
      const seen: string[] = [];
      for (const list of lists) {
        for (const label of list) {
          if (seen.indexOf(label) >= 0) { return false; }
          seen.push(label);
        }
      }
      return true;
    })());
  check('detail: the 域名 row is dropped exactly when it repeats the card title',
    m.peerFqdnDuplicatesTitle('blkint.netcenter.local', 'blkint.netcenter.local') &&
    !m.peerFqdnDuplicatesTitle('blkint (手改)', 'blkint.netcenter.local') &&
    !m.peerFqdnDuplicatesTitle('blkint.netcenter.local', '') &&
    !m.peerFqdnDuplicatesTitle('', ''));

  // -------------------------------------------------------------------------
  // 17. 0.0.39 (I4) P0.3 — the settings clamp, the button, the detail cap
  // -------------------------------------------------------------------------
  check('settings: the column cap is 680 (I3 used 840) and bites only from pane > 680',
    m.SETTINGS_CONTENT_MAX_WIDTH_VP === 680 &&
    m.settingsContentMaxWidthVp(680) === 0 && m.settingsContentMaxWidthVp(601) === 0 &&
    m.settingsContentMaxWidthVp(681) === 680 && m.settingsContentMaxWidthVp(2560) === 680 &&
    m.settingsContentMaxWidthVp(Number.NaN) === 0);
  // 0.0.40 (I5) DELIBERATE UPDATE (§2, review §2): the 60 % cap is REPLACED. The
  // property it existed for (the nav group can never be pushed out by the connection
  // region) is now arithmetic against the rail's own measured height: the ceiling is
  // "everything but the bottom region", floored so a very short window still shows
  // the identity block's head and scrolls the rest. A percentage could not see the
  // slack the ASTRA review measured under the card — it clipped the 详细状态 entry
  // with ~132 px of it sitting right there.
  check('rail: the connection region\'s ceiling is the rail minus its bottom region',
    m.RAIL_DESTINATIONS_HEIGHT_VP ===
      m.RAIL_SEAM_WIDTH_VP + m.RAIL_DESTINATIONS_PAD_TOP_VP + m.RAIL_NAV_ROW_HEIGHT_VP +
        m.RAIL_NAV_ROW_GAP_VP + m.RAIL_NAV_ROW_HEIGHT_VP + m.RAIL_DESTINATIONS_PAD_BOTTOM_VP &&
    m.RAIL_DESTINATIONS_HEIGHT_VP === 121 &&
    m.railConnectionMaxHeightVp(833) === 833 - 121 &&
    m.railConnectionMaxHeightVp(1080) === 1080 - 121 &&
    m.railConnectionMaxHeightVp(400) === 400 - 121 &&
    m.railConnectionMaxHeightVp(300) === m.RAIL_CONNECTION_MIN_VP &&
    m.railConnectionMaxHeightVp(0) === m.RAIL_CONNECTION_MIN_VP &&
    m.railConnectionMaxHeightVp(Number.NaN) === m.RAIL_CONNECTION_MIN_VP &&
    m.RAIL_CONNECTION_MAX_HEIGHT_PCT === undefined);
  check('buttons: the inline (non-primary) button is the review\'s ~240, not a bar',
    m.BUTTON_INLINE_WIDTH_VP === 240);
  check('detail: the detail card is capped at 720 vp',
    m.DETAIL_CONTENT_MAX_WIDTH_VP === 720 &&
    m.OVERVIEW_CONTENT_MAX_WIDTH_VP === 720);

  // -------------------------------------------------------------------------
  // 17b. 0.0.39 (I4) P1.8 — THE RAIL's one text axis and its seam
  // -------------------------------------------------------------------------
  check('rail axis: every rail line lands on 32 vp, from the module\'s own numbers',
    m.RAIL_TEXT_AXIS_VP === 32 &&
    m.RAIL_DESTINATIONS_PADDING_VP + m.RAIL_CARD_PADDING_VP === m.RAIL_TEXT_AXIS_VP &&
    m.RAIL_DESTINATIONS_PADDING_VP + m.RAIL_NAV_ROW_PADDING_VP + m.RAIL_NAV_BAR_WIDTH_VP +
      m.RAIL_NAV_LABEL_GAP_VP === m.RAIL_TEXT_AXIS_VP &&
    m.RAIL_PAGE_PADDING_VP + m.RAIL_CARD_PADDING_VP === m.RAIL_TEXT_AXIS_VP &&
    m.RAIL_NAV_BAR_WIDTH_VP === 3);
  check('rail seam: the rail/pane boundary is a 1 vp stroke, one vp grid',
    m.RAIL_SEAM_WIDTH_VP === 1);
  // 0.0.40 (I5) DELIBERATE UPDATE (§7, review §7): 2 → 3 vp. The review could
  // "barely see" the ring at 2 vp on a 104 vp disc; the fill, the label and the disc
  // size are unchanged, and the review's own alternative (a dark gap layer) is
  // deliberately not taken because it would shrink the disc it forbids shrinking.
  check('disc: the state ring is 3 vp (I5 §7, was 2)',
    m.CONNECT_RING_WIDTH_VP === 3);

  // -------------------------------------------------------------------------
  // 18. the I3 + I4 WIRING, against the shipped page source
  // -------------------------------------------------------------------------
  check('A: the pane\'s no-selection page is the 连接概览, and the I2 hint is gone',
    PAGE.includes('this.connectionOverviewPane()') &&
    PAGE.includes('\n  connectionOverviewPane() {') &&
    !PAGE.includes('this.widePeerHint()') &&
    !PAGE.includes("Text('选择一个对端查看详情')") &&
    PAGE.includes("Text('连接概览')") &&
    PAGE.includes('this.overviewRowKeys()') &&
    PAGE.includes('private overviewRowKeys(): string[] {') &&
    PAGE.includes('return overviewRowKeys(this.overviewFacts());'));
  // 0.0.40 (I5) DELIBERATE UPDATE (§1): the aggregate counter reader moved from
  // `overviewTrafficText` (byteText — size + exact bytes) to `railTrafficText`
  // (formatBytes only), because the bracketed duplicate unit was two thirds of the
  // string and the reason the rail's line ellipsised. The property is unchanged:
  // every value the pane/rail render is a method call on the polled document.
  check('A: the overview reads live state only (no new field, no frozen by-value row)',
    PAGE.includes('private overviewDirectCount(): number {') &&
    PAGE.includes('private overviewRelayCount(): number {') &&
    PAGE.includes('private overviewOfflineCount(): number {') &&
    PAGE.includes('private overviewOfflinePeers(): UiPeerEntry[] {') &&
    PAGE.includes('return formatBytes(sent ? traffic.tx_bytes : traffic.rx_bytes_to_tun);') &&
    PAGE.includes('private overviewHas(key: string): boolean {') &&
    !PAGE.includes('@State overviewRows'));
  check('A: THE PANE CARRIES THE NUMBERS — hero, three tiles, relay, offline, error',
    (() => {
      const from = PAGE.indexOf('connectionOverviewPane() {');
      const pane = PAGE.slice(from, PAGE.indexOf('keepAliveCard() {', from));
      return pane.includes('this.homeAddressText()') && pane.includes('this.homeFqdnText()') &&
        pane.includes('this.overviewDirectCount()') && pane.includes('this.overviewRelayCount()') &&
        pane.includes('this.relayUrlText()') &&
        pane.includes('this.overviewOfflinePeers()') &&
        pane.includes('this.lastErrorLine()') &&
        pane.includes("Text('已连接')") && pane.includes("Text('直连')") &&
        pane.includes("Text('中继')") &&
        pane.includes('OVERVIEW_TILE_HEIGHT_VP') && pane.includes('TYPE_STAT_VALUE_VP') &&
        pane.includes('OV_TILE_CONNECTED') && pane.includes('OV_HERO_ADDRESS') &&
        pane.includes('OV_ROW_RELAY_IN_USE') && pane.includes('OV_ROW_OFFLINE') &&
        pane.includes('OV_ROW_ERROR') && pane.includes('OV_ROW_NONE');
    })());
  check('A: THE NON-DUPLICATION GUARD — no connect control and no verdict row in the pane',
    (() => {
      const from = PAGE.indexOf('connectionOverviewPane() {');
      const pane = PAGE.slice(from, PAGE.indexOf('keepAliveCard() {', from));
      // No connect command and no second state lamp: the rail owns both.
      return !pane.includes('this.toggleLabel()') && !pane.includes('this.requestStart()') &&
        !pane.includes('this.requestStop()') && !pane.includes('this.beginConnectLogin()') &&
        !pane.includes('Button(') &&
        !pane.includes('this.stateText()') && !pane.includes('this.stateColor()') &&
        !pane.includes('this.toggleOn()');
    })());
  check('B: the detail is FOUR sections fed by ONE row source and one classifier',
    PAGE.includes('private peerDetailSectionFields(section: string): DetailField[] {') &&
    PAGE.includes('if (peerDetailSectionOf(field.label) !== section) {') &&
    PAGE.includes('this.peerDetailSectionFields(PEER_DETAIL_CONNECTION)') &&
    PAGE.includes('this.peerDetailSectionFields(PEER_DETAIL_PATH)') &&
    PAGE.includes('this.peerDetailSectionFields(PEER_DETAIL_ICE)') &&
    PAGE.includes('this.peerDetailSectionFields(PEER_DETAIL_IDENTITY)') &&
    (PAGE.match(/this\.selectedPeerFields\(\)/g) ?? []).length === 1);
  check('B: only the 连接 section is OPEN; 路径 / ICE / 标识 are collapsed headers',
    PAGE.includes('@State peerConnectionOpen: boolean = true;') &&
    PAGE.includes('@State peerPathOpen: boolean = false;') &&
    PAGE.includes('@State peerIceOpen: boolean = false;') &&
    PAGE.includes('@State peerIdentityOpen: boolean = false;') &&
    PAGE.includes("title: '连接',") && PAGE.includes("title: '路径',") &&
    PAGE.includes("title: 'ICE',") && PAGE.includes("title: '标识',") &&
    PAGE.includes('expanded: this.peerConnectionOpen,') &&
    PAGE.includes('expanded: this.peerIdentityOpen,') &&
    !PAGE.includes('高级信息 / 诊断') &&
    !PAGE.includes('@State peerAdvancedOpen'));
  check('B: the 域名 duplicate rule, the mono public key and the 720 vp card cap are wired',
    PAGE.includes('peerFqdnDuplicatesTitle(title, field.value)') &&
    PAGE.includes('DetailRow({ label: field.label, value: field.value, mono: true })') &&
    PAGE.includes('@Prop mono: boolean = false;') &&
    PAGE.includes('.constraintSize({ maxWidth: DETAIL_CONTENT_MAX_WIDTH_VP })') &&
    PAGE.includes('private peerDetailSectionFields(') &&
    PAGE.includes('const title: string = this.selectedPeerName();'));
  check('B: the ICE 说明 and the accounting caveat are 14 vp CAPTIONS, not rows',
    (() => {
      // Scoped to the DETAIL pane's own row source: the 详细状态 page legitimately
      // has its own 说明 rows, and this rule is about the peer detail.
      const fields = PAGE.slice(PAGE.indexOf('private selectedPeerFields(): DetailField[] {'),
        PAGE.indexOf('private peerDetailSectionFields('));
      return !fields.includes("label: 'ICE 说明'") && !fields.includes("label: '说明'") &&
        PAGE.includes('ICE 各行只描述候选路径与端点应用情况') &&
        PAGE.includes('发送/接收为本对端的明文 IP 载荷字节');
    })());
  check('B: the default rows carry plain Chinese (no 明文 / no session jargon)',
    PAGE.includes("label: '发送'") && PAGE.includes("label: '接收'") &&
    PAGE.includes("value: peer.online ? connectionLabel(peer.connection) : '未连接'") &&
    !PAGE.includes("label: '发送（明文）'") &&
    !PAGE.includes("label: '接收（明文）'") &&
    (() => {
      // Scoped to the detail pane's own rows: the 详细状态 page's per-peer BLOCK
      // text (peerBlockText) legitimately keeps the diagnostic phrasing.
      const fields = PAGE.slice(PAGE.indexOf('private selectedPeerFields(): DetailField[] {'),
        PAGE.indexOf('private peerDetailSectionFields('));
      return !fields.includes('未知（无本机会话）') && fields.includes(": '未连接'");
    })());
  check('C: the compact tier condenses the row and the 已连接 text no longer renders',
    (() => {
      const row = PAGE.slice(PAGE.indexOf('peerRow(peer: UiPeerEntry) {'),
        PAGE.indexOf('peersTab() {'));
      // the chip appears in BOTH shapes (line 1 wide / line 2 compact) and the
      // status text is gated on a non-connected status in both.
      return (row.match(/this\.peerRowCondensed\(\)/g) ?? []).length >= 3 &&
        (row.match(/connectionLabel\(peer.connection\)/g) ?? []).length === 2 &&
        (row.match(/if \(peer\.status !== 'connected'/g) ?? []).length === 2 &&
        row.indexOf('if (peer.online && !this.peerRowCondensed()) {') > 0;
    })());
  check('C: the row renders EXACTLY the fields peerRowKey carries (no new column here)',
    (() => {
      const row = PAGE.slice(PAGE.indexOf('peerRow(peer: UiPeerEntry) {'),
        PAGE.indexOf('peersTab() {'));
      return row.includes('Text(peer.name)') &&
        row.includes("Text(peer.ip.length > 0 ? peer.ip : '无 IP')") &&
        row.includes('connectionLabel(peer.connection)') &&
        row.includes('peerStatusText(peer.status)') &&
        !row.includes('peer.last_handshake_s') && !row.includes('peer.endpoint') &&
        !row.includes('peer.tx_bytes');
    })());
  check('C/F: the list column, the reservation and the search are the pure tables\' answers',
    PAGE.includes('private peerListNavBarWidth(): number {') &&
    PAGE.includes('return peerListNavBarWidthFor(this.widthVp);') &&
    PAGE.includes('private peerContentMinWidth(): number {') &&
    PAGE.includes('return peerContentMinWidthVp(this.widthVp);') &&
    PAGE.includes('private peerRowCondensed(): boolean {') &&
    PAGE.includes('return peerRowCondensed(this.widthVp);') &&
    PAGE.includes('private peerSearchWidth(): Length {') &&
    PAGE.includes('.width(this.peerSearchWidth())') &&
    PAGE.includes('.padding({ left: PEER_ROW_LIST_PADDING_VP, right: PEER_ROW_LIST_PADDING_VP })'));
  check('F: the search field exists only when there is a peer to search',
    PAGE.indexOf('if (this.peersList().length > 0) {') > 0 &&
    PAGE.indexOf('if (this.peersList().length > 0) {') <
      PAGE.indexOf("placeholder: '搜索对端名称或 IP'"));
  check('D: the settings column is capped and the 更改配置 button is a button',
    PAGE.includes('.constraintSize(this.settingsConstraint())') &&
    PAGE.includes('private settingsConstraint(): ConstraintSizeOptions {') &&
    PAGE.includes('const pane: number = this.wideLayout() ? splitPaneWidthVp(this.widthVp) : this.widthVp;') &&
    (() => {
      const card = PAGE.slice(PAGE.indexOf('serverConfigCard() {'),
        PAGE.indexOf('logoutAction() {'));
      return card.includes('.width(BUTTON_INLINE_WIDTH_VP)') &&
        !card.includes("Button('更改配置')\n        .fontSize(TYPE_BODY_VP)\n        .height(40)\n        .width('100%')");
    })());
  check('D: 注销 lives INSIDE the server card, behind a 1 vp divider (no orphan red row)',
    (() => {
      const card = PAGE.slice(PAGE.indexOf('serverConfigCard() {'),
        PAGE.indexOf('// 诊断日志 (settings tab'));
      const tab = PAGE.slice(PAGE.indexOf('settingsTab() {'),
        PAGE.indexOf('// ---- 路由选择 (设置 → 路由选择'));
      return card.includes('this.logoutAction()') && card.includes('.height(1)') &&
        !tab.includes('this.logoutAction()');
    })());
  check('D: the 关于 rows put the VALUE at the row\'s end (valueEnd: true)',
    PAGE.includes('@Prop valueEnd: boolean = false;') &&
    PAGE.includes('textAlign(this.valueEnd ? TextAlign.End : TextAlign.Start)') &&
    (PAGE.match(/valueEnd: true/g) ?? []).length === 2 &&
    PAGE.includes("label: '版本',") && PAGE.includes("label: '版本号',"));
  check('D: the settings surface has NO text literal below the 14 vp PC floor',
    (() => {
      const from = PAGE.indexOf('setupFormCard() {');
      const to = PAGE.indexOf('// ---- 路由选择 (设置 → 路由选择');
      const settings = PAGE.slice(from, to);
      const sizes = settings.match(/\.fontSize\((\d+)\)/g) ?? [];
      return sizes.length > 0 &&
        sizes.every((site) => Number(site.replace(/\D/g, '')) >= 14);
    })());
  check('D: the 诊断日志 explanation is behind a collapsed 说明, not a grey wall',
    PAGE.includes('@State diagnosticsNoteOpen: boolean = false;') &&
    PAGE.includes("title: '说明',") &&
    PAGE.includes('expanded: this.diagnosticsNoteOpen,') &&
    (() => {
      const card = PAGE.slice(PAGE.indexOf('diagnosticsLogCard() {'),
        PAGE.indexOf('// ---- 详细状态 (netbird status -d equivalent)'));
      return card.indexOf('if (this.diagnosticsNoteOpen) {') > 0 &&
        card.indexOf('if (this.diagnosticsNoteOpen) {') < card.indexOf('滚动保留：');
    })());
  check('E: the rail centres the identity block between two spacers, nav at the bottom',
    PAGE.includes('private homeBodyHeight(): Length {') &&
    PAGE.includes("return this.wideLayout() ? 'auto' : '100%';") &&
    PAGE.includes('.height(this.homeBodyHeight())') &&
    PAGE.includes('private railConnectionMaxHeight(): Length {') &&
    (() => {
      const rail = PAGE.slice(PAGE.indexOf('connectionRail() {'),
        PAGE.indexOf('railDestinations() {'));
      // 0.0.39 (I4): the review's §1.2 option (a) — TWO equal layoutWeight(1)
      // spacers around the identity block, which keeps the block a DIRECT child of
      // the rail so RAIL_CONNECTION_MAX_HEIGHT_PCT resolves against the RAIL. Nested
      // inside a weighted wrapper it resolved against the leftover space and the
      // block's Scroll clipped the 详细状态 entry (19:45 device capture).
      const spacers = rail.match(/Column\(\)\s*\n?\s*\.width\('100%'\)\s*\n?\s*\.layoutWeight\(1\)/g) ?? [];
      const nested = /Column\(\)\s*\{\s*Column\(\)/;
      return rail.includes('.height(\'auto\')') &&
        rail.includes('.constraintSize({ maxHeight: this.railConnectionMaxHeight() })') &&
        spacers.length === 2 && !nested.test(rail) &&
        rail.indexOf('.layoutWeight(1)') < rail.indexOf('this.homeTab()') &&
        rail.lastIndexOf('.layoutWeight(1)') > rail.indexOf('this.homeTab()') &&
        rail.indexOf('this.homeTab()') < rail.indexOf('this.railDestinations()');
    })());
  check('E: the 详细状态 row is a proper entry (short subtitle + chevron, no 路… artefact)',
    PAGE.includes("Text('中继 · DNS · 路由 · 流量')") &&
    !PAGE.includes('端点 / 中继 / 流量 / DNS / 路由 / 会话') &&
    (() => {
      const at = PAGE.indexOf("Text('中继 · DNS · 路由 · 流量')");
      const row = PAGE.slice(at, at + 500);
      return row.includes("Text('›')") && !row.includes('textOverflow');
    })());
  // 0.0.40 (I5) DELIBERATE UPDATE (§6): the 导航 caption's `.padding({ left:
  // RAIL_CARD_PADDING_VP, bottom: 4 })` is gone with the caption. The rail's text
  // axis is unchanged and still asserted by the module's own sum (see the 'rail axis'
  // check above): the caption was the row that USED that padding, not the axis.
  check('E+8: the rail text sits on ONE axis and the rail/pane seam is drawn',
    PAGE.includes('.padding({ left: RAIL_NAV_ROW_PADDING_VP, right: 12 })') &&
    PAGE.includes('.margin({ left: RAIL_NAV_LABEL_GAP_VP })') &&
    PAGE.includes('.width(RAIL_NAV_BAR_WIDTH_VP)') &&
    !PAGE.includes("Text('导航')") &&
    PAGE.includes('.padding({ left: RAIL_CARD_PADDING_VP })') &&
    PAGE.includes('left: RAIL_DESTINATIONS_PADDING_VP, right: RAIL_DESTINATIONS_PADDING_VP,') &&
    PAGE.includes('.border({ width: { right: RAIL_SEAM_WIDTH_VP }, color: $r(\'app.color.nb_card_border\') })'));
  check('A: THE CONNECTED + NO-SELECTION PANE IS THE 连接概览 (the whole chain)',
    // the pure rule: fully connected, no explicit choice -> the 对端 destination
    m.widePaneDestination({ activeTab: 0, connected: true, needsLogin: false }) ===
      m.PANE_DEST_PEERS &&
    // the pane's BASE route is what shows with no peer selected ...
    PAGE.includes('this.peersNavStack.pushPath({ name: PEER_PANE_HOME_ROUTE });') &&
    PAGE.includes('private ensurePeerPaneBaseRoute(): void {') &&
    // ... and that route renders the overview, not the retired hint
    PAGE.includes('} else if (name === PEER_PANE_HOME_ROUTE) {') &&
    PAGE.indexOf('} else if (name === PEER_PANE_HOME_ROUTE) {') <
      PAGE.indexOf('this.connectionOverviewPane()') &&
    !PAGE.includes('this.widePeerHint()'));
  check('P0.1: the disc fills with the ACTION role and rings with the STATE role',
    PAGE.includes('private stateColor(): ResourceColor {') &&
    PAGE.includes('.backgroundColor($r(\'app.color.nb_primary\'))') &&
    // the granular ring attributes with the radius set AFTER them: a composite
    // `.border({...})` reset the disc to the platform's default corner radius and
    // rendered a rounded SQUARE on the device (19:45 capture).
    PAGE.includes('.borderWidth(CONNECT_RING_WIDTH_VP)') &&
    PAGE.includes('.borderColor(this.stateColor())') &&
    PAGE.includes('.borderRadius(this.connectButtonSize() / 2)') &&
    PAGE.indexOf('.borderWidth(CONNECT_RING_WIDTH_VP)') <
      PAGE.indexOf('.borderRadius(this.connectButtonSize() / 2)') &&
    !PAGE.includes('.border({ width: CONNECT_RING_WIDTH_VP') &&
    !PAGE.includes('private toggleColor()') &&
    !PAGE.includes('private toggleBusy()') &&
    PAGE.includes('return this.wideLayout() ? 104 : 132;') &&
    PAGE.includes('return this.wideLayout() ? 18 : 22;'));
  // 0.0.40 (I5) DELIBERATE UPDATE (§3): the row now paints one more nb_primary — the
  // selected row's own 3 vp marker (2 chips + 1 marker; the marker is transparent
  // until that row is the selected one).
  check('P1.6: the 中继 chip carries the primary tint, warning stays for warnings',
    // The chip is the only tinted pill in the peer row and it pairs the primary INK
    // with the info tint in BOTH row shapes. The warning tint keeps its ONE other
    // user on this page (the "no status document" banner), i.e. a real warning.
    (() => {
      const row = PAGE.slice(PAGE.indexOf('peerRow(peer: UiPeerEntry) {'),
        PAGE.indexOf('peersTab() {'));
      const info = /\$r\('app\.color\.nb_status_info_background'\)/g;
      const primary = /\$r\('app\.color\.nb_primary'\)/g;
      const warning = /\$r\('app\.color\.nb_status_warning_background'\)/g;
      return (row.match(info) ?? []).length === 2 && !warning.test(row) &&
        (row.match(primary) ?? []).length === 3 &&
        (PAGE.match(warning) ?? []).length === 1;
    })());
  check('G: the narrow branch keeps its information architecture (bottom tabs, one column)',
    PAGE.includes('Tabs({ barPosition: BarPosition.End, index: this.activeTab })') &&
    PAGE.includes("this.settingsTab()") &&
    PAGE.includes('private peerSearchWidth(): Length {') &&
    PAGE.includes("return '100%';") &&
    PAGE.includes("return this.wideLayout() ? 'auto' : '100%';") &&
    !PAGE.includes('deviceInfo.deviceType'));

  // -------------------------------------------------------------------------
  // 19. 0.0.40 (I5) — THE ASTRA REVIEW'S EIGHT FINDINGS, each as a property of
  //     the pure module and/or of the shipped page source
  // -------------------------------------------------------------------------

  // §1 — THE TRAFFIC SUMMARY: omit the zero, and the surviving row cannot be
  // ellipsised. Two halves: the POLICY over the two counters, and the WIDTH of one
  // stacked line against the narrowest rail's value column.
  check('I5 §1: the traffic row exists only when a counter is above zero',
    !m.railTrafficVisible(0, 0) &&
    m.railTrafficVisible(1, 0) && m.railTrafficVisible(0, 1) &&
    m.railTrafficVisible(1024, 0) &&
    // a missing / non-finite counter is "no traffic", never a fake zero
    !m.railTrafficVisible(Number.NaN, 0) &&
    !m.railTrafficVisible(undefined as unknown as number, 0) &&
    !m.railTrafficVisible(-1, -1) &&
    // …and an infinite counter is not a value the row can print either
    !m.railTrafficVisible(Number.POSITIVE_INFINITY, 0));
  check('I5 §1: one stacked traffic line fits the narrowest rail (no ellipsis possible)',
    m.railTrafficLineWidthVp() ===
      m.RAIL_TRAFFIC_PREFIX_VP + m.RAIL_TRAFFIC_VALUE_CHARS * m.RAIL_TRAFFIC_CHAR_VP &&
    m.railTrafficLineWidthVp() <= m.railInfoValueWidthVp(840) &&
    m.railTrafficLineWidthVp() <= m.RAIL_INFO_MIN_VALUE_VP,
    `line=${m.railTrafficLineWidthVp()} value column at 840=${m.railInfoValueWidthVp(840)}`);
  check('I5 §1: the row is wired to the policy, stacked, and never ellipsised',
    PAGE.includes('private railTrafficRowVisible(): boolean {') &&
    PAGE.includes('return railTrafficVisible(traffic.tx_bytes, traffic.rx_bytes_to_tun);') &&
    PAGE.includes('private railTrafficText(sent: boolean): string {') &&
    PAGE.includes('return formatBytes(sent ? traffic.tx_bytes : traffic.rx_bytes_to_tun);') &&
    PAGE.includes('if (this.wideLayout() && this.railTrafficRowVisible()) {') &&
    PAGE.includes('Text(`发 ${this.railTrafficText(true)}`)') &&
    PAGE.includes('Text(`收 ${this.railTrafficText(false)}`)') &&
    // the bracketed byte count left the rail (byteText stays in 详细状态 only)
    !PAGE.includes('overviewTrafficText') &&
    (() => {
      // The traffic block itself must carry no textOverflow at all: with the width
      // arithmetic above, an ellipsis there would be a silent truncation waiting for
      // the one value nobody checked.
      const at = PAGE.indexOf('if (this.wideLayout() && this.railTrafficRowVisible()) {');
      const block = PAGE.slice(at, PAGE.indexOf('if (this.lastErrorLine().length > 0) {', at));
      return block.length > 0 && !block.includes('textOverflow');
    })());

  // §2 — THE CARD IS NOT CLIPPED. The geometry is expressible: the ceiling must be
  // the rail minus what the bottom region always takes, so it can only reach the
  // Scroll when the rail genuinely cannot hold the card.
  check('I5 §2: the ceiling clears the card at the captured windows and keeps the nav room',
    // the ASTRA frame's rail height and the 19:53 capture window's:
    m.railConnectionMaxHeightVp(833) === 712 && m.railConnectionMaxHeightVp(1240) === 1119 &&
    m.railConnectionMaxHeightVp(833) >= m.RAIL_CONNECTION_MIN_VP &&
    m.railConnectionMaxHeightVp(833) + m.RAIL_DESTINATIONS_HEIGHT_VP === 833);
  check('I5 §2: the ceiling is STRICTLY roomier than the retired 60 % cap',
    // The monotonicity that makes the fix safe at every window, not just the
    // captured one: H - 121 > 0.6 H for every rail taller than 302.5 vp, i.e. the
    // new ceiling can only ever give the region MORE room than the percentage did.
    (() => {
      for (let h = 303; h <= 3000; h += 1) {
        if (m.railConnectionMaxHeightVp(h) <= 0.6 * h) { return false; }
      }
      return true;
    })());
  check('I5 §2: the page measures the rail and the 详细状态 entry keeps its 48 vp row',
    PAGE.includes('@State heightVp: number = 0;') &&
    PAGE.includes('const height: number = newArea.height as number;') &&
    PAGE.includes('if (height > 0 && height !== this.heightVp) {') &&
    PAGE.includes('private railConnectionMaxHeight(): Length {') &&
    PAGE.includes('return railConnectionMaxHeightVp(railHeight);') &&
    !PAGE.includes('RAIL_CONNECTION_MAX_HEIGHT_PCT') &&
    m.RAIL_ENTRY_MIN_HEIGHT_VP === 48 &&
    PAGE.includes('.constraintSize({ minHeight: RAIL_ENTRY_MIN_HEIGHT_VP })'));

  // §3 — THE SELECTED ROW (+ the row key). The key is EXECUTED, not merely grepped:
  // the selected flag has to be part of it, both ways round.
  check('I5 §3: the selected flag is part of peerRowKey (both directions)',
    ui.peerRowKey({ pub_key: 'K', name: 'n', ip: 'i', online: true, connection: 'direct',
      status: 'connected' }, true) !==
    ui.peerRowKey({ pub_key: 'K', name: 'n', ip: 'i', online: true, connection: 'direct',
      status: 'connected' }, false) &&
    // the default keeps the 连接概览 rows' key byte-identical to 0.0.39
    ui.peerRowKey({ pub_key: 'K', name: 'n', ip: 'i', online: true, connection: 'direct',
      status: 'connected' }) ===
    ui.peerRowKey({ pub_key: 'K', name: 'n', ip: 'i', online: true, connection: 'direct',
      status: 'connected' }, false));
  check('I5 §3: the row paints the marker + the tint from the observed selection',
    PAGE.includes('private peerRowSelected(peer: UiPeerEntry): boolean {') &&
    PAGE.includes('return selected !== null && selected.pub_key === peer.pub_key;') &&
    PAGE.includes('width: { left: PEER_ROW_SELECT_BAR_VP },') &&
    PAGE.includes("this.peerRowSelected(peer) ? $r('app.color.nb_primary') : Color.Transparent") &&
    PAGE.includes('.backgroundColor(this.peerRowSelected(peer)') &&
    PAGE.includes("? $r('app.color.nb_row_selected_background') : Color.Transparent)") &&
    // …and the key carries it, so the framework cannot reuse the row that was just
    // deselected
    PAGE.includes('(peer: UiPeerEntry) => peerRowKey(peer, this.peerRowSelected(peer))'));

  // §5 — THE DISCONNECTED WORKSPACE. The title block is the SAME bytes as the
  // connected list's, so "the page starts in the same place" is checkable.
  check('I5 §5: the guide pane opens with the SAME 对端 header the list uses',
    (() => {
      const header = ["Text('对端')", '.fontSize(TYPE_PAGE_TITLE_VP)',
        '.fontWeight(FontWeight.Bold)', ".fontColor($r('app.color.nb_text_primary'))",
        ".width('100%')"].join('\n            ');
      // exactly TWO occurrences: the peer list's header and the guide's.
      const hits = PAGE.split(header).length - 1;
      const guide = PAGE.slice(PAGE.indexOf('wideGuidePane() {'),
        PAGE.indexOf('connectionOverviewPane() {'));
      return hits === 2 && guide.includes("Text('对端')") &&
        guide.indexOf("Text('对端')") < guide.indexOf('GUIDE_EMPTY_MAX_WIDTH_VP') &&
        guide.includes('.margin({ top: 8 })');
    })());
  check('I5 §5: the empty-state block is BORDERLESS, width-capped and 72 vp under the title',
    m.GUIDE_EMPTY_MAX_WIDTH_VP === 480 && m.GUIDE_BLOCK_TOP_GAP_VP === 72 &&
    m.GUIDE_BODY_LINE_HEIGHT_VP === 22 &&
    (() => {
      const guide = PAGE.slice(PAGE.indexOf('wideGuidePane() {'),
        PAGE.indexOf('connectionOverviewPane() {'));
      return guide.includes('.constraintSize({ maxWidth: GUIDE_EMPTY_MAX_WIDTH_VP })') &&
        guide.includes('.margin({ top: GUIDE_BLOCK_TOP_GAP_VP })') &&
        guide.includes('.lineHeight(GUIDE_BODY_LINE_HEIGHT_VP)') &&
        // the small centred card is gone: no card fill, no stroke, no centring
        !guide.includes('nb_card_background') && !guide.includes('.border(') &&
        !guide.includes('justifyContent(FlexAlign.Center)') &&
        // the connect control still lives in the rail, never here
        !guide.includes('this.requestStart()') && !guide.includes('this.requestStop()') &&
        !guide.includes('this.toggleLabel()');
    })());

  // §6 — the rail's retained set, and the caption that is gone.
  check('I5 §6: the PC rail keeps 状态 + NetBird IP, drops 域名/对端 to the pane',
    (() => {
      const from = PAGE.indexOf('Column({ space: 4 }) {\n          Row({ space: 10 }) {');
      const card = PAGE.slice(from, PAGE.indexOf('if (this.sessionRowOffer()) {', from));
      return from > 0 && card.includes('this.stateText()') &&
        card.includes("label: 'NetBird IP'") &&
        card.includes('if (!this.wideLayout()) {') &&
        card.includes("label: '域名'") && card.includes("label: '对端'") &&
        // the phone keeps all three: the guard is the wide branch's, not the row's
        card.indexOf('if (!this.wideLayout()) {') < card.indexOf("label: '域名'") &&
        card.indexOf("label: '对端'") < card.indexOf("Text('详细状态')");
    })());
  check('I5 §6: no 导航 caption literal is left anywhere in the page',
    // No Text('导航') and no string literal at all; a COMMENT may still explain that
    // the caption is gone — that is documentation, not a rendered line.
    !PAGE.includes("Text('导航')") && !/['"]导航['"]/.test(PAGE));

  // §4 — the neutral button's ink is its OWN role, never the primary role's.
  check('I5 §4: every neutral button wears the neutral ink (no cross-role ink)',
    PAGE.includes("$r('app.color.nb_on_neutral_button')") &&
    (PAGE.match(/\$r\('app\.color\.nb_neutral_button_background'\)/g) ?? []).length ===
      (PAGE.match(/\$r\('app\.color\.nb_on_neutral_button'\)/g) ?? []).length &&
    // the dual-surface button swaps BOTH attributes on the same condition
    PAGE.includes('this.logExportBusy\n          ? $r(\'app.color.nb_on_neutral_button\')') &&
    !PAGE.includes(".backgroundColor($r('app.color.nb_neutral_button_background'))\n" +
      "            .fontColor($r('app.color.nb_on_primary'))"));

  // §8 — the chevron's own slot, and the preview's trailing separator that the
  // reviewer could not confirm (it is not there).
  check('I5 §8: the section chevron has a fixed 24 vp slot 12 vp from the preview',
    m.SECTION_CHEVRON_SLOT_VP === 24 && m.SECTION_HEADER_GAP_VP === 12 &&
    PAGE.includes('Row({ space: SECTION_HEADER_GAP_VP })') &&
    PAGE.includes('.width(SECTION_CHEVRON_SLOT_VP)'));
  check('I5 §8: no collapsed-section summary ends with a stray separator',
    (() => {
      // Every summary literal the page hands a section header: none may end with
      // ' ·' (the reviewer's unconfirmed "尾随 ·"), and neither may the rail's own
      // caption sentence.
      const summaries = PAGE.match(/summary: '[^']*'/g) ?? [];
      const texts = PAGE.match(/Text\('[^']* · [^']*'\)/g) ?? [];
      return summaries.length > 0 && summaries.every((s: string) => !/ ·'$/.test(s)) &&
        texts.every((s: string) => !/ ·'\)$/.test(s));
    })());
} catch (error) {
  failed++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
