// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! Shared rig for the N4a signal integration tests (`signal_channel.rs`,
//! `signal_session.rs`, `signal_link.rs`): an in-process tonic
//! `SignalExchange` mock that implements the REAL protocol of the DEPLOYED
//! server generation (netbird-server v0.78.1, verified against the release
//! binary in the N10 interop; `docs/interop-run-1-20260913.md` §N10b):
//!
//! - header registration (`x-wiretrustee-peer-id` →
//!   `x-wiretrustee-peer-registered: 1`, signal/server/signal.go:87,117,
//!   134-152);
//! - `ConnectStream` NEVER reads the request body — the v0.78.1 handler
//!   (signal.go:106-132) registers, confirms the header and blocks on
//!   `stream.Context().Done()`. The mock still DRAINS the body (tonic
//!   needs it consumed to observe disconnects) but only COUNTS the frames
//!   seen there (`stream_frames_seen`): a frame on the stream is never
//!   forwarded — exactly like the real server, where such frames are not
//!   even read;
//! - forwarding happens EXCLUSIVELY on the unary `Send` RPC
//!   (signal.go:95-104 → `forwardMessageToPeer`), by pure `remoteKey`
//!   lookup — the path the upstream client's signaler actually uses
//!   (`client/internal/peer/signaler.go:36-66`).
//!
//! plus the counting listener, protected-socket fd source and rcgen TLS
//! material reused from the N3-2/N3-7 test patterns. This module is NOT a
//! test target itself.

#![allow(dead_code)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use futures_core::Stream;
use prost::Message as _;
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use tokio::net::TcpListener;
use tokio_stream::wrappers::ReceiverStream;
use tonic::codegen::BoxStream;
use tonic::metadata::AsciiMetadataValue;
use tonic::transport::{Identity, ServerTlsConfig};
use tonic::{Request, Response, Status};

// std fd traits for the counting-listener dup handles
use std::os::fd::{AsRawFd, FromRawFd};

use netbird_core::envelope::{EnvelopeKeyPair, EnvelopePublicKey};
use netbird_core::grpc::{GrpcTlsConfig, GrpcTransport};
use netbird_core::mgmtsock::{mgmt_socket_open, ManagementSocketProvider, ProtectedSocketFdSource};
use netbird_core::signal::proto::signal_exchange_server::{SignalExchange, SignalExchangeServer};
use netbird_core::signal::proto::{Body, EncryptedMessage};
use netbird_core::signal::{HEADER_ID, HEADER_REGISTERED};
use netbird_core::signal::SignalClient;

// Host-test link surface (same as the other integration tests): the
// integration test binary links the whole crate rlib on the host triple,
// where libace_napi.z.so / libhilog_ndk.z.so do not exist. These no-ops
// satisfy the linker only; nothing below calls into them.
pub mod host_link_stubs {
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
        _data: *mut *mut c_void,
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
        _data: *mut *mut c_void,
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
// TLS material (rcgen, the N3-2 management_grpc.rs pattern)
// ---------------------------------------------------------------------------

pub struct TestCa {
    pub ca_pem: String,
    pub leaf_cert_pem: String,
    pub leaf_key_pem: String,
}

/// One fresh self-signed CA + a server leaf for 127.0.0.1/localhost, per
/// test (nothing shared between processes or runs).
pub fn make_test_ca(common_name: &str) -> TestCa {
    let ca_key = KeyPair::generate().expect("ca key");
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).expect("ca params");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    ca_params
        .distinguished_name
        .push(DnType::CommonName, common_name);
    let ca_cert = ca_params.self_signed(&ca_key).expect("ca cert");
    let issuer = Issuer::new(ca_params, ca_key);

    let leaf_key = KeyPair::generate().expect("leaf key");
    let mut leaf_params = CertificateParams::new(vec![
        "127.0.0.1".to_string(),
        "localhost".to_string(),
    ])
    .expect("leaf params");
    leaf_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    leaf_params
        .distinguished_name
        .push(DnType::CommonName, "127.0.0.1");
    let leaf_cert = leaf_params.signed_by(&leaf_key, &issuer).expect("leaf cert");

    TestCa {
        ca_pem: ca_cert.pem(),
        leaf_cert_pem: leaf_cert.pem(),
        leaf_key_pem: leaf_key.serialize_pem(),
    }
}

// ---------------------------------------------------------------------------
// counting listener (mgmt_socket.rs pattern): accept counter + transport kill
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct CountingListener {
    inner: Arc<TcpListener>,
    accepts: Arc<std::sync::atomic::AtomicUsize>,
    conns: Arc<Mutex<Vec<std::net::TcpStream>>>,
}

