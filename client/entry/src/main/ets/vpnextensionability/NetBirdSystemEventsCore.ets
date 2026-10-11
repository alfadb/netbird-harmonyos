/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// NetBirdSystemEventsCore — PURE LOGIC only, no @ohos / platform imports. The
// host-side selfcheck imports this file byte-identical
// (client/entry/selfcheck/run-system-events.mts), so everything here must stay
// plain TypeScript: no enums, no decorators, no imports.
//
// WHAT THIS FILE IS: how one HiAppEvent system event becomes ONE bounded,
// sanitized persistent-log line, and how a re-delivering watcher stays
// bounded. NetBirdSystemEvents.ets only subscribes and hands the raw params
// object in here; every format/whitelist/bound/dedup decision lives below, so
// all of it is host-verifiable without a device.
//
// OFFICIAL FACTS (SDK d.ts of this toolchain, compileSdkVersion 26.0.0:
//   $DEVECO_SDK_HOME/default/openharmony/ets/api/@ohos.hiviewdfx.hiAppEvent.d.ts):
//   - domain.OS (since 11) is the system event domain.
//   - event.APP_CRASH (11), APP_FREEZE (11), RESOURCE_OVERLIMIT (12) and
//     APP_KILLED (20) are the system event names this module watches.
//   - Watcher.onReceive?: (domain: string, appEventGroups: Array<AppEventGroup>) => void
//   - AppEventGroup { name: string; appEventInfos: Array<AppEventInfo> }
//   - AppEventInfo { domain: string; name: string; eventType: EventType; params: object }
//   - addWatcher(watcher): AppEventPackageHolder (null on failure, throws on a
//     bad watcher); removeWatcher(watcher): void.
//   The event-name string constants are pinned below as LITERALS
//   (WATCHED_EVENT_NAMES) because this file cannot import the SDK; the runtime
//   subscribes with the SDK constants and the selfcheck pins literal == SDK id.
//
// OFFICIAL FIELD TABLES behind the whitelist below (OpenHarmony docs, API 26):
//   APP_KILLED (params: time/reason/foreground/last_exit_detail_info; the last
//   one is a string document with exit_msg/kill_reason/pid/process_name/
//   process_state/pss/rss/timestamp/uid):
//   https://raw.githubusercontent.com/rvaim/openharmony-docs/master/en/application-dev/dfx/hiappevent-watcher-app-killed-events.md
//   APP_FREEZE (process memory is nested: memory.rss / memory.pss):
//   https://raw.githubusercontent.com/rvaim/openharmony-docs/master/en/application-dev/dfx/hiappevent-watcher-freeze-events.md
//   RESOURCE_OVERLIMIT / resource leak (params: time/pid/uid/resource_type/
//   memory.rss/memory.pss/fd/thread/external_log/...):
//   https://raw.githubusercontent.com/rvaim/openharmony-docs/master/en/application-dev/dfx/hiappevent-watcher-resourceleak-events.md
//
// APP_KILLED IS DELIVERED ASYNCHRONOUSLY ON THE NEXT LAUNCH of the process
//   that subscribes: the event describes an exit that already happened. A
//   subscription only ever sees events from its own start() onwards; nothing
//   here can reconstruct a kill that happened before the subscription existed,
//   which is why the runtime writes a `sys_watcher_started` line (see below).
//
// PARAMS WHITELIST — only these keys are ever read, and only these lines are
// ever written (the whole params object is NEVER stringified):
//   top level: time, reason, kill_reason, pid, process_name, process_state,
//              rss, pss, resource_type (the RESOURCE_OVERLIMIT discriminator:
//              pss_memory / rss_memory / ion_memory / gpu_memory / js_heap /
//              fd / thread), and `memory` read for its `rss`/`pss` only (the
//              APP_FREEZE and RESOURCE_OVERLIMIT shapes nest process memory
//              under `memory`).
//   last_exit_detail_info (string document, API 26) -> only exit_msg,
//              kill_reason, pid, process_name, process_state, rss, pss,
//              timestamp. The raw document is bounded BEFORE parsing; a
//              document above the bound is not parsed at all and is recorded
//              as ev_detail_truncated=true.
//   Everything else in params — hilog[], event_handler[], threads[], frames,
//   exception, external_log paths, page_switch_log, bundle/uid identifiers,
//   the raw params object — is never read, never stringified, never emitted.
//   Unknown events (any name outside WATCHED_EVENT_NAMES) project to null.
//
// KILL REASON IS ALWAYS DISTINGUISHABLE: for APP_KILLED the line always
//   carries ev_kill_reason — the exit detail's kill_reason, else the top-level
//   reason, else the literal token `na` (the system reported none). A missing
//   reason is never rendered as 0/unknown/empty, so "no reason reported" and
//   "reason X" cannot be confused in the persistent log.
//
// THE VPN PROCESS'S OWN EXIT: the UI process's automatic reconnect may only
//   start the extension when it has proof the previous process is really gone,
//   and the only such proof is the platform's APP_KILLED for this app's own
//   `:vpn` process. killedProcessRecord() projects exactly that occurrence and
//   nothing else; KilledProcessTracker remembers the latest one. APP_KILLED is
//   delivered on the NEXT launch, so this is memory of an already-finished
//   process, never a prediction.
//
// LINE (one line per record, '\n'-terminated):
//   ts=<ISO-8601 UTC> pid=<writer pid> role=<ui|vpn> event=<sys_*> [<k>=<v> ...]
//   - the envelope matches NetBirdDiagnosticsCore.buildRecord, so one sink can
//     carry both line families
//   - payload keys are prefixed ev_ so the EVENT's pid can never be confused
//     with the WRITING process pid (which may be a later launch)
//   - fields are emitted in lexicographic key order (deterministic)
//   - every line is <= MAX_LINE_CHARS (512) chars incl. '\n'; overflow is cut
//     and marked with a trailing '~' immediately before the newline
//   - every value is restricted to SAFE_CHARS (so a value can never inject a
//     space = field separator, or a newline = fake record), is <=
//     MAX_VALUE_CHARS, and a secret-looking value becomes `redacted`
//
// DEDUP: EventDeduper keeps a bounded FIFO window of occurrence fingerprints
//   (event name + occurrence time + reason + pid). ONLY an exact re-delivery of
//   the same occurrence is suppressed — a genuinely new occurrence always has a
//   new `time`, so real kills/freezes are never dropped. The suppressed count is
//   handed over exactly once (takeSuppressed) and reported as ONE
//   `event=sys_event_dup ev_suppressed=<n>` line, so a watcher that re-delivers
//   the same group can never grow the log without bound.

