// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # relay — NetBird relay wire-format codec (N13 increment A)
//!
//! Byte-exact implementation of the relay client protocol frames
//! (`docs/relay-client-spec-20260914.md`, the sole implementation basis;
//! upstream anchor NetBird `791401060d2b`, `shared/relay/messages/message.go`).
//! Pure codec: no I/O, no clocks, no sockets — the WS transport lives in
//! [`crate::ws`], the connection state machine is a later increment.
//!
//! ## Wire shape (spec §2)
//!
//! Every frame = `[version 0x01][msg type 1B][body…]`. There are NO length
//! prefixes and NO multi-byte numeric fields (spec §2.1/§9.2): one relay
//! frame travels as exactly one binary WebSocket message (spec §2.6), so the
//! WS layer is what delimits frames — this codec only validates the sizes.
//!
//! | frame | type | body |
//! |---|---|---|
//! | Auth (C→S) | 6 | `[magic 4B][peerID 36B][token binary N B]` (§3.1) |
//! | AuthResponse (S→C) | 7 | `[instanceURL ASCII, ≥1B]` (§3.4) |
//! | Transport | 3 | `[peerID 36B][payload]` (§2.5; on the wire the 36B field is the SENDER id after the server rewrites it, §4.2) |
//! | Close | 4 | none (2B total) |
//! | HealthCheck | 5 | none (2B total) — oracle-confirmed live bytes `01 05` (`docs/n13-oracle-registration-20260914.md` §2) |
//! | Subscribe/Unsubscribe/PeersOnline/PeersWentOffline | 8/9/10/11 | `N × 36B peerID`, N ≥ 1, ≤ 244 ids/frame (§2.5) |
//!
//! Types 0/1/2 (Unknown/Hello/HelloResponse) are illegal wire values;
//! decoding one yields [`RelayError::UnknownType`] so the caller can decide
//! drop-and-continue (client behavior, `client.go:574-579`) vs abort.
//!
//! ## peerID (spec §3.2)
//!
//! `peerID = "sha-" (4 ASCII bytes) || SHA256(UTF8(wgPublicKey base64 STRING)) (32 raw bytes)`
//! — 36 RAW bytes on the wire, no base64 anywhere in the frame (spec §9.1).
//! The base64 form ("sha-" + base64 of the 32-byte hash) is the human-readable
//! form only ([`PeerId::readable`] / `Display`), mirroring upstream
//! `id.go:20-22` and the oracle log line `… for peer: sha-…`.
//!
//! SHA-256 comes from [`crate::util::sha256`] — the hand-written FIPS 180-4
//! implementation this crate already trusts for ledger digests. The N13 plan
//! named `ring::digest` ("依赖链已有"), but ring is only a TRANSITIVE
//! dependency here and the frozen manifest/lockfile may not change, so
//! `use ring::…` is impossible within this increment's file scope. Same
//! conclusion as the hand-written STUN codec (`stun.rs`): zero new crates,
//! correctness pinned by golden vectors (this module pins the peerID vector
//! cross-checked against the host `sha256sum`).
//!
//! ## Token (spec §3.3) — assembly only, no verification
//!
//! `[algo 0x01][raw HMAC-SHA256 signature 32B][ASCII payload]`. The client
//! base64-decodes management's `token_signature` (StdEncoding, padded) into
//! the raw 32 bytes and concatenates; it never computes or verifies the HMAC.
//!
//! ## Sensitive discipline
//!
//! The token signature and payload never appear in `Debug`/`Display`/logs —
//! [`AuthToken`]'s `Debug` prints shape/lengths only. Error payloads are
//! static shape tokens or lengths, never content. Tests use fabricated
//! values only.
//!
//! ## Size ceilings (spec §2.4) — enforced on encode AND decode
//!
//! Auth total ≤ 212, AuthResponse total ≤ 8192, every other frame ≤ 8820.
//! All failures are typed [`RelayError`] values; nothing here panics or
//! truncates silently (fail-closed, per the N13 increment plan).

use std::fmt;

// ---------------------------------------------------------------------------
// constants (spec §2.2/§2.3/§2.4; upstream shared/relay/messages/message.go)
// ---------------------------------------------------------------------------

/// Protocol version byte, first byte of every frame (`CurrentProtocolVersion`).
pub const PROTOCOL_VERSION: u8 = 0x01;

/// Frame type bytes (spec §2.2 full table). 0/1/2 are illegal on the wire
/// (Unknown / deleted Hello / HelloResponse) and never constructed here.
pub const MSG_TRANSPORT: u8 = 3;
pub const MSG_CLOSE: u8 = 4;
pub const MSG_HEALTH_CHECK: u8 = 5;
pub const MSG_AUTH: u8 = 6;
pub const MSG_AUTH_RESPONSE: u8 = 7;
pub const MSG_SUBSCRIBE_PEER_STATE: u8 = 8;
pub const MSG_UNSUBSCRIBE_PEER_STATE: u8 = 9;
pub const MSG_PEERS_ONLINE: u8 = 10;
pub const MSG_PEERS_WENT_OFFLINE: u8 = 11;

/// Auth-frame magic (`magicHeader`, `message.go:56`); appears ONLY in Auth.
pub const MAGIC: [u8; 4] = [0x21, 0x12, 0xA4, 0x42];

/// Auth frame total-length ceiling (`MaxHandshakeSize`).
pub const MAX_HANDSHAKE_SIZE: usize = 212;
/// AuthResponse total-length ceiling (`MaxHandshakeRespSize`).
pub const MAX_HANDSHAKE_RESP_SIZE: usize = 8192;
/// Runtime frame total-length ceiling (`MaxMessageSize`).
pub const MAX_MESSAGE_SIZE: usize = 8820;

/// peerID wire size = 4-byte `"sha-"` prefix + 32-byte SHA-256 (`id.go`).
pub const PEER_ID_SIZE: usize = 36;
/// peerID ASCII prefix (`prefixLength=4`, `id.go:9-18`).
pub const PEER_ID_PREFIX: &[u8; 4] = b"sha-";

