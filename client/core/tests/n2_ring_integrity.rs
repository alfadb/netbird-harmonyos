// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N2 first batch C/D columns — the app-internal evidence ring's **integrity
//! contract** (B4 of the N2-CD-RING-T0-REVIEW, 2026-09-29). Host only: no
//! device, no network, no SSH, no build; all state lives under a temp dir.
//!
//! What this file proves, one requirement per test:
//!
//! 1. every persisted record carries a durable `seq`, the arm `sess`, BOTH
//!    clocks (`mono_ms`/`wall_ms`) and the ORIGINAL line verbatim;
//! 2. `seq` continues across disable/re-arm and across a process restart — it
//!    is never reused — and different sessions are isolated: a new arm appends
//!    with its own `sess`, it never re-tags or rewrites the old records;
//! 3. capacity is bounded and audible: an unacknowledged ring OVERFLOWS
//!    (stops accepting, counts the lost seq range) and deletes NOTHING;
//! 4. rotation is ACK-gated and, when it runs, keeps the shifted older
//!    segments rather than wiping them;
//! 5. a write failure consumes its seq, is reported IN the ring with its seq
//!    range (`N2_RING_LOST` + `N2_RING_RECOVERED`), and a torn tail from a
//!    crash is isolated on its own line and its seq recovered;
//! 6. a missing segment is reported as a gap in the manifest while the
//!    surviving segments still serve dumps;
//! 7. dump pagination is by durable `seq` with a FROZEN tail seq: concurrent
//!    appends cannot move an already-read page;
//! 8. the legacy line-index request (`{"start":…}`) is EXPLICITLY refused, not
//!    silently served against a different domain;
//! 9. unarmed, the ring creates no file at all.
//!
//! Everything is diagnostic: every line ends `|diag=true|is_evidence=false`,
//! and nothing here is N2/N6 evidence.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use netbird_core::n2_ring;

// Host-process link stubs (repo convention, cf. tests/n2_log_samples.rs):
// libace_napi.z.so / libhilog_ndk.z.so do not exist on the host.
mod host_link_stubs {
    use core::ffi::c_void;

    #[no_mangle]
    pub extern "C" fn OH_LOG_Print(
        _log_type: i32,
        _level: i32,
        _domain: u32,
        _tag: *const u8,
        _fmt: *const u8,
        _arg: *const c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn OH_LOG_IsLoggable(_domain: u32, _tag: *const u8, _level: i32) -> bool {
        false
    }

    #[no_mangle]
    pub extern "C" fn napi_module_register(_mod_: *mut c_void) {}

    #[no_mangle]
    pub extern "C" fn napi_create_function(
        _env: *mut c_void,
        _utf8name: *const u8,
        _length: usize,
        _cb: *const u8,
        _data: *mut c_void,
        _result: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_set_named_property(
        _env: *mut c_void,
        _name: *const c_void,
        _value: *const c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _str_: *const u8,
        _len: usize,
        _result: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_cb_info(
        _env: *mut c_void,
        _argc: *mut usize,
        _argv: *mut *mut c_void,
        _data: *mut c_void,
        _result: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_string_utf8(
        _value: *mut c_void,
        _buf: *mut u8,
        _bufsize: usize,
        _result: *mut usize,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_int32(_value: *mut c_void, _result: *mut i32) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(_value: *mut c_void, _result: *mut bool) -> i32 {
        0
    }
}

/// The smallest accepted cap (`n2_ring::MIN_MAX_BYTES` is private): enough to
/// exercise rotation/overflow without writing tens of MiB.
const SMALL_CAP: u64 = 64 * 1024;

// The ring state is process-global inside this test binary: every test that
// configures it serializes on one lock (same discipline as tests/n2_cd.rs).
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("n2-ring-int-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        TempDir(dir)
    }

    fn file(&self, name: &str) -> PathBuf { self.0.join(name) }

    fn text(&self, name: &str) -> String {
        fs::read_to_string(self.file(name)).unwrap_or_default()
    }

    /// The kept SEGMENT files only (the sidecar is metadata, not a segment).
    fn segments(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        if let Ok(entries) = fs::read_dir(&self.0) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.contains(".state") {
                    out.insert(name);
                }
            }
        }
        out
    }
}

impl Drop for TempDir {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

fn ring_path(dir: &TempDir) -> String { dir.file("ring.log").to_string_lossy().into_owned() }

/// One distinguishable record payload (large enough to fill the small cap in a
/// few hundred appends, small enough to stay fast).
fn payload(index: usize) -> String {
    format!("N2_TUN_DEST|scope=session|dst=203.0.113.{}|frames={index}|pad={}",
        index % 256, "x".repeat(400))
}

// ---------------------------------------------------------------------------
// record / manifest readers (the test's own, independent of the core's parser)
// ---------------------------------------------------------------------------

/// The head of a record, i.e. everything before the `|line=` field.
fn head_of(line: &str) -> &str { line.split("|line=").next().unwrap_or(line) }

fn head_field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    head_of(line).split('|').find_map(|field| field.strip_prefix(key))
}

/// The original line of a record (`|line=` is the LAST field).
fn payload_of(line: &str) -> &str {
    match line.find("|line=") {
        Some(index) => &line[index + "|line=".len()..],
        None => "",
    }
}

/// `(seq, session, original)` for every parseable record in one file.
fn records_of(path: &Path) -> Vec<(u64, String, String)> {
    let text = fs::read_to_string(path).unwrap_or_default();
    text.lines().filter_map(|line| {
        if !line.starts_with("N2R|") {
            return None;
        }
        let seq = head_field(line, "seq=")?.parse().ok()?;
        let session = head_field(line, "sess=").unwrap_or("").to_string();
        Some((seq, session, payload_of(line).to_string()))
    }).collect()
}

/// Every record across all kept segments, in file order.
fn all_records(dir: &TempDir) -> Vec<(u64, String, String)> {
    let mut out = Vec::new();
    for index in (0..3u32).rev() {
        let name = if index == 0 { "ring.log".to_string() } else { format!("ring.log.{index}") };
        out.extend(records_of(&dir.file(&name)));
    }
    out
}

fn manifest_field<'a>(manifest: &'a str, key: &str) -> &'a str {
    manifest.split('|').find_map(|field| field.strip_prefix(key)).unwrap_or("")
}

