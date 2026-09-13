// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N3-4 Sync/Logout/ExtendAuthSession integration tests.
//!
//! An in-process tonic server implements `ManagementService/Sync` (server-
//! streaming, the upstream wire shape of
//! `shared/management/client/grpc.go:478-521`), `Logout` (grpc.go:848-872)
//! and `ExtendAuthSession` (grpc.go:662-699) over plaintext h2 on loopback,
//! with the REAL message-body envelope in both directions (the server holds
//! a NaCl key pair, `GetServerKey` advertises it, every frame is sealed
//! with a fresh nonce). Covered:
//!
//! 1. first frame really decrypts to `SyncRequest{meta}` with OUR identity
//!    (`syncMessageVersion = 0` — the legacy NetworkMap contract), and N
//!    pushed NetworkMap updates (peer add / route change / DNS change) are
//!    decoded IN ORDER with exact fields
//! 2. a mid-stream break surfaces as a `Closed` error carrying the mapped
//!    gRPC class; the session then reconnects through the injected backoff
//!    (deterministic delay sequence asserted through `run()`, no real long
//!    waits) and the server continues pushing on the new stream
//! 3. PermissionDenied is FATAL (upstream `backoff.Permanent`): no
//!    reconnect, `run()` stops with the `Auth` class
//! 4. Logout success: the server decrypts the sealed `Empty` body; the
//!    plain (unencrypted) `Empty` reply is consumed; NotFound maps to the
//!    `Request` class
//! 5. ExtendAuthSession success: the server decrypts `{jwtToken, meta}` and
//!    replies the sealed `ExtendAuthSessionResponse`; expired-JWT
//!    PermissionDenied → `Auth`; an empty JWT is rejected locally before
//!    any I/O
//! 6. the components wire format (`version=1` + NetworkMapEnvelope) is an
//!    explicit error; a SyncResponse without NetworkMap yields
//!    `network_map: None` with the 3-state deadline semantics preserved
//!
//! NetworkMap decode boundaries (empty peers / unknown future fields /
//! missing fields) are pinned at the unit level in `src/network_map.rs`.

use core::time::Duration;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use netbird_core::backoff::{Clock, ExponentialBackoff, Rng};
use netbird_core::envelope::{self, EnvelopeKeyPair, EnvelopePublicKey};
use netbird_core::grpc::proto::management_service_server::{ManagementService, ManagementServiceServer};
use netbird_core::grpc::proto::{
    Empty, EncryptedMessage, ExtendAuthSessionRequest, ExtendAuthSessionResponse, PeerSystemMeta,
    ServerKeyResponse, SyncRequest, SyncResponse,
};
use netbird_core::grpc::{GrpcTransport, ManagementGrpcClient, PeerMeta};
use netbird_core::management::ManagementError;
use netbird_core::sync::{SyncSession, SyncStreamError, SyncUpdate};
use prost::Message as _;
use prost_types::Timestamp;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status};

