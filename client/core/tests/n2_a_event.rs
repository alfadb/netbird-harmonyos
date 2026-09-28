// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N2 P4/P5 host integration (preregistration v4.1 §4「A 列生产事件的操作性
//! 定义」; no device, no network beyond loopback).
//!
//! What is proven here, in the frozen vocabulary:
//!
//! 1. **M reuses the EXISTING management connection.** The real production
//!    dial path (`mgmtsock::connect_protected` over a shell-fed
//!    `ProtectedSocketFdSource`) establishes the gRPC channel; the A event then
//!    runs ONE real `GetServerKey` through the connector's trigger seam. The
//!    server accepts exactly ONE connection across the whole test and the
//!    `N2_SOCK` fd/dup pair recorded at connect is byte-identical to the pair
//!    the `N2_A_EVENT|kind=M` line reports — no new dial, no second stream.
//! 2. **R arms one watch and completes `observed`/`timeout`** through the very
//!    function the relay reader calls when the production code answers a
//!    server-driven `HealthCheck`.
//! 3. **D follows Q1**: the first D event of an armed session dispatches the
//!    requery primitive once; any prior A event (or prior `n2_stimulus`)
//!    refuses with `prior-stimulus`.
//! 4. **S is refused by name** (`no-side-effect-free-production-send`), and an
//!    UNARMED session refuses before any trigger call / records nothing.
//! 5. **P5 ordering** against the REAL armed destination tee: the pre control's
//!    HIT (consumed by `N2DestinationTee::consume_control`) precedes the first
//!    production send, a first send before the attach anchor is the frozen
//!    `unsat|first-send-before-attach`, and unarmed records/emits nothing.
//!
//! Everything is diagnostic: every line ends
//! `|diag=true|is_evidence=false`, and nothing here is N2/N6 evidence.

use std::net::SocketAddr;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::net::TcpListener;
use tonic::{Request, Response, Status};

use netbird_core::envelope::EnvelopeKeyPair;
use netbird_core::grpc::proto::management_service_server::{
    ManagementService, ManagementServiceServer,
};
use netbird_core::grpc::proto::{Empty, ServerKeyResponse};
use netbird_core::grpc::{GrpcTransport, ManagementGrpcClient};
use netbird_core::mgmtsock::{mgmt_socket_open, ProtectedSocketFdSource};
use netbird_core::n2_a_event;
use netbird_core::n2_sandwich;
use netbird_core::n2_socket_trace;
use netbird_core::wg_device::N2DestinationTee;

// Host-process link stubs (repo convention, cf. tests/n2_stimulus.rs):
// libace_napi.z.so / libhilog_ndk.z.so do not exist on the host.
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
        _utf8name: *const c_void,
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

/// The arming flag, the traces and every N2 registry are process-global; the
/// harness runs these tests on parallel threads, so they serialize here (same
/// discipline as tests/n2_stimulus.rs).
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// An unconnected management socket opened through the REAL shell-facing entry.
fn open_fd() -> i32 {
    let open = mgmt_socket_open();
    let i = open.find("\"fd\":").expect("fd key") + "\"fd\":".len();
    let rest = &open[i..];
    let end = rest.find(',').unwrap_or_else(|| rest.find('}').unwrap_or(rest.len()));
    rest[..end].parse::<i32>().expect("fd number")
}

// ---------------------------------------------------------------------------
// the loopback management server: ONE method (GetServerKey), counted
// ---------------------------------------------------------------------------

#[derive(Default)]
struct KeyMock {
    calls: Arc<AtomicUsize>,
    key: String,
}

#[tonic::async_trait]
impl ManagementService for KeyMock {
    async fn get_server_key(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ServerKeyResponse>, Status> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        Ok(Response::new(ServerKeyResponse {
            key: self.key.clone(),
            ..Default::default()
        }))
    }
}

