// Diagnostics log export (2026-10-05) — ONE user-facing action for a user who
// needs their mobile logs off the device: copy this app's own rotating logs
// into a single .txt document at a location the USER picks in the system file
// picker. Nothing else in the app is reachable from this module.
//
// WHY THE PICKER: DocumentViewPicker.save() returns a file:// URI that already
// carries a TEMPORARY read/write grant for this app (SDK 26
// @ohos.file.picker.d.ts:721 save, :649 constructor(context); CoreFileKit guide
// "save-user-file": the grant is invalidated when the application exits).
// Writing through that URI needs NO storage permission in module.json5 — a
// diagnostic affordance must not buy always-on broad file access.
//
// INPUTS — the whole allow-list is the fixed table below: the eight basenames
// this app's own writers use under filesDir
//   netbird-ui.log[.1 .2 .3]    UI-side log, four segments, 2 MiB family total
//                               (512 KiB per segment; the UI writer's own
//                               TOTAL_MAX_BYTES is 2 MiB)
//   netbird-vpn.log[.1 .2 .3]   VPN-extension/native log, 2 MiB per segment x
//                               4 segments (native phone_log defaults)
// The per-file cap below (2 MiB) is therefore a safe superset of both writers'
// segment sizes: it never cuts a file the writers keep within their own cap,
// and it still bounds the copy if one of them ever exceeds it.
// Rotation convention (both writers): the base name is the CURRENT file; once
// it exceeds the writer's cap it becomes .1, .1 becomes .2, the old .3 is
// unlinked — so .3 is the oldest sibling of its family, the base name newest.
// There is deliberately NO directory listing and NO user-supplied path here:
// config, setup-key, status and every other file under filesDir cannot be
// exported through this module, and nothing outside filesDir is ever named.
//
// ORDER: oldest -> newest inside each family (family order: UI, then native).
// The two families are separate timelines written by separate processes, so
// only the within-family order is total; the preamble says so.
//
// BEST-EFFORT SNAPSHOT: the writers keep running while this copy runs, so a
// rotated sibling can be renamed away and the base name keeps growing
// mid-export. Each candidate is therefore opened at ITS turn (never all
// handles held at once), read sequentially in bounded chunks and stopped at
// the caps below; no log is renamed, truncated or deleted, and no app/VPN stop
// is required. The exported document starts with a preamble stating exactly
// that and listing which candidates were present / absent / unreadable at plan
// time, and every file gets a header plus an end-footer carrying the bytes
// actually copied.
//
// MEMORY: one fixed 64 KiB chunk buffer is allocated for the whole export and
// reused; no file's contents ever exist as a whole in memory (no readText).
// Source handles are closed in finally. A partial read/write is either retried
// to completion or reported — never silently dropped.
//
// LOG DISCIPLINE: hilog lines carry markers, counts and numeric codes only.
// The filesDir path and the picked URI are never logged (they are user data);
// the result object carries the URI back to the caller, which must not log it
// verbatim either.
import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
import picker from '@ohos.file.picker';
import common from '@ohos.app.ability.common';

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';

// The platform BusinessError shape as far as this module reads it (one numeric
// field) — the same local-shape trick NetBirdConnector uses, so no @ohos.base
// import is needed.
interface PlatformBusinessError extends Error {
  code?: number;
}

// stat()'s "no such file or directory": the ONLY code read as "this log does
// not exist". Any other stat failure means unreadable, not missing — the two
// are reported differently.
export const LOG_EXPORT_ENOENT_CODE: number = 13900002;

// Caps, in log-content bytes (the fixed per-file headers/preamble/footers are
// ASCII overhead of a few hundred bytes and are not counted).
//   per file: the writers' own segment target, 2 MiB.
//   total:    the realistic worst case is native 4 x 2 MiB + UI 2 MiB =
//             10 MiB; 12 MiB leaves headroom so a well-behaved pair of writers
//             is exported whole. The theoretical maximum is 8 x 2 MiB = 16 MiB
//             (per-file cap x 8 candidates), so the total cap CAN bind when a
//             file set exceeds the writers' configured sizes — that is why a
//             skipped candidate is reported instead of silently dropped.
export const LOG_EXPORT_PER_FILE_CAP_BYTES: number = 2 * 1024 * 1024;
export const LOG_EXPORT_TOTAL_CAP_BYTES: number = 12 * 1024 * 1024;
export const LOG_EXPORT_COPY_CHUNK_BYTES: number = 64 * 1024;
export const LOG_EXPORT_FILE_PREFIX: string = 'netbird-logs-';

