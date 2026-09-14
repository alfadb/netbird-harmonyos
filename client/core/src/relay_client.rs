// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # relay_client — NetBird relay connection state machine (N13 increment C)
//!
//! The connection lifecycle on top of the N13-A pieces ([`crate::relay`]
//! codec, [`crate::ws`] client) and the management-derived configuration
//! (`network_map::RelayServers{urls, token_payload, token_signature}` — the
//! ONLY token source; the client never touches the relay shared secret).
//! Implementation basis: `docs/relay-client-spec-20260914.md` §1 (dial/URL),
//! §4 (OpenConn/Transport), §5 (keepalive), §6 (token refresh + reconnect
//! order), §7 (WG carrier shape) and `docs/n13-relay-increment-plan-20260914.md`
//! (no new crates, non `rel://`/`rels://` schemes fail-closed, no QUIC).
//!
//! ## Architecture (two layers, both deterministically testable)
//!
//! - **Worker** (`worker_main`): the connect POLICY. Token gate → dial round
//!   (fast reconnect to the last-good server first, then exponential backoff
//!   rounds of concurrent dials over the URL list, first success wins) →
//!   spawn a session → await its end → repeat. Only TCP/TLS dialing happens
//!   here, so the reported state machine reflects exactly the WINNING
//!   attempt.
//! - **Session** (`run_session`): one live connection attempt, sequentially:
//!   WS upgrade (`Handshaking`) → Auth frame + AuthResponse
//!   (`Authenticating`) → Ready loop that `select!`s inbound frames (a
//!   dedicated reader task owns `WsClient::read_message`, so a frame read is
//!   NEVER cancelled mid-parse — see `reader_loop`), commands (send /
//!   subscribe / graceful close) and clock deadlines (35s keepalive, token
//!   expiry, `OpenConn` 30s timeouts).
//!
//! ## Fail-closed decisions (each is deliberate and documented)
//!
//! - **URL**: only `rel://`→ws and `rels://`→wss, path pinned `/relay`.
//!   Everything else (`quic://`, `turn://`, …) is a typed
//!   [`RelayClientError::UnsupportedUrl`] — no fallback, no downgrade.
//! - **TLS**: `rels://` WITHOUT a caller-supplied [`RelayTlsConnector`] is
//!   refused BEFORE dialing (`TlsRequired`) — a plaintext downgrade is
//!   impossible. This module does NOT decide certificate policy: the caller
//!   injects a connector built on their rustls config / trust root (same
//!   philosophy as the management-path `ca_pem` injection). The connector
//!   type-erases "TCP stream in → TLS stream out".
//! - **Token**: expiry is judged against the injected clock. An expired (or
//!   malformed) token NEVER reaches the wire: the worker holds in
//!   `Reconnecting` with class `TokenExpired` and dials nothing until
//!   [`RelayClient::update_token`] supplies a fresh one (wakes instantly via
//!   a watch channel, otherwise re-checks on a bounded 60s cadence). While
//!   Ready, an expiring token CLOSES the session (fail-closed). This is
//!   STRICTER than upstream — NetBird keeps established sessions (the server
//!   verifies only at handshake, spec §6.1); the N13 plan §4 requires
//!   「凭据过期未刷新必须 fail-closed」 and this increment implements the
//!   strict reading. Recorded here as a deliberate deviation.
//! - **Liveness**: 35s without ANY inbound frame ⇒ session dead
//!   (`KeepaliveTimeout`) ⇒ reconnect. HealthChecks are answered with the
//!   exact `01 05` echo (§5); the client never initiates one (the server
//!   does, §5).
//! - **Oversized frames**: a WS message > 8820 is a hard typed error ending
//!   the session (increment-A decision inherited from `ws.rs`, not silently
//!   dropped); outbound payloads exceeding the Transport ceiling are
//!   rejected at the handle BEFORE any wire byte.
//! - **Offline destinations**: `PeersWentOffline` marks the peer offline; a
//!   `send_to_peer` to a known-offline peer is refused LOCALLY
//!   (`PeerOffline`) so it is never misreported as delivered. The relay
//!   protocol has no delivery ack (§4.2: silent server-side drop) — `Ok`
//!   from `send_to_peer` therefore means "handed to the relay server",
//!   never "delivered"; server-side drops are only observable out-of-band
//!   (the test server counts them in `transports_dropped_offline`).
//! - **Backoff**: exactly `2s, 4s, 8s, 16s, 32s, 60s, 60s, …` with NO
//!   jitter (upstream randomizes) — deterministic so tests can pin the
//!   exact sequence; recorded as a deliberate deviation.
//! - **URL waves**: at most the first [`MAX_CONCURRENT_URLS`] entries of the
//!   list are dialed per round (upstream cap; the managed instance has 1).
//!
//! ## Time injection
//!
//! [`RelayClock`] is the single seam: `now()` (monotonic), `unix_now()`
//! (token expiry domain) and `sleep_until()` (the only sleep primitive in
//! this module). Production wires [`SystemClock`] (tokio sleep + system
//! time); tests wire [`VirtualClock`], whose time moves ONLY when the test
//! calls [`VirtualClock::advance`] — so "35s 判死", the 8s auth budget and
//! the exact backoff sequence are asserted with zero real waiting and no
//! sleep-masked races. Every wait is bounded (a clock deadline) and
//! cancellable (drop / `stop()` / token-watch wake).
//!
//! ## Increment-D integration points (carrier for WG)
//!
//! D maps the relay stream onto WG the way upstream wgProxy maps it onto a
//! fake UDP endpoint (spec §7.1):
//! 1. on peer (re)config: `client.open_conn(&peer_id).await` — blocks until
//!    `PeersOnline` (30s typed timeout, §4.1);
//! 2. WG bind write path → `client.send_to_peer(&peer_id, packet)` (payload
//!    ceiling [`MAX_TRANSPORT_PAYLOAD`] = 8782, spec §7.2 MTU arithmetic);
//! 3. pump task: `while let Some((sender, packet)) = client.recv().await`
//!    → feed WG (the 36B field of a received Transport frame is the SENDER
//!    id, §4.2 receive-side semantics);
//! 4. `client.state()` / `client.stats()` for the connector snapshot;
//!    `client.update_token(...)` from every new Sync `RelayServers`.
//! TODO(increment D): device-path dialing must go through a
//! protected-socket seam (`VpnConnection.protect` fd feed, the N3-7
//! `mgmtsock` pattern) instead of the plain [`TcpDialer`], and the real
//! [`RelayTlsConnector`] (rustls + management-injected CA) is provided
//! there too. Presence/subscription state does NOT survive a reconnect —
//! D must re-`open_conn` after observing `state() == Ready` again.
//!
//! ## Sensitive discipline
//!
//! Token signature/payload bytes never appear in `Debug`/logs/errors
//! (`AuthToken` is REDACTED at the codec layer; this module stores it
//! opaquely and its `Debug` output carries counters, URLs and shape tokens
//! only). Tests use fabricated values exclusively.

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, watch};

use crate::relay::{
    AuthToken, Frame, PeerId, RelayError, MAX_MESSAGE_SIZE, MSG_AUTH, MSG_AUTH_RESPONSE,
    HEADER_LEN, PEER_ID_SIZE,
};
use crate::ws::{client_handshake, random_sec_websocket_key, WsClient, WsError, WsMessage};

// ---------------------------------------------------------------------------
// constants (spec-anchored)
// ---------------------------------------------------------------------------

/// The only WS path a NetBird relay serves (§1.1, `WebSocketURLPath`).
pub const RELAY_WS_PATH: &str = "/relay";

/// §5: client-side receive death — 35s without inbound ⇒ dead.
pub const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(35);
/// §1.5: `serverResponseTimeout` for the AuthResponse (8s).
pub const AUTH_RESPONSE_TIMEOUT: Duration = Duration::from_secs(8);
/// §1.5: `DefaultConnectionTimeout` dial budget (applied per dial phase).
pub const DIAL_TIMEOUT: Duration = Duration::from_secs(30);
/// §4.1: `OpenConnectionTimeout` — wait for `PeersOnline` (30s), then
/// unsubscribe and fail typed.
pub const OPEN_CONN_TIMEOUT: Duration = Duration::from_secs(30);
/// §6.2: reconnect backoff base (2s, ×2 per round).
pub const BACKOFF_INITIAL: Duration = Duration::from_secs(2);
/// §6.2: backoff cap (`defaultMaxBackoffInterval`).
pub const BACKOFF_MAX: Duration = Duration::from_secs(60);
/// §6.2: `maxConcurrentServers` — concurrent dials per round, first win.
pub const MAX_CONCURRENT_URLS: usize = 7;

/// Transport frame fixed overhead: 2B header + 36B dst/sender id (§2.5).
pub const TRANSPORT_FRAME_OVERHEAD: usize = HEADER_LEN + PEER_ID_SIZE;

/// Max payload bytes one `send_to_peer` may carry (8820 − 38).
pub const MAX_TRANSPORT_PAYLOAD: usize = MAX_MESSAGE_SIZE - TRANSPORT_FRAME_OVERHEAD;

const CMD_CAP: usize = 128;
const SESSION_MSG_CAP: usize = 128;
/// Session→reader I/O queue: strictly larger than [`CMD_CAP`], so a full
/// I/O queue is unreachable while the command queue exists.
const IO_CAP: usize = 256;
const INBOUND_CAP: usize = 256;
const STATE_HISTORY_CAP: usize = 64;
const BACKOFF_LOG_CAP: usize = 256;
/// Bounded re-check cadence while fail-closed on an expired token.
const TOKEN_RECHECK_INTERVAL: Duration = BACKOFF_MAX;

/// Exact reconnect backoff sequence: `2s, 4s, 8s, 16s, 32s, 60s, 60s, …`
/// (§6.2 shape; deterministic — NO jitter, so tests pin the exact sequence;
/// upstream randomizes, recorded as a deliberate deviation).
pub fn backoff_delay(round: u32) -> Duration {
    let secs = 2u64 << round.min(5); // 2,4,8,16,32,64…
    Duration::from_secs(secs.min(BACKOFF_MAX.as_secs()))
}

// ---------------------------------------------------------------------------
// clock seam — the single time primitive of this module
// ---------------------------------------------------------------------------

/// Injectable clock/sleep seam. EVERY wait in this module goes through
/// `sleep_until`; `now()` and `unix_now()` back the keepalive/backoff and
/// token-expiry arithmetic respectively.
pub trait RelayClock: Send + Sync + 'static {
    /// Monotonic logical time (keepalive deadlines, backoff, budgets).
    fn now(&self) -> Instant;
    /// Unix seconds (token payload domain, §3.3).
    fn unix_now(&self) -> u64;
    /// The ONLY sleep primitive. Cancellable by dropping the future.
    fn sleep_until(&self, deadline: Instant) -> Pin<Box<dyn Future<Output = ()> + Send>>;
}

/// Production clock: monotonic `Instant`, system Unix time, tokio sleep.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl RelayClock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn unix_now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
    fn sleep_until(&self, deadline: Instant) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)))
    }
}

/// Manually advanced virtual clock for deterministic tests: time moves ONLY
/// in `advance()`, `sleep_until` parks until virtual time passes the
/// deadline — "35s 判死" and backoff sequences are driven with zero real
/// waiting. Not used by any production path.
#[derive(Clone)]
pub struct VirtualClock {
    inner: Arc<Mutex<VirtualInner>>,
}

impl Default for VirtualClock {
    fn default() -> Self {
        VirtualClock::new(0)
    }
}

struct VirtualInner {
    /// Virtual monotonic time; starts at the real `Instant::now()` base so
    /// it can never travel backwards relative to the process.
    now: Instant,
    start: Instant,
    unix_base: u64,
    next_id: u64,
    /// `(id, deadline, waker)` — woken once virtual time passes `deadline`.
    waiters: Vec<(u64, Instant, std::task::Waker)>,
}

impl VirtualClock {
    /// `unix_base` = fabricated "epoch second" at virtual time zero (tests
    /// align it with their fabricated token payloads).
    pub fn new(unix_base: u64) -> Self {
        let now = Instant::now();
        VirtualClock {
            inner: Arc::new(Mutex::new(VirtualInner {
                now,
                start: now,
                unix_base,
                next_id: 0,
                waiters: Vec::new(),
            })),
        }
    }

    /// Advance virtual time; wakes every sleeper whose deadline is reached.
    pub fn advance(&self, by: Duration) {
        let due: Vec<std::task::Waker> = {
            let mut inner = self.inner.lock().expect("virtual clock lock");
            inner.now += by;
            let now = inner.now;
            let mut due = Vec::new();
            inner.waiters.retain(|(_, deadline, waker)| {
                if *deadline <= now {
                    due.push(waker.clone());
                    false
                } else {
                    true
                }
            });
            due
        };
        for waker in due {
            waker.wake();
        }
    }
}

impl VirtualInner {
    fn unix_now(&self) -> u64 {
        self.unix_base + self.now.duration_since(self.start).as_secs()
    }
}

impl RelayClock for VirtualClock {
    fn now(&self) -> Instant {
        self.inner.lock().expect("virtual clock lock").now
    }
    fn unix_now(&self) -> u64 {
        self.inner.lock().expect("virtual clock lock").unix_now()
    }
    fn sleep_until(&self, deadline: Instant) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(VirtualSleep { clock: self.clone(), deadline, id: None })
    }
}

/// Parked virtual sleep: registers a waker keyed by a unique id, removes it
/// on drop — no stale entry ever wakes anything.
struct VirtualSleep {
    clock: VirtualClock,
    deadline: Instant,
    id: Option<u64>,
}

