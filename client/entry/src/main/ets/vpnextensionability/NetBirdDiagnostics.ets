/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// NetBirdDiagnostics — the platform-IO half of the standalone diagnostics
// helper. All format/bounds/marker/rotation DECISIONS live in the pure core
// (NetBirdDiagnosticsCore.ets, host-checked by
// client/entry/selfcheck/run-diagnostics.mts); this file only performs IO.
//
// PUBLIC API (for later wiring; nothing imports this yet)
//   start(filesDir: string, role: 'ui' | 'vpn', sink?: (line: string) => void): void
//   event(name: string, fields?: Record<string, string | number | boolean>): boolean
//   appendSystemEvent(line: string): boolean
//     the system-event watcher's own already-projected line (event=sys_*),
//     validated by the core and queued on the SAME bounded writer. Deliberately
//     NOT event('sys_evt', { detail: line }): the 48-char value bound would cut
//     an APP_KILLED reason off the record.
//   stop(): void
//
// DESTINATIONS
//   role 'ui'  -> rotating files under filesDir: netbird-ui.log (+ .1 .2 .3,
//                 1 MiB each, 4 MiB total; see the core header for the exact
//                 order). This module never imports the native bridge.
//   role 'vpn' -> the caller-supplied `sink` line callback (the native
//                 phone_log append path, which owns its own redaction and
//                 persistence). No log file is written for vpn: without a sink
//                 the records have no destination at all, so start() says so
//                 once via hilog (DIAG_SINK_MISSING) while the marker is still
//                 written. The native writer's own family is netbird-vpn.log*
//                 (24 MiB x 4 segments) — this module never writes it.
//   A `sink` may also be supplied for 'ui' (fan-out): the record is written to
//   both destinations in the same serialized order.
//   A `sink` must be cheap and must not block; it runs inside the serialized
//   drain loop, and any exception it throws is counted, never propagated.
//
// BOUNDED ASYNC WRITER
//   event() only enqueues (bounded queue, QUEUE_CAPACITY records) and never
//   does IO on the caller's stack; a single-flight drain loop then performs
//   the async open/write/close and the sink call, so records keep their order
//   and a full queue drops loudly (event=diag_dropped dropped=<n>) instead of
//   growing without bound. A line is <= 512 chars by construction, so the
//   char count is also the byte count.
//
// SAMPLING (own process, cheap reads only)
//   initial sample at start(), then every 1 s, ONE mem_sample record:
//     hidebug.getRssInfo()         -> rss_kb, swap_kb   (/proc/<pid>/status)
//     hidebug.getAppVMMemoryInfo() -> heap_kb           (JS VM heap used)
//     hidebug.getSystemMemInfo()   -> sys_avail_kb      (system MemAvailable)
//   and the 0.0.5 resource readings, chosen for a system-kill investigation
//   (all still cheap and self-scoped; see the core header for the exact units):
//     process.getPastCpuTime()     -> cpu_ms      (RAW cumulative, no IPC)
//     /proc/self/status            -> threads, vmsize_kb, rss_anon_kb,
//                                     rss_file_kb, vmdata_kb, vmstk_kb,
//                                     rss_shmem_kb (ONE bounded async read;
//                                     the three added keys cost no extra call)
//     /proc/meminfo                -> sys_swap_total_kb, sys_swap_free_kb
//                                     (one bounded async read)
//     /proc/self/fd                -> fd_count, fd_capped (COUNT only, bounded
//                                     listFile; no fd name/target is read)
//   Every reading sits in its own try/catch and becomes the explicit token `na`
//   when unavailable; a missing reading is never reported as 0.
//
//   EVERY 10TH SAMPLE — ONE SEPARATE mem_detail record (never a wider
//   mem_sample: the added field set does not fit MAX_LINE_CHARS). In the SAME
//   single-flight runSample(), after the mem_sample fields are collected:
//     hidebug.getAppNativeMemInfoAsync() -> pss_kb, private_dirty_kb,
//                                     shared_dirty_kb, private_clean_kb,
//                                     shared_clean_kb. The ASYNC variant is
//                                     used on purpose — the SDK documents the
//                                     sync getAppNativeMemInfo()/getPss() as
//                                     main-thread-hostile smaps_rollup readers,
//                                     and neither is called here; the awaited
//                                     promise is covered by the SAME
//                                     single-flight guard, so a slow PSS read
//                                     drops ticks instead of stacking them.
//     the caller's optional alloc reader (start()'s 4th argument; the VPN role
//                                     injects a function over the NAPI
//                                     `alloc_stats()` export, the UI role
//                                     injects none) -> rust_heap_kb,
//                                     rust_heap_peak_kb, rust_allocs,
//                                     rust_frees. The reader's reply is PARSED
//                                     BY THE PURE CORE (buildRustAllocFields):
//                                     an absent reader, a missing export or an
//                                     unparseable document renders all four as
//                                     `na`, never as a fabricated 0.
//   A non-detail tick emits NO mem_detail record at all; a detail tick writes
//   `na` for whatever could not be read and says the FIRST failure of that kind
//   once per run (DIAG_DETAIL_UNAVAILABLE), never per tick.
//
//   THE SAME DETAIL TICK — ONE MORE SEPARATE RECORD, event=mem_vm, immediately
//   after mem_detail (the attribution half: "whose memory is the anonymous
//   RSS?"). Its readings are SYNCHRONOUS calls, taken in the same single-flight
//   detail branch, so no timer is added and the 1 s cadence is untouched:
//     hidebug.getAppVMMemoryInfo()          -> vm_total_heap_kb, vm_array_kb (KB)
//     hidebug.getNativeHeapSize()           -> native_heap_kb       (bytes ->
//     hidebug.getNativeHeapAllocatedSize()  -> native_heap_alloc_kb  KiB by exact
//     hidebug.getNativeHeapFreeSize()       -> native_heap_free_kb   integer
//                                             division; mallinfo: uordblks +
//                                             fordblks / uordblks / fordblks —
//                                             the libc malloc heap ONLY)
//     hidebug.getVMRuntimeStat(GC_STAT_*)   -> gc_count, gc_time_ms (ms as
//                                             reported), gc_alloc_kb, gc_freed_kb
//                                             (the two gc-bytes-* counters are
//                                             BYTES -> KiB by exact integer
//                                             division). The item names live in
//                                             the pure core, never here.
//   Why a third line: at every field's own bound, mem_detail plus these nine
//   keys is ~695 chars (pid=42), past MAX_LINE_CHARS; splitting (never
//   truncating) keeps every reading, and the widest mem_vm alone is 381 chars.
//   Each leg is guarded separately: a missing/throwing/non-finite API turns its
//   own keys into `na` and says the run's FIRST detail failure once.
//
//   SINGLE-FLIGHT + GENERATION: at most one sample is in flight. A 1 s tick
//   that fires while the previous sample is still reading /proc (or while a
//   detail tick's PSS promise is still pending) is DROPPED, never queued, so a
//   hanging read can neither stack timers nor accumulate pending promises, and
//   a detail reading can never land in a later tick: it is awaited inside the
//   one runSample() whose mem_sample it belongs to. A run generation is bumped
//   by start() and stop(), so a reading that settles after a stop (or after a
//   later start) is discarded instead of being emitted into a run it does not
//   belong to. The fd of every /proc read is closed in `finally`; each read goes
//   into that node's ONE reused 16 KiB buffer (PROC_READ_MAX_BYTES) and is
//   decoded only up to the bytes actually read.
//
// LIFECYCLE MARKER (observation only)
//   start() writes <filesDir>/netbird-diag-<role>.marker with state=open,
//   after reading the previous marker; an open marker from a different pid is
//   reported once as event=PRIOR_EXIT_UNCLEAN (prev_pid/prev_ts/prev_state).
//   That is an observation that the previous run did not reach stop() — NOT
//   proof of a crash (force-stop and OOM kill look identical to a lost stop
//   callback). stop() writes state=clean, and callers must call it ONLY from
//   the real process/extension lifecycle stop callback (UIAbility.onDestroy,
//   VpnExtensionAbility.onDestroy) — never from a page's aboutToDisappear /
//   onPageHide, which fires while the process is still alive and would make
//   the next start() report a clean run for a process that never stopped.
//   Marker reads/writes are serialized on their own promise chain, so a
//   start()/stop() pair cannot interleave. A small file read/rewrite (one
//   line) is the only file work outside the queue.
//
// REDACTION / SCOPE
//   Only whitelisted field keys are emitted and values are sanitized +
//   redacted by the core; arbitrary raw UI input never becomes a record.
//   Files this module touches are fixed and few: its own log/marker files, plus
//   the three SELF-only proc nodes above for the numeric sample. No config, no
//   setup key, no credential, no status document; no pid is ever passed to a
//   proc path, and /proc/self/fd is read as a COUNT only — no fd name, target
//   or inode of any process enters a record. A node that is denied/absent is
//   written as `na` per sample and its unavailability is said once per run
//   (DIAG_PROC_UNAVAILABLE), never as a per-tick errno log.
//   Every public entry point and every IO step is wrapped: an internal
//   failure increments a counter and logs at most MAX_INTERNAL_ERROR_LOGS
//   hilog warnings — it can never throw into, or crash, the application.

