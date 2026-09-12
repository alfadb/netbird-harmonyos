// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N3-1 mock baseline for the management client skeleton
//! (`netbird_core::management`).
//!
//! A minimal HTTP/1.1 mock server built on a std `TcpListener` (NO new
//! dependencies, no test framework) answers exactly the request shapes the
//! skeleton sends. Covered cases:
//!
//! 1. register success (200, upstream-shaped Peer JSON) + request-shape checks
//! 2. register 401 → `Auth`
//! 3. register 5xx → `Server`
//! 4. malformed JSON response → `Parse`
//! 5. server never answers + short client timeout → `Timeout`
//! 6. 400 → `Request`
//! 7. login: JWT `Bearer` / PAT `Token` header forms (upstream-confirmed)
//! 8. peer list parse
//! 9. end-to-end: login → fetch node config (`GET /api/peers/{id}`)
//!
//! Scope guard: the register endpoint is the module's TODO(未确认) mock-only
//! contract — these tests validate the transport + error classification, not
//! interoperability with a real NetBird server.

use netbird_core::management::{
    parse_base_url, Credential, ManagementClient, ManagementError, PlainHttpTransport,
};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// Host-test link surface (integration-test copy of the `#[cfg(test)]
// host_stubs` pattern in src/napi.rs and src/hilog.rs, which only covers the
// lib's own unit-test binary): the integration test binary links the whole
// crate rlib on the host triple, where libace_napi.z.so / libhilog_ndk.z.so
// do not exist. Codegen-unit grouping can co-locate management code with
// hilog/napi-referencing objects, and then their OHOS extern symbols stay
// undefined. These no-ops satisfy the linker only; the management code paths
// exercised below never call into them (no NAPI runtime, no HiLog emit here).
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
        _object: *mut c_void,
        _utf8name: *const u8,
        _value: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_cb_info(
        _env: *mut c_void,
        _info: *mut c_void,
        _argc: *mut usize,
        _argv: *mut *mut c_void,
        _this_arg: *mut *mut c_void,
        _data: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _env: *mut c_void,
        _str_: *const u8,
        _length: usize,
        _result: *mut *mut c_void,
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
// mock HTTP server (std only)
// ---------------------------------------------------------------------------

/// What the client sent, reduced to what the tests assert on.
#[derive(Debug, Clone)]
struct CapturedRequest {
    method: String,
    path: String,
    authorization: Option<String>,
    content_type: Option<String>,
    body: String,
}

/// Mock answer: a canned HTTP response, or `Hang` (accept + read the request,
/// then never answer — for the timeout case).
enum MockResponse {
    Reply(u16, String),
    Hang,
}

struct MockServer {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<CapturedRequest>>>,
}

impl MockServer {
    /// Serve exactly `connections` sequential connections (one HTTP request
    /// each — the skeleton sends `Connection: close`), then exit.
    fn start(
        connections: usize,
        responder: impl Fn(&CapturedRequest) -> MockResponse + Send + Sync + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock listener");
        let addr = listener.local_addr().expect("mock addr");
        let requests: Arc<Mutex<Vec<CapturedRequest>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = requests.clone();
        thread::spawn(move || {
            for _ in 0..connections {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let req = match read_request(&mut stream) {
                    Some(r) => r,
                    None => continue,
                };
                sink.lock().unwrap().push(req.clone());
                match responder(&req) {
                    MockResponse::Reply(status, body) => {
                        let reason = match status {
                            200 => "OK",
                            400 => "Bad Request",
                            401 => "Unauthorized",
                            403 => "Forbidden",
                            _ => "Internal Server Error",
                        };
                        let resp = format!(
                            "HTTP/1.1 {status} {reason}\r\n\
                             Content-Type: application/json; charset=UTF-8\r\n\
                             Content-Length: {}\r\n\
                             Connection: close\r\n\
                             \r\n\
                             {body}",
                            body.len()
                        );
                        let _ = stream.write_all(resp.as_bytes());
                        let _ = stream.flush();
                    }
                    // hold the connection open without answering; the client's
                    // read timeout must fire first
                    MockResponse::Hang => thread::sleep(Duration::from_millis(2000)),
                }
                // stream dropped -> connection closed
            }
        });
        MockServer { addr, requests }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn requests(&self) -> Vec<CapturedRequest> {
        self.requests.lock().unwrap().clone()
    }
}

/// Read one HTTP request (head + Content-Length body). `None` on garbage.
fn read_request(stream: &mut std::net::TcpStream) -> Option<CapturedRequest> {
    let mut raw = Vec::with_capacity(512);
    let head_end = loop {
        if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        let mut chunk = [0u8; 1024];
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        raw.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next()?.to_string();
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut content_length = 0usize;
    let mut authorization = None;
    let mut content_type = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let (name, value) = (name.trim().to_ascii_lowercase(), value.trim());
        match name.as_str() {
            "content-length" => content_length = value.parse().unwrap_or(0),
            "authorization" => authorization = Some(value.to_string()),
            "content-type" => content_type = Some(value.to_string()),
            _ => {}
        }
    }
    let mut body = raw[head_end + 4..].to_vec();
    while body.len() < content_length {
        let mut chunk = vec![0u8; content_length - body.len()];
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    Some(CapturedRequest {
        method,
        path,
        authorization,
        content_type,
        body: String::from_utf8_lossy(&body).to_string(),
    })
}

fn client(url: &str) -> ManagementClient<PlainHttpTransport> {
    ManagementClient::new(url, Duration::from_secs(5)).expect("client builds")
}

fn client_with_timeout(url: &str, timeout: Duration) -> ManagementClient<PlainHttpTransport> {
    ManagementClient::new(url, timeout).expect("client builds")
}

// upstream-shaped fixtures (field names from openapi.yml Peer/PeerMinimum @
// 791401060d2b95e5f51e3439c0649729132f571e)
const SETUP_KEY: &str = "A616097E-FCF0-48FA-9354-CA4A61142761";
const PEER_ID: &str = "chacbco6lnnbn6cg5s90";

fn peer_json() -> String {
    format!(
        r#"{{"id":"{PEER_ID}","name":"harmonyos-node","ip":"10.64.0.1",
        "connected":true,"hostname":"oh-device","version":"0.78.1",
        "dns_label":"harmonyos-node.netbird.cloud",
        "created_at":"2026-09-13T00:00:00.477782Z","groups":[],
        "connection_ip":"35.64.0.1","os":"Android 15","user_id":"google-oauth2|123"}}"#
    )
}

fn user_json() -> String {
    r#"{"id":"google-oauth2|277474792786460067937","name":"Test User",
       "email":"demo@netbird.io","role":"admin","status":"active"}"#
        .to_string()
}

