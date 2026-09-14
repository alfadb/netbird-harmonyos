// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # relay_testserver — in-process fake NetBird relay SERVER (test-only)
//!
//! Minimal server side of the relay protocol
//! (`docs/relay-client-spec-20260914.md`: §1 WS handshake, §2 framing, §3
//! Auth/AuthResponse, §4.1/§4.2 Subscribe/PeersOnline + Transport semantics,
//! §5 healthcheck, §8.1 minimal-server table). It lets
//! `tests/relay_e2e.rs` drive the REAL client stack
//! ([`crate::relay`] codec + [`crate::ws`] handshake/framing) over real
//! loopback TCP without any network, device or new dependency.
//!
//! ## Module visibility (why this is an unconditional `pub mod`)
//!
//! `#[cfg(test)]` modules are INVISIBLE to integration tests: when the lib is
//! compiled as a dependency of `tests/relay_e2e.rs`, `cfg(test)` is not set.
//! A `#![cfg(any(test, feature = …))]` gate would need a new feature, which
//! the frozen manifest forbids. So the module is registered unconditionally
//! in `lib.rs` and kept inert in the product path: nothing in `src/` calls
//! it, it adds no NAPI/`extern` exports (build.sh's `llvm-nm -D` surface is
//! unchanged), and it only reuses tokio features the cdylib already links.
//! Same pattern as the host-only `host_sockets` module.
//!
//! ## Server behavior implemented (spec-referenced)
//!
//! - WS handshake: validates `Upgrade: websocket`, `Connection: Upgrade`,
//!   `Sec-WebSocket-Version: 13`, captures `Sec-WebSocket-Key` and answers
//!   `101` with `Sec-WebSocket-Accept = base64(SHA1(key || GUID))`
//!   ([`crate::ws::accept_key`]). No extension is ever negotiated; a
//!   configurable [`HandshakeBehavior`] can deliberately answer non-101,
//!   omit `Upgrade`, or send `Sec-WebSocket-Extensions` to test the client's
//!   fail-closed paths. No Origin check (spec §1.2).
//! - WS framing (server side): server frames are NEVER masked; client frames
//!   MUST be masked (violation = protocol error, connection dropped);
//!   7/16/64-bit lengths with minimal-encoding checks; fragmentation
//!   reassembly; ping→pong; close→echo-close; only BINARY data messages are
//!   accepted as relay frames (spec §2.6).
//! - Auth (§3): first frame must be Auth (pre-auth anything else = silent
//!   close, like the upstream probe branch). Magic/peerID/token structure is
//!   validated by [`Frame::decode`] (algo byte, 32B signature, ASCII
//!   payload); the payload must additionally be ASCII digits (Unix seconds,
//!   §3.3). `reject_auth` config reproduces the upstream failure mode: NO
//!   response, direct close (§3.4). Success answers `AuthResponse` with the
//!   configured instance URL.
//! - Subscribe/PeersOnline (§4.1): online peers are answered immediately
//!   with `PeersOnline`; offline targets register interest and stay SILENT
//!   until the peer authenticates (blocking-wait semantics, no negative
//!   ack). Peers going offline push `PeersWentOffline` to remaining
//!   subscribers.
//! - Transport (§4.2): the 36B field is REWRITTEN to the sender's peer id
//!   before forwarding; offline destinations are silently dropped + counted
//!   (no error frame exists in the protocol).
//! - HealthCheck (§5): every received `01 05` is echoed byte-exact; an
//!   optional configurable interval makes the server initiate HealthChecks
//!   (mirrors the upstream 25s sender, shrunk for tests).
//! - Close (§4.2): relay `Close` (01 04) is echoed and the connection ends;
//!   a WS-level close is echoed per RFC 6455 §5.5.1.
//!
//! ## Observability and control plane
//!
//! [`RelayTestServer::stats`] snapshots received-frame counts per type,
//! forwarded byte counts, offline-drop counts, auth rejections and the seen
//! `Sec-WebSocket-Key`s (public per RFC — not secret material). The handle
//! exposes deterministic control: [`RelayTestServer::wait_connections`],
//! [`RelayTestServer::drop_connection`], [`RelayTestServer::close_connection`],
//! [`RelayTestServer::send_ws_binary_to`] (raw frame injection for oversized
//! frames) and [`RelayTestServer::shutdown`]. No sleeps anywhere: tests
//! synchronize via channels/`tokio::time::timeout` only.
//!
//! ## Sensitive discipline
//!
//! Test-only fabricated values; token bytes are never stored, logged or
//! counted beyond shape validation. Stats and errors carry shape/counter
//! data only.

use std::collections::HashMap;
use std::future::{poll_fn, Future};
use std::io;
use std::net::SocketAddr;
use std::pin::{pin, Pin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::time::{timeout, Instant};

use crate::relay::{
    Frame, PeerId, RelayError, MAX_MESSAGE_SIZE, MSG_AUTH, MSG_AUTH_RESPONSE, MSG_CLOSE,
    MSG_HEALTH_CHECK, MSG_PEERS_ONLINE, MSG_PEERS_WENT_OFFLINE, MSG_SUBSCRIBE_PEER_STATE,
    MSG_TRANSPORT, MSG_UNSUBSCRIBE_PEER_STATE, TOKEN_SIGNATURE_LEN,
};
use crate::ws::accept_key;

// ---------------------------------------------------------------------------
// public config / handle / stats
// ---------------------------------------------------------------------------

/// Instance URL answered in `AuthResponse` unless overridden (fabricated,
/// non-routable placeholder — spec §3.4 carries it verbatim).
pub const DEFAULT_INSTANCE_URL: &str = "rels://fake.relay.test:443";

/// Only WS path this server accepts (spec §1.1, `WebSocketURLPath`).
const RELAY_PATH: &str = "/relay";

/// Fixed token bytes before the ASCII payload: `[algo 1B][signature 32B]`.
const TOKEN_PAYLOAD_OFFSET: usize = 1 + TOKEN_SIGNATURE_LEN;

/// Cap on the inbound HTTP handshake head.
const HEADER_READ_LIMIT: usize = 16 * 1024;

/// How the fake server answers the WS upgrade (to exercise client failure
/// paths, spec §1.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandshakeBehavior {
    /// Spec-conformant `101` + correct `Sec-WebSocket-Accept`.
    Normal,
    /// Answer with this HTTP status instead of `101` (client must fail
    /// typed: `WsError::HandshakeStatus`).
    HttpStatus(u16),
    /// `101` with correct Accept but WITHOUT the `Upgrade` header.
    OmitUpgrade,
    /// `101` that illegally negotiates `permessage-deflate` (client must
    /// fail closed: `WsError::ExtensionNegotiated`).
    NegotiateExtension,
}

