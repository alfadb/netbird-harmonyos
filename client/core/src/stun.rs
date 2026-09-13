// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # stun — minimal STUN client codec for srflx gathering (N5a)
//!
//! Self-built RFC 5389 subset: Binding Request construction, Binding
//! Success/Error Response parsing with XOR-MAPPED-ADDRESS / MAPPED-ADDRESS /
//! ERROR-CODE, and transaction-id entropy. Pure codec — every function here
//! is I/O-free except [`random_transaction_id`] (a fixed `/dev/urandom`
//! read), which keeps the whole module table-testable (RFC 5769 vectors live
//! in `tests/ice_stun_codec.rs`).
//!
//! ## Why self-built (dependency probe, N5a)
//!
//! The webrtc-ice / stun / turn crate family would pull a multi-crate tree
//! whose sockets are created INSIDE the library — un-gateable through the
//! protected-socket seam (governance `docs/native-nx-governance.md` §二.4
//! requires every ICE/STUN/TURN socket to be protected BEFORE use,
//! fail-closed). N5a needs exactly one STUN exchange (Binding →
//! XOR-MAPPED-ADDRESS), so the codec is self-built (~200 lines, zero
//! dependencies, offline/locked unchanged) and the agent machinery stays
//! upstream-shaped but deferred (see `ice.rs` / `docs/n3-ice-notes.md`).
//! Cross-compile surface verified in the repo-external probe
//! `~/harmonyos-signing/netbird-n1bdisc/refs/ice-probe` (exit 0,
//! aarch64-unknown-linux-ohos, zero crates).
//!
//! ## Upstream anchor (pinned commit `791401060d2b`)
//!
//! The srflx flow this codec serves is upstream's pion/stun usage:
//! `client/internal/engine.go:1525-1541 updateSTUNs` parses each
//! `NetbirdConfig.stuns[].uri` with `stun.ParseURI`; the URIs ride into the
//! agent as `Urls` (`client/internal/peer/ice/agent.go:54`), where pion
//! sends Binding Requests and derives srflx candidates. CandidateTypes
//! includes `ServerReflexive` (`agent.go:127-133`). We reproduce exactly
//! that exchange; nothing more (no MESSAGE-INTEGRITY, no FINGERPRINT
//! verification, no retransmit backoff — N5b scope, listed in the notes).
//!
//! ## Error taxonomy
//!
//! No new error type: every failure maps onto the existing
//! [`ManagementError`] classes with stable `stun:*` tokens —
//! [`ManagementError::Network`] for entropy I/O, [`ManagementError::Parse`]
//! for every malformed/inauthentic response shape. A server ERROR-CODE
//! response is NOT a parse failure; it is [`StunReply::Error`].

use crate::management::ManagementError;
use crate::sys;

/// RFC 5389 §6 fixed magic cookie.
pub const MAGIC_COOKIE: u32 = 0x2112_A442;
/// RFC 5389 §5 message types used by this client.
pub const BINDING_REQUEST: u16 = 0x0001;
pub const BINDING_SUCCESS: u16 = 0x0101;
pub const BINDING_ERROR: u16 = 0x0111;
/// Attribute types (RFC 5389 §18.2): MAPPED-ADDRESS, ERROR-CODE,
/// XOR-MAPPED-ADDRESS.
pub const ATTR_MAPPED: u16 = 0x0001;
pub const ATTR_ERROR_CODE: u16 = 0x0009;
pub const ATTR_XOR_MAPPED: u16 = 0x0020;
/// STUN header (type 2 + length 2 + cookie 4 + txn id 12) and our request
/// size (header only — no attributes; RFC 5389 §2.2 an empty Binding Request
/// is legal and every compliant server answers XOR-MAPPED-ADDRESS).
pub const HEADER_LEN: usize = 20;
pub const REQUEST_LEN: usize = 20;

/// 96-bit transaction identifier (RFC 5389 §6). Equality against our own
/// outstanding ids is the ONLY response-authenticity check in N5a — entropy
/// is therefore mandatory, and an unreadable `/dev/urandom` fails the
/// exchange (never a constant id).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionId(pub [u8; 12]);

