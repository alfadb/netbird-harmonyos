// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # grpc — NetBird management gRPC channel + `Login` (N3-2)
//!
//! Real gRPC registration/login against the NetBird management service,
//! replacing the N3-1 mock-only `register()` assumption. The wire protocol,
//! message shapes and field names below come from the pinned upstream
//! reference commit; the `.proto` lives in-repo (verbatim copy, see
//! `proto/README.md`):
//!
//! ```text
//! upstream: netbirdio/netbird @ 791401060d2b95e5f51e3439c0649729132f571e
//! path:     shared/management/proto/management.proto  (BSD-3-Clause)
//! ```
//!
//! ## Login field mapping (from that commit)
//!
//! | this API | wire field | evidence @ pinned commit |
//! | --- | --- | --- |
//! | `LoginParams::setup_key` | `LoginRequest.setupKey` (1) | management.proto L175-186 |
//! | `LoginParams::meta` | `LoginRequest.meta: PeerSystemMeta` (2) | management.proto L257-278 |
//! | `LoginParams::jwt_token` | `LoginRequest.jwtToken` (3) | management.proto L181 |
//! | `LoginParams::peer_keys.wg_pub_key` | `PeerKeys.wgPubKey: bytes` (2) | management.proto L190-196 |
//! | `LoginParams::peer_keys.ssh_pub_key` | `PeerKeys.sshPubKey: bytes` (1) | management.proto L190-196 |
//! | envelope `wgPubKey` | `EncryptedMessage.wgPubKey: string` (1) | `shared/management/client/grpc.go` `login()` (`c.key.PublicKey().String()`, i.e. base64) |
//!
//! Upstream quirk reproduced on purpose: `EncryptedMessage.wgPubKey` is the
//! base64 STRING of the peer WireGuard key, while `PeerKeys.wgPubKey` (bytes)
//! carries the BYTES OF THAT BASE64 STRING (`[]byte(c.key.PublicKey().String())`
//! in grpc.go `Register()`). We do not silently "fix" this.
//!
//! ## JWT reality (do not guess)
//!
//! `LoginResponse` = `{netbirdConfig, peerConfig, Checks, sessionExpiresAt}`
//! (management.proto L280-293). It carries **no JWT**. The `jwtToken` is an
//! INPUT (SSO token, management.proto L181); REST `/api/*` credentials come
//! from the IdP/SSO flow (out of scope) or a PAT. "Session information" the
//! response actually yields: `sessionExpiresAt` (3-state: unset → no info;
//! set-zero → expiry disabled; set → absolute deadline; comment L288-291) and
//! the peer's VPN config (`PeerConfig.address`, L404-426).
//!
//! ## Message-body encryption (N3-3): GetServerKey + NaCl crypto_box
//!
//! Upstream wraps each RPC payload in `EncryptedMessage.body` **NaCl-box
//! encrypted** after a `GetServerKey` key exchange. THIS INCREMENT implements
//! it: [`ManagementGrpcClient::login`] fetches the server key
//! ([`ManagementGrpcClient::get_server_key`]), seals the `LoginRequest` with
//! [`crate::envelope::seal`] and opens the reply with
//! [`crate::envelope::open`]. Wire details, key provenance (the WireGuard
//! private key is reused upstream) and nonce handling live in
//! `crate::envelope` module docs; the flow is: `GetServerKey` → seal →
//! `Login` → open (upstream `shared/management/client/grpc.go:585-637`).
//!
//! [`LoginBodyMode::Plaintext`] is an EXPLICIT opt-out that sends the raw
//! serialized `LoginRequest` as the envelope body — **NOT upstream
//! behavior**, kept only for tests/debugging against non-NetBird mock
//! servers. The default ([`LoginBodyMode::Encrypted`]) is the only
//! interop-capable path.
//!
//! ## TLS trust root — injected, never a system store
//!
//! rustls reads NO system certificate store (recorded as a risk in
//! docs/n3-stack-freeze-20260913.md). [`GrpcTlsConfig`] therefore carries the
//! PEM-encoded CA certificate(s) to trust, supplied by the caller; tonic is
//! configured ONLY with those roots (`ClientTlsConfig::ca_certificates` —
//! never `with_native_roots`/`with_webpki_roots`). No roots → plaintext
//! [`GrpcTransport::Plaintext`] is rejected for `https://` endpoints and
//! vice versa.
//!
//! ## Error classification
//!
//! The six N3-1 classes are reused ([`crate::management::ManagementError`]);
//! gRPC status codes map per [`map_grpc_status`] (unit-tested below).
//! Envelope crypto failures reuse the same classes — see `crate::envelope`
//! module docs ("Error classification").
//!
//! ## N3-4 additions: Sync stream, Logout, ExtendAuthSession
//!
//! - [`ManagementGrpcClient::open_sync_stream`] opens the `Sync` RPC
//!   (`management.proto` L20: `rpc Sync(EncryptedMessage) returns (stream
//!   EncryptedMessage)` — **server-streaming, one request frame**, NOT
//!   bidi; the bidi RPC upstream is `Job`, L53). The first (and only)
//!   client frame is the sealed `SyncRequest{meta}`
//!   (upstream `shared/management/client/grpc.go:478-495
//!   connectToSyncStream`); frames come back as sealed `SyncResponse`s.
//!   Session state lives in [`crate::sync::SyncSession`].
//! - [`ManagementGrpcClient::logout`] mirrors upstream
//!   `GrpcClient.Logout` (grpc.go:848-872): seal `Empty{}` → `Logout` →
//!   the `Empty` reply is ignored (not an envelope).
//! - [`ManagementGrpcClient::extend_auth_session`] mirrors upstream
//!   `GrpcClient.ExtendAuthSession` (grpc.go:662-699): seal
//!   `ExtendAuthSessionRequest{jwtToken, meta}` → `ExtendAuthSession` →
//!   open `ExtendAuthSessionResponse` and surface `sessionExpiresAt`.
//!   Failure classification goes through the same [`map_grpc_status`]
//!   six classes as login.
//!
//! Per-RPC deadlines stay the client-wide `request_timeout` passed to
//! [`ManagementGrpcClient::connect`] (the established N3-2 contract).
//! Upstream uses per-call contexts instead: Logout 15s (grpc.go:854) and
//! ExtendAuthSession 10s = `ConnectTimeout` (grpc.go:33, L681) — keep those
//! numbers in mind when choosing `request_timeout`.

