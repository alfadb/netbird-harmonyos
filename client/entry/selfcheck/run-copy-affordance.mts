// 0.0.43 (I8) §3 — host-side checks for the COPY affordance. No device, no build.
// 0.0.44 (I9) widened half B's copyable set to the owner's identifier/address list
// and moved the GLYPH into the shared FieldIcon component; those two assertions were
// re-pointed at the shipped code without changing what the file proves.
//
// Three halves, all against the shipped sources:
//
//   A. THE CALL PATH, against a stubbed platform. The real
//      vpnextensionability/NetBirdClipboard.ets is staged byte-for-byte and driven
//      through copy-affordance-hooks.mjs's recording @ohos.pasteboard stub, so the
//      EXACT string the app writes is observed ON the shipped call path (createData
//      -> getSystemPasteboard().setDataSync) rather than on a mirror of it. The
//      same stub only implements the WRITE half of the namespace: a read API in the
//      production module would fail here instead of passing silently, which is what
//      makes "no ACL" (no ohos.permission.READ_PASTEBOARD) a checked property.
//   B. THE POLICY: which surfaces show the icon — the list rows on hover/selection
//      only, the detail pane and the rail ALWAYS, the phone's rows never — and which
//      VALUES are copyable at all. 0.0.44 (I9) made that the owner's identifier /
//      address set (域名, NetBird IP, 对端端点, 中继通道地址, 对端网络, 公钥) and
//      moved its full label-by-label table into run-detail-icons.mts; this file
//      keeps the surface policy and the two ends of the set.
//   C. THE ROW KEY: the icon's presence is an ATTRIBUTE driven by observed state,
//      so peerRowKey gained no hover/selection/copy input. That is the 0.0.35
//      lesson's other direction: a key that changed with the pointer would rebuild
//      both rows on every pointer move.
//
// Run: node client/entry/selfcheck/run-copy-affordance.mts
// Exit 0 = all assertions pass.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./copy-affordance-hooks.mjs', import.meta.url).href);

const here = dirname(fileURLToPath(import.meta.url));
const ets = join(here, '..', 'src', 'main', 'ets');
const PAGE = readFileSync(join(ets, 'pages', 'Index.ets'), 'utf8');
const LAYOUT = readFileSync(join(ets, 'pages', 'NetBirdHarmonyLayout.ets'), 'utf8');
const MODEL = readFileSync(join(ets, 'vpnextensionability', 'NetBirdUiModel.ets'), 'utf8');
const CLIPBOARD = readFileSync(join(ets, 'vpnextensionability', 'NetBirdClipboard.ets'), 'utf8');

const globals = globalThis as unknown as Record<string, unknown>;

let passed = 0;
let failed = 0;
function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else {
    failed++;
    if (detail !== undefined) { console.log(`     ${detail}`); }
  }
}

const dir = mkdtempSync(join(tmpdir(), 'nb-copy-'));

// Comments legitimately NAME things the code must not do (the clipboard module's
// header cites the reading side's permission; the two components' own comments say
// they have no hover rule). The negative assertions below are about CODE, so they
// run over the comment-stripped text.
function stripComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');
}
const PAGE_CODE = stripComments(PAGE);
const CLIPBOARD_CODE = stripComments(CLIPBOARD);

