// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # ws — hand-written RFC 6455 WebSocket CLIENT subset (N13 increment A)
//!
//! Async (tokio) client-side WebSocket over an ALREADY-ESTABLISHED
//! [`AsyncRead + AsyncWrite`](tokio::io) stream: the relay `rel`/`rels` URL
//! mapping, TCP dialing and TLS (rustls, SNI per spec §1.4) belong to the
//! caller — this module takes the connected stream and speaks the wire
//! protocol on it (`docs/relay-client-spec-20260914.md` §1.2: the official
//! client sends only the standard RFC 6455 header set, no Authorization, no
//! subprotocols; auth rides the in-band relay Auth frame, not HTTP).
//!
//! ## Why hand-written (dependency discipline, N13 plan §1)
//!
//! `tokio-tungstenite` & co. would be NEW crates and flip the frozen-stack
//! T0 ruling; `httparse` and `ring` are in Cargo.lock only as TRANSITIVE
//! dependencies and the frozen manifest may not change, so the handshake
//! headers are parsed like `management.rs` already does (plain `\r\n`
//! splitting) and the one hash this module needs — SHA-1 for
//! `Sec-WebSocket-Accept` — is an RFC 3174 implementation pinned below
//! against the RFC 6455 §1.3 vector (`dGhlIHNhbXBsZSBub25jZQ==` →
//! `s3pPLMBiTxaQ9kYGzzhZRbK+xOo=`, cross-checked with host `sha1sum`).
//! Everything else uses only tokio core traits + `crate::sys` entropy.
//!
//! ## Capabilities (RFC 6455 client subset)
//!
//! - handshake: `GET path HTTP/1.1` + `Upgrade: websocket` +
//!   `Connection: Upgrade` + 16-byte random `Sec-WebSocket-Key` +
//!   `Sec-WebSocket-Version: 13`; response must be `HTTP/1.1 101` with the
//!   matching `Sec-WebSocket-Accept = base64(SHA1(key + GUID))`
//! - client→server frames ALWAYS masked (RFC 6455 §5.3, key from
//!   `/dev/urandom` — entropy failure is an error, never a fixed key);
//!   server→client frames MUST be unmasked (violation = protocol error)
//! - binary/text/ping/pong/close; 7/16/64-bit length encodings, decoded
//!   with the RFC-mandated minimal-encoding check; fragmented data messages
//!   reassembled (bounded by `max_message_size`, default
//!   [`crate::relay::MAX_MESSAGE_SIZE`]); ping → automatic pong
//! - NO extensions are ever offered; if a server responds with
//!   `Sec-WebSocket-Extensions` the handshake FAILS (fail-closed, not a
//!   silent ignore) — the relay spec §2.6 makes every frame a whole binary
//!   WS message, which permessage-deflate would corrupt
//! - close: `write_close` sends the close frame (and further writes are
//!   rejected); a received close is answered with an echo close and
//!   surfaced as [`WsMessage::Closed`]; reads after that are `AlreadyClosed`
//!
//! ## Failure model — fail-closed, never panic
//!
//! Every malformed byte sequence (RSV bits, unknown opcode, masked server
//! frame, non-minimal length, oversized message, 1-byte close body, invalid
//! close code, non-UTF-8 text/reason, EOF mid-frame) is a typed
//! [`WsError`]. Any `Err` from [`WsClient::read_message`] means the
//! connection state is undefined for further reads: the caller drops the
//! client and redials (relay reconnect guard, spec §6.2). Oversized frames
//! are a hard error — the spec §2.6 note suggests "drop and keep reading"
//! for the RELAY layer, but that needs payload skipping against a
//! server-declared length; this increment rejects and lets the caller
//! redial (recorded as an implementation decision, not silently dropped).

use std::io;
use std::pin::Pin;
use std::task::Poll;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::sys;

// ---------------------------------------------------------------------------
// constants
// ---------------------------------------------------------------------------

/// RFC 6455 §1.3 fixed GUID appended to `Sec-WebSocket-Key` for the accept
/// hash.
pub const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Cap on the 101 response header block before we call the server broken.
pub const HEADER_READ_LIMIT: usize = 16 * 1024;

/// RFC 6455 §5.5: control frame payload cap.
pub const MAX_CONTROL_PAYLOAD: usize = 125;

const OP_CONT: u8 = 0x0;
const OP_TEXT: u8 = 0x1;
const OP_BINARY: u8 = 0x2;
const OP_CLOSE: u8 = 0x8;
const OP_PING: u8 = 0x9;
const OP_PONG: u8 = 0xA;

// ---------------------------------------------------------------------------
// errors — typed, fail-closed, no panics
// ---------------------------------------------------------------------------

/// Every failure mode of this module. Nothing here carries frame content.
#[derive(Debug)]
pub enum WsError {
    /// Underlying stream I/O error.
    Io(io::Error),
    /// Handshake got an HTTP status other than 101.
    HandshakeStatus(u16),
    /// Handshake response malformed (`what` names the broken header/line).
    HandshakeHeader(&'static str),
    /// `Sec-WebSocket-Accept` missing/mismatching (spec: never trust a
    /// server that cannot echo the key hash).
    HandshakeAccept,
    /// Server negotiated a WebSocket extension although none was offered —
    /// fail-closed (relay framing assumes raw binary messages).
    ExtensionNegotiated,
    /// Handshake header block exceeded [`HEADER_READ_LIMIT`].
    HandshakeTooLarge,
    /// Wire-protocol violation (`what` is a stable shape token).
    Protocol(&'static str),
    /// Frame/message exceeded `max_message_size` (carries the limit).
    MessageTooLarge(usize),
    /// Control-frame payload exceeded [`MAX_CONTROL_PAYLOAD`].
    ControlTooLarge(usize),
    /// Peer closed the stream before completing the exchange.
    Eof,
    /// Clean close already done on this direction.
    AlreadyClosed,
}

impl std::fmt::Display for WsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WsError::Io(e) => write!(f, "ws io error: {e}"),
            WsError::HandshakeStatus(c) => write!(f, "ws handshake status {c} != 101"),
            WsError::HandshakeHeader(what) => write!(f, "ws handshake bad header: {what}"),
            WsError::HandshakeAccept => write!(f, "ws handshake Sec-WebSocket-Accept mismatch"),
            WsError::ExtensionNegotiated => write!(f, "ws server negotiated an extension"),
            WsError::HandshakeTooLarge => write!(f, "ws handshake headers over limit"),
            WsError::Protocol(what) => write!(f, "ws protocol violation: {what}"),
            WsError::MessageTooLarge(limit) => write!(f, "ws message over limit {limit}"),
            WsError::ControlTooLarge(limit) => write!(f, "ws control payload over limit {limit}"),
            WsError::Eof => write!(f, "ws unexpected eof"),
            WsError::AlreadyClosed => write!(f, "ws already closed"),
        }
    }
}

impl std::error::Error for WsError {}

// ---------------------------------------------------------------------------
// SHA-1 (RFC 3174) — for Sec-WebSocket-Accept ONLY (legacy use)
// ---------------------------------------------------------------------------

