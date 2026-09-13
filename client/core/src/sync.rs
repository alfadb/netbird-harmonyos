// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # sync — management `Sync` session (N3-4)
//!
//! The long-lived `Sync` session against the management service: open the
//! server-streaming RPC, send the (single) sealed first frame, decode every
//! pushed `SyncResponse` into the minimal
//! [`crate::network_map`] model, and reconnect with the upstream backoff
//! when the stream breaks.
//!
//! ## Wire shape (upstream facts, pinned commit
//! `791401060d2b95e5f51e3439c0649729132f571e`)
//!
//! - **`Sync` is server-streaming, not bidi**: `rpc Sync(EncryptedMessage)
//!   returns (stream EncryptedMessage)` (management.proto L20). The client
//!   sends exactly ONE frame — the sealed
//!   `SyncRequest{meta}` (management.proto L126-128; there is NO `msg`
//!   field) — then only receives. The bidi streaming RPC upstream is `Job`
//!   (L53), which this crate does not use. Upstream opens the stream in
//!   `connectToSyncStream` (`shared/management/client/grpc.go:478-495`) and
//!   consumes it in `receiveUpdatesEvents` (grpc.go:497-521).
//! - **First frame content**: `SyncRequest{Meta: infoToMetaData(sysInfo)}`
//!   sealed for the server key
//!   (`grpc.go:479-484`), sent as
//!   `EncryptedMessage{WgPubKey: base64(our public key), Body: sealed}` and
//!   the RPC invoked with it (grpc.go:489-490).
//! - **Frames**: each `EncryptedMessage.body` is opened with the SAME
//!   server key and decoded as `SyncResponse` (grpc.go:510-515). The FIRST
//!   response carries the full state; later ones carry changes
//!   (management.proto L15-18). A decrypt/decode failure ends the stream
//!   (grpc.go:512-515 returns the error → the retry loop reconnects).
//! - **Server-closed stream (EOF)**: `stream.Recv() == io.EOF` ends the
//!   receive loop (grpc.go:500-503) and is treated as a retryable stream
//!   error like any other transport failure.
//! - **Fatal vs retryable**: `PermissionDenied` on stream open or receive
//!   is `backoff.Permanent` — it propagates and STOPS the session
//!   (grpc.go:436-438, L468-470); context cancellation ends cleanly
//!   (grpc.go:464-467); everything else retries silently
//!   (grpc.go:471-472).
//! - **Backoff**: see [`crate::backoff`] — upstream `defaultBackoff`
//!   (grpc.go:188-198) driven by the retry loop of
//!   `client/grpc/retry.go:21-55`, reset on every successful stream
//!   establishment (grpc.go:457) and before the first attempt
//!   (retry.go:22). The extra upstream behavior of waking the wait early on
//!   OS network changes (`netMgr.QuickRetryBackoff` +
//!   `watcher.Changed()`, grpc.go:228/233-241, retry.go:42-54) has NO
//!   HarmonyOS event source yet — we wait out the computed delay, i.e. we
//!   reconnect at most one jittered interval later than upstream would.
//!
//! ## Components envelope (explicit non-support)
//!
//! A peer that advertises `PeerCapabilityComponentNetworkMap` receives
//! `SyncResponse.NetworkMapEnvelope` with `NetworkMap` (field 5) left empty
//! (management.proto L819-824). This crate does NOT advertise that
//! capability and pins `syncMessageVersion = 0` (`Base`) in its meta — see
//! [`crate::grpc::build_peer_system_meta`]. If a components envelope
//! arrives anyway (`SyncResponse.version == 1` with the envelope set), that
//! is an explicit [`ManagementError::Parse`], not a silent no-op: applying
//! nothing while claiming a snapshot was processed would hide the loss.
//!
//! ## Error surface
//!
//! [`SyncStreamError`] splits exactly along the upstream retry decision:
//! `Fatal` (Auth class — stop the session) vs `Closed` (everything else —
//! reconnect). Frame-level envelope/decode failures are `Closed`
//! (retryable, upstream parity: grpc.go:512-515), while a response that
//! decodes but cannot be converted to the minimal model is ALSO `Closed`
//! with the conversion error preserved — the model conversion never panics
//! and never silently drops a response.