fn manifest_u64(manifest: &str, key: &str) -> u64 {
    manifest_field(manifest, key).parse().unwrap_or(u64::MAX)
}

fn manifest_bool(manifest: &str, key: &str) -> bool {
    manifest_field(manifest, key) == "true"
}

// ---------------------------------------------------------------------------
// 9. unarmed
// ---------------------------------------------------------------------------

#[test]
fn unarmed_ring_creates_no_file_at_all() {
    let _guard = serial();
    n2_ring::disable();
    let dir = TempDir::new("unarmed");
    n2_ring::append("N2_TUN_DEST|dst=203.0.113.1");
    assert!(!dir.file("ring.log").exists(), "no segment while unarmed");
    assert!(!dir.file("ring.log.state").exists(), "no sidecar while unarmed");
    assert!(!n2_ring::enabled());
}

// ---------------------------------------------------------------------------
// 1. the durable record
// ---------------------------------------------------------------------------

#[test]
fn records_carry_persistent_seq_session_both_clocks_and_the_original_line() {
    let _guard = serial();
    n2_ring::disable();
    let dir = TempDir::new("record");
    let path = ring_path(&dir);
    assert_eq!(n2_ring::configure_session(&path, n2_ring::DEFAULT_MAX_BYTES,
        n2_ring::DEFAULT_SEGMENTS, Some("arm-1")), Ok(()));
    n2_ring::append("N2_TUN_DEST|scope=session|dst=203.0.113.9|frames=3");
    n2_ring::append("VPN_NETWORK_EVENT|event=netAvailable|netId=7|switched=true");

    let text = dir.text("ring.log");
    let records = records_of(&dir.file("ring.log"));
    assert_eq!(records.len(), 3, "armed + two appends: {text}");
    assert_eq!(records[0].1, "arm-1", "the arm token is on every record: {text}");
    assert!(records[0].2.starts_with("N2_RING_ARMED|"), "{text}");
    assert_eq!(records[1].0, records[0].0 + 1, "seq increments per record: {text}");
    assert_eq!(records[2].0, records[1].0 + 1, "{text}");
    assert_eq!(records[1].2, "N2_TUN_DEST|scope=session|dst=203.0.113.9|frames=3",
        "the original line is verbatim: {text}");

    // Both clocks are on the record, and wall_ms is a plausible epoch ms.
    let sample = text.lines().find(|line| payload_of(line).starts_with("VPN_NETWORK_EVENT|"))
        .expect("the forwarded line");
    let mono: u64 = head_field(sample, "mono_ms=").expect("mono_ms").parse().expect("mono ms");
    let wall: u128 = head_field(sample, "wall_ms=").expect("wall_ms").parse().expect("wall ms");
    assert!(mono > 0, "{sample}");
    assert!(wall > 1_600_000_000_000, "wall_ms is an epoch reading: {sample}");
    n2_ring::disable();
}

// ---------------------------------------------------------------------------
// 2. restart continuation + session isolation
// ---------------------------------------------------------------------------

