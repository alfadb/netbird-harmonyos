//! R2-A tun fd-contract tests — INTEGRATION-PROCESS tests.
//!
//! Why a separate test binary: the fd ledger and the tun session table are
//! process-global singletons, and `ledger.rs`'s own unit tests assert digest
//! determinism across their emission points. Running the tun contract suite
//! (which emits ~28 ledger transitions) in a DIFFERENT PROCESS makes the two
//! suites structurally unable to interleave, instead of relying on lock
//! domains that cannot be shared without editing ledger.rs.
//!
//! The tun fd semantics are simulated with socketpair/pipe on the host triple
//! (no device, no HDC): the platform raw fd / VpnConnection.destroy() roles
//! are played by one socketpair end / an explicit close(2) of it. Device-bound
//! verification stays gated behind the physical-campaign governance and is
//! NOT claimed here.
//!
//! Host link surface: the crate under test references OHOS-only symbols
//! (libhilog_ndk.z.so, libace_napi.z.so) that do not exist on the host; the
//! stubs below satisfy the linker (same approach as the crate's cfg(test)
//! host_stubs, which are absent from the rlib an integration test links).

use netbird_core::ledger::{self, Role};
use netbird_core::sys;
use netbird_core::tun::{
    open_session, session_close, session_poll, session_read, session_write, tun_close_json,
    tun_open_json, tun_poll_json, tun_read_json, tun_write_json, TunError, TunFd, READ_BUF,
    WRITE_MAX,
};
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// host link stubs (OHOS-only symbols)
// ---------------------------------------------------------------------------

#[no_mangle]
extern "C" fn OH_LOG_Print(
    _t: i32,
    _l: i32,
    _d: u32,
    _tag: *const u8,
    _fmt: *const u8,
    _arg: *const core::ffi::c_void,
) -> i32 {
    0
}

#[no_mangle]
extern "C" fn OH_LOG_IsLoggable(_d: u32, _tag: *const u8, _l: i32) -> bool {
    false
}

#[no_mangle]
extern "C" fn napi_module_register(_m: *mut core::ffi::c_void) {}

#[no_mangle]
extern "C" fn napi_create_function(
    _e: *mut core::ffi::c_void,
    _n: *const u8,
    _l: usize,
    _cb: *mut core::ffi::c_void,
    _d: *mut core::ffi::c_void,
    r: *mut *mut core::ffi::c_void,
) -> i32 {
    unsafe { *r = 0x10 as *mut core::ffi::c_void };
    0
}

#[no_mangle]
extern "C" fn napi_set_named_property(
    _e: *mut core::ffi::c_void,
    _o: *mut core::ffi::c_void,
    _n: *const u8,
    _v: *mut core::ffi::c_void,
) -> i32 {
    0
}

#[no_mangle]
extern "C" fn napi_get_cb_info(
    _e: *mut core::ffi::c_void,
    _i: *mut core::ffi::c_void,
    _argc: *mut usize,
    _argv: *mut *mut core::ffi::c_void,
    _this: *mut core::ffi::c_void,
    _data: *mut *mut core::ffi::c_void,
) -> i32 {
    0
}

#[no_mangle]
extern "C" fn napi_create_string_utf8(
    _e: *mut core::ffi::c_void,
    _s: *const u8,
    _l: usize,
    _r: *mut core::ffi::c_void,
) -> i32 {
    0
}

#[no_mangle]
extern "C" fn napi_get_value_string_utf8(
    _e: *mut core::ffi::c_void,
    _v: *mut core::ffi::c_void,
    _b: *mut u8,
    _bs: usize,
    _r: *mut usize,
) -> i32 {
    0
}

#[no_mangle]
extern "C" fn napi_get_value_int32(
    _e: *mut core::ffi::c_void,
    _v: *mut core::ffi::c_void,
    _r: *mut i32,
) -> i32 {
    0
}

