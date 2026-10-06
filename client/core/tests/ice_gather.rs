// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N5a gather tests against an in-process mock STUN server (std::net UDP,
//! minimal Binding Request/Response with XOR-MAPPED-ADDRESS). The protected
//! socket path is proven three ways per the mgmt `taken == attempts`
//! audit shape:
//!
//! 1. the source's `taken()` equals the gather's socket attempts (fresh fd
//!    per interface round — no reuse across rounds);
//! 2. the PROVIDED fd itself ends up bound to the host candidate's port —
//!    the gather dups + binds the dup, and the dup shares the open-file
//!    description with the original (an own-socket cheat would leave the
//!    provided fd unbound);
//! 3. the mock only ever sees datagrams sourced from that port;
//! 4. the bound socket is handed over ALIVE (`GatheredSocket`) and the session
//!    that adopts it advertises/sends/receives on that same socket — the 0052
//!    fix (see `gathered_socket_stays_live_and_carries_the_session_on_the_
//!    advertised_port`).
//!
//! All timings are deadlines under test (150 ms timeout case), never sleeps
//! as assertions. Offline: loopback + literal IPs only, no DNS.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use netbird_core::ice::{
    gather_candidates, interface_allowed, Candidate, CandidateType, GatherConfig,
    InterfaceAddr, ProtectedUdpFdSource, StaticInterfaces, DEFAULT_INTERFACE_BLACKLIST,
};
use netbird_core::management::ManagementError;
use netbird_core::stun::MAGIC_COOKIE;

use netbird_core::ice_session::{
    build_check_request, parse_stun, verify_message_integrity, CheckRequest, IceCredentials,
    IceSession,
};
use netbird_core::stun::{random_transaction_id, BINDING_REQUEST, BINDING_SUCCESS};

// --- host-test link surface (same as tests/management_grpc.rs): the test
// --- binary links the whole crate rlib on the host triple, where
// --- libace_napi.z.so / libhilog_ndk.z.so do not exist. No-ops only.
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

// --- raw fd helpers: create an UNBOUND protected-style UDP4 socket --------

extern "C" {
    fn socket(domain: i32, ty: i32, protocol: i32) -> i32;
    fn close(fd: i32) -> i32;
    fn getsockname(fd: i32, addr: *mut SockAddrIn, len: *mut u32) -> i32;
    fn bind(fd: i32, addr: *const SockAddrIn, len: u32) -> i32;
}

#[repr(C)]
struct SockAddrIn {
    family: u16,
    port: u16, // big endian
    addr: [u8; 4],
    zero: [u8; 8],
}

/// The shell-side shape: fresh, UNBOUND, AF_INET/SOCK_DGRAM fd (here: NOT
/// actually protected — loopback tests; production protects before feed).
fn unbound_udp_fd() -> i32 {
    let fd = unsafe { socket(2, 2, 0) };
    assert!(fd >= 0, "socket() failed");
    fd
}

/// Local port of a bound fd (0 = unbound) — the proof-2 probe.
fn fd_port(fd: i32) -> u16 {
    let mut a = SockAddrIn { family: 0, port: 0, addr: [0; 4], zero: [0; 8] };
    let mut len = std::mem::size_of::<SockAddrIn>() as u32;
    assert_eq!(unsafe { getsockname(fd, &mut a, &mut len) }, 0, "getsockname");
    u16::from_be(a.port)
}

/// `sockaddr_in` for the raw bind probe (AF_INET, big-endian port).
fn sa(addr: [u8; 4], port: u16) -> SockAddrIn {
    SockAddrIn { family: 2, port: port.to_be(), addr, zero: [0; 8] }
}

/// Can a fresh socket take `addr:port` RIGHT NOW? `false` = a live socket
/// (ours) currently owns the address — the ownership probe the 0052 tests use.
/// No `SO_REUSEADDR` anywhere, so a held `addr:port` is an honest EADDRINUSE.
fn port_is_free(addr: [u8; 4], port: u16) -> bool {
    let fd = unbound_udp_fd();
    let rc = unsafe { bind(fd, &sa(addr, port), std::mem::size_of::<SockAddrIn>() as u32) };
    unsafe { close(fd) };
    rc == 0
}

// --- mock STUN server ------------------------------------------------------