#[test]
fn seq_survives_restarts_is_never_reused_and_sessions_are_isolated() {
    let _guard = serial();
    n2_ring::disable();
    let dir = TempDir::new("restart");
    let path = ring_path(&dir);
    assert_eq!(n2_ring::configure_session(&path, n2_ring::DEFAULT_MAX_BYTES,
        n2_ring::DEFAULT_SEGMENTS, Some("arm-1")), Ok(()));
    n2_ring::append("L1");
    n2_ring::disable();

    // process restart: a re-arm continues the seq and appends with its own sess
    assert_eq!(n2_ring::configure_session(&path, n2_ring::DEFAULT_MAX_BYTES,
        n2_ring::DEFAULT_SEGMENTS, Some("arm-2")), Ok(()));
    n2_ring::append("L2");
    n2_ring::disable();
    // …and a third arm with the SAME token as arm-2 resumes without a reset
    assert_eq!(n2_ring::configure_session(&path, n2_ring::DEFAULT_MAX_BYTES,
        n2_ring::DEFAULT_SEGMENTS, Some("arm-2")), Ok(()));
    n2_ring::append("L3");

    let records = all_records(&dir);
    let l1 = records.iter().find(|record| record.2 == "L1").expect("L1 survives");
    let l2 = records.iter().find(|record| record.2 == "L2").expect("L2 survives");
    let l3 = records.iter().find(|record| record.2 == "L3").expect("L3 survives");
    assert_eq!(l1.1, "arm-1", "the old record keeps its own session");
    assert_eq!(l2.1, "arm-2");
    assert_eq!(l3.1, "arm-2");
    assert!(l2.0 > l1.0 && l3.0 > l2.0, "seq is monotonic across restarts");
    // no seq is reused anywhere in the ring
    let mut seqs: Vec<u64> = records.iter().map(|record| record.0).collect();
    let count = seqs.len();
    seqs.sort_unstable();
    seqs.dedup();
    assert_eq!(seqs.len(), count, "every seq is unique");
    // the sidecar carried the continuation watermark
    let sidecar = dir.text("ring.log.state");
    assert!(sidecar.contains("next_seq="), "{sidecar}");
    assert!(sidecar.contains("session=arm-2"), "{sidecar}");
    n2_ring::disable();
}

// ---------------------------------------------------------------------------
// 3/4. capacity: overflow is audible, nothing is deleted without an ACK
// ---------------------------------------------------------------------------

#[test]
fn capacity_overflow_is_detectable_and_deletes_nothing_without_an_ack() {
    let _guard = serial();
    n2_ring::disable();
    let dir = TempDir::new("overflow");
    let path = ring_path(&dir);
    assert_eq!(n2_ring::configure_session(&path, SMALL_CAP, 2, Some("arm-ovf")), Ok(()));
    for index in 0..800 {
        n2_ring::append(&payload(index));
    }
    let manifest = n2_ring::manifest_line_now();
    assert!(manifest_bool(&manifest, "overflow="), "overflow is audible: {manifest}");
    assert!(manifest_u64(&manifest, "lost_records=") > 0, "{manifest}");
    assert!(manifest_u64(&manifest, "lost_from=") > 0, "{manifest}");
    assert!(manifest_u64(&manifest, "acked_seq=") == 0, "nothing was acked: {manifest}");
    // no `.2`: segments=2 keeps exactly two files, and the unacked `.1` survived
    assert!(dir.file("ring.log.1").exists(), "the unacked older segment survives");
    assert!(!dir.file("ring.log.2").exists());
    assert!(dir.text("ring.log.1").contains("|line=N2_TUN_DEST|scope=session|dst=203.0.113.0|"),
        "the oldest payload is still there");

    // further appends while overflowing neither delete nor create a segment
    let before = dir.segments();
    for index in 800..900 {
        n2_ring::append(&payload(index));
    }
    let after = dir.segments();
    assert_eq!(before, after, "an overflowing ring does not touch its segments");
    let manifest = n2_ring::manifest_line_now();
    assert!(manifest_bool(&manifest, "overflow="), "{manifest}");
    assert!(manifest_u64(&manifest, "acked_seq=") == 0, "{manifest}");

    // a STALE arm's ACK is refused and releases nothing: the un-ACKed `.1`
    // survives and the ring stays in overflow
    let stale_tail = manifest_u64(&manifest, "last_seq=");
    assert_eq!(n2_ring::ack_seq(stale_tail, "arm-dead"), Err("session-mismatch"));
    assert_eq!(n2_ring::acked_seq(), 0, "a stale session confirmed nothing");
    n2_ring::append(&payload(900));
    assert_eq!(dir.segments(), before, "a stale ACK released no segment");
    assert!(n2_ring::manifest_line_now().contains("|overflow=true|"));

    // ACK-gated rotation: only now may the retrieved `.1` be dropped
    let tail = manifest_u64(&n2_ring::manifest_line_now(), "last_seq=");
    let acked = n2_ring::ack_seq(tail, "arm-ovf").expect("ack the retrieved tail");
    assert_eq!(acked.acked_seq, tail);
    n2_ring::append(&payload(901));
    let manifest = n2_ring::manifest_line_now();
    assert!(manifest_u64(&manifest, "rotations=") >= 2, "rotation resumed: {manifest}");
    assert!(!manifest_bool(&manifest, "overflow="), "the overflow episode closed: {manifest}");
    assert!(manifest_u64(&manifest, "dropped_by_ack=") > 0, "{manifest}");
    n2_ring::disable();
}

