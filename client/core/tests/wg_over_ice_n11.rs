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
use netbird_core::peer_conn::{ice_ready_for_default_route, InlineGatherExec, PeerIceDeps, PeerIceOrchestrator, PeerIceState, PeerSignalKind, SignalExchange};
use netbird_core::wg_device::{WgDeviceConfig, WgDeviceFeed, WgEgressCarrier};
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
        let source = Arc::new(ProtectedUdpFdSource::new_with_fd(-1).unwrap());
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
        Node::new_with_peers(
            me_key,
            &[(peer_key.to_string(), peer_vpn)],
            secret_b64,
            tie_breaker,
            bus,
            ice_sockets,
            &[],
        )
    }

    /// STAGE 3 multi-peer flavour: one node with MANY registered peers, all
    /// riding the SAME shared socket (the hub of the many-peer test).
    /// `answerers` lists the peers this node must NOT initiate to (deterministic
    /// offerer/answerer roles).
    fn new_with_peers(
        me_key: &str,
        peers: &[(String, [u8; 4])],
        secret_b64: &str,
        tie_breaker: u64,
        bus: Arc<SignalBus>,
        ice_sockets: usize,
        answerers: &[&str],
    ) -> Self {
        // The "shell-fed" WG outer socket — STAGE 3: this ONE socket is also the
        // ICE socket (upstream: the mux is built over the WG bind's own conn,
        // `ice_bind.go:291-316`). Bound WILDCARD like `wg_fwd_open` does, so the
        // shared socket's local address is `0.0.0.0:<port>` — the production
        // shape — and loopback checks reach it all the same.
        let wg_raw = unsafe { socket(2, 2, 0) };
        assert!(wg_raw >= 0);
        let sa = sys::sockaddr_in::new([0, 0, 0, 0], 0);
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
        feed.apply_peers(
            &peers
                .iter()
                .map(|(key, vpn)| WgPeerEntry {
                    pub_key_b64: key.clone(),
                    allowed_ips: vec![config::Route { addr: *vpn, prefix_len: 32 }],
                })
                .collect::<Vec<_>>(),
        )
        .expect("device peers");
        assert!(feed.device_up(), "both feeds present → device up");

        let socks = FedSocks::with_lo_sockets(ice_sockets);
        let keys: Vec<String> = peers.iter().map(|(k, _)| k.clone()).collect();
        let mut orch = PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(StaticInterfaces(vec![InterfaceAddr {
                name: "eth0".into(),
                addr: [127, 0, 0, 1],
            }])),
            socks: socks.source.clone(),
            signal: Arc::new(MockSignalEndpoint { me: me_key.to_string(), bus: bus.clone() }),
            wg: feed.clone(),
            tie_breaker: Some(tie_breaker),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        });
        // STAGE 1（官方对齐审计 §2.7）：注入时钟 harness —— 本地候选收集走
        // 确定性内联执行者（异步模型由 `src/peer_conn.rs` 的线程执行者用例覆盖）。
        orch.set_gather_exec(Arc::new(InlineGatherExec));
        // STAGE 3: the ONE mux is built OVER the device's own protected socket —
        // the production wiring (`armIceMux` → `connector_ice_mux_arm`). No socket
        // comes from the ICE provider any more: the pool stays untouched, which is
        // exactly what the assertions below pin.
        let (mux, replaced) = orch
            .arm_mux_from_wg_socket()
            .expect("stage-3 mux over the WG outer socket");
        assert!(!replaced, "first arm of this instance");
        assert_eq!(
            mux.local_port(),
            sock_name(wg_raw).1,
            "the mux IS the WG outer socket (one open + one bind + one protect)"
        );
        assert!(
            !mux.reader_running(),
            "injected-clock harness: no reader thread, the pumps own the cadence"
        );
        assert!(mux.sink_armed(), "non-STUN datagrams go straight into the device");
        assert_eq!(
            socks.source.pending(),
            ice_sockets,
            "the ICE socket provider is NOT consumed: the whole plane rides the WG socket"
        );
        orch.set_peers(&keys);
        for answerer in answerers {
            orch.set_initiator(answerer, false);
        }
        orch.set_signal_ready(true);
        set_nonblock(sv[1]); // test-side TUN end: never blocks the harness
        Node {
            me: me_key.to_string(),
            peer_key: peers[0].0.clone(),
            feed,
            orch,
            socks,
            tun_hand: sv[1],
            raws: vec![wg_raw, sv[0], sv[1]],
        }
    }

    /// The shared socket this instance's ICE plane AND WireGuard ride.
    fn mux(&self) -> Arc<netbird_core::ice_mux::IceMux> {
        self.orch.ice_mux().expect("the one shared mux")
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

    /// The device-side snapshot of ONE named peer (multi-peer nodes).
    fn device_peer_of(&self, key: &str) -> netbird_core::wg_device::WgPeerStatus {
        self.feed
            .with_device(|d| {
                d.peers()
                    .into_iter()
                    .find(|p| p.pub_key_b64 == key)
                    .expect("registered peer")
            })
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
    // (1) each device's egress fd IS the local socket the selected pair runs on;
    //     STAGE 3 (official-alignment §2.7): that socket is the ONE shared socket
    //     for ICE *and* WireGuard — the WG outer socket itself, WILDCARD-bound
    //     (upstream's `0.0.0.0:NET_PORT` single socket, `ice_bind.go:291-316`), so
    //     its own local address is `0.0.0.0:<mux port>`. The PORT is what ties it
    //     to the selected pair — it is the port every candidate of that side
    //     advertised.
    let a_cand = &candidate_frames(&bus, &key_a)[0];
    let b_cand = &candidate_frames(&bus, &key_b)[0];
    let a_egress = a.device_peer().egress_local.expect("A egress attached");
    let b_egress = b.device_peer().egress_local.expect("B egress attached");
    assert_eq!(a_egress, ([0, 0, 0, 0], a_cand.port), "A egress == A's ONE shared socket");
    assert_eq!(b_egress, ([0, 0, 0, 0], b_cand.port), "B egress == B's ONE shared socket");
    // (2) each side's landed endpoint is the peer's selected socket;
    assert_eq!(a.status_of(&key_b).selected_remote, Some((parse_addr(&b_cand.address), b_cand.port)));
    assert_eq!(b.status_of(&key_a).selected_remote, Some((parse_addr(&a_cand.address), a_cand.port)));
    // (3) nothing traveled off-path — with STAGE 3's exact semantics. The mux now
    //     hands every non-STUN datagram STRAIGHT to the device (upstream shape:
    //     WG's receive path consumes what the STUN filter left, `ice_bind.go:257-285`),
    //     so the properties are read off the mux's and the device's own counters:
    //     * `mux.data_unknown == 0` — every WG datagram came from an address one of
    //       OUR sessions had already written to: no foreign source at all (this is
    //       the round-7 property the per-session-queue `unknown_peer_drops == 0`
    //       assertion used to stand for);
    //     * `decrypt_errors == 0` — no STUN datagram ever reached WG (the demux holds
    //       on the shared socket);
    //     * `unknown_peer_drops <= 1` — the ONE drop the shared socket legitimately
    //       produces: the CONTROLLED side selects (and starts its WG campaign) one
    //       check-response round before the CONTROLLING side applies the endpoint, so
    //       that one initiation arrives at a device whose `src == endpoint` rule has
    //       not seen the peer yet (upstream WG adopts the endpoint from the handshake
    //       itself; ours deliberately does not — pinned by
    //       `wg_from_a_non_selected_socket_never_establishes_a_session`). BOTH sides
    //       run a campaign, so the redundant copy costs nothing: the session is
    //       established below either way.
    let a_stats = a.device_stats();
    let b_stats = b.device_stats();
    assert!(a_stats.handshake_initiations >= 1 && b_stats.handshake_initiations >= 1);
    assert_eq!(a.mux().stats().data_unknown, 0, "A: no WG datagram from an unregistered source");
    assert_eq!(b.mux().stats().data_unknown, 0, "B: no WG datagram from an unregistered source");
    assert!(a_stats.unknown_peer_drops <= 1, "A: at most one pre-endpoint initiation");
    assert!(b_stats.unknown_peer_drops <= 1, "B: at most one pre-endpoint initiation");
    assert_eq!(a_stats.decrypt_errors, 0);
    assert_eq!(b_stats.decrypt_errors, 0);
    assert!(a_stats.rx_packets >= 1 && b_stats.rx_packets >= 1, "WG handshakes were received");

    // (4) STAGE 3 reader-ownership facts: the mux is the ONE reader of the shared
    //     socket (the device's own `service_udp` is retired), every datagram on it
    //     was read through the mux — STUN demuxed to the sessions, WG-shaped handed
    //     straight into the device — and the ICE socket provider was never touched
    //     (see `Node::new`'s `pending()` assertion: no second socket exists).
    assert!(a.mux().stats().reads > 0 && b.mux().stats().reads > 0, "the shared socket was read");
    assert_eq!(a.socks.source.pending(), 16, "no pool socket consumed on A");
    assert_eq!(b.socks.source.pending(), 16, "no pool socket consumed on B");
    assert!(
        a.feed.with_device(|d| d.shared_socket_reader()).unwrap_or(false),
        "A: the device's own socket read is retired — the mux is the ONE reader"
    );
    assert!(
        b.feed.with_device(|d| d.shared_socket_reader()).unwrap_or(false),
        "B: the device's own socket read is retired — the mux is the ONE reader"
    );

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
    // WG data did not disturb ICE, and STUN never leaked into WG. The one allowed
    // `unknown_peer_drops` is the pre-endpoint initiation counted in (3) above — it
    // must not GROW while the sessions are up (which is what a misrouted STUN or a
    // stray source would look like).
    assert_eq!(a.device_stats().decrypt_errors, 0);
    assert_eq!(b.device_stats().decrypt_errors, 0);
    assert!(a.device_stats().unknown_peer_drops <= 1, "A: no drop beyond the pre-endpoint window");
    assert!(b.device_stats().unknown_peer_drops <= 1, "B: no drop beyond the pre-endpoint window");
    assert_eq!(a.mux().stats().data_unknown, 0);
    assert_eq!(b.mux().stats().data_unknown, 0);
}

// ---------------------------------------------------------------------------
// STAGE 3: MANY peers, ONE socket per node
// ---------------------------------------------------------------------------

/// The overlay addresses of the three spokes of the many-peer test.
const SPOKE_VPN: [[u8; 4]; 3] = [[10, 77, 0, 11], [10, 77, 0, 12], [10, 77, 0, 13]];

/// One beat with N nodes: every orchestrator runs (and drains ITS shared socket
/// once per tick — the injected-clock stand-in for the mux reader thread), every
/// queued frame is delivered to the ONE node it names, then every device pumps.
fn step_nodes(bus: &Arc<SignalBus>, now: u64, nodes: &mut [Node]) {
    for n in nodes.iter_mut() {
        let _ = n.orch.run_once(now);
    }
    let frames = bus.drain();
    for f in frames {
        bus.seen.lock().expect("seen").push(f.clone());
        for n in nodes.iter_mut() {
            if f.to == n.me {
                let _ = n.orch.handle_signal(&f.from, f.kind, &f.payload, now);
            }
        }
    }
    for n in nodes.iter_mut() {
        let _ = n.feed.with_device(|d| {
            d.service_tun(now);
            d.service_udp(now);
            d.tick(now);
        });
    }
}

/// STAGE 3 core: a hub with THREE peers plus three one-peer spokes — four nodes,
/// each with exactly ONE socket serving both planes. Proven here:
/// * every negotiation converges over that one socket (STUN demuxed by ufrag /
///   remote address), and every device shows ZERO `decrypt_errors` (no STUN ever
///   reached WG) and ZERO `unknown_peer_drops` (nothing arrived off-path);
/// * the hub holds THREE concurrent established WG sessions on that one socket
///   and `peers_with_session == 3`;
/// * probes flow end-to-end on every one of the three pairs (hub → each spoke and
///   each spoke → hub) **over the shared socket**, i.e. through the mux's sink
///   into `handle_udp` — the device's own `service_udp` is retired;
/// * the ICE socket provider is never consumed on any node: there is no second
///   socket anywhere.
#[test]
fn many_peers_ride_one_shared_socket_per_node() {
    let bus = Arc::new(SignalBus::default());
    let hub = key_pair(0xA1);
    let spokes: Vec<(String, String)> = (0..3u8).map(|i| key_pair(0xB0 + i)).collect();

    let hub_peers: Vec<(String, [u8; 4])> = spokes
        .iter()
        .enumerate()
        .map(|(i, (_, pubk))| (pubk.clone(), SPOKE_VPN[i]))
        .collect();
    let mut nodes: Vec<Node> = vec![
        Node::new_with_peers(&hub.1, &hub_peers, &hub.0, 0x1111, bus.clone(), 12, &[]),
        Node::new_with_peers(&spokes[0].1, &[(hub.1.clone(), VPN_A)], &spokes[0].0, 0x2222, bus.clone(), 12, &[&hub.1]),
        Node::new_with_peers(&spokes[1].1, &[(hub.1.clone(), VPN_A)], &spokes[1].0, 0x3333, bus.clone(), 12, &[&hub.1]),
        Node::new_with_peers(&spokes[2].1, &[(hub.1.clone(), VPN_A)], &spokes[2].0, 0x4444, bus.clone(), 12, &[&hub.1]),
    ];

    let all_ready =
        |nodes: &[Node]| nodes.iter().all(|n| n.feed.dataplane_status().map(|s| s.ready).unwrap_or(false));
    let mut now = sys::mono_ms();
    let deadline = now + 30_000;
    let mut converged = false;
    while now <= deadline {
        step_nodes(&bus, now, &mut nodes);
        now += 10;
        if all_ready(&nodes) {
            converged = true;
            break;
        }
    }
    assert!(converged, "all four nodes must reach established WG sessions over ONE socket each");

    // The hub: THREE sessions on the ONE socket, all three pairs Connected, every
    // candidate of the hub carrying that socket's port.
    let a_status = nodes[0].feed.dataplane_status().expect("hub status");
    assert_eq!(a_status.peers_with_session, 3, "three concurrent WG sessions on one socket: {a_status:?}");
    let hub_port = nodes[0].mux().local_port();
    assert_eq!(
        hub_port,
        nodes[0].feed.with_device(|d| d.local_addr()).unwrap().1,
        "the mux port IS the WG socket's port"
    );
    let hub_cands = candidate_frames(&bus, &hub.1);
    assert!(!hub_cands.is_empty());
    assert!(
        hub_cands.iter().all(|c| c.port == hub_port),
        "every candidate of the hub carries the ONE socket's port"
    );
    for (i, (_, pubk)) in spokes.iter().enumerate() {
        assert_eq!(
            nodes[0].status_of(pubk).state,
            PeerIceState::Connected,
            "hub peer {i} must be Connected over the shared socket"
        );
        assert!(
            nodes[0].device_peer_of(pubk).egress_local.is_some(),
            "hub peer {i} rides the shared socket as its egress"
        );
    }

    // Probes on EVERY pair, both directions, over the shared socket.
    let to_spoke: Vec<Vec<u8>> =
        (0..3).map(|i| probe_frame(VPN_A, SPOKE_VPN[i], format!("hub-probe-{i}").as_bytes())).collect();
    let to_hub: Vec<Vec<u8>> =
        (0..3).map(|i| probe_frame(SPOKE_VPN[i], VPN_A, format!("spoke-probe-{i}").as_bytes())).collect();
    let hub_rx0 = nodes[0].device_stats().rx_bytes_to_tun;
    let spoke_rx0: Vec<u64> =
        (1..4).map(|i| nodes[i].device_stats().rx_bytes_to_tun).collect();
    let hub_need: u64 = to_hub.iter().map(|p| p.len() as u64).sum();
    for (i, f) in to_spoke.iter().enumerate() {
        tun_write(nodes[0].tun_hand, f);
        let _ = i;
    }
    for i in 0..3 {
        tun_write(nodes[1 + i].tun_hand, &to_hub[i]);
    }
    let deadline = now + 8_600; // past TWO ICE keepalives (4 s) and the 6 s disconnect threshold
    let mut spoke_all = vec![false; 3];
    let mut hub_all = false;
    while now <= deadline {
        step_nodes(&bus, now, &mut nodes);
        now += 10;
        for i in 0..3 {
            if !spoke_all[i]
                && nodes[1 + i].device_stats().rx_bytes_to_tun - spoke_rx0[i] >= to_spoke[i].len() as u64
            {
                spoke_all[i] = true;
            }
        }
        if !hub_all && nodes[0].device_stats().rx_bytes_to_tun - hub_rx0 >= hub_need {
            hub_all = true;
        }
        if spoke_all.iter().all(|x| *x) && hub_all {
            break;
        }
    }
    assert!(spoke_all.iter().all(|x| *x), "hub probes must reach all three spokes: {spoke_all:?}");
    assert!(hub_all, "every spoke probe must reach the hub over the shared socket");

    // STUN was demuxed on the same socket for 8.6 s (keepalives answered past the
    // 6 s threshold): every pair is still Connected and no STUN leaked into WG.
    for (i, (_, pubk)) in spokes.iter().enumerate() {
        assert_eq!(
            nodes[1 + i].status_of(&hub.1).state,
            PeerIceState::Connected,
            "spoke {i}: keepalives answered on the shared socket"
        );
        assert_eq!(nodes[0].status_of(pubk).state, PeerIceState::Connected, "hub peer {i} still alive");
    }
    for (i, n) in nodes.iter().enumerate() {
        let s = n.device_stats();
        assert_eq!(s.decrypt_errors, 0, "node {i}: no STUN ever reached WG");
        // STAGE 3: the mux hands every non-STUN datagram straight to the device, so
        // the "no off-path source" property lives on the MUX (see the two-node test
        // for the full argument): every WG datagram came from an address a session had
        // written to. The only drops are the peers whose endpoint had not landed yet,
        // at most one per peer (both sides campaign, so nothing is lost).
        assert_eq!(n.mux().stats().data_unknown, 0, "node {i}: no WG datagram from an unregistered source");
        assert!(
            s.unknown_peer_drops <= 3,
            "node {i}: at most one pre-endpoint initiation per peer (got {})",
            s.unknown_peer_drops
        );
        assert!(n.mux().stats().reads > 0, "node {i}: the shared socket was read");
        assert!(
            n.feed.with_device(|d| d.shared_socket_reader()).unwrap_or(false),
            "node {i}: the mux is the ONE reader of the shared socket"
        );
        assert_eq!(n.socks.source.pending(), 12, "node {i}: no pool socket consumed");
    }
}

// ---------------------------------------------------------------------------
// upstream roaming semantics (netbirdio/wireguard-go @8bf8fa968f1a)
// ---------------------------------------------------------------------------
//
// Upstream identifies the peer from the DATAGRAM (transport keypair index /
// handshake static decrypt), never from the source address, and adopts the
// source address as the peer's endpoint only once the crypto CONSUMED the
// packet (`device/receive.go:373-374`, `:400-401`, `:459`, `:515` →
// `peer.SetEndpointFromPacket`, `device/peer.go:280-288`). NetBird never
// disables roaming: `DisableSomeRoamingForBrokenMobileSemantics` is commented
// out on BOTH mobile paths (`client/iface/device/device_ios.go:92-94`,
// `device_android.go:91-93`). Stage 3's device rule ("inbound only when
// `src == endpoint`") is the deviation these tests pin shut.

/// The single peer of a bare roaming-test node.
fn roam_peer(f: &Arc<WgDeviceFeed>) -> netbird_core::wg_device::WgPeerStatus {
    let mut peers = f.with_device(|d| d.peers()).expect("device up");
    assert_eq!(peers.len(), 1, "one peer per bare node");
    peers.pop().expect("peer")
}

/// Bare device harness for the roaming tests (the N11 nodes minus the ICE
/// machinery): one protected WG socket on `127.0.0.1:0`, one TUN stand-in
/// socketpair (both ends non-blocking) and ONE registered peer.
/// Returns `(feed, wg socket raw, TUN TEST end, TUN DEVICE end)` — the device
/// dups the device end, so both numbers stay usable by the test.
fn roam_node(secret_b64: &str, peer_key: &str, peer_vpn: [u8; 4]) -> (Arc<WgDeviceFeed>, i32, i32, i32) {
    let wg_raw = unsafe { socket(2, 2, 0) };
    assert!(wg_raw >= 0);
    let sa = sys::sockaddr_in::new([127, 0, 0, 1], 0);
    assert_eq!(unsafe { sys::bind(wg_raw, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) }, 0);
    let mut sv = [-1i32; 2];
    assert_eq!(unsafe { socketpair(1, 2, 0, &mut sv) }, 0);
    set_nonblock(sv[0]);
    set_nonblock(sv[1]);
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
    (feed, wg_raw, sv[1], sv[0])
}

/// Pump both bare nodes on the injected clock until both report a live
/// session (or the step budget is spent).
fn roam_pump(a: &Arc<WgDeviceFeed>, b: &Arc<WgDeviceFeed>, now: &mut u64, steps: usize) -> bool {
    for _ in 0..steps {
        for f in [a, b] {
            let _ = f.with_device(|d| {
                d.service_tun(*now);
                d.service_udp(*now);
                d.tick(*now);
            });
        }
        *now += 10;
        let ra = a.dataplane_status().map(|s| s.ready).unwrap_or(false);
        let rb = b.dataplane_status().map(|s| s.ready).unwrap_or(false);
        if ra && rb {
            return true;
        }
    }
    false
}

/// 上游语义：包被**认证消费**后，其来源地址成为该 peer 的新 endpoint（标准
/// WireGuard roaming）。这里的对端 **B 完全没有 endpoint**（ICE 还没选中），
/// A 的发起从 B 从未见过的来源到达 —— stage 3 的 `src == endpoint` 规则会把
/// 这一包丢掉并计 `unknown_peer_drops`（deviation 2 记的正是这一包），上游则
/// 采纳来源地址、照常应答建会话。
#[test]
fn wg_roaming_adopts_the_source_of_an_authenticated_handshake() {
    let (sec_a, key_a) = key_pair(0xA1);
    let (sec_b, key_b) = key_pair(0xB2);
    let (feed_a, a_wg, a_tun, _a_dev) = roam_node(&sec_a, &key_b, VPN_B);
    let (feed_b, b_wg, b_tun, _b_dev) = roam_node(&sec_b, &key_a, VPN_A);
    let (a_addr, a_port) = sock_name(a_wg);
    let (b_addr, b_port) = sock_name(b_wg);

    // A knows where B is; B has NO endpoint at all.
    feed_a.apply_endpoint(&key_b, b_addr, b_port).expect("A → B endpoint");
    assert_eq!(roam_peer(&feed_a).endpoint, Some((b_addr, b_port)));
    assert_eq!(roam_peer(&feed_b).endpoint, None, "B starts with no endpoint at all");

    // Real loopback cable: the initiation lands on B's WG socket from A's WG
    // socket — a source B has never seen.
    let mut now = sys::mono_ms();
    assert!(roam_pump(&feed_a, &feed_b, &mut now, 400), "the roaming handshake must establish");

    // THE roaming fact: B adopted A's source address from the authenticated
    // handshake itself (it had NO endpoint before).
    assert_eq!(
        roam_peer(&feed_b).endpoint,
        Some((a_addr, a_port)),
        "B roams onto the source of the authenticated handshake"
    );
    assert_eq!(
        roam_peer(&feed_a).endpoint,
        Some((b_addr, b_port)),
        "A keeps its configured endpoint (the source it already knew)"
    );
    let sa = feed_a.dataplane_status().expect("status a");
    let sb = feed_b.dataplane_status().expect("status b");
    let st_a = feed_a.with_device(|d| d.stats()).expect("stats a");
    let st_b = feed_b.with_device(|d| d.stats()).expect("stats b");
    assert_eq!(sa.peers_with_session, 1);
    assert_eq!(sb.peers_with_session, 1);
    assert_eq!(st_a.unknown_peer_drops, 0, "an authenticated stranger is not a drop");
    assert_eq!(st_b.unknown_peer_drops, 0);
    assert_eq!(sa.decrypt_errors, 0);
    assert_eq!(sb.decrypt_errors, 0);
    assert!(sa.rx_packets >= 1 && sb.rx_packets >= 1, "both directions were received");

    // The adopted endpoint is USABLE for egress (upstream: the bind sends to
    // `peer.endpoint.val`): B → A rides the roamed address.
    let probe = probe_frame(VPN_B, VPN_A, b"roam-probe");
    tun_write(b_tun, &probe);
    let mut got = false;
    for _ in 0..200 {
        let _ = feed_b.with_device(|d| {
            d.service_tun(now);
            d.service_udp(now);
            d.tick(now);
        });
        let _ = feed_a.with_device(|d| {
            d.service_tun(now);
            d.service_udp(now);
            d.tick(now);
        });
        now += 10;
        if tun_try_read(a_tun).as_deref() == Some(&probe) {
            got = true;
            break;
        }
    }
    assert!(got, "B → A payload must ride B's ROAMED endpoint");
}

/// 未认证的包**永不**改动任何东西：不是 WG 报文 / 形态对但索引与 tag 全错 /
/// 形态对但 MAC1 全错 / 短于最小 WG 报文 —— 全都不动 endpoint、不动会话，
/// 计入 `unknown_peer_drops`，且隧道照常工作。认证是唯一的入口条件。
#[test]
fn wg_unauthenticated_packets_never_move_an_endpoint() {
    let (sec_a, key_a) = key_pair(0xC1);
    let (sec_b, key_b) = key_pair(0xC2);
    let (feed_a, a_wg, a_tun, _a_dev) = roam_node(&sec_a, &key_b, VPN_B);
    let (feed_b, b_wg, b_tun, _b_dev) = roam_node(&sec_b, &key_a, VPN_A);
    let (a_addr, a_port) = sock_name(a_wg);
    let (b_addr, b_port) = sock_name(b_wg);
    feed_a.apply_endpoint(&key_b, b_addr, b_port).expect("endpoint");
    let mut now = sys::mono_ms();
    assert!(roam_pump(&feed_a, &feed_b, &mut now, 400), "precondition: established");

    let endpoint_a = roam_peer(&feed_a).endpoint.expect("A endpoint landed");
    let endpoint_b = roam_peer(&feed_b).endpoint.expect("B roamed");
    assert_eq!(endpoint_a, (b_addr, b_port));
    assert_eq!(endpoint_b, (a_addr, a_port));
    let st0 = feed_a.with_device(|d| d.stats()).expect("stats");
    let sess0 = feed_a.dataplane_status().expect("status").peers_with_session;

    // A stranger address no peer is registered at: four shapes, none of which
    // can authenticate.
    let stranger = ([203, 0, 113, 9], 40000);
    // (1) long enough, unknown message type
    let r = feed_a.with_device(|d| d.handle_udp(&[0xAAu8; 40], stranger, now)).expect("device");
    assert!(!r.peer_matched && r.wrote_tun == 0 && r.sent == 0);
    // (2) WG transport SHAPE (type 4, 32 B) with a bogus receiver index + tag
    let mut fake_data = [0u8; 32];
    fake_data[..4].copy_from_slice(&4u32.to_le_bytes());
    fake_data[4..8].copy_from_slice(&0x00AB_CDEFu32.to_le_bytes());
    let r = feed_a.with_device(|d| d.handle_udp(&fake_data, stranger, now)).expect("device");
    assert!(!r.peer_matched && r.wrote_tun == 0);
    // (3) WG initiation SHAPE (type 1, 148 B) with a bogus body (MAC1 fails)
    let mut fake_init = vec![0u8; 148];
    fake_init[..4].copy_from_slice(&1u32.to_le_bytes());
    let r = feed_a.with_device(|d| d.handle_udp(&fake_init, stranger, now)).expect("device");
    assert!(!r.peer_matched && r.wrote_tun == 0);
    // (4) shorter than the smallest WG message (keepalive, 32 B)
    let r = feed_a.with_device(|d| d.handle_udp(&[1u8; 31], stranger, now)).expect("device");
    assert!(!r.peer_matched && r.wrote_tun == 0);

    // NOTHING moved — not the endpoint, not the session; the stranger counts as
    // an unknown peer (never as a decrypt failure: no registered peer was
    // involved).
    assert_eq!(roam_peer(&feed_a).endpoint, Some(endpoint_a), "endpoint must not move");
    assert_eq!(roam_peer(&feed_b).endpoint, Some(endpoint_b));
    assert_eq!(feed_a.dataplane_status().expect("status").peers_with_session, sess0);
    let st1 = feed_a.with_device(|d| d.stats()).expect("stats");
    assert_eq!(st1.unknown_peer_drops, st0.unknown_peer_drops + 4);
    assert_eq!(st1.decrypt_errors, st0.decrypt_errors, "no registered peer was involved");
    assert_eq!(st1.rx_packets, st0.rx_packets, "a drop is not peer traffic");

    // …and the tunnel is untouched: the next honest frame flows A → B.
    let probe = probe_frame(VPN_A, VPN_B, b"after-garbage");
    tun_write(a_tun, &probe);
    let mut got = false;
    for _ in 0..200 {
        let _ = feed_a.with_device(|d| {
            d.service_tun(now);
            d.service_udp(now);
            d.tick(now);
        });
        let _ = feed_b.with_device(|d| {
            d.service_tun(now);
            d.service_udp(now);
            d.tick(now);
        });
        now += 10;
        if tun_try_read(b_tun).as_deref() == Some(&probe) {
            got = true;
            break;
        }
    }
    assert!(got, "the established session must keep carrying traffic");
}

/// 设备层录制的载体桩（生产实现是 `RelayWgCarrier`，设备 seam 完全相同）：
/// 只把设备交出的报文收下来，用来证明「这一包确实走了 relay」。
#[derive(Default)]
struct RecordingCarrier {
    sent: Mutex<Vec<Vec<u8>>>,
}

impl WgEgressCarrier for RecordingCarrier {
    fn send_datagram(&self, datagram: &[u8]) -> Result<(), String> {
        self.sent.lock().expect("carrier").push(datagram.to_vec());
        Ok(())
    }
    fn kind(&self) -> &'static str {
        "recording-stub"
    }
}

