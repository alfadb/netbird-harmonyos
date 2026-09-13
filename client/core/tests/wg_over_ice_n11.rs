// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N11 end-to-end tests: WG RIDES the ICE-selected socket — the full
//! protocol stack twice over in one process (real loopback UDP through the
//! protected-fd seams, REAL `WgDeviceFeed` devices, mock signal bus,
//! injected clock — no sleeps, no wall clock).
//!
//! Proven here (the N11 acceptance core):
//! - **closed loop**: both instances converge ICE → the WG handshakes cross
//!   the SELECTED sockets (each device's egress = its selected local
//!   candidate; each side's landed endpoint = the other's selected socket),
//!   sessions establish, and probe payloads flow BOTH directions end-to-end
//!   (TUN stand-in → device → selected socket → peer demux → peer TUN);
//! - **demux**: ICE keepalives (STUN) and WG transport packets coexist on
//!   the same selected socket — keepalives keep being answered past the
//!   6 s disconnected threshold (both stay Connected at +8 s of continued
//!   pumping), and the devices show ZERO `decrypt_errors` (no STUN ever
//!   reached WG) and ZERO `unknown_peer_drops` (nothing arrived off-path);
//! - **anti-example (the round-7 bug shape)**: a WG device that sends its
//!   handshake from a NON-selected socket to the peer's ICE socket never
//!   establishes a session — the handshakes leave (counted), land on the ICE
//!   socket, are correctly NOT eaten as STUN (`is_wg_datagram`), but cannot
//!   be matched (rx stays 0, peers_with_session stays 0);
//! - **migration/failure**: killing the selected path → Disconnected at +6 s
//!   (endpoint + egress recycled — nothing can leave the device: fail-
//!   closed) → Failed at +12 s → session torn down, renegotiation armed →
//!   the peers re-converge over FRESH sockets (egress re-attached, sessions
//!   re-established, probes flow again) — and the N3-7 default-route gate
//!   holds 0.0.0.0/0 through the whole failure window, allowing only after
//!   recovery.

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
        _value: *const c_void,
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
        _cbinfo: *const c_void,
        _argc: *mut usize,
        _argv: *mut *mut c_void,
        _data: *mut *mut c_void,
        _result: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_string_utf8(
        _env: *mut c_void,
        _value: *const c_void,
        _buf: *mut u8,
        _bufsize: usize,
        _result: *mut usize,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_int32(_env: *mut c_void, _value: *const c_void, _result: *mut i32) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(_env: *mut c_void, _value: *const c_void, _result: *mut bool) -> i32 {
        0
    }
}

use std::sync::{Arc, Mutex};

use base64::Engine as _;

use netbird_core::config;
use netbird_core::connector::{ShellNetworkConfig, WgPeerApplier, WgPeerEntry};
use netbird_core::ice::{Candidate, InterfaceAddr, ProtectedUdpFdSource, StaticInterfaces};
use netbird_core::ice_session::is_wg_datagram;
use netbird_core::peer_conn::{ice_ready_for_default_route, PeerIceDeps, PeerIceOrchestrator, PeerIceState, PeerSignalKind, SignalExchange};
use netbird_core::wg_device::{WgDeviceConfig, WgDeviceFeed};
use netbird_core::sys;

extern "C" {
    fn socket(domain: i32, ty: i32, protocol: i32) -> i32;
    fn socketpair(domain: i32, ty: i32, protocol: i32, sv: *mut [i32; 2]) -> i32;
    fn close(fd: i32) -> i32;
    fn write(fd: i32, buf: *const core::ffi::c_void, n: usize) -> isize;
    fn read(fd: i32, buf: *mut core::ffi::c_void, n: usize) -> isize;
    fn fcntl(fd: i32, cmd: i32, arg: i32) -> i32;
}

/// Every test-side fd is O_NONBLOCK: a read with nothing queued returns
/// -1/EAGAIN instead of blocking the injected-clock harness forever.
fn set_nonblock(fd: i32) {
    const F_GETFL: i32 = 3;
    const F_SETFL: i32 = 4;
    const O_NONBLOCK: i32 = 2048;
    let fl = unsafe { fcntl(fd, F_GETFL, 0) };
    assert!(fl >= 0, "F_GETFL");
    assert_eq!(unsafe { fcntl(fd, F_SETFL, fl | O_NONBLOCK) }, 0, "F_SETFL");
}