/// Fake-server configuration. All fields are plain test knobs.
#[derive(Clone, Debug)]
pub struct TestServerConfig {
    /// `AuthResponse` instance URL.
    pub instance_url: String,
    /// Reproduce upstream auth failure: close silently instead of answering
    /// (spec §3.4).
    pub reject_auth: bool,
    /// WS upgrade answer behavior.
    pub handshake: HandshakeBehavior,
    /// Server-initiated HealthCheck cadence (`None` = never initiates).
    /// Upstream sends every 25s (spec §5); tests shrink this.
    pub server_healthcheck_interval: Option<Duration>,
    /// Per-operation fuse (read/write). A safety net against hangs, never a
    /// synchronization device. Default 30s.
    pub op_timeout: Duration,
}

impl Default for TestServerConfig {
    fn default() -> Self {
        TestServerConfig {
            instance_url: DEFAULT_INSTANCE_URL.to_string(),
            reject_auth: false,
            handshake: HandshakeBehavior::Normal,
            server_healthcheck_interval: None,
            op_timeout: Duration::from_secs(30),
        }
    }
}

/// Point-in-time server counters (a cloned snapshot; cheap).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerStats {
    /// TCP connections accepted by the listener.
    pub connections_accepted: u64,
    /// Failed/never-upgraded WS handshakes (incl. deliberate misbehavior).
    pub handshake_failures: u64,
    /// `Sec-WebSocket-Key` values seen (RFC-public values, not secret).
    pub handshake_keys: Vec<String>,
    /// Received relay frames by message type (index = type byte, §2.2).
    pub frames_rx_by_type: [u64; 12],
    /// Received WS-legal but relay-illegal frames (unknown type / undecodable).
    pub unknown_frames_rx: u64,
    pub bad_frames_rx: u64,
    /// Non-Auth first frames (pre-auth data / probes) — connection closed.
    pub pre_auth_drops: u64,
    /// Auth attempts rejected (silent close, spec §3.4).
    pub auth_rejected: u64,
    /// Payload bytes forwarded to online destinations (§4.2).
    pub transport_bytes_forwarded: u64,
    /// Transport frames silently dropped because the destination was offline.
    pub transports_dropped_offline: u64,
    /// Server-INITIATED HealthChecks (excludes request echoes).
    pub server_healthchecks_sent: u64,
    /// WS protocol violations that ended a connection.
    pub ws_protocol_errors: u64,
    /// Underlying I/O failures that ended a connection.
    pub ws_io_errors: u64,
    /// Oversized WS messages rejected on receive.
    pub ws_oversized_rx: u64,
    /// Shape token of the LAST WS protocol violation (diagnostics only).
    pub last_protocol_error: Option<&'static str>,
}

impl ServerStats {
    /// Frames received of one message type (`0` for out-of-table types).
    pub fn frames_rx(&self, msg_type: u8) -> u64 {
        self.frames_rx_by_type.get(msg_type as usize).copied().unwrap_or(0)
    }
}

/// One live connection's senders into its writer task / read loop.
struct ConnEntry {
    /// Frames to write to this client (server frames are never masked).
    outbound: UnboundedSender<ConnMsg>,
    /// Control plane into the connection's read loop.
    ctrl: UnboundedSender<Ctrl>,
}

/// Server→client WS message (server frames are never masked, RFC 6455 §5.1).
enum ConnMsg {
    Ws { opcode: u8, payload: Vec<u8> },
    Abort,
}

/// Control commands delivered into a connection's read loop.
enum Ctrl {
    Abort,
    SendClose { code: u16, reason: String },
}

/// Cross-connection registry: who is online, who waits for whom.
#[derive(Default)]
struct Registry {
    next_conn_id: u64,
    conns: HashMap<u64, ConnEntry>,
    /// peerID → connection currently authenticated with it.
    peer_conn: HashMap<PeerId, u64>,
    /// connection → its authenticated peerID (reverse map for teardown).
    conn_peer: HashMap<u64, PeerId>,
    /// peerID → connections blocked in Subscribe-wait (§4.1: no negative
    /// ack; they are answered `PeersOnline` when the peer authenticates).
    interest: HashMap<PeerId, Vec<u64>>,
}

struct Shared {
    config: TestServerConfig,
    /// Arc'd so short-lived tasks (the HealthCheck ticker) can bump counters
    /// without keeping the whole `Shared` alive.
    stats: Arc<Mutex<ServerStats>>,
    registry: Mutex<Registry>,
    /// Notified once per accepted connection (`wait_connections`).
    conn_notify: Notify,
    shutdown: AtomicBool,
    shutdown_notify: Notify,
}

impl Shared {
    fn bump_stats(&self, f: impl FnOnce(&mut ServerStats)) {
        let mut stats = self.stats.lock().expect("stats lock");
        f(&mut stats);
    }
}

/// Handle to a running fake relay server.
pub struct RelayTestServer {
    addr: SocketAddr,
    shared: Arc<Shared>,
    accept_join: Mutex<Option<JoinHandle<()>>>,
}

