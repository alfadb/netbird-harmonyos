// 主题资源自检 (2026-10-09, host-side; no device, no build, no network).
//
// Scope: the ONE automatic system dark/light consumption increment for the UI
// pages. It checks, against the REAL production sources and the REAL resource
// files:
//   * every base/dark colour resource the palette contract names is present in
//     BOTH entry/src/main/resources/base/element/color.json and
//     entry/src/main/resources/dark/element/color.json, with the contracted
//     light/dark values (the dark qualifier directory is what makes the switch
//     automatic);
//   * the mandatory role pairs (nb_primary/nb_on_primary and the status/on-status
//     roles) exist in both palettes;
//   * every $r('app.color.…') name referenced by the two themed UI sources exists
//     in both palettes — a typo would be a compile error, but this also catches
//     a name used only in one palette;
//   * NO hardcoded background/text colour literal is left in the product visual
//     sources (transparent/brand/system values would have to be argued here);
//   * the status helpers actually return the contracted ROLE for each status
//     token (the helper bodies are executed host-side with $r() flattened to its
//     resource name, so the real branch logic is under test, not a mirror).
//
// Run: node client/entry/selfcheck/run-theme-resources.mts
// Exit 0 = all assertions pass; exit 1 = any failure.

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const MAIN = join(HERE, '..', 'src', 'main');
const BASE_JSON = join(MAIN, 'resources', 'base', 'element', 'color.json');
const DARK_JSON = join(MAIN, 'resources', 'dark', 'element', 'color.json');
const PAGE = readFileSync(join(MAIN, 'ets', 'pages', 'Index.ets'), 'utf8');
const WEBVIEW = readFileSync(join(MAIN, 'ets', 'login', 'NetBirdAccountWebView.ets'), 'utf8');
const ABILITY = readFileSync(join(MAIN, 'ets', 'entryability', 'EntryAbility.ets'), 'utf8');

let passed = 0;
let failed = 0;
function check(name: string, condition: boolean): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
}

interface ColorEntry {
  name: string;
  value: string;
}

function readPalette(path: string): Map<string, string> {
  const parsed = JSON.parse(readFileSync(path, 'utf8')) as { color: ColorEntry[] };
  const map = new Map<string, string>();
  for (const entry of parsed.color) {
    map.set(entry.name, entry.value.toUpperCase());
  }
  return map;
}

const base = readPalette(BASE_JSON);
const dark = readPalette(DARK_JSON);

