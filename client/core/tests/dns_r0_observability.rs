// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! R0 (design `analysis-20261026-dns-structural` §4.2): the TWO correlation
//! fields the EPERM capture could not join before.
//!
//! 1. `protected-dns: eperm` gains the request id and the core-owned dup fd, so
//!    one refusal can be paired BY ID with its `socket-supply: result` line
//!    instead of by (host, server, time);
//! 2. `socket-supply: result` gains the core-owned supply epoch (`epoch=`), the
//!    one platform identity the core can state without a shell self-report.
//!
//! Both additions are APPEND-ONLY: every pre-existing field keeps its position
//! and meaning, the `protected-dns:` / `socket-supply:` grep prefixes stay, and
//! no decision reads a new field (EPERM still does not retry; the fallback
//! order is untouched).
//!
//! Red first (repo convention, cf. `tests/obs_contracts.rs`): this file does
//! not compile before the observation surface exists.

use std::net::{Ipv4Addr, SocketAddrV4};

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

use netbird_core::protected_dns::eperm_line;
use netbird_core::socket_supply::{
    meta_from_reply_args, supply_result_line, Purpose, SocketKind, SocketSupply, SupplyMeta,
};

/// The R0 gap (`design.md` §5.1 #4): the EPERM record had no `id=`/`fd=`, so
/// the only way to bind it to the supply line that handed out the socket was
/// an approximate (host, server, time) join. The new fields must appear without
/// moving any pre-R0 field.
#[test]
fn eperm_line_binds_the_request_id_and_the_core_dup_fd_additively() {
    let line = eperm_line(
        "relay.example",
        1,
        2,
        SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 53), 53),
        42,
        17,
        7,
        &SupplyMeta::default(),
        "dns:send-failed errno=1 (Operation not permitted (os error 1))",
    );
    // (1) The pre-R0 prefix is byte-identical: host/stage/attempt/server kept
    // their order and spelling.
    assert!(
        line.starts_with(
            "protected-dns: eperm host=relay.example stage=1 attempt=2 server=192.0.2.53:53 "
        ),
        "{line}"
    );
    // (2) The two additive correlation fields land before the free-text tail.
    assert!(line.contains(" id=42 fd=17 errno=1 generation=7 result="), "{line}");
    // (3) The legacy `result=` tail stays last and verbatim (it carries spaces,
    // so the new fields must precede it, never follow it).
    assert!(
        line.ends_with("result=dns:send-failed errno=1 (Operation not permitted (os error 1))"),
        "{line}"
    );
}

/// The R0 gap (`design.md` §5.2 #2): the supply result line carried no
/// platform identity at all. `epoch` is the core-owned supply generation at the
/// reply instant; it is TAIL-appended after every pre-existing field.
#[test]
fn supply_result_line_appends_the_core_epoch_after_every_existing_field() {
    let protected = supply_result_line(
        42, Purpose::Dns, SocketKind::Udp, 17, "protected", 3, 5, &SupplyMeta::default());
    assert_eq!(
        protected,
        "socket-supply: result id=42 purpose=dns kind=udp fd=17 result=protected \
         elapsed_ms=3 epoch=5"
    );
    // The refusal spelling (`result=socket-protect-failed`) is unchanged, and
    // the rejection line gets the same additive suffix.
    let refused = supply_result_line(
        43, Purpose::Management, SocketKind::Tcp, -1, "socket-protect-failed", 8, 5,
        &SupplyMeta::default());
    assert_eq!(
        refused,
        "socket-supply: result id=43 purpose=management kind=tcp fd=-1 \
         result=socket-protect-failed elapsed_ms=8 epoch=5"
    );
    // The grep token every analysis script matches is still the line prefix.
    assert!(protected.starts_with("socket-supply: result id=42 "));
    assert!(refused.starts_with("socket-supply: result id=43 "));
}