#[no_mangle]
extern "C" fn napi_get_value_bool(
    _e: *mut core::ffi::c_void,
    _v: *mut core::ffi::c_void,
    _r: *mut bool,
) -> i32 {
    0
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

extern "C" {
    fn socketpair(domain: i32, ty: i32, protocol: i32, sv: *mut [i32; 2]) -> i32;
    fn pipe(fds: *mut [i32; 2]) -> i32;
}

/// The ledger and the session table are process-global; all tests in THIS
/// binary serialize on this lock so their assertions are deterministic.
static TEST_LOCK: Mutex<()> = Mutex::new(());

fn sp() -> [i32; 2] {
    let mut sv = [-1i32; 2];
    let r = unsafe { socketpair(1 /* AF_UNIX */, 1 /* SOCK_STREAM */, 0, &mut sv) };
    assert_eq!(r, 0, "socketpair failed errno={}", sys::errno());
    sv
}

fn pipe2() -> [i32; 2] {
    let mut fds = [-1i32; 2];
    let r = unsafe { pipe(&mut fds) };
    assert_eq!(r, 0, "pipe failed errno={}", sys::errno());
    fds
}

fn fd_is_open(fd: i32) -> bool {
    (unsafe { sys::fcntl(fd, sys::F_GETFD) }) != -1
}

fn fd_is_nonblock(fd: i32) -> bool {
    let fl = unsafe { sys::fcntl(fd, sys::F_GETFL) };
    fl != -1 && (fl & sys::O_NONBLOCK) != 0
}

fn set_nonblock(fd: i32) {
    let fl = unsafe { sys::fcntl(fd, sys::F_GETFL) };
    assert_ne!(fl, -1);
    let r = unsafe { sys::fcntl(fd, sys::F_SETFL, fl | sys::O_NONBLOCK) };
    assert_ne!(r, -1);
}

/// Identify the FdDup ledger entry created between `before_lines` and now
/// for this fd, by its unique `fd_dup#inst|fd|` prefix. Diff-based so a
/// stale still-open entry with a reused fd number cannot be confused with
/// ours.
fn new_dup_prefix(before_lines: &str, fd: i32) -> String {
    let before: std::collections::HashSet<&str> = before_lines.lines().collect();
    let after = ledger::canonical_lines();
    let mut found = None;
    for l in after.lines() {
        if !before.contains(l) && l.starts_with("fd_dup#") && l.contains(&format!("|{fd}|")) {
            assert!(found.is_none(), "more than one new fd_dup entry for fd {fd}");
            found = Some(l.splitn(3, '|').take(2).collect::<Vec<_>>().join("|"));
        }
    }
    found.expect("the new fd_dup ledger entry")
}

/// First unsigned integer value of `"key":<n>` in a flat JSON string.
fn json_num(j: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\":");
    let i = j.find(&needle)? + needle.len();
    let rest = &j[i..];
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

// ---------------------------------------------------------------------------
// 1. open contract: raw fd read-only use, CLOEXEC, ledger ownership table
// ---------------------------------------------------------------------------

#[test]
fn dup_open_contract_raw_fd_untouched_cloexec_ledger() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sv = sp();
    let (raw, peer) = (sv[0], sv[1]);
    let t = TunFd::dup_from_raw(raw).expect("dup_from_raw");
    let fd_dup = t.fd().expect("dup stored");
    assert_ne!(fd_dup, raw, "the copy must be a new fd number");
    // the raw fd was NOT consumed by the dup (read-only use only)
    assert!(fd_is_open(raw), "raw fd must survive the dup");
    assert!(fd_is_open(peer));
    // the copy carries FD_CLOEXEC (F_DUPFD_CLOEXEC contract)
    let getfd = unsafe { sys::fcntl(fd_dup, sys::F_GETFD) };
    assert_ne!(getfd, -1);
    assert_ne!(getfd & sys::FD_CLOEXEC, 0, "dup must be CLOEXEC");
    // ownership table registered the dup (and the observed raw fd once)
    assert_eq!(
        ledger::live_fd(Role::FdDup),
        Some(fd_dup),
        "fd_dup must be the live FdDup ledger entry"
    );
    assert!(ledger::is_created(Role::FdOrig));
    t.close().expect("close");
    assert!(!fd_is_open(fd_dup), "our dup must be closed");
    assert!(fd_is_open(raw), "closing our dup must NOT close the raw fd");
    unsafe {
        sys::close(raw);
        sys::close(peer);
    }
}

#[test]
fn dup_from_raw_rejects_closed_or_invalid_raw_fd() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // invalid fd number: EBADF mapped to the explicit BadFd contract error
    assert_eq!(
        TunFd::dup_from_raw(-1).unwrap_err(),
        TunError::BadFd,
        "invalid raw fd must fail closed"
    );
    let sv = sp();
    unsafe { sys::close(sv[1]) };
    assert_eq!(
        TunFd::dup_from_raw(sv[1]).unwrap_err(),
        TunError::BadFd,
        "already-closed raw fd (destroy semantics) must fail closed"
    );
    unsafe { sys::close(sv[0]) };
}