// Outcomes of exportDiagnostics(). Kept as plain documented literals (the
// house shape for the pure modules' verdicts): 'exported' — the picked
// document holds at least one candidate (possibly cut at a cap, see the
// per-file reports); 'cancelled' — the user dismissed the picker; 'no-logs' —
// none of the eight candidates exists, so the picker was never shown;
// 'failed' — the picker or the file system refused (numeric `code`), or every
// candidate failed to stat.
export const LOG_EXPORT_OUTCOME_EXPORTED: string = 'exported';
export const LOG_EXPORT_OUTCOME_CANCELLED: string = 'cancelled';
export const LOG_EXPORT_OUTCOME_NO_LOGS: string = 'no-logs';
export const LOG_EXPORT_OUTCOME_FAILED: string = 'failed';

// The WHOLE allow-list, oldest -> newest inside each family, UI family first.
// Fixed literals only: no listing, no pattern, no caller input.
const LOG_EXPORT_CANDIDATES: string[] = [
  'netbird-ui.log.3', 'netbird-ui.log.2', 'netbird-ui.log.1', 'netbird-ui.log',
  'netbird-vpn.log.3', 'netbird-vpn.log.2', 'netbird-vpn.log.1', 'netbird-vpn.log'
];

// A candidate as found at plan time (stat only — nothing is read yet).
export interface LogExportCandidate {
  name: string;
  present: boolean;
  sizeBytes: number;
  mtimeSec: number;
  // stat failed with a code other than ENOENT: the file may well exist but
  // could not be measured. Kept so the export can say "unreadable", not
  // "missing".
  statCode?: number;
}

// Per-file outcome inside the exported document.
export interface LogExportFileReport {
  name: string;
  present: boolean;
  bytes: number;      // log-content bytes copied from this file
  truncated: boolean; // its content was cut at a per-file/total cap
  skipped: boolean;   // not copied: the total cap was already reached
  errorCode?: number; // source open/read failure (numeric platform code)
}

export interface LogExportResult {
  outcome: string;
  // The picker's URI, returned for the caller to show/act on. Present only
  // when the picker answered with a URI (a 'failed' export may have left a
  // partial document there). NEVER hilog-ed.
  uri?: string;
  files: LogExportFileReport[];
  // Total log-content bytes copied = the sum of files[].bytes.
  bytes: number;
  // The total cap stopped a present candidate from being copied, or was
  // reached exactly at the last byte copied.
  totalCapHit: boolean;
  code?: number; // numeric platform code when outcome === 'failed'
}

// ---------------------------------------------------------------------------
// Pure helpers (no I/O): the candidate table, the document shape and the
// timestamp strings. Everything here is driven by the host selfcheck
// (client/entry/selfcheck/run-log-export.mts), so it must stay plain logic.
// ---------------------------------------------------------------------------

// A copy of the allow-list, in export order. A copy so no caller can extend
// the allow-list by mutating the table.
export function logExportCandidateNames(): string[] {
  return LOG_EXPORT_CANDIDATES.slice(0);
}

function pad2(value: number): string {
  return value < 10 ? `0${value}` : `${value}`;
}

// UTC stamp for the default document name: YYYYMMDD-HHMMSS. Built from Date
// getters (no locale, no regex), so it is identical on every device.
export function exportStampUtc(nowMs: number): string {
  const d: Date = new Date(nowMs);
  return `${d.getUTCFullYear()}${pad2(d.getUTCMonth() + 1)}${pad2(d.getUTCDate())}-` +
    `${pad2(d.getUTCHours())}${pad2(d.getUTCMinutes())}${pad2(d.getUTCSeconds())}`;
}

export function isoTimestampUtc(ms: number): string {
  return new Date(ms).toISOString();
}