#[test]
fn rotation_keeps_the_shifted_older_segments() {
    let _guard = serial();
    n2_ring::disable();
    let dir = TempDir::new("rotation");
    let path = ring_path(&dir);
    assert_eq!(n2_ring::configure_session(&path, SMALL_CAP, 3, Some("arm-rot")), Ok(()));
    for index in 0..300 {
        n2_ring::append(&payload(index));
    }
    let manifest = n2_ring::manifest_line_now();
    assert!(manifest_u64(&manifest, "rotations=") >= 2, "at least two rotations: {manifest}");
    assert!(dir.file("ring.log.1").exists(), "the shifted segment survives");
    assert!(dir.file("ring.log.2").exists(), "the oldest shifted segment survives");
    assert!(!dir.file("ring.log.3").exists(), "segments=3 keeps exactly three files");
    // the shifted segments still hold the EARLIER records
    let older = records_of(&dir.file("ring.log.2"));
    let newer = records_of(&dir.file("ring.log.1"));
    let current = records_of(&dir.file("ring.log"));
    assert!(!older.is_empty() && !newer.is_empty() && !current.is_empty());
    assert!(older.iter().all(|record| record.0 < newer[0].0),
        "segment order is chronological (oldest first)");
    assert!(newer.iter().all(|record| record.0 < current[0].0));
    n2_ring::disable();
}

// ---------------------------------------------------------------------------
// 5. write failure + torn tail
// ---------------------------------------------------------------------------

#[test]
fn write_failures_consume_their_seq_and_are_reported_in_the_ring() {
    let _guard = serial();
    n2_ring::disable();
    let dir = TempDir::new("wfail");
    let nested = dir.file("nested");
    fs::create_dir_all(&nested).expect("nested dir");
    let path = nested.join("ring.log").to_string_lossy().into_owned();
    assert_eq!(n2_ring::configure_session(&path, n2_ring::DEFAULT_MAX_BYTES,
        n2_ring::DEFAULT_SEGMENTS, Some("arm-fail")), Ok(()));
    let armed_seq = manifest_u64(&n2_ring::manifest_line_now(), "last_seq=");
    // break the destination for real: the parent directory disappears
    fs::remove_dir_all(&nested).expect("remove parent");
    n2_ring::append("N2_TUN_DEST|dst=203.0.113.1");
    n2_ring::append("N2_TUN_DEST|dst=203.0.113.2");
    let manifest = n2_ring::manifest_line_now();
    assert_eq!(manifest_u64(&manifest, "failures="), 2, "{manifest}");
    assert_eq!(manifest_u64(&manifest, "lost_records="), 2, "{manifest}");
    assert_eq!(manifest_u64(&manifest, "lost_from="), armed_seq + 1, "{manifest}");
    assert_eq!(manifest_u64(&manifest, "lost_to="), armed_seq + 2, "{manifest}");
    assert_eq!(manifest_field(&manifest, "lost_reason="), "write-failed", "{manifest}");
    // restore: the recovery and the lost span are recorded IN the ring
    fs::create_dir_all(&nested).expect("restore parent");
    n2_ring::append("N2_TUN_DEST|dst=203.0.113.3");
    let text = fs::read_to_string(&path).unwrap_or_default();
    assert!(text.contains("|line=N2_RING_LOST|from_seq="), "{text}");
    assert!(text.contains("|records=2|reason=write-failed|"), "{text}");
    assert!(text.contains("|line=N2_RING_RECOVERED|failures=2|"), "{text}");
    let recovered = records_of(Path::new(&path))
        .into_iter().find(|record| record.2 == "N2_TUN_DEST|dst=203.0.113.3")
        .expect("the recovered append");
    assert!(recovered.0 > armed_seq + 2, "a failed seq is never reused: {text}");
    let manifest = n2_ring::manifest_line_now();
    assert_eq!(manifest_u64(&manifest, "lost_from="), 0, "the span was reported: {manifest}");
    assert_eq!(manifest_u64(&manifest, "lost_records="), 2, "the total stays honest: {manifest}");
    n2_ring::disable();
}

