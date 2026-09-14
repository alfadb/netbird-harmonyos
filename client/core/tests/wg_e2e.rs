// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N6 end-to-end data-plane tests: TWO (and once THREE) full WireGuard
//! devices (`wg_device::WgDevice`) in ONE process — real BoringTun tunnels
//! (the on-device-verified ffi path) + real loopback UDP sockets as the WG
//! "cable" + socketpair datagram ends playing the TUN half (same host-test
//! pattern as tests/tun_fd_contract.rs; DATAGRAM socketpairs preserve frame
//! boundaries, one write == one read == one frame).
//!
//! Proven here (the N6 acceptance core):
//! - bidirectional payload end-to-end: A writes an IP/UDP packet into its
//!   TUN side, B reads the SAME bytes out of its TUN side, and the reverse;
//! - the bytes ON THE WIRE are ciphertext: tapped straight off B's UDP
//!   socket they differ from the plaintext, carry the exact +32 transport
//!   overhead, contain the payload NOWHERE, and the same plaintext encrypts
//!   differently twice (fresh nonce) — a real cipher, not a pass-through;
//! - allowed_ips LONGEST-PREFIX routing: 10.99.0.2 hits the /32 peer, the
//!   /16 peer gets the rest of the range, unmatched destinations are
//!   dropped + counted (no panic);
//! - `tunnel_ready()` is FALSE before the handshake and TRUE after, and the
//!   N3-7 default-route gate follows it through the real `WgDeviceApplier`
//!   seam (fail-closed stays: no peers ⇒ held, force ⇒ loud override,
//!   clear ⇒ held);
//! - handshake RETRANSMISSION: a dropped initiation is re-sent on the
//!   injected clock with a fresh ephemeral, and the session still
//!   establishes and carries payload;
//! - ENDPOINT CHANGE (ICE re-selection shape): after the local WG socket is
//!   re-attached (new port) and the peer's endpoint re-lands, traffic
//!   follows the new path and the old socket stays silent;
//! - the ICE harness itself lands the dataplane: two `PeerIceOrchestrator`s
//!   select a pair, the selected-pair `apply_endpoint` call drives the REAL
//!   devices, and payload flows — the landed endpoint IS the peer's WG
//!   socket (the selected candidate socket).
//!
//! No sleeps anywhere: device timers (keepalive / handshake retry / session
//! expiry) run on the injected `now_ms`; pump loops poll with timeout 0 and
//! bound their iterations (loopback delivery inside one process is
//! synchronous). Test keys below are EXPLICIT synthetic constants — never
//! real deployment material.

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

use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine as _;
use boringtun::ffi;

use netbird_core::config;
use netbird_core::connector::{ShellNetworkConfig, WgPeerApplier, WgPeerEntry};
use netbird_core::ice::{InterfaceAddr, ProtectedUdpFdSource, StaticInterfaces};
use netbird_core::network_map::{ManagedRoute, NetworkMap, PeerInfo};
use netbird_core::peer_conn::{PeerIceDeps, PeerIceOrchestrator, PeerIceState, PeerSignalKind, SignalExchange};
use netbird_core::sys;
use netbird_core::tun::TunFd;
use netbird_core::wg_device::{WgDevice, WgDeviceApplier, WgDeviceConfig, WgDeviceStats};

// ---------------------------------------------------------------------------
// explicit TEST key material (synthetic; never real deployment secrets) and
// the identity domain shared with the signal layer (base64 WG public keys)
// ---------------------------------------------------------------------------

/// Device A test secret: 32 bytes of 0xA5.
const SECRET_A: [u8; 32] = [0xA5; 32];
/// Device B test secret: 32 bytes of 0xB6.
const SECRET_B: [u8; 32] = [0xB6; 32];
/// Device C test secret: 32 bytes of 0xC7.
const SECRET_C: [u8; 32] = [0xC7; 32];

/// Tunnel addresses (NetBird-style /32 identities inside the routed range).
const ADDR_A: [u8; 4] = [10, 99, 0, 1];
const ADDR_B: [u8; 4] = [10, 99, 0, 2];

fn pub_key_raw(secret: &[u8; 32]) -> [u8; 32] {
    ffi::x25519_public_key(ffi::x25519_key { key: *secret }).key
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn static_key(secret: &[u8; 32], slot: &'static OnceLock<String>) -> &'static str {
    slot.get_or_init(|| b64(&pub_key_raw(secret)))
}

/// Each identity is derived through the same frozen `x25519_public_key`
/// export the probes use, so both ends of a test pair agree on the peer
/// identity by construction (the same string is the signal-layer key).
fn key_a() -> &'static str {
    static K: OnceLock<String> = OnceLock::new();
    static_key(&SECRET_A, &K)
}
fn key_b() -> &'static str {
    static K: OnceLock<String> = OnceLock::new();
    static_key(&SECRET_B, &K)
}
fn key_c() -> &'static str {
    static K: OnceLock<String> = OnceLock::new();
    static_key(&SECRET_C, &K)
}

// ---------------------------------------------------------------------------
// socket helpers (host)
// ---------------------------------------------------------------------------

extern "C" {
    fn socketpair(domain: i32, ty: i32, protocol: i32, sv: *mut [i32; 2]) -> i32;
}

/// Bound loopback UDP socket (the "cable end" a device adopts); returns
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

fn sock_port(fd: i32) -> u16 {
    let mut name = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut len = core::mem::size_of::<sys::sockaddr_in>() as u32;
    assert_eq!(unsafe { sys::getsockname(fd, &mut name, &mut len) }, 0);
    u16::from_be(name.sin_port)
}

/// Datagram socketpair: end 0 plays the platform TUN fd (the device dups it),
/// end 1 is the test's hand — one write == one frame in either direction.
fn tun_socketpair() -> (i32, i32) {
    let mut sv = [-1i32; 2];
    let rc = unsafe { socketpair(1 /* AF_UNIX */, 2 /* SOCK_DGRAM */, 0, &mut sv) };
    assert_eq!(rc, 0, "socketpair failed errno={}", sys::errno());
    (sv[0], sv[1])
}