// The document preamble. It is the honest part of the snapshot: what was
// asked for, what was there at plan time, what was not, and what the caps are.
// ASCII only, so the byte counting in writeAsciiText holds.
export function buildExportPreamble(candidates: LogExportCandidate[], nowMs: number): string {
  let out: string = 'NetBird HarmonyOS diagnostics log export\n';
  out += `exported-at: ${isoTimestampUtc(nowMs)}\n`;
  out += 'snapshot: BEST-EFFORT. The app and the VPN extension keep running and\n';
  out += '  keep writing these logs while this copy runs: each source file is opened\n';
  out += '  at its turn and read up to the caps below, so a file here is a bounded\n';
  out += '  portion of that file at that moment, not a frozen image of it.\n';
  out += `caps: per-file ${LOG_EXPORT_PER_FILE_CAP_BYTES} bytes, total ` +
    `${LOG_EXPORT_TOTAL_CAP_BYTES} bytes (log content only)\n`;
  out += 'order: oldest -> newest inside each family (UI family first); the UI log\n';
  out += '  and the native/VPN log are separate timelines written by separate\n';
  out += '  processes, so only the order inside one family is total.\n';
  out += 'sources:\n';
  for (let i = 0; i < candidates.length; i++) {
    const c: LogExportCandidate = candidates[i];
    if (c.present) {
      out += `  present    ${c.name}  ${c.sizeBytes} bytes  mtime ` +
        `${isoTimestampUtc(c.mtimeSec * 1000)}\n`;
    } else if (c.statCode !== undefined) {
      out += `  unreadable ${c.name}  code=${c.statCode}\n`;
    } else {
      out += `  absent     ${c.name}\n`;
    }
  }
  return out;
}

// One header per file, written before its content. Sizes/mtime are the PLAN
// values (the file may have grown since); the end-footer carries what was
// actually copied.
export function buildFileHeader(name: string, sizeBytes: number, mtimeSec: number): string {
  return `\n===== ${name} | snapshot ${sizeBytes} bytes | mtime ` +
    `${isoTimestampUtc(mtimeSec * 1000)} =====\n`;
}

// The end-footer: the only place inside the document that can tell the truth
// about the bytes that really came across.
export function buildFileFooter(name: string, contentBytes: number, truncated: boolean,
  errorCode: number | undefined): string {
  let line: string = `===== end ${name} | copied ${contentBytes} bytes`;
  if (truncated) {
    line += ' | TRUNCATED at the export cap (this log holds more)';
  }
  if (errorCode !== undefined) {
    line += ` | read failed, code=${errorCode}`;
  }
  return `${line} =====\n`;
}

// Read the numeric platform code out of whatever was thrown. Non-number /
// absent codes answer undefined (reported as "unknown", never invented).
function numericCodeOf(raw: Error): number | undefined {
  const code: number | undefined = (raw as PlatformBusinessError).code;
  if (code !== undefined && code !== null && Number.isFinite(code)) {
    return code;
  }
  return undefined;
}

// ---------------------------------------------------------------------------
// I/O helpers
//
// Exact APIs used (installed SDK 26, @ohos.file.fs.d.ts, promise forms only):
//   stat  :3184   open :2491   read :2664   write :3557   close :381
//   OpenMode.READ_ONLY :136   WRITE_ONLY :145   TRUNC :173
// read()/write() default to the CURRENT file position (no offset is ever
// passed), which is what makes the sequential copy and the short-write retry
// below position-safe.
// ---------------------------------------------------------------------------

// Write an ASCII string completely, retrying short writes. The export document
// text is ASCII by construction (buildExportPreamble / buildFileHeader /
// buildFileFooter use ASCII literals only), so the returned byte count and the
// string index advance together; log CONTENT never goes through this path.
async function writeAsciiText(fd: number, text: string): Promise<number> {
  let done: number = 0;
  while (done < text.length) {
    const wrote: number = await fs.write(fd, text.slice(done));
    if (wrote <= 0) {
      throw new Error('log-export-short-write');
    }
    done += wrote;
  }
  return done;
}

