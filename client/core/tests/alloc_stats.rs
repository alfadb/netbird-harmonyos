// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! Rust-heap accounting (`alloc_stats`) — real-allocator tests.
//!
//! This file is its own test PROCESS and holds exactly ONE test function:
//! libtest runs the test functions of one binary in parallel threads, and
//! parallel allocation would make the byte DELTAS below unattributable. With
//! one test body running while libtest's main thread is parked on its join,
//! every allocation between two snapshots is this test's own — which is what
//! makes exact assertions (not merely "the counter moved") legitimate here.
//!
//! It also PROVES the installation: `lib.rs` declares the crate's
//! `#[global_allocator]`, and an integration test binary links the crate as an
//! rlib. If that allocator were not picked up for this binary, every counter
//! below would read 0 and the first assertion would fail.

use netbird_core::alloc_stats;

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

/// Large enough that nothing incidental can hide the block, small enough to be
/// trivial for a test process.
const BLOCK: u64 = 8 * 1024 * 1024;

#[test]
fn rust_heap_counters_track_a_held_and_released_vec() {
    let base = alloc_stats::snapshot();

    // ONE allocation, no reallocation: `with_capacity` performs the single
    // alloc this test measures and `push` stays inside the spare capacity.
    let mut payload: Vec<u8> = Vec::with_capacity(BLOCK as usize);
    payload.push(0xA5);

    let held = alloc_stats::snapshot();
    assert!(
        held.bytes >= base.bytes + BLOCK,
        "a live {} MiB Vec must show up in the live-byte counter: base={base:?} held={held:?}",
        BLOCK / (1024 * 1024)
    );
    assert!(
        held.allocs > base.allocs,
        "one allocation must move the allocation counter: base={base:?} held={held:?}"
    );
    assert!(
        held.frees >= base.frees,
        "the free counter is cumulative and cannot decrease: base={base:?} held={held:?}"
    );
    assert!(held.peak >= held.bytes, "peak must cover the live bytes: {held:?}");

    drop(payload);

    let after = alloc_stats::snapshot();
    assert!(
        after.bytes + BLOCK <= held.bytes,
        "releasing the Vec must give its bytes back: held={held:?} after={after:?}"
    );
    assert!(
        after.frees > held.frees,
        "the drop must move the free counter: held={held:?} after={after:?}"
    );
    // Monotonic (cumulative) counters: they only ever grow, in this order.
    assert!(
        after.allocs >= held.allocs,
        "the allocation counter is cumulative: held={held:?} after={after:?}"
    );
    assert!(
        after.peak >= held.peak,
        "peak is a high-water mark and never falls: held={held:?} after={after:?}"
    );
    assert!(after.peak >= after.bytes, "peak still covers the live bytes: {after:?}");

    // The NAPI-shaped document carries the same four numbers, flat, numeric and
    // in the frozen order `bytes, peak, allocs, frees` (no `ok`, no extras).
    let json = alloc_stats::snapshot_json();
    assert!(json.starts_with("{\"bytes\":") && json.ends_with('}'), "{json}");
    assert_eq!(
        (json.matches(':').count(), json.matches(',').count()),
        (4, 3),
        "exactly four flat fields: {json}"
    );
    let keys = ["\"bytes\":", "\"peak\":", "\"allocs\":", "\"frees\":"];
    let positions: Vec<usize> = keys
        .iter()
        .map(|key| json.find(key).unwrap_or_else(|| panic!("{key} missing in {json}")))
        .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "key order changed: {json}");
}
