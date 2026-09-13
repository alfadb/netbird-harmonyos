// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N4a signal channel integration tests: `netbird_core::signal` against an
//! in-process mock `SignalExchange` server speaking the REAL upstream
//! protocol (header registration + pure forwarding). Covered:
//!
//! 1. registration: `ConnectStream` carries `x-wiretrustee-peer-id` == our
//!    base64 WireGuard public key; a server that does NOT confirm with
//!    `x-wiretrustee-peer-registered` fails the registration (Parse) —
//!    upstream always sends it (signal.go:87,117), absence = misbehaving
//!    server → fail closed.
//! 2. Offer/Answer/Candidate round trip between TWO REAL clients through
//!    the mock forwarder — every frame rides the UNARY `Send` RPC, the
//!    only path the deployed server generation forwards (v0.78.1 never
//!    reads `ConnectStream` inbound frames) — with server-side REAL
//!    decryption of the sealed bodies (the sink peer opens with the
//!    sender's public key and asserts the plaintext fields —
//!    type/payload/wgListenPort/netBirdVersion). The registered streams
//!    carry NO client→server frames (N10b: stream-side sends are
//!    black-holed by the real server, `SignalClient::register` no longer
//!    exposes an outbound half).
//! 3. HEARTBEAT self-addressed round trip (Key = RemoteKey = own key,
//!    grpc.go:555-566): the server routes it straight back.
//! 4. REAL TLS handshake (rcgen CA + leaf): exchange over TLS through the
//!    protected socket; an untrusted CA fails the dial with `Network`.
//! 5. protected socket: the channel rides ONLY the injected sockets —
//!    taken == accepted at every step; every reconnect re-protects (a
//!    fresh fd per re-dial); an exhausted provider fails closed with NO
//!    unprotected connection reaching the server.

mod signal_mock;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tonic::Streaming;

use netbird_core::envelope::EnvelopeKeyPair;
use netbird_core::grpc::{GrpcTlsConfig, GrpcTransport};
use netbird_core::management::ManagementError;
use netbird_core::mgmtsock::ProtectedSocketFdSource;
use netbird_core::signal::proto::{body, EncryptedMessage};
use netbird_core::signal::{SignalClient, SIGNAL_CLIENT_VERSION};

use signal_mock::{
    connect_client, make_test_ca, socket_source, spawn_signal_mock, spawn_signal_mock_tls,
    SinkPeer, SignalMock, TestTransport,
};

fn keys() -> EnvelopeKeyPair {
    EnvelopeKeyPair::generate().expect("key pair")
}

/// Receive one frame from a registered stream (test helper).
async fn recv(stream: &mut Streaming<EncryptedMessage>) -> EncryptedMessage {
    stream
        .message()
        .await
        .expect("frame from signal server")
        .expect("stream open")
}