impl RelayTestServer {
    /// Bind `127.0.0.1:0` (loopback, random port) and start accepting.
    pub async fn start(config: TestServerConfig) -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let shared = Arc::new(Shared {
            config,
            stats: Arc::new(Mutex::new(ServerStats::default())),
            registry: Mutex::new(Registry::default()),
            conn_notify: Notify::new(),
            shutdown: AtomicBool::new(false),
            shutdown_notify: Notify::new(),
        });
        let join = tokio::spawn(accept_loop(listener, shared.clone()));
        Ok(RelayTestServer { addr, shared, accept_join: Mutex::new(Some(join)) })
    }

    /// Bound loopback address (random port).
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Snapshot of all counters.
    pub fn stats(&self) -> ServerStats {
        self.shared.stats.lock().expect("stats lock").clone()
    }

    /// Resolve once at least `n` connections have been accepted. Deterministic
    /// (notification-driven) with `budget` as a fuse, never a sleep.
    pub async fn wait_connections(&self, n: usize, budget: Duration) -> Result<(), Duration> {
        let deadline = Instant::now() + budget;
        loop {
            if self.shared.stats.lock().expect("stats lock").connections_accepted >= n as u64 {
                return Ok(());
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(budget);
            }
            let _ = timeout(deadline - now, self.shared.conn_notify.notified()).await;
        }
    }

    /// Forcibly drop the TCP connection authenticated as `peer`
    /// (mid-session disconnect test path). `false` if no such peer.
    pub fn drop_connection(&self, peer: &PeerId) -> bool {
        let reg = self.shared.registry.lock().expect("registry lock");
        let Some(conn_id) = reg.peer_conn.get(peer).copied() else {
            return false;
        };
        let Some(entry) = reg.conns.get(&conn_id) else {
            return false;
        };
        let _ = entry.outbound.send(ConnMsg::Abort);
        let _ = entry.ctrl.send(Ctrl::Abort);
        true
    }

    /// Initiate a WS-level close toward `peer` with code+reason (the client
    /// must surface `WsMessage::Closed(Some((code, reason)))`). `false` if
    /// no such peer.
    pub fn close_connection(&self, peer: &PeerId, code: u16, reason: &str) -> bool {
        let reg = self.shared.registry.lock().expect("registry lock");
        let Some(conn_id) = reg.peer_conn.get(peer).copied() else {
            return false;
        };
        let Some(entry) = reg.conns.get(&conn_id) else {
            return false;
        };
        let _ = entry.ctrl.send(Ctrl::SendClose { code, reason: reason.to_string() });
        true
    }

    /// Push a raw WS binary message (unmasked server frame) to `peer` —
    /// wire-level injection for oversized/invalid frames. `false` if no such
    /// peer.
    pub fn send_ws_binary_to(&self, peer: &PeerId, payload: Vec<u8>) -> bool {
        let reg = self.shared.registry.lock().expect("registry lock");
        let Some(conn_id) = reg.peer_conn.get(peer).copied() else {
            return false;
        };
        let Some(entry) = reg.conns.get(&conn_id) else {
            return false;
        };
        let _ = entry.outbound.send(ConnMsg::Ws { opcode: OP_BINARY, payload });
        true
    }

    /// Stop accepting, end every live connection, and join the accept loop.
    pub async fn shutdown(self) {
        self.shared.shutdown.store(true, Ordering::Relaxed);
        // notify_waiters wakes pollers mid-wait; notify_one stores a permit
        // for a poller that has not registered yet.
        self.shared.shutdown_notify.notify_waiters();
        self.shared.shutdown_notify.notify_one();
        let entries: Vec<(UnboundedSender<ConnMsg>, UnboundedSender<Ctrl>)> = {
            let reg = self.shared.registry.lock().expect("registry lock");
            reg.conns.values().map(|e| (e.outbound.clone(), e.ctrl.clone())).collect()
        };
        for (outbound, ctrl) in entries {
            let _ = outbound.send(ConnMsg::Abort);
            let _ = ctrl.send(Ctrl::Abort);
        }
        let join = self.accept_join.lock().expect("join lock").take();
        if let Some(join) = join {
            // Fuse only: the loop exits on its flag at the latest.
            let _ = timeout(Duration::from_secs(2), join).await;
        }
    }
}

// ---------------------------------------------------------------------------
// accept loop + per-connection task
// ---------------------------------------------------------------------------

async fn accept_loop(listener: TcpListener, shared: Arc<Shared>) {
    loop {
        let mut accept_fut = pin!(listener.accept());
        let mut wake_fut = pin!(shared.shutdown_notify.notified());
        let accepted: Option<io::Result<(TcpStream, SocketAddr)>> = poll_fn(|cx| {
            if shared.shutdown.load(Ordering::Relaxed) {
                return Poll::Ready(None);
            }
            // Keep the shutdown wakeup registered while blocked in accept.
            let _ = wake_fut.as_mut().poll(cx);
            match accept_fut.as_mut().poll(cx) {
                Poll::Ready(res) => Poll::Ready(Some(res)),
                Poll::Pending => Poll::Pending,
            }
        })
        .await;
        let Some(Ok((stream, _peer))) = accepted else {
            break; // shutdown flagged or listener failed
        };
        shared.bump_stats(|s| s.connections_accepted += 1);
        shared.conn_notify.notify_one();
        let shared_conn = shared.clone();
        tokio::spawn(run_conn(stream, shared_conn));
    }
}

/// Per-connection state.
#[derive(Default)]
struct ConnState {
    peer: Option<PeerId>,
}

/// What the read loop should do next.
enum Flow {
    Message(Vec<u8>),
    Ping(Vec<u8>),
    WsClose(Option<u16>),
    Continue,
    Abort,
    Eof,
    Error(ErrorKind),
}