const VPN_A: [u8; 4] = [10, 77, 0, 1];
const VPN_B: [u8; 4] = [10, 77, 0, 2];

/// (secret_b64, public_b64) through the same frozen boringtun derivation the
/// devices use — the registered peer key IS the peer's public key.
fn key_pair(byte: u8) -> (String, String) {
    let secret = base64::engine::general_purpose::STANDARD.encode([byte; 32]);
    let public = netbird_core::wg_device::x25519_public_b64(&secret).expect("public key");
    (secret, public)
}

// ---------------------------------------------------------------------------
// harness
// ---------------------------------------------------------------------------

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
    seen: Mutex<Vec<Frame>>,
}

impl SignalBus {
    fn drain(&self) -> Vec<Frame> {
        self.queue.lock().expect("queue").drain(..).collect()
    }
    fn seen(&self) -> Vec<Frame> {
        self.seen.lock().expect("seen").clone()
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
        self.bus
            .queue
            .lock()
            .expect("queue")
            .push(Frame { from: self.me.clone(), to: to_key.to_string(), kind, payload: payload.to_string() });
        Ok(())
    }
}

/// Protected-UDP provider holding ORIGINALS (the sessions only touch dups).
struct FedSocks {
    source: Arc<ProtectedUdpFdSource>,
    raw_fds: Vec<i32>,
}

impl FedSocks {
    fn with_lo_sockets(n: usize) -> Self {
        let mut raw_fds = Vec::with_capacity(n);
        for _ in 0..n {
            let fd = unsafe { socket(2, 2, 0) }; // AF_INET, SOCK_DGRAM
            assert!(fd >= 0, "socket() failed");
            raw_fds.push(fd);
        }
        let source = Arc::new(ProtectedUdpFdSource::new_with_fd(-1));
        for &fd in &raw_fds {
            source.feed(fd);
        }
        FedSocks { source, raw_fds }
    }
}

impl Drop for FedSocks {
    fn drop(&mut self) {
        for fd in self.raw_fds.drain(..) {
            unsafe { close(fd) };
        }
    }
}

/// One full instance: REAL WgDeviceFeed (loopback outer socket + TUN stand-in
/// socketpair) + REAL per-peer ICE orchestrator riding the SAME feed as its
/// WG seam (the production wiring) + mock-signal send endpoint.
struct Node {
    me: String,
    peer_key: String,
    feed: Arc<WgDeviceFeed>,
    orch: PeerIceOrchestrator,
    socks: FedSocks,
    /// TUN stand-in test end: frames written here enter the device; frames
    /// the device decapsulates are readable here.
    tun_hand: i32,
    raws: Vec<i32>,
}

