// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N5b end-to-end tests: two `IceSession`s (A/B) in one process, each with
//! a REAL loopback UDP socket taken through the `ProtectedUdpFdSource`
//! seam, exchanging credentials and marshaled candidates exactly like the
//! signal channel would, pumping connectivity checks with an INJECTED clock
//! (no sleeps; loopback latency means each `run_once` sees the peer's
//! previous datagram).
//!
//! Proven here:
//! - both agents' pairs reach Succeeded and the SELECTED PAIR IS CONSISTENT
//!   (A's remote == B's local, and vice versa);
//! - a role conflict (both agents claim controlling) converges per
//!   RFC 8445 §7.3.1.1/§7.2.5.1 with the tie-breaker deciding;
//! - a wrong password is REJECTED (request direction: 401 → pair Failed;
//!   response direction: MI-mismatched success → ignored, retransmit,
//!   exhaust);
//! - a request with a broken FINGERPRINT is silently dropped (no response),
//!   while the same request with the fingerprint intact IS answered;
//! - a success response for an unknown transaction id is ignored.

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

use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use netbird_core::ice::{Candidate, ProtectedUdpFdSource};
use netbird_core::ice_session::{
    build_check_request, build_success_response, parse_stun, verify_fingerprint,
    verify_message_integrity, CheckRequest, IceCredentials, IceEvent, IceSession, PairState,
};
use netbird_core::stun::TransactionId;

extern "C" {
    fn socket(domain: i32, ty: i32, protocol: i32) -> i32;
    fn close(fd: i32) -> i32;
    fn recvfrom(fd: i32, buf: *mut u8, len: usize, flags: i32, addr: *mut u8, addrlen: *mut u32) -> isize;
}

const A: (&str, &str) = ("AAAAAAAAAAAAAAAA", "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef");
const B: (&str, &str) = ("BBBBBBBBBBBBBBBB", "ABCDEFGHIJKLMNOPQRSTUVWXYZghijkl");

fn creds(c: (&str, &str)) -> IceCredentials {
    IceCredentials { ufrag: c.0.into(), pwd: c.1.into() }
}

/// One test peer: an IceSession plus its protected-fd provider holding the
/// ORIGINAL fds (the session only ever touches dups; the originals stay
/// observable for wire assertions).
struct TestPeer {
    session: IceSession,
    provider: ProtectedUdpFdSource,
    raw_fds: Vec<i32>,
    local: Option<Candidate>,
}

impl TestPeer {
    fn new(creds: IceCredentials, controlling: bool, tie_breaker: u64) -> Self {
        let session = IceSession::new(creds, controlling, Some(tie_breaker)).expect("session");
        TestPeer { session, provider: ProtectedUdpFdSource::new_with_fd(-1), raw_fds: Vec::new(), local: None }
    }

    fn add_local(&mut self) {
        let fd = unsafe { socket(2, 2, 0) };
        assert!(fd >= 0, "socket() failed");
        self.raw_fds.push(fd);
        self.provider.feed(fd);
        let cand = Candidate::host_candidate([127, 0, 0, 1], 0);
        let bound = self.session.add_local_candidate(cand, &self.provider).expect("bind protected dup");
        self.local = Some(bound);
    }

    fn addr(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], self.local.as_ref().expect("local candidate").port))
    }
}

impl Drop for TestPeer {
    fn drop(&mut self) {
        for fd in self.raw_fds.drain(..) {
            unsafe { close(fd) };
        }
    }
}

/// Signal-shaped exchange: each candidate crosses the marshaled wire form
/// (`Body.payload`) before the other side consumes it; remote credentials
/// arrive like the `"ufrag:pwd"` offer/answer payload.
fn connect_peers(a: &mut TestPeer, b: &mut TestPeer, a_creds: &IceCredentials, b_creds: &IceCredentials) {
    let ca = Candidate::unmarshal(&a.local.as_ref().expect("A local").marshal()).expect("A payload");
    let cb = Candidate::unmarshal(&b.local.as_ref().expect("B local").marshal()).expect("B payload");
    a.session.add_remote_candidate(cb);
    b.session.add_remote_candidate(ca);
    a.session.set_remote_credentials(b_creds.clone()).expect("remote creds");
    b.session.set_remote_credentials(a_creds.clone()).expect("remote creds");
    a.session.start().expect("A start");
    b.session.start().expect("B start");
}