impl Future for VirtualSleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = &mut *self;
        let mut inner = this.clock.inner.lock().expect("virtual clock lock");
        if inner.now >= this.deadline {
            if let Some(id) = this.id.take() {
                inner.waiters.retain(|(wid, _, _)| *wid != id);
            }
            return Poll::Ready(());
        }
        match this.id {
            Some(id) => {
                if let Some(slot) = inner.waiters.iter_mut().find(|(wid, _, _)| *wid == id) {
                    slot.2 = cx.waker().clone();
                }
            }
            None => {
                let id = inner.next_id;
                inner.next_id += 1;
                inner.waiters.push((id, this.deadline, cx.waker().clone()));
                this.id = Some(id);
            }
        }
        Poll::Pending
    }
}

impl Drop for VirtualSleep {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            let mut inner = self.clock.inner.lock().expect("virtual clock lock");
            inner.waiters.retain(|(wid, _, _)| *wid != id);
        }
    }
}

// ---------------------------------------------------------------------------
// URL parsing — rel/rels only, fail-closed on everything else
// ---------------------------------------------------------------------------

/// Parsed relay server URL. `rel://host[:port]` ⇒ ws (default port 80),
/// `rels://host[:port]` ⇒ wss (default port 443); path is always
/// [`RELAY_WS_PATH`] (§1.1). Any other scheme is a typed failure — never a
/// silent fallback or downgrade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayUrl {
    scheme: RelayScheme,
    /// Host without brackets (IPv6 literals keep their colons).
    host: String,
    port: u16,
}

/// URL scheme of the managed relay list (§1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayScheme {
    /// `rel://` → plain `ws://`.
    Rel,
    /// `rels://` → `wss://` (TLS via the injected connector).
    Rels,
}

impl RelayScheme {
    fn config_name(self) -> &'static str {
        match self {
            RelayScheme::Rel => "rel",
            RelayScheme::Rels => "rels",
        }
    }
    fn ws_name(self) -> &'static str {
        match self {
            RelayScheme::Rel => "ws",
            RelayScheme::Rels => "wss",
        }
    }
}

impl RelayUrl {
    /// Parse a managed `rel://`/`rels://` URL. Fail-closed: unsupported
    /// schemes and malformed hosts/ports are typed errors (`reason` is a
    /// stable shape token).
    pub fn parse(input: &str) -> Result<RelayUrl, RelayClientError> {
        let (scheme_str, rest) = input
            .split_once("://")
            .ok_or(RelayClientError::UnsupportedUrl { reason: "missing-scheme-separator" })?;
        let scheme = match scheme_str.to_ascii_lowercase().as_str() {
            // URL schemes are ASCII case-insensitive (Go url.Parse lowercases).
            "rel" => RelayScheme::Rel,
            "rels" => RelayScheme::Rels,
            _ => return Err(RelayClientError::UnsupportedUrl { reason: "scheme" }),
        };
        // Authority only. The relay path is pinned to /relay upstream, so a
        // URL carrying its own path/query/fragment/userinfo is rejected
        // instead of silently ignored.
        if rest.is_empty() {
            return Err(RelayClientError::UnsupportedUrl { reason: "host-empty" });
        }
        if !rest.is_ascii() {
            return Err(RelayClientError::UnsupportedUrl { reason: "not-ascii" });
        }
        if rest.contains(['/', '?', '#']) {
            return Err(RelayClientError::UnsupportedUrl { reason: "path-not-allowed" });
        }
        if rest.contains('@') {
            return Err(RelayClientError::UnsupportedUrl { reason: "userinfo-not-allowed" });
        }
        let (host, port_str) = if let Some(stripped) = rest.strip_prefix('[') {
            // Bracketed IPv6 literal: `[h:h:h][:port]`.
            let close =
                stripped.find(']').ok_or(RelayClientError::UnsupportedUrl { reason: "host" })?;
            let host = &stripped[..close];
            let after = &stripped[close + 1..];
            let port_str = match after.strip_prefix(':') {
                Some(p) => Some(p),
                None if after.is_empty() => None,
                None => return Err(RelayClientError::UnsupportedUrl { reason: "host" }),
            };
            (host.to_string(), port_str)
        } else {
            match rest.rsplit_once(':') {
                Some((h, p)) => (h.to_string(), Some(p)),
                None => (rest.to_string(), None),
            }
        };
        if host.is_empty() || host.bytes().any(|b| b <= 0x20 || b == 0x7f) {
            return Err(RelayClientError::UnsupportedUrl { reason: "host" });
        }
        let port = match port_str {
            None => match scheme {
                // §1.1: no relay-specific default upstream — fall to the
                // ws/wss protocol defaults 80/443.
                RelayScheme::Rel => 80,
                RelayScheme::Rels => 443,
            },
            Some(p) => {
                if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(RelayClientError::UnsupportedUrl { reason: "port" });
                }
                let n: u32 = p
                    .parse()
                    .map_err(|_| RelayClientError::UnsupportedUrl { reason: "port-range" })?;
                if n == 0 || n > u16::MAX as u32 {
                    return Err(RelayClientError::UnsupportedUrl { reason: "port-range" });
                }
                n as u16
            }
        };
        Ok(RelayUrl { scheme, host, port })
    }

    pub fn scheme(&self) -> RelayScheme {
        self.scheme
    }

    /// True when the URL requires TLS (`rels://`).
    pub fn is_tls(&self) -> bool {
        self.scheme == RelayScheme::Rels
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// `Host:` header value and TCP dial target form.
    pub fn host_header(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// The mapped WS URL (§1.1 mapping; display/diagnostics).
    pub fn ws_url(&self) -> String {
        format!("{}://{}", self.scheme.ws_name(), self.host_header())
    }

    /// Canonical managed form `rel://host:port` / `rels://host:port`
    /// (reported as the effective relay URL in stats).
    pub fn config_string(&self) -> String {
        format!("{}://{}", self.scheme.config_name(), self.host_header())
    }
}

impl fmt::Display for RelayUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.config_string())
    }
}

// ---------------------------------------------------------------------------
// errors — typed, shape-only (never token content)
// ---------------------------------------------------------------------------

/// Compact error class for stats snapshots (`last_error_class`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayErrorClass {
    Url,
    TlsRequired,
    Dial,
    DialTimeout,
    WsHandshake,
    Ws,
    Codec,
    FrameTooLarge,
    AuthRejected,
    AuthTimeout,
    AuthProtocol,
    ServerClosed,
    Io,
    KeepaliveTimeout,
    TokenExpired,
    ConnectionLost,
    OpenConnTimeout,
    SubscribeDuplicate,
    PeerOffline,
    Backpressured,
    NotReady,
    Stopped,
}

/// Typed failure of every client path (§2: 「任何一步失败 → 类型化错误」).
/// `Debug`/`Display` carry shape tokens and public values only — never
/// token bytes (pinned by the sensitive-discipline test below).
#[derive(Debug)]
pub enum RelayClientError {
    /// Non-`rel`/`rels` scheme or malformed URL — fail-closed, no fallback.
    UnsupportedUrl { reason: &'static str },
    /// `rels://` dialed without a caller-supplied TLS connector — refused
    /// BEFORE any wire byte (no plaintext downgrade).
    TlsRequired,
    /// TCP dial failure.
    Dial(io::Error),
    /// A dial/handshake/auth phase exceeded its injected-clock budget.
    Timeout(RelayErrorClass),
    /// WS upgrade failure.
    Ws(WsError),
    /// Relay frame codec failure.
    Codec(RelayError),
    /// Server closed without answering Auth (spec §3.4: silent close).
    AuthRejected,
    /// No AuthResponse within [`AUTH_RESPONSE_TIMEOUT`].
    AuthTimeout,
    /// Server's first frame was not an AuthResponse (protocol violation).
    AuthProtocol,
    /// Server ended the connection (relay Close / WS close).
    ServerClosed,
    /// 35s without any inbound frame (§5 keepalive death).
    KeepaliveTimeout,
    /// Token expired/malformed — fail-closed, nothing was sent or dialed.
    TokenExpired,
    /// The session ended while this operation was in flight.
    ConnectionLost,
    /// `open_conn` waited 30s without `PeersOnline` (§4.1; unsubscribed).
    OpenConnTimeout,
    /// A subscribe for this peer is already pending (upstream
    /// "already waiting", §4.1).
    SubscribeDuplicate,
    /// Destination is known-offline (`PeersWentOffline`) — refused locally,
    /// never misreported as delivered.
    PeerOffline(PeerId),
    /// Outbound queue full (upstream drops under backpressure, §4.2).
    Backpressured,
    /// Operation attempted while not `Ready` (e.g. unauthenticated).
    NotReady { state: RelayState },
    /// Client stopped.
    Stopped,
    /// Payload would exceed the Transport frame ceiling.
    FrameTooLarge { limit: usize, got: usize },
}

impl RelayClientError {
    /// Stats-facing class of this error.
    pub fn class(&self) -> RelayErrorClass {
        match self {
            RelayClientError::UnsupportedUrl { .. } => RelayErrorClass::Url,
            RelayClientError::TlsRequired => RelayErrorClass::TlsRequired,
            RelayClientError::Dial(_) => RelayErrorClass::Dial,
            RelayClientError::Timeout(c) => *c,
            RelayClientError::Ws(_) => RelayErrorClass::Ws,
            RelayClientError::Codec(_) => RelayErrorClass::Codec,
            RelayClientError::AuthRejected => RelayErrorClass::AuthRejected,
            RelayClientError::AuthTimeout => RelayErrorClass::AuthTimeout,
            RelayClientError::AuthProtocol => RelayErrorClass::AuthProtocol,
            RelayClientError::ServerClosed => RelayErrorClass::ServerClosed,
            RelayClientError::KeepaliveTimeout => RelayErrorClass::KeepaliveTimeout,
            RelayClientError::TokenExpired => RelayErrorClass::TokenExpired,
            RelayClientError::ConnectionLost => RelayErrorClass::ConnectionLost,
            RelayClientError::OpenConnTimeout => RelayErrorClass::OpenConnTimeout,
            RelayClientError::SubscribeDuplicate => RelayErrorClass::SubscribeDuplicate,
            RelayClientError::PeerOffline(_) => RelayErrorClass::PeerOffline,
            RelayClientError::Backpressured => RelayErrorClass::Backpressured,
            RelayClientError::NotReady { .. } => RelayErrorClass::NotReady,
            RelayClientError::Stopped => RelayErrorClass::Stopped,
            RelayClientError::FrameTooLarge { .. } => RelayErrorClass::FrameTooLarge,
        }
    }
}

impl fmt::Display for RelayClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelayClientError::UnsupportedUrl { reason } => {
                write!(f, "relay url unsupported (fail-closed): {reason}")
            }
            RelayClientError::TlsRequired => {
                write!(f, "rels:// without a TLS connector — refusing plaintext downgrade")
            }
            RelayClientError::Dial(e) => write!(f, "relay tcp dial failed: {e}"),
            RelayClientError::Timeout(c) => write!(f, "relay phase timed out: {c:?}"),
            RelayClientError::Ws(e) => write!(f, "relay ws error: {e}"),
            RelayClientError::Codec(e) => write!(f, "relay codec error: {e}"),
            RelayClientError::AuthRejected => write!(f, "relay auth rejected (silent close, §3.4)"),
            RelayClientError::AuthTimeout => write!(f, "relay auth response timeout"),
            RelayClientError::AuthProtocol => {
                write!(f, "relay auth answered with a non-AuthResponse frame")
            }
            RelayClientError::ServerClosed => write!(f, "relay server ended the connection"),
            RelayClientError::KeepaliveTimeout => {
                write!(f, "relay keepalive timeout (35s no inbound)")
            }
            RelayClientError::TokenExpired => write!(f, "relay token expired — fail-closed"),
            RelayClientError::ConnectionLost => write!(f, "relay session lost mid-operation"),
            RelayClientError::OpenConnTimeout => {
                write!(f, "relay open_conn timeout (30s no PeersOnline)")
            }
            RelayClientError::SubscribeDuplicate => {
                write!(f, "relay subscribe already pending for peer")
            }
            RelayClientError::PeerOffline(_) => {
                write!(f, "relay destination peer is offline (refused locally)")
            }
            RelayClientError::Backpressured => {
                write!(f, "relay outbound queue full (dropped, upstream semantics)")
            }
            RelayClientError::NotReady { state } => {
                write!(f, "relay client not ready: state {state:?}")
            }
            RelayClientError::Stopped => write!(f, "relay client stopped"),
            RelayClientError::FrameTooLarge { limit, got } => {
                write!(f, "relay transport payload makes a {got}-byte frame, ceiling {limit}")
            }
        }
    }
}

impl std::error::Error for RelayClientError {}

/// Equality by kind: exact for shape-data variants, kind-only for the
/// `io::Error`/`WsError` carriers (they expose no structural equality).
/// Exists for typed assertions in tests and diagnostics.
impl PartialEq for RelayClientError {
    fn eq(&self, other: &Self) -> bool {
        use RelayClientError as E;
        match (self, other) {
            (E::UnsupportedUrl { reason: a }, E::UnsupportedUrl { reason: b }) => a == b,
            (E::TlsRequired, E::TlsRequired) => true,
            (E::Dial(a), E::Dial(b)) => a.kind() == b.kind(),
            (E::Timeout(a), E::Timeout(b)) => a == b,
            (E::Ws(_), E::Ws(_)) => true,
            (E::Codec(a), E::Codec(b)) => a == b,
            (E::AuthRejected, E::AuthRejected) => true,
            (E::AuthTimeout, E::AuthTimeout) => true,
            (E::AuthProtocol, E::AuthProtocol) => true,
            (E::ServerClosed, E::ServerClosed) => true,
            (E::KeepaliveTimeout, E::KeepaliveTimeout) => true,
            (E::TokenExpired, E::TokenExpired) => true,
            (E::ConnectionLost, E::ConnectionLost) => true,
            (E::OpenConnTimeout, E::OpenConnTimeout) => true,
            (E::SubscribeDuplicate, E::SubscribeDuplicate) => true,
            (E::PeerOffline(a), E::PeerOffline(b)) => a == b,
            (E::Backpressured, E::Backpressured) => true,
            (E::NotReady { state: a }, E::NotReady { state: b }) => a == b,
            (E::Stopped, E::Stopped) => true,
            (E::FrameTooLarge { limit: a, got: b }, E::FrameTooLarge { limit: c, got: d }) => {
                a == c && b == d
            }
            _ => false,
        }
    }
}

