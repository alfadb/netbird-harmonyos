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
//!    cleared) — and NO management Logout on any stop path; the EXPLICIT
//!    `logout_json()` is the only Logout caller (fires exactly once, one
//!    shot; NotFound = "peer already gone", upstream logoutPeerGone)
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
/// HANG GUARD, not a latency assertion: the awaited events happen in
/// microseconds over loopback; the fuse is generous (60s) so a fully loaded
/// parallel `cargo test` run never trips it spuriously.
async fn wait_for<F: Fn() -> bool>(what: &'static str, cond: F) {
    let deadline = Instant::now() + Duration::from_secs(60);
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
    /// 2026-10-11 A1: optional SCRIPTED verdict per Login call, popped in
    /// order — entry 1 answers the account-mode JWT-less attempt, entry 2 the
    /// one allowed JWT retry. Empty (every pre-existing test) = the
    /// single-verdict knobs below. Scripted calls are decoded and captured
    /// BEFORE their verdict, so a REJECTED attempt's wire shape is assertable
    /// (that is exactly what the sequencing tests pin).
    login_script: Arc<Mutex<VecDeque<Result<LoginResponse, Status>>>>,
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
            login_script: Arc::new(Mutex::new(VecDeque::new())),
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

impl MgmtMock {
    /// Decode+capture ONE Login body (wire fields only). Never echoes a body.
    fn capture_login(&self, env: &EncryptedMessage) -> Result<CapturedLogin, Status> {
        let plaintext = decrypt_body(&self.keys, env)?;
        let req = LoginRequest::decode(plaintext.as_slice())
            .map_err(|e| Status::invalid_argument(format!("body is not LoginRequest: {e}")))?;
        Ok(CapturedLogin {
            setup_key: req.setup_key.clone(),
            jwt_token: req.jwt_token.clone(),
            meta_hostname: req.meta.as_ref().map(|m| m.hostname.clone()).unwrap_or_default(),
        })
    }

    /// Seal `response` for the requesting client identity.
    fn seal_login_reply(
        &self,
        env: EncryptedMessage,
        response: LoginResponse,
    ) -> Result<Response<EncryptedMessage>, Status> {
        let client_pk = EnvelopePublicKey::from_base64(&env.wg_pub_key).expect("client pk");
        let sealed = envelope::seal(&client_pk, &self.keys, &response.encode_to_vec())
            .expect("seal login reply");
        Ok(Response::new(EncryptedMessage { wg_pub_key: env.wg_pub_key, body: sealed, version: 0 }))
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
        let env = request.into_inner();
        // 2026-10-11 A1: a scripted verdict, if any, answers THIS call — the
        // account-mode two-step drives the 1st against `no-jwt` and the 2nd
        // against the stored JWT. Captured before the verdict on purpose.
        if let Some(step) = self.login_script.lock().expect("login script").pop_front() {
            let captured = self.capture_login(&env)?;
            self.login_calls.lock().expect("login calls").push(captured);
            return match step {
                Err(status) => Err(status),
                Ok(response) => self.seal_login_reply(env, response),
            };
        }
        if let Some(status) = self.login_fail.lock().expect("login fail").clone() {
            return Err(status);
        }
        let captured = self.capture_login(&env)?;
        self.login_calls.lock().expect("login calls").push(captured);
        let response = self
            .login_response
            .lock()
            .expect("login response")
            .clone()
            .unwrap_or_default();
        self.seal_login_reply(env, response)
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
        false, // N3-7 default-route gate: no force opt-in
        None,  // N3-7: no protected-socket source in the generic helper
        None,  // N5c: build the production (idle, signal-less) ICE orchestrator
        None,  // N5d: no signal material in the generic helper
        None,  // N7: no shell-fed WG device seam in the generic helper
        netbird_core::connector::HostIceTuning::default(), // N12a HOST-ONLY: defaults
    )
}

/// A plaintext management client for the login-action tests (the login action
/// itself dials from its own connector config; the mock speaks plaintext h2 on
/// loopback, so the tests drive the SHARED `login_action` step over the same
/// seam the connector uses).
async fn plaintext_client(
    addr: std::net::SocketAddr,
    keys: EnvelopeKeyPair,
) -> netbird_core::grpc::ManagementGrpcClient {
    netbird_core::grpc::ManagementGrpcClient::connect(
        &format!("http://{addr}"),
        netbird_core::grpc::GrpcTransport::Plaintext,
        Duration::from_secs(5),
        Duration::from_secs(5),
        keys,
    )
    .await
    .expect("plaintext connect")
}

fn login_action_meta() -> netbird_core::grpc::PeerMeta {
    netbird_core::grpc::PeerMeta {
        hostname: "ohos-connector".into(),
        os_name: "harmonyos".into(),
        os_version: "5.0.0".into(),
        netbird_version: "0.1.0".into(),
    }
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
        ConnectorSecrets { setup_key: "SETUP-KEY-OK".into() },
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

    // login captured once (the key-less Login: this peer is already
    // registered, so the setup key never needs to leave the device) with the
    // right identity meta
    let logins = mock.login_calls.lock().expect("logins");
    assert_eq!(logins.len(), 1);
    assert_eq!(logins[0].meta_hostname, "ohos-connector");
    assert_eq!(logins[0].setup_key, "");
    assert_eq!(logins[0].jwt_token, "");
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
        ConnectorSecrets { setup_key: "SETUP-KEY-OK".into() },
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
/// parks in Failed with the sanitized Auth class. 2026-10-12: setup-key mode
/// runs upstream's TWO-STEP shape, so the key-less Login AND the setup-key
/// `Register` are both refused here — two RPCs, one attempt each, no retry.
#[tokio::test(flavor = "multi_thread")]
async fn permission_denied_on_login_is_terminal() {
    let mock = MgmtMock::default();
    *mock.login_fail.lock().expect("fail") =
        Some(Status::permission_denied("invalid setup key"));
    let addr = spawn_mock(mock.clone()).await;

    let handle = spawn_connector(
        addr,
        ConnectorSecrets { setup_key: SENTINEL_SETUP_KEY.into() },
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

    // bounded: exactly the two steps, no retry, no sync at all (the
    // `login_fail` knob answers EVERY call, so this is the key-less Login and
    // the setup-key Register both being refused — their wire shape is pinned by
    // the dedicated setup-key tests below)
    wait_for("login attempt recorded", || {
        *mock.login_attempts.lock().expect("attempts") == 2
    })
    .await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        *mock.login_attempts.lock().expect("attempts"),
        2,
        "no retry after the two-step was refused"
    );
    assert!(
        mock.sync_connects.lock().expect("sync").is_empty(),
        "the Sync RPC is never attempted after a fatal login"
    );

    handle.stop();
}

// ---------------------------------------------------------------------------
// 2026-10-12: connect login sequencing
// (upstream client/internal/connect.go:774-796 `loginToManagement`;
//  client/internal/auth/auth.go:208-236, :336-355, :491-521;
//  client/internal/state.go:15 — see connector.rs `login_sequence`
//  and account_auth.rs `login_action`)
//
// A connect carries the SETUP KEY at most: there is no JWT anywhere in the
// connect path (the interactive token step happens in the UI process BEFORE
// the VPN starts and keeps nothing on disk). These tests pin that a connect
// sends no token, that setup-key mode uses upstream's two-step
// `Login` → `Register(setup_key)` shape, that a refused account-mode login
// surfaces as `needs-login` (never as the generic permanent `auth`), and that
// the login action's own two-step `Login` → `Register(jwt)` never persists the
// token.
// ---------------------------------------------------------------------------

/// An already-registered peer reconnects with NO credential at all — the
/// public-key-only shape of every upstream connect: one Login, no token on the
/// wire, Connected.
#[tokio::test(flavor = "multi_thread")]
async fn account_mode_connect_is_always_token_less() {
    let mock = MgmtMock::default();
    mock.login_script.lock().expect("script").push_back(Ok(login_response(None)));
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![snapshot(5, 1, 1, None)],
        None,
    ));
    let addr = spawn_mock(mock.clone()).await;

    let handle = spawn_connector(
        addr,
        // account mode: the shell has nothing to send — no setup key, no token
        ConnectorSecrets::default(),
        None,
        Arc::new(WgPeerRegistry::new()),
        Arc::new(RecordingHost::default()),
        None,
    );
    wait_for("connected after the token-less login", || {
        handle.status().state == ConnState::Connected
    })
    .await;

    assert_eq!(
        *mock.login_attempts.lock().expect("attempts"),
        1,
        "a registered peer needs exactly ONE Login"
    );
    let logins = mock.login_calls.lock().expect("logins").clone();
    assert_eq!(logins.len(), 1);
    assert_eq!(logins[0].jwt_token, "", "a connect never carries a JWT");
    assert_eq!(logins[0].setup_key, "");
    assert_eq!(handle.status().last_error, None);
    assert!(handle.status().running);
    handle.stop();
}

