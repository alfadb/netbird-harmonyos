// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N3-2 gRPC Login tests for `netbird_core::grpc` (+ the TLS REST bridge).
//!
//! An in-process tonic server implements `ManagementService/Login` and serves
//! over REAL TLS (self-signed rcgen CA + leaf certificate; the client trusts
//! exactly the generated CA — injected trust root, no system store). Covered
//! cases:
//!
//! 1. login success: real TLS handshake, request-field assertions
//!    (setupKey, PeerKeys.wgPubKey incl. the bytes-of-base64-string quirk,
//!    PeerSystemMeta), response decode (peer address + 3-state
//!    sessionExpiresAt)
//! 2. invalid setup key → server PermissionDenied(7) → `Auth`
//! 3. InvalidArgument(3) → `Request`
//! 4. Internal(13) → `Server`
//! 5. Unavailable(14) → `Network`
//! 6. per-request deadline exceeded → `Timeout`
//! 7. untrusted CA (server signed by another CA) → handshake fails → `Network`
//! 8. connect refused (no server) → `Network`
//! 9. full bridge: gRPC `Login` over TLS, then `GET /api/peers` over a real
//!    TLS HTTP/1.1 endpoint with the session JWT (`Bearer`) — the N3-1 REST
//!    client on the new TLS transport
//!
//! Wire contract under test: `EncryptedMessage.body` carries the serialized
//! `LoginRequest`/`LoginResponse` protobuf. The upstream NaCl body-encryption
//! layer (GetServerKey + crypto_box) is explicitly NOT implemented in this
//! increment (src/grpc.rs module docs); the test server validates this
//! plaintext-body contract.

use netbird_core::grpc::proto::management_service_server::{ManagementService, ManagementServiceServer};
use netbird_core::grpc::proto::{
    EncryptedMessage, LoginRequest, LoginResponse, PeerConfig, ServerKeyResponse,
};
use netbird_core::grpc::{
    map_grpc_status, GrpcTlsConfig, GrpcTransport, LoginParams, ManagementGrpcClient, PeerKeySet,
    PeerMeta,
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

#[derive(Clone, Default)]
struct MockManagement {
    /// Decoded LoginRequest per call (asserted client-side afterwards).
    captured: Arc<Mutex<Vec<CapturedLogin>>>,
    /// When set, the handler replies with this gRPC status instead of a
    /// LoginResponse.
    fail_with: Option<Status>,
    /// Success response (encoded into the reply envelope body).
    response: Option<LoginResponse>,
    /// Sleep before answering (deadline tests).
    delay: Option<Duration>,
}

impl MockManagement {
    fn ok(response: LoginResponse) -> Self {
        MockManagement { response: Some(response), ..Default::default() }
    }
    fn failing(status: Status) -> Self {
        MockManagement { fail_with: Some(status), ..Default::default() }
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
        // Plaintext-body contract (see module docs): decode LoginRequest.
        let req = LoginRequest::decode(envelope.body.as_slice())
            .map_err(|e| Status::invalid_argument(format!("body: {e}")))?;
        let echo_key = envelope.wg_pub_key.clone();
        self.captured.lock().expect("captured lock").push(CapturedLogin {
            envelope_wg_pub_key: envelope.wg_pub_key,
            setup_key: req.setup_key.clone(),
            jwt_token: req.jwt_token.clone(),
            peer_keys_wg_pub_key: req.peer_keys.as_ref().map(|k| k.wg_pub_key.clone()).unwrap_or_default(),
            meta_hostname: req.meta.as_ref().map(|m| m.hostname.clone()).unwrap_or_default(),
        });
        let response = self.response.clone().unwrap_or_default();
        Ok(Response::new(EncryptedMessage {
            wg_pub_key: echo_key,
            body: response.encode_to_vec(),
            version: envelope.version,
        }))
    }

    // GetServerKey would be the entry point of the upstream NaCl message
    // encryption; it is out of scope here and answers unimplemented.
    async fn get_server_key(
        &self,
        _request: Request<netbird_core::grpc::proto::Empty>,
    ) -> Result<Response<ServerKeyResponse>, Status> {
        Err(Status::unimplemented("message-body crypto not in N3-2 scope"))
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

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

async fn connect_tls(ca: &TestCa, addr: SocketAddr) -> ManagementGrpcClient {
    let endpoint = format!("https://{addr}");
    ManagementGrpcClient::connect(
        &endpoint,
        GrpcTransport::Tls(GrpcTlsConfig::new(vec![ca.ca_pem.clone().into_bytes()])),
        CONNECT_TIMEOUT,
        REQUEST_TIMEOUT,
    )
    .await
    .expect("tls connect")
}

fn login_params(setup_key: &str) -> LoginParams {
    LoginParams {
        setup_key: setup_key.into(),
        jwt_token: String::new(),
        peer_keys: PeerKeySet {
            wg_pub_key: b"BASE64WGKEY".to_vec(),
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
async fn login_success_over_real_tls_asserts_request_fields() {
    let ca = make_test_ca("n3-2 test CA");
    let svc = MockManagement::ok(LoginResponse {
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

    let mut client = connect_tls(&ca, addr).await;
    let outcome = client
        .login(login_params("n3-2-setup-key-ok"))
        .await
        .expect("login ok");

    // response mapping
    assert_eq!(outcome.peer_address.as_deref(), Some("10.64.0.7"));
    assert_eq!(outcome.session_deadline_unix, Some(1_893_456_000));

    // request fields as seen on the wire by the server
    let seen = captured.captured_login();
    assert_eq!(seen.setup_key, "n3-2-setup-key-ok");
    assert_eq!(seen.envelope_wg_pub_key, "BASE64WGKEY");
    assert_eq!(
        seen.peer_keys_wg_pub_key,
        b"BASE64WGKEY".to_vec(),
        "PeerKeys.wgPubKey carries the bytes of the base64 string (upstream quirk)"
    );
    assert_eq!(seen.meta_hostname, "ohos-device");
    assert_eq!(seen.jwt_token, "");
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
        ..MockManagement::ok(LoginResponse::default())
    };
    let addr = spawn_tls_management(&ca, svc).await;

    let endpoint = format!("https://{addr}");
    let mut client = ManagementGrpcClient::connect(
        &endpoint,
        GrpcTransport::Tls(GrpcTlsConfig::new(vec![ca.ca_pem.clone().into_bytes()])),
        CONNECT_TIMEOUT,
        Duration::from_millis(300), // request deadline < server delay
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
    let addr = spawn_tls_management(&server_ca, MockManagement::ok(LoginResponse::default())).await;

    // client only trusts a DIFFERENT CA
    let endpoint = format!("https://{addr}");
    let err = ManagementGrpcClient::connect(
        &endpoint,
        GrpcTransport::Tls(GrpcTlsConfig::new(vec![other_ca.ca_pem.into_bytes()])),
        CONNECT_TIMEOUT,
        REQUEST_TIMEOUT,
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
        let addr = spawn_tls_management(&ca, MockManagement::ok(LoginResponse::default())).await;

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
