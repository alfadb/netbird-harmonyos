// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! 3-D / R1 red-first observation tests (phase 1: existing APIs only).
//!
//! R1 (`artifacts/analysis-20261025-upstream-compare/report.md` §4.4): the
//! `direct->carrier` edge must distinguish the STARTUP transient (a peer that
//! has NEVER had a direct endpoint, i.e. its first attach already rides the
//! relay lane) from a REAL fallback (a direct path was landed and then lost).
//!
//! This test drives the REAL production seam — `WgDevice` adopt / set_peers /
//! set_carrier / set_endpoint / recycle_endpoint / send_from_tun, i.e. the one
//! egress decision point `dispatch` — and reads the phone log the markers are
//! teed into. It asserts the new stable token, so it FAILS before the
//! observation exists and passes after (zero forwarding behavior asserted here
//! beyond "the frame still leaves by the selected bearer").
//!
//! The scenario: carrier attached with no endpoint (startup) → a real endpoint
//! lands → the endpoint is recycled (real fallback). The first edge must carry
//! `initial=1`; the second must stay the plain edge.

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

use base64::Engine as _;

use netbird_core::phone_log;
use netbird_core::sys;
use netbird_core::tun::TunFd;
use netbird_core::wg_device::{WgDevice, WgDeviceConfig, WgEgressCarrier, WgPeerSpec};

/// Synthetic keys — never real deployment material.
const LOCAL_SECRET: [u8; 32] = [0x11; 32];
const PEER_SECRET: [u8; 32] = [0x22; 32];

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// A no-op relay bearer: the observation is about which EDGE is classified,
/// not about delivery.
struct NullCarrier;

impl WgEgressCarrier for NullCarrier {
    fn send_datagram(&self, _datagram: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn kind(&self) -> &'static str {
        "relay"
    }
}

/// Bound loopback UDP socket (the "protected" WG outer socket stand-in).
fn udp_socket_lo() -> (i32, u16) {
    let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
    assert!(fd >= 0, "socket() failed errno={}", sys::errno());
    let sa = sys::sockaddr_in::new([127, 0, 0, 1], 0);
    let rc = unsafe { sys::bind(fd, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) };
    assert_eq!(rc, 0, "bind() failed errno={}", sys::errno());
    let mut name = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut len = core::mem::size_of::<sys::sockaddr_in>() as u32;
    assert_eq!(unsafe { sys::getsockname(fd, &mut name, &mut len) }, 0);
    (fd, u16::from_be(name.sin_port))
}

/// Minimal IPv4/UDP frame; `send_from_tun` only reads the destination.
fn ipv4_frame(dst: [u8; 4]) -> Vec<u8> {
    let mut frame = vec![0u8; 28];
    frame[0] = 0x45;
    frame[2..4].copy_from_slice(&28u16.to_be_bytes());
    frame[8] = 64;
    frame[9] = 17;
    frame[12..16].copy_from_slice(&[10, 77, 0, 1]);
    frame[16..20].copy_from_slice(&dst);
    frame[20..22].copy_from_slice(&40000u16.to_be_bytes());
    frame[22..24].copy_from_slice(&53u16.to_be_bytes());
    frame[24..26].copy_from_slice(&8u16.to_be_bytes());
    frame
}

/// boringtun's ffi installs a process-global panic hook; re-install a printing
/// hook so a failed observation assertion stays diagnosable.
fn show_panics() {
    use std::sync::OnceLock;
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        std::panic::set_hook(Box::new(|info| {
            eprintln!("\nTEST PANIC: {info}");
        }));
    });
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
fn direct_to_carrier_marks_the_never_direct_startup_edge_as_initial() {
    let path = log_path("carrier-initial");
    configure_phone_log(&path);

    let (wg_raw, _port) = udp_socket_lo();
    let (tun_raw, _tun_port) = udp_socket_lo();
    let tun = TunFd::dup_from_raw(tun_raw).expect("tun dup");
    let mut cfg = WgDeviceConfig::new(b64(&LOCAL_SECRET));
    cfg.hs_retry_ms = 100;
    cfg.hs_deadline_ms = 10_000;
    let mut dev = WgDevice::adopt(cfg, wg_raw, tun).expect("device adopt");
    let peer_key =
        netbird_core::wg_device::x25519_public_b64(&b64(&PEER_SECRET)).expect("peer public key");
    dev.set_peers(&[WgPeerSpec {
        pub_key_b64: peer_key.clone(),
        allowed_ips: vec![([10, 77, 0, 2], 32)],
        preshared_key_b64: None,
    }])
    .expect("register the peer");
    // boringtun's ffi owns the panic hook from the first tunnel on.
    show_panics();
    dev.set_carrier(&peer_key, Arc::new(NullCarrier))
        .expect("attach the relay carrier");
    assert!(
        !dev.peers().iter().any(|p| p.endpoint.is_some()),
        "fixture: no direct endpoint has ever landed"
    );

    let frame = ipv4_frame([10, 77, 0, 2]);
    // Startup: no endpoint has ever existed, so this edge is the warm-up edge.
    assert!(dev.send_from_tun(&frame, 1_000), "the first frame leaves via the carrier");

    // A real direct path lands (carrier->direct), then disappears again. The
    // next handshake datagram (device clock) is the real fallback edge.
    dev.set_endpoint(&peer_key, [127, 0, 0, 1], 51820, 1_100)
        .expect("land a direct endpoint");
    let _ = dev.send_from_tun(&frame, 1_100);
    dev.recycle_endpoint(&peer_key);
    let _ = dev.tick(1_300);

    let log = read_phone_log(&path);
    let initial = "N13_WG|direct->carrier|no-ice-endpoint|initial=1|peer=";
    let plain = "N13_WG|direct->carrier|no-ice-endpoint|peer=";
    assert!(
        log.contains(initial),
        "the never-direct startup edge must carry initial=1; log={log}"
    );
    assert_eq!(
        log.matches(initial).count(),
        1,
        "exactly one initial edge (the startup attach); log={log}"
    );
    assert_eq!(
        log.matches(plain).count(),
        1,
        "the post-direct loss stays the plain fallback edge; log={log}"
    );

    unsafe { sys::close(wg_raw) };
    unsafe { sys::close(tun_raw) };
}
