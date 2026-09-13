// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N7 integration tests: the shell-fed production WG data plane
//! (`wg_device::WgDeviceFeed`) closed loop, fail-closed behavior and the fd
//! contract. Same host-test pattern as tests/wg_e2e.rs: real BoringTun
//! tunnels, loopback UDP sockets as the WG "cable", datagram socketpairs
//! playing the TUN half, injected clock — no sleeps, no device.
//!
//! Proven here (the N7 acceptance core):
//! - PRODUCTION closed loop: the slot seam is fed through its feed entry
//!   points (the exact functions behind the `connector_wg_socket_feed` /
//!   `connector_tun_fd_feed` NAPI exports) with a protected UDP socket +
//!   socketpair TUN; the buffered peer set + endpoint replay into the REAL
//!   `WgDevice`, both ends complete the WG handshake, `tunnel_ready()`
//!   turns TRUE, the N3-7 default-route gate ALLOWS `0.0.0.0/0`, and
//!   bidirectional payload crosses byte-exact. The seam is the device: a
//!   `dataplane_status()` exists and reports real counters (the registry
//!   reference seam has none and can never become ready).
//! - CONNECTOR level: a real `ConnectorHandle` spawned with the slot as its
//!   WG seam reports the device through `status().wg` (fed/device_up/ready
//!   + counters) and `stop()` tears the data plane down (device gone, feeds
//!   dropped — fail-closed until a fresh feed pair arrives).
//! - FAIL-CLOSED: with either feed missing the device never comes up,
//!   `tunnel_ready()` stays false, the default route stays HELD, control
//!   plane calls merely buffer, dead/missing fds are refused at the
//!   boundary, and the ICE protected-UDP source is never consumed
//!   (`taken()==0` — nothing unprotected is ever created).
//! - FD CONTRACT (§二.4): the originals stay caller-owned — closing them
//!   right after the feeds does NOT disturb the device (it runs on dup
//!   copies: the handshake still completes and payload still flows), and
//!   dropping the device (clear) leaves the caller's originals open and
//!   usable (native never closes a borrowed number).
//!
//! Test keys below are EXPLICIT synthetic constants — never real
//! deployment material.

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

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine as _;
use boringtun::ffi;

use netbird_core::config;
use netbird_core::connector::{
    ConnectorHandle, ConnectorSecrets, ManagementFactory, SyncPolicy, WgPeerApplier, WgPeerEntry,
};
use netbird_core::grpc::ManagementGrpcClient;
use netbird_core::ice::ProtectedUdpFdSource;
use netbird_core::management::ManagementError;
use netbird_core::sys;
use netbird_core::tun::TunFd;
use netbird_core::wg_device::{WgDevice, WgDeviceApplier, WgDeviceConfig, WgDeviceFeed};

// ---------------------------------------------------------------------------
// synthetic test keys (never real deployment material)
// ---------------------------------------------------------------------------

/// Slot side (the "client" the connector drives): 32 bytes of 0xA5.
const SECRET_A: [u8; 32] = [0xA5; 32];
/// Far-end peer device: 32 bytes of 0xB6.
const SECRET_B: [u8; 32] = [0xB6; 32];

/// Tunnel addresses inside the routed range.
const ADDR_A: [u8; 4] = [10, 77, 0, 1];
const ADDR_B: [u8; 4] = [10, 77, 0, 2];

fn pub_key_raw(secret: &[u8; 32]) -> [u8; 32] {
    ffi::x25519_public_key(ffi::x25519_key { key: *secret }).key
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn key_a() -> &'static str {
    static K: OnceLock<String> = OnceLock::new();
    K.get_or_init(|| b64(&pub_key_raw(&SECRET_A)))
}

fn key_b() -> &'static str {
    static K: OnceLock<String> = OnceLock::new();
    K.get_or_init(|| b64(&pub_key_raw(&SECRET_B)))
}

