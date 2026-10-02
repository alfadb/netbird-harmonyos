// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N2 第一批 C/D 列的 **应用内证据环形文件**（默认关闭，仅武装时启用）。
//!
//! ## Why it exists
//!
//! The C column (Wi-Fi → cellular) and the D column (cellular → Wi-Fi) put the
//! device on a cellular bearer, where the host has NO live log channel: the
//! `hdc` link is Wi-Fi (target `192.168.50.199`) and the HiLog buffer rotates
//! or flow-controls under load — it can drop `N2_*` lines or the leak warning
//! itself. The App therefore keeps its own copy: every line the core emits
//! through [`crate::hilog::emit`] plus every shell-side line the extension
//! forwards ([`crate::connector::n2_ring_append_json`]) is appended to ONE file
//! in the App's private directory, and the operator fetches it after returning
//! to Wi-Fi.
//!
//! ## The durable record (B4, 2026-09-29)
//!
//! Every persisted line is ONE self-describing record:
//!
//! ```text
//! N2R|sess=<arm token>|seq=<persistent monotonic>|mono_ms=<n>|wall_ms=<n>|line=<original line>
//! ```
//!
//! * `seq` is persistent across segments, across rotation and across process
//!   restarts: the next seq is recovered from `<path>.state` (sidecar) AND
//!   reconciled against the highest seq actually found on disk, so a stale or
//!   lost sidecar can never make the ring reuse a seq.
//! * `sess` is the arm token (`configure_session`), so records of two arms are
//!   distinguishable even though they append to the same file; a restart never
//!   rewrites or re-tags existing records.
//! * `mono_ms`/`wall_ms` carry BOTH clocks for the offline window (the host
//!   cannot read the device clock while on cellular).
//! * `line=` is the LAST field and holds the original text verbatim (it may
//!   contain `|`); only interior CR/LF are folded to spaces, because they would
//!   break the one-record-per-line framing.
//!
//! ## Bounds, rotation, capacity (no silent overwrite)
//!
//! * `max_bytes` (default [`DEFAULT_MAX_BYTES`] = 4 MiB) caps the CURRENT
//!   segment; `segments` (default [`DEFAULT_SEGMENTS`] = 3) is the number of
//!   files kept: `<path>`, `<path>.1`, … `<path>.(segments-1)`, oldest first.
//!   A diagnostic arm ticket may SHRINK the cap (never below `MIN_MAX_BYTES`
//!   = 64 KiB, never above the default) so that rotation actually happens in a
//!   short run; the shrink is announced once by [`segment_line`]
//!   (`N2_RING_SEGMENT_BYTES`) and leaves the frozen `N2_RING_ARMED` line
//!   untouched. The default arm emits no extra line at all.
//! * Rotation is **ACK-GATED**: the oldest segment is dropped ONLY when its
//!   last seq is `<= acked_seq`, and `acked_seq` moves ONLY through an
//!   EXPLICIT host ACK request ([`ack_seq`] over [`parse_ack_request`]) that
//!   names THIS arm's session token and an `up_to_seq`. Reading the ring
//!   ([`dump_page`]/[`dump_lines`]) or reporting it ([`manifest_line_now`],
//!   [`status_line_now`]) never advances the watermark: the host must first
//!   persist the fetched bytes and verify the window is complete. An
//!   unacknowledged segment is never deleted.
//! * When the current segment is full and the oldest segment is NOT
//!   acknowledged, the ring enters **overflow**: it stops accepting records,
//!   counts every dropped record with its seq range and keeps every existing
//!   segment. The state is reported by [`manifest_line_now`] (`overflow=true`,
//!   `lost_records=`, `lost_from=`/`lost_to=`) and announced once on HiLog
//!   (`N2_RING_OVERFLOW`). The ring never grows past its cap and never
//!   overwrites a segment.
//! * A failed record consumes its seq (never reused) and is counted into the
//!   lost span; once the writer works again the span is written INTO the ring
//!   as `N2_RING_LOST` plus `N2_RING_RECOVERED`. A partial write marks the tail
//!   as torn, and the next record is separated from it by a newline, so the
//!   torn bytes stay their own (unparseable) line instead of merging.
//!
//! ## Fetching it
//!
//! * The bundle-scoped transfer on a DEBUG-signed, launched App
//!   (`hdc file recv -b cn.alfadb.netbird ./data/storage/el2/base/files/<file>`
//!   `<host>`) is the vendor-documented path.
//! * The in-ring fallback is [`dump_lines`]: pagination is by durable `seq`
//!   (`from_seq`/`to_seq`), never by concatenated line index, so a page already
//!   read cannot move when the ring is appended to — and because rotation only
//!   happens after an ACK, an un-ACKed window cannot be rotated away either.
//!   The legacy `{"start":<line index>,"count":…}` request is REFUSED with
//!   `legacy-start-index-unsupported`: a line index over concatenated segments
//!   has no safe mapping onto durable seqs.
//! * [`manifest_line_now`] answers head/tail/failure/gap questions
//!   (`first_seq`, `last_seq`, `frozen_tail_seq`, `failures`, `lost_records`,
//!   `lost_from`/`lost_to`, `gaps`, `malformed`, `overflow`, `acked_seq`).
//!
//! ## Default off / armed only
//!
//! Nothing is written until [`configure`] was called by the armed session (the
//! shell calls it from the accepted `n2_evidence` arm and calls [`disable`] on
//! every arming change). Unconfigured, [`append`] is one atomic load and
//! returns and NO file (not even the sidecar) is created.
//!
//! ## The explicit ACK (the ONLY way `acked_seq` moves)
//!
//! `connector.rs`/`napi.rs` expose the retrieval surfaces: `n2_ring_dump`
//! (seq-paged, over [`dump_page`]/[`chunk_line`]/[`dump_summary_line`]),
//! `n2_ring_manifest` (over [`manifest_line_now`]) and `n2_ring_ack` (over
//! [`parse_ack_request`] + [`ack_seq`]). The first two are READ-ONLY with
//! respect to the watermark. The third takes one document,
//!
//! ```text
//! {"session":"<the arm token the records carry>","up_to_seq":<durable seq>}
//! ```
//!
//! and is refused (`not-configured` / `not-armed` / `invalid-session` /
//! `session-mismatch` / `seq-beyond-tail`) unless the ring is ARMED and the
//! named session is the CURRENT arm's token: a stale request from an expired
//! arm can never release another arm's segments, and the check happens under
//! the same lock as the watermark update (the shell never reads the session and
//! then acks unconditionally). Re-sending the same `up_to_seq` is idempotent
//! (`advanced=false`); the watermark is monotonic (`max`) and never rolls back.
//!
//! The shell side (`NetBirdN2Ring.ets` + `NetBirdVpnExtensionAbility.ets`)
//! forwards the host's document VERBATIM from the `n2_ring_ack` private
//! channel and only mirrors the outcome in `N2_RING_ACK_SHELL`.

use std::cell::Cell;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Json;
use crate::hilog;
use crate::sys;

/// Default cap of one segment (4 MiB, ~3 segments ≈ 12 MiB worst case).
pub const DEFAULT_MAX_BYTES: u64 = 4 * 1024 * 1024;
/// Default number of segment files kept (current + 2 older).
pub const DEFAULT_SEGMENTS: u32 = 3;
/// The App-private basename the shell configures (documented, not secret).
pub const DEFAULT_FILE_NAME: &str = "netbird-n2-evidence-ring.log";

/// The smallest accepted cap: below this a single burst of N2 lines would
/// rotate constantly instead of keeping a usable window.
const MIN_MAX_BYTES: u64 = 64 * 1024;
/// The largest accepted segment count (bounded disk, bounded fetch window).
const MAX_SEGMENTS: u32 = 8;
/// The persistent record prefix (see the module docs).
pub const RECORD_PREFIX: &str = "N2R|";
/// The default `count` of one seq-paged dump.
pub const DEFAULT_DUMP_COUNT: usize = 64;
/// The largest `count` of one seq-paged dump (bounded HiLog traffic).
pub const MAX_DUMP_LINES: usize = 256;
/// The largest accepted dump request document.
const MAX_DUMP_REQUEST_BYTES: usize = 512;
/// The largest accepted ACK request document (the shell mirrors this bound).
const MAX_ACK_REQUEST_BYTES: usize = 512;

/// Fast path: false until [`configure`] enabled the ring, so the hilog tee is
/// one atomic load in the common (unarmed) case.
static ENABLED: AtomicBool = AtomicBool::new(false);

/// Auto session tokens (when the shell does not supply an arm nonce) must be
/// unique per arm inside one process.
static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// One kept segment, in the ring's own indexing (`0` = current `<path>`).
#[derive(Clone, Copy, Debug)]
struct SegMeta {
    index: u32,
    first_seq: u64,
    last_seq: u64,
    bytes: u64,
    records: u64,
}

