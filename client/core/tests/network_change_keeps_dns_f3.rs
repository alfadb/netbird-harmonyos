// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! F3 (v0.0.27 outage REPORT.md §5 / §2.5 / §3 R3): a physical network change
//! must NEVER empty the shell-fed protected-DNS server list.
//!
//! The pre-F3 `ConnectorHandle::physical_network_changed` cleared both resolver
//! lists whenever the caller carried no new management address
//! (`set_dns_servers(Vec::new())` plus an empty STUN-DNS refresh), and the shell
//! called it with `''` on every switch. The shell could only refill the list
//! from inside a recovery that the same switch's first failed step then aborted
//! — which is why the device capture holds six `dns:no-physical-server` lines,
//! each 1 ms after a switch (REPORT.md §2.5), and why the core could not
//! resolve anything for the rest of the outage.
//!
//! Driven through the REAL NAPI seam the shell uses
//! (`connector_start_with_socket` → `connector_physical_network_changed` →
//! `connector_protected_dns_start`). `connector_protected_dns_start` answers
//! `{"ok":false}` exactly when the supply's server list is EMPTY
//! (src/connector.rs:6987), so its answer is a direct read of the invariant.
//!
//! ONE test fn on purpose: the seam reads the PROCESS-GLOBAL connector and
//! socket-supply slots, so nothing in this binary may race it.

use std::os::fd::AsRawFd;

// Host-test link surface (same block as tests/connector.rs /
// tests/connector_resource_status.rs): the test binary links the whole crate
// rlib on the host triple, where libace_napi.z.so / libhilog_ndk.z.so do not
// exist. No-ops for the linker; never compiled into the shipped .so.
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
        _name: *const u8,
        _length: usize,
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
        _this: *mut c_void,
        _data: *mut c_void,
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

/// A network change with NO new management address keeps the DNS list; the
/// address-bearing shape keeps working and keeps it too; an EXPLICIT empty
/// re-feed still empties it (the wipe stays available as an explicit shell
/// action — it is merely no longer a side effect of a network change).
#[test]
fn a_network_change_without_an_address_keeps_the_protected_dns_list() {
    use netbird_core::connector::{
        connector_physical_network_changed_json, connector_protected_dns_start_json,
        connector_start_with_socket_json, connector_stop_json,
    };
    use netbird_core::socket_supply::set_dns_servers_json;

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a local dial target");
    let dial = listener.local_addr().expect("local addr");
    let cfg = format!(
        "{{\"management_url\":\"http://{dial}\",\"private_key\":\"{}\"}}",
        netbird_core::util::base64(&[0x22u8; 32])
    );

    // The shell's start document carries the resolved address AND the physical
    // DNS servers it derived from the current network — the same fields
    // `connectorStartWithSocket` sends.
    let addr_json =
        format!("{{\"connect_addr\":\"{dial}\",\"dns_servers\":[\"192.0.2.1\",\"198.51.100.53\"]}}");
    let started = connector_start_with_socket_json(
        listener.as_raw_fd(),
        &cfg,
        "{\"setup_key\":\"SETUP-F3\"}",
        &addr_json,
    );
    assert!(started.contains("\"started\":true"), "start refused: {started}");

    // The list is live: a protected lookup is ACCEPTED (it then waits for a
    // socket this test never serves — the acceptance is the signal).
    let before = connector_protected_dns_start_json("api.example.test");
    assert!(
        before.contains("\"ok\":true"),
        "the shell-fed DNS list must be installed: {before}"
    );

    // The EXACT pre-F3 call the shell made on every switch: no new address.
    let changed = connector_physical_network_changed_json("\"\"");
    assert!(changed.contains("\"ok\":true"), "the change must apply: {changed}");

    // F3: the list survived. Pre-F3 this answered `{"ok":false}` because
    // `physical_network_changed` had just called `set_dns_servers(Vec::new())`.
    let after = connector_protected_dns_start_json("api.example.test");
    assert!(
        after.contains("\"ok\":true"),
        "an address-less network change must keep the DNS server list: {after}"
    );

    // The address-bearing shape — what the recovery sends once it has resolved
    // the new management address — keeps working, and keeps the list too.
    let with_addr =
        connector_physical_network_changed_json(&format!("{{\"connect_addr\":\"{dial}\"}}"));
    assert!(with_addr.contains("\"ok\":true"), "{with_addr}");
    let still = connector_protected_dns_start_json("api.example.test");
    assert!(still.contains("\"ok\":true"), "{still}");

    // An EXPLICIT empty re-feed is still honoured: the wipe is a shell
    // decision (an empty `physicalDefaultDnsServers`), not a side effect.
    let fed = set_dns_servers_json("[]");
    assert!(fed.contains("\"ok\":true"), "{fed}");
    let drained = connector_protected_dns_start_json("api.example.test");
    assert!(
        drained.contains("\"ok\":false"),
        "an explicit empty feed must still empty the list: {drained}"
    );

    let stopped = connector_stop_json();
    assert!(stopped.contains("\"already_stopped\":false"), "{stopped}");
}