#[derive(Clone, Copy)]
enum MockMode {
    /// Binding Success, XOR-MAPPED-ADDRESS = datagram source.
    Echo,
    /// Receive, never answer.
    Silent,
    /// Binding Success with a ZEROED transaction id — unauthenticated
    /// datagram, must be dropped, server effectively times out.
    WrongTxn,
    /// Echo the txn but corrupt the magic cookie.
    BadCookie,
    /// Echo the txn with message type 0x0999.
    BadType,
    /// Echo the txn with an IPv6-family XOR-MAPPED-ADDRESS.
    Ipv6Mapped,
    /// Binding Error Response with ERROR-CODE 400.
    StunError400,
}

struct MockStun {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    src_ports: Arc<Mutex<Vec<u16>>>,
    handle: JoinHandle<()>,
}

impl MockStun {
    fn spawn(mode: MockMode) -> Self {
        let sock = std::net::UdpSocket::bind("127.0.0.1:0").expect("mock bind");
        sock.set_read_timeout(Some(Duration::from_millis(100))).expect("timeout");
        let addr = sock.local_addr().expect("local addr");
        let stop = Arc::new(AtomicBool::new(false));
        let src_ports = Arc::new(Mutex::new(Vec::new()));
        let (stop_t, ports_t) = (stop.clone(), src_ports.clone());
        let handle = std::thread::spawn(move || {
            let mut buf = [0u8; 1500];
            while !stop_t.load(Ordering::Acquire) {
                let (n, src) = match sock.recv_from(&mut buf) {
                    Ok(x) => x,
                    Err(_) => continue, // timeout → re-check stop
                };
                ports_t.lock().expect("src ports").push(src.port());
                if n < 20 || u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]) != MAGIC_COOKIE {
                    continue; // not a STUN request we model
                }
                let mut txn = [0u8; 12];
                txn.copy_from_slice(&buf[8..20]);
                let reply = match mode {
                    MockMode::Silent => continue,
                    MockMode::Echo => success_response(&txn, &buf, src),
                    MockMode::WrongTxn => success_response(&[0u8; 12], &buf, src),
                    MockMode::BadCookie => {
                        let mut m = success_response(&txn, &buf, src);
                        m[5] ^= 0xff;
                        m
                    }
                    MockMode::BadType => {
                        let mut m = success_response(&txn, &buf, src);
                        m[0..2].copy_from_slice(&0x0999u16.to_be_bytes());
                        m
                    }
                    MockMode::Ipv6Mapped => success_response_v6(&txn),
                    MockMode::StunError400 => error_response(&txn, 400),
                };
                let _ = sock.send_to(&reply, src);
            }
        });
        MockStun { addr, stop, src_ports, handle }
    }

    /// `netbird_config.stuns[].uri` form for the mock.
    fn uri(&self) -> String {
        format!("stun:{}", self.addr)
    }

    fn seen_source_ports(&self) -> Vec<u16> {
        self.src_ports.lock().expect("src ports").clone()
    }
}

impl Drop for MockStun {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = &self.handle; // thread exits on its next 100 ms timeout tick
    }
}

fn success_response(txn: &[u8; 12], _req: &[u8], src: SocketAddr) -> Vec<u8> {
    let port = src.port();
    let addr = match src.ip() {
        std::net::IpAddr::V4(v4) => v4.octets(),
        _ => panic!("mock is IPv4-only"),
    };
    let xport = port ^ (MAGIC_COOKIE >> 16) as u16;
    let mut m = Vec::with_capacity(20 + 12);
    m.extend_from_slice(&0x0101u16.to_be_bytes());
    m.extend_from_slice(&12u16.to_be_bytes()); // attr header 4 + value 8
    m.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
    m.extend_from_slice(txn);
    m.extend_from_slice(&0x0020u16.to_be_bytes()); // XOR-MAPPED-ADDRESS
    m.extend_from_slice(&8u16.to_be_bytes());
    m.extend_from_slice(&[0x00, 0x01]);
    m.extend_from_slice(&xport.to_be_bytes());
    for (i, b) in addr.iter().enumerate() {
        m.push(b ^ (MAGIC_COOKIE >> (24 - 8 * i)) as u8);
    }
    m
}

fn success_response_v6(txn: &[u8; 12]) -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(&0x0101u16.to_be_bytes());
    m.extend_from_slice(&24u16.to_be_bytes()); // attr header 4 + value 20
    m.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
    m.extend_from_slice(txn);
    m.extend_from_slice(&0x0020u16.to_be_bytes());
    m.extend_from_slice(&20u16.to_be_bytes());
    m.extend_from_slice(&[0x00, 0x02]); // family: IPv6 — unsupported in N5a
    m.extend_from_slice(&[0u8; 18]);
    m
}

