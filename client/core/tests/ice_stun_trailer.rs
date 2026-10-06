// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! ICE STUN trailer / post-integrity shape tests (synthetic, wire-level).
//!
//! Official `netbirdio/ice` v4 (commit 6202be846b51, `candidatepair.go`)
//! appends a NON-STUN-standard candidate-pair id attribute (type `0x8100`,
//! 65-byte value) AFTER FINGERPRINT and updates the STUN header length to
//! cover it — without recomputing the FINGERPRINT CRC. Every such datagram
//! therefore has a trailer past the FP boundary.
//!
//! Two wire rules are pinned here, both of which the fixed verifier/parser
//! must respect and neither of which the old code did:
//!
//! 1. FINGERPRINT input ends at the FINGERPRINT attribute (header length
//!    pointing at the END of FP, RFC 5389 §15.5) — not at the end of the
//!    datagram. A trailer after FP must not invalidate an otherwise correct
//!    fingerprint, and a fingerprint computed over the trailer length is NOT
//!    a valid fingerprint.
//! 2. MESSAGE-INTEGRITY is the last attribute before FINGERPRINT (§15.4), so
//!    nothing appended after MI is authenticated. Appended USERNAME /
//!    PRIORITY / role / USE-CANDIDATE must NOT override the authenticated
//!    values (the old last-wins walk let exactly that happen), a duplicate MI
//!    must not displace the first one, and bytes past the declared STUN
//!    length must not be read at all.
//!
//! Everything is built from the crate's own builders plus throwaway test
//! secrets; no captured traffic, hostnames or pcap identifiers are used.

