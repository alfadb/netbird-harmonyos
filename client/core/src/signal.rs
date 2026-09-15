// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # signal — NetBird signal service channel (N4a)
//!
//! The peer discovery / candidate exchange transport: a gRPC client for the
//! `SignalExchange` service, riding the SAME protected-socket seam and the
//! SAME NaCl envelope primitives as the management channel (no second stack,
//! per governance `docs/native-nx-governance.md` §二 item 4 — "signal" is an
//! enumerated socket that must be protected per FIRST connect AND per
//! reconnect, fail-closed).
//!
//! ## Upstream recon: what signal really is (pinned commit
//! `791401060d2b95e5f51e3439c0649729132f571e` — [`crate::grpc::UPSTREAM_COMMIT`])
//!
//! **It is a gRPC (bidi streaming) service, not a WebSocket.** Evidence:
//!
//! - `shared/signal/proto/signalexchange.proto:9-14`:
//!   `service SignalExchange` with `rpc Send(EncryptedMessage) returns
//!   (EncryptedMessage)` (L11, unary) and `rpc ConnectStream(stream
//!   EncryptedMessage) returns (stream EncryptedMessage)` (L13, bidi).
//!   → NO new dependency: the existing tonic 0.14 stack generates both stubs
//!   (an offline `cargo build` was verified; tokio gained only the `sync`
//!   feature flag for the mpsc request stream — no new crate, lockfile
//!   unchanged).
//! - **Entry URL / TLS**: the URI + protocol come from the management
//!   LoginResponse `NetbirdConfig.signal` (`HostConfig`, management.proto
//!   L341-355: `uri` L348, `protocol` L349 with `HTTPS = 3` L352). Upstream:
//!   `client/internal/connect.go:715-731 connectToSignal` — TLS enabled iff
//!   `wtConfig.Signal.Protocol == HTTPS` (L717-721), then
//!   `signal.NewClient(ctx, wtConfig.Signal.Uri, ourPrivateKey, sigTLSEnabled)`
//!   (L722-723) dials the URI through `client/grpc/dialer.go:31-66
//!   CreateConnection` (system roots + embedded fallback, L35-43 — OUR trust
//!   root stays INJECTED, `GrpcTlsConfig`, never a system store).
//! - **Registration** is header-based on `ConnectStream` (there is no
//!   register request message): the client attaches metadata
//!   `x-wiretrustee-peer-id: <base64 WireGuard public key>`
//!   (`shared/signal/proto/constants.go:4`, `shared/signal/client/grpc.go:311-314`)
//!   and then BLOCKS on the response headers requiring
//!   `x-wiretrustee-peer-registered` (grpc.go:320-327). Server side:
//!   `signal/server/signal.go:134-152 RegisterPeer` reads the header (L136;
//!   missing → `FailedPrecondition` L137-140), registers the peer, and the
//!   stream handler confirms with `SendHeader(successHeader)` where
//!   `successHeader = x-wiretrustee-peer-registered: 1` (L87, L117-121).
//! - **The server is a pure forwarder** — and in the DEPLOYED generation
//!   (v0.78.1, still true on main) it forwards ONLY frames that arrive on
//!   the unary `Send` RPC (`signal.go:95-104`: lookup `msg.remote_key` in
//!   the registry, forward; unknown destination → dispatcher). The
//!   `ConnectStream` handler (`signal.go:106-132`) registers the peer,
//!   confirms with the success header, and then blocks on
//!   `stream.Context().Done()` — it NEVER calls `stream.Recv()`. Frames a
//!   client pushes onto the stream body are ACKed by the gRPC transport
//!   (HTTP/2 flow control) but never read by the application: no forward,
//!   no error, no log — a silent black hole. That is exactly how the
//!   upstream client drives it: `client/internal/peer/signaler.go:36-66`
//!   sends OFFER/ANSWER/CANDIDATE/GO_IDLE through `signal.Send` (the UNARY,
//!   `grpc.go:454-492`), the receive-watchdog probe is unary too
//!   (`grpc.go:555-566`), and `SendToStream` (`grpc.go:396-411`) has no
//!   callers in the engine. THIS crate originally pushed frames onto the
//!   stream body and passed every mock test — the mock implemented the OLD
//!   upstream shape (a Recv loop inside `ConnectStream`) and forwarded
//!   them. Against the real 0.78.1 server the frames never left the
//!   process observably (N10b root cause; see
//!   `docs/interop-run-1-20260913.md` §N10b). Delivery here is therefore
//!   UNARY-ONLY: [`SignalSession::send_outgoing`] routes through
//!   [`SignalClient::send`], and the stream body is held open without ever
//!   sending (see [`SignalClient::register`]).
//! - **Heartbeat / keepalive**: `Body.HEARTBEAT = 6`
//!   (signalexchange.proto L51) is used as a SELF-ADDRESSED probe the server
//!   routes back to the sender (`shared/signal/client/grpc.go:555-566
//!   sendReceiveProbe`, Key = RemoteKey = own public key). The receive
//!   watchdog fires it after 30s of stream silence and reconnects if the
//!   probe does not return within 10s (constants L29-41, watchdog
//!   L522-553). Transport keepalive: `dialer.go:53-56` (Time 30s / Timeout
//!   10s). The watchdog itself is an N5 increment; the [`SignalClient::
//!   send_heartbeat`] primitive is provided so it can be added without
//!   protocol work.
//! - **Reconnect semantics**: upstream retries the whole stream-open+receive
//!   with `defaultBackoff` — 800ms initial / randomization 1 / multiplier
//!   1.7 / max 10s / 3-month budget (`shared/signal/client/grpc.go:173-183`)
//!   — the same numbers as [`crate::backoff::ExponentialBackoff::
//!   upstream_stream_default`]. Receive-loop error split (grpc.go:570-587):
//!   `Canceled` → shutdown, `Unavailable` → retry, EOF (server closed) →
//!   retry, everything else → retry. Only `connectivity.Shutdown` is
//!   `backoff.Permanent` (grpc.go:211-213). A registration without the
//!   confirm header is a plain retried error (grpc.go:324-327).
//! - **PermissionDenied → terminate**: the signal retry loop itself only
//!   marks Shutdown permanent, but the signal client lives INSIDE the engine
//!   connect loop, which treats management-login `PermissionDenied` as
//!   `backoff.Permanent` ("unrecoverable error") and cancels the run
//!   (`client/internal/connect.go:353-356`). This crate pins that policy at
//!   the session layer: `Auth`-class errors (PermissionDenied AND
//!   Unauthenticated, [`crate::grpc::map_grpc_status`]) are
//!   [`SignalStreamError::Fatal`] and end
//!   [`SignalSession::run_events`] with the cause — the same split as the
//!   management Sync session (`crate::sync`, grpc.go:436-438 parity).
//!
//! ## Encryption: the SAME NaCl envelope, sealed PER PEER
//!
//! `EncryptedMessage.body` carries `Body` **protobuf-serialized and then
//! crypto_box-sealed** ("encrypted with the Wireguard private key and the
//! remote Peer key", signalexchange.proto L16-17):
//!
//! - seal: `encryption/encryption.go:18-24` — `box.Seal(nonce[:], msg,
//!   nonce, remotePub, ourPriv)`, i.e. wire = `nonce(24B) || ciphertext`,
//!   fresh random 24-byte nonce per message (L44-51);
//! - open: `encryption/encryption.go:27-42` — nonce from the first 24
//!   bytes, open over the remainder;
//! - keys are the WireGuard identity keys, EXACTLY as management
//!   (`connect.go:243` → the same private key is handed to the signal
//!   client, `connect.go:722-723`): incoming frames open with
//!   `peer = msg.key` (the SENDER's public key,
//!   `shared/signal/client/grpc.go:414-431 decryptMessage`), outgoing
//!   frames seal with `peer = msg.remote_key` (the RECIPIENT's public key,
//!   grpc.go:434-451 encryptMessage).
//!
//! → [`crate::envelope::seal`]/[`crate::envelope::open`] are reused
//! unchanged (byte-compatible with Go `box.Seal/Open`), with the
//! [`crate::envelope::EnvelopeKeyPair`] injected at
//! [`SignalClient::connect_with_socket_source`]. Unlike management there is
//! NO `GetServerKey` exchange here and NO JWT: the only identity is the
//! WireGuard public key header (grpc.go:311 — "identifying ourselves with a
//! public WireGuard key"). [`SignalClient::send`] seals the body for the
//! named remote key and refuses keys that are not base64/32-byte
//! ([`crate::envelope::EnvelopePublicKey::from_base64`]).
//!
//! ## Message types and roles
//!
//! `Body.Type` = `OFFER=0 / ANSWER=1 / CANDIDATE=2 / MODE=4 / GO_IDLE=5 /
//! HEARTBEAT=6` (signalexchange.proto L45-52). Either peer may send any of
//! them (the server is direction-agnostic): upstream wraps OFFER/ANSWER
//! with ICE credentials `"ufrag:pwd"` in `payload` plus `wgListenPort`,
//! `netBirdVersion`, optional rosenpass/relay/sessionId fields
//! (`shared/signal/client/client.go:74-97 MarshalCredential`, driven by
//! `client/internal/peer/signaler.go:24-79`); `CANDIDATE.payload` is the
//! marshaled ICE candidate string (signaler.go:32-41). The engine consumes
//! the decoded stream in one handler (`client/internal/engine.go:2021-2088`)
//! — that consumption is the N5 (ICE) boundary: this module delivers the
//! decrypted [`SignalMessage`] and stays ICE-agnostic.
//!
//! ## Protected socket: reuse, not a second seam
//!
//! Every dial — the first connect and every tonic re-dial after a pooled
//! connection dies, i.e. EVERY reconnect — takes a fresh socket from the
//! injected [`crate::mgmtsock::ManagementSocketProvider`] through the
//! [`crate::mgmtsock::ProtectedSocketConnector`] (`mgmtsock.rs` N3-7
//! component used as-is: per-dial `take_fd` → dup → connect AFTER protect;
//! empty provider fails the dial — fail-closed, no unprotected fallback).
//! DNS is resolved shell-side (`connect_addr`), the endpoint URL host still
//! drives TLS SNI/verification — the management `connect_with_socket_source`
//! contract verbatim. There is deliberately NO unprotected constructor.
//!
//! ## Loop model (mirrors `crate::sync`)
//!
//! [`SignalSession::run_events`] emits [`SignalLoopEvent`]::
//! `Registered` (stream registered, incl. reconnects), `Message`
//! (decrypted [`SignalMessage`]), `Malformed` (a frame failed
//! envelope-open/decode — the stream STAYS UP, upstream parity: decryption
//! failures are logged in the worker and never disconnect, grpc.go:600-602
//! + 610-625), `Broken` (transport-class failure; reconnect after backoff).
//! Auth-class failures return `Err` and end the loop. Frame crypto failures
//! keep the stream; backoff exhaustion (injected [`crate::backoff`]) ends
//! the session with the pending error.
//!
//! N5d adds [`SignalSession::run_events_with_outbox`]: the same
//! register/receive/backoff policy with an outbound queue
//! ([`SignalOutgoing`]) multiplexed into every inner step — the seam the
//! per-peer ICE orchestrator pushes its offer/answer/candidate frames
//! through (queued sends never wait for inbound traffic; a queued frame
//! that was never sent survives a stream break only as far as the open
//! stream — re-signaling on reconnect is the orchestrator's outbox job).
//! N10b: queued frames are delivered through the UNARY
//! `SignalExchange/Send` (`SignalSession::send_outgoing` →
//! [`SignalClient::send`]) — the deployed server generation never reads
//! the `ConnectStream` request body, so frame delivery must not touch it
//! (module docs, "the server is a pure forwarder").

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_core::Stream;
use prost::Message as _;
use tonic::transport::Channel;
use tonic::Streaming;

