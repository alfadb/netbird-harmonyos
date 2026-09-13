// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N3-5 Connector lifecycle integration tests.
//!
//! An in-process tonic server implements the WHOLE management surface the
//! connector drives — `GetServerKey` + `Login` (sealed envelope,
//! grpc.go:585-637) + `Sync` (server-streaming, grpc.go:478-521) + `Logout`
//! (grpc.go:848-872) + `ExtendAuthSession` (grpc.go:662-699) — over
//! plaintext h2 on loopback, with the REAL message-body envelope in both
//! directions. The connector runs its REAL production code path
//! (`GrpcManagementFactory` + `SyncSession` + state machine) against it:
//!
//! 1. login success → 2 network-map updates (+1 outdated serial) → status,
//!    peer/route counts, WG registry and host-config applier contents
//!    (outdated snapshots ignored, engine.go:1572-1576)
//! 2. mid-stream break → automatic reconnect through the injected short
//!    backoff → the server continues on the new stream
//! 3. PermissionDenied on login → terminal `Failed` (`Auth` class), zero
//!    retries, no Sync attempt at all
//! 4. retry budget exhaustion via an injected failing
//!    [`netbird_core::connector::ManagementFactory`] → bounded attempts
//!    (exactly one), `Failed` with a sanitized class
//! 5. `stop()` idempotency + cleanup (WG registry cleared, host applier
//!    cleared, best-effort logout — including the logout-failure path)
//! 6. sentinel setup key / JWT / private key NEVER appear in
//!    `connector_status()` / `connector_stop()` JSON or `Debug` output
//!    (they DO reach the server — the mock captures them)
//! 7. session renewal fires only when a JWT exists and the anchored
//!    deadline is inside the injected lead window (upstream
//!    engine_authsession.go:83-107 semantics)

use core::time::Duration;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use base64::Engine as _;
use prost::Message as _;
use prost_types::Timestamp;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status};

use netbird_core::backoff::{Clock, ExponentialBackoff, Rng};
use netbird_core::config::Route;
use netbird_core::connector::{
    ConfigApplier, ConnectorHandle, ConnectorSecrets, ErrorClass, ManagementFactory,
    SyncPolicy, WgPeerApplier, WgPeerEntry, WgPeerRegistry,
};
use netbird_core::envelope::{self, EnvelopeKeyPair, EnvelopePublicKey};
use netbird_core::grpc::proto::management_service_server::{ManagementService, ManagementServiceServer};
use netbird_core::grpc::proto::{
    Empty, EncryptedMessage, ExtendAuthSessionRequest, ExtendAuthSessionResponse, LoginRequest,
    LoginResponse, ServerKeyResponse, SyncRequest, SyncResponse,
};
use netbird_core::management::ManagementError;
use netbird_core::state::ConnState;