export const RECORD_SCHEMA_VERSION: number = 1;
export const MAX_LINE_CHARS: number = 512;
export const MAX_VALUE_CHARS: number = 48;
export const MAX_EVENT_NAME_CHARS: number = 40;
export const MAX_TS_CHARS: number = 32;
export const MAX_ROLE_CHARS: number = 8;

// A params object arriving as a JSON string is parsed only below this length.
export const MAX_PARAMS_INPUT_CHARS: number = 4096;
// last_exit_detail_info is bounded BEFORE parsing: above this, it is not parsed.
export const MAX_DETAIL_INPUT_CHARS: number = 2048;
// keys copied out of a Map-shaped params object (the SDK itself caps params).
export const MAX_PARAM_KEYS: number = 32;
// infos projected out of one event group; the remainder is reported as dropped.
export const MAX_INFOS_PER_GROUP: number = 16;
// groups projected out of one onReceive callback (the runtime enforces this).
export const MAX_GROUPS_PER_CALLBACK: number = 8;
export const DEDUP_WINDOW: number = 64;

export const UNAVAILABLE: string = 'na';
export const REDACTED: string = 'redacted';
export const EMPTY_VALUE: string = '-';

// Literal SDK values of hiAppEvent.event.* (see the SDK d.ts header). The
// runtime subscribes with the SDK constants; the selfcheck pins these literals
// against those constants' documented values and against the runtime text.
export const WATCHED_EVENT_NAMES: string[] = ['APP_CRASH', 'APP_FREEZE', 'RESOURCE_OVERLIMIT', 'APP_KILLED'];
export const APP_KILLED_NAME: string = 'APP_KILLED';