import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
import hidebug from '@ohos.hidebug';
import process from '@ohos.process';
import type { ListFileOptions } from '@ohos.file.fs';
import { BoundedQueue, buildMarker, buildNativeMemFields, buildRecord,
  buildResourceFields, buildRustAllocFields, buildSampleFields, buildVmMemFields,
  bytesToKiBText, cpuMsText, isDetailSample, isPriorUnclean, isPublicEventName,
  isSystemEventLine, markerBasename, mergeFields, needsRotation, parseMarker,
  parseProcNumbers, procNumberText, recordPath, rotationSteps, sampleIntervalFor,
  statNumberText, FD_LIST_MAX, GC_STAT_BYTES_ALLOCATED, GC_STAT_BYTES_FREED,
  GC_STAT_GC_COUNT, GC_STAT_GC_TIME, MARKER_STATE_CLEAN, MARKER_STATE_OPEN,
  PROC_MAX_LINES, PROC_MEMINFO, PROC_MEMINFO_KEYS, PROC_READ_MAX_BYTES,
  PROC_SELF_FD, PROC_SELF_STATUS, PROC_STATUS_KEYS, QUEUE_CAPACITY, UI_LOG_ACTIVE }
  from './NetBirdDiagnosticsCore';
import type { Marker, RotationStep } from './NetBirdDiagnosticsCore';

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';
const MAX_INTERNAL_ERROR_LOGS: number = 8;

// Platform BusinessError as far as this module reads it (one numeric field).
interface BusinessFailure extends Error {
  code?: number;
}