// Host-test link surface (same as tests/management_grpc.rs / sync_stream.rs):
// the test binary links the whole crate rlib on the host triple, where
// libace_napi.z.so / libhilog_ndk.z.so do not exist. No-ops for the linker.
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
        _value: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _env: *mut c_void,
        _name: *const u8,
        _length: usize,
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
        _this: *mut c_void,
        _data: *mut c_void,
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
        _env: *mut c_void,
        _value: *mut c_void,
        _result: *mut i32,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(
        _env: *mut c_void,
        _value: *mut c_void,
        _result: *mut bool,
    ) -> i32 {
        0
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

const SENTINEL_SETUP_KEY: &str = "SENTINEL-SETUP-KEY-n3-5-NEVER-LOG-ME";
const SENTINEL_JWT: &str = "SENTINEL-JWT-n3-5.header.payload.SIG";
/// A private key whose BASE64 literally contains a sentinel substring: the
/// 32 bytes are the ASCII of the sentinel itself (32 chars → 32 bytes).
const SENTINEL_PRIVATE_KEY_TEXT: &str = "SENTINEL-PRIVATE-KEY-n3-5-012345";

fn sentinel_private_key_bytes() -> [u8; 32] {
    let mut key = [0u8; 32];
    key.copy_from_slice(SENTINEL_PRIVATE_KEY_TEXT.as_bytes());
    key
}

fn sentinel_private_key_b64() -> String {
    base64::engine::general_purpose::STANDARD.encode(sentinel_private_key_bytes())
}

/// Wait (bounded) until `cond()` holds; panics with `what` on deadline.
async fn wait_for<F: Fn() -> bool>(what: &'static str, cond: F) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !cond() {
        assert!(Instant::now() <= deadline, "deadline waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Deterministic reconnect policy (f=0 → the delay IS the current interval;
/// millisecond-scale waits; never exhausts unless `max_elapsed` is set).
fn deterministic_backoff(max_elapsed: Option<Duration>) -> ExponentialBackoff {
    ExponentialBackoff::new(
        Duration::from_millis(5),
        0.0,
        1.7,
        Duration::from_millis(100),
        max_elapsed,
    )
}

/// RNG that must never be sampled (the f=0 test policy draws nothing).
#[derive(Default)]
struct NoRng;

impl Rng for NoRng {
    fn next_uniform(&mut self) -> f64 {
        panic!("f=0 backoff must not consume randomness");
    }
}

#[derive(Default)]
struct RealClock;

impl Clock for RealClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Records every host-applied NetworkMap (serial + route count) and whether
/// `clear()` ran (connector stop).
#[derive(Default)]
struct RecordingHost {
    serials: Mutex<Vec<u64>>,
    route_counts: Mutex<Vec<usize>>,
    cleared: AtomicBool,
}

impl ConfigApplier for RecordingHost {
    fn apply(&self, map: &netbird_core::network_map::NetworkMap) {
        self.serials.lock().expect("serials").push(map.serial);
        self.route_counts.lock().expect("routes").push(map.routes.len());
    }
    fn clear(&self) {
        self.cleared.store(true, Ordering::Release);
    }
}

/// A WG seam that always fails — proves a data-plane failure is REPORTED
/// (status counters) but never kills the control-plane session.
struct FailingWgApplier;

impl WgPeerApplier for FailingWgApplier {
    fn apply_peers(&self, _peers: &[WgPeerEntry]) -> Result<(), String> {
        Err("wg device unavailable".into())
    }
    fn clear(&self) {}
}

/// Seam demo: a management factory that never connects (injected failure).
struct FailingFactory;

impl ManagementFactory for FailingFactory {
    fn connect(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<netbird_core::grpc::ManagementGrpcClient, ManagementError>> + Send + '_>,
    > {
        Box::pin(async { Err(ManagementError::Network("refused (injected)".into())) })
    }
}

// ---------------------------------------------------------------------------
// in-process management mock (GetServerKey + Login + Sync + Logout + Extend)
// ---------------------------------------------------------------------------

/// One scripted Sync connection: push these (sealed) SyncResponses, then
/// either hold the stream open or fail it with the given status.
enum SyncStep {
    UpdatesThen(Vec<SyncResponse>, Option<Status>),
}

#[derive(Clone)]
struct CapturedLogin {
    setup_key: String,
    jwt_token: String,
    meta_hostname: String,
}

#[derive(Clone)]
struct MgmtMock {
    keys: EnvelopeKeyPair,
    login_response: Arc<Mutex<Option<LoginResponse>>>,
    login_fail: Arc<Mutex<Option<Status>>>,
    /// EVERY Login RPC seen, including ones rejected by `login_fail` —
    /// this is what pins the retry-count upper bounds.
    login_attempts: Arc<Mutex<usize>>,
    login_calls: Arc<Mutex<Vec<CapturedLogin>>>,
    sync_script: Arc<Mutex<VecDeque<SyncStep>>>,
    sync_connects: Arc<Mutex<Vec<()>>>,
    logout_calls: Arc<Mutex<Vec<()>>>,
    logout_fail: Arc<Mutex<Option<Status>>>,
    extend_calls: Arc<Mutex<Vec<String>>>, // captured jwt tokens
}

impl Default for MgmtMock {
    fn default() -> Self {
        MgmtMock {
            keys: EnvelopeKeyPair::generate().expect("server key pair"),
            login_response: Arc::new(Mutex::new(None)),
            login_fail: Arc::new(Mutex::new(None)),
            login_attempts: Arc::new(Mutex::new(0)),
            login_calls: Arc::new(Mutex::new(Vec::new())),
            sync_script: Arc::new(Mutex::new(VecDeque::new())),
            sync_connects: Arc::new(Mutex::new(Vec::new())),
            logout_calls: Arc::new(Mutex::new(Vec::new())),
            logout_fail: Arc::new(Mutex::new(None)),
            extend_calls: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

fn decrypt_body(server: &EnvelopeKeyPair, env: &EncryptedMessage) -> Result<Vec<u8>, Status> {
    let client_pk = EnvelopePublicKey::from_base64(&env.wg_pub_key)
        .map_err(|_| Status::invalid_argument("envelope wgPubKey is not a NaCl key"))?;
    envelope::open(&client_pk, server, &env.body)
        .map_err(|_| Status::internal("cannot decrypt request body"))
}

#[tonic::async_trait]
impl ManagementService for MgmtMock {
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
        // count the attempt BEFORE any rejection so retry bounds are
        // observable even for failed logins
        *self.login_attempts.lock().expect("attempts") += 1;
        if let Some(status) = self.login_fail.lock().expect("login fail").clone() {
            return Err(status);
        }
        let env = request.into_inner();
        let plaintext = decrypt_body(&self.keys, &env)?;
        let req = LoginRequest::decode(plaintext.as_slice())
            .map_err(|e| Status::invalid_argument(format!("body is not LoginRequest: {e}")))?;
        self.login_calls.lock().expect("login calls").push(CapturedLogin {
            setup_key: req.setup_key.clone(),
            jwt_token: req.jwt_token.clone(),
            meta_hostname: req.meta.as_ref().map(|m| m.hostname.clone()).unwrap_or_default(),
        });
        let response = self
            .login_response
            .lock()
            .expect("login response")
            .clone()
            .unwrap_or_default();
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
        self.sync_connects.lock().expect("sync connects").push(());

        let step = self.sync_script.lock().expect("script").pop_front();
        let (updates, fail_after, hold) = match step {
            None => (Vec::new(), None, true),
            Some(SyncStep::UpdatesThen(updates, fail)) => {
                let hold = fail.is_none();
                (updates, fail, hold)
            }
        };
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<EncryptedMessage, Status>>(16);
        for resp in updates {
            let sealed = envelope::seal(
                &EnvelopePublicKey::from_base64(&env.wg_pub_key).expect("client pk"),
                &self.keys,
                &resp.encode_to_vec(),
            )
            .expect("seal sync frame");
            tx.send(Ok(EncryptedMessage {
                wg_pub_key: env.wg_pub_key.clone(),
                body: sealed,
                version: 0,
            }))
            .await
            .expect("receiver alive while scripting");
        }
        if let Some(status) = fail_after {
            tx.send(Err(status)).await.expect("receiver alive for failure");
        }
        if hold {
            tokio::spawn(async move {
                let _tx = tx;
                std::future::pending::<()>().await;
            });
        }
        Ok(Response::new(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))))
    }

    async fn logout(&self, request: Request<EncryptedMessage>) -> Result<Response<Empty>, Status> {
        let env = request.into_inner();
        let plaintext = decrypt_body(&self.keys, &env)?;
        assert_eq!(plaintext.len(), 0, "logout body decrypts to the empty proto::Empty");
        self.logout_calls.lock().expect("logout calls").push(());
        if let Some(status) = self.logout_fail.lock().expect("logout fail").clone() {
            return Err(status);
        }
        Ok(Response::new(Empty {}))
    }

    async fn extend_auth_session(
        &self,
        request: Request<EncryptedMessage>,
    ) -> Result<Response<EncryptedMessage>, Status> {
        let env = request.into_inner();
        let plaintext = decrypt_body(&self.keys, &env)?;
        let req = ExtendAuthSessionRequest::decode(plaintext.as_slice())
            .map_err(|e| Status::invalid_argument(format!("body: {e}")))?;
        self.extend_calls.lock().expect("extend calls").push(req.jwt_token.clone());
        let response = ExtendAuthSessionResponse {
            // far-future deadline (year 2030) — well outside any test lead
            session_expires_at: Some(Timestamp { seconds: 1_893_456_000, nanos: 0 }),
        };
        let client_pk = EnvelopePublicKey::from_base64(&env.wg_pub_key).expect("client pk");
        let sealed = envelope::seal(&client_pk, &self.keys, &response.encode_to_vec())
            .expect("seal extend reply");
        Ok(Response::new(EncryptedMessage { wg_pub_key: env.wg_pub_key, body: sealed, version: 0 }))
    }
}

async fn spawn_mock(svc: MgmtMock) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind loopback");
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
// SyncResponse builders
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

fn snapshot(serial: u64, peers: usize, routes: usize, deadline: Option<i64>) -> SyncResponse {
    SyncResponse {
        network_map: Some(netbird_core::grpc::proto::NetworkMap {
            serial,
            peer_config: Some(netbird_core::grpc::proto::PeerConfig {
                address: "10.64.0.9".into(),
                ..Default::default()
            }),
            remote_peers: (0..peers)
                .map(|i| mgmt_peer(&format!("UEVFUjBBM{0:0>2}", i), &format!("10.30.30.{i}")))
                .collect(),
            remote_peers_is_empty: false,
            routes: (0..routes)
                .map(|i| netbird_core::grpc::proto::Route {
                    id: format!("r-{serial}-{i}"),
                    network: format!("172.16.{i}.0/24"),
                    network_type: 1,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }),
        session_expires_at: deadline.map(|s| Timestamp { seconds: s, nanos: 0 }),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// connector construction helper
// ---------------------------------------------------------------------------

fn config_json_for(addr: std::net::SocketAddr) -> String {
    format!(
        "{{\"management_url\":\"http://{addr}\",\"private_key\":\"{}\",\
          \"hostname\":\"ohos-connector\",\"allow_unprotected_management\":true}}",
        sentinel_private_key_b64()
    )
}

/// Spawn a connector against the mock listening on `addr` with a
/// deterministic sync policy.
#[allow(clippy::too_many_arguments)]
fn spawn_connector(
    addr: std::net::SocketAddr,
    secrets: ConnectorSecrets,
    renew_lead: Duration,
    renew_interval: Duration,
    max_elapsed: Option<Duration>,
    wg: Arc<dyn WgPeerApplier>,
    host: Arc<dyn ConfigApplier>,
    factory: Option<Arc<dyn ManagementFactory>>,
) -> Arc<ConnectorHandle> {
    ConnectorHandle::spawn(
        tokio::runtime::Handle::current(),
        factory.unwrap_or_else(|| {
            Arc::new(netbird_core::connector::GrpcManagementFactory::new(
                &netbird_core::connector::ConnectorConfig::from_json(&config_json_for(addr))
                    .expect("test config"),
                EnvelopeKeyPair::from_secret_bytes(&sentinel_private_key_bytes()),
            ))
        }),
        wg,
        host,
        secrets,
        netbird_core::grpc::PeerMeta {
            hostname: "ohos-connector".into(),
            os_name: "harmonyos".into(),
            os_version: "5.0.0".into(),
            netbird_version: "0.1.0".into(),
        },
        deterministic_backoff(max_elapsed),
        SyncPolicy {
            backoff: deterministic_backoff(max_elapsed),
            rng: Box::new(NoRng),
            clock: Box::new(RealClock),
        },
        renew_lead,
        renew_interval,
        false, // N3-7 default-route gate: no force opt-in
        None,  // N3-7: no protected-socket source in the generic helper
        None,  // N5c: build the production (idle, signal-less) ICE orchestrator
        None,  // N5d: no signal material in the generic helper
    )
}

fn login_response(deadline: Option<i64>) -> LoginResponse {
    LoginResponse {
        netbird_config: None,
        peer_config: Some(netbird_core::grpc::proto::PeerConfig {
            address: "10.64.0.9".into(),
            ..Default::default()
        }),
        checks: vec![],
        session_expires_at: deadline.map(|s| Timestamp { seconds: s, nanos: 0 }),
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

/// Login success → 2 network-map updates (+1 OUTDATED serial in between):
/// status counts, WG registry contents and the host-config applier reflect
/// exactly the latest applied snapshot; the outdated one is ignored
/// (engine.go:1572-1576).
#[tokio::test(flavor = "multi_thread")]
async fn lifecycle_login_updates_apply_to_data_plane() {
    let mock = MgmtMock::default();
    *mock.login_response.lock().expect("lr") = Some(login_response(Some(1_700_000_000)));
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(vec![
        snapshot(5, 2, 1, Some(1_700_000_000)),
        snapshot(4, 1, 9, None), // OUTDATED serial — must be ignored
        snapshot(6, 2, 2, None),
    ], None));
    let addr = spawn_mock(mock.clone()).await;

    let wg = Arc::new(WgPeerRegistry::new());
    let host = Arc::new(RecordingHost::default());
    let handle = spawn_connector(
        addr,
        ConnectorSecrets { setup_key: "SETUP-KEY-OK".into(), jwt: String::new() },
        Duration::from_secs(600),
        Duration::from_secs(30),
        None,
        wg.clone(),
        host.clone(),
        None,
    );

    wait_for("serial 6 applied to the host applier", || {
        host.serials.lock().expect("serials").last() == Some(&6)
    })
    .await;

    // state machine + counters
    let status = handle.status();
    assert_eq!(status.state, ConnState::Connected, "login + stream = Connected");
    assert!(status.running);
    assert_eq!(status.peer_count, 2);
    assert_eq!(status.route_count, 2);
    assert_eq!(status.last_error, None);
    assert_eq!(status.deadline, netbird_core::connector::SessionDeadline::At(1_700_000_000));
    assert_eq!(status.reconnects, 0);
    // N3-7 gap 3: a healthy (even just connecting) connector is NOT terminal
    assert!(!status.terminal, "running connector is not terminal");

    // host applier saw serials 5 and 6 — the outdated 4 was skipped
    assert_eq!(host.serials.lock().expect("serials").clone(), vec![5, 6]);
    assert_eq!(host.route_counts.lock().expect("routes").clone(), vec![1, 2]);

    // WG registry holds the LATEST snapshot's peer set (full replace)
    let peers = wg.snapshot();
    assert_eq!(peers.len(), 2);
    assert_eq!(peers[0].pub_key_b64, format!("UEVFUjBBM{0:0>2}", 0));
    assert_eq!(
        peers[0].allowed_ips,
        vec![Route { addr: [10, 30, 30, 0], prefix_len: 32 }]
    );

    // login captured once with the right identity meta
    let logins = mock.login_calls.lock().expect("logins");
    assert_eq!(logins.len(), 1);
    assert_eq!(logins[0].meta_hostname, "ohos-connector");
    assert_eq!(logins[0].setup_key, "SETUP-KEY-OK");
    drop(logins);
    assert_eq!(mock.sync_connects.lock().expect("sync").len(), 1);

    // status JSON contract
    let json = handle.status_json();
    for fragment in [
        "\"running\":true",
        "\"state\":\"connected\"",
        "\"peer_count\":2",
        "\"route_count\":2",
        "\"reconnects\":0",
        "\"last_error\":null",
        "\"session_expiry\":\"set\"",
        "\"session_expires_at_unix\":1700000000",
    ] {
        assert!(json.contains(fragment), "status JSON missing {fragment}: {json}");
    }

    // N3-6: the shell network-config snapshot reflects the LATEST applied
    // map (serial 6), not the skipped outdated one (serial 4)
    let netcfg = handle.network_config_json();
    for fragment in [
        "\"available\":true",
        "\"serial\":6",
        "\"address\":\"10.64.0.9\"",
        "\"address_prefix_len\":32",
        "\"network\":\"172.16.1.0/24\"",
        "\"is_default\":false",
        "\"peer_count\":2",
        "\"allowed_ips\":1",
    ] {
        assert!(netcfg.contains(fragment), "network config missing {fragment}: {netcfg}");
    }
    assert!(netcfg.starts_with("{\"available\":true,"), "{netcfg}");

    handle.stop();
    // stop tears the applied shell config down with the rest of the seams
    assert_eq!(
        handle.network_config_json(),
        "{\"available\":false,\"reason\":\"no-network-map\"}"
    );
}

/// Mid-stream break → automatic reconnect through the injected backoff →
/// the server continues pushing on the NEW stream; the state machine comes
/// back to Connected and the break is counted.
#[tokio::test(flavor = "multi_thread")]
async fn stream_break_reconnects_and_continues() {
    let mock = MgmtMock::default();
    // 1st connection: one update, then the stream dies
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![snapshot(5, 1, 1, None)],
        Some(Status::internal("h2 stream reset")),
    ));
    // 2nd connection: the server continues and holds
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![snapshot(6, 1, 3, None)],
        None,
    ));
    let addr = spawn_mock(mock.clone()).await;

    let wg = Arc::new(WgPeerRegistry::new());
    let host = Arc::new(RecordingHost::default());
    let handle = spawn_connector(
        addr,
        ConnectorSecrets { setup_key: "SETUP-KEY-OK".into(), jwt: String::new() },
        Duration::from_secs(600),
        Duration::from_secs(30),
        None,
        wg.clone(),
        host.clone(),
        None,
    );

    wait_for("serial 6 applied after the reconnect", || {
        host.serials.lock().expect("serials").last() == Some(&6)
    })
    .await;

    let status = handle.status();
    assert_eq!(status.state, ConnState::Connected, "re-established after the break");
    assert_eq!(status.reconnects, 1, "exactly one counted stream break");
    assert_eq!(status.route_count, 3, "post-reconnect snapshot applied");
    assert_eq!(host.serials.lock().expect("serials").clone(), vec![5, 6]);
    assert_eq!(mock.sync_connects.lock().expect("sync").len(), 2, "initial + reconnect");

    handle.stop();
}

/// PermissionDenied on login is TERMINAL (upstream backoff.Permanent):
/// zero retries, the Sync RPC is never attempted, and the state machine
/// parks in Failed with the sanitized Auth class.
#[tokio::test(flavor = "multi_thread")]
async fn permission_denied_on_login_is_terminal() {
    let mock = MgmtMock::default();
    *mock.login_fail.lock().expect("fail") =
        Some(Status::permission_denied("invalid setup key"));
    let addr = spawn_mock(mock.clone()).await;

    let handle = spawn_connector(
        addr,
        ConnectorSecrets { setup_key: SENTINEL_SETUP_KEY.into(), jwt: String::new() },
        Duration::from_secs(600),
        Duration::from_millis(10),
        Some(Duration::from_secs(60)), // a REAL budget: retries would be visible
        Arc::new(WgPeerRegistry::new()),
        Arc::new(RecordingHost::default()),
        None,
    );

    wait_for("terminal Failed state", || handle.status().state == ConnState::Failed).await;

    let status = handle.status();
    assert_eq!(status.state, ConnState::Failed);
    assert!(!status.running, "worker exited");
    // N3-7 gap 3: a self-terminated worker IS terminal — the field the
    // shell watcher tears the VPN down on
    assert!(status.terminal, "fatal login is terminal");
    assert!(handle.status_json().contains("\"terminal\":true"));
    assert_eq!(
        status.last_error,
        Some(ErrorClass::Auth { status: 7 }),
        "PermissionDenied → Auth class (status 7)"
    );
    assert_eq!(status.reconnects, 0, "fatal: no retryable failures counted");

    // bounded: exactly ONE login attempt, no retry, no sync at all
    wait_for("login attempt recorded", || {
        *mock.login_attempts.lock().expect("attempts") == 1
    })
    .await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        *mock.login_attempts.lock().expect("attempts"),
        1,
        "no second login attempt after PermissionDenied"
    );
    assert!(
        mock.sync_connects.lock().expect("sync").is_empty(),
        "the Sync RPC is never attempted after a fatal login"
    );

    handle.stop();
}

/// Retry-budget exhaustion through an INJECTED failing management factory
/// (the ManagementFactory seam): attempts are bounded (here: exactly one —
/// the budget is spent after the first failure), the state machine parks in
/// Failed with a sanitized class, and the error carries NO injected message.
#[tokio::test(flavor = "multi_thread")]
async fn retry_budget_exhaustion_is_bounded_and_sanitized() {
    let handle = spawn_connector(
        std::net::SocketAddr::from(([127, 0, 0, 1], 1)),
        ConnectorSecrets { setup_key: "SETUP-KEY-X".into(), jwt: String::new() },
        Duration::from_secs(600),
        Duration::from_millis(10),
        Some(Duration::ZERO), // spent immediately → exactly one attempt
        Arc::new(WgPeerRegistry::new()),
        Arc::new(RecordingHost::default()),
        Some(Arc::new(FailingFactory)),
    );

    wait_for("terminal Failed state", || handle.status().state == ConnState::Failed).await;
    let status = handle.status();
    assert_eq!(status.state, ConnState::Failed);
    // N3-7 gap 3: budget exhaustion is also a SELF-terminated worker
    assert!(status.terminal, "retry exhaustion is terminal");
    assert!(handle.status_json().contains("\"terminal\":true"));
    assert_eq!(
        status.last_error,
        Some(ErrorClass::Network),
        "sanitized Network class"
    );
    assert_eq!(status.reconnects, 1, "exactly one failed attempt, budget spent");

    // the injected (non-secret) message must NOT reach the status JSON —
    // only the class crosses the boundary
    let json = handle.status_json();
    assert!(json.contains("\"last_error\":{\"class\":\"network\",\"status\":0}"), "{json}");
    assert!(!json.contains("refused"), "error message leaked into status: {json}");
    assert!(!json.contains("injected"), "error message leaked into status: {json}");

    handle.stop();
}

/// stop() closes the stream, clears the data-plane seams and logs out
/// (best-effort). A second stop() is a no-op. The logout-FAILURE path does
/// not block the stop either.
#[tokio::test(flavor = "multi_thread")]
async fn stop_is_idempotent_and_cleans_up_even_when_logout_fails() {
    for logout_fail_status in [None, Some(Status::internal("logout boom"))] {
        let mock = MgmtMock::default();
        *mock.logout_fail.lock().expect("fail") = logout_fail_status.clone();
        mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
            vec![snapshot(5, 2, 1, None)],
            None,
        ));
        let addr = spawn_mock(mock.clone()).await;

        let wg = Arc::new(WgPeerRegistry::new());
        let host = Arc::new(RecordingHost::default());
        let handle = spawn_connector(
            addr,
            ConnectorSecrets { setup_key: "SETUP-KEY-OK".into(), jwt: String::new() },
            Duration::from_secs(600),
            Duration::from_secs(30),
            None,
            wg.clone(),
            host.clone(),
            None,
        );

        wait_for("snapshot applied", || handle.status().peer_count == 2).await;

        let json = handle.stop_json();
        assert!(json.contains("\"ok\":true"), "{json}");
        assert!(json.contains("\"already_stopped\":false"), "{json}");
        assert!(json.contains("\"state\":\"disconnected\""), "{json}");

        let status = handle.status();
        assert_eq!(status.state, ConnState::Disconnected);
        assert!(!status.running);
        // N3-7 gap 3: a USER stop is explicitly NOT terminal (the watcher
        // must not treat it as a connector death)
        assert!(!status.terminal, "user stop is not terminal");
        assert!(handle.status_json().contains("\"terminal\":false"));
        assert!(host.cleared.load(Ordering::Acquire), "host applier cleared");
        wait_for("wg registry cleared", || wg.is_empty()).await;

        // logout fired (exactly once), success or failure must not block
        wait_for("logout called once", || {
            mock.logout_calls.lock().expect("logout").len() == 1
        })
        .await;
        match logout_fail_status {
            None => {
                wait_for("logout_ok=true", || handle.status().logout_ok == Some(true)).await;
            }
            Some(_) => {
                wait_for("logout_ok=false recorded, stop unaffected", || {
                    handle.status().logout_ok == Some(false)
                })
                .await;
            }
        }

        // idempotent second stop
        let json2 = handle.stop_json();
        assert!(json2.contains("\"already_stopped\":true"), "{json2}");
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            mock.logout_calls.lock().expect("logout").len(),
            1,
            "no second logout"
        );
    }
}

/// A failing WG applier is REPORTED (status counters) but never kills the
/// control-plane session.
#[tokio::test(flavor = "multi_thread")]
async fn failing_wg_applier_is_reported_not_fatal() {
    let mock = MgmtMock::default();
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![snapshot(5, 2, 1, None), snapshot(6, 1, 1, None)],
        None,
    ));
    let addr = spawn_mock(mock).await;

    let host = Arc::new(RecordingHost::default());
    let handle = spawn_connector(
        addr,
        ConnectorSecrets { setup_key: "SETUP-KEY-OK".into(), jwt: String::new() },
        Duration::from_secs(600),
        Duration::from_secs(30),
        None,
        Arc::new(FailingWgApplier),
        host.clone(),
        None,
    );

    wait_for("both updates applied to the host applier", || {
        host.serials.lock().expect("serials").len() == 2
    })
    .await;

    let status = handle.status();
    assert_eq!(status.state, ConnState::Connected, "control plane unaffected");
    assert!(status.wg_apply_failed);
    assert_eq!(status.wg_apply_errors, 2, "one counter increment per snapshot");

    handle.stop();
}

