// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! R2 (design `analysis-20261026-dns-structural` §4.2): EPERM means "the
//! protected path is unavailable", and that verdict must be a FIRST-CLASS
//! STATE, not one line of evidence.
//!
//! This file pins the surface the SHELL reads back (`socket_supply_path_state`,
//! the one new synchronous NAPI document) and the two stable tokens both halves
//! share:
//!
//! - `protected-path-unusable` — the reason token the shell's supply gate
//!   answers with (`event=socket_supply … reason=protected-path-unusable`);
//! - `socket-supply-unusable` — the core's error token for a DNS request
//!   refused while that state is set (mgmt/signal/relay are NOT refused).
//!
//! Red first (repo convention, cf. `tests/dns_r0_observability.rs`): this file
//! does not compile before the R2 surface exists.

use std::os::fd::{FromRawFd, OwnedFd};
use std::sync::Arc;
use std::time::{Duration, Instant};

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

use netbird_core::protected_dns::{eperm_state_line, EpermSummary};
use netbird_core::socket_supply::{
    install, path_state_json, shutdown, Purpose, SocketSupply, PROTECTED_PATH_UNUSABLE,
    SUPPLY_UNUSABLE_TOKEN,
};

/// The two stable tokens both halves of R2 share: the shell's refusal reason
/// and the core's error token. Changing either without the other silently
/// turns a fast failure back into an 8 s supply wait.
#[test]
fn the_fast_fail_tokens_are_the_stable_ones() {
    assert_eq!(PROTECTED_PATH_UNUSABLE, "protected-path-unusable");
    assert_eq!(SUPPLY_UNUSABLE_TOKEN, "socket-supply-unusable");
}

/// The idle document the shell reads before anything went wrong: not unusable,
/// nothing to refuse, no invented generation.
#[test]
fn the_idle_path_state_document_is_the_shells_not_unusable_answer() {
    let supply = SocketSupply::new();
    let state = supply.path_state();
    assert!(!state.unusable);
    assert!(!state.refuse_dns);
    assert_eq!(state.reason, "");
    assert_eq!(state.generation, 0);
    assert_eq!(
        supply.path_state_json(),
        "{\"unusable\":false,\"refuse_dns\":false,\"reason\":\"\",\"generation\":0,\"age_ms\":0}"
    );
}

/// R2 (`design.md` §4.2/§5.2 #5): ONE first-class state record per chain — what
/// `retry-skipped` becomes — carrying the attempt count and the request-id range
/// of the refused sockets, with `result=` still last (the free-text tail).
#[test]
fn the_eperm_state_record_is_one_line_per_chain_with_the_refused_id_range() {
    let line = eperm_state_line(
        "relay.example",
        "unusable",
        &EpermSummary { attempts: 2, first_id: 41, last_id: 42 },
        7,
        503,
        "dns:send-failed errno=1 (Operation not permitted (os error 1))",
    );
    assert_eq!(
        line,
        "protected-dns: eperm-state host=relay.example state=unusable attempts=2 \
         first_id=41 last_id=42 generation=7 elapsed_ms=503 \
         result=dns:send-failed errno=1 (Operation not permitted (os error 1))"
    );
    assert!(line.starts_with("protected-dns: eperm-state "), "{line}");
    assert!(line.ends_with("(os error 1))"), "{line}");
}

/// The EPERM verdict: latched (the OS stage stays bounded) AND refusing DNS
/// within the probe window (new chains fail fast), with the reason token and
/// the core generation the capture needs. Marking is orthogonal to the
/// generation (`design.md` §4.2: "与 generation 正交，不推进代际").
#[test]
fn an_eperm_latches_the_path_unusable_without_advancing_the_generation() {
    let supply = SocketSupply::new();
    let generation = supply.generation();
    assert_eq!(supply.mark_unusable(PROTECTED_PATH_UNUSABLE), 0, "nothing was in flight");
    let state = supply.path_state();
    assert!(state.unusable, "the latch is what bounds the OS stage");
    assert!(state.refuse_dns, "a fresh EPERM refuses new DNS requests");
    assert_eq!(state.reason, PROTECTED_PATH_UNUSABLE);
    assert_eq!(state.generation, generation, "marking must not advance the epoch");
    assert_eq!(supply.generation(), generation);
    assert!(state.age_ms < 1000, "age is measured from the refusal: {}", state.age_ms);
    let json = supply.path_state_json();
    assert!(json.contains("\"unusable\":true"), "{json}");
    assert!(json.contains("\"refuse_dns\":true"), "{json}");
    assert!(json.contains("\"reason\":\"protected-path-unusable\""), "{json}");
}

