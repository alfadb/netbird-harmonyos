//! R2-B close-attribution regression — INTEGRATION-PROCESS test.
//!
//! Deterministic reproduction of the ledger close-attribution ambiguity at
//! the REAL tun boundary (open_session / session_close), complementing the
//! pure-ledger unit tests in src/ledger.rs:
//!
//! 1. a session's dup is foreign-closed behind our back -> its ledger entry
//!    stays open (the no-fabricated-close contract) while the NUMBER becomes
//!    reusable;
//! 2. a new session dups the same raw fd: dup(2)/F_DUPFD hand out the lowest
//!    free number, so the freed number is deterministically reused — and the
//!    test self-checks that assumption with an explicit assert (if the
//!    kernel ever handed out a different number, this fails loudly instead
//!    of passing vacuously);
//! 3. closing the NEW session must close ITS OWN ledger entry
//!    (probe-protocol-close), while the stale same-number entry stays open.
//!    Before the fix (role+fd first-match in `emit_close`) the close landed
//!    on the stale entry and the new entry stayed open forever — the 2/240
//!    pressure flake in close_once_double_close_and_use_after_close.
//!
//! Serialization: this binary contains a single test, and cargo runs
//! integration test binaries sequentially, so no lock is required for the
//! fd-number reuse window. The ledger/session table are process-global; no
//! assertion here depends on concurrency.
//!
//! Host link stubs: same OHOS-only symbol set as tests/tun_fd_contract.rs.

use netbird_core::ledger;
use netbird_core::sys;
use netbird_core::tun::{open_session, session_close};

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
    _data: *mut core::ffi::c_void,
) -> i32 {
    0
}

#[no_mangle]
extern "C" fn napi_create_string_utf8(
    _e: *mut core::ffi::c_void,
    _s: *const u8,
    _l: usize,
    _r: *mut *mut core::ffi::c_void,
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
}

fn sp() -> [i32; 2] {
    let mut sv = [-1i32; 2];
    let r = unsafe { socketpair(1 /* AF_UNIX */, 1 /* SOCK_STREAM */, 0, &mut sv) };
    assert_eq!(r, 0, "socketpair failed errno={}", sys::errno());
    sv
}

/// The `fd_dup#inst|fd|` prefix of the fd_dup entry created between
/// `before_lines` and now (diff-based, same technique as tun_fd_contract).
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

fn line_with_prefix(lines: &str, prefix: &str) -> String {
    lines
        .lines()
        .find(|l| l.starts_with(prefix))
        .unwrap_or_else(|| panic!("entry {prefix} present"))
        .to_string()
}

// ---------------------------------------------------------------------------

#[test]
fn session_close_after_foreign_close_and_number_reuse_lands_on_its_own_entry() {
    let sv = sp();

    // instance 1: open, then the kernel dup is foreign-closed behind our
    // back — the ledger entry STAYS open (no fabricated close) and the
    // NUMBER becomes reusable
    let before1 = ledger::canonical_lines();
    let (id1, fd_dup1, _) = open_session(sv[0]).expect("open session 1");
    let prefix1 = new_dup_prefix(&before1, fd_dup1);
    unsafe { sys::close(fd_dup1) };

    // instance 2: the freed number is reused (lowest-free dup semantics —
    // self-checked below so a violated assumption fails loudly)
    let before2 = ledger::canonical_lines();
    let (id2, fd_dup2, _) = open_session(sv[0]).expect("open session 2");
    let prefix2 = new_dup_prefix(&before2, fd_dup2);
    assert_eq!(fd_dup2, fd_dup1, "kernel must reuse the freed number for this test to be the reuse scenario");
    assert_ne!(prefix1, prefix2, "two distinct ledger instances share the number");

    // pre-close state: BOTH entries open on the same fd number
    let before_close = ledger::canonical_lines();
    let stale_before = line_with_prefix(&before_close, &prefix1);
    let own_before = line_with_prefix(&before_close, &prefix2);
    assert!(stale_before.contains("|open|none|none"), "{stale_before}");
    assert!(own_before.contains("|open|none|none"), "{own_before}");

    // the NEW session closes: must mark ITS OWN entry, not the stale one
    assert_eq!(session_close(id2).expect("close session 2"), fd_dup2);
    let after = ledger::canonical_lines();
    let own_after = line_with_prefix(&after, &prefix2);
    let stale_after = line_with_prefix(&after, &prefix1);
    assert!(
        own_after.contains("probe-protocol-close"),
        "the new instance's close must land on its own entry: {own_after}"
    );
    assert_eq!(
        stale_after, stale_before,
        "the stale same-number entry must stay exactly as it was"
    );
    assert!(
        stale_after.contains("|open|none|none"),
        "the stale entry must remain open (never mis-closed): {stale_after}"
    );

    // cleanup: closing session 1 hits the (now free) reused number -> EBADF;
    // that failed close must still not fabricate a clean ledger close
    let before_cleanup = ledger::canonical_lines();
    let _ = session_close(id1).expect("session 1 close consumed");
    assert_eq!(
        ledger::canonical_lines(),
        before_cleanup,
        "failed close of a foreign-closed dup must not fabricate a ledger close"
    );
    unsafe {
        sys::close(sv[0]);
        sys::close(sv[1]);
    }
}
