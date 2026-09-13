// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # ice_session — ICE connectivity checks: credentials, trickle, pairs,
//! nomination, keepalive (N5b)
//!
//! The ICE SESSION half: short-term credentials (`IceCredentials`,
//! `ufrag:pwd`), remote candidate trickle (signal `CANDIDATE` payloads),
//! Binding Request/Response connectivity checks with REAL MESSAGE-INTEGRITY
//! (HMAC-SHA1) and FINGERPRINT (CRC32), the RFC 8445 pair state machine,
//! regular nomination → selected pair, and keepalive/disconnect timers with
//! an INJECTED clock (`run_once(now_ms)` — no internal wall clock anywhere,
//! tests never sleep).
//!
//! NOT in this increment (N5c boundary): WireGuard endpoint configuration
//! (`ConfigureWGEndpoint`, `conn.go:476-478`), the connector loop that turns
//! a selected pair into a tunnel, TURN/relay candidates, full prflx
//! handling (minimal subset only — see below), IPv6, mDNS.
//!
//! ## Upstream anchors (pinned commit `791401060d2b`)
//!
//! - **Credentials**: `client/internal/peer/ice/agent.go:114-123`
//!   `GenerateICECredentials` = `randutil.GenerateCryptoRandomString(16,
//!   runesAlpha)` for the ufrag and `(32, runesAlpha)` for the pwd
//!   (`agent.go:16-17` `lenUFrag=16`/`lenPwd=32`, `runesAlpha` = A-Za-z);
//!   the payload rides signal as `"ufrag:pwd"` (`shared/signal/client/
//!   client.go:74-101`, payload at L77; parse at L60-71 — exactly two
//!   `:`-separated fields). RFC 8445 §16 floor: ufrag ≥ 4, pwd ≥ 22
//!   characters (pion enforces the same in `NewAgent`); local ceiling 256
//!   (RFC 8839 §16 `ice-char` budget). [`IceCredentials::generate`] mirrors
//!   the 16/32 runesAlpha rule; [`IceCredentials::validate`] enforces the
//!   floor, the charset and the ceiling.
//! - **Check packet shape** (RFC 8445 §7.1.2, RFC 5389 §15.4/§15.5):
//!   Binding Request with USERNAME `"remote-ufrag:local-ufrag"` (§7.1.2.1),
//!   PRIORITY = local candidate's prflx-type priority (§7.1.2.2), one of
//!   ICE-CONTROLLING/ICE-CONTROLLED carrying the 64-bit tie-breaker
//!   (§7.1.2.3, random per §5.1.3.1), USE-CANDIDATE on nomination checks
//!   only (§7.1.2.4/§8.1.1), MESSAGE-INTEGRITY keyed with the REMOTE peer's
//!   password (§7.2.2: short-term credential — the key is "the password
//!   for the remote agent"; the receiver validates with its own password
//!   and answers with the same), FINGERPRINT last. MI input = message up
//!   to (not including) the MI attribute with the header length field
//!   pointing at the END of the MI attribute (RFC 5389 §15.4 — WITHOUT the
//!   FINGERPRINT length; RFC 5769 §2.1 transmits 0x58 but HMACs over
//!   0x50); FINGERPRINT input = message up to the FP attribute with the
//!   header length field equal to the FULL transmitted length INCLUDING
//!   the FP attribute (§15.5; §2.1 CRC-input = 0x58), CRC-32 XORed with
//!   0x5354554e ("STUN"). Both
//!   layouts are pinned byte-exact by the RFC 5769 §2.1/§2.2 vectors
//!   (tests/ice_session_codec.rs).
//! - **Timers**: keepalive 4s, disconnected 6s, failed 6s — upstream
//!   `agent.go:22-24` (`iceKeepAliveDefault`/`iceDisconnectedTimeoutDefault`/
//!   `iceFailedTimeoutDefault`), wired into the agent at `agent.go:61-63`.
//!   Two-stage semantics: >6s without valid inbound STUN → Disconnected;
//!   6s more → Failed. Keepalives are Binding Requests re-keyed like checks
//!   (pion agent keepalive shape).
//! - **Retransmit**: RTO 500ms (RFC 5389 §7.2.1 default), 7 attempts per
//!   transaction (RFC 5245 §16 `Rc` default), then the pair is Failed
//!   (RFC 8445 §7.2.5.2.3).
//! - **Pair state machine** (RFC 8445 §6.1.2.6/§8.1.2): Frozen → Waiting →
//!   InProgress → Succeeded | Failed. Frozen here = pair formed but the
//!   session not started (single component, single check list — the
//!   multi-list thaw of §6.1.1 never applies). Checks run serially in pair-
//!   priority order (`Ta` pacing = one outstanding check per pump), pair
//!   priority = RFC 8445 §6.1.2.3 `2^32*MIN + 2*MAX + (G>D)`.
//! - **Nomination**: regular nomination (RFC 8445 §8.1.1) — the CONTROLLING
//!   agent re-runs the check of the best Succeeded pair with USE-CANDIDATE;
//!   the pair is SELECTED when that nomination check succeeds. The
//!   CONTROLLED agent selects a pair when a valid check carrying
//!   USE-CANDIDATE succeeds on it (§7.3.1.5/§8.2). First-success nomination
//!   (checks are serial by priority, so the first success is the best
//!   surviving pair) — the pion regular-nomination shape, grace-free for
//!   determinism.
//! - **Role conflict** (RFC 8445 §7.3.1.1): controlling + inbound
//!   ICE-CONTROLLING → our tie-breaker ≥ theirs: 487 and RETAIN role;
//!   smaller: switch to controlled. Controlled + inbound ICE-CONTROLLED →
//!   our tie-breaker ≥ theirs: switch to controlling; smaller: 487 and
//!   retain. On receiving 487, switch roles, re-trigger the pair, and
//!   change the tie-breaker (§7.2.5.1). Role change recomputes pair
//!   priorities (§6.1.2.3 is role-dependent).
//! - **prflx (minimal subset)**: an otherwise-valid check from an unknown
//!   source address creates a peer-reflexive REMOTE candidate with the
//!   request's PRIORITY (RFC 8445 §7.3.1.3) and pairs it (triggered check,
//!   §7.3.1.4). NOT done: prflx LOCAL candidates from XOR-MAPPED-ADDRESS
//!   asymmetry on responses (§7.2.5.3.1) — loopback/NB-socket topologies
//!   here always match a known candidate; left for N5c if a real topology
//!   needs it.
//!
//! ## Crypto dependency choice (probed, documented)
//!
//! MESSAGE-INTEGRITY needs HMAC-SHA1, FINGERPRINT needs CRC-32. The task's
//! preferred option ① (`sha1` crate + hand-rolled CRC32) is OFFLINE-DEAD in
//! this environment: `sha1` is not in the project's cargo registry cache
//! (`ls $CARGO_HOME/registry/cache/* | grep sha1` → empty, while
//! `hmac-0.12.1.crate` and `digest-0.10.7.crate` ARE present), so
//! `cargo build --offline --locked` cannot resolve it — the offline/locked
//! chain is a hard requirement. `hmac` 0.12 alone is useless without a
//! SHA-1 (and wiring a hand-rolled core into `digest` traits is more code
//! than SHA-1 itself). Chosen: option ② — fully self-written SHA-1
//! (RFC 3174, ~50 lines), HMAC-SHA1 (RFC 2104, ~20 lines), CRC-32 IEEE
//! (reflected 0xEDB88320, ~12 lines). Zero new dependencies, zero lockfile
//! changes, zero new FFI/link surface (pure `core` code — the cross-compile
//! surface is re-verified by `client/core/build.sh` step 4 itself; no
//! separate probe needed since no dependency was added). Correctness is
//! pinned by RFC 2202 HMAC-SHA1 vectors, the SHA-1("abc") vector, the
//! CRC-32 check value, and the RFC 5769 MI/FINGERPRINT bytes.
//!
//! ## Sockets (governance §二.4, fail-closed)
//!
//! Every socket is taken from [`crate::ice::UdpSocketSource`] (production:
//! [`crate::ice::ProtectedUdpFdSource`]) — take_fd → dup → O_NONBLOCK →
//! bind(candidate addr) → send/recv on the dup only → close exactly the
//! dup. Empty provider → [`ManagementError::Network`] fail-closed; there is
//! NO unprotected fallback and no `socket(2)` call anywhere in this module.
//! The provided fd number stays borrowed (never read/written/closed here),
//! the mgmtsock/ice borrow contract.
//!
//! ## N11 — WG rides the selected socket: packet demux (upstream shape)
//!
//! Upstream NetBird has ONE UDP socket serve both ICE and WireGuard: the WG
//! bind owns the socket and demuxes on receive — WireGuard-shaped or
//! non-STUN packets go to WG, STUN goes to the ICE mux
//! (`client/iface/bind/ice_bind.go:313-345`, WG-first classification
//! `isWireGuardMsg` at `ice_bind.go:403-421` so a WG receiver index that
//! happens to equal the STUN magic cookie can never misroute data into the
//! STUN handler). This module reproduces exactly that demux on ITS socket
//! reads: [`is_wg_datagram`] first, then STUN ([`is_stun_message`] +
//! [`parse_stun`]); everything else is non-STUN data and is queued for the
//! WG data plane via [`IceSession::take_data_rx`]. The WG device reads
//! NOTHING from the selected socket — this session is the sole reader, so
//! keepalives/checks and WG transport packets coexist without stealing from
//! each other. Egress rides the same socket: the orchestrator dups the
//! selected local fd ([`IceSession::selected_local_fd`]) into the WG device
//! as the peer's send socket (each layer closes only its own dup — the fd
//! contract is unchanged).
//!
//! ## Error taxonomy
//!
//! No new error class (the six [`ManagementError`] variants suffice):
//! socket/entropy failures are [`ManagementError::Network`]; malformed
//! candidate payloads / credentials are [`ManagementError::Parse`] /
//! [`ManagementError::Request`]; retransmit exhaustion surfaces as pair
//! state + `IceEvent::Failed`, not as a returned error. Inauthentic or
//! malformed CHECK TRAFFIC is not an "error" at all — it is per-datagram
//! silence (RFC 5389: drop on bad FINGERPRINT/integrity), observable only
//! through the pair state machine, which is what makes the poison-datagram
//! tests meaningful.

