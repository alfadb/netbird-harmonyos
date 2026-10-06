//! Phone-side persistent log (`phone_log`) — REAL-filesystem tests.
//!
//! These run in their own test process; each test configures the
//! process-global logger, so the whole file is serialized on one lock. Every
//! assertion reads actual files under a unique temp directory.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use netbird_core::phone_log::{self, Options};
use netbird_core::{hilog, n2_ring};

// Host-process link stubs (repo convention, cf. tests/n2_log_samples.rs):
// libace_napi.z.so / libhilog_ndk.z.so do not exist on the host. No-ops for
// the linker only; never called by these tests.
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
        _value: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_cb_info(
        _env: *mut c_void,
        _cbinfo: *mut c_void,
        _argc: *mut usize,
        _argv: *mut *mut c_void,
        _data: *mut c_void,
        _result: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _env: *mut c_void,
        _str_: *const u8,
        _len: usize,
        _result: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_string_utf8(
        _env: *mut c_void,
        _value: *mut c_void,
        _buf: *mut u8,
        _bufsize: usize,
        _result: *mut usize,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_int32(
        _env: *mut c_void,
        _value: *mut c_void,
        _result: *mut i32,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(
        _env: *mut c_void,
        _value: *mut c_void,
        _result: *mut bool,
    ) -> i32 {
        0
    }
}

static TEST_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Unique temp directory, removed on drop.
struct Dir {
    path: PathBuf,
}

impl Dir {
    fn new(tag: &str) -> Dir {
        static N: AtomicU64 = AtomicU64::new(0);
        let counter = N.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nb-phone-log-{}-{}-{}-{}",
            std::process::id(),
            tag,
            nanos,
            counter
        ));
        fs::create_dir_all(&path).unwrap();
        Dir { path }
    }

    fn file(&self, name: &str) -> String {
        self.path.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Test options with the admission budget effectively OFF, so the rotation /
/// FIFO / queue-pressure tests below exercise exactly what they name. The
/// budget itself is covered by the `rate_budget_*` tests with a tiny budget.
fn options(max_bytes: u64, segments: u32, queue: usize) -> Options {
    options_with_rate(max_bytes, segments, queue, u64::MAX, u64::MAX)
}

fn options_with_rate(
    max_bytes: u64,
    segments: u32,
    queue: usize,
    bytes_per_sec: u64,
    records_per_sec: u64,
) -> Options {
    Options {
        max_bytes,
        segments,
        queue,
        rate_bytes_per_sec: bytes_per_sec,
        rate_records_per_sec: records_per_sec,
        ..Options::default()
    }
}

/// Concatenate the kept segments oldest -> newest (`<path>.n` … `<path>.1`,
/// then `<path>`); that is also the record order.
fn read_segments(path: &str, segments: u32) -> String {
    let mut out = String::new();
    for index in (1..segments).rev() {
        if let Ok(text) = fs::read_to_string(format!("{path}.{index}")) {
            out.push_str(&text);
        }
    }
    if let Ok(text) = fs::read_to_string(path) {
        out.push_str(&text);
    }
    out
}

fn segment_files(path: &str, segments: u32) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for index in 0..segments {
        let candidate =
            if index == 0 { PathBuf::from(path) } else { PathBuf::from(format!("{path}.{index}")) };
        if candidate.exists() {
            files.push(candidate);
        }
    }
    files
}

#[derive(Debug)]
struct Rec {
    sess: String,
    seq: u64,
    dropped: u64,
    line: String,
}

fn parse_records(text: &str) -> Vec<Rec> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let Some(rest) = raw.strip_prefix("PL|") else { continue };
        let Some((head, line)) = rest.split_once("|line=") else { continue };
        let mut sess = String::new();
        let mut seq = 0u64;
        let mut dropped = 0u64;
        for field in head.split('|') {
            if let Some(value) = field.strip_prefix("sess=") {
                sess = value.to_string();
            } else if let Some(value) = field.strip_prefix("seq=") {
                seq = value.parse().unwrap();
            } else if let Some(value) = field.strip_prefix("dropped=") {
                dropped = value.parse().unwrap();
            }
        }
        assert!(raw.contains("|wall_ms="), "record lacks wall_ms: {raw}");
        assert!(raw.contains("|mono_ms="), "record lacks mono_ms: {raw}");
        assert!(raw.contains("|pid="), "record lacks pid: {raw}");
        out.push(Rec { sess, seq, dropped, line: line.to_string() });
    }
    out
}