impl Eq for RelayClientError {}

fn ws_error_class(e: &WsError) -> RelayErrorClass {
    match e {
        WsError::Eof | WsError::AlreadyClosed => RelayErrorClass::ServerClosed,
        WsError::MessageTooLarge(_) => RelayErrorClass::FrameTooLarge,
        WsError::Io(_) => RelayErrorClass::Io,
        _ => RelayErrorClass::Ws,
    }
}

// ---------------------------------------------------------------------------
// token expiry judgement
// ---------------------------------------------------------------------------

/// Judgement of the current token against the clock (task §6: 「距过期多久」).
/// Boundary rule (fail-closed, documented): the payload IS the expiry
/// second, so `unix_now >= expires_at` is already Expired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenValidity {
    /// No token stored (cannot happen after `start`; completeness).
    Missing,
    /// Payload is not ASCII digits — unusable, fail-closed like expired.
    Malformed,
    Valid {
        /// Payload value (Unix seconds, §3.3).
        expires_at_unix: u64,
        /// `expires_at_unix − unix_now`.
        remaining: Duration,
    },
    Expired {
        /// `unix_now − expires_at_unix` (0 exactly at the boundary).
        expired_for: Duration,
    },
}

/// Parse the expiry second out of the token payload (ASCII Unix seconds,
/// §3.3). `None` = malformed/empty/non-numeric payload.
pub fn token_expiry_unix(token: &AuthToken) -> Option<u64> {
    let bytes = token.to_bytes();
    let payload = &bytes[1 + crate::relay::TOKEN_SIGNATURE_LEN..];
    let text = std::str::from_utf8(payload).ok()?;
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<u64>().ok()
}