/// SHA-1 over one buffer, RFC 3174. Pinned in tests against the official
/// vectors and the RFC 6455 accept example (host `sha1sum` cross-check).
fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];

    let bitlen = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());

    let mut w = [0u32; 80];
    for block in msg.chunks_exact(64) {
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// `Sec-WebSocket-Accept` expected from the server (RFC 6455 §1.3):
/// `base64(SHA1(key || WS_GUID))`, standard alphabet with padding.
pub fn accept_key(sec_websocket_key: &str) -> String {
    crate::util::base64(&sha1(format!("{sec_websocket_key}{WS_GUID}").as_bytes()))
}

// ---------------------------------------------------------------------------
// entropy — masking keys and Sec-WebSocket-Key must be unpredictable
// ---------------------------------------------------------------------------

/// Fill `buf` from `/dev/urandom` (same fixed-path rawfd pattern as
/// `stun::random_transaction_id`). Any failure is an error: RFC 6455 §5.3
/// needs unpredictable mask keys, so a fixed key is never used (fail-closed).
fn urandom_bytes(buf: &mut [u8]) -> Result<(), WsError> {
    const PATH: &[u8] = b"/dev/urandom\0";
    let fd = unsafe { sys::openat(sys::AT_FDCWD, PATH.as_ptr(), sys::O_RDONLY) };
    if fd < 0 {
        return Err(WsError::Io(io::Error::last_os_error()));
    }
    let mut filled = 0usize;
    let mut interrupted = 0u8;
    while filled < buf.len() {
        let (n, errno) = sys::read_fd(fd, &mut buf[filled..]);
        if n > 0 {
            filled += n as usize;
            interrupted = 0;
            continue;
        }
        if n < 0 && errno == sys::EINTR && interrupted < 2 {
            interrupted += 1;
            continue;
        }
        unsafe { sys::close(fd) };
        let errno = if n < 0 { errno } else { sys::errno() };
        return Err(WsError::Io(io::Error::from_raw_os_error(errno)));
    }
    unsafe { sys::close(fd) };
    Ok(())
}

/// Fresh 16-byte `Sec-WebSocket-Key` (RFC 6455 §4.1), base64 form.
pub fn random_sec_websocket_key() -> Result<String, WsError> {
    let mut raw = [0u8; 16];
    urandom_bytes(&mut raw)?;
    Ok(crate::util::base64(&raw))
}

// ---------------------------------------------------------------------------
// frame codec (pure, golden-tested)
// ---------------------------------------------------------------------------

/// Parsed fixed part of one WS frame header.
struct FrameHeader {
    fin: bool,
    opcode: u8,
    mask: Option<[u8; 4]>,
    payload_len: u64,
    header_len: usize,
}

enum FrameParse {
    /// Need more bytes before the header is complete.
    NeedMore,
    Header(FrameHeader),
}

/// Parse the frame header from the front of `buf` with RFC 6455 §5.2
/// validation: no RSV bits (we negotiate nothing), known opcode, control
/// frames never fragmented and ≤ 125B, minimal length encoding, MSB of the
/// 64-bit length zero.
fn parse_frame_header(buf: &[u8]) -> Result<FrameParse, WsError> {
    if buf.len() < 2 {
        return Ok(FrameParse::NeedMore);
    }
    let b0 = buf[0];
    let fin = b0 & 0x80 != 0;
    let opcode = b0 & 0x0F;
    if b0 & 0x70 != 0 {
        return Err(WsError::Protocol("rsv-set"));
    }
    match opcode {
        OP_CONT | OP_TEXT | OP_BINARY | OP_CLOSE | OP_PING | OP_PONG => {}
        _ => return Err(WsError::Protocol("unknown-opcode")),
    }
    let is_control = opcode >= 0x8;
    let b1 = buf[1];
    let masked = b1 & 0x80 != 0;
    let len7 = (b1 & 0x7F) as u64;
    if is_control {
        if !fin {
            return Err(WsError::Protocol("fragmented-control"));
        }
        if len7 > MAX_CONTROL_PAYLOAD as u64 {
            return Err(WsError::Protocol("control-too-long"));
        }
    }
    let (payload_len, mut header_len) = if len7 < 126 {
        (len7, 2usize)
    } else if len7 == 126 {
        if buf.len() < 4 {
            return Ok(FrameParse::NeedMore);
        }
        let n = u16::from_be_bytes([buf[2], buf[3]]) as u64;
        if n < 126 {
            return Err(WsError::Protocol("non-minimal-length"));
        }
        (n, 4)
    } else {
        if buf.len() < 10 {
            return Ok(FrameParse::NeedMore);
        }
        let n = u64::from_be_bytes(buf[2..10].try_into().expect("len checked above"));
        if n & 0x8000_0000_0000_0000 != 0 {
            return Err(WsError::Protocol("length-top-bit"));
        }
        if n < 65536 {
            return Err(WsError::Protocol("non-minimal-length"));
        }
        (n, 10)
    };
    if masked {
        header_len += 4;
    }
    if buf.len() < header_len {
        return Ok(FrameParse::NeedMore);
    }
    let mask = masked.then(|| {
        let mut k = [0u8; 4];
        k.copy_from_slice(&buf[header_len - 4..header_len]);
        k
    });
    Ok(FrameParse::Header(FrameHeader { fin, opcode, mask, payload_len, header_len }))
}

/// RFC 6455 §5.3 masking in place (`payload[i] ^= mask[i % 4]`); XOR is its
/// own inverse, so the same function masks client frames and unmasks for
/// inspection.
fn apply_mask(payload: &mut [u8], mask: [u8; 4]) {
    for (i, b) in payload.iter_mut().enumerate() {
        *b ^= mask[i & 3];
    }
}

/// Encode ONE complete client frame: FIN set (we send whole messages only —
/// the relay protocol is one frame per WS message, spec §2.6), MASK set,
/// minimal length encoding. Control frames must obey §5.5 (≤ 125B; the
/// write API enforces this before reaching here).
fn encode_client_frame(opcode: u8, payload: &[u8], mask: [u8; 4]) -> Vec<u8> {
    let len = payload.len();
    let mut frame = Vec::with_capacity(len + 14);
    frame.push(0x80 | opcode);
    if len < 126 {
        frame.push(0x80 | len as u8);
    } else if len <= 0xFFFF {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }
    frame.extend_from_slice(&mask);
    let start = frame.len();
    frame.extend_from_slice(payload);
    apply_mask(&mut frame[start..], mask);
    frame
}

/// Wire-valid close codes (RFC 6455 §7.4.1). 1004 is undefined, 1005/1006/
/// 1015 are local-state codes that must never appear on the wire, 1016+ is
/// registry space.
fn is_valid_close_code(code: u16) -> bool {
    matches!(code, 1000..=1003 | 1007..=1014 | 3000..=4999)
}

/// `[code 2B][reason UTF-8]` or empty; RFC 6455 §5.5.1 forbids a 1-byte body.
fn parse_close_payload(payload: &[u8]) -> Result<Option<(u16, String)>, WsError> {
    match payload.len() {
        0 => Ok(None),
        1 => Err(WsError::Protocol("close-payload-1")),
        _ => {
            let code = u16::from_be_bytes([payload[0], payload[1]]);
            if !is_valid_close_code(code) {
                return Err(WsError::Protocol("close-code-invalid"));
            }
            let reason = std::str::from_utf8(&payload[2..])
                .map_err(|_| WsError::Protocol("close-reason-utf8"))?;
            Ok(Some((code, reason.to_string())))
        }
    }
}

// ---------------------------------------------------------------------------
// handshake
// ---------------------------------------------------------------------------

/// Validate the 101 response header block (pure). Exact `Sec-WebSocket-Accept`
/// match required; ANY `Sec-WebSocket-Extensions` response fails — we offer
/// none, so accepting one would silently change every frame's bytes.
fn validate_handshake_response(head: &str, sec_websocket_key: &str) -> Result<(), WsError> {
    let mut lines = head.split("\r\n");
    let status = lines.next().ok_or(WsError::HandshakeHeader("status-line"))?;
    let mut parts = status.split_whitespace();
    if parts.next() != Some("HTTP/1.1") {
        return Err(WsError::HandshakeHeader("http-version"));
    }
    let code: u16 = parts
        .next()
        .unwrap_or("")
        .parse()
        .map_err(|_| WsError::HandshakeHeader("status-code"))?;
    if code != 101 {
        return Err(WsError::HandshakeStatus(code));
    }

    let mut upgrade: Option<&str> = None;
    let mut connection: Option<&str> = None;
    let mut accept: Option<&str> = None;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(WsError::HandshakeHeader("header-line"));
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "upgrade" => upgrade = Some(value.trim()),
            "connection" => connection = Some(value.trim()),
            "sec-websocket-accept" => accept = Some(value.trim()),
            "sec-websocket-extensions" => return Err(WsError::ExtensionNegotiated),
            // Sec-WebSocket-Protocol (none requested), Date, Set-Cookie…: ignore.
            _ => {}
        }
    }
    match upgrade {
        Some(v) if v.eq_ignore_ascii_case("websocket") => {}
        _ => return Err(WsError::HandshakeHeader("upgrade")),
    }
    let connection_upgrades = connection
        .map(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case("upgrade")))
        .unwrap_or(false);
    if !connection_upgrades {
        return Err(WsError::HandshakeHeader("connection"));
    }
    match accept {
        Some(a) if a == accept_key(sec_websocket_key) => Ok(()),
        Some(_) => Err(WsError::HandshakeAccept),
        None => Err(WsError::HandshakeHeader("sec-websocket-accept")),
    }
}

