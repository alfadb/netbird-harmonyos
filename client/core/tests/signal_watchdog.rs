// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N2 P8 host integration (no device, no network beyond loopback).
//!
//! The receive watchdog's own silence/probe/echo/dead behaviour is asserted in
//! `tests/signal_session.rs` (the session loop). This binary covers the two
//! facts that need the PROCESS-GLOBAL probe handle — exactly one live signal
//! stream is registered per process (one connector session), so a second
//! session in the same binary could steal a request:
//!
//! 1. **The product-internal outside request**
//!    (`signal::request_receive_probe`) sends ONE self-addressed HEARTBEAT
//!    immediately — long before the watchdog's own silence window — on the
//!    SAME connection; with no registered stream it refuses and sends nothing.
//!    N2 T7a item 5: the N2 path no longer calls this seam.
//! 2. **The frozen v4.1 §4 `S` A-event** (`n2_a_event::apply("S", …)`) arms a
//!    watcher for the NEXT **natural** watchdog HEARTBEAT (the frozen S row
//!    (i) shape, N2 T7a item 5) through the connector's trigger seam
//!    (`Trigger::arm_s_watch`) and reports the SAME `fd`/`dup_fd` pair the
//!    `N2_SOCK` receipts report. The frame that actually leaves is the
//!    production self-addressed HEARTBEAT (`Body.type = 6`, server-side
//!    decrypted) on the EXISTING connection — no new dial, no other frame
//!    kind, no peer-directed message — and the trigger itself sends NOTHING.
//!
//! Everything is diagnostic: every line ends `|diag=true|is_evidence=false`,
//! and nothing here is N2/N6 evidence.

mod signal_mock;

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use netbird_core::envelope::EnvelopeKeyPair;
use netbird_core::grpc::GrpcTransport;
use netbird_core::mgmtsock::{mgmt_socket_open, ProtectedSocketFdSource};
use netbird_core::n2_a_event;
use netbird_core::n2_socket_trace;
use netbird_core::signal::{SignalLoopEvent, SignalSession};

use signal_mock::{spawn_signal_mock, SinkPeer, SignalMock};

/// The probe handle is process-global: serialize the two tests of this binary
/// so a request can only ever be answered by the stream the test registered.
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// An unconnected TCP socket opened through the REAL shell-facing entry.
fn open_fd() -> i32 {
    let open = mgmt_socket_open();
    let i = open.find("\"fd\":").expect("fd key") + "\"fd\":".len();
    let rest = &open[i..];
    let end = rest.find(',').unwrap_or_else(|| rest.find('}').unwrap_or(rest.len()));
    rest[..end].parse::<i32>().expect("fd number")
}

/// The connector-side `S` seam in miniature: the frozen production shape is
/// `ConnectorATrigger::arm_s_watch` (spawn the `S_WAIT_MS` timeout on the
/// session runtime; NO frame is sent), reproduced here so the A-event path
/// under test is the real one.
struct SWatchTrigger {
    runtime: tokio::runtime::Handle,
    calls: Arc<std::sync::atomic::AtomicUsize>,
    fail: Arc<AtomicBool>,
}

impl n2_a_event::Trigger for SWatchTrigger {
    fn management_round_trip(&self) -> Result<(), String> {
        Err("not-wired".into())
    }

    fn dns_requery(&self) -> Result<(), String> {
        Err("not-wired".into())
    }

    fn arm_s_watch(&self, id: &str, wait_ms: u64, seq: u64) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        if self.fail.load(Ordering::Acquire) {
            return Err("signal-heartbeat-network".into());
        }
        let id = id.to_string();
        self.runtime.spawn(async move {
            tokio::time::sleep(Duration::from_millis(wait_ms)).await;
            let _ = n2_a_event::s_watch_timeout(&id, seq);
        });
        Ok(())
    }

    fn arm_healthcheck_watch(&self, _id: &str, _wait_ms: u64, _seq: u64) -> Result<(), String> {
        Err("not-wired".into())
    }
}