/// `PermissionDenied` on the account-mode connect is upstream's
/// `StatusNeedsLogin` (connect.go:373-377): terminal for this attempt, ONE
/// Login, no Sync, and the class-only JSON carries `needs-login` — never the
/// generic `auth` and never a retry loop.
#[tokio::test(flavor = "multi_thread")]
async fn account_mode_permission_denied_needs_login() {
    let mock = MgmtMock::default();
    mock.login_script.lock().expect("script").push_back(Err(Status::permission_denied(
        "peer login has expired, please log in once more",
    )));
    let addr = spawn_mock(mock.clone()).await;

    let handle = spawn_connector(
        addr,
        // the pubkey-only reconnect shape: nothing to send
        ConnectorSecrets::default(),
        Some(Duration::from_secs(60)), // a REAL budget: a retry loop would be visible
        Arc::new(WgPeerRegistry::new()),
        Arc::new(RecordingHost::default()),
        None,
    );
    wait_for("terminal Failed state", || handle.status().state == ConnState::Failed).await;

    let status = handle.status();
    assert!(!status.running, "worker exited");
    assert!(status.terminal, "needs-login is terminal for this attempt");
    assert_eq!(
        status.last_error,
        Some(ErrorClass::NeedsLogin { status: 7 }),
        "a refused account-mode login is needs-login(7), NOT the generic auth class"
    );
    assert_eq!(status.reconnects, 0, "fatal: no retryable failures counted");
    let json = handle.status_json();
    assert!(json.contains("\"class\":\"needs-login\",\"status\":7"), "{json}");
    // the server's own words never reach the class-only JSON surface
    assert!(!json.contains("peer login has expired"), "{json}");

    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        *mock.login_attempts.lock().expect("attempts"),
        1,
        "no retry and no Register on the connect path"
    );
    assert!(
        mock.sync_connects.lock().expect("sync").is_empty(),
        "the Sync RPC is never attempted after a fatal login"
    );
    handle.stop();
}