/// Pump both sessions on an injected clock until `cond` holds or the sim
/// deadline passes.
fn pump_until(
    a: &mut IceSession,
    b: &mut IceSession,
    now: &mut u64,
    deadline_ms: u64,
    cond: impl Fn(&IceSession, &IceSession) -> bool,
) -> bool {
    while *now <= deadline_ms {
        a.run_once(*now).expect("A pump");
        b.run_once(*now).expect("B pump");
        *now += 10;
        if cond(a, b) {
            return true;
        }
    }
    false
}

fn recv_one(sock: &UdpSocket) -> Vec<u8> {
    let mut buf = [0u8; 1500];
    let (n, _) = sock.recv_from(&mut buf).expect("expected a datagram");
    buf[..n].to_vec()
}

/// The core N5b assertion: two agents, one loopback, checks conclude and
/// BOTH sides select THE SAME pair (consistent local/remote endpoints).
#[test]
fn dual_agent_selects_a_consistent_pair() {
    let (a_creds, b_creds) = (creds(A), creds(B));
    let mut a = TestPeer::new(a_creds.clone(), true, 0x1111);
    let mut b = TestPeer::new(b_creds.clone(), false, 0x2222);
    a.add_local();
    b.add_local();
    connect_peers(&mut a, &mut b, &a_creds, &b_creds);

    let mut now = 1000u64;
    let converged = pump_until(
        &mut a.session,
        &mut b.session,
        &mut now,
        10_000,
        |a, b| a.selected_pair().is_some() && b.selected_pair().is_some(),
    );
    assert!(converged, "both agents must reach a selected pair within the sim deadline");

    // Consistency: each side's selected pair endpoints mirror the other's.
    let (a_local, a_remote) = a.session.selected_pair().expect("A selected");
    let (b_local, b_remote) = b.session.selected_pair().expect("B selected");
    assert_eq!(a_local.address, "127.0.0.1");
    assert_eq!(a_local.port, a.local.as_ref().expect("A cand").port);
    assert_eq!(b_local.port, b.local.as_ref().expect("B cand").port);
    assert_eq!(a_remote.port, b_local.port, "A's remote endpoint is B's local endpoint");
    assert_eq!(b_remote.port, a_local.port, "B's remote endpoint is A's local endpoint");

    // Both agents observed check success AND selection.
    let a_events = a.session.take_events();
    assert!(a_events.iter().any(|e| matches!(e, IceEvent::CheckSucceeded { .. })), "{a_events:?}");
    assert!(a_events.iter().any(|e| matches!(e, IceEvent::SelectedPair { .. })), "{a_events:?}");
    let b_events = b.session.take_events();
    assert!(b_events.iter().any(|e| matches!(e, IceEvent::CheckSucceeded { .. })), "{b_events:?}");
    assert!(b_events.iter().any(|e| matches!(e, IceEvent::SelectedPair { .. })), "{b_events:?}");

    // Nominated + Succeeded pair on both sides.
    for (peer, name) in [(&a, "A"), (&b, "B")] {
        let snaps = peer.session.pair_snapshots();
        assert!(
            snaps.iter().any(|s| s.state == PairState::Succeeded && s.nominated),
            "{name} must have a nominated succeeded pair: {snaps:?}"
        );
    }
    a.session.stop();
    b.session.stop();
}

