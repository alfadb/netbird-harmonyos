// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N5a STUN URI + entropy tests at the module boundary. The RFC 5769 wire
//! vectors live in-module (`src/stun.rs` #[cfg(test)]); here we pin what a
//! caller of the gather touches: URI parsing (`stun:` scheme, default port,
//! `turn:` rejection — upstream `stun.ParseURI` shape, engine.go:1529-1537)
//! and the entropy guarantee of the transaction id.

use netbird_core::ice::parse_stun_uri;
use netbird_core::management::ManagementError;
use netbird_core::stun::random_transaction_id;

// Host-test link surface (same as tests/management_grpc.rs): the integration
// test binary links the whole crate rlib on the host triple, where
// libace_napi.z.so / libhilog_ndk.z.so do not exist. These no-ops satisfy the
// linker only; nothing below calls into them.
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
        _cb: *const c_void,
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
        _this_arg: *mut *mut c_void,
        _data: *mut *mut c_void,
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

/// Upstream shape: `stun:stun.netbird.io:3478` (the value management
/// actually sends — see `network_map.rs` fixture), explicit port kept.
#[test]
fn parses_stun_uri_with_port() {
    let s = parse_stun_uri("stun:stun.netbird.io:3478").expect("parses");
    assert_eq!(s.host, "stun.netbird.io");
    assert_eq!(s.port, 3478);
    let s = parse_stun_uri("stun:1.2.3.4:3479").expect("parses");
    assert_eq!((s.host.as_str(), s.port), ("1.2.3.4", 3479));
}

/// Port omitted → pion stun default 3478 (upstream URIs from management
/// always carry a port; the default matches `stun.ParseURI`).
#[test]
fn stun_uri_default_port_is_3478() {
    let s = parse_stun_uri("stun:stun.example.net").expect("parses");
    assert_eq!(s.host, "stun.example.net");
    assert_eq!(s.port, 3478);
}

/// `turn:` is NOT silently accepted — N5a has no relay client, and a
/// silently-dropped TURN entry would masquerade as a working STUN set.
#[test]
fn turn_uri_is_unsupported_url() {
    for uri in ["turn:turn.example.net:3478", "TURN:turn.example.net"] {
        assert!(
            matches!(parse_stun_uri(uri), Err(ManagementError::UnsupportedUrl(_))),
            "{uri} must be UnsupportedUrl"
        );
    }
}

/// Every other malformed shape lands in the taxonomy with the right class:
/// missing scheme / wrong scheme / IPv6 literal (udp4-only N5a) →
/// UnsupportedUrl; empty host, bad port, port 0 → Request{status: 0}
/// (local pre-flight validation, management.rs convention).
#[test]
fn malformed_stun_uris_are_classified() {
    for uri in ["stun.example.net:3478", "ftp:host", "TURN:host", "stun:[::1]:3478"] {
        assert!(
            matches!(parse_stun_uri(uri), Err(ManagementError::UnsupportedUrl(_))),
            "{uri} must be UnsupportedUrl"
        );
    }
    for uri in ["stun:", "stun:host:0", "stun:host:notaport"] {
        assert!(
            matches!(parse_stun_uri(uri), Err(ManagementError::Request { status: 0, .. })),
            "{uri} must be a local Request error"
        );
    }
}

/// Transaction ids are non-constant (real /dev/urandom on the host test
/// runner): two draws differ, and each is exactly 12 bytes. On-device the
/// same read failing would fail the exchange (fail-closed entropy).
#[test]
fn transaction_ids_come_from_entropy() {
    let a = random_transaction_id().expect("entropy available");
    let b = random_transaction_id().expect("entropy available");
    assert_ne!(a.0, b.0, "two urandom draws must differ");
}