/// 2-byte proto header shared by every frame.
pub const HEADER_LEN: usize = 2;

/// Peer-state frames carry `N × 36B` ids, chunked so a frame stays ≤ 8820
/// (`(8820-2)/36 = 244`, spec §2.5).
pub const MAX_PEERS_PER_FRAME: usize = (MAX_MESSAGE_SIZE - HEADER_LEN) / PEER_ID_SIZE;

/// Auth token PAYLOAD ceiling: `[algo 1B][sig 32B][payload M B]` inside
/// `2+4+36+len(token) ≤ 212` ⇒ M ≤ 137 (spec §2.6 arithmetic).
pub const AUTH_TOKEN_MAX_LEN: usize =
    MAX_HANDSHAKE_SIZE - HEADER_LEN - MAGIC.len() - PEER_ID_SIZE - 1 - TOKEN_SIGNATURE_LEN;

/// Auth token algorithm byte: `AuthAlgoHMACSHA256 = 1` (`v2/algo.go:8-11`).
pub const TOKEN_ALGO_HMAC_SHA256: u8 = 0x01;

/// Raw HMAC-SHA256 signature size (`v2/algo.go:33-39`).
pub const TOKEN_SIGNATURE_LEN: usize = 32;

// ---------------------------------------------------------------------------
// errors — static shape tokens / lengths only, never token content
// ---------------------------------------------------------------------------

/// Typed failure for every encode/decode path. Fail-closed: no panic, no
/// silent truncation, and no variant carries token content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayError {
    /// Buffer ends inside a fixed-layout region (`what` names the region).
    Truncated(&'static str),
    /// Header version byte ≠ [`PROTOCOL_VERSION`] (carries the observed byte).
    BadVersion(u8),
    /// Type byte is 0/1/2 or beyond the §2.2 table (carries the byte).
    UnknownType(u8),
    /// Auth magic ≠ [`MAGIC`].
    BadMagic,
    /// 36-byte id field malformed (`reason`: length / prefix).
    BadPeerId(&'static str),
    /// Peer-id list invalid (spec requires N ≥ 1, ids multiple of 36B).
    BadPeerList(&'static str),
    /// Token assembly/parse failure; `reason` is a static shape token.
    BadToken(&'static str),
    /// Decoded token signature is not exactly [`TOKEN_SIGNATURE_LEN`] bytes.
    TokenSignatureLength(usize),
    /// AuthResponse instance URL invalid (spec: ≥1 ASCII byte).
    BadInstanceUrl(&'static str),
    /// Body-less frame (Close/HealthCheck) carried trailing bytes.
    UnexpectedBody,
    /// Frame exceeds its ceiling (`limit` = the exact spec ceiling).
    FrameTooLarge { limit: usize, got: usize },
}

impl fmt::Display for RelayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelayError::Truncated(what) => write!(f, "relay frame truncated in {what}"),
            RelayError::BadVersion(v) => write!(f, "relay frame version {v:#04x} != 1"),
            RelayError::UnknownType(t) => write!(f, "relay frame type {t} not in §2.2 table"),
            RelayError::BadMagic => write!(f, "relay auth magic mismatch"),
            RelayError::BadPeerId(reason) => write!(f, "relay peer id invalid: {reason}"),
            RelayError::BadPeerList(reason) => write!(f, "relay peer list invalid: {reason}"),
            RelayError::BadToken(reason) => write!(f, "relay token invalid: {reason}"),
            RelayError::TokenSignatureLength(n) => {
                write!(f, "relay token signature is {n} bytes, want 32")
            }
            RelayError::BadInstanceUrl(reason) => write!(f, "relay instance url invalid: {reason}"),
            RelayError::UnexpectedBody => write!(f, "relay body-less frame carried extra bytes"),
            RelayError::FrameTooLarge { limit, got } => {
                write!(f, "relay frame is {got} bytes, ceiling {limit}")
            }
        }
    }
}

impl std::error::Error for RelayError {}

// ---------------------------------------------------------------------------
// PeerId
// ---------------------------------------------------------------------------

/// Raw 36-byte relay peer id: `"sha-"` + SHA-256 of the WireGuard public-key
/// base64 string (spec §3.2). Never base64 on the wire.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct PeerId([u8; PEER_ID_SIZE]);

impl PeerId {
    /// Derive the peer id from the WireGuard public key in its base64 STRING
    /// form (upstream `connect.go:425` hashes `PublicKey().String()`, i.e.
    /// the ASCII bytes of the base64 text — not the decoded key bytes).
    pub fn from_wg_pubkey_string(pub_key_b64: &str) -> Self {
        let hash = crate::util::sha256(pub_key_b64.as_bytes());
        let mut id = [0u8; PEER_ID_SIZE];
        id[..PEER_ID_PREFIX.len()].copy_from_slice(PEER_ID_PREFIX);
        id[PEER_ID_PREFIX.len()..].copy_from_slice(&hash);
        PeerId(id)
    }

    /// Adopt 36 wire bytes, validating length and the `"sha-"` prefix.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RelayError> {
        if bytes.len() != PEER_ID_SIZE {
            return Err(RelayError::BadPeerId("length"));
        }
        let mut id = [0u8; PEER_ID_SIZE];
        id.copy_from_slice(bytes);
        if &id[..PEER_ID_PREFIX.len()] != PEER_ID_PREFIX {
            return Err(RelayError::BadPeerId("prefix"));
        }
        Ok(PeerId(id))
    }

    /// Raw wire bytes.
    pub fn as_bytes(&self) -> &[u8; PEER_ID_SIZE] {
        &self.0
    }

    /// Human-readable form (`id.go:20-22`): `"sha-"` + base64(StdEncoding) of
    /// the 32-byte hash. FOR LOGS ONLY — never sent on the wire (spec §9.1).
    pub fn readable(&self) -> String {
        format!("sha-{}", crate::util::base64(&self.0[PEER_ID_PREFIX.len()..]))
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.readable())
    }
}

/// Log-shaped debug: `PeerId(sha-<base64>)` — the same form upstream logs
/// (`client.go:662`), not a raw 36-byte array dump.
impl fmt::Debug for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PeerId({})", self.readable())
    }
}