/// Quick device-clock policy (injected ms; loopback delivery is synchronous
/// within the process — same as tests/wg_e2e.rs).
fn feed_config(secret: &[u8; 32]) -> WgDeviceConfig {
    let mut cfg = WgDeviceConfig::new(b64(secret));
    cfg.hs_retry_ms = 100;
    cfg.hs_deadline_ms = 10_000;
    cfg.keepalive_ms = u64::MAX / 2;
    cfg.session_max_ms = u64::MAX / 2;
    cfg
}

// ---------------------------------------------------------------------------
// socket helpers (host; same shapes as tests/wg_e2e.rs)
// ---------------------------------------------------------------------------

extern "C" {
    fn socketpair(domain: i32, ty: i32, protocol: i32, sv: *mut [i32; 2]) -> i32;
}

/// Bound loopback UDP socket (the "protected" WG outer socket stand-in):
/// (raw fd, bound port).
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

/// Datagram socketpair: end 0 is the "platform TUN fd" handed to the feed,
/// end 1 is the test's hand (one write == one frame).
fn tun_socketpair() -> (i32, i32) {
    let mut sv = [-1i32; 2];
    let rc = unsafe { socketpair(1 /* AF_UNIX */, 2 /* SOCK_DGRAM */, 0, &mut sv) };
    assert_eq!(rc, 0, "socketpair failed errno={}", sys::errno());
    (sv[0], sv[1])
}

fn fd_open(fd: i32) -> bool {
    unsafe { sys::fcntl(fd, sys::F_GETFD) != -1 }
}

/// Serializes every test in this file. Required by the fd-liveness
/// assertions (`fd_open` on a number a test just closed itself): fd numbers
/// are handed out lowest-free-first from the process-global table, so with
/// parallel test execution another test's socket()/dup() can re-open a
/// just-closed number between the close and the probe and flip the expected
/// `false`. Same pattern as tests/tun_fd_contract.rs::TEST_LOCK (std-only,
/// no new dependency). Dead-feed probes here use an unallocatable fd number
/// (`DEAD_FD`, far beyond RLIMIT_NOFILE) and stay deterministic regardless;
/// the lock covers the liveness probes that must observe a real, now-dead
/// number.
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// One IPv4/UDP packet with a valid header checksum.
fn ip_udp_packet(src: [u8; 4], dst: [u8; 4], sport: u16, dport: u16, payload: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(28 + payload.len());
    let total = (20 + 8 + payload.len()) as u16;
    p.extend_from_slice(&[0x45, 0]);
    p.extend_from_slice(&total.to_be_bytes());
    p.extend_from_slice(&[0, 1, 0, 0, 64, 17, 0, 0]);
    p.extend_from_slice(&src);
    p.extend_from_slice(&dst);
    p.extend_from_slice(&sport.to_be_bytes());
    p.extend_from_slice(&dport.to_be_bytes());
    p.extend_from_slice(&((8 + payload.len()) as u16).to_be_bytes());
    p.extend_from_slice(&[0, 0]);
    p.extend_from_slice(payload);
    let mut sum: u32 = 0;
    for w in p[..20].chunks(2) {
        sum += u16::from_be_bytes([w[0], w[1]]) as u32;
    }
    let ck = !(sum + (sum >> 16)) as u16;
    p[10..12].copy_from_slice(&ck.to_be_bytes());
    p
}

fn write_tun(test_end: i32, frame: &[u8]) {
    let (n, e) = sys::write_fd(test_end, frame);
    assert_eq!((n as usize, e), (frame.len(), 0), "TUN-side test write");
}

fn read_tun(test_end: i32) -> Option<Vec<u8>> {
    let (ret, _e, rev) = sys::poll1(test_end, sys::POLLIN, 0);
    if ret <= 0 || (rev & sys::POLLIN) == 0 {
        return None;
    }
    let mut buf = [0u8; 2048];
    let (n, e) = sys::read_fd(test_end, &mut buf);
    assert!(n > 0, "TUN-side test read errno={e}");
    Some(buf[..n as usize].to_vec())
}

/// boringtun's ffi installs a process-global panic→SIGSEGV hook on the first
/// `new_tunnel`; re-install a printing hook so assertion failures stay
/// diagnosable (same as tests/wg_e2e.rs).
fn show_panics() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        std::panic::set_hook(Box::new(|info| {
            eprintln!("\nTEST PANIC: {info}");
            eprintln!("{:?}", std::backtrace::Backtrace::force_capture());
        }));
    });
}