/// Sentinel setup key / JWT / private key reach the server (captured by the
/// mock) but NEVER appear in connector_status() / connector_stop() JSON or
/// in Debug output — including after a FAILED login (an error is recorded)
/// and after a successful session + stop.
#[tokio::test(flavor = "multi_thread")]
async fn secrets_never_leak_into_status_errors_or_debug() {
    let key_b64 = sentinel_private_key_b64();
    let sentinels: Vec<String> = vec![
        SENTINEL_SETUP_KEY.to_string(),
        SENTINEL_JWT.to_string(),
        key_b64.clone(),
        SENTINEL_PRIVATE_KEY_TEXT.to_string(),
    ];

    // ---- scenario A: failed login (PermissionDenied) → error recorded ----
    let mock = MgmtMock::default();
    *mock.login_fail.lock().expect("fail") =
        Some(Status::permission_denied("invalid setup key"));
    let addr = spawn_mock(mock.clone()).await;
    let handle = spawn_connector(
        addr,
        ConnectorSecrets { setup_key: SENTINEL_SETUP_KEY.into(), jwt: SENTINEL_JWT.into() },
        Duration::from_secs(600),
        Duration::from_millis(10),
        Some(Duration::ZERO),
        Arc::new(WgPeerRegistry::new()),
        Arc::new(RecordingHost::default()),
        None,
    );
    wait_for("failed state", || handle.status().state == ConnState::Failed).await;

    // the sentinel setup key really reached the server (the scan below is
    // therefore meaningful — the secret was in play). The login FAILED
    // (PermissionDenied), so the capture comes from the attempts counter
    // side: assert the request was seen, then flip the mock and log in
    // successfully to prove the sentinels cross the real path.
    assert_eq!(*mock.login_attempts.lock().expect("attempts"), 1);

    // capture the sentinel values through a SUCCESSFUL login: reuse the
    // same mock, clear the failure, restart a connector on it
    *mock.login_fail.lock().expect("fail") = None;
    *mock.login_response.lock().expect("lr") = Some(login_response(Some(1_700_000_000)));
    let handle2 = spawn_connector(
        addr,
        ConnectorSecrets { setup_key: SENTINEL_SETUP_KEY.into(), jwt: SENTINEL_JWT.into() },
        Duration::from_secs(600),
        Duration::from_millis(10),
        None,
        Arc::new(WgPeerRegistry::new()),
        Arc::new(RecordingHost::default()),
        None,
    );
    wait_for("successful login captured", || {
        !mock.login_calls.lock().expect("logins").is_empty()
    })
    .await;
    {
        let logins = mock.login_calls.lock().expect("logins");
        assert_eq!(logins[logins.len() - 1].setup_key, SENTINEL_SETUP_KEY);
        assert_eq!(logins[logins.len() - 1].jwt_token, SENTINEL_JWT);
        assert_eq!(logins[logins.len() - 1].meta_hostname, "ohos-connector");
    }
    handle2.stop();
    handle.stop();

    let status = handle.status_json();
    let debug = format!("{:?}", handle);
    let status_struct_debug = format!("{:?}", handle.status());
    for s in &sentinels {
        assert!(!status.contains(s.as_str()), "secret leaked into status JSON: {s}");
        assert!(!debug.contains(s.as_str()), "secret leaked into Debug: {s}");
        assert!(
            !status_struct_debug.contains(s.as_str()),
            "secret leaked into status Debug: {s}"
        );
    }
    // the error surface is class-only
    assert!(status.contains("\"last_error\":{\"class\":\"auth\",\"status\":7}"), "{status}");
    handle.stop();

    // ---- scenario B: successful login + updates + stop ----
    let mock = MgmtMock::default();
    *mock.login_response.lock().expect("lr") = Some(login_response(Some(1_700_000_000)));
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![snapshot(5, 1, 1, None)],
        None,
    ));
    let addr = spawn_mock(mock.clone()).await;
    let wg = Arc::new(WgPeerRegistry::new());
    let handle = spawn_connector(
        addr,
        ConnectorSecrets { setup_key: SENTINEL_SETUP_KEY.into(), jwt: SENTINEL_JWT.into() },
        Duration::from_secs(600),
        Duration::from_millis(10),
        None,
        wg.clone(),
        Arc::new(RecordingHost::default()),
        None,
    );
    wait_for("snapshot applied", || handle.status().peer_count == 1).await;

    let status = handle.status_json();
    let stop = handle.stop_json();
    wait_for("logout fired", || {
        mock.logout_calls.lock().expect("logout").len() == 1
    })
    .await;
    let status_after_stop = handle.status_json();
    let debug = format!("{:?}", handle);
    let registry_debug = format!("{:?}", wg.snapshot());
    for s in &sentinels {
        assert!(!status.contains(s.as_str()), "secret leaked into status JSON: {s}");
        assert!(!stop.contains(s.as_str()), "secret leaked into stop JSON: {s}");
        assert!(
            !status_after_stop.contains(s.as_str()),
            "secret leaked into status JSON: {s}"
        );
        assert!(!debug.contains(s.as_str()), "secret leaked into Debug: {s}");
        assert!(!registry_debug.contains(s.as_str()), "secret leaked into registry Debug: {s}");
    }
}