use crate::backoff::{Clock, ExponentialBackoff, MonotonicClock, OsRandom, Rng};
use crate::envelope::{EnvelopeKeyPair, EnvelopePublicKey};
use crate::grpc::{map_grpc_status, GrpcTransport};
use crate::management::ManagementError;

/// Generated protobuf + tonic stubs for `signalexchange.proto`
/// (`$OUT_DIR/signalexchange.rs`; verbatim proto, see proto/README.md).
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/signalexchange.rs"));
}

// ---------------------------------------------------------------------------
// protocol constants (upstream `shared/signal/proto/constants.go:4-5`)
// ---------------------------------------------------------------------------

/// gRPC metadata header carrying OUR base64 WireGuard public key on
/// `ConnectStream` (constants.go:4; set by the client at grpc.go:312, read
/// by the server at signal.go:136).
pub const HEADER_ID: &str = "x-wiretrustee-peer-id";
/// gRPC metadata header the server sets on the response (value "1") to
/// confirm registration (constants.go:5; signal.go:87, client check
/// grpc.go:320-327).
pub const HEADER_REGISTERED: &str = "x-wiretrustee-peer-registered";

/// `netBirdVersion` stamped into outgoing `Body`s (upstream
/// `version.NetbirdVersion()`, client.go:79).
pub const SIGNAL_CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------
// models
// ---------------------------------------------------------------------------