#[test]
fn rotation_is_bounded_and_conserves_the_tail() {
    let _guard = lock();
    let dir = Dir::new("rotation");
    let path = dir.file("netbird-vpn.log");
    let segments = 4u32;
    let max_bytes = 1024u64;
    phone_log::configure_with(&path, &options(max_bytes, segments, 4096)).unwrap();

    for i in 0..400 {
        phone_log::append(&format!("ROTLINE|i={i}|pad={}", "x".repeat(60)));
    }
    assert!(phone_log::flush(Duration::from_secs(5)), "flush timed out");

    let files = segment_files(&path, segments);
    assert!(!files.is_empty(), "no segment written");
    assert!(files.len() <= segments as usize, "more segments than configured: {files:?}");
    let total: u64 = files.iter().map(|f| fs::metadata(f).unwrap().len()).sum();
    // Every segment holds at most one record more than the cap.
    assert!(
        total <= segments as u64 * max_bytes + 2048,
        "rotation exceeded the bound: {total} bytes in {files:?}"
    );

    let records = parse_records(&read_segments(&path, segments));
    let indexes: Vec<u64> = records
        .iter()
        .filter_map(|r| r.line.strip_prefix("ROTLINE|i="))
        .filter_map(|rest| rest.split('|').next())
        .map(|value| value.parse().unwrap())
        .collect();
    assert!(!indexes.is_empty(), "no ROTLINE records survived");
    assert_eq!(indexes.last().copied(), Some(399), "the newest record was lost");
    for pair in indexes.windows(2) {
        assert_eq!(pair[1], pair[0] + 1, "tail records are not contiguous: {indexes:?}");
    }
    assert!(phone_log::stats().rotations > 0, "no rotation happened");
    assert!(phone_log::shutdown(Duration::from_secs(5)));
}

#[test]
fn native_emit_is_routed_while_n2_ring_stays_unarmed() {
    let _guard = lock();
    let dir = Dir::new("emit");
    let path = dir.file("netbird-vpn.log");
    phone_log::configure(&path).unwrap();
    assert!(phone_log::enabled());
    assert!(!n2_ring::enabled(), "the frozen ring must stay unarmed");

    hilog::emit("PHONE_LOG_SENTINEL|marker=emit-routed|endpoint=100.64.0.7:51820");
    let route_line = netbird_core::route_audit::managed_route_line(
        73,
        0,
        1,
        &netbird_core::network_map::ManagedRoute {
            id: "route-a".into(),
            network: netbird_core::config::Route { addr: [192, 168, 50, 0], prefix_len: 24 },
            domains: vec![],
            net_id: "net-lan".into(),
            network_type: 1,
            peer: "FyLw4PSBg1hjr+9/x0=".into(),
            metric: 10,
            masquerade: true,
            keep_route: true,
            skip_auto_apply: false,
        },
    );
    hilog::emit(&route_line);
    assert!(phone_log::flush(Duration::from_secs(5)), "flush timed out");
    assert!(phone_log::shutdown(Duration::from_secs(5)));

    let text = read_segments(&path, 4);
    assert!(
        text.contains("PHONE_LOG_SENTINEL|marker=emit-routed|endpoint=100.64.0.7:51820"),
        "the emitted marker was not routed into the file: {text}"
    );
    assert!(
        text.contains("N6_ROUTE_MAP|serial=73|index=0|total=1|id=route-a|net_id=net-lan|network=192.168.50.0/24|peer=FyLw4PSBg1hjr+9/x0=|metric=10|network_type=1|masquerade=true|keep_route=true|skip_auto_apply=false"),
        "the route-audit marker did not survive the persistent phone-log tee: {text}"
    );
    assert!(!text.contains("N2R|"), "n2_ring records leaked into the phone log: {text}");
    assert!(!n2_ring::enabled(), "the tee must not arm the ring");
}

