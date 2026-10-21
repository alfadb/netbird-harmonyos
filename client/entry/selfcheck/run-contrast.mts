// 对比度自检 (2026-10-21, host-side; no device, no build, no network).
//
// Scope: WCAG 2.1 contrast of the SHIPPED base/dark palettes, computed from the two
// real colour resource files (the same files the HAP packages). It exists because
// "the muted ink is a bit light" is a measurement, not a taste:
//
//   * every text role (primary / secondary / tertiary / muted) must clear AA (4.5:1)
//     on BOTH surfaces text is painted on — nb_page_background and
//     nb_card_background — and on the two ROW TINTS the list paints text onto
//     (nb_row_hover_background / nb_row_selected_background);
//   * every ink that carries a GLYPH or a STATUS DOT (the title inks, the action
//     role, the three status roles and nb_border, which is the 离线 dot) must clear
//     the 3:1 non-text threshold on those same two surfaces;
//   * the strokes and separators (nb_border, nb_card_border, nb_row_divider) must be
//     VISIBLE against the card they outline (>= 1.5:1), and their ordering — the card
//     outline stronger than the internal hairline — must survive the adjustment;
//   * every filled role's own on-ink (nb_on_primary / nb_on_neutral_button /
//     nb_on_status) must clear AA against its fill, because that is a button LABEL;
//   * the status inks must clear AA on the tint they are painted on (the chip and
//     verdict-line surfaces), which is what the base warning step missed by 0.38.
//
// Every computed pair is PRINTED with its real ratio, so a failure names the colour
// and the number instead of only saying "contrast check failed".
//
// Run: node client/entry/selfcheck/run-contrast.mts
// Exit 0 = all assertions pass; exit 1 = any failure.

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const MAIN = join(HERE, '..', 'src', 'main');
const PALETTES: Array<[string, string]> = [
  ['base', join(MAIN, 'resources', 'base', 'element', 'color.json')],
  ['dark', join(MAIN, 'resources', 'dark', 'element', 'color.json')],
];

let passed = 0;
let failed = 0;
function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (detail !== undefined && detail.length > 0) { console.log(`     ${detail}`); }
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

// ---- WCAG 2.1 relative luminance / contrast --------------------------------
function channel(hex: string, index: number): number {
  return parseInt(hex.replace('#', '').slice(index * 2, index * 2 + 2), 16) / 255;
}