use std::collections::VecDeque;

use crate::ice::{priority_for, seam_to_management, set_nonblock, Candidate, CandidateType, UdpSocketSource};
use crate::management::ManagementError;
use crate::mgmtsock::dup_socket_fd;
use crate::stun::{
    self, decode_address, TransactionId, BINDING_ERROR, BINDING_REQUEST, BINDING_SUCCESS,
    HEADER_LEN, MAGIC_COOKIE,
};
use crate::sys;

// ---------------------------------------------------------------------------
// timers (upstream agent.go:22-24; RFC 5245 §16 Rc)
// ---------------------------------------------------------------------------

/// Upstream `iceKeepAliveDefault` (`client/internal/peer/ice/agent.go:22`).
pub const KEEPALIVE_INTERVAL_MS: u64 = 4000;
/// Upstream `iceDisconnectedTimeoutDefault` (`agent.go:23`).
pub const DISCONNECTED_TIMEOUT_MS: u64 = 6000;
/// Upstream `iceFailedTimeoutDefault` (`agent.go:24`) — counted AFTER the
/// disconnected transition, so Failed lands at disconnected + failed.
pub const FAILED_TIMEOUT_MS: u64 = 6000;
/// Per-transaction retransmission interval (RFC 5389 §7.2.1 default RTO;
/// pion default rto = 500ms).
pub const CHECK_RTO_MS: u64 = 500;
/// Attempts per transaction before the pair is Failed
/// (RFC 5245 §16 `Rc` default = 7).
pub const MAX_CHECK_ATTEMPTS: u8 = 7;

// ---------------------------------------------------------------------------
// STUN attributes used by connectivity checks (RFC 5389 §18.2, RFC 8445 §16.3)
// ---------------------------------------------------------------------------

pub const ATTR_USERNAME: u16 = 0x0006;
pub const ATTR_MESSAGE_INTEGRITY: u16 = 0x0008;
pub const ATTR_ERROR_CODE: u16 = 0x0009;
pub const ATTR_PRIORITY: u16 = 0x0024;
pub const ATTR_USE_CANDIDATE: u16 = 0x0025;
pub const ATTR_FINGERPRINT: u16 = 0x8028;
pub const ATTR_ICE_CONTROLLED: u16 = 0x8029;
pub const ATTR_ICE_CONTROLLING: u16 = 0x802A;
/// RFC 5389 §15.5: CRC-32 XOR the ASCII "STUN".
const FINGERPRINT_XOR: u32 = 0x5354_554e;
/// Binding error responses we emit/honor: 401 (bad short-term credential,
/// RFC 5389 §10.1.2) and 487 (role conflict, RFC 8445 §7.2.5.1/§7.3.1.1).
const ERR_UNAUTHORIZED: u16 = 401;
pub const ERR_ROLE_CONFLICT: u16 = 487;

// ---------------------------------------------------------------------------
// N11 packet classification — WG vs STUN on a shared socket (upstream
// client/iface/bind/ice_bind.go demux, pinned commit 791401060d2b)
// ---------------------------------------------------------------------------

/// `wgMsgTypeHandshakeInitiation` — the lowest WireGuard message type
/// (ice_bind.go:25-27).
const WG_MSG_TYPE_MIN: u32 = 1;
/// `wgMsgTypeTransport` — the highest WireGuard message type
/// (ice_bind.go:28-30).
const WG_MSG_TYPE_MAX: u32 = 4;
/// `wgMinMsgSize` — the smallest WG message: an empty-payload transport
/// packet (keepalive), 32 bytes (ice_bind.go:31-33).
pub const WG_MIN_MSG_SIZE: usize = 32;

/// Upstream `isWireGuardMsg` (ice_bind.go:403-421): a little-endian u32
/// message type in 1..=4, in a packet long enough to hold any WG message.
/// Deliberately checked BEFORE the STUN classifier: a WG transport packet's
/// receiver index (bytes 4..8) can coincide with the STUN magic cookie, and
/// `stun.IsMessage` looks only at the cookie — the WG-first order is what
/// keeps such a session's data from being misrouted into the STUN handler
/// (ice_bind.go:408-420 comment).
pub fn is_wg_datagram(pkt: &[u8]) -> bool {
    if pkt.len() < WG_MIN_MSG_SIZE {
        return false;
    }
    let msg_type = u32::from_le_bytes([pkt[0], pkt[1], pkt[2], pkt[3]]);
    (WG_MSG_TYPE_MIN..=WG_MSG_TYPE_MAX).contains(&msg_type)
}

/// pion `stun.IsMessage` shape (ice_bind.go:320 consumes it): at least a
/// STUN header, magic cookie at bytes 4..8, and the top two bits of the
/// first byte clear (RFC 5389 §5 — the method space). A WG packet's first
/// byte is also < 0xC0, which is exactly why [`is_wg_datagram`] must run
/// first.
pub(crate) fn is_stun_message(pkt: &[u8]) -> bool {
    pkt.len() >= HEADER_LEN
        && u32::from_be_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]) == MAGIC_COOKIE
        && (pkt[0] & 0xC0) == 0
}

/// The upstream demux predicate verbatim (`filterOutStunMessages`,
/// ice_bind.go:319: `isWireGuardMsg(pkt) || !stun.IsMessage(pkt)` → hand to
/// WireGuard): true = this datagram belongs to the WG data plane, false = it
/// is STUN and goes to the ICE machinery. The WG-shaped disjunct evaluated
/// FIRST is what keeps a WG receiver index coinciding with the magic cookie
/// from misrouting data into the STUN handler (ice_bind.go:408-420).
pub(crate) fn demux_routes_to_wg(pkt: &[u8]) -> bool {
    is_wg_datagram(pkt) || !is_stun_message(pkt)
}

// ---------------------------------------------------------------------------
// credentials
// ---------------------------------------------------------------------------

/// ICE short-term credentials (RFC 8445 §16): ufrag + pwd, both
/// `ice-char`-only strings carried in the signal payload as `"ufrag:pwd"`
/// (upstream `shared/signal/client/client.go:74-101`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IceCredentials {
    pub ufrag: String,
    pub pwd: String,
}

/// `runesAlpha` from upstream (`agent.go:18`) — the alphabet
/// `randutil.GenerateCryptoRandomString` draws from.
const RUNES_ALPHA: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

impl IceCredentials {
    /// Upstream `GenerateICECredentials` (`agent.go:114-123`): ufrag 16 and
    /// pwd 32 characters, uniform over `runesAlpha`. Entropy comes from
    /// [`stun::random_transaction_id`] (`/dev/urandom`, fixed path,
    /// fail-closed on I/O errors — no PRNG fallback). Rejection sampling
    /// (accept bytes < 208 = 4×52) keeps the distribution uniform over the
    /// 52-character alphabet.
    pub fn generate() -> Result<Self, ManagementError> {
        let draw = |len: usize| -> Result<String, ManagementError> {
            let mut out = String::with_capacity(len);
            while out.len() < len {
                let txn = stun::random_transaction_id()?;
                for b in txn.0 {
                    if (b as usize) < 208 && out.len() < len {
                        out.push(RUNES_ALPHA[(b % 52) as usize] as char);
                    }
                }
            }
            Ok(out)
        };
        let ufrag = draw(16)?;
        let pwd = draw(32)?;
        Ok(IceCredentials { ufrag, pwd })
    }