// Same safe set as NetBirdDiagnosticsCore plus '()[]', because the official
// reason strings are spelled `ResourceLeak(...)` / `ResourceLeak[...]`; none of
// these characters can inject a space (field separator) or a newline.
const SAFE_CHARS: string = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_.:+@=/-,;()[]';

// ---------------------------------------------------------------------------
// value sanitising / redaction
// ---------------------------------------------------------------------------

export function sanitizeToken(raw: string, maxChars: number): string {
  if (raw.length === 0 || maxChars <= 0) {
    return EMPTY_VALUE;
  }
  let out: string = '';
  for (let i = 0; i < raw.length && out.length < maxChars; i++) {
    const ch: string = raw.charAt(i);
    out += SAFE_CHARS.indexOf(ch) >= 0 ? ch : '_';
  }
  return out.length === 0 ? EMPTY_VALUE : out;
}

// A >=32-char run of base64/base64url/hex characters with no '.', '/', '-' or
// '_' separator is treated as an opaque credential. Dotted values (FQDNs) and
// values with spaces/colons stay readable unless a prefix rule matches.
function isOpaqueRun(value: string): boolean {
  if (value.length < 32) {
    return false;
  }
  for (let i = 0; i < value.length; i++) {
    const ch: string = value.charAt(i);
    const alnum: boolean = (ch >= 'A' && ch <= 'Z') || (ch >= 'a' && ch <= 'z') ||
      (ch >= '0' && ch <= '9');
    if (!alnum && ch !== '+' && ch !== '-' && ch !== '_' && ch !== '=') {
      return false;
    }
  }
  return true;
}

export function looksSecret(key: string, value: string): boolean {
  const k: string = key.toLowerCase();
  if (k.indexOf('key') >= 0 || k.indexOf('token') >= 0 || k.indexOf('secret') >= 0 ||
    k.indexOf('pass') >= 0 || k.indexOf('credential') >= 0 || k.indexOf('auth') >= 0) {
    return true;
  }
  if (value.indexOf('Bearer ') >= 0 || value.indexOf('eyJ') === 0 ||
    value.indexOf('sb-') === 0 || value.indexOf('sk-') === 0 || value.indexOf('nbp_') === 0) {
    return true;
  }
  return isOpaqueRun(value);
}

// Only scalars survive: an array/object value under a known key is never
// stringified, it becomes `na` (the field exists but is not a readable scalar).
export function normalizeScalar(key: string, value: Object | undefined | null): string {
  if (value === undefined || value === null) {
    return UNAVAILABLE;
  }
  if (typeof value === 'number') {
    return Number.isFinite(value) ? String(value) : UNAVAILABLE;
  }
  if (typeof value === 'boolean') {
    return value ? 'true' : 'false';
  }
  if (typeof value !== 'string') {
    return UNAVAILABLE;
  }
  const text: string = value;
  if (looksSecret(key, text)) {
    return REDACTED;
  }
  return sanitizeToken(text, MAX_VALUE_CHARS);
}

// ---------------------------------------------------------------------------
// params access (JSON string / Map / plain object — all three SDK shapes)
// ---------------------------------------------------------------------------

// Duck-typed view of a Map-shaped params object. A plain object never passes
// the typeof guards below, so the cast is only ever READ through those guards.
interface FieldMapLike {
  has(key: string): boolean;
  get(key: string): Object | undefined;
  forEach(callback: (value: Object, key: string) => void): void;
}

function parseJsonMap(text: string): Record<string, Object> | null {
  if (text.length === 0 || text.length > MAX_PARAMS_INPUT_CHARS) {
    return null;
  }
  let parsed: Object | null = null;
  try {
    parsed = JSON.parse(text) as Object;
  } catch (error) {
    return null;
  }
  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
    return null;
  }
  return toFieldMap(parsed);
}