/// Which read failure ended the connection (drives the stats counters).
enum ErrorKind {
    Protocol(&'static str),
    Io,
    MessageTooLarge,
}

async fn run_conn(mut stream: TcpStream, shared: Arc<Shared>) {
    let conn_id = {
        let mut reg = shared.registry.lock().expect("registry lock");
        let id = reg.next_conn_id;
        reg.next_conn_id += 1;
        id
    };

    // -- WS handshake (server side, spec §1.2/§1.3/§8.1) ---------------------
    let head = match timeout(shared.config.op_timeout, read_http_head(&mut stream)).await {
        Ok(Ok(head)) => head,
        _ => {
            shared.bump_stats(|s| s.handshake_failures += 1);
            return;
        }
    };
    let request = parse_handshake_request(&head);
    if let Some(key) = request.as_ref().and_then(|r| r.key.clone()) {
        // The observed key is always recorded (RFC-public value).
        shared.bump_stats(|s| s.handshake_keys.push(key));
    }
    let (response_bytes, upgraded) = handshake_reply(request.as_ref(), &shared.config);
    if !upgraded {
        // Bump BEFORE the client can observe the failure response (the test
        // asserts this counter right after its handshake fails).
        shared.bump_stats(|s| s.handshake_failures += 1);
    }
    if write_all(&mut stream, &response_bytes).await.is_err() {
        return;
    }
    if !upgraded {
        return;
    }

    // -- split + channels -----------------------------------------------------
    let (mut rd, wr) = stream.into_split();
    let (out_tx, out_rx) = mpsc::unbounded_channel::<ConnMsg>();
    let (ctrl_tx, mut ctrl_rx) = mpsc::unbounded_channel::<Ctrl>();
    {
        let mut reg = shared.registry.lock().expect("registry lock");
        reg.conns.insert(
            conn_id,
            ConnEntry { outbound: out_tx.clone(), ctrl: ctrl_tx.clone() },
        );
    }
    let writer = tokio::spawn(writer_task(wr, out_rx, shared.config.op_timeout));

    // -- read loop (hand-rolled select: ctrl + WS message) --------------------
    let mut ws = WsServerRead::default();
    let mut conn = ConnState::default();
    loop {
        let flow = poll_fn(|cx| {
            match ctrl_rx.poll_recv(cx) {
                Poll::Ready(Some(Ctrl::Abort)) | Poll::Ready(None) => {
                    return Poll::Ready(Flow::Abort);
                }
                Poll::Ready(Some(Ctrl::SendClose { code, reason })) => {
                    // Queue the close; the connection ends right after (the
                    // client's RFC-mandated close echo is not awaited).
                    let mut payload = code.to_be_bytes().to_vec();
                    payload.extend_from_slice(reason.as_bytes());
                    let _ = out_tx.send(ConnMsg::Ws { opcode: OP_CLOSE, payload });
                    return Poll::Ready(Flow::Abort);
                }
                Poll::Pending => {}
            }
            match ws.poll_message(&mut rd, cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Ok(ServerWsEvent::Binary(bytes))) => Poll::Ready(Flow::Message(bytes)),
                Poll::Ready(Ok(ServerWsEvent::Ping(payload))) => Poll::Ready(Flow::Ping(payload)),
                Poll::Ready(Ok(ServerWsEvent::Pong)) => Poll::Ready(Flow::Continue),
                Poll::Ready(Ok(ServerWsEvent::Close(info))) => {
                    Poll::Ready(Flow::WsClose(info.map(|i| i.0)))
                }
                Poll::Ready(Err(WsReadError::Eof)) => Poll::Ready(Flow::Eof),
                Poll::Ready(Err(WsReadError::Io)) => Poll::Ready(Flow::Error(ErrorKind::Io)),
                Poll::Ready(Err(WsReadError::Protocol(token))) => {
                    Poll::Ready(Flow::Error(ErrorKind::Protocol(token)))
                }
                Poll::Ready(Err(WsReadError::MessageTooLarge)) => {
                    Poll::Ready(Flow::Error(ErrorKind::MessageTooLarge))
                }
            }
        });
        match timeout(shared.config.op_timeout, flow).await {
            Err(_elapsed) => break, // read fuse: treat as dead peer
            Ok(Flow::Message(bytes)) => {
                if handle_relay_frame(&shared, conn_id, &out_tx, &mut conn, &bytes) {
                    break;
                }
            }
            Ok(Flow::Ping(payload)) => {
                // RFC 6455 §5.5.3: answer ping with an echoing pong.
                let _ = out_tx.send(ConnMsg::Ws { opcode: OP_PONG, payload });
            }
            Ok(Flow::Continue) => {}
            Ok(Flow::WsClose(code)) => {
                // RFC 6455 §5.5.1: answer close with close (echo the code,
                // drop the reason), then end the connection.
                let payload = code.map(|c| c.to_be_bytes().to_vec()).unwrap_or_default();
                let _ = out_tx.send(ConnMsg::Ws { opcode: OP_CLOSE, payload });
                break;
            }
            Ok(Flow::Abort) | Ok(Flow::Eof) => break,
            Ok(Flow::Error(kind)) => {
                shared.bump_stats(|s| match kind {
                    ErrorKind::Protocol(token) => {
                        s.ws_protocol_errors += 1;
                        s.last_protocol_error = Some(token);
                    }
                    ErrorKind::Io => s.ws_io_errors += 1,
                    ErrorKind::MessageTooLarge => s.ws_oversized_rx += 1,
                });
                break;
            }
        }
    }

    // -- teardown: flush queued frames, then end the TCP stream ---------------
    let _ = out_tx.send(ConnMsg::Abort);
    drop(out_tx);
    drop(ctrl_tx);
    unregister_conn(&shared, conn_id);
    let _ = writer.await;
}

/// Remove the connection from the registry; authenticated peers go offline
/// and remaining subscribers get `PeersWentOffline` (spec §4.2).
fn unregister_conn(shared: &Shared, conn_id: u64) {
    let mut reg = shared.registry.lock().expect("registry lock");
    reg.conns.remove(&conn_id);
    let went_offline = match reg.conn_peer.remove(&conn_id) {
        Some(peer) => {
            if reg.peer_conn.get(&peer) == Some(&conn_id) {
                reg.peer_conn.remove(&peer);
            }
            Some(peer)
        }
        None => None,
    };
    let Some(peer) = went_offline else {
        return;
    };
    let Some(waiters) = reg.interest.remove(&peer) else {
        return;
    };
    let payload = Frame::PeersWentOffline { peer_ids: vec![peer] }.encode().ok();
    for waiter in waiters {
        if let (Some(entry), Some(bytes)) = (reg.conns.get(&waiter), payload.as_ref()) {
            let _ = entry.outbound.send(ConnMsg::Ws { opcode: OP_BINARY, payload: bytes.clone() });
        }
    }
}