#[test]
fn accepted_records_keep_fifo_order_and_shutdown_drains() {
    let _guard = lock();
    let dir = Dir::new("fifo");
    let path = dir.file("netbird-vpn.log");
    phone_log::configure_with(&path, &options(2 * 1024 * 1024, 4, 1024)).unwrap();

    let expected: Vec<String> = (0..200).map(|i| format!("FIFO|i={i}")).collect();
    for line in &expected {
        phone_log::append(line);
    }
    assert!(phone_log::shutdown(Duration::from_secs(5)), "shutdown must drain within the bound");

    let records = parse_records(&read_segments(&path, 4));
    assert_eq!(records.len(), expected.len(), "shutdown did not drain every accepted record");
    let lines: Vec<String> = records.iter().map(|r| r.line.clone()).collect();
    assert_eq!(lines, expected, "queue order is not FIFO");
    for pair in records.windows(2) {
        assert_eq!(pair[1].seq, pair[0].seq + 1, "seq is not monotonic: {:?}", pair);
    }
    let stats = phone_log::stats();
    assert_eq!(stats.dropped, 0);
    assert_eq!(stats.accepted, stats.written);
    assert_eq!(stats.pending, 0);
    assert!(!stats.running);
}

#[test]
fn pressure_drops_are_counted_and_accounted() {
    let _guard = lock();
    let dir = Dir::new("pressure");
    let path = dir.file("netbird-vpn.log");
    // Tiny queue + minimum-segment rotation: the writer cannot keep up.
    phone_log::configure_with(&path, &options(1024, 4, 8)).unwrap();

    for i in 0..50_000 {
        phone_log::append(&format!("PRESSURE|i={i}|{}", "y".repeat(40)));
    }
    let during = phone_log::stats();
    assert!(during.dropped > 0, "expected queue pressure to drop records: {during:?}");
    assert_eq!(during.rate_suppressed, 0, "the budget is off in this test: {during:?}");
    assert_eq!(during.accepted, during.offered - during.dropped);
    assert!(phone_log::shutdown(Duration::from_secs(10)));

    let after = phone_log::stats();
    assert_eq!(
        after.accepted,
        after.written + after.failed,
        "every accepted record must be written or counted failed: {after:?}"
    );
    assert!(!after.running);
    let records = parse_records(&read_segments(&path, 4));
    let pressure = records.iter().filter(|r| r.line.starts_with("PRESSURE|")).count() as u64;
    assert!(pressure > 0, "no PRESSURE record survived");
    // Rotation deletes whole segments, so the files hold a SUBSET of everything
    // written; the counter must still be an upper bound.
    assert!(pressure <= after.written, "on-disk records exceed the written counter");
    // The drop count is visible in the records themselves, not only in stats.
    assert!(
        records.iter().any(|r| r.dropped > 0),
        "no record carried the cumulative drop count"
    );
}