/// relay 不受 roaming 影响（上游形态）：relayed 入向把包连同**对端自己的
/// synthetic endpoint** 交给 wireguard-go（`wgproxy/bind/proxy.go:199` →
/// `ice_bind.go:378`），`SetEndpointFromPacket` 于是写回同一个值 = no-op；
/// 本层的载体入向按身份（公钥）匹配、**根本没有地址可采纳**，语义只会更严。
/// 于是：载体报文（认证的也一样）永不设置 endpoint；共享 socket 上的垃圾包
/// 永不设置 endpoint；roam 到 UDP 之后载体通道仍然挂着，egress 在下一次
/// `dispatch` 离开载体（上游 `SendBuffers` 读 `endpoint.val`），
/// `recycle_endpoint` 立刻回落 relay。
#[test]
fn wg_roaming_leaves_the_relay_carrier_intact() {
    let (sec_a, key_a) = key_pair(0xE1);
    let (sec_b, key_b) = key_pair(0xE2);
    // A: relay-only (no endpoint at all) + a recording carrier.
    let (feed_a, _a_wg, a_tun, _a_dev) = roam_node(&sec_a, &key_b, VPN_B);
    let carrier = Arc::new(RecordingCarrier::default());
    feed_a.attach_carrier(&key_b, carrier.clone()).expect("carrier attach");
    // B: a plain device whose endpoint is a bare socket the test can read — it
    // produces the peer's REAL WireGuard packets for us.
    let (feed_b, _b_wg, b_tun, _b_dev) = roam_node(&sec_b, &key_a, VPN_A);
    let sink = bound_lo_socket();
    let (s_addr, s_port) = sock_name(sink);
    feed_b.apply_endpoint(&key_a, s_addr, s_port).expect("B → sink");
    let init = tun_try_read(sink).expect("B's initiation on the sink");
    assert!(is_wg_datagram(&init) && init.len() == 148, "a real WG initiation");

    let mut now = sys::mono_ms();
    // (1) the initiation arrives over the RELAY carrier (identity-keyed).
    let r = feed_a.with_device(|d| d.handle_carrier(&init, &key_b, now)).expect("A device");
    assert!(r.peer_matched && r.sent >= 1, "A consumed it and answered");
    let p = roam_peer(&feed_a);
    assert_eq!(p.endpoint, None, "a carrier arrival never creates an endpoint");
    assert!(p.carrier_attached && p.on_carrier, "the relay stays the bearer");
    let s1 = feed_a.with_device(|d| d.stats()).expect("stats");
    assert_eq!(s1.carrier_takeovers, 1, "one UDP→relay episode");
    assert_eq!(s1.carrier_tx_packets, 1, "the handshake response left via the relay");
    assert_eq!(s1.direct_restores, 0);
    assert_eq!(s1.decrypt_errors, 0, "carrier traffic never counts as a decrypt failure");

    // (2) garbage on the SHARED socket from a stranger: no endpoint, relay kept.
    let mut garbage = [0u8; 32];
    garbage[..4].copy_from_slice(&4u32.to_le_bytes());
    let r = feed_a
        .with_device(|d| d.handle_udp(&garbage, ([203, 0, 113, 9], 40000), now))
        .expect("A device");
    assert!(!r.peer_matched);
    let p = roam_peer(&feed_a);
    assert_eq!(p.endpoint, None, "garbage never sets an endpoint");
    assert!(p.on_carrier, "the relay stays the bearer");
    let s2 = feed_a.with_device(|d| d.stats()).expect("stats");
    assert_eq!(s2.unknown_peer_drops, s1.unknown_peer_drops + 1);
    assert_eq!(s2.decrypt_errors, 0);
    assert_eq!(s2.direct_restores, 0);

    // (3) finish B's session through the lane (the carrier's write side does
    // this in production) so it starts emitting REAL authenticated packets.
    let resp = carrier.sent.lock().expect("carrier").last().expect("response").clone();
    let r = feed_b.with_device(|d| d.handle_carrier(&resp, &key_a, now)).expect("B device");
    assert!(r.peer_matched && r.sent >= 1, "B consumed the response and sent its keepalive");
    while tun_try_read(sink).is_some() {} // drain the keepalive
    let probe = probe_frame(VPN_B, VPN_A, b"roam-over-relay");
    tun_write(b_tun, &probe);
    assert_eq!(
        feed_b.with_device(|d| d.service_tun(now)).expect("B device"),
        1,
        "exactly the probe is encapsulated"
    );
    let data = tun_try_read(sink).expect("B's data packet on the sink");
    assert!(is_wg_datagram(&data) && data.len() == probe.len() + 32);

    // (4) the SAME peer over UDP: an authenticated DATA packet from an address A
    // has never seen. Upstream roams onto it (`receive.go:459`/`:515` →
    // `peer.SetEndpointFromPacket`, `peer.go:280-288`) — an inbound write of
    // `endpoint.val` ONLY: the relay lane stays attached, and the EGRESS leaves
    // it on the next send (`SendBuffers` reads `peer.endpoint.val`,
    // `peer.go:117-126`), which here is `dispatch`.
    let udp_sink = bound_lo_socket();
    let (n_addr, n_port) = sock_name(udp_sink);
    let newcomer = (n_addr, n_port);
    let r = feed_a.with_device(|d| d.handle_udp(&data, newcomer, now)).expect("A device");
    assert!(r.peer_matched, "the roamed packet authenticated on A");
    assert_eq!(r.wrote_tun, probe.len(), "…and it is the real probe");
    assert_eq!(tun_try_read(a_tun).as_deref(), Some(&probe[..]));
    let p = roam_peer(&feed_a);
    assert_eq!(p.endpoint, Some(newcomer), "roaming adopts the authenticated source");
    assert!(p.carrier_attached, "roaming does NOT detach the relay lane");
    assert!(p.on_carrier, "the adoption alone moves no egress — that is dispatch's call");
    let s3 = feed_a.with_device(|d| d.stats()).expect("stats");
    assert_eq!(s3.direct_restores, 0, "no egress happened yet");
    assert_eq!(s3.carrier_tx_packets, s2.carrier_tx_packets, "the lane is untouched");
    assert_eq!(
        s3.unknown_peer_drops,
        s2.unknown_peer_drops,
        "an authenticated stranger is not a drop"
    );

    // (4b) the egress follows the ROAMED endpoint (upstream `SendBuffers` reads
    // `peer.endpoint.val`): A's next outbound probe dispatches UDP-direct to the
    // newcomer's socket — NOT through the relay.
    let probe_udp = probe_frame(VPN_A, VPN_B, b"post-roam");
    tun_write(a_tun, &probe_udp);
    assert_eq!(
        feed_a.with_device(|d| d.service_tun(now)).expect("A device"),
        1,
        "exactly the post-roam probe is encapsulated"
    );
    let udp_data = tun_try_read(udp_sink).expect("A's probe left via the ROAMED endpoint");
    assert!(is_wg_datagram(&udp_data) && udp_data.len() == probe_udp.len() + 32);
    let p = roam_peer(&feed_a);
    assert!(!p.on_carrier, "the egress left the relay");
    let s3b = feed_a.with_device(|d| d.stats()).expect("stats");
    assert_eq!(s3b.direct_restores, 1, "relay → direct on the roamed endpoint");
    assert_eq!(
        s3b.carrier_tx_packets,
        s3.carrier_tx_packets,
        "the probe did NOT ride the relay"
    );
    unsafe { close(udp_sink) };

    // (5) ICE teardown (`recycle_endpoint`) → straight back to the relay.
    feed_a.recycle_endpoint(&key_b);
    let p = roam_peer(&feed_a);
    assert_eq!(p.endpoint, None, "endpoint recycled (fail-closed)");
    assert!(p.carrier_attached, "the lane outlives the UDP path");
    let probe2 = probe_frame(VPN_A, VPN_B, b"roam-back-to-relay");
    tun_write(a_tun, &probe2);
    assert_eq!(feed_a.with_device(|d| d.service_tun(now)).expect("A device"), 1);
    let s4 = feed_a.with_device(|d| d.stats()).expect("stats");
    assert_eq!(s4.carrier_takeovers, 2, "a NEW relay episode after the endpoint is gone");
    assert_eq!(s4.carrier_tx_packets, s3.carrier_tx_packets + 1, "the probe rode the relay");
    assert_eq!(
        carrier.sent.lock().expect("carrier").last().map(|d| d.len()),
        Some(probe2.len() + 32),
        "the last relayed datagram is the encapsulated probe"
    );
    unsafe { close(sink) };
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
    let mut reoffered = false;
    while now <= selected_at + 16_000 {
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
        // guard re-offer at ~+6.8 s (800 ms ticker after the disconnect):
        // the SAME stable credentials. The session was closed at Disconnected
        // (closeAgent, worker_ice.go:591-600), so there is no +12 s Failed
        // transition — renegotiation is the guard's job (guard.go:148-153).
        if !reoffered && disconnected_at.is_some() {
            reoffered = bus.seen().iter().rev().take(8).any(|f| {
                f.from == key_a && f.kind == PeerSignalKind::Offer && f.payload == first_offer_payload
            });
        }
        if reoffered && now > selected_at + 10_000 {
            break;
        }
    }
    let d = disconnected_at.expect("Disconnected must land at ~+6 s of silence");
    assert!(
        (selected_at + 6_000..=selected_at + 7_000).contains(&d),
        "Disconnected at ~+6 s (got +{} ms)",
        d - selected_at
    );
    assert_ne!(
        a.status_of(&key_b).state,
        PeerIceState::Failed,
        "closeAgent at Disconnected: no +12 s Failed transition follows"
    );
    assert!(
        reoffered,
        "the guard re-offers the SAME stable credentials after the disconnect"
    );

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
    // STAGE 2: B's egress is B's shared WILDCARD-bound mux socket, so its own
    // local address is `0.0.0.0:<port>`; the mirror property is about the PORT
    // A landed its endpoint on (that port IS the socket A sends to and B receives
    // WG data on).
    let b_egress = b.device_peer().egress_local.expect("B egress");
    assert_eq!(b_egress.0, [0, 0, 0, 0], "B's egress is the wildcard-bound shared mux");
    assert_eq!(
        a.status_of(&key_b).selected_remote.map(|(_, port)| port),
        Some(b_egress.1),
        "A's landed endpoint port == B's (new) shared mux port — the mirror property"
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