pub fn token_validity(token: &AuthToken, unix_now: u64) -> TokenValidity {
    match token_expiry_unix(token) {
        None => TokenValidity::Malformed,
        Some(expires_at) => {
            if unix_now >= expires_at {
                TokenValidity::Expired { expired_for: Duration::from_secs(unix_now - expires_at) }
            } else {
                TokenValidity::Valid {
                    expires_at_unix: expires_at,
                    remaining: Duration::from_secs(expires_at - unix_now),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// stream / dial / TLS seams
// ---------------------------------------------------------------------------

/// Type-erased transport stream: what the dialer hands back and the WS layer
/// consumes (tokio `TcpStream` in production, in-memory duplex in tests).
pub trait RelayStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static> RelayStream for T {}

pub type BoxStream = Box<dyn RelayStream>;

/// Dial seam: open the raw transport stream for one parsed URL (TCP today;
/// TLS wrapping for `rels://` is applied by the caller-supplied
/// [`RelayTlsConnector`]). The returned future must own its inputs
/// (`'static`). Production: [`TcpDialer`]; tests: scripted/in-memory
/// dialers.
pub trait RelayDialer: Send + Sync + 'static {
    fn dial(
        &self,
        url: &RelayUrl,
    ) -> Pin<Box<dyn Future<Output = Result<BoxStream, RelayClientError>> + Send>>;
}

/// Production TCP dialer (plain `TcpStream::connect`).
///
/// TODO(increment D / device path): before ANY device use this must be
/// replaced by the protected-socket seam (`VpnConnection.protect` fd feed,
/// the N3-7 `mgmtsock` pattern) — an unprotected relay TCP connection
/// violates the N2-H endpoint-exclusion governance. Nothing in the
/// connector references this module yet, so no device path exists.
#[derive(Debug, Clone, Copy, Default)]
pub struct TcpDialer;

impl RelayDialer for TcpDialer {
    fn dial(
        &self,
        url: &RelayUrl,
    ) -> Pin<Box<dyn Future<Output = Result<BoxStream, RelayClientError>> + Send>> {
        let target = (url.host.clone(), url.port);
        Box::pin(async move {
            TcpStream::connect(target)
                .await
                .map(|s| Box::new(s) as BoxStream)
                .map_err(RelayClientError::Dial)
        })
    }
}

/// TLS seam: wrap an established TCP stream for `server_name` (SNI, §1.4).
/// The IMPLEMENTATION owns the rustls config and the trust root — this
/// module never decides certificate policy (management-path `ca_pem`
/// philosophy). TODO(increment D): the production connector (rustls with
/// the management-injected CA driving the async stream) lives with the
/// caller that already builds TLS configs for management/gRPC.
pub trait RelayTlsConnector: Send + Sync + 'static {
    fn connect(
        &self,
        stream: BoxStream,
        server_name: String,
    ) -> Pin<Box<dyn Future<Output = Result<BoxStream, RelayClientError>> + Send>>;
}

// ---------------------------------------------------------------------------
// state / stats / observability
// ---------------------------------------------------------------------------

/// Client state machine (task: Disconnected→Dialing→Handshaking→
/// Authenticating→Ready→Dead/Reconnecting). The reported transitions belong
/// to the WINNING attempt only — losing concurrent dials never touch the
/// state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayState {
    /// Created, worker not yet (or no longer) dialing.
    Disconnected,
    /// Dial round in flight (TCP [+TLS] toward one or more URLs).
    Dialing,
    /// WS upgrade of the winning attempt in flight.
    Handshaking,
    /// Auth frame sent, waiting for AuthResponse.
    Authenticating,
    /// Authenticated session live (frames flow).
    Ready,
    /// Between sessions: fast-retry, backoff, or the fail-closed
    /// expired-token hold.
    Reconnecting,
    /// Terminal: stopped by the owner (`stop()`); no further reconnects.
    Dead,
}

/// Point-in-time client snapshot (cheap clone; `Debug`-safe — counters,
/// URLs and shape tokens only, never token bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayStats {
    pub state: RelayState,
    /// Effective relay URL of the live (or last live) session,
    /// `rel://host:port` / `rels://host:port` form.
    pub current_url: Option<String>,
    /// `instanceURL` from the server's AuthResponse (§3.4).
    pub instance_url: Option<String>,
    /// Received relay frames by type byte (§2.2 table, index = type).
    pub frames_rx: [u64; 12],
    /// Sent relay frames by type byte.
    pub frames_tx: [u64; 12],
    /// Inbound Transport payload bytes.
    pub transport_rx_bytes: u64,
    /// Outbound Transport payload bytes.
    pub transport_tx_bytes: u64,
    /// Inbound Transport frames dropped because the recv queue was full
    /// (upstream 满则丢, §4.2 — WG retransmits).
    pub inbound_dropped_full: u64,
    /// Outbound frames dropped before the wire (queue full / encode or
    /// write failure — bounded queues, upstream WG retransmits).
    pub outbound_dropped: u64,
    /// `send_to_peer` refused locally because the peer was offline.
    pub send_rejected_offline: u64,
    /// Inbound frames with an illegal type byte (dropped + kept,
    /// upstream client.go:574-579).
    pub unknown_frames_rx: u64,
    /// `PeersWentOffline` notifications received.
    pub peers_went_offline_rx: u64,
    /// Dial attempts launched (each URL of each round counts once).
    pub dial_attempts: u64,
    /// Established (Ready) sessions that ended and triggered a reconnect.
    pub reconnects: u64,
    /// Successful Auth handshakes.
    pub auth_successes: u64,
    /// Backoff delays actually slept, in order (exact-sequence evidence).
    pub observed_backoff: Vec<Duration>,
    /// Token refusals (expired/malformed at the gate).
    pub token_expired_refusals: u64,
    pub last_error_class: Option<RelayErrorClass>,
    /// Transition history (newest last, capped — deterministic
    /// state-machine evidence for tests and diagnostics).
    pub state_history: Vec<RelayState>,
}

impl Default for RelayStats {
    fn default() -> Self {
        RelayStats {
            state: RelayState::Disconnected,
            current_url: None,
            instance_url: None,
            frames_rx: [0; 12],
            frames_tx: [0; 12],
            transport_rx_bytes: 0,
            transport_tx_bytes: 0,
            inbound_dropped_full: 0,
            outbound_dropped: 0,
            send_rejected_offline: 0,
            unknown_frames_rx: 0,
            peers_went_offline_rx: 0,
            dial_attempts: 0,
            reconnects: 0,
            auth_successes: 0,
            observed_backoff: Vec::new(),
            token_expired_refusals: 0,
            last_error_class: None,
            state_history: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// shared state + handle
// ---------------------------------------------------------------------------

/// Per-session command (handle → session task).
enum SessionCmd {
    SendTransport { peer: PeerId, payload: Vec<u8> },
    Subscribe { peer: PeerId, reply: oneshot::Sender<Result<(), RelayClientError>> },
    GracefulClose,
}

/// Reader-task → session-loop message.
enum SessionMsg {
    Frame(Frame),
    /// Illegal type byte (dropped + keep the connection, client.go:574-579).
    UnknownFrame,
    /// Undecodable frame — session ends (fail-closed).
    DecodeError,
    /// Inbound WS-level liveness without a relay frame (unsolicited Pong).
    KeepAlive,
    Ended(RelayErrorClass),
}

/// Session → reader I/O request. The reader owns the [`WsClient`] and ALL
/// socket I/O (a `read_message` await must never hold the transport away
/// from writers — a lock would starve writes on a silent server). Writes are
/// FIFO: a Subscribe is on the wire before any answer can be processed.
enum IoReq {
    Write(Frame),
    /// Relay Close + ack: the session must not report a graceful stop until
    /// the frame is actually on the wire (§4.2).
    WriteCloseAndStop(Frame, oneshot::Sender<()>),
}

struct Shared {
    cfg: RelayClientConfig,
    stats: Mutex<RelayStats>,
    token: Mutex<Option<AuthToken>>,
    token_epoch_tx: watch::Sender<u64>,
    stop_tx: watch::Sender<bool>,
    /// Live session's command sender (`None` between sessions).
    session: Mutex<Option<mpsc::Sender<SessionCmd>>>,
    /// Peer presence cache from `PeersOnline`/`PeersWentOffline` (cleared on
    /// every session end — server state is unknown after a reconnect).
    presence: Mutex<HashMap<PeerId, bool>>,
    /// Receiver half of the inbound Transport queue (`recv()` consumer).
    inbound: tokio::sync::Mutex<mpsc::Receiver<(PeerId, Vec<u8>)>>,
}

impl Shared {
    fn clock(&self) -> &Arc<dyn RelayClock> {
        &self.cfg.clock
    }

    fn bump(&self, f: impl FnOnce(&mut RelayStats)) {
        let mut stats = self.stats.lock().expect("relay stats lock");
        f(&mut stats);
    }

    fn set_state(&self, state: RelayState) {
        let mut stats = self.stats.lock().expect("relay stats lock");
        if stats.state == state {
            return;
        }
        stats.state = state;
        stats.state_history.push(state);
        if stats.state_history.len() > STATE_HISTORY_CAP {
            stats.state_history.remove(0);
        }
    }

    fn observe_error(&self, class: RelayErrorClass) {
        self.bump(|s| s.last_error_class = Some(class));
    }

    fn is_offline(&self, peer: &PeerId) -> bool {
        self.presence.lock().expect("relay presence lock").get(peer) == Some(&false)
    }
}

/// Configuration for [`RelayClient::start`]. Defaults: [`SystemClock`] +
/// [`TcpDialer`], no TLS connector — so `rels://` fails closed until one is
/// injected (see [`RelayTlsConnector`]).
pub struct RelayClientConfig {
    pub urls: Vec<RelayUrl>,
    /// Local peer id, derived from OUR WireGuard public key (§3.2).
    pub peer_id: PeerId,
    pub token: AuthToken,
    pub clock: Arc<dyn RelayClock>,
    pub dialer: Arc<dyn RelayDialer>,
    pub tls: Option<Arc<dyn RelayTlsConnector>>,
}

impl RelayClientConfig {
    /// Build from the management-synced `RelayServers` shape: `urls` as
    /// delivered, the local WG public key (base64 string form) and the
    /// assembled token.
    pub fn new(
        urls: &[String],
        wg_pubkey_b64: &str,
        token: AuthToken,
    ) -> Result<Self, RelayClientError> {
        let mut parsed = Vec::with_capacity(urls.len());
        for url in urls {
            parsed.push(RelayUrl::parse(url)?);
        }
        Ok(RelayClientConfig {
            urls: parsed,
            peer_id: PeerId::from_wg_pubkey_string(wg_pubkey_b64),
            token,
            clock: Arc::new(SystemClock),
            dialer: Arc::new(TcpDialer),
            tls: None,
        })
    }

    pub fn with_clock(mut self, clock: Arc<dyn RelayClock>) -> Self {
        self.clock = clock;
        self
    }

    pub fn with_dialer(mut self, dialer: Arc<dyn RelayDialer>) -> Self {
        self.dialer = dialer;
        self
    }

    pub fn with_tls(mut self, tls: Arc<dyn RelayTlsConnector>) -> Self {
        self.tls = Some(tls);
        self
    }
}

impl fmt::Debug for RelayClientConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Deliberately NOT the derived shape: only URL/host data and the
        // peer id (readable form) — never token material.
        f.debug_struct("RelayClientConfig")
            .field("urls", &self.urls.iter().map(|u| u.config_string()).collect::<Vec<_>>())
            .field("peer_id", &self.peer_id)
            .field("token", &"<redacted>")
            .finish()
    }
}

/// Handle to a running relay client. Cheap to clone; all methods are
/// `&self`. The worker stops only via [`RelayClient::stop`] (or runtime
/// teardown) — dropping the handle does NOT stop it.
#[derive(Clone)]
pub struct RelayClient {
    shared: Arc<Shared>,
}

impl RelayClient {
    /// Validate the configuration and start the worker task. Requires a
    /// running tokio runtime. At least one URL is mandatory (fail-closed).
    pub fn start(cfg: RelayClientConfig) -> Result<RelayClient, RelayClientError> {
        if cfg.urls.is_empty() {
            return Err(RelayClientError::UnsupportedUrl { reason: "no-urls" });
        }
        let (inbound_tx, inbound_rx) = mpsc::channel(INBOUND_CAP);
        let (token_epoch_tx, _token_epoch_rx) = watch::channel(0u64);
        let (stop_tx, _stop_rx) = watch::channel(false);
        let shared = Arc::new(Shared {
            token: Mutex::new(Some(cfg.token.clone())),
            token_epoch_tx,
            stop_tx,
            stats: Mutex::new(RelayStats {
                state_history: vec![RelayState::Disconnected],
                ..RelayStats::default()
            }),
            session: Mutex::new(None),
            presence: Mutex::new(HashMap::new()),
            inbound: tokio::sync::Mutex::new(inbound_rx),
            cfg,
        });
        tokio::spawn(worker_main(shared.clone(), inbound_tx));
        Ok(RelayClient { shared })
    }

    /// Current state (single source of truth: the stats snapshot).
    pub fn state(&self) -> RelayState {
        self.shared.stats.lock().expect("relay stats lock").state
    }

    /// Full observability snapshot (task §8: frame counts, bytes,
    /// reconnects, last error class, effective URL, backoff history).
    pub fn stats(&self) -> RelayStats {
        self.shared.stats.lock().expect("relay stats lock").clone()
    }

    /// Accept a freshly-synced token (increment-D entry point: call from the
    /// Sync handler on every new `RelayServers`). Wakes the fail-closed
    /// expired-token hold instantly. Per spec §6.1 a fresh token does NOT
    /// force a reconnect of a live session.
    pub fn update_token(&self, token: AuthToken) {
        *self.shared.token.lock().expect("relay token lock") = Some(token);
        let _ = self.shared.token_epoch_tx.send(1); // any change wakes holders
    }

    /// Judgement of the current token against the injected clock.
    pub fn token_validity(&self) -> TokenValidity {
        let guard = self.shared.token.lock().expect("relay token lock");
        match guard.as_ref() {
            None => TokenValidity::Missing,
            Some(t) => token_validity(t, self.shared.clock().unix_now()),
        }
    }

    /// Queue one outbound WG packet for `peer` (increment-D WG bind write
    /// path). NON-BLOCKING and bounded (upstream semantics, §4.2: a full
    /// queue drops rather than stalls the reader).
    ///
    /// `Ok` = accepted onto the wire path — the protocol has no delivery
    /// ack, so this is never a delivery confirmation. Known-offline peers
    /// are refused typed (`PeerOffline`) instead of being silently
    /// black-holed; oversized payloads are rejected before any wire byte.
    pub fn send_to_peer(&self, peer: &PeerId, payload: &[u8]) -> Result<(), RelayClientError> {
        let total = TRANSPORT_FRAME_OVERHEAD + payload.len();
        if total > MAX_MESSAGE_SIZE {
            return Err(RelayClientError::FrameTooLarge { limit: MAX_MESSAGE_SIZE, got: total });
        }
        let state = self.state();
        if state != RelayState::Ready {
            return Err(RelayClientError::NotReady { state });
        }
        if self.shared.is_offline(peer) {
            self.shared.bump(|s| s.send_rejected_offline += 1);
            return Err(RelayClientError::PeerOffline(peer.clone()));
        }
        let sender = self
            .shared
            .session
            .lock()
            .expect("relay session lock")
            .clone()
            .ok_or(RelayClientError::NotReady { state })?;
        match sender
            .try_send(SessionCmd::SendTransport { peer: peer.clone(), payload: payload.to_vec() })
        {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => Err(RelayClientError::Backpressured),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(RelayClientError::ConnectionLost),
        }
    }

    /// OpenConn (§4.1): subscribe `peer` and wait for `PeersOnline` (30s
    /// typed timeout; unsubscribes on timeout). Typed errors on duplicate
    /// pending subscribe, peer going offline while waiting, session loss or
    /// stop. TODO(increment D): call when a WG peer needs the relay
    /// carrier, then keep pumping via [`RelayClient::recv`]/`send_to_peer`.
    pub async fn open_conn(&self, peer: &PeerId) -> Result<(), RelayClientError> {
        let state = self.state();
        if state != RelayState::Ready {
            return Err(RelayClientError::NotReady { state });
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        let sender = self
            .shared
            .session
            .lock()
            .expect("relay session lock")
            .clone()
            .ok_or(RelayClientError::NotReady { state })?;
        sender
            .try_send(SessionCmd::Subscribe { peer: peer.clone(), reply: reply_tx })
            .map_err(|_| RelayClientError::Backpressured)?;
        match reply_rx.await {
            Ok(res) => res,
            // Session aborted without resolving (hard-stop path).
            Err(_) => Err(RelayClientError::Stopped),
        }
    }

    /// Next inbound Transport payload: `(sender_peer_id, payload)` — the
    /// 36B field of a received Transport frame is the SENDER id (§4.2, the
    /// server rewrites dstID in place). Returns `None` once the client has
    /// stopped and the queue drained. Queue-full drops are counted in stats
    /// (`inbound_dropped_full`) — never backpressured into the read loop
    /// (upstream semantics).
    pub async fn recv(&self) -> Option<(PeerId, Vec<u8>)> {
        self.shared.inbound.lock().await.recv().await
    }

    /// Stop the client: the live session sends the relay Close frame (§4.2
    /// graceful-exit shape, best effort) and the worker exits to `Dead`.
    /// Idempotent.
    pub fn stop(&self) {
        let _ = self.shared.stop_tx.send(true);
    }
}

/// Debug output: state + stats only. Never any token material.
impl fmt::Debug for RelayClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RelayClient").field("stats", &self.stats()).finish()
    }
}

// ---------------------------------------------------------------------------
// worker — connect policy
// ---------------------------------------------------------------------------

async fn worker_main(shared: Arc<Shared>, inbound_tx: mpsc::Sender<(PeerId, Vec<u8>)>) {
    let mut token_epoch = shared.token_epoch_tx.subscribe();
    let mut stop = shared.stop_tx.subscribe();
    let mut round: u32 = 0;
    // Last URL that COMPLETED auth — the fast-reconnect target (§6.2).
    let mut last_good: Option<RelayUrl> = None;
    'worker: loop {
        if *stop.borrow() {
            break;
        }
        // ---- token gate: fail-closed BEFORE any wire byte ----
        'gate: loop {
            let current = shared.token.lock().expect("relay token lock").clone();
            let verdict =
                current.as_ref().map(|t| token_validity(t, shared.clock().unix_now()));
            match verdict {
                Some(TokenValidity::Valid { .. }) => break 'gate,
                _ => {
                    shared.observe_error(RelayErrorClass::TokenExpired);
                    shared.bump(|s| s.token_expired_refusals += 1);
                    shared.set_state(RelayState::Reconnecting);
                    // Hold: wake on fresh token, stop, or the bounded
                    // re-check tick. NOTHING is dialed or sent here.
                    let deadline = shared.clock().now() + TOKEN_RECHECK_INTERVAL;
                    tokio::select! {
                        _ = shared.clock().sleep_until(deadline) => {}
                        _ = token_epoch.changed() => {}
                        _ = stop.changed() => {}
                    }
                    if *stop.borrow() {
                        break 'worker;
                    }
                }
            }
        };

        // ---- dial round ----
        let (set, delay) = if round == 0 {
            match &last_good {
                // §6.2: fast reconnect to the SAME server first (no delay).
                Some(url) => (vec![url.clone()], None),
                None => (first_wave(&shared.cfg.urls), None),
            }
        } else {
            // §6.2: exponential backoff, then a concurrent full-list round.
            (first_wave(&shared.cfg.urls), Some(backoff_delay(round - 1)))
        };
        if let Some(delay) = delay {
            shared.bump(|s| {
                s.observed_backoff.push(delay);
                if s.observed_backoff.len() > BACKOFF_LOG_CAP {
                    s.observed_backoff.remove(0);
                }
            });
            tokio::select! {
                _ = shared.clock().sleep_until(shared.clock().now() + delay) => {}
                _ = stop.changed() => {}
            }
            if *stop.borrow() {
                break;
            }
        }
        shared.set_state(RelayState::Dialing);
        // Anchor the whole round's dial budgets at round start (§1.5: one
        // connection budget, not per-restart).
        let round_anchor = shared.clock().now();
        shared.bump(|s| s.dial_attempts += set.len() as u64);
        let dialed = tokio::select! {
            outcome = dial_first_win(&shared, &set, round_anchor) => outcome,
            _ = stop.changed() => break,
        };
        let Some((url, stream)) = dialed else {
            round += 1;
            shared.set_state(RelayState::Reconnecting);
            continue;
        };

        // ---- session lifecycle (handshake + auth + runtime) ----
        let ctx = SessionCtx {
            shared: shared.clone(),
            url: url.clone(),
            stream,
            peer_id: shared.cfg.peer_id.clone(),
            inbound_tx: inbound_tx.clone(),
        };
        let (cmd_tx, cmd_rx) = mpsc::channel(CMD_CAP);
        *shared.session.lock().expect("relay session lock") = Some(cmd_tx.clone());
        let mut join = tokio::spawn(run_session(ctx, cmd_rx));
        let (class, was_ready) = tokio::select! {
            res = &mut join => res.unwrap_or((RelayErrorClass::ConnectionLost, false)),
            _ = stop.changed() => {
                // Graceful first (relay Close frame, §4.2), hard bound
                // behind it — stop is teardown, not on the deterministic
                // test path, so this one fuse is real time.
                let _ = cmd_tx.try_send(SessionCmd::GracefulClose);
                match tokio::time::timeout(Duration::from_secs(2), &mut join).await {
                    Ok(res) => res.unwrap_or((RelayErrorClass::Stopped, false)),
                    Err(_elapsed) => {
                        join.abort();
                        (RelayErrorClass::Stopped, false)
                    }
                }
            }
        };
        *shared.session.lock().expect("relay session lock") = None;
        shared.presence.lock().expect("relay presence lock").clear();
        shared.bump(|s| {
            s.last_error_class = Some(class);
            if was_ready {
                s.reconnects += 1;
            }
        });
        if was_ready {
            last_good = Some(url);
        }
        // §6.2: after an ESTABLISHED session ends, the next attempt is the
        // fast one; failed attempts (dial or auth) advance the backoff.
        round = if was_ready { 0 } else { round + 1 };
        shared.set_state(RelayState::Reconnecting);
        if *stop.borrow() {
            break;
        }
    }
    // Terminal state; dropping inbound_tx below releases `recv()` waiters.
    shared.set_state(RelayState::Dead);
}

fn first_wave(urls: &[RelayUrl]) -> Vec<RelayUrl> {
    urls.iter().take(MAX_CONCURRENT_URLS).cloned().collect()
}

/// Concurrent dials over the round's URL set (≤ [`MAX_CONCURRENT_URLS`]),
/// FIRST success wins, the rest are dropped (§6.2 `PickServer`). All dial
/// budgets share the round's anchored deadline.
async fn dial_first_win(
    shared: &Arc<Shared>,
    set: &[RelayUrl],
    round_anchor: Instant,
) -> Option<(RelayUrl, BoxStream)> {
    let mut tasks = tokio::task::JoinSet::new();
    for url in set {
        let shared = shared.clone();
        let url = url.clone();
        tasks.spawn(async move {
            let res = dial_stream(&shared, &url, round_anchor).await;
            (url, res)
        });
    }
    let mut winner = None;
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok((url, Ok(stream))) => {
                winner = Some((url, stream));
                break;
            }
            Ok((_, Err(e))) => shared.observe_error(e.class()),
            Err(_join_panic) => shared.observe_error(RelayErrorClass::ConnectionLost),
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    winner
}

/// One URL: TCP (+TLS via the injected connector). Bounded by
/// `DIAL_TIMEOUT` on the injected clock. `rels://` without a TLS connector
/// fails closed BEFORE dialing. No shared-state writes here — the reported
/// state machine belongs to the winning attempt only.
async fn dial_stream(
    shared: &Shared,
    url: &RelayUrl,
    round_anchor: Instant,
) -> Result<BoxStream, RelayClientError> {
    let cfg = &shared.cfg;
    let clock = cfg.clock.as_ref();
    if url.is_tls() && cfg.tls.is_none() {
        return Err(RelayClientError::TlsRequired);
    }
    let stream = within_deadline(
        clock,
        round_anchor + DIAL_TIMEOUT,
        RelayErrorClass::DialTimeout,
        cfg.dialer.clone().dial(url),
    )
    .await??;
    if url.is_tls() {
        let tls = cfg.tls.clone().expect("checked above");
        let server_name = url.host.clone();
        let fut = tls.connect(stream, server_name);
        Ok(within_deadline(clock, round_anchor + DIAL_TIMEOUT, RelayErrorClass::DialTimeout, fut)
            .await??)
    } else {
        Ok(stream)
    }
}

