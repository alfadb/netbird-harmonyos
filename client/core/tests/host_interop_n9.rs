// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N9 host-interop harness tests (INTEGRATION-PROCESS; separate binary keeps
//! the connector's process-global slot away from the other suites).
//!
//! The load-bearing assertion for this increment: **the host-only socket
//! provider does NOT relax the library's default fail-closed posture** —
//! with `host_sockets` linked in, `connector_start` (the unprotected entry)
//! STILL refuses (`management-socket-required`), the fd feed seams STILL
//! refuse dead/missing fds, and the host path only works by feeding REAL
//! fds through the UNCHANGED production start/feed seams.

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

/// The connector slot is process-global: every test that touches it
/// serializes on this lock.
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
    // valid 32-byte synthetic private key (bytes 0x05; not real)
    format!(
        "{{\"management_url\":\"http://127.0.0.1:1\",\"private_key\":\"{}\"}}",
        test_key_b64()
    )
}

// ---------------------------------------------------------------------------
// THE fail-closed assertion (host module linked in, default path unchanged)
// ---------------------------------------------------------------------------

#[test]
fn default_start_still_requires_protected_socket_with_host_module_linked() {
    let _g = slot_lock();
    netbird_core::connector::connector_stop_json(); // clean slate
    // the unprotected entry point refuses exactly as before — linking
    // host_sockets changed nothing about the default posture
    let out = netbird_core::connector::connector_start_json(&config_json(), &secrets_json());
    assert!(out.contains("\"started\":false"), "{out}");
    assert!(
        out.contains("\"error\":\"management-socket-required\""),
        "default start must stay fail-closed: {out}"
    );
    // an explicit false opt-in changes nothing either
    let cfg = config_json().replace(
        "\"private_key\"",
        "\"allow_unprotected_management\":false,\"private_key\"",
    );
    let out2 = netbird_core::connector::connector_start_json(&cfg, &secrets_json());
    assert!(out2.contains("\"error\":\"management-socket-required\""), "{out2}");
    assert!(
        !out2.contains("\"started\":true"),
        "no start may happen without the production socket path: {out2}"
    );
    netbird_core::connector::connector_stop_json();
}

#[test]
fn fd_feed_seams_still_refuse_missing_and_dead_fds() {
    let _g = slot_lock();
    netbird_core::connector::connector_stop_json();
    // missing fd
    let out = netbird_core::connector::connector_ice_socket_feed_json(-1);
    assert!(out.contains("\"error\":\"no-connector\""), "no connector yet: {out}");
    // start nothing: the seams keep their token contract with no connector
    let out = netbird_core::connector::connector_socket_feed_json(-1);
    assert!(out.contains("no-connector"), "{out}");

    // host start, then a dead-fd feed must be refused by the seam's dup
    // probe (fd semantics unchanged — the host provider cannot bypass them)
    let fd = hs::open_tcp_prebound().expect("host tcp socket");
    let state = hs::start_connector_over_host_socket(
        fd,
        &config_json(),
        &secrets_json(),
        "127.0.0.1:1".parse().unwrap(),
    )
    .expect("host-path start must succeed with a REAL fd");
    assert_eq!(state, "connecting");
    let dead = {
        // open + close a socket to obtain an fd number that is now dead
        let d = hs::open_udp_unbound().unwrap();
        unsafe { sys::close(d) };
        d
    };
    let out = netbird_core::connector::connector_ice_socket_feed_json(dead);
    assert!(
        out.contains("\"error\":\"socket-fd-invalid\""),
        "dead fd must be refused: {out}"
    );
    let out = netbird_core::connector::connector_socket_feed_json(dead);
    assert!(out.contains("socket-fd-invalid"), "{out}");
    hs::stop_connector();
}

// ---------------------------------------------------------------------------
// host socket contracts (creation, bound ports, socketpair TUN stand-in)
// ---------------------------------------------------------------------------