    /// RFC 8445 §16 floors (ufrag ≥ 4, pwd ≥ 22; pion's `NewAgent` enforces
    /// the same), RFC 8839 §16 charset (ALPHA/DIGIT/"+"/"/"), local ceiling
    /// 256 (USERNAME/PASSWORD wire budget). A generated credential always
    /// passes; the check exists for REMOTE credentials arriving from the
    /// signal channel and for locally configured ones.
    pub fn validate(&self) -> Result<(), ManagementError> {
        let bad = |what: &str, why: String| {
            ManagementError::Request { status: 0, message: format!("ice-credentials: {what} {why}") }
        };
        for (what, value, min) in [("ufrag", &self.ufrag, 4usize), ("pwd", &self.pwd, 22)] {
            if value.len() < min {
                return Err(bad(what, format!("must be at least {min} characters")));
            }
            if value.len() > 256 {
                return Err(bad(what, "must be at most 256 characters".into()));
            }
            if !value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/') {
                return Err(bad(what, "must contain only ice-chars (ALPHA/DIGIT/+//)".into()));
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// crypto: SHA-1 (RFC 3174), HMAC-SHA1 (RFC 2104), CRC-32 (IEEE 802.3)
// ---------------------------------------------------------------------------

/// SHA-1 digest (RFC 3174). Self-written — see module docs for the
/// dependency probe outcome (`sha1` crate not in the offline cache).
fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
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
            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
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

/// HMAC-SHA1 (RFC 2104): H((K ⊕ opad) ∥ H((K ⊕ ipad) ∥ m)). Keys longer
/// than 64 bytes are hashed first; ICE passwords are ≤ 256 bytes so that
/// path exists but is rarely taken.
fn hmac_sha1(key: &[u8], msg: &[u8]) -> [u8; 20] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..20].copy_from_slice(&sha1(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Vec::with_capacity(64 + msg.len());
    for b in k.iter() {
        inner.push(b ^ 0x36);
    }
    inner.extend_from_slice(msg);
    let mut outer = Vec::with_capacity(64 + 20);
    for b in k.iter() {
        outer.push(b ^ 0x5c);
    }
    outer.extend_from_slice(&sha1(&inner));
    sha1(&outer)
}

/// CRC-32 (IEEE 802.3, reflected, poly 0xEDB88320) — bitwise, no table
/// (~12 lines; called once per message, performance irrelevant).
fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

// ---------------------------------------------------------------------------
// check message codec (build + parse + verify)
// ---------------------------------------------------------------------------

/// Parameters of one outbound connectivity-check Binding Request
/// (RFC 8445 §7.1.2). `username` = `"remote-ufrag:local-ufrag"` (§7.1.2.1),
/// `integrity_key` = the REMOTE peer's password (§7.2.2).
#[derive(Debug, Clone)]
pub struct CheckRequest {
    pub username: String,
    pub priority: u32,
    pub controlling: bool,
    pub tie_breaker: u64,
    pub use_candidate: bool,
    pub integrity_key: String,
}

/// Set the header message-length field (bytes 2..4).
fn set_msg_len(msg: &mut [u8], len: u16) {
    msg[2..4].copy_from_slice(&len.to_be_bytes());
}

fn push_attr(msg: &mut Vec<u8>, typ: u16, value: &[u8]) {
    msg.extend_from_slice(&typ.to_be_bytes());
    msg.extend_from_slice(&(value.len() as u16).to_be_bytes());
    msg.extend_from_slice(value);
    while msg.len() % 4 != 0 {
        msg.push(0); // RFC 5389 §15: 4-byte alignment, zero padding
    }
}

/// Seal a STUN message: header + attrs + MESSAGE-INTEGRITY + FINGERPRINT.
///
/// MI input = message up to (not incl.) the MI attribute, with the header
/// length field pointing at the END of the MESSAGE-INTEGRITY attribute
/// (RFC 5389 §15.4) — WITHOUT the FINGERPRINT length. FINGERPRINT input =
/// message up to (not incl.) FP with the header length field equal to the
/// FULL transmitted message length INCLUDING the FP attribute itself
/// (§15.5), CRC-32 ⊕ 0x5354554e. The TRANSMITTED header length (set last)
/// covers everything. All three "length" rules are pinned byte-exact by
/// the RFC 5769 §2.1/§2.2 vectors (§2.1: transmitted 0x58, HMAC-input
/// 0x50, CRC-input 0x58) — the interop semantics pion/stun implements.
/// `key = None` seals without MI (401 error responses to requests we could
/// not authenticate carry no integrity).
fn seal_message(mtype: u16, txn: TransactionId, attrs: Vec<u8>, key: Option<&str>) -> Vec<u8> {
    let mut msg = Vec::with_capacity(HEADER_LEN + attrs.len() + 32);
    msg.extend_from_slice(&mtype.to_be_bytes());
    msg.extend_from_slice(&0u16.to_be_bytes());
    msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
    msg.extend_from_slice(&txn.0);
    msg.extend_from_slice(&attrs);

    if let Some(key) = key {
        let mi_off = msg.len();
        // HMAC-input length: up to the END of the MI attribute (no FP term)
        set_msg_len(&mut msg, (mi_off - HEADER_LEN + 24) as u16);
        let mac = hmac_sha1(key.as_bytes(), &msg);
        push_attr(&mut msg, ATTR_MESSAGE_INTEGRITY, &mac);
    }

    let fp_off = msg.len();
    // CRC-input length: the FULL message length (the FP attribute included)
    set_msg_len(&mut msg, (fp_off - HEADER_LEN + 8) as u16);
    let crc = crc32_ieee(&msg) ^ FINGERPRINT_XOR;
    push_attr(&mut msg, ATTR_FINGERPRINT, &crc.to_be_bytes());

    let total = (msg.len() - HEADER_LEN) as u16;
    set_msg_len(&mut msg, total);
    msg
}

/// Build one connectivity-check Binding Request (RFC 8445 §7.1.2).
pub fn build_check_request(req: &CheckRequest, txn: TransactionId) -> Vec<u8> {
    let mut attrs = Vec::with_capacity(64);
    push_attr(&mut attrs, ATTR_USERNAME, req.username.as_bytes());
    push_attr(&mut attrs, ATTR_PRIORITY, &req.priority.to_be_bytes());
    if req.controlling {
        push_attr(&mut attrs, ATTR_ICE_CONTROLLING, &req.tie_breaker.to_be_bytes());
    } else {
        push_attr(&mut attrs, ATTR_ICE_CONTROLLED, &req.tie_breaker.to_be_bytes());
    }
    if req.use_candidate {
        push_attr(&mut attrs, ATTR_USE_CANDIDATE, &[]);
    }
    seal_message(BINDING_REQUEST, txn, attrs, Some(&req.integrity_key))
}

/// Binding Success Response for a validated check: XOR-MAPPED-ADDRESS = the
/// request's source transport address (RFC 8445 §7.3.1.2), MESSAGE-INTEGRITY
/// keyed with the responder's OWN password (which is the requester's REMOTE
/// password), FINGERPRINT last.
pub fn build_success_response(txn: TransactionId, src: ([u8; 4], u16), integrity_key: &str) -> Vec<u8> {
    let (addr, port) = src;
    let mut val = Vec::with_capacity(8);
    val.push(0x00);
    val.push(0x01); // IPv4
    val.extend_from_slice(&(port ^ (MAGIC_COOKIE >> 16) as u16).to_be_bytes());
    for (i, b) in addr.iter().enumerate() {
        val.push(b ^ (MAGIC_COOKIE >> (24 - 8 * i)) as u8);
    }
    let mut attrs = Vec::with_capacity(16);
    push_attr(&mut attrs, stun::ATTR_XOR_MAPPED, &val);
    seal_message(BINDING_SUCCESS, txn, attrs, Some(integrity_key))
}

/// Binding Error Response. `key = Some(pwd)` for 487 (sent after the
/// USERNAME matched, so the peer can authenticate it); `None` for 401
/// (the request was never authenticated — no integrity on purpose).
pub fn build_error_response(txn: TransactionId, code: u16, key: Option<&str>) -> Vec<u8> {
    let class = (code / 100) as u8;
    let number = (code % 100) as u8;
    let mut attrs = Vec::with_capacity(8);
    push_attr(&mut attrs, ATTR_ERROR_CODE, &[0x00, 0x00, class & 0x07, number]);
    seal_message(BINDING_ERROR, txn, attrs, key)
}

/// One parsed STUN message relevant to connectivity checks. Offset fields
/// are private — verification goes through [`verify_message_integrity`] /
/// [`verify_fingerprint`], which re-derive the exact RFC 5389 §15.4/§15.5
/// input ranges from them.
#[derive(Debug, Clone)]
pub struct ParsedStun {
    pub msg_type: u16,
    pub txn: TransactionId,
    pub username: Option<String>,
    pub priority: Option<u32>,
    pub controlling: Option<u64>,
    pub controlled: Option<u64>,
    pub use_candidate: bool,
    pub error_code: Option<u16>,
    pub xor_mapped: Option<([u8; 4], u16)>,
    mi_off: Option<usize>,
    mi_value: Option<[u8; 20]>,
    fp_off: Option<usize>,
    fp_value: Option<u32>,
}

/// Parse one datagram (RFC 5389 §5/§15 walk). Header sanity (length ≥ 20,
/// magic cookie, declared length within the datagram) and bounds-checked
/// attribute walk; unknown attributes are skipped. Parse success does NOT
/// imply authenticity — that is what MI/FP verification decide.
pub fn parse_stun(msg: &[u8]) -> Result<ParsedStun, ManagementError> {
    let bad = |tok: &'static str| ManagementError::Parse(format!("stun-check:{tok}"));
    if msg.len() < HEADER_LEN {
        return Err(bad("short-header"));
    }
    let mtype = u16::from_be_bytes([msg[0], msg[1]]);
    let mlen = u16::from_be_bytes([msg[2], msg[3]]) as usize;
    if msg.len() < HEADER_LEN + mlen {
        return Err(bad("truncated"));
    }
    if u32::from_be_bytes([msg[4], msg[5], msg[6], msg[7]]) != MAGIC_COOKIE {
        return Err(bad("bad-magic"));
    }
    let mut txn = [0u8; 12];
    txn.copy_from_slice(&msg[8..20]);
    let txn = TransactionId(txn);

    let mut p = ParsedStun {
        msg_type: mtype,
        txn,
        username: None,
        priority: None,
        controlling: None,
        controlled: None,
        use_candidate: false,
        error_code: None,
        xor_mapped: None,
        mi_off: None,
        mi_value: None,
        fp_off: None,
        fp_value: None,
    };
    let body = &msg[HEADER_LEN..HEADER_LEN + mlen];
    let mut off = 0usize;
    while off + 4 <= body.len() {
        let atype = u16::from_be_bytes([body[off], body[off + 1]]);
        let alen = u16::from_be_bytes([body[off + 2], body[off + 3]]) as usize;
        off += 4;
        if off + alen > body.len() {
            return Err(bad("attr-overrun"));
        }
        let val = &body[off..off + alen];
        match atype {
            ATTR_USERNAME => {
                p.username = Some(String::from_utf8_lossy(val).trim_end_matches('\0').to_string());
            }
            ATTR_PRIORITY if alen == 4 => {
                p.priority = Some(u32::from_be_bytes([val[0], val[1], val[2], val[3]]));
            }
            ATTR_ICE_CONTROLLING if alen == 8 => {
                p.controlling = Some(u64::from_be_bytes([
                    val[0], val[1], val[2], val[3], val[4], val[5], val[6], val[7],
                ]));
            }
            ATTR_ICE_CONTROLLED if alen == 8 => {
                p.controlled = Some(u64::from_be_bytes([
                    val[0], val[1], val[2], val[3], val[4], val[5], val[6], val[7],
                ]));
            }
            ATTR_USE_CANDIDATE => p.use_candidate = true,
            ATTR_ERROR_CODE if alen >= 4 => {
                let class = (val[2] & 0x07) as u16;
                p.error_code = Some(class * 100 + val[3] as u16);
            }
            stun::ATTR_XOR_MAPPED => p.xor_mapped = decode_address(val, true),
            ATTR_MESSAGE_INTEGRITY if alen == 20 => {
                let mut mac = [0u8; 20];
                mac.copy_from_slice(val);
                p.mi_off = Some(HEADER_LEN + off - 4); // start of the MI attribute
                p.mi_value = Some(mac);
            }
            ATTR_FINGERPRINT if alen == 4 => {
                p.fp_off = Some(HEADER_LEN + off - 4);
                p.fp_value = Some(u32::from_be_bytes([val[0], val[1], val[2], val[3]]));
            }
            _ => {} // SOFTWARE, unknown comprehension-optional, ...
        }
        off += (alen + 3) & !3;
    }
    Ok(p)
}

/// MESSAGE-INTEGRITY verification (RFC 5389 §15.4): HMAC-SHA1 over the
/// message up to the MI attribute, header length field pointing at the END
/// of the MI attribute (NO FINGERPRINT term — the RFC 5769 §2.1 vector
/// transmits length 0x58 but HMACs over 0x50), key = `password`. False
/// when MI is absent, malformed, or wrong.
pub fn verify_message_integrity(msg: &[u8], parsed: &ParsedStun, password: &str) -> bool {
    let (Some(off), Some(want)) = (parsed.mi_off, parsed.mi_value) else {
        return false;
    };
    if off > msg.len() {
        return false;
    }
    let mut input = msg[..off].to_vec();
    let fake_len = (input.len() - HEADER_LEN + 24) as u16;
    set_msg_len(&mut input, fake_len);
    hmac_sha1(password.as_bytes(), &input) == want
}

/// FINGERPRINT verification (RFC 5389 §15.5): CRC-32 over the message up to
/// the FP attribute with the header length field equal to the FULL message
/// length INCLUDING the FP attribute (pinned by RFC 5769 §2.1/§2.2:
/// CRC-input lengths 0x58/0x3c — the transmitted lengths), ⊕ "STUN".
/// False when FP is absent or wrong.
pub fn verify_fingerprint(msg: &[u8], parsed: &ParsedStun) -> bool {
    let (Some(off), Some(want)) = (parsed.fp_off, parsed.fp_value) else {
        return false;
    };
    if off > msg.len() {
        return false;
    }
    let mut input = msg[..off].to_vec();
    let full_len = (msg.len() - HEADER_LEN) as u16;
    set_msg_len(&mut input, full_len);
    crc32_ieee(&input) ^ FINGERPRINT_XOR == want
}

// ---------------------------------------------------------------------------
// pair state machine (RFC 8445 §6.1.2.6/§8.1.2)
// ---------------------------------------------------------------------------

/// RFC 8445 §6.1.2.6 pair states. `Frozen` here = formed but the session
/// not started (single component / single check list — the multi-list thaw
/// of §6.1.1 never applies to this protocol path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairState {
    Frozen,
    Waiting,
    InProgress,
    Succeeded,
    Failed,
}

impl PairState {
    pub fn as_str(&self) -> &'static str {
        match self {
            PairState::Frozen => "frozen",
            PairState::Waiting => "waiting",
            PairState::InProgress => "in-progress",
            PairState::Succeeded => "succeeded",
            PairState::Failed => "failed",
        }
    }
}

/// Session events, drained by the owner via [`IceSession::take_events`].
#[derive(Debug, Clone, PartialEq)]
pub enum IceEvent {
    /// A local candidate got its protected socket bound and is ready to
    /// trickle out via the signal channel.
    LocalCandidateReady(Candidate),
    /// One of OUR connectivity checks got a valid, integrity-verified
    /// success response.
    CheckSucceeded { local: Candidate, remote: Candidate, rtt_ms: u64 },
    /// Regular nomination concluded: this pair is THE selected pair.
    SelectedPair { local: Candidate, remote: Candidate, rtt_ms: u64 },
    /// No valid inbound STUN for `DISCONNECTED_TIMEOUT_MS` (upstream
    /// Disconnected).
    Disconnected,
    /// All pairs failed, or the disconnected state persisted through
    /// `FAILED_TIMEOUT_MS` (upstream Failed).
    Failed(String),
    /// [`IceSession::stop`] ran; sockets closed.
    Closed,
}

/// Snapshot of one pair for tests/diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct PairSnapshot {
    pub local: Candidate,
    pub remote: Candidate,
    pub state: PairState,
    pub nominated: bool,
}