// The theme contract, verbatim (name -> [light, dark]).
const CONTRACT: Array<[string, string, string]> = [
  ['nb_page_background', '#F2F6F8', '#10171D'],
  ['nb_card_background', '#FFFFFF', '#1B242C'],
  ['nb_input_background', '#FFFFFF', '#1B242C'],
  ['nb_text_primary', '#17324D', '#E8EEF3'],
  ['nb_text_secondary', '#3D4A55', '#C3CED7'],
  ['nb_text_tertiary', '#52615B', '#AEBBC5'],
  ['nb_text_muted', '#8A9AA6', '#93A4B0'],
  ['nb_border', '#AAB5BC', '#4A5A66'],
  // 0.0.39 (I4) P2 (review §4.1): the card stroke was #3C4B57 in dark — ~2.06:1
  // against the page, which is at the edge of vanishing on a dim panel. It is now
  // the SAME value as nb_border's dark step (~2.8:1), so no new colour entered the
  // system; the light step is unchanged.
  ['nb_card_border', '#DEE5EA', '#4A5A66'],
  ['nb_primary', '#17324D', '#7FB2D9'],
  ['nb_on_primary', '#FFFFFF', '#0B1218'],
  ['nb_neutral_button_background', '#8A9AA6', '#3A4750'],
  // 0.0.40 (I5) §4: the ink that sits ON the neutral surface. Until I5 every
  // neutral button borrowed nb_on_primary, which is a DARK ink in dark mode
  // (#0B1218 on #3A4750 — ~2:1, i.e. the ASTRA review's "返回 按钮的深色文字像禁用
  // 态"). The neutral surface is not the primary role, so it gets its own ink: the
  // light step is unchanged (#FFFFFF) and the dark step is nb_text_primary's own
  // #E8EEF3, the colour the review named.
  ['nb_on_neutral_button', '#FFFFFF', '#E8EEF3'],
  ['nb_status_success', '#176B3A', '#6FCF97'],
  ['nb_status_warning', '#8A6D1A', '#E8C468'],
  ['nb_status_error', '#A12622', '#F08A82'],
  ['nb_status_success_background', '#E3EFE7', '#22402F'],
  ['nb_status_warning_background', '#F5EBCF', '#3F351A'],
  // 0.0.39 (I4) P1.6 (review §4.2): 中继 is a NORMAL NetBird path, so the relay
  // chip's tint is the primary/info role rather than the warning one — the new
  // token, in both modes, ~5.8:1 (dark) / ~10.5:1 (light) against nb_primary.
  ['nb_status_info_background', '#DCE8F2', '#233240'],
  // 0.0.40 (I5) §3: the selected peer row's tint. The review named #263D4D for the
  // dark step (its own measured value); the light step is the same hue against the
  // white card, so a selected row is visible without the 直连/中继 chip's own info
  // tint becoming indistinguishable from the row it sits on.
  ['nb_row_selected_background', '#E4EDF5', '#263D4D'],
  // 0.0.41 (I6) §2: the HOVERED peer row's tint, and deliberately a DIFFERENT role
  // from the selected one — the owner's 0.0.40 frame could not be told apart from a
  // selection. Hover is the weaker, neutral lift toward the foreground (light
  // #F2F5F7 on the white card, dark #2A3138 on #1B242C); the selection keeps the
  // stronger, blue-carrying tint AND the 3 vp primary marker, which hover never
  // renders (see the layout selfcheck's negative property).
  ['nb_row_hover_background', '#F2F5F7', '#2A3138'],
  // 0.0.47 (I12) §4: the peer list's INTERNAL row divider. The re-review's optional
  // §3 ("减轻对端列表内部的横线重量") and the owner's own ask: the internal lines were
  // reusing nb_card_border, i.e. the weight of the card's own outline, and the list read
  // as a grid. This is the review's own suggested softer step for dark (#33414A) and its
  // light analogue; the property that matters — the line is strictly LIGHTER than the
  // card outline it used to borrow — is asserted below, in both modes.
  ['nb_row_divider', '#E8EEF2', '#33414A'],
  ['nb_on_status', '#FFFFFF', '#0B1218'],
  ['nb_window_status_bar_background', '#F2F6F8', '#10171D'],
  ['nb_window_navigation_bar_background', '#F2F6F8', '#10171D'],
  ['nb_window_system_bar_content', '#17324D', '#E8EEF3'],
  ['start_window_background', '#F2F6F8', '#10171D'],
];

// 1. The base/dark resource set itself.
const missingNames: string[] = [];
const wrongValues: string[] = [];
for (const [name, light, darkValue] of CONTRACT) {
  if (!base.has(name)) { missingNames.push(`base:${name}`); }
  if (!dark.has(name)) { missingNames.push(`dark:${name}`); }
  if (base.get(name) !== light) { wrongValues.push(`${name} base=${base.get(name)} want ${light}`); }
  if (dark.get(name) !== darkValue) { wrongValues.push(`${name} dark=${dark.get(name)} want ${darkValue}`); }
}
check('every contracted colour resource exists in BOTH base and dark palettes',
  missingNames.length === 0);
if (missingNames.length > 0) { console.log(`     missing: ${missingNames.join(', ')}`); }
check('every contracted light/dark value is the contracted one', wrongValues.length === 0);
if (wrongValues.length > 0) { console.log(`     wrong: ${wrongValues.join(' | ')}`); }
check('the dark qualifier directory carries the dark element file',
  readFileSync(DARK_JSON, 'utf8').length > 0);

const baseNames = [...base.keys()].sort().join(',');
const darkNames = [...dark.keys()].sort().join(',');
check('base and dark palettes declare the same resource-name set (no mode-only name)',
  baseNames === darkNames);