use crate::envelope::{EnvelopeKeyPair, EnvelopePublicKey};
use crate::management::ManagementError;
use prost::Message as _;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint};
use tonic::Status;

/// Pinned upstream reference commit this module's protocol facts come from.
pub const UPSTREAM_COMMIT: &str = "791401060d2b95e5f51e3439c0649729132f571e";

/// Generated protobuf + tonic stubs for `management.proto`
/// (`$OUT_DIR/management.rs` — messages and client/server code in one
/// file with tonic-prost-build 0.14 `compile_fds`).
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/management.rs"));
}

// ---------------------------------------------------------------------------
// TLS configuration (injectable trust root)
// ---------------------------------------------------------------------------

/// Which transport the channel uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrpcTransport {
    /// Plaintext h2 (no TLS). Dev/test only: credentials in clear.
    Plaintext,
    /// TLS with a caller-injected trust root (PEM CA certificates).
    /// There is no default system store: rustls reads none, and this module
    /// never calls tonic's `with_native_roots`/`with_webpki_roots`.
    Tls(GrpcTlsConfig),
}

/// Caller-injected TLS trust root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrpcTlsConfig {
    /// PEM-encoded CA certificate(s) to trust (at least one).
    pub root_certs_pem: Vec<Vec<u8>>,
    /// Optional SNI/verification name override; defaults to the endpoint host.
    pub server_name: Option<String>,
}

impl GrpcTlsConfig {
    /// TLS config trusting exactly `root_certs_pem`.
    pub fn new(root_certs_pem: Vec<Vec<u8>>) -> Self {
        GrpcTlsConfig { root_certs_pem, server_name: None }
    }

    /// Override the name used for certificate verification.
    pub fn with_server_name(mut self, name: impl Into<String>) -> Self {
        self.server_name = Some(name.into());
        self
    }