/// A decrypted, decoded signal message (the `Message`/`Body` pair of
/// signalexchange.proto L31-39/L43-78, reduced to what a consumer needs;
/// ICE semantics stay with N5).
#[derive(Debug, Clone, PartialEq)]
pub struct SignalMessage {
    /// Base64 WireGuard public key of the SENDER (`EncryptedMessage.key`,
    /// the key the frame's envelope was sealed FOR us with).
    pub from_key: String,
    /// Base64 WireGuard public key the sender addressed (`remoteKey`) —
    /// normally our own public key.
    pub remote_key: String,
    /// `Body.type`.
    pub kind: proto::body::Type,
    /// `Body.payload` — ICE credentials `"ufrag:pwd"` for OFFER/ANSWER,
    /// the marshaled candidate for CANDIDATE (client.go:74-79,
    /// signaler.go:32-41).
    pub payload: String,
    /// `Body.wgListenPort`.
    pub wg_listen_port: u32,
    /// `Body.netBirdVersion` (sender's client version).
    pub net_bird_version: String,
    /// `Body.sessionId`.
    pub session_id: Option<Vec<u8>>,
    /// `Body.relayServerAddress`.
    pub relay_server_address: Option<String>,
}

impl SignalMessage {
    fn from_parts(key: String, remote_key: String, body: proto::Body) -> Self {
        SignalMessage {
            from_key: key,
            remote_key,
            kind: proto::body::Type::try_from(body.r#type)
                .unwrap_or(proto::body::Type::Candidate),
            payload: body.payload,
            wg_listen_port: body.wg_listen_port,
            net_bird_version: body.net_bird_version,
            session_id: body.session_id,
            relay_server_address: body.relay_server_address,
        }
    }
}

/// Failure of the active signal stream — the [`crate::sync::SyncStreamError`]
/// split: only the Auth class is unrecoverable (module docs,
/// "PermissionDenied → terminate").
#[derive(Debug, Clone, PartialEq)]
pub enum SignalStreamError {
    /// Stream ended; reconnecting is allowed (transport failure, server
    /// EOF, non-Auth gRPC status).
    Closed(ManagementError),
    /// Unrecoverable (PermissionDenied / Unauthenticated → the engine-level
    /// permanent policy, connect.go:353-356). The session loop MUST stop.
    Fatal(ManagementError),
}

impl SignalStreamError {
    fn from_management(err: ManagementError) -> Self {
        match err {
            e @ ManagementError::Auth { .. } => SignalStreamError::Fatal(e),
            e => SignalStreamError::Closed(e),
        }
    }

    /// The concrete error, for logging/reporting.
    pub fn into_inner(self) -> ManagementError {
        match self {
            SignalStreamError::Closed(e) | SignalStreamError::Fatal(e) => e,
        }
    }
}

/// One decoded receive-side outcome: a good message, or a frame that could
/// not be opened/decoded (the stream STAYS UP — upstream decrypt-worker
/// behavior, grpc.go:600-602; surfaced instead of silently dropped).
#[derive(Debug, Clone, PartialEq)]
pub enum IncomingFrame {
    Message(SignalMessage),
    Malformed(ManagementError),
}

// ---------------------------------------------------------------------------
// the client
// ---------------------------------------------------------------------------

/// Signal exchange gRPC client over the protected-socket seam.
///
/// Construct ONLY through [`SignalClient::connect_with_socket_source`]
/// (fail-closed protected dial; there is no unprotected constructor).
#[derive(Debug, Clone)]
pub struct SignalClient {
    stub: proto::signal_exchange_client::SignalExchangeClient<Channel>,
    /// Our WireGuard/NaCl identity keys: header identity
    /// (`public_key_base64`), envelope seal/open keys.
    keys: EnvelopeKeyPair,
    /// Per-RPC deadline for the unary `Send` (tokio::time::timeout →
    /// [`ManagementError::Timeout`]), the established N3-2 contract.
    request_timeout: core::time::Duration,
}

/// A registered `ConnectStream`: the inbound frame source (upstream
/// `proto.SignalExchange_ConnectStreamClient`). The request body is held
/// open and NEVER sent on — current servers never read it (see
/// [`SignalClient::register`]); all outbound frames ride the unary `Send`.
pub struct RegisteredStream {
    /// Receive [`proto::EncryptedMessage`]s pushed by the server.
    pub inbound: Streaming<proto::EncryptedMessage>,
}

// No derived Debug: the stream handles are not inspectable and must never
// leak frame content through formatting.
impl core::fmt::Debug for RegisteredStream {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RegisteredStream").finish_non_exhaustive()
    }
}

impl SignalClient {
    /// Connect to `endpoint` (`https://host:port` or `http://host:port`)
    /// with the gRPC transport running over a PROTECTED signal socket —
    /// every dial (initial AND every tonic re-dial after the pooled
    /// connection dies) takes a fresh pre-protected socket from `sockets`
    /// (governance §二 item 4; `crate::mgmtsock` module docs). `connect_addr`
    /// is the shell-resolved `ip:port` the protected socket connects to;
    /// the endpoint URL host still drives TLS SNI/certificate verification.
    ///
    /// `keys` are the device WireGuard/NaCl identity keys (upstream passes
    /// the WireGuard private key, `connect.go:722-723`); they double as the
    /// registration identity (base64 public key) and the envelope keys.
    pub async fn connect_with_socket_source(
        endpoint: &str,
        transport: GrpcTransport,
        connect_timeout: core::time::Duration,
        request_timeout: core::time::Duration,
        keys: EnvelopeKeyPair,
        sockets: Arc<dyn crate::mgmtsock::ManagementSocketProvider>,
        connect_addr: std::net::SocketAddr,
    ) -> Result<Self, ManagementError> {
        let ep = crate::grpc::build_endpoint(endpoint, &transport, connect_timeout)?;
        let connector = crate::mgmtsock::ProtectedSocketConnector::new(sockets, connect_addr);
        let channel = ep.connect_with_connector(connector).await.map_err(|e| {
            // includes protected-dial failures: no socket / connect refused /
            // TLS verification failure — fail-closed, nothing dialed bare.
            ManagementError::Network(format!("connect(protected signal socket): {e}"))
        })?;
        Ok(SignalClient {
            stub: proto::signal_exchange_client::SignalExchangeClient::new(channel),
            keys,
            request_timeout,
        })
    }

