// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N13-D1 relay CONNECTOR end-to-end tests: the connector
//! ([`netbird_core::connector::ConnectorHandle`], spawned with the relay
//! opt-in `relay_enabled=true`) driven over a real in-process management
//! mock and the real loopback fake relay server
//! ([`netbird_core::relay_testserver`], unchanged from increment B).
//!
//! Covered here:
//! 1. `relay_enabled` absent (hard default FALSE) → zero relay behavior
//!    (no dial, `enabled:false`), everything else byte-identical;
//! 2. opt-in → the connector's relay client reaches Ready over the fake
//!    server and Transport frames round-trip BYTE-EXACT through it, with
//!    the status document's `relay` section reconciling against the fake
//!    server's counters;
//! 3. token refresh: a sync-expired token goes fail-closed and VISIBLE
//!    (`token_valid:false`), and a NEW sync carrying fresh `RelayServers`
//!    recovers it with ZERO real waiting (injected [`VirtualClock`]);
//! 4. `connector_stop()` revocation cleanup: the relay client reaches the
//!    terminal Dead state and the server observes the relay Close frame —
//!    no orphan outbound connection;
//! 5. the rustls TLS connector ([`netbird_core::relay_client::
//!    RustlsRelayTlsConnector`]) performs a REAL TLS 1.3 handshake and
//!    record exchange over loopback against a rustls SERVER built from a
//!    test-only self-signed certificate (`rel://` plaintext remains the
//!    topology for the relay-protocol tests; the `rels://` no-root refusal
//!    is pinned in `src/connector.rs` unit tests).
//!
//! Determinism discipline: no sleeps as assertions — every positive wait
//! is a bounded predicate loop (yield + fuse) over an event that MUST
//! happen; negative assertions are bounded predicate loops whose fuse
//! expiry IS the pass condition. All tokens/keys/certificates are
//! fabricated test material; no token content is ever printed.

use core::time::Duration;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use base64::Engine as _;
use prost::Message as _;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status};

use netbird_core::backoff::ExponentialBackoff;
use netbird_core::connector::{
    ConfigApplier, ConnectorConfig, ConnectorHandle, GrpcManagementFactory, RelayAttach,
    RelayMaterials, SyncPolicy, WgPeerRegistry,
};
use netbird_core::envelope::{self, EnvelopeKeyPair, EnvelopePublicKey};
use netbird_core::grpc::proto::management_service_server::{
    ManagementService, ManagementServiceServer,
};
use netbird_core::grpc::proto::{
    Empty, EncryptedMessage, LoginRequest, LoginResponse, ServerKeyResponse, SyncRequest,
    SyncResponse,
};
use netbird_core::grpc::PeerMeta;
use netbird_core::relay::{AuthToken, Frame, PeerId, MSG_CLOSE};
use netbird_core::relay_client::{RustlsRelayTlsConnector, VirtualClock};
use netbird_core::relay_testserver::{RelayTestServer, TestServerConfig};
use netbird_core::ws::{client_handshake, random_sec_websocket_key, WsClient, WsMessage};

// Host-process link stubs (repo convention, cf. tests/connector.rs):
// libace_napi.z.so / libhilog_ndk.z.so do not exist on the host. No-ops for
// the linker only; never called by these tests.
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
        _argv: *mut c_void,
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
        _value: *mut c_void,
        _result: *mut i32,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(
        _value: *mut c_void,
        _result: *mut bool,
    ) -> i32 {
        0
    }
}

// ---------------------------------------------------------------------------
// fixtures (all fabricated — never secret)
// ---------------------------------------------------------------------------

/// Real-time fuse for events that MUST happen (the relay client runs on the
/// injected virtual clock; this bounds only scheduler latency). HANG GUARD,
/// not a latency assertion: the events happen in microseconds over loopback
/// and the fuse only bounds a stuck scheduler/peer. Generous (60s) so a fully
/// loaded parallel `cargo test` run never trips it spuriously.
const FUSE: Duration = Duration::from_secs(60);

/// Bounded predicate wait (no sleeps as assertions; fuse expiry = failure).
async fn wait_for(fuse: Duration, mut pred: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + fuse;
    loop {
        if pred() {
            return;
        }
        assert!(Instant::now() < deadline, "condition not met within fuse: {what}");
        tokio::task::yield_now().await;
    }
}

/// Bounded NEGATIVE assertion: `pred` must stay false for `window` (fuse
/// expiry = pass; the predicate firing = failure).
async fn assert_stays_false(window: Duration, mut pred: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + window;
    while Instant::now() < deadline {
        assert!(!pred(), "condition that must never happen occurred: {what}");
        tokio::task::yield_now().await;
    }
}

/// Fabricated (NOT secret) 32-byte signature, base64 std — same fixture
/// shape as tests/relay_client_e2e.rs.
const SIG_B64: &str = "paWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaU=";

/// Fabricated virtual epoch (2026-02-02).
const EPOCH: u64 = 1_770_000_000;

fn relay_token(expires_at_unix: u64) -> AuthToken {
    AuthToken::from_management(&expires_at_unix.to_string(), SIG_B64)
        .expect("fabricated token is valid")
}

/// Fabricated remote peer id (base64-of-ASCII fixture keys, mirrors the
/// other suites).
fn remote_peer() -> PeerId {
    PeerId::from_wg_pubkey_string("ZTJlLXJlbGF5LWNvbm5lY3Rvci1yZW1vdGUtd2drZXk=")
}