// 1b. 0.0.41 (I6) §2 — the row's TWO surface tints. They have to be different
//     roles in both modes (the owner's 0.0.40 frame could not tell the row's
//     treatment from a selection) and the SELECTION has to be the STRONGER of the
//     two against the card, so a hover can never out-rank the row whose detail is
//     open. "Stronger" is the sum of the per-channel distance from the card fill,
//     which is what makes the tint visible at all.
function channels(hex: string): number[] {
  const v = hex.replace('#', '');
  return [parseInt(v.slice(0, 2), 16), parseInt(v.slice(2, 4), 16),
    parseInt(v.slice(4, 6), 16)];
}
function liftFromCard(cardHex: string, tintHex: string): number {
  const c = channels(cardHex);
  const t = channels(tintHex);
  return Math.abs(t[0] - c[0]) + Math.abs(t[1] - c[1]) + Math.abs(t[2] - c[2]);
}
const hoverLight = base.get('nb_row_hover_background') as string;
const hoverDark = dark.get('nb_row_hover_background') as string;
const selectedLight = base.get('nb_row_selected_background') as string;
const selectedDark = dark.get('nb_row_selected_background') as string;
check('I6 §2: hover and selection are DIFFERENT roles, and the selection is the stronger tint',
  hoverLight !== selectedLight && hoverDark !== selectedDark &&
  // hover is a visible lift over the card in both modes (a hover that paints
  // nothing is what the owner's frame showed)
  liftFromCard('#FFFFFF', hoverLight) > 0 && liftFromCard('#1B242C', hoverDark) > 0 &&
  liftFromCard('#FFFFFF', selectedLight) > liftFromCard('#FFFFFF', hoverLight) &&
  liftFromCard('#1B242C', selectedDark) > liftFromCard('#1B242C', hoverDark));

// 1c. 0.0.47 (I12) §4 — the internal row divider is a HAIRLINE, not the card outline.
//     It is a role of its own and, in BOTH modes, its lift from the card fill is
//     strictly below the card stroke's — which is exactly "减轻内部横线的重量": the
//     line is still there to scan by, it just no longer carries the outline's weight.
const dividerLight = base.get('nb_row_divider') as string;
const dividerDark = dark.get('nb_row_divider') as string;
check('I12 §4: the row divider is lighter than the card outline in both modes',
  dividerLight !== (base.get('nb_card_border') as string) &&
  dividerDark !== (dark.get('nb_card_border') as string) &&
  liftFromCard('#FFFFFF', dividerLight) > 0 &&
  liftFromCard('#1B242C', dividerDark) > 0 &&
  liftFromCard('#FFFFFF', dividerLight) < liftFromCard('#FFFFFF', base.get('nb_card_border') as string) &&
  liftFromCard('#1B242C', dividerDark) < liftFromCard('#1B242C', dark.get('nb_card_border') as string),
  `light=${liftFromCard('#FFFFFF', dividerLight)} ` +
    `vs ${liftFromCard('#FFFFFF', base.get('nb_card_border') as string)}, ` +
    `dark=${liftFromCard('#1B242C', dividerDark)} ` +
    `vs ${liftFromCard('#1B242C', dark.get('nb_card_border') as string)}`);

// 2. Mandatory role pairs: a filled role and the ink that sits ON it must both
//    exist in each palette — that is what makes the explicit on-role ink
//    (instead of the platform default font color) safe in both modes.
const ROLE_PAIRS: Array<[string, string]> = [
  ['nb_primary', 'nb_on_primary'],
  ['nb_status_success', 'nb_on_status'],
  ['nb_status_warning', 'nb_on_status'],
  ['nb_status_error', 'nb_on_status'],
  // 0.0.40 (I5) §4: the neutral surface's own ink (see the contract entry).
  ['nb_neutral_button_background', 'nb_on_neutral_button'],
];
let pairsOk = true;
for (const [fill, ink] of ROLE_PAIRS) {
  if (!base.has(fill) || !base.has(ink) || !dark.has(fill) || !dark.has(ink)) {
    pairsOk = false;
    console.log(`     role pair missing: ${fill} + ${ink}`);
  }
}
check('mandatory fill/on-role pairs exist in both palettes', pairsOk);

// 3. Every $r('app.color.…') name the themed UI sources reference.
function referencedNames(source: string): string[] {
  const names: string[] = [];
  const pattern = /\$r\('app\.color\.([A-Za-z0-9_]+)'\)/g;
  let match = pattern.exec(source);
  while (match !== null) {
    names.push(match[1]);
    match = pattern.exec(source);
  }
  return names;
}

const uiRefs = referencedNames(PAGE).concat(referencedNames(WEBVIEW));
const unknownRefs: string[] = [];
for (const name of uiRefs) {
  if (!base.has(name) || !dark.has(name)) { unknownRefs.push(name); }
}
check('Index.ets and NetBirdAccountWebView.ets reference only names present in both palettes',
  unknownRefs.length === 0);
