// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N5b state-machine and timer tests with an INJECTED clock (no sleeps as
//! assertions; the only real waits are bounded negative assertions on a
//! fake peer's socket):
//!
//! - keepalive: NOTHING before `KEEPALIVE_INTERVAL_MS` (4s, upstream
//!   agent.go:22) after selection, then a full authenticated Binding
//!   Request (username/MI/FP verified off the wire);
//! - disconnect/failed: >6s without valid inbound → Disconnected; 6s more
//!   (12s total) → Failed (agent.go:23-24 two-stage semantics), keepalives
//!   continuing throughout;
//! - silent peer during checks: 7 attempts × 500ms RTO (RFC 5245 §16 Rc,
//!   RFC 5389 §7.2.1), all the SAME transaction id, then pair Failed →
//!   session Failed;
//! - fail-closed: an empty protected-fd provider means no socket, no
//!   candidate, no start (governance §二.4 — the session never dials
//!   unprotected);
//! - pair lifecycle: Frozen before `start()`, Waiting after; relay/IPv6/
//!   duplicate remote candidates never form pairs.

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

use netbird_core::ice::{Candidate, ProtectedUdpFdSource};
use netbird_core::ice_session::{
    build_success_response, parse_stun, verify_fingerprint, verify_message_integrity,
    IceCredentials, IceEvent, IceSession, PairState, CHECK_RTO_MS, DISCONNECTED_TIMEOUT_MS,
    FAILED_TIMEOUT_MS, KEEPALIVE_INTERVAL_MS, MAX_CHECK_ATTEMPTS,
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

/// Session-side peer fixture (same as e2e): originals observable.
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

    fn add_local(&mut self) -> Candidate {
        let fd = unsafe { socket(2, 2, 0) };
        assert!(fd >= 0, "socket() failed");
        self.raw_fds.push(fd);
        self.provider.feed(fd);
        let bound =
            self.session.add_local_candidate(Candidate::host_candidate([127, 0, 0, 1], 0), &self.provider).expect("bind");
        self.local = Some(bound.clone());
        bound
    }

    fn addr(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], self.local.as_ref().expect("local candidate").port))
    }

    fn drain_raw(&self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let mut buf = [0u8; 1500];
        loop {
            let mut addr = [0u8; 16];
            let mut alen = 16u32;
            let n =
                unsafe { recvfrom(self.raw_fds[0], buf.as_mut_ptr(), buf.len(), 0, addr.as_mut_ptr(), &mut alen) };
            if n <= 0 {
                return out;
            }
            out.push(buf[..n as usize].to_vec());
        }
    }
}

impl Drop for TestPeer {
    fn drop(&mut self) {
        for fd in self.raw_fds.drain(..) {
            unsafe { close(fd) };
        }
    }
}

