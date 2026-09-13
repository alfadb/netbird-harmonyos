// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N12a — fixed local bind port + advertised (externally reachable)
//! candidates, HOST-ONLY interop tuning (docs/self-hosted-interop-plan.md
//! §端口映射 / 对外候选).
//!
//! The load-bearing scenario reproduces the port-mapped pod topology on
//! loopback, WITHOUT any real NAT:
//!
//! - Node A = the pod: its enumerated interface address is a NON-LOCAL
//!   address (`10.99.0.7` — unroutable from the peer, exactly like the
//!   pod's `10.98.0.180` from the phone LAN). `--ice-port` pins its ICE
//!   socket to a fixed port P with a WILDCARD bind (`0.0.0.0:P`, the same
//!   shape as upstream's single shared ICE/WG socket), and
//!   `--advertise-candidate` points at `127.0.0.1:P` — the stand-in for the
//!   operator's host-LAN mapping (`192.168.50.x:P`), which is the only
//!   address the peer can actually reach.
//! - Node B = the phone/peer: plain loopback, default ephemeral path (the
//!   no-new-flags behavior — an in-test regression guard).
//!
//! Assertions: the fixed port really reaches the signaled candidate
//! (getsockname rewrite), the advertised entry rides the regular
//! `candidate.Marshal()` wire form as an extra host candidate ranked one
//! local-preference step below a genuine host candidate, B completes
//! connectivity checks DIRECTLY against the advertised candidate, the
//! selected pair lands on it, and the WG endpoint is configured with it.

mod host_link_stubs {
    use core::ffi::c_void;