export type DiagRole = 'ui' | 'vpn';
export type DiagValue = string | number | boolean;
export type DiagFields = Record<string, DiagValue>;
export type DiagSink = (line: string) => void;
// Optional per-process Rust-heap reader injected at start() (see the SAMPLING
// header): returns the NAPI `alloc_stats()` JSON document, or undefined when
// the export/document is unavailable. It must be cheap and synchronous — it is
// called once per detail tick inside the single-flight sample — and it must
// never throw; the caller's wrapper already answers undefined instead.
export type DiagAllocReader = () => string | undefined;

function nowIso(): string {
  return new Date().toISOString();
}

// Reused read buffers for the two self-only /proc nodes: ONE dedicated buffer
// PER NODE, never one shared (see readProcText). The read is the cap — fs.read
// fills at most PROC_READ_MAX_BYTES and only the bytes actually read are
// decoded — so a tick allocates no buffer at all.
//
// WHY NOT ONE SHARED BUFFER: a /proc read can outlive the run that started it.
// stop() bumps the generation and returns while a read is still in flight, and
// the next start() samples immediately, so a stale read of one node and a fresh
// read of the other can be in flight at the same time (the generation overlap
// the selfcheck drives). With one shared buffer the fresher sample would decode
// the other node's bytes and report `na` for keys that were in fact readable;
// with one buffer per node a reading can only be overwritten by another reading
// of the SAME node — same shape, same keys — and its result is discarded by the
// generation guard unless it belongs to the current run.
const STATUS_READ_BUFFER: ArrayBuffer = new ArrayBuffer(PROC_READ_MAX_BYTES);
const MEMINFO_READ_BUFFER: ArrayBuffer = new ArrayBuffer(PROC_READ_MAX_BYTES);

// /proc content is ASCII, so the bytes actually read are decoded directly —
// no extra framework module for one fixed-size buffer. Only parseProcNumbers()
// consumes this, so a mangled non-ASCII byte can at worst fail a key/digit
// match; nothing from the buffer ever reaches a record verbatim.
//
// The bytes are decoded in fixed-size blocks through the multi-argument
// String.fromCharCode — the call shape the account core already compiles with —
// so it is one call per 16 bytes instead of one per byte. Byte i still maps to
// charCode i: 0x80..0xff stay single U+0080..U+00FF chars (never a UTF-8
// replacement char) and the text is byte-identical to the per-byte loop this
// replaces. Deliberately no spread/apply argument (whose ArkTS acceptance is
// unverified here) and no TextDecoder/@ohos.util import (the selfcheck forbids
// this module from pulling one in).
function decodeAscii(buffer: ArrayBuffer, length: number): string {
  const block: number = 16;
  const bytes: Uint8Array = new Uint8Array(buffer, 0, length);
  let text: string = '';
  let i: number = 0;
  while (i + block <= length) {
    text += String.fromCharCode(bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3],
      bytes[i + 4], bytes[i + 5], bytes[i + 6], bytes[i + 7], bytes[i + 8],
      bytes[i + 9], bytes[i + 10], bytes[i + 11], bytes[i + 12], bytes[i + 13],
      bytes[i + 14], bytes[i + 15]);
    i += block;
  }
  while (i < length) {
    text += String.fromCharCode(bytes[i]);
    i += 1;
  }
  return text;
}

class DiagRuntime {
  private filesDir: string = '';
  private role: DiagRole = 'ui';
  private sink: DiagSink | undefined = undefined;
  // Caller-injected Rust-heap reader (see DiagAllocReader). Undefined for a
  // role that cannot see the native connector (the UI process): its every-10th
  // mem_detail then writes `na` for the four rust keys.
  private allocReader: DiagAllocReader | undefined = undefined;
  private running: boolean = false;
  private starting: boolean = false;
  private stopping: boolean = false;
  private ready: boolean = false;
  private queue: BoundedQueue = new BoundedQueue(QUEUE_CAPACITY);
  private draining: boolean = false;
  private drainPromise: Promise<void> | null = null;
  private markerChain: Promise<void> = Promise.resolve();
  private markerPath: string = '';
  private activeLogPath: string = '';
  private activeBytes: number = 0;
  private sampleTimer: number = -1;
  // Sampling state: at most one sample in flight, and a generation that start()
  // and stop() bump so a delayed reading is never reused by a later run.
  // sampleIndex counts mem_sample ATTEMPTS in the CURRENT run (reset by
  // start()); the every-MEM_DETAIL_INTERVAL detail tick is derived from it.
  // The tick PERIOD comes from sampleIntervalFor(role) (ui 10 s, vpn 1 s); the
  // detail cadence stays per tick, so ui details every 100 s, vpn every 10 s.
  private sampling: boolean = false;
  private generation: number = 0;
  private sampleIndex: number = 0;
  private procUnavailableLogged: boolean = false;
  private detailUnavailableLogged: boolean = false;
  private internalErrors: number = 0;
  private rejectedEvents: number = 0;

  // ---- public API ---------------------------------------------------------

