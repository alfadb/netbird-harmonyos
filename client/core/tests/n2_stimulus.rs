// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N2 T5a「B 断流刺激」host integration (preregistration v4 §4 B column /
//! §7 T5a; no device, no server, loopback only).
//!
//! What is proven here, in the frozen vocabulary:
//!
//! 1. **Each class only touches its own target.** M/S/R run the REAL
//!    production dial path (`mgmtsock::connect_protected` over a
//!    `ProtectedSocketFdSource` fed exactly like the shell feeds it): the
//!    stimulus `shutdown`s the class's current outer socket, the PEER observes
//!    the close (bounded read → `Ok(0)`), and live sockets of the other two
//!    classes survive untouched.
//! 2. **The re-dial produces a new registration.** The registry's counter is
//!    bumped inside the SAME `n2_socket_trace::first_io` call that emits the
//!    `N2_SOCK` receipt, so "a new registration appeared" and "a new
//!    `N2_SOCK` receipt appeared" are one observation; every repetition
//!    increments it exactly once.
//! 3. **Three reproducible repetitions per class** (the frozen minimum
//!    observation count), each with its own fresh socket.
//! 4. **Unarmed is refused** (`result=rejected|reason=not-armed`) and a class
//!    with no registered socket answers `result=no-target`.
//! 5. **I/Wb/Ws/D go through the connector sink** (endpoint-directed break /
//!    forced fresh protected DNS query) — the classes for which no
//!    socket-level syscall exists; `0` from the sink is `no-target`, never a
//!    fake `applied`.
//!
//! Everything is diagnostic: every emitted line ends
//! `|diag=true|is_evidence=false`, and nothing here is N2/N6 evidence.

use std::collections::HashMap;
use std::io::Read;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::fd::AsRawFd;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

// Host-process link stubs (repo convention, cf. tests/n2h_device_tee.rs):
// libace_napi.z.so / libhilog_ndk.z.so do not exist on the host. No-ops for
// the linker only; never called by these tests.
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
        _data: *mut c_void,
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

use netbird_core::mgmtsock::{connect_protected, ProtectedSocketFdSource};
use netbird_core::{n2_socket_trace, n2_stimulus, sys};

/// The stimulus registry and the arming flag are process-wide, so the tests in
/// this binary serialize themselves instead of racing each other.
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// One loopback listener per outer class: every dial is accepted and handed to
/// the test, so "the socket really broke" is observed at the PEER (a FIN/RST),
/// not inferred from a return value.
struct ClassServer {
    addr: SocketAddr,
    accepted: Receiver<TcpStream>,
}

impl ClassServer {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let (sender, accepted) = channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        if sender.send(stream).is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        });
        Self { addr, accepted }
    }

    fn next(&self) -> TcpStream {
        self.accepted.recv_timeout(Duration::from_secs(5)).expect("dial was accepted")
    }
}

/// Bounded read: `true` = the peer's close arrived, `false` = the connection
/// was still open when the budget ran out (an unexpected error is also false —
/// never a pass).
fn expect_close(stream: &mut TcpStream, wait: Duration) -> bool {
    let previous = stream.read_timeout().ok().flatten();
    stream.set_read_timeout(Some(wait)).expect("read timeout");
    let mut buf = [0u8; 32];
    let closed = loop {
        match stream.read(&mut buf) {
            Ok(0) => break true,
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break false,
        }
    };
    stream.set_read_timeout(previous).expect("restore read timeout");
    closed
}

/// The shell's half of one outer socket: open + bound, NEVER connected —
/// exactly what `mgmt_socket_open` hands to the platform protect gate.
fn shell_tcp_fd() -> i32 {
    let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_STREAM | sys::SOCK_CLOEXEC, 0) };
    assert!(fd >= 0, "socket");
    let local = sys::sockaddr_in::new([127, 0, 0, 1], 0);
    let len = std::mem::size_of::<sys::sockaddr_in>() as u32;
    assert_eq!(unsafe { sys::bind(fd, &local, len) }, 0, "bind");
    fd
}

