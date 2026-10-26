// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! R3 (REPORT §6.4 / `analysis-20261025-upstream-compare` §6): "is there a
//! network at all" is NOT the same event as "the network changed".
//!
//! Upstream keeps the two apart (`client/netevents/netevents.go:48-68`):
//! `SetNetworkAvailable` drives the availability bit the peer guard reads
//! (`peer/guard/guard.go:123-125` — offline ticks skip the attempt and spend no
//! ICE retry budget), while `NotifyNetworkChange` sweeps the stale sockets.
//! This core had only the change event. R3 adds the availability entry plus an
//! explicit host-resume observation.
//!
//! This file pins the two new SYNCHRONOUS NAPI documents (`connector: ` log
//! lines ride the frozen HiLog channel) and the process-global availability
//! record that survives a connector that is not running yet — the shell
//! registers its network observer BEFORE the start leg, so an offline start
//! must not be lost.
//!
//! Red first (repo convention, cf. `tests/dns_r2_fastfail.rs`): this file does
//! not compile before the R3 surface exists.

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

use netbird_core::connector::{
    connector_host_resumed_json, connector_network_available_json,
};

/// ONE test drives both process-global surfaces: the availability record is
/// shared by every caller in this process (the shell calls it from the network
/// observer), so two parallel tests mutating it would race. The default is
/// ONLINE — an unknown availability never gates a startup dial.
#[test]
fn the_availability_and_resume_exports_record_their_inputs_without_a_connector() {
    // offline: recorded even with no connector installed (the observer runs
    // BEFORE the start leg), and reported as a real transition.
    let offline = connector_network_available_json(false);
    assert_eq!(
        offline,
        "{\"ok\":true,\"available\":false,\"changed\":true,\"applied\":false}",
        "the offline report must be recorded before any connector exists"
    );
    // the SAME report again is not a transition (no duplicate core log line).
    let repeat = connector_network_available_json(false);
    assert_eq!(
        repeat,
        "{\"ok\":true,\"available\":false,\"changed\":false,\"applied\":false}"
    );
    // back online: a real transition again.
    let online = connector_network_available_json(true);
    assert_eq!(
        online,
        "{\"ok\":true,\"available\":true,\"changed\":true,\"applied\":false}"
    );

    // the resume observation carries the shell's wall-clock gap verbatim; the
    // absence of a connector is reported honestly (`applied:false`), never as
    // an invented action.
    let resumed = connector_host_resumed_json(16_000);
    assert_eq!(
        resumed,
        "{\"ok\":true,\"gap_ms\":16000,\"applied\":false}"
    );
    let boundary = connector_host_resumed_json(15_000);
    assert_eq!(boundary, "{\"ok\":true,\"gap_ms\":15000,\"applied\":false}");
}