    fn tonic_config(&self) -> Result<ClientTlsConfig, ManagementError> {
        if self.root_certs_pem.is_empty() {
            return Err(ManagementError::Request {
                status: 0,
                message: "TLS requested but no CA certificates provided \
                          (trust root must be injected; no system store is used)"
                    .into(),
            });
        }
        let certs = self
            .root_certs_pem
            .iter()
            .map(|pem| Certificate::from_pem(pem.clone()));
        let mut cfg = ClientTlsConfig::new().ca_certificates(certs);
        if let Some(name) = &self.server_name {
            cfg = cfg.domain_name(name.clone());
        }
        Ok(cfg)
    }
}

// ---------------------------------------------------------------------------
// request/response models
// ---------------------------------------------------------------------------

/// Peer WireGuard/SSH public keys (`PeerKeys`, management.proto L190-196).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerKeySet {
    /// WireGuard public key. Wire format upstream quirk: the BYTES OF THE
    /// BASE64 STRING (see module docs). Empty = omit.
    pub wg_pub_key: Vec<u8>,
    /// SSH public key (optional). Empty = omit.
    pub ssh_pub_key: Vec<u8>,
}

/// Minimal peer system metadata (`PeerSystemMeta` subset,
/// management.proto L257-278; unknown-to-us fields are simply not set).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerMeta {
    pub hostname: String,
    /// OS family (`goOS`, field 2) — e.g. "harmonyos".
    pub os_name: String,
    /// OS release string (`OSVersion`, field 10).
    pub os_version: String,
    /// Client version (`netbirdVersion`, field 7).
    pub netbird_version: String,
}

/// Parameters for [`ManagementGrpcClient::login`] — mirrors upstream
/// `GrpcClient::Register(...)` (`shared/management/client/grpc.go`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoginParams {
    /// Pre-authorized setup key (can be empty for SSO logins).
    pub setup_key: String,
    /// SSO JWT token (can be empty for setup-key registration).
    pub jwt_token: String,
    pub peer_keys: PeerKeySet,
    pub meta: PeerMeta,
}

/// How the `EncryptedMessage.body` is produced/consumed on `login`.
///
/// Upstream ALWAYS encrypts (`grpc.go:595` `EncryptMessage`); the plaintext
/// variant here is a LOCAL test/debug extension, not an upstream mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LoginBodyMode {
    /// `GetServerKey` → NaCl `crypto_box` seal → `Login`, reply opened with
    /// the same server key. The upstream wire behavior and the default.
    #[default]
    Encrypted,
    /// Raw serialized `LoginRequest` in `EncryptedMessage.body` (N3-2
    /// intermediate contract). **NOT upstream behavior** — debug/test only.
    Plaintext,
}

/// The session-relevant parts of a decoded `LoginResponse`.
#[derive(Debug, Clone, PartialEq)]
pub struct LoginOutcome {
    /// Full decoded response (netbirdConfig / peerConfig / checks included).
    pub response: proto::LoginResponse,
    /// `sessionExpiresAt`, 3-state per management.proto L288-291:
    /// `None` = field unset (no info — keep any deadline you had);
    /// `Some(0)` = explicit "expiry disabled"; `Some(t)` = absolute deadline
    /// in unix seconds.
    pub session_deadline_unix: Option<i64>,
    /// The peer's assigned VPN IP (`PeerConfig.address`, L404-406), if set.
    pub peer_address: Option<String>,
}

// ---------------------------------------------------------------------------
// gRPC status → fixed error classes
// ---------------------------------------------------------------------------

/// Map a tonic/gRPC status onto the six N3-1 error classes. The numeric gRPC
/// code is preserved in `status` (u16), the server message in `message`.
///
/// | gRPC code | class | rationale |
/// | --- | --- | --- |
/// | Unauthenticated, PermissionDenied | `Auth` | credential/authorization rejected (upstream: invalid setup key → PermissionDenied, management.proto L33-35) |
/// | InvalidArgument, NotFound, AlreadyExists, FailedPrecondition, OutOfRange, Aborted, Cancelled, Unimplemented | `Request` | the call itself was rejected |
/// | Internal, Unknown, DataLoss, ResourceExhausted | `Server` | server-side failure |
/// | Unavailable | `Network` | transient connectivity (server unreachable) |
/// | DeadlineExceeded | `Timeout` | per-request budget exhausted |
pub fn map_grpc_status(status: Status) -> ManagementError {
    let code = status.code();
    let message = status.message().to_string();
    let num = code as u16;
    match code {
        tonic::Code::Unauthenticated | tonic::Code::PermissionDenied => {
            ManagementError::Auth { status: num, message }
        }
        tonic::Code::InvalidArgument
        | tonic::Code::NotFound
        | tonic::Code::AlreadyExists
        | tonic::Code::FailedPrecondition
        | tonic::Code::OutOfRange
        | tonic::Code::Aborted
        | tonic::Code::Cancelled
        | tonic::Code::Unimplemented => ManagementError::Request { status: num, message },
        tonic::Code::Internal
        | tonic::Code::Unknown
        | tonic::Code::DataLoss
        | tonic::Code::ResourceExhausted => ManagementError::Server { status: num },
        tonic::Code::Unavailable => ManagementError::Network(format!(
            "grpc UNAVAILABLE ({num}): {message}"
        )),
        tonic::Code::DeadlineExceeded => ManagementError::Timeout,
        _ => ManagementError::Server { status: num },
    }
}