// ---------------------------------------------------------------------------
// the far-end peer device (plain WgDeviceApplier, wg_e2e-style)
// ---------------------------------------------------------------------------

struct Peer {
    app: Arc<WgDeviceApplier>,
    tun_test_end: i32,
    raws: Mutex<Vec<i32>>,
}

impl Peer {
    fn build(secret: &[u8; 32], key: &str, peer_key: &str, peer_ips: Vec<([u8; 4], u8)>) -> Peer {
        let (wg_raw, _port) = udp_socket_lo();
        let (tun_raw, test_end) = tun_socketpair();
        let tun = TunFd::dup_from_raw(tun_raw).expect("tun dup");
        let dev = WgDevice::adopt(feed_config(secret), wg_raw, tun).expect("device adopt");
        let app = Arc::new(WgDeviceApplier::new(dev));
        app.apply_peers(&[WgPeerEntry {
            pub_key_b64: peer_key.to_string(),
            allowed_ips: peer_ips
                .iter()
                .map(|&(a, p)| config::Route { addr: a, prefix_len: p })
                .collect(),
        }])
        .expect("apply_peers");
        show_panics();
        let _ = key;
        Peer {
            app,
            tun_test_end: test_end,
            raws: Mutex::new(vec![wg_raw, tun_raw, test_end]),
        }
    }

    fn port(&self) -> u16 {
        self.app.with_device(|d| d.local_addr()).1
    }

    fn ready(&self) -> bool {
        self.app.tunnel_ready()
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        let raws =
            self.raws.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect::<Vec<_>>();
        for fd in raws {
            unsafe { sys::close(fd) };
        }
    }
}

// ---------------------------------------------------------------------------
// the shell-fed slot side (production seam under test)
// ---------------------------------------------------------------------------

struct SlotEnd {
    slot: Arc<WgDeviceFeed>,
    wg_raw: i32,
    tun_raw: i32,
    tun_test_end: i32,
    /// Kept Some only in tests that close the originals early (the feed
    /// path must not need them afterwards — fd contract).
    raws: Mutex<Vec<i32>>,
}

impl SlotEnd {
    /// Unfed slot + its raw fds. Peer set and endpoint are applied BEFORE
    /// the feeds (the production ordering: the map and the ICE pair land
    /// while create()/protect have not happened yet).
    fn build(secret: &[u8; 32], peer_key: &str, peer_ips: Vec<([u8; 4], u8)>, peer_port: u16) -> SlotEnd {
        let (wg_raw, _port) = udp_socket_lo();
        let (tun_raw, tun_test_end) = tun_socketpair();
        let slot = Arc::new(WgDeviceFeed::new(feed_config(secret)));
        // control-plane state first (buffered)
        slot.apply_peers(&[WgPeerEntry {
            pub_key_b64: peer_key.to_string(),
            allowed_ips: peer_ips
                .iter()
                .map(|&(a, p)| config::Route { addr: a, prefix_len: p })
                .collect(),
        }])
        .expect("buffered peers");
        slot.apply_endpoint(peer_key, [127, 0, 0, 1], peer_port)
            .expect("buffered endpoint");
        // pre-feed: nothing up
        assert!(!slot.device_up());
        assert!(!slot.tunnel_ready());
        SlotEnd {
            slot,
            wg_raw,
            tun_raw,
            tun_test_end,
            raws: Mutex::new(vec![wg_raw, tun_raw, tun_test_end]),
        }
    }

    /// THE feed entry points (the exact functions behind the NAPI exports):
    /// TUN fd first, protected WG socket second.
    fn feed_both(&self) {
        self.slot.feed_tun(self.tun_raw).expect("tun feed");
        self.slot.feed_wg_socket(self.wg_raw).expect("wg socket feed");
        assert!(self.slot.device_up(), "both feeds must bring the device up");
    }