// Host-test link surface (same as tests/management_grpc.rs): the test binary
// links the whole crate rlib on the host triple, where libace_napi.z.so /
// libhilog_ndk.z.so do not exist. These no-ops satisfy the linker only.
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
        _name: *const u8,
        _value: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _env: *mut c_void,
        _name: *const u8,
        _value: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_cb_info(
        _env: *mut c_void,
        _cbinfo: *mut c_void,
        _argc: *mut usize,
        _argv: *mut *mut c_void,
        _this_arg: *mut c_void,
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
    pub extern "C" fn napi_get_value_int32(_env: *mut c_void, _value: *mut c_void, _result: *mut i32) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(_env: *mut c_void, _value: *mut c_void, _result: *mut bool) -> i32 {
        0
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn peer_meta() -> PeerMeta {
    PeerMeta {
        hostname: "ohos-sync-device".into(),
        os_name: "harmonyos".into(),
        os_version: "26.0.0".into(),
        netbird_version: "0.1.0".into(),
    }
}

fn client_keys() -> EnvelopeKeyPair {
    EnvelopeKeyPair::generate().expect("client key pair")
}

async fn connect_plain(addr: std::net::SocketAddr, keys: EnvelopeKeyPair) -> ManagementGrpcClient {
    ManagementGrpcClient::connect(
        &format!("http://{addr}"),
        GrpcTransport::Plaintext,
        Duration::from_secs(5),
        Duration::from_secs(5),
        keys,
    )
    .await
    .expect("plaintext connect")
}

/// Wait (bounded) until `cond()` holds; panics with `what` on deadline.
async fn wait_for<F: Fn() -> bool>(what: &'static str, cond: F) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !cond() {
        assert!(Instant::now() <= deadline, "deadline waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Deterministic reconnect policy: f=0 → the delay IS the current interval
/// (the upstream formula's `randomizationFactor == 0` short-circuit), the
/// growth chain is exact, and the waits stay at milliseconds.
fn deterministic_backoff() -> ExponentialBackoff {
    ExponentialBackoff::new(
        Duration::from_millis(5),
        0.0,
        1.7,
        Duration::from_millis(100),
        None,
    )
}

/// RNG that must never be sampled: the deterministic backoff uses f=0 (no
/// jitter). An accidental sample fails the test loudly instead of passing
/// silently.
#[derive(Default)]
struct NoRng;

impl Rng for NoRng {
    fn next_uniform(&mut self) -> f64 {
        panic!("f=0 backoff must not consume randomness");
    }
}

/// Pass-through monotonic clock (the deterministic test asserts the delay
/// VALUES via `observed_reconnect_delays`; the elapsed budget never binds).
#[derive(Default)]
struct RealClock;

impl Clock for RealClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

// ---------------------------------------------------------------------------
// in-process management mock with a Sync script
// ---------------------------------------------------------------------------

/// One scripted Sync connection: push these (sealed) SyncResponses, then
/// either hold the stream open or fail it with the given status.
enum SyncStep {
    UpdatesThen(Vec<SyncResponse>, Option<Status>),
}

#[derive(Debug, Clone)]
struct CapturedSyncConnect {
    /// envelope wgPubKey (our public key, base64)
    envelope_wg_pub_key: String,
    /// decrypted first frame
    meta: Option<PeerSystemMeta>,
}

#[derive(Debug, Clone)]
struct CapturedLogout {
    envelope_wg_pub_key: String,
    /// decrypted body length (Empty encodes to 0 bytes)
    body_plaintext_len: usize,
}

#[derive(Debug, Clone)]
struct CapturedExtend {
    envelope_wg_pub_key: String,
    jwt_token: String,
    meta_hostname: String,
}

#[derive(Clone)]
struct SyncMock {
    keys: EnvelopeKeyPair,
    sync_script: Arc<Mutex<VecDeque<SyncStep>>>,
    sync_connects: Arc<Mutex<Vec<CapturedSyncConnect>>>,
    logout_calls: Arc<Mutex<Vec<CapturedLogout>>>,
    logout_fail: Arc<Mutex<Option<Status>>>,
    extend_calls: Arc<Mutex<Vec<CapturedExtend>>>,
    /// None → reply the default sealed response; Some → fail with status.
    extend_fail: Arc<Mutex<Option<Status>>>,
}

impl Default for SyncMock {
    fn default() -> Self {
        SyncMock {
            keys: EnvelopeKeyPair::generate().expect("server key pair"),
            sync_script: Arc::new(Mutex::new(VecDeque::new())),
            sync_connects: Arc::new(Mutex::new(Vec::new())),
            logout_calls: Arc::new(Mutex::new(Vec::new())),
            logout_fail: Arc::new(Mutex::new(None)),
            extend_calls: Arc::new(Mutex::new(Vec::new())),
            extend_fail: Arc::new(Mutex::new(None)),
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
impl ManagementService for SyncMock {
    async fn sync(
        &self,
        request: Request<EncryptedMessage>,
    ) -> Result<Response<tonic::codegen::BoxStream<EncryptedMessage>>, Status> {
        let env = request.into_inner();
        let plaintext = decrypt_body(&self.keys, &env)?;
        let req = SyncRequest::decode(plaintext.as_slice())
            .map_err(|e| Status::invalid_argument(format!("first frame is not SyncRequest: {e}")))?;
        self.sync_connects.lock().expect("connects lock").push(CapturedSyncConnect {
            envelope_wg_pub_key: env.wg_pub_key.clone(),
            meta: req.meta,
        });

        let step = self.sync_script.lock().expect("script lock").pop_front();
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
            tx.send(Ok(EncryptedMessage { wg_pub_key: env.wg_pub_key.clone(), body: sealed, version: 0 }))
                .await
                .expect("receiver alive while scripting");
        }
        if let Some(status) = fail_after {
            tx.send(Err(status)).await.expect("receiver alive for failure");
        }
        if hold {
            // keep the stream open (upstream holds it until the next change
            // or a transport break)
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
        self.logout_calls.lock().expect("logout lock").push(CapturedLogout {
            envelope_wg_pub_key: env.wg_pub_key,
            body_plaintext_len: plaintext.len(),
        });
        if let Some(status) = self.logout_fail.lock().expect("logout fail lock").clone() {
            return Err(status);
        }
        // upstream proto: Logout returns a PLAIN Empty (not an envelope)
        Ok(Response::new(Empty {}))
    }

    async fn extend_auth_session(
        &self,
        request: Request<EncryptedMessage>,
    ) -> Result<Response<EncryptedMessage>, Status> {
        let env = request.into_inner();
        let plaintext = decrypt_body(&self.keys, &env)?;
        let req = ExtendAuthSessionRequest::decode(plaintext.as_slice()).map_err(|e| {
            Status::invalid_argument(format!("body is not ExtendAuthSessionRequest: {e}"))
        })?;
        self.extend_calls.lock().expect("extend lock").push(CapturedExtend {
            envelope_wg_pub_key: env.wg_pub_key.clone(),
            jwt_token: req.jwt_token.clone(),
            meta_hostname: req.meta.as_ref().map(|m| m.hostname.clone()).unwrap_or_default(),
        });
        if let Some(status) = self.extend_fail.lock().expect("extend fail lock").clone() {
            return Err(status);
        }
        let response = ExtendAuthSessionResponse {
            session_expires_at: Some(Timestamp { seconds: 1_893_456_000, nanos: 0 }),
        };
        let client_pk = EnvelopePublicKey::from_base64(&env.wg_pub_key).expect("client pk");
        let sealed = envelope::seal(&client_pk, &self.keys, &response.encode_to_vec())
            .expect("seal extend reply");
        Ok(Response::new(EncryptedMessage { wg_pub_key: env.wg_pub_key, body: sealed, version: 0 }))
    }

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
}

/// Spawn the mock on loopback (plaintext h2 — the envelope provides the
/// application-layer crypto; TLS is exercised by the N3-2/N3-3 suite).
async fn spawn_mock(svc: SyncMock) -> std::net::SocketAddr {
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
// SyncResponse builders (initial snapshot / route change / DNS + offline)
// ---------------------------------------------------------------------------

fn mgmt_peer(key: &str, ip: &str, fqdn: &str) -> netbird_core::grpc::proto::RemotePeerConfig {
    netbird_core::grpc::proto::RemotePeerConfig {
        wg_pub_key: key.into(),
        allowed_ips: vec![format!("{ip}/32")],
        ssh_config: None,
        fqdn: fqdn.into(),
        agent_version: "1.0.0".into(),
        lazy_state: 0,
    }
}

fn base_snapshot() -> SyncResponse {
    SyncResponse {
        netbird_config: Some(netbird_core::grpc::proto::NetbirdConfig {
            stuns: vec![netbird_core::grpc::proto::HostConfig {
                uri: "stun:stun.example.io:3478".into(),
                protocol: 0,
            }],
            turns: vec![],
            signal: Some(netbird_core::grpc::proto::HostConfig {
                uri: "signal.example.io:10000".into(),
                protocol: 0,
            }),
            relay: Some(netbird_core::grpc::proto::RelayConfig {
                urls: vec!["rels://relay.example.io:443".into()],
                token_payload: "tok".into(),
                token_signature: "sig".into(),
            }),
            flow: None,
            metrics: None,
        }),
        network_map: Some(netbird_core::grpc::proto::NetworkMap {
            serial: 5,
            peer_config: Some(netbird_core::grpc::proto::PeerConfig {
                address: "10.64.0.9".into(),
                fqdn: "me.example.net".into(),
                mtu: 1380,
                ..Default::default()
            }),
            remote_peers: vec![
                mgmt_peer("UEVFUjFBQQ==", "10.30.30.1", "peer1.example.net"),
                mgmt_peer("UEVFUjJCQg==", "10.30.30.2", "peer2.example.net"),
            ],
            remote_peers_is_empty: false,
            routes: vec![netbird_core::grpc::proto::Route {
                id: "r-100".into(),
                network: "172.16.0.0/12".into(),
                network_type: 1,
                peer: "UEVFUjFBQQ==".into(),
                metric: 5,
                masquerade: true,
                net_id: "net-a".into(),
                domains: vec![],
                keep_route: false,
                skip_auto_apply: false,
            }],
            dns_config: Some(netbird_core::grpc::proto::DnsConfig {
                service_enable: true,
                name_server_groups: vec![netbird_core::grpc::proto::NameServerGroup {
                    name_servers: vec![netbird_core::grpc::proto::NameServer {
                        ip: "1.1.1.1".into(),
                        ns_type: 0,
                        port: 53,
                    }],
                    primary: true,
                    domains: vec!["corp.example".into()],
                    search_domains_enabled: true,
                }],
                custom_zones: vec![],
                // forwarder_port is deprecated upstream — not consumed here
                ..Default::default()
            }),
            offline_peers: vec![],
            ..Default::default()
        }),
        session_expires_at: Some(Timestamp { seconds: 1_700_000_000, nanos: 0 }),
        ..Default::default()
    }
}

/// Update 2: a route change (new route + metric change), no netbird config.
fn snapshot2_route_change() -> SyncResponse {
    let mut s = base_snapshot();
    if let Some(map) = s.network_map.as_mut() {
        map.serial = 6;
        map.routes = vec![
            netbird_core::grpc::proto::Route {
                id: "r-100".into(),
                network: "172.16.0.0/12".into(),
                metric: 9,
                ..Default::default()
            },
            netbird_core::grpc::proto::Route {
                id: "r-101".into(),
                network: "192.168.1.0/24".into(),
                metric: 1,
                ..Default::default()
            },
        ];
    }
    s.netbird_config = None;
    s.session_expires_at = None;
    s
}

/// Update 3: DNS change (resolver disabled) + one peer offline/removed.
fn snapshot3_dns_and_offline() -> SyncResponse {
    let mut s = base_snapshot();
    if let Some(map) = s.network_map.as_mut() {
        map.serial = 7;
        map.remote_peers = vec![mgmt_peer("UEVFUjFBQQ==", "10.30.30.1", "peer1.example.net")];
        map.offline_peers = vec![mgmt_peer("UEVFUjJCQg==", "10.30.30.2", "peer2.example.net")];
        map.routes = vec![];
        map.dns_config = Some(netbird_core::grpc::proto::DnsConfig {
            service_enable: false,
            name_server_groups: vec![],
            custom_zones: vec![],
            ..Default::default()
        });
    }
    s.netbird_config = None;
    s.session_expires_at = None;
    s
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

/// N pushed NetworkMap updates (peer/route/DNS changes) are decoded in
/// order with exact fields; the first frame really decrypts to
/// SyncRequest{meta} carrying our identity and the legacy
/// syncMessageVersion=0 contract.
#[tokio::test(flavor = "multi_thread")]
async fn sync_first_frame_and_updates_decode_in_order() {
    let mock = SyncMock::default();
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![base_snapshot(), snapshot2_route_change(), snapshot3_dns_and_offline()],
        None,
    ));
    let addr = spawn_mock(mock.clone()).await;

    let keys = client_keys();
    let client_pk_b64 = keys.public_key_base64();
    let mut session = SyncSession::new(connect_plain(addr, keys).await, peer_meta());
    session.connect().await.expect("connect");

    // first frame as REALLY decrypted by the server
    let seen = mock.sync_connects.lock().expect("connects");
    assert_eq!(seen.len(), 1, "exactly one Sync connection so far");
    assert_eq!(seen[0].envelope_wg_pub_key, client_pk_b64);
    let meta = seen[0].meta.as_ref().expect("meta present");
    assert_eq!(meta.hostname, "ohos-sync-device");
    assert_eq!(meta.go_os, "harmonyos");
    assert_eq!(meta.netbird_version, "0.1.0");
    assert_eq!(
        meta.sync_message_version, 0,
        "legacy NetworkMap contract: we never advertise the components version"
    );
    assert!(
        meta.capabilities.is_empty(),
        "we do not advertise PeerCapabilityComponentNetworkMap"
    );
    drop(seen);

    // three updates, in order, decoded into the minimal model
    let u1 = session.next_update().await.expect("update 1");
    assert_eq!(u1.session_deadline_unix, Some(1_700_000_000));
    let map1 = u1.network_map.as_ref().expect("map 1");
    assert_eq!(map1.serial, 5);
    assert_eq!(map1.peers.len(), 2);
    assert_eq!(map1.peers[0].wg_pub_key, "UEVFUjFBQQ==");
    assert_eq!(map1.peers[0].fqdn.as_deref(), Some("peer1.example.net"));
    assert_eq!(
        map1.peers[0].allowed_ips,
        vec![netbird_core::config::Route { addr: [10, 30, 30, 1], prefix_len: 32 }]
    );
    let pc = map1.peer.as_ref().expect("own peer config");
    assert_eq!(pc.address.as_deref(), Some("10.64.0.9"));
    assert_eq!(pc.mtu, Some(1380));
    // netbird config extracted (STUN/relay/signal)
    let servers = u1.netbird_config.as_ref().expect("netbird config 1");
    assert_eq!(servers.stuns, vec!["stun:stun.example.io:3478"]);
    assert_eq!(servers.signal.as_deref(), Some("signal.example.io:10000"));
    let relay = servers.relay.as_ref().expect("relay");
    assert_eq!(relay.urls, vec!["rels://relay.example.io:443"]);
    // routes
    assert_eq!(map1.routes.len(), 1);
    assert_eq!(map1.routes[0].id, "r-100");
    assert_eq!(
        map1.routes[0].network,
        netbird_core::config::Route { addr: [172, 16, 0, 0], prefix_len: 12 }
    );
    // dns
    let dns = map1.dns.as_ref().expect("dns 1");
    assert!(dns.service_enable);
    assert_eq!(dns.name_server_groups[0].name_servers[0].ip, "1.1.1.1");

    let u2 = session.next_update().await.expect("update 2");
    let map2 = u2.network_map.as_ref().expect("map 2");
    assert_eq!(map2.serial, 6, "route change snapshot");
    assert_eq!(map2.routes.len(), 2);
    assert_eq!(map2.routes[1].id, "r-101");
    assert_eq!(map2.routes[1].metric, 1);
    assert!(u2.netbird_config.is_none(), "change snapshots may omit netbirdConfig");
    assert_eq!(u2.session_deadline_unix, None, "unset → 3-state None (keep current)");

    let u3 = session.next_update().await.expect("update 3");
    let map3 = u3.network_map.as_ref().expect("map 3");
    assert_eq!(map3.serial, 7, "dns/offline snapshot");
    assert_eq!(map3.peers.len(), 1, "peer removed");
    assert_eq!(map3.offline_peers.len(), 1);
    assert_eq!(map3.offline_peers[0].wg_pub_key, "UEVFUjJCQg==");
    assert!(map3.routes.is_empty());
    let dns3 = map3.dns.as_ref().expect("dns 3");
    assert!(!dns3.service_enable);
    assert!(dns3.name_server_groups.is_empty());
}

/// A mid-stream break surfaces as a `Closed` error carrying the mapped gRPC
/// class (Internal → Server). The caller/session treats it as the
/// reconnect trigger (upstream grpc.go:460-473 "will retry silently").
#[tokio::test(flavor = "multi_thread")]
async fn midstream_break_reports_closed_error_with_mapped_class() {
    let mock = SyncMock::default();
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![base_snapshot()],
        Some(Status::internal("h2 stream reset")),
    ));
    let addr = spawn_mock(mock).await;

    let mut session = SyncSession::new(connect_plain(addr, client_keys()).await, peer_meta());
    session.connect().await.expect("connect");
    let u1 = session.next_update().await.expect("first update ok");
    assert_eq!(u1.network_map.as_ref().expect("map").serial, 5);

    let err = session.next_update().await.expect_err("stream must break");
    match err {
        SyncStreamError::Closed(ManagementError::Server { status: 13 }) => {
            // Internal(13) → Server class — the reconnect-eligible surface
        }
        other => panic!("expected Closed(Server{{13}}), got {other:?}"),
    }
}

/// Stream break → reconnect through the backoff → the server continues
/// pushing on the new connection, in order.
#[tokio::test(flavor = "multi_thread")]
async fn stream_break_reconnects_and_server_continues() {
    let mock = SyncMock::default();
    // 1st connection: one update, then the stream dies
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![base_snapshot()],
        Some(Status::internal("h2 stream reset")),
    ));
    // 2nd connection: the server continues with the next snapshot and holds
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![snapshot2_route_change()],
        None,
    ));
    let addr = spawn_mock(mock.clone()).await;

    let updates: Arc<Mutex<Vec<SyncUpdate>>> = Arc::new(Mutex::new(Vec::new()));
    let collector = updates.clone();
    let driver = tokio::spawn(async move {
        let mut session =
            SyncSession::new(connect_plain(addr, client_keys()).await, peer_meta())
                .with_policy(deterministic_backoff(), Box::new(NoRng), Box::new(RealClock));
        let _ = session.run(move |u| collector.lock().expect("collector").push(u.clone())).await;
    });

    wait_for("two updates across the reconnect", || {
        updates.lock().expect("collector").len() == 2
    })
    .await;

    let got = updates.lock().expect("collector");
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].network_map.as_ref().expect("map 1").serial, 5);
    assert_eq!(got[1].network_map.as_ref().expect("map 2").serial, 6);
    assert_eq!(
        got[1].network_map.as_ref().expect("map 2").routes.len(),
        2,
        "route change arrived after the reconnect"
    );
    drop(got);

    // the server really saw TWO Sync connections (initial + reconnect),
    // each with a properly sealed+decodable first frame
    wait_for("two Sync connections on the server", || {
        mock.sync_connects.lock().expect("connects").len() == 2
    })
    .await;

    driver.abort();
}