  start(filesDir: string, role: DiagRole, sink: DiagSink | undefined,
    allocReader: DiagAllocReader | undefined): void {
    try {
      if (this.running || this.starting || this.stopping) {
        return; // idempotent: a second start() in the same process is ignored
      }
      if (filesDir.length === 0) {
        hilog.warn(DOMAIN, TAG, 'DIAG_START_REJECTED|reason=empty-filesdir');
        return;
      }
      if (role !== 'ui' && role !== 'vpn') {
        hilog.warn(DOMAIN, TAG, 'DIAG_START_REJECTED|reason=role');
        return;
      }
      this.filesDir = filesDir;
      this.role = role;
      this.sink = sink;
      this.allocReader = allocReader;
      if (role === 'vpn' && sink === undefined) {
        // No file writer for vpn: without the caller's phone_log sink the
        // records have no destination. The marker is still written, but say
        // this out loud once instead of silently consuming records.
        hilog.warn(DOMAIN, TAG, 'DIAG_SINK_MISSING|role=vpn|note=records-have-no-destination');
      }
      this.queue = new BoundedQueue(QUEUE_CAPACITY);
      this.activeBytes = 0;
      this.internalErrors = 0;
      this.rejectedEvents = 0;
      // New run: invalidate any sample still in flight from a previous run and
      // let this run sample again. The detail cadence restarts with the run.
      this.generation += 1;
      this.sampling = false;
      this.sampleIndex = 0;
      this.procUnavailableLogged = false;
      this.detailUnavailableLogged = false;
      this.running = true;
      this.starting = true;
      this.stopping = false;
      this.ready = false;
      this.markerPath = recordPath(filesDir, markerBasename(role));
      this.activeLogPath = role === 'ui' ? recordPath(filesDir, UI_LOG_ACTIVE) : '';
      this.chain((): Promise<void> => this.initialiseRun());
    } catch (error) {
      this.noteError('start', error as Error);
    }
  }

  // false = the record was not accepted (not running, unsafe name, or the
  // bounded queue was full). Callers may ignore the result.
  event(name: string, fields: DiagFields | undefined): boolean {
    try {
      if (!this.running) {
        return false;
      }
      if (!isPublicEventName(name)) {
        this.rejectedEvents += 1;
        hilog.warn(DOMAIN, TAG, 'DIAG_EVENT_REJECTED|reason=name|n=%{public}d', this.rejectedEvents);
        return false;
      }
      const line: string = buildRecord(nowIso(), process.pid, this.role, name, fields, false);
      const accepted: boolean = this.queue.push(line);
      if (!accepted) {
        this.rejectedEvents += 1;
      }
      this.pump();
      return accepted;
    } catch (error) {
      this.noteError('event', error as Error);
      return false;
    }
  }

  // The system-event watcher's sink body. The line was already projected,
  // bounded and redacted by NetBirdSystemEventsCore; here it is validated
  // against this writer's own sys_ ingress rule (core.isSystemEventLine) and
  // pushed onto the SAME bounded queue as the caller's records, so ordering,
  // rotation, the drop report and stop()'s flush all behave identically. Cheap
  // by construction: no IO on this stack, and every failure is swallowed.
  appendSystemEvent(line: string): boolean {
    try {
      if (!this.running) {
        return false;
      }
      if (!isSystemEventLine(line)) {
        this.rejectedEvents += 1;
        hilog.warn(DOMAIN, TAG, 'DIAG_SYS_EVENT_REJECTED|reason=line|n=%{public}d',
          this.rejectedEvents);
        return false;
      }
      const accepted: boolean = this.queue.push(line);
      if (!accepted) {
        this.rejectedEvents += 1;
      }
      this.pump();
      return accepted;
    } catch (error) {
      this.noteError('sys-event', error as Error);
      return false;
    }
  }

  // Marks the run clean. ONLY the real lifecycle stop callback may call this.
  stop(): void {
    try {
      if ((!this.running && !this.starting) || this.stopping) {
        return;
      }
      this.stopping = true;
      if (this.sampleTimer >= 0) {
        clearInterval(this.sampleTimer);
        this.sampleTimer = -1;
      }
      // A reading still in flight belongs to the run that is ending: bump the
      // generation so its result is discarded instead of emitted after stop.
      this.generation += 1;
      this.sampling = false;
      this.emitInternal('diag_stop', {});
      this.running = false; // no new caller records; the queue still drains
      this.chain((): Promise<void> => this.finishRun());
    } catch (error) {
      this.noteError('stop', error as Error);
      this.starting = false;
    }
  }

  // ---- run lifecycle (serialized on markerChain) --------------------------

  private chain(task: () => Promise<void>): void {
    this.markerChain = this.markerChain.then((): Promise<void> => task())
      .catch((error: Error): void => { this.noteError('chain', error); });
  }

  private async initialiseRun(): Promise<void> {
    try {
      const pid: number = process.pid;
      const prevText: string = await this.readTextSafe(this.markerPath);
      const prev: Marker | null = parseMarker(prevText);
      const priorUnclean: boolean = isPriorUnclean(prev, pid);
      await this.writeTextSafe(this.markerPath,
        buildMarker(nowIso(), pid, this.role, MARKER_STATE_OPEN));
      if (priorUnclean && prev !== null) {
        const fields: DiagFields = {};
        fields['prev_pid'] = prev.pid;
        fields['prev_ts'] = prev.ts;
        fields['prev_state'] = prev.state;
        this.emitInternal('PRIOR_EXIT_UNCLEAN', fields);
      }
      this.emitInternal('diag_start', {});
      if (this.role === 'ui' && this.activeLogPath.length > 0) {
        // Seed the byte counter BEFORE the first write, so rotation accounts
        // for whatever a previous run left in the active file.
        this.activeBytes = await this.sizeSafe(this.activeLogPath);
      }
    } finally {
      // Records enqueued by the caller before this point are still in the
      // queue (FIFO) and are written now, in call order, after the seed.
      this.ready = true;
      this.starting = false;
      this.sample(); // initial sample, then one timer at sampleIntervalFor(role)
      this.pump();
      this.sampleTimer = setInterval((): void => { this.sample(); },
        sampleIntervalFor(this.role));
    }
  }