function relativeLuminance(hex: string): number {
  const linear: number[] = [0, 1, 2].map((index: number): number => {
    const c: number = channel(hex, index);
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
}

function contrast(fgHex: string, bgHex: string): number {
  const a: number = relativeLuminance(fgHex);
  const b: number = relativeLuminance(bgHex);
  const hi: number = a > b ? a : b;
  const lo: number = a > b ? b : a;
  return (hi + 0.05) / (lo + 0.05);
}

const AA_TEXT: number = 4.5;
const NON_TEXT: number = 3;
const VISIBLE_STROKE: number = 1.5;

const palettes: Array<[string, Map<string, string>]> = PALETTES.map(
  (entry: [string, string]): [string, Map<string, string>] =>
    [entry[0], readPalette(entry[1])]);

function colorOf(palette: Map<string, string>, name: string): string {
  const value: string | undefined = palette.get(name);
  return value === undefined ? '#000000' : value;
}

// A pair that failed, stated with the two colours and the real ratio.
interface Failure {
  role: string;
  fg: string;
  bg: string;
  ratio: number;
  floor: number;
}

function evaluate(failures: Failure[], role: string, fg: string, bg: string,
  floor: number): number {
  const ratio: number = contrast(fg, bg);
  if (ratio + 1e-9 < floor) {
    failures.push({ role, fg, bg, ratio, floor });
  }
  return ratio;
}

function report(failures: Failure[]): string {
  return failures.map((f: Failure): string =>
    `${f.role}: ${f.fg} on ${f.bg} = ${f.ratio.toFixed(2)}:1 (needs ${f.floor}:1)`).join('\n     ');
}

const TEXT_SURFACES: string[] = ['nb_page_background', 'nb_card_background'];
const ROW_TINTS: string[] = ['nb_row_hover_background', 'nb_row_selected_background'];
const TEXT_ROLES: string[] = ['nb_text_primary', 'nb_text_secondary', 'nb_text_tertiary',
  'nb_text_muted'];
// Every ink that draws a glyph, an icon or a status dot rather than running text.
const NON_TEXT_ROLES: string[] = ['nb_primary', 'nb_text_primary', 'nb_text_secondary',
  'nb_text_muted', 'nb_status_success', 'nb_status_warning', 'nb_status_error', 'nb_border'];
const STROKE_ROLES: string[] = ['nb_border', 'nb_card_border', 'nb_row_divider'];
const INK_ON_FILL: Array<[string, string]> = [
  ['nb_on_primary', 'nb_primary'],
  ['nb_on_neutral_button', 'nb_neutral_button_background'],
  ['nb_on_status', 'nb_status_success'],
  ['nb_on_status', 'nb_status_warning'],
  ['nb_on_status', 'nb_status_error'],
];
// The status inks are painted on their own tint by the chips and the verdict lines.
const STATUS_ON_TINT: Array<[string, string]> = [
  ['nb_status_success', 'nb_status_success_background'],
  ['nb_status_warning', 'nb_status_warning_background'],
  ['nb_primary', 'nb_status_info_background'],
];

for (const [mode, palette] of palettes) {
  const surfaces: string = TEXT_SURFACES.concat(ROW_TINTS).join(', ');
  console.log(`--- ${mode} palette (text surfaces: ${surfaces}) ---`);

  const textFailures: Failure[] = [];
  const textLines: string[] = [];
  for (const role of TEXT_ROLES) {
    for (const surface of TEXT_SURFACES.concat(ROW_TINTS)) {
      const ratio: number = evaluate(textFailures, `${mode}:${role}`, colorOf(palette, role),
        colorOf(palette, surface), AA_TEXT);
      textLines.push(`${role} on ${surface} = ${ratio.toFixed(2)}:1`);
    }
  }
  check(`${mode}: every TEXT role clears 4.5:1 on page, card and both row tints`,
    textFailures.length === 0, textFailures.length > 0 ? report(textFailures)
      : textLines.join(', '));

  const glyphFailures: Failure[] = [];
  const glyphLines: string[] = [];
  for (const role of NON_TEXT_ROLES) {
    for (const surface of TEXT_SURFACES) {
      const ratio: number = evaluate(glyphFailures, `${mode}:${role}`,
        colorOf(palette, role), colorOf(palette, surface), NON_TEXT);
      glyphLines.push(`${role} on ${surface} = ${ratio.toFixed(2)}:1`);
    }
  }
  check(`${mode}: every TITLE/ICON/STATUS-DOT role clears 3:1 on page and card`,
    glyphFailures.length === 0, glyphFailures.length > 0 ? report(glyphFailures)
      : `${glyphLines.join(', ')} (lowest ${Math.min(...NON_TEXT_ROLES.flatMap(
        (role: string): number[] => TEXT_SURFACES.map((surface: string): number =>
          contrast(colorOf(palette, role), colorOf(palette, surface))))).toFixed(2)}:1)`);

  const strokeFailures: Failure[] = [];
  const strokeLines: string[] = [];
  for (const role of STROKE_ROLES) {
    const ratio: number = evaluate(strokeFailures, `${mode}:${role}`,
      colorOf(palette, role), colorOf(palette, 'nb_card_background'), VISIBLE_STROKE);
    strokeLines.push(`${role} on card = ${ratio.toFixed(2)}:1`);
  }
  check(`${mode}: every STROKE/SEPARATOR is visible on the card (>= 1.5:1)`,
    strokeFailures.length === 0, strokeFailures.length > 0 ? report(strokeFailures)
      : strokeLines.join(', '));

  // The 0.0.47 (I12) property the stronger strokes must not invert: the card's own
  // outline still outranks the internal hairline, so a list does not read as a grid.
  const cardEdge: number = contrast(colorOf(palette, 'nb_card_border'),
    colorOf(palette, 'nb_card_background'));
  const divider: number = contrast(colorOf(palette, 'nb_row_divider'),
    colorOf(palette, 'nb_card_background'));
  check(`${mode}: the card outline stays stronger than the internal hairline`,
    cardEdge > divider,
    `card_border ${cardEdge.toFixed(2)}:1 > nb_row_divider ${divider.toFixed(2)}:1`);

  const inkFailures: Failure[] = [];
  const inkLines: string[] = [];
  for (const [ink, fill] of INK_ON_FILL) {
    const ratio: number = evaluate(inkFailures, `${mode}:${ink}`,
      colorOf(palette, ink), colorOf(palette, fill), AA_TEXT);
    inkLines.push(`${ink} on ${fill} = ${ratio.toFixed(2)}:1`);
  }
  check(`${mode}: every filled role's ON-INK is readable (>= 4.5:1, it is a label)`,
    inkFailures.length === 0, inkFailures.length > 0 ? report(inkFailures)
      : inkLines.join(', '));

  const tintFailures: Failure[] = [];
  const tintLines: string[] = [];
  for (const [ink, tint] of STATUS_ON_TINT) {
    const ratio: number = evaluate(tintFailures, `${mode}:${ink}`,
      colorOf(palette, ink), colorOf(palette, tint), AA_TEXT);
    tintLines.push(`${ink} on ${tint} = ${ratio.toFixed(2)}:1`);
  }
  check(`${mode}: every STATUS ink is readable on the tint it is painted on (>= 4.5:1)`,
    tintFailures.length === 0, tintFailures.length > 0 ? report(tintFailures)
      : tintLines.join(', '));
}

// Both palettes must actually carry every role this check measures — a renamed or
// dropped resource would otherwise be silently evaluated as black.
const measured: string[] = TEXT_ROLES.concat(NON_TEXT_ROLES, STROKE_ROLES,
  TEXT_SURFACES, ROW_TINTS, INK_ON_FILL.flat(), STATUS_ON_TINT.flat())
  .filter((name: string, index: number, all: string[]): boolean => all.indexOf(name) === index);
const absent: string[] = [];
for (const [mode, palette] of palettes) {
  for (const name of measured) {
    if (!palette.has(name)) { absent.push(`${mode}:${name}`); }
  }
}
check('both palettes declare every role this check measures', absent.length === 0,
  absent.join(', '));

// The two palettes must be measured on the same role set (a mode-only role would make
// one mode's numbers meaningless).
const baseNames: string = [...(palettes[0][1].keys())].sort().join(',');
const darkNames: string = [...(palettes[1][1].keys())].sort().join(',');
check('base and dark declare the same role set', baseNames === darkNames);

console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