struct State {
    path: PathBuf,
    max_bytes: u64,
    segments: u32,
    session: String,
    /// Next persistent seq to assign (recovered, never reused).
    seq_next: u64,
    /// Highest persisted seq (0 while the ring is empty).
    last_seq: u64,
    /// Lowest persisted seq (0 while the ring is empty).
    first_seq: u64,
    /// Persisted record count across all kept segments.
    records_total: u64,
    /// Non-record lines found at arm time (legacy or torn bytes).
    malformed: u64,
    /// Bytes currently in the CURRENT segment.
    bytes: u64,
    /// The current segment does not end at a record boundary (a torn write).
    tail_open: bool,
    metas: Vec<SegMeta>,
    writes: u64,
    failures: u64,
    rotations: u64,
    /// Operator-confirmed retrieved watermark (the rotation gate).
    acked_seq: u64,
    /// Records removed by ACK-gated rotation.
    dropped_by_ack: u64,
    /// Records attempted but NOT persisted (write failures + overflow drops).
    lost_records: u64,
    lost_from: u64,
    lost_to: u64,
    lost_episode_records: u64,
    lost_reason: String,
    overflow: bool,
    overflow_events: u64,
    overflow_announced: bool,
    /// The last write error, as an errno-style code (never a path/content).
    last_error: String,
    /// Failures already announced by an `N2_RING_WRITE_FAILED` line.
    announced_failures: u64,
    /// A success after ≥1 failure still owes its recovery report.
    recovered_pending: bool,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

fn state() -> Option<MutexGuard<'static, State>> {
    STATE.get().map(|state| state.lock().unwrap_or_else(PoisonError::into_inner))
}

thread_local! {
    /// Re-entrancy guard: an announcement emitted while the writer runs must
    /// NOT be appended again (that would either duplicate the line or, on a
    /// failing writer, loop).
    static IN_APPEND: Cell<bool> = const { Cell::new(false) };
}

pub fn enabled() -> bool { ENABLED.load(Ordering::Acquire) }

fn valid_name(path: &str) -> bool {
    !path.is_empty() && path.len() <= 512 && !path.contains('\n') && path.contains('/')
}

/// Enable the ring on `path`. Validation only — an absent file is normal (the
/// first append creates it); the shell re-creates the ring on every arm.
///
/// The session token is the auto-generated arm token; use
/// [`configure_session`] to pass the shell's arm nonce / ticket fingerprint.
pub fn configure(path: &str, max_bytes: u64, segments: u32) -> Result<(), &'static str> {
    configure_session(path, max_bytes, segments, None)
}

/// [`configure`] with an explicit session token (`sess=`); `None` generates a
/// fresh per-arm token. A repeated token is a resume of the same arm: seq still
/// continues (never restarts) and old records are never re-tagged.
pub fn configure_session(path: &str, max_bytes: u64, segments: u32,
    session: Option<&str>) -> Result<(), &'static str> {
    if !valid_name(path) {
        return Err("invalid-path");
    }
    let max_bytes = if max_bytes == 0 { DEFAULT_MAX_BYTES } else { max_bytes };
    if max_bytes < MIN_MAX_BYTES {
        return Err("invalid-max-bytes");
    }
    let segments = if segments == 0 { DEFAULT_SEGMENTS } else { segments };
    if segments > MAX_SEGMENTS {
        return Err("invalid-segments");
    }
    let session = match session {
        Some(given) => {
            let cleaned = clean(given);
            if cleaned.is_empty() {
                return Err("invalid-session");
            }
            cleaned
        }
        None => String::new(),
    };
    let path = PathBuf::from(path);
    let file = file_name(&path);
    let sidecar = read_sidecar(&path);
    let session = if session.is_empty() {
        // A fresh arm keeps its own token unless the shell resumed an explicit
        // one; the persisted token being equal is impossible for a new arm.
        let mut generated = format!("auto-{}-{}-{}", sys::mono_ms(), std::process::id(),
            SESSION_COUNTER.fetch_add(1, Ordering::Relaxed));
        if generated == sidecar.session {
            generated.push_str("-b");
        }
        generated
    } else {
        session
    };
    // Recovery: scan what is ACTUALLY on disk; it is authoritative over the
    // sidecar, so a stale sidecar can never make us reuse a seq or delete data.
    let scan = scan_segments(&path, segments);
    let last_seq = scan.max_seq;
    let first_seq = scan.metas.iter().filter(|meta| meta.first_seq > 0)
        .map(|meta| meta.first_seq).min().unwrap_or(0);
    let acked_seq = sidecar.acked_seq.min(last_seq);
    let seq_next = sidecar.next_seq.max(last_seq.saturating_add(1)).max(1);
    let fresh = State {
        path,
        max_bytes,
        segments,
        session,
        seq_next,
        last_seq,
        first_seq,
        records_total: scan.records,
        malformed: scan.malformed,
        bytes: scan.current_bytes,
        tail_open: scan.tail_open,
        metas: scan.metas,
        writes: sidecar.writes,
        failures: sidecar.failures,
        rotations: sidecar.rotations,
        acked_seq,
        dropped_by_ack: sidecar.dropped_by_ack,
        lost_records: sidecar.lost_records,
        lost_from: sidecar.lost_from,
        lost_to: sidecar.lost_to,
        lost_episode_records: 0,
        lost_reason: sidecar.lost_reason,
        overflow: false,
        overflow_events: sidecar.overflow_events,
        overflow_announced: false,
        last_error: String::new(),
        announced_failures: 0,
        recovered_pending: sidecar.lost_from != 0,
    };
    match STATE.get() {
        Some(slot) => *slot.lock().unwrap_or_else(PoisonError::into_inner) = fresh,
        None => { let _ = STATE.set(Mutex::new(fresh)); }
    }
    ENABLED.store(true, Ordering::Release);
    // The armed line IS the ring's first record (the tee appends it).
    hilog::emit(&armed_line(&file, max_bytes, segments, scan.current_bytes, sys::mono_ms()));
    // A shrunk cap (the diagnostic `ring_segment_bytes` arm ticket) gets its own
    // record: the frozen N2_RING_ARMED line keeps its documented field list, and
    // the default arm emits NOTHING extra (byte-identical default behavior).
    if max_bytes != DEFAULT_MAX_BYTES {
        hilog::emit(&segment_line(max_bytes, sys::mono_ms()));
    }
    persist_now();
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".into())
}

/// Turn the ring off (every arming change / teardown). The FILE is kept — the
/// operator still has to fetch it; the counters are kept too (sidecar), so a
/// later status/manifest call can still report them.
pub fn disable() {
    persist_now();
    ENABLED.store(false, Ordering::Release);
}

/// Append ONE line verbatim as a durable record. No-op while disabled, empty,
/// or re-entered from an announcement.
pub fn append(line: &str) {
    if line.is_empty() || !enabled() {
        return;
    }
    if IN_APPEND.with(|flag| flag.get()) {
        return;
    }
    IN_APPEND.with(|flag| flag.set(true));
    let outcome = write_record(line);
    IN_APPEND.with(|flag| flag.set(false));
    match outcome {
        Ok(Some(rotated)) => {
            // The rotation fact is IN the ring (first record of the new
            // segment); the live copy rides HiLog.
            hilog_only(&rotated.report);
            announce_recovery();
        }
        Ok(None) => announce_recovery(),
        Err(WriteError::Overflow) => announce_overflow(),
        Err(WriteError::Io(code)) => announce_failure(&code),
    }
}

enum WriteError {
    /// Capacity: the record was dropped, its seq consumed, the state keeps the
    /// span; no segment was deleted.
    Overflow,
    /// An I/O error, as an errno-style code.
    Io(String),
}

struct RotatedInfo {
    /// The `N2_RING_ROTATED` report, already the first record of the new
    /// segment when `write_record` returns it.
    report: String,
}

/// `Ok(Some(rotated))` when this append rotated the ring. The rotation report
/// is already the first record of the new segment when `Some` is returned.
fn write_record(text: &str) -> Result<Option<RotatedInfo>, WriteError> {
    let mut state = match state() {
        Some(state) => state,
        None => return Ok(None),
    };
    let mut seq = state.seq_next;
    let mono = sys::mono_ms();
    let mut record = encode_record(seq, &state.session, mono, wall_ms(), text);
    let mut rotated = None;
    if needs_rotation(&state, &record) {
        if can_rotate(&state) {
            let info = match rotate_current_segment(&mut state) {
                Ok(info) => info,
                Err(code) => {
                    note_lost(&mut state, seq, "write-failed", &code);
                    persist(&state);
                    return Err(WriteError::Io(code));
                }
            };
            // The rotation report is the FIRST record of the new segment.
            seq = state.seq_next;
            let report = encode_record(seq, &state.session, sys::mono_ms(), wall_ms(), &info.report);
            if let Err(code) = commit(&mut state, seq, &report) {
                note_lost(&mut state, seq, "write-failed", &code);
                persist(&state);
                return Err(WriteError::Io(code));
            }
            rotated = Some(info);
            seq = state.seq_next;
            record = encode_record(seq, &state.session, sys::mono_ms(), wall_ms(), text);
        } else {
            // CAPACITY: stop accepting, keep every segment, make it auditable.
            let first = !state.overflow;
            note_lost(&mut state, seq, "overflow", "");
            state.overflow = true;
            if first {
                state.overflow_events = state.overflow_events.saturating_add(1);
                persist(&state);
            }
            return Err(WriteError::Overflow);
        }
    }
    match commit(&mut state, seq, &record) {
        Ok(()) => Ok(rotated),
        Err(code) => {
            note_lost(&mut state, seq, "write-failed", &code);
            persist(&state);
            Err(WriteError::Io(code))
        }
    }
}

