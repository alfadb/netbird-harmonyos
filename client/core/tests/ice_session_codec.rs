// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N5b check-message codec tests against the RFC 5769 vectors — the exact
//! bytes MESSAGE-INTEGRITY (HMAC-SHA1) and FINGERPRINT (CRC-32 ⊕ "STUN")
//! must reproduce to interoperate with pion/any ICE peer:
//!
//! - §2.1 sample request: USERNAME/PRIORITY/ICE-CONTROLLED + MI + FP, key
//!   `VOkJxbRl1RmTxUk/WvJxBt`; MI input = message up to MI with the header
//!   length covering MI **plus the FINGERPRINT length** (RFC 5389 §15.4);
//!   FP input = message up to FP with the length EXCLUDING FP (§15.5).
//! - §2.2 sample IPv4 response: XOR-MAPPED-ADDRESS decodes to
//!   192.0.2.1:32853 with MI + FP riding along.
//!
//! Tamper cases prove rejection is real (a flipped byte flips the verdict),
//! and the builder/verifier round-trip pins the ICE check shape offline.

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
        _data: *mut *mut c_void,
        _result: *mut c_void,
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
    pub extern "C" fn napi_get_value_int32(_env: *mut c_void, _value: *mut c_void, _result: *mut i32) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(_env: *mut c_void, _value: *mut c_void, _result: *mut bool) -> i32 {
        0
    }
}

use netbird_core::ice_session::{
    build_check_request, build_error_response, build_success_response, parse_stun,
    verify_fingerprint, verify_message_integrity, CheckRequest, ERR_ROLE_CONFLICT,
};
use netbird_core::management::ManagementError;
use netbird_core::stun::TransactionId;

/// RFC 5769 §2.1 Sample Request, verbatim.
const RFC5769_REQ: &[u8] = &[
    0x00, 0x01, 0x00, 0x58, 0x21, 0x12, 0xa4, 0x42, 0xb7, 0xe7, 0xa7, 0x01, 0xbc, 0x34, 0xd6, 0x86,
    0xfa, 0x87, 0xdf, 0xae, 0x80, 0x22, 0x00, 0x10, 0x53, 0x54, 0x55, 0x4e, 0x20, 0x74, 0x65, 0x73,
    0x74, 0x20, 0x63, 0x6c, 0x69, 0x65, 0x6e, 0x74, 0x00, 0x24, 0x00, 0x04, 0x6e, 0x00, 0x01, 0xff,
    0x80, 0x29, 0x00, 0x08, 0x93, 0x2f, 0xf9, 0xb1, 0x51, 0x26, 0x3b, 0x36, 0x00, 0x06, 0x00, 0x09,
    0x65, 0x76, 0x74, 0x6a, 0x3a, 0x68, 0x36, 0x76, 0x59, 0x20, 0x20, 0x20, 0x00, 0x08, 0x00, 0x14,
    0x9a, 0xea, 0xa7, 0x0c, 0xbf, 0xd8, 0xcb, 0x56, 0x78, 0x1e, 0xf2, 0xb5, 0xb2, 0xd3, 0xf2, 0x49,
    0xc1, 0xb5, 0x71, 0xa2, 0x80, 0x28, 0x00, 0x04, 0xe5, 0x7a, 0x3b, 0xcf,
];

/// RFC 5769 §2.2 Sample IPv4 Response, verbatim.
const RFC5769_RESPV4: &[u8] = &[
    0x01, 0x01, 0x00, 0x3c, 0x21, 0x12, 0xa4, 0x42, 0xb7, 0xe7, 0xa7, 0x01, 0xbc, 0x34, 0xd6, 0x86,
    0xfa, 0x87, 0xdf, 0xae, 0x80, 0x22, 0x00, 0x0b, 0x74, 0x65, 0x73, 0x74, 0x20, 0x76, 0x65, 0x63,
    0x74, 0x6f, 0x72, 0x20, 0x00, 0x20, 0x00, 0x08, 0x00, 0x01, 0xa1, 0x47, 0xe1, 0x12, 0xa6, 0x43,
    0x00, 0x08, 0x00, 0x14, 0x2b, 0x91, 0xf5, 0x99, 0xfd, 0x9e, 0x90, 0xc3, 0x8c, 0x74, 0x89, 0xf9,
    0x2a, 0xf9, 0xba, 0x53, 0xf0, 0x6b, 0xe7, 0xd7, 0x80, 0x28, 0x00, 0x04, 0xc0, 0x7d, 0x4c, 0x96,
];