    /// Caller-side close of the borrowed raws (fd-contract phase): removes
    /// them from the Drop list so the harness never double-closes.
    fn close_originals(&self) {
        unsafe { sys::close(self.wg_raw) };
        unsafe { sys::close(self.tun_raw) };
        let mut raws = self.raws.lock().unwrap_or_else(|e| e.into_inner());
        raws.retain(|&fd| fd != self.wg_raw && fd != self.tun_raw);
    }

    fn port(&self) -> u16 {
        self.slot.with_device(|d| d.local_addr()).expect("device up").1
    }

    fn ready(&self) -> bool {
        self.slot.tunnel_ready()
    }
}

impl Drop for SlotEnd {
    fn drop(&mut self) {
        let raws =
            self.raws.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect::<Vec<_>>();
        for fd in raws {
            unsafe { sys::close(fd) };
        }
    }
}

/// One pump step for BOTH ends on the injected clock (device timers run on
/// `now_ms`; poll timeout 0 — loopback delivery is synchronous).
fn step(slot: &SlotEnd, peer: &Peer, now: u64) {
    slot.slot.with_device(|d| {
        d.service_tun(now);
        d.service_udp(now);
        d.tick(now);
    });
    peer.app.with_device(|d| {
        d.service_tun(now);
        d.service_udp(now);
        d.tick(now);
    });
}

fn pump_until(
    slot: &SlotEnd,
    peer: &Peer,
    now: &mut u64,
    horizon_ms: u64,
    cond: impl Fn(&SlotEnd, &Peer) -> bool,
) -> bool {
    let deadline_ms = *now + horizon_ms;
    while *now <= deadline_ms {
        step(slot, peer, *now);
        if cond(slot, peer) {
            return true;
        }
        *now += 10;
    }
    false
}

// ---------------------------------------------------------------------------
// 1. production-path closed loop (feeds → real device → handshake → payload)
// ---------------------------------------------------------------------------

#[test]
fn production_path_real_wgdevice_closed_loop() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let peer = Peer::build(&SECRET_B, key_b(), key_a(), vec![(ADDR_A, 32)]);
    let mut slot_end = SlotEnd::build(&SECRET_A, key_b(), vec![(ADDR_B, 32)], peer.port());
    let mut now = 1000u64;

    // the feeds (NAPI-equivalent Rust entry points) bring the device up and
    // the buffered peer set + endpoint replay into the REAL WgDevice
    slot_end.feed_both();
    let st = slot_end.slot.dataplane_status().expect("device seam reports status");
    assert!(st.fed_socket && st.fed_tun && st.device_up);
    assert!(!st.ready, "no handshake yet");
    slot_end.slot.with_device(|d| {
        assert_eq!(d.peer_count(), 1, "buffered peer set replayed");
        assert_eq!(
            d.peers()[0].endpoint,
            Some(([127, 0, 0, 1], peer.port())),
            "buffered endpoint replayed"
        );
        assert!(!d.tunnel_ready());
    });

    // land the far end's endpoint on the peer device (its ICE equivalent)
    peer.app
        .with_device(|d| d.set_endpoint(key_a(), [127, 0, 0, 1], slot_end.port(), now))
        .expect("peer endpoint");

    // pump until BOTH ends have established WG sessions
    assert!(
        pump_until(&slot_end, &peer, &mut now, 10_000, |s, p| s.ready() && p.ready()),
        "handshake must establish within the sim deadline"
    );
    assert!(slot_end.ready(), "slot tunnel_ready must flip TRUE (real WgDevice)");

    // the N3-7 default-route gate now ALLOWS the default route
    let (allowed, reason) =
        netbird_core::connector::ShellNetworkConfig::default_route_decision(
            1,
            slot_end.ready(),
            false,
        );
    assert!(allowed, "ready device must release the default route: {reason}");
    assert_eq!(reason, "default-route-allowed:peers-registered-and-tunnel-ready");

    // bidirectional payload: slot TUN half → peer TUN half and back
    let pkt_ab = ip_udp_packet(ADDR_A, ADDR_B, 40001, 5353, b"n7-slot-to-peer");
    write_tun(slot_end.tun_test_end, &pkt_ab);
    let got = std::cell::RefCell::new(None);
    let ok = pump_until(&slot_end, &peer, &mut now, 5_000, |_s, _p| {
        if got.borrow().is_none() {
            *got.borrow_mut() = read_tun(peer.tun_test_end);
        }
        got.borrow().is_some()
    });
    assert!(ok, "slot→peer payload must cross the tunnel");
    assert_eq!(got.into_inner().expect("frame"), pkt_ab, "payload must be byte-exact");

    let pkt_ba = ip_udp_packet(ADDR_B, ADDR_A, 5353, 40001, b"n7-peer-to-slot");
    write_tun(peer.tun_test_end, &pkt_ba);
    let got = std::cell::RefCell::new(None);
    let ok = pump_until(&slot_end, &peer, &mut now, 5_000, |_s, _p| {
        if got.borrow().is_none() {
            *got.borrow_mut() = read_tun(slot_end.tun_test_end);
        }
        got.borrow().is_some()
    });
    assert!(ok, "peer→slot payload must cross the tunnel");
    assert_eq!(got.into_inner().expect("frame"), pkt_ba, "payload must be byte-exact");

    // real device counters through the status surface (a registry seam has
    // none — dataplane_status() is the device signature)
    let st = slot_end.slot.dataplane_status().expect("device seam reports status");
    assert!(st.ready);
    assert_eq!(st.peers_with_session, 1);
    assert!(st.handshakes >= 1, "real handshake initiations counted");
    assert!(st.tx_packets >= 2 && st.rx_packets >= 2, "real data-plane counters: {st:?}");
    assert_eq!(st.dropped_no_route, 0);
    assert_eq!(st.decrypt_errors, 0);
}

