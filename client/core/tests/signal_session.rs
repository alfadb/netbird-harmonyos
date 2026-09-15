// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N4a signal session tests: the `run_events` reconnect loop semantics of
//! `netbird_core::signal::SignalSession` against the in-process mock
//! server — upstream retry shape (`shared/signal/client/grpc.go:189-284`)
//! driven through the injected `crate::backoff` policy:
//!
//! 1. Registered → Message: a candidate sent by a REAL second client
//!    arrives decrypted; a stream break (transport kill) yields `Broken`,
//!    a fresh protected socket per reconnect, and `Registered` again.
//! 2. PermissionDenied (Auth class) on registration terminates the loop
//!    with `Err` — NO retry, NO backoff delay consumed (engine policy,
//!    client/internal/connect.go:353-356; sync.rs convention).
//! 3. A malformed frame (corrupted ciphertext) surfaces `Malformed` and
//!    the stream STAYS up — the next good frame is delivered (upstream
//!    decrypt-worker behavior, grpc.go:600-602).
//! 4. Backoff exhaustion ends the session with the pending error.

mod signal_mock;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use netbird_core::backoff::ExponentialBackoff;
use netbird_core::envelope::EnvelopeKeyPair;
use netbird_core::management::ManagementError;
use netbird_core::signal::proto::body;
use netbird_core::signal::{SignalLoopEvent, SignalSession};

use signal_mock::{
    connect_client, socket_source, spawn_signal_mock, SignalMock, TestTransport,
};

/// Deterministic RNG (pops fixed samples in order) — the sync_stream.rs
/// test contract; `Send` via plain VecDeque.
struct FixedRng {
    samples: std::collections::VecDeque<f64>,
}

impl FixedRng {
    fn filled(n: usize, v: f64) -> Box<FixedRng> {
        Box::new(FixedRng { samples: std::collections::VecDeque::from(vec![v; n]) })
    }
}

impl netbird_core::backoff::Rng for FixedRng {
    fn next_uniform(&mut self) -> f64 {
        self.samples.pop_front().expect("rng samples exhausted")
    }
}

/// What the run_events callback recorded.
#[derive(Debug, Clone, PartialEq)]
enum Rec {
    Registered,
    Message { kind: i32, payload: String },
    Malformed(String),
    Broken(String),
}

fn record(event: SignalLoopEvent<'_>) -> Rec {
    match event {
        SignalLoopEvent::Registered => Rec::Registered,
        SignalLoopEvent::Message(m) => Rec::Message {
            kind: m.kind as i32,
            payload: m.payload.clone(),
        },
        SignalLoopEvent::Malformed(e) => Rec::Malformed(format!("{e}")),
        SignalLoopEvent::Broken(e) => Rec::Broken(format!("{e}")),
    }
}

fn quick_policy(max_elapsed: Option<Duration>) -> ExponentialBackoff {
    ExponentialBackoff::new(
        Duration::from_millis(2),
        0.0,
        1.7,
        Duration::from_millis(10),
        max_elapsed,
    )
}