/// One IPv4/UDP packet with a valid header checksum and a recognizable
/// payload (WireGuard does not validate inner checksums; we build them
/// correctly anyway so the frames are honest IP packets).
fn ip_udp_packet(src: [u8; 4], dst: [u8; 4], sport: u16, dport: u16, payload: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(28 + payload.len());
    let total = (20 + 8 + payload.len()) as u16;
    p.extend_from_slice(&[0x45, 0]);
    p.extend_from_slice(&total.to_be_bytes());
    p.extend_from_slice(&[0, 1, 0, 0, 64, 17, 0, 0]); // id, flags, ttl 64, proto UDP, cksum 0
    p.extend_from_slice(&src);
    p.extend_from_slice(&dst);
    p.extend_from_slice(&sport.to_be_bytes());
    p.extend_from_slice(&dport.to_be_bytes());
    p.extend_from_slice(&((8 + payload.len()) as u16).to_be_bytes());
    p.extend_from_slice(&[0, 0]); // udp cksum 0
    p.extend_from_slice(payload);
    let mut sum: u32 = 0;
    for w in p[..20].chunks(2) {
        sum += u16::from_be_bytes([w[0], w[1]]) as u32;
    }
    let ck = !(sum + (sum >> 16)) as u16;
    p[10..12].copy_from_slice(&ck.to_be_bytes());
    p
}

fn write_tun(node: &Node, frame: &[u8]) {
    let (n, e) = sys::write_fd(node.tun_test_end, frame);
    assert_eq!((n as usize, e), (frame.len(), 0), "TUN-side test write");
}

fn read_tun(node: &Node) -> Option<Vec<u8>> {
    let (ret, _e, rev) = sys::poll1(node.tun_test_end, sys::POLLIN, 0);
    if ret <= 0 || (rev & sys::POLLIN) == 0 {
        return None;
    }
    let mut buf = [0u8; 2048];
    let (n, e) = sys::read_fd(node.tun_test_end, &mut buf);
    assert!(n > 0, "TUN-side test read errno={e}");
    Some(buf[..n as usize].to_vec())
}

/// Wire tap: take ONE datagram straight off the node's raw WG socket (the
/// bytes as they crossed the loopback cable), before the device sees it.
fn wire_recv(node: &Node) -> Option<(Vec<u8>, ([u8; 4], u16))> {
    let (ret, _e, rev) = sys::poll1(node.wg_raw, sys::POLLIN, 0);
    if ret <= 0 || (rev & sys::POLLIN) == 0 {
        return None;
    }
    let mut buf = [0u8; 2048];
    let mut from = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut flen = core::mem::size_of::<sys::sockaddr_in>() as u32;
    let n = unsafe {
        sys::recvfrom(
            node.wg_raw,
            buf.as_mut_ptr() as *mut core::ffi::c_void,
            buf.len(),
            0,
            &mut from,
            &mut flen,
        )
    };
    assert!(n > 0, "wire tap recvfrom errno={}", sys::errno());
    Some((buf[..n as usize].to_vec(), (from.sin_addr, u16::from_be(from.sin_port))))
}

fn wire_silent(fd: i32) -> bool {
    let (ret, _e, rev) = sys::poll1(fd, sys::POLLIN, 0);
    ret == 0 || (rev & sys::POLLIN) == 0
}

/// Send a datagram INTO a node's WG socket from an arbitrary source socket
/// (unknown-peer injection).
fn wire_send_from(fd: i32, to: ([u8; 4], u16), data: &[u8]) {
    let sa = sys::sockaddr_in::new(to.0, to.1);
    let n = unsafe {
        sys::sendto(
            fd,
            data.as_ptr() as *const core::ffi::c_void,
            data.len(),
            0,
            &sa,
            core::mem::size_of::<sys::sockaddr_in>() as u32,
        )
    };
    assert_eq!(n as usize, data.len(), "wire send");
}

// ---------------------------------------------------------------------------
// node harness
// ---------------------------------------------------------------------------

/// One instance: a real `WgDevice` over a loopback UDP socket (its "cable
/// end") and a datagram socketpair playing the TUN half. Device access goes
/// through the SAME `WgDeviceApplier` seam the connector uses
/// (`apply_peers`/`apply_endpoint`/`tunnel_ready` + `with_device` for the
/// pump), so every test exercises the production seam surface.
struct Node {
    app: Arc<WgDeviceApplier>,
    key: String,
    /// raw WG socket the device adopted (kept for the wire tap; rebindable
    /// by endpoint-change tests via `reattach`).
    wg_raw: i32,
    tun_test_end: i32,
    /// raw fds owned by the test (closed on drop; the device closes only its
    /// dups — fd contract).
    raws: Mutex<Vec<i32>>,
}

impl Node {
    /// Fresh instance with its own bound socket + TUN socketpair, registering
    /// `peers` (key, allowed_ips) WITHOUT endpoints.
    fn build(
        secret: &[u8; 32],
        _addr: [u8; 4],
        key: &str,
        peers: Vec<(&str, Vec<([u8; 4], u8)>)>,
    ) -> Node {
        let (wg_raw, _port) = udp_socket_lo();
        let mut cfg = test_config(secret);
        cfg.keepalive_ms = 25_000;
        cfg.session_max_ms = 30_000;
        Node::adopt(wg_raw, _addr, key, peers, cfg)
    }

    /// Variant with explicit device-clock windows (keepalive/expiry test).
    fn build_timed(
        secret: &[u8; 32],
        _addr: [u8; 4],
        key: &str,
        peers: Vec<(&str, Vec<([u8; 4], u8)>)>,
        keepalive_ms: u64,
        session_max_ms: u64,
    ) -> Node {
        let (wg_raw, _port) = udp_socket_lo();
        let mut cfg = test_config(secret);
        cfg.keepalive_ms = keepalive_ms;
        cfg.session_max_ms = session_max_ms;
        Node::adopt(wg_raw, _addr, key, peers, cfg)
    }

    /// Instance over an ALREADY-EXISTING raw socket (ICE case: the socket
    /// the orchestrator gathers its first candidate from).
    fn adopt(
        wg_raw: i32,
        _addr: [u8; 4],
        key: &str,
        peers: Vec<(&str, Vec<([u8; 4], u8)>)>,
        cfg: WgDeviceConfig,
    ) -> Node {
        let (tun_raw, test_end) = tun_socketpair();
        let tun = TunFd::dup_from_raw(tun_raw).expect("tun dup");
        let dev = WgDevice::adopt(cfg, wg_raw, tun).expect("device adopt");
        let entries: Vec<WgPeerEntry> = peers
            .into_iter()
            .map(|(k, ips)| WgPeerEntry {
                pub_key_b64: k.to_string(),
                allowed_ips: ips
                    .iter()
                    .map(|&(a, p)| config::Route { addr: a, prefix_len: p })
                    .collect(),
            })
            .collect();
        let app = Arc::new(WgDeviceApplier::new(dev));
        app.apply_peers(&entries).expect("apply_peers");
        show_panics();
        Node {
            app,
            key: key.to_string(),
            wg_raw,
            tun_test_end: test_end,
            raws: Mutex::new(vec![wg_raw, tun_raw, test_end]),
        }
    }