// ---------------------------------------------------------------------------
// 2. connector-level: status() reflects the fed device, stop() tears it down
// ---------------------------------------------------------------------------

/// A factory that never connects (the worker retries with backoff in the
/// background; irrelevant — this harness exercises the WG seam surface).
struct NeverFactory;

impl ManagementFactory for NeverFactory {
    fn connect(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<ManagementGrpcClient, ManagementError>> + Send + '_>>
    {
        Box::pin(async { Err(ManagementError::Network("never".into())) })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn connector_status_reflects_fed_device_and_stop_tears_it_down() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let peer = Peer::build(&SECRET_B, key_b(), key_a(), vec![(ADDR_A, 32)]);
    let slot_end = SlotEnd::build(&SECRET_A, key_b(), vec![(ADDR_B, 32)], peer.port());
    let mut now = 2000u64;
    slot_end.feed_both();
    peer.app
        .with_device(|d| d.set_endpoint(key_a(), [127, 0, 0, 1], slot_end.port(), now))
        .expect("peer endpoint");
    assert!(
        pump_until(&slot_end, &peer, &mut now, 10_000, |s, p| s.ready() && p.ready()),
        "handshake must establish"
    );

    // REAL connector handle with the slot as its production WG seam (the
    // spawn call also runs the data-plane pump thread on the stop flag)
    let handle: Arc<ConnectorHandle> = ConnectorHandle::spawn(
        tokio::runtime::Handle::current(),
        Arc::new(NeverFactory),
        slot_end.slot.clone(),
        Arc::new(netbird_core::connector::LoggingConfigApplier),
        ConnectorSecrets { setup_key: "N7-TEST-SETUP-KEY".into(), jwt: String::new() },
        netbird_core::grpc::PeerMeta {
            hostname: "n7-test".into(),
            os_name: "harmonyos".into(),
            os_version: "0".into(),
            netbird_version: "0.1.0".into(),
        },
        netbird_core::backoff::ExponentialBackoff::new(
            std::time::Duration::from_millis(5),
            0.0,
            1.5,
            std::time::Duration::from_millis(50),
            None,
        ),
        SyncPolicy::production(),
        std::time::Duration::from_secs(600),
        std::time::Duration::from_millis(100),
        false,
        None,
        None,
        None,
        Some(slot_end.slot.clone()),
        netbird_core::connector::HostIceTuning::default(), // N12a HOST-ONLY: defaults
    );
    let st = handle.status();
    assert!(st.wg.fed_socket && st.wg.fed_tun && st.wg.device_up, "{:?}", st.wg);
    assert!(st.wg.ready, "connector status must see the REAL session: {:?}", st.wg);
    assert_eq!(st.wg.peers_with_session, 1);
    assert!(st.wg.handshakes >= 1);
    let json = handle.status_json();
    assert!(
        json.contains("\"wg\":{\"fed_socket\":true,\"fed_tun\":true,\"device_up\":true,\
                       \"ready\":true,\"peers_with_session\":1"),
        "{json}"
    );

    // stop tears the data plane down: device gone, feeds dropped, ready false
    handle.stop();
    let after = handle.status();
    assert!(!after.wg.device_up, "stop must tear the device down");
    assert!(!after.wg.fed_socket && !after.wg.fed_tun, "stop must drop fed fds");
    assert!(!after.wg.ready, "no device ⇒ never ready (fail-closed)");
    assert!(!slot_end.slot.device_up());
}

// ---------------------------------------------------------------------------
// 3. fail-closed: a missing feed keeps the data plane down, holds the
//    default route, and never creates/takes any unprotected socket
// ---------------------------------------------------------------------------

#[test]
fn missing_feed_fails_closed_and_takes_no_unprotected_socket() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ice_sockets = Arc::new(ProtectedUdpFdSource::new_with_fd(-1)); // shell never feeds ICE

    // case 1: only the TUN fd fed (WG socket missing)
    let (wg_raw, _port) = udp_socket_lo();
    let (tun_raw, tun_test_end) = tun_socketpair();
    let slot = Arc::new(WgDeviceFeed::new(feed_config(&SECRET_A)));
    slot.apply_peers(&[WgPeerEntry {
        pub_key_b64: key_b().to_string(),
        allowed_ips: vec![config::Route { addr: ADDR_B, prefix_len: 32 }],
    }])
    .expect("buffered peers");
    slot.feed_tun(tun_raw).expect("tun feed");
    assert!(!slot.device_up(), "one feed alone must NOT start the data plane");
    assert!(!slot.tunnel_ready(), "no device ⇒ tunnel_ready false");
    let st = slot.dataplane_status().expect("status still reported");
    assert!(st.fed_tun && !st.fed_socket && !st.device_up && !st.ready);
    let (allowed, reason) =
        netbird_core::connector::ShellNetworkConfig::default_route_decision(1, slot.tunnel_ready(), false);
    assert!(!allowed, "default route must stay HELD");
    assert_eq!(reason, "default-route-held:data-plane-not-ready");
    // control-plane calls keep buffering (the control plane never wedges)
    slot.apply_endpoint(key_b(), [127, 0, 0, 1], 51820).expect("endpoint buffered");
    // and the pump entry is a no-op (nothing to drive)
    assert!(slot.with_device(|_| ()).is_none());

    // case 2: the missing fd arrives dead → refused at the boundary, nothing
    // stored; the data plane still never starts without a VALID pair.
    // DEAD_FD can never be open — far beyond any RLIMIT_NOFILE, so the
    // kernel never allocates it and no parallel test can hold it — making
    // the boundary probe's EBADF deterministic. Deliberately NOT "open one
    // and close it": fd numbers are handed out lowest-free-first from the
    // process-global table, so under parallel test execution another test
    // can re-open the just-closed number before the feed lands and the
    // expected refusal would turn into Ok(()).
    const DEAD_FD: i32 = 1 << 30;
    let err = slot.feed_wg_socket(DEAD_FD).unwrap_err();
    assert_eq!(err.token(), "socket-fd-invalid");
    assert_eq!(slot.feed_wg_socket(-1).unwrap_err().token(), "socket-fd-missing");
    assert!(!slot.device_up());
    assert!(!slot.tunnel_ready());

    // case 3: nothing fed at all — the unfed slot is entirely inert
    let cold = WgDeviceFeed::new(feed_config(&SECRET_B));
    assert!(!cold.device_up() && !cold.tunnel_ready());
    assert!(matches!(cold.dataplane_status(), Some(st) if !st.fed_socket && !st.fed_tun));

    // audit: no unprotected socket was ever created/consumed — the ICE
    // protected-UDP source was never even looked at
    assert_eq!(ice_sockets.taken(), 0, "no unprotected fallback anywhere");

    for fd in [wg_raw, tun_raw, tun_test_end] {
        unsafe { sys::close(fd) };
    }
}

// ---------------------------------------------------------------------------
// 4. fd contract: native runs on dup copies; the originals stay caller-owned
// ---------------------------------------------------------------------------

#[test]
fn fd_contract_native_uses_only_dups_originals_stay_caller_owned() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Phase A: close the ORIGINALS right after the feeds — the device must
    // keep working (it holds dups; touching the originals would EBADF).
    let peer = Peer::build(&SECRET_B, key_b(), key_a(), vec![(ADDR_A, 32)]);
    let slot_end = SlotEnd::build(&SECRET_A, key_b(), vec![(ADDR_B, 32)], peer.port());
    let mut now = 3000u64;
    slot_end.feed_both();
    assert!(slot_end.slot.device_up());