/// Keepalive: nothing before +4000ms, one authenticated Binding Request
/// right after — verified on the wire through the provider's ORIGINAL fd.
#[test]
fn keepalive_starts_at_4s_and_is_a_full_check() {
    let (a_creds, b_creds) = (creds(A), creds(B));
    let mut a = TestPeer::new(a_creds.clone(), true, 0x1111);
    let mut b = TestPeer::new(b_creds.clone(), false, 0x2222);
    a.add_local();
    b.add_local();
    // connect
    let ca = Candidate::unmarshal(&a.local.as_ref().unwrap().marshal()).unwrap();
    let cb = Candidate::unmarshal(&b.local.as_ref().unwrap().marshal()).unwrap();
    a.session.add_remote_candidate(cb);
    b.session.add_remote_candidate(ca);
    a.session.set_remote_credentials(b_creds.clone()).unwrap();
    b.session.set_remote_credentials(a_creds.clone()).unwrap();
    a.session.start().unwrap();
    b.session.start().unwrap();

    // Pump to selection.
    let mut now = 1000u64;
    let t_sel = loop {
        a.session.run_once(now).unwrap();
        b.session.run_once(now).unwrap();
        if a.session.selected_pair().is_some() && b.session.selected_pair().is_some() {
            break now; // last_inbound == this pump's now (same run_once)
        }
        now += 10;
        assert!(now < 10_000, "must converge");
    };

    // B goes silent: its dup is closed but the ORIGINAL fd stays bound, so
    // A's keepalives remain observable there.
    b.session.stop();

    // Nothing before +4000.
    let mut t = t_sel + 10;
    while t < t_sel + KEEPALIVE_INTERVAL_MS {
        a.session.run_once(t).unwrap();
        t += 30;
    }
    assert!(b.drain_raw().is_empty(), "no keepalive before KEEPALIVE_INTERVAL_MS");

    // One full authenticated check right after the interval.
    let mut ka: Vec<Vec<u8>> = Vec::new();
    while ka.is_empty() {
        a.session.run_once(t).unwrap();
        t += 30;
        ka = b.drain_raw();
        assert!(t < t_sel + KEEPALIVE_INTERVAL_MS + 500, "keepalive must fire close to +4000");
    }
    assert_eq!(ka.len(), 1);
    let p = parse_stun(&ka[0]).expect("keepalive parses");
    assert_eq!(p.msg_type, 0x0001, "keepalive is a Binding Request");
    assert!(!p.use_candidate, "keepalive carries no USE-CANDIDATE");
    assert_eq!(p.username.as_deref(), Some("BBBBBBBBBBBBBBBB:AAAAAAAAAAAAAAAA"));
    assert!(verify_fingerprint(&ka[0], &p));
    assert!(verify_message_integrity(&ka[0], &p, &b_creds.pwd), "keepalive keyed with the remote pwd");
    let late = a.session.take_events();
    assert!(
        !late.iter().any(|e| matches!(e, IceEvent::Disconnected | IceEvent::Failed(_))),
        "no disconnect events before 6s: {late:?}"
    );
}

/// Two-stage disconnect/failed timers (upstream agent.go:23-24, both 6s):
/// Disconnected at last-valid-inbound + 6s, Failed 6s later — exactly, on
/// the injected clock; keepalives keep flowing throughout.
#[test]
fn disconnected_then_failed_on_the_injected_clock() {
    let a_creds = creds(A);
    let mut a = TestPeer::new(a_creds.clone(), true, 0x1111);
    a.add_local();
    let fake_sock = UdpSocket::bind("127.0.0.1:0").expect("fake bind");
    fake_sock.set_nonblocking(true).expect("nonblocking");
    let fake_addr = fake_sock.local_addr().unwrap();
    let (fip, fport) = match fake_addr {
        SocketAddr::V4(v4) => (v4.ip().octets(), v4.port()),
        _ => panic!("ipv4 only"),
    };
    a.session
        .add_remote_candidate(Candidate::unmarshal(&Candidate::host_candidate(fip, fport).marshal()).unwrap());
    a.session.set_remote_credentials(creds(B)).unwrap();
    a.session.start().unwrap();

    // Drive A to selection: respond to every check until nominated check
    // succeeds. last_inbound tracks the LAST valid response exactly.
    let mut now = 1000u64;
    let t_sel = loop {
        a.session.run_once(now).unwrap();
        for txn in drain_requests(&fake_sock) {
            let resp = build_success_response(txn, (fip, fport), B.1);
            let _ = fake_sock.send_to(&resp, a.addr());
        }
        if a.session.selected_pair().is_some() {
            break now;
        }
        now += 10;
        assert!(now < 10_000, "must converge");
    };

    // From here the fake peer NEVER responds again (keepalives unheard).
    let mut events: Vec<IceEvent> = Vec::new();
    let mut t = t_sel + 10;
    let mut keepalives_after_disconnect = 0usize;
    let mut disconnected_at: Option<u64> = None;
    let mut failed_at: Option<u64> = None;
    while t <= t_sel + DISCONNECTED_TIMEOUT_MS + FAILED_TIMEOUT_MS + 200 {
        a.session.run_once(t).unwrap();
        events.extend(a.session.take_events());
        for e in events.iter() {
            match e {
                IceEvent::Disconnected if disconnected_at.is_none() => disconnected_at = Some(t),
                IceEvent::Failed(_) if failed_at.is_none() => failed_at = Some(t),
                _ => {}
            }
        }
        if disconnected_at.is_some() && !drain_requests(&fake_sock).is_empty() {
            keepalives_after_disconnect += 1;
        }
        t += 10;
    }

    assert!(
        disconnected_at.unwrap_or(u64::MAX) >= t_sel + DISCONNECTED_TIMEOUT_MS,
        "Disconnected must not fire before 6s: {disconnected_at:?} vs {}",
        t_sel + DISCONNECTED_TIMEOUT_MS
    );
    assert!(disconnected_at.is_some(), "Disconnected must fire");
    assert!(
        failed_at.unwrap_or(u64::MAX) >= t_sel + DISCONNECTED_TIMEOUT_MS + FAILED_TIMEOUT_MS,
        "Failed must not fire before 12s"
    );
    assert!(failed_at.is_some(), "Failed must fire");
    assert!(
        matches!(events.iter().find(|e| matches!(e, IceEvent::Failed(_))), Some(IceEvent::Failed(r)) if r.contains("disconnected")),
        "Failed reason is the disconnect path: {events:?}"
    );
    assert!(keepalives_after_disconnect > 0, "keepalives continue while disconnected");
}