// ---------------------------------------------------------------------------
// AuthToken — assembly only; content never printed
// ---------------------------------------------------------------------------

/// Relay auth token in its wire binary form (spec §3.3):
/// `[algo 0x01][raw HMAC-SHA256 signature 32B][ASCII payload]`.
///
/// Assembled from management's `token_payload` (ASCII Unix-seconds string)
/// and `token_signature` (base64 StdEncoding, padded) exactly like upstream
/// `auth/hmac/store.go:18-38`. This type ASSEMBLES and length-checks only —
/// it neither computes nor verifies the HMAC (spec-conformant: the server
/// verifies at handshake; the client never sees the key).
#[derive(Clone, PartialEq, Eq)]
pub struct AuthToken {
    signature: [u8; TOKEN_SIGNATURE_LEN],
    payload: Vec<u8>,
}

impl AuthToken {
    /// Assemble from the management-relayed strings (`RelayServers`
    /// `token_payload`/`token_signature`, `network_map.rs`).
    ///
    /// Checks (fail-closed, spec §2.6/§3.3): signature base64-decodes to
    /// exactly 32 raw bytes; payload is non-empty ASCII and fits the Auth
    /// frame ceiling (≤ [`AUTH_TOKEN_MAX_LEN`] bytes).
    pub fn from_management(token_payload: &str, token_signature_b64: &str) -> Result<Self, RelayError> {
        use base64::Engine as _;
        let payload = token_payload.as_bytes();
        if payload.is_empty() {
            return Err(RelayError::BadToken("payload-empty"));
        }
        if !token_payload.is_ascii() {
            return Err(RelayError::BadToken("payload-not-ascii"));
        }
        if payload.len() > AUTH_TOKEN_MAX_LEN {
            return Err(RelayError::BadToken("payload-too-long"));
        }
        let sig = base64::engine::general_purpose::STANDARD
            .decode(token_signature_b64)
            .map_err(|_| RelayError::BadToken("signature-not-base64"))?;
        let signature: [u8; TOKEN_SIGNATURE_LEN] = sig
            .as_slice()
            .try_into()
            .map_err(|_| RelayError::TokenSignatureLength(sig.len()))?;
        Ok(AuthToken { signature, payload: payload.to_vec() })
    }

    /// Parse the token body of a received Auth frame: `[algo][32B sig][payload]`.
    pub fn from_wire(token_bytes: &[u8]) -> Result<Self, RelayError> {
        let fixed = 1 + TOKEN_SIGNATURE_LEN;
        if token_bytes.len() < fixed {
            return Err(RelayError::Truncated("auth-token"));
        }
        if token_bytes[0] != TOKEN_ALGO_HMAC_SHA256 {
            return Err(RelayError::BadToken("unknown-algo"));
        }
        let payload = &token_bytes[fixed..];
        if payload.is_empty() {
            return Err(RelayError::BadToken("payload-empty"));
        }
        if !payload.is_ascii() {
            return Err(RelayError::BadToken("payload-not-ascii"));
        }
        if payload.len() > AUTH_TOKEN_MAX_LEN {
            return Err(RelayError::BadToken("payload-too-long"));
        }
        let mut signature = [0u8; TOKEN_SIGNATURE_LEN];
        signature.copy_from_slice(&token_bytes[1..fixed]);
        Ok(AuthToken { signature, payload: payload.to_vec() })
    }

    /// Full wire bytes: `[algo 0x01][signature 32B][payload]`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(1 + TOKEN_SIGNATURE_LEN + self.payload.len());
        out.push(TOKEN_ALGO_HMAC_SHA256);
        out.extend_from_slice(&self.signature);
        out.extend_from_slice(&self.payload);
        out
    }

    /// Token wire length (`1 + 32 + payload`).
    pub fn wire_len(&self) -> usize {
        1 + TOKEN_SIGNATURE_LEN + self.payload.len()
    }
}

/// REDACTED debug (sensitive discipline): shape and lengths only — never the
/// signature bytes, payload bytes, or any encoding of them.
impl fmt::Debug for AuthToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthToken")
            .field("algo", &TOKEN_ALGO_HMAC_SHA256)
            .field("signature", &"<32 bytes redacted>")
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Frame
// ---------------------------------------------------------------------------

/// One complete relay protocol message (spec §2.5 layouts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// Client→server handshake frame (§3.1): first frame after WS connect.
    Auth { peer_id: PeerId, token: AuthToken },
    /// Server→server handshake success (§3.4): instance URL for cross-relay.
    AuthResponse { instance_url: String },
    /// Data forwarding (§4.2). On RECEIVE the 36B field is the SENDER peer id
    /// (the server rewrites dstID in place); on SEND it is the destination.
    Transport { peer_id: PeerId, payload: Vec<u8> },
    /// Graceful close of the WHOLE relay connection (2B total).
    Close,
    /// Health-check keepalive (2B total; oracle live bytes `01 05`).
    HealthCheck,
    /// `N × 36B` ids, 1..=[`MAX_PEERS_PER_FRAME`] (spec §2.5).
    SubscribePeerState { peer_ids: Vec<PeerId> },
    /// Same body shape as subscribe.
    UnsubscribePeerState { peer_ids: Vec<PeerId> },
    /// Server→client online notification.
    PeersOnline { peer_ids: Vec<PeerId> },
    /// Server→client offline notification.
    PeersWentOffline { peer_ids: Vec<PeerId> },
}

impl Frame {
    /// Frame type byte (§2.2).
    pub fn msg_type(&self) -> u8 {
        match self {
            Frame::Auth { .. } => MSG_AUTH,
            Frame::AuthResponse { .. } => MSG_AUTH_RESPONSE,
            Frame::Transport { .. } => MSG_TRANSPORT,
            Frame::Close => MSG_CLOSE,
            Frame::HealthCheck => MSG_HEALTH_CHECK,
            Frame::SubscribePeerState { .. } => MSG_SUBSCRIBE_PEER_STATE,
            Frame::UnsubscribePeerState { .. } => MSG_UNSUBSCRIBE_PEER_STATE,
            Frame::PeersOnline { .. } => MSG_PEERS_ONLINE,
            Frame::PeersWentOffline { .. } => MSG_PEERS_WENT_OFFLINE,
        }
    }