/// ONE real signal registration over the shell's pre-protected fd, running the
/// production receive loop with the GIVEN watchdog window. The default window
/// used by the probe test is far longer than the test, so ONLY an outside
/// request can produce a heartbeat; the `S` event test shrinks it so the
/// NATURAL silence-triggered probe really fires.
async fn registered_watchdog_with_window(
    addr: SocketAddr,
    keys: EnvelopeKeyPair,
    source: Arc<ProtectedSocketFdSource>,
    probe_after: Duration,
    dead_after: Duration,
) -> tokio::task::JoinHandle<()> {
    let client = netbird_core::signal::SignalClient::connect_with_socket_source(
        &format!("http://{addr}"),
        GrpcTransport::Plaintext,
        Duration::from_secs(5),
        Duration::from_secs(5),
        keys,
        source,
        addr,
    )
    .await
    .expect("protected signal dial");
    let mut session = SignalSession::new(client)
        .with_receive_watchdog(probe_after, dead_after);
    let registered = Arc::new(AtomicBool::new(false));
    let flag = registered.clone();
    let handle = tokio::spawn(async move {
        let _ = session
            .run_events(move |event| {
                if matches!(event, SignalLoopEvent::Registered) {
                    flag.store(true, Ordering::Release);
                }
            })
            .await;
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !registered.load(Ordering::Acquire) {
        assert!(std::time::Instant::now() <= deadline, "signal session never registered");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    handle
}

async fn registered_watchdog(
    addr: SocketAddr,
    keys: EnvelopeKeyPair,
    source: Arc<ProtectedSocketFdSource>,
) -> tokio::task::JoinHandle<()> {
    registered_watchdog_with_window(addr, keys, source, Duration::from_secs(600),
        Duration::from_secs(900)).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_external_probe_request_sends_one_heartbeat_immediately() {
    let _guard = serial();

    // No registered stream at all: nothing may be sent, and the caller is told
    // so by name.
    assert_eq!(
        netbird_core::signal::request_receive_probe().await,
        Err("no-registered-signal-stream".to_string())
    );
    assert!(!netbird_core::signal::receive_probe_registered());

    let mock = SignalMock::default();
    let (addr, counter) = spawn_signal_mock(mock.clone()).await;
    let keys = EnvelopeKeyPair::generate().expect("keys");
    let fd = open_fd();
    assert!(fd >= 0, "shell fd");
    let source = Arc::new(ProtectedSocketFdSource::new_with_fd(fd).expect("seed"));
    let handle = registered_watchdog(addr, keys, source).await;
    assert!(netbird_core::signal::receive_probe_registered(), "a registered stream publishes it");

    let started = std::time::Instant::now();
    netbird_core::signal::request_receive_probe().await.expect("one heartbeat");
    assert!(
        started.elapsed() < Duration::from_millis(2_000),
        "the request is answered as soon as the send returned"
    );
    assert_eq!(mock.self_addressed_sends(), 1, "exactly one probe for one request");
    assert_eq!(counter.accepted(), 1, "no new connection for the requested probe");
    assert_eq!(mock.stream_frames_seen(), 0, "the probe is a unary Send, never a stream frame");

    handle.abort();
    let _ = handle.await;
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while netbird_core::signal::receive_probe_registered() {
        assert!(std::time::Instant::now() <= deadline, "the probe handle outlived its stream");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s_a_event_observes_the_natural_watchdog_heartbeat_on_the_existing_stream() {
    let _guard = serial();
    n2_socket_trace::arm(false);
    n2_a_event::reset();

    let mock = SignalMock::default();
    let (addr, counter) = spawn_signal_mock(mock.clone()).await;

    // The shell's own order: open the fd, arm the trace, register the protect
    // receipt, then hand the fd to the connector. The dial dups it and the SAME
    // `first_io` call emits `N2_SOCK` and registers the live pair.
    let fd = open_fd();
    assert!(fd >= 0, "shell fd");
    n2_socket_trace::arm(true);
    n2_socket_trace::protect(fd, "S", "req-signal-watchdog", 0, 31, 4, true);
    let keys = EnvelopeKeyPair::generate().expect("keys");
    // The server-side proof that the frame really is the production
    // self-addressed HEARTBEAT: this sink holds OUR keys, so it decrypts every
    // frame addressed to us and records `Body.type|payload|port|version`.
    *mock.sink.lock().expect("sink") = Some(SinkPeer::for_keys(keys.clone()));
    let source = Arc::new(ProtectedSocketFdSource::new_with_fd(fd).expect("seed"));
    // N2 T7a item 5: the watchdog window is SHRUNK so the NATURAL
    // silence-triggered production heartbeat really fires inside the test; the
    // product default is 20 s / 30 s.
    let handle = registered_watchdog_with_window(addr, keys, source,
        Duration::from_secs(1), Duration::from_secs(10)).await;

    let (dup_fd, original_fd, remote) = n2_socket_trace::live_of("S")
        .expect("live S registration from the N2_SOCK call site");
    assert_eq!(original_fd, fd, "the reported original is the shell fd");
    assert_ne!(dup_fd, fd, "the core dials its own dup, never the original");
    assert_eq!(counter.accepted(), 1, "one connection so far");
    assert_eq!(mock.self_addressed_sends(), 0, "no natural probe yet");

    // The trigger runs OFF the runtime, exactly like the shell/NAPI thread in
    // production, and only ARMS the observer (N2 T7a item 5).
    let trigger = Arc::new(SWatchTrigger {
        runtime: tokio::runtime::Handle::current(),
        calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        fail: Arc::new(AtomicBool::new(false)),
    });
    n2_a_event::install_trigger(trigger.clone());

    let (outcome, json) = std::thread::spawn(|| {
        (n2_a_event::apply("S", "a-S-1"), netbird_core::connector::n2_a_event_json("S", "a-S-2"))
    })
    .join()
    .expect("join");

    // The arming line: `phase=begin`, `result=armed`, the live fd pair and the
    // 45 s box — and NOTHING was sent by the trigger.
    assert_eq!(outcome.result, "armed", "{}", outcome.line);
    assert_eq!(outcome.reason, "signal-heartbeat-watch");
    assert_eq!(outcome.fd, original_fd);
    assert_eq!(outcome.dup_fd, dup_fd);
    assert!(outcome.line.starts_with("N2_A_EVENT|phase=begin|id=a-S-1|kind=S|"),
        "{}", outcome.line);
    assert!(outcome.line.contains(&format!("|fd={original_fd}|dup_fd={dup_fd}|remote={remote}|")));
    assert!(outcome.line.contains("|detail=wait_ms=45000|"), "{}", outcome.line);
    assert!(outcome.line.contains("|event_id=a-S-1|"), "{}", outcome.line);
    assert!(outcome.line.ends_with("|diag=true|is_evidence=false"), "{}", outcome.line);
    // the FIRST apply armed the watch, so the second (JSON) call is refused by
    // name instead of arming a second watcher
    assert!(json.contains("\"result\":\"rejected\"") &&
        json.contains("\"reason\":\"watch-already-armed\""), "{json}");
    assert_eq!(trigger.calls.load(Ordering::Acquire), 1, "one watch arm");
    assert!(n2_a_event::s_watch_open(), "the observer is waiting");
    assert_eq!(n2_socket_trace::event_window(dup_fd).as_deref(), Some("a-S-1"),
        "the forced-receipt window is open on the live S descriptor");

    // The NATURAL watchdog heartbeat fires on its own silence window (no N2
    // request), and the armed observer completes with `result=observed`.
    //
    // The mock counts the frame on the SERVER side (as soon as it decrypts
    // it), while the client-side completion `natural_heartbeat_sent` is
    // recorded only AFTER `send_heartbeat().await` returns (src/signal.rs):
    // the server count alone does not yet imply the A-event line exists. Wait
    // for the completion this test asserts on — bounded, so a heartbeat that
    // really never completes the watch still fails loudly.
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    while mock.self_addressed_sends() == 0 || n2_a_event::last_result() != Some("observed") {
        assert!(std::time::Instant::now() <= deadline,
            "the natural watchdog heartbeat never completed the watch: sends={} last={:?}",
            mock.self_addressed_sends(), n2_a_event::last_line());
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(n2_a_event::last_result(), Some("observed"),
        "the natural heartbeat completed the watch: {:?}", n2_a_event::last_line());
    let observed = n2_a_event::last_line().expect("observed line");
    assert!(observed.starts_with("N2_A_EVENT|phase=end|id=a-S-1|kind=S|"), "{observed}");
    assert!(observed.contains("|result=observed|reason=watchdog-heartbeat-sent|"), "{observed}");
    assert!(observed.contains("|detail=natural-heartbeat|"), "{observed}");
    assert!(observed.contains("|event_id=a-S-1|"), "{observed}");
    assert!(!n2_a_event::s_watch_open(), "one observation per watch");
    assert_eq!(n2_socket_trace::event_window(dup_fd), None, "the window closed at the end");

    // The frame that left IS the production self-addressed HEARTBEAT on the
    // EXISTING connection: exactly one connection, and the natural probe is a
    // unary Send decrypted by our own keys with type 6.
    assert_eq!(counter.accepted(), 1, "NO new connection for the S event");
    assert_eq!(mock.self_addressed_sends(), 1, "exactly one natural heartbeat");
    assert_eq!(mock.stream_frames_seen(), 0, "unary Send only — never a stream frame");
    let records = mock
        .sink
        .lock()
        .expect("sink")
        .as_ref()
        .expect("sink installed")
        .records
        .lock()
        .expect("records")
        .clone();
    assert!(
        records.iter().any(|r| r.starts_with("6||0|")),
        "the decrypted frame is Body.type=6 (HEARTBEAT), got {records:?}"
    );
    assert!(
        !records.iter().any(|r| r.starts_with("0|") || r.starts_with("1|") || r.starts_with("2|")),
        "no OFFER/ANSWER/CANDIDATE frame was created: {records:?}"
    );
    assert_eq!(
        n2_socket_trace::live_of("S"),
        Some((dup_fd, original_fd, remote)),
        "the live S socket is the same fd/dup chain"
    );

    // A refused watch arm is reported as an error, never as `armed`.
    trigger.fail.store(true, Ordering::Release);
    let failed = n2_a_event::apply("S", "a-S-3");
    assert_eq!(failed.result, "error");
    assert_eq!(failed.reason, "signal-heartbeat-network");
    assert!(!n2_a_event::s_watch_open(), "a refused arm opens no watch");
    assert_eq!(mock.self_addressed_sends(), 1, "a refused arm sends nothing");

    handle.abort();
    let _ = handle.await;
    n2_socket_trace::closed(dup_fd);
    n2_socket_trace::arm(false);
    n2_a_event::reset();
}
