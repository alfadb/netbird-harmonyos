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
    ice_ready_for_default_route, parse_ufrag_pwd, PeerIceDeps, PeerIceOrchestrator,
    PeerIceState, PeerSignalKind, SignalExchange,
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
#[derive(Default)]
struct RecordingWg {
    endpoints: Mutex<Vec<(String, [u8; 4], u16)>>,
    egress: Mutex<Vec<String>>,
}

impl RecordingWg {
    fn calls(&self) -> Vec<(String, [u8; 4], u16)> {
        self.endpoints.lock().expect("endpoints").clone()
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
        let source = Arc::new(ProtectedUdpFdSource::new_with_fd(-1));
        for &fd in &raw_fds {
            source.feed(fd);
        }
        FedSocks { source, raw_fds }
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
        let socks = FedSocks::with_lo_sockets(8);
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
        orch.set_peers(&keys);
        orch.set_signal_ready(true);
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

    // Protected sockets: 1 gather round + 1 session candidate = 2 taken,
    // nothing left queued — every fd came from the provider.
    assert_eq!(a.socks.taken(), 2, "A: gather socket + check socket");
    assert_eq!(b.socks.taken(), 2, "B: gather socket + check socket");
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
