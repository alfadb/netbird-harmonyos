// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N3-2/N3-3 gRPC Login tests for `netbird_core::grpc` (+ the TLS REST bridge).
//!
//! An in-process tonic server implements `ManagementService/GetServerKey` +
//! `Login` and serves over REAL TLS (self-signed rcgen CA + leaf certificate;
//! the client trusts exactly the generated CA — injected trust root, no
//! system store). N3-3 adds the real message-body envelope: the server holds
//! a NaCl key pair, `GetServerKey` advertises its public key, and `Login`
//! REALLY decrypts the sealed request body and seals the reply (the upstream
//! `grpc.go login()` contract). Covered cases:
//!
//! 1. encrypted login success: server-side real decrypt + field assertions
//!    (setupKey, PeerKeys.wgPubKey incl. the bytes-of-base64-string quirk,
//!    PeerSystemMeta), response sealed back, client-side open + decode
//!    (peer address + 3-state sessionExpiresAt)
//! 2. default body mode is ENCRYPTED: against a plaintext-body-only mock,
//!    `login()` must fail (ciphertext is not a `LoginRequest`) while the
//!    explicit `LoginBodyMode::Plaintext` debug mode succeeds
//! 3. wrong server public key advertised by GetServerKey → server cannot
//!    decrypt → `Server`
//! 4. reply sealed for the wrong key → client open fails → `Parse`
//! 5. tampered reply ciphertext → client open fails → `Parse`
//! 6. GetServerKey returns 5xx → `Server`; GetServerKey past the deadline
//!    → `Timeout`; malformed key string → `Parse`
//! 7. legacy N3-2 status classes over the encrypted path:
//!    PermissionDenied → `Auth`, InvalidArgument → `Request`,
//!    Internal → `Server`, Unavailable → `Network`
//! 8. per-request deadline exceeded on Login → `Timeout`
//! 9. untrusted CA → handshake fails → `Network`; connect refused →
//!    `Network`; URL/transport mismatches → `UnsupportedUrl`
//! 10. full bridge: gRPC Login over TLS, then `GET /api/peers` over a real
//!     TLS HTTP/1.1 endpoint with the session JWT (`Bearer`)
//!
//! Envelope wire contract under test (`src/envelope.rs`): request body =
//! `nonce(24B) || XSalsa20-Poly1305(protobuf LoginRequest)` sealed for the
//! server public key; reply body symmetric for `LoginResponse`; envelope
//! `wgPubKey` + `PeerKeys.wgPubKey` both derive from the client identity key
//! pair injected at `connect()`.

use netbird_core::envelope::{EnvelopeKeyPair, EnvelopePublicKey};
use netbird_core::grpc::proto::management_service_server::{ManagementService, ManagementServiceServer};
use netbird_core::grpc::proto::{
    EncryptedMessage, LoginRequest, LoginResponse, PeerConfig, ServerKeyResponse,
};
use netbird_core::grpc::{
    map_grpc_status, GrpcTlsConfig, GrpcTransport, LoginBodyMode, LoginParams, ManagementGrpcClient,
    PeerKeySet, PeerMeta,
};
use netbird_core::management::{
    decode_pem_certificates, Credential, ManagementClient, ManagementError,
};
use prost::Message as _;
use prost_types::Timestamp;
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status};

// Host-test link surface (same as tests/management_mock.rs): the integration
// test binary links the whole crate rlib on the host triple, where
// libace_napi.z.so / libhilog_ndk.z.so do not exist. These no-ops satisfy the
// linker only; nothing below calls into them.
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
        _this_arg: *mut *mut c_void,
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
// TLS material (rcgen, ring backend)
// ---------------------------------------------------------------------------

struct TestCa {
    ca_pem: String,
    leaf_cert_pem: String,
    leaf_key_pem: String,
}

/// One fresh self-signed CA + a server leaf for 127.0.0.1/localhost, per test
/// (nothing shared between processes or runs).
fn make_test_ca(common_name: &str) -> TestCa {
    let ca_key = KeyPair::generate().expect("ca key");
    let mut ca_params =
        CertificateParams::new(Vec::<String>::new()).expect("ca params");
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
// in-process tonic management server
// ---------------------------------------------------------------------------

/// What the server saw on the wire for one Login call.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CapturedLogin {
    envelope_wg_pub_key: String,
    setup_key: String,
    jwt_token: String,
    peer_keys_wg_pub_key: Vec<u8>,
    meta_hostname: String,
}