/// `InvalidArgument` is the other half of upstream's `isLoginNeeded`
/// (auth.go:491-521) and means the same thing for the connect: the peer record
/// is not usable, so the user must log in (the server answers this for a peer
/// whose registration is gone; upstream's connect loop treats the pair as
/// "the peer must re-register").
#[tokio::test(flavor = "multi_thread")]
async fn account_mode_invalid_argument_needs_login() {
    let mock = MgmtMock::default();
    mock.login_script.lock().expect("script").push_back(Err(Status::invalid_argument(
        "peer is not registered",
    )));
    let addr = spawn_mock(mock.clone()).await;

    let handle = spawn_connector(
        addr,
        ConnectorSecrets::default(),
        Some(Duration::from_secs(60)),
        Arc::new(WgPeerRegistry::new()),
        Arc::new(RecordingHost::default()),
        None,
    );
    wait_for("terminal Failed state", || handle.status().state == ConnState::Failed).await;

    assert_eq!(
        handle.status().last_error,
        Some(ErrorClass::NeedsLogin { status: 3 }),
        "InvalidArgument is the second login-needed code"
    );
    assert!(handle.status().terminal);
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(*mock.login_attempts.lock().expect("attempts"), 1);
    handle.stop();
}

/// Transient failures keep the OLD behaviour: retried with backoff until the
/// next attempt succeeds. `Unauthenticated` on a token-less attempt is NOT
/// re-classified (a connect carries no token for it to be a verdict about).
#[tokio::test(flavor = "multi_thread")]
async fn account_mode_transient_login_failure_is_retried() {
    let mock = MgmtMock::default();
    mock.login_script.lock().expect("script").push_back(Err(Status::unavailable(
        "management temporarily unreachable",
    )));
    mock.login_script.lock().expect("script").push_back(Ok(login_response(None)));
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![snapshot(5, 1, 1, None)],
        None,
    ));
    let addr = spawn_mock(mock.clone()).await;

    let handle = spawn_connector(
        addr,
        ConnectorSecrets::default(),
        None,
        Arc::new(WgPeerRegistry::new()),
        Arc::new(RecordingHost::default()),
        None,
    );
    wait_for("connected after the transient retry", || {
        handle.status().state == ConnState::Connected
    })
    .await;

    assert_eq!(*mock.login_attempts.lock().expect("attempts"), 2);
    // last_error is a HISTORICAL record (it does not self-clear) — the point
    // here is that the transient class was retried, not re-classified
    assert_eq!(
        handle.status().last_error,
        Some(ErrorClass::Network),
        "the transient failure keeps its own class and is retried"
    );
    assert_eq!(handle.status().reconnects, 1, "the retried failure is counted once");
    handle.stop();
}