use crate::backoff::{Clock, ExponentialBackoff, MonotonicClock, OsRandom, Rng};
use crate::envelope::EnvelopeKeyPair;
use crate::grpc::{map_grpc_status, proto, ManagementGrpcClient, PeerMeta};
use crate::management::ManagementError;
use crate::network_map::{NetbirdServers, NetworkMap};
use prost::Message as _;
use tonic::Streaming;

/// Component-based wire-format version tag
/// (`shared/management/grpc/sync_message_versions.go:9-16`: `Base = 0`,
/// `ComponentNetworkMap = 1`).
const SYNC_MESSAGE_VERSION_COMPONENT_NETWORK_MAP: i32 = 1;

/// Failure of the active Sync stream.
///
/// Mirrors the upstream retry decision (grpc.go:436-472): only the Auth
/// class is unrecoverable.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncStreamError {
    /// Stream ended; reconnecting is allowed (transport failure, server
    /// EOF, mapped non-Auth gRPC status, envelope/decode/convert failure).
    /// The inner error preserves the exact cause.
    Closed(ManagementError),
    /// Unrecoverable — upstream `backoff.Permanent` (PermissionDenied /
    /// Unauthenticated). The session loop must stop and surface this.
    Fatal(ManagementError),
}

impl SyncStreamError {
    fn from_management(err: ManagementError) -> Self {
        match err {
            // grpc.go:436-438/L468-470 — PermissionDenied → Permanent.
            // map_grpc_status puts Unauthenticated AND PermissionDenied in
            // the Auth class; both are credential rejections a retry
            // cannot fix.
            e @ ManagementError::Auth { .. } => SyncStreamError::Fatal(e),
            e => SyncStreamError::Closed(e),
        }
    }

    /// The concrete error, for logging/reporting.
    pub fn into_inner(self) -> ManagementError {
        match self {
            SyncStreamError::Closed(e) | SyncStreamError::Fatal(e) => e,
        }
    }
}

/// One decoded `SyncResponse` reduced to the fields this client consumes.
///
/// The 3-state `session_deadline_unix` encoding matches
/// `crate::grpc::LoginOutcome`: `None` = field unset (keep the current
/// deadline), `Some(0)` = explicit "expiry disabled / not SSO", `Some(t)` =
/// absolute deadline (management.proto L281-291; consumed upstream at
/// engine.go:1022 → engine_authsession.go:32-66).
#[derive(Debug, Clone, PartialEq)]
pub struct SyncUpdate {
    pub session_deadline_unix: Option<i64>,
    /// Connection servers (`SyncResponse.netbirdConfig`, field 1);
    /// `None` = absent from this snapshot.
    pub netbird_config: Option<NetbirdServers>,
    /// The network map (`SyncResponse.NetworkMap`, field 5); `None` =
    /// absent from this snapshot.
    pub network_map: Option<NetworkMap>,
}

impl SyncUpdate {
    fn from_response(resp: proto::SyncResponse) -> Result<Self, ManagementError> {
        // Components wire format is explicitly unsupported: we never
        // advertise the capability (see module docs), so receiving it means
        // the peer identity/key on this account IS configured for
        // components — applying nothing here must be loud.
        if resp.version == SYNC_MESSAGE_VERSION_COMPONENT_NETWORK_MAP
            && resp.network_map_envelope.is_some()
        {
            return Err(ManagementError::Parse(
                "SyncResponse carries the components NetworkMapEnvelope \
                 (version=1), which this client does not support"
                    .into(),
            ));
        }
        let network_map = match &resp.network_map {
            Some(map) => Some(NetworkMap::from_proto(map)?),
            None => None,
        };
        let netbird_config = resp.netbird_config.as_ref().map(NetbirdServers::from_proto);
        Ok(SyncUpdate {
            session_deadline_unix: resp.session_expires_at.map(|ts| ts.seconds),
            netbird_config,
            network_map,
        })
    }
}

/// An established Sync stream: the server key it was sealed for plus the
/// frame source.
struct ActiveStream {
    server_key: crate::envelope::EnvelopePublicKey,
    frames: Streaming<proto::EncryptedMessage>,
}