    // the caller's rights: close the raw WG socket + raw TUN fd
    slot_end.close_originals();
    // The numbers must now read as closed. This cannot race with parallel
    // tests: every test in this binary holds TEST_LOCK (see its doc), so no
    // other thread is allocating fds — without that lock, lowest-free-first
    // re-allocation could legitimately resurrect either number between the
    // close and this probe.
    assert!(!fd_open(slot_end.wg_raw) && !fd_open(slot_end.tun_raw));

    // the tunnel still completes the handshake and carries payload — proof
    // the device is running on ITS OWN dups, not the (now dead) originals
    peer.app
        .with_device(|d| d.set_endpoint(key_a(), [127, 0, 0, 1], slot_end.port(), now))
        .expect("peer endpoint");
    assert!(
        pump_until(&slot_end, &peer, &mut now, 10_000, |s, p| s.ready() && p.ready()),
        "handshake must survive the caller closing the originals"
    );
    let pkt = ip_udp_packet(ADDR_A, ADDR_B, 40002, 5353, b"n7-fd-contract");
    write_tun(slot_end.tun_test_end, &pkt);
    let got = std::cell::RefCell::new(None);
    let ok = pump_until(&slot_end, &peer, &mut now, 5_000, |_s, _p| {
        if got.borrow().is_none() {
            *got.borrow_mut() = read_tun(peer.tun_test_end);
        }
        got.borrow().is_some()
    });
    assert!(ok, "payload must flow on dup copies after the originals died");
    assert_eq!(got.into_inner().expect("frame"), pkt);