/// Wrap a future in an injected-clock budget: `Ok(inner)` = the future's own
/// result, `Err(_)` = the budget fired. `biased` prefers the work when both
/// complete simultaneously. The deadline is ABSOLUTE (anchored by the
/// caller) so an owner advancing the clock mid-setup cannot silently extend
/// the window.
async fn within_deadline<T, F>(
    clock: &dyn RelayClock,
    deadline: Instant,
    timeout_class: RelayErrorClass,
    fut: F,
) -> Result<T, RelayClientError>
where
    F: Future<Output = T>,
{
    tokio::select! {
        biased;
        res = fut => Ok(res),
        _ = clock.sleep_until(deadline) => Err(RelayClientError::Timeout(timeout_class)),
    }
}

// ---------------------------------------------------------------------------
// session — one live connection
// ---------------------------------------------------------------------------

struct SessionCtx {
    shared: Arc<Shared>,
    url: RelayUrl,
    stream: BoxStream,
    peer_id: PeerId,
    inbound_tx: mpsc::Sender<(PeerId, Vec<u8>)>,
}

/// Drive one connection to completion. Returns `(end_class, reached_ready)`
/// — the second flag separates true reconnects (an established session
/// broke) from failed attempts.
async fn run_session(
    ctx: SessionCtx,
    mut cmd_rx: mpsc::Receiver<SessionCmd>,
) -> (RelayErrorClass, bool) {
    let SessionCtx { shared, url, stream, peer_id, inbound_tx } = ctx;
    let clock = shared.clock().clone();

    // ---- Handshaking (§1) ----
    let handshake_anchor = clock.now();
    shared.set_state(RelayState::Handshaking);
    let ws = {
        let key = match random_sec_websocket_key() {
            Ok(key) => key,
            Err(e) => return (ws_error_class(&e), false),
        };
        let host_header = url.host_header();
        let fut = async {
            client_handshake(stream, &host_header, RELAY_WS_PATH, &key)
                .await
                .map_err(RelayClientError::Ws)
        };
        match within_deadline(
            clock.as_ref(),
            handshake_anchor + DIAL_TIMEOUT,
            RelayErrorClass::WsHandshake,
            fut,
        )
        .await
        {
            Ok(Ok(ws)) => ws,
            Ok(Err(e)) => return (e.class(), false),
            Err(timeout_err) => return (timeout_err.class(), false),
        }
    };

    // ---- Authenticating (§3) ----
    let auth_anchor = clock.now();
    shared.set_state(RelayState::Authenticating);
    let token = match shared.token.lock().expect("relay token lock").clone() {
        Some(t) => t,
        None => return (RelayErrorClass::TokenExpired, false),
    };
    let expires_at_unix = match token_validity(&token, clock.unix_now()) {
        TokenValidity::Valid { expires_at_unix, .. } => expires_at_unix,
        // Re-checked here so a token that expired between the worker gate
        // and this point still never touches the wire.
        _ => {
            shared.observe_error(RelayErrorClass::TokenExpired);
            shared.bump(|s| s.token_expired_refusals += 1);
            return (RelayErrorClass::TokenExpired, false);
        }
    };
    let ws = {
        let mut ws = ws;
        let auth = Frame::Auth { peer_id, token };
        let bytes = match auth.encode() {
            Ok(bytes) => bytes,
            Err(_) => return (RelayErrorClass::Codec, false),
        };
        shared.bump(|s| s.frames_tx[MSG_AUTH as usize] += 1);
        if let Err(e) = ws.write_binary(&bytes).await {
            return (ws_error_class(&e), false);
        }
        let first = match within_deadline(
            clock.as_ref(),
            auth_anchor + AUTH_RESPONSE_TIMEOUT,
            RelayErrorClass::AuthTimeout,
            ws.read_message(),
        )
        .await
        {
            Err(timeout_err) => return (timeout_err.class(), false),
            Ok(Err(e)) => {
                // §3.4: auth failure = silent close. DURING the auth phase
                // an EOF/close IS the typed rejection (an EOF here is how
                // the server says no); other transports errors keep their
                // own classes.
                let class = match &e {
                    WsError::Eof => RelayErrorClass::AuthRejected,
                    other => ws_error_class(other),
                };
                return (class, false);
            }
            Ok(Ok(msg)) => msg,
        };
        match first {
            // NOTE: the pre-auth read IS cancelled by the 8s budget (the
            // only read cancellation in this module). A WS ping arriving in
            // that exact window would poison the session — fail-closed
            // redial, by construction.
            WsMessage::Binary(bytes) => match Frame::decode(&bytes) {
                Ok(Frame::AuthResponse { instance_url }) => {
                    shared.bump(|s| {
                        s.frames_rx[MSG_AUTH_RESPONSE as usize] += 1;
                        s.instance_url = Some(instance_url);
                        s.current_url = Some(url.config_string());
                        s.auth_successes += 1;
                    });
                    ws
                }
                Ok(_) => return (RelayErrorClass::AuthProtocol, false),
                Err(_) => return (RelayErrorClass::Codec, false),
            },
            WsMessage::Closed(_) => return (RelayErrorClass::AuthRejected, false),
            WsMessage::Text(_) | WsMessage::Pong(_) => {
                return (RelayErrorClass::AuthProtocol, false)
            }
        }
    };

    // ---- Ready (§4/§5) ----
    // Liveness is anchored to the LAST INBOUND instant — captured BEFORE the
    // observable Ready transition — never to "loop entry time": an owner
    // that observes Ready and immediately advances the clock must not have
    // its signal lost to a deadline recomputed after the jump.
    let anchor = clock.now();
    let anchor_unix = clock.unix_now();
    shared.set_state(RelayState::Ready);
    let (msg_tx, mut msg_rx) = mpsc::channel(SESSION_MSG_CAP);
    let (io_tx, io_rx) = mpsc::channel::<IoReq>(IO_CAP);
    // The reader owns the WsClient and ALL socket I/O from here on; the
    // session (and through it every caller) only sends I/O requests.
    tokio::spawn(reader_loop(ws, msg_tx, io_rx, shared.clone()));
    let mut last_inbound = anchor;
    // Token expiry mapped onto the monotonic domain for the wake trigger,
    // anchored to the same instant (the authoritative check re-reads the
    // unix clock when it fires).
    let token_deadline_mono =
        anchor + Duration::from_secs(expires_at_unix.saturating_sub(anchor_unix));
    let mut waiters: HashMap<
        PeerId,
        (Vec<oneshot::Sender<Result<(), RelayClientError>>>, Instant),
    > = HashMap::new();
    let class = loop {
        let mut deadline = last_inbound + KEEPALIVE_TIMEOUT;
        if token_deadline_mono < deadline {
            deadline = token_deadline_mono;
        }
        for (_, (_, waiter_deadline)) in waiters.iter() {
            if *waiter_deadline < deadline {
                deadline = *waiter_deadline;
            }
        }
        tokio::select! {
            msg = msg_rx.recv() => match msg {
                None => break RelayErrorClass::ConnectionLost,
                Some(SessionMsg::KeepAlive) => {
                    last_inbound = clock.now();
                }
                Some(SessionMsg::UnknownFrame) => {
                    // Drop + keep (upstream client behavior).
                    shared.bump(|s| s.unknown_frames_rx += 1);
                }
                Some(SessionMsg::DecodeError) => break RelayErrorClass::Codec,
                Some(SessionMsg::Ended(class)) => break class,
                Some(SessionMsg::Frame(frame)) => {
                    // ANY inbound frame proves liveness (§5; task: 35s 无
                    // 入站判死).
                    last_inbound = clock.now();
                    let frame_type = frame.msg_type();
                    shared.bump(|s| s.frames_rx[frame_type as usize] += 1);
                    match classify_inbound(frame) {
                        InboundAction::EchoHealthCheck => {
                            // Fire and forget: the echo is best effort; a
                            // dying reader ends the session anyway.
                            let _ = send_io(&io_tx, &shared, Frame::HealthCheck).await;
                        }
                        InboundAction::DeliverTransport { sender, payload } => {
                            shared.bump(|s| s.transport_rx_bytes += payload.len() as u64);
                            if inbound_tx.send((sender, payload)).await.is_err() {
                                shared.bump(|s| s.inbound_dropped_full += 1);
                            }
                        }
                        InboundAction::PeersOnline { peer_ids } => {
                            let mut presence =
                                shared.presence.lock().expect("relay presence lock");
                            for peer in peer_ids {
                                presence.insert(peer.clone(), true);
                                if let Some((replies, _)) = waiters.remove(&peer) {
                                    for reply in replies {
                                        let _ = reply.send(Ok(()));
                                    }
                                }
                            }
                        }
                        InboundAction::PeersWentOffline { peer_ids } => {
                            shared.bump(|s| s.peers_went_offline_rx += 1);
                            let mut presence =
                                shared.presence.lock().expect("relay presence lock");
                            for peer in peer_ids {
                                presence.insert(peer.clone(), false);
                                if let Some((replies, _)) = waiters.remove(&peer) {
                                    for reply in replies {
                                        let _ = reply
                                            .send(Err(RelayClientError::ConnectionLost));
                                    }
                                }
                            }
                        }
                        InboundAction::EndSession => break RelayErrorClass::ServerClosed,
                        InboundAction::Ignore => {}
                    }
                }
            },
            cmd = cmd_rx.recv() => match cmd {
                None => break RelayErrorClass::ConnectionLost,
                Some(SessionCmd::GracefulClose) => {
                    // §4.2: relay Close = graceful exit of the WHOLE
                    // connection. Acked by the reader AFTER the frame is on
                    // the wire (the reader owns all frames_tx counting); if
                    // the reader died first, the teardown decision is
                    // unchanged.
                    let (ack_tx, ack_rx) = oneshot::channel();
                    if io_tx.send(IoReq::WriteCloseAndStop(Frame::Close, ack_tx)).await.is_ok() {
                        let _ = ack_rx.await;
                    }
                    break RelayErrorClass::Stopped;
                }
                Some(SessionCmd::SendTransport { peer, payload }) => {
                    // Presence may have flipped since the handle check.
                    if shared.is_offline(&peer) {
                        shared.bump(|s| s.send_rejected_offline += 1);
                        continue;
                    }
                    // Fire and forget: delivery is the relay server's job
                    // now; a full queue was already counted as a drop.
                    let _ = send_io(&io_tx, &shared, Frame::Transport { peer_id: peer, payload })
                        .await;
                }
                Some(SessionCmd::Subscribe { peer, reply }) => {
                    if waiters.contains_key(&peer) {
                        let _ = reply.send(Err(RelayClientError::SubscribeDuplicate));
                        continue;
                    }
                    let frame = Frame::SubscribePeerState { peer_ids: vec![peer.clone()] };
                    if send_io(&io_tx, &shared, frame).await.is_err() {
                        let _ = reply.send(Err(RelayClientError::ConnectionLost));
                        break RelayErrorClass::ConnectionLost;
                    }
                    waiters.insert(peer, (vec![reply], clock.now() + OPEN_CONN_TIMEOUT));
                }
            },
            _ = clock.sleep_until(deadline) => {
                let now = clock.now();
                if now >= last_inbound + KEEPALIVE_TIMEOUT {
                    // §5: 35s without inbound ⇒ dead ⇒ reconnect. Never
                    // silently fake-alive.
                    break RelayErrorClass::KeepaliveTimeout;
                }
                if clock.unix_now() >= expires_at_unix {
                    // Fail-closed (task §6): an expired token kills the
                    // session; nothing further is sent with it. STRICTER
                    // than upstream — deliberate, documented deviation.
                    break RelayErrorClass::TokenExpired;
                }
                // OpenConn timeouts: resolve + unsubscribe (§4.1).
                let expired: Vec<PeerId> = waiters
                    .iter()
                    .filter(|(_, (_, waiter_deadline))| now >= *waiter_deadline)
                    .map(|(peer, _)| peer.clone())
                    .collect();
                for peer in expired {
                    if let Some((replies, _)) = waiters.remove(&peer) {
                        for reply in replies {
                            let _ = reply.send(Err(RelayClientError::OpenConnTimeout));
                        }
                        // §4.1: timeout unsubscribes the interest (fire and
                        // forget — the session may be ending anyway).
                        let _ = send_io(
                            &io_tx,
                            &shared,
                            Frame::UnsubscribePeerState { peer_ids: vec![peer] },
                        )
                        .await;
                    }
                }
            }
        }
    };
    // The reader exits on its own once `io_tx`/`msg_tx` drop with this
    // function (both channel-closes break its loop) — no abort, so an
    // in-flight queued write always gets its chance before shutdown.
    drop(io_tx);
    // Any op still parked on a waiter learns the session ended.
    for (_, (replies, _)) in waiters.drain() {
        for reply in replies {
            let _ = reply.send(Err(RelayClientError::ConnectionLost));
        }
    }
    shared.presence.lock().expect("relay presence lock").clear();
    (class, true)
}

