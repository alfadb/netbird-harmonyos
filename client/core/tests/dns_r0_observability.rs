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
use netbird_core::socket_supply::{supply_result_line, Purpose, SocketKind};

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
    let protected = supply_result_line(42, Purpose::Dns, SocketKind::Udp, 17, "protected", 3, 5);
    assert_eq!(
        protected,
        "socket-supply: result id=42 purpose=dns kind=udp fd=17 result=protected \
         elapsed_ms=3 epoch=5"
    );
    // The refusal spelling (`result=socket-protect-failed`) is unchanged, and
    // the rejection line gets the same additive suffix.
    let refused = supply_result_line(
        43, Purpose::Management, SocketKind::Tcp, -1, "socket-protect-failed", 8, 5);
    assert_eq!(
        refused,
        "socket-supply: result id=43 purpose=management kind=tcp fd=-1 \
         result=socket-protect-failed elapsed_ms=8 epoch=5"
    );
    // The grep token every analysis script matches is still the line prefix.
    assert!(protected.starts_with("socket-supply: result id=42 "));
    assert!(refused.starts_with("socket-supply: result id=43 "));
}