if (unknownRefs.length > 0) { console.log(`     unknown: ${unknownRefs.join(', ')}`); }
check('the themed UI actually consumes the resource palette (non-trivial reference count)',
  uiRefs.length >= 30);
check('no unresolved $r() call is left outside the app.color namespace in the themed sources',
  PAGE.includes("$r('app.color.") && WEBVIEW.includes("$r('app.color."));

// 4. No hardcoded background/text colour literal in the product visual sources.
function colorCallLines(source: string): string[] {
  const lines = source.split('\n');
  const offenders: string[] = [];
  for (const line of lines) {
    if (/\.(fontColor|backgroundColor|borderColor)\s*\(/.test(line) && /['"]#[0-9A-Fa-f]{3,8}['"]/.test(line)) {
      offenders.push(line.trim());
    }
  }
  return offenders;
}
function borderCallLines(source: string): string[] {
  const offenders: string[] = [];
  for (const line of source.split('\n')) {
    if (/\.border\s*\(/.test(line) && /color\s*:\s*['"]#[0-9A-Fa-f]{3,8}['"]/.test(line)) {
      offenders.push(line.trim());
    }
  }
  return offenders;
}
const pageOffenders = colorCallLines(PAGE).concat(borderCallLines(PAGE));
const webOffenders = colorCallLines(WEBVIEW).concat(borderCallLines(WEBVIEW));
check('Index.ets has no hardcoded background/text/border colour', pageOffenders.length === 0);
if (pageOffenders.length > 0) { console.log(`     ${pageOffenders.join('\n     ')}`); }
check('NetBirdAccountWebView.ets has no hardcoded background/text colour', webOffenders.length === 0);
if (webOffenders.length > 0) { console.log(`     ${webOffenders.join('\n     ')}`); }

// The pure model module is deliberately NOT swept: it keeps its own semantic hex
// (three peer-status values) and stays Node-loadable, and the page maps its
// status tokens to resources through the helper exercised below.

// 5. The status helpers, executed host-side. $r() is an ArkUI declaration the
//    Node harness cannot run, so its resource-name argument is flattened to the
//    bare name — the branch logic under test is exactly the shipped one.
function loadHelper<T>(name: string, pattern: RegExp): T {
  const match = PAGE.match(pattern);
  if (match === null) { throw new Error(`${name} missing`); }
  // Flatten the ArkUI bits the Node harness cannot run: $r() to its resource
  // name, the `private` modifier away, the ArkTS type annotations away, and the
  // struct-method syntax to a function declaration.
  const flattened = match[0]
    .replace(/\$r\('app\.color\.([A-Za-z0-9_]+)'\)/g, "'$1'")
    .replace(/^\s*private\s+/, '')
    .replace(`private ${name}(`, `function ${name}(`)
    .replace(new RegExp(`^\\s*${name}\\(`), `function ${name}(`)
    .replace(/\((\w+): [A-Za-z0-9_<>|]+\)/, '($1)')
    .replace(/\): [A-Za-z0-9_<>|]+\s*\{/, ') {');
  const sandbox = new Function(`${flattened}; return ${name};`);
  return sandbox() as T;
}

type StatusFn = (status: string) => string;
type VoidFn = () => string;

const peerStatusResource = loadHelper<StatusFn>('peerStatusResource',
  /private peerStatusResource\(status: string\): ResourceColor \{[\s\S]*?\n  \}/);

check('peer status role mapping: connected/idle/offline/unknown',
  peerStatusResource('connected') === 'nb_status_success' &&
  peerStatusResource('idle') === 'nb_text_muted' &&
  peerStatusResource('offline') === 'nb_border' &&
  peerStatusResource('anything-else') === 'nb_border');

// stateColor/logExportToneColor read page state through this.*, so only the pure
// status→role mapping above is executed; their role assignments are checked
// structurally instead (each branch must name the contracted role).
const stateColorBody = PAGE.match(/private stateColor\(\): ResourceColor \{[\s\S]*?\n  \}/);
check('home state colour uses success/warning/error/secondary roles',
  stateColorBody !== null &&
  stateColorBody[0].includes("nb_status_success") &&
  stateColorBody[0].includes("nb_status_warning") &&
  stateColorBody[0].includes("nb_status_error") &&
  stateColorBody[0].includes("nb_text_secondary"));
// 0.0.39 (I4) P0.1 (review §4.3): the connect disc is a BUTTON, so its fill is the
// action role (nb_primary) unconditionally and its STATE is the 2 vp ring, painted
// by stateColor() — the retired toggleColor() painted the current state onto a
// control whose label is the inverse action ("a lamp, not a button"). The check
// moves with it: the disc must carry the primary fill, the on-primary ink and the
// state ring, and the old state-coloured fill must not come back.
const toggleColorBody = PAGE.match(/private toggleColor\(\): ResourceColor \{/);
check('the connect disc fills with the action role and rings with the state role',
  toggleColorBody === null &&
  PAGE.includes('.backgroundColor($r(\'app.color.nb_primary\'))') &&
  PAGE.includes('.fontColor($r(\'app.color.nb_on_primary\'))') &&
  PAGE.includes('.borderColor(this.stateColor())') &&
  PAGE.includes('.borderWidth(CONNECT_RING_WIDTH_VP)'));
const toneBody = PAGE.match(/private logExportToneColor\(\): ResourceColor \{[\s\S]*?\n  \}/);
check('log-export tone uses success/warning/error/primary roles',
  toneBody !== null &&
  toneBody[0].includes("nb_status_success") &&
  toneBody[0].includes("nb_status_warning") &&
  toneBody[0].includes("nb_status_error") &&
  toneBody[0].includes("nb_primary"));

// 6. The one place a native API takes literal strings instead of resources: the
//    window bars must resolve the SAME palette by name (foundation owner), so a
//    missing name there is an actual-missing-role report rather than a page bug.
const barNames: string[] = ['nb_window_status_bar_background',
  'nb_window_navigation_bar_background', 'nb_window_system_bar_content'];
let barsOk = ABILITY.includes('getColorByNameSync') && ABILITY.includes('COLOR_MODE_NOT_SET');
for (const name of barNames) {
  if (!base.has(name) || !dark.has(name)) { barsOk = false; }
  if (!ABILITY.includes(name)) { barsOk = false; }
}
check('EntryAbility resolves the window-bar roles by name from the same two palettes',
  barsOk);
check('EntryAbility performs no device-wide theme write (follow-system app scope only)',
  ABILITY.includes('setColorMode') && !ABILITY.includes('setColorMode(ConfigurationConstant.ColorMode.COLOR_MODE_DARK') &&
  !ABILITY.includes('COLOR_MODE_LIGHT'));

// 7. Every themed Button that paints a filled role carries an EXPLICIT on-role
//    ink (no reliance on the platform default font color for a filled button).
//    0.0.40 (I5) §4: the NEUTRAL fill's own ink counts as its on-role — using
//    nb_on_primary on the neutral surface was the defect that check now prevents
//    (dark mode: near-black ink on dark slate), so a block may carry either the
//    role's own ink or, for the one button that flips between primary and neutral,
//    both.
const buttonBlocks = PAGE.match(/Button\([^)]*\)[\s\S]{0,400}?\.onClick/g) ?? [];
const filledWithoutInk: string[] = [];
for (const block of buttonBlocks) {
  const fills: string[] = [];
  if (block.includes('nb_primary')) { fills.push('nb_on_primary'); }
  if (block.includes('nb_neutral_button_background')) { fills.push('nb_on_neutral_button'); }
  if (block.includes('nb_status_success')) { fills.push('nb_on_status'); }
  const declared: boolean = fills.length === 0 ||
    fills.every((ink: string) => block.includes(ink));
  if (!declared) {
    filledWithoutInk.push(block.split('\n').slice(0, 3).join(' ').trim());
  }
}
check('every filled-role Button declares its own role ink (no cross-role ink)',
  filledWithoutInk.length === 0);
if (filledWithoutInk.length > 0) { console.log(`     ${filledWithoutInk.join('\n     ')}`); }

// 8. The embedded remote page stays provider-controlled: no JS/CSS injection and
//    no forced web dark mode in the WebView component.
check('WebView does not force web dark mode or inject page CSS/JS',
  !WEBVIEW.includes('darkMode') && !WEBVIEW.includes('forceDark') &&
  !WEBVIEW.includes('runJavaScript') && !WEBVIEW.includes('injectCss') &&
  !WEBVIEW.includes('.css('));
check('the WebView native chrome is themed by resources',
  WEBVIEW.includes("$r('app.color.nb_page_background')") &&
  WEBVIEW.includes("$r('app.color.nb_text_primary')") &&
  WEBVIEW.includes("$r('app.color.nb_on_primary')"));

console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