/// Setup-key mode: upstream's TWO-STEP shape (auth.go:208-236). The key-less
/// `Login` runs first; only its login-needed verdict sends the setup key, and
/// it goes to the `Register` step (grpc.go:641-648) — never inside `Login`.
#[tokio::test(flavor = "multi_thread")]
async fn setup_key_mode_registers_with_the_setup_key_after_a_keyless_login() {
    let mock = MgmtMock::default();
    mock.login_script.lock().expect("script").push_back(Err(Status::permission_denied(
        "no peer auth method provided, please use a setup key or interactive SSO login",
    )));
    mock.login_script.lock().expect("script").push_back(Ok(login_response(None)));
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![snapshot(5, 1, 1, None)],
        None,
    ));
    let addr = spawn_mock(mock.clone()).await;

    let handle = spawn_connector(
        addr,
        ConnectorSecrets { setup_key: SENTINEL_SETUP_KEY.into() },
        None,
        Arc::new(WgPeerRegistry::new()),
        Arc::new(RecordingHost::default()),
        None,
    );
    wait_for("connected after the setup-key registration", || {
        handle.status().state == ConnState::Connected
    })
    .await;

    assert_eq!(*mock.login_attempts.lock().expect("attempts"), 2);
    let logins = mock.login_calls.lock().expect("logins").clone();
    assert_eq!(logins.len(), 2, "both steps are captured (the refused one too)");
    assert_eq!(logins[0].setup_key, "", "step 1 is the key-less Login");
    assert_eq!(logins[1].setup_key, SENTINEL_SETUP_KEY, "step 2 is Register with the key");
    assert_eq!(logins[0].jwt_token, "");
    assert_eq!(logins[1].jwt_token, "");
    assert_eq!(handle.status().last_error, None, "a successful register records no error");
    handle.stop();
}

/// A setup-key profile whose peer is ALREADY registered needs no Register:
/// the key-less Login succeeds and the setup key never reaches the wire.
#[tokio::test(flavor = "multi_thread")]
async fn setup_key_mode_skips_register_when_the_keyless_login_succeeds() {
    let mock = MgmtMock::default();
    mock.login_script.lock().expect("script").push_back(Ok(login_response(None)));
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![snapshot(5, 1, 1, None)],
        None,
    ));
    let addr = spawn_mock(mock.clone()).await;

    let handle = spawn_connector(
        addr,
        ConnectorSecrets { setup_key: SENTINEL_SETUP_KEY.into() },
        None,
        Arc::new(WgPeerRegistry::new()),
        Arc::new(RecordingHost::default()),
        None,
    );
    wait_for("connected", || handle.status().state == ConnState::Connected).await;

    assert_eq!(*mock.login_attempts.lock().expect("attempts"), 1, "no second step needed");
    let logins = mock.login_calls.lock().expect("logins").clone();
    assert_eq!(logins.len(), 1);
    assert_eq!(logins[0].setup_key, "", "the key rides only the Register step");
    assert_eq!(logins[0].jwt_token, "");
    handle.stop();
}

// ---------------------------------------------------------------------------
// 2026-10-12: the SSO login action's management step (account_auth.rs
// `login_action` — the core entry the UI-process `account_login_*` NAPI job
// runs right after the PKCE token exchange, BEFORE the VPN starts)
//
// Upstream: client/android/client.go:194-215 (`Run` → `auth.login(urlOpener)`)
// → client/internal/auth/auth.go:184-236 (key-less `doMgmLogin`, then
// `registerPeer` with the fresh JWT). The token is a local: these tests pin
// the wire shape (key-less Login first, JWT only on the Register step) and that
// nothing about it survives the call.
// ---------------------------------------------------------------------------