/// `true` when the encoded record no longer fits the current segment (an empty
/// segment always accepts one record, so rotation cannot spin).
fn needs_rotation(state: &State, record: &str) -> bool {
    if state.bytes == 0 {
        return false;
    }
    let newline = if state.tail_open { 1 } else { 0 };
    state.bytes.saturating_add(newline).saturating_add(record.len() as u64).saturating_add(1)
        > state.max_bytes
}

/// The oldest kept segment may be dropped ONLY when it was acknowledged as
/// retrieved. A segment that exists but holds no parseable record (legacy or
/// torn bytes) is NOT deletable: it was never verified as retrieved.
fn can_rotate(state: &State) -> bool {
    let segments = state.segments.max(1);
    if segments == 1 {
        return state.bytes == 0
            || (state.last_seq > 0 && state.last_seq <= state.acked_seq);
    }
    match state.metas.iter().find(|meta| meta.index == segments - 1) {
        Some(meta) => meta.bytes == 0
            || (meta.last_seq > 0 && meta.last_seq <= state.acked_seq),
        None => true,
    }
}

/// Drop the oldest segment, shift the rest, start a fresh current segment.
/// A rename failure is a WRITE failure: the ring must not silently grow past
/// its cap (the line is dropped and counted instead).
///
/// This is the PHYSICAL step only; the caller (`write_record`) must have
/// established through `can_rotate` that the segment being deleted was
/// acknowledged as retrieved.
fn rotate(state: &mut State) -> Result<(u32, u64), String> {
    let path = state.path.clone();
    let segments = state.segments.max(1);
    let rotated_bytes = state.bytes;
    if segments > 1 {
        let oldest = segment_path(&path, segments - 1);
        if oldest.exists() {
            fs::remove_file(&oldest).map_err(|error| error_code(&error))?;
        }
        for index in (1..segments - 1).rev() {
            let from = segment_path(&path, index);
            if from.exists() {
                fs::rename(&from, segment_path(&path, index + 1))
                    .map_err(|error| error_code(&error))?;
            }
        }
        fs::rename(&path, segment_path(&path, 1)).map_err(|error| error_code(&error))?;
    } else {
        // segments == 1: the ring is a single capped file — truncate it.
        fs::write(&path, b"").map_err(|error| error_code(&error))?;
    }
    state.rotations = state.rotations.saturating_add(1);
    state.bytes = 0;
    Ok((segments, rotated_bytes))
}

/// `rotate` plus the ring's own bookkeeping: shift the segment metadata, count
/// what left the ring, and build the in-ring `N2_RING_ROTATED` report.
fn rotate_current_segment(state: &mut State) -> Result<RotatedInfo, String> {
    let segments = state.segments.max(1);
    let dropped = state.metas.iter().find(|meta| meta.index == segments - 1).copied();
    let (segments_out, rotated_bytes) = rotate(state)?;
    let mut shifted: Vec<SegMeta> = Vec::new();
    for meta in state.metas.drain(..) {
        if meta.index == segments - 1 {
            continue;
        }
        shifted.push(SegMeta { index: meta.index + 1, ..meta });
    }
    state.metas = shifted;
    if let Some(meta) = &dropped {
        state.dropped_by_ack = state.dropped_by_ack.saturating_add(meta.records);
    }
    state.bytes = 0;
    state.tail_open = false;
    let dropped_from = dropped.as_ref().map(|meta| meta.first_seq).unwrap_or(0);
    let dropped_to = dropped.as_ref().map(|meta| meta.last_seq).unwrap_or(0);
    let dropped_records = dropped.as_ref().map(|meta| meta.records).unwrap_or(0);
    let report = rotated_span_line(segments_out, rotated_bytes, dropped_from, dropped_to,
        dropped_records, state.acked_seq, sys::mono_ms());
    Ok(RotatedInfo { report })
}

fn segment_path(path: &Path, index: u32) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".{index}"));
    PathBuf::from(name)
}

/// Append one encoded record (no trailing newline) to the current segment and
/// book it. A partial write reconciles the byte count from the file itself and
/// marks the tail as torn, so the next record cannot merge into it.
fn commit(state: &mut State, seq: u64, record: &str) -> Result<(), String> {
    let mut buffer = String::new();
    if state.tail_open {
        // Close the torn record on its own line before starting a new one.
        buffer.push('\n');
    }
    buffer.push_str(record);
    buffer.push('\n');
    let mut file = OpenOptions::new().create(true).append(true).open(&state.path)
        .map_err(|error| error_code(&error))?;
    if let Err(error) = file.write_all(buffer.as_bytes()) {
        if let Ok(meta) = file.metadata() {
            state.bytes = meta.len();
        }
        state.tail_open = true;
        return Err(error_code(&error));
    }
    let len = file.metadata().map(|meta| meta.len())
        .unwrap_or_else(|_| state.bytes.saturating_add(buffer.len() as u64));
    state.bytes = len;
    state.tail_open = false;
    state.writes = state.writes.saturating_add(1);
    state.seq_next = seq.saturating_add(1);
    state.last_seq = seq;
    if state.first_seq == 0 {
        state.first_seq = seq;
    }
    state.records_total = state.records_total.saturating_add(1);
    match state.metas.iter_mut().find(|meta| meta.index == 0) {
        Some(meta) => {
            if meta.first_seq == 0 {
                meta.first_seq = seq;
            }
            meta.last_seq = seq;
            meta.bytes = state.bytes;
            meta.records = meta.records.saturating_add(1);
        }
        None => state.metas.push(SegMeta {
            index: 0, first_seq: seq, last_seq: seq, bytes: state.bytes, records: 1,
        }),
    }
    Ok(())
}

/// Consume `seq` as LOST (never reused) and extend the unreported span.
fn note_lost(state: &mut State, seq: u64, reason: &str, code: &str) {
    state.seq_next = seq.saturating_add(1);
    state.lost_records = state.lost_records.saturating_add(1);
    state.lost_episode_records = state.lost_episode_records.saturating_add(1);
    if state.lost_from == 0 {
        state.lost_from = seq;
    }
    state.lost_to = seq;
    state.lost_reason = reason.to_string();
    if !code.is_empty() {
        state.last_error = code.to_string();
    }
}

/// Emit on HiLog with the ring tee suppressed: the ring copy is written by the
/// caller (so the ring gets exactly one record and no recursion).
fn hilog_only(text: &str) {
    IN_APPEND.with(|flag| flag.set(true));
    hilog::emit(text);
    IN_APPEND.with(|flag| flag.set(false));
}

/// Emit on HiLog AND append the same text as a ring record.
fn ringed_record(text: &str) {
    if !enabled() {
        return;
    }
    IN_APPEND.with(|flag| flag.set(true));
    hilog::emit(text);
    let _ = write_record(text);
    IN_APPEND.with(|flag| flag.set(false));
}

fn announce_failure(code: &str) {
    // Announce the FIRST failure of this ring only: the counter stays honest
    // for every one of them, and the recovery line reports the total. (A
    // per-failure line would emit one hilog record per dropped line under a
    // sustained failure — e.g. a full disk — for no extra information.)
    let first = match state() {
        Some(mut state) => {
            state.failures = state.failures.saturating_add(1);
            state.last_error = code.to_string();
            state.recovered_pending = true;
            if state.announced_failures == 0 {
                state.announced_failures = 1;
                true
            } else {
                false
            }
        }
        None => false,
    };
    if !first {
        return;
    }
    let failures = state().map(|state| state.failures).unwrap_or(0);
    hilog_only(&fail_line(failures, code, sys::mono_ms()));
}

/// The writer works again: report the lost span and the recovery INTO the ring
/// (the failure facts have seq ranges, so a gap can be judged `UNSAT`).
fn announce_recovery() {
    let plan = match state() {
        Some(mut state) => {
            if state.lost_from == 0 && !state.recovered_pending {
                None
            } else {
                let from = state.lost_from;
                let to = state.lost_to;
                let records = state.lost_episode_records;
                let reason = std::mem::take(&mut state.lost_reason);
                let code = state.last_error.clone();
                let failures = state.failures;
                let recovered = state.recovered_pending;
                state.lost_from = 0;
                state.lost_to = 0;
                state.lost_episode_records = 0;
                state.recovered_pending = false;
                state.overflow = false;
                state.overflow_announced = false;
                Some((from, to, records, reason, code, failures, recovered, sys::mono_ms()))
            }
        }
        None => None,
    };
    let (from, to, records, reason, code, failures, recovered, mono) = match plan {
        Some(plan) => plan,
        None => return,
    };
    if from != 0 {
        ringed_record(&lost_line(from, to, records, &reason, &code, mono));
    }
    if recovered {
        ringed_record(&recovered_line(failures, mono));
    }
    persist_now();
}

