/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Diagnostics core — PURE LOGIC only, no @ohos / platform imports. The
// host-side selfcheck imports this file byte-identical
// (client/entry/selfcheck/run-diagnostics.mts), so everything here must stay
// plain TypeScript: no enums, no decorators, no imports.
//
// WHAT THIS FILE IS: the whole observable contract of the diagnostics helper
// (file names + rotation order, record/line format and bounds, marker format
// and prior-exit rule, value sanitising/redaction, queue bounding). The
// runtime module NetBirdDiagnostics.ets only does platform IO and calls in
// here, so every decision below is host-verifiable without a device.
//
// FIXED BASENAMES (export-friendly; nothing else is ever touched)
//   UI records, newest -> oldest, exactly 4 files, 1 MiB each, 4 MiB total:
//     [0] netbird-ui.log      (active append target)
//     [1] netbird-ui.log.1
//     [2] netbird-ui.log.2
//     [3] netbird-ui.log.3    (oldest; dropped on the next rotation)
//   Lifecycle markers, ONE PER ROLE (separate basenames, never the log name):
//     ui  -> netbird-diag-ui.marker
//     vpn -> netbird-diag-vpn.marker
//   recordPath() refuses any basename outside this allowlist, so the runtime
//   can only unlink/rename its own files.
//
// RECORD LINE (one line per record, '\n'-terminated):
//   ts=<ISO-8601 UTC> pid=<pid> role=<ui|vpn> event=<name>[ <key>=<value> ...]
//   - fields are emitted in lexicographic key order (deterministic)
//   - every line is <= MAX_LINE_CHARS (512) chars incl. '\n'; overflow is cut
//     and marked with a trailing '~' immediately before the newline
//   - all values are restricted to SAFE_CHARS, so a value can never inject a
//     space (field separator) or a newline (fake record)
//   - memory keys carry their unit AND scope: rss_kb / swap_kb / heap_kb are
//     THIS process (hidebug.getRssInfo / getAppVMMemoryInfo, KB);
//     sys_avail_kb is the SYSTEM MemAvailable (hidebug.getSystemMemInfo, KB);
//     an unavailable reading is the literal token `na` (never a fake 0)
//   - the 0.0.5 resource keys ride the SAME mem_sample record and cadence:
//     cpu_ms is the platform's own CUMULATIVE CPU time of THIS process since
//     its start (process.getPastCpuTime, ms) — a raw counter, never turned into
//     a rate, because the wall-clock gap between ticks is not guaranteed;
//     threads / vmsize_kb / rss_anon_kb / rss_file_kb are THIS process from
//     /proc/self/status (Threads, VmSize, RssAnon, RssFile);
//     vmdata_kb / vmstk_kb / rss_shmem_kb are the SAME ONE read of that node
//     (VmData, VmStk, RssShmem) — zero added system calls;
//     sys_swap_total_kb / sys_swap_free_kb are the SYSTEM SwapTotal/SwapFree
//     from /proc/meminfo (MemAvailable stays with hidebug, no duplicate);
//     fd_count is the NUMBER of entries in /proc/self/fd — a count only, no fd
//     name/target/inode is ever read into a record — and fd_capped=true marks
//     that the count hit the enumeration cap, i.e. the value is a lower bound
//
//   - every MEM_DETAIL_INTERVAL (10) samples, ONE SEPARATE `event=mem_detail`
//     record — a second line, NOT a wider mem_sample: with PSS and the Rust
//     heap added, the full field set no longer fits MAX_LINE_CHARS, and a cut
//     line would silently lose its tail. It carries the ASYNC smaps_rollup
//     reader's five readings (hidebug.getAppNativeMemInfoAsync):
//     pss_kb / private_dirty_kb / shared_dirty_kb / private_clean_kb /
//     shared_clean_kb (pss = smaps_rollup Pss + SwapPss), plus — when the
//     caller injected a Rust-allocator reader (role-dependent; see start()) —
//     rust_heap_kb / rust_heap_peak_kb / rust_allocs / rust_frees parsed from
//     the NAPI `alloc_stats()` document, bytes converted to KiB by integer
//     division by 1024. rust_heap is the process's RUST GLOBAL ALLOCATOR ONLY:
//     never the ArkTS/JS heap, libace_napi, the platform TUN/route buffers or
//     process RSS. A NON-detail tick emits NO mem_detail record at all (the
//     keys are absent, not `na`); a detail record writes `na` for anything
//     unreadable, and a process with no injected reader (the UI role) writes
//     `na` for all four rust keys.
//
//   - the SAME detail tick emits ONE MORE record, `event=mem_vm`, immediately
//     after mem_detail: the ArkTS-VM / native-malloc / GC attribution readings
//     that answer "whose memory is the anonymous RSS?". They are SYNCHRONOUS
//     calls made inside the same single-flight runSample() detail branch (no
//     timer is added; the 1 s cadence is untouched):
//       hidebug.getAppVMMemoryInfo()         -> vm_total_heap_kb (totalHeap) and
//                                               vm_array_kb (allArraySize), KB —
//                                               the ArkTS heap total and every
//                                               array / ArrayBuffer-like object
//       hidebug.getNativeHeapSize()          -> native_heap_kb        (uordblks
//       hidebug.getNativeHeapAllocatedSize() -> native_heap_alloc_kb   + fordblks
//       hidebug.getNativeHeapFreeSize()      -> native_heap_free_kb    / uordblks
//                                               / fordblks, from mallinfo). The
//                                               platform answers BYTES; they are
//                                               converted to KiB by EXACT integer
//                                               division (bytesToKiBText), never
//                                               through a lossy bigint -> number
//                                               step. This is the libc malloc heap
//                                               ONLY — not the ArkTS heap, not
//                                               libace_napi, not process RSS.
//       hidebug.getVMRuntimeStat(key)        -> gc_count, gc_time_ms (ms, as the
//                                               platform reports it) and
//                                               gc_alloc_kb / gc_freed_kb (the two
//                                               gc-bytes-* counters, BYTES -> KiB
//                                               by exact integer division). The
//                                               four keys are declared HERE
//                                               (GC_STAT_*) and never spelled out
//                                               in the runtime module.
//       hidebug.getAppMemoryLimit()           -> rss_limit_kb / vss_limit_kb /
//                                               vm_heap_limit_kb (API 12
//                                               MemoryLimit; KILOBYTES as the
//                                               platform reports them, so no
//                                               division — only the decimal
//                                               validation of limitKiBText).
//     WHY A THIRD LINE: with every field at its own bound the mem_detail set above
//     plus these nine keys reaches ~695 chars (pid=42), past MAX_LINE_CHARS — so
//     the host selfcheck pins both the split and the lengths: the widest
//     mem_detail alone is 371 chars and the widest mem_vm alone is 487 (the nine
//     above plus the three memory-limit keys below), while the two sets joined
//     would be truncated. The new keys therefore get their OWN record instead of
//     being cut away. Each of the three legs is guarded: an API that is missing /
//     throws / answers a non-finite or negative number renders its own keys as
//     `na` (never a fabricated 0) and the FIRST such failure of the run is said
//     once (DIAG_DETAIL_UNAVAILABLE), exactly like the mem_detail legs. A
//     non-detail tick emits no mem_vm record at all.
//
//   - the platform's OWN per-process memory limit (2026-10-12, the analysis
//     report's extra measurement (a)): hidebug.getAppMemoryLimit() (API 12,
//     MemoryLimit) answers rssLimit / vssLimit / vmHeapLimit in KiB — the
//     platform memory manager's number, not RLIMIT_RSS. It is logged ONCE PER
//     PROCESS START on the diag_start record (the run's first record, so the
//     value is in the log from t=0) AND on every mem_vm record, so the limit
//     can be read off the same series as the RSS. `vmTotalHeapSize` is not
//     carried (a fourth key would push the widest mem_vm past MAX_LINE_CHARS).
//     An absent/throwing API renders `na` in both places; the detail-tick leg
//     reports it once (DIAG_DETAIL_UNAVAILABLE, stage `mem-limit`).
//
// MARKER FILE (single line): v=1 state=<open|clean> ts=<ISO> pid=<pid> role=<r>
//   start() writes state=open; stop() writes state=clean ONLY from the real
//   process/extension lifecycle stop callback. A marker left at state=open
//   with a DIFFERENT pid is reported once as PRIOR_EXIT_UNCLEAN — an
//   observation that the previous run of that role did not reach its stop
//   callback. It is NOT proof of a crash (force-stop, OOM kill and a lost
//   stop callback all look the same from here).
//
// QUEUE: bounded (QUEUE_CAPACITY records); overflow increments a dropped
//   counter, and the runtime emits ONE `event=diag_dropped dropped=<n>`
//   record when the queue has room again — records are never silently lost.
//
// REDACTION: persistent native logging (phone_log, supplied by the caller as
//   the VPN sink) owns its own redaction. This module adds the UI-side guard:
//   only whitelisted field keys are emitted (unknown keys are dropped), and a
//   value that looks like a credential (secret-ish key name, JWT/`Bearer `
//   header, `sb-`/`sk-`/`nbp_` prefix, or a >=32-char opaque base64url/hex
//   run) becomes the literal token `redacted`. This module never reads
//   config, keys, setup keys or any raw status document — only its own log
//   and marker files.
//
// SYSTEM-EVENT INGRESS: the system-event watcher projects its own already
//   sanitized lines (event=sys_*, NetBirdSystemEventsCore) and hands them to
//   the runtime's appendSystemEvent() instead of the public event() API, whose
//   48-char value bound would cut an APP_KILLED reason off the record. The
//   shape is validated HERE by isSystemEventLine(): the ts/pid/role/event
//   envelope, a `sys_` event name, and only `ev_`-prefixed bounded values from
//   the projector's own safe character set, one '\n'-terminated line within
//   MAX_LINE_CHARS (the projector's marked truncation shape is accepted). A raw
//   params dump, a foreign record or a multi-line blob cannot enter this way.