/// Dispatch one decoded relay frame. Returns `true` when the connection must
/// end (upstream semantics per frame type, see module docs).
fn handle_relay_frame(
    shared: &Shared,
    conn_id: u64,
    out_tx: &UnboundedSender<ConnMsg>,
    conn: &mut ConnState,
    bytes: &[u8],
) -> bool {
    let frame = match Frame::decode(bytes) {
        Ok(frame) => frame,
        Err(RelayError::UnknownType(_)) => {
            // upstream server disconnects on unknown types (peer.go:104-108)
            shared.bump_stats(|s| s.unknown_frames_rx += 1);
            return true;
        }
        Err(_) => {
            shared.bump_stats(|s| s.bad_frames_rx += 1);
            return true;
        }
    };
    if !matches!(frame, Frame::Auth { .. }) && conn.peer.is_none() {
        // Auth must be the FIRST frame (relay/server/handshake.go); the
        // upstream probe branch silently closes everything else pre-auth.
        shared.bump_stats(|s| s.pre_auth_drops += 1);
        return true;
    }
    match frame {
        Frame::Auth { peer_id, token } => {
            shared.bump_stats(|s| s.frames_rx_by_type[MSG_AUTH as usize] += 1);
            if shared.config.reject_auth {
                shared.bump_stats(|s| s.auth_rejected += 1);
                return true; // §3.4: NO response — silent close
            }
            // Frame::decode validated the token structure (algo byte, 32B
            // raw signature, non-empty ASCII payload). Upstream additionally
            // treats the payload as a Unix-seconds string (§3.3), so require
            // ASCII digits — anything else fails verification.
            let token_bytes = token.to_bytes();
            if !token_bytes[TOKEN_PAYLOAD_OFFSET..].iter().all(u8::is_ascii_digit) {
                shared.bump_stats(|s| s.auth_rejected += 1);
                return true;
            }
            let waiters = {
                let mut reg = shared.registry.lock().expect("registry lock");
                reg.peer_conn.insert(peer_id.clone(), conn_id);
                reg.conn_peer.insert(conn_id, peer_id.clone());
                reg.interest.remove(&peer_id).unwrap_or_default()
            };
            conn.peer = Some(peer_id.clone());
            // §5: server-initiated HealthCheck sender (configurable cadence;
            // upstream sends every 25s). Exits when the connection's writer
            // goes away (send fails → break).
            if let Some(interval) = shared.config.server_healthcheck_interval {
                let tx = out_tx.clone();
                let stats = shared.stats.clone();
                tokio::spawn(async move {
                    let mut tick = tokio::time::interval(interval);
                    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    loop {
                        tick.tick().await;
                        stats.lock().expect("stats lock").server_healthchecks_sent += 1;
                        match Frame::HealthCheck.encode() {
                            Ok(payload) => {
                                if tx.send(ConnMsg::Ws { opcode: OP_BINARY, payload }).is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
            let reply = Frame::AuthResponse { instance_url: shared.config.instance_url.clone() };
            match reply.encode() {
                Ok(payload) => {
                    let _ = out_tx.send(ConnMsg::Ws { opcode: OP_BINARY, payload });
                }
                Err(_) => return true,
            }
            // §4.1: subscribers blocked on this peer get their PeersOnline now.
            for waiter in waiters {
                let target = {
                    let reg = shared.registry.lock().expect("registry lock");
                    reg.conns.get(&waiter).map(|e| e.outbound.clone())
                };
                if let Some(tx) = target {
                    if let Ok(payload) =
                        (Frame::PeersOnline { peer_ids: vec![peer_id.clone()] }).encode()
                    {
                        let _ = tx.send(ConnMsg::Ws { opcode: OP_BINARY, payload });
                    }
                }
            }
            false
        }
        Frame::AuthResponse { .. } => {
            // server does not receive this (§8.1 minimal table: ignore)
            shared.bump_stats(|s| s.frames_rx_by_type[MSG_AUTH_RESPONSE as usize] += 1);
            false
        }
        Frame::Transport { peer_id: dst, payload } => {
            shared.bump_stats(|s| s.frames_rx_by_type[MSG_TRANSPORT as usize] += 1);
            let sender = match &conn.peer {
                Some(peer) => peer.clone(),
                None => return true, // unreachable behind the pre-auth gate
            };
            let target = {
                let reg = shared.registry.lock().expect("registry lock");
                reg.peer_conn.get(&dst).and_then(|id| reg.conns.get(id)).map(|e| e.outbound.clone())
            };
            match target {
                Some(tx) => {
                    // §4.2: the 36B field is REWRITTEN to the SENDER id.
                    let forwarded = Frame::Transport { peer_id: sender, payload: payload.clone() };
                    match forwarded.encode() {
                        Ok(wire) => {
                            let _ = tx.send(ConnMsg::Ws { opcode: OP_BINARY, payload: wire });
                        }
                        Err(_) => {
                            // would exceed the frame ceiling: drop silently
                            shared.bump_stats(|s| s.transports_dropped_offline += 1);
                            return false;
                        }
                    }
                    shared.bump_stats(|s| s.transport_bytes_forwarded += payload.len() as u64);
                }
                None => {
                    // §4.2: offline destination — silent drop, no error frame
                    shared.bump_stats(|s| s.transports_dropped_offline += 1);
                }
            }
            false
        }
        Frame::HealthCheck => {
            shared.bump_stats(|s| s.frames_rx_by_type[MSG_HEALTH_CHECK as usize] += 1);
            match Frame::HealthCheck.encode() {
                Ok(payload) => {
                    let _ = out_tx.send(ConnMsg::Ws { opcode: OP_BINARY, payload });
                }
                Err(_) => return true,
            }
            false
        }
        Frame::Close => {
            shared.bump_stats(|s| s.frames_rx_by_type[MSG_CLOSE as usize] += 1);
            if let Ok(payload) = Frame::Close.encode() {
                let _ = out_tx.send(ConnMsg::Ws { opcode: OP_BINARY, payload });
            }
            true // §4.2: reply Close and end the connection
        }
        Frame::SubscribePeerState { peer_ids } => {
            shared.bump_stats(|s| s.frames_rx_by_type[MSG_SUBSCRIBE_PEER_STATE as usize] += 1);
            let mut online = Vec::new();
            {
                let mut reg = shared.registry.lock().expect("registry lock");
                for id in &peer_ids {
                    if reg.peer_conn.contains_key(id) {
                        online.push(id.clone());
                    } else {
                        // §4.1: offline target = register interest, STAY SILENT
                        reg.interest.entry(id.clone()).or_default().push(conn_id);
                    }
                }
            }
            if !online.is_empty() {
                match (Frame::PeersOnline { peer_ids: online }).encode() {
                    Ok(payload) => {
                        let _ = out_tx.send(ConnMsg::Ws { opcode: OP_BINARY, payload });
                    }
                    Err(_) => return true,
                }
            }
            false
        }
        Frame::UnsubscribePeerState { peer_ids } => {
            shared
                .bump_stats(|s| s.frames_rx_by_type[MSG_UNSUBSCRIBE_PEER_STATE as usize] += 1);
            let mut reg = shared.registry.lock().expect("registry lock");
            for id in &peer_ids {
                let remove = match reg.interest.get_mut(id) {
                    Some(waiters) => {
                        waiters.retain(|c| *c != conn_id);
                        waiters.is_empty()
                    }
                    None => false,
                };
                if remove {
                    reg.interest.remove(id);
                }
            }
            false
        }
        Frame::PeersOnline { .. } => {
            shared.bump_stats(|s| s.frames_rx_by_type[MSG_PEERS_ONLINE as usize] += 1);
            false // server→client type; ignore on receive
        }
        Frame::PeersWentOffline { .. } => {
            shared.bump_stats(|s| s.frames_rx_by_type[MSG_PEERS_WENT_OFFLINE as usize] += 1);
            false
        }
    }
}

/// Writer task: owns the write half; drains queued frames in order, so a
/// response queued before `Abort` still reaches the wire before the FIN.
async fn writer_task(
    mut wr: OwnedWriteHalf,
    mut rx: UnboundedReceiver<ConnMsg>,
    write_timeout: Duration,
) {
    while let Some(msg) = rx.recv().await {
        match msg {
            ConnMsg::Ws { opcode, payload } => {
                let write = async {
                    let frame = encode_server_frame(opcode, &payload);
                    write_all(&mut wr, &frame).await?;
                    flush_io(&mut wr).await
                };
                if timeout(write_timeout, write).await.is_err() {
                    break;
                }
            }
            ConnMsg::Abort => break,
        }
    }
    let _ =
        poll_fn(|cx| Pin::new(&mut wr).poll_shutdown(cx)).await; // FIN after buffered data
}

// ---------------------------------------------------------------------------
// HTTP upgrade request/response (server side)
// ---------------------------------------------------------------------------

struct HandshakeRequest {
    path: String,
    upgrade: Option<String>,
    connection: Option<String>,
    key: Option<String>,
    version: Option<String>,
}

fn parse_handshake_request(head: &str) -> Option<HandshakeRequest> {
    let mut lines = head.split("\r\n");
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    if parts.next()? != "GET" {
        return None;
    }
    let mut req = HandshakeRequest {
        path: parts.next()?.to_string(),
        upgrade: None,
        connection: None,
        key: None,
        version: None,
    };
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = line.split_once(':')?;
        match name.trim().to_ascii_lowercase().as_str() {
            "upgrade" => req.upgrade = Some(value.trim().to_string()),
            "connection" => req.connection = Some(value.trim().to_string()),
            "sec-websocket-key" => req.key = Some(value.trim().to_string()),
            "sec-websocket-version" => req.version = Some(value.trim().to_string()),
            _ => {}
        }
    }
    Some(req)
}

fn request_is_valid(req: &HandshakeRequest) -> bool {
    req.path == RELAY_PATH
        && req
            .upgrade
            .as_deref()
            .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
        && req
            .connection
            .as_deref()
            .is_some_and(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case("upgrade")))
        && req.version.as_deref() == Some("13")
        && req.key.as_deref().is_some_and(|k| !k.is_empty())
}

/// Build the upgrade answer. Returns `(wire bytes, upgrade granted)`.
fn handshake_reply(req: Option<&HandshakeRequest>, config: &TestServerConfig) -> (Vec<u8>, bool) {
    let valid = req.is_some_and(request_is_valid);
    if !valid {
        return (
            b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
            false,
        );
    }
    let accept = accept_key(req.and_then(|r| r.key.as_deref()).unwrap_or(""));
    let base = format!(
        "HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n"
    );
    match &config.handshake {
        HandshakeBehavior::Normal => {
            (format!("{base}Upgrade: websocket\r\n\r\n").into_bytes(), true)
        }
        HandshakeBehavior::HttpStatus(code) => (
            format!("HTTP/1.1 {code} Denied\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .into_bytes(),
            false,
        ),
        HandshakeBehavior::OmitUpgrade => (format!("{base}\r\n").into_bytes(), false),
        HandshakeBehavior::NegotiateExtension => (
            format!(
                "{base}Upgrade: websocket\r\nSec-WebSocket-Extensions: permessage-deflate\r\n\r\n"
            )
            .into_bytes(),
            false,
        ),
    }
}

async fn read_http_head<S: AsyncRead + Unpin>(io: &mut S) -> io::Result<String> {
    let mut buf: Vec<u8> = Vec::with_capacity(512);
    loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            return String::from_utf8(buf[..pos].to_vec())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "non-utf8 handshake"));
        }
        if buf.len() > HEADER_READ_LIMIT {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "handshake head too large"));
        }
        let start = buf.len();
        buf.resize(start + 512, 0);
        let n = read_some(io, &mut buf[start..]).await?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "eof in handshake"));
        }
        buf.truncate(start + n);
    }
}