fn error_response(txn: &[u8; 12], code: u16) -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(&0x0111u16.to_be_bytes());
    m.extend_from_slice(&8u16.to_be_bytes());
    m.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
    m.extend_from_slice(txn);
    m.extend_from_slice(&0x0009u16.to_be_bytes()); // ERROR-CODE
    m.extend_from_slice(&4u16.to_be_bytes());
    let class = (code / 100) as u8;
    let number = (code % 100) as u8;
    m.extend_from_slice(&[0x00, 0x00, class & 0x07, number]);
    m
}

// --- fixtures --------------------------------------------------------------

fn iface(name: &str, addr: [u8; 4]) -> InterfaceAddr {
    InterfaceAddr { name: name.to_string(), addr }
}

struct ProviderFixture {
    source: ProtectedUdpFdSource,
    fds: Vec<i32>,
}

impl ProviderFixture {
    fn with_fds(count: usize) -> Self {
        let mut fixture = ProviderFixture { source: ProtectedUdpFdSource::new_with_fd(-1).unwrap(), fds: Vec::new() };
        fixture.refill(count);
        fixture
    }

    fn refill(&mut self, count: usize) {
        for _ in 0..count {
            let fd = unbound_udp_fd();
            self.fds.push(fd);
            self.source.feed(fd);
        }
    }
}

impl Drop for ProviderFixture {
    fn drop(&mut self) {
        for fd in self.fds.drain(..) {
            unsafe { close(fd) };
        }
    }
}

// --- tests -----------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn protected_stun_dns_without_shell_reply_does_not_block_gather() {
    use std::time::Instant;
    let provider = ProviderFixture::with_fds(1);
    let supply = Arc::new(netbird_core::socket_supply::SocketSupply::new());
    supply.set_dns_servers(vec!["127.0.0.1".parse().unwrap()]);
    provider.source.attach_dns(supply, tokio::runtime::Handle::current());
    let servers = vec![netbird_core::ice::parse_stun_uri("stun:stun.example.invalid:3478").unwrap()];
    provider.source.refresh_stun_dns(&servers);
    let ifaces = StaticInterfaces(vec![iface("eth0", [127, 0, 0, 1])]);
    let cfg = GatherConfig { blacklist: &DEFAULT_INTERFACE_BLACKLIST, servers: &servers, timeout_ms: 100 };
    let start = Instant::now();
    let result = gather_candidates(&cfg, &ifaces, &provider.source).unwrap();
    assert!(start.elapsed() < Duration::from_secs(1), "gather must not await the 8s supply deadline");
    assert_eq!(result.host.len(), 1);
    assert!(result.srflx.is_empty(), "unresolved STUN host cannot produce a candidate");
    assert_eq!(provider.source.taken(), 1);
    assert!(result.errors.iter().any(|(ctx, _)| ctx == "-/stun.example.invalid"));
}

/// Full happy path: one interface, one STUN server — srflx mapping parses,
/// traffic provably rides the PROVIDED protected fd (proofs 1–3 in the
/// module docs), and the srflx candidate references its base.
#[test]
fn srflx_gather_rides_the_protected_socket() {
    let mock = MockStun::spawn(MockMode::Echo);
    let mut provider = ProviderFixture::with_fds(1);
    let ifaces = StaticInterfaces(vec![iface("eth0", [127, 0, 0, 1])]);
    let servers = vec![netbird_core::ice::parse_stun_uri(&mock.uri()).expect("uri")];
    let cfg = GatherConfig { blacklist: &DEFAULT_INTERFACE_BLACKLIST, servers: &servers, timeout_ms: 1500 };

    let result = gather_candidates(&cfg, &ifaces, &provider.source).expect("gather ok");

    assert!(result.errors.is_empty(), "unexpected errors: {:?}", result.errors);
    assert_eq!(result.host.len(), 1, "one host candidate");
    let base = &result.host[0];
    assert_eq!(base.address, "127.0.0.1");
    assert_eq!(base.typ, CandidateType::Host);
    assert_ne!(base.port, 0, "host candidate carries the bound ephemeral port");

    // Proof 1: exactly one fd handed out for one interface round.
    assert_eq!(provider.source.taken(), 1);

    // Proof 2: the PROVIDED fd shares the bound port (dup+bind on the dup).
    assert_eq!(fd_port(provider.fds[0]), base.port, "provided fd must be the one bound");

    // Proof 3: the mock saw our request sourced from that port only.
    assert_eq!(mock.seen_source_ports(), vec![base.port]);

    // The srflx candidate mirrors the mock's mapping (no NAT on loopback).
    assert_eq!(result.srflx.len(), 1);
    let srflx = &result.srflx[0];
    assert_eq!(srflx.typ, CandidateType::Srflx);
    assert_eq!(srflx.address, "127.0.0.1");
    assert_eq!(srflx.port, base.port);
    assert_eq!(srflx.related_address.as_deref(), Some("127.0.0.1"));
    assert_eq!(srflx.related_port, Some(base.port));
    assert!(srflx.priority < base.priority);
    assert_eq!(
        Candidate::unmarshal(&srflx.marshal()).unwrap(),
        *srflx,
        "produced candidate marshals cleanly for the signal payload"
    );
}

