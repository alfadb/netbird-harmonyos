// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N2 v4.1 P7 — the host-side outer-UDP source receipt, END TO END.
//!
//! **INTEGRATION-PROCESS, its own binary on purpose.** The connector slot is
//! process-global and `tests/host_interop_n9.rs` contains an fd-NUMBER
//! assertion (`host_socket_creators_follow_their_contracts` checks that the
//! creators' fd numbers are closed) that any concurrently running test opening
//! sockets can steal — so this real-socket test lives in its own process
//! instead of widening that race (the library seams it drives are the same).
//!
//! What is proven: a REAL UDP datagram sent to the fed nbinterop WG listening
//! port is (a) consumed and counted by the WG device while the receipt switch
//! is OFF, with **zero** receipt output, and (b) reported as a `first`
//! `n2h-outer-receipt` line for that very source once `--outer-receipt`'s
//! arming call (`n2_outer_receipt::arm(true)`) has run. Auxiliary,
//! application-layer evidence only — never a network-side oracle.

mod host_link_stubs {
    use core::ffi::c_void;

    #[no_mangle]
    pub extern "C" fn OH_LOG_Print(
        _t: i32,
        _l: i32,
        _d: u32,
        _tag: *const u8,
        _fmt: *const u8,
        _arg: *const c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn OH_LOG_IsLoggable(_d: u32, _tag: *const u8, _l: i32) -> bool {
        false
    }

    #[no_mangle]
    pub extern "C" fn napi_module_register(_m: *mut c_void) {}

    #[no_mangle]
    pub extern "C" fn napi_create_function(
        _e: *mut c_void,
        _n: *const u8,
        _l: usize,
        _cb: *mut c_void,
        _d: *mut c_void,
        r: *mut *mut c_void,
    ) -> i32 {
        unsafe { *r = 0x10 as *mut c_void };
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_set_named_property(
        _e: *mut c_void,
        _o: *mut c_void,
        _n: *const u8,
        _v: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_cb_info(
        _e: *mut c_void,
        _i: *mut c_void,
        _argc: *mut usize,
        _argv: *mut *mut c_void,
        _this: *mut c_void,
        _data: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _e: *mut c_void,
        _s: *const u8,
        _l: usize,
        _r: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_string_utf8(
        _e: *mut c_void,
        _v: *mut c_void,
        _b: *mut u8,
        _bs: usize,
        _r: *mut usize,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_int32(
        _e: *mut c_void,
        _v: *mut c_void,
        _r: *mut i32,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(
        _e: *mut c_void,
        _v: *mut c_void,
        _r: *mut bool,
    ) -> i32 {
        0
    }
}

use std::sync::{Mutex, MutexGuard, OnceLock};

use base64::Engine as _;

use netbird_core::host_sockets as hs;
use netbird_core::sys;

/// Synthetic key material — 32 bytes of 0x05, base64 (never a deployment
/// secret; any 32 bytes parse, X25519 clamps at use time).
fn test_key_b64() -> String {
    base64::engine::general_purpose::STANDARD.encode([0x05u8; 32])
}

/// The connector slot is process-global: serialize the connector-touching
/// tests of THIS binary.
fn slot_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = LOCK.get_or_init(|| Mutex::new(()));
    match lock.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn secrets_json() -> String {
    "{\"setup_key\":\"interop-test-key-not-real\"}".to_string()
}

fn config_json() -> String {
    format!(
        "{{\"management_url\":\"http://127.0.0.1:1\",\"private_key\":\"{}\"}}",
        test_key_b64()
    )
}

/// The bound port of a raw UDP fd (the "nbinterop listening port").
fn local_port(fd: i32) -> u16 {
    let mut name = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut len = core::mem::size_of::<sys::sockaddr_in>() as u32;
    assert_eq!(unsafe { sys::getsockname(fd, &mut name, &mut len) }, 0);
    u16::from_be(name.sin_port)
}

/// One REAL loopback datagram — the device-behind-NAT stand-in.
fn send_loopback_udp(fd: i32, port: u16, payload: &[u8]) {
    let dst = sys::sockaddr_in::new([127, 0, 0, 1], port);
    let n = unsafe {
        sys::sendto(
            fd,
            payload.as_ptr() as *const core::ffi::c_void,
            payload.len(),
            0,
            &dst,
            core::mem::size_of::<sys::sockaddr_in>() as u32,
        )
    };
    assert_eq!(n, payload.len() as isize);
}

/// This host's own live WG `unknown_peer_drops` (the I6 seam read).
fn wg_unknown_drops() -> Option<u64> {
    netbird_core::n2h_collector::installed_host()
        .and_then(|h| h.wg_stats())
        .map(|s| s.unknown_peer_drops)
}

/// Wait (bounded) until the device's pump consumed the probe datagram(s), so
/// the assertions never race the 100 ms pump tick. Returns the counter read.
fn wait_for_unknown_drops(at_least: u64) -> u64 {
    for _ in 0..200 {
        if let Some(v) = wg_unknown_drops() {
            if v >= at_least {
                return v;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the fed WG device never consumed the probe datagram(s)");
}

/// P7 acceptance: a REAL UDP datagram sent to the nbinterop WG listening port
/// produces the outer-source receipt — and, with the switch OFF, the very same
/// datagram produces NO line while the device still records the drop (the
/// zero-output negative control).
#[test]
fn outer_receipt_reports_a_real_outer_udp_source_end_to_end() {
    let _g = slot_lock();
    netbird_core::connector::connector_stop_json();
    let mut bag = hs::FdBag::new();

    let mgmt = hs::open_tcp_prebound().expect("mgmt tcp socket");
    bag.keep(mgmt);
    hs::start_connector_over_host_socket(
        mgmt,
        &config_json(),
        &secrets_json(),
        "127.0.0.1:1".parse().unwrap(),
    )
    .expect("start over host socket");

    let wg = hs::open_udp_ephemeral().expect("wg outer socket");
    bag.keep(wg);
    hs::feed_wg_socket(wg).expect("wg feed");
    let (tun_fed, _tun_hand) = hs::open_tun_standby_pair().expect("tun stand-in");
    bag.keep(tun_fed);
    hs::feed_tun(tun_fed).expect("tun feed");

    let wg_port = local_port(wg);
    let sender = hs::open_udp_ephemeral().expect("sender");
    bag.keep(sender);
    let sender_port = local_port(sender);

    // --- switch OFF: consumed + counted by the device, and NOT reported ----
    netbird_core::n2_outer_receipt::arm(false);
    let base = wait_for_unknown_drops(wg_unknown_drops().unwrap_or(0));
    send_loopback_udp(sender, wg_port, &[1u8; 32]);
    let after_off = wait_for_unknown_drops(base + 1);
    assert_eq!(after_off, base + 1, "the datagram WAS read by the device");
    assert!(
        netbird_core::n2_outer_receipt::emitted_lines().is_empty(),
        "switch off ⇒ zero receipt output"
    );

    // --- switch ON: the same real source is reported ----------------------
    netbird_core::n2_outer_receipt::arm(true);
    send_loopback_udp(sender, wg_port, &[4u8; 32]);
    let mut mine: Vec<String> = Vec::new();
    for _ in 0..200 {
        mine = netbird_core::n2_outer_receipt::emitted_lines()
            .into_iter()
            .filter(|l| l.contains(&format!("|src=127.0.0.1:{sender_port}|")))
            .collect();
        if !mine.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert_eq!(mine.len(), 1, "one first line per new source: {mine:?}");
    let line = &mine[0];
    assert!(line.starts_with("n2h-outer-receipt|first|"), "{line}");
    assert!(line.contains(&format!("|local=0.0.0.0:{wg_port}|")), "{line}");
    assert!(line.contains("|type=wg-data|"), "{line}");
    assert!(line.contains("|peer=unknown|"), "{line}");
    assert!(line.contains("|mono_ms="), "{line}");
    assert!(line.contains("|unix_ms="), "{line}");
    wait_for_unknown_drops(after_off + 1);

    netbird_core::n2_outer_receipt::arm(false);
    hs::stop_connector();
}
