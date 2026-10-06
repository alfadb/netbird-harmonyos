// Diagnostics log export self-check (host-side, no device, no network, no
// build).
//
// The production module client/entry/src/main/ets/vpnextensionability/
// NetBirdLogExport.ets is copied byte-for-byte into an OS-temp .ts fixture at
// run time — the code under test is the shipped code, never a hand-held mirror
// — and driven END-TO-END through the REAL exportDiagnostics(): the picker,
// the fs promises and the hilog seam are functional stubs injected by
// log-export-hooks.mjs (path-aware in-memory fs, three real picker answers, a
// recording hilog formatter).
//
// What this suite pins (the parts a device run cannot cheaply repeat):
//   A. the allow-list: exactly the eight fixed basenames, oldest -> newest,
//      no path/pattern/caller input anywhere;
//   B. the caps (2 MiB per file, 12 MiB total) and the document shape
//      (preamble / per-file header / end-footer);
//   C. honest reporting: absent vs unreadable, a file rotated away
//      mid-export, a read that failed, a candidate skipped by the total cap;
//   D. bounded copying: one source handle at a time (maxOpen === 2), every
//      handle closed, short reads/writes retried, no whole-file read;
//   E. the picker seam: never opened when there is nothing to export, a
//      dismissal and a blank URI are cancellations, a rejection is a failure
//      that keeps its code, the picked URI is used for the write;
//   F. log discipline: no filesDir path, no picked URI and no log content in
//      any recorded hilog line.
//
// Run: node client/entry/selfcheck/run-log-export.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { register } from 'node:module';
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import * as fsStub from './log-export-fs-stub.mjs';
import * as pickerStub from './log-export-picker-stub.mjs';

register(new URL('./log-export-hooks.mjs', import.meta.url).href);

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability',
  'NetBirdLogExport.ets');

const fixtureDir: string = mkdtempSync(join(tmpdir(), 'nb-log-export-'));
const fixture: string = join(fixtureDir, 'NetBirdLogExport.ts');
writeFileSync(fixture, readFileSync(REAL)); // byte-identical copy of the real module

// The shared filesDir both abilities use, and the URI the picker stub answers.
const FILES_DIR: string = '/data/storage/el2/base/haps/entry/files';
const DEST: string = pickerStub.state.uri;

const NAMES: string[] = [
  'netbird-ui.log.3', 'netbird-ui.log.2', 'netbird-ui.log.1', 'netbird-ui.log',
  'netbird-vpn.log.3', 'netbird-vpn.log.2', 'netbird-vpn.log.1', 'netbird-vpn.log'
];
const TWO_MIB: number = 2 * 1024 * 1024;

let passed: number = 0;
let failedCount: number = 0;

function check(name: string, condition: boolean, detail?: string): void {
  if (condition) {
    passed++;
    console.log(`ok   ${name}`);
  } else {
    failedCount++;
    console.log(`FAIL ${name}${detail !== undefined ? ` — ${detail}` : ''}`);
  }
}