export type DiagRole = 'ui' | 'vpn';
export type DiagValue = string | number | boolean;
export type DiagFields = Record<string, DiagValue>;
export type DiagSink = (line: string) => void;

// ---------------------------------------------------------------------------
// fixed names, order and bounds
// ---------------------------------------------------------------------------

export const RECORD_SCHEMA_VERSION: number = 1;

// active file first, then rotated newest -> oldest. Rotation unlinks the last
// entry, shifts every other entry one step right, then moves the active file
// into slot 0 (see rotationSteps()).
export const UI_LOG_ACTIVE: string = 'netbird-ui.log';
export const UI_LOG_ROTATED: string[] = ['netbird-ui.log.1', 'netbird-ui.log.2', 'netbird-ui.log.3'];
export const UI_LOG_FILES_BY_AGE: string[] = [
  'netbird-ui.log', 'netbird-ui.log.1', 'netbird-ui.log.2', 'netbird-ui.log.3'
];
export const MARKER_BASENAME_UI: string = 'netbird-diag-ui.marker';
export const MARKER_BASENAME_VPN: string = 'netbird-diag-vpn.marker';
export const MARKER_STATE_OPEN: string = 'open';
export const MARKER_STATE_CLEAN: string = 'clean';
// 2026-10-09 (0.0.17): the extension writes this state SYNCHRONOUSLY from the
// onDestroy ENTRY (NetBirdVpnExtensionAbility.writeStopMarkerSync), because the
// asynchronous teardown continuation that would write `clean` does not survive
// the process on the device: the JS context is destroyed ~3 ms after onDestroy
// returns and the process is gone within ~21 ms, so the destroyOnce
// continuation (await createPromise -> await connection.destroy() -> the late
// stopDiagnostics()) never runs — the marker stayed `open` and the NEXT session
// reported PRIOR_EXIT_UNCLEAN for an orderly user stop (V016 evidence:
// netbird-vpn.log:1528-1547 ends at `vpn_destroy_phase phase=requested`, no
// `settled`). `stopping` = an ORDERLY stop was entered and only the marker's
// last, asynchronous transition (-> clean) was lost to the process end; it is
// NOT a prior unclean exit (see isPriorUnclean: only state=open is claimed).
// The token is 8 chars, exactly the state bound buildMarker() enforces.
export const MARKER_STATE_STOPPING: string = 'stopping';

export const PER_FILE_MAX_BYTES: number = 1048576; // 1 MiB
export const TOTAL_MAX_BYTES: number = 4194304; // 4 MiB = 4 files x 1 MiB
export const MAX_LINE_CHARS: number = 512; // incl. the trailing newline
export const MAX_VALUE_CHARS: number = 48;
export const MAX_EVENT_NAME_CHARS: number = 40;
export const QUEUE_CAPACITY: number = 256;
export const SAMPLE_INTERVAL_MS: number = 1000; // vpn role
// 2026-10-11 (0.0.21): the UI process samples every 10 s, NOT every 1 s. The UI
// log is 1 MiB x 4 and its dominant writer is one mem_sample (~280-320 B) per
// tick, so the shared 1 s cadence filled the whole 4 MiB in ~5 h; 10 s keeps the
// same format, fields and bounds while stretching the window past a day. The vpn
// process keeps 1 s — its per-second series is what the recycle analysis reads.
export const UI_SAMPLE_INTERVAL_MS: number = 10000; // ui role
// Every Nth mem_sample is followed by ONE mem_detail record (see the header:
// a separate line, because the PSS + Rust-heap field set does not fit
// MAX_LINE_CHARS). The counter counts mem_sample ATTEMPTS inside a run, so a
// tick dropped by the single-flight guard does not advance it. The cadence is
// counted in SAMPLES, not seconds, so it is unchanged per role — only the
// wall-clock period it implies moves with the role's interval: ui emits
// mem_detail/mem_vm every 10 ticks (100 s), vpn every 10 ticks (10 s).
export const MEM_DETAIL_INTERVAL: number = 10;

