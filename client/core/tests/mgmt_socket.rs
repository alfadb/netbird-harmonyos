// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N3-7 protected management socket tests (fail-closed gap 1).
//!
//! An in-process tonic management server counts every TCP connection it
//! accepts; the connector/factory run the REAL production dial path
//! (`GrpcManagementFactory::with_socket_source` →
//! `Endpoint::connect_with_connector` → `mgmtsock::connect_protected`), where
//! every dial takes a fresh socket fd from the provider, dups it
//! (`F_DUPFD_CLOEXEC`) and never touches the original:
//!
//! 1. the fd number native uses is a DIFFERENT dup — the provider-side
//!    original stays open/usable and closeable by its owner;
//! 2. the gRPC channel really runs over the provided socket — the server
//!    accepts EXACTLY ONE connection per dial (an unprotected self-dial
//!    would produce a second accept), for both the fresh-connect and the
//!    pre-connected (adopted, EISCONN) provider variants;
//! 3. a missing/dead fd or a failing provider fails CLOSED (start refused /
//!    dial errors → terminal `Failed`, sanitized class);
//! 4. every re-dial re-protects: after a transport-level break the fresh fd
//!    count equals the connection-attempt count (server accepts), no fd
//!    reuse.
//!
//! No `sleep` as an assertion: all waits are bounded `wait_for` polls on
//! observed state.

use core::pin::Pin;
use std::collections::VecDeque;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use base64::Engine as _;
use prost::Message as _;
use prost_types::Timestamp;
use tokio::net::TcpListener;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

use netbird_core::backoff::{Clock, ExponentialBackoff, Rng};
use netbird_core::connector::{
    ConfigApplier, ConnectorHandle, ConnectorSecrets, GrpcManagementFactory, ManagementFactory,
    SyncPolicy, WgPeerRegistry,
};
use netbird_core::envelope::{self, EnvelopeKeyPair, EnvelopePublicKey};
use netbird_core::grpc::proto::management_service_server::{
    ManagementService, ManagementServiceServer,
};
use netbird_core::grpc::proto::{
    Empty, EncryptedMessage, ExtendAuthSessionRequest, ExtendAuthSessionResponse, LoginRequest,
    LoginResponse, ServerKeyResponse, SyncRequest, SyncResponse,
};
use netbird_core::mgmtsock::{
    dup_socket_fd, mgmt_socket_open, ManagementSocketProvider, ProtectedSocketFdSource,
};
use netbird_core::state::ConnState;
use netbird_core::sys;

// Host-test link surface (same as the other integration tests): stub the
// OHOS-only symbols the crate's NAPI/hilog surface needs at link time.
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
        _utf8name: *const c_void,
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
        _name: *const c_void,
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

fn sentinel_private_key_bytes() -> [u8; 32] {
    let mut key = [0u8; 32];
    key.copy_from_slice("SENTINEL-PRIVATE-KEY-n3-7-012345".as_bytes());
    key
}

fn sentinel_private_key_b64() -> String {
    base64::engine::general_purpose::STANDARD.encode(sentinel_private_key_bytes())
}

fn fd_open(fd: i32) -> bool {
    unsafe { sys::fcntl(fd, sys::F_GETFD) != -1 }
}