/// The DNS/ICE/WG classes ride UDP sockets; the test annotates one exactly the
/// way `protected_dns` / `ice.rs` / `wg_device.rs` complete a real send, so the
/// registration path under test is the shipped one.
fn shell_udp_fd() -> i32 {
    let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM | sys::SOCK_CLOEXEC, 0) };
    assert!(fd >= 0, "socket");
    let local = sys::sockaddr_in::new([127, 0, 0, 1], 0);
    let len = std::mem::size_of::<sys::sockaddr_in>() as u32;
    assert_eq!(unsafe { sys::bind(fd, &local, len) }, 0, "bind");
    fd
}

/// One completed send of an outer socket of `kind` (the `N2_SOCK` receipt's own
/// call pair), then the shell's close of its original.
fn observe_send(kind: &str, fd: i32, remote: &str) {
    let remote: SocketAddr = remote.parse().expect("remote");
    n2_socket_trace::protect(fd, kind, "req-test", 0, 31, 1, true);
    n2_socket_trace::io_begin(fd, remote);
    n2_socket_trace::first_io(fd, remote, "sent", "test_send");
}

/// One protected dial of `kind` through the production seam: annotate the
/// shell fd, feed it, connect, then close the shell's original (ownership has
/// passed to the source's duplicate).
async fn protected_dial(kind: &str, server: &ClassServer, rep: usize) -> tokio::net::TcpStream {
    let fd = shell_tcp_fd();
    n2_socket_trace::protect(fd, kind, &format!("req-{kind}-{rep}"), rep as u64, 31, 1, true);
    let source = ProtectedSocketFdSource::new_with_fd(fd).expect("source");
    let stream = connect_protected(&source, server.addr).await.expect("dial");
    n2_socket_trace::closed(fd);
    unsafe { sys::close(fd) };
    stream
}

fn assert_line(out: &n2_stimulus::Outcome, kind: &str, id: &str, result: &str) {
    assert!(out.line.starts_with("N2_STIMULUS|"), "{}", out.line);
    for field in [
        format!("|id={id}|"),
        format!("|kind={kind}|"),
        format!("|result={result}|"),
        "|mono_ms=".to_string(),
        "|diag=true|is_evidence=false".to_string(),
    ] {
        assert!(out.line.contains(&field), "{field} missing in {}", out.line);
    }
    assert!(out.line.contains("|target_fd="), "{}", out.line);
    assert!(out.line.contains("|remote="), "{}", out.line);
    assert!(out.line.ends_with("diag=true|is_evidence=false"), "{}", out.line);
}

fn target_fd(kind: &str) -> Option<i32> {
    n2_stimulus::target(kind).map(|(fd, _)| fd)
}

/// ① + ② + ③: M, S and R each break ONLY their own class's current outer
/// socket, the peer observes the close, and every re-dial registers a new
/// outer socket — three reproducible repetitions per class, with the other two
/// classes held live throughout.
#[tokio::test(flavor = "multi_thread")]
async fn tcp_classes_isolate_and_repeat_three_times() {
    let _guard = serial();
    n2_socket_trace::arm(true);

    let servers: HashMap<&str, ClassServer> =
        [("M", ClassServer::spawn()), ("S", ClassServer::spawn()), ("R", ClassServer::spawn())]
            .into_iter()
            .collect();

    for kind in ["M", "S", "R"] {
        // The other two classes: one live socket each, dialed once here and
        // never stimulated — they are the isolation witnesses.
        let mut witnesses: Vec<(&str, tokio::net::TcpStream, TcpStream)> = Vec::new();
        for other in ["M", "S", "R"] {
            if other == kind {
                continue;
            }
            let client = protected_dial(other, &servers[other], 0).await;
            let peer = servers[other].next();
            witnesses.push((other, client, peer));
        }

        for rep in 0..3usize {
            let observed_before = n2_stimulus::observed(kind);
            let stream = protected_dial(kind, &servers[kind], rep).await;
            let mut peer = servers[kind].next();
            let fd = stream.as_raw_fd();
            let remote = stream.peer_addr().expect("peer addr");

            // ② the fresh dial registered exactly one new outer socket of this
            // class — inside the very first_io that emits its N2_SOCK receipt.
            assert_eq!(n2_stimulus::observed(kind), observed_before + 1, "{kind} rep {rep}");
            assert_eq!(n2_stimulus::target(kind), Some((fd, remote)), "{kind} rep {rep}");

            // ... and the witnesses of the other classes are untouched.
            for (other, client, _) in &witnesses {
                assert_eq!(
                    target_fd(other),
                    Some(client.as_raw_fd()),
                    "{other} must survive a {kind} stimulus"
                );
            }

            let id = format!("b-{kind}-{rep}");
            let out = n2_stimulus::apply(kind, &id);
            assert_eq!(out.result, "applied", "{kind} rep {rep}: {}", out.reason);
            assert_eq!(out.reason, "shutdown-socket");
            assert_eq!(out.target_fd, fd);
            assert_line(&out, kind, &id, "applied");

            // ① the PEER observes the break (FIN) on the stimulated class...
            assert!(expect_close(&mut peer, Duration::from_secs(5)), "{kind} rep {rep} peer close");
            // ... the registration is consumed (one stimulus = one socket)...
            assert_eq!(target_fd(kind), None, "{kind} rep {rep}");
            let again = n2_stimulus::apply(kind, &format!("{id}-again"));
            assert_eq!(again.result, "no-target");
            assert_eq!(again.reason, "no-outer-socket");
            assert_line(&again, kind, &format!("{id}-again"), "no-target");

            // ... while the other classes' sockets are still open and
            // registered, never re-dialed by this stimulus.
            for (other, client, _) in &witnesses {
                assert_eq!(target_fd(other), Some(client.as_raw_fd()), "{other}");
            }
        }

        // The witnesses' own connections were never closed by any stimulus.
        for (other, _, peer) in witnesses.iter_mut() {
            assert!(
                !expect_close(peer, Duration::from_millis(200)),
                "{other} witness was closed by a {kind} stimulus"
            );
        }
    }

    n2_socket_trace::arm(false);
}