// ---------------------------------------------------------------------------
// 2. data path: read/write through the dup
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_read_write_through_the_dup() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sv = sp();
    let t = TunFd::dup_from_raw(sv[0]).expect("dup");
    // TUN -> peer direction
    assert_eq!(t.write_frame(b"frame-001").expect("write"), 9);
    let mut rx = [0u8; 16];
    let n = sys::read_fd(sv[1], &mut rx);
    assert_eq!(n, (9, 0));
    assert_eq!(&rx[..9], b"frame-001");
    // peer -> TUN direction
    let (wn, we) = sys::write_fd(sv[1], b"frame-002");
    assert_eq!((wn, we), (9, 0));
    let mut buf = [0u8; READ_BUF];
    assert_eq!(t.read_frame(&mut buf).expect("read"), 9);
    assert_eq!(&buf[..9], b"frame-002");
    t.close().expect("close");
    unsafe {
        sys::close(sv[0]);
        sys::close(sv[1]);
    }
}

// ---------------------------------------------------------------------------
// 3. EAGAIN + shared open-file-description O_NONBLOCK side effect
// ---------------------------------------------------------------------------

#[test]
fn nonblock_eagain_and_shared_ofd_side_effect() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sv = sp();
    let t = TunFd::dup_from_raw(sv[0]).expect("dup");
    let fd_dup = t.fd().unwrap();
    assert!(!t.nonblock_ofd(), "fresh socketpair starts blocking");
    set_nonblock(fd_dup);
    // O_NONBLOCK is an open-file-description property: the dup and the raw
    // fd flip TOGETHER — exactly why TunFd never writes flags itself.
    assert!(fd_is_nonblock(sv[0]), "shared ofd: raw fd flipped too");
    let mut buf = [0u8; 64];
    assert_eq!(
        t.read_frame(&mut buf).unwrap_err(),
        TunError::WouldBlock,
        "empty non-blocking read must be EAGAIN/WouldBlock"
    );
    // bounded read: budget elapsed, still WouldBlock, unblocked by slices
    let t0 = std::time::Instant::now();
    assert_eq!(
        t.read_frame_timeout(&mut buf, 120).unwrap_err(),
        TunError::WouldBlock
    );
    assert!(
        (110..2000).contains(&t0.elapsed().as_millis()),
        "bounded wait took {:?}",
        t0.elapsed()
    );
    t.close().expect("close");
    unsafe {
        sys::close(sv[0]);
        sys::close(sv[1]);
    }
}

// ---------------------------------------------------------------------------
// 4. backpressure: partial write + budget exhaustion + POLLOUT drain
// ---------------------------------------------------------------------------

#[test]
fn backpressure_partial_write_budget_then_drain_completes() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sv = sp();
    let t = TunFd::dup_from_raw(sv[0]).expect("dup");
    set_nonblock(t.fd().unwrap());
    // fill until EAGAIN: budget 0 => fail fast with the partial count
    let big = vec![0xa5u8; 300_000];
    let pushed_first;
    match t.write_frame_budget(&big, 0) {
        Err(TunError::Backpressure { written }) => {
            assert!(written > 0 && written < big.len(), "partial write expected, got {written}");
            pushed_first = written;
        }
        other => panic!("expected backpressure with partial count, got {other:?}"),
    }
    // drain from the peer in parallel: the same write with a real budget
    // completes through POLLOUT unblocks (short writes + EAGAIN handled)
    let peer = sv[1];
    let total = big.len();
    let reader = std::thread::spawn(move || {
        let mut seen = 0usize;
        let mut buf = [0u8; 65536];
        while seen < total {
            let (n, e) = sys::read_fd(peer, &mut buf);
            if n > 0 {
                seen += n as usize;
            } else if n == -1 && e != sys::EAGAIN {
                break;
            }
        }
        seen
    });
    assert_eq!(
        t.write_frame_budget(&big, 5000).expect("write completes after drain"),
        total
    );
    let seen = reader.join().expect("reader");
    // the peer receives the first partial attempt's bytes plus this frame;
    // the 64KiB reader chunks may only overshoot `total`, never miss it
    assert!(
        seen >= total && seen <= pushed_first + total,
        "peer bytes out of range: {seen} not in [{total}, {}]",
        pushed_first + total
    );
    t.close().expect("close");
    unsafe {
        sys::close(sv[0]);
        sys::close(sv[1]);
    }
}