/// R0 ets half (`design.md` §4.2, AC-R0-1): the shell reports the protect-time
/// network identity and the measured protect duration on the reply. The core
/// keeps that group BY REQUEST ID and prints the SAME group on the supply line
/// AND on the `protected-dns: eperm` line of a socket handed out under it, so a
/// capture can state the `net_id` a refusal belongs to without guessing from
/// (host, server, time).
#[test]
fn eperm_and_supply_lines_share_the_shell_reported_meta_group() {
    let meta = SupplyMeta {
        net_id: Some(297),
        protect_generation: Some(7),
        protect_ms: Some(34),
    };
    let supply = supply_result_line(
        42, Purpose::Dns, SocketKind::Udp, 17, "protected", 3, 5, &meta);
    assert_eq!(
        supply,
        "socket-supply: result id=42 purpose=dns kind=udp fd=17 result=protected \
         elapsed_ms=3 epoch=5 net_id=297 protect_generation=7 protect_ms=34"
    );

    let eperm = eperm_line(
        "relay.example",
        1,
        2,
        SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 53), 53),
        42,
        17,
        5,
        &meta,
        "dns:send-failed errno=1 (Operation not permitted (os error 1))",
    );
    // The group is additive and sits between the existing generation= token and
    // the free-text `result=` tail — no pre-R0 field moved, `result=` stays last.
    assert!(
        eperm.contains(
            " id=42 fd=17 errno=1 generation=5 net_id=297 protect_generation=7 protect_ms=34 result="
        ),
        "{eperm}"
    );
    assert!(eperm.starts_with(
        "protected-dns: eperm host=relay.example stage=1 attempt=2 server=192.0.2.53:53 "), "{eperm}");
    assert!(eperm.ends_with(
        "result=dns:send-failed errno=1 (Operation not permitted (os error 1))"), "{eperm}");

    // A partial group prints only what the shell sent (an older shell sends
    // nothing at all, which is the byte-identical pre-R0 line).
    let partial = SupplyMeta { net_id: Some(301), ..SupplyMeta::default() };
    assert!(supply_result_line(44, Purpose::Dns, SocketKind::Udp, 18, "protected", 2, 5, &partial)
        .ends_with(" epoch=5 net_id=301"));
}

/// R0: the optional reply numbers are the transport shape of the ets half. A
/// negative value is the shell's "unknown" sentinel (the pre-R0 signature's
/// absence), so it must NOT be printed as a fabricated field — while a real 0
/// (a platform net id of 0, a generation 0) is an honest value.
#[test]
fn reply_meta_numbers_map_the_shell_unknown_sentinel_to_absent() {
    assert_eq!(meta_from_reply_args(-1, -1, -1), SupplyMeta::default());
    assert_eq!(
        meta_from_reply_args(0, 0, 0),
        SupplyMeta {
            net_id: Some(0),
            protect_generation: Some(0),
            protect_ms: Some(0),
        }
    );
    assert_eq!(
        meta_from_reply_args(297, 7, 34),
        SupplyMeta {
            net_id: Some(297),
            protect_generation: Some(7),
            protect_ms: Some(34),
        }
    );
}

/// R0: the core keeps the shell's group by request id so the EPERM evidence of
/// a socket handed out under it can state the same `net_id`. The retention is
/// evidence only — a reply WITHOUT the optional args (the pre-R0 shell) leaves
/// no group, and an unknown id answers `None` (never a fabricated default).
#[tokio::test]
async fn reply_meta_is_retained_by_request_id_for_the_eperm_join() {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::sync::Arc;

    let supply = Arc::new(SocketSupply::new());
    let task = tokio::spawn({
        let supply = supply.clone();
        async move { supply.request_with_id(Purpose::Dns).await }
    });
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let id = loop {
        if let Some((id, _purpose)) = supply.poll_request() { break id; }
        assert!(tokio::time::Instant::now() < deadline, "the request was never queued");
        tokio::task::yield_now().await;
    };
    assert_eq!(supply.request_meta(id), None, "no reply yet, no group");

    let raw = netbird_core::host_sockets::open_udp_unbound().expect("udp socket");
    // The shell keeps its original fd until handoff; the core only holds the dup.
    let shell_fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let meta = meta_from_reply_args(297, 7, 34);
    assert_eq!(supply.reply_with_meta(id, raw, "protected", &meta), "{\"ok\":true}");
    let (_id, delivered) = task.await.expect("dial task").expect("protected fd");
    assert_ne!(delivered.as_raw_fd(), raw, "the dial owns the dup only");
    drop(shell_fd); // the shell's `finally { socketSupplyCloseFd(fd) }`
    assert_eq!(supply.request_meta(id), Some(meta), "the group is kept by request id");
    assert_eq!(supply.request_meta(id + 9999), None, "an unknown id has no group");

    // The pre-R0 reply shape (no optional args) must stay byte-identical: no
    // group is stored and none can be printed for the EPERM of that request.
    let task = tokio::spawn({
        let supply = supply.clone();
        async move { supply.request_with_id(Purpose::Dns).await }
    });
    let id = loop {
        if let Some((id, _purpose)) = supply.poll_request() { break id; }
        assert!(tokio::time::Instant::now() < deadline, "the second request was never queued");
        tokio::task::yield_now().await;
    };
    let raw = netbird_core::host_sockets::open_udp_unbound().expect("udp socket");
    let shell_fd = unsafe { OwnedFd::from_raw_fd(raw) };
    assert_eq!(supply.reply(id, raw, "protected"), "{\"ok\":true}");
    let _delivered = task.await.expect("dial task").expect("protected fd");
    drop(shell_fd);
    assert_eq!(supply.request_meta(id), None, "an old-shell reply carries no group");
}

