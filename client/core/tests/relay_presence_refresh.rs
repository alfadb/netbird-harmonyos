// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! Relay presence-refresh defect reproduction (read-only root-cause
//! analysis: `docs/relay-presence-refresh-analysis-20260916.md`).
//!
//! The upstream relay server (NetBird 0.78.1) keeps presence interests
//! asymmetric: the `PeersOnline` interest is ONE-SHOT (a came-online event
//! is delivered to a subscriber once, then consumed — only a NEW
//! `SubscribePeerState` re-arms it), while the `PeersWentOffline` interest
//! is PERSISTENT. An upstream CLIENT recovers by unsubscribing + closing the
//! per-peer connection on `PeersWentOffline` and re-running `OpenConn` on
//! demand. Our port lost that second half: `PeersWentOffline` only writes
//! `presence=false`, the lane orchestrator short-circuits on an attached
//! lane, so after the remote bounces once (offline→online) the presence
//! cache sticks at `false` and every relay datagram for that peer is
//! refused locally — the device evidence showed exactly this as
//! `carrier-reject|reason=relay-peer-offline` (analysis arm A2).
//!
//! The LEGACY fake-server semantics (interest table consumed by the first
//! event, no interest for online targets) structurally cannot reproduce
//! that sequence (analysis §6「前置发现」), so the repro below runs the fake
//! server in `TestServerConfig::faithful_presence` mode, which replicates
//! the 0.78.1 store behavior.
//!
//! Test inventory:
//! 1. [`peer_returning_online_must_restore_the_relay_lane`] — THE defect
//!    reproduction. Asserts the CORRECT end-to-end behavior (presence
//!    restored, lane re-opened, frames flowing again after the remote
//!    re-authenticates). **Currently RED** by design: it may only turn
//!    GREEN once the client stack recovers the lane on `PeersWentOffline`.
//! 2. [`lane_that_never_bounces_keeps_flowing`] — control case: the same
//!    harness without the bounce keeps flowing, isolating the
//!    offline→online cycle as the trigger.
//! 3. [`faithful_server_peers_online_is_one_shot_and_pwo_is_persistent`] —
//!    locks the fake server's faithful semantics at the wire level (the
//!    second `PeersOnline` must NOT arrive; every PWO must).
//! 4. [`set_peer_online_scripts_presence_cycles_deterministically`] — the
//!    `set_peer_online` control plane drives the same one-shot asymmetry
//!    without any TCP churn.
//! 5. [`legacy_mode_keeps_the_pre_faithful_semantics`] — pins the
//!    `faithful_presence = false` side of the knob (the pre-faithful
//!    behavior the established suites were written against).
//!
//! Determinism discipline: the relay client runs on the injected
//! [`VirtualClock`], which is NEVER advanced — no keepalive death and no
//! 30s `open_conn` timeout can fire, so an accidental session reset (the
//! one path that clears presence) cannot mask the defect. Every wait is a
//! bounded predicate loop over an event that MUST happen; negative
//! assertions are bounded windows whose expiry is the pass condition. No
//! sleeps as assertions.

// Host-process link stubs (repo convention, cf.
// tests/relay_connector_e2e.rs): libace_napi.z.so / libhilog_ndk.z.so do
// not exist on the host. No-ops for the linker only; never called by these
// tests.
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
        _str_: *const c_void,
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
        _data: *mut *mut c_void,
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
    pub extern "C" fn napi_get_value_int32(_value: *mut c_void, _result: *mut i32) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(_value: *mut c_void, _result: *mut bool) -> i32 {
        0
    }
}

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use netbird_core::connector::{RelayCarrier, WgPeerApplier, WgPeerEntry};
use netbird_core::relay::{
    AuthToken, Frame, PeerId, MSG_AUTH, MSG_PEERS_ONLINE, MSG_SUBSCRIBE_PEER_STATE,
};
use netbird_core::relay_client::{RelayClient, RelayClientConfig, RelayState, VirtualClock};
use netbird_core::relay_testserver::{RelayTestServer, TestServerConfig};
use netbird_core::wg_device::WgEgressCarrier;
use netbird_core::ws::{client_handshake, random_sec_websocket_key, WsClient, WsMessage};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Fuse for every wait on an event that MUST happen. HANG GUARD, not a
/// latency assertion (loopback events take microseconds; the fuse only
/// bounds a stuck scheduler). Generous (60s) so a fully loaded parallel
/// `cargo test` run never trips it spuriously.
const FUSE: Duration = Duration::from_secs(60);

