// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N13-C relay CLIENT end-to-end tests: the real client state machine
//! ([`netbird_core::relay_client`]) driven over real loopback TCP
//! (`127.0.0.1:0`) against the in-process fake relay server
//! ([`netbird_core::relay_testserver`], unchanged from increment B), with
//! the client's clock INJECTED ([`VirtualClock`]) so every deadline — the
//! 35s keepalive death, the 8s auth budget, the 30s `open_conn` timeout and
//! the exact 2s×2 backoff sequence — is exercised deterministically with
//! zero real waiting.
//!
//! Determinism discipline: no sleeps as assertions. Every positive wait is
//! a bounded predicate loop (`wait_for`, yield/fuse based) over an event
//! that MUST happen; the single negative assertion (no `PeersOnline` for an
//! offline peer) is discharged by driving the injected clock past the
//! `open_conn` deadline — the ONLY two ways `open_conn` can return are the
//! deadline and a real `PeersOnline`, and the counter evidence shows the
//! latter never arrived. The one exception is the server-side echo counter
//! in `healthcheck_echo_reaches_the_server`, which has no ordering
//! substitute; it uses a bounded poll-with-fuse and treats fuse expiry as
//! failure. All token/peer values are fabricated; no token content is ever
//! printed.

use std::time::{Duration, Instant};

// Host-process link stubs (repo convention, cf. tests/relay_e2e.rs):
// libace_napi.z.so / libhilog_ndk.z.so do not exist on the host. No-ops for
// the linker only; never called by these tests.
mod host_link_stubs {
    #[no_mangle]
    pub extern "C" fn napi_module_register(_mod_: *mut core::ffi::c_void) {}

