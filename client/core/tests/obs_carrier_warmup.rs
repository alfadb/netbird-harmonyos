// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! 2-B red-first observation test (phase 1: existing APIs only).
//!
//! `artifacts/analysis-20261025-codeplan/plan.md` §2.3 option B: classify the
//! STARTUP window in which the relay carrier is attached while ICE has not yet
//! selected a pair (`carrier-warmup`), so the logs can separate "the startup
//! transient rides relay because relay is the only path" from "a real direct
//! path was lost and the peer fell back". ZERO behavior change: the marker is
//! a classification only — attach timing, the ICE pump and the guard budget
//! are untouched.
//!
//! The test drives the REAL `PeerIceOrchestrator`/`guard_tick` path with the
//! injected WG seam (`WgPeerApplier::carrier_attached`) and reads the phone log.
//! It FAILS before the marker exists. The negative peer shows the marker is
//! not emitted when no carrier is attached. (The "had a direct path before"
//! branch is pinned by the pure classifier in `obs_contracts.rs`, because
//! landing a selected pair needs a live session harness.)

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

use std::sync::Arc;

use netbird_core::connector::{WgPeerApplier, WgPeerEntry};
use netbird_core::ice::{InterfaceAddr, StaticInterfaces, UdpSocketSource};
use netbird_core::mgmtsock::SocketSeamError;
use netbird_core::peer_conn::{LoggingSignalExchange, PeerIceDeps, PeerIceOrchestrator};
use netbird_core::phone_log;

/// ICE must never need a socket in this test: the pump is stopped at the
/// guard classification (signal never ready), which is BEFORE the first offer.
struct NoSockets;

impl UdpSocketSource for NoSockets {
    fn take_fd(&self) -> Result<i32, SocketSeamError> {
        Err(SocketSeamError::NoSocket)
    }
}

/// The ONE WG seam fact `guard_tick` classifies a peer on.
struct CarrierSeam {
    attached: bool,
}

impl WgPeerApplier for CarrierSeam {
    fn apply_peers(&self, _peers: &[WgPeerEntry]) -> Result<(), String> {
        Ok(())
    }
    fn clear(&self) {}
    fn carrier_attached(&self, _pub_key_b64: &str) -> bool {
        self.attached
    }
}

fn orchestrator(attached: bool, key: &str) -> PeerIceOrchestrator {
    let mut orch = PeerIceOrchestrator::new(PeerIceDeps {
        ifaces: Arc::new(StaticInterfaces(vec![InterfaceAddr {
            name: "eth0".into(),
            addr: [127, 0, 0, 1],
        }])),
        socks: Arc::new(NoSockets),
        signal: Arc::new(LoggingSignalExchange::default()),
        wg: Arc::new(CarrierSeam { attached }),
        tie_breaker: Some(0x2b),
        fixed_local_port: None,
        advertised_candidates: Vec::new(),
    });
    orch.set_peers(&[key.to_string()]);
    orch
}

fn log_path(tag: &str) -> String {
    std::env::temp_dir()
        .join(format!("n2h-obs-{tag}-{}.log", std::process::id()))
        .to_string_lossy()
        .into_owned()
}

fn configure_phone_log(path: &str) {
    let _ = std::fs::remove_file(path);
    let mut opts = phone_log::Options::default();
    opts.max_bytes = 1 << 20;
    opts.segments = 2;
    opts.queue = 4096;
    opts.rate_bytes_per_sec = 1_000_000;
    opts.rate_records_per_sec = 10_000;
    phone_log::configure_with(path, &opts).expect("install the phone log");
}

fn read_phone_log(path: &str) -> String {
    assert!(phone_log::flush(std::time::Duration::from_secs(3)), "phone log flush");
    std::fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn startup_window_with_a_carrier_attached_is_marked_carrier_warmup() {
    let path = log_path("carrier-warmup");
    configure_phone_log(&path);

    // Startup window: carrier attached, ICE has never selected/landed a pair.
    let mut warm = orchestrator(true, "WARMUP01");
    warm.run_once(0).expect("warm-up pump pass");

    // Control: the same state without a carrier attached is not warm-up.
    let mut cold = orchestrator(false, "COLD0001");
    cold.run_once(0).expect("control pump pass");

    let log = read_phone_log(&path);
    assert!(
        log.contains("N5c_ICE|carrier-warmup|peer=WARMUP01|state=idle|never-direct=1"),
        "a never-direct peer with the carrier attached is the warm-up window; log={log}"
    );
    assert!(
        !log.contains("carrier-warmup|peer=COLD0001"),
        "no carrier attached means no warm-up classification; log={log}"
    );
}