/// The EXISTING recovery channel is what clears the state: a reported physical
/// network change already cancels in-flight requests and advances the
/// generation, so R2 adds no second recovery path.
#[test]
fn a_reported_network_change_is_the_recovery_that_clears_the_latch() {
    let supply = SocketSupply::new();
    supply.mark_unusable(PROTECTED_PATH_UNUSABLE);
    assert!(supply.protected_path_unusable());
    supply.cancel_pending_for_network_change();
    assert_eq!(supply.generation(), 1, "the recovery path advances the epoch");
    assert!(!supply.protected_path_unusable(), "network recovery clears the R2 latch");
    assert!(!supply.path_state().unusable);
    assert!(!supply.path_state().refuse_dns);
}

/// The document the installed supply answers for the new NAPI export. Exactly
/// one test touches the process-global slot (the same rule the 006 probe
/// tests follow).
#[test]
fn the_installed_supplys_document_is_what_the_shell_reads() {
    shutdown();
    assert_eq!(
        path_state_json(),
        "{\"unusable\":false,\"refuse_dns\":false,\"reason\":\"\",\"generation\":0,\"age_ms\":0}",
        "no supply installed is a known not-unusable answer, never an error"
    );
    let supply = Arc::new(SocketSupply::new());
    install(supply.clone());
    assert!(path_state_json().contains("\"unusable\":false"));
    supply.mark_unusable(PROTECTED_PATH_UNUSABLE);
    let json = path_state_json();
    assert!(json.contains("\"unusable\":true"), "{json}");
    assert!(json.contains("\"refuse_dns\":true"), "{json}");
    shutdown();
}

async fn queued(supply: &SocketSupply) -> (u64, String) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let json = supply.poll();
            if json != "{\"id\":\"\"}" {
                let id = json.split('"').nth(3).unwrap().parse().unwrap();
                return (id, json);
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("a queued supply request")
}

/// R2 scope, conservative by design (`design.md` §4.2): while the path is
/// unusable a DNS request is refused INSTANTLY (no 8 s wait, nothing queued),
/// and management/signal/relay keep being served.
#[tokio::test]
async fn a_dns_request_is_refused_instantly_and_management_is_still_served() {
    let supply = Arc::new(SocketSupply::new());
    supply.mark_unusable(PROTECTED_PATH_UNUSABLE);

    let started = Instant::now();
    let refused = supply.request_with_id(Purpose::Dns).await.expect_err("the DNS request must be refused");
    assert_eq!(refused.token(), SUPPLY_UNUSABLE_TOKEN);
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "a refused DNS request must not wait: {:?}",
        started.elapsed()
    );
    assert!(supply.poll_request().is_none(), "a refused request is never enqueued");

    let dialed = tokio::spawn({
        let supply = supply.clone();
        async move { supply.request_with_id(Purpose::Management).await }
    });
    let (id, json) = queued(&supply).await;
    assert!(json.contains("\"purpose\":\"management\""), "{json}");
    let raw = netbird_core::host_sockets::open_tcp_prebound().expect("a pre-bound TCP socket");
    let original = unsafe { OwnedFd::from_raw_fd(raw) };
    assert_eq!(supply.reply(id, raw, "protected"), "{\"ok\":true}");
    let delivered = dialed.await.expect("dial task").expect("management is still supplied");
    drop(delivered);
    drop(original);
}

/// The in-flight half of the same verdict: marking the path unusable cancels
/// DNS requests that are already waiting for their protect reply (they must
/// not sit out the 8 s bound), while a concurrent management request keeps
/// waiting for its own answer.
#[tokio::test]
async fn marking_the_path_unusable_cancels_in_flight_dns_requests_only() {
    let supply = Arc::new(SocketSupply::new());
    let dns = tokio::spawn({
        let supply = supply.clone();
        async move { supply.request_with_id(Purpose::Dns).await }
    });
    let management = tokio::spawn({
        let supply = supply.clone();
        async move { supply.request_with_id(Purpose::Management).await }
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while supply.counts_try() != Some((2, 2)) {
        assert!(Instant::now() < deadline, "both requests must be pending");
        tokio::task::yield_now().await;
    }
    supply.mark_unusable(PROTECTED_PATH_UNUSABLE);
    let cancelled = tokio::time::timeout(Duration::from_millis(300), dns)
        .await
        .expect("the in-flight DNS request is cancelled at once")
        .expect("dns task");
    assert_eq!(cancelled.expect_err("the DNS request is cancelled").token(), SUPPLY_UNUSABLE_TOKEN);

    let (id, json) = queued(&supply).await;
    assert!(json.contains("\"purpose\":\"management\""), "{json}");
    let raw = netbird_core::host_sockets::open_tcp_prebound().expect("a pre-bound TCP socket");
    let original = unsafe { OwnedFd::from_raw_fd(raw) };
    assert_eq!(supply.reply(id, raw, "protected"), "{\"ok\":true}");
    let delivered = management.await.expect("management task").expect("management is still supplied");
    drop(delivered);
    drop(original);
}