/// Map a tonic transport (connect/handshake/IO) failure onto the fixed
/// classes. TLS verification failures surface here (handshake error).
fn map_transport_error(stage: &'static str, e: tonic::transport::Error) -> ManagementError {
    ManagementError::Network(format!("{stage}: {e}"))
}

// ---------------------------------------------------------------------------
// client
// ---------------------------------------------------------------------------

/// Management gRPC client: a tonic `Channel` + the generated
/// `ManagementService` client stub. Async API: the future NAPI layer owns a
/// tokio runtime (frozen stack) and will wrap these calls.
#[derive(Debug, Clone)]
pub struct ManagementGrpcClient {
    stub: proto::management_service_client::ManagementServiceClient<Channel>,
    /// Device identity keys for the message-body envelope. Upstream injects
    /// the WireGuard private key here (`grpc.go:128 NewClient(...
    /// ourPrivateKey wgtypes.Key ...)` fed from `connect.go:243`).
    envelope_keys: EnvelopeKeyPair,
    /// Per-RPC deadline, enforced client-side (tokio::time::timeout).
    request_timeout: core::time::Duration,
}

impl ManagementGrpcClient {
    /// Connect to `endpoint` (`https://host:port` or `http://host:port`).
    ///
    /// - `envelope_keys` is the device NaCl/WireGuard identity key pair used
    ///   to seal/unseal RPC bodies (see [`crate::envelope`]).
    /// - `https://` requires [`GrpcTransport::Tls`] with an injected trust
    ///   root; `http://` requires [`GrpcTransport::Plaintext`]. Mismatches are
    ///   rejected before any I/O.
    /// - `connect_timeout` bounds connection establishment (incl. TLS
    ///   handshake); `request_timeout` bounds each RPC (enforced by this
    ///   client via `tokio::time::timeout`, mapped to
    ///   [`ManagementError::Timeout`] — NOT via tonic's endpoint timeout
    ///   layer, which surfaces expired deadlines as `Cancelled`).
    pub async fn connect(
        endpoint: &str,
        transport: GrpcTransport,
        connect_timeout: core::time::Duration,
        request_timeout: core::time::Duration,
        envelope_keys: EnvelopeKeyPair,
    ) -> Result<Self, ManagementError> {
        let endpoint_url = endpoint.to_string();
        let mut ep = Endpoint::from_shared(endpoint_url.clone())
            .map_err(|e| {
                ManagementError::UnsupportedUrl(format!("{endpoint:?}: bad endpoint uri ({e})"))
            })?
            .connect_timeout(connect_timeout);

        match (&transport, endpoint_url.starts_with("https://")) {
            (GrpcTransport::Tls(tls), true) => {
                ep = ep.tls_config(tls.tonic_config()?)
                    .map_err(|e| map_transport_error("tls config", e))?;
            }
            (GrpcTransport::Tls(_), false) => {
                return Err(ManagementError::UnsupportedUrl(format!(
                    "{endpoint:?}: TLS configured but endpoint is not https://"
                )));
            }
            (GrpcTransport::Plaintext, true) => {
                return Err(ManagementError::UnsupportedUrl(format!(
                    "{endpoint:?}: https endpoint requires an injected TLS trust root"
                )));
            }
            (GrpcTransport::Plaintext, false) => {}
        }

        let channel = ep.connect().await.map_err(|e| {
            // Connect failures include TLS verification failures (the
            // handshake happens during/around connect).
            map_transport_error("connect", e)
        })?;
        Ok(ManagementGrpcClient {
            stub: proto::management_service_client::ManagementServiceClient::new(channel),
            envelope_keys,
            request_timeout,
        })
    }