/// A listening stream that counts every accepted connection (the proof that
/// the M event did not dial again). The item type is the plain accepted
/// `TcpStream`, whose own `Connected` impl tonic uses.
struct CountingListener {
    inner: TcpListener,
    accepts: Arc<AtomicUsize>,
}

impl tokio_stream::Stream for CountingListener {
    type Item = std::io::Result<tokio::net::TcpStream>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match this.inner.poll_accept(cx) {
            std::task::Poll::Ready(Ok((stream, _))) => {
                this.accepts.fetch_add(1, Ordering::AcqRel);
                std::task::Poll::Ready(Some(Ok(stream)))
            }
            std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Some(Err(e))),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

async fn spawn_key_server() -> (SocketAddr, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let accepts = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let svc = KeyMock {
        calls: calls.clone(),
        key: EnvelopeKeyPair::generate().expect("keys").public_key_base64(),
    };
    let incoming = CountingListener { inner: listener, accepts: accepts.clone() };
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(ManagementServiceServer::new(svc))
            .serve_with_incoming(incoming)
            .await
            .expect("tonic serve");
    });
    (addr, accepts, calls)
}

/// A trigger seam whose three actions are recorded; `rpc` runs the REAL client
/// `GetServerKey` handed to it by the test.
struct TestTrigger {
    rpc: Mutex<Option<Box<dyn Fn() -> Result<(), String> + Send>>>,
    requery: Arc<AtomicUsize>,
    watches: Mutex<Vec<(String, u64, u64)>>,
}

impl n2_a_event::Trigger for TestTrigger {
    fn management_round_trip(&self) -> Result<(), String> {
        match self.rpc.lock().expect("rpc").as_ref() {
            Some(call) => call(),
            None => Err("no-rpc-wired".into()),
        }
    }