// ---------------------------------------------------------------------------
// register (TODO(未确认) mock-only contract)
// ---------------------------------------------------------------------------

#[test]
fn register_success_parses_peer_and_request_shape() {
    let mock = MockServer::start(1, |_| MockResponse::Reply(200, peer_json()));
    let peer = client(&mock.url())
        .register(SETUP_KEY, "harmonyos-node")
        .expect("register must succeed");
    assert_eq!(peer.id, PEER_ID);
    assert_eq!(peer.name, "harmonyos-node");
    assert_eq!(peer.ip.as_deref(), Some("10.64.0.1"));
    assert_eq!(peer.connected, Some(true));
    assert_eq!(peer.dns_label.as_deref(), Some("harmonyos-node.netbird.cloud"));

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    let req = &reqs[0];
    assert_eq!(req.method, "POST");
    assert_eq!(req.path, "/api/peers");
    assert_eq!(req.authorization, None, "setup key is the credential");
    assert!(
        req.content_type.as_deref().unwrap_or("").starts_with("application/json"),
        "content-type: {:?}",
        req.content_type
    );
    assert!(
        req.body.contains(&format!("\"setup_key\":\"{SETUP_KEY}\"")),
        "body: {}",
        req.body
    );
    assert!(req.body.contains("\"name\":\"harmonyos-node\""), "body: {}", req.body);
}

#[test]
fn register_invalid_setup_key_401_maps_to_auth_error() {
    let body = r#"{"message":"setup key is invalid","code":401}"#;
    let mock = MockServer::start(1, move |_| MockResponse::Reply(401, body.to_string()));
    let err = client(&mock.url())
        .register("EXPIRED-KEY", "harmonyos-node")
        .expect_err("401 must be an error");
    assert_eq!(
        err,
        ManagementError::Auth {
            status: 401,
            message: "setup key is invalid".into()
        }
    );
}