// Write exactly `count` bytes of `buffer` starting at `from`, retrying short
// writes. The remainder is re-sliced from the original buffer (<= one chunk),
// so memory stays bounded and the write position always continues where the
// previous write stopped (the SDK's write() defaults to the current position).
async function writeChunk(fd: number, buffer: ArrayBuffer, from: number,
  count: number): Promise<number> {
  let done: number = 0;
  while (done < count) {
    const part: ArrayBuffer = buffer.slice(from + done, from + count);
    const wrote: number = await fs.write(fd, part);
    if (wrote <= 0) {
      throw new Error('log-export-short-write');
    }
    done += wrote;
  }
  return done;
}

// stat one candidate. A missing file is honestly absent (ENOENT); any other
// failure is carried as `statCode` (unreadable), never folded into "absent".
async function statCandidate(filesDir: string, name: string): Promise<LogExportCandidate> {
  const path: string = `${filesDir}/${name}`;
  try {
    const stat: fs.Stat = await fs.stat(path);
    const candidate: LogExportCandidate = {
      name: name, present: true, sizeBytes: stat.size, mtimeSec: stat.mtime
    };
    return candidate;
  } catch (raw) {
    const code: number | undefined = numericCodeOf(raw as Error);
    if (code === LOG_EXPORT_ENOENT_CODE) {
      const absent: LogExportCandidate = {
        name: name, present: false, sizeBytes: 0, mtimeSec: 0
      };
      return absent;
    }
    const unreadable: LogExportCandidate = {
      name: name, present: false, sizeBytes: 0, mtimeSec: 0, statCode: code
    };
    return unreadable;
  }
}

// Copy ONE candidate into the open destination. The report object is filled in
// place as the bytes move, so an exception thrown later still leaves an honest
// partial report behind.
//
// Error split: a SOURCE failure (open/read) is that file's problem and only
// lands in the report — the rest of the export continues. A DESTINATION
// failure (header/footer/content write) propagates: the export cannot continue
// after it, and the caller turns it into outcome 'failed'.
async function copyCandidate(report: LogExportFileReport, dstFd: number, path: string,
  sizeBytes: number, mtimeSec: number, limit: number, chunk: ArrayBuffer): Promise<void> {
  let src: fs.File;
  try {
    src = await fs.open(path, fs.OpenMode.READ_ONLY);
  } catch (raw) {
    report.errorCode = numericCodeOf(raw as Error);
    hilog.warn(DOMAIN, TAG, 'LOG_EXPORT_SOURCE_OPEN_FAILED|file=%{public}s|code=%{public}d',
      report.name, report.errorCode === undefined ? -1 : report.errorCode);
    return;
  }
  try {
    // Header first (plan values), then the content, then the footer with what
    // really came across.
    await writeAsciiText(dstFd, buildFileHeader(report.name, sizeBytes, mtimeSec));
    while (report.bytes < limit) {
      let read: number = 0;
      try {
        read = await fs.read(src.fd, chunk);
      } catch (raw) {
        report.errorCode = numericCodeOf(raw as Error);
        hilog.warn(DOMAIN, TAG, 'LOG_EXPORT_SOURCE_READ_FAILED|file=%{public}s|code=%{public}d',
          report.name, report.errorCode === undefined ? -1 : report.errorCode);
        break;
      }
      if (read <= 0) {
        break; // EOF, or a rotating writer truncated the file under us
      }
      const take: number = Math.min(read, limit - report.bytes);
      await writeChunk(dstFd, chunk, 0, take);
      report.bytes += take;
      if (take < read) {
        report.truncated = true; // the cap cut inside this chunk
        break;
      }
    }
    if (!report.truncated && report.errorCode === undefined && report.bytes >= limit) {
      // Exact fit: ask the source for one more byte. That is what makes
      // `truncated` honest ("the log holds more") instead of "the file just
      // happened to end here". One extra byte is read; nothing is exported.
      const peek: ArrayBuffer = new ArrayBuffer(1);
      try {
        const more: number = await fs.read(src.fd, peek);
        report.truncated = more > 0;
      } catch (raw) {
        report.errorCode = numericCodeOf(raw as Error);
      }
    }
    if (report.truncated) {
      hilog.warn(DOMAIN, TAG, 'LOG_EXPORT_SOURCE_TRUNCATED|file=%{public}s|copied=%{public}d',
        report.name, report.bytes);
    }
    await writeAsciiText(dstFd,
      buildFileFooter(report.name, report.bytes, report.truncated, report.errorCode));
  } finally {
    try {
      await fs.close(src);
    } catch (raw) {
      const code: number | undefined = numericCodeOf(raw as Error);
      hilog.warn(DOMAIN, TAG, 'LOG_EXPORT_SOURCE_CLOSE_FAILED|file=%{public}s|code=%{public}d',
        report.name, code === undefined ? -1 : code);
    }
  }
}