/// Decoded outcome of a response matched to our transaction id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StunReply {
    /// Server-reflexive mapping: XOR-MAPPED-ADDRESS preferred,
    /// MAPPED-ADDRESS fallback (RFC 5389 §15). IPv4 only — the N5a gather
    /// is UDP4 (upstream `DisableIPv6Discovery` shape, agent.go:64-68).
    Mapped { addr: [u8; 4], port: u16 },
    /// Valid Binding Error Response carrying ERROR-CODE `code`
    /// (class × 100 + number, e.g. 400/420/500; RFC 5389 §15.6).
    Error(u16),
}

impl TransactionId {
    fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

/// 12 fresh bytes from `/dev/urandom` (fixed path, `O_RDONLY` — the sys.rs
/// openat whitelist shape; path extension registered in
/// `docs/n3-ice-notes.md` §fd/syscall surface). Any I/O failure (open,
/// short read, EINTR twice) is [`ManagementError::Network`] — fail-closed:
/// an unpredictable transaction id is what makes off-path response forgery
/// harder than reading our txns off the wire.
pub fn random_transaction_id() -> Result<TransactionId, ManagementError> {
    const PATH: &[u8] = b"/dev/urandom\0";
    let mut id = [0u8; 12];
    let fd = unsafe { sys::openat(sys::AT_FDCWD, PATH.as_ptr(), sys::O_RDONLY) };
    if fd < 0 {
        return Err(ManagementError::Network("stun:txn-entropy-unavailable".into()));
    }
    let mut filled = 0usize;
    let mut interrupted = 0u8;
    while filled < id.len() {
        let (n, _e) = sys::read_fd(fd, &mut id[filled..]);
        if n > 0 {
            filled += n as usize;
            interrupted = 0;
            continue;
        }
        if n < 0 && sys::errno() == sys::EINTR && interrupted < 2 {
            interrupted += 1;
            continue;
        }
        unsafe { sys::close(fd) };
        return Err(ManagementError::Network("stun:txn-entropy-short-read".into()));
    }
    unsafe { sys::close(fd) };
    Ok(TransactionId(id))
}

/// One empty Binding Request (RFC 5389 §2.2/§6): type, length 0, magic
/// cookie, transaction id. No SOFTWARE/FINGERPRINT — N5a minimality; both
/// are optional to a compliant server.
pub fn build_binding_request(id: TransactionId) -> [u8; REQUEST_LEN] {
    let mut msg = [0u8; REQUEST_LEN];
    msg[0..2].copy_from_slice(&BINDING_REQUEST.to_be_bytes());
    msg[2..4].copy_from_slice(&0u16.to_be_bytes());
    msg[4..8].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
    msg[8..20].copy_from_slice(id.as_slice());
    msg
}

/// Parse ONE datagram expected to be the response for `id`.
///
/// Strict where it is cheap: length ≥ header, declared message length fits
/// the datagram, magic cookie equal, transaction id EQUAL (mismatching
/// datagrams happen — other transactions can land on the same socket — and
/// are parse failures with token `stun:txn-mismatch`), attribute walk fully
/// bounds-checked, padding per RFC 5389 §15. The RFC 5769 §2.2/§2.3 vectors
/// (with SOFTWARE + MESSAGE-INTEGRITY + FINGERPRINT attributes before/after
/// XOR-MAPPED-ADDRESS) pin the walk in `tests/ice_stun_codec.rs`.
pub fn parse_binding_response(msg: &[u8], id: &TransactionId) -> Result<StunReply, ManagementError> {
    let bad = |tok: &'static str| ManagementError::Parse(format!("stun:{tok}"));
    if msg.len() < HEADER_LEN {
        return Err(bad("short-header"));
    }
    let mtype = u16::from_be_bytes([msg[0], msg[1]]);
    let mlen = u16::from_be_bytes([msg[2], msg[3]]) as usize;
    if msg.len() < HEADER_LEN + mlen {
        return Err(bad("truncated"));
    }
    let cookie = u32::from_be_bytes([msg[4], msg[5], msg[6], msg[7]]);
    if cookie != MAGIC_COOKIE {
        return Err(bad("bad-magic"));
    }
    let mut txn = [0u8; 12];
    txn.copy_from_slice(&msg[8..20]);
    if txn != id.0 {
        return Err(bad("txn-mismatch"));
    }
    match mtype {
        BINDING_SUCCESS => parse_success(&msg[HEADER_LEN..HEADER_LEN + mlen]).ok_or_else(|| bad("no-ipv4-mapping")),
        BINDING_ERROR => {
            let code = parse_error_code(&msg[HEADER_LEN..HEADER_LEN + mlen]).ok_or_else(|| bad("error-unreadable"))?;
            Ok(StunReply::Error(code))
        }
        _ => Err(bad("unexpected-type")),
    }
}

/// Walk a success body: XOR-MAPPED-ADDRESS preferred, MAPPED-ADDRESS
/// fallback; `None` when no IPv4 mapping is present. Unknown attributes are
/// skipped (SOFTWARE, MESSAGE-INTEGRITY, FINGERPRINT, ...); a declared
/// attribute length that crosses the body end poisons the walk (`None`).
fn parse_success(body: &[u8]) -> Option<StunReply> {
    let mut mapped: Option<([u8; 4], u16)> = None;
    let mut off = 0usize;
    while off + 4 <= body.len() {
        let atype = u16::from_be_bytes([body[off], body[off + 1]]);
        let alen = u16::from_be_bytes([body[off + 2], body[off + 3]]) as usize;
        off += 4;
        if off + alen > body.len() {
            return None; // attribute crosses the declared end
        }
        let val = &body[off..off + alen];
        let decoded = match atype {
            ATTR_XOR_MAPPED => decode_address(val, true),
            ATTR_MAPPED if mapped.is_none() => decode_address(val, false),
            _ => None,
        };
        if let Some((addr, port)) = decoded {
            if atype == ATTR_XOR_MAPPED {
                return Some(StunReply::Mapped { addr, port });
            }
            mapped = Some((addr, port));
        }
        off += (alen + 3) & !3; // 4-byte value alignment (RFC 5389 §15)
    }
    mapped.map(|(addr, port)| StunReply::Mapped { addr, port })
}

/// RFC 5389 §15.1 address value: `0x00 | family | port(2, BE) | addr`.
/// `family` 0x01 = IPv4; XOR form un-masks port with the cookie's high half
/// and each address byte with the corresponding cookie byte.
///
/// `pub(crate)` since N5b: `ice_session.rs` reuses this for XOR-MAPPED-
/// ADDRESS on connectivity-check responses (same wire format, same
/// UDP4-only semantics — no second decoder).
pub(crate) fn decode_address(val: &[u8], xored: bool) -> Option<([u8; 4], u16)> {
    if val.len() < 8 || val[0] != 0x00 {
        return None;
    }
    match val[1] {
        0x01 => {}
        _ => return None, // IPv6 (RFC 5769 §2.3 shape) — UDP4-only gather
    }
    let mut port = u16::from_be_bytes([val[2], val[3]]);
    let mut addr = [0u8; 4];
    addr.copy_from_slice(&val[4..8]);
    if xored {
        port ^= (MAGIC_COOKIE >> 16) as u16;
        for (i, b) in addr.iter_mut().enumerate() {
            *b ^= (MAGIC_COOKIE >> (24 - 8 * i)) as u8;
        }
    }
    Some((addr, port))
}

/// RFC 5389 §15.6 ERROR-CODE value: `0x00 | 0x00 | class(3 bits) | number`,
/// then reason phrase. Code = class × 100 + number (class 4 + number 20
/// → 420 Unknown Attribute).
fn parse_error_code(body: &[u8]) -> Option<u16> {
    let mut off = 0usize;
    while off + 4 <= body.len() {
        let atype = u16::from_be_bytes([body[off], body[off + 1]]);
        let alen = u16::from_be_bytes([body[off + 2], body[off + 3]]) as usize;
        off += 4;
        if off + alen > body.len() {
            return None;
        }
        if atype == ATTR_ERROR_CODE && alen >= 4 {
            let class = (body[off + 2] & 0x07) as u16;
            let number = body[off + 3] as u16;
            return Some(class * 100 + number);
        }
        off += (alen + 3) & !3;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txn(b: u8) -> TransactionId {
        TransactionId([b; 12])
    }

    /// The RFC 5769 §2.2 Sample IPv4 Response, verbatim bytes; SOFTWARE,
    /// MESSAGE-INTEGRITY and FINGERPRINT attributes ride along un-touched.
    const RFC5769_RESPV4: &[u8] = &[
        0x01, 0x01, 0x00, 0x3c, 0x21, 0x12, 0xa4, 0x42, 0xb7, 0xe7, 0xa7, 0x01, 0xbc, 0x34, 0xd6,
        0x86, 0xfa, 0x87, 0xdf, 0xae, 0x80, 0x22, 0x00, 0x0b, 0x74, 0x65, 0x73, 0x74, 0x20, 0x76,
        0x65, 0x63, 0x74, 0x6f, 0x72, 0x20, 0x00, 0x20, 0x00, 0x08, 0x00, 0x01, 0xa1, 0x47, 0xe1,
        0x12, 0xa6, 0x43, 0x00, 0x08, 0x00, 0x14, 0x2b, 0x91, 0xf5, 0x99, 0xfd, 0x9e, 0x90, 0xc3,
        0x8c, 0x74, 0x89, 0xf9, 0x2a, 0xf9, 0xba, 0x53, 0xf0, 0x6b, 0xe7, 0xd7, 0x80, 0x28, 0x00,
        0x04, 0xc0, 0x7d, 0x4c, 0x96,
    ];
    const RFC5769_TXN: TransactionId = TransactionId([
        0xb7, 0xe7, 0xa7, 0x01, 0xbc, 0x34, 0xd6, 0x86, 0xfa, 0x87, 0xdf, 0xae,
    ]);

    /// RFC 5769 §2.1 Sample Request pins the request builder: the fixed
    /// 20-byte prefix (type/length/cookie/txn) matches the vector's header
    /// field for field (the vector itself carries attributes we never emit;
    /// the RESPONSE vector's header differs and must not be compared here).
    #[test]
    fn request_header_matches_rfc5769() {
        let req = build_binding_request(RFC5769_TXN);
        assert_eq!(&req[0..2], &[0x00, 0x01]);
        assert_eq!(&req[2..4], &[0x00, 0x00]);
        assert_eq!(&req[4..8], &[0x21, 0x12, 0xa4, 0x42]);
        assert_eq!(&req[8..20], &RFC5769_TXN.0[..]);
    }

    /// RFC 5769 §2.2: XOR-MAPPED-ADDRESS `00 01 a1 47 e1 12 a6 43` decodes
    /// to 192.0.2.1:32853 despite SOFTWARE (before) and MESSAGE-INTEGRITY +
    /// FINGERPRINT (after) sharing the attribute walk.
    #[test]
    fn parses_rfc5769_ipv4_response() {
        let reply = parse_binding_response(RFC5769_RESPV4, &RFC5769_TXN).expect("vector parses");
        assert_eq!(reply, StunReply::Mapped { addr: [192, 0, 2, 1], port: 32853 });
    }

    /// RFC 5769 §2.3 Sample IPv6 Response: valid in every byte except the
    /// family (0x02) this UDP4-only client consumes → explicit parse error,
    /// never a garbage mapping.
    #[test]
    fn rejects_rfc5769_ipv6_response() {
        let respv6: Vec<u8> = vec![
            0x01, 0x01, 0x00, 0x48, 0x21, 0x12, 0xa4, 0x42, 0xb7, 0xe7, 0xa7, 0x01, 0xbc, 0x34,
            0xd6, 0x86, 0xfa, 0x87, 0xdf, 0xae, 0x80, 0x22, 0x00, 0x0b, 0x74, 0x65, 0x73, 0x74,
            0x20, 0x76, 0x65, 0x63, 0x74, 0x6f, 0x72, 0x20, 0x00, 0x20, 0x00, 0x14, 0x00, 0x02,
            0xa1, 0x47, 0x01, 0x13, 0xa9, 0xfa, 0xa5, 0xd3, 0xf1, 0x79, 0xbc, 0x25, 0xf4, 0xb5,
            0xbe, 0xd2, 0xb9, 0xd9, 0x00, 0x08, 0x00, 0x14, 0xa3, 0x82, 0x95, 0x4e, 0x4b, 0xe6,
            0x7b, 0xf1, 0x17, 0x84, 0xc9, 0x7c, 0x82, 0x92, 0xc2, 0x75, 0xbf, 0xe3, 0xed, 0x41,
            0x80, 0x28, 0x00, 0x04, 0xc8, 0xfb, 0x0b, 0x4c,
        ];
        let err = parse_binding_response(&respv6, &RFC5769_TXN).unwrap_err();
        assert_eq!(err, ManagementError::Parse("stun:no-ipv4-mapping".into()));
    }

    /// MAPPED-ADDRESS fallback (non-xor, RFC 5389 §15.1) when the server is
    /// RFC 3489-shaped and omits XOR-MAPPED-ADDRESS.
    #[test]
    fn falls_back_to_plain_mapped_address() {
        let mut msg = Vec::new();
        msg.extend_from_slice(&BINDING_SUCCESS.to_be_bytes());
        msg.extend_from_slice(&12u16.to_be_bytes());
        msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        msg.extend_from_slice(&txn(9).0);
        msg.extend_from_slice(&ATTR_MAPPED.to_be_bytes());
        msg.extend_from_slice(&8u16.to_be_bytes());
        msg.extend_from_slice(&[0x00, 0x01]);
        msg.extend_from_slice(&7777u16.to_be_bytes());
        msg.extend_from_slice(&[203, 0, 113, 9]);
        let reply = parse_binding_response(&msg, &txn(9)).expect("mapped parses");
        assert_eq!(reply, StunReply::Mapped { addr: [203, 0, 113, 9], port: 7777 });
    }

    /// A valid Binding Error Response is a legitimate reply — classified as
    /// [`StunReply::Error`] with the RFC 5389 §15.6 code, not a Parse error.
    #[test]
    fn error_response_decodes_error_code() {
        let mut msg = Vec::new();
        msg.extend_from_slice(&BINDING_ERROR.to_be_bytes());
        msg.extend_from_slice(&8u16.to_be_bytes());
        msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        msg.extend_from_slice(&txn(3).0);
        msg.extend_from_slice(&ATTR_ERROR_CODE.to_be_bytes());
        msg.extend_from_slice(&4u16.to_be_bytes());
        msg.extend_from_slice(&[0x00, 0x00, 0x04, 0x14]); // class 4, number 20 → 420
        let reply = parse_binding_response(&msg, &txn(3)).expect("error parses");
        assert_eq!(reply, StunReply::Error(420));
    }

    /// Every inauthentic/truncated shape is a `stun:*` Parse error and —
    /// the point of the table — never a panic.
    #[test]
    fn malformed_shapes_are_parse_errors() {
        let ok = build_binding_response_fixture(&txn(5));
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("empty", Vec::new()),
            ("19-bytes", ok[..19].to_vec()),
            ("declared-overrun", {
                let mut m = ok.clone();
                m[2..4].copy_from_slice(&999u16.to_be_bytes());
                m
            }),
            ("bad-magic", {
                let mut m = ok.clone();
                m[4] ^= 0xff;
                m
            }),
            ("txn-mismatch", {
                let mut m = ok.clone();
                m[8] ^= 0xff;
                m
            }),
            ("unexpected-type", {
                let mut m = ok.clone();
                m[0..2].copy_from_slice(&0x0111u16.to_be_bytes());
                m
            }),
            ("attr-overrun", {
                let mut m = ok.clone();
                m[2..4].copy_from_slice(&8u16.to_be_bytes());
                let attr = ATTR_XOR_MAPPED.to_be_bytes();
                m[20..22].copy_from_slice(&attr);
                m[22..24].copy_from_slice(&200u16.to_be_bytes());
                m
            }),
            ("random-noise", (0..64u8).cycle().take(60).collect()),
        ];
        for (name, datagram) in cases {
            let err = parse_binding_response(&datagram, &txn(5)).err().unwrap_or_else(|| {
                panic!("{name} must be rejected, not accepted")
            });
            assert!(
                matches!(&err, ManagementError::Parse(t) if t.starts_with("stun:")),
                "{name} misclassified: {err:?}"
            );
        }
    }

    fn build_binding_response_fixture(id: &TransactionId) -> Vec<u8> {
        let mut msg = Vec::new();
        msg.extend_from_slice(&BINDING_SUCCESS.to_be_bytes());
        msg.extend_from_slice(&8u16.to_be_bytes());
        msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        msg.extend_from_slice(&id.0);
        msg.extend_from_slice(&ATTR_XOR_MAPPED.to_be_bytes());
        msg.extend_from_slice(&8u16.to_be_bytes());
        msg.extend_from_slice(&[0x00, 0x01, 0x00, 0x00, 1, 2, 3, 4]);
        msg
    }
}