/// Fail-closed: an empty provider means NO socket, NO candidate, and a hard
/// error — never an unprotected socket (governance §二.4).
#[test]
fn empty_provider_fails_closed() {
    let mock = MockStun::spawn(MockMode::Echo);
    let provider = ProtectedUdpFdSource::new_with_fd(-1).unwrap();
    let ifaces = StaticInterfaces(vec![iface("eth0", [127, 0, 0, 1])]);
    let servers = vec![netbird_core::ice::parse_stun_uri(&mock.uri()).expect("uri")];
    let cfg = GatherConfig { blacklist: &DEFAULT_INTERFACE_BLACKLIST, servers: &servers, timeout_ms: 300 };

    let err = gather_candidates(&cfg, &ifaces, &provider).expect_err("must fail closed");
    assert!(
        matches!(&err, ManagementError::Network(t) if t.contains("no-protected-socket")),
        "wrong failure: {err:?}"
    );
    assert_eq!(provider.taken(), 0);
}

/// Audit: every interface round of every gather takes a FRESH fd —
/// `taken()` accumulates exactly with attempts, across rounds (the mgmt
/// `every_redial_takes_a_fresh_fd` contract, generalized to UDP).
#[test]
fn every_round_takes_a_fresh_fd() {
    let mut provider = ProviderFixture::with_fds(2);
    let ifaces =
        StaticInterfaces(vec![iface("eth0", [127, 0, 0, 1]), iface("eth1", [127, 0, 0, 2])]);
    let cfg = GatherConfig { blacklist: &DEFAULT_INTERFACE_BLACKLIST, servers: &[], timeout_ms: 100 };

    let r1 = gather_candidates(&cfg, &ifaces, &provider.source).expect("round 1");
    assert_eq!(r1.host.len(), 2);
    assert_eq!(provider.source.taken(), 2, "two interfaces → two sockets");
    assert_eq!(provider.source.pending(), 0);

    provider.refill(2);
    let r2 = gather_candidates(&cfg, &ifaces, &provider.source).expect("round 2");
    assert_eq!(r2.host.len(), 2);
    assert_eq!(provider.source.taken(), 4, "no fd reuse across gather rounds");
    assert_ne!(r1.host[0].port, 0);
    assert_ne!(r2.host[0].port, 0);
}

/// VPN/loopback exclusion (upstream filter.go semantics +
/// profilemanager/config.go:56-59 defaults): tunnel-named and loopback
/// interfaces never become candidates; with no allowed interface there is
/// no socket consumption at all.
#[test]
fn vpn_and_loopback_interfaces_are_excluded() {
    // Prefix semantics, upstream stdnet/filter.go:13-24.
    assert!(!interface_allowed("lo", &DEFAULT_INTERFACE_BLACKLIST));
    assert!(!interface_allowed("lo0", &DEFAULT_INTERFACE_BLACKLIST));
    assert!(!interface_allowed("wt0", &DEFAULT_INTERFACE_BLACKLIST)); // our tunnel
    assert!(!interface_allowed("wth1", &DEFAULT_INTERFACE_BLACKLIST)); // prefix "wt"
    assert!(!interface_allowed("utun3", &DEFAULT_INTERFACE_BLACKLIST));
    assert!(!interface_allowed("wg1", &DEFAULT_INTERFACE_BLACKLIST));
    assert!(!interface_allowed("docker0", &DEFAULT_INTERFACE_BLACKLIST));
    assert!(!interface_allowed("br-abc123", &DEFAULT_INTERFACE_BLACKLIST));
    assert!(interface_allowed("eth0", &DEFAULT_INTERFACE_BLACKLIST));
    assert!(interface_allowed("wlan0", &DEFAULT_INTERFACE_BLACKLIST));

    let provider = ProviderFixture::with_fds(1);
    let ifaces = StaticInterfaces(vec![
        iface("lo", [127, 0, 0, 1]),
        iface("wt0", [10, 64, 0, 7]), // THE tunnel interface itself
        iface("utun4", [100, 100, 1, 2]),
        // Bindable on every host: the whole 127/8 loopback range routes
        // locally, so the single allowed interface can actually bind.
        iface("eth0", [127, 0, 0, 9]),
    ]);
    let cfg = GatherConfig { blacklist: &DEFAULT_INTERFACE_BLACKLIST, servers: &[], timeout_ms: 100 };

    let result = gather_candidates(&cfg, &ifaces, &provider.source).expect("gather ok");
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.host.len(), 1, "only eth0 survives");
    assert_eq!(result.host[0].address, "127.0.0.9");
    assert_eq!(provider.source.taken(), 1, "no socket attempted for excluded ifaces");
    assert!(!result
        .host
        .iter()
        .any(|c| c.address.starts_with("10.64.") || c.address == "127.0.0.1" || c.address.starts_with("100.100.")));
}