    /// Total wire length this frame will encode to (header included).
    pub fn wire_len(&self) -> Result<usize, RelayError> {
        match self {
            Frame::Auth { token, .. } => Ok(HEADER_LEN + MAGIC.len() + PEER_ID_SIZE + token.wire_len()),
            Frame::AuthResponse { instance_url } => Ok(HEADER_LEN + instance_url.len()),
            Frame::Transport { payload, .. } => Ok(HEADER_LEN + PEER_ID_SIZE + payload.len()),
            Frame::Close | Frame::HealthCheck => Ok(HEADER_LEN),
            Frame::SubscribePeerState { peer_ids }
            | Frame::UnsubscribePeerState { peer_ids }
            | Frame::PeersOnline { peer_ids }
            | Frame::PeersWentOffline { peer_ids } => {
                Ok(HEADER_LEN + PEER_ID_SIZE * peer_ids.len())
            }
        }
    }

    /// Encode to the exact wire bytes (§2.5). Size ceilings and shape rules
    /// are enforced here too — encoding is fail-closed, not a silent shim.
    pub fn encode(&self) -> Result<Vec<u8>, RelayError> {
        let mut out = Vec::new();
        out.push(PROTOCOL_VERSION);
        out.push(self.msg_type());
        match self {
            Frame::Auth { peer_id, token } => {
                let token_bytes = token.to_bytes();
                let total = out.len() + MAGIC.len() + PEER_ID_SIZE + token_bytes.len();
                if total > MAX_HANDSHAKE_SIZE {
                    return Err(RelayError::FrameTooLarge {
                        limit: MAX_HANDSHAKE_SIZE,
                        got: total,
                    });
                }
                out.extend_from_slice(&MAGIC);
                out.extend_from_slice(peer_id.as_bytes());
                out.extend_from_slice(&token_bytes);
            }
            Frame::AuthResponse { instance_url } => {
                if instance_url.is_empty() {
                    return Err(RelayError::BadInstanceUrl("empty"));
                }
                if !instance_url.is_ascii() {
                    return Err(RelayError::BadInstanceUrl("not-ascii"));
                }
                let total = out.len() + instance_url.len();
                if total > MAX_HANDSHAKE_RESP_SIZE {
                    return Err(RelayError::FrameTooLarge {
                        limit: MAX_HANDSHAKE_RESP_SIZE,
                        got: total,
                    });
                }
                out.extend_from_slice(instance_url.as_bytes());
            }
            Frame::Transport { peer_id, payload } => {
                let total = out.len() + PEER_ID_SIZE + payload.len();
                if total > MAX_MESSAGE_SIZE {
                    return Err(RelayError::FrameTooLarge { limit: MAX_MESSAGE_SIZE, got: total });
                }
                out.extend_from_slice(peer_id.as_bytes());
                out.extend_from_slice(payload);
            }
            Frame::Close | Frame::HealthCheck => {}
            Frame::SubscribePeerState { peer_ids }
            | Frame::UnsubscribePeerState { peer_ids }
            | Frame::PeersOnline { peer_ids }
            | Frame::PeersWentOffline { peer_ids } => {
                encode_peer_list(&mut out, peer_ids)?;
            }
        }
        Ok(out)
    }

    /// Decode one complete frame from a full WS message body (§2.6: one relay
    /// frame == one binary WS message, so `buf` must hold exactly one frame —
    /// the caller passes the message bytes it read from the WS layer).
    ///
    /// Unknown/illegal type bytes come back as [`RelayError::UnknownType`]
    /// (spec §2.2: a relay CLIENT drops unknown frames and keeps the
    /// connection; deciding that drop is the caller's job, not ours).
    pub fn decode(buf: &[u8]) -> Result<Frame, RelayError> {
        if buf.len() < HEADER_LEN {
            return Err(RelayError::Truncated("header"));
        }
        if buf[0] != PROTOCOL_VERSION {
            return Err(RelayError::BadVersion(buf[0]));
        }
        match buf[1] {
            MSG_AUTH => {
                if buf.len() > MAX_HANDSHAKE_SIZE {
                    return Err(RelayError::FrameTooLarge {
                        limit: MAX_HANDSHAKE_SIZE,
                        got: buf.len(),
                    });
                }
                let fixed = HEADER_LEN + MAGIC.len() + PEER_ID_SIZE;
                if buf.len() < fixed {
                    return Err(RelayError::Truncated("auth-fixed"));
                }
                if buf[HEADER_LEN..HEADER_LEN + MAGIC.len()] != MAGIC {
                    return Err(RelayError::BadMagic);
                }
                let peer_id = PeerId::from_bytes(&buf[HEADER_LEN + MAGIC.len()..fixed])?;
                let token = AuthToken::from_wire(&buf[fixed..])?;
                Ok(Frame::Auth { peer_id, token })
            }
            MSG_AUTH_RESPONSE => {
                if buf.len() > MAX_HANDSHAKE_RESP_SIZE {
                    return Err(RelayError::FrameTooLarge {
                        limit: MAX_HANDSHAKE_RESP_SIZE,
                        got: buf.len(),
                    });
                }
                if buf.len() <= HEADER_LEN {
                    return Err(RelayError::Truncated("auth-response"));
                }
                let url_bytes = &buf[HEADER_LEN..];
                if !url_bytes.is_ascii() {
                    return Err(RelayError::BadInstanceUrl("not-ascii"));
                }
                // spec §3.4: ≥1 byte, taken verbatim as the instance URL; the
                // upstream client does no URL validation (`client.go:537-542`).
                Ok(Frame::AuthResponse {
                    instance_url: String::from_utf8(url_bytes.to_vec())
                        .map_err(|_| RelayError::BadInstanceUrl("not-utf8"))?,
                })
            }
            MSG_TRANSPORT => {
                if buf.len() > MAX_MESSAGE_SIZE {
                    return Err(RelayError::FrameTooLarge { limit: MAX_MESSAGE_SIZE, got: buf.len() });
                }
                let fixed = HEADER_LEN + PEER_ID_SIZE;
                if buf.len() < fixed {
                    return Err(RelayError::Truncated("transport-fixed"));
                }
                let peer_id = PeerId::from_bytes(&buf[HEADER_LEN..fixed])?;
                Ok(Frame::Transport { peer_id, payload: buf[fixed..].to_vec() })
            }
            MSG_CLOSE => {
                expect_empty(buf)?;
                Ok(Frame::Close)
            }
            MSG_HEALTH_CHECK => {
                expect_empty(buf)?;
                Ok(Frame::HealthCheck)
            }
            MSG_SUBSCRIBE_PEER_STATE => Ok(Frame::SubscribePeerState {
                peer_ids: decode_peer_list(buf)?,
            }),
            MSG_UNSUBSCRIBE_PEER_STATE => Ok(Frame::UnsubscribePeerState {
                peer_ids: decode_peer_list(buf)?,
            }),
            MSG_PEERS_ONLINE => Ok(Frame::PeersOnline { peer_ids: decode_peer_list(buf)? }),
            MSG_PEERS_WENT_OFFLINE => Ok(Frame::PeersWentOffline {
                peer_ids: decode_peer_list(buf)?,
            }),
            other => Err(RelayError::UnknownType(other)),
        }
    }
}