  private async finishRun(): Promise<void> {
    try {
      await this.flush();
      await this.writeTextSafe(this.markerPath,
        buildMarker(nowIso(), process.pid, this.role, MARKER_STATE_CLEAN));
    } finally {
      this.starting = false;
      this.stopping = false;
    }
  }

  // Wait until everything already enqueued has been written. Bounded on
  // purpose: after stop() no new caller records appear, so this settles in at
  // most a couple of passes.
  private async flush(): Promise<void> {
    for (let i = 0; i < 16; i++) {
      this.pump();
      const pending: Promise<void> | null = this.drainPromise;
      if (pending !== null) {
        await pending;
      }
      if (this.queue.size() === 0 && this.drainPromise === null) {
        return;
      }
    }
  }

  // ---- single-flight drain loop -------------------------------------------

  private pump(): void {
    if (this.draining || !this.ready) {
      // Not initialised yet: initialiseRun() pumps once the marker is written
      // and the active-file byte counter is seeded.
      return;
    }
    this.draining = true;
    this.drainPromise = this.drainLoop().then((): void => {
      this.draining = false;
      this.drainPromise = null;
      if (this.queue.size() > 0) {
        this.pump();
      }
    }).catch((error: Error): void => {
      this.draining = false;
      this.drainPromise = null;
      this.noteError('drain', error);
    });
  }

  private async drainLoop(): Promise<void> {
    while (true) {
      const line: string | undefined = this.queue.shift();
      if (line === undefined) {
        const dropped: number = this.queue.takeDropped();
        if (dropped > 0) {
          const fields: DiagFields = {};
          fields['dropped'] = dropped;
          await this.writeOne(buildRecord(nowIso(), process.pid, this.role, 'diag_dropped',
            fields, true));
          continue; // records may have arrived while the report was written
        }
        return;
      }
      await this.writeOne(line);
    }
  }

  private async writeOne(line: string): Promise<void> {
    if (this.role === 'ui' && this.activeLogPath.length > 0) {
      await this.appendLine(line);
    }
    const sink: DiagSink | undefined = this.sink;
    if (sink !== undefined) {
      try {
        sink(line);
      } catch (error) {
        this.noteError('sink', error as Error);
      }
    }
  }

  private emitInternal(name: string, fields: DiagFields): void {
    const line: string = buildRecord(nowIso(), process.pid, this.role, name, fields, true);
    this.queue.push(line);
    this.pump();
  }

  // ---- rotating UI file ---------------------------------------------------

  private async appendLine(line: string): Promise<void> {
    const path: string = this.activeLogPath;
    if (path.length === 0) {
      return;
    }
    try {
      if (needsRotation(this.activeBytes, line.length)) {
        await this.rotate();
      }
      const file: fs.File = await fs.open(path,
        fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.APPEND);
      try {
        const written: number = await fs.write(file.fd, line);
        this.activeBytes += written;
      } finally {
        await fs.close(file);
      }
    } catch (error) {
      this.noteError('append', error as Error);
    }
  }

  private async rotate(): Promise<void> {
    const steps: RotationStep[] = rotationSteps();
    for (let i = 0; i < steps.length; i++) {
      const step: RotationStep = steps[i];
      const from: string = recordPath(this.filesDir, step.from);
      if (from.length === 0) {
        continue; // allowlist guard: never touch a path outside the fixed set
      }
      let to: string = '';
      if (step.op !== 'unlink') {
        to = recordPath(this.filesDir, step.to);
        if (to.length === 0) {
          continue;
        }
      }
      try {
        if (!(await fs.access(from))) {
          continue; // absent source (first rotation): nothing to drop or move
        }
        if (step.op === 'unlink') {
          await fs.unlink(from);
        } else {
          await fs.rename(from, to);
        }
      } catch (error) {
        this.noteError('rotate', error as Error);
      }
    }
    this.activeBytes = 0;
  }

  // ---- sampling (single-flight, generation-guarded) -----------------------
  //
  // One sample in flight per run. A tick that arrives while the previous sample
  // is still reading /proc — or while a detail tick's PSS promise is pending —
  // is DROPPED, never queued, so a hanging read can neither stack timers nor
  // accumulate pending promises. runSample() clears the flag only when the
  // generation still matches: a result that settles after stop() (or after a
  // later start()) is discarded instead of being emitted into a run it does not
  // belong to. The detail decision is taken HERE, once per accepted tick, and
  // travels with the sample, so a late detail reading can never be attached to
  // a later tick.

  private sample(): void {
    if (!this.running || this.sampling) {
      return;
    }
    this.sampling = true;
    this.sampleIndex += 1;
    const detail: boolean = isDetailSample(this.sampleIndex);
    const generation: number = this.generation;
    this.runSample(generation, detail).catch((error: Error): void => {
      if (generation === this.generation) {
        this.noteError('sample', error);
      }
    });
  }

  private async runSample(generation: number, detail: boolean): Promise<void> {
    try {
      const fields: DiagFields = await this.collectSample();
      // The detail readings are awaited INSIDE this one runSample: the same
      // single-flight guard covers the PSS promise, and the tick's records are
      // enqueued together (mem_sample first, then mem_detail, then mem_vm), so a
      // failure or a delay can neither stack ticks nor mis-attribute a reading.
      let detailFields: DiagFields | null = null;
      let vmFields: DiagFields | null = null;
      if (detail) {
        detailFields = await this.collectDetail();
        // Synchronous attribution readings, inside the same single-flight
        // window: no timer is added and no await can let a tick interleave.
        vmFields = this.collectVmMem();
      }
      if (generation === this.generation && this.running) {
        this.emitInternal('mem_sample', fields);
        if (detailFields !== null) {
          this.emitInternal('mem_detail', detailFields);
        }
        // Same tick, immediately after mem_detail (see the header): the
        // ArkTS-VM / native-malloc / GC attribution half.
        if (vmFields !== null) {
          this.emitInternal('mem_vm', vmFields);
        }
      }
    } finally {
      if (generation === this.generation) {
        this.sampling = false;
      }
    }
  }