#[test]
fn a_torn_tail_recovers_its_seq_and_stays_on_its_own_line() {
    let _guard = serial();
    n2_ring::disable();
    let dir = TempDir::new("torn");
    let path = dir.file("ring.log");
    // simulate a crash in the middle of a record: no trailing newline
    fs::write(&path, b"N2R|sess=arm-old|seq=1|mono_ms=5|wall_ms=6|line=N2_TUN_DEST|dst=203").unwrap();
    let path_string = path.to_string_lossy().into_owned();
    assert_eq!(n2_ring::configure_session(&path_string, n2_ring::DEFAULT_MAX_BYTES,
        n2_ring::DEFAULT_SEGMENTS, Some("arm-new")), Ok(()));
    // the torn record's seq was recovered, so the next seq does not reuse it
    let manifest = n2_ring::manifest_line_now();
    assert!(manifest_u64(&manifest, "next_seq=") > 1, "{manifest}");
    n2_ring::append("N2_TUN_DEST|dst=203.0.113.2");
    let text = dir.text("ring.log");
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].starts_with("N2R|sess=arm-old|seq=1|"), "the torn bytes stay put: {text}");
    assert!(lines.len() >= 3, "the torn record was separated: {text}");
    assert!(lines[1].starts_with("N2R|sess=arm-new|"), "the armed record is its own line: {text}");
    assert!(lines.iter().any(|line| payload_of(line) == "N2_TUN_DEST|dst=203.0.113.2"),
        "{text}");
    n2_ring::disable();
}

// ---------------------------------------------------------------------------
// 6. missing segment detection
// ---------------------------------------------------------------------------

#[test]
fn a_missing_segment_is_reported_as_a_gap_and_the_rest_still_serves_dumps() {
    let _guard = serial();
    n2_ring::disable();
    let dir = TempDir::new("gap");
    let path = ring_path(&dir);
    assert_eq!(n2_ring::configure_session(&path, SMALL_CAP, 3, Some("arm-gap")), Ok(()));
    for index in 0..800 {
        n2_ring::append(&payload(index));
    }
    assert!(dir.file("ring.log.1").exists() && dir.file("ring.log.2").exists());
    // the middle segment disappears (disk loss / operator error)
    fs::remove_file(dir.file("ring.log.1")).expect("remove middle segment");
    let manifest = n2_ring::manifest_line_now();
    assert_eq!(manifest_u64(&manifest, "gaps="), 1, "the missing segment is a gap: {manifest}");
    assert!(manifest_u64(&manifest, "gap_from=") > 0, "{manifest}");
    assert!(manifest_u64(&manifest, "gap_to=") > 0, "{manifest}");
    // the surviving segments still serve a dump
    let page = n2_ring::dump_page("{\"from_seq\":1,\"count\":4}");
    assert_eq!((page.result, page.reason), ("dumped", "ok"));
    assert_eq!(page.lines.len(), 4, "the oldest records still come from .2");
    assert_eq!(page.lines[0].0, 1);
    n2_ring::disable();
}

// ---------------------------------------------------------------------------
// 7. seq pagination is stable under concurrent appends
// ---------------------------------------------------------------------------

#[test]
fn paged_dump_by_seq_is_stable_while_the_ring_is_appended_to() {
    let _guard = serial();
    n2_ring::disable();
    let dir = TempDir::new("paging");
    let path = ring_path(&dir);
    assert_eq!(n2_ring::configure_session(&path, n2_ring::DEFAULT_MAX_BYTES,
        n2_ring::DEFAULT_SEGMENTS, Some("arm-page")), Ok(()));
    for index in 0..50 {
        n2_ring::append(&payload(index));
    }
    let tail = manifest_u64(&n2_ring::manifest_line_now(), "last_seq=");
    let frozen_request = format!("{{\"from_seq\":1,\"to_seq\":{tail},\"count\":256}}");
    let frozen = n2_ring::dump_page(&frozen_request);
    assert_eq!((frozen.result, frozen.reason), ("dumped", "ok"));
    assert_eq!(frozen.frozen_tail_seq, tail);
    let expected: Vec<u64> = frozen.lines.iter().map(|(seq, _)| *seq).collect();
    assert_eq!(expected.len() as u64, tail, "the whole frozen window: seq 1..={tail}");

    // a writer keeps appending while the host re-reads the same frozen page
    let writer = std::thread::spawn(move || {
        for index in 50..400 {
            n2_ring::append(&payload(index));
        }
    });
    for round in 0..20 {
        let page = n2_ring::dump_page(&frozen_request);
        assert_eq!((page.result, page.reason), ("dumped", "ok"), "round {round}");
        let seqs: Vec<u64> = page.lines.iter().map(|(seq, _)| *seq).collect();
        assert_eq!(seqs, expected, "a frozen seq window never moves (round {round})");
    }
    writer.join().expect("writer thread");

    // the ring kept accepting after the page was frozen, and the frozen page
    // still serves exactly the same records
    let manifest = n2_ring::manifest_line_now();
    assert!(manifest_u64(&manifest, "last_seq=") > tail, "{manifest}");
    let again = n2_ring::dump_page(&frozen_request);
    let seqs: Vec<u64> = again.lines.iter().map(|(seq, _)| *seq).collect();
    assert_eq!(seqs, expected, "appends did not disturb the already-read window");
    n2_ring::disable();
}