#[test]
fn rate_budget_bounds_bytes_per_window_and_stays_separate_from_queue_drops() {
    let _guard = lock();
    let dir = Dir::new("rate-bytes");
    let path = dir.file("netbird-vpn.log");
    // 4 KiB of charged bytes per second; the record budget is off and the queue
    // is large, so the BYTE budget is the only brake and nothing here is a
    // queue drop.
    const BUDGET: u64 = 4096;
    const LINE_BYTES: u64 = 120;
    phone_log::configure_with(&path, &options_with_rate(2 * 1024 * 1024, 4, 4096, BUDGET, u64::MAX))
        .unwrap();

    let line = format!("RATE|{}", "z".repeat(115));
    assert_eq!(line.len() as u64, LINE_BYTES);
    let offered = 400u64;
    let start = Instant::now();
    for _ in 0..offered {
        phone_log::append(&line);
    }
    let elapsed = start.elapsed();
    let during = phone_log::stats();
    assert_eq!(during.offered, offered, "every line is offered exactly once: {during:?}");
    assert_eq!(during.dropped, during.rate_suppressed, "no queue drop expected: {during:?}");
    assert!(during.rate_suppressed > 0, "the budget must refuse a burst: {during:?}");
    assert_eq!(during.accepted + during.dropped, during.offered);
    // The strict rate bound: admitted charged bytes stay inside the window
    // budget, allowing the burst to straddle one boundary (plus one for a
    // scheduling delay before `elapsed` is taken).
    let windows = elapsed.as_millis() as u64 / 1000 + 2;
    let charged = during.accepted * LINE_BYTES;
    assert!(
        charged <= windows * BUDGET,
        "admission budget exceeded: {charged} charged bytes in {windows} window(s)"
    );
    // ... and the bound is not trivially satisfied by refusing everything.
    assert!(during.accepted >= 30, "the first window must admit its bytes: {during:?}");

    assert!(phone_log::flush(Duration::from_secs(5)), "flush timed out");
    assert!(phone_log::shutdown(Duration::from_secs(5)));
    let records = parse_records(&read_segments(&path, 4));
    assert!(!records.is_empty());
    // Suppression is visible in the persisted records as the refusal count.
    assert!(records.iter().any(|r| r.dropped > 0), "no record carried the refusal count");
}

#[test]
fn rate_budget_bounds_records_per_window_and_recovers_next_window() {
    let _guard = lock();
    let dir = Dir::new("rate-records");
    let path = dir.file("netbird-vpn.log");
    // 3 records per second with the byte budget off: the RECORD budget is the
    // only brake, so recovery is observable without generating megabytes.
    const BUDGET: u64 = 3;
    phone_log::configure_with(&path, &options_with_rate(1024, 4, 4096, u64::MAX, BUDGET)).unwrap();

    let offered = 20u64;
    let start = Instant::now();
    for i in 0..offered {
        phone_log::append(&format!("RATEREC|i={i}"));
    }
    let elapsed = start.elapsed();
    let during = phone_log::stats();
    assert_eq!(during.offered, offered);
    assert_eq!(during.dropped, during.rate_suppressed, "the queue is not the brake here");
    assert!(during.rate_suppressed > 0, "the record budget must refuse the burst: {during:?}");
    assert!(during.accepted >= BUDGET, "the first window must admit its records");
    let windows = elapsed.as_millis() as u64 / 1000 + 2;
    assert!(
        during.accepted <= windows * BUDGET,
        "record budget exceeded: {} admitted in {windows} window(s)",
        during.accepted
    );
    assert!(phone_log::flush(Duration::from_secs(5)), "flush timed out");

    // Recovery: the exact boundary is proven with the injected clock in the
    // unit tests; here the INSTALLED logger must recover by itself, which needs
    // the window to turn over — one bounded 1.1 s wait, no stress loop.
    let suppressed_before = during.rate_suppressed;
    thread::sleep(Duration::from_millis(1100));
    let marker = "RATEREC|recovered-after-the-window";
    phone_log::append(marker);
    let after = phone_log::stats();
    assert_eq!(
        after.rate_suppressed, suppressed_before,
        "the next window must admit again: {after:?}"
    );
    assert_eq!(after.accepted, during.accepted + 1, "the recovered line must be queued");
    assert!(phone_log::flush(Duration::from_secs(5)));
    assert!(phone_log::shutdown(Duration::from_secs(5)));
    let text = read_segments(&path, 4);
    assert!(text.contains(marker), "the recovered line was not written: {text}");
}