  private async collectSample(): Promise<DiagFields> {
    // Cheap platform readings first (no IPC): each in its own guard.
    let rssKb: string | null = null;
    let swapKb: string | null = null;
    try {
      const info: hidebug.RssInfo = hidebug.getRssInfo();
      rssKb = info.rss.toString();
      swapKb = info.swapRss.toString();
    } catch (error) {
      rssKb = null; // unsupported on this build/device -> explicit `na`
      swapKb = null;
    }
    let heapKb: string | null = null;
    try {
      const vm: hidebug.VMMemoryInfo = hidebug.getAppVMMemoryInfo();
      heapKb = vm.heapUsed.toString();
    } catch (error) {
      heapKb = null;
    }
    let sysAvailKb: string | null = null;
    try {
      const sys: hidebug.SystemMemInfo = hidebug.getSystemMemInfo();
      sysAvailKb = sys.availableMem.toString();
    } catch (error) {
      sysAvailKb = null;
    }
    // Raw cumulative CPU ms of THIS process; never divided by wall clock (see
    // cpuMsText in the core).
    let cpuMs: string | null = null;
    try {
      cpuMs = cpuMsText(process.getPastCpuTime());
    } catch (error) {
      cpuMs = null;
    }

    // /proc/self/status: thread count and the anon/file split of this process,
    // plus the VmData/VmStk/RssShmem readings — ONE read feeds all seven
    // fields, so the three added keys cost no extra open/read/close.
    let threads: string | null = null;
    let vmsizeKb: string | null = null;
    let rssAnonKb: string | null = null;
    let rssFileKb: string | null = null;
    let vmdataKb: string | null = null;
    let vmstkKb: string | null = null;
    let rssShmemKb: string | null = null;
    const statusText: string | null =
      await this.readProcText(PROC_SELF_STATUS, 'status', STATUS_READ_BUFFER);
    if (statusText !== null) {
      const values: Map<string, number> =
        parseProcNumbers(statusText, PROC_STATUS_KEYS, PROC_MAX_LINES);
      threads = procNumberText(values.get('Threads'));
      vmsizeKb = procNumberText(values.get('VmSize'));
      rssAnonKb = procNumberText(values.get('RssAnon'));
      rssFileKb = procNumberText(values.get('RssFile'));
      vmdataKb = procNumberText(values.get('VmData'));
      vmstkKb = procNumberText(values.get('VmStk'));
      rssShmemKb = procNumberText(values.get('RssShmem'));
    }

    // /proc/meminfo: system swap only. MemAvailable stays with hidebug above,
    // so sys_avail_kb is not duplicated from this file.
    let sysSwapTotalKb: string | null = null;
    let sysSwapFreeKb: string | null = null;
    const meminfoText: string | null =
      await this.readProcText(PROC_MEMINFO, 'meminfo', MEMINFO_READ_BUFFER);
    if (meminfoText !== null) {
      const values: Map<string, number> =
        parseProcNumbers(meminfoText, PROC_MEMINFO_KEYS, PROC_MAX_LINES);
      sysSwapTotalKb = procNumberText(values.get('SwapTotal'));
      sysSwapFreeKb = procNumberText(values.get('SwapFree'));
    }

    // /proc/self/fd: COUNT only, bounded by listNum. A full list cannot be told
    // apart from a truncated one, so it is flagged as a lower bound.
    let fdCount: string | null = null;
    let fdCapped: boolean | null = null;
    try {
      const options: ListFileOptions = { listNum: FD_LIST_MAX };
      const names: string[] = await fs.listFile(PROC_SELF_FD, options);
      fdCount = String(names.length);
      fdCapped = names.length >= FD_LIST_MAX;
    } catch (error) {
      fdCount = null; // denied/unsupported on this build -> explicit `na`
      fdCapped = null;
      this.noteProcUnavailable('fd');
    }

    return mergeFields(buildSampleFields(rssKb, swapKb, heapKb, sysAvailKb),
      buildResourceFields(cpuMs, threads, vmsizeKb, rssAnonKb, rssFileKb,
        vmdataKb, vmstkKb, rssShmemKb, sysSwapTotalKb, sysSwapFreeKb, fdCount, fdCapped));
  }

