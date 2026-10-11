// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N5c end-to-end tests: TWO full per-peer ICE orchestrators in one process
//! (the "device" and the "remote peer"), each with real loopback UDP sockets
//! taken through the `ProtectedUdpFdSource` seam, exchanging credentials and
//! candidates through an IN-PROCESS MOCK SIGNAL (the exact frame shapes the
//! signal channel carries: `"ufrag:pwd"` offers/answers, marshaled
//! candidates), pumping checks with an INJECTED clock — no sleeps anywhere.
//!
//! Proven here (the N5c acceptance core):
//! - dual-instance convergence: both peers reach Connected, the SELECTED
//!   PAIRS ARE CONSISTENT (each side's landed endpoint is the other's
//!   signaled candidate), and BOTH WG seams received the apply_endpoint call
//!   with exactly the peer's selected address (injectable WG seam);
//! - glare (both peers send OFFER simultaneously) converges through the N5b
//!   tie-breaker semantics (exactly one controlling agent survives);
//! - an unreachable candidate set → pair Failed → the peer is marked
//!   UNREACHABLE (never silently usable) → the N3-7 default-route gate keeps
//!   0.0.0.0/0 HELD;
//! - protected sockets: every fd flows through the provider
//!   (`taken == attempts`), and an EMPTY provider fails closed BEFORE any
//!   signal frame leaves (no candidates ⇒ no offer/answer);
//! - keepalive/disconnect state transitions are driven purely by the
//!   injected clock (Disconnected at +6 s of silence, Failed at +12 s —
//!   upstream `agent.go:22-24` numbers via `ice_session`).

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
    pub extern "C" fn napi_get_value_int32(_env: *mut c_void, _value: *mut c_void, _result: *mut i32) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(_env: *mut c_void, _value: *mut c_void, _result: *mut bool) -> i32 {
        0
    }
}

use std::sync::{Arc, Mutex};

use netbird_core::config;
use netbird_core::connector::{ShellNetworkConfig, WgPeerApplier, WgPeerEntry};
use netbird_core::ice::{Candidate, InterfaceAddr, ProtectedUdpFdSource, StaticInterfaces};
use netbird_core::peer_conn::{
    ice_ready_for_default_route, parse_ufrag_pwd, InlineGatherExec, PeerIceDeps,
    PeerIceOrchestrator, PeerIceState, PeerSignalKind, SignalExchange,
};

extern "C" {
    fn socket(domain: i32, ty: i32, protocol: i32) -> i32;
    fn close(fd: i32) -> i32;
}

const KEY_A: &str = "QUtFWV9BPT0="; // base64-ish peer identity for logs/records
const KEY_B: &str = "QUtFWV9CPT0=";
/// 死对端的凭证（合法 ice-char 形态，RFC 8445 §16 下限）。
const GHOST_CREDS: &str = "ghostghostghost:ABCDEFGHIJKLMNOPQRSTUVWXYZghost";

// ---------------------------------------------------------------------------
// harness
// ---------------------------------------------------------------------------

/// One in-flight signal frame, exactly the fields the real channel preserves
/// (`EncryptedMessage.key/remoteKey` + `Body{type,payload}`).
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
    fn push(&self, f: Frame) {
        self.queue.lock().expect("queue").push(f);
    }

    fn drain(&self) -> Vec<Frame> {
        self.queue.lock().expect("queue").drain(..).collect()
    }

    fn seen(&self) -> Vec<Frame> {
        self.seen.lock().expect("seen").clone()
    }
}

/// Mock signal: the orchestrator's sends land on the shared bus; the test
/// loop routes frames to the receiving orchestrator (`handle_signal`).
struct MockSignalEndpoint {
    me: &'static str,
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
            from: self.me.to_string(),
            to: to_key.to_string(),
            kind,
            payload: payload.to_string(),
        });
        Ok(())
    }
}

/// Injectable WG seam: records every endpoint landing (peer, addr, port) and
/// every N11 egress attach (peer; fd numbers are process-global, so only the
/// fact + ordering vs the endpoint landing are asserted).
///
/// STAGE 3 (official-alignment audit §2.7): the same seam can also play the
/// SHARED-SOCKET device — a real bound UDP socket it hands to the mux
/// ([`WgPeerApplier::wg_socket_fd`]), the read-half hand-over flag, and the
/// non-STUN datagrams the mux pushed straight into it
/// ([`WgPeerApplier::handle_udp_inbound`], upstream `ice_bind.go:257-285`).
#[derive(Default)]
struct RecordingWg {
    endpoints: Mutex<Vec<(String, [u8; 4], u16)>>,
    egress: Mutex<Vec<String>>,
    /// The "WG outer socket" this seam offers the shared mux (`None` = the
    /// pre-stage-3 seam with no socket of its own).
    sock: Mutex<Option<i32>>,
    /// Non-STUN datagrams the mux delivered straight to this seam.
    sink_calls: Mutex<Vec<([u8; 4], u16)>>,
    /// The shared-reader hand-over flag (the mux owns the socket's read half).
    shared_reader: Mutex<bool>,
}

impl RecordingWg {
    fn calls(&self) -> Vec<(String, [u8; 4], u16)> {
        self.endpoints.lock().expect("endpoints").clone()
    }

    fn with_own_socket() -> Self {
        let wg = RecordingWg::default();
        // a real bound loopback socket: the mux is built over THIS fd
        let fd = unsafe { socket(2, 2, 0) };
        assert!(fd >= 0, "socket() failed");
        let sa = netbird_core::sys::sockaddr_in::new([127, 0, 0, 1], 0);
        let rc = unsafe {
            netbird_core::sys::bind(fd, &sa, core::mem::size_of::<netbird_core::sys::sockaddr_in>() as u32)
        };
        assert_eq!(rc, 0, "bind() failed");
        *wg.sock.lock().expect("sock") = Some(fd);
        wg
    }

    fn sock_fd(&self) -> Option<i32> {
        *self.sock.lock().expect("sock")
    }

    fn sink_calls(&self) -> Vec<([u8; 4], u16)> {
        self.sink_calls.lock().expect("sink").clone()
    }

    fn shared_reader(&self) -> bool {
        *self.shared_reader.lock().expect("reader flag")
    }
}

impl Drop for RecordingWg {
    fn drop(&mut self) {
        if let Some(fd) = self.sock.lock().expect("sock").take() {
            unsafe { close(fd) };
        }
    }
}

impl WgPeerApplier for RecordingWg {
    fn apply_peers(&self, _peers: &[WgPeerEntry]) -> Result<(), String> {
        Ok(())
    }
    fn clear(&self) {
        self.endpoints.lock().expect("endpoints").clear();
        self.egress.lock().expect("egress").clear();
    }
    fn apply_endpoint(&self, pub_key_b64: &str, addr: [u8; 4], port: u16) -> Result<(), String> {
        self.endpoints.lock().expect("endpoints").push((pub_key_b64.to_string(), addr, port));
        Ok(())
    }
    fn attach_egress_socket(&self, pub_key_b64: &str, _raw_fd: i32) -> Result<(), String> {
        self.egress.lock().expect("egress").push(pub_key_b64.to_string());
        Ok(())
    }

    /// STAGE 3: the fd the shared mux is built over — the device's own socket.
    fn wg_socket_fd(&self) -> Option<i32> {
        self.sock_fd()
    }

    /// STAGE 3: the mux's reader owns the socket's read half from here on.
    fn set_shared_socket_reader(&self, on: bool) -> bool {
        let mut flag = self.shared_reader.lock().expect("reader flag");
        let changed = *flag != on;
        *flag = on;
        changed
    }

    /// STAGE 3: the mux hands non-STUN datagrams straight in (no session queue).
    fn handle_udp_inbound(&self, _datagram: &[u8], src: ([u8; 4], u16), _now_ms: u64) -> usize {
        self.sink_calls.lock().expect("sink").push(src);
        0
    }
}