// ---------------------------------------------------------------------------
// sampling cadence (per role)
// ---------------------------------------------------------------------------

// The period of the ONE sampling timer a run owns (there is still exactly one
// setInterval per run — this only picks its delay). The ui role is the log
// space hog, so it gets the slow cadence; vpn keeps 1 s. Anything that is not
// the ui role keeps the historical 1 s (an unknown role is rejected by start()
// anyway).
export function sampleIntervalFor(role: DiagRole): number {
  return role === 'ui' ? UI_SAMPLE_INTERVAL_MS : SAMPLE_INTERVAL_MS;
}

// Bounds for the /proc half of the sample (see PROC SAMPLING below). The byte
// cap is the actual read buffer, so a proc node can never be slurped whole;
// the line/key/digit caps bound the pure parse of that capped text.
export const PROC_READ_MAX_BYTES: number = 16384; // 16 KiB per proc node
export const PROC_MAX_LINES: number = 512;
export const MAX_PROC_KEY_CHARS: number = 32;
export const MAX_PROC_DIGITS: number = 15; // < 2^53, so Number() stays exact

// /proc/self/fd is enumerated with listFile(path, { listNum: FD_LIST_MAX }), so
// the list — and therefore the count — is bounded; a full list is reported as a
// lower bound (fd_capped=true) instead of an exact count.
export const FD_LIST_MAX: number = 1024;

export const REDACTED: string = 'redacted';
export const UNAVAILABLE: string = 'na';
export const EMPTY_VALUE: string = '-';

// Values may only contain these characters. Deliberately EXCLUDES space and
// newline (record/field separators → injection) and '[' ']' etc.
const SAFE_CHARS: string = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_.:+@=/-,;';

// Internal record names the runtime itself emits.
export const INTERNAL_EVENT_NAMES: string[] = [
  'diag_start', 'diag_stop', 'diag_dropped', 'mem_sample', 'mem_detail', 'mem_vm',
  'PRIOR_EXIT_UNCLEAN'
];

// Public (caller-supplied) event names must be lowercase tokens and must not
// shadow the internal names above. The value is a module constant, never raw
// user input.
export function isSafeEventName(name: string): boolean {
  if (name.length === 0 || name.length > MAX_EVENT_NAME_CHARS) {
    return false;
  }
  const first: string = name.charAt(0);
  if (first < 'a' || first > 'z') {
    return false;
  }
  for (let i = 0; i < name.length; i++) {
    const ch: string = name.charAt(i);
    const lower: boolean = ch >= 'a' && ch <= 'z';
    const digit: boolean = ch >= '0' && ch <= '9';
    if (!lower && !digit && ch !== '_') {
      return false;
    }
  }
  return INTERNAL_EVENT_NAMES.indexOf(name) < 0;
}

export function isPublicEventName(name: string): boolean {
  return isSafeEventName(name);
}

// ---------------------------------------------------------------------------
// system-event line ingress (own SystemEvents format only)
// ---------------------------------------------------------------------------

// The ONLY foreign line family this writer accepts (see the header): the
// system-event watcher's own projected line. Envelope keys are exact and
// ordered; the event name is `sys_` + [a-z0-9_]; every payload field is
// `ev_`-prefixed with a bounded value from the projector's safe set. Anything
// else — a public record, a raw params dump, a foreign role, a second line —
// is refused, so nothing can be smuggled into the log through this door.
export const SYS_EVENT_NAME_PREFIX: string = 'sys_';
export const SYS_EVENT_FIELD_PREFIX: string = 'ev_';
export const MAX_SYSTEM_TS_CHARS: number = 32;
export const MAX_SYSTEM_PID_CHARS: number = 20;

// Same safe set as NetBirdSystemEventsCore (it adds '()[]' for the official
// `ResourceLeak(...)` reason strings). This is a membership check only: the
// value was already projected, bounded and redacted by that core, so the
// ingress never re-sanitizes it (that would corrupt a legitimate reason).
const SYS_SAFE_CHARS: string =
  'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_.:+@=/-,;()[]';

function isSysSafeText(text: string, maxChars: number): boolean {
  if (text.length === 0 || text.length > maxChars) {
    return false;
  }
  for (let i = 0; i < text.length; i++) {
    if (SYS_SAFE_CHARS.indexOf(text.charAt(i)) < 0) {
      return false;
    }
  }
  return true;
}