    fn port(&self) -> u16 {
        self.dev(|d| d.local_addr()).1
    }

    fn stats(&self) -> WgDeviceStats {
        self.dev(|d| d.stats())
    }

    fn dev<R>(&self, f: impl FnOnce(&mut WgDevice) -> R) -> R {
        self.app.with_device(f)
    }

    fn endpoint_of(&self, peer: &str) -> Option<([u8; 4], u16)> {
        self.dev(|d| {
            d.peers().into_iter().find(|p| p.pub_key_b64 == peer).and_then(|p| p.endpoint)
        })
    }

    fn ready(&self) -> bool {
        self.app.tunnel_ready()
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        let raws =
            self.raws.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect::<Vec<_>>();
        for fd in raws {
            unsafe { sys::close(fd) };
        }
    }
}

/// boringtun's ffi installs a process-global panic→SIGSEGV hook the first
/// time `new_tunnel` runs (boringtun-0.7.1 ffi/mod.rs PANIC_HOOK), which
/// would swallow real test failure messages as silent signal-11 deaths.
/// Re-install a PRINTING hook after the first tunnel exists so assertion
/// failures stay diagnosable. (The device layer must still not panic —
/// panic=abort ships in the cdylib.)
fn show_panics() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        std::panic::set_hook(Box::new(|info| {
            eprintln!("\nTEST PANIC: {info}");
            eprintln!("{:?}", std::backtrace::Backtrace::force_capture());
        }));
    });
}

/// Test device-clock policy: quick handshake retries (100 ms), bounded
/// campaign (10 s), generous session window (30 s) — all injected-clock
/// values, keepalive effectively off unless the test opts in.
fn test_config(secret: &[u8; 32]) -> WgDeviceConfig {    let mut cfg = WgDeviceConfig::new(b64(secret));
    cfg.hs_retry_ms = 100;
    cfg.hs_deadline_ms = 10_000;
    cfg.keepalive_ms = u64::MAX / 2; // never due unless the test shrinks it
    cfg.session_max_ms = u64::MAX / 2; // never expires unless shrunk
    cfg
}

/// One pump step: TUN half + UDP half + tick, all on the injected clock,
/// poll timeout 0 (loopback delivery is synchronous within the process).
fn step(node: &Node, now: u64) {
    node.dev(|d| {
        d.service_tun(now);
        d.service_udp(now);
        d.tick(now);
    });
}

/// Pump both nodes until `cond` holds or the (simulated) deadline passes.
fn pump_until(
    a: &Node,
    b: &Node,
    now: &mut u64,
    deadline_ms: u64,
    cond: impl Fn(&Node, &Node) -> bool,
) -> bool {
    while *now <= deadline_ms {
        step(a, *now);
        step(b, *now);
        if cond(a, b) {
            return true;
        }
        *now += 10;
    }
    false
}

/// Land endpoints both ways and pump until both sessions are established.
fn establish(a: &Node, b: &Node, now: &mut u64) {
    let (pa, pb) = (a.port(), b.port());
    a.dev(|d| d.set_endpoint(b.key.as_str(), [127, 0, 0, 1], pb, *now)).expect("endpoint A→B");
    b.dev(|d| d.set_endpoint(a.key.as_str(), [127, 0, 0, 1], pa, *now)).expect("endpoint B→A");
    assert!(
        pump_until(a, b, now, *now + 10_000, |a, b| a.ready() && b.ready()),
        "handshake must establish within the sim deadline"
    );
}

/// Push a frame through `a`'s TUN half and pump until it falls out of `b`'s
/// TUN half; returns the received frame. (The read IS the consumption, so
/// the frame is captured inside the cond.)
fn carry_frame(a: &Node, b: &Node, now: &mut u64, frame: &[u8]) -> Vec<u8> {
    write_tun(a, frame);
    let got = std::cell::RefCell::new(None);
    let ok = pump_until(a, b, now, *now + 5_000, |_a, b| {
        if got.borrow().is_none() {
            *got.borrow_mut() = read_tun(b);
        }
        got.borrow().is_some()
    });
    assert!(ok, "frame must cross the tunnel within the sim deadline");
    got.into_inner().expect("frame")
}

// ---------------------------------------------------------------------------
// 1. THE core: bidirectional payload end-to-end
// ---------------------------------------------------------------------------

#[test]
fn dual_instance_bidirectional_payload_end_to_end() {
    let a = Node::build(&SECRET_A, ADDR_A, key_a(), vec![(key_b(), vec![(ADDR_B, 32)])]);
    let b = Node::build(&SECRET_B, ADDR_B, key_b(), vec![(key_a(), vec![(ADDR_A, 32)])]);
    let mut now = 1000u64;

    // before any endpoint: no readiness on either side
    assert!(!a.ready());
    assert!(!b.ready());

    establish(&a, &b, &mut now);
    assert!(a.ready(), "A must be ready after the handshake");
    assert!(b.ready(), "B must be ready after the handshake");
    assert_eq!(a.stats().handshake_initiations, 1, "A initiated exactly once");
    assert_eq!(b.stats().handshake_initiations, 1, "B initiated exactly once (glare)");

    // A's TUN half -> B's TUN half: the same bytes, nothing else.
    let pkt_ab = ip_udp_packet(ADDR_A, ADDR_B, 40001, 5353, b"n6-a2b-payload");
    let rx = carry_frame(&a, &b, &mut now, &pkt_ab);
    assert_eq!(rx, pkt_ab, "A→B payload must survive the tunnel byte-exact");

    // reverse direction
    let pkt_ba = ip_udp_packet(ADDR_B, ADDR_A, 5353, 40001, b"n6-b2a-payload");
    let rx2 = carry_frame(&b, &a, &mut now, &pkt_ba);
    assert_eq!(rx2, pkt_ba, "B→A payload must survive the tunnel byte-exact");

    // both devices saw real data-plane traffic
    assert!(a.stats().tx_packets >= 1 && a.stats().rx_packets >= 1);
    assert_eq!(a.stats().rx_bytes_to_tun, pkt_ba.len() as u64);
    assert_eq!(b.stats().rx_bytes_to_tun, pkt_ab.len() as u64);
}

// ---------------------------------------------------------------------------
// 2. the cable carries CIPHERTEXT, not the plaintext
// ---------------------------------------------------------------------------