/// The capacity state is announced once per overflow episode; the ring record
/// itself is written once the writer has room again (space only frees through
/// an ACK-gated rotation).
fn announce_overflow() {
    let plan = match state() {
        Some(mut state) => {
            if state.overflow_announced {
                None
            } else {
                state.overflow_announced = true;
                Some((state.lost_records, state.lost_from, state.lost_to, state.acked_seq,
                    sys::mono_ms()))
            }
        }
        None => None,
    };
    if let Some((total, from, to, acked, mono)) = plan {
        hilog_only(&overflow_line(total, from, to, acked, mono));
    }
}

// ---------------------------------------------------------------------------
// retrieval: ACK, manifest, seq-paged dump
// ---------------------------------------------------------------------------

/// The operator confirms that every record with `seq <= up_to_seq` was
/// retrieved AND persisted on the host. ONLY this call moves `acked_seq`, and
/// only an acknowledged watermark lets the next rotation drop the oldest
/// segment; the call deletes NOTHING by itself.
///
/// The request is bound to the arm that produced the records: `session` must be
/// the CURRENT arm's token ([`configure_session`]), checked under the same lock
/// as the watermark update, so a stale ACK from an expired arm is refused
/// (`session-mismatch`) instead of releasing the live ring's segments. A
/// repeated (or lower) `up_to_seq` is idempotent: the watermark is monotonic.
///
/// `acked_seq` also does NOT move while the ring is unarmed (`not-armed`): the
/// watermark gates a writer, and a ring nobody is writing must not have its
/// segments released by a file that outlived the session.
pub fn ack_seq(up_to_seq: u64, session: &str) -> Result<AckOutcome, &'static str> {
    let cleaned = clean(session);
    if cleaned.is_empty() {
        return Err("invalid-session");
    }
    let mut state = state().ok_or("not-configured")?;
    if !enabled() {
        return Err("not-armed");
    }
    if cleaned != state.session {
        return Err("session-mismatch");
    }
    if up_to_seq > state.last_seq {
        return Err("seq-beyond-tail");
    }
    let previous_acked_seq = state.acked_seq;
    state.acked_seq = state.acked_seq.max(up_to_seq);
    let advanced = state.acked_seq > previous_acked_seq;
    persist(&state);
    Ok(AckOutcome {
        acked_seq: state.acked_seq,
        previous_acked_seq,
        advanced,
        session: cleaned,
        up_to_seq,
    })
}

/// One explicit ACK request: the arm the records belong to plus the durable seq
/// the host has persisted and verified.
#[derive(Clone, Debug, PartialEq)]
pub struct AckRequest {
    pub session: String,
    pub up_to_seq: u64,
}

/// One served (or refused) ACK. `advanced` reports whether THIS request moved
/// the watermark; a repeat of an already-acknowledged watermark answers
/// `advanced=false` with the same `acked_seq` (idempotent, never an error).
#[derive(Clone, Debug, PartialEq)]
pub struct AckOutcome {
    pub acked_seq: u64,
    pub previous_acked_seq: u64,
    pub advanced: bool,
    pub session: String,
    pub up_to_seq: u64,
}

/// Parse one `n2_ring_ack` request document. BOTH keys are required: an ACK
/// without a session could release another arm's segments, and one without
/// `up_to_seq` confirms nothing. The named refusals are the SAME tokens the
/// shell answers for the same document, so a refusal reads identically whether
/// the shell or the core produced it.
pub fn parse_ack_request(text: &str) -> Result<AckRequest, &'static str> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("empty-request");
    }
    if trimmed.len() > MAX_ACK_REQUEST_BYTES {
        return Err("request-too-long");
    }
    let document = crate::config::parse_document(trimmed).map_err(|_| "invalid-json")?;
    let entries = match document {
        Json::Obj(entries) => entries,
        _ => return Err("invalid-request"),
    };
    let session = match json_str(&entries, "session")? {
        Some(session) => {
            let cleaned = clean(session);
            if cleaned.is_empty() {
                return Err("invalid-session");
            }
            cleaned
        }
        None => return Err("invalid-session"),
    };
    let up_to_seq = match json_u64(&entries, "up_to_seq")? {
        Some(up_to_seq) => up_to_seq,
        None => return Err("invalid-request"),
    };
    Ok(AckRequest { session, up_to_seq })
}

/// The current acknowledged-retrieved watermark.
pub fn acked_seq() -> u64 {
    state().map(|state| state.acked_seq).unwrap_or(0)
}

/// The head/tail/failure/gap manifest as one `N2_RING_MANIFEST` line.
///
/// The manifest rescans the segments first, because a missing or truncated
/// segment (disk loss, operator error) is exactly what it must report; the seq
/// watermarks stay monotonic (`seq` is never rolled back).
pub fn manifest_line_now() -> String {
    let mono = sys::mono_ms();
    match state() {
        Some(mut state) => {
            refresh_from_disk(&mut state);
            manifest_line(&file_name(&state.path), &state, mono)
        }
        None => manifest_line("none", &State::empty(), mono),
    }
}

/// Re-read the kept segments into the in-memory metadata. Authoritative for
/// what is ON DISK (segment set, byte counts, torn tail); `seq_next`/`last_seq`
/// only ever move forward.
fn refresh_from_disk(state: &mut State) {
    let scan = scan_segments(&state.path, state.segments);
    let disk_first = scan.metas.iter().filter(|meta| meta.first_seq > 0)
        .map(|meta| meta.first_seq).min().unwrap_or(0);
    state.metas = scan.metas;
    state.malformed = scan.malformed;
    state.records_total = scan.records;
    state.bytes = scan.current_bytes;
    state.tail_open = scan.tail_open;
    if scan.max_seq > state.last_seq {
        state.last_seq = scan.max_seq;
    }
    if disk_first > 0 {
        state.first_seq = disk_first;
    }
    if state.seq_next <= state.last_seq {
        state.seq_next = state.last_seq.saturating_add(1);
    }
    if state.acked_seq > state.last_seq {
        state.acked_seq = state.last_seq;
    }
}

impl State {
    fn empty() -> State {
        State {
            path: PathBuf::new(), max_bytes: 0, segments: 0, session: String::new(),
            seq_next: 0, last_seq: 0, first_seq: 0, records_total: 0, malformed: 0,
            bytes: 0, tail_open: false, metas: Vec::new(), writes: 0, failures: 0,
            rotations: 0, acked_seq: 0, dropped_by_ack: 0, lost_records: 0, lost_from: 0,
            lost_to: 0, lost_episode_records: 0, lost_reason: String::new(), overflow: false,
            overflow_events: 0, overflow_announced: false, last_error: String::new(),
            announced_failures: 0, recovered_pending: false,
        }
    }
}

/// One seq-paged dump request. `to_seq` is the FROZEN tail seq taken from a
/// manifest: a page is a seq window, so later appends cannot move it.
#[derive(Clone, Debug, PartialEq)]
pub struct DumpRequest {
    pub from_seq: u64,
    pub to_seq: Option<u64>,
    pub count: usize,
    pub session: Option<String>,
}

/// Parse one `n2_ring_dump` request document. The legacy line-index form
/// (`{"start":<line index>}`) is REFUSED: a line index over concatenated
/// segments has no safe mapping onto durable seqs (rotation shifts it), and
/// pretending to serve it would silently duplicate or skip records.
pub fn parse_dump_request(text: &str) -> Result<DumpRequest, &'static str> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("empty-request");
    }
    if trimmed.len() > MAX_DUMP_REQUEST_BYTES {
        return Err("request-too-long");
    }
    let document = crate::config::parse_document(trimmed).map_err(|_| "invalid-json")?;
    let entries = match document {
        Json::Obj(entries) => entries,
        _ => return Err("invalid-request"),
    };
    if entries.iter().any(|(key, _)| key == "start") {
        return Err("legacy-start-index-unsupported");
    }
    let from_seq = json_u64(&entries, "from_seq")?.unwrap_or(0);
    let to_seq = json_u64(&entries, "to_seq")?;
    let count = match json_u64(&entries, "count")? {
        Some(count) => count as usize,
        None => DEFAULT_DUMP_COUNT,
    };
    if count == 0 || count > MAX_DUMP_LINES {
        return Err("invalid-count");
    }
    let session = match json_str(&entries, "session")? {
        Some(session) => {
            let cleaned = clean(session);
            if cleaned.is_empty() {
                return Err("invalid-session");
            }
            Some(cleaned)
        }
        None => None,
    };
    Ok(DumpRequest { from_seq, to_seq, count, session })
}