// ---------------------------------------------------------------------------
// server-side WS framing (test-only reimplementation: ws.rs is client-side
// and its helpers are private)
// ---------------------------------------------------------------------------

const OP_CONT: u8 = 0x0;
const OP_TEXT: u8 = 0x1;
const OP_BINARY: u8 = 0x2;
const OP_CLOSE: u8 = 0x8;
const OP_PING: u8 = 0x9;
const OP_PONG: u8 = 0xA;

/// RFC 6455 §5.5 control-frame payload cap.
const MAX_CONTROL_PAYLOAD: usize = 125;

/// One parsed server-side frame header. `mask` is ALWAYS present: client
/// frames must be masked (RFC 6455 §5.1) — unmasked is a protocol violation.
#[derive(Debug)]
struct ServerFrameHeader {
    fin: bool,
    opcode: u8,
    mask: [u8; 4],
    payload_len: u64,
    header_len: usize,
}

#[derive(Debug)]
enum ServerFrameParse {
    NeedMore,
    Header(ServerFrameHeader),
}

/// Errors of the server-side WS reader. Shape tokens only, never content;
/// payloads are exactly what the connection loop consumes (into stats).
#[derive(Debug)]
enum WsReadError {
    /// Peer closed the stream (clean EOF).
    Eof,
    /// Underlying socket I/O failure.
    Io,
    /// Wire-protocol violation; carries the stable shape token (recorded in
    /// [`ServerStats::last_protocol_error`]).
    Protocol(&'static str),
    /// Frame/message over the 8820 ceiling.
    MessageTooLarge,
}

/// Events surfaced by the server-side WS reader.
enum ServerWsEvent {
    /// Complete binary data message (the only relay-legal data message).
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong,
    Close(Option<(u16, String)>),
}

/// Buffered WS message reader for one server-side connection. Pure poll
/// state machine: cancellation-safe across `poll_fn` re-creations (the
/// buffer and fragmentation state live here, not in any future).
#[derive(Default)]
struct WsServerRead {
    buf: Vec<u8>,
    frag: Option<(u8, Vec<u8>)>,
}

impl WsServerRead {
    fn poll_message<S: AsyncRead + Unpin>(
        &mut self,
        io: &mut S,
        cx: &mut Context<'_>,
    ) -> Poll<Result<ServerWsEvent, WsReadError>> {
        loop {
            let header = match parse_server_frame(&self.buf)? {
                ServerFrameParse::NeedMore => {
                    match poll_fill(io, cx, &mut self.buf) {
                        Poll::Ready(Ok(_)) => {}
                        Poll::Ready(Err(e)) => {
                            return Poll::Ready(Err(
                                if e.kind() == io::ErrorKind::UnexpectedEof {
                                    WsReadError::Eof
                                } else {
                                    WsReadError::Io
                                },
                            ));
                        }
                        Poll::Pending => return Poll::Pending,
                    }
                    continue;
                }
                ServerFrameParse::Header(h) => h,
            };
            if header.payload_len > MAX_MESSAGE_SIZE as u64 {
                return Poll::Ready(Err(WsReadError::MessageTooLarge));
            }
            let total = header.header_len + header.payload_len as usize;
            if self.buf.len() < total {
                match poll_fill(io, cx, &mut self.buf) {
                    Poll::Ready(Ok(_)) => {}
                    Poll::Ready(Err(e)) => {
                        return Poll::Ready(Err(
                            if e.kind() == io::ErrorKind::UnexpectedEof {
                                WsReadError::Eof
                            } else {
                                WsReadError::Io
                            },
                        ));
                    }
                    Poll::Pending => return Poll::Pending,
                }
                continue;
            }
            let mut payload = self.buf[header.header_len..total].to_vec();
            self.buf.drain(..total);
            apply_mask(&mut payload, header.mask);
            match header.opcode {
                OP_CLOSE => {
                    return Poll::Ready(Ok(ServerWsEvent::Close(parse_close_payload(&payload)?)));
                }
                OP_PING => return Poll::Ready(Ok(ServerWsEvent::Ping(payload))),
                OP_PONG => return Poll::Ready(Ok(ServerWsEvent::Pong)),
                OP_CONT => {
                    let Some((_, acc)) = self.frag.as_mut() else {
                        return Poll::Ready(Err(WsReadError::Protocol("unexpected-continuation")));
                    };
                    if acc.len() + payload.len() > MAX_MESSAGE_SIZE {
                        return Poll::Ready(Err(WsReadError::MessageTooLarge));
                    }
                    acc.extend_from_slice(&payload);
                    if header.fin {
                        let (opcode, data) = self.frag.take().expect("checked above");
                        return Poll::Ready(finish_data(opcode, data));
                    }
                }
                _ => {
                    // OP_TEXT | OP_BINARY
                    if self.frag.is_some() {
                        return Poll::Ready(Err(WsReadError::Protocol(
                            "data-during-fragmentation",
                        )));
                    }
                    if header.fin {
                        return Poll::Ready(finish_data(header.opcode, payload));
                    }
                    self.frag = Some((header.opcode, payload));
                }
            }
        }
    }
}

fn finish_data(opcode: u8, data: Vec<u8>) -> Result<ServerWsEvent, WsReadError> {
    match opcode {
        OP_BINARY => Ok(ServerWsEvent::Binary(data)),
        // §2.6: a relay frame travels as one BINARY WS message — text is a
        // protocol violation on this socket.
        OP_TEXT => Err(WsReadError::Protocol("text-message")),
        _ => Err(WsReadError::Protocol("unknown-opcode")),
    }
}

fn parse_server_frame(buf: &[u8]) -> Result<ServerFrameParse, WsReadError> {
    if buf.len() < 2 {
        return Ok(ServerFrameParse::NeedMore);
    }
    let b0 = buf[0];
    let fin = b0 & 0x80 != 0;
    let opcode = b0 & 0x0F;
    if b0 & 0x70 != 0 {
        return Err(WsReadError::Protocol("rsv-set"));
    }
    match opcode {
        OP_CONT | OP_TEXT | OP_BINARY | OP_CLOSE | OP_PING | OP_PONG => {}
        _ => return Err(WsReadError::Protocol("unknown-opcode")),
    }
    let is_control = opcode >= 0x8;
    let b1 = buf[1];
    if b1 & 0x80 == 0 {
        // RFC 6455 §5.1: client→server frames MUST be masked.
        return Err(WsReadError::Protocol("client-frame-unmasked"));
    }
    let len7 = (b1 & 0x7F) as u64;
    if is_control {
        if !fin {
            return Err(WsReadError::Protocol("fragmented-control"));
        }
        if len7 > MAX_CONTROL_PAYLOAD as u64 {
            return Err(WsReadError::Protocol("control-too-long"));
        }
    }
    let (payload_len, mut header_len) = if len7 < 126 {
        (len7, 2usize)
    } else if len7 == 126 {
        if buf.len() < 4 {
            return Ok(ServerFrameParse::NeedMore);
        }
        let n = u16::from_be_bytes([buf[2], buf[3]]) as u64;
        if n < 126 {
            return Err(WsReadError::Protocol("non-minimal-length"));
        }
        (n, 4)
    } else {
        if buf.len() < 10 {
            return Ok(ServerFrameParse::NeedMore);
        }
        let n = u64::from_be_bytes(buf[2..10].try_into().expect("length checked above"));
        if n & 0x8000_0000_0000_0000 != 0 {
            return Err(WsReadError::Protocol("length-top-bit"));
        }
        if n < 65536 {
            return Err(WsReadError::Protocol("non-minimal-length"));
        }
        (n, 10)
    };
    header_len += 4; // masking key (client frames are always masked)
    if buf.len() < header_len {
        return Ok(ServerFrameParse::NeedMore);
    }
    let mut mask = [0u8; 4];
    mask.copy_from_slice(&buf[header_len - 4..header_len]);
    Ok(ServerFrameParse::Header(ServerFrameHeader {
        fin,
        opcode,
        mask,
        payload_len,
        header_len,
    }))
}

/// RFC 6455 §5.3 masking in place (XOR is its own inverse).
fn apply_mask(payload: &mut [u8], mask: [u8; 4]) {
    for (i, b) in payload.iter_mut().enumerate() {
        *b ^= mask[i & 3];
    }
}

/// Server→client frame: FIN set, minimal length encoding, NEVER masked.
fn encode_server_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let len = payload.len();
    let mut frame = Vec::with_capacity(len + 10);
    frame.push(0x80 | opcode);
    if len < 126 {
        frame.push(len as u8);
    } else if len <= 0xFFFF {
        frame.push(126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }
    frame.extend_from_slice(payload);
    frame
}

fn valid_close_code(code: u16) -> bool {
    matches!(code, 1000..=1003 | 1007..=1014 | 3000..=4999)
}

fn parse_close_payload(payload: &[u8]) -> Result<Option<(u16, String)>, WsReadError> {
    match payload.len() {
        0 => Ok(None),
        1 => Err(WsReadError::Protocol("close-payload-1")),
        _ => {
            let code = u16::from_be_bytes([payload[0], payload[1]]);
            if !valid_close_code(code) {
                return Err(WsReadError::Protocol("close-code-invalid"));
            }
            let reason = std::str::from_utf8(&payload[2..])
                .map_err(|_| WsReadError::Protocol("close-reason-utf8"))?;
            Ok(Some((code, reason.to_string())))
        }
    }
}

// ---------------------------------------------------------------------------
// stream helpers (tokio core traits only — mirrors ws.rs, no io-util feature)
// ---------------------------------------------------------------------------

async fn read_some<S: AsyncRead + Unpin>(io: &mut S, buf: &mut [u8]) -> io::Result<usize> {
    std::future::poll_fn(|cx| {
        let mut rb = ReadBuf::new(buf);
        match Pin::new(&mut *io).poll_read(cx, &mut rb) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(rb.filled().len())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    })
    .await
}

fn poll_fill<S: AsyncRead + Unpin>(
    io: &mut S,
    cx: &mut Context<'_>,
    buf: &mut Vec<u8>,
) -> Poll<io::Result<usize>> {
    const CHUNK: usize = 4096;
    let start = buf.len();
    buf.resize(start + CHUNK, 0);
    let mut rb = ReadBuf::new(&mut buf[start..]);
    match Pin::new(io).poll_read(cx, &mut rb) {
        Poll::Ready(Ok(())) => {
            let n = rb.filled().len();
            buf.truncate(start + n);
            if n == 0 {
                Poll::Ready(Err(io::Error::new(io::ErrorKind::UnexpectedEof, "eof")))
            } else {
                Poll::Ready(Ok(n))
            }
        }
        Poll::Pending => {
            buf.truncate(start);
            Poll::Pending
        }
        Poll::Ready(Err(e)) => {
            buf.truncate(start);
            Poll::Ready(Err(e))
        }
    }
}

async fn write_all<S: AsyncWrite + Unpin>(io: &mut S, mut data: &[u8]) -> io::Result<()> {
    while !data.is_empty() {
        let n = std::future::poll_fn(|cx| Pin::new(&mut *io).poll_write(cx, data)).await?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "write made no progress"));
        }
        data = &data[n..];
    }
    Ok(())
}

