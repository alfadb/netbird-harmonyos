// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! Loop-dependency fix: the connector must EXPOSE the management-advertised
//! relay URL set (`NetbirdConfig.relay`) in its status document ALWAYS —
//! also while the relay opt-in `relay_enabled` is FALSE (the hard default).
//! The shell's N2-H endpoint exclusion needs the relay host BEFORE relay can
//! be enabled; the old expose-only-while-running rule made the exclusion
//! depend on the very state it must precede.
//!
//! Pinned here (real connector, real in-process management mock, real
//! loopback fake relay server — same topology as tests/relay_connector_e2e):
//! 1. `relay_enabled=false` + a sync advertising relay urls → the status
//!    shows `advertised_urls` while the `relay` section stays exactly
//!    "disabled", and the advertised-but-reachable fake server counts ZERO
//!    accepted connections (no outbound relay behavior whatsoever);
//! 2. `relay_enabled=true` → the pre-existing runtime behavior is unchanged
//!    (client reaches Ready over the fake server, exactly one dial) and the
//!    advertised view coexists with the runtime view;
//! 3. management advertising NO relay → `advertised_urls` empty, semantics
//!    unchanged;
//! 4. a second sync carrying a NEW url set → the advertised record follows
//!    the latest sync — still zero dials while disabled.
//!
//! Determinism discipline: no sleeps as assertions — positive waits are
//! bounded predicate loops, negative assertions are bounded windows whose
//! expiry IS the pass condition. All tokens/keys are fabricated test
//! material; token content must never appear in the status document and the
//! tests pin that too.

use core::time::Duration;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use base64::Engine as _;
use prost::Message as _;
use tokio::net::TcpListener;
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
use netbird_core::relay_client::VirtualClock;
use netbird_core::relay_testserver::{RelayTestServer, TestServerConfig};

// Host-process link stubs (repo convention, cf. tests/relay_connector_e2e.rs):
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
        _cb: *const c_void,
        _data: *const c_void,
        _result: *mut *mut c_void,
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
        _data: *mut *mut c_void,
        _this: *mut *mut c_void,
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

    #[no_mangle]
    pub extern "C" fn napi_set_named_property(
        _env: *mut c_void,
        _name: *const c_void,
        _value: *mut c_void,
    ) -> i32 {
        0
    }
}

// ---------------------------------------------------------------------------
// fixtures (all fabricated — never secret)
// ---------------------------------------------------------------------------

/// Real-time fuse for events that MUST happen. HANG GUARD, not a latency
/// assertion (cf. tests/relay_connector_e2e.rs).
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

/// Distinctive fabricated token payload/sentinel — the tests pin that this
/// string NEVER appears in the status document (credential discipline).
const TOKEN_SENTINEL: &str = "4242424242";

/// Fabricated (NOT secret) 32-byte signature, base64 std — same fixture
/// shape as tests/relay_connector_e2e.rs.
const SIG_B64: &str = "paWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaU=";

/// Fabricated virtual epoch (2026-02-02).
const EPOCH: u64 = 1_770_000_000;

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
    sync_opens: Arc<Mutex<usize>>,
}

impl Default for MiniMgmt {
    fn default() -> Self {
        MiniMgmt {
            keys: EnvelopeKeyPair::generate().expect("server key pair"),
            sync_script: Arc::new(Mutex::new(Vec::new())),
            sync_opens: Arc::new(Mutex::new(0)),
        }
    }
}