#[test]
fn secrets_are_redacted_useful_endpoints_survive() {
    let _guard = lock();
    let dir = Dir::new("redact");
    let path = dir.file("netbird-vpn.log");
    phone_log::configure_with(&path, &options(2 * 1024 * 1024, 4, 1024)).unwrap();

    const SETUP: &str = "SETUPKEY-SENTINEL-9f3a2b7c";
    const AUTH: &str = "AUTHTOKEN-SENTINEL-4d5e6f";
    const JWT: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dBjftJeZ4CVPmB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    const BEARER: &str = "BEARER-SENTINEL-77889900";

    hilog::emit(&format!("PLSEC|setup_key={SETUP}|endpoint=100.64.0.9:51820"));
    hilog::emit(&format!("PLSEC|auth_token: {AUTH}|host=peer.example.net:51820"));
    hilog::emit(&format!("PLSEC|jwt={JWT}"));
    hilog::emit("PLSEC|url=rels://alice:hunter2@relay.example.org:28443/path");
    hilog::emit(&format!("PLSEC|header=Bearer {BEARER}"));
    hilog::emit("PLSEC|password=none|token_present=false|relay=relay.example.org:28443");
    assert!(phone_log::flush(Duration::from_secs(5)));
    assert!(phone_log::shutdown(Duration::from_secs(5)));

    let text = read_segments(&path, 4);
    for secret in [SETUP, AUTH, JWT, BEARER, "hunter2"] {
        assert!(!text.contains(secret), "secret survived into the file: {secret}");
    }
    assert!(text.contains("[redacted]"), "no redaction marker in: {text}");
    // Useful diagnostic data is deliberately kept.
    for keep in [
        "100.64.0.9:51820",
        "peer.example.net:51820",
        "relay.example.org:28443",
        "password=none",
        "token_present=false",
        "PLSEC|",
    ] {
        assert!(text.contains(keep), "useful data was deleted: {keep} in {text}");
    }
    assert_eq!(parse_records(&text).len(), 6, "one record per emitted line: {text}");
}

#[test]
fn reconfigure_retains_previous_sessions_and_reaps_the_old_writer() {
    let _guard = lock();
    let dir = Dir::new("reconfigure");
    let path = dir.file("netbird-vpn.log");

    phone_log::configure(&path).unwrap();
    let first_session = phone_log::stats().session;
    phone_log::append("SESSION-A|one|endpoint=100.64.0.1:51820");
    assert!(phone_log::shutdown(Duration::from_secs(5)));
    assert!(!phone_log::stats().running);

    // A second configure on the SAME path must append, never truncate.
    phone_log::configure(&path).unwrap();
    let second_session = phone_log::stats().session;
    assert_ne!(first_session, second_session, "each configure gets a fresh session token");
    phone_log::append("SESSION-B|two|endpoint=100.64.0.2:51820");
    assert!(phone_log::shutdown(Duration::from_secs(5)));

    let text = read_segments(&path, 4);
    assert!(text.contains("SESSION-A|one|endpoint=100.64.0.1:51820"), "{text}");
    assert!(text.contains("SESSION-B|two|endpoint=100.64.0.2:51820"), "{text}");
    let records = parse_records(&text);
    let sessions: Vec<&str> = records.iter().map(|r| r.sess.as_str()).collect();
    assert_eq!(sessions, vec![first_session.as_str(), second_session.as_str()]);

    // No leaked writer: emitting after shutdown must not create records, and
    // reconfigure must have reaped the previous thread.
    assert!(!phone_log::stats().running);
    hilog::emit("SESSION-B|after-shutdown-should-not-land");
    assert!(phone_log::flush(Duration::from_secs(2)));
    let text = read_segments(&path, 4);
    assert!(!text.contains("after-shutdown-should-not-land"), "{text}");
}