/// Bound for the LANE-RECOVERY wait specifically. The recovery is an
/// orchestration loop, not a single frame: the production pump re-opens a
/// lane within one pass (500ms) of the presence flip, and the re-open
/// itself is a loopback subscribe + immediate `PeersOnline` (sub-ms). 10s
/// is ≈20 production passes — a stuck recovery (the defect) fails within
/// seconds, while a loaded CI still never false-trips.
const RECOVERY_FUSE: Duration = Duration::from_secs(10);

// Fabricated (NOT secret) token parts — same shape as the relay.rs fixtures.
const SIG_B64: &str = "paWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaU=";

/// Fabricated virtual epoch: 1770000000 (2026-02-02).
const EPOCH: u64 = 1_770_000_000;

fn token_valid() -> AuthToken {
    AuthToken::from_management(&(EPOCH + 3_600).to_string(), SIG_B64)
        .expect("fabricated token is valid")
}

/// Fabricated WG public key of the OBSERVING client (the lane owner — the
/// role of the nbinterop host in analysis arm A2).
const LOCAL_KEY: &str = "presence-refresh-local-observer-wgkey";

/// Fabricated WG public key of the REMOTE peer (the role of the bouncing
/// device in arm A2: same peer id before and after the reconnect).
const REMOTE_KEY: &str = "presence-refresh-remote-bouncer-wgkey";

/// Fabricated WG public key for the control-plane scripted peer.
const SCRIPTED_KEY: &str = "presence-refresh-scripted-peer-wgkey";

fn local_id() -> PeerId {
    PeerId::from_wg_pubkey_string(LOCAL_KEY)
}

fn remote_id() -> PeerId {
    PeerId::from_wg_pubkey_string(REMOTE_KEY)
}

fn subscriber_id() -> PeerId {
    PeerId::from_wg_pubkey_string("presence-refresh-subscriber-wgkey")
}

async fn start_faithful() -> RelayTestServer {
    RelayTestServer::start(TestServerConfig {
        faithful_presence: true,
        ..TestServerConfig::default()
    })
    .await
    .expect("bind 127.0.0.1:0")
}

async fn start_legacy() -> RelayTestServer {
    RelayTestServer::start(TestServerConfig {
        faithful_presence: false,
        ..TestServerConfig::default()
    })
    .await
    .expect("bind 127.0.0.1:0")
}

/// The relay client under test: REAL stack (TcpDialer + WS + session) over
/// real loopback TCP, on the injected (never-advanced) virtual clock.
fn start_client(server: &RelayTestServer, clock: &VirtualClock) -> RelayClient {
    let cfg = RelayClientConfig::new(
        &[format!("rel://127.0.0.1:{}", server.addr().port())],
        LOCAL_KEY,
        token_valid(),
    )
    .expect("relay config parses")
    .with_clock(Arc::new(clock.clone()));
    RelayClient::start(cfg).expect("relay client starts")
}

/// A raw authenticated WS client with an arbitrary peer id (handshake +
/// Auth + AuthResponse — returns only after the server processed the auth).
async fn connect_raw(server: &RelayTestServer, id: &PeerId) -> WsClient<TcpStream> {
    let tcp = TcpStream::connect(server.addr()).await.expect("raw client dials");
    let key = random_sec_websocket_key().expect("urandom key");
    let mut ws = client_handshake(tcp, &server.addr().to_string(), "/relay", &key)
        .await
        .expect("ws handshake must get 101");
    ws.write_binary(
        &Frame::Auth { peer_id: id.clone(), token: token_valid() }
            .encode()
            .expect("auth encodes"),
    )
    .await
    .expect("auth send");
    match ws.read_message().await.expect("auth response") {
        WsMessage::Binary(bytes) => assert!(
            matches!(Frame::decode(&bytes), Ok(Frame::AuthResponse { .. })),
            "raw client must authenticate first"
        ),
        other => panic!("expected AuthResponse, got {other:?}"),
    }
    ws
}