/// ⑤a: the classes without a socket-level syscall delegate to the connector
/// sink — three reproducible repetitions, `applied` only when the sink really
/// acted, and the TCP classes' registrations are never touched by them.
#[tokio::test(flavor = "multi_thread")]
async fn endpoint_classes_delegate_to_the_sink_and_repeat_three_times() {
    let _guard = serial();
    n2_socket_trace::arm(true);

    let calls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    struct Recording(Arc<Mutex<Vec<String>>>);
    impl n2_stimulus::Sink for Recording {
        fn act(&self, kind: &str) -> usize {
            self.0.lock().expect("calls").push(kind.to_string());
            1
        }
    }
    n2_stimulus::install_sink(Arc::new(Recording(calls.clone())));

    // Live outer sockets of the three sink classes (UDP: annotated at a real
    // completed send exactly like the shipped call sites).
    let mut observed: Vec<(&str, i32)> = Vec::new();
    for (kind, remote) in
        [("I", "198.51.100.7:3478"), ("Wb", "198.51.100.9:51820"), ("Ws", "198.51.100.9:51820")]
    {
        let fd = shell_udp_fd();
        observe_send(kind, fd, remote);
        observed.push((kind, fd));
    }
    // A management socket in the same session: it must stay registered.
    let mgmt = ClassServer::spawn();
    let mgmt_stream = protected_dial("M", &mgmt, 0).await;
    let _mgmt_peer = mgmt.next();
    let mgmt_fd = mgmt_stream.as_raw_fd();

    for rep in 0..3usize {
        for (kind, fd) in &observed {
            let registrations = n2_stimulus::observed(kind);
            let id = format!("b-{kind}-{rep}");
            let out = n2_stimulus::apply(kind, &id);
            assert_eq!(out.result, "applied", "{kind} rep {rep}: {}", out.reason);
            assert_eq!(out.reason, "endpoint-drop");
            assert_eq!(out.target_fd, *fd, "{kind}");
            assert_line(&out, kind, &id, "applied");
            // The sink is the ONLY thing that ran: no new registration, no
            // consumed target (dropping an endpoint is the sink's decision).
            assert_eq!(n2_stimulus::observed(kind), registrations, "{kind}");
            assert_eq!(target_fd(kind), Some(*fd), "{kind}");
            assert_eq!(target_fd("M"), Some(mgmt_fd), "an M socket must survive a {kind} stimulus");
        }
    }
    {
        let calls = calls.lock().expect("calls");
        assert_eq!(calls.len(), 9, "{calls:?}");
        for (idx, kind) in calls.iter().enumerate() {
            let expected = ["I", "Wb", "Ws"][idx % 3];
            assert_eq!(kind, expected);
        }
    }

    // A sink that finds nothing is `no-target`; a missing sink is its own
    // honest token — never a fabricated `applied`.
    struct Nothing;
    impl n2_stimulus::Sink for Nothing {
        fn act(&self, _kind: &str) -> usize {
            0
        }
    }
    n2_stimulus::install_sink(Arc::new(Nothing));
    let out = n2_stimulus::apply("Wb", "b-nothing");
    assert_eq!(out.result, "no-target");
    assert_eq!(out.reason, "no-live-endpoint");
    assert_line(&out, "Wb", "b-nothing", "no-target");
    n2_stimulus::clear_sink();
    let out = n2_stimulus::apply("I", "b-nosink");
    assert_eq!(out.result, "no-target");
    assert_eq!(out.reason, "no-sink");
    assert_line(&out, "I", "b-nosink", "no-target");

    for (_, fd) in observed {
        n2_socket_trace::closed(fd);
        unsafe { sys::close(fd) };
    }
    n2_socket_trace::arm(false);
}