// ---------------------------------------------------------------------------
// 5. shutdown unblock: EOF/POLLHUP, and EPIPE on the write side
// ---------------------------------------------------------------------------

#[test]
fn shutdown_unblock_eof_and_hup_then_epipe() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // pipe: read end is "our" side; closing the writer simulates shutdown
    let p = pipe2();
    let t = TunFd::dup_from_raw(p[0]).expect("dup");
    unsafe { sys::close(p[1]) };
    let mut buf = [0u8; 64];
    let t0 = std::time::Instant::now();
    assert_eq!(
        t.read_frame_timeout(&mut buf, 5000).unwrap_err(),
        TunError::Eof,
        "closed writer must surface as EOF, not a hang"
    );
    assert!(
        t0.elapsed().as_millis() < 2000,
        "EOF unblock took {:?}",
        t0.elapsed()
    );
    let rev = t.poll_ready(sys::POLLIN, 0).expect("poll");
    assert_ne!(rev & sys::POLLHUP, 0, "shutdown must show POLLHUP");
    t.close().expect("close");
    unsafe { sys::close(p[0]) };

    // socketpair peer close: write direction fails with EPIPE (SIGPIPE is
    // ignored by the Rust runtime), never a hang or a crash
    let sv = sp();
    let t2 = TunFd::dup_from_raw(sv[0]).expect("dup");
    unsafe { sys::close(sv[1]) };
    assert_eq!(
        t2.write_frame(b"dead").unwrap_err(),
        TunError::Io(32),
        "write after peer close must be EPIPE"
    );
    t2.close().expect("close");
    unsafe { sys::close(sv[0]) };
}

// ---------------------------------------------------------------------------
// 6. close-once / double close / use-after-close, with ledger accounting
// ---------------------------------------------------------------------------

#[test]
fn close_once_double_close_and_use_after_close() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sv = sp();
    let before_open = ledger::canonical_lines();
    let (id, fd_dup, _) = open_session(sv[0]).expect("open_session");
    assert_ne!(fd_dup, sv[0]);

    // exactly one NEW ledger entry for this dup, identified by prefix
    let prefix = new_dup_prefix(&before_open, fd_dup);

    // close-once: explicit close works, kernel fd is gone
    assert_eq!(session_close(id).expect("first close"), fd_dup);
    assert!(!fd_is_open(fd_dup), "dup closed at kernel level");
    let lines = ledger::canonical_lines();
    let closed = lines
        .lines()
        .filter(|l| l.starts_with(&prefix))
        .collect::<Vec<_>>();
    assert_eq!(closed.len(), 1, "exactly one transition set for this entry");
    assert!(
        closed[0].contains("probe-protocol-close"),
        "our close must be ledger-marked: {}",
        closed[0]
    );

    // double close: rejected BEFORE any syscall, no new ledger transition
    let before = ledger::canonical_lines();
    assert_eq!(session_close(id).unwrap_err(), TunError::AlreadyClosed);
    assert_eq!(
        ledger::canonical_lines(),
        before,
        "double close must not add ledger transitions"
    );

    // use after close: explicit errors everywhere (no fd-number guessing)
    let mut buf = [0u8; 16];
    assert_eq!(session_read(id, &mut buf).unwrap_err(), TunError::Closed);
    assert_eq!(session_write(id, b"x").unwrap_err(), TunError::Closed);
    assert_eq!(session_poll(id, 0).unwrap_err(), TunError::Closed);
    assert_eq!(session_close(id).unwrap_err(), TunError::AlreadyClosed);

    // unknown session
    assert_eq!(
        session_read(u32::MAX, &mut buf).unwrap_err(),
        TunError::NoSuchSession
    );

    // dup-close independence: the raw fd is still open after everything
    assert!(fd_is_open(sv[0]), "raw fd must outlive the session");
    unsafe {
        sys::close(sv[0]);
        sys::close(sv[1]);
    }
}

// ---------------------------------------------------------------------------
// 7. destroy-after: raw fd close does not invalidate our dup
// ---------------------------------------------------------------------------