fn loopback_url(port: u16) -> String {
    format!("rel://127.0.0.1:{port}")
}

fn test_private_key_b64(seed: u8) -> String {
    base64::engine::general_purpose::STANDARD.encode([seed; 32])
}

// ---------------------------------------------------------------------------
// minimal management mock (GetServerKey + Login + Sync + Logout; sync pushes)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct MiniMgmt {
    keys: EnvelopeKeyPair,
    /// Scripted updates consumed by the NEXT sync connection.
    sync_script: Arc<Mutex<Vec<SyncResponse>>>,
    /// The CLIENT public key of the last sync connection (public material)
    /// + the live stream sender, so tests can push further SyncResponses
    /// while the stream is open.
    live: Arc<Mutex<Option<(EnvelopePublicKey, tokio::sync::mpsc::Sender<Result<EncryptedMessage, Status>>)>>>,
    sync_opens: Arc<Mutex<usize>>,
    logout_calls: Arc<Mutex<usize>>,
}

impl Default for MiniMgmt {
    fn default() -> Self {
        MiniMgmt {
            keys: EnvelopeKeyPair::generate().expect("server key pair"),
            sync_script: Arc::new(Mutex::new(Vec::new())),
            live: Arc::new(Mutex::new(None)),
            sync_opens: Arc::new(Mutex::new(0)),
            logout_calls: Arc::new(Mutex::new(0)),
        }
    }
}

impl MiniMgmt {
    fn script(&self, updates: Vec<SyncResponse>) {
        *self.sync_script.lock().expect("script") = updates;
    }

    /// Push one more SyncResponse into the LIVE sync stream (test →
    /// "management"); the frames are envelope-sealed like the RPC path.
    fn push(&self, resp: SyncResponse) {
        let guard = self.live.lock().expect("live");
        let Some((client_pk, tx)) = guard.as_ref() else {
            panic!("no live sync stream (connector not connected?)");
        };
        let sealed = envelope::seal(client_pk, &self.keys, &resp.encode_to_vec())
            .expect("seal pushed sync frame");
        tx.try_send(Ok(EncryptedMessage {
            wg_pub_key: client_pk.to_base64(),
            body: sealed,
            version: 0,
        }))
        .expect("push into live sync stream");
    }
}

fn decrypt_body(server: &EnvelopeKeyPair, env: &EncryptedMessage) -> Result<Vec<u8>, Status> {
    let client_pk = EnvelopePublicKey::from_base64(&env.wg_pub_key)
        .map_err(|_| Status::invalid_argument("envelope wgPubKey is not a NaCl key"))?;
    envelope::open(&client_pk, server, &env.body)
        .map_err(|_| Status::internal("cannot decrypt request body"))
}

#[tonic::async_trait]
impl ManagementService for MiniMgmt {
    async fn get_server_key(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ServerKeyResponse>, Status> {
        Ok(Response::new(ServerKeyResponse {
            key: self.keys.public_key_base64(),
            expires_at: None,
            version: 0,
        }))
    }

    async fn login(
        &self,
        request: Request<EncryptedMessage>,
    ) -> Result<Response<EncryptedMessage>, Status> {
        let env = request.into_inner();
        let plaintext = decrypt_body(&self.keys, &env)?;
        LoginRequest::decode(plaintext.as_slice())
            .map_err(|e| Status::invalid_argument(format!("body is not LoginRequest: {e}")))?;
        let response = LoginResponse::default();
        let client_pk = EnvelopePublicKey::from_base64(&env.wg_pub_key).expect("client pk");
        let sealed = envelope::seal(&client_pk, &self.keys, &response.encode_to_vec())
            .expect("seal login reply");
        Ok(Response::new(EncryptedMessage { wg_pub_key: env.wg_pub_key, body: sealed, version: 0 }))
    }

    async fn sync(
        &self,
        request: Request<EncryptedMessage>,
    ) -> Result<Response<tonic::codegen::BoxStream<EncryptedMessage>>, Status> {
        let env = request.into_inner();
        let plaintext = decrypt_body(&self.keys, &env)?;
        SyncRequest::decode(plaintext.as_slice())
            .map_err(|e| Status::invalid_argument(format!("first frame is not SyncRequest: {e}")))?;
        *self.sync_opens.lock().expect("sync opens") += 1;
        let client_pk = EnvelopePublicKey::from_base64(&env.wg_pub_key).expect("client pk");

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<EncryptedMessage, Status>>(16);
        let scripted: Vec<SyncResponse> =
            std::mem::take(&mut *self.sync_script.lock().expect("script"));
        for resp in scripted {
            let sealed = envelope::seal(&client_pk, &self.keys, &resp.encode_to_vec())
                .expect("seal scripted sync frame");
            tx.send(Ok(EncryptedMessage {
                wg_pub_key: env.wg_pub_key.clone(),
                body: sealed,
                version: 0,
            }))
            .await
            .expect("receiver alive while scripting");
        }
        // Keep the stream open for later pushes (dropping the last sender
        // would end the sync session).
        *self.live.lock().expect("live") = Some((client_pk, tx));
        Ok(Response::new(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))))
    }

    async fn logout(
        &self,
        request: Request<EncryptedMessage>,
    ) -> Result<Response<Empty>, Status> {
        let env = request.into_inner();
        let _ = decrypt_body(&self.keys, &env)?;
        *self.logout_calls.lock().expect("logout calls") += 1;
        Ok(Response::new(Empty {}))
    }
}