try {
  // ---- A. the shipped write path, against the recording pasteboard stub --------
  const written: Array<{ mimeType: string; value: string }> = [];
  const handed: Array<{ mimeType: string; value: string }> = [];
  globals.__NB_COPY = { created: handed, written: written };

  writeFileSync(join(dir, 'NetBirdClipboard.ts'), CLIPBOARD);
  const clipboard = await import(pathToFileURL(join(dir, 'NetBirdClipboard.ts')).href);

  const IP = '100.108.114.25';
  check('A: the write returns true for a real value',
    clipboard.writeClipboardText(IP) === true);
  check('A: createData got the plain-text MIME type and EXACTLY the value',
    handed.length === 1 && handed[0].mimeType === 'text/plain' && handed[0].value === IP,
    JSON.stringify(handed));
  check('A: setDataSync wrote that same PasteData — the value the caller passed',
    written.length === 1 && written[0].value === IP && written[0].mimeType === 'text/plain',
    JSON.stringify(written));
  check('A: an EMPTY value is refused here and never reaches the clipboard',
    clipboard.writeClipboardText('') === false && handed.length === 1 && written.length === 1);
  check('A: the seam is the public write API and nothing reads the clipboard (no ACL)',
    CLIPBOARD.includes("import pasteboard from '@ohos.pasteboard';") &&
    CLIPBOARD.includes('pasteboard.createData(CLIPBOARD_TEXT_MIME, text)') &&
    CLIPBOARD.includes('pasteboard.getSystemPasteboard().setDataSync(data)') &&
    !/getData|hasData|ohos\.permission|READ_PASTEBOARD/.test(CLIPBOARD_CODE) &&
    // the citations the module's own header carries (the SDK lines the API is
    // verified against; READ_PASTEBOARD is named only as the reading side's)
    CLIPBOARD.includes('@ohos.pasteboard.d.ts:71') &&
    CLIPBOARD.includes('@ohos.pasteboard.d.ts:163') &&
    CLIPBOARD.includes('@ohos.pasteboard.d.ts:254') &&
    CLIPBOARD.includes('@ohos.pasteboard.d.ts:1434'));

  // ---- the pure policy modules, byte-identical --------------------------------
  writeFileSync(join(dir, 'NetBirdHarmonyLayout.ts'), LAYOUT);
  writeFileSync(join(dir, 'NetBirdUiModel.ts'), MODEL);
  const m = await import(pathToFileURL(join(dir, 'NetBirdHarmonyLayout.ts')).href);
  const ui = await import(pathToFileURL(join(dir, 'NetBirdUiModel.ts')).href);

  // ---- B. which VALUES are copyable -------------------------------------------
  // 0.0.44 (I9): the owner's identifier/address list. The full table (every
  // rendered label, its glyph and its copyability) is asserted in
  // run-detail-icons.mts; here are the two ends of the set plus the rows the owner
  // excluded, and the fact that the predicate IS a predicate (no hand-kept list in
  // the page).
  check('B: the identifier/address rows copy their own value (I9 widened I8\'s two)',
    m.peerDetailCopyValue('域名', 'alfadbphone.netcenter.local') ===
      'alfadbphone.netcenter.local' &&
    m.peerDetailCopyValue('NetBird IP', IP) === IP &&
    m.peerDetailCopyValue('对端端点', '192.168.50.183:43833') === '192.168.50.183:43833' &&
    m.peerDetailCopyValue('中继通道地址', 'rels://home.alfadb.cn:28443') ===
      'rels://home.alfadb.cn:28443' &&
    m.peerDetailCopyValue('对端网络', '10.0.0.0/24、192.168.50.0/24') ===
      '10.0.0.0/24、192.168.50.0/24' &&
    m.peerDetailCopyValue('公钥', 'aVeryLongPublicKeyThatIsCopied') ===
      'aVeryLongPublicKeyThatIsCopied');
  check('B: the state/time/traffic/bearer/ICE rows the owner excluded stay excluded',
    m.peerDetailCopyValue('状态', '已连接') === '' &&
    m.peerDetailCopyValue('连接方式', '直连') === '' &&
    m.peerDetailCopyValue('最近握手', '1 分钟前') === '' &&
    m.peerDetailCopyValue('发送', '12.3 MB') === '' &&
    m.peerDetailCopyValue('接收', '4.5 MB') === '' &&
    m.peerDetailCopyValue('本机出口', '192.168.50.12') === '' &&
    m.peerDetailCopyValue('承载', '中继') === '' &&
    m.peerDetailCopyValue('本机中继会话', 'rels://home.alfadb.cn:28443') === '' &&
    m.peerDetailCopyValue('ICE 对端', '1.2.3.4:51820') === '' &&
    m.peerDetailCopyValue('ICE 可达性', '可达') === '');
  check('B: a not-carried value is not copied either (no「未知」on the clipboard)',
    m.peerDetailCopyValue('域名', '未知') === '' && m.peerDetailCopyValue('NetBird IP', '未知') === '' &&
    // the rows' own other placeholder — the collapsed 路径 rows fall back to it
    m.peerDetailCopyValue('对端端点', '未知（核心未上报）') === '');
  check('B: the copy policy is a PREDICATE over the row kind, not a list in the page',
    m.peerDetailCopyable('公钥') === true && m.peerDetailCopyable('状态') === false &&
    m.peerDetailKindCopyable(m.DETAIL_KIND_KEY) === true &&
    m.peerDetailKindCopyable(m.DETAIL_KIND_STATE) === false &&
    m.peerDetailKindCopyable(m.DETAIL_KIND_HANDSHAKE) === false &&
    m.peerDetailKindCopyable(m.DETAIL_KIND_TX) === false &&
    m.peerDetailKindCopyable(m.DETAIL_KIND_RX) === false &&
    !PAGE.includes('peerDetailCopyValue(field.label) ===') &&
    (PAGE.match(/copyValue: peerDetailCopyValue\(field\.label, field\.value\),/g) ?? [])
      .length === 4);

  // ---- B. which SURFACES show it ---------------------------------------------
  check('B: the LIST rows show the icon only on the pointer tiers (hover/selection)',
    // the tier: the split shell from the medium rail tier up, never the phone branch
    m.peerRowCopyVisible(1024) === true && m.peerRowCopyVisible(1250) === true &&
    m.peerRowCopyVisible(1440) === true && m.peerRowCopyVisible(4000) === true &&
    m.peerRowCopyVisible(840) === false && m.peerRowCopyVisible(1023) === false &&
    // the narrow (phone/tablet) branch: no hover, so no slot at all — its rows keep
    // today's bytes and the copy affordance there is the detail pane's
    m.peerRowCopyVisible(600) === false && m.peerRowCopyVisible(320) === false &&
    m.peerRowCopyVisible(839) === false && m.peerRowCopySlotVp(600) === 0 &&
    // the TIER gate and the POINTER gate are two different questions in the page:
    // the slot exists for every row of the tier (so a hidden icon keeps its place
    // and hover cannot reflow the list) and only `visible` follows the pointer
    PAGE.includes('private peerRowCopySlotVisible(): boolean {') &&
    PAGE.includes('return peerRowCopyVisible(this.widthVp);') &&
    PAGE.includes('private peerRowCopyIconVisible(peer: UiPeerEntry): boolean {') &&
    PAGE.includes('return this.peerRowHovered(peer) || this.peerRowSelected(peer);'));

  const row = PAGE.slice(PAGE.indexOf('peerRow(peer: UiPeerEntry) {'),
    PAGE.indexOf('peersTab() {'));
  check('B: the row\'s NAME icon carries the hostname and the IP icon carries the IP',
    (row.match(/if \(this\.peerRowCopySlotVisible\(\)\) \{/g) ?? []).length === 2 &&
    row.includes('visible: this.peerRowCopyIconVisible(peer) && peer.name.length > 0,') &&
    row.includes('visible: this.peerRowCopyIconVisible(peer) && peer.ip.length > 0,') &&
    row.includes('value: peer.name,') && row.includes('value: peer.ip,') &&
    // BOTH call the page's ONE copy path, which writes exactly what it is handed
    (row.match(/this\.copyToClipboard\(value\);/g) ?? []).length === 2 &&
    // and neither of them is an always-on icon: the `visible` gate above is the whole
    // pointer policy, and there is no `visible: false` anywhere in the row
    !row.includes('visible: false'));
  check('B: the row keeps the two-line structure and the single-line IP rule',
    row.includes('Text(peer.name)') &&
    row.includes("Text(peer.ip.length > 0 ? peer.ip : '无 IP')") &&
    row.includes('.maxLines(1)') &&
    // the IP's own floor is the layout module's, swept at every window there
    m.peerRowIpWidthFor(1024) >= m.PEER_ROW_IP_MIN_WIDTH_VP &&
    m.peerRowIpWidthFor(840) >= m.PEER_ROW_IP_MIN_WIDTH_VP);

  const detailRow = PAGE.slice(PAGE.indexOf('struct DetailRow {'),
    PAGE.indexOf('struct DetailSectionHeader {'));
  const railRow = PAGE.slice(PAGE.indexOf('struct RailInfoRow {'),
    PAGE.indexOf("// 0.0.37 (I2) — THE PANE'S EMPTY / ERROR STATE"));
  check('B: the DETAIL and the RAIL show it ALWAYS (no hover term in either component)',
    detailRow.includes('if (this.copyValue.length > 0) {') &&
    railRow.includes('if (this.copyValue.length > 0) {') &&
    !/hover/i.test(stripComments(detailRow)) && !/hover/i.test(stripComments(railRow)) &&
    detailRow.includes('visible: true,') && railRow.includes('visible: true,'));
  check('B: the detail pane wires its copyable rows through the label policy',
    (PAGE.match(/copyValue: peerDetailCopyValue\(field\.label, field\.value\),/g) ?? [])
      .length === 4 &&
    (PAGE.match(/onCopy: \(value: string\): void => \{\n\s+this\.copyToClipboard\(value\);/g) ?? [])
      .length >= 4);
  check('B: the rail wires exactly its local IP and 域名, and NOT its 对端 count',
    (PAGE.match(/copyValue: this\.homeAddressText\(\),/g) ?? []).length === 1 &&
    (PAGE.match(/copyValue: this\.homeFqdnText\(\),/g) ?? []).length === 1 &&
    (PAGE.match(/copyValue: this\.homePeersText\(\),/g) ?? []).length === 0 &&
    // exactly FOUR icon call sites exist in the whole page: two in the peer row,
    // one in the detail row, one in the rail row — no other surface has one
    (PAGE.match(/CopyIconButton\(\{/g) ?? []).length === 4);
  check('B: the phone branch renders the SAME row builder (no phone-only icon branch)',
    // one builder, two branches: the narrow Tabs and the wide pane both render
    // peerListBody(), so the phone's rows cannot grow an icon of their own
    PAGE.includes('private peerRowCopyIconVisible(peer: UiPeerEntry): boolean {') &&
    (PAGE.match(/this\.peerListBody\(\)/g) ?? []).length >= 1 &&
    !PAGE.includes('phoneCopy') && !PAGE.includes('isPhone'));

  // ---- B. the confirmation and the glyph --------------------------------------
  check('B: a copy is confirmed with a SHORT toast and no value in the message',
    PAGE.includes("const COPY_DONE_TEXT: string = '已复制';") &&
    PAGE.includes('const COPY_TOAST_MS: number = 2000;') &&
    PAGE.includes('message: COPY_DONE_TEXT, duration: COPY_TOAST_MS') &&
    PAGE.includes('writeClipboardText(value);') &&
    // the log carries the LENGTH, never the copied string
    PAGE.includes("'UI_COPY|bundle=%{public}s|length=%{public}d'"));
  const icon = PAGE.slice(PAGE.indexOf('struct CopyIconButton {'),
    PAGE.indexOf('// One label/value row of the detail screens'));
  // 0.0.44 (I9): the drawing moved into the ONE shared FieldIcon component, so the
  // copy mark is asserted on its own branch of that switch, and CopyIconButton is
  // asserted to RENDER it rather than to re-draw it (one drawing of each mark).
  const glyphs = PAGE.slice(PAGE.indexOf('struct FieldIcon {'),
    PAGE.indexOf('// 0.0.43 (I8) §3 — THE COPY AFFORDANCE'));
  const copyGlyph = glyphs.slice(glyphs.indexOf('DETAIL_ICON_COPY) {'),
    glyphs.indexOf('.width(DETAIL_ROW_ICON_SLOT_VP)'));
  check('B: the glyph is DRAWN (two overlapping rounded sheets) — no unverifiable symbol',
    copyGlyph.length > 0 &&
    (copyGlyph.match(/\.offset\(\{ x: -?3, y: -?3 \}\)/g) ?? []).length === 2 &&
    (copyGlyph.match(/\.borderRadius\(2\)/g) ?? []).length === 2 &&
    (copyGlyph.match(/\.border\(\{ width: 1\.5,/g) ?? []).length === 2 &&
    icon.includes('FieldIcon({ kind: DETAIL_ICON_COPY })') &&
    !PAGE_CODE.includes('sys.symbol') && !PAGE_CODE.includes('SymbolGlyph'));
  check('B: the touch target is 40x40 though the glyph is 16, and the slot never reflows',
    icon.includes('width: PEER_ROW_COPY_TOUCH_VP') &&
    icon.includes('height: PEER_ROW_COPY_TOUCH_VP') &&
    icon.includes('.responseRegion({') &&
    m.PEER_ROW_COPY_TOUCH_VP === 40 && m.PEER_ROW_COPY_GLYPH_VP === 16 &&
    // 0.0.43 device-frame fix: the slot IS the glyph (16), because it is reserved on
    // every row of the tier — a 24 vp slot raised the line boxes and measured 74 vp
    // rows against their neighbours' 64.
    m.PEER_ROW_COPY_SLOT_VP === m.PEER_ROW_COPY_GLYPH_VP && m.PEER_ROW_COPY_SLOT_VP === 16 &&
    m.PEER_ROW_COPY_TOUCH_VP > m.PEER_ROW_COPY_SLOT_VP &&
    // Hidden keeps the placeholder in the layout (Visibility.None would drop it and
    // shorten the row the moment the pointer arrived)
    icon.includes('.visibility(this.visible ? Visibility.Visible : Visibility.Hidden)') &&
    !icon.includes('Visibility.None'));

  // ---- C. the row key did not gain the copy/hover state -----------------------
  const peer = {
    pub_key: 'K', name: 'n', ip: 'i', online: true, connection: 'direct',
    status: 'connected'
  };
  check('C: the key is identity + the OBSERVED SELECTION, and nothing about a copy',
    ui.peerRowKey(peer) === ui.peerRowKey(Object.assign({}, peer,
      { hovered: true, copyVisible: true, copySlotVp: 24 })) &&
    ui.peerRowKey(peer, true) !== ui.peerRowKey(peer, false) &&
    ui.peerRowKey(peer) === ui.peerRowKey(peer, false));
  check('C: the page keys the list on (peer, selection) and never on the copy state',
    PAGE.includes('(peer: UiPeerEntry) => peerRowKey(peer, this.peerRowSelected(peer))') &&
    !PAGE.includes('peerRowKey(peer, this.peerRowHovered') &&
    !/peerRowKey\([^)]*peerRowCopyIconVisible/.test(PAGE) &&
    !/peerRowKey\([^)]*hover/i.test(PAGE));
  check('C: the icon\'s presence is an ATTRIBUTE read from observed state, not a field',
    // both icons live inside the builder, gated by the observed hover/selection and
    // by the tier — no @State of their own, no row field
    (row.match(/this\.peerRowCopyIconVisible\(peer\)/g) ?? []).length === 2 &&
    !row.includes('@State') && !row.includes('copyVisible:') &&
    m.peerRowCopyVisible(m.HARMONY_BP_LG_VP) === false);
} catch (error) {
  failed++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