/// Role-conflict repair (RFC 8445 §7.3.1.1 + §7.2.5.1): both agents claim
/// controlling; the smaller tie-breaker yields. The pair still converges to
/// a consistent selection, with exactly one controlling agent at the end.
/// Uses GENERATED credentials (upstream 16/32 runesAlpha rule on the wire).
#[test]
fn both_controlling_converges_via_tiebreaker() {
    let a_creds = IceCredentials::generate().expect("entropy");
    let b_creds = IceCredentials::generate().expect("entropy");
    let mut a = TestPeer::new(a_creds.clone(), true, 0xFFFF_FFFF_FFFF_0000); // big tb
    let mut b = TestPeer::new(b_creds.clone(), true, 1); // small tb — must yield
    a.add_local();
    b.add_local();
    connect_peers(&mut a, &mut b, &a_creds, &b_creds);

    let mut now = 2000u64;
    let converged = pump_until(
        &mut a.session,
        &mut b.session,
        &mut now,
        20_000,
        |a, b| a.selected_pair().is_some() && b.selected_pair().is_some(),
    );
    assert!(converged, "role conflict must converge, not livelock");
    assert!(a.session.is_controlling(), "the larger tie-breaker retains controlling");
    assert!(!b.session.is_controlling(), "the smaller tie-breaker switched to controlled");

    let (a_local, a_remote) = a.session.selected_pair().expect("A selected");
    let (b_local, b_remote) = b.session.selected_pair().expect("B selected");
    assert_eq!(a_remote.port, b_local.port);
    assert_eq!(b_remote.port, a_local.port);
    a.session.stop();
    b.session.stop();
}

/// Wrong-password rejection, REQUEST direction: A was given a corrupted
/// remote password. B validates MI with its own pwd, fails, answers 401;
/// A treats the unrecoverable STUN error as pair failure (RFC 8445
/// §7.2.5.2.4). B never succeeds anything.
#[test]
fn wrong_password_requests_are_rejected() {
    let (a_creds, b_creds) = (creds(A), creds(B));
    // Same shape/charset/length as B's real pwd — only the VALUE is wrong,
    // isolating the integrity check from format validation.
    let bad_b = IceCredentials { ufrag: B.0.into(), pwd: "XXCDEFGHIJKLMNOPQRSTUVWXYZghijkl".into() };
    bad_b.validate().expect("wrong pwd must still be well-formed");
    let mut a = TestPeer::new(a_creds.clone(), true, 0x1111);
    let mut b = TestPeer::new(b_creds.clone(), false, 0x2222);
    a.add_local();
    b.add_local();
    // A learns a CORRUPTED password for B; B learns A's correctly.
    let ca = Candidate::unmarshal(&a.local.as_ref().expect("A local").marshal()).expect("payload");
    let cb = Candidate::unmarshal(&b.local.as_ref().expect("B local").marshal()).expect("payload");
    a.session.add_remote_candidate(cb);
    b.session.add_remote_candidate(ca);
    a.session.set_remote_credentials(bad_b).expect("valid-format wrong pwd");
    b.session.set_remote_credentials(a_creds).expect("remote creds");
    a.session.start().expect("A start");
    b.session.start().expect("B start");

    let mut now = 1000u64;
    let failed = pump_until(&mut a.session, &mut b.session, &mut now, 10_000, |a, _| {
        a.pair_snapshots().iter().all(|s| s.state == PairState::Failed)
    });
    assert!(failed, "A's pairs must fail fast on the 401");

    let a_events = a.session.take_events();
    assert!(a_events.iter().any(|e| matches!(e, IceEvent::Failed(_))), "A must fail: {a_events:?}");
    assert!(a.session.selected_pair().is_none());

    // B's OWN checks (keyed with A's correct pwd) legitimately succeed in
    // the A←B direction; what must NEVER happen is a nomination/selection:
    // A's checks were all rejected, so no USE-CANDIDATE can arrive.
    let b_events = b.session.take_events();
    assert!(
        !b_events.iter().any(|e| matches!(e, IceEvent::SelectedPair { .. })),
        "B must never nominate/select with a peer that fails integrity: {b_events:?}"
    );
    assert!(b.session.selected_pair().is_none());
    assert!(
        b.session.pair_snapshots().iter().all(|s| !s.nominated),
        "no B pair may be nominated: {:?}",
        b.session.pair_snapshots()
    );
    a.session.stop();
    b.session.stop();
}