/// The reconnect delay sequence driven through `run()` is the injected
/// backoff's exact growth chain (f=0 → 5ms on the first draw), asserted
/// WITHOUT real long waits, and a subsequent PermissionDenied stops the
/// session as FATAL (`Auth`, no second reconnect).
#[tokio::test(flavor = "multi_thread")]
async fn reconnect_backoff_sequence_then_fatal_stops() {
    let mock = SyncMock::default();
    // 1st connection: one update, then the stream dies (retryable)
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![base_snapshot()],
        Some(Status::internal("h2 stream reset")),
    ));
    // 2nd connection attempt: PermissionDenied — upstream Permanent
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![],
        Some(Status::permission_denied("peer revoked")),
    ));
    let addr = spawn_mock(mock).await;

    let updates: Arc<Mutex<Vec<SyncUpdate>>> = Arc::new(Mutex::new(Vec::new()));
    let collector = updates.clone();
    let mut session =
        SyncSession::new(connect_plain(addr, client_keys()).await, peer_meta())
            .with_policy(deterministic_backoff(), Box::new(NoRng), Box::new(RealClock));
    let err = session
        .run(move |u| collector.lock().expect("collector").push(u.clone()))
        .await
        .expect_err("session must stop with the fatal error");
    assert!(
        matches!(err, ManagementError::Auth { status: 7, .. }),
        "PermissionDenied → Auth fatal, got {err:?}"
    );

    // exactly one retryable break happened before the fatal stop, and the
    // delay that was actually driven through run() is the initial interval
    // of the injected backoff (f=0 → bare current interval)
    assert_eq!(session.reconnects(), 1);
    assert_eq!(session.observed_reconnect_delays(), vec![Duration::from_millis(5)]);
    // the update from the first connection was delivered before the break
    assert_eq!(updates.lock().expect("collector").len(), 1);
}