  // ---- every-10th detail readings (ONE separate mem_detail record) --------
  //
  // Runs INSIDE runSample's single-flight window (see there): the awaited PSS
  // promise is covered by the same guard, so a pending/slow smaps read drops
  // ticks instead of stacking them, and the fields are attached to the exact
  // tick that asked for them. Every leg is guarded: a failure becomes `na` and
  // is said out loud ONCE per run (DIAG_DETAIL_UNAVAILABLE), never per tick.
  private async collectDetail(): Promise<DiagFields> {
    // The SDK's ASYNC smaps_rollup reader (API >= 20): the sync
    // getAppNativeMemInfo()/getPss() variants are documented as unsuitable for
    // the main thread and are deliberately not called anywhere in this module.
    let pssKb: string | null = null;
    let privateDirtyKb: string | null = null;
    let sharedDirtyKb: string | null = null;
    let privateCleanKb: string | null = null;
    let sharedCleanKb: string | null = null;
    try {
      const info: hidebug.NativeMemInfo = await hidebug.getAppNativeMemInfoAsync();
      pssKb = info.pss.toString();
      privateDirtyKb = info.privateDirty.toString();
      sharedDirtyKb = info.sharedDirty.toString();
      privateCleanKb = info.privateClean.toString();
      sharedCleanKb = info.sharedClean.toString();
    } catch (error) {
      pssKb = null; // unsupported/denied on this build -> explicit `na`
      privateDirtyKb = null;
      sharedDirtyKb = null;
      privateCleanKb = null;
      sharedCleanKb = null;
      this.noteDetailUnavailable('native-mem');
    }

    // The caller's Rust-allocator reader: absent for a role that cannot see the
    // native connector (UI). A missing reader is NOT a failure — the four keys
    // simply stay `na`; a reader that throws or answers undefined IS reported
    // once (it means the injected surface is broken, not that the role has
    // none). The reply is parsed by the pure core, which never throws.
    let allocText: string | null = null;
    const reader: DiagAllocReader | undefined = this.allocReader;
    if (reader !== undefined) {
      try {
        const text: string | undefined = reader();
        if (text === undefined) {
          this.noteDetailUnavailable('rust-alloc');
        } else {
          allocText = text;
        }
      } catch (error) {
        allocText = null;
        this.noteDetailUnavailable('rust-alloc');
      }
    }

    return mergeFields(
      buildNativeMemFields(pssKb, privateDirtyKb, sharedDirtyKb, privateCleanKb, sharedCleanKb),
      buildRustAllocFields(allocText));
  }

  // ---- the same tick's event=mem_vm readings (SYNCHRONOUS) -----------------
  //
  // Called from runSample's detail branch right after collectDetail(): three
  // SYNCHRONOUS legs, each in its own guard, all inside the one single-flight
  // window (no timer, no await, so a tick cannot interleave). A missing or
  // throwing API — or a non-finite/negative stat — turns only that leg's keys
  // into `na`, and the run's FIRST detail failure is said once
  // (DIAG_DETAIL_UNAVAILABLE, stage label only). The byte counters reach the
  // pure core as the decimal text of the platform reading, which divides them
  // down to KiB exactly (bytesToKiBText), never through a lossy number step.
  private collectVmMem(): DiagFields {
    let vmTotalHeapKb: string | null = null;
    let vmArrayKb: string | null = null;
    try {
      const vm: hidebug.VMMemoryInfo = hidebug.getAppVMMemoryInfo();
      vmTotalHeapKb = vm.totalHeap.toString();
      vmArrayKb = vm.allArraySize.toString();
    } catch (error) {
      vmTotalHeapKb = null; // unsupported on this build -> explicit `na`
      vmArrayKb = null;
      this.noteDetailUnavailable('vm-heap');
    }

    // The libc malloc heap (mallinfo), in BYTES: uordblks + fordblks / uordblks
    // / fordblks. This is NOT the ArkTS heap and NOT process RSS.
    let nativeHeapKb: string | null = null;
    let nativeHeapAllocKb: string | null = null;
    let nativeHeapFreeKb: string | null = null;
    try {
      nativeHeapKb = bytesToKiBText(hidebug.getNativeHeapSize().toString());
      nativeHeapAllocKb = bytesToKiBText(hidebug.getNativeHeapAllocatedSize().toString());
      nativeHeapFreeKb = bytesToKiBText(hidebug.getNativeHeapFreeSize().toString());
    } catch (error) {
      nativeHeapKb = null;
      nativeHeapAllocKb = null;
      nativeHeapFreeKb = null;
      this.noteDetailUnavailable('native-heap');
    }

    // ArkTS GC cumulative statistics. gc-time is already milliseconds; the two
    // gc-bytes-* items are BYTE counters, divided down to KiB by the core. The
    // item names are the core's GC_STAT_* constants, never a literal here.
    let gcCount: string | null = null;
    let gcTimeMs: string | null = null;
    let gcAllocKb: string | null = null;
    let gcFreedKb: string | null = null;
    try {
      gcCount = statNumberText(hidebug.getVMRuntimeStat(GC_STAT_GC_COUNT));
      gcTimeMs = statNumberText(hidebug.getVMRuntimeStat(GC_STAT_GC_TIME));
      gcAllocKb = bytesToKiBText(hidebug.getVMRuntimeStat(GC_STAT_BYTES_ALLOCATED).toString());
      gcFreedKb = bytesToKiBText(hidebug.getVMRuntimeStat(GC_STAT_BYTES_FREED).toString());
    } catch (error) {
      gcCount = null;
      gcTimeMs = null;
      gcAllocKb = null;
      gcFreedKb = null;
      this.noteDetailUnavailable('gc-stat');
    }

    return buildVmMemFields(vmTotalHeapKb, vmArrayKb, nativeHeapKb, nativeHeapAllocKb,
      nativeHeapFreeKb, gcCount, gcTimeMs, gcAllocKb, gcFreedKb);
  }

  // ONE concise line per run when a detail reading fails: the native PSS leg,
  // the injected Rust reader, or one of the three synchronous mem_vm legs
  // (vm-heap / native-heap / gc-stat). The per-tick `na` fields carry the ongoing
  // signal; this must never become a per-tick errno log, so only the first
  // failure of the run is said out loud (and `stage` is a label — no pid, no
  // path, no platform item name).
  private noteDetailUnavailable(stage: string): void {
    if (this.detailUnavailableLogged) {
      return;
    }
    this.detailUnavailableLogged = true;
    try {
      hilog.warn(DOMAIN, TAG, 'DIAG_DETAIL_UNAVAILABLE|stage=%{public}s|status=na', stage);
    } catch (error) {
      // hilog itself failing is not actionable here.
    }
  }