function isAllDigits(text: string): boolean {
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

// `key=value` with the exact key; null unless the shape, the char set and the
// length bound all hold.
function sysFieldValue(token: string, key: string, maxChars: number): string | null {
  const prefix: string = key + '=';
  if (token.length <= prefix.length || token.substring(0, prefix.length) !== prefix) {
    return null;
  }
  const value: string = token.substring(prefix.length);
  return isSysSafeText(value, maxChars) ? value : null;
}

function isSysFieldKey(key: string): boolean {
  if (key.length <= SYS_EVENT_FIELD_PREFIX.length ||
    key.substring(0, SYS_EVENT_FIELD_PREFIX.length) !== SYS_EVENT_FIELD_PREFIX) {
    return false;
  }
  for (let i = SYS_EVENT_FIELD_PREFIX.length; i < key.length; i++) {
    const ch: string = key.charAt(i);
    const lower: boolean = ch >= 'a' && ch <= 'z';
    const digit: boolean = ch >= '0' && ch <= '9';
    if (!lower && !digit && ch !== '_') {
      return false;
    }
  }
  return true;
}

function isSysEventName(name: string): boolean {
  if (name.length <= SYS_EVENT_NAME_PREFIX.length || name.length > MAX_EVENT_NAME_CHARS ||
    name.substring(0, SYS_EVENT_NAME_PREFIX.length) !== SYS_EVENT_NAME_PREFIX) {
    return false;
  }
  for (let i = SYS_EVENT_NAME_PREFIX.length; i < name.length; i++) {
    const ch: string = name.charAt(i);
    const lower: boolean = ch >= 'a' && ch <= 'z';
    const digit: boolean = ch >= '0' && ch <= '9';
    if (!lower && !digit && ch !== '_') {
      return false;
    }
  }
  return true;
}

// A `~`-marked truncated line may end inside a payload field, so its LAST
// token is allowed to be a partial `ev_` field (any safe prefix of one, even
// empty). The visible start must still be the field prefix, so the cut can
// never introduce a foreign token.
function isSysPartialTail(token: string): boolean {
  const bound: number = token.length < SYS_EVENT_FIELD_PREFIX.length
    ? token.length : SYS_EVENT_FIELD_PREFIX.length;
  if (token.substring(0, bound) !== SYS_EVENT_FIELD_PREFIX.substring(0, bound)) {
    return false;
  }
  for (let i = 0; i < token.length; i++) {
    if (SYS_SAFE_CHARS.indexOf(token.charAt(i)) < 0) {
      return false;
    }
  }
  return true;
}

export function isSystemEventLine(line: string): boolean {
  if (line.length === 0 || line.length > MAX_LINE_CHARS ||
    line.charAt(line.length - 1) !== '\n') {
    return false;
  }
  let body: string = line.substring(0, line.length - 1);
  if (body.indexOf('\n') >= 0 || body.indexOf('\r') >= 0) {
    return false;
  }
  // buildSystemEventLine marks an over-long line by cutting it to exactly
  // MAX_LINE_CHARS with '~' immediately before the newline; only that exact
  // shape may carry a partial trailing field.
  let truncated: boolean = false;
  if (line.length === MAX_LINE_CHARS && body.charAt(body.length - 1) === '~') {
    truncated = true;
    body = body.substring(0, body.length - 1);
  }
  const parts: string[] = body.split(' ');
  if (parts.length < 4) {
    return false;
  }
  if (sysFieldValue(parts[0], 'ts', MAX_SYSTEM_TS_CHARS) === null) {
    return false;
  }
  const pid: string | null = sysFieldValue(parts[1], 'pid', MAX_SYSTEM_PID_CHARS);
  if (pid === null || !isAllDigits(pid)) {
    return false;
  }
  const role: string | null = sysFieldValue(parts[2], 'role', 8);
  if (role !== 'ui' && role !== 'vpn') {
    return false;
  }
  const name: string | null = sysFieldValue(parts[3], 'event', MAX_EVENT_NAME_CHARS);
  if (name === null || !isSysEventName(name)) {
    return false;
  }
  for (let i = 4; i < parts.length; i++) {
    const token: string = parts[i];
    if (truncated && i === parts.length - 1) {
      if (!isSysPartialTail(token)) {
        return false;
      }
      continue;
    }
    const eq: number = token.indexOf('=');
    if (eq <= SYS_EVENT_FIELD_PREFIX.length) {
      return false;
    }
    const key: string = token.substring(0, eq);
    if (!isSysFieldKey(key)) {
      return false;
    }
    if (!isSysSafeText(token.substring(eq + 1), MAX_VALUE_CHARS)) {
      return false;
    }
  }
  return true;
}

// ---------------------------------------------------------------------------
// field keys
// ---------------------------------------------------------------------------

// Keys a caller may attach to a public event. Everything else is dropped.
export const PUBLIC_FIELD_KEYS: string[] = [
  'bytes', 'code', 'count', 'detail', 'from', 'gen', 'iface', 'index', 'kind',
  'level', 'mode', 'ms', 'net', 'ok', 'peer', 'phase', 'proto', 'reason',
  'result', 'role', 'route', 'seq', 'stage', 'state', 'to', 'trigger',
  // 006 native resource snapshot (event `native_resource_sample`): the scalar
  // leaves of connector_resource_status() the shell samples ONCE per 60 s.
  // Names are prefixed by their native section so a flat record stays
  // unambiguous; all values are counts, booleans or the explicit `na`. The
  // set is deliberately closed: an arbitrary extra field of the native
  // document never reaches a record.
  'conn_gen', 'conn_present', 'conn_running', 'conn_starts', 'conn_state',
  'conn_stops', 'dns_pending', 'ice_available', 'ice_fds', 'ice_live',
  'ice_sessions', 'ice_stopped', 'icesock_queued', 'icesock_ready',
  'icesock_taken', 'relay_foreign', 'relay_lanes', 'relay_opens',
  'relay_ready', 'supply_installed', 'supply_pending', 'supply_queued',
  // 2026-10-13 observability increment (device report §C/D/G1): the three
  // facts the shell could not persist before — `purpose` (the socket a supply
  // request asked for: dns/management/signal/relay), `mtu` (the effective
  // tunnel MTU submitted to the platform) and `source` (its provenance:
  // default vs network-map). All three are bounded, non-secret tokens/numbers;
  // the set stays closed.
  'purpose', 'mtu', 'source',
  // 0.0.34 peer-status lag fix (peer-status-lag REPORT §3.5): the two ends of
  // the cross-process status latency — `vpn_status_publish` (the extension's
  // write: the document's stamp, the core's edge revision and the two peer
  // counts the F5 label change moves) and `ui_status_read` (the page's read:
  // the same stamp/revision, the document's age at that instant, and whether
  // the read re-derived). All six keys below are bounded scalars — a monotonic
  // epoch-ms stamp, a small counter, a millisecond age and a boolean — and the
  // set stays closed.
  'updated_at_ms', 'status_revision', 'peers_online', 'peers_relay',
  'age_ms', 'derived',
  // 0.0.54 data-plane watchdog: `vpn_dataplane_stale` records how many SECONDS
  // the "connected but no data plane" run had lasted when the recovery was
  // started (a bounded small integer computed from the watcher's own clocks).
  // A bounded scalar, so the public key set stays closed.
  'age'
];

// Public keys plus the runtime's own keys (memory readings, the every-10th
// mem_detail readings, /proc resource readings, marker provenance, drops).
export const INTERNAL_FIELD_KEYS: string[] = [
  'bytes', 'code', 'count', 'detail', 'from', 'gen', 'iface', 'index', 'kind',
  'level', 'mode', 'ms', 'net', 'ok', 'peer', 'phase', 'proto', 'reason',
  'result', 'role', 'route', 'seq', 'stage', 'state', 'to', 'trigger',
  'dropped', 'heap_kb', 'prev_pid', 'prev_state', 'prev_ts', 'rss_kb',
  'swap_kb', 'sys_avail_kb',
  'cpu_ms', 'threads', 'vmsize_kb', 'rss_anon_kb', 'rss_file_kb',
  'sys_swap_total_kb', 'sys_swap_free_kb', 'fd_count', 'fd_capped',
  // same /proc/self/status read as the four above (event mem_sample)
  'vmdata_kb', 'vmstk_kb', 'rss_shmem_kb',
  // every-10th event mem_detail: async smaps_rollup readings ...
  'pss_kb', 'private_dirty_kb', 'shared_dirty_kb', 'private_clean_kb',
  'shared_clean_kb',
  // ... and the injected Rust-global-allocator counters (Rust heap only)
  'rust_heap_kb', 'rust_heap_peak_kb', 'rust_allocs', 'rust_frees',
  // every-10th event mem_vm: ArkTS VM heap shape, the native (libc malloc) heap
  // and the ArkTS GC cumulative counters — a third record, see the header
  'vm_total_heap_kb', 'vm_array_kb',
  'native_heap_kb', 'native_heap_alloc_kb', 'native_heap_free_kb',
  'gc_count', 'gc_time_ms', 'gc_alloc_kb', 'gc_freed_kb',
  // ... plus the platform's OWN per-process memory limit
  // (hidebug.getAppMemoryLimit(), KiB): on the once-per-start diag_start record
  // AND on every mem_vm record — see buildMemLimitFields
  'rss_limit_kb', 'vss_limit_kb', 'vm_heap_limit_kb'
];

const PUBLIC_FIELD_KEY_SET: Set<string> = new Set<string>(PUBLIC_FIELD_KEYS);
const INTERNAL_FIELD_KEY_SET: Set<string> = new Set<string>(INTERNAL_FIELD_KEYS);

// ---------------------------------------------------------------------------
// /proc sampling (self only; pure parsing)
// ---------------------------------------------------------------------------
//
// The runtime reads exactly three SELF nodes, each in its own guard, each with
// the caller's byte cap and an fd closed in `finally`:
//   /proc/self/status -> Threads, VmSize, RssAnon, RssFile, VmData, VmStk,
//                        RssShmem (ONE read feeds all seven fields)
//   /proc/meminfo     -> SwapTotal, SwapFree
//   /proc/self/fd     -> listFile with listNum = FD_LIST_MAX, COUNT only
// 'self' means no pid is ever passed in, so no other process is addressed and
// no fd name/target may enter a record. An unavailable node (denied, absent,
// unsupported, empty) becomes `na` for its fields — never 0 — and only a
// present reading of a real 0 is written as '0'.

export const PROC_SELF_STATUS: string = '/proc/self/status';
export const PROC_MEMINFO: string = '/proc/meminfo';
export const PROC_SELF_FD: string = '/proc/self/fd';

// The ONLY keys requested from each node; parseProcNumbers ignores every other
// line, key and value in the file. One read of /proc/self/status satisfies the
// whole list (no per-key system call).
export const PROC_STATUS_KEYS: string[] =
  ['Threads', 'VmSize', 'RssAnon', 'RssFile', 'VmData', 'VmStk', 'RssShmem'];
export const PROC_MEMINFO_KEYS: string[] = ['SwapTotal', 'SwapFree'];

function leadingProcInteger(line: string, start: number): number | null {
  let i: number = start;
  while (i < line.length && (line.charAt(i) === ' ' || line.charAt(i) === '\t')) {
    i += 1;
  }
  let digits: string = '';
  while (i < line.length && digits.length < MAX_PROC_DIGITS) {
    const ch: string = line.charAt(i);
    if (ch < '0' || ch > '9') {
      break;
    }
    digits += ch;
    i += 1;
  }
  if (digits.length === 0) {
    return null;
  }
  const value: number = parseInt(digits, 10);
  return Number.isNaN(value) ? null : value;
}

// First occurrence of each requested key, first decimal integer after the ':',
// units and everything else ignored. Bounded four ways: the caller caps the
// text bytes, maxLines caps the scanned lines, MAX_PROC_KEY_CHARS caps the
// accepted key and MAX_PROC_DIGITS caps the integer. A garbled line, a missing
// key or a non-numeric value is simply absent from the map.
export function parseProcNumbers(text: string, keys: string[], maxLines: number): Map<string, number> {
  const found: Map<string, number> = new Map<string, number>();
  if (text.length === 0 || keys.length === 0 || maxLines <= 0) {
    return found;
  }
  const lines: string[] = text.split('\n');
  const limit: number = lines.length < maxLines ? lines.length : maxLines;
  for (let i = 0; i < limit; i++) {
    const line: string = lines[i];
    const colon: number = line.indexOf(':');
    if (colon <= 0 || colon > MAX_PROC_KEY_CHARS) {
      continue;
    }
    const key: string = line.substring(0, colon).trim();
    if (key.length === 0 || keys.indexOf(key) < 0 || found.has(key)) {
      continue;
    }
    const value: number | null = leadingProcInteger(line, colon + 1);
    if (value !== null) {
      found.set(key, value);
    }
  }
  return found;
}

// number -> field text without inventing a reading: an absent map entry is
// null (`na`); a present 0 stays the honest '0'.
export function procNumberText(value: number | undefined): string | null {
  if (value === undefined || !Number.isFinite(value)) {
    return null;
  }
  return String(value);
}

// Raw cumulative CPU time (ms) -> field text. NEVER converted into a rate: the
// platform's value is cumulative since process start and the wall-clock gap
// between ticks is not guaranteed, so a computed rate would be wrong whenever
// a tick is late or the process restarted. Non-finite/negative -> null (`na`).
export function cpuMsText(value: number): string | null {
  if (!Number.isFinite(value) || value < 0) {
    return null;
  }
  return String(value);
}

// Merge `extra` into `base` (extra wins) and return base. Pure so the runtime
// can build ONE mem_sample record out of the memory and resource halves.
export function mergeFields(base: DiagFields, extra: DiagFields): DiagFields {
  const keys: string[] = Object.keys(extra);
  for (let i = 0; i < keys.length; i++) {
    base[keys[i]] = extra[keys[i]];
  }
  return base;
}

// ---------------------------------------------------------------------------
// paths (allowlist)
// ---------------------------------------------------------------------------

export function markerBasename(role: DiagRole): string {
  return role === 'vpn' ? MARKER_BASENAME_VPN : MARKER_BASENAME_UI;
}

export function isAllowlistedBasename(basename: string): boolean {
  return UI_LOG_FILES_BY_AGE.indexOf(basename) >= 0 ||
    basename === MARKER_BASENAME_UI || basename === MARKER_BASENAME_VPN;
}

// filesDir + allowlisted basename only; '' means "refuse" (the runtime then
// skips the operation instead of touching an unknown path).
export function recordPath(filesDir: string, basename: string): string {
  if (filesDir.length === 0 || !isAllowlistedBasename(basename)) {
    return '';
  }
  return filesDir + '/' + basename;
}

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

// A >=32-char run of base64/base64url/hex characters with no '.' or '/'
// separator is treated as an opaque credential (WireGuard keys, setup keys,
// hex secrets). Dotted values (JWTs are caught by their 'eyJ' header, long
// FQDNs stay readable) and values with spaces/colons stay readable unless
// they match a prefix rule below.
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

export function normalizeValue(key: string, value: DiagValue): string {
  if (typeof value === 'number') {
    return Number.isFinite(value) ? String(value) : UNAVAILABLE;
  }
  if (typeof value === 'boolean') {
    return value ? 'true' : 'false';
  }
  const text: string = value as string;
  if (looksSecret(key, text)) {
    return REDACTED;
  }
  return sanitizeToken(text, MAX_VALUE_CHARS);
}

// ---------------------------------------------------------------------------
// records
// ---------------------------------------------------------------------------

// internal=true selects the internal key allowlist (memory/provenance/drops).
export function buildRecord(tsIso: string, pid: number, role: DiagRole, name: string,
  fields: DiagFields | undefined, internal: boolean): string {
  const allowed: Set<string> = internal ? INTERNAL_FIELD_KEY_SET : PUBLIC_FIELD_KEY_SET;
  let line: string = 'ts=' + sanitizeToken(tsIso, 32) + ' pid=' + String(pid) +
    ' role=' + role + ' event=' + sanitizeToken(name, MAX_EVENT_NAME_CHARS);
  if (fields !== undefined) {
    const keys: string[] = Object.keys(fields);
    keys.sort();
    for (let i = 0; i < keys.length; i++) {
      const key: string = keys[i];
      if (!allowed.has(key)) {
        continue;
      }
      line += ' ' + key + '=' + normalizeValue(key, fields[key]);
    }
  }
  line += '\n';
  if (line.length > MAX_LINE_CHARS) {
    line = line.substring(0, MAX_LINE_CHARS - 2) + '~\n';
  }
  return line;
}

// ---------------------------------------------------------------------------
// lifecycle markers
// ---------------------------------------------------------------------------

export interface Marker {
  state: string;
  ts: string;
  pid: number;
  role: string;
}

export function buildMarker(tsIso: string, pid: number, role: DiagRole, state: string): string {
  return 'v=' + String(RECORD_SCHEMA_VERSION) + ' state=' + sanitizeToken(state, 8) +
    ' ts=' + sanitizeToken(tsIso, 32) + ' pid=' + String(pid) +
    ' role=' + sanitizeToken(role, 8) + '\n';
}

// null = missing/unreadable/garbled marker (the runtime treats it exactly like
// "no previous run": observation only, never a claim).
export function parseMarker(text: string): Marker | null {
  if (text.length === 0) {
    return null;
  }
  // Only the first line counts: a garbled/older file must not smuggle a
  // second line's fields into the verdict.
  const firstLine: string = text.split('\n')[0].trim();
  const parts: string[] = firstLine.split(' ');
  let state: string = '';
  let ts: string = '';
  let role: string = '';
  let pidText: string = '';
  for (let i = 0; i < parts.length; i++) {
    const part: string = parts[i];
    const eq: number = part.indexOf('=');
    if (eq <= 0) {
      continue;
    }
    const key: string = part.substring(0, eq);
    const value: string = part.substring(eq + 1);
    if (key === 'state') {
      state = value;
    } else if (key === 'ts') {
      ts = value;
    } else if (key === 'role') {
      role = value;
    } else if (key === 'pid') {
      pidText = value;
    }
  }
  if (state.length === 0 || pidText.length === 0) {
    return null;
  }
  const pid: number = parseInt(pidText, 10);
  if (Number.isNaN(pid)) {
    return null;
  }
  const marker: Marker = { state: state, ts: ts, pid: pid, role: role };
  return marker;
}

// state=open from ANOTHER pid: the previous run of this role never reached
// stop(). Same pid cannot be a prior process (re-init inside a live process),
// so it is not reported.
export function isPriorUnclean(marker: Marker | null, currentPid: number): boolean {
  return marker !== null && marker.state === MARKER_STATE_OPEN && marker.pid !== currentPid;
}

// ---------------------------------------------------------------------------
// rotation
// ---------------------------------------------------------------------------

export interface RotationStep {
  op: string; // 'unlink' | 'rename'
  from: string;
  to: string; // '' for unlink
}

// Exact ordered plan; the runtime executes it with the allowlist guard.
export function rotationSteps(): RotationStep[] {
  const steps: RotationStep[] = [];
  const last: number = UI_LOG_ROTATED.length - 1;
  steps.push({ op: 'unlink', from: UI_LOG_ROTATED[last], to: '' });
  for (let i = last - 1; i >= 0; i--) {
    steps.push({ op: 'rename', from: UI_LOG_ROTATED[i], to: UI_LOG_ROTATED[i + 1] });
  }
  steps.push({ op: 'rename', from: UI_LOG_ACTIVE, to: UI_LOG_ROTATED[0] });
  return steps;
}

// Rotate BEFORE the write that would push the active file past its cap, so
// the total stays <= TOTAL_MAX_BYTES. An empty active file never rotates
// (a single line is far below the cap either way).
export function needsRotation(activeBytes: number, lineChars: number): boolean {
  return activeBytes > 0 && activeBytes + lineChars > PER_FILE_MAX_BYTES;
}

// ---------------------------------------------------------------------------
// memory + resource sample fields (ONE mem_sample record)
// ---------------------------------------------------------------------------

// null -> 'na' (API unsupported / threw). Values are decimal KB strings as
// returned by the platform bigint readings; keep them as strings to avoid a
// lossy bigint -> number conversion.
export function buildSampleFields(rssKb: string | null, swapKb: string | null,
  heapKb: string | null, sysAvailKb: string | null): DiagFields {
  const fields: DiagFields = {};
  fields['rss_kb'] = rssKb === null ? UNAVAILABLE : rssKb;
  fields['swap_kb'] = swapKb === null ? UNAVAILABLE : swapKb;
  fields['heap_kb'] = heapKb === null ? UNAVAILABLE : heapKb;
  fields['sys_avail_kb'] = sysAvailKb === null ? UNAVAILABLE : sysAvailKb;
  return fields;
}

// The 0.0.5 resource half, merged into the same mem_sample record by the
// runtime (mergeFields). null -> 'na' for every reading; `fd_capped` is the one
// boolean and is 'na' when the enumeration itself was unavailable.
// vmdataKb / vmstkKb / rssShmemKb come from the SAME /proc/self/status read as
// threads/vmsize/anon/file (VmData / VmStk / RssShmem) — no extra system call.
export function buildResourceFields(cpuMs: string | null, threads: string | null,
  vmsizeKb: string | null, rssAnonKb: string | null, rssFileKb: string | null,
  vmdataKb: string | null, vmstkKb: string | null, rssShmemKb: string | null,
  sysSwapTotalKb: string | null, sysSwapFreeKb: string | null,
  fdCount: string | null, fdCapped: boolean | null): DiagFields {
  const fields: DiagFields = {};
  fields['cpu_ms'] = cpuMs === null ? UNAVAILABLE : cpuMs;
  fields['threads'] = threads === null ? UNAVAILABLE : threads;
  fields['vmsize_kb'] = vmsizeKb === null ? UNAVAILABLE : vmsizeKb;
  fields['rss_anon_kb'] = rssAnonKb === null ? UNAVAILABLE : rssAnonKb;
  fields['rss_file_kb'] = rssFileKb === null ? UNAVAILABLE : rssFileKb;
  fields['vmdata_kb'] = vmdataKb === null ? UNAVAILABLE : vmdataKb;
  fields['vmstk_kb'] = vmstkKb === null ? UNAVAILABLE : vmstkKb;
  fields['rss_shmem_kb'] = rssShmemKb === null ? UNAVAILABLE : rssShmemKb;
  fields['sys_swap_total_kb'] = sysSwapTotalKb === null ? UNAVAILABLE : sysSwapTotalKb;
  fields['sys_swap_free_kb'] = sysSwapFreeKb === null ? UNAVAILABLE : sysSwapFreeKb;
  fields['fd_count'] = fdCount === null ? UNAVAILABLE : fdCount;
  fields['fd_capped'] = fdCapped === null ? UNAVAILABLE : (fdCapped ? 'true' : 'false');
  return fields;
}

// ---------------------------------------------------------------------------
// every-MEM_DETAIL_INTERVAL mem_detail fields (separate record)
// ---------------------------------------------------------------------------

// true for the Nth, 2Nth, ... mem_sample attempt of a run. The index starts at
// 1 (the initial sample at start()), so the first mem_detail rides the 10th
// attempt. A tick dropped by the single-flight guard never reaches the counter.
export function isDetailSample(index: number): boolean {
  return index > 0 && index % MEM_DETAIL_INTERVAL === 0;
}

// The ASYNC smaps_rollup reading (hidebug.getAppNativeMemInfoAsync, API >= 20).
// Values arrive as the platform's bigint KB readings and are kept as strings
// (no lossy bigint -> number step); null -> 'na' for every field. `pss` is the
// platform's own smaps_rollup Pss+SwapPss, NOT this module's arithmetic.
export function buildNativeMemFields(pssKb: string | null, privateDirtyKb: string | null,
  sharedDirtyKb: string | null, privateCleanKb: string | null,
  sharedCleanKb: string | null): DiagFields {
  const fields: DiagFields = {};
  fields['pss_kb'] = pssKb === null ? UNAVAILABLE : pssKb;
  fields['private_dirty_kb'] = privateDirtyKb === null ? UNAVAILABLE : privateDirtyKb;
  fields['shared_dirty_kb'] = sharedDirtyKb === null ? UNAVAILABLE : sharedDirtyKb;
  fields['private_clean_kb'] = privateCleanKb === null ? UNAVAILABLE : privateCleanKb;
  fields['shared_clean_kb'] = sharedCleanKb === null ? UNAVAILABLE : sharedCleanKb;
  return fields;
}

// ---------------------------------------------------------------------------
// the same detail tick: event=mem_vm (ArkTS VM heap, native heap, GC)
// ---------------------------------------------------------------------------
//
// A third record, right after mem_detail: the attribution readings for the
// anonymous RSS. All legs are SYNCHRONOUS platform calls made inside the SAME
// single-flight runSample() detail branch (see the runtime header) — no timer
// is added and the 1 s cadence does not change. See the core header for why
// these nine keys do not fit on the mem_detail line, and for the three
// memory-limit keys (hidebug.getAppMemoryLimit) that ride this record and the
// once-per-start diag_start record.

export const BYTES_PER_KIB: number = 1024;
// A byte counter is printed by the runtime from a platform bigint; 20 digits is
// the u64 maximum, so anything longer is not a reading of that counter.
export const MAX_BYTE_DIGITS: number = 32;
// A `getAppMemoryLimit()` field is a platform bigint reported in KILOBYTES
// (API 12: rssLimit / vssLimit / vmHeapLimit / vmTotalHeapSize), so it is
// already a KiB value and needs no division — only the same decimal-text
// validation. 20 digits is the u64 maximum, so a longer text is not a reading
// of that field (the same bound the byte counters are pinned at below).
export const MAX_LIMIT_DIGITS: number = 20;

// The getVMRuntimeStat() item names, declared HERE so the runtime never spells
// out a platform string. Units per the API 26 declaration: gc-count and
// fullgc-longtime-count are counts, gc-time is milliseconds, gc-bytes-allocated
// and gc-bytes-freed are BYTES.
export const GC_STAT_GC_COUNT: string = 'ark.gc.gc-count';
export const GC_STAT_GC_TIME: string = 'ark.gc.gc-time';
export const GC_STAT_BYTES_ALLOCATED: string = 'ark.gc.gc-bytes-allocated';
export const GC_STAT_BYTES_FREED: string = 'ark.gc.gc-bytes-freed';

// Decimal byte count -> KiB field text: EXACT floor division by 1024 on the
// digit string the runtime printed, so the platform's bigint never passes
// through a lossy bigint -> number conversion (the same discipline the KB
// sample fields already follow). A missing, empty, over-long or non-decimal
// text is null (`na`); a real sub-KiB value floors to the honest '0'.
export function bytesToKiBText(text: string | null): string | null {
  if (text === null || text.length === 0 || text.length > MAX_BYTE_DIGITS) {
    return null;
  }
  let out: string = '';
  let remainder: number = 0;
  for (let i = 0; i < text.length; i++) {
    const ch: string = text.charAt(i);
    if (ch < '0' || ch > '9') {
      return null;
    }
    const current: number = remainder * 10 + (ch.charCodeAt(0) - 48);
    const digit: number = Math.floor(current / BYTES_PER_KIB);
    remainder = current - digit * BYTES_PER_KIB;
    if (out.length > 0 || digit > 0) {
      out += String(digit);
    }
  }
  return out.length === 0 ? '0' : out;
}

// One getVMRuntimeStat() reply -> field text. The platform answers a plain
// number; a non-finite or negative reply is not a reading and becomes null
// (`na`), never a fabricated 0. The value is NOT scaled for gc_time_ms (the
// platform already reports milliseconds) and the two byte counters are divided
// by the CALLER through bytesToKiBText.
export function statNumberText(value: number): string | null {
  if (!Number.isFinite(value) || value < 0) {
    return null;
  }
  return String(value);
}

// One getAppMemoryLimit() field -> field text. The platform answers a bigint
// ALREADY IN KILOBYTES, so nothing is scaled here (unlike bytesToKiBText): the
// runtime prints the bigint and this validates the decimal text. A missing,
// empty, over-long or non-decimal text (including a negative sign) is null
// (`na`) — a limit is never a fabricated 0.
export function limitKiBText(text: string | null): string | null {
  if (text === null || text.length === 0 || text.length > MAX_LIMIT_DIGITS) {
    return null;
  }
  for (let i = 0; i < text.length; i++) {
    const ch: string = text.charAt(i);
    if (ch < '0' || ch > '9') {
      return null;
    }
  }
  return text;
}

// The platform's OWN per-process memory limit (hidebug.getAppMemoryLimit(),
// API 12 — this is the only reading of it in the whole app, and the number the
// kill investigation had to infer before). rssLimit / vssLimit / vmHeapLimit,
// all in KiB. null -> `na` for that one key, never a fabricated 0.
//
// It rides TWO records: the once-per-process-start diag_start record and every
// mem_vm record of a detail tick (the same values, read once at start and
// re-read per detail tick). vmTotalHeapSize is deliberately not carried: a
// fourth key would push the widest reachable mem_vm past MAX_LINE_CHARS, and
// the three above are the ones the report asks for.
export function buildMemLimitFields(rssLimitKb: string | null, vssLimitKb: string | null,
  vmHeapLimitKb: string | null): DiagFields {
  const fields: DiagFields = {};
  fields['rss_limit_kb'] = rssLimitKb === null ? UNAVAILABLE : rssLimitKb;
  fields['vss_limit_kb'] = vssLimitKb === null ? UNAVAILABLE : vssLimitKb;
  fields['vm_heap_limit_kb'] = vmHeapLimitKb === null ? UNAVAILABLE : vmHeapLimitKb;
  return fields;
}

// The event=mem_vm record of a detail tick. Every argument is already this
// module's field text (byte counters divided down to KiB by bytesToKiBText);
// null -> `na` for that one key, never a fabricated 0. vm_array_kb is the VM's
// array-object footprint (ArkTS arrays / ArrayBuffer-like objects), the
// native_heap_* trio is the libc malloc heap from mallinfo (uordblks+fordblks /
// uordblks / fordblks) and the gc_* quartet is ArkTS's own CUMULATIVE GC
// statistics for this process — none of them is RSS and none overlaps the Rust
// heap or the smaps_rollup readings on the mem_detail line.
export function buildVmMemFields(vmTotalHeapKb: string | null, vmArrayKb: string | null,
  nativeHeapKb: string | null, nativeHeapAllocKb: string | null,
  nativeHeapFreeKb: string | null, gcCount: string | null, gcTimeMs: string | null,
  gcAllocKb: string | null, gcFreedKb: string | null): DiagFields {
  const fields: DiagFields = {};
  fields['vm_total_heap_kb'] = vmTotalHeapKb === null ? UNAVAILABLE : vmTotalHeapKb;
  fields['vm_array_kb'] = vmArrayKb === null ? UNAVAILABLE : vmArrayKb;
  fields['native_heap_kb'] = nativeHeapKb === null ? UNAVAILABLE : nativeHeapKb;
  fields['native_heap_alloc_kb'] = nativeHeapAllocKb === null ? UNAVAILABLE : nativeHeapAllocKb;
  fields['native_heap_free_kb'] = nativeHeapFreeKb === null ? UNAVAILABLE : nativeHeapFreeKb;
  fields['gc_count'] = gcCount === null ? UNAVAILABLE : gcCount;
  fields['gc_time_ms'] = gcTimeMs === null ? UNAVAILABLE : gcTimeMs;
  fields['gc_alloc_kb'] = gcAllocKb === null ? UNAVAILABLE : gcAllocKb;
  fields['gc_freed_kb'] = gcFreedKb === null ? UNAVAILABLE : gcFreedKb;
  return fields;
}

// One allocator counter -> field text. A counter must be a finite, non-negative
// JSON number; anything else (absent, string, null, NaN, negative, Infinity) is
// `na`, never a fabricated 0. `divisor` is 1024 for the two BYTE counters, 1
// for the two event counters.
function allocCounterText(value: Object, divisor: number): string {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) {
    return UNAVAILABLE;
  }
  return String(Math.floor(value / divisor));
}