    /// Crate-internal clone of the client identity keys — the streaming
    /// session layer (`crate::sync`) opens pushed frames with the same key
    /// pair that sealed its requests.
    #[doc(hidden)]
    pub(crate) fn envelope_keys(&self) -> crate::envelope::EnvelopeKeyPair {
        self.envelope_keys.clone()
    }

    /// `ManagementService/GetServerKey` (management.proto L24, L314-320):
    /// the server's base64 WireGuard public key, used as the envelope peer
    /// key. Upstream wraps this with a 5s context (grpc.go:537-549); the
    /// deadline here is the client-wide `request_timeout` (Timeout class),
    /// status failures map through [`map_grpc_status`], and a malformed
    /// `key` string is `ManagementError::Parse`.
    pub async fn get_server_key(&mut self) -> Result<EnvelopePublicKey, ManagementError> {
        let reply = tokio::time::timeout(self.request_timeout, async {
            self.stub
                .get_server_key(tonic::Request::new(proto::Empty {}))
                .await
                .map_err(map_grpc_status)
        })
        .await
        .map_err(|_| ManagementError::Timeout)??
        .into_inner();
        EnvelopePublicKey::from_base64(&reply.key)
    }

    /// `ManagementService/Login` with the DEFAULT body mode
    /// ([`LoginBodyMode::Encrypted`] — the upstream wire behavior).
    pub async fn login(&mut self, params: LoginParams) -> Result<LoginOutcome, ManagementError> {
        self.login_with_mode(params, LoginBodyMode::Encrypted).await
    }

    /// `ManagementService/Login` (management.proto L33-36, L175-186) with an
    /// explicit body mode. The encrypted path mirrors upstream
    /// `GrpcClient::login` (grpc.go:585-637): `GetServerKey` → seal body →
    /// send → open reply with the same server key. The peer identity on the
    /// envelope AND in `PeerKeys.wgPubKey` always comes from the injected
    /// key pair (upstream `c.key`; `PeerKeys.wgPubKey` = the BYTES OF THE
    /// base64 STRING, grpc.go:643-647), overriding any caller-set value.
    pub async fn login_with_mode(
        &mut self,
        params: LoginParams,
        mode: LoginBodyMode,
    ) -> Result<LoginOutcome, ManagementError> {
        if params.setup_key.is_empty() && params.jwt_token.is_empty() {
            return Err(ManagementError::Request {
                status: 0,
                message: "login needs a setup key or a jwt token (both empty)".into(),
            });
        }

        let mut request = Self::build_login_request(&params);
        // Upstream single source of identity: our own public key, base64
        // string for EncryptedMessage.wgPubKey (grpc.go:608) and the bytes
        // of that string for PeerKeys.wgPubKey (grpc.go:644).
        let wg_pub_key_b64 = self.envelope_keys.public_key_base64();
        if let Some(peer_keys) = request.peer_keys.as_mut() {
            peer_keys.wg_pub_key = wg_pub_key_b64.clone().into_bytes();
        }

        // Fetched ONCE per login and reused for the reply (upstream keeps
        // `serverKey` in scope across send + DecryptMessage, grpc.go:587-631).
        let mut server_key: Option<EnvelopePublicKey> = None;
        let body = match mode {
            LoginBodyMode::Encrypted => {
                let key = self.get_server_key().await?;
                let sealed =
                    crate::envelope::seal(&key, &self.envelope_keys, &request.encode_to_vec())?;
                server_key = Some(key);
                sealed
            }
            LoginBodyMode::Plaintext => request.encode_to_vec(),
        };
        let envelope = proto::EncryptedMessage {
            wg_pub_key: wg_pub_key_b64,
            body,
            version: 0, // upstream Go does not set it on Login either
        };

        let reply = tokio::time::timeout(self.request_timeout, async {
            self.stub
                .login(tonic::Request::new(envelope))
                .await
                .map_err(map_grpc_status)
        })
        .await
        .map_err(|_| ManagementError::Timeout)?? // outer: our deadline; inner: mapped status
        .into_inner();

        let response = match mode {
            LoginBodyMode::Encrypted => {
                // open with the SAME server key the request was sealed for
                // (grpc.go:627-631 DecryptMessage(*serverKey, c.key, ...)).
                let key = server_key
                    .expect("encrypted mode fetches the server key before sending");
                let plaintext = crate::envelope::open(&key, &self.envelope_keys, &reply.body)?;
                proto::LoginResponse::decode(plaintext.as_slice())
            }
            LoginBodyMode::Plaintext => proto::LoginResponse::decode(reply.body.as_slice()),
        }
        .map_err(|e| ManagementError::Parse(format!("LoginResponse decode failed: {e}")))?;

        // 3-state sessionExpiresAt (management.proto L288-291).
        let session_deadline_unix = response.session_expires_at.as_ref().map(|ts| ts.seconds);
        let peer_address = response
            .peer_config
            .as_ref()
            .map(|cfg| cfg.address.clone())
            .filter(|addr| !addr.is_empty());
        Ok(LoginOutcome { response, session_deadline_unix, peer_address })
    }

