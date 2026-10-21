// 0.0.46 (I11) — THE FONT SCALE: fp units, the follow-the-system profile, and the
// per-surface ceilings. Host-side, no device, no build.
//
// Run: node client/entry/selfcheck/run-font-scale.mts
// Exit 0 = all assertions pass; exit 1 = a fixture is unreadable or any failure.
//
// THE DEFECT THIS PINS. The owner reported that the app's text does not follow
// 设置 → 显示和亮度 → 字体大小. Two facts make that a checkable statement rather than
// an opinion:
//
//   * every font size in the UI is a plain NUMBER (the module's TYPE_*_FP scale),
//     and a number handed to `fontSize()` is fp — the unit that follows the system
//     scale (local SDK ets/component/text.d.ts:146). So the code was already in the
//     scaling unit and the owner's "it looks fixed" could NOT be a vp/fp mistake;
//   * what WAS missing is the platform opt-in: a HarmonyOS application does not
//     follow the system font size unless AppScope/app.json5 points its
//     `configuration` tag at a profile whose `fontSizeScale` is `followSystem`
//     (OpenHarmony app.json5 reference: the tag's initial value is
//     `nonFollowSystem`; the profile's schema is
//     sdk/.../toolchains/modulecheck/configuration.json). With no such tag the app's
//     scale stayed 1, where 1 fp = 1 vp — fixed.
//
// The checks below therefore answer the three questions the fix has to keep true:
//   A. no font size in the UI source is left in a non-scaling unit (or in a bare
//      literal at all) — a textual check over the real sources;
//   B. the type scale's VALUES are unchanged and expressed in fp (the `_FP` names,
//      the six original numbers, the chevron's 18, the fp lineHeight);
//   C. every fixed-height or width-bounded surface carries the chosen ceiling as
//      its own `maxFontScale`, every growable surface is minHeight/auto, and the
//      ceilings are DERIVED (the arithmetic is executed here, at the real budgets).
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const ets = join(here, '..', 'src', 'main', 'ets');
const PAGE = readFileSync(join(ets, 'pages', 'Index.ets'), 'utf8');
const LOGIN = readFileSync(join(ets, 'login', 'NetBirdAccountWebView.ets'), 'utf8');
const ABILITY = readFileSync(join(ets, 'entryability', 'EntryAbility.ets'), 'utf8');
const APP_JSON5 = readFileSync(join(here, '..', '..', 'AppScope', 'app.json5'), 'utf8');
const CONFIG_JSON = readFileSync(join(here, '..', '..', 'AppScope', 'resources', 'base',
  'profile', 'configuration.json'), 'utf8');
const dir = mkdtempSync(join(tmpdir(), 'nb-font-scale-'));
let passed = 0;
let failed = 0;
function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
  if (!condition && detail !== undefined) {
    console.log(`     ${detail}`);
  }
}

// The font-size arguments of a source, in order. The argument may hold ONE level of
// parentheses (`this.connectButtonFontSize()`), never more, so this form is exact.
function fontArgs(text: string): string[] {
  const out: string[] = [];
  const re = /\.fontSize\(((?:[^()]|\([^()]*\))*)\)/g;
  let m: RegExpExecArray | null = re.exec(text);
  while (m !== null) {
    out.push(m[1].trim());
    m = re.exec(text);
  }
  return out;
}
function capArgs(text: string): string[] {
  const out: string[] = [];
  const re = /\.maxFontScale\(([^)]*)\)/g;
  let m: RegExpExecArray | null = re.exec(text);
  while (m !== null) {
    out.push(m[1].trim());
    m = re.exec(text);
  }
  return out;
}
function region(text: string, from: string, to: string): string {
  const i = text.indexOf(from);
  const j = text.indexOf(to);
  if (i < 0 || j < 0 || j < i) {
    return '';
  }
  return text.slice(i, j);
}