    /// Our base64 WireGuard public key — the `x-wiretrustee-peer-id`
    /// registration identity (grpc.go:312) and `EncryptedMessage.key`
    /// value (grpc.go:446-448).
    pub fn local_key(&self) -> String {
        self.keys.public_key_base64()
    }

    /// `ConnectStream` registration (upstream `connect`, grpc.go:308-330):
    /// opens the bidi stream with `x-wiretrustee-peer-id: <our public key>`
    /// and REQUIRES the `x-wiretrustee-peer-registered` confirm header in
    /// the response metadata (missing → [`ManagementError::Parse`], retried
    /// by the session like upstream grpc.go:324-327). A server rejecting
    /// the identity (e.g. `FailedPrecondition` for a missing header,
    /// signal.go:137-140) maps through [`map_grpc_status`].
    ///
    /// The request body is a stream that NEVER yields and NEVER ends: the
    /// deployed server generation (v0.78.1, still main) does not read
    /// `ConnectStream` inbound frames at all (`signal.go:106-132` blocks on
    /// `stream.Context().Done()`), and the upstream Go client likewise
    /// holds the stream open without ever sending on it — every
    /// client-originated frame rides the unary `Send` (module docs, "the
    /// server is a pure forwarder"). A pending body keeps the stream open
    /// for the server→client half while making stream-side sends
    /// structurally impossible (the N10b silent-black-hole path is gone by
    /// construction, not by discipline).
    pub async fn register(&mut self) -> Result<RegisteredStream, ManagementError> {
        let mut request = tonic::Request::new(PendingStream);
        let id = self.keys.public_key_base64();
        let value: tonic::metadata::AsciiMetadataValue = id
            .parse()
            .map_err(|e| {
                ManagementError::Request {
                    status: 0,
                    message: format!("peer id is not header-safe: {e}"),
                }
            })?;
        request.metadata_mut().insert(HEADER_ID, value);

        let response = self
            .stub
            .connect_stream(request)
            .await
            .map_err(map_grpc_status)?;
        let registered = response.metadata().get(HEADER_REGISTERED).ok_or_else(|| {
            ManagementError::Parse(
                "signal server did not confirm registration \
                 (no x-wiretrustee-peer-registered response header)"
                    .into(),
            )
        })?;
        if registered.is_empty() {
            return Err(ManagementError::Parse(
                "signal server sent an empty x-wiretrustee-peer-registered header".into(),
            ));
        }
        Ok(RegisteredStream { inbound: response.into_inner() })
    }

    /// Seal `msg.body` for `msg.remote_key` into the wire
    /// [`proto::EncryptedMessage`] (upstream `encryptMessage`,
    /// grpc.go:434-451): envelope peer = the RECIPIENT's public key, our
    /// private key; `key` = our base64 public key.
    pub fn encrypt_message(
        &self,
        msg: &proto::Message,
    ) -> Result<proto::EncryptedMessage, ManagementError> {
        let body = msg.body.as_ref().ok_or_else(|| {
            ManagementError::Request { status: 0, message: "signal message without body".into() }
        })?;
        let remote = EnvelopePublicKey::from_base64(&msg.remote_key)?;
        let sealed = crate::envelope::seal(&remote, &self.keys, &body.encode_to_vec())?;
        Ok(proto::EncryptedMessage {
            key: self.keys.public_key_base64(),
            remote_key: msg.remote_key.clone(),
            body: sealed,
        })
    }

    /// Open a received wire envelope and decode the `Body` (upstream
    /// `decryptMessage`, grpc.go:414-431): envelope peer = the SENDER's
    /// public key (`msg.key`), our private key. Wrong key / tampered data /
    /// bad protobuf → [`ManagementError::Parse`].
    pub fn decrypt_envelope(
        &self,
        envelope: &proto::EncryptedMessage,
    ) -> Result<SignalMessage, ManagementError> {
        let sender = EnvelopePublicKey::from_base64(&envelope.key)?;
        let plaintext = crate::envelope::open(&sender, &self.keys, &envelope.body)?;
        let body = proto::Body::decode(plaintext.as_slice())
            .map_err(|e| ManagementError::Parse(format!("signal Body decode failed: {e}")))?;
        Ok(SignalMessage::from_parts(
            envelope.key.clone(),
            envelope.remote_key.clone(),
            body,
        ))
    }

    /// Build a plaintext (not yet sealed) `Message` — the
    /// `MarshalCredential` shape (client.go:74-97) minus the fields this
    /// client does not produce yet (rosenpass, features, mode, sessionId):
    /// set explicitly on the returned `body` if ever needed.
    ///
    /// `relay_server_address` is OUR relay server URL (`Body` field 8,
    /// `relayServerAddress`, signalexchange.proto L66-67) — upstream stamps
    /// it into every OFFER and ANSWER (handshaker.go:211,218 both build
    /// through `buildOfferAnswer`; marshal at signaler.go:57-68), and
    /// the remote side refuses to open a relay lane toward us without it
    /// (`isRelaySupported`: `RelaySrvAddress != ""`, worker_relay.go:122-127
    /// → OpenConn at :69). `None` keeps the field absent (no-relay
    /// deployments; upstream sends empty relay fields for as long as its
    /// relay client has no instance address, handshaker.go:239-242).
    /// CANDIDATE/HEARTBEAT frames never carry it upstream (the CANDIDATE
    /// body at signaler.go:32-38 has no relay fields) — callers pass `None`
    /// there. `relayServerIP` (field 11) stays unset: upstream fills it with
    /// the RESOLVED IP of the connected relay instance
    /// (manager.go:248-263 → client.go:763-774), which this client does not
    /// track; receivers use it only as a DNS fallback, never as the
    /// lane-open trigger.
    pub fn build_message(
        &self,
        remote_key: &str,
        kind: proto::body::Type,
        payload: impl Into<String>,
        wg_listen_port: u32,
        relay_server_address: Option<&str>,
    ) -> proto::Message {
        proto::Message {
            key: self.keys.public_key_base64(),
            remote_key: remote_key.to_string(),
            body: Some(proto::Body {
                r#type: kind as i32,
                payload: payload.into(),
                wg_listen_port,
                net_bird_version: SIGNAL_CLIENT_VERSION.to_string(),
                relay_server_address: relay_server_address.map(str::to_string),
                ..Default::default()
            }),
        }
    }

