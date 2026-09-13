// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N5d end-to-end tests: the REAL signal path (per-peer ICE orchestrator →
//! `RealSignalExchange` → `spawn_signal_link` → real `SignalSession` →
//! in-process mock signal server → the peer's real `SignalSession` → its
//! orchestrator), replacing the N5c mock-bus seam.
//!
//! Proven here (the N5d acceptance core):
//! - dual-instance exchange through REAL `SignalSession`s: registration
//!   headers on the server, sealed offer/answer/candidate frames (the mock
//!   sink REALLY decrypts server-side), both peers ICE Connected with
//!   consistent selected pairs and WG endpoints landed on the injectable
//!   WG seam; `signal_ready` arms ONLY after the server confirmed
//!   registration (asserted inside the event callback, against the
//!   server-side registry — no client-side lying possible);
//! - frames sent while the link is NOT registered are REFUSED (explicit
//!   `Network` error, upstream `ErrSignalIsNotReady` parity,
//!   handshaker.go:16,208,212-214) and RETAINED in the orchestrator outbox
//!   until registration — never silently dropped (the LoggingSignalExchange
//!   contrast is pinned by the refusal counter);
//! - a transport kill → `Broken` → reconnect with a FRESH protected fd and
//!   a SECOND registration header on the server; ICE sessions are kept
//!   (state stays Connected), `signal_ready` re-arms, and the signal path
//!   carries frames again after the re-registration;
//! - an EMPTY protected-fd source fails CLOSED: the provider hands out
//!   nothing (taken == 0), the server sees NO connection (no unprotected
//!   fallback), `registered` stays false.

mod signal_mock;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use netbird_core::connector::{WgPeerApplier, WgPeerEntry};
use netbird_core::envelope::EnvelopeKeyPair;
use netbird_core::ice::{Candidate, InterfaceAddr, ProtectedUdpFdSource, StaticInterfaces};
use netbird_core::mgmtsock::ProtectedSocketFdSource;
use netbird_core::peer_conn::{
    parse_ufrag_pwd, spawn_signal_link, PeerIceDeps, PeerIceOrchestrator, PeerIceState,
    PeerSignalKind, RealSignalExchange, SignalExchange, SignalLinkConfig, SignalLinkEvent,
};
use netbird_core::signal::proto::body::Type;

use signal_mock::{spawn_signal_mock, SinkPeer, SignalMock};

extern "C" {
    fn socket(domain: i32, ty: i32, protocol: i32) -> i32;
    fn close(fd: i32) -> i32;
}

// ---------------------------------------------------------------------------
// harness (FedSocks / RecordingWg patterns from tests/peer_conn_e2e.rs)
// ---------------------------------------------------------------------------

/// Protected-UDP provider holding the ORIGINAL ICE fds (sessions only ever
/// touch dups); originals close exactly once, on drop. `add` refills later
/// (the shell feed contract).
struct FedSocks {
    source: Arc<ProtectedUdpFdSource>,
    raw_fds: Mutex<Vec<i32>>,
}

impl FedSocks {
    fn new(n: usize) -> Self {
        let source = Arc::new(ProtectedUdpFdSource::new_with_fd(-1));
        let fed = FedSocks { source, raw_fds: Mutex::new(Vec::new()) };
        fed.add(n);
        fed
    }

    fn add(&self, n: usize) {
        let mut raw = self.raw_fds.lock().expect("raw_fds");
        for _ in 0..n {
            let fd = unsafe { socket(2, 2, 0) }; // AF_INET, SOCK_DGRAM
            assert!(fd >= 0, "socket() failed");
            self.source.feed(fd);
            raw.push(fd);
        }
    }

    fn taken(&self) -> u64 {
        self.source.taken()
    }
}

impl Drop for FedSocks {
    fn drop(&mut self) {
        let mut raw = self.raw_fds.lock().expect("raw_fds");
        for fd in raw.drain(..) {
            unsafe { close(fd) };
        }
    }
}

/// Injectable WG seam: records every endpoint landing (peer, addr, port).
#[derive(Default)]
struct RecordingWg {
    endpoints: Mutex<Vec<(String, [u8; 4], u16)>>,
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
    }
    fn apply_endpoint(&self, pub_key_b64: &str, addr: [u8; 4], port: u16) -> Result<(), String> {
        self.endpoints
            .lock()
            .expect("endpoints")
            .push((pub_key_b64.to_string(), addr, port));
        Ok(())
    }
}