struct LocalSock {
    cand: Candidate,
    fd: sys::c_int,
}

/// Cap of the outbound-of-ICE queue ([`IceSession::take_data_rx`]): non-STUN
/// datagrams this session read but the WG data plane has not absorbed yet.
/// Bounded so a stalled WG consumer can never grow the session without
/// bound; overflow drops the NEWEST datagram and counts
/// ([`IceSession::data_dropped`]) — a stalled consumer is a failure the
/// pump must observe, not a silent unbounded buffer.
const DATA_RX_CAP: usize = 64;

struct Pair {
    local: usize,
    remote: usize,
    state: PairState,
    priority: u64,
    nominated: bool,
    txn: Option<TransactionId>,
    attempts: u8,
    first_send_ms: u64,
    last_send_ms: u64,
}

impl Pair {
    fn key(&self, _locals: &[LocalSock], remotes: &[Candidate]) -> (usize, [u8; 4], u16) {
        let addr = parse_ipv4(&remotes[self.remote].address).unwrap_or([0, 0, 0, 0]);
        (self.local, addr, remotes[self.remote].port)
    }
}

/// ICE session: local candidates (each owning one protected dup socket),
/// remote candidates (trickled from the signal channel), the check list,
/// nomination and timers. Explicit pump design: every interaction goes
/// through [`IceSession::run_once(now_ms)`] with an INJECTED clock; the
/// module never reads the wall clock.
pub struct IceSession {
    local: IceCredentials,
    remote: Option<IceCredentials>,
    controlling: bool,
    tie_breaker: u64,
    local_socks: Vec<LocalSock>,
    remote_cands: Vec<Candidate>,
    pairs: Vec<Pair>,
    events: VecDeque<IceEvent>,
    started: bool,
    stopped: bool,
    /// Selected pair identity: (local sock index, remote addr, port).
    selected: Option<(usize, [u8; 4], u16)>,
    last_inbound_ms: Option<u64>,
    last_keepalive_ms: u64,
    keepalives: Vec<(TransactionId, u64)>,
    emitted_disconnected: bool,
    emitted_failed: bool,
    emitted_all_pairs_failed: bool,
    /// N11: non-STUN (WG data-plane) datagrams read on our sockets, in
    /// arrival order — consumed by the orchestrator via
    /// [`IceSession::take_data_rx`] and fed to the WG device (upstream: the
    /// shared receive loop hands non-STUN packets to WG,
    /// ice_bind.go:279-303).
    data_rx: VecDeque<([u8; 4], u16, Vec<u8>)>,
    data_dropped: u64,
}

impl IceSession {
    /// New session. `tie_breaker = None` draws 64 bits of entropy (the
    /// first 8 bytes of a `/dev/urandom` transaction draw, fail-closed);
    /// tests pass a fixed value for determinism. Credentials are validated
    /// eagerly (RFC 8445 §16 floors) so a session object is always
    /// wire-legal.
    pub fn new(
        local: IceCredentials,
        controlling: bool,
        tie_breaker: Option<u64>,
    ) -> Result<Self, ManagementError> {
        local.validate()?;
        let tb = match tie_breaker {
            Some(tb) => tb,
            None => {
                let txn = stun::random_transaction_id()?;
                let mut b = [0u8; 8];
                b.copy_from_slice(&txn.0[..8]);
                u64::from_be_bytes(b)
            }
        };
        Ok(IceSession {
            local,
            remote: None,
            controlling,
            tie_breaker: tb,
            local_socks: Vec::new(),
            remote_cands: Vec::new(),
            pairs: Vec::new(),
            events: VecDeque::new(),
            started: false,
            stopped: false,
            selected: None,
            last_inbound_ms: None,
            last_keepalive_ms: 0,
            keepalives: Vec::new(),
            emitted_disconnected: false,
            emitted_failed: false,
            emitted_all_pairs_failed: false,
            data_rx: VecDeque::new(),
            data_dropped: 0,
        })
    }

    /// Set the REMOTE credentials carried in the offer/answer
    /// (`"ufrag:pwd"`, `shared/signal/client/client.go:74-101`). Validated
    /// (RFC 8445 §16) — a bad remote credential fails here, not mid-check.
    pub fn set_remote_credentials(&mut self, remote: IceCredentials) -> Result<(), ManagementError> {
        remote.validate()?;
        self.remote = Some(remote);
        Ok(())
    }

    /// Add a local candidate and give it its long-lived check socket: a
    /// FRESH protected fd is taken from `socks`, dup'd, non-blocked and
    /// bound to the candidate's address (`port 0` → the ephemeral port from
    /// `getsockname` REWRITES the candidate, so shell-supplied candidates
    /// may leave the port to the kernel). Fail-closed at every step — no
    /// socket, no candidate, no event. Emits
    /// [`IceEvent::LocalCandidateReady`] with the FINAL candidate.
    pub fn add_local_candidate(
        &mut self,
        cand: Candidate,
        socks: &dyn UdpSocketSource,
    ) -> Result<Candidate, ManagementError> {
        self.add_local_candidate_on(cand, false, socks)
    }

    /// HOST-ONLY (N12a) fixed-port variant: the check socket binds
    /// `0.0.0.0:<cand.port>` (wildcard) while the candidate KEEPS its
    /// interface address for signaling. The port-mapped interop topology
    /// (docs/self-hosted-interop-plan.md §端口映射) forwards an external
    /// `ip:port` to the peer's mapped local address — which socket address
    /// the forwarded datagram carries is not knowable in advance, so the
    /// wildcard bind is what makes a fixed port reachable at all (upstream
    /// does the same: ONE `0.0.0.0:NET_PORT` shared ICE/WG socket,
    /// `client/iface/bind/ice_bind.go`). Everything else — protected fd
    /// source, dup-only consumption, fail-closed steps, getsockname port
    /// pinning — is identical to [`IceSession::add_local_candidate`].
    pub fn add_local_candidate_fixed_port(
        &mut self,
        cand: Candidate,
        socks: &dyn UdpSocketSource,
    ) -> Result<Candidate, ManagementError> {
        self.add_local_candidate_on(cand, true, socks)
    }