    fn dns_requery(&self) -> Result<(), String> {
        self.requery.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    fn arm_healthcheck_watch(&self, id: &str, wait_ms: u64, seq: u64) -> Result<(), String> {
        self.watches.lock().expect("watches").push((id.to_string(), wait_ms, seq));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// P4: M / R / D / S
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m_runs_one_real_rpc_on_the_existing_connection_only() {
    let _guard = serial();
    n2_socket_trace::arm(false);
    n2_a_event::reset();

    let (addr, accepts, calls) = spawn_key_server().await;

    // The shell's own order: open the fd, arm the trace, register the protect
    // receipt, then hand the fd to the connector. `connect_protected` dups it
    // and the SAME `first_io` call emits `N2_SOCK` and registers the live pair.
    let fd = open_fd();
    assert!(fd >= 0, "shell fd");
    n2_socket_trace::arm(true);
    n2_socket_trace::protect(fd, "M", "req-a-event", 0, 31, 4, true);
    let source = Arc::new(ProtectedSocketFdSource::new_with_fd(fd).expect("seed"));
    let keys = EnvelopeKeyPair::generate().expect("keys");
    let client = ManagementGrpcClient::connect_with_socket_source(
        &format!("http://{addr}"),
        GrpcTransport::Plaintext,
        Duration::from_secs(5),
        Duration::from_secs(10),
        keys,
        source,
        addr,
    )
    .await
    .expect("protected dial");

    let (dup_fd, original_fd, remote) =
        n2_socket_trace::live_of("M").expect("live M registration from the N2_SOCK call site");
    assert_eq!(original_fd, fd, "the reported original is the shell fd");
    assert_ne!(dup_fd, fd, "native dials its own dup, never the original");
    assert_eq!(accepts.load(Ordering::Acquire), 1, "one connection so far");
    assert_eq!(calls.load(Ordering::Acquire), 0, "no RPC before the trigger");

    // The trigger executes the REAL client on a plain thread (never inside a
    // runtime), exactly like the connector's `block_on` seam.
    let handle = tokio::runtime::Handle::current();
    let client = Arc::new(Mutex::new(client));
    let trigger = Arc::new(TestTrigger {
        rpc: Mutex::new(Some(Box::new({
            let client = client.clone();
            let handle = handle.clone();
            move || {
                handle
                    .block_on(async {
                        client.lock().expect("client").get_server_key().await
                    })
                    .map(|_| ())
                    .map_err(|e| format!("management-rpc-{e:?}"))
            }
        }))),
        requery: Arc::new(AtomicUsize::new(0)),
        watches: Mutex::new(Vec::new()),
    });
    n2_a_event::install_trigger(trigger.clone());

    let outcome = std::thread::spawn(|| {
        // Both calls run OFF the runtime, exactly like the shell/NAPI thread
        // the connector's `block_on` seam is reached from in production.
        let out = n2_a_event::apply("M", "a-M-1");
        let json = netbird_core::connector::n2_a_event_json("M", "a-M-2");
        (out, json)
    })
    .join()
    .expect("join");
    let (outcome, json) = outcome;
    assert_eq!(outcome.result, "sent", "{}", outcome.line);
    assert_eq!(outcome.reason, "ok");
    assert_eq!(outcome.fd, original_fd);
    assert_eq!(outcome.dup_fd, dup_fd);
    assert!(outcome.line.starts_with("N2_A_EVENT|id=a-M-1|kind=M|fd="), "{}", outcome.line);
    assert!(outcome.line.contains(&format!("|fd={original_fd}|dup_fd={dup_fd}|remote={remote}|")));
    assert!(outcome.line.ends_with("|diag=true|is_evidence=false"), "{}", outcome.line);

    // The RPC really happened, on the SAME connection: one accept and the
    // live registration did not move (a re-dial would replace it).
    assert_eq!(calls.load(Ordering::Acquire), 2, "exactly two GetServerKey calls");
    assert_eq!(accepts.load(Ordering::Acquire), 1, "NO new connection");
    assert_eq!(n2_socket_trace::live_of("M"), Some((dup_fd, original_fd, remote)),
        "the live M socket is the same fd/dup chain");

    // The JSON surface the shell consumes.
    assert!(json.contains("\"ok\":true") && json.contains("\"result\":\"sent\""), "{json}");

    // Dropping the stream registration (what `ProtectedTcpStream`'s Drop does
    // when the descriptor goes away) removes the live entry — never a stale
    // number.
    drop(trigger);
    drop(client);
    n2_socket_trace::closed(dup_fd);
    assert_eq!(n2_socket_trace::live_of("M"), None);
    netbird_core::connector::connector_stop_json();
    n2_socket_trace::arm(false);
    n2_a_event::reset();
}

#[test]
fn r_arms_the_documented_watch_and_completes_observed_or_timeout() {
    let _guard = serial();
    n2_socket_trace::arm(false);
    n2_a_event::reset();
    n2_socket_trace::arm(true);

    let requery = Arc::new(AtomicUsize::new(0));
    let trigger = Arc::new(TestTrigger {
        rpc: Mutex::new(None),
        requery: requery.clone(),
        watches: Mutex::new(Vec::new()),
    });
    n2_a_event::install_trigger(trigger.clone());

    // A live relay socket as the armed trace would have registered it.
    let sock = std::net::UdpSocket::bind("127.0.0.1:0").expect("udp");
    let fd = sock.as_raw_fd();
    n2_socket_trace::protect(fd, "R", "req-a-event", 0, 31, 4, true);
    n2_socket_trace::io_begin(fd, "192.0.2.9:28443".parse().expect("addr"));
    n2_socket_trace::first_io(fd, "192.0.2.9:28443".parse().expect("addr"), "connected", "connect");

    let armed = n2_a_event::apply("R", "a-R-1");
    assert_eq!(armed.result, "armed", "{}", armed.line);
    assert!(armed.line.contains("|detail=wait_ms=90000|"), "{}", armed.line);
    assert_eq!(n2_a_event::r_watch_open(), true);
    assert_eq!(trigger.watches.lock().expect("w").as_slice(),
        [("a-R-1".to_string(), n2_a_event::R_WAIT_MS, 1)]);

    // The production echo answered a SERVER-driven HealthCheck: the exact
    // completion the relay reader calls.
    let observed = n2_a_event::healthcheck_answered().expect("watch armed");
    assert_eq!(observed.result, "observed");
    assert_eq!(observed.reason, "server-healthcheck-answered");
    assert_eq!(observed.fd, fd);
    assert_eq!(n2_a_event::last_result(), Some("observed"));
    assert!(n2_a_event::healthcheck_answered().is_none(), "one observation per watch");
    assert!(!n2_a_event::r_watch_open());

    // The timeout completion names the frozen branch; a stale (id, seq) is a
    // no-op (a timer armed for an earlier watch can never complete a later one).
    assert_eq!(n2_a_event::apply("R", "a-R-2").result, "armed");
    let seq = trigger.watches.lock().expect("w").last().expect("watch").2;
    assert!(n2_a_event::r_watch_timeout("a-R-other", seq).is_none());
    assert!(n2_a_event::r_watch_timeout("a-R-2", seq + 99).is_none(), "stale timer");
    let timed_out = n2_a_event::r_watch_timeout("a-R-2", seq).expect("armed");
    assert_eq!(timed_out.result, "timeout");
    assert_eq!(timed_out.reason, "healthcheck-wait-exceeded");
    assert_eq!(n2_a_event::last_result(), Some("timeout"));

    n2_socket_trace::arm(false);
    n2_a_event::reset();
}

#[test]
fn d_q1_and_s_refusals_through_the_json_surface() {
    let _guard = serial();
    n2_socket_trace::arm(false);
    n2_a_event::reset();
    n2_socket_trace::arm(true);

    let requery = Arc::new(AtomicUsize::new(0));
    n2_a_event::install_trigger(Arc::new(TestTrigger {
        rpc: Mutex::new(None),
        requery: requery.clone(),
        watches: Mutex::new(Vec::new()),
    }));

    // S: no side-effect-free production send exists -> always refused by name.
    let s = netbird_core::connector::n2_a_event_json("S", "a-S-1");
    assert!(s.contains("\"result\":\"rejected\""), "{s}");
    assert!(s.contains("\"reason\":\"no-side-effect-free-production-send\""), "{s}");

    // D: the first A event of the session dispatches the requery once.
    let d1 = netbird_core::connector::n2_a_event_json("D", "a-D-1");
    assert!(d1.contains("\"result\":\"requery\"") && d1.contains("\"ok\":true"), "{d1}");
    assert_eq!(requery.load(Ordering::Acquire), 1);
    // …and every later D is refused with the Q1 token.
    let d2 = netbird_core::connector::n2_a_event_json("D", "a-D-2");
    assert!(d2.contains("\"result\":\"rejected\"") && d2.contains("\"reason\":\"prior-stimulus\""),
        "{d2}");
    assert_eq!(requery.load(Ordering::Acquire), 1, "no second requery");
    let d3 = n2_a_event::apply("D", "a-D-3");
    assert!(d3.line.contains("|detail=prior-a-event|"), "{}", d3.line);

    // UNARMED: refused before any seam call, and the trigger is dropped.
    n2_socket_trace::arm(false);
    let unarmed = netbird_core::connector::n2_a_event_json("D", "a-D-4");
    assert!(unarmed.contains("\"reason\":\"not-armed\""), "{unarmed}");
    assert_eq!(requery.load(Ordering::Acquire), 1);
    assert!(!n2_a_event::trigger_installed(), "a disarm leaves no seam behind");
    n2_a_event::reset();
}

// ---------------------------------------------------------------------------
// P5: the I/Ws sandwich against the REAL armed tee
// ---------------------------------------------------------------------------

/// One IPv4/UDP frame carrying the control marker payload (the shape
/// `N2DestinationTee::consume_control` scans).
fn control_frame(dst: [u8; 4], sport: u16, dport: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 20 + 8 + payload.len()];
    out[0] = 0x45;
    let len = out.len() as u16;
    out[2..4].copy_from_slice(&len.to_be_bytes());
    out[9] = 17; // UDP
    out[12..16].copy_from_slice(&[10, 0, 0, 1]);
    out[16..20].copy_from_slice(&dst);
    out[20..22].copy_from_slice(&sport.to_be_bytes());
    out[22..24].copy_from_slice(&dport.to_be_bytes());
    out[24..26].copy_from_slice(&((8 + payload.len()) as u16).to_be_bytes());
    out[28..].copy_from_slice(payload);
    out
}

#[test]
fn sandwich_pre_hit_precedes_the_first_send_and_unarmed_is_silent() {
    let _guard = serial();
    n2_socket_trace::arm(false);
    n2_sandwich::reset();

    // UNARMED: every entry point is a silent no-op.
    n2_sandwich::attach_anchor(1_000);
    assert_eq!(n2_sandwich::pre("I", "n2s-I-pre").line, "");
    n2_sandwich::observe_io("I", 7, 6, "192.0.2.9:3478".parse().expect("addr"), "sent");
    assert!(n2_sandwich::poll(netbird_core::sys::mono_ms()).is_empty());
    assert_eq!(n2_sandwich::attach_ms(), None);
    assert_eq!(n2_sandwich::first_send_ms("I"), None);
    assert!(n2_sandwich::last_line().is_none(), "zero output while unarmed");

    n2_socket_trace::arm(true);
    let dst = [192, 0, 2, 9];
    let tee = N2DestinationTee::new(vec![dst], netbird_core::sys::mono_ms());

    // NO attach anchor yet: the frozen pre gate refuses by name.
    let early = n2_sandwich::pre("I", "n2s-I-pre");
    assert_eq!(early.result, "unsat");
    assert_eq!(early.reason, "no-attach-anchor");

    // The tee attach anchor (wg_device's try_build) is what legalises the pre.
    n2_sandwich::attach_anchor(netbird_core::sys::mono_ms());
    let pre = n2_sandwich::pre("I", "n2s-I-pre");
    assert_eq!(pre.result, "registered", "{}", pre.line);
    assert!(pre.line.contains("|phase=pre|marker=n2s-I-pre|event_ref=attach@"));
    assert!(pre.line.contains("|wait_ms=3000|"), "{}", pre.line);
    assert!(!n2_sandwich::leg_pre_hit("I"), "no HIT yet");

    // The REAL armed tee consumes the pre marker — that consumption is the
    // only thing that marks the pre HIT.
    let now = netbird_core::sys::mono_ms();
    tee.register_control("n2s-I-pre", dst, 3478, 17, 0, now).expect("register");
    let frame = control_frame(dst, 4444, 3478, b"N2CTRL:n2s-I-pre");
    assert!(tee.consume_control(&frame, netbird_core::sys::mono_ms()), "pre HIT");
    assert!(n2_sandwich::leg_pre_hit("I"), "the tee HIT marked the leg");

    // The first REAL I send (its `first_io` completion) comes after the HIT.
    n2_sandwich::observe_io("I", 7, 6, "192.0.2.9:3478".parse().expect("addr"), "sent");
    assert!(n2_sandwich::first_send_ms("I").is_some());

    // The shell is offered the post control exactly once…
    let posts = n2_sandwich::poll(netbird_core::sys::mono_ms());
    assert_eq!(posts.len(), 1, "{posts:?}");
    assert_eq!(posts[0].kind, "I");
    assert_eq!(posts[0].marker, "n2s-I-pre");
    assert!(n2_sandwich::poll(netbird_core::sys::mono_ms()).is_empty(), "offered once");
    // …and the JSON surface carries it.
    let poll_json = netbird_core::connector::n2_sandwich_poll_json();
    assert!(poll_json.contains("\"posts\":[]"), "no second offer: {poll_json}");
    let pre_json = netbird_core::connector::n2_sandwich_pre_json("Ws", "n2s-Ws-pre2");
    assert!(pre_json.contains("\"result\":\"registered\""), "{pre_json}");

    let post = n2_sandwich::post("I", "n2s-I-pre", "observed", "ok");
    assert_eq!(post.result, "observed", "{}", post.line);
    assert!(post.line.contains("|phase=post|marker=n2s-I-pre|event_ref=first-send@"));
    assert!(post.line.contains("|result=observed|reason=control-hit-after-first-send|"));
    assert!(post.line.contains("|fd=6|dup_fd=7|remote=192.0.2.9:3478|"), "{}", post.line);

    // A first Ws send BEFORE the attach anchor: the frozen unsat token.
    n2_socket_trace::arm(false);
    n2_socket_trace::arm(true);
    n2_sandwich::observe_io("Ws", 9, 8, "192.0.2.9:51820".parse().expect("addr"), "sent");
    let unsat = n2_sandwich::last_line().expect("unsat line");
    assert!(unsat.contains("|kind=Ws|phase=first-send|marker=none|"), "{unsat}");
    assert!(unsat.contains("|result=unsat|reason=first-send-before-attach|"), "{unsat}");
    assert!(unsat.contains("|fd=8|dup_fd=9|"), "{unsat}");

    n2_socket_trace::arm(false);
    n2_sandwich::reset();
}

#[test]
fn first_send_after_attach_but_before_the_pre_control_is_unsat() {
    let _guard = serial();
    n2_socket_trace::arm(false);
    n2_sandwich::reset();
    n2_socket_trace::arm(true);
    n2_sandwich::attach_anchor(netbird_core::sys::mono_ms());
    // The production send happened first…
    n2_sandwich::observe_io("I", 7, 6, "192.0.2.9:3478".parse().expect("addr"), "sent");
    // …so the leg can never be a valid sandwich.
    let pre = n2_sandwich::pre("I", "n2s-I-late");
    assert_eq!(pre.result, "unsat", "{}", pre.line);
    assert_eq!(pre.reason, "first-send-before-pre-control");
    assert_eq!(n2_sandwich::leg_count(), 0, "no leg survives an unsat pre");
    n2_socket_trace::arm(false);
    n2_sandwich::reset();
}

#[test]
fn a_live_relay_socket_reports_its_own_fd_pair_after_the_shell_close() {
    let _guard = serial();
    n2_socket_trace::arm(false);
    n2_a_event::reset();
    n2_socket_trace::arm(true);

    // The shell closes its original after feeding; the core-owned dup keeps
    // the live registration, and the A line reports the same pair N2_SOCK did.
    let std_sock = std::net::UdpSocket::bind("127.0.0.1:0").expect("udp");
    let fd = std_sock.into_raw_fd();
    n2_socket_trace::protect(fd, "R", "req-a-event", 0, 31, 4, true);
    let dup = netbird_core::mgmtsock::dup_socket_fd(fd).expect("dup");
    n2_socket_trace::first_io(dup, "192.0.2.9:28443".parse().expect("addr"), "connected", "connect");
    let owned = unsafe { std::os::fd::OwnedFd::from_raw_fd(dup) };
    n2_socket_trace::closed(fd);
    assert_eq!(n2_socket_trace::live_of("R"), Some((dup, fd, "192.0.2.9:28443".parse().unwrap())));
    n2_a_event::install_trigger(Arc::new(TestTrigger {
        rpc: Mutex::new(None),
        requery: Arc::new(AtomicUsize::new(0)),
        watches: Mutex::new(Vec::new()),
    }));
    let armed = n2_a_event::apply("R", "a-R-9");
    assert_eq!(armed.fd, fd, "shell original");
    assert_eq!(armed.dup_fd, dup, "core-owned dup");
    assert!(n2_a_event::healthcheck_answered().is_some());
    // Closing the core dup removes the live entry: a reused number can never
    // be reported as this socket.
    n2_socket_trace::closed(dup);
    assert_eq!(n2_socket_trace::live_of("R"), None);
    drop(owned);
    n2_socket_trace::arm(false);
    n2_a_event::reset();
}