/// What a node's signal link recorded (test-side event log).
#[derive(Debug, Clone, PartialEq)]
enum EventRec {
    DialFailed,
    Registered,
    Message { kind: i32, payload: String },
    Malformed,
    Broken,
}

/// One full peer instance: ICE orchestrator + REAL signal exchange + REAL
/// signal session worker over the protected-socket seam. All signal frames
/// between two nodes flow through the mock server (pure forwarder).
struct PeerNode {
    key: String,
    orch: Arc<Mutex<PeerIceOrchestrator>>,
    ice_socks: FedSocks,
    wg: Arc<RecordingWg>,
    exchange: Arc<RealSignalExchange>,
    signal_source: Arc<ProtectedSocketFdSource>,
    events: Arc<Mutex<Vec<EventRec>>>,
    sim_now: Arc<AtomicU64>,
    link: tokio::task::JoinHandle<()>,
}

/// `Body.type` → the orchestrator's frame subset (HEARTBEAT/MODE/GO_IDLE
/// are not ICE frames and are dropped by the router, mirroring the
/// connector's `route_signal_message`).
fn ice_kind(t: Type) -> Option<PeerSignalKind> {
    match t {
        Type::Offer => Some(PeerSignalKind::Offer),
        Type::Answer => Some(PeerSignalKind::Answer),
        Type::Candidate => Some(PeerSignalKind::Candidate),
        _ => None,
    }
}

/// Spawn one node: the orchestrator is NOT ready (signal_ready false) until
/// the link's Registered event — which the callback only accepts after the
/// SERVER saw our registration header (the mock's seen_ids is the server
/// side of the story). `keys` is the node's identity (generated by the test
/// so the server-side sink can be aimed at it); `remote_key` is the peer's
/// public key. `signal_fds` seeds the protected signal-socket queue
/// (0 = starved shell: the link dials fail-closed).
#[allow(clippy::too_many_arguments)]
fn spawn_node(
    keys: EnvelopeKeyPair,
    remote_key: &str,
    tie_breaker: u64,
    addr: std::net::SocketAddr,
    mock_seen: Arc<Mutex<Vec<String>>>,
    answerer: bool,
    signal_fds: usize,
) -> PeerNode {
    let ice_socks = FedSocks::new(6);
    let wg = Arc::new(RecordingWg::default());
    let (exchange, rx) = RealSignalExchange::new();
    let orch = Arc::new(Mutex::new(PeerIceOrchestrator::new(PeerIceDeps {
        ifaces: Arc::new(StaticInterfaces(vec![InterfaceAddr {
            name: "eth0".into(),
            addr: [127, 0, 0, 1],
        }])),
        socks: ice_socks.source.clone(),
        signal: exchange.clone(),
        wg: wg.clone(),
        tie_breaker: Some(tie_breaker),
    })));
    orch.lock().expect("orch").set_peers(&[remote_key.to_string()]);
    if answerer {
        orch.lock().expect("orch").set_initiator(remote_key, false);
    }
    assert!(
        !orch.lock().expect("orch").signal_ready(),
        "a fresh node must NOT be signal_ready"
    );

    let events = Arc::new(Mutex::new(Vec::<EventRec>::new()));
    let sim_now = Arc::new(AtomicU64::new(1000));
    // protected signal sockets: the mock rig's opener (TCP via
    // mgmt_socket_open — the production gRPC shape); 0 = starved shell
    let signal_source: Arc<ProtectedSocketFdSource> = if signal_fds > 0 {
        signal_mock::socket_source(signal_fds)
    } else {
        Arc::new(ProtectedSocketFdSource::new_with_fd(-1))
    };

    let cb_orch = orch.clone();
    let cb_events = events.clone();
    let cb_sim = sim_now.clone();
    let cb_key = keys.public_key_base64();
    let cb_seen = mock_seen.clone();
    let on_event: Arc<dyn Fn(SignalLinkEvent) + Send + Sync> = Arc::new(move |ev| match ev {
        SignalLinkEvent::DialFailed(_) => {
            cb_events.lock().expect("events").push(EventRec::DialFailed);
        }
        SignalLinkEvent::Registered => {
            // THE ordering pin: signal_ready may only arm AFTER the server
            // confirmed OUR registration (registry insert happens before the
            // confirm header is sent, signal.go:117-121 + 134-152).
            let seen = cb_seen.lock().expect("seen");
            assert!(
                seen.contains(&cb_key),
                "Registered must follow the server-side registration \
                 (server saw {seen:?}, we are {cb_key})"
            );
            drop(seen);
            cb_orch.lock().expect("orch").set_signal_ready(true);
            cb_events.lock().expect("events").push(EventRec::Registered);
        }
        SignalLinkEvent::Message(m) => {
            let now = cb_sim.load(Ordering::Acquire);
            if let Some(kind) = ice_kind(m.kind) {
                let _ = cb_orch
                    .lock()
                    .expect("orch")
                    .handle_signal(&m.from_key, kind, &m.payload, now);
            }
            cb_events.lock().expect("events").push(EventRec::Message {
                kind: m.kind as i32,
                payload: m.payload.clone(),
            });
        }
        SignalLinkEvent::Malformed => {
            // stream stays up (crate::signal semantics) — just record it
            cb_events.lock().expect("events").push(EventRec::Malformed);
        }
        SignalLinkEvent::Broken(_) => {
            cb_orch.lock().expect("orch").set_signal_ready(false);
            cb_events.lock().expect("events").push(EventRec::Broken);
        }
        SignalLinkEvent::Ended(_) => {}
    });

    let link = spawn_signal_link(
        tokio::runtime::Handle::current(),
        SignalLinkConfig {
            endpoint: format!("http://{addr}"),
            transport: netbird_core::grpc::GrpcTransport::Plaintext,
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(5),
            keys: keys.clone(),
            sockets: signal_source.clone(),
            connect_addr: addr,
        },
        exchange.clone(),
        rx,
        on_event,
    );

    PeerNode {
        key: keys.public_key_base64(),
        orch,
        ice_socks,
        wg,
        exchange,
        signal_source,
        events,
        sim_now,
        link,
    }
}