/// Wrong-password rejection, RESPONSE direction: a fake peer answers A's
/// check with a success response whose MESSAGE-INTEGRITY is keyed with the
/// WRONG password but whose FINGERPRINT is VALID. A must ignore it,
/// retransmit the SAME transaction id, and eventually exhaust + fail. The
/// request itself is wire-parsed to pin the check shape (USERNAME order,
/// role attribute, MI keyed with the remote pwd, FINGERPRINT).
#[test]
fn wrong_password_responses_are_rejected() {
    let (a_creds, b_creds) = (creds(A), creds(B));
    let mut a = TestPeer::new(a_creds, true, 0x1111);
    a.add_local();
    let fake = UdpSocket::bind("127.0.0.1:0").expect("fake bind");
    fake.set_read_timeout(Some(Duration::from_millis(300))).expect("timeout");
    let fake_addr = fake.local_addr().expect("addr");
    let (fip, fport) = match fake_addr {
        SocketAddr::V4(v4) => (v4.ip().octets(), v4.port()),
        _ => panic!("ipv4 only"),
    };
    a.session
        .add_remote_candidate(Candidate::unmarshal(&Candidate::host_candidate(fip, fport).marshal()).expect("payload"));
    a.session.set_remote_credentials(creds(B)).expect("remote creds");
    a.session.start().expect("start");

    a.session.run_once(1000).expect("pump");
    let req = recv_one(&fake);
    let parsed = parse_stun(&req).expect("request parses");
    assert_eq!(parsed.msg_type, 0x0001);
    // USERNAME = "remote:local" (RFC 8445 §7.1.2.1) — B's ufrag FIRST.
    assert_eq!(parsed.username.as_deref(), Some("BBBBBBBBBBBBBBBB:AAAAAAAAAAAAAAAA"));
    assert!(parsed.controlling.is_some(), "A is controlling");
    assert!(!parsed.use_candidate, "the first check is not a nomination");
    assert!(verify_fingerprint(&req, &parsed), "FINGERPRINT present and correct");
    assert!(verify_message_integrity(&req, &parsed, &creds(B).pwd), "MI keyed with the remote pwd");
    let req_txn = parsed.txn;

    // Answer with a VALID FINGERPRINT but WRONG-KEY integrity.
    let forged = build_success_response(req_txn, (fip, fport), "XXCDEFGHIJKLMNOPQRSTUVWXYZghijkl");
    fake.send_to(&forged, a.addr()).expect("send forged response");

    // A must NOT accept: pump through the full retransmit schedule.
    let mut now = 1010u64;
    while now <= 5000 {
        a.session.run_once(now).expect("pump");
        now += 50;
    }
    let events = a.session.take_events();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, IceEvent::CheckSucceeded { .. } | IceEvent::SelectedPair { .. })),
        "forged response must be rejected: {events:?}"
    );
    assert!(
        events.iter().any(|e| matches!(e, IceEvent::Failed(_))),
        "transaction exhaustion must fail the session: {events:?}"
    );
    assert!(a.session.pair_snapshots().iter().all(|s| s.state == PairState::Failed));
    a.session.stop();
}