    /// `SignalExchange/Send` (unary, signalexchange.proto L11): seals and
    /// delivers one message (upstream `GrpcClient::send`, grpc.go:454-492 —
    /// their 4-attempt timeout ladder is deferred; one attempt under the
    /// client-wide `request_timeout`, failing loudly). The reply is an
    /// empty `EncryptedMessage` (signal.go:100) and is discarded.
    pub async fn send(&mut self, msg: &proto::Message) -> Result<(), ManagementError> {
        let wire = self.encrypt_message(msg)?;
        let _reply = tokio::time::timeout(self.request_timeout, async {
            self.stub
                .send(tonic::Request::new(wire))
                .await
                .map_err(map_grpc_status)
        })
        .await
        .map_err(|_| ManagementError::Timeout)??;
        Ok(())
    }

    /// Self-addressed HEARTBEAT (`Key = RemoteKey = own public key`,
    /// upstream `sendReceiveProbe`, grpc.go:555-566): the server routes it
    /// straight back (signal.go:95-104), exercising the exact receive path
    /// the N5 receive watchdog will guard.
    pub async fn send_heartbeat(&mut self) -> Result<(), ManagementError> {
        let msg = self.build_message(
            &self.keys.public_key_base64(),
            proto::body::Type::Heartbeat,
            "",
            0,
            None,
        );
        self.send(&msg).await
    }
}

// ---------------------------------------------------------------------------
// session (run_events loop, mirrors crate::sync)
// ---------------------------------------------------------------------------

struct ActiveStream {
    frames: Streaming<proto::EncryptedMessage>,
}

/// One outbound frame queued by the exchange seam (N5d) and drained by the
/// session loop onto the ACTIVE stream. Identity stamping (`Message.key` =
/// our public key) happens inside [`SignalSession::send_outgoing`] — the
/// queue never carries a spoofable identity (grpc.go:446-448 parity).
#[derive(Debug, Clone, PartialEq)]
pub struct SignalOutgoing {
    /// Base64 WireGuard public key of the RECIPIENT (`remoteKey` — the
    /// envelope is sealed FOR this key, grpc.go:434-451).
    pub remote_key: String,
    /// `Body.type` (OFFER/ANSWER/CANDIDATE are the ICE-relevant subset;
    /// other values pass through untouched — the server is
    /// direction-agnostic).
    pub kind: proto::body::Type,
    /// `Body.payload` — `"ufrag:pwd"` for OFFER/ANSWER, the marshaled
    /// candidate for CANDIDATE (client.go:74-101, signaler.go:32-41).
    pub payload: String,
    /// `Body.wgListenPort`.
    pub wg_listen_port: u32,
    /// OUR advertised relay server URL (`Body.relayServerAddress`, field 8)
    /// for OFFER/ANSWER — the field that makes the remote peer open a relay
    /// lane toward us (worker_relay.go:122-127 → :69; handshaker.go:239-242).
    /// `None` = field absent (CANDIDATE/HEARTBEAT never carry it upstream;
    /// no-relay deployments stay field-free).
    pub relay_server_address: Option<String>,
}

/// Long-lived signal session with upstream-shaped reconnect: register the
/// stream, receive/decrypt frames, back off and re-register on breaks.
///
/// Drive it with [`SignalSession::run_events`] (loop + callback), or step
/// manually with [`SignalSession::connect`] / [`SignalSession::next_frame`].
pub struct SignalSession {
    client: SignalClient,
    active: Option<ActiveStream>,
    backoff: ExponentialBackoff,
    rng: Box<dyn Rng + Send>,
    clock: Box<dyn Clock + Send>,
    reconnects: u64,
    observed_delays: std::sync::Mutex<Vec<core::time::Duration>>,
    last_stream_error: Option<ManagementError>,
}