try {
  // The pure module is imported byte-identical (only the extension changes).
  const layoutFixture = join(dir, 'NetBirdHarmonyLayout.ts');
  writeFileSync(layoutFixture, readFileSync(join(ets, 'pages', 'NetBirdHarmonyLayout.ets')));
  const m = await import(pathToFileURL(layoutFixture).href);

  const pageArgs = fontArgs(PAGE);
  const loginArgs = fontArgs(LOGIN);
  const SCALE_NAMES: string[] = ['TYPE_APP_TITLE_FP', 'TYPE_PAGE_TITLE_FP',
    'TYPE_SECTION_TITLE_FP', 'TYPE_BODY_FP', 'TYPE_CAPTION_FP', 'TYPE_STAT_VALUE_FP',
    'TYPE_CHEVRON_FP', 'TYPE_LARGE_TITLE_FP',
    // 0.0.52: the round's own fp roles — the dense peer row's name (15), the settings
    // group title (14, an alias of the caption) and the title bar's back glyph (24).
    'PEER_ROW_NAME_FP', 'SETTINGS_GROUP_TITLE_FP', 'PAGE_BACK_GLYPH_FP'];

  // =========================================================================
  // A. THE UNIT: every font size is the fp scale, never a fixed-unit literal
  // =========================================================================
  check('A: the page has 115 fontSize call sites (the pinned inventory of 0.0.52)',
    // 0.0.52 DELIBERATE UPDATE: 129 → 115. The settings page's per-item cards became
    // ONE SettingsRow component (3 sizes) plus group containers, the three back rows
    // became ONE pageTitleBar builder (2 sizes), the route page's two 38 vp buttons
    // became two text actions, the 关于 rows moved to SettingsRow, and the search field
    // is ArkUI's Search component (which sizes its text through textFont /
    // placeholderFont, so it carries no fontSize call at all). The INVENTORY is what
    // this check pins (a new ad-hoc size must be a deliberate edit); the scale itself
    // is still checked by the "names one constant" assertion below.
    pageArgs.length === 115, `got ${pageArgs.length}`);
  check('A: the login chrome has 3 (the page title and its two captions)',
    loginArgs.length === 3, `got ${loginArgs.length}`);
  check('A: NO font size is a numeric literal (a bare number IS fp, but a literal is ' +
    'an ad-hoc size this scale exists to prevent)',
    [...pageArgs, ...loginArgs].every((arg) => !/^[0-9]/.test(arg)),
    JSON.stringify([...pageArgs, ...loginArgs].filter((arg) => /^[0-9]/.test(arg))));
  check('A: NO font size is a string with a unit — "14vp" / "14px" would be FIXED, ' +
    'the exact defect class the owner reported',
    [...pageArgs, ...loginArgs].every((arg) =>
      !arg.includes("'") && !arg.includes('"') && !/\d\s*(vp|px|fp)/.test(arg)));
  check('A: NO LengthMetrics anywhere in the UI sources (LengthMetrics.vp is fixed)',
    !PAGE.includes('LengthMetrics') && !LOGIN.includes('LengthMetrics'));
  check('A: NO resource-based font size ($r(\'sys.float.*\') / $r(\'app.float.*\'))',
    !/\.fontSize\(\s*\$r\(/.test(PAGE) && !/\.fontSize\(\s*\$r\(/.test(LOGIN) &&
    !PAGE.includes("sys.float.") && !PAGE.includes("app.float."));
  check('A: every page font size names ONE of the module\'s fp scale constants',
    pageArgs.every((arg) => arg === 'this.connectButtonFontSize()' ||
      SCALE_NAMES.indexOf(arg) >= 0),
    JSON.stringify(pageArgs.filter((arg) => arg !== 'this.connectButtonFontSize()' &&
      SCALE_NAMES.indexOf(arg) < 0)));
  check('A: the two login sizes are the scale constants (no ad-hoc 20 / 14)',
    loginArgs.length === 3 && loginArgs[0] === 'TYPE_PAGE_TITLE_FP' &&
    loginArgs[1] === 'TYPE_CAPTION_FP' && loginArgs[2] === 'TYPE_CAPTION_FP',
    JSON.stringify(loginArgs));
  check('A: no numeric fontSize literal is left in ANY selfchecked UI surface',
    !/\.fontSize\([0-9]/.test(PAGE) && !/\.fontSize\([0-9]/.test(LOGIN));

  // =========================================================================
  // B. THE SCALE: the values are unchanged, and they are fp
  // =========================================================================
  check('B: the official scale is 32 / 20 / 16 / 16 / 14 / 24 (0.0.52)',
    m.TYPE_APP_TITLE_FP === 22 && m.TYPE_LARGE_TITLE_FP === 32 &&
    m.TYPE_PAGE_TITLE_FP === 20 &&
    m.TYPE_SECTION_TITLE_FP === 16 && m.TYPE_BODY_FP === 16 &&
    m.TYPE_CAPTION_FP === 14 && m.TYPE_STAT_VALUE_FP === 24);
  check('B: the one row chevron role is still 18 fp at its two remaining sites, and ' +
    'the settings rows take the caption-size 14 fp mark',
    m.TYPE_CHEVRON_FP === 18 && PAGE.split('.fontSize(TYPE_CHEVRON_FP)').length - 1 === 2 &&
    PAGE.split("Text('›')\n          .fontSize(TYPE_CAPTION_FP)").length - 1 === 1);
  check('B: the guide body\'s line height is fp too (a number lineHeight is fp: ' +
    '"设置值不大于0时…number类型时单位为fp"), so it grows with the glyphs',
    m.GUIDE_BODY_LINE_HEIGHT_FP === 22 &&
    PAGE.split('.lineHeight(GUIDE_BODY_LINE_HEIGHT_FP)').length - 1 === 2);
  check('B: the module declares exactly the 8 fp TYPE_*_FP constants, and NO ' +
    'TYPE_*_VP anywhere (the old name was the misleading half of the defect)',
    (readFileSync(join(ets, 'pages', 'NetBirdHarmonyLayout.ets'), 'utf8')
      .match(/export const TYPE_[A-Z_]+_FP: number/g) ?? []).length === 8 &&
    !/TYPE_[A-Z0-9_]*_VP/.test(PAGE) && !/TYPE_[A-Z0-9_]*_VP/.test(LOGIN) &&
    !/TYPE_[A-Z0-9_]*_VP/.test(readFileSync(join(ets, 'pages',
      'NetBirdHarmonyLayout.ets'), 'utf8')));

  // =========================================================================
  // B2. THE OPT-IN: app.json5 → the profile → followSystem
  // =========================================================================
  check('B2: app.json5 points its configuration tag at $profile:configuration',
    /configuration:\s*'\$profile:configuration'/.test(APP_JSON5));
  const cfg = JSON.parse(CONFIG_JSON) as {
    configuration?: { fontSizeScale?: string; fontSizeMaxScale?: string };
  };
  check('B2: the profile says followSystem — WITHOUT this tag the app renders every ' +
    'fp size at 1 fp = 1 vp, which is the owner\'s "fixed size"',
    cfg.configuration !== undefined &&
    cfg.configuration.fontSizeScale === 'followSystem');
  check('B2: the app-level ceiling is the 适老化 guide\'s 1.75 (its enum: 1, 1.15, ' +
    '1.3, 1.45, 1.75, 2, 3.2) and equals the module\'s TYPE_SCALE_MAX_APP',
    cfg.configuration !== undefined &&
    cfg.configuration.fontSizeMaxScale === '1.75' && m.TYPE_SCALE_MAX_APP === 1.75);
  check('B2: the three ceilings are the shipped ones (app 1.75 / row 1.3 / disc 1)',
    m.TYPE_SCALE_MAX_APP === 1.75 && m.TYPE_SCALE_MAX_ROW === 1.3 &&
    m.TYPE_SCALE_MAX_DISC === 1);

  // =========================================================================
  // C. THE SURFACES: each one's decision, and the arithmetic behind the ceiling
  // =========================================================================
  const caps = capArgs(PAGE);
  check('C: every maxFontScale in the page names one of the three declared ceilings ' +
    '(no literal, no fourth value)',
    // 0.0.52 DELIBERATE UPDATE: 38 → 41 — SettingsRow's label/value/chevron (3), the
    // title chip, the Search field and the two route text actions, less the caps of
    // the six back/title texts the one pageTitleBar replaced.
    caps.length === 41 && caps.every((arg) => arg === 'TYPE_SCALE_MAX_ROW' ||
      arg === 'TYPE_SCALE_MAX_DISC'),
    `n=${caps.length} distinct=${JSON.stringify([...new Set(caps)])}`);
  check('C: the disc is the ONLY surface on the disc ceiling',
    caps.filter((arg) => arg === 'TYPE_SCALE_MAX_DISC').length === 1 &&
    PAGE.includes('Button(this.toggleLabel())\n          ' +
      '.fontSize(this.connectButtonFontSize())\n          .maxFontScale(TYPE_SCALE_MAX_DISC)'));

  // The surfaces whose text is bounded END TO END: every font size there is capped.
  const boundedRegions: string[][] = [
    ['the peer detail row (48 vp minHeight, width-bounded value)',
      '@Component\nstruct DetailRow {', '@Component\nstruct DetailSectionHeader {'],
    ['the rail label/value row (width-bounded value + copy mark)',
      '@Component\nstruct RailInfoRow {', '@Component\nstruct PaneEmptyState {'],
    ['the peer list row (two-line, minHeight 64, IP must stay on one line)',
      'peerRow(peer: UiPeerEntry) {', 'peersTab() {'],
    // 0.0.52: the settings group row — a 48 vp minimum band whose label, value and
    // '›' all carry the row ceiling.
    ['the settings group row (48 vp minHeight)',
      '@Component\nstruct SettingsRow {', '@Component\nstruct PaneEmptyState {'],
    ['the three 连接概览 tiles (fixed 96 vp)',
      'if (this.overviewHas(OV_TILE_CONNECTED)) {',
      'if (this.overviewHas(OV_ROW_RELAY_IN_USE)) {'],    ['the rail\'s two destination rows (fixed 48 vp)',
      'railDestinations() {', 'contentPane() {']
  ];
  for (const row of boundedRegions) {
    const slice = region(PAGE, row[1], row[2]);
    const fs = fontArgs(slice).length;
    const mf = capArgs(slice).length;
    check(`C: ${row[0]} — every text carries a ceiling (${fs} = ${mf})`,
      slice.length > 0 && fs > 0 && fs === mf, `fontSize=${fs} maxFontScale=${mf}`);
  }

  // The MIXED surfaces: their fixed-height text components are named one by one
  // (the auto-height siblings above them legitimately take the app ceiling).
  const chains: string[][] = [
    ['the settings form\'s url field (44 vp)', '      TextInput({ text: this.urlInput, ' +
      "placeholder: '服务器地址（https://…）' })", '        .fontSize(TYPE_BODY_FP)',
      '        .maxFontScale(TYPE_SCALE_MAX_ROW)', '        .height(44)'],
    ['the settings form\'s key field (44 vp)',
      "        TextInput({ text: this.keyInput, placeholder: 'Setup Key' })",
      '          .fontSize(TYPE_BODY_FP)', '          .maxFontScale(TYPE_SCALE_MAX_ROW)',
      '          .height(44)'],
    ['the auth-mode buttons (40 vp)',
      "        Button('账户登录')\n          .fontSize(TYPE_CAPTION_FP)\n" +
      '          .maxFontScale(TYPE_SCALE_MAX_ROW)\n          .height(40)'],
    ['the 取消登录 button (36 vp)',
      "          Button('取消登录')\n            .fontSize(TYPE_CAPTION_FP)\n" +
      '            .maxFontScale(TYPE_SCALE_MAX_ROW)\n            .height(36)'],
    ['the save-and-connect button (42 vp)',
      "        : '保存并连接')\n        .fontSize(TYPE_BODY_FP)\n" +
      '        .maxFontScale(TYPE_SCALE_MAX_ROW)\n        .height(42)'],
    ['the 更改配置 button (40 vp)',
      "        Button('更改配置')\n          .fontSize(TYPE_BODY_FP)\n" +
      '          .maxFontScale(TYPE_SCALE_MAX_ROW)\n          .height(40)'],
    ['the 导出日志 button (38 vp)',
      "      Button(this.logExportBusy ? '正在导出…' : '导出日志')\n" +
      '        .fontSize(TYPE_BODY_FP)\n        .maxFontScale(TYPE_SCALE_MAX_ROW)\n' +
      '        .height(38)'],
    // 0.0.52: the two batch actions are TEXT actions on a caption line (14 fp,
    // primary ink, no fill) — each still carries the row ceiling.
    ['the 全部启用 batch action (text, no fill)',
      "        Text('全部启用')\n          .fontSize(TYPE_CAPTION_FP)\n" +
      '          .maxFontScale(TYPE_SCALE_MAX_ROW)'],
    ['the 全部停用 batch action (text, no fill)',
      "        Text('全部停用')\n          .fontSize(TYPE_CAPTION_FP)\n" +
      '          .maxFontScale(TYPE_SCALE_MAX_ROW)'],
    // 0.0.52: the ONE subpage title bar — 24 fp back glyph in a 40 vp target, 20 fp
    // title; both are fixed by the bar's 56 vp band (three contiguous chains, one per
    // pinned piece, because the chain form matches contiguous lines only).
    ['the title bar back glyph (24 fp system symbol)',
      "        SymbolGlyph($r('sys.symbol.chevron_left'))\n" +
      '          .fontSize(PAGE_BACK_GLYPH_FP)'],
    ['the title bar back hit target (40 vp)',
      '      .width(PAGE_BACK_TARGET_VP)\n      .height(PAGE_BACK_TARGET_VP)'],
    ['the title bar title (20 fp, capped)',
      "      Text(title)\n        .fontSize(TYPE_PAGE_TITLE_FP)\n" +
      '        .maxFontScale(TYPE_SCALE_MAX_ROW)'],
    // 0.0.52: the Search field (40 vp pill) — Search sizes its text through
    // textFont/placeholderFont (it has no `.fontSize`), and the row ceiling is the
    // component's own `.maxFontScale`. Two chains: the fonts and the height.
    ['the 对端 search field fonts (body text / caption placeholder)',
      "          Search({ value: this.peerQuery, placeholder: '搜索对端名称或 IP' })\n" +
      '            .maxFontScale(TYPE_SCALE_MAX_ROW)\n' +
      '            .placeholderFont({ size: TYPE_CAPTION_FP })'],
    ['the 对端 search field height (40 vp)',
      '            .height(PEER_SEARCH_HEIGHT_VP)\n' +
      '            .width(this.peerSearchWidth())'],
    ['the home card\'s 去设置 (40 vp)',
      "            Button('去设置')\n              .fontSize(TYPE_SECTION_TITLE_FP)\n" +
      '              .maxFontScale(TYPE_SCALE_MAX_ROW)\n              .height(40)'],
    ['the guide pane\'s 去设置 (44 vp)',
      "            Button('去设置')\n              .fontSize(TYPE_SECTION_TITLE_FP)\n" +
      '              .maxFontScale(TYPE_SCALE_MAX_ROW)\n              .height(44)'],
    ['the empty-state action (40 vp)',
      '        Button(this.actionLabel)\n          .fontSize(TYPE_SECTION_TITLE_FP)\n' +
      '          .maxFontScale(TYPE_SCALE_MAX_ROW)\n          .height(40)']
  ];
  for (const row of chains) {
    const text = row.length === 2 ? row[1] : [row[1], ...row.slice(2)].join('\n');
    check(`C: ${row[0]} carries TYPE_SCALE_MAX_ROW`, PAGE.includes(text));
  }

  // 0.0.52: the three subpages share ONE 56 vp title bar whose back control is the
  // SYSTEM symbol (sys.symbol.chevron_left) at 24 fp inside a 40 vp target; the old
  // '‹' text glyph and the filled 返回 button are both gone.
  check('C: the three subpages use the ONE title bar, and its back control is the ' +
    'system chevron symbol (no filled 返回 button, no text glyph)',
    (PAGE.match(/this\.pageTitleBar\('/g) ?? []).length === 3 &&
    PAGE.includes("SymbolGlyph($r('sys.symbol.chevron_left'))") &&
    !PAGE.includes("Text('‹')") && !PAGE.includes("Button('返回')"));
  check('C: the title bar is a 56 vp band with a 40 vp back target',
    m.PAGE_TITLE_BAR_HEIGHT_VP === 56 && m.PAGE_BACK_TARGET_VP === 40 &&
    m.PAGE_BACK_GLYPH_FP === 24 &&
    PAGE.includes('.height(PAGE_TITLE_BAR_HEIGHT_VP)') &&
    PAGE.includes('.width(PAGE_BACK_TARGET_VP)\n      .height(PAGE_BACK_TARGET_VP)'));

  // THE GROW DECISIONS. The two multi-line row kinds are minHeight, so a large
  // scale makes them taller and the list/scroll absorbs it; the disclosure chevrons
  // live in padded auto-height rows and therefore grow with the app ceiling.
  check('C: the peer row GROWS (minHeight, not a fixed height)',
    PAGE.includes('.constraintSize({ minHeight: PEER_ROW_MIN_HEIGHT_VP })') &&
    m.PEER_ROW_MIN_HEIGHT_VP === 64);
  check('C: the detail row GROWS (minHeight 48 when it carries an icon)',
    PAGE.includes('.constraintSize({ minHeight: this.iconKind.length > 0 ? ' +
      'DETAIL_ROW_MIN_HEIGHT_VP : 0 })') && m.DETAIL_ROW_MIN_HEIGHT_VP === 48);
  check('C: the 2 remaining row chevrons sit in auto-height rows (no fixed height in ' +
    'their own chains), so they grow with the app ceiling',
    // 0.0.52 DELIBERATE UPDATE: 5 → 2. The settings page's three '›' marks are gone
    // (they are SettingsRow's own caption-size mark now, one site, pinned above); the
    // two left are the home card's 详细状态 row and the overview's peer link.
    (() => {
      const sites = PAGE.match(/\.fontSize\(TYPE_CHEVRON_FP\)[\s\S]{0,140}/g) ?? [];
      return sites.length === 2 && sites.every((site) => !/\.height\(/.test(site));
    })());
  check('C: the rail\'s destination rows stay the measured 48 vp bands',
    m.RAIL_NAV_ROW_HEIGHT_VP === 48 &&
    PAGE.split('.height(RAIL_NAV_ROW_HEIGHT_VP)').length - 1 === 2);

  // THE CEILING'S ARITHMETIC, executed.
  check('C: at the row ceiling the shortest fixed row (36 vp) still holds the ' +
    'tallest text a fixed row carries (16 fp): 16 x 1.3 x 1.5 em = 31.2 <= 36',
    m.typeFitsFixedHeightVp(36, m.TYPE_SECTION_TITLE_FP, m.TYPE_SCALE_MAX_ROW) &&
    m.typeLineBoxVp(m.TYPE_SECTION_TITLE_FP, m.TYPE_SCALE_MAX_ROW) <= 36);
  check('C: the row ceiling is the largest system 档位 at which the peer row\'s ' +
    'dotted quad still fits ONE line at the narrowest split tier (840 vp)',
    m.peerRowIpOneLineAtCapVp() <= m.peerRowIpWidthFor(840) &&
    m.DETAIL_ROW_VALUE_CHARS * (m.TYPE_CAPTION_FP / 2) * 1.45 > m.peerRowIpWidthFor(840),
    `at cap=${m.peerRowIpOneLineAtCapVp()} budget=${m.peerRowIpWidthFor(840)}`);
  check('C: …and at EVERY window of the split shelf, not only 840',
    (() => {
      for (let w = 840; w <= 4000; w += 8) {
        if (m.paneLayoutOf(w) !== m.PANE_SPLIT) {
          continue;
        }
        if (!(m.peerRowIpOneLineAtCapVp() <= m.peerRowIpWidthFor(w))) {
          return false;
        }
      }
      return true;
    })());
  check('C: the rail\'s copied value column holds that address at the cap too ' +
    '(the row that carries the always-visible copy mark)',
    m.railInfoValueHoldsIpAtCap(840) && m.railInfoValueHoldsIpAtCap(1024) &&
    m.railInfoValueHoldsIpAtCap(1440) && m.railInfoValueHoldsIpAtCap(2560));
  check('C: at the row ceiling the 64-character mono key stays inside the detail ' +
    'row\'s 8-line allowance at the narrowest pane (8 lines at the 16 fp body)',
    m.detailRowLongestValueLinesAtCap(320, true) <= m.DETAIL_ROW_VALUE_MAX_LINES &&
    // 0.0.52 DELIBERATE UPDATE: 7 → 8. The value is 16 fp now (was 15), so the mono
    // advance at the cap is 9.6 x 1.3 = 12.48 vp and 64 chars need 8 of the 8 allowed
    // lines — still not ellipsised, but with no slack left (recorded as a risk).
    m.detailRowLongestValueLinesAtCap(320, true) === 8,
    `lines=${m.detailRowLongestValueLinesAtCap(320, true)}`);
  check('C: the disc\'s ceiling is 1 because 1.15 (the NEXT system 档位) clips the ' +
    'widest label on BOTH shells — 110 of 126 vp / 90 of 98 vp at 1x',
    m.connectLabelFitsDiscVp(m.CONNECT_DISC_NARROW_SIZE_VP, m.CONNECT_DISC_NARROW_FONT_FP,
      m.TYPE_SCALE_MAX_DISC) &&
    m.connectLabelFitsDiscVp(m.CONNECT_DISC_WIDE_SIZE_VP, m.CONNECT_DISC_WIDE_FONT_FP,
      m.TYPE_SCALE_MAX_DISC) &&
    !m.connectLabelFitsDiscVp(m.CONNECT_DISC_NARROW_SIZE_VP, m.CONNECT_DISC_NARROW_FONT_FP,
      1.15) &&
    !m.connectLabelFitsDiscVp(m.CONNECT_DISC_WIDE_SIZE_VP, m.CONNECT_DISC_WIDE_FONT_FP,
      1.15));
  check('C: the page\'s disc geometry/type read those module constants (one source)',
    PAGE.includes('this.wideLayout() ? CONNECT_DISC_WIDE_SIZE_VP : ' +
      'CONNECT_DISC_NARROW_SIZE_VP;') &&
    PAGE.includes('this.wideLayout() ? CONNECT_DISC_WIDE_FONT_FP : ' +
      'CONNECT_DISC_NARROW_FONT_FP;'));

  // =========================================================================
  // D. THE OBSERVED SCALE: the app says which 档位 it saw
  // =========================================================================
  check('D: the ability logs the observed scale at startup (context.config)',
    ABILITY.includes('FONT_SCALE_OBSERVED|fontSizeScale=%{public}s') &&
    ABILITY.includes('this.context.config.fontSizeScale'));
  check('D: …and on the runtime configuration update (the same callback as the ' +
    'color mode)',
    ABILITY.includes('FONT_SCALE_UPDATE|fontSizeScale=%{public}s') &&
    ABILITY.includes('newConfig.fontSizeScale'));
  check('D: the diagnostic is bounded — the value is a number or the literal ' +
    '\'undefined\', never free-form configuration text',
    ABILITY.includes('startupFontScale === undefined ? \'undefined\' : ' +
      'String(startupFontScale)') &&
    ABILITY.includes('fontScale === undefined ? \'undefined\' : String(fontScale)'));
} finally {
  rmSync(dir, { recursive: true, force: true });
}

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