/// PermissionDenied on the FIRST connect attempt is fatal too: no update,
/// no reconnect, no backoff delay.
#[tokio::test(flavor = "multi_thread")]
async fn permission_denied_on_first_connect_is_fatal() {
    let mock = SyncMock::default();
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(
        vec![],
        Some(Status::permission_denied("peer not authorized")),
    ));
    let addr = spawn_mock(mock).await;

    let mut session = SyncSession::new(connect_plain(addr, client_keys()).await, peer_meta());
    let err = session.run(|_| panic!("no update expected")).await.expect_err("must fail");
    assert!(
        matches!(err, ManagementError::Auth { status: 7, .. }),
        "PermissionDenied → Auth class, got {err:?}"
    );
    assert_eq!(session.reconnects(), 0, "fatal: no reconnect attempted");
    assert!(session.observed_reconnect_delays().is_empty(), "no backoff on fatal");
}

/// Logout success: the server decrypts the sealed body to an EMPTY
/// protobuf and the plain Empty reply is consumed without error.
#[tokio::test(flavor = "multi_thread")]
async fn logout_success_sends_sealed_empty() {
    let mock = SyncMock::default();
    let addr = spawn_mock(mock.clone()).await;
    let keys = client_keys();
    let client_pk_b64 = keys.public_key_base64();
    let mut client = connect_plain(addr, keys).await;
    client.logout().await.expect("logout ok");

    let calls = mock.logout_calls.lock().expect("logout calls");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].envelope_wg_pub_key, client_pk_b64);
    assert_eq!(
        calls[0].body_plaintext_len, 0,
        "the sealed body decrypts to the empty proto::Empty (grpc.go:857)"
    );
}