// The injected Rust-heap reader's document -> the four mem_detail fields. The
// document is the NAPI `alloc_stats()` reply, `{"bytes":n,"peak":n,"allocs":n,
// "frees":n}`. ANY failure (no reader / missing export / unparseable text /
// non-object root / anomalous field) answers all four as `na` — this function
// never throws and never invents a reading. bytes -> KiB by integer division
// (1024); allocs/frees are event counts and stay as they are.
export function buildRustAllocFields(text: string | null): DiagFields {
  const fields: DiagFields = {};
  fields['rust_heap_kb'] = UNAVAILABLE;
  fields['rust_heap_peak_kb'] = UNAVAILABLE;
  fields['rust_allocs'] = UNAVAILABLE;
  fields['rust_frees'] = UNAVAILABLE;
  if (text === null || text.length === 0) {
    return fields;
  }
  let doc: Record<string, Object>;
  try {
    const parsed: Object = JSON.parse(text) as Object;
    if (parsed === null || typeof parsed !== 'object') {
      return fields;
    }
    doc = parsed as Record<string, Object>;
  } catch (error) {
    return fields; // not JSON: the reader's failure is already visible as `na`
  }
  fields['rust_heap_kb'] = allocCounterText(doc['bytes'], 1024);
  fields['rust_heap_peak_kb'] = allocCounterText(doc['peak'], 1024);
  fields['rust_allocs'] = allocCounterText(doc['allocs'], 1);
  fields['rust_frees'] = allocCounterText(doc['frees'], 1);
  return fields;
}

// ---------------------------------------------------------------------------
// bounded queue
// ---------------------------------------------------------------------------

// FIFO with a hard capacity and an honest drop counter: push() returns false
// when the record was dropped, and takeDropped() hands the count to the
// runtime exactly once so it can emit a diag_dropped record.
export class BoundedQueue {
  private lines: string[] = [];
  private droppedCount: number = 0;
  private readonly maxLines: number;

  constructor(maxLines: number) {
    this.maxLines = maxLines > 0 ? maxLines : 1;
  }

  push(line: string): boolean {
    if (this.lines.length >= this.maxLines) {
      this.droppedCount += 1;
      return false;
    }
    this.lines.push(line);
    return true;
  }

  shift(): string | undefined {
    return this.lines.shift();
  }

  size(): number {
    return this.lines.length;
  }

  dropped(): number {
    return this.droppedCount;
  }

  takeDropped(): number {
    const taken: number = this.droppedCount;
    this.droppedCount = 0;
    return taken;
  }
}