    /// Shared body: `wildcard_bind = false` binds the candidate's own
    /// address (default path, unchanged), `true` binds `0.0.0.0`.
    fn add_local_candidate_on(
        &mut self,
        mut cand: Candidate,
        wildcard_bind: bool,
        socks: &dyn UdpSocketSource,
    ) -> Result<Candidate, ManagementError> {
        if self.stopped {
            return Err(ManagementError::Request { status: 0, message: "ice:session-closed".into() });
        }
        let addr = parse_ipv4(&cand.address).ok_or_else(|| {
            ManagementError::Parse(format!("ice:local-candidate-not-ipv4 '{}'", cand.address))
        })?;
        let provided = socks.take_fd().map_err(seam_to_management)?;
        let dup = dup_socket_fd(provided).map_err(seam_to_management)?;
        let cleanup = |fd: sys::c_int| unsafe { sys::close(fd) };
        if let Err(e) = set_nonblock(dup) {
            cleanup(dup);
            return Err(seam_to_management(e));
        }
        let bind_addr = if wildcard_bind { [0, 0, 0, 0] } else { addr };
        let local = sys::sockaddr_in::new(bind_addr, cand.port);
        if unsafe { sys::bind(dup, &local, core::mem::size_of::<sys::sockaddr_in>() as u32) } == -1 {
            let e = sys::errno();
            cleanup(dup);
            return Err(ManagementError::Network(format!("ice-bind-failed (errno={e})")));
        }
        let mut name_addr = sys::sockaddr_in::new([0, 0, 0, 0], 0);
        let mut name_len = core::mem::size_of::<sys::sockaddr_in>() as u32;
        if unsafe { sys::getsockname(dup, &mut name_addr, &mut name_len) } == -1 {
            cleanup(dup);
            return Err(ManagementError::Network("ice-getsockname-failed".into()));
        }
        let bound_port = u16::from_be(name_addr.sin_port);
        if cand.port == 0 {
            cand.port = bound_port;
        } else if cand.port != bound_port {
            cleanup(dup);
            return Err(ManagementError::Network("ice-bind-port-mismatch".into()));
        }
        self.local_socks.push(LocalSock { cand: cand.clone(), fd: dup });
        self.rebuild_pairs();
        self.events.push_back(IceEvent::LocalCandidateReady(cand.clone()));
        Ok(cand)
    }

    /// Add a remote candidate from a signal `CANDIDATE` payload
    /// (`Body.payload` = `candidate.Marshal()`; the caller unmarshals via
    /// [`Candidate::unmarshal`]). RELAY candidates are skipped (no TURN
    /// client this increment — an unpaired relay candidate would fake
    /// coverage). Duplicates by address:port are skipped (a host and its
    /// srflx share the mapping on loopback; RFC 8445 §6.1.2.4 dedups by
    /// address pair).
    pub fn add_remote_candidate(&mut self, cand: Candidate) {
        if self.stopped || cand.typ == CandidateType::Relay {
            return;
        }
        if cand.transport != "udp" || cand.component != 1 {
            return; // UDP4 component-1 checks only (upstream NetworkTypes, agent.go:53)
        }
        if parse_ipv4(&cand.address).is_none() {
            return; // IPv6 candidate — UDP4-only increment
        }
        let key = (cand.address.clone(), cand.port);
        if self.remote_cands.iter().any(|c| (c.address.clone(), c.port) == key) {
            return;
        }
        self.remote_cands.push(cand);
        self.rebuild_pairs();
    }

    /// Form/re-form the check list: cartesian local × remote (component 1,
    /// udp), dedup by remote address, pair priority per RFC 8445 §6.1.2.3,
    /// sorted descending. Existing pair state survives re-forms (role
    /// switches, late prflx candidates). The G/D operands swap with the
    /// role, which is exactly what re-sorting after a role conflict does.
    fn rebuild_pairs(&mut self) {
        let mut remotes: Vec<(usize, u32)> =
            self.remote_cands.iter().enumerate().map(|(i, c)| (i, c.priority)).collect();
        remotes.sort_by(|a, b| b.1.cmp(&a.1)); // high-priority remote first wins dedup

        let mut next: Vec<Pair> = Vec::new();
        for (li, _l) in self.local_socks.iter().enumerate() {
            let mut seen_addr: Vec<([u8; 4], u16)> = Vec::new();
            for (ri, prio) in remotes.iter() {
                let rc = &self.remote_cands[*ri];
                let Some(addr) = parse_ipv4(&rc.address) else { continue };
                if seen_addr.contains(&(addr, rc.port)) {
                    continue;
                }
                seen_addr.push((addr, rc.port));
                let lp = self.local_socks[li].cand.priority as u64;
                let rp = *prio as u64;
                let (g, d) = if self.controlling { (lp, rp) } else { (rp, lp) };
                let priority = (g.min(d) << 32) + 2 * g.max(d) + u64::from(g > d);
                next.push(Pair {
                    local: li,
                    remote: *ri,
                    state: PairState::Frozen,
                    priority,
                    nominated: false,
                    txn: None,
                    attempts: 0,
                    first_send_ms: 0,
                    last_send_ms: 0,
                });
            }
        }
        // carry over state from the previous list
        for old in self.pairs.drain(..) {
            let key = old.key(&self.local_socks, &self.remote_cands);
            if let Some(p) = next.iter_mut().find(|p| p.key(&self.local_socks, &self.remote_cands) == key) {
                p.state = old.state;
                p.nominated = old.nominated;
                p.txn = old.txn;
                p.attempts = old.attempts;
                p.first_send_ms = old.first_send_ms;
                p.last_send_ms = old.last_send_ms;
            }
        }
        next.sort_by(|a, b| b.priority.cmp(&a.priority));
        self.pairs = next;
    }

    /// Start connectivity checks: requires remote credentials, at least one
    /// local candidate with a socket, and at least one pair. Thaws all
    /// pairs Frozen → Waiting (single check list).
    pub fn start(&mut self) -> Result<(), ManagementError> {
        if self.stopped {
            return Err(ManagementError::Request { status: 0, message: "ice:session-closed".into() });
        }
        if self.remote.is_none() {
            return Err(ManagementError::Request { status: 0, message: "ice:no-remote-credentials".into() });
        }
        if self.local_socks.is_empty() {
            return Err(ManagementError::Request { status: 0, message: "ice:no-local-candidates".into() });
        }
        if self.remote_cands.is_empty() {
            return Err(ManagementError::Request { status: 0, message: "ice:no-remote-candidates".into() });
        }
        if self.pairs.is_empty() {
            return Err(ManagementError::Request { status: 0, message: "ice:no-pairs".into() });
        }
        for p in self.pairs.iter_mut() {
            if p.state == PairState::Frozen {
                p.state = PairState::Waiting;
            }
        }
        self.started = true;
        Ok(())
    }

    /// One pump of the session at injected time `now_ms`: sends the next
    /// check / retransmits / keepalives, drains all sockets, applies
    /// timers. Poll timeout is 0 (pure non-blocking pump — the caller owns
    /// the loop and the clock).
    pub fn run_once(&mut self, now_ms: u64) -> Result<(), ManagementError> {
        if self.stopped || !self.started {
            return Ok(());
        }
        self.pump_checks(now_ms)?;
        self.pump_keepalive(now_ms)?;
        self.pump_recv(now_ms)?;
        self.pump_timeouts(now_ms);
        Ok(())
    }