#[test]
fn wire_bytes_are_ciphertext_not_plaintext() {
    let a = Node::build(&SECRET_A, ADDR_A, key_a(), vec![(key_b(), vec![(ADDR_B, 32)])]);
    let b = Node::build(&SECRET_B, ADDR_B, key_b(), vec![(key_a(), vec![(ADDR_A, 32)])]);
    let mut now = 2000u64;
    establish(&a, &b, &mut now);

    // Wire mode: the TEST taps every datagram off the cable and feeds the
    // receiving device explicitly, so the raw bytes are observable.
    let pkt = ip_udp_packet(ADDR_A, ADDR_B, 41000, 443, b"ciphertext-check!!");
    write_tun(&a, &pkt);
    assert_eq!(a.dev(|d| d.service_tun(now)), 1, "one frame encapsulated");
    let (wire1, src) = wire_recv(&b).expect("ciphertext datagram on the cable");
    assert_eq!(src, ([127, 0, 0, 1], a.port()), "datagram must come from A's WG socket");

    // NOT a pass-through: different bytes, exactly the WG transport overhead
    // (4 type + 4 receiver index + 8 counter + 16 poly1305 tag = +32), and
    // the plaintext payload appears NOWHERE in the datagram.
    assert_ne!(wire1, pkt, "the cable must not carry the plaintext");
    assert_eq!(wire1.len(), pkt.len() + 32, "WG transport data overhead is 32 bytes");
    let payload = &pkt[28..];
    assert!(
        !wire1.windows(payload.len()).any(|w| w == payload),
        "payload must not be visible in the ciphertext"
    );

    // real decryption on B
    let inbound = b.dev(|d| d.handle_udp(&wire1, src, now));
    assert!(inbound.peer_matched && inbound.wrote_tun == pkt.len());
    assert_eq!(read_tun(&b).expect("decrypted frame"), pkt);

    // same plaintext AGAIN -> different ciphertext (fresh nonce per packet),
    // still an exact WG transport packet, still decrypts to the same bytes
    write_tun(&a, &pkt);
    assert_eq!(a.dev(|d| d.service_tun(now)), 1);
    let (wire2, src2) = wire_recv(&b).expect("second ciphertext datagram");
    assert_ne!(wire2, wire1, "same plaintext must NOT repeat the same ciphertext");
    assert_eq!(src2, src);
    let _ = b.dev(|d| d.handle_udp(&wire2, src2, now));
    assert_eq!(read_tun(&b).expect("second decrypted frame"), pkt);
}

// ---------------------------------------------------------------------------
// 3. allowed_ips longest-prefix routing + drop accounting
// ---------------------------------------------------------------------------