const RFC5769_TXN: TransactionId =
    TransactionId([0xb7, 0xe7, 0xa7, 0x01, 0xbc, 0x34, 0xd6, 0x86, 0xfa, 0x87, 0xdf, 0xae]);
const RFC5769_PWD: &str = "VOkJxbRl1RmTxUk/WvJxBt";

/// §2.1: the vector parses to exactly its announced parameters and BOTH
/// authenticators verify with the RFC password — the interoperability bar.
#[test]
fn rfc5769_request_parses_and_verifies() {
    let p = parse_stun(RFC5769_REQ).expect("vector parses");
    assert_eq!(p.msg_type, 0x0001);
    assert_eq!(p.txn, RFC5769_TXN);
    assert_eq!(p.username.as_deref(), Some("evtj:h6vY"));
    assert_eq!(p.priority, Some(0x6e00_01ff));
    assert_eq!(p.controlling, None);
    assert_eq!(p.controlled, Some(0x932f_f9b1_5126_3b36));
    assert!(!p.use_candidate);
    assert!(verify_message_integrity(RFC5769_REQ, &p, RFC5769_PWD), "MESSAGE-INTEGRITY must verify");
    assert!(verify_fingerprint(RFC5769_REQ, &p), "FINGERPRINT must verify");
    assert!(!verify_message_integrity(RFC5769_REQ, &p, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"));
}

/// §2.2: the response vector's XOR-MAPPED-ADDRESS decodes to the announced
/// mapping and both authenticators verify.
#[test]
fn rfc5769_ipv4_response_parses_and_verifies() {
    let p = parse_stun(RFC5769_RESPV4).expect("vector parses");
    assert_eq!(p.msg_type, 0x0101);
    assert_eq!(p.txn, RFC5769_TXN);
    assert_eq!(p.xor_mapped, Some(([192, 0, 2, 1], 32853)));
    assert!(verify_message_integrity(RFC5769_RESPV4, &p, RFC5769_PWD));
    assert!(verify_fingerprint(RFC5769_RESPV4, &p));
}

/// A flipped MI byte flips the MI verdict; a flipped FP byte flips the FP
/// verdict — rejection is real, not decorative.
#[test]
fn tampered_vector_bytes_flip_verdicts() {
    let mut bad_mi = RFC5769_REQ.to_vec();
    // MI value = second-to-last attribute value (FP attr occupies the
    // final 8 bytes; MI value is the 20 bytes before it)
    let mi_start = RFC5769_REQ.len() - 8 - 20;
    bad_mi[mi_start] ^= 0x01;
    let p1 = parse_stun(&bad_mi).expect("still parses");
    assert!(!verify_message_integrity(&bad_mi, &p1, RFC5769_PWD), "flipped MI byte must fail");

    let mut bad_fp = RFC5769_RESPV4.to_vec();
    let last = bad_fp.len() - 1;
    bad_fp[last] ^= 0x80;
    let p2 = parse_stun(&bad_fp).expect("still parses");
    assert!(!verify_fingerprint(&bad_fp, &p2), "flipped FP byte must fail");
}

/// Absence is not success: without MI/FP attributes both verifiers return
/// false (an unauthenticated message never passes by default).
#[test]
fn missing_authenticators_reject() {
    let mut bare = Vec::new();
    bare.extend_from_slice(&0x0001u16.to_be_bytes());
    bare.extend_from_slice(&0u16.to_be_bytes());
    bare.extend_from_slice(&netbird_core::stun::MAGIC_COOKIE.to_be_bytes());
    bare.extend_from_slice(&RFC5769_TXN.0);
    let p = parse_stun(&bare).expect("header-only parses");
    assert!(!verify_message_integrity(&bare, &p, RFC5769_PWD));
    assert!(!verify_fingerprint(&bare, &p));
}

/// Our built ICE check request: exact wire anatomy (USERNAME remote:local,
/// PRIORITY, role attr, USE-CANDIDATE, MI, FP last), 4-byte alignment, and
/// self-verification with the remote password.
#[test]
fn built_check_request_wire_anatomy() {
    let req = CheckRequest {
        username: "BBBBufragBBBBBB:AAAAufragAAAAAA".into(),
        priority: (1 << 24) * 110 + (1 << 8) * 65535 + 255,
        controlling: true,
        tie_breaker: 0xdead_beef_cafe_f00d,
        use_candidate: true,
        integrity_key: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh".into(),
    };
    let txn = TransactionId([9; 12]);
    let msg = build_check_request(&req, txn);
    assert_eq!(msg.len() % 4, 0, "STUN messages are 4-byte aligned");
    let declared = u16::from_be_bytes([msg[2], msg[3]]) as usize;
    assert_eq!(msg.len(), 20 + declared, "header length must cover the body");

    let p = parse_stun(&msg).expect("built request parses");
    assert_eq!(p.txn, txn);
    assert_eq!(p.username.as_deref(), Some(req.username.as_str()));
    assert_eq!(p.priority, Some(req.priority));
    assert_eq!(p.controlling, Some(0xdead_beef_cafe_f00d));
    assert!(p.controlled.is_none());
    assert!(p.use_candidate, "nomination check carries USE-CANDIDATE");
    assert!(verify_message_integrity(&msg, &p, &req.integrity_key));
    assert!(verify_fingerprint(&msg, &p));
    assert!(!verify_message_integrity(&msg, &p, "nope"), "wrong key rejected");

    // controlled-role request: the role attribute flips
    let mut req2 = req.clone();
    req2.controlling = false;
    req2.use_candidate = false;
    let msg2 = build_check_request(&req2, txn);
    let p2 = parse_stun(&msg2).expect("parses");
    assert_eq!(p2.controlled, Some(0xdead_beef_cafe_f00d));
    assert!(p2.controlling.is_none());
    assert!(!p2.use_candidate);
}

/// Built success response round-trips the XOR-MAPPED-ADDRESS and verifies
/// with the responder password; the 487 error response carries its code and
/// (when keyed) its integrity.
#[test]
fn built_responses_roundtrip() {
    let txn = TransactionId([7; 12]);
    let resp = build_success_response(txn, ([127, 0, 0, 1], 51820), "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh");
    let p = parse_stun(&resp).expect("parses");
    assert_eq!(p.msg_type, 0x0101);
    assert_eq!(p.txn, txn);
    assert_eq!(p.xor_mapped, Some(([127, 0, 0, 1], 51820)));
    assert!(verify_message_integrity(&resp, &p, "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh"));
    assert!(verify_fingerprint(&resp, &p));

    let err = build_error_response(txn, ERR_ROLE_CONFLICT, Some("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh"));
    let pe = parse_stun(&err).expect("parses");
    assert_eq!(pe.msg_type, 0x0111);
    assert_eq!(pe.error_code, Some(487));
    assert!(verify_message_integrity(&err, &pe, "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh"));

    let err401 = build_error_response(txn, 401, None);
    let p4 = parse_stun(&err401).expect("parses");
    assert_eq!(p4.error_code, Some(401));
    assert!(!verify_message_integrity(&err401, &p4, "x"), "unkeyed 401 has no MI");
    assert!(verify_fingerprint(&err401, &p4));
}

/// Hostile shapes are Parse errors with a stable token — never a panic and
/// never a false accept.
#[test]
fn hostile_shapes_are_parse_errors() {
    let ok = {
        let req = CheckRequest {
            username: "abcd:efgh".into(),
            priority: 1,
            controlling: false,
            tie_breaker: 2,
            use_candidate: false,
            integrity_key: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh".into(),
        };
        build_check_request(&req, TransactionId([3; 12]))
    };
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
        ("attr-overrun", {
            let mut m = ok.clone();
            m[2..4].copy_from_slice(&8u16.to_be_bytes());
            m[20..22].copy_from_slice(&0x0020u16.to_be_bytes());
            m[22..24].copy_from_slice(&200u16.to_be_bytes());
            m
        }),
        ("noise", (0..=60u8).collect()),
    ];
    for (name, datagram) in cases {
        let err = parse_stun(&datagram)
            .err()
            .unwrap_or_else(|| panic!("{name} must be rejected"));
        assert!(
            matches!(&err, ManagementError::Parse(t) if t.starts_with("stun-check:")),
            "{name} misclassified: {err:?}"
        );
    }
}