    fn pump_checks(&mut self, now: u64) -> Result<(), ManagementError> {
        // retransmit / fail in-flight checks — the SAME transaction id every
        // time (RFC 5389 §7.2.1 retransmission = same request, new send)
        for i in 0..self.pairs.len() {
            let due = {
                let p = &self.pairs[i];
                p.state == PairState::InProgress
                    && p.txn.is_some()
                    && now.saturating_sub(p.last_send_ms) >= CHECK_RTO_MS
            };
            if !due {
                continue;
            }
            let (txn, nominated, attempts) = {
                let p = &self.pairs[i];
                (p.txn.expect("in-progress has txn"), p.nominated, p.attempts)
            };
            if attempts < MAX_CHECK_ATTEMPTS {
                let msg = self.build_check(txn, nominated)?;
                let fd = self.local_socks[self.pairs[i].local].fd;
                let target = self.remote_target(&self.pairs[i]);
                if !send_to(fd, &target, &msg) {
                    self.pairs[i].state = PairState::Failed;
                    self.pairs[i].txn = None;
                    continue;
                }
                self.pairs[i].last_send_ms = now;
                self.pairs[i].attempts += 1;
            } else {
                // RFC 8445 §7.2.5.2.3: transaction timed out → pair Failed
                self.pairs[i].state = PairState::Failed;
                self.pairs[i].txn = None;
            }
        }

        // start the next check (serial pacing — one outstanding check)
        let has_in_progress = self.pairs.iter().any(|p| p.state == PairState::InProgress);
        if !has_in_progress {
            if let Some(i) = self.pairs.iter().position(|p| p.state == PairState::Waiting) {
                let txn = stun::random_transaction_id()?;
                let msg = self.build_check(txn, false)?;
                let fd = self.local_socks[self.pairs[i].local].fd;
                let target = self.remote_target(&self.pairs[i]);
                if send_to(fd, &target, &msg) {
                    let p = &mut self.pairs[i];
                    p.state = PairState::InProgress;
                    p.txn = Some(txn);
                    p.attempts = 1;
                    p.first_send_ms = now;
                    p.last_send_ms = now;
                } else {
                    self.pairs[i].state = PairState::Failed;
                }
            }
        }

        // regular nomination (RFC 8445 §8.1.1): the controlling agent
        // nominates the best Succeeded pair as soon as one exists (serial
        // priority-ordered checks ⇒ the first success IS the best pair);
        // selection happens when the nomination check's response returns.
        if self.controlling && self.selected.is_none() {
            let nominated_in_flight = self
                .pairs
                .iter()
                .any(|p| p.nominated && p.state == PairState::InProgress);
            if !nominated_in_flight {
                let best = self
                    .pairs
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.state == PairState::Succeeded && !p.nominated)
                    .max_by_key(|(_, p)| p.priority)
                    .map(|(i, _)| i);
                if let Some(i) = best {
                    let txn = stun::random_transaction_id()?;
                    let msg = self.build_check(txn, true)?;
                    let fd = self.local_socks[self.pairs[i].local].fd;
                    let target = self.remote_target(&self.pairs[i]);
                    if send_to(fd, &target, &msg) {
                        let p = &mut self.pairs[i];
                        p.nominated = true;
                        p.state = PairState::InProgress;
                        p.txn = Some(txn);
                        p.attempts = 1;
                        p.first_send_ms = now;
                        p.last_send_ms = now;
                    }
                }
            }
        }
        Ok(())
    }

    fn pump_keepalive(&mut self, now: u64) -> Result<(), ManagementError> {
        let Some(sel) = self.selected else { return Ok(()) };
        if now.saturating_sub(self.last_keepalive_ms) < KEEPALIVE_INTERVAL_MS {
            return Ok(());
        }
        let Some(i) = self
            .pairs
            .iter()
            .position(|p| p.key(&self.local_socks, &self.remote_cands) == sel)
        else {
            return Ok(());
        };
        let txn = stun::random_transaction_id()?;
        // keepalive = a plain connectivity check (no USE-CANDIDATE) on the
        // selected pair — the pion keepalive shape (Binding Request, re-keyed).
        let msg = self.build_check(txn, false)?;
        let fd = self.local_socks[self.pairs[i].local].fd;
        let target = self.remote_target(&self.pairs[i]);
        if send_to(fd, &target, &msg) {
            self.keepalives.push((txn, now));
            self.last_keepalive_ms = now;
            if self.keepalives.len() > 8 {
                self.keepalives.remove(0); // bound the keepalive history
            }
        }
        Ok(())
    }

    fn pump_recv(&mut self, now: u64) -> Result<(), ManagementError> {
        if self.local_socks.is_empty() {
            return Ok(());
        }
        let mut fds: Vec<sys::pollfd> = self
            .local_socks
            .iter()
            .map(|s| sys::pollfd { fd: s.fd, events: sys::POLLIN, revents: 0 })
            .collect();
        let ret = unsafe { sys::poll(fds.as_mut_ptr(), fds.len() as u64, 0) };
        if ret <= 0 {
            return Ok(());
        }
        for i in 0..fds.len() {
            if fds[i].revents & sys::POLLIN == 0 {
                continue;
            }
            self.drain_socket(i, now);
        }
        Ok(())
    }

    fn drain_socket(&mut self, idx: usize, now: u64) {
        let fd = self.local_socks[idx].fd;
        let mut buf = [0u8; 1500];
        for _ in 0..8 {
            // bounded drain per pump: a flood cannot starve the timers
            let mut src = sys::sockaddr_in::new([0, 0, 0, 0], 0);
            let mut src_len = core::mem::size_of::<sys::sockaddr_in>() as u32;
            let n = unsafe {
                sys::recvfrom(
                    fd,
                    buf.as_mut_ptr() as *mut core::ffi::c_void,
                    buf.len(),
                    0,
                    &mut src,
                    &mut src_len,
                )
            };
            if n <= 0 {
                return; // EAGAIN — drained
            }
            let addr = src.sin_addr;
            let port = u16::from_be(src.sin_port);
            self.demux_inbound(idx, &buf[..n as usize], addr, port, now);
        }
    }

    /// N11 demux — upstream `ICEBind` receive shape (ice_bind.go:279-345):
    /// the WG/non-STUN predicate (`demux_routes_to_wg`, the verbatim
    /// `filterOutStunMessages` condition) is evaluated FIRST so a
    /// cookie-overlap WG packet can never reach the STUN parser; STUN
    /// messages go to the check/keepalive machinery (malformed STUN is
    /// dropped at the ICE layer, ice_bind.go:332-338, not handed to WG);
    /// everything else is queued for the WG data plane.
    fn demux_inbound(&mut self, idx: usize, datagram: &[u8], src: [u8; 4], src_port: u16, now: u64) {
        if demux_routes_to_wg(datagram) {
            self.push_data_rx(src, src_port, datagram);
            return;
        }
        match parse_stun(datagram) {
            Ok(parsed) => self.handle_inbound(idx, datagram, &parsed, src, src_port, now),
            // STUN-shaped but undecodable: dropped (upstream clears the
            // buffer on a parse error, ice_bind.go:332-338).
            Err(_) => {}
        }
    }

    fn push_data_rx(&mut self, src: [u8; 4], src_port: u16, datagram: &[u8]) {
        if self.data_rx.len() >= DATA_RX_CAP {
            self.data_dropped += 1;
            return;
        }
        self.data_rx.push_back((src, src_port, datagram.to_vec()));
    }

    fn handle_inbound(&mut self, local_idx: usize, datagram: &[u8], parsed: &ParsedStun, src: [u8; 4], src_port: u16, now: u64) {
        match parsed.msg_type {
            BINDING_REQUEST => self.on_check_request(local_idx, datagram, &parsed, src, src_port, now),
            BINDING_SUCCESS => self.on_check_response(datagram, &parsed, now),
            BINDING_ERROR => self.on_error_response(datagram, &parsed, now),
            _ => {} // indications / other methods: not in this protocol path
        }
    }

    /// Inbound connectivity check (RFC 8445 §7.3): fingerprint gate,
    /// username gate, integrity gate, role-conflict repair, then a success
    /// response; the pair is (re)triggered and — controlled side + USE-
    /// CANDIDATE — selected.
    fn on_check_request(
        &mut self,
        local_idx: usize,
        datagram: &[u8],
        parsed: &ParsedStun,
        src: [u8; 4],
        src_port: u16,
        now: u64,
    ) {
        // FINGERPRINT is mandatory on ICE checks (RFC 8445 §7.1.2); wrong
        // FP = silently discarded (RFC 5389 §10.1.3? §15.5 semantics).
        if !verify_fingerprint(datagram, parsed) {
            return;
        }
        // USERNAME: "<our-ufrag>:<their-ufrag>" (§7.3.1.2); wrong → 401.
        let username_ok = parsed
            .username
            .as_deref()
            .and_then(|u| u.split_once(':'))
            .map(|(local_part, _)| local_part == self.local.ufrag)
            .unwrap_or(false);
        if !username_ok {
            let resp = build_error_response(parsed.txn, ERR_UNAUTHORIZED, None);
            let target = sys::sockaddr_in::new(src, src_port);
            let _ = send_to(self.local_socks[local_idx].fd, &target, &resp);
            return;
        }
        // MESSAGE-INTEGRITY: keyed with OUR password (RFC 8445 §7.2.2 —
        // the request was keyed with "the password for the remote agent",
        // which is us). Wrong → 401.
        if !verify_message_integrity(datagram, parsed, &self.local.pwd) {
            let resp = build_error_response(parsed.txn, ERR_UNAUTHORIZED, None);
            let target = sys::sockaddr_in::new(src, src_port);
            let _ = send_to(self.local_socks[local_idx].fd, &target, &resp);
            return;
        }
        // Role-conflict detection & repair (RFC 8445 §7.3.1.1).
        if let Some(theirs) = parsed.controlling {
            if self.controlling {
                if self.tie_breaker >= theirs {
                    let resp = build_error_response(parsed.txn, ERR_ROLE_CONFLICT, Some(&self.local.pwd));
                    let target = sys::sockaddr_in::new(src, src_port);
                    let _ = send_to(self.local_socks[local_idx].fd, &target, &resp);
                    return; // retain role
                }
                self.switch_role(false, false);
            }
        }
        if let Some(theirs) = parsed.controlled {
            if !self.controlling {
                if self.tie_breaker >= theirs {
                    self.switch_role(true, false);
                } else {
                    let resp = build_error_response(parsed.txn, ERR_ROLE_CONFLICT, Some(&self.local.pwd));
                    let target = sys::sockaddr_in::new(src, src_port);
                    let _ = send_to(self.local_socks[local_idx].fd, &target, &resp);
                    return; // retain role
                }
            }
        }
        // Success response: XOR-MAPPED-ADDRESS = source, integrity keyed
        // with our own password (§7.3.1.5 shape).
        let resp = build_success_response(parsed.txn, (src, src_port), &self.local.pwd);
        let target = sys::sockaddr_in::new(src, src_port);
        let _ = send_to(self.local_socks[local_idx].fd, &target, &resp);
        self.last_inbound_ms = Some(now);

        // Pair bookkeeping: match (this socket ← source); unknown source ⇒
        // prflx remote candidate from the request's PRIORITY (§7.3.1.3,
        // minimal subset).
        self.ensure_pair_for(local_idx, src, src_port, parsed.priority);
        let Some(remote_idx) = self.remote_index_for(src, src_port) else { return };
        let Some(pi) = self
            .pairs
            .iter()
            .position(|p| p.local == local_idx && p.remote == remote_idx)
        else {
            return;
        };
        // Triggered check (§7.3.1.4): a valid request validates the pair;
        // Frozen/Failed pairs go back to Waiting (Waiting/InProgress/
        // Succeeded are untouched).
        match self.pairs[pi].state {
            PairState::Frozen | PairState::Failed => self.pairs[pi].state = PairState::Waiting,
            _ => {}
        }
        // Controlled side: USE-CANDIDATE nominates the pair (§7.3.1.5/§8.2);
        // selection concludes as soon as the pair is Succeeded (now, or on
        // our own check's response later).
        if parsed.use_candidate && !self.controlling {
            self.pairs[pi].nominated = true;
            if self.pairs[pi].state == PairState::Succeeded && self.selected.is_none() {
                self.select_pair(pi, 0, now);
            }
        }
    }

    /// Success response to one of OUR checks (RFC 8445 §7.2.5.3): FINGERPRINT
    /// and MESSAGE-INTEGRITY (remote password) gates, then the pair
    /// succeeds; a succeeded NOMINATION check selects the pair.
    fn on_check_response(&mut self, datagram: &[u8], parsed: &ParsedStun, now: u64) {
        if !verify_fingerprint(datagram, parsed) {
            return;
        }
        if !verify_message_integrity(datagram, parsed, &self.remote.as_ref().map(|r| r.pwd.as_str()).unwrap_or("")) {
            return;
        }
        // keepalive answer?
        if let Some(pos) = self.keepalives.iter().position(|(t, _)| t == &parsed.txn) {
            self.keepalives.remove(pos);
            self.last_inbound_ms = Some(now);
            return;
        }
        let Some(pi) = self
            .pairs
            .iter()
            .position(|p| p.state == PairState::InProgress && p.txn == Some(parsed.txn))
        else {
            return; // unknown/duplicate transaction — noise
        };
        let rtt = now.saturating_sub(self.pairs[pi].first_send_ms);
        let was_nominated = self.pairs[pi].nominated;
        self.pairs[pi].state = PairState::Succeeded;
        self.pairs[pi].txn = None;
        self.last_inbound_ms = Some(now);
        let (local, remote) = self.pair_candidates(pi);
        self.events.push_back(IceEvent::CheckSucceeded { local, remote, rtt_ms: rtt });
        // Controlling: the NOMINATION check concluded → selected
        // (§8.1.1). Controlled: a nominated pair that SUCCEEDED becomes
        // selected (§8.2) — whichever of nomination/response lands first.
        if was_nominated && self.selected.is_none() {
            self.select_pair(pi, rtt, now);
        }
    }

    /// Error response to one of OUR checks (RFC 8445 §7.2.5.1/§7.2.5.2.4).
    fn on_error_response(&mut self, datagram: &[u8], parsed: &ParsedStun, now: u64) {
        if !verify_fingerprint(datagram, parsed) {
            return;
        }
        let Some(pi) = self
            .pairs
            .iter()
            .position(|p| p.state == PairState::InProgress && p.txn == Some(parsed.txn))
        else {
            return;
        };
        self.pairs[pi].txn = None;
        if parsed.error_code == Some(ERR_ROLE_CONFLICT) {
            // §7.2.5.1: switch role, re-trigger the pair, CHANGE the
            // tie-breaker.
            self.switch_role(!self.controlling, true);
            if self.pairs[pi].state != PairState::Succeeded {
                self.pairs[pi].state = PairState::Waiting;
                self.pairs[pi].nominated = false;
            }
        } else {
            // 401/400/… — unrecoverable for this check (§7.2.5.2.4).
            self.pairs[pi].state = PairState::Failed;
        }
        self.last_inbound_ms = Some(now);
    }

    /// Role switch (RFC 8445 §7.3.1.1/§7.2.5.1): swap the role, optionally
    /// re-draw the tie-breaker, clear nomination state (nomination is the
    /// CONTROLLING agent's job), recompute pair priorities (§6.1.2.3).
    fn switch_role(&mut self, to_controlling: bool, redraw_tie_breaker: bool) {
        if self.controlling == to_controlling {
            return;
        }
        self.controlling = to_controlling;
        if redraw_tie_breaker {
            if let Ok(txn) = stun::random_transaction_id() {
                let mut b = [0u8; 8];
                b.copy_from_slice(&txn.0[..8]);
                self.tie_breaker = u64::from_be_bytes(b);
            }
        }
        for p in self.pairs.iter_mut() {
            p.nominated = false;
            if p.state == PairState::InProgress {
                p.txn = None;
                p.state = PairState::Waiting;
            }
        }
        self.selected = None;
        self.rebuild_pairs();
    }

    /// Match/derive the remote candidate for a check source; unknown
    /// sources become prflx candidates (minimal subset, §7.3.1.3).
    fn ensure_pair_for(&mut self, _local_idx: usize, src: [u8; 4], src_port: u16, priority: Option<u32>) {
        if self.remote_index_for(src, src_port).is_some() {
            return;
        }
        let cand = Candidate {
            foundation: format!("prflx-{:02x}{:02x}{:02x}{:02x}", src[0], src[1], src[2], src[3]),
            component: 1,
            transport: "udp".into(),
            priority: priority.unwrap_or(priority_for(CandidateType::Prflx)),
            address: format!("{}.{}.{}.{}", src[0], src[1], src[2], src[3]),
            port: src_port,
            typ: CandidateType::Prflx,
            related_address: None,
            related_port: None,
        };
        self.remote_cands.push(cand);
        self.rebuild_pairs();
    }

    fn remote_index_for(&self, src: [u8; 4], src_port: u16) -> Option<usize> {
        self.remote_cands
            .iter()
            .position(|c| parse_ipv4(&c.address) == Some(src) && c.port == src_port)
    }

    fn remote_target(&self, p: &Pair) -> sys::sockaddr_in {
        let rc = &self.remote_cands[p.remote];
        sys::sockaddr_in::new(parse_ipv4(&rc.address).unwrap_or([0, 0, 0, 0]), rc.port)
    }

    /// Build the check Binding Request (RFC 8445 §7.1.2): USERNAME
    /// `remote:local`, PRIORITY = local candidate's prflx-type priority,
    /// role attribute + tie-breaker, integrity keyed with the REMOTE
    /// password. `txn` is caller-supplied so RETRANSMISSIONS re-send the
    /// SAME transaction id (RFC 5389 §7.2.1).
    fn build_check(&self, txn: TransactionId, use_candidate: bool) -> Result<Vec<u8>, ManagementError> {
        let remote = self.remote.as_ref().ok_or_else(|| {
            ManagementError::Request { status: 0, message: "ice:no-remote-credentials".into() }
        })?;
        let req = CheckRequest {
            username: format!("{}:{}", remote.ufrag, self.local.ufrag),
            priority: priority_for(CandidateType::Prflx),
            controlling: self.controlling,
            tie_breaker: self.tie_breaker,
            use_candidate,
            integrity_key: remote.pwd.clone(),
        };
        Ok(build_check_request(&req, txn))
    }

    fn pair_candidates(&self, pi: usize) -> (Candidate, Candidate) {
        let p = &self.pairs[pi];
        (self.local_socks[p.local].cand.clone(), self.remote_cands[p.remote].clone())
    }

    fn select_pair(&mut self, pi: usize, rtt_ms: u64, now: u64) {
        let key = self.pairs[pi].key(&self.local_socks, &self.remote_cands);
        self.selected = Some(key);
        // keepalive cadence counts from selection (4s later, not 4s after 0)
        self.last_keepalive_ms = now;
        let (local, remote) = self.pair_candidates(pi);
        self.events.push_back(IceEvent::SelectedPair { local, remote, rtt_ms });
    }

    fn pump_timeouts(&mut self, now: u64) {
        // all pairs failed → session failed (RFC 8445 §7.2.5.2.3 + §8.1.2
        // checklist Failed state)
        if self.selected.is_none()
            && !self.pairs.is_empty()
            && self.pairs.iter().all(|p| p.state == PairState::Failed)
            && !self.emitted_all_pairs_failed
        {
            self.emitted_all_pairs_failed = true;
            self.events.push_back(IceEvent::Failed("all-pairs-failed".into()));
        }
        // keepalive/disconnect timers (upstream agent.go:22-24 semantics):
        // 6s without valid inbound → Disconnected; 6s more → Failed.
        if let Some(last) = self.last_inbound_ms {
            if now.saturating_sub(last) >= DISCONNECTED_TIMEOUT_MS && !self.emitted_disconnected {
                self.emitted_disconnected = true;
                self.events.push_back(IceEvent::Disconnected);
            }
            let failed_after = DISCONNECTED_TIMEOUT_MS + FAILED_TIMEOUT_MS;
            if now.saturating_sub(last) >= failed_after && !self.emitted_failed {
                self.emitted_failed = true;
                self.events.push_back(IceEvent::Failed("disconnected-too-long".into()));
            }
        }
    }

    /// Drain pending events.
    pub fn take_events(&mut self) -> Vec<IceEvent> {
        self.events.drain(..).collect()
    }

    /// The selected pair, if nomination concluded: (local, remote).
    pub fn selected_pair(&self) -> Option<(Candidate, Candidate)> {
        let key = self.selected?;
        let pi = self
            .pairs
            .iter()
            .position(|p| p.key(&self.local_socks, &self.remote_cands) == key)?;
        Some(self.pair_candidates(pi))
    }

    /// N11: the selected pair's LOCAL socket fd (our dup) — the orchestrator
    /// dups it into the WG device as the peer's egress so WG data rides the
    /// selected transport. The number stays BORROWED: the consumer takes its
    /// own dup (`dup_socket_fd`) and never closes this one (fd contract, the
    /// `WgDeviceFeed::feed_wg_socket` borrow shape).
    pub fn selected_local_fd(&self) -> Option<sys::c_int> {
        let key = self.selected?;
        let pi = self
            .pairs
            .iter()
            .position(|p| p.key(&self.local_socks, &self.remote_cands) == key)?;
        Some(self.local_socks[self.pairs[pi].local].fd)
    }

    /// N11: drain the non-STUN (WG data-plane) datagrams this session read
    /// since the last call, in arrival order: `(src_addr, src_port, bytes)`.
    pub fn take_data_rx(&mut self) -> Vec<([u8; 4], u16, Vec<u8>)> {
        self.data_rx.drain(..).collect()
    }

    /// N11: datagrams dropped because the WG data plane fell
    /// [`DATA_RX_CAP`] behind (observability; never silent loss).
    pub fn data_dropped(&self) -> u64 {
        self.data_dropped
    }

    /// Current role (tests/inspection).
    pub fn is_controlling(&self) -> bool {
        self.controlling
    }

    /// Check-list snapshot (tests/inspection).
    pub fn pair_snapshots(&self) -> Vec<PairSnapshot> {
        self.pairs
            .iter()
            .enumerate()
            .map(|(pi, p)| {
                let (local, remote) = self.pair_candidates(pi);
                PairSnapshot { local, remote, state: p.state, nominated: p.nominated }
            })
            .collect()
    }

    /// Stop the session: close exactly our dup sockets, emit `Closed`.
    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        for s in self.local_socks.drain(..) {
            unsafe { sys::close(s.fd) };
        }
        self.stopped = true;
        self.events.push_back(IceEvent::Closed);
    }
}