/// Session renewal: with a JWT present and the anchored deadline inside the
/// injected lead window, the connector calls ExtendAuthSession and applies
/// the new (3-state) deadline; with a setup-key-only session it never
/// extends (upstream empty-JWT guard, engine_authsession.go:84-86).
#[tokio::test(flavor = "multi_thread")]
async fn session_renewal_fires_only_when_jwt_and_near_expiry() {
    let soon = unix_now() + 2; // expires in 2s — inside the 60s lead
    let mock = MgmtMock::default();
    *mock.login_response.lock().expect("lr") = Some(login_response(Some(soon)));
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![snapshot(5, 1, 0, None)],
        None,
    ));
    let addr = spawn_mock(mock.clone()).await;
    let handle = spawn_connector(
        addr,
        ConnectorSecrets { setup_key: String::new(), jwt: SENTINEL_JWT.into() },
        Duration::from_secs(60),  // lead: renewal triggers immediately
        Duration::from_millis(10), // check every 10ms
        None,
        Arc::new(WgPeerRegistry::new()),
        Arc::new(RecordingHost::default()),
        None,
    );
    wait_for("snapshot applied", || handle.status().peer_count == 1).await;

    wait_for("renewal fired with the injected jwt", || {
        mock.extend_calls.lock().expect("extend").len() == 1
    })
    .await;
    assert_eq!(mock.extend_calls.lock().expect("extend")[0], SENTINEL_JWT);

    // the new absolute deadline is applied and reported
    wait_for("renewed deadline reported", || {
        handle
            .status_json()
            .contains("\"session_expires_at_unix\":1893456000")
    })
    .await;

    // the renewed deadline is far outside the lead → no further extensions
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        mock.extend_calls.lock().expect("extend").len(),
        1,
        "renewal must not loop once the deadline moved out"
    );
    handle.stop();

    // ---- negative: setup-key-only → no JWT → never extends ----
    let mock = MgmtMock::default();
    *mock.login_response.lock().expect("lr") = Some(login_response(Some(soon)));
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![snapshot(5, 1, 0, None)],
        None,
    ));
    let addr = spawn_mock(mock.clone()).await;
    let handle = spawn_connector(
        addr,
        ConnectorSecrets { setup_key: "SETUP-KEY-ONLY".into(), jwt: String::new() },
        Duration::from_secs(3600),
        Duration::from_millis(10),
        None,
        Arc::new(WgPeerRegistry::new()),
        Arc::new(RecordingHost::default()),
        None,
    );
    wait_for("snapshot applied", || handle.status().peer_count == 1).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        mock.extend_calls.lock().expect("extend").is_empty(),
        "a setup-key session has no JWT to extend (engine_authsession.go:84-86)"
    );
    handle.stop();
}