/// Protected-UDP provider holding the ORIGINAL fds (sessions only ever touch
/// dups); the originals are closed exactly once, on drop.
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
            source.feed(fd).expect("feed");
        }
        FedSocks { source, raw_fds }
    }

    /// Feed `n` fresh loopback UDP fds LATER than construction — the device
    /// run 0048 timeline (session created ~2 ms before the first
    /// `VPN_ICE_FEED|fed` reaches the provider).
    fn feed_lo_sockets(&mut self, n: usize) {
        for _ in 0..n {
            let fd = unsafe { socket(2, 2, 0) };
            assert!(fd >= 0, "socket() failed");
            self.raw_fds.push(fd);
            self.source.feed(fd).expect("feed");
        }
    }

    fn taken(&self) -> u64 {
        self.source.taken()
    }
}

impl Drop for FedSocks {
    fn drop(&mut self) {
        for fd in self.raw_fds.drain(..) {
            unsafe { close(fd) };
        }
    }
}

/// One orchestrator wired for loopback: eth0@127.0.0.1 (passes the upstream
/// interface blacklist), fixed tie-breaker, mock signal, recording WG.
struct TestOrch {
    orch: PeerIceOrchestrator,
    socks: FedSocks,
    wg: Arc<RecordingWg>,
    me: &'static str,
}

impl TestOrch {
    fn new(me: &'static str, peers: &[&str], tie_breaker: u64, bus: Arc<SignalBus>) -> Self {
        TestOrch::new_with_sockets(me, peers, tie_breaker, bus, 8)
    }

    /// `socket_count = 0` models the device run 0048 race: the initiate beat
    /// runs while the protected ICE socket queue is still empty.
    fn new_with_sockets(
        me: &'static str,
        peers: &[&str],
        tie_breaker: u64,
        bus: Arc<SignalBus>,
        socket_count: usize,
    ) -> Self {
        let socks = FedSocks::with_lo_sockets(socket_count);
        let wg = Arc::new(RecordingWg::default());
        let mut orch = PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(StaticInterfaces(vec![InterfaceAddr {
                name: "eth0".into(),
                addr: [127, 0, 0, 1],
            }])),
            socks: socks.source.clone(),
            signal: Arc::new(MockSignalEndpoint { me, bus: bus.clone() }),
            wg: wg.clone(),
            tie_breaker: Some(tie_breaker),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        });
        let keys: Vec<String> = peers.iter().map(|p| p.to_string()).collect();
        // STAGE 1（官方对齐审计 §2.7）：本地候选收集已移到 worker 上异步跑。
        // 这个 harness 用**注入时钟**推进 pump（`run_once(now_ms)`，且从不等待
        // 真实时间），异步 worker 会与它赛跑，所以这里显式用确定性内联执行者
        // （`InlineGatherExec`：round 当场跑完、事件入队、同一拍取件）。异步
        // （线程）执行者本身由 `src/peer_conn.rs` 的
        // `many_peers_start_gathering_in_one_tick_without_a_budget` /
        // `black_holed_stun_never_blocks_the_pump` /
        // `candidates_are_delivered_as_each_interface_round_finishes` 覆盖。
        orch.set_gather_exec(Arc::new(InlineGatherExec));
        orch.set_peers(&keys);
        orch.set_signal_ready(true);
        TestOrch { orch, socks, wg, me }
    }

    /// STAGE 3 (official-alignment audit §2.7): the production shape — the WG seam
    /// OWNS the outer socket and the ONE shared mux is armed OVER it
    /// ([`PeerIceOrchestrator::arm_mux_from_wg_socket`], upstream
    /// `createOrUpdateMux`). The ICE provider is still fed, and must stay
    /// untouched: with a shared socket there is no pool socket to take.
    fn new_sharing_wg_socket(
        me: &'static str,
        peers: &[&str],
        tie_breaker: u64,
        bus: Arc<SignalBus>,
    ) -> Self {
        let socks = FedSocks::with_lo_sockets(4);
        let wg = Arc::new(RecordingWg::with_own_socket());
        let mut orch = PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(StaticInterfaces(vec![InterfaceAddr {
                name: "eth0".into(),
                addr: [127, 0, 0, 1],
            }])),
            socks: socks.source.clone(),
            signal: Arc::new(MockSignalEndpoint { me, bus: bus.clone() }),
            wg: wg.clone(),
            tie_breaker: Some(tie_breaker),
            fixed_local_port: None,
            advertised_candidates: Vec::new(),
        });
        orch.set_gather_exec(Arc::new(InlineGatherExec));
        let keys: Vec<String> = peers.iter().map(|p| p.to_string()).collect();
        orch.set_peers(&keys);
        orch.set_signal_ready(true);
        let (mux, replaced) = orch
            .arm_mux_from_wg_socket()
            .expect("stage-3 mux over the seam's own socket");
        assert!(!replaced, "first arm");
        assert_eq!(
            mux.local_port(),
            bound_port(wg.sock_fd().expect("the seam's socket")),
            "the mux IS the WG seam's socket"
        );
        assert_eq!(socks.taken(), 0, "the ICE provider was never touched");
        TestOrch { orch, socks, wg, me }
    }

    fn status_of(&self, peer: &str) -> netbird_core::peer_conn::PeerIceStatus {
        self.orch.peer_status(peer).expect("peer entry")
    }
}

/// Route all queued frames to their recipient orchestrator; frames for an
/// absent recipient are dropped (a dead peer IS the failure-path test).
fn deliver(bus: &Arc<SignalBus>, now: u64, a: &mut TestOrch, mut b: Option<&mut TestOrch>) {
    let frames = bus.drain();
    for f in frames {
        // record for wire-shape assertions (only deliverable directions)
        bus.seen.lock().expect("seen").push(f.clone());
        if f.to == a.me {
            a.orch.handle_signal(&f.from, f.kind, &f.payload, now).expect("A recv");
        } else if let Some(b) = b.as_deref_mut() {
            if f.to == b.me {
                b.orch.handle_signal(&f.from, f.kind, &f.payload, now).expect("B recv");
            }
        }
        // else: dropped (ghost peer)
    }
}