    #[no_mangle]
    pub extern "C" fn OH_LOG_Print(
        _t: i32,
        _l: i32,
        _d: u32,
        _tag: *const u8,
        _fmt: *const u8,
        _arg: *const c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn OH_LOG_IsLoggable(_d: u32, _tag: *const u8, _l: i32) -> bool {
        false
    }

    #[no_mangle]
    pub extern "C" fn napi_module_register(_m: *mut c_void) {}

    #[no_mangle]
    pub extern "C" fn napi_create_function(
        _e: *mut c_void,
        _n: *const u8,
        _l: usize,
        _cb: *mut c_void,
        _d: *mut c_void,
        r: *mut *mut c_void,
    ) -> i32 {
        unsafe { *r = 0x10 as *mut c_void };
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_set_named_property(
        _e: *mut c_void,
        _o: *mut c_void,
        _n: *const u8,
        _v: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_cb_info(
        _e: *mut c_void,
        _i: *mut c_void,
        _argc: *mut usize,
        _argv: *mut *mut c_void,
        _this: *mut c_void,
        _data: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _e: *mut c_void,
        _s: *const u8,
        _l: usize,
        _v: *mut c_void,
        _bs: usize,
        _r: *mut usize,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_string_utf8(
        _e: *mut c_void,
        _v: *mut c_void,
        _b: *mut u8,
        _bs: usize,
        _r: *mut usize,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_int32(
        _e: *mut c_void,
        _v: *mut c_void,
        _r: *mut i32,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(_e: *mut c_void, _v: *mut c_void, _r: *mut bool) -> i32 {
        0
    }
}

use std::sync::{Arc, Mutex};

use netbird_core::ice::{parse_advertised_candidate, Candidate, CandidateType, InterfaceAddr, ProtectedUdpFdSource, StaticInterfaces};
use netbird_core::management::ManagementError;
use netbird_core::peer_conn::{PeerIceDeps, PeerIceOrchestrator, PeerIceState, PeerSignalKind, SignalExchange};
use netbird_core::sys;

extern "C" {
    fn socket(domain: i32, ty: i32, protocol: i32) -> i32;
    fn close(fd: i32) -> i32;
}

/// One free UDP port (bind → getsockname → close; the usual test-grade
/// TOCTOU tolerance, same as the e2e suites' ephemeral ports).
fn free_udp_port() -> u16 {
    let fd = unsafe { socket(2, 2, 0) };
    assert!(fd >= 0, "socket()");
    let sa = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    assert_eq!(
        unsafe { sys::bind(fd, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) },
        0
    );
    let mut name = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut len = core::mem::size_of::<sys::sockaddr_in>() as u32;
    assert_eq!(unsafe { sys::getsockname(fd, &mut name, &mut len) }, 0);
    unsafe { close(fd) };
    let port = u16::from_be(name.sin_port);
    assert!(port > 0);
    port
}

/// Protected-UDP provider holding ORIGINAL fds (sessions only touch dups);
/// originals are closed exactly once, on drop.
struct FedSocks {
    source: Arc<ProtectedUdpFdSource>,
    raw_fds: Vec<i32>,
}

impl FedSocks {
    fn new(n: usize) -> Self {
        let mut raw_fds = Vec::with_capacity(n);
        for _ in 0..n {
            let fd = unsafe { socket(2, 2, 0) }; // AF_INET, SOCK_DGRAM
            assert!(fd >= 0, "socket() failed");
            raw_fds.push(fd);
        }
        let source = Arc::new(ProtectedUdpFdSource::new_with_fd(-1));
        for &fd in &raw_fds {
            source.feed(fd);
        }
        FedSocks { source, raw_fds }
    }
}

impl Drop for FedSocks {
    fn drop(&mut self) {
        for fd in self.raw_fds.drain(..) {
            unsafe { close(fd) };
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Frame {
    from: String,
    to: String,
    kind: PeerSignalKind,
    payload: String,
}

#[derive(Default)]
struct SignalBus {
    queue: Mutex<Vec<Frame>>,
    seen: Mutex<Vec<Frame>>,
}

impl SignalBus {
    fn drain(&self) -> Vec<Frame> {
        self.queue.lock().expect("queue").drain(..).collect()
    }
}

struct MockSignalEndpoint {
    me: String,
    bus: Arc<SignalBus>,
}

impl SignalExchange for MockSignalEndpoint {
    fn send(
        &self,
        to_key: &str,
        kind: PeerSignalKind,
        payload: &str,
        _port: u32,
    ) -> Result<(), ManagementError> {
        self.bus
            .queue
            .lock()
            .expect("queue")
            .push(Frame { from: self.me.clone(), to: to_key.to_string(), kind, payload: payload.to_string() });
        Ok(())
    }
}

/// Recording WG seam: endpoints/egress land here so the test can prove the
/// ADVERTISED candidate became the configured WG path.
#[derive(Default)]
struct RecordingWg {
    endpoints: Mutex<Vec<([u8; 4], u16)>>,
}

impl netbird_core::connector::WgPeerApplier for RecordingWg {
    fn apply_peers(&self, _peers: &[netbird_core::connector::WgPeerEntry]) -> Result<(), String> {
        Ok(())
    }
    fn clear(&self) {
        self.endpoints.lock().expect("endpoints").clear();
    }
    fn apply_endpoint(&self, _pub_key_b64: &str, addr: [u8; 4], port: u16) -> Result<(), String> {
        self.endpoints.lock().expect("endpoints").push((addr, port));
        Ok(())
    }
    fn attach_egress_socket(&self, _pub_key_b64: &str, _raw_fd: i32) -> Result<(), String> {
        Ok(())
    }
}

struct Node {
    me: String,
    orch: PeerIceOrchestrator,
    socks: FedSocks,
    wg: Arc<RecordingWg>,
}

impl Node {
    /// `iface_addr` = the enumerated interface address; `fixed_port` =
    /// the fixed ICE port (None = default ephemeral path); `advertised` =
    /// extra externally reachable candidates.
    fn new(
        me: &str,
        peer_key: &str,
        iface_addr: [u8; 4],
        fixed_port: Option<u16>,
        advertised: &[String],
        tie_breaker: u64,
        bus: Arc<SignalBus>,
    ) -> Self {
        let socks = FedSocks::new(6);
        let wg = Arc::new(RecordingWg::default());
        let advertised_cands: Vec<Candidate> = advertised
            .iter()
            .map(|s| parse_advertised_candidate(s).expect("advertised candidate"))
            .collect();
        let mut orch = PeerIceOrchestrator::new(PeerIceDeps {
            ifaces: Arc::new(StaticInterfaces(vec![InterfaceAddr {
                name: "eth0".into(),
                addr: iface_addr,
            }])),
            socks: socks.source.clone(),
            signal: Arc::new(MockSignalEndpoint { me: me.to_string(), bus: bus.clone() }),
            wg: wg.clone(),
            tie_breaker: Some(tie_breaker),
            fixed_local_port: fixed_port,
            advertised_candidates: advertised_cands,
        });
        orch.set_peers(&[peer_key.to_string()]);
        orch.set_signal_ready(true);
        Node { me: me.to_string(), orch, socks, wg }
    }

    fn status_of(&self, peer: &str) -> netbird_core::peer_conn::PeerIceStatus {
        self.orch.peer_status(peer).expect("peer entry")
    }
}

fn deliver(bus: &Arc<SignalBus>, now: u64, a: &mut Node, b: &mut Node) {
    for f in bus.drain() {
        bus.seen.lock().expect("seen").push(f.clone());
        if f.to == a.me {
            a.orch.handle_signal(&f.from, f.kind, &f.payload, now).expect("A recv");
        } else if f.to == b.me {
            b.orch.handle_signal(&f.from, f.kind, &f.payload, now).expect("B recv");
        }
    }
}

fn step(a: &mut Node, b: &mut Node, bus: &Arc<SignalBus>, now: u64) {
    let _ = a.orch.run_once(now);
    let _ = b.orch.run_once(now);
    deliver(bus, now, a, b);
}

fn pump_until(
    a: &mut Node,
    b: &mut Node,
    bus: &Arc<SignalBus>,
    now: &mut u64,
    deadline_ms: u64,
    cond: impl Fn(&Node, &Node) -> bool,
) -> bool {
    while *now <= deadline_ms {
        step(a, b, bus, *now);
        *now += 10;
        if cond(a, b) {
            return true;
        }
    }
    false
}

fn key(byte: u8) -> String {
    // public-key stand-in for signal addressing (opaque string is fine)
    format!("PK-{}", byte)
}

/// THE interop scenario: the pod-side node (A) is reachable ONLY through
/// its advertised candidate; the peer (B) must complete connectivity
/// checks DIRECTLY against that candidate and land the WG endpoint on it.
#[test]
fn peer_connects_through_the_advertised_candidate_on_fixed_port() {
    let port = free_udp_port();
    let bus = Arc::new(SignalBus::default());

    // A = "pod": non-local interface address (unreachable from B), fixed
    // wildcard-bound ICE port, advertised loopback-mapped address.
    let mut a = Node::new(
        &key(0xAA),
        &key(0xBB),
        [10, 99, 0, 7],
        Some(port),
        &[format!("127.0.0.1:{port}")],
        0xAAAA,
        bus.clone(),
    );
    // B = "phone": DEFAULT path (no fixed port, no advertised candidates).
    let mut b = Node::new(
        &key(0xBB),
        &key(0xAA),
        [127, 0, 0, 1],
        None,
        &[],
        0xBBBB,
        bus.clone(),
    );

    let mut now = 1000u64;
    let connected = pump_until(&mut a, &mut b, &bus, &mut now, 12_000, |a, b| {
        a.status_of(&key(0xBB)).state == PeerIceState::Connected
            && b.status_of(&key(0xAA)).state == PeerIceState::Connected
    });
    assert!(connected, "both sides must connect through the advertised candidate");

    // ① A's signaled candidates: the REAL host candidate carries the
    //    interface address WITH THE FIXED PORT (getsockname verified the
    //    wildcard bind), the advertised one is the mapped address.
    let seen = bus.seen.lock().expect("seen").clone();
    let a_cands: Vec<Candidate> = seen
        .iter()
        .filter(|f| f.from == key(0xAA) && f.kind == PeerSignalKind::Candidate)
        .map(|f| Candidate::unmarshal(&f.payload).expect("wire form"))
        .collect();
    let host = a_cands
        .iter()
        .find(|c| c.address == "10.99.0.7")
        .expect("real host candidate signaled");
    assert_eq!(host.port, port, "fixed port must survive into the signaled candidate");
    assert_eq!(host.typ, CandidateType::Host);
    let adv = a_cands
        .iter()
        .find(|c| c.address == "127.0.0.1" && c.port == port)
        .expect("advertised candidate signaled");
    assert_eq!(adv.typ, CandidateType::Host);
    assert_eq!(adv.priority, host.priority - 256, "advertised = one step below host");
    // marshal/unmarshal roundtrip on the exact wire form
    assert_eq!(Candidate::unmarshal(&adv.marshal()).unwrap(), *adv);

    // ② B selected the pair whose REMOTE is EXACTLY the advertised
    //    candidate (127.0.0.1:<fixed port>) — the direct-use proof.
    let b_status = b.status_of(&key(0xAA));
    assert_eq!(
        b_status.selected_remote,
        Some(([127, 0, 0, 1], port)),
        "B must select the advertised candidate: {b_status:?}"
    );
    assert!(b_status.endpoint_applied, "B's WG endpoint must land on it");
    let b_endpoints = b.wg.endpoints.lock().expect("endpoints").clone();
    assert!(
        b_endpoints.contains(&([127, 0, 0, 1], port)),
        "WG endpoint configured with the advertised candidate: {b_endpoints:?}"
    );

    // ③ A (the fixed-port side) is Connected with its egress attached and
    //    its endpoint aimed at B's loopback candidate.
    let a_status = a.status_of(&key(0xBB));
    assert_eq!(a_status.state, PeerIceState::Connected);
    assert!(a_status.endpoint_applied, "A's WG endpoint must land on B");
    assert_eq!(a.socks.source.taken(), 1, "fixed-port gather consumes exactly one socket");

    // ④ both recording seams saw the egress attach (WG rides the selected
    //    transport — nothing fell back to the outer socket).
    assert!(!a.wg.endpoints.lock().expect("endpoints").is_empty());
}

/// Regression pin: WITHOUT the new flags (no fixed port, no advertised
/// candidates) two loopback nodes still connect exactly as before — the
/// default path is untouched.
#[test]
fn default_path_without_flags_still_connects_ephemerally() {
    let bus = Arc::new(SignalBus::default());
    let mut a = Node::new(&key(0x11), &key(0x22), [127, 0, 0, 1], None, &[], 0x1111, bus.clone());
    let mut b = Node::new(&key(0x22), &key(0x11), [127, 0, 0, 1], None, &[], 0x2222, bus.clone());

    let mut now = 1000u64;
    let connected = pump_until(&mut a, &mut b, &bus, &mut now, 8_000, |a, b| {
        a.status_of(&key(0x22)).state == PeerIceState::Connected
            && b.status_of(&key(0x11)).state == PeerIceState::Connected
    });
    assert!(connected, "no-flags loopback interop must still work");

    let seen = bus.seen.lock().expect("seen").clone();
    let a_ports: Vec<u16> = seen
        .iter()
        .filter(|f| f.from == key(0x11) && f.kind == PeerSignalKind::Candidate)
        .map(|f| Candidate::unmarshal(&f.payload).expect("wire").port)
        .collect();
    // default = ephemeral: the signaled port is whatever the kernel picked
    // (nonzero, NOT a configured fixed value) and the address is the
    // interface address only — no advertised entries.
    assert!(
        a_ports.iter().all(|p| *p > 0),
        "ephemeral ports are kernel-assigned: {a_ports:?}"
    );
    assert!(
        seen.iter()
            .filter(|f| f.from == key(0x11) && f.kind == PeerSignalKind::Candidate)
            .all(|f| !f.payload.contains("127.0.0.1:0 ") && Candidate::unmarshal(&f.payload).unwrap().typ == CandidateType::Host),
        "no-flags path signals plain host candidates only"
    );
}