// ---------------------------------------------------------------------------
// 8. legacy request refusal + the modern request shape
// ---------------------------------------------------------------------------

#[test]
fn legacy_line_index_pagination_is_explicitly_refused() {
    let _guard = serial();
    n2_ring::disable();
    let dir = TempDir::new("legacy");
    let path = ring_path(&dir);
    assert_eq!(n2_ring::configure_session(&path, n2_ring::DEFAULT_MAX_BYTES,
        n2_ring::DEFAULT_SEGMENTS, Some("arm-legacy")), Ok(()));
    n2_ring::append("N2_TUN_DEST|dst=203.0.113.1");

    // a line index over concatenated segments has no safe mapping onto durable
    // seqs: it is refused by name, never silently served
    assert_eq!(n2_ring::parse_dump_request("{\"start\":0,\"count\":8}"),
        Err("legacy-start-index-unsupported"));
    assert_eq!(n2_ring::parse_dump_request("{\"start\":12}"),
        Err("legacy-start-index-unsupported"));
    let refused = n2_ring::dump_page("{\"start\":0}");
    assert_eq!((refused.result, refused.reason),
        ("rejected", "legacy-start-index-unsupported"));
    assert!(refused.lines.is_empty());

    // the seq-paged form (the frozen tail seq comes from the manifest)
    let request = n2_ring::parse_dump_request("{\"from_seq\":1,\"to_seq\":2,\"count\":2}")
        .expect("seq request");
    assert_eq!(request, n2_ring::DumpRequest {
        from_seq: 1, to_seq: Some(2), count: 2, session: None,
    });
    let default = n2_ring::parse_dump_request("{}").expect("defaults");
    assert_eq!(default.count, n2_ring::DEFAULT_DUMP_COUNT);
    // a session filter isolates one arm's records
    let filtered = n2_ring::dump_page("{\"from_seq\":1,\"count\":256,\"session\":\"arm-legacy\"}");
    assert_eq!(filtered.lines.len(), 2);
    let other = n2_ring::dump_page("{\"from_seq\":1,\"count\":256,\"session\":\"arm-other\"}");
    assert!(other.lines.is_empty(), "{:?}", other.lines);

    // chunk + summary shapes carry the DURABLE seq and the manifest facts
    let lines = n2_ring::dump_lines("{\"from_seq\":1,\"count\":256}");
    assert!(lines[0].starts_with("N2_RING_CHUNK|seq=1|of=256|line=N2_RING_ARMED|"), "{}",
        lines[0]);
    let summary = lines.last().expect("summary");
    assert!(summary.starts_with("N2_RING_DUMP|from_seq=1|to_seq=2|count=256|emitted=2|"), "{summary}");
    assert!(summary.contains("|result=dumped|reason=ok|diag=true|is_evidence=false"), "{summary}");
    n2_ring::disable();
}

// ---------------------------------------------------------------------------
// 10. the RETRIEVAL bridges (connector JSON) serve exactly those formatters
// ---------------------------------------------------------------------------