/// The bouncing remote (arm-A2 device role): a fresh TCP+WS+Auth with the
/// SAME peer id — exactly what a reconnecting relay client looks like to
/// the server. Callers must hold the returned connection for as long as the
/// peer is meant to stay ONLINE (dropping it closes the TCP conn and the
/// server broadcasts went-offline — the real lifecycle).
async fn connect_remote(server: &RelayTestServer) -> WsClient<TcpStream> {
    connect_raw(server, &remote_id()).await
}

async fn send_frame(ws: &mut WsClient<TcpStream>, frame: Frame) {
    ws.write_binary(&frame.encode().expect("frame encodes")).await.expect("frame on the wire");
}

/// Read the next relay frame (one WS binary message == one frame, §2.6).
async fn expect_frame(ws: &mut WsClient<TcpStream>, what: &str) -> Frame {
    loop {
        let msg = timeout(FUSE, ws.read_message())
            .await
            .expect("fuse: waiting for a frame")
            .expect("ws message");
        match msg {
            WsMessage::Binary(bytes) => return Frame::decode(&bytes).expect("valid frame"),
            WsMessage::Pong(_) => continue, // WS liveness, not a relay frame
            other => panic!("unexpected ws message while waiting for {what}: {other:?}"),
        }
    }
}

/// Bounded predicate wait (no sleeps as assertions; fuse expiry = failure).
async fn wait_for(fuse: Duration, mut pred: impl FnMut() -> bool, what: &str) {
    match timeout(fuse, async {
        while !pred() {
            tokio::task::yield_now().await;
        }
    })
    .await
    {
        Ok(()) => {}
        Err(_) => panic!("condition not met within fuse: {what}"),
    }
}

/// Bounded negative window (repo convention, cf.
/// tests/relay_connector_e2e.rs): expiry IS the pass condition.
async fn assert_stays_false(window: Duration, mut pred: impl FnMut() -> bool, what: &str) {
    let deadline = std::time::Instant::now() + window;
    while std::time::Instant::now() < deadline {
        assert!(!pred(), "condition that must never happen occurred: {what}");
        tokio::task::yield_now().await;
    }
}

/// Recording WG seam: captures every attached lane handle (the device's
/// `attach_carrier` side) plus inbound carrier frames. `latest` is the LAST
/// handle attached — what the WG device would actually hold.
#[derive(Default)]
struct RecordingSeam {
    attached: Mutex<Vec<String>>,
    detached: Mutex<Vec<String>>,
    latest: Mutex<Option<Arc<dyn WgEgressCarrier>>>,
    inbound_frames: AtomicU64,
}

impl WgPeerApplier for RecordingSeam {
    fn apply_peers(&self, _peers: &[WgPeerEntry]) -> Result<(), String> {
        Ok(())
    }
    fn clear(&self) {}
    fn carrier_capable(&self) -> bool {
        true
    }
    fn attach_carrier(
        &self,
        pub_key_b64: &str,
        carrier: Arc<dyn WgEgressCarrier>,
    ) -> Result<(), String> {
        self.attached.lock().expect("attached lock").push(pub_key_b64.to_string());
        *self.latest.lock().expect("latest lock") = Some(carrier);
        Ok(())
    }
    fn detach_carrier(&self, pub_key_b64: &str) {
        self.detached.lock().expect("detached lock").push(pub_key_b64.to_string());
    }
    fn handle_carrier_inbound(&self, _datagram: &[u8], _pub_key_b64: &str, _now_ms: u64) -> usize {
        self.inbound_frames.fetch_add(1, Ordering::SeqCst);
        0
    }
}