async fn spawn_mock(svc: MiniMgmt) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(ManagementServiceServer::new(svc))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .expect("tonic serve");
    });
    addr
}

// ---------------------------------------------------------------------------
// sync builders + connector construction
// ---------------------------------------------------------------------------

fn mgmt_peer(key: &str, ip: &str) -> netbird_core::grpc::proto::RemotePeerConfig {
    netbird_core::grpc::proto::RemotePeerConfig {
        wg_pub_key: key.into(),
        allowed_ips: vec![format!("{ip}/32")],
        ssh_config: None,
        fqdn: format!("{key}.example.net"),
        agent_version: "1.0.0".into(),
        lazy_state: 0,
    }
}

/// A sync snapshot carrying `NetbirdConfig.relay` plus a minimal network
/// map (so the full apply path — including the map — is exercised).
fn sync_with_relay(
    serial: u64,
    urls: Vec<String>,
    token_payload: String,
) -> SyncResponse {
    SyncResponse {
        netbird_config: Some(netbird_core::grpc::proto::NetbirdConfig {
            relay: Some(netbird_core::grpc::proto::RelayConfig {
                urls,
                token_payload,
                token_signature: SIG_B64.into(),
            }),
            ..Default::default()
        }),
        network_map: Some(netbird_core::grpc::proto::NetworkMap {
            serial,
            peer_config: Some(netbird_core::grpc::proto::PeerConfig {
                address: "10.64.0.9".into(),
                ..Default::default()
            }),
            remote_peers: vec![mgmt_peer("UEVFUjBBMQ==", "10.30.30.1")],
            remote_peers_is_empty: false,
            routes: vec![],
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Recording host seam: applied serials (know when a map has been applied).
#[derive(Default)]
struct RecordingHost {
    serials: Mutex<Vec<u64>>,
}

impl ConfigApplier for RecordingHost {
    fn apply(&self, map: &netbird_core::network_map::NetworkMap) {
        self.serials.lock().expect("serials").push(map.serial);
    }
    fn clear(&self) {}
}

fn connector_config_for(addr: std::net::SocketAddr) -> ConnectorConfig {
    ConnectorConfig::from_json(&format!(
        "{{\"management_url\":\"http://{addr}\",\"private_key\":\"{}\",\"allow_unprotected_management\":true}}",
        test_private_key_b64(1)
    ))
    .expect("test config")
}

/// Spawn a connector against the mock; `relay` decides the N13-D1 opt-in
/// (`None` = the hard default: no relay anything).
fn spawn_connector(
    addr: std::net::SocketAddr,
    relay: Option<RelayAttach>,
) -> (Arc<ConnectorHandle>, Arc<RecordingHost>) {
    let host = Arc::new(RecordingHost::default());
    let config = connector_config_for(addr);
    let handle = ConnectorHandle::spawn_with_relay(
        tokio::runtime::Handle::current(),
        Arc::new(GrpcManagementFactory::new(
            &config,
            EnvelopeKeyPair::from_secret_bytes(&test_private_key_b64(1).decode_base64()),
        )),
        Arc::new(WgPeerRegistry::new()),
        host.clone(),
        netbird_core::connector::ConnectorSecrets {
            setup_key: "SETUP-KEY-TEST-OK".into(),
            jwt: String::new(),
        },
        PeerMeta {
            hostname: "ohos-connector-test".into(),
            os_name: "harmonyos".into(),
            os_version: "5.0.0".into(),
            netbird_version: "0.1.0".into(),
        },
        ExponentialBackoff::upstream_stream_default(),
        SyncPolicy::production(),
        Duration::from_secs(600),
        Duration::from_secs(3600), // renewal never fires in these tests
        false,                     // N3-7 default-route gate: no force opt-in
        None,                      // no protected management socket
        None,                      // production (idle) ICE orchestrator
        None,                      // no signal material
        None,                      // no WG device feed
        netbird_core::connector::HostIceTuning::default(),
        relay.is_some(),
        relay,
    );
    (handle, host)
}

trait DecodeB64 {
    fn decode_base64(&self) -> [u8; 32];
}

impl DecodeB64 for String {
    fn decode_base64(&self) -> [u8; 32] {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(self.as_bytes())
            .expect("sentinel key is valid base64");
        raw.try_into().expect("sentinel key is 32 bytes")
    }
}

/// Test relay materials: injected clock + real loopback TCP + (optionally)
/// the production rustls TLS connector. NO TLS by default: the `rel://`
/// plaintext topology (spec §1.1, fake-server tests).
fn test_relay_attach(clock: &VirtualClock, tls: Option<RustlsRelayTlsConnector>) -> RelayAttach {
    RelayAttach {
        wg_pubkey_b64: "emlwdGVkLXRlc3QtbG9jYWwtd2drZXk=".to_string(),
        materials: Arc::new(RelayMaterials {
            clock: Arc::new(clock.clone()),
            dialer: Arc::new(netbird_core::relay_client::TcpDialer),
            tls: tls.map(|t| Arc::new(t) as Arc<dyn netbird_core::relay_client::RelayTlsConnector>),
        }),
    }
}

// ---------------------------------------------------------------------------
// raw remote relay client (loopback fake server, real WS)
// ---------------------------------------------------------------------------

async fn connect_remote(
    server: &RelayTestServer,
    id: &PeerId,
    token: AuthToken,
) -> WsClient<tokio::net::TcpStream> {
    let tcp = tokio::net::TcpStream::connect(server.addr()).await.expect("remote dials");
    let key = random_sec_websocket_key().expect("random key");
    let mut ws = client_handshake(tcp, &server.addr().to_string(), "/relay", &key)
        .await
        .expect("remote ws handshake");
    ws.write_binary(&Frame::Auth { peer_id: id.clone(), token }.encode().expect("auth frame"))
        .await
        .expect("remote auth send");
    match ws.read_message().await.expect("remote auth response") {
        WsMessage::Binary(bytes) => {
            let frame = Frame::decode(&bytes).expect("remote auth response frame");
            assert!(
                matches!(frame, Frame::AuthResponse { .. }),
                "remote must be authenticated first, got {frame:?}"
            );
        }
        other => panic!("expected auth response binary, got {other:?}"),
    }
    ws
}

async fn send_frame(ws: &mut WsClient<tokio::net::TcpStream>, frame: Frame) {
    ws.write_binary(&frame.encode().expect("frame encodes")).await.expect("frame on the wire");
}

async fn expect_frame(ws: &mut WsClient<tokio::net::TcpStream>, what: &str) -> Frame {
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

// ---------------------------------------------------------------------------
// 1. relay_enabled absent (default) → zero relay behavior
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn relay_disabled_is_the_default_and_never_touches_relay() {
    let mock = MiniMgmt::default();
    // management DOES advertise relays — the disabled connector must ignore it
    mock.script(vec![sync_with_relay(
        7,
        vec!["PLACEHOLDER".into()], // patched below (needs the server port)
        (EPOCH + 3_600).to_string(),
    )]);
    let relay_server = RelayTestServer::start(TestServerConfig::default()).await.expect("fake relay");
    mock.script(vec![sync_with_relay(
        7,
        vec![loopback_url(relay_server.addr().port())],
        (EPOCH + 3_600).to_string(),
    )]);
    let addr = spawn_mock(mock.clone()).await;
    let (handle, host) = spawn_connector(addr, None);

    // the connector otherwise behaves EXACTLY as before: the map is applied
    wait_for(FUSE, || host.serials.lock().expect("serials").last() == Some(&7), "map applied")
        .await;

    let status = handle.status();
    assert!(!status.relay.enabled, "relay must be DISABLED by default");
    assert_eq!(status.relay.state, "disabled");
    assert!(status.relay.urls.is_empty());
    assert_eq!(status.relay.frames_tx + status.relay.frames_rx, 0);
    assert!(!status.relay.token_valid);
    assert!(handle.relay_client().is_none(), "no relay client handle when disabled");

    // and NOTHING ever dials the relay server (bounded negative assertion)
    assert_stays_false(
        Duration::from_millis(500),
        || relay_server.stats().connections_accepted > 0,
        "disabled connector must not open any relay connection",
    )
    .await;
    assert_eq!(relay_server.stats().connections_accepted, 0);

    // the status document renders the frozen disabled shape
    let json = handle.status_json();
    assert!(
        json.contains(
            "\"relay\":{\"enabled\":false,\"state\":\"disabled\",\"urls\":[],\"reconnects\":0,\
             \"frames_tx\":0,\"frames_rx\":0,\"transport_bytes\":0,\"token_valid\":false,\
             \"last_error_class\":null}"
        ),
        "{json}"
    );

    handle.stop();
}

// ---------------------------------------------------------------------------
// 2. opt-in → Ready + byte-exact roundtrip + status reconciliation
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn relay_optin_reaches_ready_and_round_trips_bytes_byte_exact() {
    let relay_server =
        RelayTestServer::start(TestServerConfig::default()).await.expect("fake relay");
    let mock = MiniMgmt::default();
    mock.script(vec![sync_with_relay(
        1,
        vec![loopback_url(relay_server.addr().port())],
        (EPOCH + 3_600).to_string(),
    )]);
    let addr = spawn_mock(mock.clone()).await;
    let clock = VirtualClock::new(EPOCH);
    let (handle, _host) = spawn_connector(addr, Some(test_relay_attach(&clock, None)));

    wait_for(
        FUSE,
        || handle.status().relay.state == "ready",
        "connector's relay client reaches Ready",
    )
    .await;
    assert!(handle.status().relay.token_valid);
    assert_eq!(relay_server.stats().connections_accepted, 1);

    // a raw REMOTE peer joins the same fake server
    let remote = remote_peer();
    let mut remote_ws =
        connect_remote(&relay_server, &remote, relay_token(EPOCH + 3_600)).await;

    // the connector opens a relay lane to the remote (PeersOnline flows)
    let relay_client = handle.relay_client().expect("relay client present");
    timeout(FUSE, relay_client.open_conn(&remote))
        .await
        .expect("open_conn settles")
        .expect("open_conn succeeds (remote online)");

    // WG-shaped payloads: byte-exact roundtrip (incl. zeros / 0xFF / 1400B)
    let mut payload1 = vec![0u8; 100];
    for (i, b) in payload1.iter_mut().enumerate() {
        *b = (i * 7 + 3) as u8;
    }
    let mut payload2 = vec![0u8; 1400];
    for (i, b) in payload2.iter_mut().enumerate() {
        *b = (i as u8) ^ 0xA5;
    }

    let reader = relay_client.clone();
    let want_total = payload1.len() + payload2.len();
    let recv_task = tokio::spawn(async move {
        let mut out = Vec::new();
        while out.len() < want_total {
            let (_sender, payload) =
                timeout(FUSE, reader.recv()).await.expect("recv fuse").expect("relay open");
            out.extend_from_slice(&payload);
        }
        out
    });

    relay_client.send_to_peer(&remote, &payload1).expect("send payload1");
    let f1 = expect_frame(&mut remote_ws, "transport payload1").await;
    let Frame::Transport { peer_id: sender, payload: got1 } = f1 else {
        panic!("want Transport, got {f1:?}")
    };
    assert_eq!(sender, relay_client_peer(), "36B field is the SENDER id (§4.2)");
    assert_eq!(got1, payload1, "payload1 byte-exact");

    relay_client.send_to_peer(&remote, &payload2).expect("send payload2");
    let f2 = expect_frame(&mut remote_ws, "transport payload2").await;
    let Frame::Transport { peer_id: _, payload: got2 } = f2 else {
        panic!("want Transport, got {f2:?}")
    };
    assert_eq!(got2, payload2, "payload2 byte-exact");

    // echo both back → the connector must receive the exact bytes
    send_frame(&mut remote_ws, Frame::Transport { peer_id: sender.clone(), payload: got1 }).await;
    send_frame(&mut remote_ws, Frame::Transport { peer_id: sender, payload: got2 }).await;
    let received = timeout(FUSE * 2, recv_task).await.expect("recv task settles").unwrap();
    let mut want = payload1.clone();
    want.extend_from_slice(&payload2);
    assert_eq!(received, want, "roundtrip is byte-exact");

    // status reconciliation — relay section vs the client and the server
    let stats = relay_client.stats();
    let relay = handle.status().relay;
    assert!(relay.enabled);
    assert_eq!(relay.state, "ready");
    assert_eq!(relay.urls, vec![loopback_url(relay_server.addr().port())]);
    assert_eq!(relay.reconnects, 0);
    assert_eq!(relay.frames_tx as usize, stats.frames_tx.iter().sum::<u64>() as usize);
    assert_eq!(relay.frames_rx as usize, stats.frames_rx.iter().sum::<u64>() as usize);
    assert_eq!(relay.frames_tx, 4, "auth + subscribe + 2 transports");
    assert_eq!(relay.frames_rx, 4, "authresponse + peers-online + 2 transports");
    assert_eq!(relay.transport_bytes, 3000, "1500 out + 1500 in (payload bytes)");
    assert_eq!(
        relay_server.stats().transport_bytes_forwarded,
        3000,
        "the fake server forwarded exactly the same payload bytes"
    );
    assert!(relay.token_valid);
    assert_eq!(relay.last_error_class, None);

    // the frozen JSON shape (exact substring), and it stays valid JSON
    let json = handle.status_json();
    let want_relay = format!(
        "\"relay\":{{\"enabled\":true,\"state\":\"ready\",\"urls\":[\"{}\"],\"reconnects\":0,\
         \"frames_tx\":4,\"frames_rx\":4,\"transport_bytes\":3000,\"token_valid\":true,\
         \"last_error_class\":null}}",
        loopback_url(relay_server.addr().port())
    );
    assert!(json.contains(&want_relay), "want {want_relay} in {json}");

    handle.stop();
    relay_server.shutdown().await;
}

/// The LOCAL peer id the connector's relay client derives (from the attach
/// fixture's wg pubkey) — used for the sender-id assertion.
fn relay_client_peer() -> PeerId {
    PeerId::from_wg_pubkey_string("emlwdGVkLXRlc3QtbG9jYWwtd2drZXk=")
}

// ---------------------------------------------------------------------------
// 3. token expiry → fail-closed + VISIBLE; a new sync recovers it (zero wait)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn sync_token_refresh_recovers_relay_after_expiry_with_zero_wait() {
    let relay_server =
        RelayTestServer::start(TestServerConfig::default()).await.expect("fake relay");
    let mock = MiniMgmt::default();
    // token valid for only 60 virtual seconds
    mock.script(vec![sync_with_relay(
        1,
        vec![loopback_url(relay_server.addr().port())],
        (EPOCH + 60).to_string(),
    )]);
    let addr = spawn_mock(mock.clone()).await;
    let clock = VirtualClock::new(EPOCH);
    let (handle, _host) = spawn_connector(addr, Some(test_relay_attach(&clock, None)));

    wait_for(FUSE, || handle.status().relay.state == "ready", "ready").await;
    assert_eq!(relay_server.stats().connections_accepted, 1);

    // virtual clock crosses the expiry: the session is killed fail-closed
    clock.advance(Duration::from_secs(700));
    wait_for(FUSE, || {
        let relay = handle.status().relay;
        !relay.token_valid && relay.state == "reconnecting"
    }, "expiry is fail-closed AND visible in status")
    .await;
    assert!(!handle.status().relay.enabled.then_some(true).unwrap_or(false) == false); // enabled stays
    assert!(
        handle.status_json().contains("\"token_valid\":false"),
        "expired token must be visible in the status document"
    );

    // while expired, the relay is NOT usable: send_to_peer refuses typed
    let relay_client = handle.relay_client().expect("client present");
    let err = relay_client
        .send_to_peer(&remote_peer(), b"x".as_slice())
        .expect_err("not ready while expired");
    assert!(matches!(err, netbird_core::relay_client::RelayClientError::NotReady { .. }), "{err:?}");

    // a NEW sync carries fresh RelayServers → update_token wakes the hold
    mock.push(sync_with_relay(
        2,
        vec![loopback_url(relay_server.addr().port())],
        (EPOCH + 700 + 3_600).to_string(),
    ));
    wait_for(FUSE, || {
        let relay = handle.status().relay;
        relay.state == "ready" && relay.token_valid
    }, "relay recovered after the synced token refresh (zero real wait)")
    .await;
    // exactly one recovery dial — no retry addiction
    wait_for(FUSE, || relay_server.stats().connections_accepted == 2, "one recovery session")
        .await;
    assert_stays_false(
        Duration::from_millis(300),
        || relay_server.stats().connections_accepted > 2,
        "no dial addiction after recovery",
    )
    .await;

    handle.stop();
    relay_server.shutdown().await;
}

// ---------------------------------------------------------------------------
// 4. connector_stop() → relay revocation cleanup
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn connector_stop_tears_the_relay_connection_down() {
    let relay_server =
        RelayTestServer::start(TestServerConfig::default()).await.expect("fake relay");
    let mock = MiniMgmt::default();
    mock.script(vec![sync_with_relay(
        1,
        vec![loopback_url(relay_server.addr().port())],
        (EPOCH + 3_600).to_string(),
    )]);
    let addr = spawn_mock(mock.clone()).await;
    let clock = VirtualClock::new(EPOCH);
    let (handle, _host) = spawn_connector(addr, Some(test_relay_attach(&clock, None)));

    wait_for(FUSE, || handle.status().relay.state == "ready", "ready").await;
    assert_eq!(relay_server.stats().connections_accepted, 1);

    // revoke: the connector stop tears the relay client down with it
    let stop_json = handle.stop_json();
    assert!(stop_json.contains("\"already_stopped\":false"), "{stop_json}");

    wait_for(FUSE, || handle.status().relay.state == "dead", "relay reaches terminal Dead")
        .await;
    wait_for(FUSE, || {
        relay_server.stats().frames_rx(MSG_CLOSE) == 1
    }, "the server observed exactly one relay Close frame (graceful exit)")
        .await;

    // and NO new outbound relay connection ever appears afterwards
    let accepted = relay_server.stats().connections_accepted;
    assert_eq!(accepted, 1, "no reconnect after the connector stop");
    assert_stays_false(
        Duration::from_millis(500),
        || relay_server.stats().connections_accepted > accepted,
        "connector_stop must leave no orphan outbound relay connection",
    )
    .await;

    // the stopped client stays renderable (terminal state visible) and is
    // inert: send refuses typed.
    let relay_client = handle.relay_client().expect("dead client still renderable");
    assert!(relay_client
        .send_to_peer(&remote_peer(), b"x".as_slice())
        .is_err());
    let _ = relay_server.shutdown().await;
}

// ---------------------------------------------------------------------------
// 5. the rustls TLS connector: a REAL handshake + record exchange
// ---------------------------------------------------------------------------

/// Test-only self-signed certificate + key (EC P-256, CN/SAN localhost +
/// IP 127.0.0.1, end-entity: CA:FALSE). Fabricated fixtures, NOT secret
/// material; valid until 2126. Generated offline with openssl for this test
/// suite only.
const TEST_CERT_PEM: &str = "-----BEGIN CERTIFICATE-----
MIIBqDCCAU6gAwIBAgIUSwQJY2DBLCjifDc568u3mbhHvz0wCgYIKoZIzj0EAwIw
FDESMBAGA1UEAwwJbG9jYWxob3N0MCAXDTI2MDkxNDEzNTAwMFoYDzIxMjYwODIx
MTM1MDAwWjAUMRIwEAYDVQQDDAlsb2NhbGhvc3QwWTATBgcqhkjOPQIBBggqhkjO
PQMBBwNCAAScAKnlckaLOu0g+bmdhmdrq7r0Y156t2+0wDbsm2nE6Jq+MBh1CWaH
8xPIk69mLlnrq5+Tx4/1EDRwski8Kyp8o3wwejAdBgNVHQ4EFgQUmV2rxYxTmrY8
v+67hT08AH1139wwHwYDVR0jBBgwFoAUmV2rxYxTmrY8v+67hT08AH1139wwGgYD
VR0RBBMwEYIJbG9jYWxob3N0hwR/AAABMAwGA1UdEwEB/wQCMAAwDgYDVR0PAQH/
BAQDAgWgMAoGCCqGSM49BAMCA0gAMEUCIEZczUACBqD9JOLeNVX1/0NesZth4JB4
qwA4oNHw8ibFAiEA0HL3OrshkeHfaD4J76+5K9gDBhkbcwEBBoQbGL6Sy3c=
-----END CERTIFICATE-----
";

const TEST_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgy80Urf9va5lpBJgj
pK+WSV4DBtynEVNKaVw+KaXFYpChRANCAAScAKnlckaLOu0g+bmdhmdrq7r0Y156
t2+0wDbsm2nE6Jq+MBh1CWaH8xPIk69mLlnrq5+Tx4/1EDRwski8Kyp8
-----END PRIVATE KEY-----
";

/// Server-side TLS plumbing over a tokio stream (poll-method style — the
/// frozen tokio feature set has no io-util). Mirrors the client-side
/// driving implemented in `relay_client.rs`.
struct TestServerTls<S> {
    conn: rustls::ServerConnection,
    sock: S,
}

impl<S: AsyncRead + AsyncWrite + Unpin> TestServerTls<S> {
    async fn handshake(mut self) -> std::io::Result<Self> {
        let mut buf = vec![0u8; 16_384];
        loop {
            if !self.conn.is_handshaking() {
                return Ok(self);
            }
            self.flush().await?;
            if !self.conn.is_handshaking() {
                return Ok(self);
            }
            let n = read_some(&mut self.sock, &mut buf).await?;
            if n == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "server: eof during handshake",
                ));
            }
            self.conn
                .read_tls(&mut &buf[..n])
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            self.conn
                .process_new_packets()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        }
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        let mut out = [0u8; 16_384];
        while self.conn.wants_write() {
            let mut slice: &mut [u8] = &mut out;
            let n = self
                .conn
                .write_tls(&mut slice)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            if n == 0 {
                return Ok(());
            }
            write_all(&mut self.sock, &out[..n]).await?;
        }
        Ok(())
    }

    /// Read decrypted plaintext (`Ok(0)` = clean TLS EOF).
    async fn read_plain(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use std::io::Read as _;
        loop {
            match self.conn.reader().read(buf) {
                Ok(0) => return Ok(0),
                Ok(n) => return Ok(n),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e),
            }
            self.flush().await?;
            let mut wire = vec![0u8; 16_384];
            let got = read_some(&mut self.sock, &mut wire).await?;
            if got == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "server: eof without close_notify",
                ));
            }
            // `read_tls` consumes AT MOST one internal read step (~4 KiB)
            // per call — loop to the last byte (dropping the remainder
            // would silently lose wire data), processing as we go.
            let mut chunk: &[u8] = &wire[..got];
            while !chunk.is_empty() {
                let consumed = self
                    .conn
                    .read_tls(&mut chunk)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                if consumed == 0 {
                    break;
                }
                self.conn
                    .process_new_packets()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            }
        }
    }

    async fn write_plain(&mut self, data: &[u8]) -> std::io::Result<()> {
        use std::io::Write as _;
        self.conn.writer().write_all(data)?;
        self.flush().await
    }

    async fn shutdown(&mut self) -> std::io::Result<()> {
        self.conn.send_close_notify();
        self.flush().await?;
        std::future::poll_fn(|cx| Pin::new(&mut self.sock).poll_shutdown(cx)).await
    }
}