function sumReportBytes(reports: LogExportFileReport[]): number {
  let total: number = 0;
  for (let i = 0; i < reports.length; i++) {
    total += reports[i].bytes;
  }
  return total;
}

function planReports(candidates: LogExportCandidate[]): LogExportFileReport[] {
  const reports: LogExportFileReport[] = [];
  for (let i = 0; i < candidates.length; i++) {
    const c: LogExportCandidate = candidates[i];
    const report: LogExportFileReport = {
      name: c.name, present: c.present, bytes: 0, truncated: false, skipped: false
    };
    reports.push(report);
  }
  return reports;
}

// ---------------------------------------------------------------------------
// The one public action
// ---------------------------------------------------------------------------

// Export the app's own logs to a .txt document the user picks. Must be called
// from a UIAbility (the picker page needs a UIAbilityContext; SDK 26
// @ohos.file.picker.d.ts:24 "must be called in UIAbility"). Never throws:
// every failure is folded into the returned result.
export async function exportDiagnostics(
  context: common.UIAbilityContext): Promise<LogExportResult> {
  const filesDir: string = context.filesDir;
  const nowMs: number = Date.now();
  const names: string[] = logExportCandidateNames();

  // 1. Plan: stat the eight fixed names (no listing, no reading).
  const candidates: LogExportCandidate[] = [];
  let presentCount: number = 0;
  let unreadableCount: number = 0;
  let firstUnreadableCode: number | undefined = undefined;
  for (let i = 0; i < names.length; i++) {
    const candidate: LogExportCandidate = await statCandidate(filesDir, names[i]);
    candidates.push(candidate);
    if (candidate.present) {
      presentCount++;
    } else if (candidate.statCode !== undefined) {
      unreadableCount++;
      if (firstUnreadableCode === undefined) {
        firstUnreadableCode = candidate.statCode;
      }
    }
  }
  const reports: LogExportFileReport[] = planReports(candidates);

  if (presentCount === 0) {
    const outcome: string = unreadableCount > 0 ?
      LOG_EXPORT_OUTCOME_FAILED : LOG_EXPORT_OUTCOME_NO_LOGS;
    const empty: LogExportResult = {
      outcome: outcome, files: reports, bytes: 0, totalCapHit: false,
      code: firstUnreadableCode
    };
    if (unreadableCount > 0) {
      hilog.error(DOMAIN, TAG, 'LOG_EXPORT_PLAN_FAILED|present=0|unreadable=%{public}d',
        unreadableCount);
    } else {
      hilog.info(DOMAIN, TAG, 'LOG_EXPORT_PLAN_EMPTY|present=0|unreadable=0');
    }
    return empty;
  }
  hilog.info(DOMAIN, TAG, 'LOG_EXPORT_PLAN|present=%{public}d|unreadable=%{public}d',
    presentCount, unreadableCount);

  // 2. Let the user choose the destination. The returned URI carries its own
  //    temporary read/write grant (no permission is declared for this).
  let uris: string[] = [];
  try {
    const options: picker.DocumentSaveOptions = new picker.DocumentSaveOptions();
    options.newFileNames = [`${LOG_EXPORT_FILE_PREFIX}${exportStampUtc(nowMs)}.txt`];
    const documentPicker: picker.DocumentViewPicker = new picker.DocumentViewPicker(context);
    uris = await documentPicker.save(options);
  } catch (raw) {
    const code: number | undefined = numericCodeOf(raw as Error);
    hilog.error(DOMAIN, TAG, 'LOG_EXPORT_PICKER_FAILED|code=%{public}d',
      code === undefined ? -1 : code);
    const failed: LogExportResult = {
      outcome: LOG_EXPORT_OUTCOME_FAILED, files: reports, bytes: 0, totalCapHit: false, code: code
    };
    return failed;
  }
  // The picker reports a user dismissal as an empty answer, not as an error
  // (CoreFileKit picker guide: check the returned array before using it).
  if (uris.length === 0 || uris[0].length === 0) {
    hilog.info(DOMAIN, TAG, 'LOG_EXPORT_CANCELLED');
    const cancelled: LogExportResult = {
      outcome: LOG_EXPORT_OUTCOME_CANCELLED, files: reports, bytes: 0, totalCapHit: false
    };
    return cancelled;
  }
  const uri: string = uris[0];

  // 3. Write the document: preamble, then one section per candidate in order.
  const chunk: ArrayBuffer = new ArrayBuffer(LOG_EXPORT_COPY_CHUNK_BYTES);
  let budget: number = LOG_EXPORT_TOTAL_CAP_BYTES;
  let dst: fs.File | undefined = undefined;
  try {
    // WRITE_ONLY + TRUNC: the picker has already pre-created/confirmed the
    // file (default autoCreateEmptyFile), so truncating leaves no stale bytes.
    dst = await fs.open(uri, fs.OpenMode.WRITE_ONLY | fs.OpenMode.TRUNC);
    const dstFd: number = dst.fd;
    await writeAsciiText(dstFd, buildExportPreamble(candidates, nowMs));
    for (let i = 0; i < reports.length; i++) {
      const report: LogExportFileReport = reports[i];
      if (!report.present) {
        continue;
      }
      if (budget <= 0) {
        report.skipped = true;
        report.truncated = true;
        hilog.warn(DOMAIN, TAG, 'LOG_EXPORT_SOURCE_SKIPPED|file=%{public}s|reason=total-cap',
          report.name);
        continue;
      }
      const limit: number = Math.min(LOG_EXPORT_PER_FILE_CAP_BYTES, budget);
      await copyCandidate(report, dstFd, `${filesDir}/${report.name}`,
        candidates[i].sizeBytes, candidates[i].mtimeSec, limit, chunk);
      budget -= report.bytes;
    }
  } catch (raw) {
    const code: number | undefined = numericCodeOf(raw as Error);
    hilog.error(DOMAIN, TAG, 'LOG_EXPORT_WRITE_FAILED|code=%{public}d',
      code === undefined ? -1 : code);
    const failed: LogExportResult = {
      outcome: LOG_EXPORT_OUTCOME_FAILED, uri: uri, files: reports,
      bytes: sumReportBytes(reports), totalCapHit: budget <= 0, code: code
    };
    return failed;
  } finally {
    if (dst !== undefined) {
      try {
        await fs.close(dst);
      } catch (raw) {
        // Best-effort: the bytes are already written; report the code but do
        // not turn a complete export into a failure.
        const code: number | undefined = numericCodeOf(raw as Error);
        hilog.warn(DOMAIN, TAG, 'LOG_EXPORT_DEST_CLOSE_FAILED|code=%{public}d',
          code === undefined ? -1 : code);
      }
    }
  }

  const bytes: number = sumReportBytes(reports);
  const totalCapHit: boolean = budget <= 0;
  let copiedFiles: number = 0;
  for (let i = 0; i < reports.length; i++) {
    const report: LogExportFileReport = reports[i];
    if (report.present && !report.skipped && report.errorCode === undefined) {
      copiedFiles++;
    }
  }
  const done: LogExportResult = {
    outcome: LOG_EXPORT_OUTCOME_EXPORTED, uri: uri, files: reports, bytes: bytes,
    totalCapHit: totalCapHit
  };
  hilog.info(DOMAIN, TAG,
    'LOG_EXPORT_DONE|copied=%{public}d|present=%{public}d|copiedFiles=%{public}d|capHit=%{public}s',
    bytes, presentCount, copiedFiles, totalCapHit ? 'true' : 'false');
  return done;
}