/// Logout failure: NotFound (peer already gone) maps to the Request class —
/// the caller decides whether to treat it as success (the upstream daemon
/// does; this primitive reports it).
#[tokio::test(flavor = "multi_thread")]
async fn logout_not_found_maps_to_request_class() {
    let mock = SyncMock::default();
    *mock.logout_fail.lock().expect("fail") = Some(Status::not_found("peer not found"));
    let addr = spawn_mock(mock).await;
    let mut client = connect_plain(addr, client_keys()).await;
    let err = client.logout().await.expect_err("logout must fail");
    assert!(
        matches!(err, ManagementError::Request { status: 5, .. }),
        "NotFound → Request class, got {err:?}"
    );
}

/// ExtendAuthSession success: server decrypts {jwtToken, meta}, replies the
/// sealed ExtendAuthSessionResponse; the outcome surfaces the absolute
/// deadline.
#[tokio::test(flavor = "multi_thread")]
async fn extend_auth_session_success_returns_deadline() {
    let mock = SyncMock::default();
    let addr = spawn_mock(mock.clone()).await;
    let keys = client_keys();
    let client_pk_b64 = keys.public_key_base64();
    let mut client = connect_plain(addr, keys).await;
    let outcome = client
        .extend_auth_session(&peer_meta(), "fresh.jwt.token")
        .await
        .expect("extend ok");
    assert_eq!(
        outcome.session_deadline_unix,
        Some(1_893_456_000),
        "absolute deadline from ExtendAuthSessionResponse"
    );

    let calls = mock.extend_calls.lock().expect("extend calls");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].jwt_token, "fresh.jwt.token");
    assert_eq!(calls[0].meta_hostname, "ohos-sync-device");
    assert_eq!(calls[0].envelope_wg_pub_key, client_pk_b64);
}