/// The session's I/O owner: the ONLY task touching the socket after the
/// handshake. Reads are never cancelled mid-parse unless the session itself
/// is over (the select below re-polls both arms; a write request simply
/// interleaves between complete inbound messages — the wire is FIFO, so a
/// Subscribe is always out before any answer can be processed).
async fn reader_loop(
    mut ws: WsClient<BoxStream>,
    msg_tx: mpsc::Sender<SessionMsg>,
    mut io_rx: mpsc::Receiver<IoReq>,
    shared: Arc<Shared>,
) {
    loop {
        tokio::select! {
            msg = ws.read_message() => {
                let out = match msg {
                    Ok(WsMessage::Binary(bytes)) => match Frame::decode(&bytes) {
                        Ok(frame) => SessionMsg::Frame(frame),
                        Err(RelayError::UnknownType(_)) => SessionMsg::UnknownFrame,
                        // §2.6: one relay frame per binary WS message;
                        // anything else undecodable is a fail-closed break.
                        Err(_) => SessionMsg::DecodeError,
                    },
                    // Text is a relay-protocol violation on this socket (§2.6).
                    Ok(WsMessage::Text(_)) => SessionMsg::Ended(RelayErrorClass::Ws),
                    Ok(WsMessage::Pong(_)) => SessionMsg::KeepAlive,
                    Ok(WsMessage::Closed(_)) => SessionMsg::Ended(RelayErrorClass::ServerClosed),
                    Err(e) => SessionMsg::Ended(ws_error_class(&e)),
                };
                if msg_tx.send(out).await.is_err() {
                    break;
                }
            }
            req = io_rx.recv() => match req {
                None => break, // session over: stop owning the socket
                Some(IoReq::Write(frame)) => {
                    write_owned(&mut ws, &shared, frame).await;
                }
                Some(IoReq::WriteCloseAndStop(frame, ack)) => {
                    let _ = write_owned(&mut ws, &shared, frame).await;
                    let _ = ack.send(());
                    break;
                }
            },
        }
    }
}

/// Write one queued frame, counting it only when it is fully on the wire.
async fn write_owned(ws: &mut WsClient<BoxStream>, shared: &Shared, frame: Frame) {
    let frame_type = frame.msg_type();
    let payload_len = match &frame {
        Frame::Transport { payload, .. } => payload.len() as u64,
        _ => 0,
    };
    let bytes = match frame.encode() {
        Ok(bytes) => bytes,
        Err(_) => {
            shared.bump(|s| s.outbound_dropped += 1);
            return;
        }
    };
    if ws.write_binary(&bytes).await.is_ok() {
        shared.bump(move |s| {
            s.frames_tx[frame_type as usize] += 1;
            s.transport_tx_bytes += payload_len;
        });
    } else {
        shared.bump(|s| s.outbound_dropped += 1);
    }
}

/// Queue one frame to the reader. `Err(())` = the reader is gone (session
/// ending). A full queue counts as an upstream-shaped drop, never blocks.
async fn send_io(io_tx: &mpsc::Sender<IoReq>, shared: &Shared, frame: Frame) -> Result<(), ()> {
    match io_tx.try_send(IoReq::Write(frame)) {
        Ok(()) => Ok(()),
        Err(mpsc::error::TrySendError::Full(_)) => {
            shared.bump(|s| s.outbound_dropped += 1);
            Ok(())
        }
        Err(mpsc::error::TrySendError::Closed(_)) => Err(()),
    }
}

/// Pure inbound-frame decision table (§2.2 receive set + §4/§5 semantics).
/// Unit-tested exhaustively; the session loop only executes it.
#[derive(Debug)]
enum InboundAction {
    /// §5: answer with the exact 2-byte `01 05` echo.
    EchoHealthCheck,
    /// §4.2 receive-side: the frame's id field is the SENDER.
    DeliverTransport { sender: PeerId, payload: Vec<u8> },
    PeersOnline { peer_ids: Vec<PeerId> },
    PeersWentOffline { peer_ids: Vec<PeerId> },
    /// §4.2: server ends the WHOLE relay connection.
    EndSession,
    /// Out-of-place types (AuthResponse after handshake, client→server
    /// types echoing back, Auth) — drop + keep (upstream client behavior).
    Ignore,
}

fn classify_inbound(frame: Frame) -> InboundAction {
    match frame {
        Frame::HealthCheck => InboundAction::EchoHealthCheck,
        Frame::Transport { peer_id, payload } => {
            InboundAction::DeliverTransport { sender: peer_id, payload }
        }
        Frame::PeersOnline { peer_ids } => InboundAction::PeersOnline { peer_ids },
        Frame::PeersWentOffline { peer_ids } => InboundAction::PeersWentOffline { peer_ids },
        Frame::Close => InboundAction::EndSession,
        Frame::Auth { .. }
        | Frame::AuthResponse { .. }
        | Frame::SubscribePeerState { .. }
        | Frame::UnsubscribePeerState { .. } => InboundAction::Ignore,
    }
}