/// Long-lived management `Sync` session with upstream-shaped reconnect.
///
/// Drive it with [`SyncSession::run`] (loop + callback, owns reconnection),
/// or step it manually with [`SyncSession::connect`] /
/// [`SyncSession::next_update`] if the owner wants full control.
pub struct SyncSession {
    client: ManagementGrpcClient,
    meta: PeerMeta,
    /// Client identity keys (cloned out of the client at construction) used
    /// to open pushed frames — the same key pair that sealed the requests.
    envelope_keys: EnvelopeKeyPair,
    active: Option<ActiveStream>,
    backoff: ExponentialBackoff,
    rng: Box<dyn Rng + Send>,
    clock: Box<dyn Clock + Send>,
    /// Diagnostics for the owner/tests: stream breaks that triggered (or
    /// will trigger) a reconnect attempt; the initial connect is not
    /// counted.
    reconnects: u64,
    /// Diagnostics: the backoff delays handed to the reconnect sleep, in
    /// order (one per attempt gap). Lets tests assert the exact backoff
    /// sequence driven through [`SyncSession::run`] with an injected
    /// clock/RNG instead of sleeping through real intervals.
    observed_delays: std::sync::Mutex<Vec<core::time::Duration>>,
    /// Last stream error that triggered a reconnect (cleared on success).
    last_stream_error: Option<ManagementError>,
}