/// Expired/invalid JWT → PermissionDenied → the Auth class.
#[tokio::test(flavor = "multi_thread")]
async fn extend_auth_session_expired_jwt_maps_to_auth() {
    let mock = SyncMock::default();
    *mock.extend_fail.lock().expect("fail") = Some(Status::permission_denied("jwt expired"));
    let addr = spawn_mock(mock).await;
    let mut client = connect_plain(addr, client_keys()).await;
    let err = client.extend_auth_session(&peer_meta(), "expired.jwt").await.expect_err("must fail");
    assert!(
        matches!(err, ManagementError::Auth { status: 7, .. }),
        "expired JWT → Auth class, got {err:?}"
    );
}

/// An empty JWT is rejected locally BEFORE any I/O (upstream engine guard,
/// engine_authsession.go:76-78) — no server call is made.
#[tokio::test(flavor = "multi_thread")]
async fn extend_auth_session_empty_jwt_rejected_before_io() {
    let mock = SyncMock::default();
    let addr = spawn_mock(mock.clone()).await;
    let mut client = connect_plain(addr, client_keys()).await;
    let err = client.extend_auth_session(&peer_meta(), "").await.expect_err("must fail");
    assert!(
        matches!(err, ManagementError::Request { status: 0, .. }),
        "local pre-flight → Request{{0}}, got {err:?}"
    );
    assert!(
        mock.extend_calls.lock().expect("extend calls").is_empty(),
        "no server interaction for an empty token"
    );
}