    #[no_mangle]
    pub extern "C" fn napi_create_function(
        _env: *mut core::ffi::c_void,
        _utf8name: *const u8,
        _length: usize,
        _cb: *const u8,
        _data: *mut core::ffi::c_void,
        _result: *mut *mut core::ffi::c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_set_named_property(
        _env: *mut core::ffi::c_void,
        _name: *const core::ffi::c_char,
        _value: *mut core::ffi::c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _env: *mut core::ffi::c_void,
        _str_: *const u8,
        _len: usize,
        _result: *mut *mut core::ffi::c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_cb_info(
        _env: *mut core::ffi::c_void,
        _cbinfo: *mut core::ffi::c_void,
        _argc: *mut usize,
        _argv: *mut core::ffi::c_void,
        _data: *mut core::ffi::c_void,
        _result: *mut core::ffi::c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_string_utf8(
        _env: *mut core::ffi::c_void,
        _value: *mut core::ffi::c_void,
        _buf: *mut u8,
        _bufsize: usize,
        _result: *mut usize,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_int32(
        _env: *mut core::ffi::c_void,
        _value: *mut core::ffi::c_void,
        _result: *mut i32,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(
        _env: *mut core::ffi::c_void,
        _value: *mut core::ffi::c_void,
        _result: *mut core::ffi::c_void,
    ) -> i32 {
        0
    }
}

use netbird_core::relay::{
    AuthToken, Frame, PeerId, MAX_MESSAGE_SIZE, MSG_AUTH_RESPONSE, MSG_CLOSE, MSG_HEALTH_CHECK,
    MSG_PEERS_ONLINE, MSG_SUBSCRIBE_PEER_STATE, MSG_TRANSPORT, MSG_UNSUBSCRIBE_PEER_STATE,
};
use netbird_core::relay_client::{
    RelayClient, RelayClientConfig, RelayErrorClass, RelayState, TokenValidity, VirtualClock,
    MAX_TRANSPORT_PAYLOAD,
};
use netbird_core::relay_testserver::{RelayTestServer, TestServerConfig, DEFAULT_INSTANCE_URL};
use netbird_core::ws::{client_handshake, random_sec_websocket_key, WsMessage};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Real-time fuse for events that MUST happen (test scaffolding only — the
/// client under test runs on the injected virtual clock). This is a HANG
/// GUARD, not a latency assertion: every wait here targets an event that
/// happens in microseconds over loopback, and the fuse only bounds a stuck
/// scheduler/peer. Sized generously (60s) so full `cargo test` runs under
/// heavy CPU parallelism never trip it spuriously (observed flakes at 5s
/// under load); no behavioral assertion depends on its value.
const FUSE: Duration = Duration::from_secs(60);

// Fabricated (NOT secret) token parts — same shape as the relay.rs fixtures.
const SIG_B64: &str = "paWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaU=";

/// Fabricated virtual epoch: 1770000000 (2026-02-02, well inside the
/// fabricated-token domain).
const EPOCH: u64 = 1_770_000_000;

fn token(expires_at_unix: u64) -> AuthToken {
    AuthToken::from_management(&expires_at_unix.to_string(), SIG_B64)
        .expect("fabricated token is valid")
}

/// Long-lived fabricated token for tests that never touch expiry (+1h).
fn valid_token() -> AuthToken {
    token(EPOCH + 3_600)
}

/// Fabricated remote peer id.
fn remote_peer(tag: u8) -> PeerId {
    PeerId::from_wg_pubkey_string(&format!(
        "e2e-relay-remote-peer-{tag:02}-AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRo="
    ))
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

/// The local peer id a client started with seed `seed` derives (mirrors
/// `start_client`'s pubkey construction — one source of truth per test).
fn local_peer_for(seed: &str) -> PeerId {
    PeerId::from_wg_pubkey_string(&format!("e2e-relay-client-{seed}-wgkey"))
}

async fn start_default() -> RelayTestServer {
    RelayTestServer::start(TestServerConfig::default()).await.expect("bind 127.0.0.1:0")
}

/// Start a real client (TcpDialer + VirtualClock) against `urls`.
fn start_client(
    seed: &str,
    urls: &[String],
    tok: AuthToken,
    clock: &VirtualClock,
) -> RelayClient {
    let cfg = RelayClientConfig::new(urls, &format!("e2e-relay-client-{seed}-wgkey"), tok)
        .expect("e2e urls parse")
        .with_clock(std::sync::Arc::new(clock.clone()));
    RelayClient::start(cfg).expect("client starts")
}

fn loopback_url(port: u16) -> String {
    format!("rel://127.0.0.1:{port}")
}

// ---------------------------------------------------------------------------
// 1. auth success → Ready
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn auth_success_reaches_ready_with_full_observability() {
    let server = start_default().await;
    let clock = VirtualClock::new(EPOCH);
    let client = start_client("auth-ok", &[loopback_url(server.addr().port())], valid_token(), &clock);

    wait_for(FUSE, || client.state() == RelayState::Ready, "ready after auth").await;
    let stats = client.stats();
    assert_eq!(stats.auth_successes, 1);
    assert_eq!(
        stats.state_history,
        vec![
            RelayState::Disconnected,
            RelayState::Dialing,
            RelayState::Handshaking,
            RelayState::Authenticating,
            RelayState::Ready,
        ],
        "exact state path over real loopback"
    );
    assert_eq!(stats.current_url.as_deref(), Some(loopback_url(server.addr().port()).as_str()));
    assert_eq!(stats.instance_url.as_deref(), Some(DEFAULT_INSTANCE_URL));
    assert_eq!(stats.frames_tx[netbird_core::relay::MSG_AUTH as usize], 1);
    assert_eq!(stats.frames_rx[MSG_AUTH_RESPONSE as usize], 1);
    assert_eq!(server.stats().connections_accepted, 1);
    assert_eq!(server.stats().auth_rejected, 0);
    client.stop();
}

// ---------------------------------------------------------------------------
// 2. auth rejected (silent close, §3.4) → typed error, backoff, no addiction
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn auth_rejection_is_typed_and_retry_stays_on_the_exact_backoff() {
    let server = RelayTestServer::start(TestServerConfig { reject_auth: true, ..Default::default() })
        .await
        .unwrap();
    let clock = VirtualClock::new(EPOCH);
    let client = start_client(
        "auth-rej",
        &[loopback_url(server.addr().port())],
        valid_token(),
        &clock,
    );

    // Round 0 dials immediately; the server closes without AuthResponse.
    wait_for(FUSE, || {
        client.stats().last_error_class == Some(RelayErrorClass::AuthRejected)
    }, "typed auth rejection")
    .await;
    assert_eq!(client.stats().reconnects, 0, "never-Ready attempts are not reconnects");

    // Backoff rounds on the injected clock: 2s before round 1, 4s before 2.
    wait_for(FUSE, || {
        client.stats().observed_backoff == vec![Duration::from_secs(2)]
    }, "2s backoff armed")
    .await;
    clock.advance(Duration::from_secs(2));
    wait_for(FUSE, || client.stats().dial_attempts == 2, "round 1 dialed").await;
    wait_for(FUSE, || {
        client.stats().observed_backoff == vec![Duration::from_secs(2), Duration::from_secs(4)]
    }, "4s backoff armed")
    .await;
    clock.advance(Duration::from_secs(4));
    wait_for(FUSE, || client.stats().dial_attempts == 3, "round 2 dialed").await;
    // The third attempt's rejection lands a scheduling step later, and the
    // next round records its backoff at round-arm time (§6.2 worker: the
    // push happens a scheduling step after the rejection). Wait for the
    // record itself, never assert right after a wait (repo pattern —
    // relay_client.rs `auth_timeout_is_typed_and_retry_stays_on_the_backoff`
    // does exactly this); the settled chain is the deterministic fact.
    wait_for(FUSE, || {
        server.stats().auth_rejected == 3
            && client.stats().last_error_class == Some(RelayErrorClass::AuthRejected)
            && client.state() == RelayState::Reconnecting
            && client.stats().observed_backoff
                == vec![Duration::from_secs(2), Duration::from_secs(4), Duration::from_secs(8)]
    }, "third rejection recorded, third backoff armed, back in Reconnecting")
    .await;

    let stats = client.stats();
    assert_eq!(stats.last_error_class, Some(RelayErrorClass::AuthRejected));
    assert_eq!(stats.state, RelayState::Reconnecting);
    assert_eq!(
        stats.observed_backoff,
        vec![Duration::from_secs(2), Duration::from_secs(4), Duration::from_secs(8)]
    );
    assert_eq!(stats.dial_attempts, 3, "exactly one dial per round — no retry addiction");
    assert_eq!(server.stats().auth_rejected, 3);
    client.stop();
}

// ---------------------------------------------------------------------------
// 3. offline peer: open_conn times out typed (30s, injected), unsubscribes,
//    and NO PeersOnline ever arrived
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn open_conn_to_offline_peer_times_out_without_peers_online() {
    let server = start_default().await;
    let clock = VirtualClock::new(EPOCH);
    let client = start_client("open-off", &[loopback_url(server.addr().port())], valid_token(), &clock);
    wait_for(FUSE, || client.state() == RelayState::Ready, "ready").await;

    let target = remote_peer(2);
    // §4.1: the server registers interest and stays SILENT for an offline
    // target. open_conn can only return via (a) a real PeersOnline or (b)
    // the 30s deadline — advance the injected clock to force (b); if the
    // server (wrongly) had sent PeersOnline, the waiter would have resolved
    // Ok first and the counters below would show it.
    let waiter = tokio::spawn({
        let client = client.clone();
        let target = target.clone();
        async move { client.open_conn(&target).await }
    });
    wait_for(FUSE, || {
        server.stats().frames_rx(MSG_SUBSCRIBE_PEER_STATE) == 1
    }, "subscribe reached the server")
    .await;
    clock.advance(Duration::from_secs(31));

    let err = timeout(FUSE, waiter).await.expect("open_conn settles").unwrap().unwrap_err();
    assert_eq!(err, netbird_core::relay_client::RelayClientError::OpenConnTimeout);

    let stats = client.stats();
    assert_eq!(stats.frames_rx[MSG_PEERS_ONLINE as usize], 0, "no PeersOnline ever arrived");
    assert_eq!(stats.state, RelayState::Ready, "session survives the timeout");
    assert_eq!(server.stats().frames_rx(MSG_SUBSCRIBE_PEER_STATE), 1);
    // The timeout unsubscribe is fire-and-forget — wait for the wire effect.
    wait_for(FUSE, || {
        server.stats().frames_rx(MSG_UNSUBSCRIBE_PEER_STATE) == 1
    }, "§4.1: timeout unsubscribed the interest")
    .await;
    client.stop();
}

// ---------------------------------------------------------------------------
// 4. online peer: open_conn resolves, Transport byte-exact both directions,
//    reconciled with the server's forwarded-byte counter
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn transport_roundtrip_byte_exact_and_server_reconciled() {
    let server = start_default().await;
    let target_id = remote_peer(1);

    // The remote peer joins first (raw WS client, as in tests/relay_e2e.rs).
    let tcp = TcpStream::connect(server.addr()).await.unwrap();
    let key = random_sec_websocket_key().unwrap();
    let mut target = client_handshake(tcp, &server.addr().to_string(), "/relay", &key)
        .await
        .expect("target ws handshake");
    target
        .write_binary(
            &Frame::Auth { peer_id: target_id.clone(), token: valid_token() }
                .encode()
                .expect("auth encodes"),
        )
        .await
        .unwrap();
    match timeout(FUSE, target.read_message()).await {
        Ok(Ok(WsMessage::Binary(bytes))) => {
            assert!(matches!(Frame::decode(&bytes), Ok(Frame::AuthResponse { .. })))
        }
        other => panic!("target auth failed: {other:?}"),
    }

    // Our client subscribes and opens the conn.
    let clock = VirtualClock::new(EPOCH);
    let client = start_client("transport", &[loopback_url(server.addr().port())], valid_token(), &clock);
    wait_for(FUSE, || client.state() == RelayState::Ready, "ready").await;
    client.open_conn(&target_id).await.expect("open_conn resolves PeersOnline");

    // A → B over the relay: byte-exact, sender-id rewritten (§4.2).
    let a_to_b: Vec<u8> = (0..300u32).map(|i| (i % 251) as u8).collect();
    client.send_to_peer(&target_id, &a_to_b).expect("send accepted");
    match timeout(FUSE, target.read_message()).await {
        Ok(Ok(WsMessage::Binary(bytes))) => match Frame::decode(&bytes).expect("valid frame") {
            Frame::Transport { peer_id, payload } => {
                assert_eq!(peer_id, my_peer_handle(), "receiver sees the SENDER id");
                assert_eq!(payload, a_to_b, "A→B byte-exact");
            }
            other => panic!("expected Transport, got {other:?}"),
        },
        other => panic!("target read failed: {other:?}"),
    }

    // B → A: our client's recv() yields (sender, payload) byte-exact.
    let b_to_a: Vec<u8> = (0..257u32).map(|i| (i % 253) as u8).collect();
    target
        .write_binary(
            &Frame::Transport { peer_id: my_peer_handle(), payload: b_to_a.clone() }
                .encode()
                .expect("transport encodes"),
        )
        .await
        .unwrap();
    let (sender, payload) = timeout(FUSE, client.recv())
        .await
        .expect("recv within fuse")
        .expect("queue open");
    assert_eq!(sender, target_id, "recv surfaces the SENDER id");
    assert_eq!(payload, b_to_a, "B→A byte-exact");

    // Reconciliation with the fake server's counters.
    let stats = client.stats();
    assert_eq!(stats.transport_tx_bytes, a_to_b.len() as u64);
    assert_eq!(stats.transport_rx_bytes, b_to_a.len() as u64);
    assert_eq!(stats.frames_tx[MSG_TRANSPORT as usize], 1);
    assert_eq!(stats.frames_rx[MSG_TRANSPORT as usize], 1);
    assert_eq!(stats.inbound_dropped_full, 0);
    let server_stats = server.stats();
    // BOTH directions traverse the relay: A→B from our client, B→A from the
    // raw target — the server counts per-connection receives.
    assert_eq!(server_stats.frames_rx(MSG_TRANSPORT), 2);
    assert_eq!(
        server_stats.transport_bytes_forwarded,
        (a_to_b.len() + b_to_a.len()) as u64,
        "server forwarded exactly both payloads"
    );
    client.stop();
}

/// The client's own peer id, as the server (and thus the remote peer) sees
/// it — derived from the same fabricated WG pubkey the config used.
fn my_peer_handle() -> PeerId {
    local_peer_for("transport")
}

// ---------------------------------------------------------------------------
// 5. server-initiated HealthCheck → client echoes 01 05 (server rx counter)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn healthcheck_echo_reaches_the_server() {
    let server = RelayTestServer::start(TestServerConfig {
        server_healthcheck_interval: Some(Duration::from_millis(20)),
        ..Default::default()
    })
    .await
    .unwrap();
    let clock = VirtualClock::new(EPOCH);
    let client = start_client("hc-echo", &[loopback_url(server.addr().port())], valid_token(), &clock);
    wait_for(FUSE, || client.state() == RelayState::Ready, "ready").await;

    // Bounded poll for the server's echo counter (no ordering substitute
    // exists for "the server received our frame"; fuse expiry = failure).
    let deadline = Instant::now() + FUSE;
    while server.stats().frames_rx(MSG_HEALTH_CHECK) == 0 {
        assert!(Instant::now() < deadline, "server never received our 01 05 echo");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let stats = client.stats();
    assert!(stats.frames_rx[MSG_HEALTH_CHECK as usize] >= 1, "we received server HCs");
    assert!(stats.frames_tx[MSG_HEALTH_CHECK as usize] >= 1, "we echoed them");
    assert_eq!(client.state(), RelayState::Ready, "echoes keep the session alive");
    client.stop();
}

// ---------------------------------------------------------------------------
// 6. 35s keepalive death on the INJECTED clock → typed + reconnect
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn keepalive_death_after_35s_without_inbound_triggers_fast_reconnect() {
    let server = start_default().await;
    let clock = VirtualClock::new(EPOCH);
    let client = start_client("keepalive", &[loopback_url(server.addr().port())], valid_token(), &clock);
    wait_for(FUSE, || client.state() == RelayState::Ready, "ready").await;

    // 34s: still alive (boundary precision on the injected clock).
    clock.advance(Duration::from_secs(34));
    wait_for(FUSE, || client.state() == RelayState::Ready, "still ready").await;
    assert_eq!(client.stats().last_error_class, None, "no error below the boundary");

    // +1s ⇒ 35s total without inbound ⇒ dead ⇒ fast reconnect.
    clock.advance(Duration::from_secs(1));
    wait_for(FUSE, || {
        client.stats().reconnects == 1
            && client.stats().dial_attempts == 2
            && server.stats().connections_accepted == 2
    }, "died and fast-reconnected")
    .await;
    let stats = client.stats();
    assert_eq!(stats.last_error_class, Some(RelayErrorClass::KeepaliveTimeout));
    assert!(stats.observed_backoff.is_empty(), "fast path: no backoff delay");
    wait_for(FUSE, || client.state() == RelayState::Ready, "ready again").await;
    client.stop();
}

// ---------------------------------------------------------------------------
// 7. oversized frames: send side rejected before the wire, receive side
//    ends the session typed (FrameTooLarge) and reconnects
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn oversized_frames_fail_typed_on_both_directions() {
    let server = start_default().await;
    let clock = VirtualClock::new(EPOCH);
    let client = start_client("oversize", &[loopback_url(server.addr().port())], valid_token(), &clock);
    wait_for(FUSE, || client.state() == RelayState::Ready, "ready").await;

    // Send side: rejected at the handle, nothing reaches the wire.
    let oversized = vec![0u8; MAX_TRANSPORT_PAYLOAD + 1];
    let err = client.send_to_peer(&remote_peer(9), &oversized).unwrap_err();
    assert!(
        matches!(err, netbird_core::relay_client::RelayClientError::FrameTooLarge { .. }),
        "got {err:?}"
    );
    assert_eq!(server.stats().frames_rx(MSG_TRANSPORT), 0, "nothing touched the wire");
    assert_eq!(client.state(), RelayState::Ready, "connection untouched");

    // Receive side: the server pushes an oversized raw WS binary message.
    assert!(server.send_ws_binary_to(&local_peer_for("oversize"), vec![0xAB; MAX_MESSAGE_SIZE + 1]));
    wait_for(FUSE, || {
        client.stats().last_error_class == Some(RelayErrorClass::FrameTooLarge)
    }, "oversized inbound ends the session typed")
    .await;
    wait_for(FUSE, || {
        client.stats().reconnects == 1 && client.state() == RelayState::Ready
    }, "reconnected after the oversized frame")
    .await;
    client.stop();
}

// ---------------------------------------------------------------------------
// 8. server drops the TCP connection mid-session → typed + fast reconnect
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn server_disconnect_mid_session_reconnects_without_backoff() {
    let server = start_default().await;
    let clock = VirtualClock::new(EPOCH);
    let seed = "drop";
    let client = start_client(seed, &[loopback_url(server.addr().port())], valid_token(), &clock);
    wait_for(FUSE, || client.state() == RelayState::Ready, "ready").await;

    assert!(server.drop_connection(&local_peer_for(seed)), "connection tracked by peer id");
    // Race-free wait (same read-and-race class as the accept counters): the
    // client counts the drop a scheduling step before it re-dials, so
    // `reconnects == 1 && state == Ready` alone can catch the pre-redial
    // instant. Require the redial to have been INITIATED too (dial_attempts
    // bumps at round start, before Dialing/Ready).
    wait_for(FUSE, || {
        client.stats().reconnects == 1
            && client.state() == RelayState::Ready
            && client.stats().dial_attempts == 2
    }, "fast reconnected after the drop")
    .await;
    // the server's accept counter is bumped on its own task after the new
    // TCP connection lands — wait for the event, never read-and-race
    wait_for(FUSE, || {
        server.stats().connections_accepted == 2
    }, "second connection accepted")
    .await;
    let stats = client.stats();
    assert_eq!(stats.last_error_class, Some(RelayErrorClass::ServerClosed));
    assert!(stats.observed_backoff.is_empty(), "§6.2 fast path: no backoff on the first retry");
    assert_eq!(stats.dial_attempts, 2);
    assert_eq!(server.stats().connections_accepted, 2);
    client.stop();
}

// ---------------------------------------------------------------------------
// 9. expired token: fail-closed at start (ZERO wire traffic), recovers on
//    update_token
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn expired_token_never_touches_the_wire_until_refreshed() {
    let server = start_default().await;
    let clock = VirtualClock::new(EPOCH);
    // Payload already in the past at the fabricated epoch.
    let client = start_client(
        "expired",
        &[loopback_url(server.addr().port())],
        token(1_000_000_000),
        &clock,
    );

    wait_for(FUSE, || {
        client.stats().last_error_class == Some(RelayErrorClass::TokenExpired)
    }, "fail-closed on the expired token")
    .await;
    let stats = client.stats();
    assert_eq!(stats.dial_attempts, 0, "the gate refuses BEFORE dialing");
    assert_eq!(server.stats().connections_accepted, 0, "ZERO wire traffic");
    assert_eq!(stats.state, RelayState::Reconnecting);
    assert!(stats.token_expired_refusals >= 1);
    assert!(matches!(client.token_validity(), TokenValidity::Expired { .. }));

    // A fresh token wakes the hold instantly and the client connects.
    client.update_token(valid_token());
    wait_for(FUSE, || client.state() == RelayState::Ready, "ready after token refresh").await;
    assert_eq!(server.stats().connections_accepted, 1);
    client.stop();
}

// ---------------------------------------------------------------------------
// 10. multi-URL: concurrent first-win, bounded attempts
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn multi_url_concurrent_dial_first_win_is_bounded() {
    let server = start_default().await;
    let clock = VirtualClock::new(EPOCH);
    // First URL is a loopback port with no listener → instant refused.
    let urls = [String::from("rel://127.0.0.1:1"), loopback_url(server.addr().port())];
    let client = start_client("multiurl", &urls, valid_token(), &clock);

    wait_for(FUSE, || client.state() == RelayState::Ready, "first winner ready").await;
    let stats = client.stats();
    assert_eq!(
        stats.current_url.as_deref(),
        Some(loopback_url(server.addr().port()).as_str()),
        "the URL that actually connected won"
    );
    assert_eq!(stats.dial_attempts, 2, "both URLs dialed exactly once (≤7 wave cap)");
    assert_eq!(server.stats().connections_accepted, 1);
    assert_eq!(server.stats().handshake_failures, 0);
    client.stop();
}

// ---------------------------------------------------------------------------
// 11. rels:// wiring: fail-closed without TLS, seam exercised with a
//     test-only passthrough connector
// ---------------------------------------------------------------------------

/// Test-only "TLS" connector: records the SNI and passes the stream through.
/// The fake server speaks plain WS, so the passthrough proves the SEAM
/// (connector invoked, server name propagated, stream adopted); real
/// certificate policy stays with the production connector (increment D).
struct PassthroughTls {
    seen: std::sync::Mutex<Vec<String>>,
}

impl netbird_core::relay_client::RelayTlsConnector for PassthroughTls {
    fn connect(
        &self,
        stream: netbird_core::relay_client::BoxStream,
        server_name: String,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        netbird_core::relay_client::BoxStream,
                        netbird_core::relay_client::RelayClientError,
                    >,
                > + Send,
        >,
    > {
        self.seen.lock().unwrap().push(server_name);
        Box::pin(async move { Ok(stream) })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn rels_url_fails_closed_without_tls_and_uses_the_connector_when_given() {
    let server = start_default().await;
    let clock = VirtualClock::new(EPOCH);
    let rels_url = format!("rels://127.0.0.1:{}", server.addr().port());

    // (a) rels:// WITHOUT a connector: refused BEFORE any wire byte.
    let cfg = RelayClientConfig::new(
        &[rels_url.clone()],
        "e2e-relay-client-notls-wgkey",
        valid_token(),
    )
    .unwrap()
    .with_clock(std::sync::Arc::new(clock.clone()));
    let client = RelayClient::start(cfg).unwrap();
    wait_for(FUSE, || {
        client.stats().last_error_class == Some(RelayErrorClass::TlsRequired)
    }, "fail-closed without a TLS connector")
    .await;
    assert_eq!(server.stats().connections_accepted, 0, "no plaintext downgrade");
    client.stop();

    // (b) rels:// WITH the (test-only passthrough) connector: the seam is
    // exercised and the session comes up.
    let tls = std::sync::Arc::new(PassthroughTls { seen: std::sync::Mutex::new(Vec::new()) });
    let cfg = RelayClientConfig::new(&[rels_url.clone()], "e2e-relay-client-tls-wgkey", valid_token())
        .unwrap()
        .with_clock(std::sync::Arc::new(clock.clone()))
        .with_tls(tls.clone());
    let client = RelayClient::start(cfg).unwrap();
    wait_for(FUSE, || client.state() == RelayState::Ready, "ready through the TLS seam").await;
    assert_eq!(*tls.seen.lock().unwrap(), vec![String::from("127.0.0.1")], "SNI = URL host");
    assert_eq!(client.stats().current_url.as_deref(), Some(rels_url.as_str()));
    client.stop();
}

// ---------------------------------------------------------------------------
// 12. stop(): relay Close frame (01 04) reaches the server, terminal Dead
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn stop_sends_the_relay_close_frame_and_reaches_dead() {
    let server = start_default().await;
    let clock = VirtualClock::new(EPOCH);
    let client = start_client("stop", &[loopback_url(server.addr().port())], valid_token(), &clock);
    wait_for(FUSE, || client.state() == RelayState::Ready, "ready").await;

    client.stop();
    wait_for(FUSE, || client.state() == RelayState::Dead, "terminal Dead").await;
    wait_for(FUSE, || server.stats().frames_rx(MSG_CLOSE) == 1, "relay Close (01 04) received")
        .await;
    assert_eq!(client.stats().frames_tx[MSG_CLOSE as usize], 1);
}

// ---------------------------------------------------------------------------
// helpers shared by the transport test
// ---------------------------------------------------------------------------
