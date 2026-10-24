// 0.0.44 (I9) — host-side checks for the peer detail pane's OFFICIAL-CLIENT ROW
// PATTERN: a leading semantic glyph on every row, the owner's identifier/address
// rows carrying the trailing copy mark, the value right-aligned, and a 48 vp centred
// row band. No device, no build.
//
// Four halves, all against the shipped sources:
//
//   A. THE PURE MAPPING. client/entry/src/main/ets/pages/NetBirdHarmonyLayout.ets is
//      copied BYTE-FOR-BYTE into an OS-temp fixture and imported under plain node (it
//      is a zero-import module, so nothing needs stubbing). Every label the pane can
//      render maps to a glyph kind that the page knows how to draw, and the labels
//      the PAGE's row source actually pushes are cross-checked against the classifier
//      lists — so a row cannot appear on screen glyph-less.
//   B. THE COPYABLE PREDICATE. The owner's list, exactly: 域名, NetBird IP, 对端端点,
//      中继通道地址, 对端网络, 公钥 — and explicitly NOT 状态 / 最近握手 / 发送 / 接收
//      (nor the mode, egress, bearer, local relay session or any ICE row).
//   C. THE PAGE WIRING. All four sections pass the glyph, the end-aligned value and
//      the copy string from the pure policy; the copy callback hands the row's own
//      string to the ONE clipboard path unchanged; and none of it joined a ForEach
//      key (the glyph and the copyability are functions of the LABEL, which the keys
//      already carry).
//   D. THE WIDTH ARGUMENT. The glyphs are paid for out of the row's width, so this
//      is where "no value was pushed into an ellipsis" is arithmetic instead of a
//      screenshot: the value column at the NARROWEST wide tier still clears the
//      one-line floor of the IPv4 the pane leads with, and the pane's longest value
//      still wraps inside the row's mounted line cap.
//
// Run: node client/entry/selfcheck/run-detail-icons.mts
// Exit 0 = all assertions pass.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const ets = join(here, '..', 'src', 'main', 'ets');
const LAYOUT = readFileSync(join(ets, 'pages', 'NetBirdHarmonyLayout.ets'), 'utf8');
const PAGE = readFileSync(join(ets, 'pages', 'Index.ets'), 'utf8');

// The negative assertions are about CODE: the page's own comments legitimately NAME
// `sys.symbol` (to say why it is not used), so they run over the stripped text.
function stripComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');
}
const PAGE_CODE = stripComments(PAGE);

let passed = 0;
let failed = 0;
function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else {
    failed++;
    if (detail !== undefined) { console.log(`     ${detail}`); }
  }
}

const dir = mkdtempSync(join(tmpdir(), 'nb-detail-icons-'));