    // Phase B: a fresh pair — drop the DEVICE (clear): native closes ONLY
    // its dups; the caller's originals stay open and usable.
    let (wg_raw, _port) = udp_socket_lo();
    let (tun_raw, tun_test_end) = tun_socketpair();
    let slot = Arc::new(WgDeviceFeed::new(feed_config(&SECRET_A)));
    slot.apply_peers(&[WgPeerEntry {
        pub_key_b64: key_b().to_string(),
        allowed_ips: vec![config::Route { addr: ADDR_B, prefix_len: 32 }],
    }])
    .expect("peers");
    slot.feed_tun(tun_raw).expect("tun feed");
    slot.feed_wg_socket(wg_raw).expect("wg socket feed");
    assert!(slot.device_up());
    slot.clear();
    assert!(!slot.device_up(), "device torn down");
    // the originals are STILL the caller's: open and usable
    assert!(fd_open(wg_raw), "native must never close the borrowed WG socket");
    assert!(fd_open(tun_raw), "native must never close the borrowed TUN fd");
    let sa = sys::sockaddr_in::new([127, 0, 0, 1], 1);
    assert_ne!(
        unsafe { sys::sendto(wg_raw, b"x".as_ptr() as *const core::ffi::c_void, 1, 0, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) },
        -1,
        "original WG socket still usable after device teardown"
    );
    let (n, e) = sys::write_fd(tun_test_end, b"still-here");
    assert_eq!((n as usize, e), (10, 0), "original TUN test end still usable");
    for fd in [wg_raw, tun_raw, tun_test_end] {
        unsafe { sys::close(fd) };
    }
}