/// The login action registers the peer with the JWT: a key-less, token-less
/// `Login` first, then `Register` with the token. Two RPCs, nothing else.
#[tokio::test(flavor = "multi_thread")]
async fn login_action_registers_with_the_jwt_after_a_keyless_login() {
    use netbird_core::account_auth::{login_action, LoginActionVerdict};

    let mock = MgmtMock::default();
    mock.login_script.lock().expect("script").push_back(Err(Status::permission_denied(
        "no peer auth method provided, please use a setup key or interactive SSO login",
    )));
    mock.login_script.lock().expect("script").push_back(Ok(login_response(None)));
    let addr = spawn_mock(mock.clone()).await;

    let mut client = plaintext_client(addr, EnvelopeKeyPair::from_secret_bytes(&sentinel_private_key_bytes())).await;
    let verdict = login_action(&mut client, &login_action_meta(), SENTINEL_JWT)
        .await
        .expect("login action");
    assert_eq!(verdict, LoginActionVerdict::Registered);

    assert_eq!(*mock.login_attempts.lock().expect("attempts"), 2);
    let logins = mock.login_calls.lock().expect("logins").clone();
    assert_eq!(logins.len(), 2);
    assert_eq!(logins[0].jwt_token, "", "step 1 is token-less");
    assert_eq!(logins[0].setup_key, "");
    assert_eq!(logins[1].jwt_token, SENTINEL_JWT, "step 2 is Register with the JWT");
    assert_eq!(logins[1].setup_key, "");
    assert!(
        mock.sync_connects.lock().expect("sync").is_empty(),
        "the login action never opens the Sync stream"
    );
    assert!(
        mock.extend_calls.lock().expect("extend").is_empty(),
        "the login action never extends a session"
    );

    // Nothing about the token survives the call: a SECOND action on the same
    // client starts from the same key-less Login again (there is no cached
    // credential, no file, no second use of the spent token).
    mock.login_script.lock().expect("script").push_back(Ok(login_response(None)));
    let again = login_action(&mut client, &login_action_meta(), SENTINEL_JWT)
        .await
        .expect("second login action");
    assert_eq!(again, LoginActionVerdict::Registered);
    let logins = mock.login_calls.lock().expect("logins").clone();
    assert_eq!(logins.len(), 3, "the second action re-runs only the key-less Login");
    assert_eq!(logins[2].jwt_token, "", "the spent token is never replayed");
}

/// A registration the server refuses AGAIN (peer login still expired / the
/// token not permitted) is the login-required verdict, not an error: the UI
/// reports the failed login. Two RPCs, no retry loop of its own.
#[tokio::test(flavor = "multi_thread")]
async fn login_action_refused_registration_needs_login() {
    use netbird_core::account_auth::{login_action, LoginActionVerdict};

    let mock = MgmtMock::default();
    mock.login_script.lock().expect("script").push_back(Err(Status::permission_denied(
        "peer login has expired, please log in once more",
    )));
    mock.login_script.lock().expect("script").push_back(Err(Status::permission_denied(
        "peer login has expired, please log in once more",
    )));
    let addr = spawn_mock(mock.clone()).await;

    let mut client = plaintext_client(addr, EnvelopeKeyPair::from_secret_bytes(&sentinel_private_key_bytes())).await;
    let verdict = login_action(&mut client, &login_action_meta(), SENTINEL_JWT)
        .await
        .expect("a refusal is a verdict, not a transport error");
    assert_eq!(verdict, LoginActionVerdict::NeedsLogin);
    assert_eq!(*mock.login_attempts.lock().expect("attempts"), 2);
}

/// A one-shot JWT the server itself rejects (`Unauthenticated: JWT already
/// used` / expired) is the same verdict: this account is not logged in.
#[tokio::test(flavor = "multi_thread")]
async fn login_action_rejected_token_needs_login() {
    use netbird_core::account_auth::{login_action, LoginActionVerdict};

    let mock = MgmtMock::default();
    mock.login_script.lock().expect("script").push_back(Err(Status::permission_denied(
        "peer login has expired, please log in once more",
    )));
    mock.login_script.lock().expect("script").push_back(Err(Status::unauthenticated(
        "JWT already used",
    )));
    let addr = spawn_mock(mock.clone()).await;

    let mut client = plaintext_client(addr, EnvelopeKeyPair::from_secret_bytes(&sentinel_private_key_bytes())).await;
    let verdict = login_action(&mut client, &login_action_meta(), SENTINEL_JWT)
        .await
        .expect("a rejected token is a verdict");
    assert_eq!(verdict, LoginActionVerdict::NeedsLogin);
}