/// Event emitted by [`SignalSession::run_events`] (the signal counterpart
/// of [`crate::sync::SyncLoopEvent`]).
#[derive(Debug, Clone, PartialEq)]
pub enum SignalLoopEvent<'a> {
    /// The stream is registered (initial connect or reconnect — upstream
    /// `notifyStreamConnected` after the registered header, grpc.go:236,
    /// 292-306).
    Registered,
    /// A decrypted message arrived.
    Message(&'a SignalMessage),
    /// A frame failed envelope-open/protobuf-decode. The stream STAYS UP
    /// (upstream decrypt worker logs and continues, grpc.go:600-602);
    /// surfaced so a broken peer/key is visible, not silently dropped.
    Malformed(&'a ManagementError),
    /// The stream broke or a register attempt failed with a retryable
    /// error; the session backs off and reconnects (upstream
    /// notifyDisconnected + "will retry silently", grpc.go:266-272).
    Broken(&'a ManagementError),
}

impl SignalSession {
    /// New session with the upstream signal backoff preset (800ms/×1.7/10s
    /// cap/f=1/3-months, `shared/signal/client/grpc.go:173-183` — identical
    /// numbers to [`ExponentialBackoff::upstream_stream_default`]), OS
    /// randomness and the monotonic clock.
    pub fn new(client: SignalClient) -> Self {
        SignalSession {
            client,
            active: None,
            backoff: ExponentialBackoff::upstream_stream_default(),
            rng: Box::new(OsRandom),
            clock: Box::new(MonotonicClock),
            reconnects: 0,
            observed_delays: std::sync::Mutex::new(Vec::new()),
            last_stream_error: None,
        }
    }

    /// Test/owner overrides: exact backoff policy, deterministic RNG and
    /// injectable clock (no sleeping in parameter assertions) — the
    /// [`crate::sync::SyncSession::with_policy`] contract.
    pub fn with_policy(
        mut self,
        backoff: ExponentialBackoff,
        rng: Box<dyn Rng + Send>,
        clock: Box<dyn Clock + Send>,
    ) -> Self {
        self.backoff = backoff;
        self.rng = rng;
        self.clock = clock;
        self
    }

    /// Stream breaks that triggered (or will trigger) a reconnect attempt;
    /// the initial registration is not counted (diagnostics).
    pub fn reconnects(&self) -> u64 {
        self.reconnects
    }

    /// The backoff delays driven through reconnect sleeps, in order.
    pub fn observed_reconnect_delays(&self) -> Vec<core::time::Duration> {
        self.observed_delays.lock().expect("observed_delays lock").clone()
    }

    /// The stream error that broke the most recent connection.
    pub fn last_stream_error(&self) -> Option<&ManagementError> {
        self.last_stream_error.as_ref()
    }

    /// Open + register the stream (upstream `connect` inside the Receive
    /// operation, grpc.go:230). On success the backoff resets (grpc.go:268
    /// via the shared retry helper contract, retry.go:22).
    pub async fn connect(&mut self) -> Result<(), ManagementError> {
        let registered = self.client.register().await?;
        self.active = Some(ActiveStream { frames: registered.inbound });
        self.last_stream_error = None;
        self.backoff.reset(self.clock.as_ref());
        Ok(())
    }

    /// Seal + send one queued [`SignalOutgoing`] to the server — the N5d
    /// exchange-seam entry point. Delivery rides the UNARY
    /// `SignalExchange/Send` (upstream signaler.go:36-66 → `GrpcClient::
    /// send`, grpc.go:454-492 → signal.go:95-104): the deployed server
    /// generation never reads `ConnectStream` inbound frames, so a
    /// stream-side send is silently black-holed (the N10b root cause).
    /// Identity stamping (`Message.key` = our base64 public key,
    /// grpc.go:446-448) and the `MarshalCredential` field set
    /// (client.go:74-97: type/payload/wgListenPort/netBirdVersion) happen
    /// in [`SignalClient::build_message`].
    pub async fn send_outgoing(&mut self, out: &SignalOutgoing) -> Result<(), ManagementError> {
        let msg = self.client.build_message(
            &out.remote_key,
            out.kind,
            out.payload.as_str(),
            out.wg_listen_port,
            out.relay_server_address.as_deref(),
        );
        self.client.send(&msg).await
    }

    /// Receive the next frame from the ACTIVE stream: decrypt (sender key
    /// from the envelope) + decode into [`SignalMessage`].
    ///
    /// - envelope/protobuf failure → `Ok(Malformed)` — the stream stays up
    ///   (upstream decrypt worker, grpc.go:600-602);
    /// - transport/EOF → `Err(Closed)` — the reconnect trigger
    ///   (grpc.go:570-587: EOF means the server closed the stream and is
    ///   retried like any transport failure);
    /// - `PermissionDenied`/`Unauthenticated` → `Err(Fatal)` — terminate.
    pub async fn next_frame(&mut self) -> Result<IncomingFrame, SignalStreamError> {
        let active = self.active.as_mut().ok_or_else(|| {
            SignalStreamError::Closed(ManagementError::Network(
                "signal stream not registered (call connect() first)".into(),
            ))
        })?;
        let envelope = match active.frames.message().await {
            Ok(Some(envelope)) => envelope,
            // io.EOF — server closed the stream (grpc.go:581-583)
            Ok(None) => {
                return Err(SignalStreamError::Closed(ManagementError::Network(
                    "signal stream closed by server (EOF)".into(),
                )));
            }
            Err(status) => {
                return Err(SignalStreamError::from_management(map_grpc_status(status)));
            }
        };
        match self.client.decrypt_envelope(&envelope) {
            Ok(msg) => Ok(IncomingFrame::Message(msg)),
            Err(e) => Ok(IncomingFrame::Malformed(e)),
        }
    }

    /// Run the session until a FATAL error or backoff exhaustion — the
    /// [`crate::sync::SyncSession::run_events`] shape, with the signal
    /// event set ([`SignalLoopEvent`]):
    ///
    /// 1. register (Auth-class failure → `Err`, NO retry);
    /// 2. on success emit `Registered` and deliver frames;
    /// 3. frame-level crypto/decode failure → `Malformed`, stream stays up;
    /// 4. stream failure → `Broken`, take the next backoff delay (injected
    ///    clock/rng), sleep, re-register; `None` (budget spent) → `Err`
    ///    with the pending error.
    ///
    /// Shutdown: drop the future/task owning this session — the loop holds
    /// no catch-all that survives task cancellation.
    pub async fn run_events<E>(&mut self, on_event: E) -> Result<(), ManagementError>
    where
        E: for<'a> FnMut(SignalLoopEvent<'a>),
    {
        // idle outbox: sender kept alive for the whole call, so `recv()`
        // pends forever and the loop below behaves EXACTLY like the
        // receive-only loop it replaces (no early `None` exit).
        let (idle_tx, idle_rx) = tokio::sync::mpsc::unbounded_channel::<SignalOutgoing>();
        let result = self.run_events_with_outbox(idle_rx, on_event).await;
        drop(idle_tx);
        result
    }

    /// [`SignalSession::run_events`] with an outbound queue: the SAME
    /// register/receive/backoff policy, with each inner step multiplexing
    /// `next_frame()` against `outbox.recv()` (`tokio::select!`) so queued
    /// sends never wait for inbound traffic. The queue drains through
    /// [`SignalSession::send_outgoing`] — UNARY `SignalExchange/Send`
    /// delivery (see that method; stream-side sends are black-holed by the
    /// deployed server generation):
    ///
    /// - frame-shaping failures (bad remote key → `Parse`/`Request`) drop
    ///   the single frame and keep the stream (a bad frame is not a broken
    ///   transport, grpc.go:600-602 parity);
    /// - unary delivery failures (`Timeout`/`Network`/`Server`) follow the
    ///   receive-break path: `Broken` event, backoff, re-register
    ///   (grpc.go:396-411 era semantics kept: a dead channel must be
    ///   re-registered before anything else flows);
    /// - all senders dropped (`recv() == None`) → `Ok(())`: the owning
    ///   exchange is gone, the worker exits cleanly.
    pub async fn run_events_with_outbox<E>(
        &mut self,
        mut outbox: tokio::sync::mpsc::UnboundedReceiver<SignalOutgoing>,
        mut on_event: E,
    ) -> Result<(), ManagementError>
    where
        E: for<'a> FnMut(SignalLoopEvent<'a>),
    {
        enum Step {
            Inbound(Result<IncomingFrame, SignalStreamError>),
            Outbound(Option<SignalOutgoing>),
        }
        loop {
            match self.connect().await {
                Ok(()) => {
                    on_event(SignalLoopEvent::Registered);
                    loop {
                        let step = tokio::select! {
                            frame = self.next_frame() => Step::Inbound(frame),
                            out = outbox.recv() => Step::Outbound(out),
                        };
                        match step {
                            Step::Inbound(Ok(IncomingFrame::Message(msg))) => {
                                on_event(SignalLoopEvent::Message(&msg));
                            }
                            Step::Inbound(Ok(IncomingFrame::Malformed(e))) => {
                                on_event(SignalLoopEvent::Malformed(&e));
                            }
                            Step::Inbound(Err(SignalStreamError::Fatal(e))) => return Err(e),
                            Step::Inbound(Err(SignalStreamError::Closed(e))) => {
                                self.last_stream_error = Some(e);
                                self.reconnects += 1;
                                on_event(SignalLoopEvent::Broken(
                                    self.last_stream_error.as_ref().expect("error just stored"),
                                ));
                                break;
                            }
                            Step::Outbound(None) => {
                                // every sender dropped: the exchange seam is
                                // gone — nothing will ever be queued again
                                return Ok(());
                            }
                            Step::Outbound(Some(out)) => match self.send_outgoing(&out).await {
                                Ok(()) => {}
                                Err(e @ (ManagementError::Parse(_) | ManagementError::Request { .. })) =>
                                {
                                    // undeliverable FRAME (bad remote key /
                                    // missing body): drop it, keep the stream
                                    let _ = e; // class-only logging: no message text (credential discipline)
                                    crate::hilog::emit(
                                        "signal: undeliverable frame dropped (parse/request class)",
                                    );
                                }
                                Err(e) => {
                                    // unary delivery failure (timeout /
                                    // transport / server class) → the same
                                    // reconnect path as a receive break:
                                    // fail loudly, re-register, never
                                    // silently swallow a frame
                                    self.last_stream_error = Some(e);
                                    self.reconnects += 1;
                                    on_event(SignalLoopEvent::Broken(
                                        self.last_stream_error
                                            .as_ref()
                                            .expect("error just stored"),
                                    ));
                                    break;
                                }
                            },
                        }
                    }
                }
                Err(e) => {
                    if matches!(e, ManagementError::Auth { .. }) {
                        return Err(e); // permanent — engine policy, connect.go:353-356
                    }
                    self.last_stream_error = Some(e);
                    on_event(SignalLoopEvent::Broken(
                        self.last_stream_error.as_ref().expect("error just stored"),
                    ));
                }
            }
            let delay = match self.backoff.next_delay(self.clock.as_ref(), self.rng.as_mut()) {
                Some(d) => d,
                None => {
                    return Err(self.last_stream_error.take().unwrap_or_else(|| {
                        ManagementError::Network(
                            "signal reconnect backoff exhausted without a pending error".into(),
                        )
                    }));
                }
            };
            self.observed_delays
                .lock()
                .expect("observed_delays lock")
                .push(delay);
            tokio::time::sleep(delay).await;
        }
    }
}