    /// Assemble the wire `LoginRequest` (also used by tests to assert field
    /// mapping without a server).
    pub fn build_login_request(params: &LoginParams) -> proto::LoginRequest {
        proto::LoginRequest {
            setup_key: params.setup_key.clone(),
            jwt_token: params.jwt_token.clone(),
            peer_keys: Some(proto::PeerKeys {
                ssh_pub_key: params.peer_keys.ssh_pub_key.clone(),
                wg_pub_key: params.peer_keys.wg_pub_key.clone(),
            }),
            meta: Some(build_peer_system_meta(&params.meta)),
            dns_labels: Vec::new(),
        }
    }

    /// Open the `Sync` server-streaming RPC — the wire shape of upstream
    /// `connectToSyncStream` (grpc.go:478-495): exactly ONE request frame,
    /// the sealed `SyncRequest{meta}` (`SyncRequest` has only `meta`,
    /// management.proto L126-128 — there is no `msg` field), inside
    /// `EncryptedMessage{wgPubKey: our base64 public key, body}`. The
    /// server key must be fetched fresh (upstream fetches it per attempt,
    /// grpc.go:260) and is what the reply frames are sealed for.
    ///
    /// This only opens the stream (headers + first frame sent); frame
    /// consumption, decoding and reconnect policy live in
    /// [`crate::sync::SyncSession`].
    pub async fn open_sync_stream(
        &mut self,
        meta: &PeerMeta,
        server_key: &EnvelopePublicKey,
    ) -> Result<tonic::Streaming<proto::EncryptedMessage>, ManagementError> {
        let request = proto::SyncRequest { meta: Some(build_peer_system_meta(meta)) };
        let body = crate::envelope::seal(server_key, &self.envelope_keys, &request.encode_to_vec())?;
        let envelope = proto::EncryptedMessage {
            wg_pub_key: self.envelope_keys.public_key_base64(),
            body,
            // upstream does not set version on the sync request either
            // (grpc.go:489 builds EncryptedMessage without Version)
            version: 0,
        };
        let response = tokio::time::timeout(self.request_timeout, async {
            self.stub
                .sync(tonic::Request::new(envelope))
                .await
                .map_err(map_grpc_status)
        })
        .await
        .map_err(|_| ManagementError::Timeout)??;
        Ok(response.into_inner())
    }

    /// `ManagementService/Logout` — upstream `GrpcClient.Logout`
    /// (grpc.go:848-872): seal `proto.Empty{}` and send it; the reply is a
    /// plain `Empty` (management.proto L50), NOT an `EncryptedMessage`, and
    /// is discarded without opening. One attempt, no retry (unlike login).
    ///
    /// Status failures map through [`map_grpc_status`]; note the daemon
    /// layer upstream additionally treats `NotFound` as success ("peer
    /// already gone", client/server/server.go:1602-1611 +
    /// logoutPeerGone L2754-2761) — this primitive reports it as
    /// [`ManagementError::Request`] and lets the caller decide.
    pub async fn logout(&mut self) -> Result<(), ManagementError> {
        let server_key = self.get_server_key().await?;
        let body = crate::envelope::seal(
            &server_key,
            &self.envelope_keys,
            &proto::Empty {}.encode_to_vec(),
        )?;
        let envelope = proto::EncryptedMessage {
            wg_pub_key: self.envelope_keys.public_key_base64(),
            body,
            version: 0,
        };
        let _reply = tokio::time::timeout(self.request_timeout, async {
            self.stub
                .logout(tonic::Request::new(envelope))
                .await
                .map_err(map_grpc_status)
        })
        .await
        .map_err(|_| ManagementError::Timeout)??;
        Ok(())
    }