/// An already-registered peer whose session is fine: the key-less Login
/// succeeds, the JWT never goes on the wire, and no Register step runs.
#[tokio::test(flavor = "multi_thread")]
async fn login_action_never_sends_the_jwt_when_the_login_succeeds() {
    use netbird_core::account_auth::{login_action, LoginActionVerdict};

    let mock = MgmtMock::default();
    mock.login_script.lock().expect("script").push_back(Ok(login_response(None)));
    let addr = spawn_mock(mock.clone()).await;

    let mut client = plaintext_client(addr, EnvelopeKeyPair::from_secret_bytes(&sentinel_private_key_bytes())).await;
    let verdict = login_action(&mut client, &login_action_meta(), SENTINEL_JWT)
        .await
        .expect("login action");
    assert_eq!(verdict, LoginActionVerdict::Registered);
    assert_eq!(*mock.login_attempts.lock().expect("attempts"), 1);
    let logins = mock.login_calls.lock().expect("logins").clone();
    assert_eq!(logins.len(), 1);
    assert_eq!(logins[0].jwt_token, "", "the token is only for the Register step");
    assert_eq!(logins[0].setup_key, "");
}

/// Retry-budget exhaustion through an INJECTED failing management factory
/// (the ManagementFactory seam): attempts are bounded (here: exactly one —
/// the budget is spent after the first failure), the state machine parks in
/// Failed with a sanitized class, and the error carries NO injected message.
#[tokio::test(flavor = "multi_thread")]
async fn retry_budget_exhaustion_is_bounded_and_sanitized() {
    let handle = spawn_connector(
        std::net::SocketAddr::from(([127, 0, 0, 1], 1)),
        ConnectorSecrets { setup_key: "SETUP-KEY-X".into() },
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

/// stop() closes the stream and clears the data-plane seams and NEVER
/// calls management Logout — upstream Logout is the USER-driven
/// deregistration (the server deletes the peer); a stop-time Logout turned
/// the automatic fail-closed recreate of 2026-10-01 01:34:08 into a device
/// deletion. Even with a logged-in client parked in the slot and the mock
/// ready to answer (both the success and the failure script), zero Logout
/// RPCs may arrive. A second stop() is a no-op.
#[tokio::test(flavor = "multi_thread")]
async fn stop_is_idempotent_cleans_up_and_never_logs_out() {
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
            ConnectorSecrets { setup_key: "SETUP-KEY-OK".into() },
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

        // NO logout on the stop path: settle window for any would-be
        // spawned logout task (the removed implementation fired within
        // microseconds), then the count must still be zero
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert!(
            mock.logout_calls.lock().expect("logout").is_empty(),
            "stop() must never call management Logout"
        );
        assert_eq!(handle.status().logout_ok, None, "stop leaves logout_ok untouched");
        assert!(handle.status_json().contains("\"logout_ok\":null"));

        // idempotent second stop
        let json2 = handle.stop_json();
        assert!(json2.contains("\"already_stopped\":true"), "{json2}");
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            mock.logout_calls.lock().expect("logout").is_empty(),
            "no logout on the second stop either"
        );
    }
}