#[test]
fn register_500_maps_to_server_error() {
    let body = r#"{"message":"internal error","code":500}"#;
    let mock = MockServer::start(1, move |_| MockResponse::Reply(500, body.to_string()));
    let err = client(&mock.url())
        .register(SETUP_KEY, "harmonyos-node")
        .expect_err("500 must be an error");
    assert_eq!(err, ManagementError::Server { status: 500 });
}

#[test]
fn register_400_maps_to_request_error_with_upstream_message() {
    let body = r#"{"message":"couldn't parse JSON request","code":400}"#;
    let mock = MockServer::start(1, move |_| MockResponse::Reply(400, body.to_string()));
    let err = client(&mock.url())
        .register(SETUP_KEY, "")
        .expect_err("400 must be an error");
    assert!(matches!(err, ManagementError::Request { status: 400, .. }), "{err}");
    assert!(err.to_string().contains("couldn't parse JSON request"), "{err}");
}

// ---------------------------------------------------------------------------
// malformed JSON / timeout
// ---------------------------------------------------------------------------

#[test]
fn malformed_json_response_maps_to_parse_error() {
    // truncated JSON body with 200 status
    let mock = MockServer::start(1, |_| MockResponse::Reply(200, "{\"id\": \"x\", ".to_string()));
    let err = client(&mock.url())
        .register(SETUP_KEY, "harmonyos-node")
        .expect_err("truncated JSON must be an error");
    assert!(matches!(err, ManagementError::Parse(_)), "{err}");
    assert!(err.to_string().contains("invalid JSON"), "{err}");
}