/// The NAPI-level JSON entry points (global singleton): rejection reasons,
/// the running guard, and the stop → restart path. This is the ONLY test
/// touching the global connector slot.
#[tokio::test(flavor = "multi_thread")]
async fn napi_global_start_status_stop_roundtrip() {
    use netbird_core::connector::{connector_start_json, connector_status_json, connector_stop_json};

    // invalid credentials are rejected with a stable reason
    let json = connector_start_json(&config_json_for(std::net::SocketAddr::from(([127, 0, 0, 1], 1))), "{}");
    assert!(json.contains("\"started\":false"), "{json}");
    assert!(json.contains("invalid-credentials"), "{json}");

    // invalid config rejected before credentials are even parsed
    let json = connector_start_json("not json", "{\"setup_key\":\"k\"}");
    assert!(json.contains("invalid-config"), "{json}");

    // N3-7 fail-closed: WITHOUT the explicit opt-in, an unprotected start is
    // refused even when the config itself is valid
    let no_opt_in = format!(
        "{{\"management_url\":\"http://127.0.0.1:1\",\"private_key\":\"{}\",\
          \"hostname\":\"ohos-connector\"}}",
        sentinel_private_key_b64()
    );
    let json = connector_start_json(&no_opt_in, "{\"setup_key\":\"SETUP-GLOBAL\"}");
    assert!(json.contains("\"started\":false"), "{json}");
    assert!(json.contains("management-socket-required"), "{json}");

    // a real start (endpoint unreachable → the worker retries in the
    // background; running stays true)
    let json = connector_start_json(
        &config_json_for(std::net::SocketAddr::from(([127, 0, 0, 1], 1))),
        "{\"setup_key\":\"SETUP-GLOBAL\"}",
    );
    assert!(json.contains("\"started\":true"), "{json}");
    assert!(json.contains("\"state\":\"connecting\""), "{json}");

    let status = connector_status_json();
    assert!(status.contains("\"running\":true"), "{status}");

    // a second start while running is rejected
    let json = connector_start_json(
        &config_json_for(std::net::SocketAddr::from(([127, 0, 0, 1], 1))),
        "{\"setup_key\":\"SETUP-GLOBAL-2\"}",
    );
    assert!(json.contains("already-running"), "{json}");

    // stop → free slot; status falls back to the disconnected snapshot
    let json = connector_stop_json();
    assert!(json.contains("\"ok\":true"), "{json}");
    assert!(json.contains("\"already_stopped\":false"), "{json}");
    let status = connector_status_json();
    assert!(status.contains("\"running\":false"), "{status}");
    assert!(status.contains("\"state\":\"disconnected\""), "{status}");

    // stopping with no connector at all is fine (idempotent)
    let json = connector_stop_json();
    assert!(json.contains("\"already_stopped\":true"), "{json}");
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