#[test]
fn host_socket_creators_follow_their_contracts() {
    let tcp = hs::open_tcp_prebound().expect("tcp prebound");
    assert!(tcp >= 0);
    let udp = hs::open_udp_unbound().expect("udp unbound");
    let wg = hs::open_udp_ephemeral().expect("udp ephemeral");
    let port = hs::udp_bound_port(wg).expect("bound port");
    assert!(port > 0, "ephemeral WG socket must be bound (got {port})");

    // TUN stand-in: end 0 plays the fed platform fd, end 1 is the hand —
    // one write == one frame in either direction
    let (fed, hand) = hs::open_tun_standby_pair().expect("socketpair");
    let (n, _) = sys::write_fd(hand, b"frame-1");
    assert_eq!(n, 7);
    let mut buf = [0u8; 16];
    let (rn, _) = sys::read_fd(fed, &mut buf);
    assert_eq!(rn, 7);
    assert_eq!(&buf[..7], b"frame-1");

    // provider ownership: FdBag closes every kept fd exactly once
    {
        let mut bag = hs::FdBag::new();
        bag.keep(tcp);
        bag.keep(udp);
        bag.keep(wg);
        bag.keep(fed);
        bag.keep(hand);
        assert_eq!(bag.len(), 5);
        assert!(unsafe { sys::fcntl(tcp, sys::F_GETFD) } != -1);
    }
    for fd in [tcp, udp, wg, fed, hand] {
        assert_eq!(unsafe { sys::fcntl(fd, sys::F_GETFD) }, -1, "fd {fd} must be closed");
    }
}

#[test]
fn probe_packets_are_honest_ip_frames() {
    let pkt = hs::build_ipv4_udp_packet([100, 64, 0, 1], [100, 64, 0, 2], 40000, 40001, b"x");
    assert_eq!(pkt.len(), 29);
    assert_eq!(pkt[0] >> 4, 4, "IPv4");
    assert_eq!(&pkt[16..20], &[100, 64, 0, 2], "destination");
    // checksum verifies to zero
    let mut sum: u32 = 0;
    for w in pkt[..20].chunks(2) {
        sum += u16::from_be_bytes([w[0], w[1]]) as u32;
    }
    assert_eq!(!((sum + (sum >> 16)) as u16), 0);
    assert_eq!(hs::parse_ipv4("100.64.0.2"), Some([100, 64, 0, 2]));
    assert_eq!(hs::parse_ipv4("100.64.0"), None);
    assert_eq!(hs::parse_ipv4("100.64.0.256"), None);
    assert_eq!(hs::parse_ipv4("::1"), None);
}

// ---------------------------------------------------------------------------
// host start → feeds → stop (the connect loop's core, compressed)
// ---------------------------------------------------------------------------

#[test]
fn host_start_then_feeds_then_stop_roundtrip() {
    let _g = slot_lock();
    netbird_core::connector::connector_stop_json();
    let mut bag = hs::FdBag::new();

    let fd = hs::open_tcp_prebound().expect("mgmt tcp socket");
    bag.keep(fd);
    let state = hs::start_connector_over_host_socket(
        fd,
        &config_json(),
        &secrets_json(),
        "127.0.0.1:1".parse().unwrap(),
    )
    .expect("start over host socket");
    assert_eq!(state, "connecting");

    // status is live and running
    let status = hs::status_json();
    assert!(status.contains("\"running\":true"), "{status}");

    // resupply queues accept fresh host sockets (the feeder's top-ups)
    let m2 = hs::open_tcp_prebound().expect("mgmt socket #2");
    let queued = hs::feed_management(m2).expect("mgmt feed");
    bag.keep(m2);
    assert!(queued >= 1, "queued={queued}");

    let u1 = hs::open_udp_unbound().expect("ice socket");
    let queued = hs::feed_ice(u1).expect("ice feed");
    bag.keep(u1);
    assert!(queued >= 1, "queued={queued}");

    // wg socket + tun stand-in feeds bring the device seam up
    let wg = hs::open_udp_ephemeral().expect("wg socket");
    bag.keep(wg);
    hs::feed_wg_socket(wg).expect("wg feed");
    let (tun_fed, _tun_hand) = hs::open_tun_standby_pair().expect("tun stand-in");
    bag.keep(tun_fed);
    hs::feed_tun(tun_fed).expect("tun feed");
    // the pump drives the device; give it a beat, then the status must show
    // a real (loopback-dead-endpoint) device that is UP but NOT ready —
    // fail-closed readiness still requires an actual WG session
    std::thread::sleep(std::time::Duration::from_millis(400));
    let status = hs::status_json();
    assert!(status.contains("\"fed_socket\":true"), "{status}");
    assert!(status.contains("\"fed_tun\":true"), "{status}");
    assert!(status.contains("\"device_up\":true"), "{status}");
    assert!(status.contains("\"ready\":false"), "no session without a peer: {status}");

    // idempotent same-number wg feed is a no-op; stop tears everything down
    hs::feed_wg_socket(wg).expect("same-number wg feed stays idempotent");
    hs::stop_connector();
    let status = hs::status_json();
    assert!(status.contains("\"running\":false"), "{status}");
    // seam state reset: the same fd number feeds are refused post-stop
    let out = netbird_core::connector::connector_socket_feed_json(m2);
    assert!(out.contains("no-connector"), "{out}");
}