/// The server side of the envelope (N3-3): a NaCl key pair whose public key
/// `GetServerKey` advertises, plus misbehavior knobs for the negative tests.
#[derive(Clone)]
struct ServerMock {
    keys: EnvelopeKeyPair,
    /// `GetServerKey` returns this raw string instead of the real key
    /// (misconfigured/rotating server).
    advertise_key: Option<String>,
    /// Seal the reply for this key instead of the requesting client's key
    /// (reply the client cannot open).
    reply_for: Option<EnvelopePublicKey>,
    /// Flip the last ciphertext byte of the reply (wire corruption).
    corrupt_reply: bool,
}

impl Default for ServerMock {
    fn default() -> Self {
        ServerMock {
            keys: EnvelopeKeyPair::generate().expect("server key pair"),
            advertise_key: None,
            reply_for: None,
            corrupt_reply: false,
        }
    }
}

#[derive(Clone)]
struct MockManagement {
    /// Server envelope identity. `None` = legacy N3-2 plaintext-body
    /// contract (raw protobuf bodies) — used by the "default mode is
    /// encrypted" proof and the explicit debug-mode test. Such a server
    /// still answers GetServerKey (via `plaintext_contract_keys`); only its
    /// Login body handling stays raw.
    server: Option<ServerMock>,
    /// Decoded LoginRequest per call (asserted client-side afterwards).
    captured: Arc<Mutex<Vec<CapturedLogin>>>,
    /// When set, the Login handler replies with this gRPC status before any
    /// body handling.
    fail_with: Option<Status>,
    /// Success response (sealed/encoded into the reply envelope body).
    response: Option<LoginResponse>,
    /// Sleep before answering Login (deadline tests).
    delay: Option<Duration>,
    /// Status returned by GetServerKey instead of the server key.
    get_server_key_fail: Option<Status>,
    /// Sleep before answering GetServerKey (deadline tests).
    get_server_key_delay: Option<Duration>,
    /// Identity advertised by the plaintext-contract server.
    plaintext_contract_keys: EnvelopeKeyPair,
}

impl Default for MockManagement {
    fn default() -> Self {
        MockManagement {
            server: None,
            captured: Arc::new(Mutex::new(Vec::new())),
            fail_with: None,
            response: None,
            delay: None,
            get_server_key_fail: None,
            get_server_key_delay: None,
            plaintext_contract_keys: EnvelopeKeyPair::generate().expect("mock server key pair"),
        }
    }
}

impl MockManagement {
    /// Encrypted-contract server (the upstream shape).
    fn encrypted(response: LoginResponse) -> Self {
        MockManagement { server: Some(ServerMock::default()), response: Some(response), ..Default::default() }
    }
    /// Legacy plaintext-body server (N3-2 contract, debug mode only).
    fn plaintext_contract(response: LoginResponse) -> Self {
        MockManagement { response: Some(response), ..Default::default() }
    }
    fn failing(status: Status) -> Self {
        MockManagement {
            fail_with: Some(status),
            server: Some(ServerMock::default()),
            ..Default::default()
        }
    }
    fn captured_login(&self) -> CapturedLogin {
        self.captured
            .lock()
            .expect("captured lock")
            .first()
            .cloned()
            .expect("one Login call captured")
    }
}