/// Silent peer during the CHECK phase: 7 attempts of the SAME transaction
/// id, 500ms apart, then the pair and the session fail — and Disconnected
/// never fires (there was never any inbound to measure silence from).
#[test]
fn silent_peer_fails_after_seven_attempts() {
    let mut a = TestPeer::new(creds(A), true, 0x1111);
    a.add_local();
    let fake_sock = UdpSocket::bind("127.0.0.1:0").expect("fake bind");
    fake_sock.set_nonblocking(true).expect("nonblocking");
    let fake_addr = fake_sock.local_addr().unwrap();
    let (fip, fport) = match fake_addr {
        SocketAddr::V4(v4) => (v4.ip().octets(), v4.port()),
        _ => panic!("ipv4 only"),
    };
    a.session
        .add_remote_candidate(Candidate::unmarshal(&Candidate::host_candidate(fip, fport).marshal()).unwrap());
    a.session.set_remote_credentials(creds(B)).unwrap();
    a.session.start().unwrap();

    // t=1000: first send. Retransmits at 1500, 2000, ... 3000 (7 sends).
    a.session.run_once(1000).unwrap();
    assert_eq!(a.session.pair_snapshots()[0].state, PairState::InProgress);
    let mut events = a.session.take_events(); // drain setup events
    events.clear();

    let mut t = 1000 + CHECK_RTO_MS;
    let mut sent: Vec<TransactionId> = drain_requests(&fake_sock);
    while t < 1000 + CHECK_RTO_MS * MAX_CHECK_ATTEMPTS as u64 {
        a.session.run_once(t).unwrap();
        sent.extend(drain_requests(&fake_sock));
        t += CHECK_RTO_MS;
    }
    // Through 6 retransmits (7 sends total) nothing has failed yet.
    assert_eq!(sent.len(), MAX_CHECK_ATTEMPTS as usize, "seven attempts: {sent:?}");
    assert!(sent.windows(2).all(|w| w[0] == w[1]), "retransmits reuse the SAME transaction id");
    assert_eq!(a.session.pair_snapshots()[0].state, PairState::InProgress);
    let pending = a.session.take_events();
    assert!(pending.is_empty(), "no failure before attempt exhaustion: {pending:?}");

    // One more RTO ticks the attempts counter past 7 → pair Failed →
    // session Failed (all pairs failed).
    a.session.run_once(t).unwrap();
    a.session.run_once(t + CHECK_RTO_MS).unwrap();
    assert_eq!(a.session.pair_snapshots()[0].state, PairState::Failed);
    let events = a.session.take_events();
    assert!(
        matches!(events.iter().find(|e| matches!(e, IceEvent::Failed(_))), Some(IceEvent::Failed(r)) if r.contains("all-pairs")),
        "session Failed with all-pairs reason: {events:?}"
    );
    assert!(
        !events.iter().any(|e| matches!(e, IceEvent::Disconnected)),
        "Disconnected requires prior valid inbound; there was none"
    );
}