// ---------------------------------------------------------------------------
// config layer via files (credential discipline, dry-run plan inputs)
// ---------------------------------------------------------------------------

#[test]
fn cli_config_load_rejects_bad_files_with_clear_exit_codes() {
    let dir = std::env::temp_dir();
    let missing = dir.join("nbinterop-n9-does-not-exist.json");
    let err = hs::load_cli_config(missing.to_str().unwrap()).unwrap_err();
    assert_eq!(err.exit_code, hs::EXIT_CONFIG);
    assert!(err.message.contains("not found"), "{}", err.message);

    let bad = dir.join("nbinterop-n9-bad.json");
    std::fs::write(&bad, "{\"management_url\": 42}").unwrap();
    let err = hs::load_cli_config(bad.to_str().unwrap()).unwrap_err();
    assert_eq!(err.exit_code, hs::EXIT_CONFIG);
    let _ = std::fs::remove_file(&bad);

    let norealm = dir.join("nbinterop-n9-nourl.json");
    std::fs::write(
        &norealm,
        format!("{{\"private_key\":\"{}\"}}", test_key_b64()),
    )
    .unwrap();
    let err = hs::load_cli_config(norealm.to_str().unwrap()).unwrap_err();
    assert_eq!(err.exit_code, hs::EXIT_CONFIG);
    assert!(err.message.contains("management_url"), "{}", err.message);
    let _ = std::fs::remove_file(&norealm);
}

#[test]
fn cli_config_load_maps_credentials_and_plan_outputs() {
    let dir = std::env::temp_dir();
    let good = dir.join("nbinterop-n9-good.json");
    std::fs::write(
        &good,
        format!(
            "{{\"management_url\":\"https://mgmt.example:33073\",\"ca_pem\":\"PEM\",\
             \"private_key\":\"{}\",\"hostname\":\"interop-t\",\"setup_key\":\"KEY-123\"}}",
            test_key_b64()
        ),
    )
    .unwrap();
    let loaded = hs::load_cli_config(good.to_str().unwrap()).expect("load");
    // pass-through document: connector fields kept, secrets stripped
    assert!(loaded.config_json.contains("\"management_url\":\"https://mgmt.example:33073\""));
    assert!(loaded.config_json.contains("\"ca_pem\":\"PEM\""));
    assert!(!loaded.config_json.contains("KEY-123"), "{}", loaded.config_json);
    // secrets go to their own never-printed document
    assert!(loaded.secrets_json.contains("KEY-123"));
    // the summary is secret-free and plan-renderable
    let s = &loaded.summary;
    assert_eq!(s.management_url, "https://mgmt.example:33073");
    assert!(s.tls);
    assert_eq!(s.hostname, "interop-t");
    assert_eq!(s.setup_key_source, "file");
    assert_eq!(s.setup_key_len, 7);
    let plan = hs::plan_json("dry-run", s);
    assert!(!plan.contains("KEY-123"), "plan must be secret-free: {plan}");
    assert!(plan.contains("connector-start-with-socket"));
    assert!(plan.contains("\"dry_run\":true"));
    let _ = std::fs::remove_file(&good);
}