/// Run the client handshake over an established stream and return a ready
/// [`WsClient`]. `host` feeds the `Host:` header (host[:port]); `path` is the
/// request path (`/relay` for NetBird relay, spec §1.1 — mapping from
/// `rel`/`rels` URLs is the caller's job); `sec_websocket_key` should come
/// from [`random_sec_websocket_key`]. TLS: wrap the stream yourself (rustls,
/// SNI per spec §1.4) and hand it in.
///
/// Cancellation: the request is sent before the first response byte is read;
/// dropping the future mid-handshake loses whatever response bytes were
/// already consumed into its local buffer — do not reuse the stream for a
/// second attempt, dial fresh.
pub async fn client_handshake<S>(
    mut io: S,
    host: &str,
    path: &str,
    sec_websocket_key: &str,
) -> Result<WsClient<S>, WsError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if host.is_empty() {
        return Err(WsError::Protocol("empty-host"));
    }
    if path.is_empty() || !path.starts_with('/') {
        return Err(WsError::Protocol("bad-path"));
    }
    // Spec §1.2: the standard RFC 6455 set only — no Authorization, no
    // subprotocols, and no Sec-WebSocket-Extensions (we negotiate nothing).
    let request = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {sec_websocket_key}\r\n\
         Sec-WebSocket-Version: 13\r\n\
         \r\n"
    );
    write_all(&mut io, request.as_bytes()).await.map_err(WsError::Io)?;
    flush_io(&mut io).await.map_err(WsError::Io)?;

    let mut buf: Vec<u8> = Vec::with_capacity(512);
    let head_end = loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        if buf.len() > HEADER_READ_LIMIT {
            return Err(WsError::HandshakeTooLarge);
        }
        let start = buf.len();
        buf.resize(start + 512, 0);
        let n = read_some(&mut io, &mut buf[start..]).await.map_err(WsError::Io)?;
        if n == 0 {
            return Err(WsError::Eof);
        }
        buf.truncate(start + n);
    };
    let head = std::str::from_utf8(&buf[..head_end])
        .map_err(|_| WsError::Protocol("handshake-headers-utf8"))?;
    validate_handshake_response(head, sec_websocket_key)?;
    // Bytes after the header block are already WS frames — keep them.
    Ok(WsClient::new(io, buf[head_end + 4..].to_vec()))
}

// ---------------------------------------------------------------------------
// client session
// ---------------------------------------------------------------------------

/// One message received from the server. Pings are answered internally and
/// NOT surfaced; everything else is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsMessage {
    /// Binary data message (the only thing the relay protocol uses).
    Binary(Vec<u8>),
    /// Text data message (validated UTF-8).
    Text(String),
    /// Unsolicited pong (reply to a ping we sent).
    Pong(Vec<u8>),
    /// Peer close frame: `(code, reason)` when a body was present. After this
    /// event the client echoes close (if we hadn't sent one) and further
    /// reads yield [`WsError::AlreadyClosed`].
    Closed(Option<(u16, String)>),
}

/// WebSocket client session over an established stream.
pub struct WsClient<S> {
    io: S,
    buffer: Vec<u8>,
    max_message_size: usize,
    fragmented: bool,
    frag_opcode: u8,
    message: Vec<u8>,
    close_sent: bool,
    close_received: bool,
}

/// Shape-only debug: counters and flags, NEVER buffer contents (a buffered
/// frame can carry WG packets / relay payloads — nothing of it in logs).
impl<S> std::fmt::Debug for WsClient<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WsClient")
            .field("max_message_size", &self.max_message_size)
            .field("buffered_len", &self.buffer.len())
            .field("fragmented", &self.fragmented)
            .field("close_sent", &self.close_sent)
            .field("close_received", &self.close_received)
            .finish_non_exhaustive()
    }
}

impl<S> WsClient<S> {
    fn new(io: S, buffered: Vec<u8>) -> Self {
        WsClient {
            io,
            buffer: buffered,
            max_message_size: crate::relay::MAX_MESSAGE_SIZE,
            fragmented: false,
            frag_opcode: 0,
            message: Vec::new(),
            close_sent: false,
            close_received: false,
        }
    }

    /// Current inbound message ceiling (default: the relay protocol limit
    /// `crate::relay::MAX_MESSAGE_SIZE` = 8820).
    pub fn max_message_size(&self) -> usize {
        self.max_message_size
    }

    /// Override the inbound message ceiling (for non-relay reuse).
    pub fn set_max_message_size(&mut self, max: usize) {
        self.max_message_size = max;
    }