impl MiniMgmt {
    fn script(&self, updates: Vec<SyncResponse>) {
        *self.sync_script.lock().expect("script") = updates;
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
        Ok(Response::new(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))))
    }

    async fn logout(
        &self,
        request: Request<EncryptedMessage>,
    ) -> Result<Response<Empty>, Status> {
        let env = request.into_inner();
        let _ = decrypt_body(&self.keys, &env)?;
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

/// A sync snapshot carrying `NetbirdConfig.relay` plus a minimal network map
/// (the map serial is the observable "this sync was applied" marker).
fn sync_with_relay(serial: u64, urls: Vec<String>, token_payload: String) -> SyncResponse {
    SyncResponse {
        netbird_config: Some(netbird_core::grpc::proto::NetbirdConfig {
            relay: Some(netbird_core::grpc::proto::RelayConfig {
                urls,
                token_payload,
                token_signature: SIG_B64.into(),
            }),
            ..Default::default()
        }),
        network_map: Some(minimal_map(serial)),
        ..Default::default()
    }
}

/// A sync snapshot whose `NetbirdConfig` carries NO relay (management does
/// not advertise relay).
fn sync_without_relay(serial: u64) -> SyncResponse {
    SyncResponse {
        netbird_config: Some(netbird_core::grpc::proto::NetbirdConfig::default()),
        network_map: Some(minimal_map(serial)),
        ..Default::default()
    }
}

fn minimal_map(serial: u64) -> netbird_core::grpc::proto::NetworkMap {
    netbird_core::grpc::proto::NetworkMap {
        serial,
        peer_config: Some(netbird_core::grpc::proto::PeerConfig {
            address: "10.64.0.9".into(),
            ..Default::default()
        }),
        remote_peers: vec![netbird_core::grpc::proto::RemotePeerConfig {
            wg_pub_key: "UEVFUjBBMQ==".into(),
            allowed_ips: vec!["10.30.30.1/32".into()],
            ssh_config: None,
            fqdn: "UEVFUjBBMQ==.example.net".into(),
            agent_version: "1.0.0".into(),
            lazy_state: 0,
        }],
        remote_peers_is_empty: false,
        routes: vec![],
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
            EnvelopeKeyPair::from_secret_bytes(&DecB64(test_private_key_b64(1)).into_key()),
        )),
        Arc::new(WgPeerRegistry::new()),
        host.clone(),
        netbird_core::connector::ConnectorSecrets {
            setup_key: "SETUP-KEY-TEST-OK".into(),
            jwt: String::new(),
        },
        PeerMeta {
            hostname: "ohos-advertised-urls-test".into(),
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

/// base64 → 32 raw bytes (fabricated sentinel key material).
struct DecB64(String);

impl DecB64 {
    fn into_key(self) -> [u8; 32] {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(self.0.as_bytes())
            .expect("sentinel key is valid base64");
        raw.try_into().expect("sentinel key is 32 bytes")
    }
}

/// Test relay materials: injected clock + real loopback TCP. NO TLS (the
/// `rel://` plaintext fake-server topology).
fn test_relay_attach(clock: &VirtualClock) -> RelayAttach {
    RelayAttach {
        wg_pubkey_b64: "emlwdGVkLXRlc3QtbG9jYWwtd2drZXk=".to_string(),
        materials: Arc::new(RelayMaterials {
            clock: Arc::new(clock.clone()),
            dialer: Arc::new(netbird_core::relay_client::TcpDialer),
            tls: None,
        }),
    }
}

/// The frozen pre-change shape of the disabled `relay` runtime object —
/// every test below pins that it is byte-identical (additive-only change).
fn frozen_disabled_relay_json() -> String {
    "{\"enabled\":false,\"state\":\"disabled\",\"urls\":[],\"reconnects\":0,\
     \"frames_tx\":0,\"frames_rx\":0,\"transport_bytes\":0,\"token_valid\":false,\
     \"last_error_class\":null}"
    .to_string()
}

// ---------------------------------------------------------------------------
// 1. relay_enabled=false + advertised urls → exposed, zero dial
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn relay_disabled_still_exposes_advertised_urls_and_never_dials() {
    let relay_server = RelayTestServer::start(TestServerConfig::default()).await.expect("fake relay");
    let mock = MiniMgmt::default();
    // management advertises BOTH the task's literal url AND a REACHABLE
    // loopback relay — the connector must record both and dial NEITHER.
    mock.script(vec![sync_with_relay(
        7,
        vec!["rels://a.example:28443".into(), loopback_url(relay_server.addr().port())],
        TOKEN_SENTINEL.into(),
    )]);
    let addr = spawn_mock(mock.clone()).await;
    let (handle, host) = spawn_connector(addr, None);

    // the sync (incl. the relay advertisement) was really processed
    wait_for(FUSE, || host.serials.lock().expect("serials").last() == Some(&7), "map applied")
        .await;

    let status = handle.status();
    let relay = &status.relay;
    assert!(relay.advertised, "management advertised relay");
    assert_eq!(
        relay.advertised_urls,
        vec![
            "rels://a.example:28443".to_string(),
            loopback_url(relay_server.addr().port()),
        ],
        "the advertised set is exposed verbatim while relay is disabled"
    );
    // runtime section unchanged: disabled, empty, zero, no client
    assert!(!relay.enabled);
    assert_eq!(relay.state, "disabled");
    assert!(relay.urls.is_empty());
    assert_eq!(relay.frames_tx + relay.frames_rx, 0);
    assert_eq!(relay.transport_bytes, 0);
    assert!(!relay.token_valid);
    assert!(handle.relay_client().is_none(), "no relay client handle when disabled");

    // NOTHING ever dials the advertised-and-reachable server
    assert_stays_false(
        Duration::from_millis(500),
        || relay_server.stats().connections_accepted > 0,
        "disabled connector must not open any relay connection",
    )
    .await;
    assert_eq!(relay_server.stats().connections_accepted, 0);

    // the document: advertised section + frozen disabled runtime object,
    // and the token material crossed NOWHERE
    let json = handle.status_json();
    let want_advertised = format!(
        "\"relay_advertised\":{{\"advertised\":true,\"advertised_urls\":\
         [\"rels://a.example:28443\",\"{}\"]}}",
        loopback_url(relay_server.addr().port())
    );
    assert!(json.contains(&want_advertised), "{json}");
    assert!(json.contains(&frozen_disabled_relay_json()), "{json}");
    assert!(!json.contains(TOKEN_SENTINEL), "token payload leaked into status: {json}");
    assert!(!json.contains(SIG_B64), "token signature leaked into status: {json}");

    handle.stop();
    relay_server.shutdown().await;
}

// ---------------------------------------------------------------------------
// 2. relay_enabled=true → unchanged runtime behavior + coexisting views
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn relay_enabled_keeps_runtime_semantics_and_advertised_urls_coexist() {
    let relay_server = RelayTestServer::start(TestServerConfig::default()).await.expect("fake relay");
    let mock = MiniMgmt::default();
    let url = loopback_url(relay_server.addr().port());
    mock.script(vec![sync_with_relay(1, vec![url.clone()], (EPOCH + 3_600).to_string())]);
    let addr = spawn_mock(mock.clone()).await;
    let clock = VirtualClock::new(EPOCH);
    let (handle, _host) = spawn_connector(addr, Some(test_relay_attach(&clock)));

    // pre-existing behavior: the relay client reaches Ready over the fake
    // server with EXACTLY one dial
    wait_for(FUSE, || handle.status().relay.state == "ready", "relay client Ready").await;
    assert_eq!(relay_server.stats().connections_accepted, 1, "unchanged: exactly one session");

    // the two views coexist: runtime running + management advertisement
    let relay = handle.status().relay;
    assert!(relay.enabled);
    assert_eq!(relay.state, "ready");
    assert_eq!(relay.urls, vec![url.clone()], "runtime url set");
    assert!(relay.advertised);
    assert_eq!(relay.advertised_urls, vec![url.clone()], "advertised url set");
    assert!(relay.token_valid);

    let json = handle.status_json();
    assert!(
        json.contains(&format!(
            "\"relay_advertised\":{{\"advertised\":true,\"advertised_urls\":[\"{url}\"]}}"
        )),
        "{json}"
    );
    assert!(
        json.contains(&format!("\"relay\":{{\"enabled\":true,\"state\":\"ready\",\"urls\":[\"{url}\"]")),
        "{json}"
    );

    handle.stop();
    relay_server.shutdown().await;
}

// ---------------------------------------------------------------------------
// 3. management advertising NO relay → advertised_urls empty
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn management_not_advertising_relay_leaves_advertised_urls_empty() {
    let relay_server = RelayTestServer::start(TestServerConfig::default()).await.expect("fake relay");
    let mock = MiniMgmt::default();
    mock.script(vec![sync_without_relay(3)]);
    let addr = spawn_mock(mock.clone()).await;
    let (handle, host) = spawn_connector(addr, None);

    wait_for(FUSE, || host.serials.lock().expect("serials").last() == Some(&3), "map applied")
        .await;

    let relay = handle.status().relay;
    assert!(!relay.advertised, "no relay in the sync config ⇒ not advertised");
    assert!(relay.advertised_urls.is_empty());
    // pre-existing disabled semantics untouched
    assert!(!relay.enabled);
    assert_eq!(relay.state, "disabled");
    assert!(relay.urls.is_empty());
    assert!(handle.relay_client().is_none());

    let json = handle.status_json();
    assert!(
        json.contains("\"relay_advertised\":{\"advertised\":false,\"advertised_urls\":[]}"),
        "{json}"
    );
    assert!(json.contains(&frozen_disabled_relay_json()), "{json}");

    assert_stays_false(
        Duration::from_millis(300),
        || relay_server.stats().connections_accepted > 0,
        "no relay advertisement ⇒ nothing to dial, ever",
    )
    .await;

    handle.stop();
    relay_server.shutdown().await;
}

// ---------------------------------------------------------------------------
// 4. advertisement UPDATE follows the latest sync — still zero dials
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn advertised_urls_follow_the_latest_sync_without_dialing() {
    let relay_server = RelayTestServer::start(TestServerConfig::default()).await.expect("fake relay");
    let mock = MiniMgmt::default();
    // sync 1 advertises a.example + the reachable loopback relay;
    // sync 2 REPLACES the set (management moved the relay)
    let first_urls = vec![
        "rels://a.example:28443".to_string(),
        loopback_url(relay_server.addr().port()),
    ];
    let second_urls = vec![
        "rels://b.example:28443".to_string(),
        "rel://c.example:28443".to_string(),
    ];
    mock.script(vec![
        sync_with_relay(1, first_urls, TOKEN_SENTINEL.into()),
        sync_with_relay(2, second_urls.clone(), TOKEN_SENTINEL.into()),
    ]);
    let addr = spawn_mock(mock.clone()).await;
    let (handle, host) = spawn_connector(addr, None);

    // both syncs processed in order; the advertised record reflects the
    // LATEST one
    wait_for(FUSE, || host.serials.lock().expect("serials").last() == Some(&2), "both syncs applied")
        .await;
    wait_for(FUSE, || handle.status().relay.advertised_urls == second_urls, "record follows latest sync")
        .await;

    let relay = handle.status().relay;
    assert!(relay.advertised);
    assert_eq!(relay.advertised_urls, second_urls, "second sync's set wins");
    // and the update never touched the runtime: still disabled, no client
    assert!(!relay.enabled);
    assert_eq!(relay.state, "disabled");
    assert!(relay.urls.is_empty());
    assert!(handle.relay_client().is_none());

    // across BOTH syncs (the first one advertised the reachable server!)
    // not a single relay connection was opened
    assert_stays_false(
        Duration::from_millis(500),
        || relay_server.stats().connections_accepted > 0,
        "advertisement updates must never dial while relay is disabled",
    )
    .await;
    assert_eq!(relay_server.stats().connections_accepted, 0);

    let json = handle.status_json();
    assert!(
        json.contains(&format!(
            "\"relay_advertised\":{{\"advertised\":true,\"advertised_urls\":\
             [\"rels://b.example:28443\",\"rel://c.example:28443\"]}}"
        )),
        "{json}"
    );
    assert!(!json.contains(TOKEN_SENTINEL), "token payload leaked into status: {json}");

    handle.stop();
    relay_server.shutdown().await;
}