/// A silent STUN server burns its deadline and is classified Timeout —
/// while the host candidate from the same round still exists (STUN failure
/// must not cost the host candidate; upstream gathers per-candidate too).
#[test]
fn silent_server_times_out_but_host_survives() {
    let mock = MockStun::spawn(MockMode::Silent);
    let mut provider = ProviderFixture::with_fds(1);
    let ifaces = StaticInterfaces(vec![iface("eth0", [127, 0, 0, 1])]);
    let servers = vec![netbird_core::ice::parse_stun_uri(&mock.uri()).expect("uri")];
    let cfg = GatherConfig { blacklist: &DEFAULT_INTERFACE_BLACKLIST, servers: &servers, timeout_ms: 150 };

    let result = gather_candidates(&cfg, &ifaces, &provider.source).expect("partial success");
    assert_eq!(result.host.len(), 1, "host candidate survives the STUN failure");
    assert!(result.srflx.is_empty());
    assert_eq!(result.errors.len(), 1);
    assert!(
        matches!(result.errors[0].1, ManagementError::Timeout),
        "silent server must be Timeout: {:?}",
        result.errors[0]
    );
    assert!(result.errors[0].0.contains(&mock.addr.to_string()));
    // The deadline is honored, not retried to infinity: the fd was taken
    // once and released.
    assert_eq!(provider.source.taken(), 1);
}

/// Hostile/buggy servers: every bad-response shape is classified, nothing
/// panics, and the surviving server still produces its srflx candidate.
/// A datagram whose transaction id matches nothing is unauthenticated noise
/// — dropped, and its server shows Timeout (WrongTxn case).
#[test]
fn hostile_servers_are_classified_without_panic() {
    let garbage_cookie = MockStun::spawn(MockMode::BadCookie);
    let bad_type = MockStun::spawn(MockMode::BadType);
    let stun_error = MockStun::spawn(MockMode::StunError400);
    let ipv6 = MockStun::spawn(MockMode::Ipv6Mapped);
    let wrong_txn = MockStun::spawn(MockMode::WrongTxn);
    let echo = MockStun::spawn(MockMode::Echo);

    let mut provider = ProviderFixture::with_fds(1);
    let ifaces = StaticInterfaces(vec![iface("eth0", [127, 0, 0, 1])]);
    let servers: Vec<_> = [&garbage_cookie, &bad_type, &stun_error, &ipv6, &wrong_txn, &echo]
        .iter()
        .map(|m| netbird_core::ice::parse_stun_uri(&m.uri()).expect("uri"))
        .collect();
    let cfg = GatherConfig { blacklist: &DEFAULT_INTERFACE_BLACKLIST, servers: &servers, timeout_ms: 400 };

    let result = gather_candidates(&cfg, &ifaces, &provider.source).expect("partial success");
    assert_eq!(result.host.len(), 1);
    assert_eq!(provider.source.taken(), 1);

    let by_server = |mock: &MockStun| -> Option<&ManagementError> {
        result
            .errors
            .iter()
            .find(|(ctx, _)| ctx.contains(&mock.addr.to_string()))
            .map(|(_, e)| e)
    };
    assert!(matches!(by_server(&garbage_cookie), Some(ManagementError::Parse(t)) if t.contains("bad-magic")));
    assert!(matches!(by_server(&bad_type), Some(ManagementError::Parse(t)) if t.contains("unexpected-type")));
    assert!(matches!(by_server(&stun_error), Some(ManagementError::Server { status: 400 })));
    assert!(matches!(by_server(&ipv6), Some(ManagementError::Parse(t)) if t.contains("no-ipv4-mapping")));
    assert!(matches!(by_server(&wrong_txn), Some(ManagementError::Timeout)), "unknown txn → silent drop → deadline");
    assert_eq!(by_server(&echo), None, "the healthy server records no error");

    assert_eq!(result.srflx.len(), 1, "only the healthy server yields srflx");
    assert_eq!(result.srflx[0].address, "127.0.0.1");
}