#[test]
fn the_retrieval_bridges_serialize_the_core_formatters_verbatim() {
    let _guard = serial();
    n2_ring::disable();
    let dir = TempDir::new("bridge");
    let path = ring_path(&dir);
    assert_eq!(n2_ring::configure_session(&path, n2_ring::DEFAULT_MAX_BYTES,
        n2_ring::DEFAULT_SEGMENTS, Some("arm-bridge")), Ok(()));
    // one ordinary line and one that needs JSON escaping (`"` and `\`)
    n2_ring::append("N2_TUN_DEST|dst=203.0.113.1|frames=1");
    n2_ring::append("VPN_NETWORK_EVENT|ssid=\"cafe\"|path=C:\\x");

    // the seq-paged dump: the SAME chunk lines and the SAME summary the core's
    // own `dump_lines` produces, plus the machine-readable page facts
    let request = "{\"from_seq\":1,\"count\":256}";
    let page = n2_ring::dump_page(request);
    let emitted = page.lines.len();
    let expected_lines: Vec<String> = page.lines.iter()
        .map(|(seq, line)| n2_ring::chunk_line(*seq, page.count, line))
        .collect();
    let body = expected_lines.iter()
        .map(|line| format!("\"{}\"", netbird_core::util::json_escape(line)))
        .collect::<Vec<_>>()
        .join(",");
    let dump = netbird_core::connector::n2_ring_dump_json(request);
    assert!(dump.contains("\"result\":\"dumped\""), "{dump}");
    assert!(dump.contains("\"reason\":\"ok\""), "{dump}");
    assert!(dump.contains(&format!("\"from_seq\":{}", page.from_seq)), "{dump}");
    assert!(dump.contains(&format!("\"count\":{}", page.count)), "{dump}");
    assert!(dump.contains(&format!("\"emitted\":{emitted}")), "{dump}");
    assert!(dump.contains(&format!("\"frozen_tail_seq\":{}", page.frozen_tail_seq)), "{dump}");
    assert!(dump.contains(&format!("\"lines\":[{body}]")), "the chunk lines verbatim: {dump}");
    let summary = n2_ring::dump_summary_line(&page, emitted);
    assert!(dump.contains(&format!("\"summary\":\"{}\"", netbird_core::util::json_escape(&summary))),
        "the summary is the core's own: {dump}");
    assert!(dump.contains("\\\"cafe\\\""), "JSON escaping survives: {dump}");

    // the legacy line-index request is refused by name (never silently served)
    let legacy = netbird_core::connector::n2_ring_dump_json("{\"start\":0}");
    assert!(legacy.contains("\"ok\":false"), "{legacy}");
    assert!(legacy.contains("\"reason\":\"legacy-start-index-unsupported\""), "{legacy}");
    assert!(legacy.contains("\"lines\":[]"), "{legacy}");

    // the manifest bridge answers the core's own line
    let manifest = netbird_core::connector::n2_ring_manifest_json();
    assert!(manifest.contains("\"ok\":true"), "{manifest}");
    assert!(manifest.contains("\"line\":\"N2_RING_MANIFEST|file=ring.log|"), "{manifest}");
    assert!(manifest.contains("|first_seq=1|last_seq=3|"), "{manifest}");

    // NOTHING the read paths answer moves the retrieved watermark: the dump
    // reports it, the manifest reports it, and both leave it where it is
    assert!(dump.contains("\"acked_seq\":0"), "{dump}");
    assert!(manifest.contains("|acked_seq=0|"), "{manifest}");
    let _ = netbird_core::connector::n2_ring_status_json();
    assert_eq!(n2_ring::acked_seq(), 0, "a read confirms nothing");

    // the ACK bridge: an EXPLICIT, session-bound request document is the only
    // thing that moves the watermark; a stale arm / a future seq are refused
    let last_seq = manifest_u64(&n2_ring::manifest_line_now(), "last_seq=");
    let wrong_arm = netbird_core::connector::n2_ring_ack_json(
        &format!("{{\"session\":\"arm-other\",\"up_to_seq\":{last_seq}}}"));
    assert!(wrong_arm.contains("\"ok\":false"), "{wrong_arm}");
    assert!(wrong_arm.contains("\"reason\":\"session-mismatch\""), "{wrong_arm}");
    assert!(wrong_arm.contains("\"advanced\":false"), "{wrong_arm}");
    assert_eq!(n2_ring::acked_seq(), 0, "a stale arm released nothing");
    let beyond = netbird_core::connector::n2_ring_ack_json(
        &format!("{{\"session\":\"arm-bridge\",\"up_to_seq\":{}}}", last_seq + 10));
    assert!(beyond.contains("\"ok\":false"), "{beyond}");
    assert!(beyond.contains("\"reason\":\"seq-beyond-tail\""), "{beyond}");
    assert_eq!(n2_ring::acked_seq(), 0);
    let acked = netbird_core::connector::n2_ring_ack_json(
        &format!("{{\"session\":\"arm-bridge\",\"up_to_seq\":{last_seq}}}"));
    assert!(acked.contains("\"ok\":true"), "{acked}");
    assert!(acked.contains("\"result\":\"acked\""), "{acked}");
    assert!(acked.contains(&format!("\"acked_seq\":{last_seq}")), "{acked}");
    assert!(acked.contains("\"advanced\":true"), "{acked}");
    assert_eq!(n2_ring::acked_seq(), last_seq);
    let repeat = netbird_core::connector::n2_ring_ack_json(
        &format!("{{\"session\":\"arm-bridge\",\"up_to_seq\":{last_seq}}}"));
    assert!(repeat.contains("\"ok\":true"), "{repeat}");
    assert!(repeat.contains("\"advanced\":false"), "a repeat is idempotent: {repeat}");
    n2_ring::disable();
    // an ACK that outlives the session is refused: the watermark gates a writer
    let late = netbird_core::connector::n2_ring_ack_json(
        &format!("{{\"session\":\"arm-bridge\",\"up_to_seq\":{last_seq}}}"));
    assert!(late.contains("\"reason\":\"not-armed\""), "{late}");
    assert_eq!(n2_ring::acked_seq(), last_seq);
}