/// Body-less frames (Close/HealthCheck) must be exactly 2 bytes.
fn expect_empty(buf: &[u8]) -> Result<(), RelayError> {
    if buf.len() > HEADER_LEN {
        return Err(RelayError::UnexpectedBody);
    }
    Ok(())
}

fn encode_peer_list(out: &mut Vec<u8>, peer_ids: &[PeerId]) -> Result<(), RelayError> {
    if peer_ids.is_empty() {
        return Err(RelayError::BadPeerList("empty"));
    }
    let total = out.len() + PEER_ID_SIZE * peer_ids.len();
    if total > MAX_MESSAGE_SIZE {
        return Err(RelayError::FrameTooLarge { limit: MAX_MESSAGE_SIZE, got: total });
    }
    for id in peer_ids {
        out.extend_from_slice(id.as_bytes());
    }
    Ok(())
}

fn decode_peer_list(buf: &[u8]) -> Result<Vec<PeerId>, RelayError> {
    if buf.len() > MAX_MESSAGE_SIZE {
        return Err(RelayError::FrameTooLarge { limit: MAX_MESSAGE_SIZE, got: buf.len() });
    }
    let body = &buf[HEADER_LEN..];
    if body.len() % PEER_ID_SIZE != 0 {
        return Err(RelayError::Truncated("peer-list"));
    }
    if body.is_empty() {
        return Err(RelayError::BadPeerList("empty"));
    }
    let mut ids = Vec::with_capacity(body.len() / PEER_ID_SIZE);
    for chunk in body.chunks_exact(PEER_ID_SIZE) {
        ids.push(PeerId::from_bytes(chunk)?);
    }
    Ok(ids)
}