// ---------------------------------------------------------------------------
// the `ConnectStream` request body: a stream that never yields and never
// ends (see `SignalClient::register`) — there is deliberately NO outbound
// stream half left to send on.
// ---------------------------------------------------------------------------

/// The never-yielding request body of `ConnectStream` (upstream keeps the
/// stream object's `Send` available but the engine never calls it against
/// current servers — and the servers never read the body at all).
struct PendingStream;

impl Stream for PendingStream {
    type Item = proto::EncryptedMessage;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Pending
    }
}

// ---------------------------------------------------------------------------
// unit tests: message building / envelope round trip (no I/O)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> SignalClient {
        // host unit test: never dials; only exercises the crypto/wire
        // builders (the constructor requires I/O, so assemble directly).
        SignalClient {
            stub: build_dead_stub(),
            keys: EnvelopeKeyPair::generate().unwrap(),
            request_timeout: core::time::Duration::from_secs(5),
        }
    }

    fn build_dead_stub() -> proto::signal_exchange_client::SignalExchangeClient<Channel> {
        // a channel that can never carry traffic is fine: none of the
        // tested paths touch the stub.
        let ep = tonic::transport::Endpoint::from_static("http://127.0.0.1:1");
        proto::signal_exchange_client::SignalExchangeClient::new(ep.connect_lazy())
    }

    #[test]
    fn header_constants_match_upstream() {
        assert_eq!(HEADER_ID, "x-wiretrustee-peer-id");
        assert_eq!(HEADER_REGISTERED, "x-wiretrustee-peer-registered");
    }

    // The dead-stub construction needs a lazy channel, which wants a tokio
    // reactor context; the tests below are async purely for that stub.
    #[tokio::test]
    async fn build_message_sets_identity_type_payload_and_version() {
        let c = client();
        let msg = c.build_message(
            "REMOTEKEY",
            proto::body::Type::Offer,
            "ufrag:pwd",
            51_820,
            None,
        );
        assert_eq!(msg.key, c.keys.public_key_base64());
        assert_eq!(msg.remote_key, "REMOTEKEY");
        let body = msg.body.as_ref().expect("body");
        assert_eq!(body.r#type, proto::body::Type::Offer as i32);
        assert_eq!(body.payload, "ufrag:pwd");
        assert_eq!(body.wg_listen_port, 51_820);
        assert_eq!(body.net_bird_version, SIGNAL_CLIENT_VERSION);
    }

    /// WG over relay 修复的核心断言：广告了中继时，OFFER 与 ANSWER 的
    /// Body 携带 `relayServerAddress`（字段 8）且值正确、`relayServerIP`
    /// （字段 11，需解析 IP，本端不产出）保持缺省——对端只有在该字段
    /// 非空时才为我方 OpenConn（worker_relay.go:122-127 → :69）。
    #[tokio::test]
    async fn build_message_carries_relay_address_for_offer_and_answer() {
        let c = client();
        for kind in [proto::body::Type::Offer, proto::body::Type::Answer] {
            let msg = c.build_message(
                "REMOTEKEY",
                kind,
                "ufrag:pwd",
                51_820,
                Some("rels://relay.example:28443"),
            );
            assert_eq!(msg.key, c.keys.public_key_base64());
            assert_eq!(msg.remote_key, "REMOTEKEY");
            let body = msg.body.as_ref().expect("body");
            assert_eq!(body.r#type, kind as i32);
            assert_eq!(body.payload, "ufrag:pwd");
            assert_eq!(body.wg_listen_port, 51_820);
            assert_eq!(body.net_bird_version, SIGNAL_CLIENT_VERSION);
            // the fix: field 8 set, field 11 untouched
            assert_eq!(
                body.relay_server_address.as_deref(),
                Some("rels://relay.example:28443"),
                "{kind:?} must carry relayServerAddress"
            );
            assert!(
                body.relay_server_ip.is_none(),
                "relayServerIP needs the resolved instance IP we do not track"
            );
        }
    }

    /// 回归：未广告（`None`）时字段保持缺省——无 relay 部署的字节形态
    /// 与修复前完全一致（对端照旧判定 "Relay is not supported"，行为
    /// 不变）。
    #[tokio::test]
    async fn build_message_without_relay_keeps_field_absent() {
        let c = client();
        let msg = c.build_message(
            "REMOTEKEY",
            proto::body::Type::Offer,
            "ufrag:pwd",
            51_820,
            None,
        );
        let body = msg.body.as_ref().expect("body");
        assert_eq!(body.relay_server_address, None);
        assert_eq!(body.relay_server_ip, None);
    }

    /// 明文 Body 经真实信封（seal → open → protobuf decode）往返后，
    /// 读侧 [`SignalMessage`] 携带同一中继地址——对端解出的就是我们的
    /// 广告值（signal_channel.rs 另有经 mock 服务器的全链路版本）。
    #[tokio::test]
    async fn relay_address_survives_envelope_roundtrip() {
        let alice = client();
        let bob = client();
        let msg = alice.build_message(
            &bob.keys.public_key_base64(),
            proto::body::Type::Offer,
            "ufragA:pwdA",
            51_820,
            Some("rels://relay.example:28443"),
        );
        let wire = alice.encrypt_message(&msg).unwrap();
        let opened = bob.decrypt_envelope(&wire).unwrap();
        assert_eq!(opened.kind, proto::body::Type::Offer);
        assert_eq!(opened.payload, "ufragA:pwdA");
        assert_eq!(
            opened.relay_server_address.as_deref(),
            Some("rels://relay.example:28443")
        );
    }

    #[tokio::test]
    async fn encrypt_decrypt_roundtrip_seals_for_remote_opens_with_sender() {
        let alice = client();
        let bob = client();
        let msg = alice.build_message(
            &bob.keys.public_key_base64(),
            proto::body::Type::Candidate,
            "candidate-1",
            0,
            None,
        );
        let wire = alice.encrypt_message(&msg).unwrap();
        // wire shape: key=alice pub b64, remote_key=bob pub b64
        assert_eq!(wire.key, alice.keys.public_key_base64());
        assert_eq!(wire.remote_key, bob.keys.public_key_base64());
        // nonce(24) || protobuf-Body || tag(16)
        let plain = msg.body.as_ref().unwrap().encode_to_vec();
        assert_eq!(wire.body.len(), crate::envelope::NONCE_SIZE + plain.len() + 16);

        // bob opens with alice's public key (the envelope sender)
        let opened = bob.decrypt_envelope(&wire).unwrap();
        assert_eq!(opened.from_key, alice.keys.public_key_base64());
        assert_eq!(opened.remote_key, bob.keys.public_key_base64());
        assert_eq!(opened.kind, proto::body::Type::Candidate);
        assert_eq!(opened.payload, "candidate-1");
    }

    #[tokio::test]
    async fn decrypt_rejects_wrong_sender_key_and_garbage() {
        let alice = client();
        let bob = client();
        let eve = client();
        let msg = alice.build_message(
            &bob.keys.public_key_base64(),
            proto::body::Type::Offer,
            "u:p",
            0,
            None,
        );
        let wire = alice.encrypt_message(&msg).unwrap();
        // eve is not the addressed peer: authentication fails
        assert!(matches!(
            eve.decrypt_envelope(&wire),
            Err(ManagementError::Parse(_))
        ));
        // truncated / garbage bodies fail before protobuf decoding
        let mut garbage = wire.clone();
        garbage.body.truncate(10);
        assert!(matches!(
            bob.decrypt_envelope(&garbage),
            Err(ManagementError::Parse(_))
        ));
        // not-a-protobuf plaintext after open → Parse
        let sealed = crate::envelope::seal(
            &EnvelopePublicKey::from_base64(&alice.keys.public_key_base64()).unwrap(),
            &bob.keys,
            b"not-a-proto",
        )
        .unwrap();
        let forged = proto::EncryptedMessage {
            key: alice.keys.public_key_base64(),
            remote_key: bob.keys.public_key_base64(),
            body: sealed,
        };
        assert!(matches!(
            bob.decrypt_envelope(&forged),
            Err(ManagementError::Parse(_))
        ));
    }

    #[tokio::test]
    async fn encrypt_message_requires_body_and_valid_remote_key() {
        let c = client();
        let empty = proto::Message { key: String::new(), remote_key: "x".into(), body: None };
        assert!(matches!(
            c.encrypt_message(&empty),
            Err(ManagementError::Request { status: 0, .. })
        ));
        let msg = c.build_message("not-base64!!!", proto::body::Type::Offer, "", 0, None);
        assert!(matches!(
            c.encrypt_message(&msg),
            Err(ManagementError::Parse(_))
        ));
    }

    #[test]
    fn signal_stream_error_splits_on_auth_class() {
        let auth = ManagementError::Auth { status: 7, message: "denied".into() };
        assert!(matches!(
            SignalStreamError::from_management(auth),
            SignalStreamError::Fatal(_)
        ));
        for e in [
            ManagementError::Network("x".into()),
            ManagementError::Parse("y".into()),
            ManagementError::Timeout,
            ManagementError::Server { status: 13 },
        ] {
            assert!(matches!(
                SignalStreamError::from_management(e),
                SignalStreamError::Closed(_)
            ));
        }
    }
}