impl Node {
    fn new(
        me_key: &str,
        peer_key: &str,
        peer_vpn: [u8; 4],
        secret_b64: &str,
        tie_breaker: u64,
        bus: Arc<SignalBus>,
        ice_sockets: usize,
    ) -> Self {
        // The "shell-fed" WG outer socket (fallback egress; pre-N11 the ONLY
        // egress — the round-7 bug source). Bound loopback, ephemeral.
        let wg_raw = unsafe { socket(2, 2, 0) };
        assert!(wg_raw >= 0);
        let sa = sys::sockaddr_in::new([127, 0, 0, 1], 0);
        assert_eq!(
            unsafe { sys::bind(wg_raw, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) },
            0
        );
        // The TUN stand-in (same trick as the N7/N8 feed tests).
        let mut sv = [-1i32; 2];
        assert_eq!(unsafe { socketpair(1, 2, 0, &mut sv) }, 0);

        let mut cfg = WgDeviceConfig::new(secret_b64.to_string());
        cfg.hs_retry_ms = 500; // fast campaign under the injected clock
        let feed = Arc::new(WgDeviceFeed::new(cfg));
        feed.feed_tun(sv[0]).expect("tun feed");
        feed.feed_wg_socket(wg_raw).expect("wg socket feed");
        feed.apply_peers(&[WgPeerEntry {
            pub_key_b64: peer_key.to_string(),
            allowed_ips: vec![config::Route { addr: peer_vpn, prefix_len: 32 }],
        }])
        .expect("device peers");
        assert!(feed.device_up(), "both feeds present → device up");

        let socks = FedSocks::with_lo_sockets(ice_sockets);
        let mut orch = PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(StaticInterfaces(vec![InterfaceAddr {
                name: "eth0".into(),
                addr: [127, 0, 0, 1],
            }])),
            socks: socks.source.clone(),
            signal: Arc::new(MockSignalEndpoint { me: me_key.to_string(), bus: bus.clone() }),
            wg: feed.clone(),
            tie_breaker: Some(tie_breaker),
        });
        orch.set_peers(&[peer_key.to_string()]);
        orch.set_signal_ready(true);
        set_nonblock(sv[1]); // test-side TUN end: never blocks the harness
        Node {
            me: me_key.to_string(),
            peer_key: peer_key.to_string(),
            feed,
            orch,
            socks,
            tun_hand: sv[1],
            raws: vec![wg_raw, sv[0], sv[1]],
        }
    }

    fn status_of(&self, peer: &str) -> netbird_core::peer_conn::PeerIceStatus {
        self.orch.peer_status(peer).expect("peer entry")
    }

    /// Device-side peer snapshot (endpoint/egress/session of the REAL device).
    fn device_peer(&self) -> netbird_core::wg_device::WgPeerStatus {
        self.feed
            .with_device(|d| d.peers()[0].clone())
            .expect("device up")
    }

    fn device_stats(&self) -> netbird_core::wg_device::WgDeviceStats {
        self.feed.with_device(|d| d.stats()).expect("device up")
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        for fd in self.raws.drain(..) {
            unsafe { close(fd) };
        }
    }
}

/// Route queued frames to their recipient; frames for a frozen/absent
/// recipient stay queued (a dead path is the failure test).
fn deliver(bus: &Arc<SignalBus>, now: u64, a: &mut Node, mut b: Option<&mut Node>) {
    for f in bus.drain() {
        bus.seen.lock().expect("seen").push(f.clone());
        if f.to == a.me {
            a.orch.handle_signal(&f.from, f.kind, &f.payload, now).expect("A recv");
        } else if let Some(b) = b.as_deref_mut() {
            if f.to == b.me {
                b.orch.handle_signal(&f.from, f.kind, &f.payload, now).expect("B recv");
            }
        }
    }
}

/// One pump beat on the injected clock: orchestrators, signal delivery,
/// BOTH device pumps (service_tun / service_udp / tick).
fn step(a: &mut Node, b: &mut Node, bus: &Arc<SignalBus>, now: u64) {
    let _ = a.orch.run_once(now);
    let _ = b.orch.run_once(now);
    deliver(bus, now, a, Some(b));
    for node in [a, b] {
        let _ = node.feed.with_device(|d| {
            d.service_tun(now);
            d.service_udp(now);
            d.tick(now);
        });
    }
}

/// Pump both instances until `cond` holds (bounded iterations).
fn pump_until(
    a: &mut Node,
    b: &mut Node,
    bus: &Arc<SignalBus>,
    now: &mut u64,
    deadline_ms: u64,
    cond: impl Fn(&Node, &Node) -> bool,
) -> bool {
    while *now <= deadline_ms {
        step(a, b, bus, *now);
        *now += 10;
        if cond(a, b) {
            return true;
        }
    }
    false
}

fn candidate_frames(bus: &SignalBus, from: &str) -> Vec<Candidate> {
    bus.seen()
        .iter()
        .filter(|f| f.from == from && f.kind == PeerSignalKind::Candidate)
        .map(|f| Candidate::unmarshal(&f.payload).expect("candidate wire form"))
        .collect()
}

