// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! R1 (design `analysis-20261026-dns-structural` §4.2): reprotect-once — the
//! ONE-SHOT diagnostic that decides "the protect mark is recoverable" versus
//! "the protected path is down".
//!
//! On the FIRST EPERM of one resolution chain the core hands the shell the very
//! descriptor it is about to re-send on; the shell re-issues the ONE write-side
//! SDK call (`VpnConnection.protect(fd)`) on that SAME descriptor and answers
//! `ok`/`fail` + the platform error code; the core then sends the identical
//! query once more on that SAME socket, bounded. Everything is a bounded,
//! additive diagnostic: the EPERM no-retry rule, the fallback order and the
//! supply protocol are untouched, and a failed/timed-out experiment simply
//! continues into the unchanged stage 3 → 4.
//!
//! Red first (repo convention, cf. `tests/dns_r0_observability.rs`): this file
//! does not compile before the R1 surface exists.

use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;

// Host-test link surface (repo convention, cf. tests/obs_contracts.rs): the
// test binary links the whole crate rlib on the host triple, where
// libace_napi.z.so / libhilog_ndk.z.so do not exist. No-ops for the linker
// only; never called by these tests.
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
    pub extern "C" fn napi_create_string_utf8(
        _env: *mut c_void,
        _str_: *const u8,
        _len: usize,
        _result: *mut *mut c_void,
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
    pub extern "C" fn napi_get_value_string_utf8(
        _value: *mut c_void,
        _buf: *mut u8,
        _bufsize: usize,
        _result: *mut usize,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_int32(
        _value: *mut c_void,
        _result: *mut i32,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(
        _value: *mut c_void,
        _result: *mut bool,
    ) -> i32 {
        0
    }
}

use netbird_core::protected_dns::{reprotect_line, resend_line};
use netbird_core::socket_supply::{
    poll_reprotect_json, reprotect_reply_json, set_notify, shell_listening, ReprotectOutcome,
    ReprotectResult, SocketSupply,
};

/// The process-wide wake hook is shared by every test of this binary; the R1
/// handshake tests must not race each other's listener.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Installs a no-op wake listener (the "a shell is subscribed" fact the core
/// checks before it spends its bound) and clears it on drop.
struct Listener;

impl Listener {
    fn install() -> Self {
        set_notify(Some(Arc::new(|_id: u64| {})));
        assert!(shell_listening());
        Self
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        set_notify(None);
    }
}

fn outcome_line(outcome: ReprotectOutcome, id: u64, protect_ms: Option<u64>, elapsed_ms: u128) -> String {
    reprotect_line(
        "relay.example",
        1,
        2,
        SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 53), 53),
        42,
        17,
        &ReprotectResult { id, outcome, protect_ms, elapsed_ms },
    )
}

/// R1's first line: WHERE the experiment ran (host/stage/attempt/server), WHICH
/// socket (`id`/`fd`, the same pair the EPERM record carries), what the shell
/// answered and how long each leg took. Additive beside the pre-R1 records.
#[test]
fn reprotect_line_binds_the_attempt_the_socket_and_the_shells_answer() {
    let ok = outcome_line(ReprotectOutcome::Ok, 7, Some(3), 5);
    assert!(
        ok.starts_with(
            "protected-dns: reprotect host=relay.example stage=1 attempt=2 server=192.0.2.53:53 "
        ),
        "{ok}"
    );
    assert!(ok.contains(" id=42 fd=17 reprotect_id=7 protect_ms=3 elapsed_ms=5 result=ok"), "{ok}");
    assert!(ok.ends_with("result=ok"), "{ok}");

    // A shell-side refusal is distinguishable from a recoverable mark: the
    // platform code rides the record (never invented — `-1` when the shell had
    // none), and the result token is `fail`, not `ok`.
    let failed = outcome_line(ReprotectOutcome::Failed { errno: 13 }, 7, Some(4), 6);
    assert!(
        failed.contains(" id=42 fd=17 reprotect_id=7 errno=13 protect_ms=4 elapsed_ms=6 result=fail"),
        "{failed}"
    );

    // The two non-answers: nobody answered inside the bound / nobody could be
    // asked at all. Both are the "供给通道也受影响" row of the judgement table.
    let timeout = outcome_line(ReprotectOutcome::Timeout, 7, None, 250);
    assert!(timeout.contains("reprotect_id=7 elapsed_ms=250 result=timeout"), "{timeout}");
    assert!(!timeout.contains("protect_ms="), "nothing was measured: {timeout}");
    let unavailable = outcome_line(ReprotectOutcome::Unavailable, 0, None, 0);
    assert!(
        unavailable.contains("reprotect_id=0 elapsed_ms=0 result=unavailable"),
        "{unavailable}"
    );

    // The grep prefix and the pre-R1 field vocabulary are unchanged.
    for line in [&ok, &failed, &timeout, &unavailable] {
        assert!(line.starts_with("protected-dns: reprotect "), "{line}");
        assert!(!line.contains("=None"), "an absent value is never printed as Rust: {line}");
    }
}