impl CountingListener {
    pub fn new(inner: TcpListener) -> Self {
        CountingListener {
            inner: Arc::new(inner),
            accepts: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            conns: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn accepted(&self) -> usize {
        self.accepts.load(Ordering::Acquire)
    }

    /// Kill every live accepted connection at the TRANSPORT level: the
    /// client's pooled connection dies, so its next RPC must re-dial (and
    /// therefore take a fresh protected socket from the provider).
    pub fn kill_all_connections(&self) {
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

// ---------------------------------------------------------------------------
// the mock SignalExchange server
// ---------------------------------------------------------------------------

type Registry = HashMap<String, tokio::sync::mpsc::Sender<Result<EncryptedMessage, Status>>>;

/// An in-process peer whose PRIVATE keys the mock holds: every
/// `EncryptedMessage` addressed to this peer is REALLY decrypted
/// (envelope open with the sender's public key) and the plaintext `Body`
/// fields recorded — the server-side proof that the sealed frames carry
/// what the sender intended.
pub struct SinkPeer {
    pub keys: EnvelopeKeyPair,
    pub records: Arc<Mutex<Vec<String>>>,
}

impl SinkPeer {
    pub fn new() -> Self {
        SinkPeer {
            keys: EnvelopeKeyPair::generate().expect("sink key pair"),
            records: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// A sink playing a SPECIFIC peer (its private keys), so the test can
    /// prove the mock decrypts exactly what a real peer would.
    pub fn for_keys(keys: EnvelopeKeyPair) -> Self {
        SinkPeer { keys, records: Arc::new(Mutex::new(Vec::new())) }
    }
}

#[derive(Clone)]
pub struct SignalMock {
    /// peer id → outbound half of its ConnectStream (upstream
    /// peer.Registry, signal/peer/peer.go:58-65).
    registry: Arc<Mutex<Registry>>,
    /// The identity headers seen on ConnectStream calls.
    pub seen_ids: Arc<Mutex<Vec<String>>>,
    /// Knob: reject ConnectStream with PermissionDenied (Auth-class
    /// termination test).
    pub deny_registration: Arc<AtomicBool>,
    /// Knob: reject ConnectStream with Unavailable (retryable — the backoff
    /// exhaustion test; maps to the Network class like upstream
    /// `Unavailable`, grpc.go:578-580).
    pub fail_registration_unavailable: Arc<AtomicBool>,
    /// Knob: skip the `x-wiretrustee-peer-registered` confirm header
    /// (fail-closed registration test; upstream always sends it,
    /// signal.go:117).
    pub omit_registered_header: Arc<AtomicBool>,
    /// Knob: flip a byte in every forwarded `body` (envelope authentication
    /// must fail client-side WITHOUT killing the stream).
    pub corrupt_forwarded_bodies: Arc<AtomicBool>,
    /// Frames the mock observed ON the `ConnectStream` request body. The
    /// v0.78.1 server never reads them (never forwards them either) — a
    /// client that still pushes frames onto the stream body shows up here
    /// while the peer starves (the N10b regression signature).
    stream_frames_seen: Arc<AtomicU64>,
    /// Frames delivered through the unary `Send` RPC — the ONLY path the
    /// real server forwards (signal.go:95-104).
    unary_sends: Arc<AtomicU64>,
    /// The decrypting sink peer (optionally also registered as a
    /// forwarding destination under its public key).
    pub sink: Arc<Mutex<Option<SinkPeer>>>,
}

impl Default for SignalMock {
    fn default() -> Self {
        SignalMock {
            registry: Arc::new(Mutex::new(HashMap::new())),
            seen_ids: Arc::new(Mutex::new(Vec::new())),
            deny_registration: Arc::new(AtomicBool::new(false)),
            fail_registration_unavailable: Arc::new(AtomicBool::new(false)),
            omit_registered_header: Arc::new(AtomicBool::new(false)),
            corrupt_forwarded_bodies: Arc::new(AtomicBool::new(false)),
            stream_frames_seen: Arc::new(AtomicU64::new(0)),
            unary_sends: Arc::new(AtomicU64::new(0)),
            sink: Arc::new(Mutex::new(None)),
        }
    }
}

impl SignalMock {
    /// Registrations observed so far (identity headers).
    pub fn registered_ids(&self) -> Vec<String> {
        self.seen_ids.lock().expect("seen_ids").clone()
    }

    /// Frames seen ON the stream body (never forwarded — real-server shape).
    pub fn stream_frames_seen(&self) -> u64 {
        self.stream_frames_seen.load(Ordering::Acquire)
    }

    /// Frames delivered via the unary `Send` RPC (the forwarding path).
    pub fn unary_sends(&self) -> u64 {
        self.unary_sends.load(Ordering::Acquire)
    }

    fn record_if_sink(&self, env: &EncryptedMessage) {
        let guard = self.sink.lock().expect("sink");
        let Some(sink) = guard.as_ref() else { return };
        if env.remote_key != sink.keys.public_key_base64() {
            return;
        }
        let Ok(sender) = EnvelopePublicKey::from_base64(&env.key) else { return };
        // a REAL open with the sink's private key + the sender's public key
        if let Ok(plain) = netbird_core::envelope::open(&sender, &sink.keys, &env.body) {
            if let Ok(body) = Body::decode(plain.as_slice()) {
                sink.records.lock().expect("records").push(format!(
                    "{}|{}|{}|{}",
                    body.r#type, body.payload, body.wg_listen_port, body.net_bird_version
                ));
            }
        }
    }

    fn forward(&self, env: &EncryptedMessage) {
        let mut env = env.clone();
        if self.corrupt_forwarded_bodies.load(Ordering::SeqCst) && !env.body.is_empty() {
            let last = env.body.len() - 1;
            env.body[last] ^= 0x01;
        }
        let dest = self
            .registry
            .lock()
            .expect("registry")
            .get(&env.remote_key)
            .cloned();
        if let Some(dest) = dest {
            // best-effort like upstream forwardMessageToPeer (failures are
            // counted, never fatal, signal.go:161-209)
            let _ = dest.try_send(Ok(env));
        }
    }
}

#[tonic::async_trait]
impl SignalExchange for SignalMock {
    async fn send(
        &self,
        request: Request<EncryptedMessage>,
    ) -> Result<Response<EncryptedMessage>, Status> {
        let env = request.into_inner();
        self.unary_sends.fetch_add(1, Ordering::AcqRel);
        self.record_if_sink(&env);
        self.forward(&env);
        // upstream returns an EMPTY EncryptedMessage (signal.go:100)
        Ok(Response::new(EncryptedMessage::default()))
    }

    async fn connect_stream(
        &self,
        request: Request<tonic::Streaming<EncryptedMessage>>,
    ) -> Result<Response<BoxStream<EncryptedMessage>>, Status> {
        if self.deny_registration.load(Ordering::SeqCst) {
            return Err(Status::permission_denied("signal registration denied"));
        }
        if self.fail_registration_unavailable.load(Ordering::SeqCst) {
            return Err(Status::unavailable("signal service down (test)"));
        }
        // registration = the identity header (upstream RegisterPeer,
        // signal.go:134-140; missing header → FailedPrecondition)
        let id = match request.metadata().get(HEADER_ID) {
            Some(v) => v
                .to_str()
                .map_err(|_| Status::invalid_argument("non-ascii peer id"))?
                .to_string(),
            None => {
                return Err(Status::failed_precondition(format!(
                    "missing connection header: {HEADER_ID}"
                )));
            }
        };
        self.seen_ids.lock().expect("seen_ids").push(id.clone());

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<EncryptedMessage, Status>>(64);
        self.registry.lock().expect("registry").insert(id.clone(), tx);

        // v0.78.1 ConnectStream shape (signal.go:106-132): the request body
        // is NEVER forwarded from — drained only to (a) COUNT stray frames
        // (a client that still pushes the stream is caught by the
        // `stream_frames_seen` regression assertion) and (b) keep the
        // registry entry alive for exactly the stream's lifetime. Any frame
        // seen here would be silently black-holed by the real server.
        let mut inbound = request.into_inner();
        let forward_self = self.clone();
        tokio::spawn(async move {
            while let Ok(Some(_env)) = inbound.message().await {
                forward_self.stream_frames_seen.fetch_add(1, Ordering::AcqRel);
            }
            forward_self
                .registry
                .lock()
                .expect("registry")
                .remove(&id);
        });

        let out: BoxStream<EncryptedMessage> = Box::pin(ReceiverStream::new(rx));
        let mut resp = Response::new(out);
        if !self.omit_registered_header.load(Ordering::SeqCst) {
            resp.metadata_mut()
                .insert(HEADER_REGISTERED, AsciiMetadataValue::from_static("1"));
        }
        Ok(resp)
    }
}

// ---------------------------------------------------------------------------
// spawn + client helpers
// ---------------------------------------------------------------------------

fn parse_open_fd() -> i32 {
    // `mgmt_socket_open()` JSON is `{"fd":N,"bind_rc":R,"bind_errno":E}`;
    // integration tests only get the STRING surface, so extract the fd
    // number directly (the strict reader is crate-internal).
    let open = mgmt_socket_open();
    let i = open.find("\"fd\":").expect("fd key") + "\"fd\":".len();
    let rest = &open[i..];
    let end = rest.find(',').unwrap_or(rest.find('}').unwrap_or(rest.len()));
    rest[..end].parse::<i32>().expect("fd number")
}

/// Bind + spawn the mock signal server; returns (addr, counting listener).
pub async fn spawn_signal_mock(svc: SignalMock) -> (SocketAddr, CountingListener) {
    spawn_over(svc, None).await
}

/// Same over a REAL TLS transport (rcgen CA/leaf; the client trusts exactly
/// `ca.ca_pem`).
pub async fn spawn_signal_mock_tls(svc: SignalMock, ca: &TestCa) -> (SocketAddr, CountingListener) {
    let tls = ServerTlsConfig::new().identity(Identity::from_pem(
        ca.leaf_cert_pem.clone(),
        ca.leaf_key_pem.clone(),
    ));
    spawn_over(svc, Some(tls)).await
}

async fn spawn_over(
    svc: SignalMock,
    tls: Option<ServerTlsConfig>,
) -> (SocketAddr, CountingListener) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let counter = CountingListener::new(listener);
    let task_counter = counter.clone();
    tokio::spawn(async move {
        let serve = if let Some(tls) = tls {
            tonic::transport::Server::builder()
                .tls_config(tls)
                .expect("server tls config")
                .add_service(SignalExchangeServer::new(svc))
                .serve_with_incoming(task_counter)
                .await
        } else {
            tonic::transport::Server::builder()
                .add_service(SignalExchangeServer::new(svc))
                .serve_with_incoming(task_counter)
                .await
        };
        serve.expect("tonic serve");
    });
    (addr, counter)
}

/// A `SignalClient` over the protected-socket seam (the ONLY constructor
/// that exists), plus a fresh fd source seeded with `count` protected
/// sockets (pre-opened, unconnected, bound — the production shell shape).
pub fn socket_source(count: usize) -> Arc<ProtectedSocketFdSource> {
    let source = ProtectedSocketFdSource::new_with_fd(-1);
    for _ in 0..count {
        source.feed(parse_open_fd());
    }
    Arc::new(source)
}

/// Feed ONE more pre-opened (unconnected, bound) TCP socket into an
/// existing source — the N5d "shell feeds the starved queue" contract.
pub fn feed_tcp_fd(source: &ProtectedSocketFdSource) {
    source.feed(parse_open_fd());
}

/// Which transport a [`connect_client`] call uses.
pub enum TestTransport<'a> {
    /// Plaintext h2 (`http://`) — dev/test only.
    Plain,
    /// Real TLS (`https://`), trusting exactly the given PEM CA — the
    /// injected-trust-root contract (no system store).
    Tls(&'a str),
}

/// A `SignalClient` over the protected-socket seam (the ONLY constructor
/// that exists).
pub async fn connect_client(
    transport: TestTransport<'_>,
    addr: SocketAddr,
    keys: EnvelopeKeyPair,
    source: Arc<dyn ManagementSocketProvider>,
) -> SignalClient {
    let (endpoint, transport) = match transport {
        TestTransport::Plain => {
            (format!("http://{addr}"), GrpcTransport::Plaintext)
        }
        TestTransport::Tls(ca_pem) => (
            format!("https://{addr}"),
            GrpcTransport::Tls(GrpcTlsConfig::new(vec![ca_pem.as_bytes().to_vec()])),
        ),
    };
    SignalClient::connect_with_socket_source(
        &endpoint,
        transport,
        Duration::from_secs(5),
        Duration::from_secs(5),
        keys,
        source,
        addr,
    )
    .await
    .expect("protected signal connect")
}

/// Bounded wait until `cond()` holds; panics with `what` on deadline (no
/// sleep-as-assertion: the condition itself is the assertion).
pub async fn wait_for<F: Fn() -> bool>(what: &'static str, cond: F) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !cond() {
        assert!(
            std::time::Instant::now() <= deadline,
            "deadline waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}