fn json_u64(entries: &[(String, Json)], key: &str) -> Result<Option<u64>, &'static str> {
    match entries.iter().find(|(name, _)| name == key) {
        None => Ok(None),
        Some((_, Json::Num(value))) => Ok(Some(*value)),
        Some(_) => Err("invalid-request"),
    }
}

fn json_str<'a>(entries: &'a [(String, Json)], key: &str) -> Result<Option<&'a str>, &'static str> {
    match entries.iter().find(|(name, _)| name == key) {
        None => Ok(None),
        Some((_, Json::Str(value))) => Ok(Some(value.as_str())),
        Some(_) => Err("invalid-request"),
    }
}

/// One served (or refused) seq-paged dump.
#[derive(Clone, Debug, PartialEq)]
pub struct DumpPage {
    pub result: &'static str,
    pub reason: &'static str,
    pub from_seq: u64,
    pub to_seq: u64,
    pub count: usize,
    pub frozen_tail_seq: u64,
    pub first_seq: u64,
    pub last_seq: u64,
    pub records: u64,
    pub bytes: u64,
    pub malformed: u64,
    pub lost_records: u64,
    pub overflow: bool,
    pub acked_seq: u64,
    pub session: Option<String>,
    /// `(seq, original line)` in ascending seq order.
    pub lines: Vec<(u64, String)>,
}

impl DumpPage {
    fn refused(reason: &'static str) -> DumpPage {
        DumpPage {
            result: "rejected", reason, from_seq: 0, to_seq: 0, count: 0, frozen_tail_seq: 0,
            first_seq: 0, last_seq: 0, records: 0, bytes: 0, malformed: 0, lost_records: 0,
            overflow: false, acked_seq: 0, session: None, lines: Vec::new(),
        }
    }
}

/// Serve one dump request against the CURRENT ring state. Appends that happen
/// after this call carry higher seqs and can never change this window's bytes;
/// no rotation can remove an un-ACKed record.
pub fn dump_page(request_json: &str) -> DumpPage {
    let request = match parse_dump_request(request_json) {
        Ok(request) => request,
        Err(reason) => return DumpPage::refused(reason),
    };
    let state = match state() {
        Some(state) => state,
        None => return DumpPage::refused("not-configured"),
    };
    if state.last_seq == 0 {
        return DumpPage::refused("ring-empty");
    }
    let to_seq = request.to_seq.unwrap_or(state.last_seq).min(state.last_seq);
    if to_seq < request.from_seq {
        return DumpPage::refused("empty-window");
    }
    let (lines, malformed, bytes) = read_records(&state, &request, to_seq);
    let overflow = state.overflow;
    DumpPage {
        result: "dumped",
        reason: "ok",
        from_seq: request.from_seq,
        to_seq,
        count: request.count,
        frozen_tail_seq: state.last_seq,
        first_seq: state.first_seq,
        last_seq: state.last_seq,
        records: state.records_total,
        bytes,
        malformed,
        lost_records: state.lost_records,
        overflow,
        acked_seq: state.acked_seq,
        session: request.session.clone(),
        lines,
    }
}

/// The seq-paged dump as `N2_RING_CHUNK` records plus one `N2_RING_DUMP`
/// summary (`line=` is the LAST field of a chunk and holds the original text).
pub fn dump_lines(request_json: &str) -> Vec<String> {
    let page = dump_page(request_json);
    let emitted = page.lines.len();
    let mut out: Vec<String> = Vec::with_capacity(emitted + 1);
    for (seq, original) in &page.lines {
        out.push(chunk_line(*seq, page.count, original));
    }
    out.push(dump_summary_line(&page, emitted));
    out
}

fn read_records(state: &State, request: &DumpRequest, to_seq: u64)
    -> (Vec<(u64, String)>, u64, u64) {
    let mut out: Vec<(u64, String)> = Vec::new();
    let mut malformed = 0u64;
    let mut bytes = 0u64;
    let mut indices: Vec<u32> = state.metas.iter().map(|meta| meta.index).collect();
    indices.sort_unstable();
    indices.reverse(); // chronological: oldest segment first
    'outer: for index in indices {
        let path = if index == 0 { state.path.clone() } else { segment_path(&state.path, index) };
        let content = match fs::read(&path) {
            Ok(content) => content,
            Err(_) => continue, // a missing segment is a gap, not an error
        };
        bytes = bytes.saturating_add(content.len() as u64);
        let text = String::from_utf8_lossy(&content);
        for line in text.split('\n') {
            if line.is_empty() {
                continue;
            }
            match parse_record(line) {
                Some(record) => {
                    if record.seq < request.from_seq || record.seq > to_seq {
                        continue;
                    }
                    if let Some(session) = request.session.as_deref() {
                        if record.session != session {
                            continue;
                        }
                    }
                    out.push((record.seq, record.original));
                    if out.len() >= request.count {
                        break 'outer;
                    }
                }
                None => malformed = malformed.saturating_add(1),
            }
        }
    }
    (out, malformed, bytes)
}

// ---------------------------------------------------------------------------
// sidecar + recovery
// ---------------------------------------------------------------------------

#[derive(Default, Clone)]
struct Sidecar {
    session: String,
    next_seq: u64,
    acked_seq: u64,
    writes: u64,
    failures: u64,
    rotations: u64,
    lost_records: u64,
    overflow_events: u64,
    dropped_by_ack: u64,
    lost_from: u64,
    lost_to: u64,
    lost_reason: String,
}

fn sidecar_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".state");
    PathBuf::from(name)
}

fn read_sidecar(path: &Path) -> Sidecar {
    let mut out = Sidecar::default();
    let text = match fs::read_to_string(sidecar_path(path)) {
        Ok(text) => text,
        Err(_) => return out,
    };
    for line in text.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "session" => out.session = clean(value),
                "next_seq" => out.next_seq = value.parse().unwrap_or(0),
                "acked_seq" => out.acked_seq = value.parse().unwrap_or(0),
                "writes" => out.writes = value.parse().unwrap_or(0),
                "failures" => out.failures = value.parse().unwrap_or(0),
                "rotations" => out.rotations = value.parse().unwrap_or(0),
                "lost_records" => out.lost_records = value.parse().unwrap_or(0),
                "overflow_events" => out.overflow_events = value.parse().unwrap_or(0),
                "dropped_by_ack" => out.dropped_by_ack = value.parse().unwrap_or(0),
                "lost_from" => out.lost_from = value.parse().unwrap_or(0),
                "lost_to" => out.lost_to = value.parse().unwrap_or(0),
                "lost_reason" => out.lost_reason = clean(value),
                _ => {}
            }
        }
    }
    out
}

/// Best-effort atomic sidecar write (temp + rename). A lost sidecar is NOT
/// unsafe: `configure` reconciles `next_seq` against the highest seq actually
/// on disk and clamps `acked_seq` to the tail, so recovery stays conservative.
fn persist(state: &State) {
    let path = sidecar_path(&state.path);
    let mut tmp_name = path.as_os_str().to_os_string();
    tmp_name.push(".tmp");
    let tmp = PathBuf::from(tmp_name);
    let body = format!(
        "version=1\nsession={}\nnext_seq={}\nacked_seq={}\nwrites={}\nfailures={}\nrotations={}\nlost_records={}\noverflow_events={}\ndropped_by_ack={}\nlost_from={}\nlost_to={}\nlost_reason={}\n",
        clean(&state.session), state.seq_next, state.acked_seq, state.writes, state.failures,
        state.rotations, state.lost_records, state.overflow_events, state.dropped_by_ack,
        state.lost_from, state.lost_to, clean(&state.lost_reason));
    if fs::write(&tmp, body.as_bytes()).is_ok() {
        let _ = fs::rename(&tmp, &path);
    }
}

fn persist_now() {
    if let Some(state) = state() {
        persist(&state);
    }
}

struct Scan {
    metas: Vec<SegMeta>,
    max_seq: u64,
    records: u64,
    malformed: u64,
    current_bytes: u64,
    tail_open: bool,
}

/// Read what is ACTUALLY on disk: per-segment first/last seq, record counts and
/// the torn-tail flag. The scan is authoritative over the sidecar.
fn scan_segments(base: &Path, segments: u32) -> Scan {
    let mut scan = Scan {
        metas: Vec::new(), max_seq: 0, records: 0, malformed: 0, current_bytes: 0,
        tail_open: false,
    };
    for index in 0..segments.max(1) {
        let path = if index == 0 { base.to_path_buf() } else { segment_path(base, index) };
        let content = match fs::read(&path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let text = String::from_utf8_lossy(&content);
        let mut first_seq = 0u64;
        let mut last_seq = 0u64;
        let mut records = 0u64;
        let mut malformed = 0u64;
        for line in text.split('\n') {
            if line.is_empty() {
                continue;
            }
            match parse_record(line) {
                Some(record) => {
                    if first_seq == 0 {
                        first_seq = record.seq;
                    }
                    last_seq = record.seq;
                    records = records.saturating_add(1);
                }
                None => malformed = malformed.saturating_add(1),
            }
        }
        if index == 0 {
            scan.current_bytes = content.len() as u64;
            scan.tail_open = !content.is_empty() && !content.ends_with(b"\n");
        }
        scan.records = scan.records.saturating_add(records);
        scan.malformed = scan.malformed.saturating_add(malformed);
        if last_seq > scan.max_seq {
            scan.max_seq = last_seq;
        }
        scan.metas.push(SegMeta {
            index, first_seq, last_seq, bytes: content.len() as u64, records,
        });
    }
    scan
}

// ---------------------------------------------------------------------------
// records + pure formatters (the N2 log-format baseline calls exactly these)
// ---------------------------------------------------------------------------

fn clean(text: &str) -> String {
    text.chars().take(64).map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-' | '_' | '/') {
        c
    } else {
        '_'
    }).collect()
}