function deepEq(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function recorded(): string[] {
  const g = globalThis as unknown as { __NB_LOG?: string[] };
  return g.__NB_LOG === undefined ? [] : g.__NB_LOG;
}

function resetLog(): void {
  const g = globalThis as unknown as { __NB_LOG: string[] };
  g.__NB_LOG = [];
}

// Fresh stub state + fresh hilog recorder + fresh picker for one case.
function arm(): void {
  fsStub.reset();
  pickerStub.state.mode = 'ok';
  pickerStub.state.rejectCode = 13900020;
  pickerStub.state.calls = 0;
  pickerStub.state.lastOptions = undefined;
  pickerStub.state.lastContextFilesDir = undefined;
  resetLog();
}

function pathOf(name: string): string {
  return `${FILES_DIR}/${name}`;
}

try {
  const mod = await import(pathToFileURL(fixture).href);
  const exportDiagnostics = mod.exportDiagnostics as
    (context: { filesDir: string }) => Promise<Record<string, unknown>>;
  const doc = (): string => fsStub.textOf(DEST) === undefined ? '' : (fsStub.textOf(DEST) as string);
  const reportOf = (result: Record<string, unknown>, name: string): Record<string, unknown> => {
    const files = result.files as Record<string, unknown>[];
    return files.filter((f) => f.name === name)[0];
  };

  // ---- A. the allow-list ---------------------------------------------------
  check('candidate table is exactly the eight allowed basenames, oldest first',
    deepEq(mod.logExportCandidateNames(), NAMES));
  check('no candidate is a path (no separator, no parent reference)',
    (mod.logExportCandidateNames() as string[]).every(
      (n: string) => !n.includes('/') && !n.includes('..')));
  check('no other log-ish or config name is reachable from the table',
    (mod.logExportCandidateNames() as string[]).indexOf('netbird-setup-key') === -1 &&
    (mod.logExportCandidateNames() as string[]).indexOf('netbird-status.json') === -1 &&
    (mod.logExportCandidateNames() as string[]).indexOf('netbird-device-config.json') === -1);
  check('the table is returned as a copy (a caller cannot extend the allow-list)',
    ((): boolean => {
      const table = mod.logExportCandidateNames() as string[];
      table.push('netbird-setup-key');
      return (mod.logExportCandidateNames() as string[]).length === 8;
    })());

  // ---- B. caps + document shape -------------------------------------------
  check('per-file cap is 2 MiB (the writers segment target)',
    mod.LOG_EXPORT_PER_FILE_CAP_BYTES === TWO_MIB);
  check('total cap is 12 MiB (10 MiB realistic worst case + headroom)',
    mod.LOG_EXPORT_TOTAL_CAP_BYTES === 12 * 1024 * 1024);
  check('copy chunk is 64 KiB (bounded memory, reused buffer)',
    mod.LOG_EXPORT_COPY_CHUNK_BYTES === 65536);
  check('ENOENT is the only code read as "this log does not exist"',
    mod.LOG_EXPORT_ENOENT_CODE === 13900002);
  check('stamp epoch', mod.exportStampUtc(0) === '19700101-000000');
  check('stamp 2026-10-05T18:22:31Z',
    mod.exportStampUtc(Date.UTC(2026, 9, 5, 18, 22, 31)) === '20261005-182231');

  const plan = [
    { name: 'netbird-ui.log.3', present: false, sizeBytes: 0, mtimeSec: 0 },
    { name: 'netbird-ui.log.2', present: true, sizeBytes: 12, mtimeSec: 1700000000 },
    { name: 'netbird-vpn.log.2', present: false, sizeBytes: 0, mtimeSec: 0, statCode: 13900012 }
  ];
  const preamble: string = mod.buildExportPreamble(plan, 0);
  check('preamble states the export is a best-effort snapshot',
    preamble.includes('BEST-EFFORT'));
  check('preamble says the app is not stopped and files may move under it',
    preamble.includes('keep running') && preamble.includes('not a frozen image'));
  check('preamble lists an absent candidate as absent',
    preamble.includes('  absent     netbird-ui.log.3'));
  check('preamble lists a present candidate with its plan size',
    preamble.includes('present    netbird-ui.log.2  12 bytes'));
  check('preamble reports an unreadable candidate with its code, not as absent',
    preamble.includes('unreadable netbird-vpn.log.2  code=13900012'));
  check('preamble names both caps',
    preamble.includes('per-file 2097152 bytes, total 12582912 bytes'));
  check('preamble states the ordering rule',
    preamble.includes('oldest -> newest inside each family'));
  const header: string = mod.buildFileHeader('netbird-vpn.log.1', 4096, 1700000000);
  check('header names the file, the plan size and the mtime',
    header.includes('netbird-vpn.log.1') && header.includes('snapshot 4096 bytes') &&
    header.includes('2023-11-14T22:13:20.000Z'));
  const footer: string = mod.buildFileFooter('netbird-vpn.log.1', TWO_MIB, true, undefined);
  check('footer reports the copied bytes and the cap truncation',
    footer.includes('copied 2097152 bytes') && footer.includes('TRUNCATED'));
  const footerErr: string = mod.buildFileFooter('netbird-ui.log', 5, false, 13900005);
  check('footer reports a read failure with its code and no false truncation',
    footerErr.includes('copied 5 bytes') && footerErr.includes('code=13900005') &&
    !footerErr.includes('TRUNCATED'));

  // ---- C1. nothing to export -> no picker at all --------------------------
  arm();
  const r1 = await exportDiagnostics({ filesDir: FILES_DIR });
  check('C1 no logs at all -> outcome no-logs', r1.outcome === 'no-logs');
  check('C1 the picker is never opened', pickerStub.state.calls === 0);
  check('C1 no destination file is created', fsStub.sizeOf(DEST) === undefined);
  check('C1 the plan reports all eight candidates as absent, zero bytes',
    (r1.files as Record<string, unknown>[]).length === 8 &&
    (r1.files as Record<string, unknown>[]).every((f) => f.present === false && f.bytes === 0));
  check('C1 no uri, zero bytes, no cap hit',
    r1.uri === undefined && r1.bytes === 0 && r1.totalCapHit === false);
  check('C1 no handle leaked', fsStub.assertNoLeak());

  // ---- C2. unreadable, not missing ---------------------------------------
  arm();
  for (const name of NAMES) {
    fsStub.statErrors.set(pathOf(name), 13900012);
  }
  const r2 = await exportDiagnostics({ filesDir: FILES_DIR });
  check('C2 every candidate unreadable -> failed with the code, no picker',
    r2.outcome === 'failed' && r2.code === 13900012 && pickerStub.state.calls === 0);
  check('C2 no handle leaked', fsStub.assertNoLeak());

  // ---- C3. happy path: order, headers, footer, hygiene -------------------
  arm();
  fsStub.seedFile(pathOf('netbird-ui.log.2'), 'UI-OLDEST\n', 1700000000);
  fsStub.seedFile(pathOf('netbird-ui.log'), 'UI-NEWEST\n', 1700000002);
  fsStub.seedFile(pathOf('netbird-vpn.log'), 'VPN-NEWEST\n', 1700000003);
  const r3 = await exportDiagnostics({ filesDir: FILES_DIR });
  const doc3: string = doc();
  check('C3 outcome exported with the picked uri', r3.outcome === 'exported' && r3.uri === DEST);
  check('C3 the older UI segment is written before the current one',
    doc3.indexOf('UI-OLDEST') > 0 && doc3.indexOf('UI-OLDEST') < doc3.indexOf('UI-NEWEST'));
  check('C3 the UI family is written before the native family',
    doc3.indexOf('UI-NEWEST') < doc3.indexOf('VPN-NEWEST'));
  check('C3 every seeded content appears verbatim',
    doc3.includes('UI-OLDEST\n') && doc3.includes('UI-NEWEST\n') &&
    doc3.includes('VPN-NEWEST\n'));
  check('C3 one header per copied file',
    doc3.includes('===== netbird-ui.log.2 |') && doc3.includes('===== netbird-ui.log |') &&
    doc3.includes('===== netbird-vpn.log |'));
  check('C3 one footer per copied file with the real byte counts',
    doc3.includes('===== end netbird-ui.log.2 | copied 10 bytes =====') &&
    doc3.includes('===== end netbird-ui.log | copied 10 bytes =====') &&
    doc3.includes('===== end netbird-vpn.log | copied 11 bytes ====='));
  check('C3 the five absent siblings are still listed in the preamble',
    doc3.includes('  absent     netbird-ui.log.3') &&
    doc3.includes('  absent     netbird-ui.log.1') &&
    doc3.includes('  absent     netbird-vpn.log.3'));
  check('C3 absent candidates are reported present:false',
    (r3.files as Record<string, unknown>[]).filter((f) => f.present === false).length === 5);
  check('C3 bytes equals the sum of the per-file bytes',
    r3.bytes === (r3.files as Record<string, unknown>[])
      .reduce((acc: number, f: Record<string, unknown>) => acc + (f.bytes as number), 0) &&
    r3.bytes === 31);
  check('C3 nothing was truncated and the total cap was not hit',
    (r3.files as Record<string, unknown>[]).every((f) => f.truncated === false) &&
    r3.totalCapHit === false);
  check('C3 the picker was asked for a timestamped .txt document',
    /^netbird-logs-\d{8}-\d{6}\.txt$/.test(pickerStub.state.lastOptions.newFileNames[0]));
  check('C3 the picker received the caller UIAbilityContext',
    pickerStub.state.lastContextFilesDir === FILES_DIR);
  check('C3 only two handles are ever open at once (dest + one source)',
    fsStub.maxOpen === 2);
  check('C3 the destination is opened first',
    fsStub.openTrace[0] === DEST);
  check('C3 sources are opened lazily, in candidate order',
    deepEq(fsStub.openTrace.slice(1),
      [pathOf('netbird-ui.log.2'), pathOf('netbird-ui.log'), pathOf('netbird-vpn.log')]));
  check('C3 every handle is closed', fsStub.assertNoLeak());

  // ---- C4. a rotation during the export ----------------------------------
  arm();
  fsStub.seedFile(pathOf('netbird-ui.log.3'), 'ROTATED-OLD\n', 1700000000);
  fsStub.seedFile(pathOf('netbird-ui.log'), 'UI-GOES-AWAY\n', 1700000001);
  fsStub.seedFile(pathOf('netbird-vpn.log'), 'VPN-PLAN-4\n', 1700000002);
  const grown: Buffer = Buffer.from('VPN-GROWN-TO-TWENTY-SIX\n');
  let mutated: boolean = false;
  fsStub.state.onOpen = (path: string): void => {
    if (!mutated && path === pathOf('netbird-ui.log.3')) {
      mutated = true;
      // The writers rotate while the copy runs: the UI base log (still ahead of
      // us in the plan) is renamed away and the native base log grows.
      fsStub.files.delete(pathOf('netbird-ui.log'));
      fsStub.files.set(pathOf('netbird-vpn.log'), grown);
    }
  };
  const r4 = await exportDiagnostics({ filesDir: FILES_DIR });
  const doc4: string = doc();
  check('C4 the rotation really happened between plan and copy', mutated === true);
  check('C4 a source that vanished mid-export is reported with its code',
    reportOf(r4, 'netbird-ui.log').present === true &&
    reportOf(r4, 'netbird-ui.log').bytes === 0 &&
    reportOf(r4, 'netbird-ui.log').errorCode === 13900002);
  check('C4 no section is written for a file that could not be opened',
    !doc4.includes('===== netbird-ui.log |'));
  check('C4 a log that grew is copied past its plan size',
    doc4.includes('VPN-GROWN-TO-TWENTY-SIX') &&
    doc4.includes(`===== end netbird-vpn.log | copied ${grown.length} bytes =====`));
  check('C4 the header keeps the plan snapshot while the footer carries the real size',
    doc4.includes('===== netbird-vpn.log | snapshot 11 bytes |'));
  check('C4 the export still finishes and leaks nothing',
    r4.outcome === 'exported' && fsStub.assertNoLeak());
  check('C4 the done marker separates planned presence from really copied files',
    recorded().some((l) => l.includes('LOG_EXPORT_DONE|') &&
      l.includes('present=3|copiedFiles=2|')));

  // ---- C5. per-file cap --------------------------------------------------
  arm();
  const over: Buffer = Buffer.alloc(TWO_MIB + 4096, 0x78);
  fsStub.seedBytes(pathOf('netbird-vpn.log'), over, 1700000000);
  fsStub.seedFile(pathOf('netbird-ui.log'), 'short\n', 1700000001);
  const r5 = await exportDiagnostics({ filesDir: FILES_DIR });
  const doc5: string = doc();
  check('C5 a file over the per-file cap is cut at exactly 2 MiB and flagged',
    reportOf(r5, 'netbird-vpn.log').bytes === TWO_MIB &&
    reportOf(r5, 'netbird-vpn.log').truncated === true);
  check('C5 the footer says the export was truncated',
    doc5.includes(`===== end netbird-vpn.log | copied ${TWO_MIB} bytes | TRUNCATED`));
  check('C5 the shorter UI log is still copied whole',
    doc5.includes('short\n') &&
    reportOf(r5, 'netbird-ui.log').bytes === 6 &&
    reportOf(r5, 'netbird-ui.log').truncated === false);
  check('C5 no handle leaked', fsStub.assertNoLeak());

  // ---- C6. exact fit is not reported as truncation -----------------------
  arm();
  fsStub.seedBytes(pathOf('netbird-ui.log'), Buffer.alloc(TWO_MIB, 0x79), 1700000000);
  const r6 = await exportDiagnostics({ filesDir: FILES_DIR });
  check('C6 a file of exactly the cap is not wrongly reported as truncated',
    reportOf(r6, 'netbird-ui.log').bytes === TWO_MIB &&
    reportOf(r6, 'netbird-ui.log').truncated === false);
  check('C6 its footer carries no TRUNCATED marker',
    doc().includes(`===== end netbird-ui.log | copied ${TWO_MIB} bytes =====`));
  check('C6 no handle leaked', fsStub.assertNoLeak());

  // ---- C7. total cap -----------------------------------------------------
  arm();
  for (const name of NAMES.slice(0, 6)) {
    fsStub.seedBytes(pathOf(name), Buffer.alloc(TWO_MIB, 0x61), 1700000000);
  }
  fsStub.seedFile(pathOf('netbird-vpn.log.1'), 'NEWER-BUT-SKIPPED\n', 1700000001);
  fsStub.seedFile(pathOf('netbird-vpn.log'), 'NEWEST-BUT-SKIPPED\n', 1700000002);
  const r7 = await exportDiagnostics({ filesDir: FILES_DIR });
  const doc7: string = doc();
  const skipped = (r7.files as Record<string, unknown>[]).filter((f) => f.skipped === true);
  check('C7 the total cap skips the newest candidates rather than overrunning',
    skipped.length === 2 && skipped.every((f) => f.bytes === 0 && f.truncated === true));
  check('C7 the cap hit is reported on the result', r7.totalCapHit === true);
  check('C7 exactly the 12 MiB budget is copied', r7.bytes === 12 * 1024 * 1024);
  check('C7 a skipped candidate leaves no section behind',
    !doc7.includes('NEWER-BUT-SKIPPED') && !doc7.includes('NEWEST-BUT-SKIPPED'));
  check('C7 the skipped candidate is still listed as present in the preamble',
    doc7.includes('present    netbird-vpn.log  '));
  check('C7 the six full files were not flagged as truncated',
    (r7.files as Record<string, unknown>[]).slice(0, 6)
      .every((f) => f.truncated === false && f.bytes === TWO_MIB));
  check('C7 no handle leaked', fsStub.assertNoLeak());

  // ---- C8. a source read failure ----------------------------------------
  arm();
  fsStub.seedFile(pathOf('netbird-ui.log'), 'FIRST-HALF-', 1700000000);
  fsStub.seedFile(pathOf('netbird-vpn.log'), 'OTHER-FILE\n', 1700000001);
  fsStub.readErrors.set(pathOf('netbird-ui.log'), 13900005);
  const r8 = await exportDiagnostics({ filesDir: FILES_DIR });
  const doc8: string = doc();
  check('C8 a failed read is reported with its code, not as an empty log',
    reportOf(r8, 'netbird-ui.log').errorCode === 13900005 &&
    reportOf(r8, 'netbird-ui.log').bytes === 0);
  check('C8 the document says the read failed',
    doc8.includes('===== end netbird-ui.log | copied 0 bytes | read failed, code=13900005 ====='));
  check('C8 the export continues with the next candidate',
    r8.outcome === 'exported' && doc8.includes('OTHER-FILE'));
  check('C8 no handle leaked', fsStub.assertNoLeak());

  // ---- C9. destination write failure ------------------------------------
  arm();
  fsStub.seedFile(pathOf('netbird-ui.log'), 'CONTENT-A\n', 1700000000);
  fsStub.writeErrors.set(DEST, 13900025);
  const r9 = await exportDiagnostics({ filesDir: FILES_DIR });
  check('C9 a destination write failure fails the export with its code',
    r9.outcome === 'failed' && r9.code === 13900025);
  check('C9 the partial document uri is still returned', r9.uri === DEST);
  check('C9 the report keeps exactly what was written (nothing yet)',
    reportOf(r9, 'netbird-ui.log').bytes === 0 && r9.bytes === 0);
  check('C9 no handle leaked', fsStub.assertNoLeak());

  // ---- C10. destination open failure ------------------------------------
  arm();
  fsStub.seedFile(pathOf('netbird-ui.log'), 'X\n', 1700000000);
  fsStub.openErrors.set(DEST, 13900012);
  const r10 = await exportDiagnostics({ filesDir: FILES_DIR });
  check('C10 a destination that cannot be opened fails with its code',
    r10.outcome === 'failed' && r10.code === 13900012 && r10.uri === DEST);
  check('C10 the destination refusal stops the export before any source is opened',
    fsStub.openTrace.length === 0);
  check('C10 no handle leaked', fsStub.assertNoLeak());

  // ---- C11-C13. the picker answers --------------------------------------
  arm();
  fsStub.seedFile(pathOf('netbird-ui.log'), 'X\n', 1700000000);
  pickerStub.state.mode = 'empty';
  const r11 = await exportDiagnostics({ filesDir: FILES_DIR });
  check('C11 an empty picker answer is a cancellation',
    r11.outcome === 'cancelled' && r11.bytes === 0 && r11.uri === undefined);
  check('C11 a cancellation opens nothing and writes nothing',
    fsStub.openTrace.length === 0 && fsStub.sizeOf(DEST) === undefined);

  arm();
  fsStub.seedFile(pathOf('netbird-ui.log'), 'X\n', 1700000000);
  pickerStub.state.mode = 'blank';
  const r12 = await exportDiagnostics({ filesDir: FILES_DIR });
  check('C12 a blank uri is a cancellation too',
    r12.outcome === 'cancelled' && fsStub.openTrace.length === 0);

  arm();
  fsStub.seedFile(pathOf('netbird-ui.log'), 'X\n', 1700000000);
  pickerStub.state.mode = 'reject';
  const r13 = await exportDiagnostics({ filesDir: FILES_DIR });
  check('C13 a picker rejection is a failure that keeps its code (never a silent cancel)',
    r13.outcome === 'failed' && r13.code === 13900020 && r13.uri === undefined);
  check('C13 no handle leaked', fsStub.assertNoLeak());

  // ---- C14. short-write retry loop --------------------------------------
  arm();
  const content: string = 'PARTIAL-WRITE-CONTENT\n';
  fsStub.seedFile(pathOf('netbird-ui.log'), content, 1700000000);
  fsStub.state.writeMaxBytes = 7;
  const r14 = await exportDiagnostics({ filesDir: FILES_DIR });
  const doc14: string = doc();
  check('C14 writes capped at 7 bytes still land completely',
    doc14.includes(content) &&
    doc14.includes(`===== end netbird-ui.log | copied ${content.length} bytes =====`) &&
    r14.bytes === content.length);
  arm();
  fsStub.seedFile(pathOf('netbird-ui.log'), content, 1700000000);
  const r14b = await exportDiagnostics({ filesDir: FILES_DIR });
  const section14 = (text: string): string => text.slice(text.indexOf('====='));
  check('C14 the retried log sections are byte-identical to the unshortened ones',
    r14b.outcome === 'exported' &&
    doc14.indexOf('=====') > 0 && section14(doc()) === section14(doc14));

  // ---- C15. log discipline ---------------------------------------------
  arm();
  const secret: string = 'SECRET-CONTENT\n';
  fsStub.seedFile(pathOf('netbird-ui.log'), secret, 1700000000);
  await exportDiagnostics({ filesDir: FILES_DIR });
  const lines: string[] = recorded();
  check('C15 the done marker is recorded with the copied byte count',
    lines.some((l) =>
      l.includes(`LOG_EXPORT_DONE|copied=${secret.length}|present=1|copiedFiles=1|capHit=false`)));
  check('C15 the plan marker is recorded with counts only',
    lines.some((l) => l.includes('LOG_EXPORT_PLAN|present=1|unreadable=0')));
  check('C15 no recorded line contains the filesDir path',
    lines.every((l) => !l.includes(FILES_DIR) && !l.includes('/data/')));
  check('C15 no recorded line contains the picked uri',
    lines.every((l) => !l.includes(DEST) && !l.includes('file://')));
  check('C15 no recorded line contains log content',
    lines.every((l) => !l.includes('SECRET-CONTENT')));
  check('C15 there is something recorded at all', lines.length >= 2);
} finally {
  rmSync(fixtureDir, { recursive: true, force: true });
}

console.log(`${passed} passed, ${failedCount} failed`);
if (failedCount > 0) { process.exitCode = 1; }