#[test]
fn flush_and_shutdown_are_bounded_when_idle() {
    let _guard = lock();
    let dir = Dir::new("idle");
    let path = dir.file("netbird-vpn.log");
    phone_log::shutdown(Duration::from_secs(1)); // no logger installed: no-op

    let start = Instant::now();
    assert!(phone_log::flush(Duration::from_millis(50)), "idle flush must report done");
    assert!(phone_log::shutdown(Duration::from_millis(50)), "idle shutdown must report done");
    assert!(start.elapsed() < Duration::from_secs(2), "idle calls must not block");

    phone_log::configure(&path).unwrap();
    assert!(phone_log::shutdown(Duration::from_secs(5)));
    assert!(phone_log::flush(Duration::from_secs(2)));
    let stats = phone_log::stats();
    assert!(!stats.enabled);
    assert!(!stats.running);
    assert!(stats.path.ends_with("netbird-vpn.log"));

    let json = phone_log::status_json();
    assert!(json.starts_with("{\"component\":\"phone_log\""), "{json}");
    assert!(json.ends_with('}'), "{json}");
    assert!(json.contains("\"running\":false"), "{json}");
    assert!(json.contains("\"queue_capacity\":256"), "{json}");
    assert!(json.contains("\"segment_bytes\":2097152"), "{json}");
    // The admission budget and its counter are part of the status document.
    assert!(json.contains("\"rate_bytes_per_sec\":8192"), "{json}");
    assert!(json.contains("\"rate_records_per_sec\":64"), "{json}");
    assert!(json.contains("\"rate_suppressed\":0"), "{json}");
    assert_eq!(stats.rate_bytes_per_sec, 8192);
    assert_eq!(stats.rate_records_per_sec, 64);
    assert_eq!(stats.rate_suppressed, 0);
    assert_eq!(json.matches('{').count(), json.matches('}').count(), "{json}");
    assert_eq!(json.matches('"').count() % 2, 0, "{json}");
}

#[test]
fn configure_validation_is_fail_closed() {
    let _guard = lock();
    let dir = Dir::new("validation");
    let good = dir.file("netbird-vpn.log");

    assert_eq!(phone_log::configure_with("relative.log", &Options::default()), Err("invalid-path"));
    assert_eq!(phone_log::configure_with(&good, &options(16, 4, 8)), Err("invalid-max-bytes"));
    assert_eq!(phone_log::configure_with(&good, &options(4096, 1, 8)), Err("invalid-segments"));
    assert_eq!(
        phone_log::configure_with(&good, &Options { max_line_chars: 16, ..Options::default() }),
        Err("invalid-max-line")
    );
    // `0` fields mean "use the default", so the good path is accepted — the
    // admission budget included (any budget is meaningful, so none is
    // rejected; `0` only selects the default).
    let accepted = Options {
        max_bytes: 0,
        segments: 0,
        queue: 0,
        max_line_chars: 0,
        rate_bytes_per_sec: 0,
        rate_records_per_sec: 0,
        session: None,
    };
    assert_eq!(phone_log::configure_with(&good, &accepted), Ok(()));
    let resolved = phone_log::stats();
    assert_eq!(resolved.rate_bytes_per_sec, 8192);
    assert_eq!(resolved.rate_records_per_sec, 64);
    assert!(phone_log::shutdown(Duration::from_secs(5)));

    // A path whose parent does not exist cannot be opened: fail-closed, and
    // the previously installed logger is left untouched.
    let missing = format!("{}/does/not/exist/netbird-vpn.log", dir.path.to_string_lossy());
    assert_eq!(phone_log::configure(&missing), Err("open-failed"));
    assert!(phone_log::shutdown(Duration::from_secs(5)));
    // The failed configure left the previous (stopped) logger in place.
    assert_eq!(phone_log::stats().path, good);
    assert!(!phone_log::stats().running);
}