/// Fold interior CR/LF to spaces: a record is one line, and a raw newline would
/// break the framing the seq/gap audit depends on.
fn fold_newlines(line: &str) -> String {
    if line.contains('\n') || line.contains('\r') {
        line.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect()
    } else {
        line.to_string()
    }
}

fn wall_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |time| time.as_millis())
}

fn encode_record(seq: u64, session: &str, mono_ms: u64, wall_ms: u128, line: &str) -> String {
    format!("{RECORD_PREFIX}sess={}|seq={seq}|mono_ms={mono_ms}|wall_ms={wall_ms}|line={}",
        clean(session), fold_newlines(line))
}

struct ParsedRecord {
    seq: u64,
    session: String,
    original: String,
}

fn parse_record(line: &str) -> Option<ParsedRecord> {
    let rest = line.strip_prefix(RECORD_PREFIX)?;
    let split = rest.find("|line=")?;
    let head = &rest[..split];
    let original = &rest[split + "|line=".len()..];
    let seq = field_value(head, "seq=")?.parse().ok()?;
    let session = field_value(head, "sess=").unwrap_or("").to_string();
    Some(ParsedRecord { seq, session, original: original.to_string() })
}

fn field_value<'a>(head: &'a str, key: &str) -> Option<&'a str> {
    head.split('|').find_map(|field| field.strip_prefix(key))
}

/// `(gap count, first gap from, last gap to)` across kept segment boundaries.
/// Records missing INSIDE a segment are the accounted-for `lost_` span; this
/// finds the boundaries a missing/rotated segment leaves behind.
fn gaps(state: &State) -> (u64, u64, u64) {
    let mut metas: Vec<&SegMeta> = state.metas.iter().filter(|meta| meta.last_seq > 0).collect();
    metas.sort_by(|a, b| b.index.cmp(&a.index));
    let mut gaps = 0u64;
    let mut gap_from = 0u64;
    let mut gap_to = 0u64;
    let mut previous_last = 0u64;
    for meta in metas {
        if previous_last != 0 && meta.first_seq > previous_last.saturating_add(1) {
            gaps = gaps.saturating_add(1);
            if gap_from == 0 {
                gap_from = previous_last.saturating_add(1);
            }
            gap_to = meta.first_seq.saturating_sub(1);
        }
        previous_last = previous_last.max(meta.last_seq);
    }
    (gaps, gap_from, gap_to)
}

pub fn armed_line(file: &str, max_bytes: u64, segments: u32, bytes: u64, mono_ms: u64) -> String {
    format!("N2_RING_ARMED|file={}|max_bytes={max_bytes}|segments={segments}|bytes={bytes}|mono_ms={mono_ms}|diag=true|is_evidence=false",
        clean(file))
}

/// The diagnostic shrink announcement, emitted (right after `N2_RING_ARMED`)
/// ONLY when the effective segment cap differs from [`DEFAULT_MAX_BYTES`] — the
/// default arm therefore stays byte-identical. The frozen `N2_RING_ARMED` line
/// keeps its documented field list; this is the explicit marker for a
/// `ring_segment_bytes` arm ticket that shrank the ring
/// (docs/n2-cd-app-sandwich-20260929.md). Never an evidence line.
pub fn segment_line(segment_bytes: u64, mono_ms: u64) -> String {
    format!("N2_RING_SEGMENT_BYTES|segment_bytes={segment_bytes}|default_bytes={DEFAULT_MAX_BYTES}|mono_ms={mono_ms}|diag=true|is_evidence=false")
}

#[allow(clippy::too_many_arguments)]
pub fn stats_line(file: &str, enabled: bool, bytes: u64, max_bytes: u64, segments: u32,
    writes: u64, failures: u64, rotations: u64, last_error: &str, mono_ms: u64) -> String {
    format!("N2_RING_STATS|file={}|enabled={enabled}|bytes={bytes}|max_bytes={max_bytes}|segments={segments}|writes={writes}|failures={failures}|rotations={rotations}|last_error={}|mono_ms={mono_ms}|diag=true|is_evidence=false",
        clean(file), clean(last_error))
}

pub fn fail_line(failures: u64, code: &str, mono_ms: u64) -> String {
    format!("N2_RING_WRITE_FAILED|failures={failures}|code={}|mono_ms={mono_ms}|diag=true|is_evidence=false",
        clean(code))
}

pub fn recovered_line(failures: u64, mono_ms: u64) -> String {
    format!("N2_RING_RECOVERED|failures={failures}|mono_ms={mono_ms}|diag=true|is_evidence=false")
}

pub fn rotated_line(segments: u32, bytes: u64, mono_ms: u64) -> String {
    format!("N2_RING_ROTATED|segments={segments}|bytes={bytes}|mono_ms={mono_ms}|diag=true|is_evidence=false")
}

/// The rotation report ACTUALLY recorded in the ring (B4): names the seq range
/// that left the ring and the ACK watermark that allowed it.
#[allow(clippy::too_many_arguments)]
pub fn rotated_span_line(segments: u32, bytes: u64, dropped_from_seq: u64, dropped_to_seq: u64,
    dropped_records: u64, acked_seq: u64, mono_ms: u64) -> String {
    format!("N2_RING_ROTATED|segments={segments}|bytes={bytes}|dropped_from_seq={dropped_from_seq}|dropped_to_seq={dropped_to_seq}|dropped_records={dropped_records}|acked_seq={acked_seq}|mono_ms={mono_ms}|diag=true|is_evidence=false")
}

/// The records attempted but NOT persisted, with their seq range (B4).
pub fn lost_line(from_seq: u64, to_seq: u64, records: u64, reason: &str, code: &str,
    mono_ms: u64) -> String {
    format!("N2_RING_LOST|from_seq={from_seq}|to_seq={to_seq}|records={records}|reason={}|code={}|mono_ms={mono_ms}|diag=true|is_evidence=false",
        clean(reason), clean(code))
}

/// The capacity state: the ring stopped accepting and deleted NOTHING.
pub fn overflow_line(dropped_records: u64, from_seq: u64, to_seq: u64, acked_seq: u64,
    mono_ms: u64) -> String {
    format!("N2_RING_OVERFLOW|dropped_records={dropped_records}|from_seq={from_seq}|to_seq={to_seq}|acked_seq={acked_seq}|mono_ms={mono_ms}|diag=true|is_evidence=false")
}

/// Head/tail/failure/gap manifest (B4). `frozen_tail_seq` is the seq a pager
/// should freeze `to_seq` to. `pub(crate)` only through [`manifest_line_now`]:
/// the line shape is the public surface, `State` is not.
#[allow(clippy::too_many_arguments)]
fn manifest_line(file: &str, state: &State, mono_ms: u64) -> String {
    let (gaps, gap_from, gap_to) = gaps(state);
    format!("N2_RING_MANIFEST|file={}|sess={}|enabled={}|segments={}|max_bytes={}|bytes={}|records={}|first_seq={}|last_seq={}|frozen_tail_seq={}|next_seq={}|acked_seq={}|writes={}|failures={}|rotations={}|lost_records={}|lost_from={}|lost_to={}|lost_reason={}|dropped_by_ack={}|overflow={}|overflow_events={}|gaps={}|gap_from={}|gap_to={}|malformed={}|tail_open={}|last_error={}|mono_ms={mono_ms}|diag=true|is_evidence=false",
        clean(file), clean(&state.session), enabled(), state.segments, state.max_bytes,
        state.bytes, state.records_total, state.first_seq, state.last_seq, state.last_seq,
        state.seq_next, state.acked_seq, state.writes, state.failures, state.rotations,
        state.lost_records, state.lost_from, state.lost_to, clean(&state.lost_reason),
        state.dropped_by_ack, state.overflow, state.overflow_events, gaps, gap_from, gap_to,
        state.malformed, state.tail_open, clean(&state.last_error))
}

/// One re-emitted record: `line=` is the LAST field and holds the original text
/// verbatim. `seq` is the DURABLE seq, not a concatenated line index.
pub fn chunk_line(seq: u64, of: usize, line: &str) -> String {
    format!("N2_RING_CHUNK|seq={seq}|of={of}|line={line}")
}