/// A dead STUN hostname is a recorded per-server Network error; the rest of
/// the gather is unaffected. No real DNS in tests: literal-octet forms that
/// cannot parse fail on the fast path without touching a resolver.
#[test]
fn unresolvable_server_is_recorded_and_skipped() {
    let mut provider = ProviderFixture::with_fds(1);
    let ifaces = StaticInterfaces(vec![iface("eth0", [127, 0, 0, 1])]);
    let servers = vec![netbird_core::ice::parse_stun_uri("stun:999.999.999.999:3478").expect("uri parses")];
    let cfg = GatherConfig { blacklist: &DEFAULT_INTERFACE_BLACKLIST, servers: &servers, timeout_ms: 100 };

    let result = gather_candidates(&cfg, &ifaces, &provider.source).expect("partial success");
    assert_eq!(result.host.len(), 1, "host candidate unaffected");
    assert!(result.srflx.is_empty());
    assert_eq!(result.errors.len(), 1);
    assert!(
        matches!(&result.errors[0].1, ManagementError::Network(t) if t.contains("stun-resolve")),
        "{:?}",
        result.errors[0]
    );
}

/// resolve_ipv4 sanity: literals resolve on the fast path (no resolver),
/// malformed literals fail with a Network error — never a guessed address.
#[test]
fn resolve_literal_addresses() {
    let ok = netbird_core::ice::resolve_ipv4("127.0.0.1").expect("literal");
    assert_eq!(ok, [127, 0, 0, 1]);
    let err = netbird_core::ice::resolve_ipv4("999.999.999.999").expect_err("not an address");
    assert!(matches!(err, ManagementError::Network(_)));
}

/// No usable interface (everything filtered) is an EMPTY success, not an
/// error — and consumes no sockets.
#[test]
fn no_usable_interface_is_an_empty_success() {
    let provider = ProviderFixture::with_fds(1);
    let ifaces = StaticInterfaces(vec![iface("wt0", [10, 64, 0, 7]), iface("lo", [127, 0, 0, 1])]);
    let cfg = GatherConfig { blacklist: &DEFAULT_INTERFACE_BLACKLIST, servers: &[], timeout_ms: 100 };

    let result = gather_candidates(&cfg, &ifaces, &provider.source).expect("empty gather is ok");
    assert!(result.host.is_empty() && result.srflx.is_empty() && result.errors.is_empty());
    assert_eq!(provider.source.taken(), 0, "filtered interfaces take no socket");
}

/// N10: the PRODUCTION interface source must load libc at runtime on both
/// OHOS (musl, `libc.so`) and glibc hosts (`libc.so.6`; plain `libc.so`
/// there is a linker-script stub `dlopen` rejects). Regressed as
/// `interface-enum: dlopen libc.so failed` → ICE stuck `idle` with
/// `pump error (request)` on the host interop CLI.
#[test]
fn system_interfaces_list_works_on_this_host() {
    let list =
        netbird_core::ice::InterfaceSource::list(&netbird_core::ice::SystemInterfaces)
            .expect("getifaddrs via libc fallback");
    assert!(!list.is_empty(), "a host always has at least one IPv4 interface");
}

/// N10: the STUN hostname resolver rides the same libc fallback — a
/// NAME (not a literal) forces the getaddrinfo path. `localhost` resolves
/// offline via /etc/hosts; no network is touched.
#[test]
fn resolve_ipv4_hostname_loads_libc_on_this_host() {
    let got = netbird_core::ice::resolve_ipv4("localhost").expect("getaddrinfo via libc fallback");
    assert_eq!(got, [127, 0, 0, 1]);
}

// ---------------------------------------------------------------------------
// 0052: gathered-socket ownership — the advertised host/base port and the
// STUN mapping must belong to the SAME still-live socket the session uses.
// ---------------------------------------------------------------------------