// ---------------------------------------------------------------------------
// tests — golden bytes, vectors cross-checked against host tools
// (sha256sum / openssl), fabricated non-secret values only
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::hex_lower;

    // Fixed input for the peerID golden vector, produced independently of
    // this crate:
    //   key   = base64(bytes 01..20)         -> "AQID…HyA=" (44 chars)
    //   hash  = printf '%s' "$key" | sha256sum
    //         = b01198f80d7523505107539aebd55b08d1923bfd91f72483a644cfdf40c51142
    //   readable = "sha-" + base64(raw hash)  -> sha-sBGY+A11I1BRB1Oa69VbCNGSO/2R9ySDpkTP30DFEUI=
    const WG_PUBKEY_B64: &str = "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA=";
    const GOLDEN_HASH_HEX: &str =
        "b01198f80d7523505107539aebd55b08d1923bfd91f72483a644cfdf40c51142";
    const GOLDEN_READABLE: &str = "sha-sBGY+A11I1BRB1Oa69VbCNGSO/2R9ySDpkTP30DFEUI=";

    // Fabricated (NOT secret) token parts: 32×0xA5 signature, padded base64
    // "paWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaU=", ASCII payload
    // "1770000000" (spec §3.3 shape: Unix seconds).
    const SIG_B64: &str = "paWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaU=";

    fn golden_peer_id() -> PeerId {
        PeerId::from_wg_pubkey_string(WG_PUBKEY_B64)
    }

    /// Valid (prefixed) id with a distinct hash byte, for list fixtures.
    fn fixture_peer_id(h: u8) -> PeerId {
        let mut b = [0u8; PEER_ID_SIZE];
        b[..PEER_ID_PREFIX.len()].copy_from_slice(PEER_ID_PREFIX);
        b[PEER_ID_PREFIX.len()] = h;
        PeerId::from_bytes(&b).unwrap()
    }

    fn golden_token() -> AuthToken {
        AuthToken::from_management("1770000000", SIG_B64).expect("fabricated token is valid")
    }

    // ---- peerID derivation (golden) --------------------------------------

    #[test]
    fn peer_id_golden_vector_matches_independent_sha256() {
        let id = golden_peer_id();
        assert_eq!(id.as_bytes().len(), PEER_ID_SIZE);
        assert_eq!(hex_lower(id.as_bytes()), format!("7368612d{GOLDEN_HASH_HEX}"));
        assert_eq!(id.readable(), GOLDEN_READABLE);
        // Readable form is "sha-" + base64 of the hash only (44 chars).
        assert_eq!(id.readable().len(), 4 + 44);
    }

    #[test]
    fn peer_id_display_and_debug_use_readable_form() {
        let id = golden_peer_id();
        assert_eq!(format!("{id}"), GOLDEN_READABLE);
        assert_eq!(format!("{id:?}"), format!("PeerId({GOLDEN_READABLE})"));
    }

    #[test]
    fn peer_id_from_bytes_rejects_wrong_length_and_prefix() {
        assert_eq!(PeerId::from_bytes(&[0u8; 35]), Err(RelayError::BadPeerId("length")));
        assert_eq!(PeerId::from_bytes(&[0u8; 37]), Err(RelayError::BadPeerId("length")));
        let mut bad = [0u8; 36];
        bad[4..].copy_from_slice(&[7u8; 32]); // no "sha-" prefix
        assert_eq!(PeerId::from_bytes(&bad), Err(RelayError::BadPeerId("prefix")));
        assert!(PeerId::from_bytes(golden_peer_id().as_bytes()).is_ok());
    }

    // ---- token assembly (golden + failure paths) -------------------------

    #[test]
    fn token_golden_bytes_and_length() {
        let tok = golden_token();
        let bytes = tok.to_bytes();
        // [0x01][32 × 0xA5]["1770000000"] — exact layout, 35 bytes.
        let mut want = Vec::new();
        want.push(0x01);
        want.extend_from_slice(&[0xA5; 32]);
        want.extend_from_slice(b"1770000000");
        assert_eq!(bytes, want);
        assert_eq!(bytes.len(), 43, "1 algo + 32 sig + 10 payload");
        assert_eq!(tok.wire_len(), 43);
    }

    #[test]
    fn token_rejects_signature_lengths_around_32() {
        let sig31 = crate::util::base64(&[0xA5; 31]);
        let sig33 = crate::util::base64(&[0xA5; 33]);
        assert_eq!(
            AuthToken::from_management("1770000000", &sig31),
            Err(RelayError::TokenSignatureLength(31))
        );
        assert_eq!(
            AuthToken::from_management("1770000000", &sig33),
            Err(RelayError::TokenSignatureLength(33))
        );
    }

    #[test]
    fn token_rejects_non_base64_and_bad_payloads() {
        assert_eq!(
            AuthToken::from_management("1770000000", "!!!not-base64!!!"),
            Err(RelayError::BadToken("signature-not-base64"))
        );
        assert_eq!(
            AuthToken::from_management("", SIG_B64),
            Err(RelayError::BadToken("payload-empty"))
        );
        assert_eq!(
            AuthToken::from_management("1770000000😀", SIG_B64),
            Err(RelayError::BadToken("payload-not-ascii"))
        );
        let long_payload = "9".repeat(AUTH_TOKEN_MAX_LEN + 1);
        assert_eq!(
            AuthToken::from_management(&long_payload, SIG_B64),
            Err(RelayError::BadToken("payload-too-long"))
        );
    }

    #[test]
    fn token_wire_roundtrip_and_algo_check() {
        let tok = golden_token();
        let parsed = AuthToken::from_wire(&tok.to_bytes()).expect("roundtrip");
        assert_eq!(parsed, tok);

        // Algo byte ≠ 0x01 → explicit error (spec §3.3 enumerates from 0).
        let mut wrong = tok.to_bytes();
        wrong[0] = 0x02;
        assert_eq!(AuthToken::from_wire(&wrong), Err(RelayError::BadToken("unknown-algo")));

        // Truncated below the fixed 33 bytes → Truncated.
        assert_eq!(AuthToken::from_wire(&[0x01, 0xA5]), Err(RelayError::Truncated("auth-token")));
        // Exactly 33 bytes = missing payload.
        assert_eq!(
            AuthToken::from_wire(&tok.to_bytes()[..33]),
            Err(RelayError::BadToken("payload-empty"))
        );
    }

    // ---- sensitive discipline --------------------------------------------

    #[test]
    fn debug_output_never_contains_token_signature_or_payload() {
        let tok = golden_token();
        let frame = Frame::Auth { peer_id: golden_peer_id(), token: tok.clone() };
        for text in [
            format!("{tok:?}"),
            format!("{frame:?}"),
            format!("{:?}", Frame::decode(&frame.encode().unwrap()).unwrap()),
        ] {
            assert!(!text.contains(SIG_B64), "signature base64 leaked: {text}");
            assert!(!text.contains("1770000000"), "payload leaked: {text}");
            assert!(!text.contains("a5a5"), "signature hex leaked: {text}");
            assert!(text.contains("redacted"), "redaction marker missing: {text}");
        }
    }

    // ---- golden frames -----------------------------------------------------

    #[test]
    fn healthcheck_golden_bytes_are_0105() {
        // Oracle-pinned live bytes (docs/n13-oracle-registration §2): `01 05`.
        assert_eq!(Frame::HealthCheck.encode().unwrap(), vec![0x01, 0x05]);
        assert_eq!(Frame::decode(&[0x01, 0x05]).unwrap(), Frame::HealthCheck);
    }

    #[test]
    fn close_golden_bytes_are_0104() {
        assert_eq!(Frame::Close.encode().unwrap(), vec![0x01, 0x04]);
        assert_eq!(Frame::decode(&[0x01, 0x04]).unwrap(), Frame::Close);
    }

    #[test]
    fn transport_golden_layout_matches_oracle_prefix() {
        let frame = Frame::Transport {
            peer_id: golden_peer_id(),
            payload: b"\x01\x02\x03wg".to_vec(),
        };
        let bytes = frame.encode().unwrap();
        // Oracle live sample starts `01 03 73 68 61 2d` =
        // [ver=1][type=3]["sha-"+peerID…] (docs/n13-oracle-registration §2).
        assert_eq!(hex_lower(&bytes[..6]), "01037368612d");
        assert_eq!(&bytes[0], &0x01);
        assert_eq!(&bytes[1], &MSG_TRANSPORT);
        assert_eq!(&bytes[2..6], PEER_ID_PREFIX);
        assert_eq!(&bytes[2..38], golden_peer_id().as_bytes());
        assert_eq!(&bytes[38..], b"\x01\x02\x03wg");
        assert_eq!(bytes.len(), 2 + 36 + 5);
        assert_eq!(Frame::decode(&bytes).unwrap(), frame);
    }

    #[test]
    fn auth_golden_layout() {
        let frame = Frame::Auth { peer_id: golden_peer_id(), token: golden_token() };
        let bytes = frame.encode().unwrap();
        // [01][06][21 12 A4 42]["sha-"…][01][A5×32]["1770000000"]
        assert_eq!(hex_lower(&bytes[..8]), "01062112a4427368");
        assert_eq!(&bytes[2..6], &MAGIC);
        assert_eq!(&bytes[6..42], golden_peer_id().as_bytes());
        assert_eq!(&bytes[42..], &golden_token().to_bytes()[..]);
        assert_eq!(bytes.len(), 2 + 4 + 36 + 43, "header+magic+peer + token(43B)");
        assert_eq!(Frame::decode(&bytes).unwrap(), frame);
    }

    #[test]
    fn auth_response_golden_layout() {
        let frame = Frame::AuthResponse { instance_url: "rels://home.alfadb.cn:28443".to_string() };
        let bytes = frame.encode().unwrap();
        assert_eq!(&bytes[..2], &[0x01, 0x07]);
        assert_eq!(&bytes[2..], b"rels://home.alfadb.cn:28443");
        assert_eq!(Frame::decode(&bytes).unwrap(), frame);
    }

    #[test]
    fn peer_state_golden_layout_and_roundtrip() {
        let ids = vec![golden_peer_id(), fixture_peer_id(7)];
        for frame in [
            Frame::SubscribePeerState { peer_ids: ids.clone() },
            Frame::UnsubscribePeerState { peer_ids: ids.clone() },
            Frame::PeersOnline { peer_ids: ids.clone() },
            Frame::PeersWentOffline { peer_ids: ids.clone() },
        ] {
            let bytes = frame.encode().unwrap();
            assert_eq!(&bytes[0], &0x01);
            assert_eq!(&bytes[1], &frame.msg_type());
            assert_eq!(bytes.len(), 2 + 72);
            assert_eq!(&bytes[2..6], PEER_ID_PREFIX);
            assert_eq!(Frame::decode(&bytes).unwrap(), frame);
        }
    }

    #[test]
    fn all_table_types_have_spec_type_bytes() {
        assert_eq!(Frame::Transport { peer_id: golden_peer_id(), payload: vec![] }.msg_type(), 3);
        assert_eq!(Frame::Close.msg_type(), 4);
        assert_eq!(Frame::HealthCheck.msg_type(), 5);
        assert_eq!(Frame::Auth { peer_id: golden_peer_id(), token: golden_token() }.msg_type(), 6);
        assert_eq!(Frame::AuthResponse { instance_url: "x".into() }.msg_type(), 7);
        assert_eq!(Frame::SubscribePeerState { peer_ids: vec![golden_peer_id()] }.msg_type(), 8);
        assert_eq!(Frame::UnsubscribePeerState { peer_ids: vec![golden_peer_id()] }.msg_type(), 9);
        assert_eq!(Frame::PeersOnline { peer_ids: vec![golden_peer_id()] }.msg_type(), 10);
        assert_eq!(Frame::PeersWentOffline { peer_ids: vec![golden_peer_id()] }.msg_type(), 11);
    }

    #[test]
    fn zero_length_transport_roundtrips() {
        // Spec T5 includes 0-byte payload transport frames.
        let frame = Frame::Transport { peer_id: golden_peer_id(), payload: Vec::new() };
        assert_eq!(frame.encode().unwrap().len(), 38);
        assert_eq!(Frame::decode(&frame.encode().unwrap()).unwrap(), frame);
    }

    // ---- size ceilings (spec §2.4) ----------------------------------------

    #[test]
    fn auth_frame_ceiling_is_212_bytes() {
        // payload 137 → total exactly 212: allowed.
        let payload = "1".repeat(AUTH_TOKEN_MAX_LEN);
        let tok = AuthToken::from_management(&payload, SIG_B64).unwrap();
        let frame = Frame::Auth { peer_id: golden_peer_id(), token: tok };
        assert_eq!(frame.encode().unwrap().len(), MAX_HANDSHAKE_SIZE);

        // payload 138 → total 213: rejected with exact numbers.
        let payload = "1".repeat(AUTH_TOKEN_MAX_LEN + 1);
        assert_eq!(
            AuthToken::from_management(&payload, SIG_B64),
            Err(RelayError::BadToken("payload-too-long"))
        );
    }

    #[test]
    fn transport_frame_ceiling_is_8820_bytes() {
        let ok = Frame::Transport {
            peer_id: golden_peer_id(),
            payload: vec![0u8; MAX_MESSAGE_SIZE - 38],
        };
        assert_eq!(ok.encode().unwrap().len(), MAX_MESSAGE_SIZE);
        let over = Frame::Transport {
            peer_id: golden_peer_id(),
            payload: vec![0u8; MAX_MESSAGE_SIZE - 38 + 1],
        };
        assert_eq!(
            over.encode(),
            Err(RelayError::FrameTooLarge { limit: MAX_MESSAGE_SIZE, got: MAX_MESSAGE_SIZE + 1 })
        );
    }

    #[test]
    fn auth_response_ceiling_is_8192_bytes() {
        let ok = Frame::AuthResponse { instance_url: "a".repeat(MAX_HANDSHAKE_RESP_SIZE - 2) };
        assert_eq!(ok.encode().unwrap().len(), MAX_HANDSHAKE_RESP_SIZE);
        let over = Frame::AuthResponse { instance_url: "a".repeat(MAX_HANDSHAKE_RESP_SIZE - 1) };
        assert_eq!(
            over.encode(),
            Err(RelayError::FrameTooLarge {
                limit: MAX_HANDSHAKE_RESP_SIZE,
                got: MAX_HANDSHAKE_RESP_SIZE + 1
            })
        );
    }

    #[test]
    fn peer_list_capacity_is_244_ids_per_frame() {
        let ids: Vec<PeerId> = (0..MAX_PEERS_PER_FRAME)
            .map(|i| fixture_peer_id(i as u8))
            .collect();
        assert_eq!(MAX_PEERS_PER_FRAME, 244);
        let bytes = Frame::SubscribePeerState { peer_ids: ids.clone() }.encode().unwrap();
        assert_eq!(bytes.len(), 2 + 244 * 36); // 8786 ≤ 8820
        assert_eq!(Frame::decode(&bytes).unwrap(), Frame::SubscribePeerState { peer_ids: ids.clone() });

        // 245 ids would exceed the 8820 ceiling (2 + 245*36 = 8822).
        let mut too_many = ids;
        too_many.push(golden_peer_id());
        assert_eq!(
            Frame::SubscribePeerState { peer_ids: too_many }.encode(),
            Err(RelayError::FrameTooLarge { limit: MAX_MESSAGE_SIZE, got: 2 + 245 * 36 })
        );
    }

    // ---- decode failure paths (fail-closed, no panics) ---------------------

    #[test]
    fn decode_rejects_bad_version_and_short_header() {
        assert_eq!(Frame::decode(&[]), Err(RelayError::Truncated("header")));
        assert_eq!(Frame::decode(&[0x01]), Err(RelayError::Truncated("header")));
        assert_eq!(Frame::decode(&[0x00, 0x05]), Err(RelayError::BadVersion(0)));
        assert_eq!(Frame::decode(&[0x02, 0x05]), Err(RelayError::BadVersion(2)));
    }

    #[test]
    fn decode_rejects_unknown_and_legacy_types() {
        for t in [0u8, 1, 2, 12, 200, 255] {
            assert_eq!(Frame::decode(&[0x01, t]), Err(RelayError::UnknownType(t)));
        }
    }

    #[test]
    fn decode_rejects_bad_magic_and_truncated_auth() {
        let mut bytes = Frame::Auth { peer_id: golden_peer_id(), token: golden_token() }
            .encode()
            .unwrap();
        bytes[2] ^= 0xFF;
        assert_eq!(Frame::decode(&bytes), Err(RelayError::BadMagic));

        let full = Frame::Auth { peer_id: golden_peer_id(), token: golden_token() }
            .encode()
            .unwrap();
        for cut in [2usize, 5, 41, 42] {
            let res = Frame::decode(&full[..cut]);
            assert!(matches!(res, Err(RelayError::Truncated(_))), "cut={cut} got {res:?}");
        }
        // 42 + 33 fixed token bytes, payload missing → explicit empty-payload.
        assert_eq!(
            Frame::decode(&full[..75]),
            Err(RelayError::BadToken("payload-empty"))
        );
    }

    #[test]
    fn decode_rejects_bad_peer_id_bytes() {
        // Transport whose id lacks the "sha-" prefix.
        let mut bytes = vec![0x01, 0x03];
        bytes.extend_from_slice(&[9u8; 36]);
        bytes.extend_from_slice(b"payload");
        assert_eq!(Frame::decode(&bytes), Err(RelayError::BadPeerId("prefix")));
        // Truncated 36B field.
        assert!(matches!(
            Frame::decode(&vec![0x01, 0x03].into_iter().chain(0..30).collect::<Vec<u8>>()),
            Err(RelayError::Truncated("transport-fixed"))
        ));
    }

    #[test]
    fn decode_rejects_malformed_auth_response() {
        assert_eq!(Frame::decode(&[0x01, 0x07]), Err(RelayError::Truncated("auth-response")));
        let mut long = vec![0x01, 0x07];
        long.extend_from_slice(&[b'a'; MAX_HANDSHAKE_RESP_SIZE]);
        assert_eq!(
            Frame::decode(&long),
            Err(RelayError::FrameTooLarge {
                limit: MAX_HANDSHAKE_RESP_SIZE,
                got: 2 + MAX_HANDSHAKE_RESP_SIZE
            })
        );
        // High bytes are not ASCII → explicit error.
        let mut non_ascii = vec![0x01, 0x07];
        non_ascii.extend_from_slice(&[0xE2, 0x98, 0x83]);
        assert_eq!(Frame::decode(&non_ascii), Err(RelayError::BadInstanceUrl("not-ascii")));
    }

    #[test]
    fn decode_rejects_bodyless_frames_with_extra_bytes() {
        assert_eq!(
            Frame::decode(&[0x01, 0x05, 0x00]),
            Err(RelayError::UnexpectedBody)
        );
        assert_eq!(
            Frame::decode(&[0x01, 0x04, 0xAA, 0xBB]),
            Err(RelayError::UnexpectedBody)
        );
    }

    #[test]
    fn decode_rejects_bad_peer_lists() {
        // Empty list (N=0) is illegal (spec §2.5: N≥1).
        assert_eq!(Frame::decode(&[0x01, 0x08]), Err(RelayError::BadPeerList("empty")));
        // Partial 36B id.
        let mut bytes = vec![0x01, 0x0A];
        bytes.extend_from_slice(&[0u8; 35]);
        assert_eq!(Frame::decode(&bytes), Err(RelayError::Truncated("peer-list")));
        // One bad id poisons the whole list.
        let mut bytes = vec![0x01, 0x0A];
        bytes.extend_from_slice(&[0u8; 36]);
        bytes.extend_from_slice(golden_peer_id().as_bytes());
        assert_eq!(Frame::decode(&bytes), Err(RelayError::BadPeerId("prefix")));
    }

    // ---- random-noise never panics (table style, cf. stun.rs) --------------

    #[test]
    fn decode_never_panics_on_arbitrary_noise() {
        // Deterministic pseudo-noise sweep over short buffers.
        let mut x: u32 = 0x1234_5678;
        for len in 0..=90usize {
            let mut buf = Vec::with_capacity(len);
            for _ in 0..len {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                buf.push((x >> 24) as u8);
            }
            // Either decodes or errors — never panics.
            let _ = Frame::decode(&buf);
        }
    }

    #[test]
    fn error_display_is_shape_only() {
        // Display strings never carry token content by construction; assert a
        // representative sample renders and stays short/static.
        assert!(RelayError::BadToken("payload-empty").to_string().contains("payload-empty"));
        assert!(RelayError::TokenSignatureLength(31).to_string().contains('3'));
        assert!(RelayError::FrameTooLarge { limit: 212, got: 213 }.to_string().contains("213"));
    }
}