impl SyncSession {
    /// New session with the upstream stream backoff preset
    /// (800ms/×1.7/10s cap/f=1/3-months, `grpc.go:188-198`), OS randomness
    /// and the monotonic clock.
    pub fn new(client: ManagementGrpcClient, meta: PeerMeta) -> Self {
        let envelope_keys = client.envelope_keys();
        SyncSession {
            client,
            meta,
            envelope_keys,
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
    /// injectable clock (no sleeping in parameter assertions).
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

    /// Stream breaks observed by [`SyncSession::run`] that triggered (or
    /// will trigger) a reconnect attempt; the initial connect is not
    /// counted (diagnostics for tests/owners).
    pub fn reconnects(&self) -> u64 {
        self.reconnects
    }

    /// The backoff delays driven through [`SyncSession::run`]'s reconnect
    /// sleeps, in order (diagnostics for tests).
    pub fn observed_reconnect_delays(&self) -> Vec<core::time::Duration> {
        self.observed_delays.lock().expect("observed_delays lock").clone()
    }

    /// The stream error that broke the most recent connection (kept until
    /// the next successful open).
    pub fn last_stream_error(&self) -> Option<&ManagementError> {
        self.last_stream_error.as_ref()
    }

    /// Open the Sync stream: fetch the server key (upstream fetches per
    /// attempt, grpc.go:260) and send the sealed first frame
    /// (`connectToSyncStream`, grpc.go:478-495). On success the backoff is
    /// reset (grpc.go:457, retry.go:22).
    pub async fn connect(&mut self) -> Result<(), ManagementError> {
        let server_key = self.client.get_server_key().await?;
        let frames = self.client.open_sync_stream(&self.meta, &server_key).await?;
        self.active = Some(ActiveStream { server_key, frames });
        self.last_stream_error = None;
        self.backoff.reset(self.clock.as_ref());
        Ok(())
    }

    /// Receive the next pushed update from the ACTIVE stream.
    ///
    /// Errors carry the [`SyncStreamError`] split; either way the stream
    /// should be considered broken afterwards (a `Closed` error is the
    /// reconnect trigger, exactly like upstream returns from
    /// `receiveUpdatesEvents` into the retry loop, grpc.go:460-473).
    pub async fn next_update(&mut self) -> Result<SyncUpdate, SyncStreamError> {
        let active = self.active.as_mut().ok_or_else(|| {
            SyncStreamError::Closed(ManagementError::Network(
                "sync stream not open (call connect() first)".into(),
            ))
        })?;
        let envelope = match active.frames.message().await {
            Ok(Some(envelope)) => envelope,
            // io.EOF — server closed the stream cleanly (grpc.go:500-503)
            Ok(None) => {
                return Err(SyncStreamError::Closed(ManagementError::Network(
                    "sync stream closed by server (EOF)".into(),
                )));
            }
            Err(status) => {
                return Err(SyncStreamError::from_management(map_grpc_status(status)))
            }
        };
        // Open the body with the same server key the stream was sealed for
        // (grpc.go:510-515). Failure ends the stream (retryable).
        let plaintext = crate::envelope::open(&active.server_key, &self.envelope_keys, &envelope.body)
            .map_err(|e| {
                SyncStreamError::Closed(ManagementError::Parse(format!(
                    "sync frame envelope open failed: {e}"
                )))
            })?;
        let response = proto::SyncResponse::decode(plaintext.as_slice()).map_err(|e| {
            SyncStreamError::Closed(ManagementError::Parse(format!(
                "SyncResponse decode failed: {e}"
            )))
        })?;
        // Conversion to the minimal model; a conversion error keeps the
        // response's identity loud (no silent partial application) but is
        // still retryable — a fresh snapshot may well be clean.
        SyncUpdate::from_response(response).map_err(SyncStreamError::Closed)
    }

    /// Run the session until a FATAL error or backoff exhaustion (the
    /// update-only form of [`SyncSession::run_events`]).
    ///
    /// Loop shape mirrors upstream `withMgmtStream` + `handleSyncStream`
    /// (grpc.go:224-275, L427-476) driven by the `Retry` helper
    /// (retry.go:21-55):
    ///
    /// 1. connect (server key + first frame); Auth-class failure →
    ///    `Err` (Permanent, no retry);
    /// 2. on success reset the backoff (grpc.go:457) and deliver every
    ///    decoded update to `on_update`;
    /// 3. stream failure → record it, take the next backoff delay
    ///    (injected clock/rng), sleep, reconnect; `None` (Stop, elapsed
    ///    budget spent) → `Err` with the last stream error
    ///    (retry.go:34-39);
    /// 4. a `Fatal` frame error stops immediately with the cause.
    ///
    /// Shutdown: drop the future/task owning this session (the loop holds
    /// no catch-all that survives task cancellation), matching the
    /// upstream "context canceled → clean exit" semantics
    /// (grpc.go:464-467).
    pub async fn run<F>(&mut self, mut on_update: F) -> Result<(), ManagementError>
    where
        F: FnMut(&SyncUpdate),
    {
        self.run_events(|event| {
            if let SyncLoopEvent::Update(update) = event {
                on_update(update);
            }
        })
        .await
    }
}

/// Event emitted by [`SyncSession::run_events`] — N3-5: lets a lifecycle
/// owner (the `connector` module) observe stream OPEN/BREAK transitions in
/// addition to the decoded updates, so it can drive its state machine
/// without duplicating the retry loop.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncLoopEvent<'a> {
    /// A Sync stream was established (the initial one or a reconnect —
    /// upstream `notifyConnected` after `connectToSyncStream` succeeded,
    /// grpc.go:446-447).
    Opened,
    /// A decoded update arrived on the active stream.
    Update(&'a SyncUpdate),
    /// The active stream broke or a connect attempt failed with a
    /// retryable error; the session will back off and reconnect
    /// (upstream `notifyDisconnected` + "will retry silently",
    /// grpc.go:435-441 / L472).
    Broken(&'a ManagementError),
}

impl SyncSession {
    /// [`SyncSession::run`] with lifecycle events: `Opened` fires after
    /// every successful stream establishment (including reconnects),
    /// `Broken` fires when a stream breaks or a connect attempt fails with
    /// a retryable error, `Update` per decoded snapshot. The retry/fatal/
    /// backoff semantics are exactly [`SyncSession::run`]'s (documented
    /// there); the loop shape is unchanged.
    pub async fn run_events<E>(&mut self, mut on_event: E) -> Result<(), ManagementError>
    where
        E: for<'a> FnMut(SyncLoopEvent<'a>),
    {
        loop {
            match self.connect().await {
                Ok(()) => {
                    on_event(SyncLoopEvent::Opened);
                    loop {
                        match self.next_update().await {
                            Ok(update) => on_event(SyncLoopEvent::Update(&update)),
                            Err(SyncStreamError::Fatal(e)) => return Err(e),
                            Err(SyncStreamError::Closed(e)) => {
                                self.last_stream_error = Some(e);
                                self.reconnects += 1;
                                on_event(SyncLoopEvent::Broken(
                                    self.last_stream_error.as_ref().expect("error just stored"),
                                ));
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    if matches!(e, ManagementError::Auth { .. }) {
                        return Err(e); // Permanent — grpc.go:436-438
                    }
                    self.last_stream_error = Some(e);
                    on_event(SyncLoopEvent::Broken(
                        self.last_stream_error.as_ref().expect("error just stored"),
                    ));
                }
            }
            // Backoff between attempts; exhaustion (Stop) gives up with the
            // pending error (retry.go:34-39).
            let delay = match self.backoff.next_delay(self.clock.as_ref(), self.rng.as_mut()) {
                Some(d) => d,
                None => {
                    return Err(self.last_stream_error.take().unwrap_or_else(|| {
                        ManagementError::Network(
                            "sync reconnect backoff exhausted without a pending error".into(),
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