/// THE 0052 regression: the socket the gather binds AND uses for its STUN
/// exchange is handed to the session alive (`GatheredSocket` →
/// `add_local_candidate_bound`), so the advertised host port and the srflx
/// mapping's base are that same socket — and it really carries the session's
/// checks in both directions.
///
/// Pre-fix shape this test rejects: the gather closed its socket before
/// returning; `ensure_locals` then bound a SECOND socket on a fresh ephemeral
/// port, so the signaled host port was never used by any STUN exchange and
/// `srflx.related_port` pointed at a dead mapping — the peer's checks to the
/// advertised srflx address landed on a closed socket.
#[test]
fn gathered_socket_stays_live_and_carries_the_session_on_the_advertised_port() {
    let mock = MockStun::spawn(MockMode::Echo);
    let mut provider = ProviderFixture::with_fds(1);
    let ifaces = StaticInterfaces(vec![iface("eth0", [127, 0, 0, 1])]);
    let servers = vec![netbird_core::ice::parse_stun_uri(&mock.uri()).expect("uri")];
    let cfg = GatherConfig { blacklist: &DEFAULT_INTERFACE_BLACKLIST, servers: &servers, timeout_ms: 1500 };

    let mut result = gather_candidates(&cfg, &ifaces, &provider.source).expect("gather ok");
    assert!(result.errors.is_empty(), "unexpected errors: {:?}", result.errors);
    assert_eq!(result.host.len(), 1);
    assert_eq!(result.srflx.len(), 1);
    assert_eq!(result.sockets.len(), 1, "the gather must hand its bound socket over");

    let host = result.host[0].clone();
    let srflx = result.srflx[0].clone();
    let sock = result.sockets.remove(0);
    assert_eq!(sock.cand, host, "the handed-over socket backs exactly this host candidate");
    assert_ne!(host.port, 0, "the host candidate carries the bound port");
    // The STUN exchange that produced the mapping rode THIS socket.
    assert_eq!(mock.seen_source_ports(), vec![host.port]);
    // The mapping's base is the host candidate — and therefore this socket.
    assert_eq!(srflx.related_address.as_deref(), Some(host.address.as_str()));
    assert_eq!(srflx.related_port, Some(host.port));

    // The fixture's ORIGINAL descriptor shares the socket object with the
    // gather's dup; close it so the port probe below observes EXACTLY the
    // gathered socket's ownership (pre-fix nothing was left holding the
    // object, so the advertised port was free again).
    let original = provider.fds.remove(0);
    unsafe { close(original) };

    // REGRESSION PROOF: right after the gather the advertised port is STILL
    // HELD by the gathered socket. Pre-fix the gather had closed it, so this
    // probe succeeded.
    assert!(
        !port_is_free([127, 0, 0, 1], host.port),
        "the gather must keep its bound socket alive (port {} must still be held)",
        host.port
    );

    // Session adopts the transferred socket — NO second protected socket.
    let local_creds = IceCredentials {
        ufrag: "LOCALufragXXXXXX".into(),
        pwd: "0123456789012345678901".into(),
    };
    let remote_creds = IceCredentials {
        ufrag: "REMOTEufragXXXXX".into(),
        pwd: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef".into(),
    };
    let mut session =
        IceSession::new(local_creds.clone(), true, Some(0x0102_0304_0506_0708)).expect("session");
    session.set_remote_credentials(remote_creds.clone()).expect("remote creds");
    let bound = session
        .add_local_candidate_bound(sock.cand.clone(), sock.fd)
        .expect("adopt the bound socket");
    assert_eq!(bound, host, "the session's local candidate IS the gathered candidate");
    assert_eq!(provider.source.taken(), 1, "adoption must not take a second socket");

    // Remote peer = a real loopback socket (the remote candidate).
    let peer = std::net::UdpSocket::bind("127.0.0.1:0").expect("peer bind");
    peer.set_read_timeout(Some(Duration::from_millis(500))).expect("read timeout");
    let peer_port = peer.local_addr().expect("peer addr").port();
    session.add_remote_candidate(Candidate::host_candidate([127, 0, 0, 1], peer_port));
    session.start().expect("start");

    // (1) SENDING: the check physically leaves from the advertised port.
    session.run_once(1_000).expect("pump");
    let mut buf = [0u8; 1500];
    let (n, src) = peer.recv_from(&mut buf).expect("the peer must receive our check");
    assert_eq!(src.port(), host.port, "checks must be sourced from the advertised host port");
    assert_eq!(src.port(), srflx.related_port.unwrap(), "…which is the srflx base port");
    let parsed = parse_stun(&buf[..n]).expect("check parses");
    assert_eq!(parsed.msg_type, BINDING_REQUEST);
    assert!(verify_message_integrity(&buf[..n], &parsed, &remote_creds.pwd));

    // (2) RECEIVING on the srflx MAPPED address/port: a valid Binding Request
    // to the mapping must reach the session on that same socket and be
    // answered. Pre-fix the mapping pointed at a closed socket — no listener,
    // no answer, ever.
    let txn = random_transaction_id().expect("entropy");
    let req = build_check_request(
        &CheckRequest {
            username: format!("{}:{}", local_creds.ufrag, remote_creds.ufrag),
            priority: 1,
            controlling: false,
            tie_breaker: 7,
            use_candidate: false,
            integrity_key: local_creds.pwd.clone(),
        },
        txn,
    );
    let mapped: SocketAddr = format!("{}:{}", srflx.address, srflx.port).parse().expect("mapped addr");
    peer.send_to(&req, mapped).expect("send to the mapped address");
    let mut answered = None;
    for i in 0..100 {
        session.run_once(2_000 + i as u64).expect("pump");
        match peer.recv_from(&mut buf) {
            Ok((n, src)) => {
                // Retransmitted checks may arrive first (RTO fired); only the
                // matching success response is the answer under test.
                if let Ok(p) = parse_stun(&buf[..n]) {
                    if p.msg_type == BINDING_SUCCESS && p.txn == txn {
                        answered = Some((n, src));
                        break;
                    }
                }
            }
            Err(_) => std::thread::sleep(Duration::from_millis(2)),
        }
    }
    let (n, _src) = answered.expect("the session must answer on the mapped srflx address");
    let resp = parse_stun(&buf[..n]).expect("response parses");
    assert_eq!(resp.msg_type, BINDING_SUCCESS);
    assert_eq!(resp.txn, txn);
    assert!(
        verify_message_integrity(&buf[..n], &resp, &local_creds.pwd),
        "the answer is keyed with our own password"
    );
    assert_eq!(session.health().inbound_reqs, 1);
    assert_eq!(session.health().inbound_ok, 1);

    // (3) OWNERSHIP CLEANUP: stop() closes exactly the adopted fd — the port
    // is released (and was still held a moment ago).
    assert!(!port_is_free([127, 0, 0, 1], host.port), "still owned before stop");
    session.stop();
    assert!(
        port_is_free([127, 0, 0, 1], host.port),
        "session stop() must close the adopted socket (port {} released)",
        host.port
    );
}