/// Fail-closed (governance §二.4): an empty protected-fd provider means the
/// session has NO socket, NO candidate, NO start — and provably never
/// creates an unprotected one (no candidate port exists to send from; the
/// provider handed out nothing).
#[test]
fn empty_provider_fails_closed() {
    let provider = ProtectedUdpFdSource::new_with_fd(-1);
    let mut session = IceSession::new(creds(A), true, Some(0x1111)).expect("session");

    let err = session
        .add_local_candidate(Candidate::host_candidate([127, 0, 0, 1], 0), &provider)
        .expect_err("must fail closed");
    assert!(
        matches!(&err, netbird_core::management::ManagementError::Network(t) if t.contains("no-protected-socket")),
        "fail-closed token: {err:?}"
    );
    assert_eq!(provider.taken(), 0, "empty provider hands out nothing");
    assert!(session.take_events().is_empty(), "no LocalCandidateReady without a socket");
    // With remote credentials set, the missing piece is provably the local
    // candidate: start must refuse.
    session.set_remote_credentials(creds(B)).expect("remote creds");
    let start_err = session.start().expect_err("cannot start without a local candidate");
    assert!(
        matches!(&start_err, netbird_core::management::ManagementError::Request { message, .. } if message.contains("no-local-candidates")),
        "{start_err:?}"
    );
    assert!(session.selected_pair().is_none());
    // A pump on a never-started session is a no-op (no socket access).
    session.run_once(1000).unwrap();
    assert!(session.take_events().is_empty());
}

/// Pair lifecycle: Frozen until start(), Waiting after; RELAY / IPv6 /
/// duplicate remote candidates never form pairs; LocalCandidateReady
/// carries the BOUND candidate.
#[test]
fn pair_lifecycle_frozen_waiting_and_remote_filtering() {
    let mut a = TestPeer::new(creds(A), true, 0x1111);
    let cand = a.add_local();
    assert_ne!(cand.port, 0, "port-0 candidate is rewritten to the bound port");

    // Remote filtering: relay skipped, IPv6 skipped, duplicate collapsed.
    let mut relay = Candidate::host_candidate([127, 0, 0, 1], cand.port);
    relay.typ = netbird_core::ice::CandidateType::Relay;
    a.session.add_remote_candidate(relay);
    assert!(a.session.pair_snapshots().is_empty(), "relay never pairs");

    let v6 = Candidate {
        foundation: "v6".into(),
        component: 1,
        transport: "udp".into(),
        priority: 100,
        address: "::1".into(),
        port: 3478,
        typ: netbird_core::ice::CandidateType::Host,
        related_address: None,
        related_port: None,
    };
    a.session.add_remote_candidate(v6);
    assert!(a.session.pair_snapshots().is_empty(), "IPv6 never pairs (UDP4-only)");

    let good = Candidate::host_candidate([127, 0, 0, 1], cand.port);
    a.session.add_remote_candidate(good.clone());
    assert_eq!(a.session.pair_snapshots().len(), 1);
    assert_eq!(a.session.pair_snapshots()[0].state, PairState::Frozen, "formed-but-not-started is Frozen");
    a.session.add_remote_candidate(good);
    assert_eq!(a.session.pair_snapshots().len(), 1, "duplicate remote collapsed");

    a.session.set_remote_credentials(creds(B)).unwrap();
    a.session.start().unwrap();
    assert_eq!(a.session.pair_snapshots()[0].state, PairState::Waiting, "start thaws to Waiting");

    let events = a.session.take_events();
    assert!(
        events.iter().any(|e| matches!(e, IceEvent::LocalCandidateReady(c) if c.port == cand.port)),
        "LocalCandidateReady carries the bound candidate: {events:?}"
    );
}

// --- local helpers ----------------------------------------------------------

fn drain_requests(sock: &std::net::UdpSocket) -> Vec<TransactionId> {
    let mut out = Vec::new();
    loop {
        let mut buf = [0u8; 1500];
        match sock.recv_from(&mut buf) {
            Ok((n, _)) => {
                if let Ok(p) = parse_stun(&buf[..n]) {
                    if p.msg_type == 0x0001 {
                        out.push(p.txn);
                    }
                }
            }
            Err(_) => return out,
        }
    }
}