    /// `ManagementService/ExtendAuthSession` — upstream
    /// `GrpcClient.ExtendAuthSession` (grpc.go:662-699): seal
    /// `ExtendAuthSessionRequest{jwtToken, meta}` (fields 1/2,
    /// management.proto L298-304) and open the sealed
    /// `ExtendAuthSessionResponse{sessionExpiresAt}` (L306-310). Refreshes
    /// only the SSO session deadline; no network-map sync, no tunnel churn
    /// (grpc.go:658-661 comment, engine_authsession.go:72-107).
    ///
    /// An empty JWT is rejected locally (`Request{0}`) — upstream engine
    /// guards the same way (engine_authsession.go:76-78).
    pub async fn extend_auth_session(
        &mut self,
        meta: &PeerMeta,
        jwt_token: &str,
    ) -> Result<ExtendAuthSessionOutcome, ManagementError> {
        if jwt_token.is_empty() {
            return Err(ManagementError::Request {
                status: 0,
                message: "jwt token is required (empty token cannot refresh a session)".into(),
            });
        }
        let server_key = self.get_server_key().await?;
        let request = proto::ExtendAuthSessionRequest {
            jwt_token: jwt_token.to_string(),
            meta: Some(build_peer_system_meta(meta)),
        };
        let body =
            crate::envelope::seal(&server_key, &self.envelope_keys, &request.encode_to_vec())?;
        let envelope = proto::EncryptedMessage {
            wg_pub_key: self.envelope_keys.public_key_base64(),
            body,
            version: 0,
        };
        let reply = tokio::time::timeout(self.request_timeout, async {
            self.stub
                .extend_auth_session(tonic::Request::new(envelope))
                .await
                .map_err(map_grpc_status)
        })
        .await
        .map_err(|_| ManagementError::Timeout)?? // outer deadline; inner mapped status
        .into_inner();
        // open with the SAME server key the request was sealed for
        // (grpc.go:693-698 DecryptMessage(*serverKey, c.key, ...))
        let plaintext = crate::envelope::open(&server_key, &self.envelope_keys, &reply.body)?;
        let response = proto::ExtendAuthSessionResponse::decode(plaintext.as_slice())
            .map_err(|e| ManagementError::Parse(format!("ExtendAuthSessionResponse decode failed: {e}")))?;
        Ok(ExtendAuthSessionOutcome {
            // same 3-state encoding as LoginResponse.sessionExpiresAt
            session_deadline_unix: response.session_expires_at.map(|ts| ts.seconds),
        })
    }
}

/// New absolute SSO session deadline from
/// [`ManagementGrpcClient::extend_auth_session`].
///
/// `session_deadline_unix` mirrors [`LoginOutcome::session_deadline_unix`]
/// 3-state semantics: `None` = field unset (no info — keep the deadline
/// already anchored), `Some(0)` = explicit "expiry disabled / not SSO",
/// `Some(t)` = absolute deadline in unix seconds
/// (management.proto L306-310; decoding at engine_authsession.go:32-66).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtendAuthSessionOutcome {
    pub session_deadline_unix: Option<i64>,
}