// Normalizes any SDK params shape to one plain record, or null when the input
// is unusable. A string is bounded BEFORE parse; arrays and scalars are refused
// (never stringified); a Map is copied with a bounded key count.
export function toFieldMap(raw: Object | undefined | null): Record<string, Object> | null {
  if (raw === undefined || raw === null) {
    return null;
  }
  if (typeof raw === 'string') {
    return parseJsonMap(raw);
  }
  if (typeof raw === 'number' || typeof raw === 'boolean') {
    return null;
  }
  if (Array.isArray(raw)) {
    return null;
  }
  const candidate: FieldMapLike = raw as FieldMapLike;
  if (typeof candidate.has === 'function' && typeof candidate.get === 'function' &&
    typeof candidate.forEach === 'function') {
    const out: Record<string, Object> = {};
    let count: number = 0;
    candidate.forEach((value: Object, key: string): void => {
      if (count < MAX_PARAM_KEYS) {
        out[key] = value;
        count += 1;
      }
    });
    return out;
  }
  return raw as Record<string, Object>;
}

// ---------------------------------------------------------------------------
// projection
// ---------------------------------------------------------------------------

export function isWatchedEvent(name: string): boolean {
  for (let i = 0; i < WATCHED_EVENT_NAMES.length; i++) {
    if (WATCHED_EVENT_NAMES[i] === name) {
      return true;
    }
  }
  return false;
}

export function logEventName(name: string): string {
  return sanitizeToken('sys_' + name.toLowerCase(), MAX_EVENT_NAME_CHARS);
}

// Pure projection of one event's params to the ev_* whitelist. Missing keys are
// simply absent; APP_KILLED always gets ev_kill_reason (value or `na`).
export function buildEventFields(sdkName: string, params: Object | undefined | null): Record<string, string | undefined> {
  const fields: Record<string, string | undefined> = {};
  const map: Record<string, Object> | null = toFieldMap(params);
  let hasPid: boolean = false;
  let hasProcessName: boolean = false;
  let hasProcessState: boolean = false;
  let hasRss: boolean = false;
  let hasPss: boolean = false;
  let hasKillReason: boolean = false;
  let reasonValue: string = UNAVAILABLE;
  let hasReason: boolean = false;
  let detail: Record<string, Object> | null = null;
  let detailTruncated: boolean = false;

  if (map !== null) {
    const time: Object | undefined = map['time'];
    if (time !== undefined) {
      fields['ev_time'] = normalizeScalar('time', time);
    }
    const reason: Object | undefined = map['reason'];
    if (reason !== undefined) {
      reasonValue = normalizeScalar('reason', reason);
      hasReason = true;
      fields['ev_reason'] = reasonValue;
    }
    const killReason: Object | undefined = map['kill_reason'];
    if (killReason !== undefined) {
      fields['ev_kill_reason'] = normalizeScalar('kill_reason', killReason);
      hasKillReason = true;
    }
    const pid: Object | undefined = map['pid'];
    if (pid !== undefined) {
      fields['ev_pid'] = normalizeScalar('pid', pid);
      hasPid = true;
    }
    const processName: Object | undefined = map['process_name'];
    if (processName !== undefined) {
      fields['ev_process_name'] = normalizeScalar('process_name', processName);
      hasProcessName = true;
    }
    const processState: Object | undefined = map['process_state'];
    if (processState !== undefined) {
      fields['ev_process_state'] = normalizeScalar('process_state', processState);
      hasProcessState = true;
    }
    const rss: Object | undefined = map['rss'];
    if (rss !== undefined) {
      fields['ev_rss_kb'] = normalizeScalar('rss', rss);
      hasRss = true;
    }
    const pss: Object | undefined = map['pss'];
    if (pss !== undefined) {
      fields['ev_pss_kb'] = normalizeScalar('pss', pss);
      hasPss = true;
    }
    // RESOURCE_OVERLIMIT's own discriminator (which resource leaked).
    const resourceType: Object | undefined = map['resource_type'];
    if (resourceType !== undefined) {
      fields['ev_resource_type'] = normalizeScalar('resource_type', resourceType);
    }
    // APP_FREEZE nests process memory under `memory`; only rss/pss are read.
    const memory: Object | undefined = map['memory'];
    if (memory !== undefined) {
      const memoryMap: Record<string, Object> | null = toFieldMap(memory);
      if (memoryMap !== null) {
        if (!hasRss) {
          const memoryRss: Object | undefined = memoryMap['rss'];
          if (memoryRss !== undefined) {
            fields['ev_rss_kb'] = normalizeScalar('rss', memoryRss);
            hasRss = true;
          }
        }
        if (!hasPss) {
          const memoryPss: Object | undefined = memoryMap['pss'];
          if (memoryPss !== undefined) {
            fields['ev_pss_kb'] = normalizeScalar('pss', memoryPss);
            hasPss = true;
          }
        }
      }
    }
    const rawDetail: Object | undefined = map['last_exit_detail_info'];
    if (rawDetail !== undefined && rawDetail !== null) {
      if (typeof rawDetail === 'string' && rawDetail.length > MAX_DETAIL_INPUT_CHARS) {
        // Bounded BEFORE parsing: an oversized document is not parsed at all.
        detailTruncated = true;
      } else {
        detail = toFieldMap(rawDetail);
      }
    }
  }

  if (detail !== null) {
    const detailPid: Object | undefined = detail['pid'];
    if (!hasPid && detailPid !== undefined) {
      fields['ev_pid'] = normalizeScalar('pid', detailPid);
    }
    const detailName: Object | undefined = detail['process_name'];
    if (!hasProcessName && detailName !== undefined) {
      fields['ev_process_name'] = normalizeScalar('process_name', detailName);
    }
    const detailState: Object | undefined = detail['process_state'];
    if (!hasProcessState && detailState !== undefined) {
      fields['ev_process_state'] = normalizeScalar('process_state', detailState);
    }
    const detailRss: Object | undefined = detail['rss'];
    if (!hasRss && detailRss !== undefined) {
      fields['ev_rss_kb'] = normalizeScalar('rss', detailRss);
    }
    const detailPss: Object | undefined = detail['pss'];
    if (!hasPss && detailPss !== undefined) {
      fields['ev_pss_kb'] = normalizeScalar('pss', detailPss);
    }
    const detailKillReason: Object | undefined = detail['kill_reason'];
    if (detailKillReason !== undefined) {
      fields['ev_kill_reason'] = normalizeScalar('kill_reason', detailKillReason);
      hasKillReason = true;
    }
    const exitMsg: Object | undefined = detail['exit_msg'];
    if (exitMsg !== undefined) {
      fields['ev_exit_msg'] = normalizeScalar('exit_msg', exitMsg);
    }
    const detailTs: Object | undefined = detail['timestamp'];
    if (detailTs !== undefined) {
      fields['ev_detail_ts'] = normalizeScalar('timestamp', detailTs);
    }
  }

  if (!hasKillReason && sdkName === APP_KILLED_NAME) {
    // Known vs missing: APP_KILLED always carries a kill reason, so "the system
    // reported none" is an explicit token, never a silently absent field.
    fields['ev_kill_reason'] = hasReason ? reasonValue : UNAVAILABLE;
  }
  if (detailTruncated) {
    fields['ev_detail_truncated'] = 'true';
  }
  return fields;
}