#[tonic::async_trait]
impl ManagementService for MockManagement {
    async fn login(
        &self,
        request: Request<EncryptedMessage>,
    ) -> Result<Response<EncryptedMessage>, Status> {
        if let Some(delay) = self.delay {
            tokio::time::sleep(delay).await;
        }
        if let Some(status) = &self.fail_with {
            return Err(status.clone());
        }
        let envelope = request.into_inner();
        let response = self.response.clone().unwrap_or_default();
        let reply_body = match &self.server {
            None => {
                // Legacy N3-2 contract: body is the raw LoginRequest.
                let req = LoginRequest::decode(envelope.body.as_slice())
                    .map_err(|e| Status::invalid_argument(format!("body: {e}")))?;
                self.captured.lock().expect("captured lock").push(CapturedLogin {
                    envelope_wg_pub_key: envelope.wg_pub_key.clone(),
                    setup_key: req.setup_key.clone(),
                    jwt_token: req.jwt_token.clone(),
                    peer_keys_wg_pub_key: req.peer_keys.as_ref().map(|k| k.wg_pub_key.clone()).unwrap_or_default(),
                    meta_hostname: req.meta.as_ref().map(|m| m.hostname.clone()).unwrap_or_default(),
                });
                response.encode_to_vec()
            }
            Some(server) => {
                // Upstream contract: open the sealed body with the CLIENT
                // public key named on the envelope + OUR secret.
                let client_pk = EnvelopePublicKey::from_base64(&envelope.wg_pub_key)
                    .map_err(|_| Status::invalid_argument("envelope wgPubKey is not a NaCl key"))?;
                let plaintext = netbird_core::envelope::open(&client_pk, &server.keys, &envelope.body)
                    .map_err(|_| Status::internal("cannot decrypt login request body"))?;
                let req = LoginRequest::decode(plaintext.as_slice())
                    .map_err(|e| Status::invalid_argument(format!("decrypted body: {e}")))?;
                self.captured.lock().expect("captured lock").push(CapturedLogin {
                    envelope_wg_pub_key: envelope.wg_pub_key.clone(),
                    setup_key: req.setup_key.clone(),
                    jwt_token: req.jwt_token.clone(),
                    peer_keys_wg_pub_key: req.peer_keys.as_ref().map(|k| k.wg_pub_key.clone()).unwrap_or_default(),
                    meta_hostname: req.meta.as_ref().map(|m| m.hostname.clone()).unwrap_or_default(),
                });
                let reply_peer = server.reply_for.clone().unwrap_or(client_pk);
                let mut body = netbird_core::envelope::seal(&reply_peer, &server.keys, &response.encode_to_vec())
                    .map_err(|_| Status::internal("cannot seal login response"))?;
                if server.corrupt_reply {
                    let last = body.len() - 1;
                    body[last] ^= 0x01;
                }
                body
            }
        };
        Ok(Response::new(EncryptedMessage {
            wg_pub_key: envelope.wg_pub_key,
            body: reply_body,
            version: envelope.version,
        }))
    }

    async fn get_server_key(
        &self,
        _request: Request<netbird_core::grpc::proto::Empty>,
    ) -> Result<Response<ServerKeyResponse>, Status> {
        if let Some(delay) = self.get_server_key_delay {
            tokio::time::sleep(delay).await;
        }
        if let Some(status) = &self.get_server_key_fail {
            return Err(status.clone());
        }
        let server = self
            .server
            .as_ref()
            .map(|s| (s.advertise_key.clone().unwrap_or_else(|| s.keys.public_key_base64())))
            .unwrap_or_else(|| self.plaintext_contract_keys.public_key_base64());
        Ok(Response::new(ServerKeyResponse { key: server, expires_at: None, version: 0 }))
    }
}

/// Spawn the mock management service over REAL TLS on 127.0.0.1:<ephemeral>.
async fn spawn_tls_management(ca: &TestCa, svc: MockManagement) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let identity = tonic::transport::Identity::from_pem(
        ca.leaf_cert_pem.clone(),
        ca.leaf_key_pem.clone(),
    );
    let tls = tonic::transport::ServerTlsConfig::new().identity(identity);
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .tls_config(tls)
            .expect("server tls config")
            .add_service(ManagementServiceServer::new(svc))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .expect("tonic serve");
    });
    addr
}

// Hang guards for the happy-path TLS connects/requests (loopback answers in
// microseconds). NOT latency assertions — no test relies on these firing;
// the timeout-mapping tests below pass their own tighter deadlines (300ms).
// Generous (60s) so a fully loaded parallel `cargo test` run never trips
// them spuriously.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Fresh client identity per test (upstream: one per device profile).
fn client_keys() -> EnvelopeKeyPair {
    EnvelopeKeyPair::generate().expect("client key pair")
}

async fn connect_tls(ca: &TestCa, addr: SocketAddr) -> ManagementGrpcClient {
    connect_tls_with(ca, addr, client_keys()).await
}

async fn connect_tls_with(
    ca: &TestCa,
    addr: SocketAddr,
    keys: EnvelopeKeyPair,
) -> ManagementGrpcClient {
    let endpoint = format!("https://{addr}");
    ManagementGrpcClient::connect(
        &endpoint,
        GrpcTransport::Tls(GrpcTlsConfig::new(vec![ca.ca_pem.clone().into_bytes()])),
        CONNECT_TIMEOUT,
        REQUEST_TIMEOUT,
        keys,
    )
    .await
    .expect("tls connect")
}