impl Drop for IceSession {
    fn drop(&mut self) {
        if !self.stopped {
            for s in self.local_socks.drain(..) {
                unsafe { sys::close(s.fd) };
            }
            self.stopped = true;
        }
    }
}

// ---------------------------------------------------------------------------
// small helpers
// ---------------------------------------------------------------------------

/// Strict dotted-quad IPv4 parse (the wire form carries literals; no DNS on
/// the check path).
fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let octets: Vec<&str> = s.trim().split('.').collect();
    if octets.len() != 4 {
        return None;
    }
    let mut addr = [0u8; 4];
    for (i, o) in octets.iter().enumerate() {
        if o.is_empty() || o.len() > 3 || !o.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        addr[i] = o.parse().ok()?;
    }
    Some(addr)
}

/// sendto with the EINTR retry idiom (gather shape); bool = sent.
fn send_to(fd: sys::c_int, addr: &sys::sockaddr_in, buf: &[u8]) -> bool {
    for _ in 0..3 {
        let n = unsafe {
            sys::sendto(
                fd,
                buf.as_ptr() as *const core::ffi::c_void,
                buf.len(),
                0,
                addr,
                core::mem::size_of::<sys::sockaddr_in>() as u32,
            )
        };
        if n >= 0 {
            return true;
        }
        if sys::errno() != sys::EINTR {
            return false;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// unit tests: crypto vectors + codec round-trip (wire-level cases live in
// tests/ice_session_*.rs)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 2202 test case 1: key = 20 × 0x0b, data = "Hi There".
    #[test]
    fn hmac_sha1_rfc2202_case1() {
        let key = [0x0bu8; 20];
        let mac = hmac_sha1(&key, b"Hi There");
        assert_eq!(mac, hex20("b617318655057264e28bc0b6fb378c8ef146be00"));
    }

    /// RFC 2202 test case 2: key = "Jefe", data = "what do ya want for
    /// nothing?".
    #[test]
    fn hmac_sha1_rfc2202_case2() {
        let mac = hmac_sha1(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(mac, hex20("effcdf6ae5eb2fa2d27416d5f184df9c259a7c79"));
    }

    /// SHA-1("abc") — RFC 3174 / FIPS 180 canonical vector.
    #[test]
    fn sha1_abc_vector() {
        assert_eq!(sha1(b"abc"), hex20("a9993e364706816aba3e25717850c26c9cd0d89d"));
    }

    /// SHA-1 of a 56..=63-byte tail (length field spans two words).
    #[test]
    fn sha1_long_tail_vector() {
        // SHA-1("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")
        let m = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(sha1(m), hex20("84983e441c3bd26ebaae4aa1f95129e5e54670f1"));
    }

    /// CRC-32 check value (IEEE 802.3 / RFC 1952: "123456789" → 0xCBF43926).
    #[test]
    fn crc32_check_value() {
        assert_eq!(crc32_ieee(b"123456789"), 0xCBF4_3926);
    }

    /// Credentials: generated ones validate, match upstream lengths, and
    /// two draws never collide.
    #[test]
    fn generated_credentials_are_valid_and_unique() {
        let a = IceCredentials::generate().expect("entropy");
        let b = IceCredentials::generate().expect("entropy");
        a.validate().expect("generated ufrag/pwd must validate");
        b.validate().expect("generated ufrag/pwd must validate");
        assert_eq!(a.ufrag.len(), 16);
        assert_eq!(a.pwd.len(), 32);
        assert_ne!(a, b, "two entropy draws must differ");
        assert!(a.ufrag.bytes().all(|c| RUNES_ALPHA.contains(&c)));
        assert!(a.pwd.bytes().all(|c| RUNES_ALPHA.contains(&c)));
    }

    /// Credentials validation: RFC 8445 §16 floors, charset, ceiling.
    #[test]
    fn credential_validation_rejects_bad_shapes() {
        let ok = IceCredentials { ufrag: "abcd".into(), pwd: "0123456789012345678901".into() };
        ok.validate().expect("minimal legal credentials");
        for (ufrag, pwd) in [
            ("abc", "0123456789012345678901"),     // ufrag < 4
            ("abcd", "too-short"),                  // pwd < 22
            ("abcd!", "0123456789012345678901"),    // bad char
            ("abcd", "0123456789012345678901#"),    // bad char in pwd
        ] {
            let e = IceCredentials { ufrag: ufrag.into(), pwd: pwd.into() };
            e.validate().expect_err("must reject");
        }
        let long = IceCredentials { ufrag: "a".repeat(257), pwd: "0123456789012345678901".into() };
        long.validate().expect_err("ceiling");
    }

    /// Codec round-trip: a built check parses back to its exact inputs and
    /// passes MI/FP verification with the right key; corrupting one MI or
    /// FP byte flips the verdict.
    #[test]
    fn check_request_roundtrip_and_tamper() {        let req = CheckRequest {
            username: "REMOTEufragXXXXXX:LOCALufragXXXXX".into(),
            priority: priority_for(CandidateType::Prflx),
            controlling: true,
            tie_breaker: 0x0102_0304_0506_0708,
            use_candidate: true,
            integrity_key: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef".into(),
        };
        let txn = stun::random_transaction_id().expect("entropy");
        let msg = build_check_request(&req, txn);
        let parsed = parse_stun(&msg).expect("parses");
        assert_eq!(parsed.username.as_deref(), Some(req.username.as_str()));
        assert_eq!(parsed.priority, Some(req.priority));
        assert_eq!(parsed.controlling, Some(req.tie_breaker));
        assert!(parsed.controlled.is_none());
        assert!(parsed.use_candidate);
        assert!(verify_fingerprint(&msg, &parsed));
        assert!(verify_message_integrity(&msg, &parsed, &req.integrity_key));
        assert!(!verify_message_integrity(&msg, &parsed, "wrong-password-wrong-passwordxx"));

        let mut corrupt_mi = msg.clone();
        let mi_val_start = corrupt_mi.len() - 28; // FP(8) + MI value(20) → MI value start
        corrupt_mi[mi_val_start] ^= 0x01;
        // FINGERPRINT covers every byte before it — including MI — so flip
        // the MI byte and RESEAL the fingerprint to isolate the MI verdict.
        reseal_fingerprint(&mut corrupt_mi);
        let p2 = parse_stun(&corrupt_mi).expect("still parses");
        assert!(verify_fingerprint(&corrupt_mi, &p2), "resealed FP must verify");
        assert!(!verify_message_integrity(&corrupt_mi, &p2, &req.integrity_key), "flipped MI byte must fail");

        let mut corrupt_fp = msg.clone();
        let last = corrupt_fp.len() - 1;
        corrupt_fp[last] ^= 0x80;
        let p3 = parse_stun(&corrupt_fp).expect("still parses");
        assert!(!verify_fingerprint(&corrupt_fp, &p3), "flipped FP byte must fail");
        assert!(verify_message_integrity(&corrupt_fp, &p3, &req.integrity_key), "MI unaffected by FP byte flip");
    }

    /// Recompute and patch the FINGERPRINT value in place (test helper —
    /// corrupt-then-reseal isolates MI failures from FP coverage).
    fn reseal_fingerprint(msg: &mut [u8]) {
        let fp_off = msg.len() - 8;
        let mut input = msg[..fp_off].to_vec();
        set_msg_len(&mut input, (msg.len() - HEADER_LEN) as u16);
        let crc = crc32_ieee(&input) ^ FINGERPRINT_XOR;
        msg[fp_off + 4..fp_off + 8].copy_from_slice(&crc.to_be_bytes());
    }

    /// N11 demux vectors (upstream ice_bind.go:403-421): all four WG message
    /// types classify as WG at their exact wire sizes, STUN checks do not,
    /// short WG-shaped noise does not, and the cookie-overlap transport
    /// packet (receiver index == magic cookie) stays WG — the misroute the
    /// upstream comment pins.
    #[test]
    fn wg_vs_stun_classification_matches_upstream_demux() {
        // WG handshake initiation / response / cookie / transport-keepalive.
        let mut init = vec![0u8; 148];
        init[0] = 1;
        let mut resp = vec![0u8; 92];
        resp[0] = 2;
        let mut cookie = vec![0u8; 64];
        cookie[0] = 3;
        let mut ka = vec![0u8; 32];
        ka[0] = 4;
        for (name, pkt) in [("init", &init[..]), ("resp", &resp[..]), ("cookie", &cookie[..]), ("ka", &ka[..])] {
            assert!(is_wg_datagram(pkt), "{name} must classify as WG");
        }
        // Receiver index coincides with the STUN magic cookie: the WG-shaped
        // disjunct runs FIRST, so the datagram routes to WG even though the
        // cookie now sits in STUN position (ice_bind.go:408-420).
        let mut overlap = ka.clone();
        overlap[4..8].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
        assert!(is_wg_datagram(&overlap), "cookie-overlap WG packet must stay WG");
        assert!(
            demux_routes_to_wg(&overlap),
            "cookie-overlap WG packet must route to WG, not the STUN handler"
        );
        // 31 bytes = one short of wgMinMsgSize → not provably WG.
        assert!(!is_wg_datagram(&init[..31]));
        // A real ICE check is STUN, never WG.
        let req = CheckRequest {
            username: "REMOTEufragXXXXXX:LOCALufragXXXXX".into(),
            priority: priority_for(CandidateType::Prflx),
            controlling: true,
            tie_breaker: 9,
            use_candidate: false,
            integrity_key: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef".into(),
        };
        let stun_req = build_check_request(&req, stun::random_transaction_id().expect("entropy"));
        assert!(!is_wg_datagram(&stun_req), "STUN check must not classify as WG");
        assert!(is_stun_message(&stun_req));
        // Non-STUN non-WG noise: neither classifier claims it, and the
        // upstream predicate routes it to WG (`!stun.IsMessage` branch).
        let noise = vec![0xABu8; 60];
        assert!(!is_wg_datagram(&noise) && !is_stun_message(&noise));
        assert!(demux_routes_to_wg(&noise));
        // Malformed STUN (cookie present, garbage attributes): STUN-shaped →
        // NOT to WG; the ICE layer drops it after a parse failure.
        let mut bad = stun_req.clone();
        bad.truncate(24); // header claims more attributes than exist
        assert!(!demux_routes_to_wg(&bad), "STUN-shaped stays out of WG");
        assert!(parse_stun(&bad).is_err());
    }

    /// N11: the data queue is bounded — overflowing drops the newest
    /// datagram and counts, never grows without bound.
    #[test]
    fn data_rx_queue_is_bounded_and_counts_drops() {
        let mut session = IceSession::new(
            IceCredentials { ufrag: "abcd".into(), pwd: "0123456789012345678901".into() },
            true,
            Some(5),
        )
        .expect("session");
        let pkt = vec![4u8, 0, 0, 0, 0, 0, 0, 0]; // + padding below via full size
        for i in 0..(DATA_RX_CAP + 8) {
            let mut d = pkt.clone();
            d.push(i as u8);
            session.push_data_rx([127, 0, 0, 1], 100, &d);
        }
        assert_eq!(session.data_rx.len(), DATA_RX_CAP, "queue stays at cap");
        assert_eq!(session.data_dropped(), 8, "overflow counted");
        let drained = session.take_data_rx();
        assert_eq!(drained.len(), DATA_RX_CAP);
        assert_eq!(drained[0].2[0], 4, "oldest kept (arrival order)");
        assert!(session.take_data_rx().is_empty(), "drain empties");
    }

    fn hex20(s: &str) -> [u8; 20] {
        let mut out = [0u8; 20];
        for i in 0..20 {
            out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex");
        }
        out
    }
}