#[test]
fn missing_peer_name_maps_to_parse_error() {
    // valid JSON, but the required (upstream PeerMinimum) field is missing
    let mock = MockServer::start(1, |_| MockResponse::Reply(200, r#"{"id":"x"}"#.to_string()));
    let err = client(&mock.url())
        .register(SETUP_KEY, "harmonyos-node")
        .expect_err("Peer without name must be an error");
    assert!(
        matches!(err, ManagementError::Parse(ref m) if m.contains("'name'")),
        "{err}"
    );
}

#[test]
fn server_never_answers_maps_to_timeout() {
    let mock = MockServer::start(1, |_| MockResponse::Hang);
    let started = Instant::now();
    let err = client_with_timeout(&mock.url(), Duration::from_millis(300))
        .register(SETUP_KEY, "harmonyos-node")
        .expect_err("hung server must time out");
    assert_eq!(err, ManagementError::Timeout);
    // the client budget (300ms) must fire, not the mock's 2s release
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(250) && elapsed < Duration::from_millis(1900),
        "timeout fired at {elapsed:?}, expected ~300ms"
    );
}

// ---------------------------------------------------------------------------
// login / session (upstream-confirmed: /api/users/current, Bearer|Token)
// ---------------------------------------------------------------------------

#[test]
fn login_jwt_uses_bearer_header_and_parses_user() {
    let mock = MockServer::start(1, |_| MockResponse::Reply(200, user_json()));
    let session = client(&mock.url())
        .login(Credential::Jwt("header.payload.sig".into()))
        .expect("jwt login must succeed");
    assert_eq!(session.user.id, "google-oauth2|277474792786460067937");
    assert_eq!(session.user.role.as_deref(), Some("admin"));
    let reqs = mock.requests();
    assert_eq!(reqs[0].method, "GET");
    assert_eq!(reqs[0].path, "/api/users/current");
    assert_eq!(reqs[0].authorization.as_deref(), Some("Bearer header.payload.sig"));
}

#[test]
fn login_pat_uses_token_header_form() {
    let mock = MockServer::start(1, |_| MockResponse::Reply(200, user_json()));
    client(&mock.url())
        .login(Credential::Pat("nbp_123456".into()))
        .expect("pat login must succeed");
    let reqs = mock.requests();
    assert_eq!(reqs[0].authorization.as_deref(), Some("Token nbp_123456"));
}

#[test]
fn login_403_maps_to_auth_error() {
    let body = r#"{"message":"forbidden","code":403}"#;
    let mock = MockServer::start(1, move |_| MockResponse::Reply(403, body.to_string()));
    let err = client(&mock.url())
        .login(Credential::Pat("revoked".into()))
        .expect_err("403 must be an error");
    assert_eq!(
        err,
        ManagementError::Auth { status: 403, message: "forbidden".into() }
    );
}

// ---------------------------------------------------------------------------
// peer list + end-to-end node-config fetch
// ---------------------------------------------------------------------------

#[test]
fn peers_list_parses_two_entries() {
    let list = format!("[{},{{\"id\":\"peer2\",\"name\":\"other\"}}]", peer_json());
    let mock = MockServer::start(2, move |req| match req.path.as_str() {
        "/api/users/current" => MockResponse::Reply(200, user_json()),
        "/api/peers" => MockResponse::Reply(200, list.clone()),
        other => panic!("unexpected path {other}"),
    });
    let session = client(&mock.url())
        .login(Credential::Jwt("t".into()))
        .expect("login");
    let peers = client(&mock.url()).peers(&session).expect("peers");
    assert_eq!(peers.len(), 2);
    assert_eq!(peers[0].id, PEER_ID);
    assert_eq!(peers[1].name, "other");
    assert_eq!(peers[1].ip, None, "optional fields stay Option");
    let reqs = mock.requests();
    assert_eq!(reqs[1].path, "/api/peers");
    assert_eq!(reqs[1].authorization.as_deref(), Some("Bearer t"));
}

#[test]
fn peers_non_array_response_maps_to_parse_error() {
    let mock = MockServer::start(2, |req| match req.path.as_str() {
        "/api/users/current" => MockResponse::Reply(200, user_json()),
        "/api/peers" => MockResponse::Reply(200, r#"{"oops":1}"#.to_string()),
        other => panic!("unexpected path {other}"),
    });
    let session = client(&mock.url()).login(Credential::Jwt("t".into())).expect("login");
    let err = client(&mock.url()).peers(&session).expect_err("object is not a list");
    assert!(matches!(err, ManagementError::Parse(_)), "{err}");
}

#[test]
fn end_to_end_login_then_fetch_node_config() {
    // one server, two sequential requests: login -> GET /api/peers/{id}
    let mock = MockServer::start(2, |req| match req.path.as_str() {
        "/api/users/current" => MockResponse::Reply(200, user_json()),
        p if p == format!("/api/peers/{PEER_ID}") => MockResponse::Reply(200, peer_json()),
        other => panic!("unexpected path {other}"),
    });
    let c = client(&mock.url());
    let session = c
        .login(Credential::Jwt("e2e.jwt".into()))
        .expect("login must succeed");
    let node = c.peer(&session, PEER_ID).expect("node config fetch");
    assert_eq!(node.id, PEER_ID);
    assert_eq!(node.ip.as_deref(), Some("10.64.0.1"));
    assert_eq!(node.version.as_deref(), Some("0.78.1"));

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].path, "/api/users/current");
    assert_eq!(reqs[0].authorization.as_deref(), Some("Bearer e2e.jwt"));
    assert_eq!(reqs[1].method, "GET");
    assert_eq!(reqs[1].path, format!("/api/peers/{PEER_ID}"));
    assert_eq!(reqs[1].authorization.as_deref(), Some("Bearer e2e.jwt"));
}

#[test]
fn peer_id_with_slash_is_rejected_before_any_request() {
    // client with a server that must never be contacted for the bad id
    let idle = MockServer::start(0, |_| panic!("no request may be sent"));
    let c = client(&idle.url());
    let login_server = MockServer::start(1, |_| MockResponse::Reply(200, user_json()));
    let session = client(&login_server.url())
        .login(Credential::Jwt("t".into()))
        .expect("login");
    let err = c
        .peer(&session, "../other")
        .expect_err("path traversal-ish id must be rejected locally");
    assert!(matches!(err, ManagementError::Request { status: 0, .. }), "{err}");
}

// ---------------------------------------------------------------------------
// base url gate (https rejected until TLS exists)
// ---------------------------------------------------------------------------

#[test]
fn https_base_url_is_rejected_as_unsupported() {
    let result = ManagementClient::<PlainHttpTransport>::new(
        "https://mgmt.example.com:443",
        Duration::from_secs(1),
    );
    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("https must be rejected"),
    };
    assert!(matches!(err, ManagementError::UnsupportedUrl(_)), "{err}");
    assert!(parse_base_url("http://127.0.0.1:1").is_ok());
}