// Exact re-deliveries of the same occurrence share this fingerprint; a new
// occurrence has a new `time` (ms) and therefore a new fingerprint.
export function eventFingerprint(sdkName: string, fields: Record<string, string | undefined>): string {
  const time: string | undefined = fields['ev_time'];
  const killReason: string | undefined = fields['ev_kill_reason'];
  const reason: string | undefined = fields['ev_reason'];
  const pid: string | undefined = fields['ev_pid'];
  return sdkName + '|' + (time !== undefined ? time : UNAVAILABLE) +
    '|' + (killReason !== undefined ? killReason : UNAVAILABLE) +
    '|' + (reason !== undefined ? reason : UNAVAILABLE) +
    '|' + (pid !== undefined ? pid : UNAVAILABLE);
}

export function buildSystemEventLine(tsIso: string, pid: number, role: string, eventName: string,
  fields: Record<string, string | undefined>): string {
  let line: string = 'ts=' + sanitizeToken(tsIso, MAX_TS_CHARS) + ' pid=' + String(pid) +
    ' role=' + sanitizeToken(role, MAX_ROLE_CHARS) + ' event=' + sanitizeToken(eventName, MAX_EVENT_NAME_CHARS);
  const keys: string[] = Object.keys(fields);
  keys.sort();
  for (let i = 0; i < keys.length; i++) {
    const key: string = keys[i];
    const value: string | undefined = fields[key];
    if (value === undefined) {
      continue;
    }
    line += ' ' + key + '=' + sanitizeToken(value, MAX_VALUE_CHARS);
  }
  line += '\n';
  if (line.length > MAX_LINE_CHARS) {
    line = line.substring(0, MAX_LINE_CHARS - 2) + '~\n';
  }
  return line;
}