/// Mirrors the production `relay_carrier_pump` contract through the PUBLIC
/// orchestration surface only: while the relay is Ready, keep every mapped
/// lane ensured (`ensure_lane` per pass — the lane universe here is one
/// peer); drain inbound frames between passes. The 20ms pass pace is a CPU
/// bound, never a correctness input (the production pump waits 500ms);
/// every assertion below waits on events, not on passes. Any lane-recovery
/// fix must surface through this contract (presence-aware `ensure_lane` or
/// the pump's per-pass reconcile) to be observable by the WG seam.
async fn pump_loop(carrier: Arc<RelayCarrier>, client: RelayClient, key: &'static str) {
    loop {
        match client.state() {
            RelayState::Dead => return,
            RelayState::Ready => {
                let _ = carrier.ensure_lane(key).await;
            }
            _ => {}
        }
        let drain = async {
            loop {
                if !carrier.pump_once().await {
                    return;
                }
            }
        };
        tokio::select! {
            _ = drain => return,
            _ = tokio::time::sleep(Duration::from_millis(20)) => {}
        }
    }
}

/// Lane setup shared by tests 1+2: client Ready → carrier armed → pump
/// running → remote joins → lane attached. The remote connection is
/// RETURNED and must be held by the caller (dropping it = the peer going
/// offline — the real lifecycle).
///
/// This is the subscribe-while-OFFLINE shape: `open_conn` blocks in the
/// §4.1 blocking-wait until the came-online event delivers the ONE
/// `PeersOnline` — and that delivery CONSUMES the one-shot online interest,
/// precisely the interest state analysis arm A2 was left in.
async fn arm_lane_with_bouncing_remote(
    server: &RelayTestServer,
) -> (
    RelayClient,
    Arc<RecordingSeam>,
    Arc<RelayCarrier>,
    tokio::task::JoinHandle<()>,
    WsClient<TcpStream>,
) {
    let clock = VirtualClock::new(EPOCH);
    let client = start_client(server, &clock);
    wait_for(FUSE, || client.state() == RelayState::Ready, "relay client Ready").await;

    let seam = Arc::new(RecordingSeam::default());
    let carrier = RelayCarrier::new(seam.clone());
    carrier.attach_client(client.clone());
    carrier.set_peers(&[REMOTE_KEY.to_string()]);
    let pump = tokio::spawn(pump_loop(carrier.clone(), client.clone(), REMOTE_KEY));

    let remote = connect_remote(server).await;
    wait_for(
        FUSE,
        || !seam.attached.lock().expect("attached lock").is_empty(),
        "lane attached after the remote came online",
    )
    .await;
    wait_for(
        FUSE,
        || server.stats().frames_rx(MSG_SUBSCRIBE_PEER_STATE) == 1,
        "exactly one SubscribePeerState so far",
    )
    .await;
    assert_eq!(
        client.stats().frames_rx[MSG_PEERS_ONLINE as usize],
        1,
        "the came-online event delivered the one PeersOnline"
    );
    (client, seam, carrier, pump, remote)
}