/// Wait (bounded) until `cond()` holds; panics with `what` on deadline.
async fn wait_for<F: Fn() -> bool>(what: &'static str, cond: F) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !cond() {
        assert!(Instant::now() <= deadline, "deadline waiting for {what}");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

fn deterministic_backoff(max_elapsed: Option<Duration>) -> ExponentialBackoff {
    ExponentialBackoff::new(
        Duration::from_millis(5),
        0.0,
        1.7,
        Duration::from_millis(100),
        max_elapsed,
    )
}

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

struct NoopHost;
impl ConfigApplier for NoopHost {
    fn apply(&self, _map: &netbird_core::network_map::NetworkMap) {}
    fn clear(&self) {}
}

/// A provider that ALWAYS fails (the injected "protect failed / no socket"
/// path): every dial fails closed, nothing ever dials unprotected.
struct FailingProvider;

impl ManagementSocketProvider for FailingProvider {
    fn take_fd(&self) -> Result<i32, netbird_core::mgmtsock::SocketSeamError> {
        Err(netbird_core::mgmtsock::SocketSeamError::NoSocket)
    }
}

/// Provider handing out fds from a fixed script.
struct ScriptProvider(Mutex<VecDeque<i32>>);

impl ScriptProvider {
    fn new(fds: &[i32]) -> Self {
        ScriptProvider(Mutex::new(fds.iter().copied().collect()))
    }
}

impl ManagementSocketProvider for ScriptProvider {
    fn take_fd(&self) -> Result<i32, netbird_core::mgmtsock::SocketSeamError> {
        self.0
            .lock()
            .expect("script")
            .pop_front()
            .ok_or(netbird_core::mgmtsock::SocketSeamError::NoSocket)
    }
}

// ---------------------------------------------------------------------------
// management mock (same wire surface as tests/connector.rs) + accept counting
// ---------------------------------------------------------------------------

enum SyncStep {
    UpdatesThen(Vec<SyncResponse>, Option<Status>),
}

#[derive(Clone)]
struct MgmtMock {
    keys: EnvelopeKeyPair,
    login_fail: Arc<Mutex<Option<Status>>>,
    sync_script: Arc<Mutex<VecDeque<SyncStep>>>,
    sync_connects: Arc<Mutex<Vec<()>>>,
}

impl Default for MgmtMock {
    fn default() -> Self {
        MgmtMock {
            keys: EnvelopeKeyPair::generate().expect("server key pair"),
            login_fail: Arc::new(Mutex::new(None)),
            sync_script: Arc::new(Mutex::new(VecDeque::new())),
            sync_connects: Arc::new(Mutex::new(Vec::new())),
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
        if let Some(status) = self.login_fail.lock().expect("login fail").clone() {
            return Err(status);
        }
        let env = request.into_inner();
        let plaintext = decrypt_body(&self.keys, &env)?;
        LoginRequest::decode(plaintext.as_slice())
            .map_err(|e| Status::invalid_argument(format!("body is not LoginRequest: {e}")))?;
        let response = LoginResponse {
            netbird_config: None,
            peer_config: Some(netbird_core::grpc::proto::PeerConfig {
                address: "10.64.0.9".into(),
                ..Default::default()
            }),
            checks: vec![],
            session_expires_at: Some(Timestamp { seconds: 1_893_456_000, nanos: 0 }),
        };
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

    async fn logout(
        &self,
        request: Request<EncryptedMessage>,
    ) -> Result<Response<Empty>, Status> {
        let env = request.into_inner();
        decrypt_body(&self.keys, &env)?;
        Ok(Response::new(Empty {}))
    }

    async fn extend_auth_session(
        &self,
        request: Request<EncryptedMessage>,
    ) -> Result<Response<EncryptedMessage>, Status> {
        let env = request.into_inner();
        let plaintext = decrypt_body(&self.keys, &env)?;
        ExtendAuthSessionRequest::decode(plaintext.as_slice())
            .map_err(|e| Status::invalid_argument(format!("body: {e}")))?;
        let response = ExtendAuthSessionResponse {
            session_expires_at: Some(Timestamp { seconds: 1_893_456_000, nanos: 0 }),
        };
        let client_pk = EnvelopePublicKey::from_base64(&env.wg_pub_key).expect("client pk");
        let sealed = envelope::seal(&client_pk, &self.keys, &response.encode_to_vec())
            .expect("seal extend reply");
        Ok(Response::new(EncryptedMessage { wg_pub_key: env.wg_pub_key, body: sealed, version: 0 }))
    }
}

/// `Stream<Item = io::Result<TcpStream>>` over a shared listener, counting
/// every accepted TCP connection (the observable "connection attempt"
/// counter each protected fd must map 1:1 onto) and keeping a private dup of
/// each accepted socket so the TEST can deterministically kill the transport
/// (shutdown(Both)) to force a client re-dial.
#[derive(Clone)]
struct CountingListener {
    inner: Arc<TcpListener>,
    accepts: Arc<AtomicUsize>,
    conns: Arc<Mutex<Vec<std::net::TcpStream>>>,
}

impl CountingListener {
    fn new(inner: TcpListener) -> Self {
        CountingListener {
            inner: Arc::new(inner),
            accepts: Arc::new(AtomicUsize::new(0)),
            conns: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn accepted(&self) -> usize {
        self.accepts.load(Ordering::Acquire)
    }

    /// Kill every live accepted connection at the TRANSPORT level (FIN to
    /// the client → its pooled connection is dead → the next RPC must
    /// re-dial). Each dup handle is closed afterwards; the dup-based fd
    /// contract keeps tonic's own handles valid until the close lands.
    fn kill_all_connections(&self) {
        let mut conns = self.conns.lock().expect("conns");
        for c in conns.drain(..) {
            let _ = c.shutdown(std::net::Shutdown::Both);
        }
    }
}

impl Stream for CountingListener {
    type Item = std::io::Result<tokio::net::TcpStream>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.inner.poll_accept(cx) {
            Poll::Ready(Ok((stream, _))) => {
                self.accepts.fetch_add(1, Ordering::AcqRel);
                // keep a test-private dup for the deterministic transport
                // kill (never closed implicitly before shutdown — it is
                // moved into a std TcpStream which closes exactly this dup)
                if let Ok(dup_fd) = netbird_core::mgmtsock::dup_socket_fd(stream.as_raw_fd()) {
                    let dup = unsafe { std::net::TcpStream::from_raw_fd(dup_fd) };
                    self.conns.lock().expect("conns").push(dup);
                }
                Poll::Ready(Some(Ok(stream)))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e))),
            Poll::Pending => Poll::Pending,
        }
    }
}

fn snapshot(serial: u64, peers: usize) -> SyncResponse {
    SyncResponse {
        network_map: Some(netbird_core::grpc::proto::NetworkMap {
            serial,
            peer_config: Some(netbird_core::grpc::proto::PeerConfig {
                address: "10.64.0.9".into(),
                ..Default::default()
            }),
            remote_peers: (0..peers)
                .map(|i| netbird_core::grpc::proto::RemotePeerConfig {
                    wg_pub_key: format!("UEVFUjBBM{0:0>2}", i),
                    allowed_ips: vec![format!("10.30.30.{i}/32")],
                    ..Default::default()
                })
                .collect(),
            remote_peers_is_empty: false,
            routes: vec![],
            ..Default::default()
        }),
        session_expires_at: None,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// the protected-dial factory + server spawn used across the tests
// ---------------------------------------------------------------------------

fn factory_over(addr: std::net::SocketAddr, source: Arc<dyn ManagementSocketProvider>)
-> GrpcManagementFactory {
    let config = netbird_core::connector::ConnectorConfig::from_json(&format!(
        "{{\"management_url\":\"http://{addr}\",\"private_key\":\"{}\"}}",
        sentinel_private_key_b64()
    ))
    .expect("test config");
    GrpcManagementFactory::with_socket_source(&config, config.keys.clone(), source, addr)
}

/// Bind + spawn the counting management server; returns (addr, listener).
async fn spawn_counting_server(svc: MgmtMock) -> (std::net::SocketAddr, CountingListener) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let counter = CountingListener::new(listener);
    let task_counter = counter.clone();
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(ManagementServiceServer::new(svc))
            .serve_with_incoming(task_counter)
            .await
            .expect("tonic serve");
    });
    (addr, counter)
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

/// ① (fd contract) The dial consumed a DUP: the number native used differs
/// from the provided fd, the provided fd is still open/usable afterwards
/// (its owner can close it), and exactly one connection attempt reached the
/// server. Both the fresh-connect and the pre-connected provider variants.
#[tokio::test(flavor = "multi_thread")]
async fn dial_consumes_a_dup_original_stays_owner_owned() {
    let mock = MgmtMock::default();
    let (addr, counter) = spawn_counting_server(mock).await;

    // --- variant A: production shape — native pre-opens (unconnected), the
    // dial (Rust) connects it AFTER the (simulated) shell protect.
    let fd = open_fd();
    let source = Arc::new(ProtectedSocketFdSource::new_with_fd(fd));
    let factory = factory_over(addr, source.clone());
    let mut client = factory.connect().await.expect("protected dial");
    client.get_server_key().await.expect("rpc over the protected socket");

    assert_eq!(source.taken(), 1, "exactly one fd handed out for one dial");
    assert_eq!(counter.accepted(), 1, "one connection attempt");
    assert!(fd_open(fd), "the PROVIDED fd must stay open (native only dups)");

    // the dup native used is a DIFFERENT number than the provided fd: dup
    // again (owner-side view) and pin the distinctness invariant
    let dup = dup_socket_fd(fd).expect("dup");
    assert_ne!(dup, fd, "dup must be a distinct descriptor number");
    unsafe { sys::close(dup) };

    // the owner closes the original — native never did, never will
    unsafe { sys::close(fd) };
    assert!(!fd_open(fd));

    // --- variant B: pre-connected socket (host-test shape): the dial must
    // ADOPT it (EISCONN path), again without touching the original.
    let std_client = std::net::TcpStream::connect(addr).expect("pre-connect");
    let pre_fd = std_client.into_raw_fd();
    let source2 = Arc::new(ProtectedSocketFdSource::new_with_fd(pre_fd));
    let factory2 = factory_over(addr, source2.clone());
    let mut client2 = factory2.connect().await.expect("adopted pre-connected socket");
    client2.get_server_key().await.expect("rpc over adopted socket");
    assert_eq!(source2.taken(), 1);
    assert!(fd_open(pre_fd), "pre-connected original still owner-owned");
    unsafe { sys::close(pre_fd) };

    // two dials total reached the server (one per variant) — the adopted
    // variant did NOT let native self-dial a second connection
    assert_eq!(counter.accepted(), 2);
}

/// ② (proof of "the channel really goes through the protected socket") a
/// pre-connected socket is adopted: the server sees EXACTLY ONE accept —
/// if native had dialed by itself instead, the pre-connected socket would
/// sit unused and a SECOND accept would appear (or the RPC would fail).
#[tokio::test(flavor = "multi_thread")]
async fn grpc_channel_runs_only_over_the_protected_socket() {
    let mock = MgmtMock::default();
    let (addr, counter) = spawn_counting_server(mock).await;

    // pre-connect a REAL socket to the server ourselves (the test plays the
    // shell: open + protect would sit between open and this dial on device)
    let std_client = std::net::TcpStream::connect(addr).expect("connect");
    let peer = std_client.peer_addr().expect("peer addr");
    let fd = std_client.into_raw_fd();

    let source = Arc::new(ProtectedSocketFdSource::new_with_fd(fd));
    let factory = factory_over(addr, source.clone());
    let mut client = factory.connect().await.expect("connect via provided socket");
    let key = client.get_server_key().await.expect("GetServerKey over provided socket");
    assert!(!key.to_base64().is_empty());

    assert_eq!(counter.accepted(), 1, "only our pre-connect");
    assert_eq!(
        peer, addr,
        "sanity: the provided socket really points at the management server"
    );
    assert_eq!(source.taken(), 1);
    unsafe { sys::close(fd) };
}

/// ③ (fail-closed) a failing provider fails the dial — and the connector
/// parks in the explicit terminal error state (`Failed`, sanitized Network
/// class), never falling back to an unprotected direct dial.
#[tokio::test(flavor = "multi_thread")]
async fn failing_provider_fails_closed_to_terminal_error() {
    let config = netbird_core::connector::ConnectorConfig::from_json(&format!(
        "{{\"management_url\":\"http://127.0.0.1:1\",\"private_key\":\"{}\"}}",
        sentinel_private_key_b64()
    ))
    .expect("config");
    let factory = GrpcManagementFactory::with_socket_source(
        &config,
        config.keys.clone(),
        Arc::new(FailingProvider),
        std::net::SocketAddr::from(([127, 0, 0, 1], 1)),
    );
    let handle = ConnectorHandle::spawn(
        tokio::runtime::Handle::current(),
        Arc::new(factory),
        Arc::new(WgPeerRegistry::new()),
        Arc::new(NoopHost),
        ConnectorSecrets { setup_key: "SETUP-KEY-SOCKET".into(), jwt: String::new() },
        netbird_core::grpc::PeerMeta {
            hostname: "ohos-socket".into(),
            os_name: "harmonyos".into(),
            os_version: "5.0.0".into(),
            netbird_version: "0.1.0".into(),
        },
        deterministic_backoff(Some(Duration::ZERO)),
        SyncPolicy {
            backoff: deterministic_backoff(Some(Duration::ZERO)),
            rng: Box::new(NoRng),
            clock: Box::new(RealClock),
        },
        Duration::from_secs(600),
        Duration::from_millis(10),
        false,
        None,
        None, // N5c: production (idle, signal-less) ICE orchestrator
        None, // N5d: no signal material in this harness
    );

    wait_for("terminal Failed state with a socket-starved dial", || {
        handle.status().state == ConnState::Failed
    })
    .await;
    let status = handle.status();
    assert!(!status.running, "worker ended by itself");
    assert!(status.terminal, "socket starvation is a terminal failure (N3-7 gap 3 field)");
    assert_eq!(
        status.last_error,
        Some(netbird_core::connector::ErrorClass::Network),
        "sanitized class only — no OS strings across the boundary"
    );
    let json = handle.status_json();
    assert!(json.contains("\"terminal\":true"), "{json}");
    assert!(
        !json.contains("no-protected-socket"),
        "the seam token must not leak into status either: {json}"
    );
    handle.stop();
}

/// ④ (per-reconnect re-protect) after a TRANSPORT-level break (the server
/// connection dies), the fresh-fd count equals the connection-attempt count:
/// every re-dial takes a NEW protected socket, none is reused.
#[tokio::test(flavor = "multi_thread")]
async fn every_redial_takes_a_fresh_fd_matching_attempts() {
    let mock = MgmtMock::default();
    // 1st stream: one snapshot, then it dies (scripted stream failure)
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![snapshot(5, 1)],
        Some(Status::internal("stream reset")),
    ));
    // 2nd stream: the server continues and holds
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![snapshot(6, 2)],
        None,
    ));

    let (addr, counter) = spawn_counting_server(mock).await;

    // two fresh native-opened sockets = two protected dials available
    let fd_a = open_fd();
    let fd_b = open_fd();
    let source = Arc::new(ProtectedSocketFdSource::new_with_fd(fd_a));
    source.feed(fd_b);
    let factory = factory_over(addr, source.clone());

    let host = Arc::new(Recorder::default());
    let handle = ConnectorHandle::spawn(
        tokio::runtime::Handle::current(),
        Arc::new(factory),
        Arc::new(WgPeerRegistry::new()),
        host.clone(),
        ConnectorSecrets { setup_key: "SETUP-KEY-SOCKET".into(), jwt: String::new() },
        netbird_core::grpc::PeerMeta {
            hostname: "ohos-socket".into(),
            os_name: "harmonyos".into(),
            os_version: "5.0.0".into(),
            netbird_version: "0.1.0".into(),
        },
        deterministic_backoff(None),
        SyncPolicy {
            backoff: deterministic_backoff(None),
            rng: Box::new(NoRng),
            clock: Box::new(RealClock),
        },
        Duration::from_secs(600),
        Duration::from_millis(10),
        false,
        Some(source.clone()),
        None, // N5c: production (idle, signal-less) ICE orchestrator
        None, // N5d: no signal material in this harness
    );

    wait_for("first snapshot applied over protected socket #1", || {
        host.serials.lock().expect("serials").last() == Some(&5)
    })
    .await;
    assert_eq!(source.taken(), 1, "initial dial took exactly one fd");
    assert_eq!(counter.accepted(), 1, "one connection attempt so far");

    // Kill the transport under the client's pooled connection: the next
    // stream attempt can no longer silently reuse the dead connection and
    // MUST re-dial through a fresh protected socket (fd #2).
    counter.kill_all_connections();

    wait_for("second snapshot applied over protected socket #2", || {
        host.serials.lock().expect("serials").last() == Some(&6)
    })
    .await;

    let taken = source.taken();
    let accepts = counter.accepted() as u64;
    assert!(taken >= 2, "the reconnect must take a FRESH fd (got {taken})");
    assert_eq!(
        taken, accepts,
        "fd acquisitions must equal connection attempts (per-dial re-protect)"
    );
    assert!(fd_open(fd_a) && fd_open(fd_b), "originals stay owner-owned");

    let status = handle.status();
    assert_eq!(status.state, ConnState::Connected, "re-established after the break");
    assert!(status.reconnects >= 1, "the break was counted");
    assert!(!status.terminal, "a reconnecting connector is not terminal");

    handle.stop();
    unsafe { sys::close(fd_a) };
    unsafe { sys::close(fd_b) };
}