/// Wire `PeerSystemMeta` from the minimal [`PeerMeta`] — shared by Login,
/// Sync first frame and ExtendAuthSession (upstream `infoToMetaData`,
/// grpc.go:995-1057, fills the full posture-check surface; we send the
/// identity subset and leave capabilities EMPTY and `syncMessageVersion = 0`
/// (`Base`, shared/management/grpc/sync_message_versions.go:9-16): we do
/// NOT advertise `PeerCapabilityComponentNetworkMap`, so management keeps
/// sending the legacy full `NetworkMap` this crate consumes — and a
/// components envelope arriving anyway is an explicit error, see
/// `crate::sync`).
pub fn build_peer_system_meta(meta: &PeerMeta) -> proto::PeerSystemMeta {
    proto::PeerSystemMeta {
        hostname: meta.hostname.clone(),
        go_os: meta.os_name.clone(),
        os_version: meta.os_version.clone(),
        netbird_version: meta.netbird_version.clone(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// unit tests: status mapping + LoginRequest field mapping (no I/O)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn status(code: tonic::Code, msg: &str) -> Status {
        Status::new(code, msg)
    }

    #[test]
    fn grpc_permission_denied_and_unauthenticated_map_to_auth() {
        let e = map_grpc_status(status(tonic::Code::PermissionDenied, "invalid setup key"));
        assert_eq!(
            e,
            ManagementError::Auth { status: 7, message: "invalid setup key".into() }
        );
        let e = map_grpc_status(status(tonic::Code::Unauthenticated, "bad jwt"));
        assert_eq!(e, ManagementError::Auth { status: 16, message: "bad jwt".into() });
    }

    #[test]
    fn grpc_request_class_codes_map_to_request() {
        for code in [
            tonic::Code::InvalidArgument,
            tonic::Code::NotFound,
            tonic::Code::AlreadyExists,
            tonic::Code::FailedPrecondition,
            tonic::Code::OutOfRange,
            tonic::Code::Aborted,
            tonic::Code::Cancelled,
            tonic::Code::Unimplemented,
        ] {
            let e = map_grpc_status(status(code, "no"));
            assert!(
                matches!(e, ManagementError::Request { .. }),
                "{code:?} should map to Request, got {e:?}"
            );
        }
    }

    #[test]
    fn grpc_server_class_codes_map_to_server() {
        for code in [
            tonic::Code::Internal,
            tonic::Code::Unknown,
            tonic::Code::DataLoss,
            tonic::Code::ResourceExhausted,
        ] {
            let e = map_grpc_status(status(code, "boom"));
            assert!(
                matches!(e, ManagementError::Server { .. }),
                "{code:?} should map to Server, got {e:?}"
            );
        }
    }

    #[test]
    fn grpc_unavailable_is_network_and_deadline_is_timeout() {
        let e = map_grpc_status(status(tonic::Code::Unavailable, "connection refused"));
        assert!(matches!(e, ManagementError::Network(_)), "got {e:?}");
        assert!(matches!(
            map_grpc_status(status(tonic::Code::DeadlineExceeded, "late")),
            ManagementError::Timeout
        ));
    }

    #[test]
    fn login_request_field_mapping_matches_proto() {
        let params = LoginParams {
            setup_key: "SETUP-KEY-1".into(),
            jwt_token: String::new(),
            peer_keys: PeerKeySet {
                wg_pub_key: b"BASE64WGKEY".to_vec(),
                ssh_pub_key: b"SSHPUB".to_vec(),
            },
            meta: PeerMeta {
                hostname: "ohos-dev".into(),
                os_name: "harmonyos".into(),
                os_version: "26.0.0".into(),
                netbird_version: "0.1.0".into(),
            },
        };
        let req = ManagementGrpcClient::build_login_request(&params);
        assert_eq!(req.setup_key, "SETUP-KEY-1");
        assert_eq!(req.jwt_token, "");
        let keys = req.peer_keys.as_ref().expect("peer keys set");
        assert_eq!(keys.wg_pub_key, b"BASE64WGKEY".to_vec());
        assert_eq!(keys.ssh_pub_key, b"SSHPUB".to_vec());
        let meta = req.meta.as_ref().expect("meta set");
        assert_eq!(meta.hostname, "ohos-dev");
        assert_eq!(meta.go_os, "harmonyos");
        assert_eq!(meta.os_version, "26.0.0");
        assert_eq!(meta.netbird_version, "0.1.0");
        assert!(req.dns_labels.is_empty());

        // round-trip through the wire encoding used for EncryptedMessage.body
        let bytes = req.encode_to_vec();
        let decoded = proto::LoginRequest::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded.setup_key, "SETUP-KEY-1");
    }

    #[test]
    fn tls_config_requires_injected_roots() {
        let err = GrpcTlsConfig::new(Vec::new()).tonic_config().unwrap_err();
        assert!(
            matches!(err, ManagementError::Request { status: 0, .. }),
            "got {err:?}"
        );
        let ok = GrpcTlsConfig::new(vec![b"-----BEGIN CERTIFICATE-----\n".to_vec()])
            .tonic_config();
        assert!(ok.is_ok(), "PEM root accepted: {ok:?}");
    }
}