    /// True once the peer close has been processed.
    pub fn is_peer_closed(&self) -> bool {
        self.close_received
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> WsClient<S> {
    /// Read the next complete message. Pings are auto-ponged (never
    /// surfaced). On ANY error the session is undefined for further reads —
    /// drop the client and redial.
    ///
    /// # Cancellation semantics
    ///
    /// Dropping this future at an await point (e.g. under
    /// `tokio::time::timeout`) never corrupts the READ state: no inbound
    /// bytes are consumed or buffered for a cancelled read, a partially
    /// received frame stays byte-exact in the internal buffer, and the next
    /// `read_message` resumes parsing exactly where the cancelled one
    /// parked. The exceptions are the in-flight control-frame ANSWERS, which
    /// are wire side effects and cannot be rolled back — if cancellation
    /// lands while a pong or close echo is being sent (the only awaits
    /// inside parsing, see `try_parse_one`), that answer may be partially
    /// written and its trigger frame is already consumed from the buffer;
    /// treat the session as dead and redial, exactly as after an `Err`.
    pub async fn read_message(&mut self) -> Result<WsMessage, WsError> {
        if self.close_received {
            return Err(WsError::AlreadyClosed);
        }
        loop {
            match self.try_parse_one().await? {
                Step::Message(msg) => return Ok(msg),
                // More frames may already sit in the buffer — re-parse before
                // touching the stream again (pipelined messages are normal).
                Step::Continue => {}
                Step::NeedMore => self.fill_buffer().await?,
            }
        }
    }

    /// Send one binary message (mask bit + fresh random mask key).
    pub async fn write_binary(&mut self, data: &[u8]) -> Result<(), WsError> {
        if data.len() > self.max_message_size {
            return Err(WsError::MessageTooLarge(self.max_message_size));
        }
        self.send_frame(OP_BINARY, data).await
    }

    /// Send one text message (mask bit + fresh random mask key).
    pub async fn write_text(&mut self, text: &str) -> Result<(), WsError> {
        if text.len() > self.max_message_size {
            return Err(WsError::MessageTooLarge(self.max_message_size));
        }
        self.send_frame(OP_TEXT, text.as_bytes()).await
    }

    /// Send a ping (payload ≤ 125 bytes, RFC 6455 §5.5.2).
    pub async fn write_ping(&mut self, data: &[u8]) -> Result<(), WsError> {
        if data.len() > MAX_CONTROL_PAYLOAD {
            return Err(WsError::ControlTooLarge(MAX_CONTROL_PAYLOAD));
        }
        self.send_frame(OP_PING, data).await
    }

    /// Send a pong (payload ≤ 125 bytes, RFC 6455 §5.5.3).
    pub async fn write_pong(&mut self, data: &[u8]) -> Result<(), WsError> {
        if data.len() > MAX_CONTROL_PAYLOAD {
            return Err(WsError::ControlTooLarge(MAX_CONTROL_PAYLOAD));
        }
        self.send_frame(OP_PONG, data).await
    }

    /// Begin the closing handshake. Valid codes only (see
    /// [`is_valid_close_code`]); reason ≤ 123 bytes. Further writes are
    /// rejected; reads continue until the peer's close arrives.
    ///
    /// Cancellation: `close_sent` flips only AFTER the close frame is fully
    /// on the wire. Dropping this future mid-write can leave a partial close
    /// frame on the stream while `close_sent` is still `false` — never retry
    /// on the same session; drop it and redial.
    pub async fn write_close(&mut self, code: Option<u16>, reason: &str) -> Result<(), WsError> {
        if self.close_sent {
            return Err(WsError::AlreadyClosed);
        }
        if let Some(c) = code {
            if !is_valid_close_code(c) {
                return Err(WsError::Protocol("close-code-invalid"));
            }
        }
        if 2 + reason.len() > MAX_CONTROL_PAYLOAD {
            return Err(WsError::ControlTooLarge(MAX_CONTROL_PAYLOAD));
        }
        let mut payload = Vec::new();
        if let Some(c) = code {
            payload.extend_from_slice(&c.to_be_bytes());
        }
        payload.extend_from_slice(reason.as_bytes());
        self.send_frame(OP_CLOSE, &payload).await?;
        self.close_sent = true;
        Ok(())
    }

    /// Give the stream back (for shutdown/TLS teardown by the caller).
    pub fn into_inner(self) -> S {
        self.io
    }

    /// Parse and dispatch one complete frame from the buffer, if present.
    async fn try_parse_one(&mut self) -> Result<Step, WsError> {
        let header = match parse_frame_header(&self.buffer)? {
            FrameParse::NeedMore => return Ok(Step::NeedMore),
            FrameParse::Header(h) => h,
        };
        if header.mask.is_some() {
            return Err(WsError::Protocol("server-frame-masked"));
        }
        if header.payload_len > self.max_message_size as u64 {
            return Err(WsError::MessageTooLarge(self.max_message_size));
        }
        let total = header.header_len + header.payload_len as usize;
        if self.buffer.len() < total {
            return Ok(Step::NeedMore);
        }
        // Copy out before any await/borrow dance; drain the wire bytes.
        let payload = self.buffer[header.header_len..total].to_vec();
        self.buffer.drain(..total);

        // From here on the frame is fully consumed; the arms below are the
        // ONLY places parsing awaits (pong / close echo). Those sends are
        // wire side effects and are NOT cancel-safe: if the future is
        // dropped mid-send, the trigger frame is gone from the buffer and
        // the answer may be half-written — see the cancellation notes on
        // `read_message`.
        match header.opcode {
            OP_CLOSE => {
                let info = parse_close_payload(&payload)?;
                if !self.close_sent {
                    // RFC 6455 §5.5.1: answer close with close; echo the code,
                    // drop the reason.
                    let echo: Vec<u8> = match info {
                        Some((code, _)) => code.to_be_bytes().to_vec(),
                        None => Vec::new(),
                    };
                    self.send_frame(OP_CLOSE, &echo).await?;
                    self.close_sent = true;
                }
                self.close_received = true;
                Ok(Step::Message(WsMessage::Closed(info)))
            }
            OP_PING => {
                if !self.close_sent {
                    self.send_frame(OP_PONG, &payload).await?;
                }
                Ok(Step::Continue)
            }
            OP_PONG => Ok(Step::Message(WsMessage::Pong(payload))),
            OP_CONT => {
                if !self.fragmented {
                    return Err(WsError::Protocol("unexpected-continuation"));
                }
                if self.message.len() + payload.len() > self.max_message_size {
                    return Err(WsError::MessageTooLarge(self.max_message_size));
                }
                self.message.extend_from_slice(&payload);
                if header.fin {
                    let opcode = self.frag_opcode;
                    let data = std::mem::take(&mut self.message);
                    self.fragmented = false;
                    Ok(Step::Message(finish_data(opcode, data)?))
                } else {
                    Ok(Step::Continue)
                }
            }
            _ => {
                // OP_TEXT | OP_BINARY
                if self.fragmented {
                    return Err(WsError::Protocol("data-during-fragmentation"));
                }
                if header.fin {
                    Ok(Step::Message(finish_data(header.opcode, payload)?))
                } else {
                    self.frag_opcode = header.opcode;
                    self.message = payload;
                    self.fragmented = true;
                    Ok(Step::Continue)
                }
            }
        }
    }

    /// Pull more wire bytes into the read buffer. Cancellation-safe by
    /// construction: `self` is only touched AFTER the read has completed —
    /// bytes land in a local `tmp` first and are appended on success, so a
    /// future dropped at the await point (e.g. a `tokio::time::timeout`
    /// around `read_message`) leaves `self` byte-identical and consumes no
    /// stream bytes. (The previous resize-in-place-then-truncate pattern
    /// left 4096 zero bytes behind on cancellation; the next parse read
    /// them as a continuation frame → `unexpected-continuation`.)
    async fn fill_buffer(&mut self) -> Result<(), WsError> {
        const CHUNK: usize = 4096;
        let mut tmp = [0u8; CHUNK];
        let n = read_some(&mut self.io, &mut tmp).await.map_err(WsError::Io)?;
        if n == 0 {
            return Err(WsError::Eof);
        }
        self.buffer.extend_from_slice(&tmp[..n]);
        Ok(())
    }

    /// Send one frame. NOT cancel-safe: dropping the future mid-`write_all`
    /// can leave a PARTIAL frame on the outbound stream — written bytes
    /// cannot be un-written, so a cancelled send poisons the write direction
    /// and the session must be dropped, not retried (same for the public
    /// `write_*` wrappers; the flags at their call sites are only set after
    /// the write completes, see `write_close`).
    async fn send_frame(&mut self, opcode: u8, payload: &[u8]) -> Result<(), WsError> {
        if self.close_sent {
            return Err(WsError::AlreadyClosed);
        }
        let mut mask = [0u8; 4];
        urandom_bytes(&mut mask)?;
        let frame = encode_client_frame(opcode, payload, mask);
        write_all(&mut self.io, &frame).await.map_err(WsError::Io)?;
        flush_io(&mut self.io).await.map_err(WsError::Io)
    }
}

fn finish_data(opcode: u8, data: Vec<u8>) -> Result<WsMessage, WsError> {
    match opcode {
        OP_BINARY => Ok(WsMessage::Binary(data)),
        OP_TEXT => match String::from_utf8(data) {
            Ok(text) => Ok(WsMessage::Text(text)),
            Err(_) => Err(WsError::Protocol("text-not-utf8")),
        },
        _ => Err(WsError::Protocol("unknown-opcode")),
    }
}

/// Outcome of one parse/dispatch step inside the read loop.
enum Step {
    /// A complete message for the caller.
    Message(WsMessage),
    /// Frame consumed (ping answered / fragment started) — the buffer may
    /// already hold more frames; parse again before reading the stream.
    Continue,
    /// Buffer cannot yield another header/frame — pull from the stream.
    NeedMore,
}

// ---------------------------------------------------------------------------
// stream helpers (tokio core traits only — no io-util feature required)
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

async fn write_all<S: AsyncWrite + Unpin>(io: &mut S, mut data: &[u8]) -> io::Result<()> {
    while !data.is_empty() {
        let n = std::future::poll_fn(|cx| Pin::new(&mut *io).poll_write(cx, data)).await?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "ws write made no progress"));
        }
        data = &data[n..];
    }
    Ok(())
}