/// The EXPLICIT logout (`connector_logout()` / `logout_json()`) is the only
/// Logout path: it fires exactly ONE RPC on the logged-in client, reports
/// the sanitized outcome (success; NotFound = "peer already gone" per
/// upstream logoutPeerGone; a server failure = its class), spends the
/// client (a second call refuses without another RPC) and a subsequent
/// stop() stays silent. `logout_json()` blocks on the connector runtime —
/// the NAPI shape — so the tokio test drives it from a blocking thread,
/// exactly like the shell's watcher tick would.
#[tokio::test(flavor = "multi_thread")]
async fn explicit_logout_fires_once_and_stop_after_it_stays_silent() {
    for (logout_fail_status, want_ok, want_reason) in [
        (None, true, "ok"),
        (Some(Status::not_found("peer gone")), true, "peer-already-gone"),
        (Some(Status::internal("logout boom")), false, "server"),
    ] {
        let mock = MgmtMock::default();
        *mock.logout_fail.lock().expect("fail") = logout_fail_status.clone();
        mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
            vec![snapshot(5, 2, 1, None)],
            None,
        ));
        let addr = spawn_mock(mock.clone()).await;

        let handle = spawn_connector(
            addr,
            ConnectorSecrets { setup_key: "SETUP-KEY-OK".into() },
            None,
            Arc::new(WgPeerRegistry::new()),
            Arc::new(RecordingHost::default()),
            None,
        );

        wait_for("snapshot applied", || handle.status().peer_count == 2).await;

        // one explicit logout: exactly one RPC, outcome + logout_ok recorded
        let caller = handle.clone();
        let json = tokio::task::spawn_blocking(move || caller.logout_json())
            .await
            .expect("logout_json join");
        assert!(json.contains(&format!("\"ok\":{want_ok}")), "{json}");
        assert!(json.contains(&format!("\"reason\":\"{want_reason}\"")), "{json}");
        assert_eq!(mock.logout_calls.lock().expect("logout").len(), 1);
        assert_eq!(handle.status().logout_ok, Some(want_ok));

        // the client is spent: a second explicit logout refuses, no new RPC
        let caller = handle.clone();
        let json2 = tokio::task::spawn_blocking(move || caller.logout_json())
            .await
            .expect("logout_json join 2");
        assert!(
            json2.contains("\"error\":\"no-live-management-connection\""),
            "{json2}"
        );
        assert_eq!(mock.logout_calls.lock().expect("logout").len(), 1);

        // stop after the explicit logout: still exactly one Logout total
        let stop_json = handle.stop_json();
        assert!(stop_json.contains("\"ok\":true"), "{stop_json}");
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert_eq!(
            mock.logout_calls.lock().expect("logout").len(),
            1,
            "stop() adds no Logout on top of the explicit one"
        );
        assert_eq!(handle.status().logout_ok, Some(want_ok), "logout_ok survives the stop");
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
        ConnectorSecrets { setup_key: "SETUP-KEY-OK".into() },
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
        ConnectorSecrets { setup_key: SENTINEL_SETUP_KEY.into() },
        Some(Duration::ZERO),
        Arc::new(WgPeerRegistry::new()),
        Arc::new(RecordingHost::default()),
        None,
    );
    wait_for("failed state", || handle.status().state == ConnState::Failed).await;

    // the sentinel setup key really reached the server (the scan below is
    // therefore meaningful — the secret was in play). The login FAILED
    // (PermissionDenied on BOTH steps of the setup-key two-step), so the
    // capture comes from the attempts counter side: assert the requests were
    // seen, then flip the mock and log in successfully to prove the sentinels
    // cross the real path.
    assert_eq!(*mock.login_attempts.lock().expect("attempts"), 2);

    // capture the sentinel values through a SUCCESSFUL login: reuse the
    // same mock, clear the failure, restart a connector on it. The setup key
    // only reaches the wire on the `Register` step, so the scripted first step
    // is refused (upstream's two-step) and the second one registers.
    *mock.login_fail.lock().expect("fail") = None;
    mock.login_script
        .lock()
        .expect("script")
        .push_back(Err(Status::permission_denied("no peer auth method provided")));
    mock.login_script
        .lock()
        .expect("script")
        .push_back(Ok(login_response(Some(1_700_000_000))));
    let handle2 = spawn_connector(
        addr,
        ConnectorSecrets { setup_key: SENTINEL_SETUP_KEY.into() },
        None,
        Arc::new(WgPeerRegistry::new()),
        Arc::new(RecordingHost::default()),
        None,
    );
    // wait for the REGISTER step specifically: the key-less step is captured
    // first and would satisfy a mere "any call captured" condition (the flaky
    // shape this replaced).
    wait_for("the setup-key Register reached the server", || {
        mock.login_calls
            .lock()
            .expect("logins")
            .iter()
            .any(|l| l.setup_key == SENTINEL_SETUP_KEY)
    })
    .await;
    {
        let logins = mock.login_calls.lock().expect("logins");
        assert_eq!(logins[logins.len() - 1].setup_key, SENTINEL_SETUP_KEY);
        // 2026-10-12: the setup key rides ONLY the Register step — a JWT is not
        // a connect credential at all any more, so it can never ride along (it
        // stays a local secret, which is exactly why the leak scan above/below
        // keeps covering it).
        assert_eq!(logins[logins.len() - 1].jwt_token, "");
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
        ConnectorSecrets { setup_key: SENTINEL_SETUP_KEY.into() },
        None,
        wg.clone(),
        Arc::new(RecordingHost::default()),
        None,
    );
    wait_for("snapshot applied", || handle.status().peer_count == 1).await;

    let status = handle.status_json();
    let stop = handle.stop_json();
    // the RPC-rich path that used to ride stop() is the EXPLICIT logout
    // now (stop never logs out) — drive it once and keep the same
    // secret-leak assertions over its JSON too
    let caller = handle.clone();
    let logout = tokio::task::spawn_blocking(move || caller.logout_json())
        .await
        .expect("logout_json join");
    assert_eq!(mock.logout_calls.lock().expect("logout").len(), 1);
    let status_after_stop = handle.status_json();
    let debug = format!("{:?}", handle);
    let registry_debug = format!("{:?}", wg.snapshot());
    for s in &sentinels {
        assert!(!status.contains(s.as_str()), "secret leaked into status JSON: {s}");
        assert!(!stop.contains(s.as_str()), "secret leaked into stop JSON: {s}");
        assert!(!logout.contains(s.as_str()), "secret leaked into logout JSON: {s}");
        assert!(
            !status_after_stop.contains(s.as_str()),
            "secret leaked into status JSON: {s}"
        );
        assert!(!debug.contains(s.as_str()), "secret leaked into Debug: {s}");
        assert!(!registry_debug.contains(s.as_str()), "secret leaked into registry Debug: {s}");
    }
}