// ---------------------------------------------------------------------------
// 11. the ACK DOCUMENT contract: both keys required, named refusals, nothing
//     advanced by a refusal
// ---------------------------------------------------------------------------

#[test]
fn the_ack_document_requires_a_session_and_a_watermark() {
    let _guard = serial();
    n2_ring::disable();
    let dir = TempDir::new("ackdoc");
    let path = ring_path(&dir);
    assert_eq!(n2_ring::configure_session(&path, n2_ring::DEFAULT_MAX_BYTES,
        n2_ring::DEFAULT_SEGMENTS, Some("arm-ack")), Ok(()));
    n2_ring::append("N2_TUN_DEST|dst=203.0.113.1");
    let last_seq = manifest_u64(&n2_ring::manifest_line_now(), "last_seq=");

    // every malformed / incomplete document is refused by name and by the
    // bridge's `rejected` result, and NOTHING about the ring changes
    for (document, reason) in [
        ("", "empty-request"),
        ("   ", "empty-request"),
        ("{session:1}", "invalid-json"),
        ("null", "invalid-request"),
        ("[1]", "invalid-request"),
        (r#"{"up_to_seq":2}"#, "invalid-session"),
        (r#"{"session":""}"#, "invalid-session"),
        (r#"{"session":""}"#, "invalid-session"),
        (r#"{"session":"arm-ack"}"#, "invalid-request"),
        (r#"{"session":"arm-ack","up_to_seq":"2"}"#, "invalid-request"),
        (r#"{"session":3,"up_to_seq":2}"#, "invalid-request"),
        (r#"{"session":"arm-live","up_to_seq":2,"start":0}"#, "session-mismatch"),
    ] {
        let reply = netbird_core::connector::n2_ring_ack_json(document);
        assert!(reply.contains("\"ok\":false"), "{document} -> {reply}");
        assert!(reply.contains(&format!("\"reason\":\"{reason}\"")), "{document} -> {reply}");
        assert!(reply.contains("\"advanced\":false"), "{document} -> {reply}");
        assert_eq!(n2_ring::acked_seq(), 0, "{document} moved the watermark");
    }
    // the same refusals come from the core's own parser
    assert_eq!(n2_ring::parse_ack_request("{\"session\":\"arm-ack\"}"),
        Err("invalid-request"));
    assert_eq!(n2_ring::parse_ack_request("{\"up_to_seq\":2}"), Err("invalid-session"));

    // the accepted document names both values
    assert_eq!(n2_ring::parse_ack_request(&format!(
        "{{\"session\":\"arm-ack\",\"up_to_seq\":{last_seq}}}")),
        Ok(n2_ring::AckRequest { session: "arm-ack".into(), up_to_seq: last_seq }));
    assert_eq!(n2_ring::acked_seq(), 0, "parsing alone advances nothing");

    // the reply shapes are EXACT (field order included), so a host parser never
    // sees a half document and the documented contract is pinned
    assert_eq!(netbird_core::connector::n2_ring_ack_json("{}"),
        "{\"ok\":false,\"result\":\"rejected\",\"reason\":\"invalid-session\",\"session\":\"\",\"up_to_seq\":0,\"acked_seq\":0,\"previous_acked_seq\":0,\"advanced\":false}");
    assert_eq!(netbird_core::connector::n2_ring_ack_json(&format!(
        "{{\"session\":\"arm-dead\",\"up_to_seq\":{last_seq}}}")),
        format!("{{\"ok\":false,\"result\":\"rejected\",\"reason\":\"session-mismatch\",\"session\":\"arm-dead\",\"up_to_seq\":{last_seq},\"acked_seq\":0,\"previous_acked_seq\":0,\"advanced\":false}}"));
    assert_eq!(netbird_core::connector::n2_ring_ack_json(&format!(
        "{{\"session\":\"arm-ack\",\"up_to_seq\":{last_seq}}}")),
        format!("{{\"ok\":true,\"result\":\"acked\",\"reason\":\"ok\",\"session\":\"arm-ack\",\"up_to_seq\":{last_seq},\"acked_seq\":{last_seq},\"previous_acked_seq\":0,\"advanced\":true}}"));
    assert_eq!(n2_ring::acked_seq(), last_seq);
    n2_ring::disable();
}