/// R1's second line: the ONE re-send on the SAME socket. Its `result=` is the
/// syscall's own token (so a repeated EPERM is readable verbatim) or `ok`.
#[test]
fn resend_line_carries_the_second_sends_own_syscall_result() {
    let ok = resend_line(
        "relay.example",
        1,
        2,
        SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 53), 53),
        42,
        17,
        &Ok(()),
        1,
    );
    assert!(
        ok.starts_with(
            "protected-dns: resend host=relay.example stage=1 attempt=2 server=192.0.2.53:53 "
        ),
        "{ok}"
    );
    assert!(ok.contains(" id=42 fd=17 elapsed_ms=1 result=ok"), "{ok}");

    let eperm = resend_line(
        "relay.example",
        1,
        2,
        SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 53), 53),
        42,
        17,
        &Err(std::io::Error::other(
            "dns:send-failed errno=1 (Operation not permitted (os error 1))",
        )),
        0,
    );
    assert!(
        eperm.ends_with("result=dns:send-failed errno=1 (Operation not permitted (os error 1))"),
        "{eperm}"
    );

    // The send leg's own bound has its own token: it is NOT a silent DNS
    // timeout (the chain's `dns:timeout` means "no answer", this means "the
    // second send never got to run").
    let bounded = resend_line(
        "relay.example", 1, 2, SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 53), 53), 42, 17,
        &Err(std::io::Error::other("dns:resend-timeout")), 250,
    );
    assert!(bounded.ends_with("elapsed_ms=250 result=dns:resend-timeout"), "{bounded}");
}

/// The wire half: the shell polls the ONE pending reprotect request (getting the
/// SAME descriptor the core is sending on) and answers it by id. The core's wait
/// resolves with exactly the shell's answer; a stale id answers "expired" (the
/// core already moved on) instead of silently overwriting anything.
#[tokio::test]
async fn the_reprotect_handshake_is_poll_answer_and_expired() {
    let _serial = serial();
    let _listener = Listener::install();
    let supply = Arc::new(SocketSupply::new());
    let waiter = tokio::spawn({
        let supply = Arc::clone(&supply);
        async move { supply.reprotect(41, Duration::from_millis(250)).await }
    });
    let (id, fd) = loop {
        if let Some(request) = supply.poll_reprotect() {
            break request;
        }
        assert!(!waiter.is_finished(), "no answer can arrive without a poll");
        tokio::task::yield_now().await;
    };
    assert_eq!(fd, 41, "the shell protects the descriptor the core will re-send on");
    assert!(id > 0, "the request carries a real id");

    let accepted = supply.reprotect_reply(id, "fail", 13, 7);
    assert_eq!(accepted, "{\"ok\":true}");
    let answer = waiter.await.expect("the core's wait");
    assert_eq!(answer.id, id);
    assert_eq!(answer.outcome, ReprotectOutcome::Failed { errno: 13 });
    assert_eq!(answer.protect_ms, Some(7), "the shell's measured protect duration rides the answer");

    // Late answer for a request the core already resolved: refused, not applied.
    let expired = supply.reprotect_reply(id, "ok", 0, 1);
    assert_eq!(expired, "{\"ok\":false,\"error\":\"request-expired\"}");

    // The napi documents keep the supply vocabulary for a missing supply/id.
    assert_eq!(reprotect_reply_json("not-a-number", "ok", 0, 1),
        "{\"ok\":false,\"error\":\"invalid-id\"}");
    assert_eq!(reprotect_reply_json("1", "ok", 0, 1),
        "{\"ok\":false,\"error\":\"socket-supply-closed\"}");
    assert_eq!(poll_reprotect_json(), "{\"id\":\"\",\"fd\":\"\"}");
}

/// No shell is subscribed → the diagnostic is SKIPPED, not waited out: the
/// request never enters a queue, so no shell can later protect a descriptor the
/// core has already closed.
#[tokio::test]
async fn without_a_listening_shell_the_experiment_is_unavailable_at_once() {
    let _serial = serial();
    set_notify(None);
    assert!(!shell_listening());
    let supply = Arc::new(SocketSupply::new());
    let answer = supply.reprotect(41, Duration::from_millis(250)).await;
    assert_eq!(answer, ReprotectResult {
        id: 0,
        outcome: ReprotectOutcome::Unavailable,
        protect_ms: None,
        elapsed_ms: 0,
    });
    assert!(supply.poll_reprotect().is_none(), "nothing was enqueued");
}

/// A shell that is subscribed but silent → the bound decides, and the request is
/// WITHDRAWN when the shell never took it: a late poll finds nothing, so a
/// closed descriptor number can never be protected by accident.
#[tokio::test]
async fn a_silent_shell_times_out_within_the_bound_and_the_request_is_withdrawn() {
    let _serial = serial();
    let _listener = Listener::install();
    let supply = Arc::new(SocketSupply::new());
    let answer = supply.reprotect(41, Duration::from_millis(120)).await;
    assert_eq!(answer.outcome, ReprotectOutcome::Timeout);
    assert!(answer.id > 0, "the record still names the request the shell never answered");
    assert!(
        (120..150).contains(&answer.elapsed_ms),
        "the bound decides (timer granularity aside): {}",
        answer.elapsed_ms
    );
    assert!(supply.poll_reprotect().is_none(), "the shell never took it, so it is withdrawn");
}