/// Minimal IPv4/UDP frame (checksum-free is fine: the device parses dst only
/// and boringtun encapsulates raw bytes; the receiver writes plaintext as-is).
fn probe_frame(src: [u8; 4], dst: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut f = vec![0u8; 28 + payload.len()];
    f[0] = 0x45; // IPv4, IHL 5
    let total = f.len() as u16;
    f[2..4].copy_from_slice(&total.to_be_bytes());
    f[8] = 64; // TTL
    f[9] = 17; // UDP
    f[12..16].copy_from_slice(&src);
    f[16..20].copy_from_slice(&dst);
    f[20..22].copy_from_slice(&40000u16.to_be_bytes());
    f[22..24].copy_from_slice(&40001u16.to_be_bytes());
    let ulen = (8 + payload.len()) as u16;
    f[24..26].copy_from_slice(&ulen.to_be_bytes());
    f[28..].copy_from_slice(payload);
    f
}

fn tun_write(fd: i32, frame: &[u8]) {
    let n = unsafe { write(fd, frame.as_ptr() as *const core::ffi::c_void, frame.len()) };
    assert_eq!(n, frame.len() as isize, "TUN stand-in write");
}

fn tun_try_read(fd: i32) -> Option<Vec<u8>> {
    let mut buf = [0u8; 2048];
    let n = unsafe { read(fd, buf.as_mut_ptr() as *mut core::ffi::c_void, buf.len()) };
    if n > 0 {
        Some(buf[..n as usize].to_vec())
    } else {
        None
    }
}