async fn flush_io<S: AsyncWrite + Unpin>(io: &mut S) -> io::Result<()> {
    std::future::poll_fn(|cx| Pin::new(&mut *io).poll_flush(cx)).await
}

// ---------------------------------------------------------------------------
// server-side protocol unit tests (no sockets: pure framing/decision paths)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_frames_are_never_masked_and_minimally_encoded() {
        let small = encode_server_frame(OP_BINARY, &[1, 2, 3]);
        assert_eq!(small, vec![0x82, 0x03, 1, 2, 3]);
        let mid = encode_server_frame(OP_BINARY, &vec![0u8; 300]);
        assert_eq!(&mid[..4], &[0x82, 126, 0x01, 0x2C], "16-bit length branch");
        let close = encode_server_frame(OP_CLOSE, &[0x03, 0xE8]);
        assert_eq!(close, vec![0x88, 0x02, 0x03, 0xE8]);
    }

    #[test]
    fn client_frames_must_be_masked_on_server_receive() {
        // Unmasked client frame = protocol violation (RFC 6455 §5.1).
        assert!(matches!(
            parse_server_frame(&[0x82, 0x01, 0x07]),
            Err(WsReadError::Protocol("client-frame-unmasked"))
        ));
        // Masked frame parses and carries the mask.
        match parse_server_frame(&[0x82, 0x81, 1, 2, 3, 4, 0x07]) {
            Ok(ServerFrameParse::Header(h)) => {
                assert_eq!(h.mask, [1, 2, 3, 4]);
                assert_eq!(h.payload_len, 1);
                assert_eq!(h.header_len, 6);
            }
            other => panic!("expected header, got {other:?}"),
        }
    }

    #[test]
    fn handshake_reply_covers_all_behaviors() {
        let req = HandshakeRequest {
            path: "/relay".to_string(),
            upgrade: Some("websocket".to_string()),
            connection: Some("Upgrade".to_string()),
            key: Some("dGhlIHNhbXBsZSBub25jZQ==".to_string()),
            version: Some("13".to_string()),
        };
        let normal = TestServerConfig::default();
        let (bytes, ok) = handshake_reply(Some(&req), &normal);
        assert!(ok);
        let head = String::from_utf8(bytes).unwrap();
        assert!(head.starts_with("HTTP/1.1 101 Switching Protocols\r\n"));
        assert!(head.contains("Upgrade: websocket\r\n"));
        assert!(head.contains(&format!(
            "Sec-WebSocket-Accept: {}\r\n",
            accept_key("dGhlIHNhbXBsZSBub25jZQ==")
        )));
        assert!(!head.contains("Sec-WebSocket-Extensions"), "never negotiate");

        let (bytes, ok) =
            handshake_reply(Some(&req), &TestServerConfig { handshake: HandshakeBehavior::HttpStatus(404), ..normal.clone() });
        assert!(!ok);
        assert!(String::from_utf8(bytes).unwrap().starts_with("HTTP/1.1 404 "));

        let (bytes, ok) =
            handshake_reply(Some(&req), &TestServerConfig { handshake: HandshakeBehavior::OmitUpgrade, ..normal.clone() });
        assert!(!ok);
        assert!(!String::from_utf8(bytes).unwrap().contains("Upgrade:"));

        let (bytes, ok) = handshake_reply(
            Some(&req),
            &TestServerConfig { handshake: HandshakeBehavior::NegotiateExtension, ..normal },
        );
        assert!(!ok);
        assert!(String::from_utf8(bytes).unwrap().contains("Sec-WebSocket-Extensions: permessage-deflate"));

        // Invalid request (wrong path) → 400, never an upgrade.
        let mut bad = req;
        bad.path = "/other".to_string();
        let (bytes, ok) = handshake_reply(Some(&bad), &TestServerConfig::default());
        assert!(!ok);
        assert!(String::from_utf8(bytes).unwrap().starts_with("HTTP/1.1 400 "));
        let (bytes, ok) = handshake_reply(None, &TestServerConfig::default());
        assert!(!ok);
        assert!(String::from_utf8(bytes).unwrap().starts_with("HTTP/1.1 400 "));
    }

    #[test]
    fn close_payload_rules_mirror_rfc6455() {
        assert_eq!(parse_close_payload(&[]).unwrap(), None);
        assert!(matches!(
            parse_close_payload(&[0x03]),
            Err(WsReadError::Protocol("close-payload-1"))
        ));
        assert!(matches!(
            parse_close_payload(&[0x03, 0xEE]),
            Err(WsReadError::Protocol("close-code-invalid"))
        ));
        assert_eq!(
            parse_close_payload(&[0x03, 0xE8, b'b']).unwrap(),
            Some((1000, "b".to_string()))
        );
    }
}