// Host-test link surface (same as tests/ice_stun_uri.rs): the integration
// test binary links the whole crate rlib on the host triple, where
// libace_napi.z.so / libhilog_ndk.z.so do not exist. These no-ops satisfy the
// linker only; nothing below calls into them.
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
        _cb: *const u8,
        _data: *mut c_void,
        _result: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_set_named_property(
        _env: *mut c_void,
        _name: *const c_void,
        _value: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _env: *mut c_void,
        _str_: *const u8,
        _len: usize,
        _result: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_cb_info(
        _env: *mut c_void,
        _cbinfo: *mut c_void,
        _argc: *mut usize,
        _argv: *mut *mut c_void,
        _this_arg: *mut *mut c_void,
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
    pub extern "C" fn napi_get_value_int32(
        _env: *mut c_void,
        _value: *mut c_void,
        _result: *mut i32,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(
        _env: *mut c_void,
        _value: *mut c_void,
        _result: *mut bool,
    ) -> i32 {
        0
    }
}

use netbird_core::ice_session::{
    build_check_request, build_success_response, parse_stun, verify_fingerprint,
    verify_message_integrity, CheckRequest, ATTR_FINGERPRINT, ATTR_ICE_CONTROLLING,
    ATTR_MESSAGE_INTEGRITY, ATTR_PRIORITY, ATTR_USE_CANDIDATE, ATTR_USERNAME,
};
use netbird_core::stun::{TransactionId, BINDING_REQUEST, BINDING_SUCCESS};

// ---------------------------------------------------------------------------
// synthetic fixtures (throwaway values, deliberately not device/pcap data)
// ---------------------------------------------------------------------------

const HEADER_LEN: usize = 20;
/// RFC 5389 §15.5: CRC-32 ⊕ ASCII "STUN".
const FINGERPRINT_XOR: u32 = 0x5354_554e;
/// RFC 5389 magic cookie, for the hand-built malformed header below.
const MAGIC_COOKIE: u32 = 0x2112_A442;
/// Official-ice private attribute appended after FINGERPRINT (0x8100).
const CANDIDATE_PAIR_ID: u16 = 0x8100;

const PEER_UFRAG: &str = "PEERufragAAAAAAAA";
const LOCAL_UFRAG: &str = "LOCALufragAAAAAAA";
const KEY: &str = "throwaway-test-key-32-bytes-ok!";
const WRONG_KEY: &str = "wrong-throwaway-key-different";
const PRIORITY: u32 = 0x7E00_00FF;
const TIE_BREAKER: u64 = 0x0A0B_0C0D_0E0F_1011;
const TXN: TransactionId = TransactionId([0x21; 12]);

fn expected_username() -> String {
    format!("{PEER_UFRAG}:{LOCAL_UFRAG}")
}

fn check_request() -> CheckRequest {
    CheckRequest {
        username: expected_username(),
        priority: PRIORITY,
        controlling: true,
        tie_breaker: TIE_BREAKER,
        use_candidate: false,
        integrity_key: KEY.to_string(),
    }
}

fn sealed_request() -> Vec<u8> {
    build_check_request(&check_request(), TXN)
}

// ---------------------------------------------------------------------------
// wire helpers (the crate's seal helpers are private; these re-derive only
// what a test needs: attribute layout, CRC-32, header length)
// ---------------------------------------------------------------------------

/// CRC-32 (IEEE 802.3, reflected, init/final `0xFFFF_FFFF`) — the same
/// polynomial the RFC 5389 §15.5 FINGERPRINT uses.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn set_len(msg: &mut [u8], len: u16) {
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

/// Attribute walk over the DECLARED STUN body (first match wins here — the
/// fixtures below carry exactly one of each attribute before any trailer):
/// `(absolute offset of the attribute, type, value length)`.
fn attrs(msg: &[u8]) -> Vec<(usize, u16, usize)> {
    let mlen = u16::from_be_bytes([msg[2], msg[3]]) as usize;
    let body = &msg[HEADER_LEN..HEADER_LEN + mlen];
    let mut out = Vec::new();
    let mut off = 0usize;
    while off + 4 <= body.len() {
        let typ = u16::from_be_bytes([body[off], body[off + 1]]);
        let len = u16::from_be_bytes([body[off + 2], body[off + 3]]) as usize;
        if off + 4 + len > body.len() {
            break;
        }
        out.push((HEADER_LEN + off, typ, len));
        off += 4 + ((len + 3) & !3);
    }
    out
}

fn attr_offset(msg: &[u8], typ: u16) -> usize {
    attrs(msg)
        .into_iter()
        .find(|(_, t, _)| *t == typ)
        .map(|(o, _, _)| o)
        .unwrap_or_else(|| panic!("attribute 0x{typ:04x} present"))
}

fn attr_end(msg: &[u8], typ: u16) -> usize {
    let off = attr_offset(msg, typ);
    let len = u16::from_be_bytes([msg[off + 2], msg[off + 3]]) as usize;
    off + 4 + len
}

/// Start of the (first) FINGERPRINT attribute.
fn fp_start(msg: &[u8]) -> usize {
    let off = attr_offset(msg, ATTR_FINGERPRINT);
    assert_eq!(u16::from_be_bytes([msg[off + 2], msg[off + 3]]), 4, "FP is 4 bytes");
    off
}

/// Re-seal FINGERPRINT at the RFC 5389 §15.5 boundary: CRC over the message
/// up to FP, header length field pointing at the END of FP (`off + 8`) —
/// trailer bytes after FP are deliberately outside the input.
fn reseal_fp_at_fp_boundary(msg: &mut [u8]) {
    let off = fp_start(msg);
    let mut input = msg[..off].to_vec();
    set_len(&mut input, (off + 8 - HEADER_LEN) as u16);
    let crc = crc32(&input) ^ FINGERPRINT_XOR;
    msg[off + 4..off + 8].copy_from_slice(&crc.to_be_bytes());
}

/// Re-seal FINGERPRINT the way the OLD verifier computed it: header length =
/// the whole datagram. Used to prove such a value is now rejected.
fn reseal_fp_at_datagram_boundary(msg: &mut [u8]) {
    let off = fp_start(msg);
    let mut input = msg[..off].to_vec();
    set_len(&mut input, (msg.len() - HEADER_LEN) as u16);
    let crc = crc32(&input) ^ FINGERPRINT_XOR;
    msg[off + 4..off + 8].copy_from_slice(&crc.to_be_bytes());
}

/// Splice `extra` into the datagram at `at` and widen the declared STUN
/// length to cover the result.
fn insert_at(msg: &mut Vec<u8>, at: usize, extra: &[u8]) {
    let tail = msg.split_off(at);
    msg.extend_from_slice(extra);
    msg.extend_from_slice(&tail);
    let total = (msg.len() - HEADER_LEN) as u16;
    set_len(msg, total);
}

/// Append the official post-FINGERPRINT candidate-pair id trailer and widen
/// the declared STUN length to cover it — WITHOUT touching the FP value
/// (exactly what candidatepair.go does).
fn append_candidate_pair_id_trailer(msg: &mut Vec<u8>) {
    let before = msg.len();
    push_attr(msg, CANDIDATE_PAIR_ID, &[0x5A; 65]);
    let total = (msg.len() - HEADER_LEN) as u16;
    set_len(msg, total);
    assert!(msg.len() > before + 8, "trailer really rides past the FP attribute");
}

/// The four-field override payload appended by the "attacker" fixtures.
fn override_attributes() -> Vec<u8> {
    let mut extra = Vec::new();
    push_attr(&mut extra, ATTR_USERNAME, b"ATTACKERufragAAAAAAAA:ATTACKERufragAAAAAA");
    push_attr(&mut extra, ATTR_PRIORITY, &0xDEAD_BEEFu32.to_be_bytes());
    push_attr(&mut extra, ATTR_ICE_CONTROLLING, &0xFFFF_FFFF_FFFF_FFFFu64.to_be_bytes());
    push_attr(&mut extra, ATTR_USE_CANDIDATE, &[]);
    extra
}

/// Assert every authenticated request field is the builder's original value.
fn assert_authenticated_request_fields(p: &netbird_core::ice_session::ParsedStun) {
    assert_eq!(
        p.username.as_deref(),
        Some(expected_username().as_str()),
        "appended USERNAME must not override the authenticated one"
    );
    assert_eq!(p.priority, Some(PRIORITY), "appended PRIORITY must not override");
    assert_eq!(p.controlling, Some(TIE_BREAKER), "appended role must not override");
    assert!(p.controlled.is_none(), "no ICE-CONTROLLED was ever authenticated");
    assert!(!p.use_candidate, "appended USE-CANDIDATE must not override");
}

// ---------------------------------------------------------------------------
// 1) the official trailer after FINGERPRINT
// ---------------------------------------------------------------------------

/// Request: MI + a correct (pre-trailer) FP + the 0x8100 trailer inside the
/// declared STUN length. Both checks must pass and the fields must be the
/// authenticated ones.
#[test]
fn official_candidate_pair_id_trailer_after_fingerprint_request_verifies() {
    let mut msg = sealed_request();
    append_candidate_pair_id_trailer(&mut msg);

    let p = parse_stun(&msg).expect("trailer message still parses");
    assert_eq!(p.msg_type, BINDING_REQUEST);
    assert_eq!(p.txn, TXN);
    assert_authenticated_request_fields(&p);
    assert!(verify_message_integrity(&msg, &p, KEY), "MI covers only the pre-MI prefix");
    assert!(verify_fingerprint(&msg, &p), "FP input ends at FP, trailer excluded");
    assert!(!verify_message_integrity(&msg, &p, WRONG_KEY), "MI is still a real key check");
}

/// Success response carrying the same trailer (XOR-MAPPED-ADDRESS + MI + FP
/// + 0x8100) verifies on both checks and still decodes its mapped address.
#[test]
fn official_candidate_pair_id_trailer_after_fingerprint_success_response_verifies() {
    let mut msg = build_success_response(TXN, ([192, 0, 2, 9], 50000), KEY);
    append_candidate_pair_id_trailer(&mut msg);

    let p = parse_stun(&msg).expect("trailer response parses");
    assert_eq!(p.msg_type, BINDING_SUCCESS);
    assert_eq!(p.xor_mapped, Some(([192, 0, 2, 9], 50000)), "mapped address survives the trailer");
    assert!(verify_message_integrity(&msg, &p, KEY));
    assert!(verify_fingerprint(&msg, &p));
}

/// A FINGERPRINT computed over the WHOLE datagram (trailer included, i.e.
/// the old input length) is not a valid STUN fingerprint and must be
/// rejected.
#[test]
fn fingerprint_computed_over_a_trailer_length_is_rejected() {
    let mut msg = sealed_request();
    append_candidate_pair_id_trailer(&mut msg);
    reseal_fp_at_datagram_boundary(&mut msg);

    let p = parse_stun(&msg).expect("parses");
    assert!(
        !verify_fingerprint(&msg, &p),
        "FP input length must end at the FP attribute, not at the datagram end"
    );
}

// ---------------------------------------------------------------------------
// 2) attributes appended after MESSAGE-INTEGRITY / FINGERPRINT are inert
// ---------------------------------------------------------------------------

/// The override attempt: USERNAME / PRIORITY / ICE-CONTROLLING /
/// USE-CANDIDATE spliced in AFTER MI (before FP), with the FP correctly
/// re-sealed at the FP boundary so the FINGERPRINT gate passes. The
/// authenticated fields must be unchanged — MI never covered the appendage.
#[test]
fn attributes_appended_after_message_integrity_cannot_override_authenticated_fields() {
    let base = sealed_request();
    let mi_end = attr_end(&base, ATTR_MESSAGE_INTEGRITY);
    let mut msg = base.clone();
    insert_at(&mut msg, mi_end, &override_attributes());
    reseal_fp_at_fp_boundary(&mut msg);

    let p = parse_stun(&msg).expect("parses");
    // Fields first: this is the bypass the fix closes (old last-wins walk
    // returned the appended USERNAME/priority/role/USE-CANDIDATE here).
    assert_authenticated_request_fields(&p);
    assert!(verify_fingerprint(&msg, &p), "resealed FP at the FP boundary is valid");
    assert!(verify_message_integrity(&msg, &p, KEY), "the FIRST MI still authenticates");
}

/// Same override attempt appended AFTER FINGERPRINT, inside the declared
/// STUN length. The walk stops at FP, so the trailer is never parsed.
#[test]
fn attributes_appended_after_fingerprint_are_inert() {
    let mut msg = sealed_request();
    msg.extend_from_slice(&override_attributes());
    let total = (msg.len() - HEADER_LEN) as u16;
    set_len(&mut msg, total);
    reseal_fp_at_fp_boundary(&mut msg);

    let p = parse_stun(&msg).expect("parses");
    assert_authenticated_request_fields(&p);
    assert!(verify_fingerprint(&msg, &p));
    assert!(verify_message_integrity(&msg, &p, KEY));
}

/// A second MESSAGE-INTEGRITY appended after the first must not displace it:
/// verification stays keyed to the first (authenticated) MI.
#[test]
fn duplicate_message_integrity_is_ignored_after_the_first() {
    let base = sealed_request();
    let mi_end = attr_end(&base, ATTR_MESSAGE_INTEGRITY);
    let mut second = Vec::new();
    push_attr(&mut second, ATTR_MESSAGE_INTEGRITY, &[0xAB; 20]);
    let mut msg = base.clone();
    insert_at(&mut msg, mi_end, &second);
    reseal_fp_at_fp_boundary(&mut msg);

    let p = parse_stun(&msg).expect("parses");
    assert!(verify_message_integrity(&msg, &p, KEY), "the FIRST MI must be the verified one");
    assert!(verify_fingerprint(&msg, &p));
    assert_authenticated_request_fields(&p);
}

/// Bytes past the declared STUN message length are outside the STUN message:
/// they must not change any parsed field and must not break the FINGERPRINT
/// (whose input ends at FP, which is itself before them).
#[test]
fn bytes_beyond_the_stun_message_length_have_no_effect() {
    let mut msg = sealed_request();
    let declared_end = HEADER_LEN + u16::from_be_bytes([msg[2], msg[3]]) as usize;
    assert_eq!(declared_end, msg.len(), "builder declares the whole message");
    // A complete well-formed attacker attribute rides past the declared
    // length; the header length is deliberately NOT widened.
    msg.extend_from_slice(&override_attributes());

    let p = parse_stun(&msg).expect("parses");
    assert_authenticated_request_fields(&p);
    assert!(verify_fingerprint(&msg, &p), "FP boundary is the STUN message, not the datagram");
    assert!(verify_message_integrity(&msg, &p, KEY));
}

// ---------------------------------------------------------------------------
// 3) anti-regression: real tampering and real bounds are still caught
// ---------------------------------------------------------------------------

/// Flipping an authenticated prefix byte (USERNAME, before MI) fails MI even
/// with a correctly re-sealed FINGERPRINT.
#[test]
fn tampered_authenticated_prefix_still_fails_integrity() {
    let mut msg = sealed_request();
    let uname = attr_offset(&msg, ATTR_USERNAME);
    msg[uname + 4] ^= 0x01; // first USERNAME byte, inside the authenticated prefix
    reseal_fp_at_fp_boundary(&mut msg);

    let p = parse_stun(&msg).expect("parses");
    assert!(verify_fingerprint(&msg, &p), "resealed FP stays valid");
    assert!(!verify_message_integrity(&msg, &p, KEY), "flipped USERNAME byte must fail MI");
}

/// Bounds are still enforced on every attribute the parser actually walks: a
/// declared attribute length running past the datagram is a parse error, and
/// a datagram shorter than its declared STUN length is a parse error.
#[test]
fn parsing_bounds_are_still_enforced() {
    // Well-formed header, declared body of 8 bytes, one attribute whose
    // length claims 256 bytes → attr-overrun.
    let mut overrun = vec![0u8; HEADER_LEN];
    overrun[0..2].copy_from_slice(&BINDING_REQUEST.to_be_bytes());
    overrun[2..4].copy_from_slice(&8u16.to_be_bytes());
    overrun[4..8].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
    overrun.extend_from_slice(&ATTR_USERNAME.to_be_bytes());
    overrun.extend_from_slice(&0x0100u16.to_be_bytes());
    overrun.extend_from_slice(&[0u8; 4]); // declared body is 8 bytes; the attr claims 256
    assert_eq!(overrun.len(), HEADER_LEN + 8);
    assert!(parse_stun(&overrun).is_err(), "attribute overrun must stay a parse error");

    // A real message truncated below its declared length → truncated.
    let mut truncated = sealed_request();
    truncated.truncate(HEADER_LEN + 12);
    assert!(parse_stun(&truncated).is_err(), "declared length past the datagram must stay an error");

    // Sanity: the untruncated message parses (the error above is not vacuous).
    let ok = sealed_request();
    assert!(parse_stun(&ok).is_ok());
}