fn login_params(setup_key: &str) -> LoginParams {
    LoginParams {
        setup_key: setup_key.into(),
        jwt_token: String::new(),
        // On the encrypted path this caller-set value is overridden by the
        // client identity keys (upstream grpc.go:644 — single source c.key);
        // the sentinel proves the override happened.
        peer_keys: PeerKeySet {
            wg_pub_key: b"BASE64WGKEY-IGNORED-BY-LOGIN".to_vec(),
            ssh_pub_key: Vec::new(),
        },
        meta: PeerMeta {
            hostname: "ohos-device".into(),
            os_name: "harmonyos".into(),
            os_version: "26.0.0".into(),
            netbird_version: "0.1.0".into(),
        },
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn login_encrypted_envelope_server_really_decrypts_and_replies() {
    let ca = make_test_ca("n3-3 test CA");
    let keys = client_keys();
    let client_pk_b64 = keys.public_key_base64();
    let svc = MockManagement::encrypted(LoginResponse {
        peer_config: Some(PeerConfig {
            address: "10.64.0.7".into(),
            ..Default::default()
        }),
        session_expires_at: Some(Timestamp {
            seconds: 1_893_456_000, // fixed absolute deadline (2030-01-01)
            nanos: 0,
        }),
        ..Default::default()
    });
    let captured = svc.clone();
    let addr = spawn_tls_management(&ca, svc).await;

    let mut client = connect_tls_with(&ca, addr, keys).await;
    let outcome = client
        .login(login_params("n3-3-setup-key-ok"))
        .await
        .expect("login ok");

    // response mapping (opened + decoded client-side)
    assert_eq!(outcome.peer_address.as_deref(), Some("10.64.0.7"));
    assert_eq!(outcome.session_deadline_unix, Some(1_893_456_000));

    // request fields as REALLY decrypted by the server
    let seen = captured.captured_login();
    assert_eq!(seen.setup_key, "n3-3-setup-key-ok");
    assert_eq!(seen.envelope_wg_pub_key, client_pk_b64);
    assert_eq!(
        seen.peer_keys_wg_pub_key,
        client_pk_b64.clone().into_bytes(),
        "PeerKeys.wgPubKey = bytes of the base64 string, from the injected keys (upstream quirk)"
    );
    assert_eq!(seen.meta_hostname, "ohos-device");
    assert_eq!(seen.jwt_token, "");
}

/// The default body mode must be the ENCRYPTED path: against a mock that
/// only understands the N3-2 plaintext-body contract, the default `login()`
/// must fail (its sealed body is not decodable as LoginRequest), while the
/// explicit debug mode succeeds on the same server.
#[tokio::test(flavor = "multi_thread")]
async fn login_default_mode_is_encrypted_not_plaintext() {
    let ca = make_test_ca("n3-3 test CA");
    let svc = MockManagement::plaintext_contract(LoginResponse::default());
    let addr = spawn_tls_management(&ca, svc).await;

    let mut client = connect_tls(&ca, addr).await;
    let err = client.login(login_params("k")).await.expect_err("default login must fail on a plaintext-only server");
    assert!(
        matches!(err, ManagementError::Request { status: 3, .. }),
        "sealed body must be rejected by the plaintext mock as InvalidArgument, got {err:?}"
    );

    // explicit debug mode (NOT upstream behavior) works on that same server
    let mut dbg = connect_tls(&ca, addr).await;
    dbg.login_with_mode(login_params("k"), LoginBodyMode::Plaintext)
        .await
        .expect("explicit plaintext mode reaches the legacy contract");
}

/// GetServerKey advertising a WRONG server public key (rotation/misconfig):
/// the client seals for that key, the real server cannot open it.
#[tokio::test(flavor = "multi_thread")]
async fn login_wrong_server_public_key_fails_at_the_server() {
    let ca = make_test_ca("n3-3 test CA");
    let mut svc = MockManagement::encrypted(LoginResponse::default());
    let impostor = EnvelopeKeyPair::generate().unwrap();
    if let Some(server) = svc.server.as_mut() {
        server.advertise_key = Some(impostor.public_key_base64());
    }
    let addr = spawn_tls_management(&ca, svc).await;

    let mut client = connect_tls(&ca, addr).await;
    let err = client.login(login_params("k")).await.expect_err("must fail");
    assert!(
        matches!(err, ManagementError::Server { status: 13, .. }),
        "server-side decrypt failure surfaces as Internal → Server, got {err:?}"
    );
}

/// Reply sealed for a key the client does not hold: client-side open fails
/// (Parse), not a crash and not a silent garbage decode.
#[tokio::test(flavor = "multi_thread")]
async fn login_reply_sealed_for_wrong_key_fails_to_open() {
    let ca = make_test_ca("n3-3 test CA");
    let mut svc = MockManagement::encrypted(LoginResponse::default());
    let decoy = EnvelopeKeyPair::generate().unwrap();
    if let Some(server) = svc.server.as_mut() {
        server.reply_for = Some(EnvelopePublicKey::from_bytes(&decoy.public_key_bytes()));
    }
    let addr = spawn_tls_management(&ca, svc).await;

    let mut client = connect_tls(&ca, addr).await;
    let err = client.login(login_params("k")).await.expect_err("must fail");
    assert!(matches!(err, ManagementError::Parse(_)), "got {err:?}");
}

/// One flipped ciphertext bit in the reply → authenticated encryption must
/// reject it (Poly1305 tag mismatch), surfacing as Parse.
#[tokio::test(flavor = "multi_thread")]
async fn login_tampered_reply_body_fails_authentication() {
    let ca = make_test_ca("n3-3 test CA");
    let mut svc = MockManagement::encrypted(LoginResponse::default());
    if let Some(server) = svc.server.as_mut() {
        server.corrupt_reply = true;
    }
    let addr = spawn_tls_management(&ca, svc).await;

    let mut client = connect_tls(&ca, addr).await;
    let err = client.login(login_params("k")).await.expect_err("must fail");
    assert!(matches!(err, ManagementError::Parse(_)), "got {err:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn get_server_key_5xx_maps_to_server() {
    let ca = make_test_ca("n3-3 test CA");
    let mut svc = MockManagement::encrypted(LoginResponse::default());
    svc.get_server_key_fail = Some(Status::internal("key store down"));
    let addr = spawn_tls_management(&ca, svc).await;

    let mut client = connect_tls(&ca, addr).await;
    let err = client.get_server_key().await.expect_err("must fail");
    assert!(
        matches!(err, ManagementError::Server { status: 13, .. }),
        "got {err:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn get_server_key_past_deadline_maps_to_timeout() {
    let ca = make_test_ca("n3-3 test CA");
    let mut svc = MockManagement::encrypted(LoginResponse::default());
    svc.get_server_key_delay = Some(Duration::from_secs(2));
    let addr = spawn_tls_management(&ca, svc).await;

    let endpoint = format!("https://{addr}");
    let mut client = ManagementGrpcClient::connect(
        &endpoint,
        GrpcTransport::Tls(GrpcTlsConfig::new(vec![ca.ca_pem.clone().into_bytes()])),
        CONNECT_TIMEOUT,
        Duration::from_millis(300), // request deadline < server delay
        client_keys(),
    )
    .await
    .expect("tls connect");

    let err = client.get_server_key().await.expect_err("must fail");
    assert_eq!(err, ManagementError::Timeout, "got {err:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn get_server_key_malformed_key_string_maps_to_parse() {
    let ca = make_test_ca("n3-3 test CA");
    let mut svc = MockManagement::encrypted(LoginResponse::default());
    if let Some(server) = svc.server.as_mut() {
        server.advertise_key = Some("this-is-not-base64!!".into());
    }
    let addr = spawn_tls_management(&ca, svc).await;

    let mut client = connect_tls(&ca, addr).await;
    let err = client.get_server_key().await.expect_err("must fail");
    assert!(matches!(err, ManagementError::Parse(_)), "got {err:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn login_invalid_setup_key_maps_to_auth() {
    let ca = make_test_ca("n3-2 test CA");
    let addr = spawn_tls_management(&ca, MockManagement::failing(Status::permission_denied("invalid setup key"))).await;
    let mut client = connect_tls(&ca, addr).await;

    let err = client
        .login(login_params("wrong-key"))
        .await
        .expect_err("must fail");
    assert_eq!(
        err,
        ManagementError::Auth { status: 7, message: "invalid setup key".into() },
        "got: {err:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn login_invalid_argument_maps_to_request() {
    let ca = make_test_ca("n3-2 test CA");
    let addr = spawn_tls_management(&ca, MockManagement::failing(Status::invalid_argument("bad meta"))).await;
    let mut client = connect_tls(&ca, addr).await;

    let err = client.login(login_params("k")).await.expect_err("must fail");
    assert!(
        matches!(err, ManagementError::Request { status: 3, .. }),
        "got: {err:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn login_internal_error_maps_to_server() {
    let ca = make_test_ca("n3-2 test CA");
    let addr = spawn_tls_management(&ca, MockManagement::failing(Status::internal("db down"))).await;
    let mut client = connect_tls(&ca, addr).await;

    let err = client.login(login_params("k")).await.expect_err("must fail");
    assert!(
        matches!(err, ManagementError::Server { status: 13, .. }),
        "got: {err:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn login_unavailable_maps_to_network() {
    let ca = make_test_ca("n3-2 test CA");
    let addr = spawn_tls_management(&ca, MockManagement::failing(Status::unavailable("temporarily down"))).await;
    let mut client = connect_tls(&ca, addr).await;

    let err = client.login(login_params("k")).await.expect_err("must fail");
    assert!(matches!(err, ManagementError::Network(_)), "got: {err:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn login_past_deadline_maps_to_timeout() {
    let ca = make_test_ca("n3-2 test CA");
    let svc = MockManagement {
        delay: Some(Duration::from_secs(2)),
        ..MockManagement::encrypted(LoginResponse::default())
    };
    let addr = spawn_tls_management(&ca, svc).await;

    let endpoint = format!("https://{addr}");
    let mut client = ManagementGrpcClient::connect(
        &endpoint,
        GrpcTransport::Tls(GrpcTlsConfig::new(vec![ca.ca_pem.clone().into_bytes()])),
        CONNECT_TIMEOUT,
        Duration::from_millis(300), // request deadline < server delay
        client_keys(),
    )
    .await
    .expect("tls connect");

    let err = client.login(login_params("k")).await.expect_err("must fail");
    assert_eq!(err, ManagementError::Timeout, "got: {err:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn untrusted_ca_fails_the_handshake() {
    let server_ca = make_test_ca("n3-2 server CA");
    let other_ca = make_test_ca("n3-2 OTHER CA (not trusted by the client)");
    let addr = spawn_tls_management(&server_ca, MockManagement::encrypted(LoginResponse::default())).await;

    // client only trusts a DIFFERENT CA
    let endpoint = format!("https://{addr}");
    let err = ManagementGrpcClient::connect(
        &endpoint,
        GrpcTransport::Tls(GrpcTlsConfig::new(vec![other_ca.ca_pem.into_bytes()])),
        CONNECT_TIMEOUT,
        REQUEST_TIMEOUT,
        client_keys(),
    )
    .await
    .err()
    .expect("connect must fail on untrusted CA");
    assert!(matches!(err, ManagementError::Network(_)), "got: {err:?}");
}

#[tokio::test]
async fn connect_refused_maps_to_network() {
    // port 1 on loopback: nothing listens there
    let endpoint = "http://127.0.0.1:1";
    let err = ManagementGrpcClient::connect(
        endpoint,
        GrpcTransport::Plaintext,
        Duration::from_millis(500),
        Duration::from_millis(500),
        client_keys(),
    )
    .await
    .err()
    .expect("connect must fail");
    assert!(matches!(err, ManagementError::Network(_)), "got: {err:?}");
}

#[tokio::test]
async fn plaintext_transport_rejects_https_endpoint() {
    let err = ManagementGrpcClient::connect(
        "https://127.0.0.1:1",
        GrpcTransport::Plaintext,
        Duration::from_millis(500),
        Duration::from_millis(500),
        client_keys(),
    )
    .await
    .err()
    .expect("https without TLS config must be rejected");
    assert!(matches!(err, ManagementError::UnsupportedUrl(_)), "got: {err:?}");
}

#[tokio::test]
async fn tls_transport_rejects_http_endpoint() {
    let err = ManagementGrpcClient::connect(
        "http://127.0.0.1:1",
        GrpcTransport::Tls(GrpcTlsConfig::new(vec![b"-----BEGIN CERTIFICATE-----\n".to_vec()])),
        Duration::from_millis(500),
        Duration::from_millis(500),
        client_keys(),
    )
    .await
    .err()
    .expect("http with TLS config must be rejected");
    assert!(matches!(err, ManagementError::UnsupportedUrl(_)), "got: {err:?}");
}

// ---------------------------------------------------------------------------
// REST-over-TLS bridge: gRPC Login, then GET /api/peers with the JWT
// ---------------------------------------------------------------------------

mod rest_tls {
    use super::*;

    /// TLS HTTP/1.1 server (loop, one connection per request): asserts the
    /// Authorization header on every request and answers the upstream paths
    /// the N3-1 client actually calls (`/api/users/current` for session
    /// verification, `/api/peers` for the peer list).
    fn spawn_tls_rest(ca: &TestCa, expected_authorization: &'static str) -> SocketAddr {
        use rustls::pki_types::pem::PemObject as _;
        use rustls::pki_types::{CertificateDer, PrivateKeyDer};
        use rustls::{ServerConfig, ServerConnection, StreamOwned};
        use std::io::{Read as _, Write as _};
        use std::net::TcpListener;
        use std::sync::Arc as StdArc;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let certs: Vec<CertificateDer<'static>> =
            CertificateDer::pem_slice_iter(ca.leaf_cert_pem.as_bytes())
                .collect::<Result<_, _>>()
                .expect("leaf pem");
        let key = PrivateKeyDer::from_pem_slice(ca.leaf_key_pem.as_bytes()).expect("leaf key pem");
        let config = ServerConfig::builder_with_provider(
            rustls::crypto::ring::default_provider().into(),
        )
        .with_safe_default_protocol_versions()
        .expect("protocol versions")
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("server cert");

        thread::spawn(move || {
            let config = StdArc::new(config);
            // one connection per request (client sends `Connection: close`)
            for stream in listener.incoming() {
                let Ok(sock) = stream else { break };
                let Ok(conn) = ServerConnection::new(config.clone()) else { break };
                let mut tls = StreamOwned::new(conn, sock);

                let mut buf = [0u8; 8192];
                let n = tls.read(&mut buf).expect("read request");
                let request = String::from_utf8_lossy(&buf[..n]).into_owned();
                assert!(
                    request.contains(&format!("Authorization: {expected_authorization}")),
                    "missing/incorrect Authorization header: {request:?}"
                );

                let body: Vec<u8> = if request.starts_with("GET /api/users/current") {
                    br#"{"id":"user-1","name":"ohos-admin","email":"a@b.c","role":"admin"}"#.to_vec()
                } else if request.starts_with("GET /api/peers") {
                    br#"[{"id":"peer-1","name":"ohos-device","ip":"10.64.0.7","connected":true},
                        {"id":"peer-2","name":"stage-host","ip":"10.64.0.9"}]"#.to_vec()
                } else {
                    panic!("unexpected request: {request:?}")
                };
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                tls.write_all(head.as_bytes()).expect("write head");
                tls.write_all(&body).expect("write body");
            }
        });
        addr
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn grpc_login_then_rest_peers_over_two_real_tls_channels() {
        let ca = make_test_ca("n3-2 test CA");
        let addr = spawn_tls_management(&ca, MockManagement::encrypted(LoginResponse::default())).await;

        // 1) gRPC Login over TLS — SSO variant: jwt token as the credential
        let mut grpc = connect_tls(&ca, addr).await;
        let mut params = login_params("");
        params.jwt_token = "sso-jwt-payload".into();
        let outcome = grpc.login(params).await.expect("grpc login");
        assert_eq!(outcome.session_deadline_unix, None, "unset = no info");

        // 2) the same JWT authenticates the REST surface over TLS
        let rest_addr = spawn_tls_rest(&ca, "Bearer sso-jwt-payload");
        let rest_url = format!("https://{rest_addr}");
        let roots = decode_pem_certificates(ca.ca_pem.as_bytes()).expect("ca der");
        let client = ManagementClient::new_tls(&rest_url, REQUEST_TIMEOUT, roots)
            .expect("tls rest client");
        let session = client
            .login(Credential::Jwt("sso-jwt-payload".into()))
            .expect("rest session");
        let peers = client.peers(&session).expect("peers over TLS");
        assert_eq!(peers.len(), 2);
        assert_eq!(peers[0].id, "peer-1");
        assert_eq!(peers[1].ip.as_deref(), Some("10.64.0.9"));
    }
}

/// The status mapper is pure; keep a direct unit check next to the wire tests.
#[tokio::test]
async fn status_mapper_smoke() {
    let e = map_grpc_status(Status::permission_denied("x"));
    assert!(matches!(e, ManagementError::Auth { status: 7, .. }));
}