#[test]
fn allowed_ips_longest_prefix_routes_and_unmatched_drops() {
    // A routes: B gets 10.99.0.2/32, C gets 10.99.0.0/16 (overlapping on
    // purpose — LPM must pick the /32 for .2).
    let a = Node::build(
        &SECRET_A,
        ADDR_A,
        key_a(),
        vec![(key_b(), vec![([10, 99, 0, 2], 32)]), (key_c(), vec![([10, 99, 0, 0], 16)])],
    );
    let b = Node::build(&SECRET_B, ADDR_B, key_b(), vec![(key_a(), vec![(ADDR_A, 32)])]);
    let c = Node::build(&SECRET_C, [10, 99, 0, 3], key_c(), vec![(key_a(), vec![(ADDR_A, 32)])]);
    let mut now = 3000u64;

    establish(&a, &b, &mut now);
    establish(&a, &c, &mut now);
    let b_rx0 = b.stats().rx_packets;
    let c_rx0 = c.stats().rx_packets;

    // 10.99.0.2 matches BOTH the /32 (B) and the /16 (C): the longer prefix
    // must win — B receives, C sees nothing.
    let pkt = ip_udp_packet(ADDR_A, [10, 99, 0, 2], 42000, 8080, b"to-the-/32");
    assert_eq!(carry_frame(&a, &b, &mut now, &pkt), pkt, "/32 peer must receive");
    assert_eq!(c.stats().rx_packets, c_rx0, "/16 peer must NOT receive the /32 packet");

    // 10.99.7.7 is inside the /16 only: C receives, B only has packet one.
    let pkt2 = ip_udp_packet(ADDR_A, [10, 99, 7, 7], 42001, 8080, b"to-the-/16");
    assert_eq!(carry_frame(&a, &c, &mut now, &pkt2), pkt2, "/16 peer must receive");
    assert_eq!(b.stats().rx_packets, b_rx0 + 1, "B must only have the first packet");

    // unmatched destination: dropped + counted, nothing leaves the device.
    let drops0 = a.stats().no_route_drops;
    let tx0 = a.stats().tx_packets;
    let pkt3 = ip_udp_packet(ADDR_A, [10, 200, 0, 1], 42002, 53, b"no-route");
    write_tun(&a, &pkt3);
    assert_eq!(a.dev(|d| d.service_tun(now)), 1, "the frame was read");
    assert_eq!(a.stats().no_route_drops, drops0 + 1, "no-route drop must count");
    assert_eq!(a.stats().tx_packets, tx0, "nothing may be sent for an unrouted frame");
    assert!(read_tun(&b).is_none() && read_tun(&c).is_none());

    // non-IPv4 / short frames: dropped + counted, no panic.
    let short0 = a.stats().short_frame_drops;
    write_tun(&a, &[0u8; 10]);
    write_tun(&a, &[0x60, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // IPv6
    assert_eq!(a.dev(|d| d.service_tun(now)), 2);
    assert_eq!(a.stats().short_frame_drops, short0 + 2);
}

// ---------------------------------------------------------------------------
// 4. tunnel_ready ↔ N3-7 default-route gate, through the REAL seam
// ---------------------------------------------------------------------------

fn gated_map(peers: Vec<PeerInfo>) -> NetworkMap {
    NetworkMap {
        serial: 7,
        peer: None,
        peers,
        peers_is_empty: false,
        offline_peers: vec![],
        routes: vec![ManagedRoute {
            id: "r-default".into(),
            network: config::Route { addr: [0, 0, 0, 0], prefix_len: 0 },
            domains: vec![],
            net_id: "net-d".into(),
            network_type: 1,
            peer: "relay".into(),
            metric: 9999,
            masquerade: false,
            keep_route: false,
            skip_auto_apply: false,
        }],
        skipped_routes: vec![],
        dns: None,
    }
}

fn one_peer_map() -> NetworkMap {
    gated_map(vec![PeerInfo {
        wg_pub_key: key_b().to_string(),
        allowed_ips: vec![config::Route { addr: [10, 99, 0, 2], prefix_len: 32 }],
        fqdn: None,
    }])
}

#[test]
fn tunnel_ready_drives_the_default_route_gate_through_the_real_seam() {
    let a = Node::build(&SECRET_A, ADDR_A, key_a(), vec![(key_b(), vec![(ADDR_B, 32)])]);
    let b = Node::build(&SECRET_B, ADDR_B, key_b(), vec![(key_a(), vec![(ADDR_A, 32)])]);
    let app = a.app.clone();
    let mut now = 4000u64;

    // BEFORE the handshake: real device, real peers, no session — gate HOLDS
    assert!(!app.tunnel_ready(), "no handshake, no readiness");
    let held = ShellNetworkConfig::from_map_gated(&one_peer_map(), false, app.tunnel_ready());
    assert!(!held.default_route_allowed);
    assert_eq!(held.default_route_reason, "default-route-held:data-plane-not-ready");

    // peer_count == 0 keeps the gate held EVEN IF some readiness is claimed
    let empty = ShellNetworkConfig::from_map_gated(&gated_map(vec![]), false, true);
    assert!(!empty.default_route_allowed);
    assert_eq!(empty.default_route_reason, "default-route-held:no-usable-peer");

    // dev opt-in override is unchanged (loud, black-hole token)
    let forced = ShellNetworkConfig::from_map_gated(&one_peer_map(), true, false);
    assert!(forced.default_route_allowed);
    assert_eq!(forced.default_route_reason, "default-route-forced:debug-opt-in-black-hole-risk");

    // REAL handshake through the seam's apply_endpoint (the same call the
    // ICE orchestrator makes on pair selection)
    establish(&a, &b, &mut now);
    assert!(app.tunnel_ready(), "established session must arm tunnel_ready");
    let allowed = ShellNetworkConfig::from_map_gated(&one_peer_map(), false, app.tunnel_ready());
    assert!(allowed.default_route_allowed, "gate opens on a REAL data plane");
    assert_eq!(
        allowed.default_route_reason,
        "default-route-allowed:peers-registered-and-tunnel-ready"
    );
    // 0.0.0.0/0 is actually exported now (it was stripped while held)
    assert!(allowed.routes.iter().any(|r| r.is_default));

    // teardown: clear() → no sessions → gate closes again (fail-closed)
    app.clear();
    assert!(!app.tunnel_ready());
    let held2 = ShellNetworkConfig::from_map_gated(&one_peer_map(), false, app.tunnel_ready());
    assert!(!held2.default_route_allowed);
    assert_eq!(held2.default_route_reason, "default-route-held:data-plane-not-ready");
}

// ---------------------------------------------------------------------------
// 5. handshake retransmission after a dropped initiation
// ---------------------------------------------------------------------------

#[test]
fn handshake_retransmission_recovers_a_dropped_initiation() {
    let a = Node::build(&SECRET_A, ADDR_A, key_a(), vec![(key_b(), vec![(ADDR_B, 32)])]);
    let b = Node::build(&SECRET_B, ADDR_B, key_b(), vec![(key_a(), vec![(ADDR_A, 32)])]);
    let mut now = 5000u64;

    let (pa, pb) = (a.port(), b.port());
    a.dev(|d| d.set_endpoint(key_b(), [127, 0, 0, 1], pb, now)).unwrap();
    b.dev(|d| d.set_endpoint(key_a(), [127, 0, 0, 1], pa, now)).unwrap();

    // A's first initiation flies — and is LOST on the cable (tapped, kept
    // for the fresh-ephemeral check below, never delivered)
    assert_eq!(a.stats().handshake_initiations, 1);
    let (first, _) = wire_recv(&b).expect("first initiation on the wire");
    assert_eq!(first.len(), 148, "WG handshake initiation is a 148-byte packet");
    // THE DROP: `first` is never handed to B.
    assert!(!b.ready(), "B saw nothing");
    assert!(wire_silent(b.wg_raw), "the dropped datagram was the only one");

    // device-clock retry: past hs_retry_ms (100 in the test config) the
    // campaign re-initiates — fresh ephemeral, same 148-byte shape
    now += 250;
    assert_eq!(a.dev(|d| d.tick(now)), 1, "retransmission due");
    assert_eq!(a.stats().handshake_initiations, 2, "exactly one retransmission");
    let (init2, src2) = wire_recv(&b).expect("retransmitted initiation");
    assert_eq!(init2.len(), 148);
    assert_ne!(init2, first, "a fresh ephemeral must change the initiation bytes");
    assert_eq!(src2, ([127, 0, 0, 1], pa));

    // deliver it manually (wire mode) and finish the handshake. NOTE: this
    // is a real GLARE situation (both sides configured an endpoint, so both
    // initiated); WG resolves it with the tie-breaker — whichever init wins,
    // the winner's peer accepts it as responder and the loser accepts the
    // winner's response, so draining BOTH cables converges deterministically.
    b.dev(|d| d.handle_udp(&init2, src2, now));
    let dl = now + 10_000;
    let established = loop {
        while let Some((dg, src)) = wire_recv(&a) {
            a.dev(|d| d.handle_udp(&dg, src, now));
        }
        while let Some((dg, src)) = wire_recv(&b) {
            b.dev(|d| d.handle_udp(&dg, src, now));
        }
        if a.ready() && b.ready() {
            break true;
        }
        now += 100;
        if now > dl {
            break false;
        }
        a.dev(|d| d.tick(now));
        b.dev(|d| d.tick(now));
    };
    assert!(established, "the recovered handshake must establish");

    // the recovered session carries payload
    let pkt = ip_udp_packet(ADDR_A, ADDR_B, 43000, 80, b"after-retransmit");
    write_tun(&a, &pkt);
    a.dev(|d| d.service_tun(now));
    let (ct, csrc) = wire_recv(&b).expect("payload on the wire");
    assert_ne!(ct, pkt, "payload crosses encrypted");
    let inbound = b.dev(|d| d.handle_udp(&ct, csrc, now));
    assert_eq!(inbound.wrote_tun, pkt.len());
    assert_eq!(read_tun(&b).expect("decrypted"), pkt);
}

// ---------------------------------------------------------------------------
// 6. endpoint change (ICE re-selection shape)
// ---------------------------------------------------------------------------

#[test]
fn endpoint_change_switches_the_path_and_keeps_traffic_flowing() {
    let a = Node::build(&SECRET_A, ADDR_A, key_a(), vec![(key_b(), vec![(ADDR_B, 32)])]);
    let mut b = Node::build(&SECRET_B, ADDR_B, key_b(), vec![(key_a(), vec![(ADDR_A, 32)])]);
    let mut now = 6000u64;
    establish(&a, &b, &mut now);
    let old_raw = b.wg_raw;
    let old_port = b.port();

    // sanity: traffic flows on path 1
    let pkt0 = ip_udp_packet(ADDR_A, ADDR_B, 44000, 80, b"path-one");
    assert_eq!(carry_frame(&a, &b, &mut now, &pkt0), pkt0);

    // "ICE re-selection" on B: its local WG socket is regenerated (a new
    // bound socket is adopted by the device; peers/tunnels/sessions survive)
    let (new_raw, new_port) = udp_socket_lo();
    assert_ne!(new_port, old_port, "the new socket must be a different path");
    b.dev(|d| d.reattach_socket(new_raw)).expect("reattach");
    b.raws.lock().unwrap_or_else(|e| e.into_inner()).push(new_raw);
    b.wg_raw = new_raw;

    // A's endpoint for B re-lands (apply_endpoint with the NEW selected pair)
    a.dev(|d| d.set_endpoint(key_b(), [127, 0, 0, 1], new_port, now + 1000))
        .expect("re-land endpoint");
    let dl = now + 10_000;
    assert!(
        pump_until(&a, &b, &mut now, dl, |a, b| a.ready() && b.ready()),
        "the rekey over the new path must establish"
    );

    // traffic follows the NEW path
    let pkt = ip_udp_packet(ADDR_A, ADDR_B, 44001, 80, b"path-two");
    assert_eq!(carry_frame(&a, &b, &mut now, &pkt), pkt, "payload must cross the new path");
    let back = ip_udp_packet(ADDR_B, ADDR_A, 80, 44001, b"path-two-back");
    assert_eq!(carry_frame(&b, &a, &mut now, &back), back, "reverse direction too");

    // the OLD socket stays silent after the switch
    assert!(wire_silent(old_raw), "the abandoned path must not receive anything");
    // and A's device now points at the new path only
    assert_eq!(a.endpoint_of(key_b()), Some(([127, 0, 0, 1], new_port)));
}

// ---------------------------------------------------------------------------
// 7. unknown sources and corrupted datagrams: dropped + counted, no panic
// ---------------------------------------------------------------------------

#[test]
fn unknown_sources_and_decrypt_failures_are_dropped_and_counted() {
    let a = Node::build(&SECRET_A, ADDR_A, key_a(), vec![(key_b(), vec![(ADDR_B, 32)])]);
    let b = Node::build(&SECRET_B, ADDR_B, key_b(), vec![(key_a(), vec![(ADDR_A, 32)])]);
    let mut now = 7000u64;
    establish(&a, &b, &mut now);

    // a stranger socket injects garbage into B's WG socket
    let (stranger, _sp) = udp_socket_lo();
    let unk0 = b.stats().unknown_peer_drops;
    for i in 0..3u8 {
        wire_send_from(stranger, ([127, 0, 0, 1], b.port()), &[i; 64]);
        assert_eq!(b.dev(|d| d.service_udp(now)), 1, "one datagram drained");
    }
    assert_eq!(b.stats().unknown_peer_drops, unk0 + 3, "unknown sources must count");
    unsafe { sys::close(stranger) };

    // a REAL datagram from A, corrupted on the wire: auth fails -> counted,
    // nothing reaches the TUN, and the tunnel keeps working
    let pkt = ip_udp_packet(ADDR_A, ADDR_B, 45000, 80, b"corrupt-me-not");
    write_tun(&a, &pkt);
    a.dev(|d| d.service_tun(now));
    let (mut ct, csrc) = wire_recv(&b).expect("ciphertext");
    let dec0 = b.stats().decrypt_errors;
    let last = ct.len() - 1;
    ct[last] ^= 0xff; // break the poly1305 tag
    let inbound = b.dev(|d| d.handle_udp(&ct, csrc, now));
    assert!(inbound.peer_matched && inbound.wrote_tun == 0);
    assert_eq!(b.stats().decrypt_errors, dec0 + 1, "decrypt failure must count");
    assert!(read_tun(&b).is_none(), "nothing may reach the TUN");

    // the session survives: the next honest packet flows
    let pkt2 = ip_udp_packet(ADDR_A, ADDR_B, 45001, 80, b"still-alive");
    assert_eq!(carry_frame(&a, &b, &mut now, &pkt2), pkt2);
}

// ---------------------------------------------------------------------------
// 8. injectable clock: keepalive cadence + session expiry
// ---------------------------------------------------------------------------

#[test]
fn injected_clock_drives_keepalive_and_session_expiry() {
    // 300 ms keepalive window, 1000 ms session window — all on the clock the
    // test advances; no wall-clock waiting anywhere.
    let a = Node::build_timed(
        &SECRET_A,
        ADDR_A,
        key_a(),
        vec![(key_b(), vec![(ADDR_B, 32)])],
        300,
        1000,
    );
    let b = Node::build(&SECRET_B, ADDR_B, key_b(), vec![(key_a(), vec![(ADDR_A, 32)])]);
    let mut now = 8000u64;
    establish(&a, &b, &mut now);

    // anchor last_outbound with one data packet
    let pkt = ip_udp_packet(ADDR_A, ADDR_B, 46000, 80, b"anchor");
    assert_eq!(carry_frame(&a, &b, &mut now, &pkt), pkt);
    let ka0 = a.stats().keepalives_sent;

    // data is fresh (100 ms < 300 ms window): no keepalive
    now += 100;
    a.dev(|d| d.tick(now));
    assert_eq!(a.stats().keepalives_sent, ka0, "fresh activity must suppress the keepalive");

    // past the window: exactly one empty-payload transport packet (32 bytes)
    now += 300;
    assert_eq!(a.dev(|d| d.tick(now)), 1, "keepalive due");
    assert_eq!(a.stats().keepalives_sent, ka0 + 1);
    let (ka, ksrc) = wire_recv(&b).expect("keepalive on the wire");
    assert_eq!(ksrc, ([127, 0, 0, 1], a.port()));
    assert_eq!(ka.len(), 32, "WG keepalive = transport header only (32 bytes)");
    let inb = b.dev(|d| d.handle_udp(&ka, ksrc, now));
    assert!(inb.peer_matched && inb.wrote_tun == 0, "keepalive never reaches the TUN");
    assert!(read_tun(&b).is_none());

    // session expiry on the injected clock: past session_max_ms (1000 ms)
    // with no NEW handshake the device stops claiming readiness — and the
    // SEAM reports the same state (default-route gate closes)
    assert!(a.ready());
    now += 5_000;
    a.dev(|d| d.tick(now));
    assert!(!a.ready(), "expired session must close the readiness claim");
    assert!(!a.app.tunnel_ready(), "the seam must report the expiry too");

    // B's own clock never expired ITS session — the expiry is per-device
    // state driven by its owner's clock (peer liveness is ICE's job).
    assert!(b.ready());
}

// ---------------------------------------------------------------------------
// 9. the ICE harness lands the REAL data plane (selected pair → device)
// ---------------------------------------------------------------------------

// signal-bus harness (same shape as tests/peer_conn_e2e.rs)
#[derive(Debug, Clone, PartialEq)]
struct Frame {
    from: String,
    to: String,
    kind: PeerSignalKind,
    payload: String,
}

#[derive(Default)]
struct SignalBus {
    queue: Mutex<Vec<Frame>>,
}

impl SignalBus {
    fn push(&self, f: Frame) {
        self.queue.lock().unwrap_or_else(|e| e.into_inner()).push(f);
    }
    fn drain(&self) -> Vec<Frame> {
        self.queue.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect()
    }
}

struct MockSignalEndpoint {
    me: String,
    bus: Arc<SignalBus>,
}

impl SignalExchange for MockSignalEndpoint {
    fn send(
        &self,
        to_key: &str,
        kind: PeerSignalKind,
        payload: &str,
        _port: u32,
    ) -> Result<(), netbird_core::management::ManagementError> {
        self.bus.push(Frame {
            from: self.me.clone(),
            to: to_key.to_string(),
            kind,
            payload: payload.to_string(),
        });
        Ok(())
    }
}

/// Protected-UDP provider holding the ORIGINAL candidate fds (ICE sessions
/// only touch dups); raws stay open so the WG device can adopt the selected
/// one, and are closed exactly once on drop.
struct FedSocks {
    source: Arc<ProtectedUdpFdSource>,
    raws: Vec<i32>,
}

impl FedSocks {
    fn new(n: usize) -> Self {
        let mut raws = Vec::with_capacity(n);
        for _ in 0..n {
            let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            assert!(fd >= 0);
            raws.push(fd);
        }
        let source = Arc::new(ProtectedUdpFdSource::new_with_fd(-1));
        for &fd in &raws {
            source.feed(fd);
        }
        FedSocks { source, raws }
    }
}

impl Drop for FedSocks {
    fn drop(&mut self) {
        for fd in self.raws.drain(..) {
            unsafe { sys::close(fd) };
        }
    }
}

#[test]
fn ice_selected_pair_lands_on_the_real_device_and_carries_payload() {
    let bus = Arc::new(SignalBus::default());
    // The device seam stamps REAL mono ms (apply_endpoint), so the device
    // phase runs on the real clock; the ICE phase accepts any monotonic
    // baseline (its timers only see deltas).
    let mut now = sys::mono_ms();

    // devices FIRST (the applier is the orchestrators' WG seam), each over
    // raw provider socket #0 as a PROVISIONAL socket — after the pair is
    // selected the device reattaches to the actual selected candidate socket.
    // OWNERSHIP: raws[0] MOVES into the adopted Node (Node::drop closes it),
    // so it is removed from the FedSocks list — FedSocks::drop must never
    // close it a second time. A double close does not stay harmless: the fd
    // number is recycled within microseconds under parallel tests, and the
    // second close lands on whoever owns that number by then — observed as
    // errno=9 (EBADF) on another test's brand-new socket (e.g. the stranger
    // socket of unknown_sources_…) under suite pressure.
    let mut fed_a = FedSocks::new(8);
    let mut fed_b = FedSocks::new(8);
    let prov_a = fed_a.raws.remove(0);
    let a = Node::adopt(
        prov_a,
        ADDR_A,
        key_a(),
        vec![(key_b(), vec![(ADDR_B, 32)])],
        {
            let mut c = test_config(&SECRET_A);
            c.hs_retry_ms = 100; // real ms: quick recovery if an ICE session
            c // consumed an initiation during the ICE phase
        },
    );
    let prov_b = fed_b.raws.remove(0);
    let b = Node::adopt(
        prov_b,
        ADDR_B,
        key_b(),
        vec![(key_a(), vec![(ADDR_A, 32)])],
        {
            let mut c = test_config(&SECRET_B);
            c.hs_retry_ms = 100;
            c
        },
    );
    assert!(!a.ready() && !b.ready(), "no endpoints, no readiness");

    // two ICE orchestrators whose WG seam IS the device applier
    let mut orch_a = PeerIceOrchestrator::new(PeerIceDeps {
        ifaces: Arc::new(StaticInterfaces(vec![InterfaceAddr {
            name: "eth0".into(),
            addr: [127, 0, 0, 1],
        }])),
        socks: fed_a.source.clone(),
        signal: Arc::new(MockSignalEndpoint { me: key_a().to_string(), bus: bus.clone() }),
        wg: a.app.clone(),
        tie_breaker: Some(0x1111),
        fixed_local_port: None,
        advertised_candidates: Vec::new(),
    });
    let mut orch_b = PeerIceOrchestrator::new(PeerIceDeps {
        ifaces: Arc::new(StaticInterfaces(vec![InterfaceAddr {
            name: "eth0".into(),
            addr: [127, 0, 0, 1],
        }])),
        socks: fed_b.source.clone(),
        signal: Arc::new(MockSignalEndpoint { me: key_b().to_string(), bus: bus.clone() }),
        wg: b.app.clone(),
        tie_breaker: Some(0x2222),
        fixed_local_port: None,
        advertised_candidates: Vec::new(),
    });
    orch_a.set_peers(&[key_b().to_string()]);
    orch_b.set_peers(&[key_a().to_string()]);
    orch_a.set_signal_ready(true);
    orch_b.set_signal_ready(true);
    orch_b.set_initiator(key_a(), false); // A offers, B answers

    // ICE phase: mock-signal exchange + injected clock, until both Connected
    let start = now;
    let converged = loop {
        if now > start + 30_000 {
            break false;
        }
        let _ = orch_a.run_once(now);
        let _ = orch_b.run_once(now);
        now += 10;
        for f in bus.drain() {
            if f.to == key_a() {
                orch_a.handle_signal(&f.from, f.kind, &f.payload, now).expect("A recv");
            } else if f.to == key_b() {
                orch_b.handle_signal(&f.from, f.kind, &f.payload, now).expect("B recv");
            }
        }
        let sa = orch_a.peer_status(key_b()).expect("peer entry");
        let sb = orch_b.peer_status(key_a()).expect("peer entry");
        if sa.state == PeerIceState::Connected && sb.state == PeerIceState::Connected {
            break true;
        }
    };
    assert!(converged, "ICE must converge within the sim deadline");

    // The SELECTED PAIRS landed on the devices via apply_endpoint. Now find,
    // per side, the raw candidate socket whose port is the endpoint the PEER
    // landed — i.e. OUR selected local end — and reattach the device to it:
    // from here the WG socket IS the selected candidate socket. (The gather
    // phase consumes one provider socket per round before the candidate
    // sockets are bound, so the selected socket's index is not a priori; we
    // locate it by port instead of assuming one.)
    let a_sel = b.endpoint_of(key_a()).expect("B landed A's selected pair").1;
    let b_sel = a.endpoint_of(key_b()).expect("A landed B's selected pair").1;
    let a_raw = *fed_a.raws.iter().find(|&&fd| sock_port(fd) == a_sel)
        .expect("A's selected candidate socket must be one of the fed raws");
    let b_raw = *fed_b.raws.iter().find(|&&fd| sock_port(fd) == b_sel)
        .expect("B's selected candidate socket must be one of the fed raws");
    assert_eq!(sock_port(a_raw), a_sel);
    assert_eq!(sock_port(b_raw), b_sel);
    a.dev(|d| d.reattach_socket(a_raw)).expect("reattach A to the selected socket");
    b.dev(|d| d.reattach_socket(b_raw)).expect("reattach B to the selected socket");
    assert_eq!(a.endpoint_of(key_b()), Some(([127, 0, 0, 1], b_sel)));
    assert_eq!(b.endpoint_of(key_a()), Some(([127, 0, 0, 1], a_sel)));

    // stop the ICE half: sessions close their dups, the selected sockets
    // belong to the WG devices alone from here on
    orch_a.stop_all();
    orch_b.stop_all();
    orch_a.set_signal_ready(false);
    orch_b.set_signal_ready(false);

    // WG phase: the devices (endpoint-armed by apply_endpoint during ICE)
    // complete the handshake over the selected sockets — any initiation an
    // ICE session consumed is recovered by the device-clock retry.
    let deadline = now + 30_000;
    let ready = loop {
        a.dev(|d| {
            d.service_udp(now);
            d.tick(now);
        });
        b.dev(|d| {
            d.service_udp(now);
            d.tick(now);
        });
        if a.ready() && b.ready() {
            break true;
        }
        now += 50;
        if now > deadline {
            break false;
        }
    };
    assert!(ready, "the device pair must establish over the ICE-selected sockets");

    // payload across the ICE-landed data plane, both directions
    let pkt = ip_udp_packet(ADDR_A, ADDR_B, 47000, 80, b"ice-landed-dataplane");
    let got = drive_frame(&a, &b, &mut now, &pkt);
    assert_eq!(got, pkt, "A→B payload over the ICE-selected path");
    let back = ip_udp_packet(ADDR_B, ADDR_A, 80, 47000, b"ice-landed-back");
    let got2 = drive_frame(&b, &a, &mut now, &back);
    assert_eq!(got2, back, "B→A payload over the ICE-selected path");

    // the N3-7 gate sees a genuinely ready data plane at the end of the
    // signal → ICE → WG chain
    assert!(a.ready());
    let allowed = ShellNetworkConfig::from_map_gated(&one_peer_map(), false, a.ready());
    assert!(allowed.default_route_allowed);
    assert_eq!(
        allowed.default_route_reason,
        "default-route-allowed:peers-registered-and-tunnel-ready"
    );
}

/// TUN write + bounded real-clock drive until the frame falls out of `to`'s
/// TUN half (the ICE test's pump shape; no sleeps, poll timeout 0).
fn drive_frame(from: &Node, to: &Node, now: &mut u64, frame: &[u8]) -> Vec<u8> {
    write_tun(from, frame);
    let deadline = *now + 10_000;
    loop {
        from.dev(|d| d.service_tun(*now));
        to.dev(|d| {
            d.service_udp(*now);
            d.tick(*now);
        });
        if let Some(f) = read_tun(to) {
            return f;
        }
        *now += 20;
        assert!(*now <= deadline, "frame must cross within the real-clock bound");
    }
}

// ---------------------------------------------------------------------------
// 10. the seam reconciles the peer set with network-map snapshots
// ---------------------------------------------------------------------------

#[test]
fn applier_peer_set_follows_network_map_snapshots() {
    let a = Node::build(&SECRET_A, ADDR_A, key_a(), vec![]);
    let app = a.app.clone();

    // snapshot 1: peer B
    app.apply_peers(&[entry(key_b(), &[(ADDR_B, 32)])]).expect("snapshot 1");
    assert_eq!(a.dev(|d| d.peer_count()), 1);

    // snapshot 2: B + C
    app.apply_peers(&[
        entry(key_b(), &[(ADDR_B, 32)]),
        entry(key_c(), &[([10, 99, 0, 0], 16)]),
    ])
    .expect("snapshot 2");
    assert_eq!(a.dev(|d| d.peer_count()), 2);

    // snapshot 3: C only — B is torn down; readiness (which requires an
    // established session on a registered peer) cannot survive removal
    app.apply_peers(&[entry(key_c(), &[([10, 99, 0, 0], 16)])]).expect("snapshot 3");
    assert_eq!(a.dev(|d| d.peer_count()), 1);
    assert!(!app.tunnel_ready());
    let peers = a.dev(|d| d.peers());
    assert_eq!(peers[0].pub_key_b64, key_c());
    assert_eq!(peers[0].endpoint, None);

    // allowed_ips update on an EXISTING peer keeps its identity (and would
    // keep an established session — only the routing table changes)
    app.apply_peers(&[entry(key_c(), &[([10, 99, 8, 0], 24)])]).expect("update");
    let peers = a.dev(|d| d.peers());
    assert_eq!(peers[0].allowed_ips, vec![([10, 99, 8, 0], 24)]);

    // fail-closed: endpoint for an UNREGISTERED peer is a config bug
    assert!(app.apply_endpoint(key_b(), [127, 0, 0, 1], 1).is_err());

    // invalid spec (bad prefix) is rejected BEFORE any mutation
    assert!(app
        .apply_peers(&[WgPeerEntry {
            pub_key_b64: key_b().to_string(),
            allowed_ips: vec![config::Route { addr: [10, 0, 0, 0], prefix_len: 40 }],
        }])
        .is_err());
    assert_eq!(a.dev(|d| d.peer_count()), 1, "failed snapshot must not mutate");

    // invalid key material is rejected too
    assert!(app
        .apply_peers(&[WgPeerEntry {
            pub_key_b64: "not-base64!!".to_string(),
            allowed_ips: vec![],
        }])
        .is_err());
}

fn entry(key: &str, ips: &[([u8; 4], u8)]) -> WgPeerEntry {
    WgPeerEntry {
        pub_key_b64: key.to_string(),
        allowed_ips: ips
            .iter()
            .map(|&(a, p)| config::Route { addr: a, prefix_len: p })
            .collect(),
    }
}