impl PeerNode {
    fn registered_count(&self) -> usize {
        self.events
            .lock()
            .expect("events")
            .iter()
            .filter(|e| **e == EventRec::Registered)
            .count()
    }

    fn status_of(&self, peer: &str) -> netbird_core::peer_conn::PeerIceStatus {
        self.orch.lock().expect("orch").peer_status(peer).expect("peer entry")
    }
}

/// Pump both orchestrators on the shared injected clock until `cond` holds.
/// Sim time advances 10 ms per ~2 ms of real time (no long sleeps; the
/// condition itself is the assertion).
async fn pump_until(
    a: &mut PeerNode,
    b: &mut PeerNode,
    now: &mut u64,
    deadline_ms: u64,
    cond: impl Fn(&PeerNode, &PeerNode) -> bool,
) -> bool {
    while *now <= deadline_ms {
        a.sim_now.store(*now, Ordering::Release);
        b.sim_now.store(*now, Ordering::Release);
        let _ = a.orch.lock().expect("orch").run_once(*now);
        let _ = b.orch.lock().expect("orch").run_once(*now);
        *now += 10;
        if cond(a, b) {
            return true;
        }
        // gentle pacing: four of these run in parallel in one process —
        // the loops must not starve the tokio workers carrying the real
        // signal streams (sim/real ratio ~2.5x is plenty for ICE timers)
        tokio::time::sleep(Duration::from_millis(4)).await;
    }
    false
}

async fn wait_for<F: Fn() -> bool>(what: &'static str, cond: F) {
    wait_for_box(what, 10, cond).await;
}

