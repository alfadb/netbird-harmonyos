// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N13-B relay end-to-end tests: the REAL client stack
//! ([`netbird_core::relay`] codec + [`netbird_core::ws`] handshake/framing)
//! driven over real loopback TCP against the in-process fake relay server
//! (`netbird_core::relay_testserver`, server-side semantics per
//! `docs/relay-client-spec-20260914.md` §1/§2/§3/§4/§5/§8).
//!
//! Determinism discipline: no sleeps as assertions — every wait is a
//! `tokio::time::timeout` fuse around an expected event; the single negative
//! assertion (no `PeersOnline` for an offline peer, spec §4.1 blocking-wait)
//! asserts absence over a bounded window. All token/peer values are
//! fabricated; no token content is ever printed.

use std::time::Duration;

// Host-process link stubs (repo convention, cf. tests/ice_session_codec.rs):
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
        _argv: *mut *mut core::ffi::c_void,
        _data: *mut *mut core::ffi::c_void,
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
        _result: *mut bool,
    ) -> i32 {
        0
    }
}

use netbird_core::relay::{
    AuthToken, Frame, PeerId, MAX_MESSAGE_SIZE, MSG_AUTH, MSG_CLOSE, MSG_HEALTH_CHECK,
    MSG_SUBSCRIBE_PEER_STATE, MSG_TRANSPORT,
};
use netbird_core::relay_testserver::{
    HandshakeBehavior, RelayTestServer, TestServerConfig, DEFAULT_INSTANCE_URL,
};
use netbird_core::ws::{client_handshake, random_sec_websocket_key, WsClient, WsError, WsMessage};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Fuse for every wait on an event that MUST happen.
const FUSE: Duration = Duration::from_secs(5);

// Fabricated (NOT secret) token parts — same shape as the relay.rs fixtures:
// 32×0xA5 signature, ASCII Unix-seconds payload.
const SIG_B64: &str = "paWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaU=";

fn token() -> AuthToken {
    AuthToken::from_management("1770000000", SIG_B64).expect("fabricated token is valid")
}

/// Distinct fabricated peer id per tag (36 raw bytes `"sha-"||sha256`).
fn peer(tag: u8) -> PeerId {
    PeerId::from_wg_pubkey_string(&format!(
        "e2e-fake-wg-pubkey-{tag:02}-AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRo="
    ))
}

async fn start_default() -> RelayTestServer {
    RelayTestServer::start(TestServerConfig::default())
        .await
        .expect("bind 127.0.0.1:0")
}

async fn connect_client(addr: std::net::SocketAddr) -> WsClient<TcpStream> {
    let tcp = TcpStream::connect(addr).await.expect("tcp connect");
    let key = random_sec_websocket_key().expect("urandom key");
    client_handshake(tcp, &addr.to_string(), "/relay", &key)
        .await
        .expect("ws handshake must get 101")
}

async fn send_frame(client: &mut WsClient<TcpStream>, frame: Frame) {
    let bytes = frame.encode().expect("frame encodes");
    client.write_binary(&bytes).await.expect("write_binary");
}

/// Read the next message through the real client WS layer (timeout-fused).
async fn expect_ws_message(client: &mut WsClient<TcpStream>) -> WsMessage {
    match timeout(FUSE, client.read_message()).await {
        Err(_) => panic!("timed out waiting for a ws message"),
        Ok(Err(e)) => panic!("ws error while waiting for a message: {e:?}"),
        Ok(Ok(msg)) => msg,
    }
}

/// Typed-error variant of [`expect_ws_message`] for failure-path tests.
async fn expect_ws_error(client: &mut WsClient<TcpStream>) -> WsError {
    match timeout(FUSE, client.read_message()).await {
        Err(_) => panic!("timed out: connection neither errored nor answered"),
        Ok(Err(e)) => e,
        Ok(Ok(msg)) => panic!("expected a typed ws error, got message {msg:?}"),
    }
}

/// Read the next relay frame (one WS binary message == one frame, §2.6).
async fn expect_frame(client: &mut WsClient<TcpStream>) -> Frame {
    match expect_ws_message(client).await {
        WsMessage::Binary(bytes) => Frame::decode(&bytes).expect("valid relay frame"),
        other => panic!("expected a relay frame, got {other:?}"),
    }
}