export interface SystemEventProjection {
  sdkName: string;
  logName: string;
  fields: Record<string, string | undefined>;
  fingerprint: string;
}

// Unknown events project to null: they never become records.
export function projectEvent(sdkName: string, params: Object | undefined | null): SystemEventProjection | null {
  if (!isWatchedEvent(sdkName)) {
    return null;
  }
  const fields: Record<string, string | undefined> = buildEventFields(sdkName, params);
  return {
    sdkName: sdkName,
    logName: logEventName(sdkName),
    fields: fields,
    fingerprint: eventFingerprint(sdkName, fields)
  };
}

export function buildDupLine(tsIso: string, pid: number, role: string, suppressed: number): string {
  const fields: Record<string, string> = {};
  fields['ev_suppressed'] = String(suppressed);
  return buildSystemEventLine(tsIso, pid, role, 'sys_event_dup', fields);
}

export function buildDroppedLine(tsIso: string, pid: number, role: string, sdkName: string, dropped: number): string {
  const fields: Record<string, string> = {};
  fields['ev_dropped'] = String(dropped);
  fields['ev_event'] = sdkName;
  return buildSystemEventLine(tsIso, pid, role, 'sys_event_dropped', fields);
}

// Lifecycle lines make the subscription boundary explicit: records cannot
// describe anything that happened before sys_watcher_started.
export function buildWatcherLine(tsIso: string, pid: number, role: string, state: string, code: string): string {
  const fields: Record<string, string> = {};
  fields['ev_code'] = code;
  fields['ev_state'] = state;
  return buildSystemEventLine(tsIso, pid, role, 'sys_watcher_' + sanitizeToken(state, 12), fields);
}

// Bounded projection of one event group. totalCount is the group's real info
// count (an unbounded fact); paramsList is the already-bounded prefix the
// runtime collected, so the dropped remainder is reported honestly.
export function projectInfos(tsIso: string, pid: number, role: string, sdkName: string,
  paramsList: Array<Object>, totalCount: number, deduper: EventDeduper): string[] {
  const lines: string[] = [];
  if (!isWatchedEvent(sdkName)) {
    return lines;
  }
  const bounded: number = paramsList.length < MAX_INFOS_PER_GROUP ? paramsList.length : MAX_INFOS_PER_GROUP;
  for (let i = 0; i < bounded; i++) {
    const projected: SystemEventProjection | null = projectEvent(sdkName, paramsList[i]);
    if (projected === null) {
      continue;
    }
    if (deduper.isDuplicate(projected.fingerprint)) {
      continue;
    }
    const suppressed: number = deduper.takeSuppressed();
    if (suppressed > 0) {
      lines.push(buildDupLine(tsIso, pid, role, suppressed));
    }
    lines.push(buildSystemEventLine(tsIso, pid, role, projected.logName, projected.fields));
  }
  const dropped: number = totalCount - bounded;
  if (dropped > 0) {
    lines.push(buildDroppedLine(tsIso, pid, role, sdkName, dropped));
  }
  // Duplicates that arrived after the last fresh record are still reported
  // exactly once (an all-duplicate callback yields one dup line).
  const rest: number = deduper.takeSuppressed();
  if (rest > 0) {
    lines.push(buildDupLine(tsIso, pid, role, rest));
  }
  return lines;
}

// ---------------------------------------------------------------------------
// bounded dedup window
// ---------------------------------------------------------------------------

// FIFO window of occurrence fingerprints with an honest suppression counter:
// only an exact re-delivery is suppressed, and takeSuppressed() hands the count
// over exactly once so the runtime can emit one sys_event_dup line.
export class EventDeduper {
  private window: string[] = [];
  private suppressedCount: number = 0;
  private readonly capacity: number;

  constructor(capacity: number) {
    this.capacity = capacity > 0 ? capacity : 1;
  }

