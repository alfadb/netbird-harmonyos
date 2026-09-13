// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N8 integration tests: the controlled-recreate data plane — TUN fd
//! replacement in the shell-fed production seam (`WgDeviceFeed`). Same host
//! test pattern as tests/wg_feed_n7.rs: real BoringTun tunnels, loopback UDP
//! sockets as the WG "cable", datagram socketpairs playing the TUN half,
//! injected clock — no sleeps, no device.
//!
//! Background (the N8 problem): the platform VpnConfig is fixed at
//! `VpnConnection.create()` time, while the N3-7 safety gate only releases
//! `0.0.0.0/0` once the WG handshake has completed — which is necessarily
//! AFTER create(). The fix is a controlled rebuild: destroy the connection,
//! create() again with the desired route set, feed the NEW TUN fd into the
//! still-running data plane. Proven here:
//!
//! - REPLACEMENT WITHOUT RE-HANDSHAKE: feeding a new platform TUN fd while
//!   the device is up swaps the plaintext sink/source and keeps the WG
//!   session (BoringTun tunnels never reference the TUN fd, and the outer
//!   UDP socket is untouched) — `tunnel_ready()` stays TRUE across the swap,
//!   the handshake-initiation counter does not move, and bidirectional
//!   payload crosses the NEW fd byte-exact immediately after.
//! - FD CONTRACT (§二.4) through the whole recreate sequence: the platform
//!   destroy() is simulated by closing the shell-side raw fd FIRST (the
//!   device keeps running on its own dup — the dup holds the open-file
//!   description); the replacement adopts a dup of the NEW raw (the device
//!   fd is distinct from every raw number); the shell-side raws are never
//!   closed by native (the old raw stays closed only because the TEST
//!   closed it; the new raw stays open and usable); the OLD TUN is
//!   deactivated, not drained — a frame parked on the old end is still
//!   there after the swap.
//! - FAIL-CLOSED: a dead TUN fd fed during recreate is refused at the
//!   boundary (`socket-fd-invalid`), the device stays on the OLD tun with
//!   its session (no half state), and the shell-side teardown path
//!   (`clear()`) drops everything back to the N7 cold state (feeds gone,
//!   `tunnel_ready=false`, default route HELD).
//! - SOCKET SWAP RULE: while the device is up a WG-socket feed is an
//!   idempotent no-op for the SAME number (session preserved — a rebuild
//!   would have dropped it) and refused (`socket-fd-conflict`) for a
//!   DIFFERENT number; the protected outer socket is never swapped.
//!
//! The recreate REQUEST state machine (gate flip → `recreate.required`
//! exactly once, budget + cooldown bounds) is pinned by the `connector.rs`
//! unit tests (`recreate_required_raises_once_on_gate_open_and_clears_on_
//! recreate_ack`, `recreate_flap_is_bounded_by_cooldown_and_limit`).
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
        _result: *mut *mut c_void,
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

use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine as _;
use boringtun::ffi;

use netbird_core::config;
use netbird_core::connector::{
    ShellNetworkConfig, WgPeerApplier, WgPeerEntry,
};
use netbird_core::sys;
use netbird_core::tun::TunFd;
use netbird_core::wg_device::{WgDevice, WgDeviceApplier, WgDeviceConfig, WgDeviceFeed};

/// Slot side (the "client" the connector drives): 32 bytes of 0xA5.
const SECRET_A: [u8; 32] = [0xA5; 32];
/// Far-end peer device: 32 bytes of 0xB6.
const SECRET_B: [u8; 32] = [0xB6; 32];

/// Tunnel addresses inside the routed range.
const ADDR_A: [u8; 4] = [10, 78, 0, 1];
const ADDR_B: [u8; 4] = [10, 78, 0, 2];

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
/// within the process — same as tests/wg_feed_n7.rs).
fn recreate_config(secret: &[u8; 32]) -> WgDeviceConfig {
    let mut cfg = WgDeviceConfig::new(b64(secret));
    cfg.hs_retry_ms = 100;
    cfg.hs_deadline_ms = 10_000;
    cfg.keepalive_ms = u64::MAX / 2;
    cfg.session_max_ms = u64::MAX / 2;
    cfg
}