async fn read_some<S: AsyncRead + Unpin>(io: &mut S, buf: &mut [u8]) -> std::io::Result<usize> {
    std::future::poll_fn(|cx| {
        let mut rb = ReadBuf::new(buf);
        match std::pin::Pin::new(&mut *io).poll_read(cx, &mut rb) {
            std::task::Poll::Ready(Ok(())) => std::task::Poll::Ready(Ok(rb.filled().len())),
            std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Err(e)),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    })
    .await
}

async fn write_all<S: AsyncWrite + Unpin>(io: &mut S, mut data: &[u8]) -> std::io::Result<()> {
    while !data.is_empty() {
        let n =
            std::future::poll_fn(|cx| std::pin::Pin::new(&mut *io).poll_write(cx, data)).await?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::WriteZero, "no progress"));
        }
        data = &data[n..];
    }
    Ok(())
}

/// Accept loop: for every connection run a rustls TLS 1.3 handshake and
/// echo every decrypted byte back until clean EOF.
async fn tls_echo_accept_loop(listener: TcpListener) {
    use rustls::pki_types::pem::PemObject as _;
    let certs: Vec<rustls::pki_types::CertificateDer<'static>> =
        rustls::pki_types::CertificateDer::pem_slice_iter(TEST_CERT_PEM.as_bytes())
            .map(|c| c.expect("fixture cert parses"))
            .collect();
    let key = rustls::pki_types::PrivateKeyDer::from_pem_slice(TEST_KEY_PEM.as_bytes())
        .expect("fixture key parses");
    let config = Arc::new(
        rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("protocol versions")
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("server config"),
    );
    loop {
        let (sock, _) = match listener.accept().await {
            Ok(x) => x,
            Err(_) => return,
        };
        let config = config.clone();
        tokio::spawn(async move {
            let conn = match rustls::ServerConnection::new(config) {
                Ok(c) => c,
                Err(_) => return,
            };
            let mut tls = match (TestServerTls { conn, sock }).handshake().await {
                Ok(t) => t,
                Err(_) => return,
            };
            // speak FIRST (unsolicited server data), then echo to EOF
            if tls.write_plain(b"tls-server-hello-0123456789").await.is_err() {
                return;
            }
            let mut buf = vec![0u8; 16_384];
            loop {
                match tls.read_plain(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tls.write_plain(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = tls.shutdown().await;
        });
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn rustls_relay_tls_connector_performs_a_real_tls_session() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(tls_echo_accept_loop(listener));

    let connector = RustlsRelayTlsConnector::new(vec![TEST_CERT_PEM.as_bytes().to_vec()])
        .expect("valid fixture cert builds the connector");

    // Exercise BOTH name forms: FQDN (SNI=localhost) and the IP literal
    // (cert carries an IP SAN, spec §1.4 IP-dial keeps a verified name).
    for server_name in ["localhost", "127.0.0.1"] {
        let tcp = tokio::net::TcpStream::connect(addr).await.expect("tcp");
        let fut = {
            let connector = &connector;
            let server_name = server_name.to_string();
            async move { connector.clone_connect(Box::new(tcp), server_name).await }
        };
        let mut stream = timeout(FUSE, fut)
            .await
            .expect("tls handshake within fuse")
            .expect("handshake succeeds against the rustls server");

        // the server speaks first — unsolicited data must decrypt exactly
        // (27 bytes = the full hello; reading fewer would leave one byte in
        // the rustls receive buffer and skew the next read)
        let mut hello = [0u8; 27];
        read_exact_stream(&mut stream, &mut hello).await;
        assert_eq!(&hello, b"tls-server-hello-0123456789".as_slice(), "unsolicited read");

        // then a large bidirectional roundtrip (spans multiple TLS records)
        let payload: Vec<u8> = (0..40_000u32).map(|i| (i % 251) as u8).collect();
        write_all(&mut stream, &payload).await.expect("payload out");
        let mut echo = vec![0u8; payload.len()];
        read_exact_stream(&mut stream, &mut echo).await;
        assert_eq!(echo, payload, "40 KiB roundtrip byte-exact over {server_name}");

        // graceful shutdown (close_notify) — the echo server exits cleanly
        std::future::poll_fn(|cx| Pin::new(&mut *stream_as_mut(&mut stream)).poll_shutdown(cx))
            .await
            .expect("shutdown");
    }
}

// Small shims so the stream-type-erased Box<dyn RelayStream> can be used
// with the poll helpers above.
fn stream_as_mut(
    s: &mut Box<dyn netbird_core::relay_client::RelayStream>,
) -> &mut Box<dyn netbird_core::relay_client::RelayStream> {
    s
}

async fn read_exact_stream(
    stream: &mut Box<dyn netbird_core::relay_client::RelayStream>,
    buf: &mut [u8],
) {
    let mut filled = 0;
    while filled < buf.len() {
        let n = read_some(stream, &mut buf[filled..]).await.expect("stream read");
        assert!(n > 0, "unexpected EOF at {filled}/{}", buf.len());
        filled += n;
    }
}

// The production trait object method is `connect` (RelayTlsConnector); call
// it through the trait to keep the test on the real seam surface.
trait ConnectHelper {
    async fn clone_connect(
        &self,
        stream: Box<dyn netbird_core::relay_client::RelayStream>,
        server_name: String,
    ) -> Result<Box<dyn netbird_core::relay_client::RelayStream>, netbird_core::relay_client::RelayClientError>;
}

impl ConnectHelper for RustlsRelayTlsConnector {
    async fn clone_connect(
        &self,
        stream: Box<dyn netbird_core::relay_client::RelayStream>,
        server_name: String,
    ) -> Result<Box<dyn netbird_core::relay_client::RelayStream>, netbird_core::relay_client::RelayClientError>
    {
        netbird_core::relay_client::RelayTlsConnector::connect(self, stream, server_name).await
    }
}

use std::pin::Pin;

// ---------------------------------------------------------------------------
// 6. rels:// against a PLAINTEXT server: TLS is attempted, never downgraded
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn rels_with_valid_roots_attempts_tls_and_fails_closed_on_a_plain_server() {
    let relay_server =
        RelayTestServer::start(TestServerConfig::default()).await.expect("plain fake relay");
    let mock = MiniMgmt::default();
    mock.script(vec![sync_with_relay(
        1,
        vec![format!("rels://127.0.0.1:{}", relay_server.addr().port())],
        (EPOCH + 3_600).to_string(),
    )]);
    let addr = spawn_mock(mock.clone()).await;
    let clock = VirtualClock::new(EPOCH);
    // materials WITH the rustls connector (valid roots — the fixture cert)
    let attach = test_relay_attach(
        &clock,
        Some(RustlsRelayTlsConnector::new(vec![TEST_CERT_PEM.as_bytes().to_vec()])
            .expect("fixture cert builds")),
    );
    let (handle, _host) = spawn_connector(addr, Some(attach));

    // the client dials, the TLS layer sends the ClientHello, the plain
    // server fails the bogus "HTTP request" — the session never becomes
    // Ready, and it is retried with backoff, NEVER downgraded to plaintext.
    wait_for(FUSE, || {
        let relay = handle.status().relay;
        relay.state != "ready" && relay.urls.len() == 1
    }, "relay client is running (never Ready against a plain server)")
        .await;
    // the dial REALLY happened (the server accepted the TCP connection —
    // its handshake-failure accounting only fires after its 30s op fuse,
    // which the virtual clock deliberately never advances to)
    wait_for(FUSE, || relay_server.stats().connections_accepted == 1, "the TLS dial reached the server").await;

    // no plaintext WS upgrade can EVER succeed here: auth must stay at zero
    assert_eq!(relay_server.stats().auth_rejected, 0);
    assert_eq!(
        relay_server.stats().frames_rx(netbird_core::relay::MSG_AUTH),
        0,
        "no relay auth ever flowed in the clear"
    );
    assert_ne!(handle.status().relay.state, "ready");

    handle.stop();
    relay_server.shutdown().await;
}