/// The summary of one seq-paged dump.
pub fn dump_summary_line(page: &DumpPage, emitted: usize) -> String {
    format!("N2_RING_DUMP|from_seq={}|to_seq={}|count={}|emitted={emitted}|frozen_tail_seq={}|first_seq={}|last_seq={}|records={}|bytes={}|malformed={}|lost_records={}|overflow={}|acked_seq={}|result={}|reason={}|diag=true|is_evidence=false",
        page.from_seq, page.to_seq, page.count, page.frozen_tail_seq, page.first_seq,
        page.last_seq, page.records, page.bytes, page.malformed, page.lost_records,
        page.overflow, page.acked_seq, page.result, page.reason)
}

/// The current counters, as one `N2_RING_STATS` line the caller emits (so the
/// line itself lands in the ring through the tee).
pub fn status_line_now() -> String {
    let mono = sys::mono_ms();
    match state() {
        Some(state) => stats_line(&file_name(&state.path), enabled(), state.bytes, state.max_bytes,
            state.segments, state.writes, state.failures, state.rotations, &state.last_error, mono),
        None => stats_line("none", false, 0, 0, 0, 0, 0, 0, "none", mono),
    }
}

/// An errno-style code, never a path or file content (`code=<n>`), and never
/// the OS message text (which could echo the sandbox path).
fn error_code(error: &std::io::Error) -> String {
    match error.raw_os_error() {
        Some(errno) => format!("errno-{errno}"),
        None => "io".to_string(),
    }
}