// ---------------------------------------------------------------------------
// socket helpers (host; same shapes as tests/wg_feed_n7.rs)
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
/// diagnosable (same as tests/wg_feed_n7.rs).
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
// the far-end peer device (plain WgDeviceApplier, wg_feed_n7-style)
// ---------------------------------------------------------------------------

struct Peer {
    app: Arc<WgDeviceApplier>,
    tun_test_end: i32,
    raws: Mutex<Vec<i32>>,
}

impl Peer {
    fn build(secret: &[u8; 32], peer_key: &str, peer_ips: Vec<([u8; 4], u8)>) -> Peer {
        let (wg_raw, _port) = udp_socket_lo();
        let (tun_raw, test_end) = tun_socketpair();
        let tun = TunFd::dup_from_raw(tun_raw).expect("tun dup");
        let dev = WgDevice::adopt(recreate_config(secret), wg_raw, tun).expect("device adopt");
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
// the shell-fed slot side, with recreate support
// ---------------------------------------------------------------------------

struct SlotEnd {
    slot: Arc<WgDeviceFeed>,
    wg_raw: i32,
    tun_raw: i32,
    tun_test_end: i32,
    /// Raws the TEST still owns and must close on drop. Recreate removes the
    /// destroyed tun raw from this list (the "platform" closed it, not us).
    raws: Mutex<Vec<i32>>,
}

impl SlotEnd {
    /// Unfed slot + its raw fds; peer set + endpoint buffered pre-feed (the
    /// production ordering).
    fn build(
        secret: &[u8; 32],
        peer_key: &str,
        peer_ips: Vec<([u8; 4], u8)>,
        peer_port: u16,
    ) -> SlotEnd {
        let (wg_raw, _port) = udp_socket_lo();
        let (tun_raw, tun_test_end) = tun_socketpair();
        let slot = Arc::new(WgDeviceFeed::new(recreate_config(secret)));
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
        assert!(!slot.device_up() && !slot.tunnel_ready());
        SlotEnd {
            slot,
            wg_raw,
            tun_raw,
            tun_test_end,
            raws: Mutex::new(vec![wg_raw, tun_raw, tun_test_end]),
        }
    }

    /// THE feed entry points (the exact functions behind the NAPI exports).
    fn feed_both(&self) {
        self.slot.feed_tun(self.tun_raw).expect("tun feed");
        self.slot.feed_wg_socket(self.wg_raw).expect("wg socket feed");
        assert!(self.slot.device_up(), "both feeds must bring the device up");
    }

    /// The "platform destroy()" leg of the recreate: the raw TUN fd is
    /// closed EXCLUSIVELY by the platform (fd contract — the test plays the
    /// platform here); the device keeps running on its own dup.
    fn platform_destroy_tun(&mut self) {
        assert!(fd_open(self.tun_raw));
        unsafe { sys::close(self.tun_raw) };
        self.raws
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|&fd| fd != self.tun_raw);
        self.tun_raw = -1;
        self.tun_test_end = -1;
    }

    /// The "new create()" leg: a fresh platform TUN fd pair, fed through the
    /// SAME feed entry (N8: replace-while-up).
    fn recreate_tun(&mut self) {
        let (new_raw, new_end) = tun_socketpair();
        self.slot.feed_tun(new_raw).expect("recreate tun feed (replace)");
        self.raws
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend([new_raw, new_end]);
        self.tun_raw = new_raw;
        self.tun_test_end = new_end;
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

/// One pump step for BOTH ends on the injected clock.
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

fn establish(slot_end: &SlotEnd, peer: &Peer, now: &mut u64) {
    peer.app
        .with_device(|d| d.set_endpoint(key_a(), [127, 0, 0, 1], slot_end.port(), *now))
        .expect("peer endpoint");
    assert!(
        pump_until(slot_end, peer, now, 10_000, |s, p| s.ready() && p.ready()),
        "handshake must establish within the sim deadline"
    );
}

/// Push a frame through `from`'s TUN half and pump until it falls out of
/// `to`'s TUN half (byte-exact assert is the caller's).
fn carry_frame(from: &SlotEnd, to: &Peer, now: &mut u64, frame: &[u8]) -> Vec<u8> {
    write_tun(from.tun_test_end, frame);
    let got = std::cell::RefCell::new(None);
    let ok = pump_until(from, to, now, 5_000, |_s, _p| {
        if got.borrow().is_none() {
            *got.borrow_mut() = read_tun(to.tun_test_end);
        }
        got.borrow().is_some()
    });
    assert!(ok, "frame must cross the tunnel within the sim deadline");
    got.into_inner().expect("frame")
}

// ---------------------------------------------------------------------------
// 1. recreate: TUN fd replaced, session kept, payload bidirectional on the
//    NEW fd, fd contract intact end-to-end
// ---------------------------------------------------------------------------

#[test]
fn tun_replacement_keeps_sessions_and_carries_bidirectional_payload() {
    let peer = Peer::build(&SECRET_B, key_a(), vec![(ADDR_A, 32)]);
    let mut slot_end = SlotEnd::build(&SECRET_A, key_b(), vec![(ADDR_B, 32)], peer.port());
    let mut now = 1000u64;

    // Phase 0 — first generation: feeds → handshake → payload path works
    slot_end.feed_both();
    establish(&slot_end, &peer, &mut now);
    assert!(slot_end.ready());
    let pkt0 = ip_udp_packet(ADDR_A, ADDR_B, 40001, 5353, b"n8-gen-one");
    assert_eq!(carry_frame(&slot_end, &peer, &mut now, &pkt0), pkt0, "generation-1 payload");
    let handshakes_before = slot_end
        .slot
        .dataplane_status()
        .expect("status")
        .handshakes;

    // Phase 1 — "VpnConnection.destroy()": the PLATFORM closes the raw TUN
    // fd (fd contract). The device must keep running on its own dup. NOTE:
    // we deliberately do NOT re-probe the closed NUMBER — under parallel
    // test execution another test may reopen that number before we look
    // (process-global fd table), so the non-determinism is in the probe,
    // not in the contract; the contract is proven by payload continuing on
    // the device's dup below.
    slot_end.platform_destroy_tun();
    assert!(slot_end.ready(), "device dup survives the platform destroy");
    step(&slot_end, &peer, now); // pump the orphaned-dup generation once
    now += 10;

    // Phase 2 — "VpnConnection.create()" with the desired route set: the NEW
    // raw TUN fd is fed through the SAME entry; the device adopts it WITHOUT
    // a re-handshake (session preserved — the WG keys never touched the TUN).
    slot_end.recreate_tun();
    assert!(slot_end.slot.device_up(), "replacement keeps the device up");
    assert!(
        slot_end.ready(),
        "session must survive the TUN swap (no re-handshake)"
    );
    let st = slot_end.slot.dataplane_status().expect("status");
    assert_eq!(
        st.handshakes, handshakes_before,
        "no new handshake may be needed for a TUN replacement"
    );

    // FD CONTRACT: the device's TUN fd is ITS OWN dup — distinct from every
    // FD CONTRACT: the device's TUN fd is ITS OWN dup — distinct from the
    // still-open shell-side raw (dup never returns an already-open number,
    // so this comparison is deterministic). We do NOT compare against the
    // CLOSED old number: under parallel test execution that number may be
    // reused (it is then legitimately the lowest free fd dup may return) —
    // the old generation's deactivation is proven behaviorally in Phase 4.
    let dev_tun_fd = slot_end.slot.with_device(|d| d.tun_fd()).expect("tun fd");
    assert_ne!(Some(slot_end.tun_raw), dev_tun_fd, "device must run on a dup, not the raw");
    // the NEW shell-side raw is untouched by native (still open, still ours)
    assert!(fd_open(slot_end.tun_raw), "native must never close the shell-side raw");
    // ...and the OLD shell-side raw stays closed exactly as the platform
    // left it — native neither resurrected nor needed it.

    // Phase 3 — bidirectional payload over the NEW fd, byte-exact
    let pkt = ip_udp_packet(ADDR_A, ADDR_B, 40002, 5353, b"n8-gen-two-a2b");
    assert_eq!(carry_frame(&slot_end, &peer, &mut now, &pkt), pkt, "A→B over the new fd");
    let pkt_ba = ip_udp_packet(ADDR_B, ADDR_A, 5353, 40002, b"n8-gen-two-b2a");
    write_tun(peer.tun_test_end, &pkt_ba);
    let got = std::cell::RefCell::new(None);
    let ok = pump_until(&slot_end, &peer, &mut now, 5_000, |_s, _p| {
        if got.borrow().is_none() {
            *got.borrow_mut() = read_tun(slot_end.tun_test_end);
        }
        got.borrow().is_some()
    });
    assert!(ok, "B→A must cross over the new fd");
    assert_eq!(got.into_inner().expect("frame"), pkt_ba);

    // real counters through the status surface: the data plane never reset
    let st = slot_end.slot.dataplane_status().expect("status");
    assert!(st.ready && st.peers_with_session == 1);
    assert!(st.tx_packets >= 3 && st.rx_packets >= 3, "counters continuous across swap");
    assert_eq!(st.dropped_no_route, 0);
    assert_eq!(st.decrypt_errors, 0);

    // Phase 4 — the OLD tun end is DEACTIVATED, not drained: a frame parked
    // on the old (test) end is never consumed by the device; the destroy
    // leg closed the raw, so park it on a fresh socketpair to prove the
    // device only ever reads its CURRENT tun.
    let (old_gen_raw, old_gen_end) = tun_socketpair();
    // feed old_gen as a replacement, then replace AGAIN with the good one:
    // after the second replace the device must not see old_gen's frame.
    slot_end.slot.feed_tun(old_gen_raw).expect("intermediate replace");
    slot_end.slot.feed_tun(slot_end.tun_raw).expect("final replace back to the current raw");
    write_tun(old_gen_end, b"stale-frame-must-not-be-read");
    let tx_before = slot_end.slot.dataplane_status().expect("status").tx_packets;
    step(&slot_end, &peer, now);
    assert_eq!(
        slot_end.slot.dataplane_status().expect("status").tx_packets,
        tx_before,
        "the device must not route anything from the deactivated fd"
    );
    assert_eq!(
        read_tun(old_gen_raw).as_deref(),
        Some(&b"stale-frame-must-not-be-read"[..]),
        "the frame still sits in the deactivated fd's receive queue — \
         the device neither read nor closed it"
    );
    slot_end
        .raws
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|&fd| fd != old_gen_raw && fd != old_gen_end);
    unsafe { sys::close(old_gen_raw) };
    unsafe { sys::close(old_gen_end) };
}

// ---------------------------------------------------------------------------
// 2. fail-closed: a dead TUN fd during recreate is refused, no half state;
//    the teardown path returns the seam to the cold N7 state
// ---------------------------------------------------------------------------

#[test]
fn recreate_dead_fd_refused_no_half_state_then_fail_closed_teardown() {
    let peer = Peer::build(&SECRET_B, key_a(), vec![(ADDR_A, 32)]);
    let slot_end = SlotEnd::build(&SECRET_A, key_b(), vec![(ADDR_B, 32)], peer.port());
    let mut now = 2000u64;
    slot_end.feed_both();
    establish(&slot_end, &peer, &mut now);
    assert!(slot_end.ready());

    // a DEAD fd ("create() handed us a closed fd" / protect-path casualty):
    // refused at the boundary, the device stays on the OLD tun untouched.
    // DEAD_FD can never be open — far beyond any RLIMIT_NOFILE, so the
    // kernel never allocates it and no parallel test can hold it — making
    // the boundary probe's EBADF deterministic. Deliberately NOT "open one
    // and close it": fd numbers are handed out lowest-free-first from the
    // process-global table, so under parallel test execution another test
    // can re-open the just-closed number before the feed lands and the
    // expected refusal would turn into Ok(()).
    const DEAD_FD: i32 = 1 << 30;
    let err = slot_end.slot.feed_tun(DEAD_FD).unwrap_err();
    assert_eq!(err.token(), "socket-fd-invalid", "dead fd refused at the boundary");
    assert_eq!(err.errno(), sys::EBADF);
    // NO half state: device still up, session still live, old fd still active
    assert!(slot_end.slot.device_up());
    assert!(slot_end.ready(), "refused recreate must not disturb the live session");
    let st = slot_end.slot.dataplane_status().expect("status");
    assert!(st.fed_tun && st.device_up && st.ready);
    let pkt = ip_udp_packet(ADDR_A, ADDR_B, 41001, 5353, b"n8-still-alive");
    assert_eq!(carry_frame(&slot_end, &peer, &mut now, &pkt), pkt, "old path still carries");

    // the shell's fail-closed reaction to a failed recreate: tear the VPN
    // down (connector stop ⇒ clear()). Everything drops to the cold N7
    // state — no default route, no device, no feeds.
    slot_end.slot.clear();
    assert!(!slot_end.slot.device_up(), "teardown removes the device");
    assert!(!slot_end.slot.tunnel_ready(), "no device ⇒ never ready");
    let st = slot_end.slot.dataplane_status().expect("status");
    assert!(!st.fed_socket && !st.fed_tun && !st.device_up && !st.ready);
    let (allowed, reason) = ShellNetworkConfig::default_route_decision(
        1,
        slot_end.slot.tunnel_ready(),
        false,
    );
    assert!(!allowed, "back to HOLD after a failed recreate teardown");
    assert_eq!(reason, "default-route-held:data-plane-not-ready");
    // the shell-side raws were never native's to close: all still open
    assert!(fd_open(slot_end.wg_raw) && fd_open(slot_end.tun_raw));
}

// ---------------------------------------------------------------------------
// 3. socket swap rule: same-fd feed idempotent (session preserved), a
//    different outer socket refused while the device is up
// ---------------------------------------------------------------------------

#[test]
fn wg_socket_feed_idempotent_same_fd_and_refuses_swap_while_device_up() {
    let peer = Peer::build(&SECRET_B, key_a(), vec![(ADDR_A, 32)]);
    let slot_end = SlotEnd::build(&SECRET_A, key_b(), vec![(ADDR_B, 32)], peer.port());
    let mut now = 3000u64;
    slot_end.feed_both();
    establish(&slot_end, &peer, &mut now);
    assert!(slot_end.ready());

    // SAME number again (the recreate flow re-feeds nothing, but the shell
    // may replay its feed sequence): idempotent no-op — and crucially the
    // device is NOT rebuilt (a rebuild would have dropped the session).
    slot_end.slot.feed_wg_socket(slot_end.wg_raw).expect("idempotent re-feed");
    assert!(slot_end.slot.device_up());
    assert!(slot_end.ready(), "idempotent re-feed must not rebuild the device");

    // a DIFFERENT bound socket: refused fail-closed (the protected outer
    // socket is not swappable in N8), device untouched
    let (other_raw, _other_port) = udp_socket_lo();
    let err = slot_end.slot.feed_wg_socket(other_raw).unwrap_err();
    assert_eq!(err.token(), "socket-fd-conflict");
    assert!(slot_end.slot.device_up() && slot_end.ready(), "refusal changes nothing");

    // while the device is DOWN the old N7 rules still hold: a fresh pair
    // builds a fresh device (cold start path unchanged)
    slot_end.slot.clear();
    assert!(!slot_end.slot.device_up());
    let (wg2, _p2) = udp_socket_lo();
    let (tun2, tun2_end) = tun_socketpair();
    slot_end.slot.feed_tun(tun2).expect("cold tun feed");
    slot_end.slot.feed_wg_socket(wg2).expect("cold socket feed");
    assert!(slot_end.slot.device_up(), "cold rebuild works after clear");
    assert!(!slot_end.slot.tunnel_ready(), "fresh device: no session yet");
    for fd in [other_raw, wg2, tun2, tun2_end] {
        unsafe { sys::close(fd) };
    }
}
