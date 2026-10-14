// UI-literal typo sweep — 0.0.42 (I7) §2. Host-side: no device, no build, no
// network.
//
// Run: node client/entry/selfcheck/run-ui-literals.mts
//
// WHY A RUNNER AND NOT A ONE-OFF GREP. The owner's I7 finding was a duplicated
// character inside a user-visible sentence ("连接后可可在此搜索对端并查看详情。"). A
// single string can be fixed and silently regress; the property that has to hold is
// the CLASS, so this file pins it for EVERY string literal in the product sources
// under client/entry/src/main/ets (the page, the layout module, the tray / login /
// route / vpn modules) plus every string resource JSON:
//
//   no literal may contain the same CJK character twice in a row unless that
//   doubling is a legitimate Chinese word or two legitimate words meeting.
//
// Literals are extracted with a small scanner (line comments, block comments,
// single-quoted / double-quoted / backtick strings, escapes) so a hit in a COMMENT
// cannot fail the literal check and a hit in a literal cannot hide inside one. The
// sweep's evidence is printed: every allowed hit is reported with its file:line and
// the phrase that makes it legitimate.
//
// Exit 0 = no unexplained doubling in any literal; exit 1 = at least one.
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const MAIN = join(HERE, '..', 'src', 'main');
const ETS = join(MAIN, 'ets');

let passed = 0;
let failed = 0;
function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
  if (!condition && detail !== undefined) {
    console.log(`     ${detail}`);
  }
}

/** Every .ets file under the product source tree, in a stable order. */
function etsFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir).sort()) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      out.push(...etsFiles(full));
    } else if (entry.endsWith('.ets')) {
      out.push(full);
    }
  }
  return out;
}

/** Every string resource JSON under the product resource tree, plus app.json5. */
function resourceJsonFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir).sort()) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      out.push(...resourceJsonFiles(full));
    } else if (entry.endsWith('.json')) {
      out.push(full);
    }
  }
  return out;
}

interface Literal {
  /** The literal's own text, without its quotes. */
  text: string;
  /** 1-based line the literal OPENS on. */
  line: number;
}

/**
 * The string literals of one source: line comments, block comments and code
 * punctuation are skipped, `'…'` / `"…"` / `` `…` `` are collected with their
 * escape sequences honoured. A template's `${…}` is left inside the literal text —
 * an expression that repeats a character is worth the same complaint as a message.
 */
function literalsOf(source: string): Literal[] {
  const out: Literal[] = [];
  const n = source.length;
  let i = 0;
  let line = 1;
  while (i < n) {
    const c = source[i];
    if (c === '\n') {
      line++;
      i++;
      continue;
    }
    if (c === '/' && source[i + 1] === '/') {
      while (i < n && source[i] !== '\n') { i++; }
      continue;
    }
    if (c === '/' && source[i + 1] === '*') {
      i += 2;
      while (i < n && !(source[i] === '*' && source[i + 1] === '/')) {
        if (source[i] === '\n') { line++; }
        i++;
      }
      i += 2;
      continue;
    }
    if (c === '\'' || c === '"' || c === '`') {
      const quote = c;
      const openedAt = line;
      i++;
      const start = i;
      while (i < n && source[i] !== quote) {
        if (source[i] === '\\') {
          if (source[i + 1] === '\n') { line++; }
          i += 2;
          continue;
        }
        if (source[i] === '\n') { line++; }
        i++;
      }
      out.push({ text: source.slice(start, i), line: openedAt });
      i++;
      continue;
    }
    i++;
  }
  return out;
}

/** The doubled-CJK-character class the owner named (可可 / 并并 / 在在 / …). */
const DOUBLED = /([\u3400-\u9fff])\1/;

/**
 * The doublings that are NOT typos. Each entry must name the phrase the literal
 * really contains and why the repetition is legitimate — this list is the ONLY way
 * a hit can pass, so a new accidental 可可 fails the check instead of joining it.
 */
const ALLOWED: { phrase: string; why: string }[] = [
  { phrase: '对端端点', why: '「对端」(peer) + 「端点」(endpoint) — two words meeting' },
  { phrase: '刚刚', why: 'the Chinese word 刚刚 "just now" (a real reduplication)' }
];

interface Hit {
  file: string;
  line: number;
  pair: string;
  text: string;
  allowed: { phrase: string; why: string } | undefined;
}

const files: string[] = [...etsFiles(ETS), ...resourceJsonFiles(join(MAIN, 'resources'))];
const hits: Hit[] = [];
let literalCount = 0;
const caption = '连接后可在此搜索对端并查看详情。';
let captionCount = 0;

for (const file of files) {
  const source = readFileSync(file, 'utf8');
  const shown = relative(join(HERE, '..', '..', '..'), file);
  // Resource JSONs are not source: their whole text is literal by construction.
  const literals: Literal[] = file.endsWith('.ets')
    ? literalsOf(source)
    : [{ text: source, line: 1 }];
  for (const literal of literals) {
    literalCount++;
    if (literal.text.includes(caption)) { captionCount++; }
    const matches = literal.text.match(new RegExp(DOUBLED.source, 'g')) ?? [];
    for (const pair of matches) {
      const allowed = ALLOWED.find((entry) =>
        entry.phrase.includes(pair) && literal.text.includes(entry.phrase));
      hits.push({ file: shown, line: literal.line, pair, text: literal.text, allowed });
    }
  }
}

const explained = hits.filter((hit) => hit.allowed !== undefined);
const unexplained = hits.filter((hit) => hit.allowed === undefined);
for (const hit of explained) {
  console.log(`info ${hit.file}:${hit.line} [${hit.pair}] allowed — ` +
    `${hit.allowed?.phrase} (${hit.allowed?.why})`);
}

check('the sweep really read the product literals (47-ish sources, not an empty list)',
  files.length >= 40 && literalCount >= 500,
  `files=${files.length} literals=${literalCount}`);

check('no string literal carries an unexplained doubled CJK character',
  unexplained.length === 0,
  unexplained.map((hit) => `${hit.file}:${hit.line} [${hit.pair}] in “${hit.text}”`).join('; '));

check('every allow-list entry is still exercised by a real literal (no dead entry)',
  // The allow-list is the ONLY way a doubling passes, so it has to stay honest in
  // both directions: an entry nothing needs any more must be deleted, not kept
  // "just in case" — otherwise it silently blesses a future typo.
  new Set(explained.map((hit) => hit.allowed?.phrase)).size === ALLOWED.length &&
  ALLOWED.every((entry) => explained.some((hit) => hit.allowed?.phrase === entry.phrase)),
  `explained=${explained.map((hit) => hit.pair).join(',')} expected=` +
  `${ALLOWED.map((entry) => entry.phrase).join(',')}`);

check('the disconnected workspace header reads 连接后可在此搜索对端并查看详情。 once',
  // The owner's I7 §2 target string, pinned verbatim: a second 可 ("连接后可可…")
  // fails both this count and the sweep above. (The live 0.0.41 frame was re-measured
  // glyph by glyph while fixing this — it rendered ONE 可; see the increment record.)
  captionCount === 1,
  `captionCount=${captionCount}`);

console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