/// FINGERPRINT gate: a check with valid MI but corrupted FINGERPRINT is
/// silently discarded (no response); the identical request with the
/// fingerprint intact IS answered — and the response is a full, verifiable
/// success response (XOR-MAPPED-ADDRESS + responder-keyed MI + FP).
#[test]
fn bad_fingerprint_request_is_dropped() {
    let (a_creds, b_creds) = (creds(A), creds(B));
    let mut b = TestPeer::new(b_creds.clone(), false, 0x2222);
    b.add_local();
    let fake = UdpSocket::bind("127.0.0.1:0").expect("fake bind");
    fake.set_read_timeout(Some(Duration::from_millis(300))).expect("timeout");
    let fake_addr = fake.local_addr().expect("addr");
    let (fip, fport) = match fake_addr {
        SocketAddr::V4(v4) => (v4.ip().octets(), v4.port()),
        _ => panic!("ipv4 only"),
    };
    b.session
        .add_remote_candidate(Candidate::unmarshal(&Candidate::host_candidate(fip, fport).marshal()).expect("payload"));
    b.session.set_remote_credentials(a_creds).expect("remote creds");
    b.session.start().expect("start");

    let base = build_check_request(
        &CheckRequest {
            username: format!("{}:{}", b_creds.ufrag, A.0),
            priority: 1,
            controlling: true,
            tie_breaker: 42,
            use_candidate: false,
            // RFC 8445 §7.2.2: the SENDER keys the check with the REMOTE
            // (B's) password — exactly what B validates with its own pwd.
            integrity_key: B.1.into(),
        },
        TransactionId([0x5a; 12]),
    );
    let mut bad = base.clone();
    let last = bad.len() - 1;
    bad[last] ^= 0x01; // corrupt the FINGERPRINT value only

    // First pump makes B send ITS OWN check too — read it off the wire so
    // the fake socket is empty before the fingerprint experiment.
    b.session.run_once(1000).expect("pump");
    let own = recv_one(&fake);
    let po = parse_stun(&own).expect("B's own check parses");
    assert_eq!(po.msg_type, 0x0001, "drained datagram is B's outbound check");

    fake.send_to(&bad, b.addr()).expect("send bad");
    b.session.run_once(1010).expect("pump");
    b.session.run_once(1020).expect("pump");
    let mut buf = [0u8; 1500];
    assert!(
        fake.recv_from(&mut buf).is_err(),
        "a bad-fingerprint check must be silently dropped"
    );

    fake.send_to(&base, b.addr()).expect("send good");
    b.session.run_once(1030).expect("pump");
    b.session.run_once(1040).expect("pump");
    let resp = recv_one(&fake);
    let pr = parse_stun(&resp).expect("response parses");
    assert_eq!(pr.msg_type, 0x0101);
    assert_eq!(pr.txn, TransactionId([0x5a; 12]));
    assert_eq!(pr.xor_mapped, Some((fip, fport)));
    assert!(verify_fingerprint(&resp, &pr));
    assert!(verify_message_integrity(&resp, &pr, &b_creds.pwd), "response MI keyed with responder pwd");
    b.session.stop();
}

/// A success response whose transaction id matches nothing in flight is
/// dropped even with PERFECT integrity — the pair's own check stays in
/// flight (transaction-id matching is part of response authenticity).
#[test]
fn unknown_txn_success_response_is_ignored() {
    let (a_creds, b_creds) = (creds(A), creds(B));
    let mut a = TestPeer::new(a_creds, true, 0x1111);
    a.add_local();
    let fake = UdpSocket::bind("127.0.0.1:0").expect("fake bind");
    fake.set_read_timeout(Some(Duration::from_millis(200))).expect("timeout");
    let fake_addr = fake.local_addr().expect("addr");
    let (fip, fport) = match fake_addr {
        SocketAddr::V4(v4) => (v4.ip().octets(), v4.port()),
        _ => panic!("ipv4 only"),
    };
    a.session
        .add_remote_candidate(Candidate::unmarshal(&Candidate::host_candidate(fip, fport).marshal()).expect("payload"));
    a.session.set_remote_credentials(creds(B)).expect("remote creds");
    a.session.start().expect("start");

    a.session.run_once(1000).expect("pump");
    let _req = recv_one(&fake);
    // Fully authentic success response — but for a fabricated transaction.
    let alien = build_success_response(TransactionId([0x77; 12]), (fip, fport), B.1);
    fake.send_to(&alien, a.addr()).expect("send alien response");
    a.session.run_once(1010).expect("pump");
    let events = a.session.take_events();
    assert!(
        !events.iter().any(|e| matches!(e, IceEvent::CheckSucceeded { .. })),
        "unknown txn must be ignored: {events:?}"
    );
    assert!(
        a.session.pair_snapshots().iter().any(|s| s.state == PairState::InProgress),
        "the real check stays in flight"
    );
    a.session.stop();
}