/// Full OpenConn-equivalent: send Auth, expect the AuthResponse with the
/// configured instance URL.
async fn auth_ok(client: &mut WsClient<TcpStream>, id: &PeerId, want_url: &str) {
    send_frame(client, Frame::Auth { peer_id: id.clone(), token: token() }).await;
    match expect_frame(client).await {
        Frame::AuthResponse { instance_url } => assert_eq!(instance_url, want_url),
        other => panic!("expected AuthResponse, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 1. handshake success
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn handshake_success_records_key_and_authenticates() {
    let server = start_default().await;
    let key = random_sec_websocket_key().unwrap();
    let tcp = TcpStream::connect(server.addr()).await.unwrap();
    let mut client = client_handshake(tcp, &server.addr().to_string(), "/relay", &key)
        .await
        .expect("101 handshake");
    // The 101 only comes after the server captured and echoed our key, so
    // these stats are settled (no race).
    let stats = server.stats();
    assert_eq!(stats.handshake_keys, vec![key], "server must see the exact key");
    assert_eq!(stats.connections_accepted, 1);
    assert_eq!(stats.handshake_failures, 0);
    // And the connection is fully usable: real Auth → AuthResponse roundtrip.
    auth_ok(&mut client, &peer(1), DEFAULT_INSTANCE_URL).await;
}

// ---------------------------------------------------------------------------
// 2. handshake failures — typed, no fallback
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn handshake_failures_are_typed_without_fallback() {
    let cases = [
        (HandshakeBehavior::HttpStatus(404), "non-101 status"),
        (HandshakeBehavior::OmitUpgrade, "missing Upgrade header"),
        (HandshakeBehavior::NegotiateExtension, "extension negotiation"),
    ];
    for (mode, what) in cases {
        let config =
            TestServerConfig { handshake: mode.clone(), ..TestServerConfig::default() };
        let server = RelayTestServer::start(config).await.unwrap();
        let tcp = TcpStream::connect(server.addr()).await.unwrap();
        let key = random_sec_websocket_key().unwrap();
        let err = client_handshake(tcp, &server.addr().to_string(), "/relay", &key)
            .await
            .expect_err(&format!("{what}: handshake must fail typed"));
        match &mode {
            HandshakeBehavior::HttpStatus(code) => assert!(
                matches!(err, WsError::HandshakeStatus(c) if c == *code),
                "{what}: want HandshakeStatus({code}), got {err:?}"
            ),
            HandshakeBehavior::OmitUpgrade => assert!(
                matches!(err, WsError::HandshakeHeader("upgrade")),
                "{what}: want HandshakeHeader(upgrade), got {err:?}"
            ),
            HandshakeBehavior::NegotiateExtension => assert!(
                matches!(err, WsError::ExtensionNegotiated),
                "{what}: want ExtensionNegotiated, got {err:?}"
            ),
            HandshakeBehavior::Normal => unreachable!("not a failure case"),
        }
        assert_eq!(server.stats().handshake_failures, 1, "{what}");
        server.shutdown().await;
    }
}

// ---------------------------------------------------------------------------
// 3. Auth success
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn auth_success_returns_configured_instance_url() {
    let config = TestServerConfig {
        instance_url: "rels://e2e-fake.example:12345".to_string(),
        ..TestServerConfig::default()
    };
    let server = RelayTestServer::start(config).await.unwrap();
    let mut client = connect_client(server.addr()).await;
    auth_ok(&mut client, &peer(1), "rels://e2e-fake.example:12345").await;
    let stats = server.stats();
    assert_eq!(stats.frames_rx(MSG_AUTH), 1);
    assert_eq!(stats.auth_rejected, 0);
}

// ---------------------------------------------------------------------------
// 4. Auth rejection — silent close (spec §3.4), typed client error
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn auth_rejection_is_silent_close_with_typed_error() {
    let config = TestServerConfig { reject_auth: true, ..TestServerConfig::default() };
    let server = RelayTestServer::start(config).await.unwrap();
    let mut client = connect_client(server.addr()).await;
    send_frame(&mut client, Frame::Auth { peer_id: peer(1), token: token() }).await;
    // No AuthResponse exists on this path — the server just closes, so the
    // client's read ends in a typed error (EOF from the graceful FIN).
    let err = expect_ws_error(&mut client).await;
    assert!(matches!(err, WsError::Eof), "want typed EOF, got {err:?}");
    let stats = server.stats();
    assert_eq!(stats.auth_rejected, 1);
    assert_eq!(stats.frames_rx(MSG_AUTH), 1);
}

// ---------------------------------------------------------------------------
// 5. offline peer: subscribe must NOT produce PeersOnline (§4.1)
// ---------------------------------------------------------------------------
//
// NOTE on the negative assertion: `WsClient::read_message` is not
// cancellation-safe (a cancelled `fill_buffer` leaves the internal buffer
// sized/padded), so this test never cancels a read mid-flight. Absence is
// instead proven by FIFO order on the single relay connection: the server
// has ONE reader and ONE FIFO writer channel, so any PeersOnline produced in
// response to the Subscribe would have to arrive BEFORE the HealthCheck echo
// that follows it, and before the close that terminates the stream.

#[tokio::test(flavor = "multi_thread")]
async fn subscribing_offline_peer_stays_silent() {
    let server = start_default().await;
    let mut client = connect_client(server.addr()).await;
    auth_ok(&mut client, &peer(1), DEFAULT_INSTANCE_URL).await;
    send_frame(&mut client, Frame::SubscribePeerState { peer_ids: vec![peer(2)] }).await;
    // Probe the FIFO: if the server had answered the Subscribe with
    // PeersOnline, that frame — not HealthCheck — would arrive here.
    send_frame(&mut client, Frame::HealthCheck).await;
    assert_eq!(
        expect_frame(&mut client).await,
        Frame::HealthCheck,
        "HealthCheck echo must be the FIRST frame after the subscribe (no PeersOnline)"
    );
    // Terminate the stream from the server side and confirm nothing else
    // (still no PeersOnline) was ever queued behind the echo.
    assert!(server.close_connection(&peer(1), 1000, "done"));
    assert_eq!(
        expect_ws_message(&mut client).await,
        WsMessage::Closed(Some((1000, "done".to_string()))),
        "stream must end with the close, never a late PeersOnline"
    );
    assert_eq!(server.stats().frames_rx(MSG_SUBSCRIBE_PEER_STATE), 1);
}

// ---------------------------------------------------------------------------
// 6. PeersOnline delivered when the subscribed peer comes online
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn peers_online_delivered_when_subscribed_peer_authenticates() {
    let server = start_default().await;
    let mut subscriber = connect_client(server.addr()).await;
    auth_ok(&mut subscriber, &peer(2), DEFAULT_INSTANCE_URL).await;
    // peer(1) is offline: subscribe must stay silent (§4.1). The HealthCheck
    // probe proves ordering — any wrongly-sent PeersOnline would precede it.
    send_frame(&mut subscriber, Frame::SubscribePeerState { peer_ids: vec![peer(1)] }).await;
    send_frame(&mut subscriber, Frame::HealthCheck).await;
    assert_eq!(expect_frame(&mut subscriber).await, Frame::HealthCheck);

    // peer(1) comes online → the blocked subscriber is answered.
    let mut target = connect_client(server.addr()).await;
    auth_ok(&mut target, &peer(1), DEFAULT_INSTANCE_URL).await;
    let frame = expect_frame(&mut subscriber).await;
    assert_eq!(frame, Frame::PeersOnline { peer_ids: vec![peer(1)] }, "exact peer list");
}

// ---------------------------------------------------------------------------
// 7. Transport bidirectional forwarding with sender-id rewrite (§4.2)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn transport_forwards_bidirectionally_and_rewrites_sender() {
    let server = start_default().await;
    let mut a = connect_client(server.addr()).await;
    let mut b = connect_client(server.addr()).await;
    auth_ok(&mut a, &peer(1), DEFAULT_INSTANCE_URL).await;
    auth_ok(&mut b, &peer(2), DEFAULT_INSTANCE_URL).await;
    server.wait_connections(2, FUSE).await.expect("both accepted");

    // OpenConn shape (§4.1): A subscribes to online B → immediate PeersOnline.
    send_frame(&mut a, Frame::SubscribePeerState { peer_ids: vec![peer(2)] }).await;
    assert_eq!(expect_frame(&mut a).await, Frame::PeersOnline { peer_ids: vec![peer(2)] });

    let a_to_b: Vec<u8> = (0..300u32).map(|i| (i % 251) as u8).collect();
    let b_to_a: Vec<u8> = (0..257u32).map(|i| (i % 253) as u8).collect();

    send_frame(&mut a, Frame::Transport { peer_id: peer(2), payload: a_to_b.clone() }).await;
    match expect_frame(&mut b).await {
        Frame::Transport { peer_id, payload } => {
            assert_eq!(peer_id, peer(1), "§4.2: receiver sees the SENDER id, not dst");
            assert_eq!(payload, a_to_b, "payload must arrive byte-exact");
        }
        other => panic!("expected Transport, got {other:?}"),
    }

    send_frame(&mut b, Frame::Transport { peer_id: peer(1), payload: b_to_a.clone() }).await;
    match expect_frame(&mut a).await {
        Frame::Transport { peer_id, payload } => {
            assert_eq!(peer_id, peer(2), "§4.2: reverse direction rewrites too");
            assert_eq!(payload, b_to_a, "payload must arrive byte-exact");
        }
        other => panic!("expected Transport, got {other:?}"),
    }

    let stats = server.stats();
    assert_eq!(stats.frames_rx(MSG_TRANSPORT), 2);
    assert_eq!(
        stats.transport_bytes_forwarded,
        (a_to_b.len() + b_to_a.len()) as u64,
        "forwarded bytes must equal both directions' payload totals"
    );
    assert_eq!(stats.transports_dropped_offline, 0);
}

// ---------------------------------------------------------------------------
// 8. Transport to an offline peer: silent drop + count, connection alive
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn transport_to_offline_peer_dropped_and_counted() {
    let server = start_default().await;
    let mut client = connect_client(server.addr()).await;
    auth_ok(&mut client, &peer(1), DEFAULT_INSTANCE_URL).await;
    send_frame(&mut client, Frame::Transport {
        peer_id: peer(9),
        payload: b"into the void".to_vec(),
    })
    .await;
    // No error frame exists in the protocol (§4.2): prove the connection is
    // still fully usable with a HealthCheck roundtrip.
    send_frame(&mut client, Frame::HealthCheck).await;
    assert_eq!(expect_frame(&mut client).await, Frame::HealthCheck);
    let stats = server.stats();
    assert_eq!(stats.transports_dropped_offline, 1, "drop must be counted");
    assert_eq!(stats.transport_bytes_forwarded, 0);
    assert_eq!(stats.frames_rx(MSG_TRANSPORT), 1);
}

// ---------------------------------------------------------------------------
// 9. HealthCheck: exact 01 05 bytes, echo + server-initiated
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn healthcheck_roundtrip_exact_bytes_and_server_initiated() {
    // (1) client-initiated: echo is exactly the 2 bytes [01 05] (§5).
    let server = start_default().await;
    let mut client = connect_client(server.addr()).await;
    auth_ok(&mut client, &peer(1), DEFAULT_INSTANCE_URL).await;
    send_frame(&mut client, Frame::HealthCheck).await;
    match expect_ws_message(&mut client).await {
        WsMessage::Binary(bytes) => {
            assert_eq!(bytes, vec![0x01, 0x05], "echo must be exactly 01 05");
        }
        other => panic!("expected binary healthcheck echo, got {other:?}"),
    }
    server.shutdown().await;

    // (2) server-initiated (configurable cadence, §5 sender side): the client
    // receives [01 05] unprompted and its reply echoes back.
    let config = TestServerConfig {
        server_healthcheck_interval: Some(Duration::from_millis(50)),
        ..TestServerConfig::default()
    };
    let server = RelayTestServer::start(config).await.unwrap();
    let mut client = connect_client(server.addr()).await;
    auth_ok(&mut client, &peer(2), DEFAULT_INSTANCE_URL).await;
    match expect_ws_message(&mut client).await {
        WsMessage::Binary(bytes) => assert_eq!(bytes, vec![0x01, 0x05], "server-initiated"),
        other => panic!("expected server-initiated healthcheck, got {other:?}"),
    }
    send_frame(&mut client, Frame::HealthCheck).await;
    match expect_ws_message(&mut client).await {
        WsMessage::Binary(bytes) => assert_eq!(bytes, vec![0x01, 0x05], "echo of our reply"),
        other => panic!("expected healthcheck echo, got {other:?}"),
    }
    let stats = server.stats();
    assert!(stats.server_healthchecks_sent >= 1, "server initiated at least one");
    assert_eq!(stats.frames_rx(MSG_HEALTH_CHECK), 1, "client reply was received");
}

// ---------------------------------------------------------------------------
// 10. oversized frame: typed hard rejection on both sides
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn oversized_frame_rejected_typed_on_both_sides() {
    let server = start_default().await;
    let mut client = connect_client(server.addr()).await;
    auth_ok(&mut client, &peer(1), DEFAULT_INSTANCE_URL).await;

    // Send side: hard rejection before anything reaches the wire (increment-A
    // decision: oversized = hard error, not silent truncation).
    let oversized = vec![0u8; MAX_MESSAGE_SIZE + 1];
    let err = client.write_binary(&oversized).await.unwrap_err();
    assert!(
        matches!(err, WsError::MessageTooLarge(MAX_MESSAGE_SIZE)),
        "send side: want MessageTooLarge(8820), got {err:?}"
    );
    // The connection was never touched: still fully usable.
    send_frame(&mut client, Frame::HealthCheck).await;
    assert_eq!(expect_frame(&mut client).await, Frame::HealthCheck);
    assert_eq!(
        server.stats().frames_rx(MSG_HEALTH_CHECK),
        1,
        "oversized frame never reached the wire"
    );

    // Receive side: the server pushes an oversized raw WS binary message.
    assert!(server.send_ws_binary_to(&peer(1), vec![0xAB; MAX_MESSAGE_SIZE + 1]));
    let err = expect_ws_error(&mut client).await;
    assert!(
        matches!(err, WsError::MessageTooLarge(MAX_MESSAGE_SIZE)),
        "receive side: want MessageTooLarge(8820), got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 11. mid-session server disconnect → typed error, no panic
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn server_disconnect_mid_session_is_typed_error() {
    let server = start_default().await;
    let mut client = connect_client(server.addr()).await;
    auth_ok(&mut client, &peer(3), DEFAULT_INSTANCE_URL).await;
    send_frame(&mut client, Frame::HealthCheck).await;
    assert_eq!(expect_frame(&mut client).await, Frame::HealthCheck);

    assert!(server.drop_connection(&peer(3)), "connection must be tracked");
    let err = expect_ws_error(&mut client).await;
    assert!(
        matches!(err, WsError::Eof | WsError::Io(_)),
        "want typed EOF/IO, got {err:?}"
    );
    assert_eq!(server.stats().connections_accepted, 1);
}

// ---------------------------------------------------------------------------
// 12. close semantics: WS close both directions + relay Close frame
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn close_semantics_ws_close_both_directions_and_relay_close() {
    let server = start_default().await;

    // (a) server-initiated WS close: the client surfaces Closed(code, reason)
    // and further writes are rejected.
    let mut closed_by_server = connect_client(server.addr()).await;
    auth_ok(&mut closed_by_server, &peer(1), DEFAULT_INSTANCE_URL).await;
    assert!(server.close_connection(&peer(1), 1000, "server closing"));
    assert_eq!(
        expect_ws_message(&mut closed_by_server).await,
        WsMessage::Closed(Some((1000, "server closing".to_string()))),
        "client must receive the close code and reason"
    );
    assert!(
        matches!(closed_by_server.write_binary(b"late").await, Err(WsError::AlreadyClosed)),
        "write after close must be rejected typed"
    );

    // (b) client-initiated WS close: the server echoes the close handshake.
    let mut closes_itself = connect_client(server.addr()).await;
    auth_ok(&mut closes_itself, &peer(2), DEFAULT_INSTANCE_URL).await;
    closes_itself.write_close(Some(1000), "bye").await.unwrap();
    assert_eq!(
        expect_ws_message(&mut closes_itself).await,
        WsMessage::Closed(Some((1000, String::new()))),
        "server must echo the close code (reason dropped per RFC 6455 §5.5.1)"
    );

    // (c) relay-level Close (01 04): echoed byte-exact, then the server ends
    // the TCP stream (§4.2).
    let mut relay_close = connect_client(server.addr()).await;
    auth_ok(&mut relay_close, &peer(3), DEFAULT_INSTANCE_URL).await;
    send_frame(&mut relay_close, Frame::Close).await;
    match expect_ws_message(&mut relay_close).await {
        WsMessage::Binary(bytes) => assert_eq!(bytes, vec![0x01, 0x04], "relay Close echo"),
        other => panic!("expected relay Close echo, got {other:?}"),
    }
    let err = expect_ws_error(&mut relay_close).await;
    assert!(matches!(err, WsError::Eof), "server must end the stream: {err:?}");
    assert_eq!(server.stats().frames_rx(MSG_CLOSE), 1);
}