/// Bounded wait for a predicate over the recorded events (no
/// sleep-as-assertion; the condition itself is the assertion). HANG GUARD,
/// not a latency assertion; generous (60s) so a fully loaded parallel
/// `cargo test` run never trips it spuriously.
async fn wait_for<F: Fn() -> bool>(what: &'static str, cond: F) {
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while !cond() {
        assert!(
            std::time::Instant::now() <= deadline,
            "deadline waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn session_receives_breaks_reconnects_and_receives_again() {
    let mock = SignalMock::default();
    let (addr, counter) = spawn_signal_mock(mock.clone()).await;
    let keys_a = EnvelopeKeyPair::generate().expect("keys A");
    // the SESSION peer: source with 2 sockets; the shell (test) refills
    // between reconnects only when needed — here both are pre-seeded
    let source_a = socket_source(2);
    let client_a = connect_client(TestTransport::Plain, addr, keys_a.clone(), source_a.clone()).await;
    let mut session = SignalSession::new(client_a).with_policy(
        quick_policy(None),
        FixedRng::filled(64, 0.5),
        Box::new(netbird_core::backoff::MonotonicClock),
    );

    let recs = Arc::new(std::sync::Mutex::new(Vec::<Rec>::new()));
    let loop_recs = recs.clone();
    let handle = tokio::spawn(async move {
        let _ = session.run_events(move |ev| loop_recs.lock().expect("recs").push(record(ev))).await;
    });

    wait_for("initial Registered", || {
        recs.lock().expect("recs").iter().any(|r| r == &Rec::Registered)
    })
    .await;
    assert_eq!(source_a.taken(), 1, "initial registration: one protected fd");
    assert_eq!(counter.accepted(), 1);

    // a REAL second client sends a CANDIDATE to the session peer (unary
    // Send RPC → the mock forwards into the session's stream)
    let keys_b = EnvelopeKeyPair::generate().expect("keys B");
    let source_b = socket_source(1);
    let mut client_b =
        connect_client(TestTransport::Plain, addr, keys_b.clone(), source_b.clone()).await;
    client_b
        .send(&client_b.build_message(
            &keys_a.public_key_base64(),
            body::Type::Candidate,
            "ice-candidate-1",
            0,
            None,
        ))
        .await
        .expect("B sends candidate");
    wait_for("candidate received", || {
        recs.lock().expect("recs").iter().any(
            |r| matches!(r, Rec::Message { kind, payload } if *kind == body::Type::Candidate as i32 && payload == "ice-candidate-1"),
        )
    })
    .await;

    // transport kill → Broken → reconnect with a fresh protected socket
    counter.kill_all_connections();
    wait_for("Broken after kill", || {
        recs.lock().expect("recs").iter().any(|r| matches!(r, Rec::Broken(_)))
    })
    .await;
    wait_for("second Registered (reconnect)", || {
        recs.lock().expect("recs").iter().filter(|r| r == &&Rec::Registered).count() >= 2
    })
    .await;
    // exact per-dial equality (taken == accepted) is pinned in
    // signal_channel::every_reconnect_...; here the shared counter sees TWO
    // peers and the channel may additionally start-and-cancel a racing dial
    // inside the session loop (an fd taken but never accepted). Invariants:
    // peer B consumed exactly one fd; the session peer re-protected (a
    // fresh fd for the new connection).
    assert_eq!(source_b.taken(), 1, "peer B: one fd, one dial");
    assert!(
        source_b.taken() as usize + source_a.taken() as usize >= counter.accepted(),
        "every real connection rode a provided socket (racing dials may take extra fds)"
    );
    assert!(source_a.taken() >= 2, "the reconnect took a fresh fd");
    assert_eq!(
        counter.accepted() - source_b.taken() as usize,
        2,
        "the session peer made exactly two real connections (initial + reconnect)"
    );
    assert!(
        !recs.lock().expect("recs").iter().any(|r| matches!(r, Rec::Malformed(_))),
        "clean exchange must not report malformed frames"
    );

    handle.abort();
    let _ = handle.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn permission_denied_registration_terminates_without_retry() {
    let mock = SignalMock::default();
    mock.deny_registration.store(true, Ordering::SeqCst);
    let (addr, counter) = spawn_signal_mock(mock.clone()).await;
    let client = connect_client(
        TestTransport::Plain,
        addr,
        EnvelopeKeyPair::generate().expect("keys"),
        socket_source(4),
    )
    .await;
    let mut session = SignalSession::new(client).with_policy(
        quick_policy(None),
        FixedRng::filled(16, 0.5),
        Box::new(netbird_core::backoff::MonotonicClock),
    );

    let recs = Arc::new(std::sync::Mutex::new(Vec::<Rec>::new()));
    let loop_recs = recs.clone();
    let err = session
        .run_events(move |ev| {
            loop_recs.lock().expect("recs").push(record(ev));
        })
        .await
        .expect_err("PermissionDenied must end the session with Err");
    assert!(
        matches!(err, ManagementError::Auth { status: 7, .. }),
        "PermissionDenied → Auth class, got {err:?}"
    );
    assert_eq!(session.reconnects(), 0, "no reconnect counting on fatal");
    assert!(
        session.observed_reconnect_delays().is_empty(),
        "fatal path consumes NO backoff delay"
    );
    assert_eq!(
        counter.accepted(),
        1,
        "exactly one attempt: no retry after the Auth rejection"
    );
    assert!(
        !recs.lock().expect("recs").iter().any(|r| r == &Rec::Registered),
        "no Registered event when registration is denied"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn malformed_frame_is_surfaced_and_stream_stays_up() {
    let mock = SignalMock::default();
    mock.corrupt_forwarded_bodies.store(true, Ordering::SeqCst);
    let (addr, _counter) = spawn_signal_mock(mock.clone()).await;
    let keys_a = EnvelopeKeyPair::generate().expect("keys A");
    let client_a =
        connect_client(TestTransport::Plain, addr, keys_a.clone(), socket_source(1)).await;
    let mut session = SignalSession::new(client_a).with_policy(
        quick_policy(None),
        FixedRng::filled(16, 0.5),
        Box::new(netbird_core::backoff::MonotonicClock),
    );

    let recs = Arc::new(std::sync::Mutex::new(Vec::<Rec>::new()));
    let loop_recs = recs.clone();
    let handle = tokio::spawn(async move {
        let _ = session.run_events(move |ev| loop_recs.lock().expect("recs").push(record(ev))).await;
    });

    wait_for("initial Registered", || {
        recs.lock().expect("recs").iter().any(|r| r == &Rec::Registered)
    })
    .await;

    // corrupt forwarding ON: the candidate arrives but cannot be
    // authenticated → Malformed, stream stays up
    let keys_b = EnvelopeKeyPair::generate().expect("keys B");
    let mut client_b =
        connect_client(TestTransport::Plain, addr, keys_b.clone(), socket_source(1)).await;
    client_b
        .send(&client_b.build_message(
            &keys_a.public_key_base64(),
            body::Type::Candidate,
            "corrupted-payload",
            0,
            None,
        ))
        .await
        .expect("B sends corrupted candidate");
    wait_for("Malformed surfaced", || {
        recs.lock().expect("recs").iter().any(|r| matches!(r, Rec::Malformed(_)))
    })
    .await;

    // corrupt forwarding OFF: the next candidate is delivered normally —
    // proof the stream was never torn down (no Broken / reconnect)
    mock.corrupt_forwarded_bodies.store(false, Ordering::SeqCst);
    client_b
        .send(&client_b.build_message(
            &keys_a.public_key_base64(),
            body::Type::Candidate,
            "clean-payload",
            0,
            None,
        ))
        .await
        .expect("B sends clean candidate");
    wait_for("clean candidate received", || {
        recs.lock().expect("recs").iter().any(
            |r| matches!(r, Rec::Message { payload, .. } if payload == "clean-payload"),
        )
    })
    .await;
    {
        let recs = recs.lock().expect("recs");
        assert!(
            !recs.iter().any(|r| matches!(r, Rec::Broken(_))),
            "a malformed frame must NOT break the stream: {recs:?}"
        );
        assert_eq!(
            recs.iter().filter(|r| r == &&Rec::Registered).count(),
            1,
            "no reconnect happened: {recs:?}"
        );
    }

    handle.abort();
    let _ = handle.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn backoff_exhaustion_ends_the_session_with_the_pending_error() {
    // every registration attempt is rejected with a RETRYABLE Unavailable
    // (upstream grpc.go:578-580 class) until the injected budget is spent
    let mock = SignalMock::default();
    mock.fail_registration_unavailable.store(true, Ordering::SeqCst);
    let (addr, counter) = spawn_signal_mock(mock.clone()).await;

    let client = connect_client(
        TestTransport::Plain,
        addr,
        EnvelopeKeyPair::generate().expect("keys"),
        socket_source(4),
    )
    .await;
    let mut session = SignalSession::new(client).with_policy(
        // generous budget (robust against scheduler jitter between
        // construction and run start), tiny delays: still ~a dozen retries
        // and well under a second before exhaustion
        quick_policy(Some(Duration::from_millis(100))),
        FixedRng::filled(64, 0.5),
        Box::new(netbird_core::backoff::MonotonicClock),
    );

    let recs = Arc::new(std::sync::Mutex::new(Vec::<Rec>::new()));
    let loop_recs = recs.clone();
    let err = session
        .run_events(move |ev| loop_recs.lock().expect("recs").push(record(ev)))
        .await
        .expect_err("exhausted budget must end with the pending error");
    assert!(
        matches!(err, ManagementError::Network(_)),
        "Unavailable is retryable Network class, got {err:?}"
    );
    // every failure here is a failed REGISTRATION attempt (the transport
    // stays pooled), so per the sync.rs diagnostics contract `reconnects`
    // counts stream breaks only — the retries show up as repeated Broken
    // events and consumed delays
    assert_eq!(session.reconnects(), 0);
    let broken_count = recs
        .lock()
        .expect("recs")
        .iter()
        .filter(|r| matches!(r, Rec::Broken(_)))
        .count();
    assert!(broken_count >= 2, "the loop actually retried ({broken_count})");
    assert!(
        !session.observed_reconnect_delays().is_empty(),
        "delays were consumed before exhaustion"
    );
    assert!(
        recs.lock().expect("recs").iter().all(|r| !matches!(r, Rec::Registered)),
        "no stream ever came up"
    );
    // the pooled transport stayed alive (dials were not the failing part)
    assert_eq!(counter.accepted(), 1);
}