  // Bounded async read of ONE self-only /proc node into that node's OWN reused
  // buffer (STATUS_READ_BUFFER / MEMINFO_READ_BUFFER — see there for why one per
  // node, never one shared). The fixed buffer IS the byte cap (the node is never
  // slurped into a growing string); the fd is always closed in `finally`; every
  // failure (permission, absence, empty, malformed) returns null so the caller
  // writes `na`. Availability is reported once per run, never once per sample.
  private async readProcText(path: string, stage: string, buffer: ArrayBuffer):
    Promise<string | null> {
    let file: fs.File | null = null;
    try {
      file = await fs.open(path, fs.OpenMode.READ_ONLY);
      const length: number = await fs.read(file.fd, buffer);
      if (length <= 0) {
        this.noteProcUnavailable(stage);
        return null; // an empty reading is not a reading
      }
      const take: number = length < PROC_READ_MAX_BYTES ? length : PROC_READ_MAX_BYTES;
      return decodeAscii(buffer, take);
    } catch (error) {
      this.noteProcUnavailable(stage);
      return null;
    } finally {
      if (file !== null) {
        try {
          await fs.close(file);
        } catch (error) {
          // A proc fd close failure cannot be retried or acted on.
        }
      }
    }
  }

  // ONE concise line per run when a /proc node is not readable. The per-sample
  // `na` fields carry the ongoing availability signal; this must never become a
  // per-tick errno log, so only the first failure of the run is said out loud
  // (and the stage is a label — no path, no pid, no fd target).
  private noteProcUnavailable(stage: string): void {
    if (this.procUnavailableLogged) {
      return;
    }
    this.procUnavailableLogged = true;
    try {
      hilog.warn(DOMAIN, TAG, 'DIAG_PROC_UNAVAILABLE|stage=%{public}s|status=na', stage);
    } catch (error) {
      // hilog itself failing is not actionable here.
    }
  }

  // ---- guarded small IO ---------------------------------------------------

  private async readTextSafe(path: string): Promise<string> {
    if (path.length === 0) {
      return '';
    }
    try {
      if (!(await fs.access(path))) {
        return '';
      }
      return await fs.readText(path);
    } catch (error) {
      this.noteError('marker-read', error as Error);
      return '';
    }
  }

  private async writeTextSafe(path: string, text: string): Promise<void> {
    if (path.length === 0) {
      return;
    }
    try {
      const file: fs.File = await fs.open(path,
        fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      try {
        await fs.write(file.fd, text);
        await fs.fsync(file.fd); // one line: make the marker state durable
      } finally {
        await fs.close(file);
      }
    } catch (error) {
      this.noteError('marker-write', error as Error);
    }
  }

  private async sizeSafe(path: string): Promise<number> {
    if (path.length === 0) {
      return 0;
    }
    try {
      const stat: fs.Stat = await fs.stat(path);
      return stat.size;
    } catch (error) {
      return 0; // absent active file is the normal first-run case
    }
  }

  private noteError(stage: string, error: Error): void {
    this.internalErrors += 1;
    if (this.internalErrors > MAX_INTERNAL_ERROR_LOGS) {
      return; // keep the counter, stop the log flood
    }
    let code: string = 'none';
    try {
      code = String((error as BusinessFailure).code ?? 'none');
    } catch (inner) {
      code = 'unreadable';
    }
    try {
      hilog.warn(DOMAIN, TAG, 'DIAG_IO_ERROR|stage=%{public}s|code=%{public}s|total=%{public}d',
        stage, code, this.internalErrors);
    } catch (inner) {
      // hilog itself failing is not actionable here.
    }
  }
}

const runtime: DiagRuntime = new DiagRuntime();

// Wiring: NetBirdDiagnostics.start(this.context.filesDir, 'ui') from the UI
// ability's create callback; start(this.context.filesDir, 'vpn', sink,
// allocReader) from the VPN extension's create callback with the native
// phone_log append supplied as sink and a reader over the NAPI `alloc_stats()`
// export supplied as the optional 4th argument; stop() from the
// ability/extension DESTROY callback only. The UI ability also passes
// appendSystemEvent as the system-event watcher's sink (see
// NetBirdSystemEvents.start), and stops that watcher BEFORE this stop() so its
// final sys_watcher_stopped line is still flushed.
// The alloc reader is the ONLY role-dependent argument: the UI process cannot
// see the VPN extension's native connector, so it injects none and its
// every-10th mem_detail carries `na` for the four rust keys. The mem_vm record
// that follows it is role-independent: every process has its own VM heap,
// native malloc heap and GC counters.
export function start(filesDir: string, role: DiagRole, sink?: DiagSink,
  allocReader?: DiagAllocReader): void {
  runtime.start(filesDir, role, sink, allocReader);
}

export function event(name: string, fields?: DiagFields): boolean {
  return runtime.event(name, fields);
}

// The system-event watcher's sink: an already-projected, bounded (<=512-char)
// sys_ line enters the same writer without passing through event(), whose
// 48-char value bound would truncate a kill reason. false = not accepted (not
// running, refused shape, or a full queue).
export function appendSystemEvent(line: string): boolean {
  return runtime.appendSystemEvent(line);
}

export function stop(): void {
  runtime.stop();
}
