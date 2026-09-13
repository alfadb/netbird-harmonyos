// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N5a candidate model tests: the `Body.payload` wire form against
//! `signalexchange.proto` — upstream carries pion's `candidate.Marshal()`
//! string under `Body_CANDIDATE` (`signaler.go:32-41`, parsed at
//! `engine.go:2063-2071`) and `"ufrag:pwd"` under OFFER/ANSWER
//! (`shared/signal/client/client.go:77`). Everything here is offline.

use netbird_core::ice::{Candidate, CandidateType};
use netbird_core::management::ManagementError;
use netbird_core::signal::proto::{body, Body};
use prost::Message as _;

// Host-test link surface (same as tests/management_grpc.rs): the integration
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
        _cb: *const c_void,
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

/// The canonical pion/libwebrtc wire shape (RFC 8839 candidate-attribute):
/// this exact string must parse field-for-field, with `generation` treated
/// as an ignorable extension.
const PION_SRFLX: &str = "candidate:842163049 1 udp 1677729535 192.168.0.1 45332 typ srflx raddr 10.0.0.17 rport 8998 generation 0";

#[test]
fn parses_pion_candidate_string_field_for_field() {
    let c = Candidate::unmarshal(PION_SRFLX).expect("canonical string parses");
    assert_eq!(c.foundation, "842163049");
    assert_eq!(c.component, 1);
    assert_eq!(c.transport, "udp");
    assert_eq!(c.priority, 1677729535);
    assert_eq!(c.address, "192.168.0.1");
    assert_eq!(c.port, 45332);
    assert_eq!(c.typ, CandidateType::Srflx);
    assert_eq!(c.related_address.as_deref(), Some("10.0.0.17"));
    assert_eq!(c.related_port, Some(8998));
}

/// Round-trip both produced flavors; the `a=`-prefixed form parses to the
/// same value (SDP-wrapped payloads). The bare `candidate:` prefix stays
/// REQUIRED — it is what both peers put on the wire (pion Marshal /
/// UnmarshalCandidate symmetry).
#[test]
fn marshal_unmarshal_roundtrip_is_lossless() {
    let host = Candidate::host_candidate([192, 168, 1, 5], 40122);
    let srflx = Candidate::srflx_candidate(&host, [203, 0, 113, 9], 55555);
    for c in [&host, &srflx] {
        let back = Candidate::unmarshal(&c.marshal()).expect("roundtrips");
        assert_eq!(&back, c);
    }
    assert_eq!(host.typ, CandidateType::Host);
    assert!(host.related_address.is_none() && host.related_port.is_none());
    assert_eq!(srflx.related_address.as_deref(), Some("192.168.1.5"));
    assert_eq!(srflx.related_port, Some(40122));
    assert_eq!(Candidate::unmarshal(&format!("a={}", host.marshal())).unwrap(), host);
}

/// RFC 8445 §5.1.2.1 with pion type preferences: host 126 / srflx 100,
/// local pref 65535, component 1. Both must sit far above relay (0) so a
/// later N5b pair selection prefers them, and srflx < host.
#[test]
fn priorities_follow_the_rfc_formula() {
    let host = Candidate::host_candidate([10, 0, 0, 1], 1);
    let srflx = Candidate::srflx_candidate(&host, [10, 0, 0, 1], 2);
    let expected_host = (1 << 24) * 126 + (1 << 8) * 65535 + 255;
    let expected_srflx = (1 << 24) * 100 + (1 << 8) * 65535 + 255;
    assert_eq!(host.priority, expected_host);
    assert_eq!(srflx.priority, expected_srflx);
    assert!(srflx.priority < host.priority);
}

/// Deterministic foundations: equal inputs equal foundations; the srflx
/// foundation differs from its base's (different type + mapped address).
#[test]
fn foundations_are_deterministic_per_base_and_type() {
    let a = Candidate::host_candidate([10, 1, 2, 3], 1000);
    let b = Candidate::host_candidate([10, 1, 2, 3], 9999);
    let other = Candidate::host_candidate([10, 1, 2, 4], 1000);
    assert_eq!(a.foundation, b.foundation, "same base address shares foundation");
    assert_ne!(a.foundation, other.foundation);
    let srflx = Candidate::srflx_candidate(&a, [4, 4, 4, 4], 5);
    assert_ne!(srflx.foundation, a.foundation);
}

/// signalexchange.proto alignment: the candidate travels as
/// `Body { type: CANDIDATE(2), payload: <marshal string> }` (proto fields
/// `type = 1`, `payload = 2`; upstream sets exactly these two under
/// CANDIDATE, signaler.go:35-39). Encoded + decoded through the real
/// generated prost types, the payload must unmarshal to the original.
#[test]
fn candidate_survives_a_proto_body_roundtrip() {
    let host = Candidate::host_candidate([172, 16, 0, 9], 33445);
    let mut body = Body::default();
    body.r#type = body::Type::Candidate as i32; // enum value 2, proto L47
    body.payload = host.marshal(); // field `payload`, proto L49
    let bytes = body.encode_to_vec();
    let decoded = Body::decode(&bytes[..]).expect("prost roundtrip");
    assert_eq!(decoded.r#type, body::Type::Candidate as i32);
    assert_eq!(decoded.payload, body.payload);
    let back = Candidate::unmarshal(&decoded.payload).expect("payload parses");
    assert_eq!(back, host);
}

/// OFFER/ANSWER payloads are `"ufrag:pwd"` (client.go:77) — the other half
/// of the N5a wire contract; parsing mirrors upstream UnMarshalCredential
/// (client.go:60-71: exactly two colon-separated parts).
#[test]
fn offer_answer_credential_payload_shape_matches_upstream() {
    let payload = "MYufrag12345678:MYpwd012345678901234567890";
    let (ufrag, pwd) = payload.split_once(':').expect("two parts");
    assert_eq!(ufrag, "MYufrag12345678");
    assert_eq!(pwd, "MYpwd012345678901234567890");
    assert!("only-one-part".split(':').count() != 2);
}

/// Malformed remote payloads are `candidate:*` Parse errors and never
/// panics — a hostile/buggy peer must not take the client down.
#[test]
fn malformed_payloads_are_parse_errors() {
    let cases = [
        "",
        "not a candidate at all",
        "candidate:",
        "candidate:f 1 udp notanumber 1.2.3.4 5 typ host",
        "candidate:f 1 tcp 1 1.2.3.4 5 typ host", // transport: udp only
        "candidate:f 1 udp 1 1.2.3.4 99999 typ host", // port overflow
        "candidate:f 1 udp 1 1.2.3.4 5 typ bogus",
        "candidate:f 1 udp 1 1.2.3.4 5",        // no typ
        "candidate:f 1 udp 1 1.2.3.4 5 typ host raddr 9.9.9.9", // raddr without rport
        "candidate:f 1 udp 1 1.2.3.4 5 typ host raddr 9.9.9.9 rport notaport",
    ];
    for payload in cases {
        let err = Candidate::unmarshal(payload)
            .err()
            .unwrap_or_else(|| panic!("'{payload}' must be rejected"));
        assert!(
            matches!(&err, ManagementError::Parse(t) if t.starts_with("candidate:")),
            "'{payload}' misclassified: {err:?}"
        );
    }
}