/// ⑤b: D forces a fresh protected query through the sink, and its registration
/// comes from the same completed-send pair the shipped `protected_dns` uses.
#[tokio::test(flavor = "multi_thread")]
async fn dns_class_forces_a_fresh_query_three_times() {
    let _guard = serial();
    n2_socket_trace::arm(true);

    struct DnsSink;
    impl n2_stimulus::Sink for DnsSink {
        fn act(&self, kind: &str) -> usize {
            usize::from(kind == "D")
        }
    }
    n2_stimulus::install_sink(Arc::new(DnsSink));

    let fd = shell_udp_fd();
    observe_send("D", fd, "192.0.2.53:53");

    for rep in 0..3usize {
        let id = format!("b-D-{rep}");
        let out = n2_stimulus::apply("D", &id);
        assert_eq!(out.result, "applied", "rep {rep}: {}", out.reason);
        assert_eq!(out.reason, "dns-requery");
        assert_eq!(out.target_fd, fd);
        assert!(out.remote.starts_with("192.0.2.53:"), "{}", out.remote);
        assert_line(&out, "D", &id, "applied");
    }
    // The class stays registered (the forced query re-registers on its own
    // fresh fd): the repetitions are driven by the sink, not by a consumed fd.
    assert_eq!(target_fd("D"), Some(fd));

    n2_socket_trace::closed(fd);
    unsafe { sys::close(fd) };
    n2_socket_trace::arm(false);
}

/// ④: unarmed is refused (and still logged); armed with no registration is
/// `no-target`; a disarm clears the registry.
#[tokio::test(flavor = "multi_thread")]
async fn unarmed_is_rejected_and_empty_registry_is_no_target() {
    let _guard = serial();
    n2_socket_trace::arm(false);

    let fd = shell_udp_fd();
    observe_send("I", fd, "198.51.100.7:3478");
    assert_eq!(target_fd("I"), None, "an unarmed observation records nothing");
    for kind in n2_stimulus::KINDS {
        let out = n2_stimulus::apply(kind, "b-unarmed");
        assert_eq!(out.result, "rejected", "{kind}");
        assert_eq!(out.reason, "not-armed", "{kind}");
        assert_eq!(out.target_fd, -1);
        assert_line(&out, kind, "b-unarmed", "rejected");
    }

    n2_socket_trace::arm(true);
    let recorded = shell_udp_fd();
    observe_send("I", recorded, "198.51.100.7:3478");
    assert!(target_fd("I").is_some());
    // A second arming change clears it: nothing survives a disarm/rearm.
    n2_socket_trace::arm(false);
    assert_eq!(target_fd("I"), None);
    n2_socket_trace::arm(true);
    for kind in n2_stimulus::KINDS {
        let out = n2_stimulus::apply(kind, "b-empty");
        assert_eq!(out.result, "no-target", "{kind}");
        assert_eq!(out.reason, "no-outer-socket", "{kind}");
        assert_line(&out, kind, "b-empty", "no-target");
    }
    // Unknown kinds and malformed ids are refused, not silently ignored.
    assert_eq!(n2_stimulus::apply("W", "b-x").reason, "invalid-kind");
    assert_eq!(n2_stimulus::apply("M", "not an id").reason, "invalid-id");

    for fd in [fd, recorded] {
        n2_socket_trace::closed(fd);
        unsafe { sys::close(fd) };
    }
    n2_socket_trace::arm(false);
}