/// ⑤ (NAPI-level fail-closed gate, global slot) `connector_start_with_socket`
/// refuses missing fds, dead fds and bad addresses with stable tokens; a
/// healthy fd starts the protected path; `connector_socket_feed` validates
/// and resupplies.
#[tokio::test(flavor = "multi_thread")]
async fn start_with_socket_refusal_gate_and_feed() {
    use netbird_core::connector::{
        connector_socket_feed_json, connector_start_with_socket_json, connector_stop_json,
        connector_status_json,
    };

    let addr = {
        let l = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        l.local_addr().expect("addr")
    };
    let cfg = format!(
        "{{\"management_url\":\"http://{addr}\",\"private_key\":\"{}\"}}",
        sentinel_private_key_b64()
    );
    let creds = "{\"setup_key\":\"SETUP-GATE\"}";

    // feed with no connector at all
    let json = connector_socket_feed_json(-1);
    assert!(json.contains("\"ok\":false"), "{json}");
    assert!(json.contains("no-connector"), "{json}");

    // missing fd
    let json = connector_start_with_socket_json(-1, &cfg, creds, &format!("{{\"connect_addr\":\"{addr}\"}}"));
    assert!(json.contains("\"started\":false"), "{json}");
    assert!(json.contains("socket-fd-missing"), "{json}");

    // dead fd — a number high enough to be unallocated in this process (fd
    // numbers fill lowest-first; parallel tests hold only tens of fds), so
    // the dup probe must fail closed without a recycling race
    let json = connector_start_with_socket_json(
        UNALLOCATED_FD, &cfg, creds, &format!("{{\"connect_addr\":\"{addr}\"}}"),
    );
    assert!(json.contains("\"started\":false"), "{json}");
    assert!(json.contains("socket-fd-invalid"), "{json}");

    // invalid config still rejected first-class
    let json = connector_start_with_socket_json(-1, "not json", creds, "{}");
    assert!(json.contains("invalid-config"), "{json}");

    // bad address documents
    let fd = open_fd();
    for bad_addr in ["{}", "{\"connect_addr\":\"nope\"}", "{\"connect_addr\":\"999.1.1.1:1\"}"] {
        let json = connector_start_with_socket_json(fd, &cfg, creds, bad_addr);
        assert!(json.contains("\"started\":false"), "{json}");
        assert!(json.contains("socket-addr-invalid"), "{json}");
    }
    // the fd survived the refused starts untouched
    assert!(fd_open(fd), "refusals must not consume or close the provided fd");

    // healthy start over a real protected socket (worker dials the mock in
    // the background — here just the mock-less addr, it retries harmlessly)
    let json = connector_start_with_socket_json(fd, &cfg, creds, &format!("{{\"connect_addr\":\"{addr}\"}}"));
    assert!(json.contains("\"started\":true"), "{json}");
    assert!(json.contains("\"protected\":true"), "{json}");
    assert!(json.contains("\"state\":\"connecting\""), "{json}");

    // status while running: not terminal
    let status = connector_status_json();
    assert!(status.contains("\"terminal\":false"), "{status}");

    // feed: invalid fds rejected, a fresh socket queued (the seed may or may
    // not have been taken by the dialing worker yet — the queue holds it
    // until some dial needs it)
    let json = connector_socket_feed_json(-1);
    assert!(json.contains("socket-fd-missing"), "{json}");
    let fd2 = open_fd();
    let json = connector_socket_feed_json(fd2);
    assert!(json.contains("\"ok\":true"), "{json}");
    let q = json
        .split("\"queued\":")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .and_then(|v| v.parse::<u64>().ok())
        .expect("queued number");
    assert!((1..=2).contains(&q), "queued {q} in {json}");

    // stop frees the slot; a second stop is idempotent
    let json = connector_stop_json();
    assert!(json.contains("\"ok\":true"), "{json}");
    let json = connector_stop_json();
    assert!(json.contains("\"already_stopped\":true"), "{json}");
    assert!(fd_open(fd) && fd_open(fd2), "stop must not close provider-owned fds");
    unsafe { sys::close(fd) };
    unsafe { sys::close(fd2) };
}

// ---------------------------------------------------------------------------
// small helpers used above
// ---------------------------------------------------------------------------

fn open_fd() -> i32 {
    // `mgmt_socket_open()` JSON is `{"fd":N,"bind_rc":R,"bind_errno":E}`;
    // integration tests only get the STRING surface, so extract the fd
    // number directly (the strict reader is crate-internal).
    let open = mgmt_socket_open();
    let i = open.find("\"fd\":").expect("fd key") + "\"fd\":".len();
    let rest = &open[i..];
    let end = rest.find(',').unwrap_or(rest.find('}').unwrap_or(rest.len()));
    rest[..end].parse::<i32>().expect("fd number")
}

/// An fd number that is not open in this process: descriptors fill
/// lowest-first and the test process holds only tens of them, so a high
/// number is deterministically dead (unlike a closed-low-number race under
/// parallel tests).
const UNALLOCATED_FD: i32 = 900;

/// Records applied network-map serials (host seam).
#[derive(Default)]
struct Recorder {
    serials: Mutex<Vec<u64>>,
}

impl ConfigApplier for Recorder {
    fn apply(&self, map: &netbird_core::network_map::NetworkMap) {
        self.serials.lock().expect("serials").push(map.serial);
    }
    fn clear(&self) {}
}