/// Host-test only: the process-global ring state is shared by the whole test
/// binary, so every test that configures it serializes on the same lock the
/// other N2 modules use.
#[cfg(test)]
pub(crate) fn test_guard() -> MutexGuard<'static, ()> {
    crate::n2_stimulus::test_guard()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("n2-ring-{}-{}", tag, std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("temp dir");
            TempDir(dir)
        }

        fn file(&self, name: &str) -> PathBuf { self.0.join(name) }

        fn contents(&self, name: &str) -> String {
            fs::read_to_string(self.file(name)).unwrap_or_default()
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }

    fn ring_path(dir: &TempDir) -> String { dir.file("ring.log").to_string_lossy().into_owned() }

    /// The seq of the record whose `line=` equals `original`.
    fn seq_of(text: &str, original: &str) -> Option<u64> {
        text.lines().filter_map(parse_record)
            .find(|record| record.original == original)
            .map(|record| record.seq)
    }

    #[test]
    fn unconfigured_ring_is_silent_and_writes_nothing() {
        let _guard = test_guard();
        disable();
        let dir = TempDir::new("off");
        append("N2_TUN_DEST|dst=203.0.113.1|frames=1");
        assert!(!dir.file("ring.log").exists());
        assert!(!dir.file("ring.log.state").exists(), "no sidecar while unarmed");
        assert!(!enabled());
        // …and a status call still answers honestly (the counters of whatever
        // ring this process configured last are reported with enabled=false)
        let line = status_line_now();
        assert!(line.starts_with("N2_RING_STATS|file="), "{line}");
        assert!(line.contains("|enabled=false|"), "{line}");
        assert!(line.ends_with("|diag=true|is_evidence=false"), "{line}");
    }

    #[test]
    fn configure_validates_and_appends_durable_records() {
        let _guard = test_guard();
        disable();
        let dir = TempDir::new("append");
        assert_eq!(configure("", DEFAULT_MAX_BYTES, DEFAULT_SEGMENTS), Err("invalid-path"));
        assert_eq!(configure("no-slash", DEFAULT_MAX_BYTES, DEFAULT_SEGMENTS), Err("invalid-path"));
        assert_eq!(configure(&ring_path(&dir), 1024, DEFAULT_SEGMENTS), Err("invalid-max-bytes"));
        assert_eq!(configure(&ring_path(&dir), DEFAULT_MAX_BYTES, MAX_SEGMENTS + 1),
            Err("invalid-segments"));
        assert!(!enabled(), "a refused configure leaves the ring off");
        assert_eq!(configure_session(&ring_path(&dir), DEFAULT_MAX_BYTES, DEFAULT_SEGMENTS,
            Some("sess-A")), Ok(()));
        assert!(enabled());
        append("N2_TUN_DEST|scope=session|dst=203.0.113.1|frames=3");
        append("VPN_NETWORK_EVENT|event=netAvailable|netId=7|switched=true");
        let text = dir.contents("ring.log");
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].starts_with("N2R|sess=sess-A|seq=1|"), "{text}");
        assert!(lines[0].contains("|line=N2_RING_ARMED|file=ring.log|"), "{text}");
        let first = seq_of(&text, "N2_TUN_DEST|scope=session|dst=203.0.113.1|frames=3")
            .expect("the first append");
        let second = seq_of(&text, "VPN_NETWORK_EVENT|event=netAvailable|netId=7|switched=true")
            .expect("the second append");
        assert_eq!(first, 2, "{text}");
        assert_eq!(second, 3, "{text}");
        for line in &lines {
            assert!(line.starts_with("N2R|sess=sess-A|"), "{line}");
            assert!(line.contains("|mono_ms="), "{line}");
            assert!(line.contains("|wall_ms="), "{line}");
        }
        disable();
        assert!(!enabled());
    }

    #[test]
    fn ack_gated_rotation_never_deletes_an_unacknowledged_segment() {
        let _guard = test_guard();
        disable();
        let dir = TempDir::new("rotate");
        assert_eq!(configure(&ring_path(&dir), MIN_MAX_BYTES, 2), Ok(()));
        {
            // Tiny cap (after the armed record) so the rotation path is
            // exercised without writing 64 KiB: ~10 records per segment.
            let mut state = state().expect("state");
            state.max_bytes = 1_000;
        }
        for index in 0..40 {
            append(&format!("N2_FD_SEND|kind=Wb|fd=7|packets=0|bytes=0|seq={index}"));
        }
        // The first rotation is allowed (no older segment existed); the next is
        // blocked because `.1` holds data nobody acknowledged.
        assert!(dir.file("ring.log.1").exists(), "the first rotation happened");
        let older = dir.contents("ring.log.1");
        assert!(older.contains("|line=N2_FD_SEND|kind=Wb|fd=7|packets=0|bytes=0|seq=0"),
            "the oldest segment survives: {older}");
        let manifest = manifest_line_now();
        assert!(manifest.contains("|overflow=true|"), "{manifest}");
        assert!(!manifest.contains("|lost_records=0|"), "{manifest}");
        // ACK the retrieved watermark: the next rotation may now drop `.1`.
        let tail = state().expect("state").last_seq;
        let session = state().expect("state").session.clone();
        let acked = ack_seq(tail, &session).expect("the explicit session-bound ACK");
        assert_eq!((acked.acked_seq, acked.advanced), (tail, true));
        for index in 40..80 {
            append(&format!("N2_FD_SEND|kind=Wb|fd=7|packets=0|bytes=0|seq={index}"));
        }
        let manifest = manifest_line_now();
        assert!(!manifest.contains("|rotations=1|"), "{manifest}");
        assert!(!manifest.contains("|dropped_by_ack=0|"), "{manifest}");
        assert!(state().expect("state").last_seq > tail,
            "appends continue after the ACK: {manifest}");
        disable();
    }

    #[test]
    fn a_torn_tail_is_isolated_and_failures_are_reported_with_a_seq_span() {
        let _guard = test_guard();
        // The ring is process-global and every other test's `hilog::emit` tees
        // into whatever ring is configured. This test deliberately breaks the
        // destination, so its failure counters must only see ITS OWN appends:
        // hold the re-entrant tee lock (the same lock `hilog::emit` takes) for
        // the whole body — no concurrent emitter can append meanwhile, and this
        // thread's own nested emits do not deadlock.
        let _tee = crate::hilog::tee_lock::lock();
        disable();
        let dir = TempDir::new("fail");
        let nested = dir.file("nested");
        fs::create_dir_all(&nested).expect("nested dir");
        let path = nested.join("ring.log").to_string_lossy().into_owned();
        assert_eq!(configure(&path, DEFAULT_MAX_BYTES, DEFAULT_SEGMENTS), Ok(()));
        let armed_seq = state().expect("state").last_seq;
        // break the destination: the parent directory disappears, so every
        // open fails for real (no injected failure)
        fs::remove_dir_all(&nested).expect("remove parent");
        append("N2_TUN_DEST|dst=203.0.113.1");
        append("N2_TUN_DEST|dst=203.0.113.2");
        let (failures, lost, lost_from, recovered_pending) = {
            let state = state().expect("state");
            (state.failures, state.lost_records, state.lost_from, state.recovered_pending)
        };
        assert_eq!(failures, 2, "every failed append counts");
        assert_eq!(lost, 2, "every failed record consumes a seq");
        assert_eq!(lost_from, armed_seq + 1, "the span starts at the first lost seq");
        assert!(recovered_pending);
        // restore the destination: the next append succeeds and the lost span
        // plus the recovery are recorded IN the ring
        fs::create_dir_all(&nested).expect("restore parent");
        append("N2_TUN_DEST|dst=203.0.113.3");
        let text = fs::read_to_string(&path).unwrap_or_default();
        assert!(text.contains("N2_RING_LOST|from_seq="), "{text}");
        assert!(text.contains("|reason=write-failed|"), "{text}");
        assert!(text.contains("N2_RING_RECOVERED|failures=2|"), "{text}");
        let recovered_seq = seq_of(&text, "N2_TUN_DEST|dst=203.0.113.3").expect("recovered append");
        assert!(recovered_seq > lost_from + 1, "a failed seq is never reused: {recovered_seq}");
        assert_eq!(state().expect("state").failures, 2, "the counter only moves on failure");
        disable();
    }

    /// N2 C/D (2026-09-29, B4/T0 follow-up): `acked_seq` moves ONLY on an
    /// EXPLICIT, session-bound ACK request. The read paths never move it, a
    /// stale arm cannot move it, a repeat is idempotent, and an unarmed ring
    /// refuses the ACK the same way.
    #[test]
    fn only_an_explicit_session_bound_ack_moves_the_watermark() {
        let _guard = test_guard();
        // The ring is process-global and `hilog::emit` tees into whatever ring is
        // configured: hold the re-entrant tee lock for the whole body so no
        // OTHER test thread can append a record between this test's appends and
        // its seq assertions (the same discipline as the torn-tail test).
        let _tee = crate::hilog::tee_lock::lock();
        disable();
        let dir = TempDir::new("ack");
        assert_eq!(configure_session(&ring_path(&dir), DEFAULT_MAX_BYTES, DEFAULT_SEGMENTS,
            Some("arm-live")), Ok(()));
        append("N2_TUN_DEST|dst=203.0.113.1");
        append("N2_TUN_DEST|dst=203.0.113.2");
        let tail = state().expect("state").last_seq;
        assert_eq!(tail, 3, "armed + two appends");

        // 1. the READ paths never advance the watermark
        assert_eq!(dump_page("{\"from_seq\":1,\"count\":256}").acked_seq, 0);
        let _ = dump_lines("{\"from_seq\":1,\"count\":256}");
        let manifest = manifest_line_now();
        assert!(manifest.contains("|acked_seq=0|"), "{manifest}");
        let _ = status_line_now();
        assert_eq!(acked_seq(), 0, "reading the ring confirms nothing");

        // 2. the named refusals, each leaving the watermark untouched
        assert_eq!(parse_ack_request(""), Err("empty-request"));
        assert_eq!(parse_ack_request("   "), Err("empty-request"));
        assert_eq!(parse_ack_request("{session:1}"), Err("invalid-json"));
        assert_eq!(parse_ack_request("null"), Err("invalid-request"));
        assert_eq!(parse_ack_request("[1]"), Err("invalid-request"));
        assert_eq!(parse_ack_request(&format!("{{\"session\":\"{}\"}}", "x".repeat(600))),
            Err("request-too-long"));
        assert_eq!(parse_ack_request("{\"up_to_seq\":2}"), Err("invalid-session"));
        assert_eq!(parse_ack_request("{\"session\":\"\"}"), Err("invalid-session"));
        assert_eq!(parse_ack_request("{\"session\":3,\"up_to_seq\":2}"), Err("invalid-request"));
        assert_eq!(parse_ack_request("{\"session\":\"arm-live\"}"), Err("invalid-request"));
        assert_eq!(parse_ack_request("{\"session\":\"arm-live\",\"up_to_seq\":\"3\"}"),
            Err("invalid-request"));
        // (a negative / fractional / quoted watermark is not a number to the
        // shared JSON reader, exactly like the dump request's `from_seq`; the
        // shell refuses the SAME document earlier as `invalid-request`.)
        assert_eq!(parse_ack_request("{\"session\":\"arm-live\",\"up_to_seq\":-1}"),
            Err("invalid-json"));
        assert_eq!(parse_ack_request("{\"session\":\"arm-live\",\"up_to_seq\":3}"),
            Ok(AckRequest { session: "arm-live".into(), up_to_seq: 3 }));

        // a stale / expired arm, an invalid token and a future seq are refused
        assert_eq!(ack_seq(tail, "arm-dead"), Err("session-mismatch"));
        assert_eq!(ack_seq(tail, ""), Err("invalid-session"));
        assert_eq!(ack_seq(tail + 1, "arm-live"), Err("seq-beyond-tail"));
        assert_eq!(acked_seq(), 0, "every refusal left the watermark untouched");

        // 3. the explicit ACK advances it, and a repeat is idempotent
        let first = ack_seq(tail, "arm-live").expect("the explicit ACK");
        assert_eq!((first.acked_seq, first.previous_acked_seq, first.advanced), (tail, 0, true));
        assert_eq!(acked_seq(), tail);
        let again = ack_seq(tail, "arm-live").expect("a repeat is not an error");
        assert_eq!((again.acked_seq, again.previous_acked_seq, again.advanced), (tail, tail, false));
        let lower = ack_seq(1, "arm-live").expect("a lower watermark never rolls back");
        assert_eq!((lower.acked_seq, lower.previous_acked_seq, lower.advanced), (tail, tail, false));
        assert_eq!(acked_seq(), tail, "the watermark is monotonic");

        // 4. a re-arm makes the old token stale (its records are another arm's)
        assert_eq!(configure_session(&ring_path(&dir), DEFAULT_MAX_BYTES, DEFAULT_SEGMENTS,
            Some("arm-next")), Ok(()));
        assert_eq!(ack_seq(tail, "arm-live"), Err("session-mismatch"));
        assert_eq!(acked_seq(), tail, "the resumed arm keeps the persisted watermark");

        // 5. an unarmed ring refuses an ACK that outlived the session
        disable();
        assert_eq!(ack_seq(tail, "arm-next"), Err("not-armed"));
        assert_eq!(acked_seq(), tail);
        // an all-unconfigured ring (fresh process) answers not-configured
        // through its own empty state — covered by `unconfigured_ring_*` above.
    }

    #[test]
    fn formatters_match_the_documented_line_shapes() {
        let _guard = test_guard();
        assert_eq!(
            armed_line(DEFAULT_FILE_NAME, DEFAULT_MAX_BYTES, DEFAULT_SEGMENTS, 0, 1_000),
            "N2_RING_ARMED|file=netbird-n2-evidence-ring.log|max_bytes=4194304|segments=3|bytes=0|mono_ms=1000|diag=true|is_evidence=false");
        assert_eq!(
            stats_line("netbird-n2-evidence-ring.log", true, 4096, DEFAULT_MAX_BYTES,
                DEFAULT_SEGMENTS, 12, 3, 1, "errno-28", 2_000),
            "N2_RING_STATS|file=netbird-n2-evidence-ring.log|enabled=true|bytes=4096|max_bytes=4194304|segments=3|writes=12|failures=3|rotations=1|last_error=errno-28|mono_ms=2000|diag=true|is_evidence=false");
        assert_eq!(fail_line(1, "errno-28", 1_500),
            "N2_RING_WRITE_FAILED|failures=1|code=errno-28|mono_ms=1500|diag=true|is_evidence=false");
        assert_eq!(recovered_line(2, 2_500),
            "N2_RING_RECOVERED|failures=2|mono_ms=2500|diag=true|is_evidence=false");
        assert_eq!(rotated_line(3, 4_194_304, 3_000),
            "N2_RING_ROTATED|segments=3|bytes=4194304|mono_ms=3000|diag=true|is_evidence=false");
        assert_eq!(rotated_span_line(3, 4_194_304, 11, 40, 30, 40, 3_000),
            "N2_RING_ROTATED|segments=3|bytes=4194304|dropped_from_seq=11|dropped_to_seq=40|dropped_records=30|acked_seq=40|mono_ms=3000|diag=true|is_evidence=false");
        assert_eq!(lost_line(5, 6, 2, "write-failed", "errno-28", 1_500),
            "N2_RING_LOST|from_seq=5|to_seq=6|records=2|reason=write-failed|code=errno-28|mono_ms=1500|diag=true|is_evidence=false");
        assert_eq!(overflow_line(7, 5, 11, 4, 1_500),
            "N2_RING_OVERFLOW|dropped_records=7|from_seq=5|to_seq=11|acked_seq=4|mono_ms=1500|diag=true|is_evidence=false");
        assert_eq!(encode_record(9, "sess-A", 1_000, 1_700_000_000_000,
            "N2_TUN_DEST|dst=203.0.113.1"),
            "N2R|sess=sess-A|seq=9|mono_ms=1000|wall_ms=1700000000000|line=N2_TUN_DEST|dst=203.0.113.1");
        let parsed = parse_record(&encode_record(9, "sess-A", 1_000, 1_700_000_000_000,
            "N2_TUN_DEST|dst=203.0.113.1")).expect("record parses");
        assert_eq!((parsed.seq, parsed.session.as_str(), parsed.original.as_str()),
            (9, "sess-A", "N2_TUN_DEST|dst=203.0.113.1"));
    }
}