/// 2026-10-12: NO silent session renewal. The connector holds no JWT, so it
/// cannot and must not call `ExtendAuthSession` on its own — upstream's
/// extension is an INTERACTIVE app-process flow that fetches a fresh token
/// first (`client/android/session.go:127-155`, `login.go:191-204`) and the
/// Android engine arms no expiry timers (`client/android/session.go:20-24`).
/// The anchored 3-state deadline is still published for the UI, whatever the
/// credential mode.
#[tokio::test(flavor = "multi_thread")]
async fn no_silent_session_renewal_and_the_deadline_is_published() {
    let soon = unix_now() + 2; // expires in 2s: a silent renewal would fire now
    let mock = MgmtMock::default();
    *mock.login_response.lock().expect("lr") = Some(login_response(Some(soon)));
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![snapshot(5, 1, 0, None)],
        None,
    ));
    let addr = spawn_mock(mock.clone()).await;
    let handle = spawn_connector(
        addr,
        ConnectorSecrets::default(), // account mode: the token-less connect shape
        None,
        Arc::new(WgPeerRegistry::new()),
        Arc::new(RecordingHost::default()),
        None,
    );
    wait_for("snapshot applied", || handle.status().peer_count == 1).await;

    // long enough that the old 10 ms renewal tick would have fired many times
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        mock.extend_calls.lock().expect("extend").is_empty(),
        "the connector must never extend a session by itself (no token exists)"
    );
    let json = handle.status_json();
    assert!(json.contains("\"session_expiry\":\"set\""), "{json}");
    assert!(
        json.contains(&format!("\"session_expires_at_unix\":{soon}")),
        "the anchored deadline is published for the UI: {json}"
    );
    assert!(
        !json.contains("session_renew_attempts"),
        "the silent-renewal counter is gone with the path: {json}"
    );
    handle.stop();
}

/// The NAPI-level JSON entry points (global singleton): rejection reasons,
/// the running guard, and the stop → restart path. This is the ONLY test
/// touching the global connector slot.
#[tokio::test(flavor = "multi_thread")]
async fn napi_global_start_status_stop_roundtrip() {
    use netbird_core::connector::{connector_start_json, connector_status_json, connector_stop_json};

    // invalid credentials are rejected with a stable reason (2026-10-11 A1:
    // an ALL-EMPTY document is a VALID pubkey-only reconnect now — the
    // rejection is for a malformed one).
    let json = connector_start_json(
        &config_json_for(std::net::SocketAddr::from(([127, 0, 0, 1], 1))),
        "{\"setup_key\":7}",
    );
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
    // INVARIANT, not a fixed value: the start response snapshots the state
    // machine AFTER the worker task is already running, so the snapshot
    // races the worker's FIRST login. The endpoint (127.0.0.1:1) refuses by
    // construction, so the worker may already have taken
    // Connecting --Lost--> Reconnecting (src/state.rs next_state) before the
    // status read — that transition is legal at any instant after spawn.
    // Reachable set here is exactly {connecting, reconnecting}: Connected
    // needs a successful login (nothing listens on port 1), Failed needs the
    // retry budget of ExponentialBackoff::upstream_stream_default() to
    // exhaust (hours of sim time), Disconnected/Closed need a stop.
    let state = json_field_str(&json, "state");
    assert!(
        matches!(state.as_deref(), Some("connecting" | "reconnecting")),
        "freshly spawned connector must be connecting|reconnecting, got {json}"
    );

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

/// String value of `"key":"..."` in a flat connector JSON document (the
/// napi surface is a string-only seam; the strict reader is crate-internal).
fn json_field_str(doc: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let i = doc.find(&needle)? + needle.len();
    let rest = &doc[i..];
    let end = rest.find('"').unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