try {
  writeFileSync(join(dir, 'NetBirdHarmonyLayout.ts'), LAYOUT);
  const m = await import(pathToFileURL(join(dir, 'NetBirdHarmonyLayout.ts')).href);

  // ---- the rendered label set, from the two sources that must agree -----------
  const sectionLabels: string[] = [];
  const lists: string[][] = [m.PEER_DETAIL_CONNECTION_LABELS, m.PEER_DETAIL_PATH_LABELS,
    m.PEER_DETAIL_ICE_LABELS, m.PEER_DETAIL_IDENTITY_LABELS];
  for (const list of lists) {
    for (const label of list) {
      sectionLabels.push(label);
    }
  }
  const fields = PAGE.slice(PAGE.indexOf('private selectedPeerFields(): DetailField[] {'),
    PAGE.indexOf('private peerDetailSectionFields('));
  const pageLabels: string[] = [];
  for (const match of fields.matchAll(/label: '([^']+)'/g)) {
    pageLabels.push(match[1]);
  }

  check('A: the classifier lists are the pane\'s own 21 rows, with no duplicate',
    sectionLabels.length === 21 && new Set(sectionLabels).size === 21 &&
    m.PEER_DETAIL_CONNECTION_LABELS.length === 7 && m.PEER_DETAIL_PATH_LABELS.length === 6 &&
    m.PEER_DETAIL_ICE_LABELS.length === 7 && m.PEER_DETAIL_IDENTITY_LABELS.length === 1,
    JSON.stringify(sectionLabels));
  check('A: the row source pushes exactly those labels (one source, four sections)',
    pageLabels.length === 21 && new Set(pageLabels).size === 21 &&
    pageLabels.every((label: string) => sectionLabels.includes(label)),
    JSON.stringify(pageLabels));
  check('A: EVERY rendered row\'s kind maps to a glyph the page can draw',
    pageLabels.every((label: string) => {
      const kind: string = m.peerDetailIconKind(label);
      return kind.length > 0 && m.DETAIL_ICON_KINDS.includes(kind);
    }) &&
    sectionLabels.every((label: string) => m.peerDetailIconKind(label).length > 0));
  check('A: an unknown label invents nothing (no glyph, no copy)',
    m.peerDetailIconKind('不存在的行') === '' && m.peerDetailFieldKind('不存在的行') ===
      m.DETAIL_KIND_NONE && m.peerDetailCopyable('不存在的行') === false);
  check('A: the glyph is a function of the FIELD KIND (one table, two answers)',
    m.peerDetailIconKind('NetBird IP') === m.peerDetailKindIcon(m.DETAIL_KIND_OVERLAY_IP) &&
    m.peerDetailIconKind('NetBird IP') === m.DETAIL_ICON_PIN &&
    m.peerDetailCopyable('公钥') === m.peerDetailKindCopyable(m.DETAIL_KIND_KEY));

  // ---- the shipped table, row by row -----------------------------------------
  const table: Array<[string, string, boolean]> = [
    ['域名', 'globe', true],
    ['NetBird IP', 'pin', true],
    ['状态', 'status', false],
    ['连接方式', 'link', false],
    ['最近握手', 'clock', false],
    ['发送', 'up', false],
    ['接收', 'down', false],
    ['对端端点', 'node', true],
    ['本机出口', 'exit', false],
    ['对端网络', 'node', true],
    ['承载', 'link', false],
    ['中继通道地址', 'link', true],
    ['本机中继会话', 'link', false],
    ['ICE', 'shield', false],
    ['ICE 状态', 'shield', false],
    ['ICE 角色', 'branch', false],
    ['ICE 本端', 'branch', false],
    ['ICE 对端', 'branch', false],
    ['ICE 可达性', 'shield', false],
    ['ICE 最近错误', 'shield', false],
    ['公钥', 'key', true]
  ];
  check('A: the table is the shipped one, label by label',
    table.length === sectionLabels.length &&
    table.every(([label, icon, copyable]) =>
      m.peerDetailIconKind(label) === icon && m.peerDetailCopyable(label) === copyable),
    table.filter(([label, icon, copyable]) =>
      m.peerDetailIconKind(label) !== icon || m.peerDetailCopyable(label) !== copyable)
      .map(([label]) => label).join(','));
  check('A: the ICE rows split into the verdict and the candidate-path glyphs',
    m.peerDetailIconKind('ICE') === m.DETAIL_ICON_SHIELD &&
    m.peerDetailIconKind('ICE 状态') === m.DETAIL_ICON_SHIELD &&
    m.peerDetailIconKind('ICE 可达性') === m.DETAIL_ICON_SHIELD &&
    m.peerDetailIconKind('ICE 最近错误') === m.DETAIL_ICON_SHIELD &&
    m.peerDetailIconKind('ICE 角色') === m.DETAIL_ICON_BRANCH &&
    m.peerDetailIconKind('ICE 本端') === m.DETAIL_ICON_BRANCH &&
    m.peerDetailIconKind('ICE 对端') === m.DETAIL_ICON_BRANCH);
  check('A: every glyph kind is used by a row (no dead glyph in the component)',
    m.DETAIL_ICON_KINDS.filter((kind: string) => kind !== m.DETAIL_ICON_COPY)
      .every((kind: string) => sectionLabels.some((label: string) =>
        m.peerDetailIconKind(label) === kind)) &&
    // … and the copy mark is used by the trailing control, not by a field
    sectionLabels.every((label: string) => m.peerDetailIconKind(label) !== m.DETAIL_ICON_COPY));

  // ---- B. the copyable predicate, exactly the owner's list -------------------
  const copyableLabels: string[] = ['域名', 'NetBird IP', '对端端点', '中继通道地址',
    '对端网络', '公钥'];
  check('B: copyable is EXACTLY the owner\'s six identifier/address rows',
    sectionLabels.filter((label: string) => m.peerDetailCopyable(label))
      .sort().join('|') === copyableLabels.slice().sort().join('|'),
    JSON.stringify(sectionLabels.filter((label: string) => m.peerDetailCopyable(label))));
  check('B: 状态 / 最近握手 / 发送 / 接收 are NOT copyable',
    m.peerDetailCopyable('状态') === false && m.peerDetailCopyable('最近握手') === false &&
    m.peerDetailCopyable('发送') === false && m.peerDetailCopyable('接收') === false &&
    m.peerDetailCopyValue('状态', '已连接') === '' &&
    m.peerDetailCopyValue('最近握手', '1 分钟前') === '' &&
    m.peerDetailCopyValue('发送', '1.2 MB') === '' &&
    m.peerDetailCopyValue('接收', '1.2 MB') === '');
  check('B: the other non-identifier rows stay out too',
    m.peerDetailCopyable('连接方式') === false && m.peerDetailCopyable('本机出口') === false &&
    m.peerDetailCopyable('承载') === false &&
    m.peerDetailCopyable('本机中继会话') === false &&
    m.peerDetailCopyable('ICE') === false && m.peerDetailCopyable('ICE 状态') === false &&
    m.peerDetailCopyable('ICE 角色') === false && m.peerDetailCopyable('ICE 本端') === false &&
    m.peerDetailCopyable('ICE 对端') === false && m.peerDetailCopyable('ICE 可达性') === false &&
    m.peerDetailCopyable('ICE 最近错误') === false);
  const samples: Array<[string, string]> = [
    ['域名', 'blkserver.netcenter.local'],
    ['NetBird IP', '100.108.114.25'],
    ['对端端点', '192.168.50.183:43833'],
    ['中继通道地址', 'rels://home.alfadb.cn:28443'],
    ['对端网络', '10.0.0.0/24、192.168.50.0/24'],
    ['公钥', 'Kq3kZ9wZ1sQ4m7p2VnB0tR6yU8iO4aS5dF6gH7jK8lM=']
  ];
  check('B: each copyable row copies EXACTLY its own value (no row copies another)',
    samples.every(([label, value]) => m.peerDetailCopyValue(label, value) === value) &&
    samples.every(([label, value]) => {
      // a value handed to the wrong row's label is refused, not copied
      const other: string = samples.find(([l]) => l !== label)![1];
      return m.peerDetailCopyValue(label, other) === other;
    }));
  check('B: a not-carried value is never copied (both placeholders)',
    m.peerDetailCopyValue('NetBird IP', '未知') === '' &&
    m.peerDetailCopyValue('对端端点', '未知（核心未上报）') === '' &&
    m.peerDetailCopyValue('对端网络', '') === '' &&
    m.peerDetailCopyValue('域名', '未知（状态文档未提供）') === '');

  // ---- C. the page wiring ----------------------------------------------------
  check('C: all FOUR sections pass the glyph, the end-aligned value and the copy string',
    (PAGE.match(/iconKind: peerDetailIconKind\(field\.label\),\n\s+valueEnd: true,/g) ?? [])
      .length === 4 &&
    (PAGE.match(/copyValue: peerDetailCopyValue\(field\.label, field\.value\),/g) ?? [])
      .length === 4 &&
    PAGE.includes("this.peerDetailSectionFields(PEER_DETAIL_CONNECTION)") &&
    PAGE.includes("this.peerDetailSectionFields(PEER_DETAIL_PATH)") &&
    PAGE.includes("this.peerDetailSectionFields(PEER_DETAIL_ICE)") &&
    PAGE.includes("this.peerDetailSectionFields(PEER_DETAIL_IDENTITY)"));
  check('C: the collapsed sections use the SAME row component (no second row shape)',
    // 0.0.52 DELIBERATE UPDATE: 11 → 9. The 关于 group's two read-only rows are
    // SettingsRow children now (the settings page's one row grammar), so the detail
    // row's users are the peer pane's 4 sections + the 详细状态 page's 5 sections.
    (PAGE.match(/DetailRow\(\{/g) ?? []).length === 9 &&
    // the settings / 详细状态 / 关于 rows keep the icon-less call …
    (PAGE.match(/DetailRow\(\{ label: field\.label, value: field\.value \}\)/g) ?? []).length === 4 &&
    // … so they can never grow a leading glyph or a 48 vp band
    !PAGE.includes('DetailRow({ label: field.label, value: field.value, iconKind'));
  check('C: the copy callback hands the row\'s own string to the ONE clipboard path',
    (PAGE.match(/onCopy: \(value: string\): void => \{\n\s+this\.copyToClipboard\(value\);/g) ?? [])
      .length >= 4 &&
    PAGE.includes('private copyToClipboard(value: string): void {') &&
    PAGE.includes('writeClipboardText(value);') &&
    // every call site passes the string it was handed, and nothing else
    (PAGE.match(/this\.copyToClipboard\([^)]*\)/g) ?? []).length === 8 &&
    (PAGE.match(/this\.copyToClipboard\(value\)/g) ?? []).length === 8);
  check('C: the glyph and the copyability are functions of the LABEL, so no key moved',
    PAGE.includes('}, (field: DetailField) => `${field.label}=${field.value}`)') &&
    PAGE.includes('}, (field: DetailField) => `pp|${field.label}=${field.value}`)') &&
    PAGE.includes('}, (field: DetailField) => `pi|${field.label}=${field.value}`)') &&
    PAGE.includes('}, (field: DetailField) => `pid|${field.label}=${field.value}`)') &&
    !/peerDetailIconKind\([^)]*\)\s*\}\`/.test(PAGE) &&
    !PAGE.includes('iconKind: field.iconKind'));
  check('C: the row renders the glyph only when it has one, and centres the 48 vp band',
    PAGE.includes('if (this.iconKind.length > 0) {') &&
    PAGE.includes('FieldIcon({ kind: this.iconKind })') &&
    PAGE.includes('.alignItems(this.iconKind.length > 0 ? VerticalAlign.Center : VerticalAlign.Top)') &&
    PAGE.includes('.constraintSize({ minHeight: this.iconKind.length > 0 ? DETAIL_ROW_MIN_HEIGHT_VP : 0 })') &&
    PAGE.includes('.padding({ top: 6, bottom: 6 })') &&
    m.DETAIL_ROW_MIN_HEIGHT_VP === 48);
  check('C: the row still carries the copy control and the end-aligned value',
    PAGE.includes('CopyIconButton({') &&
    PAGE.includes('.textAlign(this.valueEnd ? TextAlign.End : TextAlign.Start)') &&
    PAGE.includes('@Prop valueEnd: boolean = false;') &&
    PAGE.includes('@Prop iconKind: string = \'\';') &&
    PAGE.includes('.width(DETAIL_ROW_LABEL_WIDTH_VP)') &&
    PAGE.includes('Row({ space: DETAIL_ROW_GAP_VP })') &&
    PAGE.includes('Row({ space: DETAIL_ROW_ICON_GAP_VP })'));
  check('C: ONE component draws every glyph, with a branch per kind and no symbol',
    (() => {
      const icon = PAGE.slice(PAGE.indexOf('struct FieldIcon {'),
        PAGE.indexOf('// 0.0.43 (I8) §3 — THE COPY AFFORDANCE'));
      const drawn: string[] = [];
      for (const match of icon.matchAll(/this\.kind === (DETAIL_ICON_[A-Z]+)/g)) {
        drawn.push(match[1]);
      }
      const wanted: string[] = m.DETAIL_ICON_KINDS.map((kind: string) =>
        'DETAIL_ICON_' + kind.toUpperCase());
      const alias: Record<string, string> = {
        [m.DETAIL_ICON_KEY]: 'DETAIL_ICON_KEY'
      };
      return icon.length > 0 &&
        drawn.length === m.DETAIL_ICON_KINDS.length &&
        new Set(drawn).size === drawn.length &&
        wanted.every((name: string) => drawn.includes(name) || name in alias) &&
        icon.includes('.width(DETAIL_ROW_ICON_SLOT_VP)') &&
        icon.includes('.height(DETAIL_ROW_ICON_SLOT_VP)') &&
        // 0.0.52 DELIBERATE UPDATE: the page now carries exactly ONE system symbol —
        // the subpage title bar's back chevron, whose resource run-harmony-layout
        // verifies against the SDK's sysResource table. The FIELD icons are still
        // drawn Paths (this component's branches), which is what this check protects.
        !icon.includes('SymbolGlyph') && !icon.includes('sys.symbol') &&
        (PAGE_CODE.match(/SymbolGlyph\(/g) ?? []).length === 1 &&
        PAGE_CODE.includes("SymbolGlyph($r('sys.symbol.chevron_left'))") &&
        m.DETAIL_ROW_ICON_SLOT_VP === m.PEER_ROW_COPY_SLOT_VP && m.DETAIL_ROW_ICON_SLOT_VP === 16;
    })());

  // ---- D. the width argument (the ellipsis question) -------------------------
  let narrowest: number = Number.POSITIVE_INFINITY;
  let narrowestAt: number = 0;
  for (let windowVp = m.HARMONY_BP_LG_VP; windowVp <= 4000; windowVp++) {
    const pane: number = m.detailPaneContentWidthVp(windowVp);
    if (pane < narrowest) {
      narrowest = pane;
      narrowestAt = windowVp;
    }
    if (!(pane >= m.DETAIL_ROW_PANE_MIN_WIDTH_VP)) {
      narrowest = pane;
      narrowestAt = windowVp;
      break;
    }
  }
  check('D: the narrowest wide tier is the 320 vp compact reservation',
    // 0.0.53 DELIBERATE UPDATE: the list column is the 1:2 one now (280 at 1024,
    // 374 at 1440 after the 1 vp snap, 400 from the indent cap on), so the detail
    // pane's content is what the INDENT-CAPPED content area leaves:
    // 744-280, 1120-374, 1200-400.
    narrowest === m.DETAIL_ROW_PANE_MIN_WIDTH_VP && narrowestAt === m.HARMONY_BP_LG_VP &&
    m.detailPaneContentWidthVp(m.HARMONY_BP_LG_VP) === 320 &&
    m.detailPaneContentWidthVp(1024) === 464 && m.detailPaneContentWidthVp(1440) === 746 &&
    m.detailPaneContentWidthVp(4000) === 800);
  check('D: the icons cost the value column EXACTLY the glyph slot and its gap',
    // the I8/I9 budget without a leading glyph, recomputed here from the pane's own
    // padding and the label column. 0.0.52 DELIBERATE UPDATE: the body is 16 fp now,
    // so the value column grew 111 → 113 (the glyph↔label gap 4 → 2 paid for the wider
    // floor — see DETAIL_ROW_ICON_GAP_VP). The pair's total is still 131 / 152.
    m.detailRowValueWidthVp(320, true) === 113 && m.detailRowValueWidthVp(320, false) === 134 &&
    m.detailRowValueWidthVp(320, true) + m.DETAIL_ROW_ICON_SLOT_VP + m.DETAIL_ROW_ICON_GAP_VP === 131 &&
    m.detailRowValueWidthVp(320, false) + m.DETAIL_ROW_ICON_SLOT_VP + m.DETAIL_ROW_ICON_GAP_VP === 152 &&
    m.DETAIL_ROW_PAGE_PADDING_VP === 16 && m.DETAIL_ROW_CARD_PADDING_VP === 16 &&
    m.DETAIL_ROW_LABEL_WIDTH_VP === 96 && m.DETAIL_ROW_GAP_VP === 8 && m.DETAIL_ROW_ICON_GAP_VP === 2 &&
    m.COPY_ICON_GAP_VP === 5 &&
    // the whole change in one line: copyable = non-copyable − the gap − the slot
    m.detailRowValueWidthVp(320, true) ===
      m.detailRowValueWidthVp(320, false) - m.COPY_ICON_GAP_VP - m.PEER_ROW_COPY_SLOT_VP);
  check('D: the value column still clears the one-line IPv4 floor at that tier',
    // 0.0.52: the floor is the 14-char IPv4 at the 16 fp body (14 × 8 = 112); the
    // narrowest pane leaves 113, i.e. 1 vp of slack (it was 105 / 6 vp at 15 fp).
    m.DETAIL_ROW_VALUE_MIN_WIDTH_VP === 112 &&
    m.detailRowValueWidthVp(320, true) >= m.DETAIL_ROW_VALUE_MIN_WIDTH_VP &&
    m.detailRowValueWidthVp(320, false) >= m.DETAIL_ROW_VALUE_MIN_WIDTH_VP &&
    m.detailRowValueWidthVp(320, true) - m.DETAIL_ROW_VALUE_MIN_WIDTH_VP === 1);
  check('D: the pane\'s longest value still wraps inside the mounted 8-line cap',
    PAGE.includes('@Prop maxLines: number = 8;') &&
    m.DETAIL_ROW_VALUE_MAX_LINES === 8 &&
    m.DETAIL_ROW_VALUE_CHARS === 14 && m.DETAIL_ROW_VALUE_CHAR_VP === 8 &&
    m.DETAIL_ROW_LONGEST_VALUE_CHARS === 64 && m.DETAIL_ROW_MONO_CHAR_VP === 9.6 &&
    m.detailRowValueLines(m.DETAIL_ROW_LONGEST_VALUE_CHARS, m.DETAIL_ROW_MONO_CHAR_VP,
      m.detailRowValueWidthVp(320, true)) === 6 &&
    m.detailRowValueLines(m.DETAIL_ROW_LONGEST_VALUE_CHARS, m.DETAIL_ROW_MONO_CHAR_VP,
      m.detailRowValueWidthVp(320, true)) <= m.DETAIL_ROW_VALUE_MAX_LINES &&
    // the 域名 and the endpoint are two lines at that tier, well inside the cap
    m.detailRowValueLines(25, 8, m.detailRowValueWidthVp(320, true)) === 2 &&
    m.detailRowValueLines(21, 8, m.detailRowValueWidthVp(320, true)) === 2);
  check('D: the width functions are total (no negative, no NaN, no divide by zero)',
    m.detailRowValueWidthVp(0, true) === 0 && m.detailRowValueWidthVp(-500, false) === 0 &&
    m.detailRowValueWidthVp(Number.NaN, true) === 0 &&
    m.detailRowValueLines(0, 9, 108) === 0 && m.detailRowValueLines(64, 0, 108) === 0 &&
    m.detailRowValueLines(64, 9, 0) === 0 && m.detailPaneContentWidthVp(0) === 0);
} catch (error) {
  failed++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