/// 0052 ownership on the FAILURE path: an interface round whose bind fails
/// closes the socket it took exactly once (no leak) and hands nothing over.
///
/// Deterministic and fd-number-free: the fed socket is ALREADY bound, so the
/// gather's re-bind fails with EINVAL; the test's own original descriptor is
/// closed first, which leaves the gather's copy as the ONLY descriptor of
/// that socket object — the port probe therefore observes exactly its
/// lifetime (a leak keeps the port held, one close releases it).
#[test]
fn failed_interface_round_closes_the_taken_socket() {
    use std::os::fd::IntoRawFd;

    let owned = std::net::UdpSocket::bind("127.0.0.1:0").expect("pre-bound socket");
    let port = owned.local_addr().expect("addr").port();
    let raw = owned.into_raw_fd();
    let source = ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source");
    source.feed(raw).expect("feed");
    unsafe { close(raw) }; // only the provider's dup remains

    let ifaces = StaticInterfaces(vec![iface("eth0", [127, 0, 0, 1])]);
    let cfg = GatherConfig { blacklist: &DEFAULT_INTERFACE_BLACKLIST, servers: &[], timeout_ms: 100 };

    let err = gather_candidates(&cfg, &ifaces, &source)
        .expect_err("re-binding an already-bound socket must fail closed");
    assert!(
        matches!(&err, ManagementError::Network(t) if t.contains("stun-bind-failed")),
        "wrong failure: {err:?}"
    );
    assert_eq!(source.taken(), 1, "exactly one socket attempted for one interface");
    assert_eq!(source.pending(), 0, "the taken socket is never recycled into the pool");
    assert!(
        port_is_free([127, 0, 0, 1], port),
        "the failed gather must close the socket it took (port {port} released; a leak holds it)"
    );
}