// ---------------------------------------------------------------------------
// unit tests — deterministic, injected clock/dialer, zero real sleeping
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::{MSG_HEALTH_CHECK, MSG_TRANSPORT};
    use std::collections::VecDeque;
    // Trait-scope imports for poll-method syntax on the duplex helpers.
    use tokio::io::{AsyncRead, AsyncWrite};

    // ---- URL parsing ------------------------------------------------------

    #[test]
    fn url_parse_maps_rel_and_rels_with_default_ports() {
        let rel = RelayUrl::parse("rel://relay.example").unwrap();
        assert_eq!(rel.scheme(), RelayScheme::Rel);
        assert_eq!(rel.host(), "relay.example");
        assert_eq!(rel.port(), 80, "§1.1: no relay default upstream → ws default 80");
        assert!(!rel.is_tls());
        assert_eq!(rel.ws_url(), "ws://relay.example:80");
        assert_eq!(rel.config_string(), "rel://relay.example:80");
        assert_eq!(rel.host_header(), "relay.example:80");

        let rels = RelayUrl::parse("rels://home.alfadb.cn:28443").unwrap();
        assert_eq!(rels.scheme(), RelayScheme::Rels);
        assert_eq!(rels.port(), 28443, "explicit port kept verbatim");
        assert!(rels.is_tls());
        assert_eq!(rels.ws_url(), "wss://home.alfadb.cn:28443");
        assert_eq!(rels.config_string(), "rels://home.alfadb.cn:28443");

        let rels_default = RelayUrl::parse("rels://relay.example").unwrap();
        assert_eq!(rels_default.port(), 443, "wss default 443");
    }

    #[test]
    fn url_parse_is_case_insensitive_like_url_schemes() {
        let upper = RelayUrl::parse("RELS://relay.example:443").unwrap();
        assert_eq!(upper.scheme(), RelayScheme::Rels);
        let mixed = RelayUrl::parse("Rel://relay.example").unwrap();
        assert_eq!(mixed.scheme(), RelayScheme::Rel);
    }

    #[test]
    fn url_parse_accepts_ipv6_literals() {
        let v6 = RelayUrl::parse("rel://[::1]:8080").unwrap();
        assert_eq!(v6.host(), "::1");
        assert_eq!(v6.port(), 8080);
        assert_eq!(v6.host_header(), "[::1]:8080");
        let v6_default = RelayUrl::parse("rels://[2001:db8::1]").unwrap();
        assert_eq!(v6_default.port(), 443);
        assert_eq!(v6_default.host_header(), "[2001:db8::1]:443");
    }

    #[test]
    fn url_parse_fails_closed_on_every_other_scheme() {
        for bad in [
            "quic://relay.example:443", // N13 plan: no QUIC this increment
            "turn://relay.example:3478",
            "http://relay.example",
            "https://relay.example",
            "ws://relay.example",
            "wss://relay.example",
            "foo://relay.example",
        ] {
            let err = RelayUrl::parse(bad).unwrap_err();
            assert_eq!(err, RelayClientError::UnsupportedUrl { reason: "scheme" }, "{bad}");
            assert_eq!(err.class(), RelayErrorClass::Url);
        }
    }

    #[test]
    fn url_parse_fails_closed_on_malformed_authorities() {
        let cases = [
            ("", "missing-scheme-separator"),
            ("rel://", "host-empty"),
            ("://host", "scheme"), // empty scheme never matches rel/rels
            ("rels://host/extra/path", "path-not-allowed"),
            ("rels://host?x=1", "path-not-allowed"),
            ("rels://user@host:443", "userinfo-not-allowed"),
            ("rels://host :443", "host"),
            ("rel:// host", "host"),
            ("rels://host:0", "port-range"),
            ("rels://host:65536", "port-range"),
            ("rels://host:-1", "port"),
            ("rels://host:abc", "port"),
            ("rels://host:", "port"),
            ("rels://nöthost:443", "not-ascii"),
            ("rels://[::1:443", "host"),
        ];
        for (input, reason) in cases {
            let err = RelayUrl::parse(input).unwrap_err();
            assert_eq!(err, RelayClientError::UnsupportedUrl { reason }, "input {input:?}");
        }
    }

    #[test]
    fn start_refuses_an_empty_url_list_fail_closed() {
        let urls: Vec<String> = Vec::new();
        let cfg = RelayClientConfig::new(&urls, "k", token_with_payload("1770000000"))
            .expect("zero urls parse trivially");
        let err = RelayClient::start(cfg).unwrap_err();
        assert_eq!(err, RelayClientError::UnsupportedUrl { reason: "no-urls" });
    }

    // ---- token expiry judgement ------------------------------------------

    const SIG_B64: &str = "paWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaU=";

    fn token_with_payload(payload: &str) -> AuthToken {
        AuthToken::from_management(payload, SIG_B64).expect("fabricated token is valid")
    }

    #[test]
    fn token_validity_boundaries_exact_past_and_future() {
        let token = token_with_payload("1770000000");
        // Exactly at the expiry second: already Expired (fail-closed rule).
        assert_eq!(
            token_validity(&token, 1_770_000_000),
            TokenValidity::Expired { expired_for: Duration::ZERO }
        );
        // Past.
        assert_eq!(
            token_validity(&token, 1_770_000_042),
            TokenValidity::Expired { expired_for: Duration::from_secs(42) }
        );
        // Ample remaining.
        assert_eq!(
            token_validity(&token, 1_760_000_000),
            TokenValidity::Valid {
                expires_at_unix: 1_770_000_000,
                remaining: Duration::from_secs(10_000_000)
            }
        );
        // One second before the boundary is still valid.
        assert_eq!(
            token_validity(&token, 1_769_999_999),
            TokenValidity::Valid {
                expires_at_unix: 1_770_000_000,
                remaining: Duration::from_secs(1)
            }
        );
    }

    #[test]
    fn token_validity_malformed_payloads_are_fail_closed() {
        // Non-digit ASCII payloads assemble as valid AuthTokens (the codec
        // allows any ASCII) but are useless as expiries → Malformed.
        for payload in ["not-digits", "12x4", "1770-0000", "+123", " 5"] {
            let token = AuthToken::from_management(payload, SIG_B64)
                .unwrap_or_else(|e| panic!("{payload:?} must assemble: {e}"));
            assert_eq!(
                token_validity(&token, 0),
                TokenValidity::Malformed,
                "{payload:?}"
            );
        }
        // An empty payload cannot assemble an AuthToken at all (codec).
        assert!(AuthToken::from_management("", SIG_B64).is_err());
    }

    // ---- backoff sequence -------------------------------------------------

    #[test]
    fn backoff_sequence_is_exact_and_capped_at_60s() {
        let want: Vec<Duration> = [2u64, 4, 8, 16, 32, 60, 60, 60, 60, 60]
            .iter()
            .map(|s| Duration::from_secs(*s))
            .collect();
        let got: Vec<Duration> = (0..10u32).map(backoff_delay).collect();
        assert_eq!(got, want, "exact deterministic sequence 2s×2 capped 60s");
    }

    // ---- inbound classification -------------------------------------------

    fn fixture_peer(tag: u8) -> PeerId {
        PeerId::from_wg_pubkey_string(&format!("relay-client-unit-peer-{tag:02}"))
    }

    #[test]
    fn inbound_classification_covers_the_whole_receive_set() {
        // HealthCheck → exact echo (01 05 pinned at the codec layer).
        assert!(matches!(classify_inbound(Frame::HealthCheck), InboundAction::EchoHealthCheck));
        // Transport: id field is the SENDER on the receive side (§4.2).
        match classify_inbound(Frame::Transport {
            peer_id: fixture_peer(1),
            payload: b"wg".to_vec(),
        }) {
            InboundAction::DeliverTransport { sender, payload } => {
                assert_eq!(sender, fixture_peer(1));
                assert_eq!(payload, b"wg");
            }
            other => panic!("want DeliverTransport, got {other:?}"),
        }
        match classify_inbound(Frame::PeersOnline { peer_ids: vec![fixture_peer(2)] }) {
            InboundAction::PeersOnline { peer_ids } => assert_eq!(peer_ids, vec![fixture_peer(2)]),
            other => panic!("got {other:?}"),
        }
        match classify_inbound(Frame::PeersWentOffline { peer_ids: vec![fixture_peer(2)] }) {
            InboundAction::PeersWentOffline { peer_ids } => {
                assert_eq!(peer_ids, vec![fixture_peer(2)])
            }
            other => panic!("got {other:?}"),
        }
        // Server Close ends the session (§4.2).
        assert!(matches!(classify_inbound(Frame::Close), InboundAction::EndSession));
        // Client→server types echoing back and post-handshake AuthResponse:
        // ignored.
        let ignore_cases = [
            Frame::AuthResponse { instance_url: "rels://late.example:1".into() },
            Frame::SubscribePeerState { peer_ids: vec![fixture_peer(3)] },
            Frame::UnsubscribePeerState { peer_ids: vec![fixture_peer(3)] },
        ];
        for frame in ignore_cases {
            let what = format!("{frame:?}");
            assert!(matches!(classify_inbound(frame), InboundAction::Ignore), "{what}");
        }
    }

    // ---- in-memory duplex + scripted WS server (no sockets) ---------------

    #[derive(Default)]
    struct HalfState {
        incoming: Vec<u8>,
        eof: bool,
        closed_for_write: bool,
        waker: Option<std::task::Waker>,
    }

    #[derive(Clone)]
    struct DuplexEnd {
        my: Arc<Mutex<HalfState>>,
        peer: Arc<Mutex<HalfState>>,
    }

    fn duplex_pair() -> (DuplexEnd, DuplexEnd) {
        let a = Arc::new(Mutex::new(HalfState::default()));
        let b = Arc::new(Mutex::new(HalfState::default()));
        (DuplexEnd { my: a.clone(), peer: b.clone() }, DuplexEnd { my: b, peer: a })
    }

    impl tokio::io::AsyncRead for DuplexEnd {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let this = self.get_mut();
            let mut me = this.my.lock().unwrap();
            if me.incoming.is_empty() {
                if me.eof {
                    return Poll::Ready(Ok(())); // EOF: zero filled
                }
                me.waker = Some(cx.waker().clone());
                return Poll::Pending;
            }
            let n = me.incoming.len().min(buf.remaining());
            buf.initialize_unfilled()[..n].copy_from_slice(&me.incoming[..n]);
            buf.advance(n);
            me.incoming.drain(..n);
            Poll::Ready(Ok(()))
        }
    }

    impl tokio::io::AsyncWrite for DuplexEnd {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            let this = self.get_mut();
            if this.my.lock().unwrap().closed_for_write {
                return Poll::Ready(Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed")));
            }
            let mut peer = this.peer.lock().unwrap();
            peer.incoming.extend_from_slice(buf);
            if let Some(w) = peer.waker.take() {
                w.wake();
            }
            Poll::Ready(Ok(buf.len()))
        }
        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            let this = self.get_mut();
            this.my.lock().unwrap().closed_for_write = true;
            let mut peer = this.peer.lock().unwrap();
            peer.eof = true;
            if let Some(w) = peer.waker.take() {
                w.wake();
            }
            Poll::Ready(Ok(()))
        }
    }

    // Raw stream helpers (no io-util: frozen feature set).
    async fn read_some(io: &mut DuplexEnd, buf: &mut [u8]) -> io::Result<usize> {
        std::future::poll_fn(|cx| {
            let mut rb = tokio::io::ReadBuf::new(buf);
            match Pin::new(&mut *io).poll_read(cx, &mut rb) {
                Poll::Ready(Ok(())) => Poll::Ready(Ok(rb.filled().len())),
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Pending => Poll::Pending,
            }
        })
        .await
    }

    async fn write_all(io: &mut DuplexEnd, mut data: &[u8]) -> io::Result<()> {
        while !data.is_empty() {
            let n = std::future::poll_fn(|cx| Pin::new(&mut *io).poll_write(cx, data)).await?;
            data = &data[n..];
        }
        Ok(())
    }

    async fn read_exact(io: &mut DuplexEnd, out: &mut [u8]) -> io::Result<()> {
        let mut filled = 0;
        while filled < out.len() {
            let n = read_some(io, &mut out[filled..]).await?;
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "eof"));
            }
            filled += n;
        }
        Ok(())
    }

    /// Send one unmasked server frame (server frames are never masked).
    async fn server_send(io: &mut DuplexEnd, payload: &[u8]) -> io::Result<()> {
        let mut wire = vec![0x82u8];
        if payload.len() < 126 {
            wire.push(payload.len() as u8);
        } else {
            wire.push(126);
            wire.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        }
        wire.extend_from_slice(payload);
        write_all(io, &wire).await
    }

    /// Scripted in-memory relay server behaviors.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum ScriptMode {
        /// Complete upgrade+Auth, then NEVER send anything: drives the 35s
        /// liveness death deterministically.
        Silent,
        /// Complete upgrade, consume the Auth frame, then answer NOTHING:
        /// drives the 8s AuthResponse budget deterministically.
        StallAuth,
        /// Complete upgrade+Auth, then push a HealthCheck every 10ms REAL
        /// time (test scaffolding only — the CLIENT clock stays virtual):
        /// keeps liveness alive while virtual time advances, so token
        /// expiry can be isolated from keepalive death.
        Heartbeat,
    }

    /// Scripted dialer: per-host plan queues; records dial order.
    struct ScriptedDialer {
        peer_id: PeerId,
        plans: Mutex<HashMap<String, VecDeque<DialPlan>>>,
        calls: Mutex<Vec<String>>,
    }

    enum DialPlan {
        Fail(&'static str),
        Serve(ScriptMode),
    }

    impl RelayDialer for ScriptedDialer {
        fn dial(
            &self,
            url: &RelayUrl,
        ) -> Pin<Box<dyn Future<Output = Result<BoxStream, RelayClientError>> + Send>> {
            self.calls.lock().unwrap().push(url.config_string());
            let plan = self
                .plans
                .lock()
                .unwrap()
                .get_mut(&url.host)
                .and_then(|q| q.pop_front())
                .unwrap_or_else(|| panic!("script exhausted for {}", url.config_string()));
            match plan {
                DialPlan::Fail(why) => Box::pin(async move {
                    Err(RelayClientError::Dial(io::Error::new(
                        io::ErrorKind::ConnectionRefused,
                        why,
                    )))
                }),
                DialPlan::Serve(mode) => {
                    let (client_end, server_end) = duplex_pair();
                    let expect_peer = self.peer_id.clone();
                    tokio::spawn(scripted_server(server_end, expect_peer, mode));
                    Box::pin(async move { Ok(Box::new(client_end) as BoxStream) })
                }
            }
        }
    }

    impl ScriptedDialer {
        fn new(peer_id: PeerId) -> Self {
            ScriptedDialer {
                peer_id,
                plans: Mutex::new(HashMap::new()),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn plan(&self, host: &str, plans: impl IntoIterator<Item = DialPlan>) -> &Self {
            self.plans
                .lock()
                .unwrap()
                .insert(host.to_string(), plans.into_iter().collect());
            self
        }

        fn called(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    /// The minimal WS server side over the duplex: upgrade + Auth check +
    /// AuthResponse, then behave per script. Hand-rolled framing is small
    /// because client frames are always masked and our tests are small.
    async fn scripted_server(mut io: DuplexEnd, expect_peer: PeerId, mode: ScriptMode) {
        // -- read the HTTP head --
        let mut head: Vec<u8> = Vec::new();
        loop {
            if let Some(pos) = head.windows(4).position(|w| w == b"\r\n\r\n") {
                head.truncate(pos);
                break;
            }
            let mut tmp = [0u8; 512];
            match read_some(&mut io, &mut tmp).await {
                Ok(0) | Err(_) => return,
                Ok(n) => head.extend_from_slice(&tmp[..n]),
            }
        }
        let head = String::from_utf8(head).expect("utf8 handshake");
        assert!(head.starts_with("GET /relay HTTP/1.1\r\n"), "path pinned /relay: {head:?}");
        let key_line = head
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("sec-websocket-key:"))
            .expect("ws key present");
        let key = key_line.split_once(':').unwrap().1.trim().to_string();
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Accept: {}\r\n\
             \r\n",
            crate::ws::accept_key(&key)
        );
        write_all(&mut io, response.as_bytes()).await.expect("upgrade reply");

        // -- relay frames --
        loop {
            let mut header = [0u8; 2];
            if read_exact(&mut io, &mut header).await.is_err() {
                return;
            }
            assert_eq!(header[0] & 0x0F, 0x02, "client frames are binary");
            assert_eq!(header[1] & 0x80, 0x80, "client frames must be masked");
            let len7 = (header[1] & 0x7F) as usize;
            let payload_len = if len7 < 126 {
                len7
            } else if len7 == 126 {
                let mut ext = [0u8; 2];
                if read_exact(&mut io, &mut ext).await.is_err() {
                    return;
                }
                u16::from_be_bytes(ext) as usize
            } else {
                panic!("scripted server only sees small test frames");
            };
            let mut mask = [0u8; 4];
            if read_exact(&mut io, &mut mask).await.is_err() {
                return;
            }
            let mut payload = vec![0u8; payload_len];
            if read_exact(&mut io, &mut payload).await.is_err() {
                return;
            }
            for (i, b) in payload.iter_mut().enumerate() {
                *b ^= mask[i & 3];
            }
            let relay = Frame::decode(&payload).expect("client sends valid relay frames");
            if let Frame::Auth { peer_id, .. } = relay {
                assert_eq!(peer_id, expect_peer, "auth carries OUR derived peer id");
                if mode != ScriptMode::StallAuth {
                    let reply = Frame::AuthResponse {
                        instance_url: "rels://scripted.relay.test:443".to_string(),
                    }
                    .encode()
                    .expect("auth response encodes");
                    let _ = server_send(&mut io, &reply).await;
                    if mode == ScriptMode::Heartbeat {
                        // §5 server-side sender, shrunk for the test; dies
                        // with the duplex when the session/runtime ends.
                        loop {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            if server_send(&mut io, &[0x01, 0x05]).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        }
    }

    // ---- harness pieces ----------------------------------------------------

    const WG_PUBKEY: &str = "unit-relay-client-wg-pubkey-AQIDBAUGBwgJCgsMDQ4PEBESExQ=";

    fn unit_token(expires_at_unix: u64) -> AuthToken {
        token_with_payload(&expires_at_unix.to_string())
    }

    async fn wait_for(fuse: Duration, mut pred: impl FnMut() -> bool, what: &str) {
        match tokio::time::timeout(fuse, async {
            while !pred() {
                tokio::task::yield_now().await;
            }
        })
        .await
        {
            Ok(()) => {}
            Err(_) => panic!("condition not met within fuse: {what}"),
        }
    }

    fn started(
        urls: &[&str],
        token: AuthToken,
        dialer: &Arc<ScriptedDialer>,
        clock: &VirtualClock,
    ) -> RelayClient {
        let urls: Vec<String> = urls.iter().map(|s| s.to_string()).collect();
        let cfg = RelayClientConfig::new(&urls, WG_PUBKEY, token)
            .expect("unit urls parse")
            .with_clock(Arc::new(clock.clone()))
            .with_dialer(dialer.clone());
        RelayClient::start(cfg).expect("client starts")
    }

    // ---- illegal transitions / unauthenticated sends ------------------------

    // All unit tests use the CURRENT-THREAD runtime: the client's tasks
    // (worker/session/reader/scripted server) are fully cooperative and
    // `wait_for` yields to them explicitly — no worker-thread explosion
    // under full-suite parallel load, no scheduling starvation flake.
    #[tokio::test]
    async fn transport_before_ready_is_typed_rejected() {
        let clock = VirtualClock::new(1_770_000_000);
        let dialer = Arc::new(ScriptedDialer::new(PeerId::from_wg_pubkey_string(WG_PUBKEY)));
        // Every dial fails: the client never reaches Authenticating, let
        // alone Ready.
        dialer.plan("never.up", [DialPlan::Fail("scripted-fail")]);
        let client = started(&["rel://never.up:1"], unit_token(1_770_003_600), &dialer, &clock);
        wait_for(
            Duration::from_secs(5),
            || client.state() == RelayState::Reconnecting,
            "reconnecting after failed round",
        )
        .await;
        let err = client.send_to_peer(&fixture_peer(9), b"too early").unwrap_err();
        assert!(
            matches!(err, RelayClientError::NotReady { .. }),
            "unauthenticated send must be typed-rejected, got {err:?}"
        );
        let err = client.open_conn(&fixture_peer(9)).await.unwrap_err();
        assert!(matches!(err, RelayClientError::NotReady { .. }), "got {err:?}");
        // Nothing was ever sent anywhere.
        let stats = client.stats();
        assert_eq!(stats.frames_tx[MSG_TRANSPORT as usize], 0);
        assert_eq!(stats.transport_tx_bytes, 0);
        assert_eq!(stats.dial_attempts, 1);
        assert_eq!(dialer.called(), ["rel://never.up:1"]);
        client.stop();
    }

    #[tokio::test]
    async fn oversized_payload_rejected_before_any_wire_byte() {
        let clock = VirtualClock::new(1_770_000_000);
        let dialer = Arc::new(ScriptedDialer::new(PeerId::from_wg_pubkey_string(WG_PUBKEY)));
        dialer.plan("h", [DialPlan::Serve(ScriptMode::Silent)]);
        let client = started(&["rel://h:1"], unit_token(1_770_003_600), &dialer, &clock);
        wait_for(Duration::from_secs(5), || client.state() == RelayState::Ready, "ready").await;

        let oversized = vec![0u8; MAX_TRANSPORT_PAYLOAD + 1];
        let err = client.send_to_peer(&fixture_peer(2), &oversized).unwrap_err();
        assert_eq!(
            err,
            RelayClientError::FrameTooLarge { limit: MAX_MESSAGE_SIZE, got: MAX_MESSAGE_SIZE + 1 }
        );
        // Exactly at the ceiling (8820 − 38) is accepted.
        assert!(
            client.send_to_peer(&fixture_peer(2), &[0u8; MAX_TRANSPORT_PAYLOAD]).is_ok(),
            "boundary payload must pass"
        );
        client.stop();
    }

    // ---- full state machine over the scripted server ------------------------

    #[tokio::test]
    async fn state_transitions_scripted_reach_ready_then_keepalive_death() {
        let clock = VirtualClock::new(1_770_000_000);
        let dialer = Arc::new(ScriptedDialer::new(PeerId::from_wg_pubkey_string(WG_PUBKEY)));
        // Two scripted servers: the initial session AND the fast reconnect
        // after the 35s liveness death (same URL → same plan key).
        dialer.plan(
            "h",
            [DialPlan::Serve(ScriptMode::Silent), DialPlan::Serve(ScriptMode::Silent)],
        );
        let client = started(&["rel://h:1"], unit_token(1_770_003_600), &dialer, &clock);
        wait_for(Duration::from_secs(5), || client.state() == RelayState::Ready, "ready").await;

        let stats = client.stats();
        assert_eq!(
            stats.state_history,
            vec![
                RelayState::Disconnected,
                RelayState::Dialing,
                RelayState::Handshaking,
                RelayState::Authenticating,
                RelayState::Ready,
            ],
            "exact transition path with zero real network"
        );
        assert_eq!(stats.auth_successes, 1);
        assert_eq!(stats.current_url.as_deref(), Some("rel://h:1"));
        assert_eq!(stats.instance_url.as_deref(), Some("rels://scripted.relay.test:443"));
        assert_eq!(stats.frames_tx[MSG_AUTH as usize], 1);
        assert_eq!(stats.frames_rx[MSG_AUTH_RESPONSE as usize], 1);

        // 35s 判死: advance the INJECTED clock past the keepalive window —
        // the scripted server never sends anything, so this must kill the
        // session. No real second passes.
        clock.advance(Duration::from_secs(36));
        // The reconnect dial's counter bump trails the session-end record by
        // a scheduling step — wait for BOTH, never assert right after a wait.
        wait_for(
            Duration::from_secs(5),
            || client.stats().reconnects == 1 && client.stats().dial_attempts == 2,
            "reconnect with fast re-dial",
        )
        .await;
        let stats = client.stats();
        assert_eq!(stats.last_error_class, Some(RelayErrorClass::KeepaliveTimeout));
        // Fast reconnect to the same server engaged (no backoff, 2nd dial).
        assert!(stats.observed_backoff.is_empty(), "fast path has no backoff");
        assert_eq!(stats.dial_attempts, 2);
        wait_for(
            Duration::from_secs(5),
            || client.state() == RelayState::Ready,
            "ready again",
        )
        .await;
        let stats = client.stats();
        assert_eq!(
            stats.state_history,
            vec![
                RelayState::Disconnected,
                RelayState::Dialing,
                RelayState::Handshaking,
                RelayState::Authenticating,
                RelayState::Ready,
                RelayState::Reconnecting,
                RelayState::Dialing,
                RelayState::Handshaking,
                RelayState::Authenticating,
                RelayState::Ready,
            ],
        );
        client.stop();
        wait_for(Duration::from_secs(5), || client.state() == RelayState::Dead, "dead").await;
    }

    #[tokio::test]
    async fn auth_timeout_is_typed_and_retry_stays_on_the_backoff() {
        let clock = VirtualClock::new(1_770_000_000);
        let dialer = Arc::new(ScriptedDialer::new(PeerId::from_wg_pubkey_string(WG_PUBKEY)));
        // StallAuth: upgrade completes, Auth is consumed, NO answer → the
        // 8s AuthResponse budget must fire, on the injected clock. Three
        // plans: attempt #1 and #2 stall; #3 stays unused (stop arrives).
        dialer.plan(
            "stall",
            [
                DialPlan::Serve(ScriptMode::StallAuth),
                DialPlan::Serve(ScriptMode::StallAuth),
                DialPlan::Serve(ScriptMode::StallAuth),
            ],
        );
        let client = started(&["rel://stall:1"], unit_token(1_770_003_600), &dialer, &clock);
        wait_for(
            Duration::from_secs(5),
            || client.state() == RelayState::Authenticating,
            "authenticating",
        )
        .await;
        clock.advance(Duration::from_secs(8));
        wait_for(Duration::from_secs(5), || {
            client.stats().last_error_class == Some(RelayErrorClass::AuthTimeout)
        }, "auth budget fired typed")
        .await;
        let stats = client.stats();
        assert_eq!(stats.reconnects, 0, "a session that never went Ready is not a reconnect");
        assert_eq!(stats.auth_successes, 0);
        // The 2s backoff for the retry is recorded at round-arm time —
        // wait for the record itself, never assert right after a wait.
        wait_for(
            Duration::from_secs(5),
            || client.stats().observed_backoff == vec![Duration::from_secs(2)],
            "first backoff recorded",
        )
        .await;
        assert_eq!(client.stats().last_error_class, Some(RelayErrorClass::AuthTimeout));
        clock.advance(Duration::from_secs(2));
        wait_for(
            Duration::from_secs(5),
            || client.stats().dial_attempts == 2 && client.state() == RelayState::Authenticating,
            "second attempt reached authenticating",
        )
        .await;
        clock.advance(Duration::from_secs(8));
        wait_for(
            Duration::from_secs(5),
            || client.stats().observed_backoff == vec![Duration::from_secs(2), Duration::from_secs(4)],
            "second backoff recorded",
        )
        .await;
        clock.advance(Duration::from_secs(4));
        wait_for(Duration::from_secs(5), || client.stats().dial_attempts == 3, "third attempt")
            .await;
        assert_eq!(
            client.stats().observed_backoff,
            vec![Duration::from_secs(2), Duration::from_secs(4)],
            "exact exponential backoff, no retry addiction"
        );
        client.stop();
    }

    #[tokio::test]
    async fn token_expired_at_start_never_dials_then_recovers_on_update() {
        let clock = VirtualClock::new(1_770_000_000);
        let dialer = Arc::new(ScriptedDialer::new(PeerId::from_wg_pubkey_string(WG_PUBKEY)));
        dialer.plan("h", [DialPlan::Serve(ScriptMode::Silent)]);
        // Payload already in the past at virtual time zero.
        let client = started(&["rel://h:1"], unit_token(1_000_000_000), &dialer, &clock);
        wait_for(
            Duration::from_secs(5),
            || client.stats().last_error_class == Some(RelayErrorClass::TokenExpired),
            "fail-closed on expired token",
        )
        .await;
        let stats = client.stats();
        assert_eq!(stats.dial_attempts, 0, "NOTHING dialed with an expired token");
        assert!(dialer.called().is_empty(), "dialer never invoked");
        assert_eq!(stats.state, RelayState::Reconnecting);
        assert_eq!(
            client.token_validity(),
            TokenValidity::Expired { expired_for: Duration::from_secs(770_000_000) }
        );
        assert!(stats.token_expired_refusals >= 1);

        // Fresh token ⇒ the hold wakes instantly (watch channel) and the
        // client connects.
        client.update_token(unit_token(1_770_003_600));
        wait_for(
            Duration::from_secs(5),
            || client.state() == RelayState::Ready,
            "ready after update",
        )
        .await;
        match client.token_validity() {
            TokenValidity::Valid { remaining, .. } => {
                assert_eq!(remaining, Duration::from_secs(3_600))
            }
            other => panic!("want Valid, got {other:?}"),
        }
        assert_eq!(dialer.called(), ["rel://h:1"]);
        client.stop();
    }

    #[tokio::test]
    async fn token_expiry_mid_session_fails_closed_until_refresh() {
        let clock = VirtualClock::new(1_770_000_000);
        let dialer = Arc::new(ScriptedDialer::new(PeerId::from_wg_pubkey_string(WG_PUBKEY)));
        // Heartbeat servers keep liveness alive so ONLY the token deadline
        // can end the session — isolating the token path from keepalive.
        dialer.plan(
            "h",
            [
                DialPlan::Serve(ScriptMode::Heartbeat),
                DialPlan::Serve(ScriptMode::Heartbeat),
            ],
        );
        // Token lives 600 virtual seconds.
        let client = started(&["rel://h:1"], unit_token(1_770_000_600), &dialer, &clock);
        wait_for(Duration::from_secs(5), || client.state() == RelayState::Ready, "ready").await;
        // HealthChecks really are flowing (server-initiated, §5).
        wait_for(
            Duration::from_secs(5),
            || client.stats().frames_rx[MSG_HEALTH_CHECK as usize] >= 2,
            "heartbeats arriving",
        )
        .await;

        // 700s: past the 600s token expiry, session must die fail-closed.
        clock.advance(Duration::from_secs(700));
        wait_for(
            Duration::from_secs(5),
            || client.stats().last_error_class == Some(RelayErrorClass::TokenExpired),
            "mid-session token expiry kills the session",
        )
        .await;
        assert_eq!(client.state(), RelayState::Reconnecting);
        let frozen_attempts = client.stats().dial_attempts;
        let frozen_calls = dialer.called().len();
        assert_eq!(frozen_attempts, 1, "only the initial dial happened");

        // Hold: no dial while the token stays expired, even far past the
        // re-check ticks (the gate re-refuses every time). Elapsed virtual
        // = 1300s, expiry at 600s ⇒ expired_for = 700s.
        clock.advance(Duration::from_secs(600));
        wait_for(
            Duration::from_secs(5),
            || {
                client.token_validity()
                    == TokenValidity::Expired { expired_for: Duration::from_secs(700) }
            },
            "virtual unix clock moved to 700s past expiry",
        )
        .await;
        assert_eq!(client.stats().dial_attempts, frozen_attempts, "no dials while expired");
        assert_eq!(dialer.called().len(), frozen_calls);

        // Fresh token → dial resumes → Ready on the 2nd scripted server.
        client.update_token(unit_token(1_770_100_000));
        wait_for(
            Duration::from_secs(5),
            || client.state() == RelayState::Ready,
            "ready after refresh",
        )
        .await;
        assert_eq!(dialer.called().len(), frozen_calls + 1);
        client.stop();
    }

    #[tokio::test]
    async fn multi_url_concurrent_first_win_is_bounded() {
        let clock = VirtualClock::new(1_770_000_000);
        let dialer = Arc::new(ScriptedDialer::new(PeerId::from_wg_pubkey_string(WG_PUBKEY)));
        dialer.plan("dead.relay", [DialPlan::Fail("unreachable-by-script")]);
        dialer.plan("alive.relay", [DialPlan::Serve(ScriptMode::Silent)]);
        let client = started(
            &["rel://dead.relay:1", "rel://alive.relay:2"],
            unit_token(1_770_003_600),
            &dialer,
            &clock,
        );
        wait_for(
            Duration::from_secs(5),
            || {
                client.state() == RelayState::Ready
                    && client.stats().dial_attempts == 2
                    && dialer.called().len() == 2
            },
            "winner ready with both dials recorded",
        )
        .await;
        let stats = client.stats();
        assert_eq!(
            stats.current_url.as_deref(),
            Some("rel://alive.relay:2"),
            "first WINNER (stats: {stats:?})"
        );
        assert_eq!(stats.dial_attempts, 2, "both URLs dialed exactly once (≤7 cap)");
        assert_eq!(dialer.called().len(), 2, "dial order log: {:?}", dialer.called());
        assert!(
            stats.observed_backoff.is_empty(),
            "initial round needs no backoff (stats: {stats:?})"
        );
        client.stop();
    }

    #[tokio::test]
    async fn keepalive_boundary_is_exactly_35s() {
        let clock = VirtualClock::new(1_770_000_000);
        let dialer = Arc::new(ScriptedDialer::new(PeerId::from_wg_pubkey_string(WG_PUBKEY)));
        // Second plan: the fast reconnect after the death dials the same
        // URL again.
        dialer.plan("h", [DialPlan::Serve(ScriptMode::Silent), DialPlan::Serve(ScriptMode::Silent)]);
        let client = started(&["rel://h:1"], unit_token(1_770_003_600), &dialer, &clock);
        wait_for(Duration::from_secs(5), || client.state() == RelayState::Ready, "ready").await;
        clock.advance(Duration::from_secs(34));
        tokio::task::yield_now().await;
        assert_eq!(client.state(), RelayState::Ready, "34s < 35s: still alive");
        clock.advance(Duration::from_secs(1));
        wait_for(
            Duration::from_secs(5),
            || client.stats().last_error_class == Some(RelayErrorClass::KeepaliveTimeout),
            "died at the 35s boundary",
        )
        .await;
        client.stop();
    }

    // ---- sensitive discipline ----------------------------------------------

    #[test]
    fn debug_output_never_contains_token_material() {
        let token = unit_token(1_770_000_000);
        let urls = vec![RelayUrl::parse("rels://home.alfadb.cn:28443").unwrap()];
        let cfg = RelayClientConfig::new(
            &["rels://home.alfadb.cn:28443".to_string()],
            WG_PUBKEY,
            token.clone(),
        )
        .unwrap();
        assert_eq!(cfg.urls, urls);
        let (in_tx, in_rx) = mpsc::channel(1);
        let (epoch_tx, _epoch_rx) = watch::channel(0u64);
        let (stop_tx, _stop_rx) = watch::channel(false);
        let client = RelayClient {
            shared: Arc::new(Shared {
                cfg,
                stats: Mutex::new(RelayStats::default()),
                token: Mutex::new(Some(token)),
                token_epoch_tx: epoch_tx,
                stop_tx,
                session: Mutex::new(None),
                presence: Mutex::new(HashMap::new()),
                inbound: tokio::sync::Mutex::new(in_rx),
            }),
        };
        drop(in_tx);
        let err = RelayClientError::PeerOffline(fixture_peer(7));
        for text in [
            format!("{client:?}"),
            format!("{:?}", client.stats()),
            format!("{err:?}"),
            format!("{err}"),
            format!("{:?}", RelayErrorClass::TokenExpired),
        ] {
            assert!(!text.contains("paWl"), "signature b64 leaked: {text}");
            assert!(!text.contains("1770000000"), "payload leaked: {text}");
            assert!(!text.contains("a5a5"), "signature hex leaked: {text}");
        }
        // Config Debug redacts too.
        let cfg = RelayClientConfig::new(
            &["rels://home.alfadb.cn:28443".to_string()],
            WG_PUBKEY,
            unit_token(1_770_000_000),
        )
        .unwrap();
        let cfg_text = format!("{cfg:?}");
        assert!(!cfg_text.contains("paWl"), "{cfg_text}");
        assert!(!cfg_text.contains("1770000000"), "{cfg_text}");
    }

    // helper kept off the hot path: start() refuses empty URL lists via the
    // same typed error as a bad scheme (wired above with a tiny combinator).
    trait Pipe: Sized {
        fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
            f(self)
        }
    }
    impl<T> Pipe for T {}
}