/// The N3-7 shell snapshot through the REAL gate (same shape as the N5c
/// tests): default route allowed iff tunnel_ready && a reachable ICE peer.
fn gated_config(
    tunnel_ready: bool,
    ice: &netbird_core::peer_conn::IceOrchestratorSummary,
) -> ShellNetworkConfig {
    let map = netbird_core::network_map::NetworkMap {
        serial: 1,
        peer: None,
        peers: vec![netbird_core::network_map::PeerInfo {
            wg_pub_key: "peer".into(),
            allowed_ips: vec![config::Route { addr: [10, 77, 0, 2], prefix_len: 32 }],
            fqdn: None,
        }],
        peers_is_empty: false,
        offline_peers: vec![],
        routes: vec![netbird_core::network_map::ManagedRoute {
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
    };
    ShellNetworkConfig::from_map_gated(&map, false, tunnel_ready && ice_ready_for_default_route(ice))
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

/// THE N11 core: two full instances converge ICE → WG handshakes cross the
/// SELECTED sockets → sessions establish → probes flow BOTH ways; STUN
/// keepalives and WG data coexist on the selected socket without misrouting.
#[test]
fn wg_handshake_and_probes_ride_the_ice_selected_socket() {
    let (sec_a, key_a) = key_pair(0xA1);
    let (sec_b, key_b) = key_pair(0xB2);
    let bus = Arc::new(SignalBus::default());
    let mut a = Node::new(&key_a, &key_b, VPN_B, &sec_a, 0x1111, bus.clone(), 16);
    let mut b = Node::new(&key_b, &key_a, VPN_A, &sec_b, 0x2222, bus.clone(), 16);
    b.orch.set_initiator(&key_a, false); // A offers, B answers (deterministic roles)

    // Sim-clock base = the real monotonic clock: the WG device anchors its
    // handshake campaigns (set_endpoint) on the REAL clock while the pump
    // advances this value — starting at 0 would freeze the campaign retries.
    let mut now = sys::mono_ms();
    let deadline = now + 30_000;
    let ok = pump_until(&mut a, &mut b, &bus, &mut now, deadline, |a, b| {
        a.status_of(&key_b).state == PeerIceState::Connected
            && b.status_of(&key_a).state == PeerIceState::Connected
            && a.feed.dataplane_status().map(|s| s.ready).unwrap_or(false)
            && b.feed.dataplane_status().map(|s| s.ready).unwrap_or(false)
    });
    assert!(ok, "both instances must reach Connected + established WG sessions");

    // WG rides the SELECTED socket, proven three ways:
    // (1) each device's egress local address == the local candidate THAT
    //     side signaled (the selected pair's local end);
    let a_cand = &candidate_frames(&bus, &key_a)[0];
    let b_cand = &candidate_frames(&bus, &key_b)[0];
    let a_egress = a.device_peer().egress_local.expect("A egress attached");
    let b_egress = b.device_peer().egress_local.expect("B egress attached");
    assert_eq!(a_egress, (parse_addr(&a_cand.address), a_cand.port), "A egress == A selected candidate");
    assert_eq!(b_egress, (parse_addr(&b_cand.address), b_cand.port), "B egress == B selected candidate");
    // (2) each side's landed endpoint is the peer's selected socket;
    assert_eq!(a.status_of(&key_b).selected_remote, Some((parse_addr(&b_cand.address), b_cand.port)));
    assert_eq!(b.status_of(&key_a).selected_remote, Some((parse_addr(&a_cand.address), a_cand.port)));
    // (3) nothing traveled off-path: zero unknown-peer drops and zero
    //     decrypt errors on both devices (a fallback-socket handshake would
    //     surface as unknown_peer_drops; a misrouted STUN as decrypt noise).
    let a_stats = a.device_stats();
    let b_stats = b.device_stats();
    assert!(a_stats.handshake_initiations >= 1 && b_stats.handshake_initiations >= 1);
    assert_eq!(a_stats.unknown_peer_drops, 0, "A must see no off-path datagrams");
    assert_eq!(b_stats.unknown_peer_drops, 0, "B must see no off-path datagrams");
    assert_eq!(a_stats.decrypt_errors, 0);
    assert_eq!(b_stats.decrypt_errors, 0);
    assert!(a_stats.rx_packets >= 1 && b_stats.rx_packets >= 1, "WG handshakes were received");

    // default-route gate: tunnel_ready && reachable → 0.0.0.0/0 allowed.
    let summary = a.orch.summary();
    assert_eq!(summary.reachable, 1);
    let snap = gated_config(a.feed.tunnel_ready(), &summary);
    assert!(snap.default_route_allowed, "established WG + reachable ICE must arm the gate");

    // --- keepalive/data coexistence: pump past TWO ICE keepalive intervals
    // (4 s cadence) and past the 6 s disconnected threshold, interleaving
    // bidirectional probes. If keepalives stopped being answered, the
    // sessions would drop to Disconnected at +6 s.
    let probes_a = [
        probe_frame(VPN_A, VPN_B, b"n11-probe-a1"),
        probe_frame(VPN_A, VPN_B, b"n11-probe-a2"),
        probe_frame(VPN_A, VPN_B, b"n11-probe-a3"),
    ];
    let probes_b = [probe_frame(VPN_B, VPN_A, b"n11-probe-b1"), probe_frame(VPN_B, VPN_A, b"n11-probe-b2")];
    let mut ai = 0;
    let mut bi = 0;
    let deadline = now + 8_600;
    let a_rx0 = a.device_stats().rx_bytes_to_tun;
    let b_rx0 = b.device_stats().rx_bytes_to_tun;
    let a_need: u64 = probes_a.iter().map(|p| p.len() as u64).sum();
    let b_need: u64 = probes_b.iter().map(|p| p.len() as u64).sum();
    let mut b_got_all = false;
    let mut a_got_all = false;
    while now <= deadline {
        if ai < probes_a.len() && now % 1_700 < 10 {
            tun_write(a.tun_hand, &probes_a[ai]);
            ai += 1;
        }
        if bi < probes_b.len() && now % 2_300 < 10 {
            tun_write(b.tun_hand, &probes_b[bi]);
            bi += 1;
        }
        step(&mut a, &mut b, &bus, now);
        now += 10;
        if ai == probes_a.len() && !b_got_all && b.device_stats().rx_bytes_to_tun - b_rx0 >= a_need {
            b_got_all = true;
        }
        if bi == probes_b.len() && !a_got_all && a.device_stats().rx_bytes_to_tun - a_rx0 >= b_need {
            a_got_all = true;
        }
    }
    assert!(b_got_all && a_got_all, "all probes must traverse end-to-end (A→B and B→A)");
    assert_eq!(
        a.status_of(&key_b).state,
        PeerIceState::Connected,
        "ICE keepalives must keep the selected pair alive past +6 s of pumping"
    );
    assert_eq!(b.status_of(&key_a).state, PeerIceState::Connected);
    // Payload integrity: the decapsulated frames read at the peer's TUN are
    // byte-identical to the injected probes (read them back).
    let mut got: Vec<Vec<u8>> = Vec::new();
    while let Some(f) = tun_try_read(b.tun_hand) {
        got.push(f);
    }
    for p in &probes_a {
        let tag = String::from_utf8_lossy(&p[28..]).into_owned();
        assert!(got.contains(p), "B's TUN must contain probe {tag}");
    }
    // WG data did not disturb ICE, and STUN never leaked into WG:
    assert_eq!(a.device_stats().decrypt_errors, 0);
    assert_eq!(b.device_stats().decrypt_errors, 0);
    assert_eq!(a.device_stats().unknown_peer_drops, 0);
    assert_eq!(b.device_stats().unknown_peer_drops, 0);
}

/// 反例（round-7 bug 形态复现）：WG 设备用**非选中 socket**（这里即 fallback
/// 外层 socket）向对端 **ICE socket** 发握手 —— 握手发出（计数增长）、落到
/// 对端 ICE socket 上（可读出 WG 形态的报文，证明它没有被当 STUN 吞掉）、
/// 但永远建立不了会话（rx=0、peers_with_session=0）。修复后的正常路径
/// （egress 骑选中 socket）见上一测试：unknown_peer_drops=0 即「不再发生」。
#[test]
fn wg_from_a_non_selected_socket_never_establishes_a_session() {
    let (sec_a, key_a) = key_pair(0xA1);
    let (sec_b, key_b) = key_pair(0xB2);
    // Two devices + two bare "ICE" sockets nobody reads (the round-7 shape:
    // the endpoint IS the peer's ICE socket, the egress is the wrong socket).
    let mk = |secret_b64: &str, peer_key: &str, peer_vpn: [u8; 4]| -> (Arc<WgDeviceFeed>, i32, i32, i32) {
        let wg_raw = unsafe { socket(2, 2, 0) };
        assert!(wg_raw >= 0);
        let sa = sys::sockaddr_in::new([127, 0, 0, 1], 0);
        assert_eq!(unsafe { sys::bind(wg_raw, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) }, 0);
        let mut sv = [-1i32; 2];
        assert_eq!(unsafe { socketpair(1, 2, 0, &mut sv) }, 0);
        let mut cfg = WgDeviceConfig::new(secret_b64.to_string());
        cfg.hs_retry_ms = 100;
        let feed = Arc::new(WgDeviceFeed::new(cfg));
        feed.feed_tun(sv[0]).expect("tun");
        feed.feed_wg_socket(wg_raw).expect("wg");
        feed.apply_peers(&[WgPeerEntry {
            pub_key_b64: peer_key.to_string(),
            allowed_ips: vec![config::Route { addr: peer_vpn, prefix_len: 32 }],
        }])
        .expect("peers");
        (feed, wg_raw, sv[0], sv[1])
    };
    let (feed_a, a_wg, _a_tun, _a_hand) = mk(&sec_a, &key_b, VPN_B);
    let (feed_b, b_wg, _b_tun, _b_hand) = mk(&sec_b, &key_a, VPN_A);
    // The ICE-stand sockets (stand-in for the round-7 "selected" sockets).
    let ice_a = bound_lo_socket();
    let ice_b = bound_lo_socket();
    let (ice_a_addr, ice_a_port) = sock_name(ice_a);
    let (ice_b_addr, ice_b_port) = sock_name(ice_b);

    // Exactly the round-7 mis-wiring: endpoint = peer's ICE socket; egress =
    // own (non-selected) WG socket — i.e. NO egress attach at all.
    feed_a.apply_endpoint(&key_b, ice_b_addr, ice_b_port).expect("endpoint");
    feed_b.apply_endpoint(&key_a, ice_a_addr, ice_a_port).expect("endpoint");

    // Same clock alignment as above: campaigns anchor on the real clock.
    let mut now = sys::mono_ms();
    for _ in 0..80 {
        // 80 × 50 ms = 4 s: many handshake campaigns fire
        for feed in [&feed_a, &feed_b] {
            let _ = feed.with_device(|d| {
                d.service_tun(now);
                d.service_udp(now);
                d.tick(now);
            });
        }
        now += 50;
    }

    let sa = feed_a.dataplane_status().expect("status");
    let sb = feed_b.dataplane_status().expect("status");
    assert!(sa.handshakes >= 2 && sb.handshakes >= 2, "handshakes leave the wrong socket (round-7 signature)");
    assert_eq!(sa.rx_packets, 0, "the peer never receives them on its WG socket");
    assert_eq!(sb.rx_packets, 0);
    assert_eq!(sa.peers_with_session, 0, "no session can ever establish off-path");
    assert_eq!(sb.peers_with_session, 0);
    assert!(!sa.ready && !sb.ready, "tunnel_ready stays false (gate holds)");

    // The handshakes DID land on the peer's ICE socket — as WG-shaped
    // datagrams (under the N11 demux these route to WG, not STUN; here
    // nobody demuxes them, exactly the silent black hole of round-7).
    let first = tun_try_read(ice_b).expect("B's ICE socket holds A's handshake");
    assert!(is_wg_datagram(&first), "the stranded datagram is WG-shaped, not STUN");

    unsafe { close(ice_a) };
    unsafe { close(ice_b) };
    unsafe { close(a_wg) };
    unsafe { close(b_wg) };
    unsafe { close(_a_tun) };
    unsafe { close(_b_tun) };
    unsafe { close(_a_hand) };
    unsafe { close(_b_hand) };
}

/// 迁移/失败：选中路径死亡 → +6s Disconnected（endpoint+egress 回收，
/// 设备无路径可发 = fail-closed；默认路由闸 HOLD）→ +12s Failed（会话拆除
/// + 重新协商武装）→ 解冻对端后经全新会话重新收敛（新 socket、egress 重挂、
/// 会话重建、探针恢复）。
#[test]
fn selected_connection_failure_recycles_fail_closed_then_renegotiates() {
    let (sec_a, key_a) = key_pair(0xA1);
    let (sec_b, key_b) = key_pair(0xB2);
    let bus = Arc::new(SignalBus::default());
    let mut a = Node::new(&key_a, &key_b, VPN_B, &sec_a, 0x1111, bus.clone(), 24);
    let mut b = Node::new(&key_b, &key_a, VPN_A, &sec_b, 0x2222, bus.clone(), 24);
    b.orch.set_initiator(&key_a, false);

    let mut now = sys::mono_ms();
    let deadline = now + 30_000;
    let ok = pump_until(&mut a, &mut b, &bus, &mut now, deadline, |a, b| {
        a.feed.dataplane_status().map(|s| s.ready).unwrap_or(false)
            && b.feed.dataplane_status().map(|s| s.ready).unwrap_or(false)
    });
    assert!(ok, "precondition: established over the selected sockets");
    let first_offer_payload = bus
        .seen()
        .iter()
        .find(|f| f.from == key_a && f.kind == PeerSignalKind::Offer)
        .map(|f| f.payload.clone())
        .unwrap_or_default();

    // Freeze B entirely (path death as seen from A): no pumps, no replies.
    let selected_at = now;
    let mut disconnected_at = None;
    let mut failed_at = None;
    let mut reoffered = false;
    while now <= selected_at + 20_000 {
        let _ = a.orch.run_once(now);
        deliver(&bus, now, &mut a, None); // B frozen: frames accumulate
        let _ = a.feed.with_device(|d| {
            d.service_tun(now);
            d.service_udp(now);
            d.tick(now);
        });
        now += 25;
        let st = a.status_of(&key_b);
        if disconnected_at.is_none() && st.state == PeerIceState::Disconnected && now > selected_at + 6_000 {
            disconnected_at = Some(now);
            // fail-closed: endpoint + egress recycled THE MOMENT ICE dies
            let peer = a.device_peer();
            assert_eq!(peer.endpoint, None, "endpoint recycled at Disconnected");
            assert_eq!(peer.egress_local, None, "egress closed at Disconnected");
            let summary = a.orch.summary();
            assert_eq!(summary.reachable, 0, "disconnected peer is not reachable");
            let snap = gated_config(a.feed.tunnel_ready(), &summary);
            assert!(!snap.default_route_allowed, "the gate must HOLD 0.0.0.0/0 through the failure");
        }
        if failed_at.is_none() && st.state == PeerIceState::Failed && now > selected_at + 12_000 {
            failed_at = Some(now);
        }
        // renegotiation armed: a FRESH offer appears after the Failed cooldown
        if failed_at.is_some() && !reoffered {
            reoffered = bus.seen().iter().rev().take(8).any(|f| {
                f.from == key_a && f.kind == PeerSignalKind::Offer && f.payload != first_offer_payload
            });
        }
        if reoffered && now > selected_at + 16_000 {
            break;
        }
    }
    let d = disconnected_at.expect("Disconnected must land at ~+6 s of silence");
    assert!(
        (selected_at + 6_000..=selected_at + 7_000).contains(&d),
        "Disconnected at ~+6 s (got +{} ms)",
        d - selected_at
    );
    let f = failed_at.expect("Failed must land at ~+12 s of silence");
    assert!(
        (selected_at + 12_000..=selected_at + 13_000).contains(&f),
        "Failed at ~+12 s (got +{} ms)",
        f - selected_at
    );
    assert!(reoffered, "a fresh negotiation must be armed after Failed");

    // Unfreeze B: its frozen session times out immediately (Failed →
    // session dropped → endpoint recycled), then A's queued fresh offer
    // re-converges both sides over NEW sockets.
    let deadline = now + 30_000;
    let recovered = pump_until(&mut a, &mut b, &bus, &mut now, deadline, |a, b| {
        a.feed.dataplane_status().map(|s| s.ready).unwrap_or(false)
            && b.feed.dataplane_status().map(|s| s.ready).unwrap_or(false)
            && a.status_of(&key_b).state == PeerIceState::Connected
            && b.status_of(&key_a).state == PeerIceState::Connected
    });
    assert!(recovered, "both sides must re-converge after the failure");

    // The WG transport was RE-attached to the (new) selected socket and
    // sessions re-established: egress present again, probes flow again.
    assert!(a.device_peer().egress_local.is_some(), "egress re-attached after renegotiation");
    assert!(b.device_peer().egress_local.is_some());
    assert!(a.device_peer().endpoint.is_some());
    assert_eq!(
        a.status_of(&key_b).selected_remote,
        b.device_peer().egress_local,
        "A's landed endpoint == B's (new) egress — the mirror property"
    );
    let summary = a.orch.summary();
    let snap = gated_config(a.feed.tunnel_ready(), &summary);
    assert!(snap.default_route_allowed, "the gate re-arms after recovery");

    let probe = probe_frame(VPN_A, VPN_B, b"n11-post-recovery");
    tun_write(a.tun_hand, &probe);
    let mut got = false;
    for _ in 0..400 {
        step(&mut a, &mut b, &bus, now);
        now += 10;
        if tun_try_read(b.tun_hand).as_deref() == Some(&probe[..]) {
            got = true;
            break;
        }
    }
    assert!(got, "probe payload must traverse the NEW selected path");
}

fn bound_lo_socket() -> i32 {
    let fd = unsafe { socket(2, 2, 0) };
    assert!(fd >= 0);
    let sa = sys::sockaddr_in::new([127, 0, 0, 1], 0);
    assert_eq!(unsafe { sys::bind(fd, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) }, 0);
    set_nonblock(fd);
    fd
}

fn sock_name(fd: i32) -> ([u8; 4], u16) {
    let mut addr = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut len = core::mem::size_of::<sys::sockaddr_in>() as u32;
    assert_eq!(unsafe { sys::getsockname(fd, &mut addr, &mut len) }, 0);
    (addr.sin_addr, u16::from_be(addr.sin_port))
}

fn parse_addr(s: &str) -> [u8; 4] {
    let parts: Vec<u8> = s.split('.').map(|o| o.parse().expect("ipv4")).collect();
    [parts[0], parts[1], parts[2], parts[3]]
}