/// Bounded wait with an explicit box (seconds) — generous boxes for
/// reconnect steps that ride the upstream 800 ms backoff under full-suite
/// CPU contention; the condition itself is still the assertion.
async fn wait_for_box<F: Fn() -> bool>(what: &'static str, box_secs: u64, cond: F) {
    let deadline = std::time::Instant::now() + Duration::from_secs(box_secs);
    while !cond() {
        assert!(
            std::time::Instant::now() <= deadline,
            "deadline waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Server-side decrypted capture of the frames addressed to `sink` (the
/// mock REALLY opens the envelope: sink private key + sender public key).
fn sink_records(mock: &SignalMock) -> Vec<(i32, String)> {
    mock.sink
        .lock()
        .expect("sink")
        .as_ref()
        .expect("sink set")
        .records
        .lock()
        .expect("records")
        .iter()
        .map(|r| {
            let mut parts = r.splitn(4, '|');
            let t: i32 = parts.next().expect("type").parse().expect("type num");
            let payload = parts.next().expect("payload").to_string();
            (t, payload)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

/// THE N5d core: two full peer instances exchange offer/answer/candidates
/// through REAL SignalSessions over the mock signal server → both ICE
/// Connected, selected pairs consistent, WG endpoints landed on the
/// injectable seam. The server-side sink decrypts the frames (sealed
/// envelopes), and signal_ready armed only after registration.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dual_peer_real_signal_sessions_converge_and_land_wg_endpoints() {
    let mock = SignalMock::default();
    let keys_a = EnvelopeKeyPair::generate().expect("keys A");
    let keys_b = EnvelopeKeyPair::generate().expect("keys B");
    // server-side decrypting sink for B: captures everything addressed to B
    mock.sink
        .lock()
        .expect("sink")
        .replace(SinkPeer::for_keys(keys_b.clone()));
    let (addr, counter) = spawn_signal_mock(mock.clone()).await;

    let mut a = spawn_node(
        keys_a.clone(),
        &keys_b.public_key_base64(),
        0x1111,
        addr,
        mock.seen_ids.clone(),
        false,
        8,
    );
    let mut b = spawn_node(
        keys_b.clone(),
        &keys_a.public_key_base64(),
        0x2222,
        addr,
        mock.seen_ids.clone(),
        true,
        8,
    );
    let key_a = a.key.clone();
    let key_b = b.key.clone();

    // pre-registration: signal_ready is FALSE on both nodes
    assert!(!a.orch.lock().expect("orch").signal_ready());
    assert!(!b.orch.lock().expect("orch").signal_ready());
    wait_for("both nodes registered", || {
        a.registered_count() >= 1 && b.registered_count() >= 1
    })
    .await;

    // converged ICE over the real signal path
    let mut now = 1000u64;
    let converged = pump_until(&mut a, &mut b, &mut now, 360_000, |a, b| {
        a.status_of(&key_b).state == PeerIceState::Connected
            && b.status_of(&key_a).state == PeerIceState::Connected
    })
    .await;
    assert!(converged, "both peers must reach Connected through the REAL signal path");

    // signal_ready armed by the Registered events (ordering asserted in the
    // callback against the server registry)
    assert!(a.orch.lock().expect("orch").signal_ready());
    assert!(b.orch.lock().expect("orch").signal_ready());

    // selected pairs mirror each other; each WG seam landed exactly the
    // peer's selected address (injectable-seam assertion). The sink sees
    // A→B frames only, so A's candidate port is the server-verified anchor
    // for B's landed endpoint.
    let a_cands: Vec<Candidate> = sink_records(&mock)
        .iter()
        .filter(|(t, _)| *t == Type::Candidate as i32)
        .map(|(_, p)| Candidate::unmarshal(p).expect("candidate wire form"))
        .collect();
    assert!(!a_cands.is_empty(), "A's candidates must have reached B (server-decrypted)");
    let a_local_port = a_cands[0].port;
    let sa = a.status_of(&key_b);
    let sb = b.status_of(&key_a);
    assert_eq!(
        sb.selected_remote,
        Some(([127, 0, 0, 1], a_local_port)),
        "B landed A's signaled candidate"
    );
    assert_eq!(
        b.wg.calls(),
        vec![(key_a.clone(), [127, 0, 0, 1], a_local_port)],
        "B's WG seam got A's selected address"
    );
    // A's view: some B candidate landed and its WG call matches it exactly
    assert!(sa.selected_remote.is_some(), "A must have selected a B address");
    assert_eq!(
        a.wg.calls(),
        vec![(key_b.clone(), [127, 0, 0, 1], sa.selected_remote.expect("selected").1)],
        "A's WG seam got B's selected address"
    );
    assert!(sa.endpoint_applied && sb.endpoint_applied, "endpoints must be flagged applied");
    assert!(sa.reachable && sb.reachable, "both peers reachable");
    assert_eq!(sa.controlling, Some(true), "offerer controlling");
    assert_eq!(sb.controlling, Some(false), "answerer controlled");

    // server-side frame capture: the OFFER A→B reached the server through
    // the UNARY Send path (envelope opened by the mock with A's public key
    // + B's private key) and carries "ufrag:pwd"
    let offers: Vec<String> = sink_records(&mock)
        .into_iter()
        .filter(|(t, _)| *t == Type::Offer as i32)
        .map(|(_, p)| p)
        .collect();
    assert_eq!(offers.len(), 1, "exactly one OFFER (A initiates, B answers)");
    let creds = parse_ufrag_pwd(&offers[0]).expect("offer payload is ufrag:pwd");
    assert_eq!(creds.ufrag.len(), 16, "upstream 16-char ufrag rule");
    assert_eq!(creds.pwd.len(), 32, "upstream 32-char pwd rule");

    // registration headers on the server for both peers
    let seen = mock.seen_ids.lock().expect("seen");
    assert!(seen.contains(&key_a) && seen.contains(&key_b), "both registered: {seen:?}");

    // N10b regression pins, enforced on every dual-instance exchange: the
    // frames rode the UNARY Send (the only path the deployed server
    // generation forwards) and NONE of them rode the ConnectStream request
    // body (which the v0.78.1 server never reads — a stream frame would be
    // silently black-holed while the peer starves)
    assert!(
        mock.stream_frames_seen() == 0,
        "no frame may ride the ConnectStream body (real servers never read it)"
    );
    assert!(
        mock.unary_sends() >= 3,
        "offer/answer/candidates must ride the unary Send, got {}",
        mock.unary_sends()
    );

    // protected fds: both nodes' signal dials came from the provider
    assert!(a.signal_source.taken() >= 1 && b.signal_source.taken() >= 1);
    assert!(
        (a.signal_source.taken() + b.signal_source.taken()) as usize >= counter.accepted(),
        "every accepted connection rode a provided socket (taken={}, accepted={})",
        a.signal_source.taken() + b.signal_source.taken(),
        counter.accepted()
    );

    a.link.abort();
    b.link.abort();
}

/// N10b 回归（真实 0.78.1 服务端形态：**服务端只收不回**）——真正的
/// 抓 bug 测试。v0.78.1 的 `ConnectStream` 处理器从不读请求体
/// （signal.go:106-132 阻塞在 `stream.Context().Done()`），转发只发生在
/// unary `Send`（signal.go:95-104）。本 mock 与真实服务端同型：流上的帧
/// 只计数不转发（`stream_frames_seen`），转发只走 unary（`unary_sends`）。
///
/// 修复前（帧被推上 ConnectStream 请求体）：服务端收到帧也不转发，B 一
///无所获，本测试在 `offer_delivered` 断言处失败（且 `stream_frames_seen`
/// > 0）；修复后（unary 送达）：OFFER 经服务端解密验证抵达 sink，且流上
/// 零帧。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn offer_reaches_sink_via_unary_send_while_server_never_reads_the_stream() {
    let mock = SignalMock::default();
    let keys_a = EnvelopeKeyPair::generate().expect("keys A");
    let keys_b = EnvelopeKeyPair::generate().expect("keys B");
    // B plays "server only receives, never sends": no second node, just the
    // decrypting sink aimed at B's public key
    mock.sink
        .lock()
        .expect("sink")
        .replace(SinkPeer::for_keys(keys_b.clone()));
    let (addr, _counter) = spawn_signal_mock(mock.clone()).await;

    let mut a = spawn_node(
        keys_a.clone(),
        &keys_b.public_key_base64(),
        0x1111,
        addr,
        mock.seen_ids.clone(),
        false,
        8,
    );
    let key_a = a.key.clone();

    wait_for("A registered", || a.registered_count() >= 1).await;

    // pump A alone: initiate → gather → OFFER + candidates into the
    // exchange → the session's unary delivery
    let mut now = 1000u64;
    let offer_delivered = pump_until_sink(&mut a, &mut now, 120_000, &mock, |records| {
        records.iter().any(|(t, _)| *t == Type::Offer as i32)
    })
    .await;
    assert!(
        offer_delivered,
        "the OFFER must reach the server through the unary Send path \
         (a stream-pushed frame is black-holed by v0.78.1 servers)"
    );

    // the exact N10b signature: NOTHING rode the ConnectStream request body
    assert_eq!(
        mock.stream_frames_seen(),
        0,
        "client must not push frames on the stream body (real servers never read it)"
    );
    assert!(
        mock.unary_sends() >= 1,
        "frames must ride the unary Send, got {}",
        mock.unary_sends()
    );
    // and the sink REALLY decrypted A's offer (server-side envelope proof)
    let offers: Vec<String> = sink_records(&mock)
        .into_iter()
        .filter(|(t, _)| *t == Type::Offer as i32)
        .map(|(_, p)| p)
        .collect();
    assert_eq!(offers.len(), 1);
    let creds = parse_ufrag_pwd(&offers[0]).expect("offer payload is ufrag:pwd");
    assert_eq!(creds.ufrag.len(), 16);
    assert!(mock
        .seen_ids
        .lock()
        .expect("seen")
        .contains(&key_a));

    a.link.abort();
}

/// Pump ONE orchestrator on the shared injected clock until the mock sink
/// satisfies `cond` (server-side decrypted capture).
async fn pump_until_sink(
    a: &mut PeerNode,
    now: &mut u64,
    deadline_ms: u64,
    mock: &SignalMock,
    cond: impl Fn(&[(i32, String)]) -> bool,
) -> bool {
    while *now <= deadline_ms {
        a.sim_now.store(*now, Ordering::Release);
        let _ = a.orch.lock().expect("orch").run_once(*now);
        *now += 10;
        if cond(&sink_records(mock)) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(4)).await;
    }
    false
}

/// 未就绪不出帧：with the link unable to register (empty protected fd
/// source), a forced-ready orchestrator's send is REFUSED (explicit
/// Network error — never the silent Ok of LoggingSignalExchange) and the
/// frame is RETAINED; once fds are fed and registration lands, the SAME
/// pump delivers it — exactly one OFFER reaches the peer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unregistered_sends_are_refused_retained_and_delivered_after_registration() {
    let mock = SignalMock::default();
    let keys_a = EnvelopeKeyPair::generate().expect("keys A");
    let keys_b = EnvelopeKeyPair::generate().expect("keys B");
    mock.sink
        .lock()
        .expect("sink")
        .replace(SinkPeer::for_keys(keys_b.clone()));
    let (addr, _counter) = spawn_signal_mock(mock.clone()).await;

    // A starves (0 signal fds), B feeds (registers normally)
    let mut a = spawn_node(
        keys_a.clone(),
        &keys_b.public_key_base64(),
        0x1111,
        addr,
        mock.seen_ids.clone(),
        false,
        0,
    );
    let mut b = spawn_node(
        keys_b.clone(),
        &keys_a.public_key_base64(),
        0x2222,
        addr,
        mock.seen_ids.clone(),
        true,
        4,
    );
    let key_a = a.key.clone();
    let key_b = b.key.clone();

    // B (answerer) registers normally
    wait_for("B registered", || b.registered_count() >= 1).await;

    // A: STARVE the signal fd source — the link cannot dial, cannot register
    wait_for("A dial failed (fail-closed)", || {
        a.events.lock().expect("events").contains(&EventRec::DialFailed)
    })
    .await;
    assert!(!a.exchange.is_registered(), "no fds → no registration");
    assert!(!a.orch.lock().expect("orch").signal_ready());

    // force the orchestrator ready anyway (defense-in-depth scenario: the
    // seam must hold even against a stale-ready orchestrator) and pump one
    // initiation tick: gather succeeds (ICE fds exist) but the OFFER hits
    // the unregistered seam
    a.orch.lock().expect("orch").set_signal_ready(true);
    let mut now = 1000u64;
    let first = a.orch.lock().expect("orch").run_once(now);
    assert!(first.is_err(), "the refused send must surface an explicit error");
    assert!(
        a.exchange.refused_sends() >= 1,
        "the unregistered seam REFUSES (counted) — the LoggingSignalExchange contrast"
    );
    // the offer sits in the orchestrator outbox (state stuck pre-checking,
    // nothing delivered on the server)
    assert_eq!(a.status_of(&key_b).state, PeerIceState::Idle, "no candidates flow, no start");
    assert!(
        sink_records(&mock).iter().all(|(t, _)| *t != Type::Offer as i32),
        "NO offer may reach the server while unregistered"
    );

    // now feed protected fds (the shell contract) → the dial retry succeeds
    signal_mock::feed_tcp_fd(&a.signal_source);
    signal_mock::feed_tcp_fd(&a.signal_source);
    wait_for_box("A registered after feed", 10, || a.registered_count() >= 1).await;
    assert!(a.exchange.is_registered());

    // the SAME pump now flushes the retained offer → convergence
    let converged = pump_until(&mut a, &mut b, &mut now, 360_000, |a, b| {
        a.status_of(&key_b).state == PeerIceState::Connected
            && b.status_of(&key_a).state == PeerIceState::Connected
    })
    .await;
    assert!(converged, "retained offer must be delivered, not dropped");

    // exactly ONE offer total: the frame queued during the refused window is
    // the one that got delivered (retention), not a fresh one after drop
    let offers = sink_records(&mock)
        .into_iter()
        .filter(|(t, _)| *t == Type::Offer as i32)
        .count();
    assert_eq!(offers, 1, "retained frame delivered exactly once");

    a.link.abort();
    b.link.abort();
}

/// 断流重连：kill the transport → Broken → reconnect takes a FRESH
/// protected fd and the server sees a SECOND registration header; ICE
/// sessions are KEPT (Connected survives the signal outage, per the
/// connector semantics), signal_ready re-arms, and the signal path carries
/// frames again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transport_break_reconnects_re_registers_and_re_takes_protected_fd() {
    let mock = SignalMock::default();
    let keys_a = EnvelopeKeyPair::generate().expect("keys A");
    let keys_b = EnvelopeKeyPair::generate().expect("keys B");
    let (addr, counter) = spawn_signal_mock(mock.clone()).await;

    let mut a = spawn_node(
        keys_a.clone(),
        &keys_b.public_key_base64(),
        0x1111,
        addr,
        mock.seen_ids.clone(),
        false,
        8,
    );
    let mut b = spawn_node(
        keys_b.clone(),
        &keys_a.public_key_base64(),
        0x2222,
        addr,
        mock.seen_ids.clone(),
        true,
        8,
    );
    let key_a = a.key.clone();
    let key_b = b.key.clone();

    let mut now = 1000u64;
    let converged = pump_until(&mut a, &mut b, &mut now, 360_000, |a, b| {
        a.status_of(&key_b).state == PeerIceState::Connected
            && b.status_of(&key_a).state == PeerIceState::Connected
    })
    .await;
    assert!(converged, "precondition: converged over the real signal path");
    let taken_before = a.signal_source.taken();
    let registered_before = a.registered_count();

    // transport kill → Broken → (fresh fd) → re-register. The kill path
    // rides the upstream 800 ms backoff plus real-time EOF detection — a
    // wide box for full-suite CPU contention.
    counter.kill_all_connections();
    wait_for_box("A Broken event", 20, || {
        a.events.lock().expect("events").contains(&EventRec::Broken)
    })
    .await;
    assert!(
        !a.orch.lock().expect("orch").signal_ready(),
        "signal_ready must drop while the stream is down"
    );
    wait_for_box("A re-registered", 20, || {
        a.registered_count() >= registered_before + 1
    })
    .await;
    wait_for_box("B re-registered", 20, || b.registered_count() >= 2).await;
    assert!(a.exchange.is_registered(), "re-registered");
    assert!(
        a.orch.lock().expect("orch").signal_ready(),
        "signal_ready re-armed by the second Registered"
    );

    // audit: taken == fresh fd per reconnect (≥ attempts), and the SERVER
    // saw one more registration header for our key
    assert!(
        a.signal_source.taken() > taken_before,
        "the reconnect re-took a protected fd ({} → {})",
        taken_before,
        a.signal_source.taken()
    );
    let registrations = mock
        .seen_ids
        .lock()
        .expect("seen")
        .iter()
        .filter(|k| **k == key_a)
        .count();
    assert!(registrations >= 2, "server saw the re-registration ({registrations})");

    // ICE sessions were kept across the signal outage (connector semantics:
    // signal down ≠ peer teardown; checks/keepalive live on the ICE sockets)
    assert_eq!(a.status_of(&key_b).state, PeerIceState::Connected);
    assert_eq!(b.status_of(&key_a).state, PeerIceState::Connected);

    // the signal path carries frames again: B re-signals a duplicate
    // candidate (B's own ICE check-socket, from A's landed endpoint) through
    // its re-registered exchange; A receives it end-to-end
    let (_, _, b_port) = *a
        .wg
        .calls()
        .first()
        .expect("A landed B's endpoint before the kill");
    let cand = Candidate::host_candidate([127, 0, 0, 1], b_port);
    let dup = cand.marshal();
    b.exchange
        .send(&key_a, PeerSignalKind::Candidate, &dup, 0)
        .expect("post-reconnect send must be accepted (registered)");
    wait_for("A received the post-reconnect candidate", || {
        a.events.lock().expect("events").iter().any(
            |e| matches!(e, EventRec::Message { kind, payload }
                    if *kind == Type::Candidate as i32 && payload == &dup),
        )
    })
    .await;
    assert!(
        !a.events.lock().expect("events").contains(&EventRec::Malformed),
        "a clean exchange must not surface malformed frames"
    );

    a.link.abort();
    b.link.abort();
}

/// 空源 fail-closed：an EMPTY protected fd source hands out NOTHING (the
/// provider audit: taken == 0), the server accepts NO connection for that
/// peer (no unprotected fallback), and the link never registers — ICE stays
/// Idle no matter how long the orchestrator is pumped.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_signal_socket_source_fails_closed_without_any_dial() {
    let mock = SignalMock::default();
    let keys_a = EnvelopeKeyPair::generate().expect("keys A");
    let keys_b = EnvelopeKeyPair::generate().expect("keys B");
    let (addr, counter) = spawn_signal_mock(mock.clone()).await;

    // A starves (0 fds); B is fed and registers (control side)
    let a = spawn_node(
        keys_a.clone(),
        &keys_b.public_key_base64(),
        0x1111,
        addr,
        mock.seen_ids.clone(),
        false,
        0,
    );
    let b = spawn_node(
        keys_b.clone(),
        &keys_a.public_key_base64(),
        0x2222,
        addr,
        mock.seen_ids.clone(),
        true,
        4,
    );
    let key_a = a.key.clone();
    let key_b = b.key.clone();

    wait_for("B registered", || b.registered_count() >= 1).await;
    // A's link fails its dial CLOSED (provider empty)
    wait_for("A dial failure recorded", || {
        a.events.lock().expect("events").iter().any(|e| matches!(e, EventRec::DialFailed))
    })
    .await;
    assert!(!a.exchange.is_registered(), "no fds → never registered");
    assert!(!a.orch.lock().expect("orch").signal_ready(), "ICE must stay unarmed");
    assert_eq!(
        a.signal_source.taken(),
        0,
        "the empty provider must hand out NOTHING (taken == 0)"
    );

    // even pumped (and forced ready), A never sends: the seam refuses and
    // the server never sees an unprotected connection for A
    a.orch.lock().expect("orch").set_signal_ready(true);
    let mut now = 1000u64;
    for _ in 0..20 {
        a.sim_now.store(now, Ordering::Release);
        let _ = a.orch.lock().expect("orch").run_once(now);
        now += 10;
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(
        a.status_of(&key_b).state,
        PeerIceState::Idle,
        "unregistered link → no offers, peer stays Idle"
    );
    assert!(!a.exchange.is_registered(), "still unregistered after pumping");
    // the forced-ready pump DID gather ICE candidates locally (its ICE fds
    // were used), but nothing crossed the signal seam
    assert!(a.ice_socks.taken() >= 1, "gather consumed its own protected ICE fds");
    let seen = mock.seen_ids.lock().expect("seen");
    assert!(
        !seen.contains(&key_a),
        "the server must NEVER see A (no unprotected fallback): {seen:?}"
    );
    drop(seen);
    assert!(
        (a.signal_source.taken() + b.signal_source.taken()) as usize >= counter.accepted(),
        "every accepted connection rode a fed socket"
    );

    a.link.abort();
    b.link.abort();
}