// ---------------------------------------------------------------------------
// 1. THE DEFECT: offline→online must restore the lane (currently RED)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn peer_returning_online_must_restore_the_relay_lane() {
    let server = start_faithful().await;
    let (client, seam, _carrier, pump, mut remote) = arm_lane_with_bouncing_remote(&server).await;

    // --- baseline: frames flow BOTH directions through the attached lane.
    let lane = seam.latest.lock().expect("latest lock").clone().expect("lane handle");
    let probe_before = b"presence-probe-before-bounce".to_vec();
    lane.send_datagram(&probe_before).expect("pre-bounce egress accepted");
    match expect_frame(&mut remote, "pre-bounce forwarded transport").await {
        Frame::Transport { peer_id, payload } => {
            assert_eq!(peer_id, local_id(), "§4.2: receiver sees the SENDER id");
            assert_eq!(payload, probe_before, "egress byte-exact");
        }
        other => panic!("expected Transport, got {other:?}"),
    }
    remote
        .write_binary(
            &Frame::Transport { peer_id: local_id(), payload: b"ingress-probe".to_vec() }
                .encode()
                .expect("transport encodes"),
        )
        .await
        .expect("ingress send");
    wait_for(
        FUSE,
        || seam.inbound_frames.load(Ordering::SeqCst) == 1,
        "ingress reached the WG seam",
    )
    .await;

    // --- the bounce, first half: the remote DROPS. The faithful server's
    // PERSISTENT offline interest delivers PeersWentOffline → the client
    // writes presence[remote] = false.
    assert!(server.drop_connection(&remote_id()), "remote connection tracked");
    wait_for(
        FUSE,
        || client.stats().peers_went_offline_rx == 1,
        "PeersWentOffline delivered to the client",
    )
    .await;
    // While the peer IS offline the typed refusal is correct — and must be
    // exactly the class token the device evidence shows.
    let probe_offline = b"while-offline".to_vec();
    wait_for(
        FUSE,
        || {
            matches!(
                lane.send_datagram(&probe_offline),
                Err(ref reason) if reason == "relay-peer-offline"
            )
        },
        "offline egress refused as relay-peer-offline (correct while offline)",
    )
    .await;

    // --- the bounce, second half: the remote RE-AUTHENTICATES on the same
    // server with the same peer id. The observer's relay session NEVER
    // resets (arm A2: state=ready, reconnects unchanged throughout).
    let mut remote = connect_remote(&server).await;
    // 3 auths total: our client + remote join + remote re-join. (If the
    // observer's own session ever reconnected, this tripwire fails loudly.)
    wait_for(FUSE, || server.stats().frames_rx(MSG_AUTH) == 3, "remote re-authenticated").await;

    // --- CORRECT behavior: the lane must recover — a fresh SubscribePeerState
    // re-arms the server interest (upstream client lifecycle: PWO →
    // unsubscribe → on-demand OpenConn), PeersOnline restores presence, and
    // egress is accepted + delivered again.
    // TODAY: presence sticks at false forever (no re-subscribe is ever
    // sent — the PWO handler is inert and the orchestrator short-circuits
    // on the attached lane), so this fails with relay-peer-offline.
    const RECOVERY_PROBE: &[u8] = b"presence-recovery-probe-after-return";
    let mut last_reason = String::new();
    let recovered = timeout(RECOVERY_FUSE, async {
        loop {
            let lane = seam.latest.lock().expect("latest lock").clone().expect("lane handle");
            match lane.send_datagram(RECOVERY_PROBE) {
                Ok(()) => return,
                Err(reason) => last_reason = reason,
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    if recovered.is_err() {
        panic!(
            "DEFECT: the remote re-authenticated on the same relay server, but the lane never \
             recovered — egress still refused with class token {last_reason:?} (server saw {} \
             SubscribePeerState frames, client received {} PeersOnline frames; correct behavior \
             is a fresh subscribe + PeersOnline + accepted egress)",
            server.stats().frames_rx(MSG_SUBSCRIBE_PEER_STATE),
            client.stats().frames_rx[MSG_PEERS_ONLINE as usize],
        );
    }

    // Recovery shape (reached once the client stack is fixed): the lane was
    // RE-OPENED (fresh subscribe — the upstream lifecycle), and the remote
    // receives the recovered egress byte-exact.
    wait_for(
        FUSE,
        || server.stats().frames_rx(MSG_SUBSCRIBE_PEER_STATE) == 2,
        "a fresh SubscribePeerState re-armed the server-side interest",
    )
    .await;
    assert_eq!(
        client.stats().frames_rx[MSG_PEERS_ONLINE as usize],
        2,
        "presence restored by a second PeersOnline"
    );
    match expect_frame(&mut remote, "recovered forwarded transport").await {
        Frame::Transport { payload, .. } => assert_eq!(payload, RECOVERY_PROBE.to_vec()),
        other => panic!("expected the recovered Transport, got {other:?}"),
    }

    pump.abort();
}

// ---------------------------------------------------------------------------
// 2. control case: no bounce → the lane keeps flowing
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn lane_that_never_bounces_keeps_flowing() {
    let server = start_faithful().await;
    let (client, seam, _carrier, pump, mut remote) = arm_lane_with_bouncing_remote(&server).await;

    // Sustained bidirectional flow (two exchanges, both directions).
    for round in 0..2u8 {
        let out = format!("control-egress-{round}").into_bytes();
        let lane = seam.latest.lock().expect("latest lock").clone().expect("lane handle");
        lane.send_datagram(&out).expect("egress accepted");
        match expect_frame(&mut remote, "forwarded egress").await {
            Frame::Transport { peer_id, payload } => {
                assert_eq!(peer_id, local_id());
                assert_eq!(payload, out, "round {round} byte-exact");
            }
            other => panic!("expected Transport, got {other:?}"),
        }
        remote
            .write_binary(
                &Frame::Transport {
                    peer_id: local_id(),
                    payload: format!("control-ingress-{round}").into_bytes(),
                }
                .encode()
                .expect("transport encodes"),
            )
            .await
            .expect("ingress send");
    }
    wait_for(
        FUSE,
        || seam.inbound_frames.load(Ordering::SeqCst) == 2,
        "both ingress frames arrived",
    )
    .await;

    // Bounded negative window: with the remote never dropping, nothing is
    // ever refused and no PWO ever arrives (stats-based predicate — the
    // window itself performs no egress).
    assert_stays_false(
        Duration::from_millis(500),
        || {
            let s = client.stats();
            s.peers_went_offline_rx > 0 || s.send_rejected_offline > 0
        },
        "a lane whose peer never bounced must never refuse or observe offline",
    )
    .await;
    // And the wire still flows after the window.
    let lane = seam.latest.lock().expect("latest lock").clone().expect("lane handle");
    lane.send_datagram(b"control-final").expect("egress still accepted after the window");
    assert_eq!(
        server.stats().frames_rx(MSG_SUBSCRIBE_PEER_STATE),
        1,
        "no re-subscribe without a bounce (the lane never closed)"
    );

    pump.abort();
}

// ---------------------------------------------------------------------------
// 3. faithful server semantics: PeersOnline one-shot, PWO persistent
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn faithful_server_peers_online_is_one_shot_and_pwo_is_persistent() {
    let server = start_faithful().await;
    let mut subscriber = connect_raw(&server, &subscriber_id()).await;

    // Subscribe while the target is OFFLINE: §4.1 blocking-wait, silence
    // proven by the FIFO probe (any wrongly-sent PeersOnline would precede
    // the HealthCheck echo on this single FIFO connection).
    send_frame(&mut subscriber, Frame::SubscribePeerState { peer_ids: vec![remote_id()] }).await;
    send_frame(&mut subscriber, Frame::HealthCheck).await;
    assert_eq!(
        expect_frame(&mut subscriber, "silence probe").await,
        Frame::HealthCheck,
        "no PeersOnline for an offline target (§4.1)"
    );

    // The target joins → came-online event → PeersOnline #1 is delivered…
    // (the binding only HOLDS the connection alive — dropping it would be
    // the peer going offline through the real teardown path).
    let _remote = connect_remote(&server).await;
    assert_eq!(
        expect_frame(&mut subscriber, "first came-online").await,
        Frame::PeersOnline { peer_ids: vec![remote_id()] },
        "the one-shot online interest delivers once"
    );

    // …and CONSUMED. First bounce: the PERSISTENT offline interest still
    // delivers the PWO.
    assert!(server.drop_connection(&remote_id()));
    assert_eq!(
        expect_frame(&mut subscriber, "first went-offline").await,
        Frame::PeersWentOffline { peer_ids: vec![remote_id()] },
        "the offline interest survives the came-online delivery"
    );

    // The target re-authenticates: NO second PeersOnline may arrive (the
    // online interest was consumed; only a NEW subscribe re-arms it).
    let _remote = connect_remote(&server).await;
    send_frame(&mut subscriber, Frame::HealthCheck).await;
    assert_eq!(
        expect_frame(&mut subscriber, "second came-online probe").await,
        Frame::HealthCheck,
        "the second came-online must stay SILENT (one-shot online interest)"
    );

    // Second bounce: PWO again — the offline interest is persistent forever.
    assert!(server.drop_connection(&remote_id()));
    assert_eq!(
        expect_frame(&mut subscriber, "second went-offline").await,
        Frame::PeersWentOffline { peer_ids: vec![remote_id()] },
        "every went-offline is delivered (persistent interest)"
    );

    // Counter reconciliation: one subscribe from the subscriber, three auths
    // (subscriber + two remote joins).
    let stats = server.stats();
    assert_eq!(stats.frames_rx(MSG_SUBSCRIBE_PEER_STATE), 1);
    assert_eq!(stats.frames_rx(MSG_AUTH), 3);

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// 4. control plane: set_peer_online scripts the same semantics
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn set_peer_online_scripts_presence_cycles_deterministically() {
    let server = start_faithful().await;
    let mut subscriber = connect_raw(&server, &subscriber_id()).await;
    let scripted = PeerId::from_wg_pubkey_string(SCRIPTED_KEY);

    // No-op forms report false; transitions report true.
    assert!(!server.set_peer_online(&scripted, false), "already offline: no-op");

    // Scripted came-online (no backing connection — presence only).
    assert!(server.set_peer_online(&scripted, true), "came-online transition fires");
    send_frame(
        &mut subscriber,
        Frame::SubscribePeerState { peer_ids: vec![scripted.clone()] },
    )
    .await;
    assert_eq!(
        expect_frame(&mut subscriber, "scripted online immediate answer").await,
        Frame::PeersOnline { peer_ids: vec![scripted.clone()] },
        "online target answered immediately (interest kept)"
    );

    // Cycle 1: offline → PWO; online again → the FIRST recovery still
    // delivers (the immediate answer did not consume the one-shot interest).
    assert!(server.set_peer_online(&scripted, false), "went-offline transition fires");
    assert_eq!(
        expect_frame(&mut subscriber, "scripted first PWO").await,
        Frame::PeersWentOffline { peer_ids: vec![scripted.clone()] },
    );
    assert!(server.set_peer_online(&scripted, true));
    assert_eq!(
        expect_frame(&mut subscriber, "scripted first recovery").await,
        Frame::PeersOnline { peer_ids: vec![scripted.clone()] },
        "first offline→online cycle still delivers PeersOnline"
    );

    // Cycle 2: PWO again (persistent); online → SILENT (consumed in cycle 1).
    assert!(server.set_peer_online(&scripted, false));
    assert_eq!(
        expect_frame(&mut subscriber, "scripted second PWO").await,
        Frame::PeersWentOffline { peer_ids: vec![scripted.clone()] },
    );
    assert!(server.set_peer_online(&scripted, true));
    send_frame(&mut subscriber, Frame::HealthCheck).await;
    assert_eq!(
        expect_frame(&mut subscriber, "scripted second recovery probe").await,
        Frame::HealthCheck,
        "the second scripted recovery must stay SILENT"
    );

    // Idempotence bookkeeping: already-online is a no-op; the final offline
    // transition fires once more, then further offs are no-ops.
    assert!(!server.set_peer_online(&scripted, true), "already online: no-op");
    assert!(server.set_peer_online(&scripted, false));
    assert!(!server.set_peer_online(&scripted, false), "already offline: no-op");

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// 5. the legacy knob side: pre-faithful semantics stay pinned
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn legacy_mode_keeps_the_pre_faithful_semantics() {
    let server = start_legacy().await;
    let mut subscriber = connect_raw(&server, &subscriber_id()).await;
    // The remote's binding only holds its connection alive (dropping it is
    // the real went-offline path).
    let _remote = connect_remote(&server).await;

    // LEGACY: subscribing an ONLINE target answers immediately and
    // registers NO interest at all.
    send_frame(&mut subscriber, Frame::SubscribePeerState { peer_ids: vec![remote_id()] }).await;
    assert_eq!(
        expect_frame(&mut subscriber, "legacy immediate answer").await,
        Frame::PeersOnline { peer_ids: vec![remote_id()] },
    );

    // Target drops: no interest existed → NO PWO (FIFO probe).
    assert!(server.drop_connection(&remote_id()));
    send_frame(&mut subscriber, Frame::HealthCheck).await;
    assert_eq!(
        expect_frame(&mut subscriber, "legacy drop probe").await,
        Frame::HealthCheck,
        "legacy mode must stay silent on the drop (pre-faithful behavior)"
    );

    // Target returns: no interest existed → NO PeersOnline (FIFO probe).
    let _remote = connect_remote(&server).await;
    send_frame(&mut subscriber, Frame::HealthCheck).await;
    assert_eq!(
        expect_frame(&mut subscriber, "legacy return probe").await,
        Frame::HealthCheck,
        "legacy mode must stay silent on the return (pre-faithful behavior)"
    );

    server.shutdown().await;
}
