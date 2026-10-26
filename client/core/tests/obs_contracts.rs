// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! Observation contracts for the three analysis-plan increments (red first:
//! this file does not COMPILE before the observation surface exists).
//!
//! - 3-D: the structured EPERM record carries the socket-supply network
//!   generation plus the refused server / attempt sequence, and the generation
//!   counter advances exactly at `cancel_pending_for_network_change` (the
//!   connector's physical-network-change path) — evidence only, the EPERM
//!   retry/degrade logic is untouched;
//! - R1: the `initial=1` variant of the `direct->carrier` edge is a stable
//!   token beside the plain edge;
//! - 2-B: the warm-up classifier is true ONLY for the never-direct startup
//!   window (carrier attached, ICE not selected, no direct endpoint ever).

// Host-process link stubs (repo convention, cf. tests/peer_conn_e2e.rs):
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

use std::net::{Ipv4Addr, SocketAddrV4};

use netbird_core::peer_conn::carrier_warmup;
use netbird_core::protected_dns::eperm_line;
use netbird_core::socket_supply::{SocketSupply, SupplyMeta};
use netbird_core::wg_device::direct_to_carrier_initial_line;

#[test]
fn eperm_record_pins_host_stage_attempt_server_id_fd_errno_and_generation() {
    // R0: `id=`/`fd=` are the request id and the core-owned dup fd this refusal
    // belongs to; they are inserted before the free-text `result=` tail and no
    // pre-R0 field moved. An EMPTY shell meta group (an old shell, or a reply
    // that carried no optional args) keeps this line byte-identical: the ets
    // half's `net_id=` group is additive only when the shell reports it.
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
    assert_eq!(
        line,
        "protected-dns: eperm host=relay.example stage=1 attempt=2 server=192.0.2.53:53 \
         id=42 fd=17 errno=1 generation=7 result=dns:send-failed errno=1 (Operation not permitted (os error 1))"
    );
}

#[test]
fn socket_supply_generation_advances_on_each_network_change_and_nothing_else() {
    let supply = SocketSupply::new();
    assert_eq!(supply.generation(), 0, "a fresh supply is generation 0");
    supply.cancel_pending_for_network_change();
    assert_eq!(supply.generation(), 1, "the network-change cancellation advances the generation");
    supply.cancel_pending_for_network_change();
    assert_eq!(supply.generation(), 2, "every reported network change is its own generation");
}

#[test]
fn carrier_warmup_is_only_the_never_direct_startup_window() {
    assert!(carrier_warmup(true, false, false), "carrier up, ICE unselected, never direct");
    assert!(!carrier_warmup(true, false, true), "a peer that had a direct path is a real fallback");
    assert!(!carrier_warmup(true, true, false), "ICE already selected is not the warm-up window");
    assert!(!carrier_warmup(false, false, false), "no carrier attached, no relay warm-up");
}

#[test]
fn initial_direct_to_carrier_edge_is_a_stable_additive_token() {
    let key = "bjEzLWQxLXBlZXItc2Vzc2lvbg==";
    assert_eq!(
        direct_to_carrier_initial_line(key),
        "N13_WG|direct->carrier|no-ice-endpoint|initial=1|peer=bjEzLWQx"
    );
    // The plain edge builder is untouched: `initial=1` is the ONLY difference.
    assert_eq!(
        netbird_core::wg_device::direct_to_carrier_line(key),
        "N13_WG|direct->carrier|no-ice-endpoint|peer=bjEzLWQx"
    );
}