/// Pump both orchestrators on the injected clock until `cond` holds.
fn pump_until(
    bus: &Arc<SignalBus>,
    a: &mut TestOrch,
    b: &mut TestOrch,
    now: &mut u64,
    deadline_ms: u64,
    cond: impl Fn(&TestOrch, &TestOrch) -> bool,
) -> bool {
    while *now <= deadline_ms {
        let _ = a.orch.run_once(*now);
        let _ = b.orch.run_once(*now);
        *now += 10;
        deliver(bus, *now, a, Some(b));
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

/// Build the N3-7 shell snapshot through the real gate with the ICE view
/// folded in — the exact formula `apply_update` uses.
fn gated_config(
    tunnel_ready: bool,
    ice: &netbird_core::peer_conn::IceOrchestratorSummary,
) -> ShellNetworkConfig {
    let map = netbird_core::network_map::NetworkMap {
        serial: 1,
        peer: None,
        peers: vec![netbird_core::network_map::PeerInfo {
            wg_pub_key: KEY_B.into(),
            allowed_ips: vec![config::Route { addr: [10, 30, 30, 1], prefix_len: 32 }],
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

/// THE N5c core: two orchestrators, one loopback, mock-signal exchange →
/// both Connected, selected pairs consistent, and BOTH WG seams received
/// the endpoint of the peer's selected address with the right arguments.
#[test]
fn dual_peer_signal_exchange_reaches_connected_and_lands_wg_endpoints() {
    let bus = Arc::new(SignalBus::default());
    let mut a = TestOrch::new(KEY_A, &[KEY_B], 0x1111, bus.clone());
    let mut b = TestOrch::new(KEY_B, &[KEY_A], 0x2222, bus.clone());
    // B is the pure answerer this round (initiator policy): A offers, B
    // answers — the deterministic role split; glare is its own test below.
    b.orch.set_initiator(KEY_A, false);

    let mut now = 1000u64;
    let converged = pump_until(&bus, &mut a, &mut b, &mut now, 30_000, |a, b| {
        a.status_of(KEY_B).state == PeerIceState::Connected
            && b.status_of(KEY_A).state == PeerIceState::Connected
    });
    assert!(converged, "both orchestrators must reach Connected within the sim deadline");

    // Consistency: each side's landed endpoint is the peer's signaled
    // candidate (the selected pairs mirror each other).
    let b_cands = candidate_frames(&bus, KEY_B);
    let a_cands = candidate_frames(&bus, KEY_A);
    assert!(!b_cands.is_empty() && !a_cands.is_empty(), "candidates must have been trickled");
    let b_local_port = b_cands[0].port;
    let a_local_port = a_cands[0].port;

    let sa = a.status_of(KEY_B);
    let sb = b.status_of(KEY_A);
    assert_eq!(sa.selected_remote, Some(([127, 0, 0, 1], b_local_port)), "A landed B's address");
    assert_eq!(sb.selected_remote, Some(([127, 0, 0, 1], a_local_port)), "B landed A's address");
    assert!(sa.endpoint_applied && sb.endpoint_applied, "endpoint flags must be set");
    assert!(sa.reachable && sb.reachable, "both peers are reachable");

    // WG seam: exactly one apply_endpoint per side, with the peer's key and
    // the selected address as arguments (injectable-seam assertion).
    assert_eq!(a.wg.calls(), vec![(KEY_B.to_string(), [127, 0, 0, 1], b_local_port)]);
    assert_eq!(b.wg.calls(), vec![(KEY_A.to_string(), [127, 0, 0, 1], a_local_port)]);
    // N11: the egress attach (dup of the selected LOCAL socket) precedes the
    // endpoint landing and happens exactly once per peer — the landing fires
    // the handshake, which must leave via the selected path.
    assert_eq!(*a.wg.egress.lock().expect("egress"), vec![KEY_B.to_string()]);
    assert_eq!(*b.wg.egress.lock().expect("egress"), vec![KEY_A.to_string()]);

    // Role decision: the offerer is controlling, the answerer controlled.
    assert_eq!(sa.controlling, Some(true), "A offered → controlling");
    assert_eq!(sb.controlling, Some(false), "B answered → controlled");

    // Wire shapes: offers are "ufrag:pwd" (validated), candidates marshal.
    let offers: Vec<Frame> =
        bus.seen().into_iter().filter(|f| f.kind == PeerSignalKind::Offer).collect();
    assert_eq!(offers.len(), 1, "exactly one OFFER (A initiates, B answers)");
    assert_eq!(offers[0].from, KEY_A);
    assert_eq!(offers[0].to, KEY_B);
    let creds = parse_ufrag_pwd(&offers[0].payload).expect("offer payload is ufrag:pwd");
    assert_eq!(creds.ufrag.len(), 16, "generated ufrag follows the upstream 16-char rule");
    assert_eq!(creds.pwd.len(), 32, "generated pwd follows the upstream 32-char rule");
    let answers: Vec<Frame> =
        bus.seen().into_iter().filter(|f| f.kind == PeerSignalKind::Answer).collect();
    assert_eq!(answers.len(), 1, "exactly one ANSWER (B → A)");
    assert_eq!(answers[0].from, KEY_B);
    assert!(parse_ufrag_pwd(&answers[0].payload).is_ok(), "answer payload is ufrag:pwd");

    // Summary + default-route gate input.
    let summary = a.orch.summary();
    assert_eq!(summary.peers, 1);
    assert_eq!(summary.connected, 1);
    assert_eq!(summary.endpoints_applied, 1);
    assert_eq!(summary.reachable, 1);
    assert!(ice_ready_for_default_route(&summary), "a reachable peer arms the ICE gate input");

    // Protected sockets: ONE per peer — the gather's bound socket is handed to
    // the session and adopted there (0052: no second socket, no closed socket
    // behind the advertised port), nothing left queued.
    assert_eq!(a.socks.taken(), 1, "A: the adopted gather socket");
    assert_eq!(b.socks.taken(), 1, "B: the adopted gather socket");
}

// ---------------------------------------------------------------------------
// STAGE 2 (official-alignment audit §2.7): ONE mux per connector, MANY peers
// ---------------------------------------------------------------------------

/// One step of a many-orchestrator loop: pump every orchestrator, then deliver
/// every queued frame to EVERY orchestrator that has the sender as a peer (the
/// key-addressed signal map, generalized past two sides). Every frame is
/// recorded in `seen`, exactly like the two-side `deliver`.
fn step_many(bus: &Arc<SignalBus>, now: u64, orchs: &mut [&mut TestOrch]) {
    for o in orchs.iter_mut() {
        let _ = o.orch.run_once(now);
    }
    let frames = bus.drain();
    bus.seen.lock().expect("seen").extend(frames.iter().cloned());
    for f in frames {
        for o in orchs.iter_mut() {
            if o.orch.peer_keys().iter().any(|k| k == &f.from) {
                let _ = o.orch.handle_signal(&f.from, f.kind, &f.payload, now);
            }
        }
    }
}

/// A minimal loopback STUN server: answers every Binding Request with a Binding
/// Success carrying XOR-MAPPED-ADDRESS (`mapped`) — the fixture that makes the
/// shared srflx cache observable (RFC 5389 §15.2/§15.4 encoding).
fn spawn_stun_responder() -> (u16, Arc<std::sync::atomic::AtomicBool>, std::thread::JoinHandle<()>) {
    use netbird_core::stun::{BINDING_SUCCESS, MAGIC_COOKIE};
    let sock = std::net::UdpSocket::bind("127.0.0.1:0").expect("stun sink");
    let port = sock.local_addr().expect("sink addr").port();
    sock.set_read_timeout(Some(std::time::Duration::from_millis(200))).expect("timeout");
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = stop.clone();
    let handle = std::thread::spawn(move || {
        let mut buf = [0u8; 512];
        while !flag.load(std::sync::atomic::Ordering::Relaxed) {
            let Ok((n, from)) = sock.recv_from(&mut buf) else { continue };
            let req = &buf[..n];
            if req.len() < 20 {
                continue;
            }
            let mapped_addr: [u8; 4] = [203, 0, 113, 9];
            let mapped_port: u16 = 4444;
            let mut body: Vec<u8> = Vec::new();
            body.extend_from_slice(&0x0020u16.to_be_bytes()); // XOR-MAPPED-ADDRESS
            body.extend_from_slice(&8u16.to_be_bytes());
            body.push(0x00);
            body.push(0x01);
            body.extend_from_slice(&(mapped_port ^ (MAGIC_COOKIE >> 16) as u16).to_be_bytes());
            for (i, b) in mapped_addr.iter().enumerate() {
                body.push(b ^ (MAGIC_COOKIE >> (24 - 8 * i)) as u8);
            }
            let mut msg: Vec<u8> = Vec::new();
            msg.extend_from_slice(&BINDING_SUCCESS.to_be_bytes());
            msg.extend_from_slice(&(body.len() as u16).to_be_bytes());
            msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
            msg.extend_from_slice(&req[8..20]); // echo the transaction id
            msg.extend_from_slice(&body);
            let _ = sock.send_to(&msg, from);
        }
    });
    (port, stop, handle)
}

/// TWO connector-shaped instances, one shared mux EACH, MANY peers: A holds
/// `N` peers (B0..B{N-1}) and each B_i holds one peer (A) — the 23-peer PC shape
/// of the relay report, reduced to what fits a loopback test.
///
/// Stage-2 properties this pins:
/// * every host candidate of EVERY peer carries the ONE mux port, emitted in
///   the very first tick (no STUN wait, upstream `gatherCandidatesLocalUDPMux`);
/// * the whole connector plane consumes exactly ONE protected socket;
/// * one loopback STUN round serves all N peers (the 25 s srflx cache + in-flight
///   de-duplication, upstream `GetXORMappedAddr`): all N srflx candidates carry
///   the SAME mapped address, from ONE request;
/// * all N negotiations run concurrently over that one socket and demux
///   correctly by ufrag (every pair reaches Connected).
#[test]
fn one_mux_carries_many_concurrent_peers_and_one_stun_round_serves_them_all() {
    const N: usize = 6;
    let (stun_port, stop, responder) = spawn_stun_responder();
    let bus = Arc::new(SignalBus::default());

    // `TestOrch::new` takes a `&'static str` identity (the harness treats peer
    // keys as compile-time literals); these N keys are generated, so they are
    // leaked once for the test's lifetime.
    let peer_keys: &'static [String] = Box::leak(
        (0..N).map(|i| format!("UEtFWV9CMHt9P{i}")).collect::<Vec<String>>().into_boxed_slice(),
    );
    let refs: Vec<&'static str> = peer_keys.iter().map(|k| k.as_str()).collect();
    let mut a = TestOrch::new(KEY_A, &refs, 0x1111, bus.clone());
    // One mux per connector, with the loopback STUN server: every peer shares
    // this single socket AND its srflx cache.
    assert_eq!(a.orch.set_stuns(&[format!("stun:127.0.0.1:{stun_port}")]), 1);
    // The B side: one connector per peer, one peer each.
    let mut bees: Vec<TestOrch> =
        refs.iter().map(|k| TestOrch::new(k, &[KEY_A], 0x2222, bus.clone())).collect();
    for b in bees.iter_mut() {
        b.orch.set_initiator(KEY_A, false); // deterministic: A offers, B_i answers
    }

    let mut now = 1000u64;
    {
        let mut all: Vec<&mut TestOrch> = Vec::new();
        all.push(&mut a);
        for b in bees.iter_mut() {
            all.push(b);
        }
        step_many(&bus, now, &mut all);
    }
    now += 10;

    // ONE tick: every peer of A has its host candidate, all carrying the SAME
    // mux port, and A's provider handed out exactly ONE socket for all of them.
    let mux_port = a.orch.ice_mux().expect("A's shared mux").local_port();
    let all_a_cands = candidate_frames(&bus, KEY_A);
    let host: Vec<Candidate> = all_a_cands
        .iter()
        .filter(|c| c.typ == netbird_core::ice::CandidateType::Host)
        .cloned()
        .collect();
    assert_eq!(host.len(), N, "one host candidate per peer, in the first tick: {host:?}");
    assert!(
        host.iter().all(|c| c.port == mux_port),
        "every host candidate carries the ONE mux port {mux_port}"
    );
    assert_eq!(a.socks.taken(), 1, "the whole connector plane takes ONE socket");
    // …and the srflx half of the same tick: the ONE STUN round served all N
    // peers through the shared cache.
    let stats = a.orch.ice_mux().expect("mux").stats();
    assert_eq!(stats.xor_misses, 1, "ONE STUN request for {N} peers (in-flight de-dup)");
    assert_eq!(stats.xor_filled, 1);
    assert_eq!(stats.xor_hits, (N - 1) as u64, "the other {N}-1 peers are served from the cache");

    // Pump to convergence: all N negotiations concurrently over that socket.
    let mut all_connected = false;
    while now <= 40_000 {
        {
            let mut all: Vec<&mut TestOrch> = Vec::new();
            all.push(&mut a);
            for b in bees.iter_mut() {
                all.push(b);
            }
            step_many(&bus, now, &mut all);
        }
        now += 10;
        all_connected = a.orch.summary().connected == N
            && bees.iter().all(|b| b.orch.summary().connected == 1);
        if all_connected {
            break;
        }
    }
    assert!(
        all_connected,
        "all {N} peers on A's one mux must reach Connected concurrently: A={:?} B={:?}",
        a.orch.summary(),
        bees.iter().map(|b| b.orch.summary().connected).collect::<Vec<_>>()
    );

    // The srflx candidates: every peer got the ONE cached mapping, with the
    // shared mux port as its base.
    let srflx: Vec<Candidate> = candidate_frames(&bus, KEY_A)
        .into_iter()
        .filter(|c| c.typ == netbird_core::ice::CandidateType::Srflx)
        .collect();
    assert_eq!(srflx.len(), N, "every peer got the shared mapping as a srflx candidate");
    assert!(
        srflx.iter().all(|c| c.address == "203.0.113.9" && c.port == 4444),
        "all srflx candidates carry the SAME mapped address: {srflx:?}"
    );
    assert!(
        srflx.iter().all(|c| c.related_port == Some(mux_port)),
        "…with the shared mux port as their base (rport)"
    );

    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = responder.join();
}

/// Glare: BOTH peers initiate (both send OFFER, both start controlling).
/// The signal channel carries no role field, so the conflict is repaired by
/// the N5b tie-breaker semantics (RFC 8445 §7.3.1.1/§7.2.5.1): the larger
/// tie-breaker retains controlling, the smaller yields — and the pair still
/// converges with consistent selection and endpoint landing.
#[test]
fn both_offer_glare_converges_via_tiebreaker() {
    let bus = Arc::new(SignalBus::default());
    let mut a = TestOrch::new(KEY_A, &[KEY_B], 0xFFFF_FFFF_FFFF_0000, bus.clone()); // big tb
    let mut b = TestOrch::new(KEY_B, &[KEY_A], 1, bus.clone()); // small tb — must yield

    let mut now = 2000u64;
    let converged = pump_until(&bus, &mut a, &mut b, &mut now, 40_000, |a, b| {
        a.status_of(KEY_B).state == PeerIceState::Connected
            && b.status_of(KEY_A).state == PeerIceState::Connected
    });
    assert!(converged, "glare must converge, not livelock");

    // Exactly one controlling agent survives, and it is the bigger one.
    let sa = a.status_of(KEY_B);
    let sb = b.status_of(KEY_A);
    assert_eq!(sa.controlling, Some(true), "larger tie-breaker retains controlling");
    assert_eq!(sb.controlling, Some(false), "smaller tie-breaker switched to controlled");

    // Two OFFERs crossed (glare), no ANSWER needed — credentials completed
    // through the two offers.
    let offers = bus.seen().iter().filter(|f| f.kind == PeerSignalKind::Offer).count();
    assert_eq!(offers, 2, "glare: both peers offered");

    // Selection stays consistent and both endpoints landed.
    let b_local_port = candidate_frames(&bus, KEY_B)[0].port;
    let a_local_port = candidate_frames(&bus, KEY_A)[0].port;
    assert_eq!(sa.selected_remote, Some(([127, 0, 0, 1], b_local_port)));
    assert_eq!(sb.selected_remote, Some(([127, 0, 0, 1], a_local_port)));
    assert_eq!(a.wg.calls(), vec![(KEY_B.to_string(), [127, 0, 0, 1], b_local_port)]);
    assert_eq!(b.wg.calls(), vec![(KEY_A.to_string(), [127, 0, 0, 1], a_local_port)]);
    assert!(sa.reachable && sb.reachable);
}

/// Failure path: the remote peer answers but its candidate is unreachable
/// (bound socket, then closed). Checks retransmit to exhaustion (RFC 8445
/// §7.2.5.2.3) → pair Failed → peer marked UNREACHABLE (never silently
/// usable) → the N3-7 default-route gate keeps 0.0.0.0/0 HELD.
#[test]
fn unreachable_candidate_fails_peer_and_default_route_stays_held() {
    let bus = Arc::new(SignalBus::default());
    // Only A exists; frames to KEY_B are dropped by the router (dead peer).
    let mut a = TestOrch::new(KEY_A, &[KEY_B], 0x1111, bus.clone());

    // A ghost peer: real credentials + a candidate whose socket we close —
    // the candidate is signaled but can never answer a check.
    let dead = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind");
    let addr = dead.local_addr().expect("addr");
    let (ip, port) = match addr {
        std::net::SocketAddr::V4(v4) => (v4.ip().octets(), v4.port()),
        _ => panic!("ipv4 only"),
    };
    drop(dead); // the port goes dark — the candidate is unreachable

    let mut now = 1000u64;
    // A initiates first (gather + offer + its own candidates).
    let _ = a.orch.run_once(now);
    now += 10;
    deliver(&bus, now, &mut a, None);
    // The ghost answers with an OFFER (credentials) and the dead candidate.
    a.orch.handle_signal(KEY_B, PeerSignalKind::Offer, GHOST_CREDS, now).expect("ghost offer");
    let dead_cand = Candidate::host_candidate(ip, port);
    a.orch
        .handle_signal(KEY_B, PeerSignalKind::Candidate, &dead_cand.marshal(), now)
        .expect("ghost candidate");
    assert_eq!(a.status_of(KEY_B).state, PeerIceState::Checking, "checks started");

    // Pump through the full retransmit schedule (7 × 500 ms) + margin.
    let failed = loop {
        let _ = a.orch.run_once(now);
        now += 50;
        if now > 20_000 {
            break false;
        }
        if a.status_of(KEY_B).state == PeerIceState::Failed {
            break true;
        }
    };
    assert!(failed, "the unreachable peer must reach Failed within the sim deadline");

    let st = a.status_of(KEY_B);
    assert_eq!(st.state, PeerIceState::Failed);
    assert_eq!(st.selected_remote, None, "a failed pair must never be treated as selected");
    assert!(!st.endpoint_applied, "no endpoint may land on a failed peer");
    assert!(!st.reachable, "failed peer is UNREACHABLE — never silently usable");
    assert!(a.wg.calls().is_empty(), "the WG seam must see NO endpoint call for a failed peer");

    // Orchestrator summary: 1 peer, 1 failed, 0 reachable → gate input false.
    let summary = a.orch.summary();
    assert_eq!(summary.peers, 1);
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.reachable, 0);
    assert!(!ice_ready_for_default_route(&summary), "no reachable peer → ICE never arms the gate");

    // N3-7 linkage through the REAL gate: even a tunnel-ready WG seam keeps
    // the default route HELD, and the held route is not exported.
    let snap = gated_config(true, &summary);
    assert!(!snap.default_route_allowed);
    assert_eq!(snap.default_route_reason, "default-route-held:data-plane-not-ready");
    assert!(
        !snap.routes.iter().any(|r| r.network == "0.0.0.0/0"),
        "0.0.0.0/0 must be stripped while no peer is reachable"
    );
    let json = snap.to_json();
    assert!(
        json.contains("\"default_route\":{\"allowed\":false,\"reason\":\"default-route-held:data-plane-not-ready\"}"),
        "{json}"
    );
    assert!(!json.contains("{\"network\":\"0.0.0.0/0\""), "{json}");
}

/// Keepalive/disconnect timers are driven ONLY by the injected clock:
/// after the remote goes silent, Disconnected lands at +6 s of no valid
/// inbound STUN and Failed at +12 s (upstream agent.go:22-24 numbers) —
/// never earlier, no wall-clock sleeps anywhere.
#[test]
fn disconnect_and_failed_transitions_follow_the_injected_clock() {
    let bus = Arc::new(SignalBus::default());
    let mut a = TestOrch::new(KEY_A, &[KEY_B], 0x1111, bus.clone());
    let mut b = TestOrch::new(KEY_B, &[KEY_A], 0x2222, bus.clone());

    let mut now = 1000u64;
    let converged = pump_until(&bus, &mut a, &mut b, &mut now, 30_000, |a, b| {
        a.status_of(KEY_B).state == PeerIceState::Connected
            && b.status_of(KEY_A).state == PeerIceState::Connected
    });
    assert!(converged, "precondition: both Connected");
    let selected_at = now;
    assert_eq!(a.status_of(KEY_B).state, PeerIceState::Connected);

    // B goes silent: no more pumps, no more deliveries. Only A keeps
    // running on the injected clock.
    let mut disconnected_at = None;
    let mut failed_at = None;
    while now <= selected_at + 20_000 {
        let _ = a.orch.run_once(now);
        now += 100;
        let st = a.status_of(KEY_B);
        if disconnected_at.is_none()
            && now > selected_at + 6000
            && st.state == PeerIceState::Disconnected
        {
            disconnected_at = Some(now);
        }
        if now <= selected_at + 6000 {
            assert_ne!(
                st.state,
                PeerIceState::Disconnected,
                "Disconnected must not fire before +6 s of silence (now={now})"
            );
        }
        if failed_at.is_none() && now > selected_at + 12_000 && st.state == PeerIceState::Failed {
            failed_at = Some(now);
        }
        if failed_at.is_some() {
            break;
        }
    }
    let d = disconnected_at.expect("Disconnected must arrive after +6 s of silence");
    assert!(
        (selected_at + 6000..=selected_at + 7000).contains(&d),
        "Disconnected must land at ~+6 s (got +{} ms)",
        d - selected_at
    );
    let f = failed_at.expect("Failed must arrive after +12 s of silence");
    assert!(
        (selected_at + 12_000..=selected_at + 13_500).contains(&f),
        "Failed must land at ~+12 s (got +{} ms)",
        f - selected_at
    );

    // The timed-out peer is unreachable and the gate stays held.
    let summary = a.orch.summary();
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.reachable, 0);
    assert!(!ice_ready_for_default_route(&summary));
    let snap = gated_config(true, &summary);
    assert!(!snap.default_route_allowed);
    assert_eq!(snap.default_route_reason, "default-route-held:data-plane-not-ready");
}

/// 真机 20260913 H1/H2 回归：signal 流断掉的一侧不得冻结其 ICE 会话泵。
/// 断流前双方已 Connected（WG egress 已挂）：A 断流后——
/// ① A 仍诚实应答 B 的 keepalive（B 不掉 Disconnected，数据面不陪葬）；
/// ② A 的信令活动（发起/重发/flush）仍被 `signal_ready` 门住，零帧外出；
/// ③ B 彻底静默时，A 的 keepalive/断连计时器仍按注入时钟前进
///   （+6s Disconnected、+12s Failed）——修复前 A 会永远冻结在 Connected
///   （12:00 主机侧快照 `ice{connected:1}` 的形态），对端反而被误判；
/// ④ signal 恢复后，冷却期满 A 以全新凭证重新发起（renegotiation armed）。
#[test]
fn ice_session_pump_survives_signal_outage_and_timers_stay_honest() {
    let bus = Arc::new(SignalBus::default());
    let mut a = TestOrch::new(KEY_A, &[KEY_B], 0x1111, bus.clone());
    let mut b = TestOrch::new(KEY_B, &[KEY_A], 0x2222, bus.clone());
    b.orch.set_initiator(KEY_A, false);

    let mut now = 1000u64;
    let converged = pump_until(&bus, &mut a, &mut b, &mut now, 30_000, |a, b| {
        a.status_of(KEY_B).state == PeerIceState::Connected
            && b.status_of(KEY_A).state == PeerIceState::Connected
    });
    assert!(converged, "precondition: both Connected");
    let connected_at = now;

    // ① A 的 signal 流断（Broken → set_signal_ready(false) 的生产行为）。
    a.orch.set_signal_ready(false);
    while now <= connected_at + 8_000 {
        let _ = a.orch.run_once(now);
        let _ = b.orch.run_once(now);
        now += 50;
        deliver(&bus, now, &mut a, Some(&mut b));
        let sb = b.status_of(KEY_A);
        assert_ne!(
            sb.state,
            PeerIceState::Disconnected,
            "A must keep answering B's checks while A's signal link is down"
        );
        assert!(sb.endpoint_applied, "B's data plane must not die with A's signal link");
    }

    // ② 断流期间 A 零信令帧外出（门仍生效）。
    let frames_while_down = bus.seen().iter().filter(|f| f.from == KEY_A).count();
    let _ = a.orch.run_once(now);
    deliver(&bus, now, &mut a, Some(&mut b));
    assert_eq!(
        frames_while_down,
        bus.seen().iter().filter(|f| f.from == KEY_A).count(),
        "no signaling frames may leave while signal_ready=false"
    );

    // ③ B 彻底静默：A 的计时器仍前进（不再冻结在 Connected）。
    let silence_at = now;
    let mut failed_at = None;
    while now <= silence_at + 16_000 {
        let _ = a.orch.run_once(now);
        now += 50;
        let st = a.status_of(KEY_B);
        if now <= silence_at + 6_000 {
            assert_ne!(st.state, PeerIceState::Disconnected, "no premature disconnect");
        }
        if failed_at.is_none() && now > silence_at + 12_000 && st.state == PeerIceState::Failed {
            failed_at = Some(now);
        }
        if failed_at.is_some() {
            break;
        }
    }
    assert!(
        failed_at.is_some(),
        "A must reach Failed on the injected clock while its signal is down \
         (the frozen-session bug kept such a peer at Connected forever)"
    );

    // ④ signal 恢复：冷却期满，A 以全新凭证重新发起。
    a.orch.set_signal_ready(true);
    let first_offer = bus
        .seen()
        .iter()
        .find(|f| f.kind == PeerSignalKind::Offer && f.from == KEY_A)
        .map(|f| f.payload.clone())
        .expect("the original offer was signaled");
    let deadline = now + 12_000;
    let mut reoffered = None;
    while now <= deadline {
        let _ = a.orch.run_once(now);
        now += 50;
        deliver(&bus, now, &mut a, None); // B 仍缺席
        if let Some(f) = bus
            .seen()
            .iter()
            .rev()
            .find(|f| f.kind == PeerSignalKind::Offer && f.from == KEY_A)
        {
            if f.payload != first_offer {
                reoffered = Some(f.payload.clone());
                break;
            }
        }
    }
    let fresh = reoffered
        .expect("after the signal link returns, A must re-initiate with FRESH credentials");
    assert_ne!(
        parse_ufrag_pwd(&fresh).expect("fresh offer wire form"),
        parse_ufrag_pwd(&first_offer).expect("original offer wire form"),
        "the re-initiation must carry NEW credentials (a new negotiation, not a replay)"
    );
}

/// 真机 20260913 H3 回归：对端仍持有旧会话、不再应答（收不到
/// Answer/候选）时，本端此前会**永久沉默**——会话永不 `start()`，
/// `want_initiate` 要 `session.is_none()`、`want_locals` 要
/// `!locals_done`，谁都不再触发，gather 一次后死寂（池填满、无
/// `locals-gathered`，只有对端重启才救活）。修复：会话创建后
/// [`netbird_core::peer_conn::START_DEADLINE_MS`] 内仍未 start → 拆除、
/// 冷却、以全新凭证重新发起（上游 handshaker 超时重启语义）。
#[test]
fn zombie_negotiation_times_out_and_reinitiates_with_fresh_credentials() {
    let bus = Arc::new(SignalBus::default());
    // B 从不存在：A 的 OFFER/候选全部发进虚空（对端持旧会话、永不回答）。
    let mut a = TestOrch::new(KEY_A, &[KEY_B], 0x1111, bus.clone());

    let mut now = 1000u64;
    let _ = a.orch.run_once(now);
    now += 10;
    deliver(&bus, now, &mut a, None);
    let first_offer = bus
        .seen()
        .iter()
        .find(|f| f.kind == PeerSignalKind::Offer)
        .map(|f| f.payload.clone())
        .expect("initial offer");
    assert_eq!(a.status_of(KEY_B).state, PeerIceState::Idle, "no answer → never leaves Idle");

    // 截止前：不拆除；周期重发只复用同一凭证（signaled 母本去重）。
    while now < 1000 + netbird_core::peer_conn::START_DEADLINE_MS - 500 {
        let _ = a.orch.run_once(now);
        now += 100;
        deliver(&bus, now, &mut a, None);
    }
    assert_eq!(
        a.status_of(KEY_B).state,
        PeerIceState::Idle,
        "no teardown before the start deadline"
    );
    let distinct_offers: std::collections::HashSet<String> = bus
        .seen()
        .iter()
        .filter(|f| f.kind == PeerSignalKind::Offer)
        .map(|f| f.payload.clone())
        .collect();
    assert_eq!(distinct_offers.len(), 1, "pre-deadline re-signals reuse the SAME offer");

    // 截止后：拆除 + 冷却 + 全新凭证重新发起（自愈，无需对端重启）。
    let mut fresh = None;
    while now <= 1000 + netbird_core::peer_conn::START_DEADLINE_MS + 15_000 {
        let _ = a.orch.run_once(now);
        now += 100;
        deliver(&bus, now, &mut a, None);
        if let Some(f) = bus.seen().iter().rev().find(|f| f.kind == PeerSignalKind::Offer) {
            if f.payload != first_offer {
                fresh = Some(f.payload.clone());
                break;
            }
        }
    }
    let fresh = fresh.expect("a FRESH offer must be re-initiated after the start deadline");
    assert_ne!(
        parse_ufrag_pwd(&fresh).expect("fresh offer wire form"),
        parse_ufrag_pwd(&first_offer).expect("original offer wire form"),
        "the zombie teardown must re-initiate with NEW credentials"
    );
    assert_eq!(
        a.socks.taken(),
        1,
        "STAGE 2: ONE shared mux socket for the whole ICE plane — the re-initiated \
         negotiation reuses it instead of taking a fresh per-session socket"
    );
    assert!(
        a.orch.ice_mux().is_some(),
        "the shared mux outlived the torn-down session"
    );
}

// ---------------------------------------------------------------------------
// device run 0048 regressions: the ICE socket supply arriving AFTER the
// initiate beat (the real device created peer FyLw4PSB's session 2 ms before
// the first `VPN_ICE_FEED|fed` landed) must not wedge the negotiation into a
// "candidates but no credentials" zombie.
// ---------------------------------------------------------------------------

/// 0048 requirement 4 regression: the zombie criterion must not be fooled by
/// remote CANDIDATES alone. A remote that trickles candidates but never sends
/// credentials can never satisfy `maybe_start` (`remote_creds` is mandatory),
/// so the session must still expire at [`netbird_core::peer_conn::START_DEADLINE_MS`]
/// and re-initiate with fresh credentials. The pre-fix `remote_cands_seen == 0`
/// clause kept such a peer `idle` forever (device run 0048: the host peer held
/// 64 remote candidates and 0 credentials).
#[test]
fn candidate_only_remote_still_times_out_and_reinitiates() {
    let bus = Arc::new(SignalBus::default());
    let mut a = TestOrch::new(KEY_A, &[KEY_B], 0x1111, bus.clone());

    let mut now = 1000u64;
    let _ = a.orch.run_once(now);
    now += 10;
    deliver(&bus, now, &mut a, None);
    let first_offer = bus
        .seen()
        .iter()
        .find(|f| f.kind == PeerSignalKind::Offer)
        .map(|f| f.payload.clone())
        .expect("initial offer");

    // The remote trickles a candidate but never credentials.
    let ghost = Candidate::host_candidate([127, 0, 0, 1], 51820);
    a.orch
        .handle_signal(KEY_B, PeerSignalKind::Candidate, &ghost.marshal(), now)
        .expect("remote candidate");
    assert_eq!(a.status_of(KEY_B).state, PeerIceState::Idle, "no credentials → never starts");

    // Before the deadline nothing is torn down.
    while now < 1000 + netbird_core::peer_conn::START_DEADLINE_MS - 500 {
        let _ = a.orch.run_once(now);
        now += 100;
        deliver(&bus, now, &mut a, None);
    }
    assert_eq!(a.status_of(KEY_B).state, PeerIceState::Idle, "no teardown before the deadline");

    // At the deadline the candidate-only negotiation is a zombie: tear down
    // and re-initiate with FRESH credentials.
    let mut fresh = None;
    while now <= 1000 + netbird_core::peer_conn::START_DEADLINE_MS + 15_000 {
        let _ = a.orch.run_once(now);
        now += 100;
        deliver(&bus, now, &mut a, None);
        if let Some(f) = bus.seen().iter().rev().find(|f| f.kind == PeerSignalKind::Offer) {
            if f.payload != first_offer {
                fresh = Some(f.payload.clone());
                break;
            }
        }
    }
    let fresh = fresh.expect("the candidate-only negotiation must re-arm with a fresh offer");
    assert_ne!(
        parse_ufrag_pwd(&fresh).expect("fresh offer wire form"),
        parse_ufrag_pwd(&first_offer).expect("original offer wire form"),
        "the re-initiation must carry NEW credentials"
    );
}

/// 0048 R3 regression, re-shaped by STAGE 2 (official-alignment §2.7): initiate
/// ran while the protected-socket provider was still empty — which now means
/// "no mux", so there is nothing to gather on and the beat fails CLOSED
/// (`ice:no-mux`) instead of waiting. The 0048 contract this test pins is
/// unchanged and is what matters: the half-built session is rolled back
/// (`want_initiate` demands `session.is_none()`), nothing is signaled, and the
/// post-supply retry — the provider hands out the ONE shared mux socket —
/// offers credentials again and both sides converge.
#[test]
fn initiate_before_socket_supply_still_offers_credentials() {
    let bus = Arc::new(SignalBus::default());
    // A is the default initiator and starts with an EMPTY socket provider.
    let mut a = TestOrch::new_with_sockets(KEY_A, &[KEY_B], 0x1111, bus.clone(), 0);
    let mut b = TestOrch::new_with_sockets(KEY_B, &[KEY_A], 0x2222, bus.clone(), 8);
    b.orch.set_initiator(KEY_A, false); // deterministic role split: only A offers

    let mut now = 1000u64;
    // Beat 1: no mux ⇒ fail-closed. The error names the missing mux, the
    // half-built session is rolled back and NO frame leaves.
    let err = a
        .orch
        .run_once(now)
        .expect_err("no mux is a fail-closed error, never an unprotected socket");
    assert!(format!("{err}").contains("no-mux"), "got {err}");
    assert_eq!(a.status_of(KEY_B).state, PeerIceState::Idle);
    assert!(bus.queue.lock().expect("queue").is_empty(), "fail-closed: no frame may leave");

    // The remote trickles candidates while A still has no credentials (the
    // 0048 host-peer shape: 64 candidates, 0 offer/answer). The pre-fix
    // zombie criterion (`remote_cands_seen == 0`) therefore never expired it.
    let ghost = Candidate::host_candidate([127, 0, 0, 1], 51820);
    a.orch
        .handle_signal(KEY_B, PeerSignalKind::Candidate, &ghost.marshal(), now)
        .expect("remote candidate is buffered");

    // 2 ms later the first protected ICE socket is supplied (0048 timeline).
    now += 2;
    a.socks.feed_lo_sockets(8);

    let converged = pump_until(&bus, &mut a, &mut b, &mut now, 40_000, |a, b| {
        a.status_of(KEY_B).state == PeerIceState::Connected
            && b.status_of(KEY_A).state == PeerIceState::Connected
    });
    assert!(
        converged,
        "after the late socket supply the initiation must complete: A={:?} B={:?}",
        a.status_of(KEY_B).state,
        b.status_of(KEY_A).state
    );

    // The credential frame finally left the initiator (pre-fix: candidates
    // only, forever). Exactly one distinct OFFER, legal ufrag:pwd shape.
    let offers: std::collections::HashSet<String> = bus
        .seen()
        .into_iter()
        .filter(|f| f.from == KEY_A && f.to == KEY_B && f.kind == PeerSignalKind::Offer)
        .map(|f| f.payload)
        .collect();
    assert_eq!(offers.len(), 1, "exactly one distinct OFFER must be signaled, got {offers:?}");
    let creds = parse_ufrag_pwd(offers.iter().next().expect("offer")).expect("offer wire form");
    assert_eq!(creds.ufrag.len(), 16);
    assert_eq!(creds.pwd.len(), 32);

    // Started on both sides (Checking → Connected came from a real start()).
    let sa = a.status_of(KEY_B);
    let sb = b.status_of(KEY_A);
    assert_eq!(sa.controlling, Some(true), "the late initiator stays controlling");
    assert_eq!(sb.controlling, Some(false), "the answerer is controlled");
    assert!(sa.endpoint_applied && sb.endpoint_applied, "both endpoints landed");
    assert!(sa.reachable && sb.reachable);
}

/// 0048 glare regression (requirement 2), re-shaped by STAGE 2 (official-alignment
/// §2.7): the local side attempted to initiate while its socket provider was
/// empty — which now means "no mux", so the initiate beat fails CLOSED and the
/// half-built session is rolled back. The remote's OFFER then arrives and takes
/// the normal answerer path (no session ⇒ no glare), so the ANSWER leaves as soon
/// as the provider hands out the mux and both sides converge.
#[test]
fn remote_offer_after_failed_initiate_is_answered() {
    let bus = Arc::new(SignalBus::default());
    // Both sides are initiators (the default); A's provider is still empty.
    let mut a = TestOrch::new_with_sockets(KEY_A, &[KEY_B], 0x1111, bus.clone(), 0);
    let mut b = TestOrch::new_with_sockets(KEY_B, &[KEY_A], 0x2222, bus.clone(), 8);

    let mut now = 1000u64;
    // STAGE 2: no mux ⇒ fail-closed, session rolled back.
    assert!(a.orch.run_once(now).is_err(), "no mux is a fail-closed error");

    // B offers (with candidates) while A's pool is still empty.
    now += 2;
    let _ = b.orch.run_once(now);
    deliver(&bus, now, &mut a, Some(&mut b));
    assert!(
        bus.seen().iter().any(|f| f.from == KEY_B && f.kind == PeerSignalKind::Offer),
        "precondition: B's OFFER reached A"
    );
    assert!(
        !bus.seen().iter().any(|f| f.from == KEY_A && f.kind != PeerSignalKind::Candidate),
        "A must not have signaled credentials before the socket supply"
    );

    // The socket supply lands AFTER the remote OFFER.
    now += 2;
    let supply_at = now;
    a.socks.feed_lo_sockets(8);

    // Pump observing WHEN the ANSWER leaves: the remote OFFER is unanswered
    // until the socket supply arrives, and the fix must answer it on the next
    // local-candidate collection — not after a whole failed negotiation cycle.
    let mut answer_at = None;
    let mut converged = false;
    while now <= 40_000 {
        let _ = a.orch.run_once(now);
        let _ = b.orch.run_once(now);
        now += 10;
        deliver(&bus, now, &mut a, Some(&mut b));
        if answer_at.is_none()
            && bus.seen().iter().any(|f| f.from == KEY_A && f.kind == PeerSignalKind::Answer)
        {
            answer_at = Some(now);
        }
        if a.status_of(KEY_B).state == PeerIceState::Connected
            && b.status_of(KEY_A).state == PeerIceState::Connected
        {
            converged = true;
            break;
        }
    }
    assert!(
        converged,
        "A must answer B's OFFER and both must converge: A={:?} B={:?}",
        a.status_of(KEY_B).state,
        b.status_of(KEY_A).state
    );
    let answer_at = answer_at.expect("A must signal an ANSWER");
    assert!(
        answer_at <= supply_at + 3_000,
        "the ANSWER must follow the socket supply promptly (answer at +{} ms after supply), \
         not after a failed negotiation cycle",
        answer_at - supply_at
    );

    let answers: std::collections::HashSet<String> = bus
        .seen()
        .into_iter()
        .filter(|f| f.from == KEY_A && f.to == KEY_B && f.kind == PeerSignalKind::Answer)
        .map(|f| f.payload)
        .collect();
    assert_eq!(answers.len(), 1, "A must send exactly one distinct ANSWER, got {answers:?}");
    assert!(parse_ufrag_pwd(answers.iter().next().expect("answer")).is_ok());
    assert!(
        !bus.seen().iter().any(|f| f.from == KEY_A && f.kind == PeerSignalKind::Offer),
        "A answered the remote OFFER; it must not also offer"
    );
    assert_eq!(a.status_of(KEY_B).controlling, Some(false), "A answered → controlled");
    assert_eq!(b.status_of(KEY_A).controlling, Some(true), "B offered → controlling");
}

// ---------------------------------------------------------------------------
// STAGE 3 (official-alignment audit §2.7): the shared socket IS the WG seam's
// socket, and non-STUN goes STRAIGHT into the WG seam
// ---------------------------------------------------------------------------

/// `getsockname` port of a bound test fd.
fn bound_port(fd: i32) -> u16 {
    let mut name = netbird_core::sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut len = core::mem::size_of::<netbird_core::sys::sockaddr_in>() as u32;
    assert_eq!(unsafe { netbird_core::sys::getsockname(fd, &mut name, &mut len) }, 0);
    u16::from_be(name.sin_port)
}

/// One datagram from a test-held socket (the spoke's own mux fd).
fn send_from_fd(fd: i32, to: ([u8; 4], u16), bytes: &[u8]) {
    let sa = netbird_core::sys::sockaddr_in::new(to.0, to.1);
    let n = unsafe {
        netbird_core::sys::sendto(
            fd,
            bytes.as_ptr() as *const core::ffi::c_void,
            bytes.len(),
            0,
            &sa,
            core::mem::size_of::<netbird_core::sys::sockaddr_in>() as u32,
        )
    };
    assert_eq!(n as usize, bytes.len(), "send from the shared socket");
}

/// A WG-shaped datagram (`isWireGuardMsg`: little-endian type 4 in the low byte,
/// the three following bytes zero, >= 32 bytes).
fn wg_shaped(tag: u8) -> Vec<u8> {
    let mut pkt = vec![0u8; 64];
    pkt[0] = 4;
    pkt[40] = tag;
    pkt
}

/// MANY peers on ONE socket that is the WG seam's own socket; non-STUN datagrams
/// take the DIRECT route into the WG seam.
///
/// Stage-3 properties pinned here (the connector-shaped counterpart of the
/// real-device `wg_over_ice_n11` and `wg_e2e` suites):
/// * the mux is armed OVER the WG seam's socket (`wg_socket_fd` →
///   `arm_mux_from_wg_socket`), so the mux port IS that socket's port and the ICE
///   provider is never consumed — one open+bind for ICE and WG, no pool socket;
/// * the seam's read half is handed over (`set_shared_socket_reader(true)`), and
///   the mux carries a WG sink — the `take_data_rx` per-session queue is out of
///   the data path;
/// * all N negotiations still demux correctly on that ONE socket (ufrag /
///   remote-address routing → every pair Connected);
/// * a WG-shaped datagram injected from another peer's OWN mux socket (a source
///   our sessions have already written to) is delivered STRAIGHT to
///   `handle_udp_inbound` — the WG seam — and the mux counts no foreign source.
#[test]
fn many_peers_ride_the_wg_socket_and_non_stun_goes_straight_to_the_wg_seam() {
    const N: usize = 4;
    let bus = Arc::new(SignalBus::default());
    let peer_keys: &'static [String] = Box::leak(
        (0..N).map(|i| format!("UEtFWV9CMHt9P{i}")).collect::<Vec<String>>().into_boxed_slice(),
    );
    let refs: Vec<&'static str> = peer_keys.iter().map(|k| k.as_str()).collect();

    let mut a = TestOrch::new_sharing_wg_socket(KEY_A, &refs, 0x1111, bus.clone());
    let mut bees: Vec<TestOrch> =
        refs.iter().map(|k| TestOrch::new(k, &[KEY_A], 0x2222, bus.clone())).collect();
    for b in bees.iter_mut() {
        b.orch.set_initiator(KEY_A, false); // deterministic: A offers, B_i answers
    }

    let mut now = 1000u64;
    let deadline = now + 20_000;
    let mut converged = false;
    while now <= deadline {
        {
            let mut all: Vec<&mut TestOrch> = Vec::new();
            all.push(&mut a);
            for b in bees.iter_mut() {
                all.push(b);
            }
            step_many(&bus, now, &mut all);
        }
        now += 10;
        converged = bees.iter().all(|b| {
            b.status_of(KEY_A).state == PeerIceState::Connected
                && a.status_of(b.me).state == PeerIceState::Connected
        });
        if converged {
            break;
        }
    }
    assert!(converged, "all {N} peers must converge over the ONE shared socket");

    // (1) the mux is the seam's OWN socket; the ICE provider stays untouched; the
    //     seam handed the read half over and the WG sink is armed.
    let mux = a.orch.ice_mux().expect("the one shared mux");
    let seam_port = bound_port(a.wg.sock_fd().expect("the seam's socket"));
    assert_eq!(mux.local_port(), seam_port, "the mux IS the WG seam's socket");
    assert_eq!(a.socks.taken(), 0, "no ICE provider socket was taken");
    assert_eq!(a.socks.source.pending(), 4, "…and none was consumed from the queue either");
    assert!(a.wg.shared_reader(), "the seam handed the socket's read half to the mux");
    assert!(mux.sink_armed(), "non-STUN datagrams go straight into the WG seam");
    // every candidate of every peer carries that ONE port
    let cands = candidate_frames(&bus, KEY_A);
    assert!(!cands.is_empty());
    assert!(cands.iter().all(|c| c.port == seam_port), "ONE port for every candidate");

    // (2) the direct route: a WG-shaped datagram from spoke 0's OWN mux socket (a
    //     source A's sessions have already written to, so it is registered) lands in
    //     the WG seam's `handle_udp_inbound` — during the mux drain, with NO session
    //     involvement.
    let b0_mux = bees[0].orch.ice_mux().expect("spoke 0's mux");
    let src_port = b0_mux.local_port();
    send_from_fd(b0_mux.raw_fd(), ([127, 0, 0, 1], seam_port), &wg_shaped(0x5A));
    let deadline = now + 1_000;
    let mut got = a.wg.sink_calls();
    while got.is_empty() && now <= deadline {
        let _ = a.orch.run_once(now);
        now += 10;
        got = a.wg.sink_calls();
    }
    assert_eq!(
        got,
        vec![([127, 0, 0, 1], src_port)],
        "the WG datagram reached the WG seam directly, from the registered peer source"
    );
    assert_eq!(mux.stats().data_unknown, 0, "the source was one of ours: no foreign source");
    // …and the retired per-session queue is empty on the mux side too (with a sink
    // armed the mux never queues data: `dispatch` hands it straight over)
    assert_eq!(mux.queued(0).1, 0, "no session id holds a data queue entry");
}