  isDuplicate(fingerprint: string): boolean {
    for (let i = 0; i < this.window.length; i++) {
      if (this.window[i] === fingerprint) {
        this.suppressedCount += 1;
        return true;
      }
    }
    this.window.push(fingerprint);
    while (this.window.length > this.capacity) {
      this.window.shift();
    }
    return false;
  }

  takeSuppressed(): number {
    const taken: number = this.suppressedCount;
    this.suppressedCount = 0;
    return taken;
  }

  windowSize(): number {
    return this.window.length;
  }

  capacityValue(): number {
    return this.capacity;
  }
}

// ---------------------------------------------------------------------------
// the VPN extension's own exit — the auto-reconnect's criterion (7)
// ---------------------------------------------------------------------------

// The app's VPN extension runs in its OWN `:vpn` process, so its APP_KILLED is a
// fact about the extension; the UI process itself is named WITHOUT the suffix.
// The match below is therefore EXACT: a prefix test would let the page mistake
// its own exit (or a future sibling process) for the extension's, and then start
// an extension that is still alive.
export const VPN_PROCESS_NAME: string = 'cn.alfadb.netbird:vpn';

// {pid, reason, atMs} of ONE exit occurrence: pid is the process the system
// killed, reason the projected kill reason (`na` when the system reported none),
// atMs the occurrence's own time in ms (0 when it carried none). Process memory
// only — nothing here is written anywhere.
export interface ProcessExitRecord {
  pid: number;
  reason: string;
  atMs: number;
}

function isDecimalDigits(text: string): boolean {
  if (text.length === 0) {
    return false;
  }
  for (let i = 0; i < text.length; i++) {
    const ch: string = text.charAt(i);
    if (ch < '0' || ch > '9') {
      return false;
    }
  }
  return true;
}

// Pure: ONE APP_KILLED occurrence -> the VPN extension's exit, or null for
// everything else (another event, another process, an unreadable pid). Only the
// whitelisted projection is read (buildEventFields), and the pid must be plain
// decimal digits: a value this module cannot read exactly is refused, never
// rounded into a pid that might match the marker by accident.
export function killedProcessRecord(sdkName: string, params: Object | undefined | null): ProcessExitRecord | null {
  if (sdkName !== APP_KILLED_NAME) {
    return null;
  }
  const fields: Record<string, string | undefined> = buildEventFields(sdkName, params);
  if (fields['ev_process_name'] !== VPN_PROCESS_NAME) {
    return null;
  }
  const pidText: string | undefined = fields['ev_pid'];
  if (pidText === undefined || !isDecimalDigits(pidText)) {
    return null;
  }
  const pid: number = parseInt(pidText, 10);
  if (!Number.isFinite(pid) || pid <= 0) {
    return null;
  }
  let atMs: number = 0;
  const timeText: string | undefined = fields['ev_time'];
  if (timeText !== undefined && isDecimalDigits(timeText)) {
    const parsed: number = parseInt(timeText, 10);
    if (Number.isFinite(parsed) && parsed > 0) {
      atMs = parsed;
    }
  }
  const reasonText: string | undefined = fields['ev_kill_reason'];
  const record: ProcessExitRecord = {
    pid: pid,
    reason: reasonText === undefined ? UNAVAILABLE : reasonText,
    atMs: atMs
  };
  return record;
}

// The UI process's memory of the LATEST VPN exit it was told about. Kept in the
// pure core (the runtime owns only one instance and the ui-role guard), so the
// two rules the page leans on — "only the `:vpn` process" and "a later event
// replaces an earlier one" — are host-checked by run-system-events.mts.
// Deliberately NOT cleared by stop(): the exit it describes still happened, and
// a stale record can only make the page's gate more careful (a pid that does not
// match the marker's).
export class KilledProcessTracker {
  private latest: ProcessExitRecord | null = null;

  // true = this event was the VPN extension's exit and became the record.
  note(sdkName: string, params: Object | undefined | null): boolean {
    const record: ProcessExitRecord | null = killedProcessRecord(sdkName, params);
    if (record === null) {
      return false;
    }
    this.latest = record;
    return true;
  }

  record(): ProcessExitRecord | null {
    return this.latest;
  }
}