async fn flush_io<S: AsyncWrite + Unpin>(io: &mut S) -> io::Result<()> {
    std::future::poll_fn(|cx| Pin::new(&mut *io).poll_flush(cx)).await
}

// ---------------------------------------------------------------------------
// tests — all offline, in-memory duplex, deterministic (no sleeps, no network)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::task::Context;

    // RFC vectors, cross-checked with host sha1sum:
    //   ""        -> da39a3ee5e6b4b0d3255bfef95601890afd80709
    //   "abc"     -> a9993e364706816aba3e25717850c26c9cd0d89d
    //   56 bytes  -> 84983e441c3bd26ebaae4aa1f95129e5e54670f1  (padding edge)
    //   RFC6455 key+GUID -> b37a4f2cc0624f1690f64606cf385945b2bec4ea
    #[test]
    fn sha1_official_vectors() {
        let cases: Vec<(&str, &str)> = vec![
            ("", "da39a3ee5e6b4b0d3255bfef95601890afd80709"),
            ("abc", "a9993e364706816aba3e25717850c26c9cd0d89d"),
            (
                "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
                "84983e441c3bd26ebaae4aa1f95129e5e54670f1",
            ),
            (
                "dGhlIHNhbXBsZSBub25jZQ==258EAFA5-E914-47DA-95CA-C5AB0DC85B11",
                "b37a4f2cc0624f1690f64606cf385945b2bec4ea",
            ),
        ];
        for (input, want_hex) in cases {
            assert_eq!(crate::util::hex_lower(&sha1(input.as_bytes())), want_hex, "input={input:?}");
        }
    }

    #[test]
    fn accept_key_matches_rfc6455_example() {
        // RFC 6455 §1.3, the official example — MUST pass byte-exact.
        assert_eq!(accept_key("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn random_sec_websocket_key_shape() {
        let a = random_sec_websocket_key().unwrap();
        let b = random_sec_websocket_key().unwrap();
        assert_ne!(a, b, "keys must be fresh");
        assert_eq!(a.len(), 24, "16 bytes -> 24 base64 chars, no padding");
        use base64::Engine as _;
        assert_eq!(
            base64::engine::general_purpose::STANDARD.decode(&a).unwrap().len(),
            16
        );
    }

    // ---- frame encoding goldens (fixed mask keys, hand-derived bytes) ------

    const KEY: [u8; 4] = [0xDE, 0xAD, 0xBE, 0xEF];

    #[test]
    fn encode_golden_small_payload() {
        // [0x82][0x80|4][mask][00^DE 01^AD 02^BE 03^EF]
        let frame = encode_client_frame(OP_BINARY, &[0x00, 0x01, 0x02, 0x03], KEY);
        assert_eq!(frame, vec![0x82, 0x84, 0xDE, 0xAD, 0xBE, 0xEF, 0xDE, 0xAC, 0xBC, 0xEC]);
    }

    #[test]
    fn encode_golden_len16_branch() {
        let payload = vec![0xAB; 200];
        let mask = [1u8, 2, 3, 4];
        let frame = encode_client_frame(OP_BINARY, &payload, mask);
        assert_eq!(frame.len(), 2 + 2 + 4 + 200);
        assert_eq!(&frame[..4], &[0x82, 0xFE, 0x00, 0xC8], "16-bit length = 200");
        assert_eq!(&frame[4..8], &mask);
        // Masking placement: payload[i] ^ mask[i%4] at several offsets.
        for &i in &[0usize, 1, 2, 3, 4, 127, 199] {
            assert_eq!(frame[8 + i], 0xAB ^ mask[i & 3], "offset {i}");
        }
    }

    #[test]
    fn encode_golden_len64_branch() {
        let payload = vec![0u8; 65536];
        let frame = encode_client_frame(OP_BINARY, &payload, [9u8; 4]);
        assert_eq!(frame.len(), 2 + 8 + 4 + 65536);
        assert_eq!(&frame[..2], &[0x82, 0xFF], "64-bit length marker");
        assert_eq!(&frame[2..10], &[0, 0, 0, 0, 0, 1, 0, 0], "65536 big-endian");
        assert_eq!(&frame[10..14], &[9, 9, 9, 9]);
        // Payload was zeros; masking is identity-xor-key so every byte is 9.
        assert!(frame[14..].iter().all(|&b| b == 0u8 ^ 9));
    }

    #[test]
    fn encode_golden_control_frames_never_fragmented() {
        // Ping with 2-byte payload: FIN | opcode 0x9 in the FIRST byte.
        let ping = encode_client_frame(OP_PING, b"hi", [0u8; 4]);
        assert_eq!(ping, vec![0x89, 0x82, 0, 0, 0, 0, b'h', b'i']);
        // Close with code+reason: single FIN frame, exact body.
        let close = encode_client_frame(OP_CLOSE, &[0x03, 0xE8, b'b', b'y', b'e'], [0u8; 4]);
        assert_eq!(close, vec![0x88, 0x85, 0, 0, 0, 0, 0x03, 0xE8, b'b', b'y', b'e']);
    }

    #[test]
    fn apply_mask_is_involutive() {
        let mut data: Vec<u8> = (0..=70u8).collect();
        let original = data.clone();
        apply_mask(&mut data, KEY);
        assert_ne!(&data[..8], &original[..8]);
        apply_mask(&mut data, KEY);
        assert_eq!(data, original, "xor twice = identity, across i%4 wrap");
    }

    // ---- in-memory duplex (no sockets, no sleeps, deterministic) -----------

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
        (
            DuplexEnd { my: a.clone(), peer: b.clone() },
            DuplexEnd { my: b, peer: a },
        )
    }

    impl AsyncRead for DuplexEnd {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
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
            if n > 0 {
                buf.initialize_unfilled()[..n].copy_from_slice(&me.incoming[..n]);
                buf.advance(n);
                me.incoming.drain(..n);
            }
            Poll::Ready(Ok(()))
        }
    }

    impl AsyncWrite for DuplexEnd {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            let this = self.get_mut();
            if this.my.lock().unwrap().closed_for_write {
                return Poll::Ready(Err(io::Error::new(io::ErrorKind::BrokenPipe, "shut down")));
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

    const RFC_KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";

    async fn server_read_until(io: &mut DuplexEnd, marker: &[u8]) -> Vec<u8> {
        let mut acc: Vec<u8> = Vec::new();
        loop {
            if let Some(_) = acc.windows(marker.len()).position(|w| w == marker) {
                return acc;
            }
            let mut tmp = [0u8; 1024];
            let n = read_some(io, &mut tmp).await.expect("server read");
            assert!(n > 0, "server stream EOF");
            acc.extend_from_slice(&tmp[..n]);
        }
    }

    async fn feed(server: &mut DuplexEnd, bytes: &[u8]) {
        write_all(server, bytes).await.expect("feed");
    }

    /// Test-only half-close via the trait method (avoids tokio io-util's
    /// AsyncWriteExt in tests, matching the module's no-extra-feature rule).
    async fn shutdown_io(io: &mut DuplexEnd) {
        std::future::poll_fn(|cx| Pin::new(&mut *io).poll_shutdown(cx)).await.unwrap();
    }

    fn server_buffered(server: &DuplexEnd) -> usize {
        server.peer.lock().unwrap().incoming.len()
    }

    /// Full deterministic handshake over the duplex: asserts the request is
    /// the exact spec §1.2 header set, replies a valid 101.
    async fn make_connected() -> (WsClient<DuplexEnd>, DuplexEnd) {
        let (client_io, mut server) = duplex_pair();
        let handle = tokio::spawn(client_handshake(client_io, "relay.example:443", "/relay", RFC_KEY));

        let req = server_read_until(&mut server, b"\r\n\r\n").await;
        let req = String::from_utf8(req).unwrap();
        assert!(req.starts_with("GET /relay HTTP/1.1\r\n"), "request line: {req:?}");
        assert!(req.contains("Host: relay.example:443\r\n"));
        assert!(req.contains("Upgrade: websocket\r\n"));
        assert!(req.contains("Connection: Upgrade\r\n"));
        assert!(req.contains(&format!("Sec-WebSocket-Key: {RFC_KEY}\r\n")));
        assert!(req.contains("Sec-WebSocket-Version: 13\r\n"));
        // No extensions offered, no auth header, no subprotocols (spec §1.2).
        assert!(!req.contains("Sec-WebSocket-Extensions"));
        assert!(!req.contains("Authorization"));
        assert!(!req.contains("Sec-WebSocket-Protocol"));

        let resp = format!(
            "HTTP/1.1 101 Switching Protocols\r\n\
             Upgrade: WebSocket\r\n\
             Connection: upgrade\r\n\
             Sec-WebSocket-Accept: {}\r\n\
             \r\n",
            accept_key(RFC_KEY)
        );
        feed(&mut server, resp.as_bytes()).await;
        let client = handle.await.unwrap().expect("handshake ok");
        assert_eq!(client.max_message_size(), crate::relay::MAX_MESSAGE_SIZE);
        (client, server)
    }

    // ---- handshake over the duplex -----------------------------------------

    #[tokio::test]
    async fn handshake_request_headers_golden_and_101_accepts() {
        make_connected().await;
    }

    #[tokio::test]
    async fn handshake_rejects_non_101_status() {
        let (client_io, mut server) = duplex_pair();
        let handle = tokio::spawn(client_handshake(client_io, "h", "/relay", RFC_KEY));
        let _ = server_read_until(&mut server, b"\r\n\r\n").await;
        feed(&mut server, b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n").await;
        let err = handle.await.unwrap().unwrap_err();
        assert!(matches!(err, WsError::HandshakeStatus(404)), "got {err:?}");
    }

    #[tokio::test]
    async fn handshake_rejects_accept_mismatch() {
        let (client_io, mut server) = duplex_pair();
        let handle = tokio::spawn(client_handshake(client_io, "h", "/relay", RFC_KEY));
        let _ = server_read_until(&mut server, b"\r\n\r\n").await;
        feed(
            &mut server,
            b"HTTP/1.1 101 Switching Protocols\r\n\
              Upgrade: websocket\r\n\
              Connection: Upgrade\r\n\
              Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xO0=\r\n\
              \r\n",
        )
        .await;
        let err = handle.await.unwrap().unwrap_err();
        assert!(matches!(err, WsError::HandshakeAccept), "got {err:?}");
    }

    #[tokio::test]
    async fn handshake_rejects_missing_upgrade_header() {
        let (client_io, mut server) = duplex_pair();
        let handle = tokio::spawn(client_handshake(client_io, "h", "/relay", RFC_KEY));
        let _ = server_read_until(&mut server, b"\r\n\r\n").await;
        let resp = format!(
            "HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
            accept_key(RFC_KEY)
        );
        feed(&mut server, resp.as_bytes()).await;
        let err = handle.await.unwrap().unwrap_err();
        assert!(matches!(err, WsError::HandshakeHeader("upgrade")), "got {err:?}");
    }

    #[tokio::test]
    async fn handshake_fails_closed_on_extension_negotiation() {
        let (client_io, mut server) = duplex_pair();
        let handle = tokio::spawn(client_handshake(client_io, "h", "/relay", RFC_KEY));
        let _ = server_read_until(&mut server, b"\r\n\r\n").await;
        let resp = format!(
            "HTTP/1.1 101 Switching Protocols\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Extensions: permessage-deflate\r\n\
             Sec-WebSocket-Accept: {}\r\n\
             \r\n",
            accept_key(RFC_KEY)
        );
        feed(&mut server, resp.as_bytes()).await;
        let err = handle.await.unwrap().unwrap_err();
        assert!(matches!(err, WsError::ExtensionNegotiated), "got {err:?}");
    }

    #[tokio::test]
    async fn handshake_eof_is_typed() {
        let (client_io, mut server) = duplex_pair();
        let handle = tokio::spawn(client_handshake(client_io, "h", "/relay", RFC_KEY));
        let _ = server_read_until(&mut server, b"\r\n\r\n").await;
        shutdown_io(&mut server).await;
        let err = handle.await.unwrap().unwrap_err();
        assert!(matches!(err, WsError::Eof), "got {err:?}");
    }

    #[tokio::test]
    async fn handshake_header_flood_is_too_large() {
        let (client_io, mut server) = duplex_pair();
        let handle = tokio::spawn(client_handshake(client_io, "h", "/relay", RFC_KEY));
        let _ = server_read_until(&mut server, b"\r\n\r\n").await;
        feed(&mut server, &vec![b'x'; HEADER_READ_LIMIT + 1024]).await;
        let err = handle.await.unwrap().unwrap_err();
        assert!(matches!(err, WsError::HandshakeTooLarge), "got {err:?}");
    }

    #[tokio::test]
    async fn handshake_validates_arguments_fail_closed() {
        let (a, _b) = duplex_pair();
        let err = client_handshake(a.clone(), "", "/relay", RFC_KEY).await.unwrap_err();
        assert!(matches!(err, WsError::Protocol("empty-host")));
        let err = client_handshake(a, "h", "no-slash", RFC_KEY).await.unwrap_err();
        assert!(matches!(err, WsError::Protocol("bad-path")));
    }

    // ---- write path over the wire ------------------------------------------

    #[tokio::test]
    async fn write_binary_is_masked_on_wire_and_roundtrips() {
        let (mut client, mut server) = make_connected().await;
        client.write_binary(b"hello relay").await.unwrap();

        // Read exactly the frame bytes: 2B header + 4B key + 11B payload.
        let mut raw = vec![0u8; 2 + 4 + 11];
        let n = read_some(&mut server, &mut raw).await.unwrap();
        assert_eq!(n, raw.len());
        assert_eq!(raw[0], 0x82, "FIN|binary");
        assert_eq!(raw[1] & 0x80, 0x80, "client frames MUST be masked");
        assert_eq!(raw[1] & 0x7F, 11, "7-bit length branch");
        let key = [raw[2], raw[3], raw[4], raw[5]];
        let mut payload = raw[6..].to_vec();
        apply_mask(&mut payload, key);
        assert_eq!(payload, b"hello relay");
    }

    #[tokio::test]
    async fn write_text_golden_layout() {
        let (mut client, mut server) = make_connected().await;
        client.write_text("hi").await.unwrap();
        let mut raw = vec![0u8; 2 + 4 + 2];
        read_some(&mut server, &mut raw).await.unwrap();
        assert_eq!(raw[0], 0x81, "FIN|text");
        assert_eq!(raw[1], 0x80 | 2);
        let mut payload = raw[6..].to_vec();
        apply_mask(&mut payload, [raw[2], raw[3], raw[4], raw[5]]);
        assert_eq!(payload, b"hi");
    }

    #[tokio::test]
    async fn write_close_golden_then_writes_rejected() {
        let (mut client, mut server) = make_connected().await;
        client.write_close(Some(1000), "bye").await.unwrap();
        let mut raw = vec![0u8; 2 + 4 + 5];
        read_some(&mut server, &mut raw).await.unwrap();
        assert_eq!(raw[0], 0x88, "FIN|close");
        assert_eq!(raw[1], 0x80 | 5);
        let mut payload = raw[6..].to_vec();
        apply_mask(&mut payload, [raw[2], raw[3], raw[4], raw[5]]);
        assert_eq!(payload, [0x03, 0xE8, b'b', b'y', b'e']);

        assert!(matches!(
            client.write_binary(b"late").await,
            Err(WsError::AlreadyClosed)
        ));
        assert!(matches!(
            client.write_close(None, "").await,
            Err(WsError::AlreadyClosed)
        ));
    }

    #[tokio::test]
    async fn write_size_failures_never_touch_the_wire() {
        let (mut client, server) = make_connected().await;
        let limit = client.max_message_size();
        assert!(matches!(
            client.write_binary(&vec![0u8; limit + 1]).await,
            Err(WsError::MessageTooLarge(l)) if l == limit
        ));
        assert!(matches!(
            client.write_ping(&[0u8; 126]).await,
            Err(WsError::ControlTooLarge(MAX_CONTROL_PAYLOAD))
        ));
        assert!(matches!(
            client.write_pong(&[0u8; 126]).await,
            Err(WsError::ControlTooLarge(MAX_CONTROL_PAYLOAD))
        ));
        assert!(matches!(
            client.write_close(Some(1004), "").await,
            Err(WsError::Protocol("close-code-invalid"))
        ));
        assert!(matches!(
            client.write_close(Some(1005), "").await,
            Err(WsError::Protocol("close-code-invalid"))
        ));
        assert!(matches!(
            client.write_close(Some(1000), &"r".repeat(124)).await,
            Err(WsError::ControlTooLarge(MAX_CONTROL_PAYLOAD))
        ));
        assert_eq!(server_buffered(&server), 0, "nothing may reach the wire");
    }

    // ---- read path -----------------------------------------------------------

    #[tokio::test]
    async fn read_binary_and_text_messages() {
        let (mut client, mut server) = make_connected().await;
        feed(&mut server, &[0x82, 0x05, 1, 2, 3, 4, 5]).await;
        assert_eq!(client.read_message().await.unwrap(), WsMessage::Binary(vec![1, 2, 3, 4, 5]));

        feed(&mut server, &[0x81, 0x02, b'h', b'i']).await;
        assert_eq!(client.read_message().await.unwrap(), WsMessage::Text("hi".to_string()));

        feed(&mut server, &[0x8A, 0x00]).await;
        assert_eq!(client.read_message().await.unwrap(), WsMessage::Pong(vec![]));
    }

    #[tokio::test]
    async fn ping_is_auto_ponged_and_not_surfaced() {
        let (mut client, mut server) = make_connected().await;
        feed(&mut server, &[0x89, 0x02, 0xAA, 0xBB]).await; // ping "…"
        feed(&mut server, &[0x82, 0x01, 0x07]).await; // binary behind it
        assert_eq!(client.read_message().await.unwrap(), WsMessage::Binary(vec![0x07]));

        // The pong must already be on the wire: FIN|0x8A, masked, payload echo.
        let mut raw = vec![0u8; 2 + 4 + 2];
        read_some(&mut server, &mut raw).await.unwrap();
        assert_eq!(raw[0], 0x8A);
        assert_eq!(raw[1], 0x80 | 2);
        let mut payload = raw[6..].to_vec();
        apply_mask(&mut payload, [raw[2], raw[3], raw[4], raw[5]]);
        assert_eq!(payload, vec![0xAA, 0xBB]);
    }

    #[tokio::test]
    async fn close_is_echoed_and_further_reads_are_closed() {
        let (mut client, mut server) = make_connected().await;
        feed(&mut server, &[0x88, 0x05, 0x03, 0xE8, b'b', b'y', b'e']).await;
        assert_eq!(
            client.read_message().await.unwrap(),
            WsMessage::Closed(Some((1000, "bye".to_string())))
        );
        // Echo close with the same code (no reason).
        let mut raw = vec![0u8; 2 + 4 + 2];
        read_some(&mut server, &mut raw).await.unwrap();
        assert_eq!(raw[0], 0x88);
        let mut payload = raw[6..].to_vec();
        apply_mask(&mut payload, [raw[2], raw[3], raw[4], raw[5]]);
        assert_eq!(payload, vec![0x03, 0xE8]);
        assert!(client.is_peer_closed());
        assert!(matches!(client.read_message().await, Err(WsError::AlreadyClosed)));
    }

    #[tokio::test]
    async fn fragmented_data_message_is_reassembled() {
        let (mut client, mut server) = make_connected().await;
        feed(&mut server, &[0x02, 0x02, b'a', b'b']).await; // binary, FIN=0
        feed(&mut server, &[0x80, 0x02, b'c', b'd']).await; // continuation, FIN=1
        assert_eq!(
            client.read_message().await.unwrap(),
            WsMessage::Binary(b"abcd".to_vec())
        );
    }

    #[tokio::test]
    async fn server_frames_arriving_in_pieces_are_reassembled() {
        // One frame split across stream reads (fill_buffer path).
        let (mut client, mut server) = make_connected().await;
        feed(&mut server, &[0x82, 0x04]).await;
        feed(&mut server, &[9, 9]).await;
        feed(&mut server, &[9, 9]).await;
        assert_eq!(client.read_message().await.unwrap(), WsMessage::Binary(vec![9, 9, 9, 9]));
    }

    #[tokio::test]
    async fn eof_without_close_is_typed() {
        let (mut client, mut server) = make_connected().await;
        shutdown_io(&mut server).await;
        assert!(matches!(client.read_message().await, Err(WsError::Eof)));
    }

    /// Protocol-violation table: each bad byte sequence must produce the
    /// exact typed error (and never panic / never surface a message).
    #[tokio::test]
    async fn read_protocol_violations_are_typed() {
        const MAX: usize = crate::relay::MAX_MESSAGE_SIZE;
        let cases: Vec<(&str, Vec<u8>, WsErrorShape)> = vec![
            ("server-masked", vec![0x82, 0x81, 0, 0, 0, 0, 7], WsErrorShape::P("server-frame-masked")),
            ("rsv-set", vec![0xC2, 0x01, 0x00], WsErrorShape::P("rsv-set")),
            ("unknown-opcode", vec![0x83, 0x01, 0x00], WsErrorShape::P("unknown-opcode")),
            ("fragmented-control", vec![0x09, 0x00], WsErrorShape::P("fragmented-control")),
            (
                "control-too-long",
                vec![0x89, 0xFE, 0x00, 0x7E],
                WsErrorShape::P("control-too-long"),
            ),
            (
                "non-minimal-16",
                vec![0x82, 0xFE, 0x00, 0x05],
                WsErrorShape::P("non-minimal-length"),
            ),
            (
                "non-minimal-64",
                vec![0x82, 0xFF, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF],
                WsErrorShape::P("non-minimal-length"),
            ),
            (
                "length-top-bit",
                vec![0x82, 0xFF, 0x80, 0, 0, 0, 0, 0, 0, 0],
                WsErrorShape::P("length-top-bit"),
            ),
            (
                "oversize-16bit",
                vec![0x82, 0x7E, 0x22, 0x75], // unmasked, 16-bit len = 8821
                WsErrorShape::TooLarge(MAX),
            ),
            ("unexpected-continuation", vec![0x80, 0x01, 0x78], WsErrorShape::P("unexpected-continuation")),
            (
                "data-during-fragmentation",
                vec![0x02, 0x01, b'a', 0x82, 0x01, b'b'],
                WsErrorShape::P("data-during-fragmentation"),
            ),
            ("text-not-utf8", vec![0x81, 0x02, 0xFF, 0xFE], WsErrorShape::P("text-not-utf8")),
            ("close-payload-1", vec![0x88, 0x01, 0x03], WsErrorShape::P("close-payload-1")),
            ("close-code-1006", vec![0x88, 0x02, 0x03, 0xEE], WsErrorShape::P("close-code-invalid")),
            (
                "close-reason-utf8",
                vec![0x88, 0x03, 0x03, 0xE8, 0xFF],
                WsErrorShape::P("close-reason-utf8"),
            ),
        ];
        for (name, bytes, want) in cases {
            let (mut client, mut server) = make_connected().await;
            feed(&mut server, &bytes).await;
            let err = client.read_message().await.expect_err(&format!("{name} must fail"));
            match want {
                WsErrorShape::P(token) => assert!(
                    matches!(err, WsError::Protocol(t) if t == token),
                    "{name}: want Protocol({token}), got {err:?}"
                ),
                WsErrorShape::TooLarge(limit) => assert!(
                    matches!(err, WsError::MessageTooLarge(l) if l == limit),
                    "{name}: want MessageTooLarge({limit}), got {err:?}"
                ),
            }
        }
    }

    // ---- cancellation safety ------------------------------------------------
    // The real relay client wraps reads in tokio::time::timeout; a read
    // future dropped at the await point must leave the session byte-exact.
    // Deterministic: timeout(Duration::ZERO) is already expired on the first
    // poll, so the read is polled exactly once — it parks in fill_buffer —
    // and is then dropped. No sleeps, no real waiting.

    #[tokio::test]
    async fn read_cancelled_while_idle_leaves_no_zero_residue() {
        let (client_io, mut server) = duplex_pair();
        let mut client = WsClient::new(client_io, Vec::new());

        // Peer silent: the read parks inside fill_buffer and is cancelled
        // there by the already-elapsed zero timer.
        let res = tokio::time::timeout(std::time::Duration::ZERO, client.read_message()).await;
        assert!(res.is_err(), "silent peer must time out, not complete");
        // ① Nothing may be buffered for a cancelled read (the pre-fix code
        //    left 4096 zero bytes here).
        assert!(client.buffer.is_empty(), "zero-byte residue after cancel");

        // ② A complete valid frame afterwards must parse normally — the
        //    residue zeros used to be parsed as a continuation frame
        //    ("unexpected-continuation").
        feed(&mut server, &[0x82, 0x03, 7, 8, 9]).await;
        assert_eq!(client.read_message().await.unwrap(), WsMessage::Binary(vec![7, 8, 9]));
        // ③ Buffer fully consumed again — no residue of any kind.
        assert!(client.buffer.is_empty());
    }

    #[tokio::test]
    async fn read_cancelled_mid_frame_then_completed_parses_exactly() {
        let (client_io, mut server) = duplex_pair();
        let mut client = WsClient::new(client_io, Vec::new());

        // Header + one payload byte of a 4-byte binary frame: the first read
        // completes on these, the second parks waiting for the rest and is
        // cancelled there.
        feed(&mut server, &[0x82, 0x04, 0xA1]).await;
        let res = tokio::time::timeout(std::time::Duration::ZERO, client.read_message()).await;
        assert!(res.is_err(), "partial frame must leave the read pending");
        // Exactly the three arrived bytes — no duplication, no zero padding
        // (pre-fix the buffer grew by 4096 zeros here).
        assert_eq!(client.buffer, vec![0x82, 0x04, 0xA1], "buffer after mid-frame cancel");

        // The remainder arrives; the same message must come out byte-exact.
        feed(&mut server, &[0xB2, 0xC3, 0xD4]).await;
        assert_eq!(
            client.read_message().await.unwrap(),
            WsMessage::Binary(vec![0xA1, 0xB2, 0xC3, 0xD4])
        );
        assert!(client.buffer.is_empty());
    }

    enum WsErrorShape {
        P(&'static str),
        TooLarge(usize),
    }
}