/// The components wire format (version=1 + NetworkMapEnvelope) — which we
/// never advertise — is an EXPLICIT error, not a silent no-op.
#[tokio::test(flavor = "multi_thread")]
async fn components_envelope_is_an_explicit_error() {
    let mock = SyncMock::default();
    let components = SyncResponse {
        version: 1, // ComponentNetworkMap
        network_map_envelope: Some(Default::default()),
        ..Default::default()
    };
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(vec![components], None));
    let addr = spawn_mock(mock).await;

    let mut session = SyncSession::new(connect_plain(addr, client_keys()).await, peer_meta());
    session.connect().await.expect("connect");
    let err = session.next_update().await.expect_err("components must error");
    match err {
        SyncStreamError::Closed(ManagementError::Parse(m)) => {
            assert!(m.contains("components"), "error should name the components format: {m}");
        }
        other => panic!("expected Closed(Parse(components)), got {other:?}"),
    }
}

/// A SyncResponse without NetworkMap yields `network_map: None` and keeps
/// the 3-state deadline semantics (set-zero = explicit "expiry disabled").
#[tokio::test(flavor = "multi_thread")]
async fn response_without_network_map_keeps_deadline_semantics() {
    let mock = SyncMock::default();
    let bare = SyncResponse {
        session_expires_at: Some(Timestamp { seconds: 0, nanos: 0 }),
        ..Default::default()
    };
    mock.sync_script.lock().expect("script").push_back(SyncStep::UpdatesThen(vec![bare], None));
    let addr = spawn_mock(mock).await;

    let mut session = SyncSession::new(connect_plain(addr, client_keys()).await, peer_meta());
    session.connect().await.expect("connect");
    let update = session.next_update().await.expect("update");
    assert!(update.network_map.is_none(), "no NetworkMap in this snapshot");
    assert!(update.netbird_config.is_none());
    assert_eq!(
        update.session_deadline_unix,
        Some(0),
        "set-zero is Some(0) = explicit expiry-disabled (3-state)"
    );
}