#[test]
fn destroy_after_raw_close_dup_stays_usable() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sv = sp();
    let (id, fd_dup, _) = open_session(sv[0]).expect("open_session");
    // VpnConnection.destroy() closes ONLY the platform fd
    unsafe { sys::close(sv[0]) };
    assert!(!fd_is_open(sv[0]));
    assert!(fd_is_open(fd_dup), "the dup must survive destroy()");
    // the dup keeps carrying data in both directions
    let (wn, we) = sys::write_fd(sv[1], b"post-destroy");
    assert_eq!((wn, we), (12, 0));
    let mut buf = [0u8; READ_BUF];
    assert_eq!(session_read(id, &mut buf).expect("read"), 12);
    assert_eq!(&buf[..12], b"post-destroy");
    assert_eq!(session_write(id, b"still-alive").expect("write"), 11);
    let mut rx = [0u8; 16];
    let (n, _) = sys::read_fd(sv[1], &mut rx);
    assert_eq!(n, 11);
    // ...and our close still closes exactly our copy
    assert_eq!(session_close(id).expect("close"), fd_dup);
    assert!(!fd_is_open(fd_dup));
    unsafe { sys::close(sv[1]) };
}

// ---------------------------------------------------------------------------
// 8. foreign close of our dup: BadFd detection, no false ledger close
// ---------------------------------------------------------------------------

#[test]
fn foreign_close_of_our_dup_detected_as_badfd_not_double_closed() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sv = sp();
    let t = TunFd::dup_from_raw(sv[0]).expect("dup");
    let fd_dup = t.fd().unwrap();
    // someone closes OUR dup behind our back (contract violation upstream)
    unsafe { sys::close(fd_dup) };
    let mut buf = [0u8; 16];
    assert_eq!(t.read_frame(&mut buf).unwrap_err(), TunError::BadFd);
    assert_eq!(t.poll_ready(sys::POLLIN, 0).unwrap_err(), TunError::BadFd);
    // Drop sees EBADF, emits the errno marker, does NOT crash and does NOT
    // emit a false "closed by us" transition
    let before = ledger::canonical_lines();
    drop(t);
    assert_eq!(
        ledger::canonical_lines(),
        before,
        "failed close must not fabricate a clean ledger close"
    );
    assert!(fd_is_open(sv[0]));
    unsafe {
        sys::close(sv[0]);
        sys::close(sv[1]);
    }
}

// ---------------------------------------------------------------------------
// 9. JSON surface (the NAPI boundary shapes)
// ---------------------------------------------------------------------------

#[test]
fn json_surface_roundtrip_and_error_shapes() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sv = sp();
    let j = tun_open_json(sv[0]);
    assert!(j.contains("\"ok\":true"), "{j}");
    let id = json_num(&j, "session").expect("session id in open result") as i32;
    assert!(j.contains("\"nonblock_ofd\":false"), "{j}");
    // write through hex, read it back on the peer
    let jw = tun_write_json(id, "680101"); // "h\x01\x01"
    assert!(jw.contains("\"ok\":true") && jw.contains("\"n\":3"), "{jw}");
    let mut rx = [0u8; 8];
    let (n, _) = sys::read_fd(sv[1], &mut rx);
    assert_eq!((n, &rx[..3]), (3, b"h\x01\x01".as_slice()));
    // peer -> tun read surfaces as hex
    let (wn, _) = sys::write_fd(sv[1], b"xyz");
    assert_eq!(wn, 3);
    let jr = tun_read_json(id);
    assert!(jr.contains("\"ok\":true") && jr.contains("\"hex\":\"78797a\""), "{jr}");
    // empty non-blocking read => eagain shape
    set_nonblock(sv[0]);
    assert_eq!(tun_read_json(id), "{\"eagain\":true}");
    // poll shape
    let jp = tun_poll_json(id, 0);
    assert!(jp.contains("\"ok\":true"), "{jp}");
    // bad hex / oversized frame shapes
    assert!(tun_write_json(id, "abc").contains("bad-hex"));
    let too_long = format!("00{}", "0".repeat(WRITE_MAX * 2));
    assert!(tun_write_json(id, &too_long).contains("too-long"));
    // close + double close shapes
    assert!(tun_close_json(id).contains("\"ok\":true"));
    let jdc = tun_close_json(id);
    assert!(jdc.contains("already-closed") && jdc.contains("\"errno\":0"), "{jdc}");
    assert!(tun_read_json(id).contains("\"error\":\"closed\""));
    assert!(tun_open_json(-1).contains("\"errno\":9"));
    assert!(tun_read_json(-5).contains("no-session"));
    unsafe {
        sys::close(sv[0]);
        sys::close(sv[1]);
    }
}