#[tokio::test(flavor = "multi_thread")]
async fn registration_carries_wg_identity_and_requires_confirm_header() {
    let mock = SignalMock::default();
    let (addr, counter) = spawn_signal_mock(mock.clone()).await;

    let keys = keys();
    let source = socket_source(2);
    let mut client =
        connect_client(TestTransport::Plain, addr, keys.clone(), source.clone()).await;
    // the dial itself consumed exactly one protected socket
    assert_eq!(source.taken(), 1);
    assert_eq!(counter.accepted(), 1);

    client.register().await.expect("register");
    assert_eq!(
        mock.registered_ids(),
        vec![keys.public_key_base64()],
        "x-wiretrustee-peer-id must be our base64 WG public key"
    );

    // a server that skips the registered confirm header → Parse, no stream
    mock.omit_registered_header.store(true, Ordering::SeqCst);
    let err = client.register().await.expect_err("must fail closed");
    assert!(
        matches!(err, ManagementError::Parse(_)),
        "unconfirmed registration must be Parse, got {err:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn offer_answer_candidate_roundtrip_with_real_envelope() {
    let keys_a = keys();
    let keys_b = keys();
    let mut mock = SignalMock::default();
    // the mock's decrypting sink plays peer B: everything addressed to B is
    // REALLY opened with B's private key and the plaintext recorded
    let sink = SinkPeer::for_keys(keys_b.clone());
    let sink_records = sink.records.clone();
    mock.sink = Arc::new(Mutex::new(Some(sink)));
    // install BEFORE the clone: the server copy shares the sink Arc
    let (addr, _counter) = spawn_signal_mock(mock.clone()).await;

    let source_a = socket_source(1);
    let source_b = socket_source(1);
    let mut client_a =
        connect_client(TestTransport::Plain, addr, keys_a.clone(), source_a).await;
    let mut client_b =
        connect_client(TestTransport::Plain, addr, keys_b.clone(), source_b).await;
    let mut reg_a = client_a.register().await.expect("A register");
    let mut reg_b = client_b.register().await.expect("B register");

    // --- OFFER: A → B through the UNARY Send RPC (the only forwarding
    // path of the deployed server generation; v0.78.1 never reads the
    // ConnectStream body — signal.go:106-132)
    let offer = client_a.build_message(
        &keys_b.public_key_base64(),
        body::Type::Offer,
        "ufragA:pwdA",
        51_820,
    );
    let wire = client_a.encrypt_message(&offer).expect("seal");
    assert_eq!(wire.key, keys_a.public_key_base64());
    assert_eq!(wire.remote_key, keys_b.public_key_base64());
    client_a.send(&offer).await.expect("send offer (unary)");

    let env_b = recv(&mut reg_b.inbound).await;
    let got_b = client_b.decrypt_envelope(&env_b).expect("B opens");
    assert_eq!(got_b.from_key, keys_a.public_key_base64());
    assert_eq!(got_b.remote_key, keys_b.public_key_base64());
    assert_eq!(got_b.kind, body::Type::Offer);
    assert_eq!(got_b.payload, "ufragA:pwdA");
    assert_eq!(got_b.wg_listen_port, 51_820);
    assert_eq!(got_b.net_bird_version, SIGNAL_CLIENT_VERSION);

    // --- ANSWER: B → A (unary as well)
    let answer = client_b.build_message(
        &keys_a.public_key_base64(),
        body::Type::Answer,
        "ufragB:pwdB",
        51_821,
    );
    client_b.send(&answer).await.expect("send answer (unary)");
    let env_a = recv(&mut reg_a.inbound).await;
    let got_a = client_a.decrypt_envelope(&env_a).expect("A opens");
    assert_eq!(got_a.kind, body::Type::Answer);
    assert_eq!(got_a.payload, "ufragB:pwdB");
    assert_eq!(got_a.wg_listen_port, 51_821);

    // --- CANDIDATE: A → B via the UNARY Send RPC
    client_a
        .send(&client_a.build_message(
            &keys_b.public_key_base64(),
            body::Type::Candidate,
            "candidate:udp:127.0.0.1:51820",
            0,
        ))
        .await
        .expect("unary send");
    let env_c = recv(&mut reg_b.inbound).await;
    let got_c = client_b.decrypt_envelope(&env_c).expect("B opens candidate");
    assert_eq!(got_c.kind, body::Type::Candidate);
    assert_eq!(got_c.payload, "candidate:udp:127.0.0.1:51820");

    // --- server-side decrypt proof: the sink opened the sealed frames with
    // B's private key and saw exactly the plaintext fields; everything
    // arrived through the unary path and NOTHING rode the stream bodies
    // (the v0.78.1 server never reads them — and the mock counts any such
    // frame instead of forwarding)
    let records = sink_records.lock().expect("records");
    let offer_rec =
        format!("{}|ufragA:pwdA|51820|{SIGNAL_CLIENT_VERSION}", body::Type::Offer as i32);
    let cand_rec = format!(
        "{}|candidate:udp:127.0.0.1:51820|0|{SIGNAL_CLIENT_VERSION}",
        body::Type::Candidate as i32
    );
    assert!(
        records.iter().any(|r| r.ends_with(&offer_rec)),
        "sink must have decrypted the OFFER: {records:?}"
    );
    assert!(
        records.iter().any(|r| r.ends_with(&cand_rec)),
        "sink must have decrypted the CANDIDATE: {records:?}"
    );
    // the ANSWER was addressed to A, not the sink: never opens under B's key
    assert!(
        !records.iter().any(|r| r.contains("ufragB:pwdB")),
        "messages sealed for A must not be decryptable by B's key"
    );
    drop(records);
    assert_eq!(mock.stream_frames_seen(), 0, "no frame may ride the ConnectStream body");
    assert!(mock.unary_sends() >= 3, "all frames ride the unary Send");
}

#[tokio::test(flavor = "multi_thread")]
async fn heartbeat_self_addressed_is_routed_back() {
    let mock = SignalMock::default();
    let (addr, _counter) = spawn_signal_mock(mock).await;
    let keys = keys();
    let mut client =
        connect_client(TestTransport::Plain, addr, keys.clone(), socket_source(1)).await;
    let mut reg = client.register().await.expect("register");

    client.send_heartbeat().await.expect("heartbeat");
    let env = recv(&mut reg.inbound).await;
    let msg = client.decrypt_envelope(&env).expect("open own heartbeat");
    assert_eq!(msg.kind, body::Type::Heartbeat);
    assert_eq!(msg.from_key, keys.public_key_base64(), "self-addressed");
    assert_eq!(msg.remote_key, keys.public_key_base64());
}

#[tokio::test(flavor = "multi_thread")]
async fn tls_real_handshake_exchange_and_untrusted_ca_rejected() {
    let ca = make_test_ca("signal-test-ca");
    let mock = SignalMock::default();
    let (addr, counter) = spawn_signal_mock_tls(mock, &ca).await;

    let keys = keys();
    let source = socket_source(2);
    let mut client =
        connect_client(TestTransport::Tls(&ca.ca_pem), addr, keys.clone(), source.clone()).await;
    assert_eq!(source.taken(), 1);
    assert_eq!(counter.accepted(), 1);

    let mut reg = client.register().await.expect("register over TLS");
    client.send_heartbeat().await.expect("heartbeat over TLS");
    let env = recv(&mut reg.inbound).await;
    let msg = client.decrypt_envelope(&env).expect("open over TLS");
    assert_eq!(msg.kind, body::Type::Heartbeat);

    // untrusted CA: the dial consumes a protected socket, then the TLS
    // handshake fails — a transport class, fail-closed (no plaintext
    // fallback). A fresh key pair avoids shadowing `keys`.
    let other = make_test_ca("unrelated-ca");
    let source2 = socket_source(1);
    let err = SignalClient::connect_with_socket_source(
        &format!("https://{addr}"),
        GrpcTransport::Tls(GrpcTlsConfig::new(vec![other.ca_pem.clone().into_bytes()])),
        Duration::from_secs(5),
        Duration::from_secs(5),
        EnvelopeKeyPair::generate().expect("keys"),
        source2.clone(),
        addr,
    )
    .await
    .expect_err("untrusted CA must fail");
    assert!(
        matches!(err, ManagementError::Network(_)),
        "TLS failure is Network class, got {err:?}"
    );
    assert_eq!(source2.taken(), 1, "the failed dial still re-protected");
}

#[tokio::test(flavor = "multi_thread")]
async fn every_reconnect_takes_a_fresh_protected_socket_and_exhaustion_fails_closed() {
    let mock = SignalMock::default();
    let (addr, counter) = spawn_signal_mock(mock).await;
    let keys = keys();
    // exactly TWO pre-protected sockets: two dials, then fail-closed
    let source = socket_source(2);

    let mut client =
        connect_client(TestTransport::Plain, addr, keys.clone(), source.clone()).await;
    let mut reg1 = client.register().await.expect("first stream");
    assert_eq!(source.taken(), 1, "one fd per dial");
    assert_eq!(counter.accepted(), 1, "one accept per provided socket");

    // the transport dies (server-side kill) → the frame source reports it
    counter.kill_all_connections();
    wait_dead(&mut reg1.inbound).await;

    // PROBE: hyper's pool may still hold the half-dead connection entry;
    // this tolerated attempt absorbs that stale-entry failure (or, if the
    // pool was already clean, re-dials). Either way the register below is
    // then deterministic — exactly what the session's retry loop does in
    // production.
    let _ = client.register().await;

    // reconnect: a FRESH protected socket, a FRESH server connection
    let mut reg2 = client.register().await.expect("reconnect re-registers");
    // exactly one NEW server connection for the reconnect (the accept event
    // is polled asynchronously, so wait rather than read-and-race); the
    // probe above may itself have re-dialed — accept count is what counts
    // real connections
    wait_accept(&counter, 2).await;
    assert!(
        source.taken() >= 2,
        "every reconnect re-protects (N3-7 parity): the re-dial took a fresh fd"
    );
    // the new stream really works: a self-addressed candidate through the
    // unary Send (the registered stream itself carries no client frames)
    let m = client.build_message(
        &keys.public_key_base64(),
        body::Type::Candidate,
        "post-reconnect",
        0,
    );
    client.send(&m).await.expect("send on re-registered channel");
    let env = recv(&mut reg2.inbound).await;
    assert_eq!(client.decrypt_envelope(&env).expect("open").payload, "post-reconnect");

    // provider exhausted → fail closed: the register fails and NO
    // unprotected connection reaches the server. The surfaced class for a
    // failed re-dial inside a live channel is Canceled/Request ("operation
    // was canceled") or Network — the fail-closed PROOF is the counters.
    counter.kill_all_connections();
    wait_dead(&mut reg2.inbound).await;
    let err = client.register().await.expect_err("no socket left");
    assert!(
        matches!(
            err,
            ManagementError::Request { .. }
                | ManagementError::Network(_)
                | ManagementError::Server { .. }
        ),
        "exhausted provider dials must fail closed (retryable class), got {err:?}"
    );
    assert_eq!(source.taken(), 2, "the seed was fully consumed, nothing more");
    assert_eq!(source.pending(), 0);
    assert_eq!(counter.accepted(), 2, "no unprotected dial reached the server");
}

/// Bounded wait until the listener has accepted `n` connections (the accept
/// event is delivered asynchronously to the client's connect(2)).
async fn wait_accept(counter: &signal_mock::CountingListener, n: usize) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while counter.accepted() < n {
        assert!(
            std::time::Instant::now() <= deadline,
            "deadline waiting for accept #{n}"
        );
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

/// The frame source of a killed transport: EOF or a transport status —
/// bounded wait (the FIN/RST needs a moment to cross the loopback).
async fn wait_dead(stream: &mut Streaming<EncryptedMessage>) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match stream.message().await {
            Ok(None) => return,     // clean EOF
            Err(_status) => return, // transport error
            Ok(Some(_)) => {
                assert!(
                    std::time::Instant::now() <= deadline,
                    "stream kept delivering after the transport kill"
                );
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_provider_fails_closed_with_no_unprotected_dial() {
    let mock = SignalMock::default();
    let (addr, counter) = spawn_signal_mock(mock).await;
    let source = Arc::new(ProtectedSocketFdSource::new_with_fd(-1));
    let err = SignalClient::connect_with_socket_source(
        &format!("http://{addr}"),
        GrpcTransport::Plaintext,
        Duration::from_secs(5),
        Duration::from_secs(5),
        keys(),
        source.clone(),
        addr,
    )
    .await
    .expect_err("empty provider must refuse to dial");
    assert!(matches!(err, ManagementError::Network(_)), "got {err:?}");
    assert_eq!(source.taken(), 0);
    assert_eq!(counter.accepted(), 0, "nothing may leave without protect");
}
