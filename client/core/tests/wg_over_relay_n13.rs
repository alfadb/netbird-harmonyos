// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N13-D2 end-to-end tests: the relay stream as a SECOND WireGuard bearer
//! (the upstream wgProxy equivalent) — full protocol stacks in one process:
//! REAL `WgDeviceFeed` devices, the REAL relay client (loopback fake relay
//! server, plain `rel://`), the REAL connector carrier orchestrator, mock
//! signal/management — no sleeps as assertions, no real-device commands.
//!
//! Proven here (the D2 acceptance core):
//! - **closed loop over relay**: two WG devices whose ONLY path is the relay
//!   carrier establish WG sessions and exchange overlay probes BOTH ways,
//!   byte-exact (TUN stand-in → device → `dispatch` → relay lane → fake
//!   server → peer lane pump → peer device → peer TUN); the relay client's
//!   `transport_*_bytes` and the fake server's forwarded bytes grow with the
//!   traffic, and NOTHING leaves via UDP (`endpoint` stays `None`,
//!   `carrier_tx_packets == tx_packets`);
//! - **priority/switching (T0 Q3, priority `Relay < ICE`)**: with an
//!   ICE-selected endpoint the device sends UDP direct and the relay lane is
//!   untouched (relay down ⇒ direct STILL flows); ICE loss (`recycle_-
//!   endpoint`) falls back to the carrier; every transition is counted
//!   (`carrier_takeovers` / `direct_restores`) and every carrier refusal is
//!   typed + counted (`carrier_rejects`) — never silently dropped;
//! - **MTU ceiling**: 8782B (8820 − 38, spec §7.2) is the last accepted
//!   payload; one byte more is a typed `FrameTooLarge` rejection at the
//!   carrier handle BEFORE any wire byte;
//! - **revocation cleanup**: `connector_stop()` tears the relay client AND
//!   every attached lane down (server sees the relay Close, no new outbound
//!   connection, no carrier left on the WG seam);
//! - **fd contract**: the relay path touches NO platform fd — the TUN/WG
//!   stand-in raw fds stay open (F_GETFD) across relay traffic AND across
//!   the connector/device teardown (only `VpnConnection.destroy()`-style
//!   owner closes would be allowed to change that, and none runs here);
//! - **default off**: `relay_enabled:false` keeps ZERO relay behavior (no
//!   dial, no carrier, status `disabled`) — the D1 hard default re-pinned.

// Host-process link stubs (repo convention, cf. tests/wg_over_ice_n11.rs):
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
        _cbinfo: *const c_void,
        _argc: *mut usize,
        _argv: *mut *mut c_void,
        _data: *mut c_void,
        _result: *mut usize,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_string_utf8(
        _env: *mut c_void,
        _value: *const c_void,
        _buf: *mut u8,
        _bufsize: usize,
        _result: *mut usize,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_int32(
        _value: *const c_void,
        _result: *mut i32,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(
        _value: *const c_void,
        _result: *mut bool,
    ) -> i32 {
        0
    }
}

use core::time::Duration;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use base64::Engine as _;
use prost::Message as _;
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status};

use netbird_core::backoff::ExponentialBackoff;
use netbird_core::config::Route;
use netbird_core::connector::{
    ConfigApplier, ConnectorConfig, ConnectorHandle, ConnectorSecrets, GrpcManagementFactory,
    HostIceTuning, RelayAttach, RelayCarrier, RelayMaterials, SyncPolicy, WgPeerApplier,
    WgPeerEntry,
};
use netbird_core::envelope::{self, EnvelopeKeyPair, EnvelopePublicKey};
use netbird_core::grpc::proto::management_service_server::{
    ManagementService, ManagementServiceServer,
};
use netbird_core::grpc::proto::{
    Empty, EncryptedMessage, LoginRequest, LoginResponse, ServerKeyResponse, SyncRequest,
    SyncResponse,
};
use netbird_core::grpc::PeerMeta;
use netbird_core::ice_session::is_wg_datagram;
use netbird_core::relay::{AuthToken, Frame, PeerId, MSG_CLOSE};
use netbird_core::relay_client::{
    RelayClient, RelayClientConfig, RelayClientError, RelayState, RelayWgCarrier,
    MAX_TRANSPORT_PAYLOAD,
};
use netbird_core::relay_testserver::{RelayTestServer, TestServerConfig};
use netbird_core::sys;
use netbird_core::wg_device::{WgDeviceConfig, WgDeviceFeed, WgEgressCarrier};
use netbird_core::ws::{client_handshake, random_sec_websocket_key, WsClient, WsMessage};

extern "C" {
    fn socket(domain: i32, ty: i32, protocol: i32) -> i32;
    fn socketpair(domain: i32, ty: i32, protocol: i32, sv: *mut [i32; 2]) -> i32;
    fn close(fd: i32) -> i32;
    fn write(fd: i32, buf: *const core::ffi::c_void, n: usize) -> isize;
    fn read(fd: i32, buf: *mut core::ffi::c_void, n: usize) -> isize;
    fn fcntl(fd: i32, cmd: i32, arg: i32) -> i32;
}

/// Every test-side fd is O_NONBLOCK (never blocks the harness).
fn set_nonblock(fd: i32) {
    const F_GETFL: i32 = 3;
    const F_SETFL: i32 = 4;
    const O_NONBLOCK: i32 = 2048;
    let fl = unsafe { fcntl(fd, F_GETFL, 0) };
    assert!(fl >= 0, "F_GETFL");
    assert_eq!(unsafe { fcntl(fd, F_SETFL, fl | O_NONBLOCK) }, 0, "F_SETFL");
}

const VPN_A: [u8; 4] = [10, 77, 0, 1];
const VPN_B: [u8; 4] = [10, 77, 0, 2];

/// Fabricated (NOT secret) 32-byte signature, base64 std.
const SIG_B64: &str = "paWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaU=";

/// Real-time fuse for events that MUST happen (bounded predicate loops; the
/// fuse expiry IS the failure). Generous: the fake server is loopback.
const FUSE: Duration = Duration::from_secs(15);

/// Bounded predicate wait (no sleeps as assertions).
fn wait_for(fuse: Duration, mut pred: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + fuse;
    loop {
        if pred() {
            return;
        }
        assert!(Instant::now() < deadline, "condition not met within fuse: {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Bounded NEGATIVE assertion: `pred` must stay false for `window`.
fn assert_stays_false(window: Duration, mut pred: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + window;
    while Instant::now() < deadline {
        assert!(!pred(), "condition that must never happen occurred: {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Fabricated token (valid for the next real hour — the relay client runs
/// on the production SystemClock here).
fn token_valid_now() -> AuthToken {
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        + 3_600;
    AuthToken::from_management(&expires.to_string(), SIG_B64).expect("fabricated token")
}

/// boringtun's ffi installs a process-global panic hook that turns panics
/// into silent SIGSEGV (boringtun ffi/mod.rs `new_tunnel` → `raise(SIGSEGV)`,
/// cf. tests/wg_e2e.rs `show_panics`). The hook is installed at the FIRST
/// tunnel creation, so this MUST be called again AFTER device construction —
/// hence deliberately NOT OnceLock-guarded: every call re-installs the
/// printing hook and the LAST call wins. Prints to STDOUT (libtest captures
/// it into the failure report).
fn show_panics() {
    std::panic::set_hook(Box::new(|info| {
        println!("TEST PANIC: {info}");
        let bt = std::backtrace::Backtrace::force_capture();
        let text = format!("{bt}");
        // keep the failure report readable: our frames only
        let mut shown = 0;
        for line in text.lines() {
            if line.contains("wg_over_relay_n13") && shown < 12 {
                println!("  {line}");
                shown += 1;
            }
        }
    }));
}

/// (secret_b64, public_b64) through the same frozen boringtun derivation the
/// devices use — the registered peer key IS the peer's public key.
fn key_pair(byte: u8) -> (String, String) {
    let secret = base64::engine::general_purpose::STANDARD.encode([byte; 32]);
    let public = netbird_core::wg_device::x25519_public_b64(&secret).expect("public key");
    (secret, public)
}

/// Minimal IPv4/UDP frame (checksum-free; the device parses dst only).
fn probe_frame(src: [u8; 4], dst: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut f = vec![0u8; 28 + payload.len()];
    f[0] = 0x45; // IPv4, IHL 5
    let total = f.len() as u16;
    f[2..4].copy_from_slice(&total.to_be_bytes());
    f[8] = 64; // TTL
    f[9] = 17; // UDP
    f[12..16].copy_from_slice(&src);
    f[16..20].copy_from_slice(&dst);
    f[20..22].copy_from_slice(&40000u16.to_be_bytes());
    f[22..24].copy_from_slice(&40001u16.to_be_bytes());
    let ulen = (8 + payload.len()) as u16;
    f[24..26].copy_from_slice(&ulen.to_be_bytes());
    f[28..].copy_from_slice(payload);
    f
}

fn tun_write(fd: i32, frame: &[u8]) {
    let n = unsafe { write(fd, frame.as_ptr() as *const core::ffi::c_void, frame.len()) };
    assert_eq!(n, frame.len() as isize, "TUN stand-in write");
}

fn tun_try_read(fd: i32) -> Option<Vec<u8>> {
    let mut buf = [0u8; 4096];
    let n = unsafe { read(fd, buf.as_mut_ptr() as *mut core::ffi::c_void, buf.len()) };
    if n > 0 {
        Some(buf[..n as usize].to_vec())
    } else {
        None
    }
}

fn udp_try_read(fd: i32) -> Option<Vec<u8>> {
    let mut buf = [0u8; 4096];
    let mut from = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut flen = core::mem::size_of::<sys::sockaddr_in>() as u32;
    let n = unsafe {
        sys::recvfrom(
            fd,
            buf.as_mut_ptr() as *mut core::ffi::c_void,
            buf.len(),
            0,
            &mut from,
            &mut flen,
        )
    };
    if n > 0 {
        Some(buf[..n as usize].to_vec())
    } else {
        None
    }
}

fn bound_lo_socket() -> i32 {
    let fd = unsafe { socket(2, 2, 0) };
    assert!(fd >= 0);
    let sa = sys::sockaddr_in::new([127, 0, 0, 1], 0);
    assert_eq!(
        unsafe { sys::bind(fd, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) },
        0
    );
    set_nonblock(fd);
    fd
}

fn sock_name(fd: i32) -> ([u8; 4], u16) {
    let mut addr = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut len = core::mem::size_of::<sys::sockaddr_in>() as u32;
    assert_eq!(unsafe { sys::getsockname(fd, &mut addr, &mut len) }, 0);
    (addr.sin_addr, u16::from_be(addr.sin_port))
}

/// fd-contract probe: the raw fd must still be an open descriptor.
fn assert_fd_open(raw: i32, what: &str) {
    const F_GETFD: i32 = 1;
    assert!(unsafe { fcntl(raw, F_GETFD, 0) } >= 0, "{what} raw fd must stay open (fd contract)");
}

// ---------------------------------------------------------------------------
// device harness (the wg_over_ice_n11 shape, minus ICE)
// ---------------------------------------------------------------------------

struct Dev {
    feed: Arc<WgDeviceFeed>,
    tun_hand: i32,
    raws: Vec<i32>,
}

impl Drop for Dev {
    fn drop(&mut self) {
        for fd in self.raws.drain(..) {
            unsafe { close(fd) };
        }
    }
}

/// A REAL WgDeviceFeed device: bound loopback outer socket (the "protected"
/// WG socket; on the relay path it receives/sends NOTHING) + a socketpair
/// TUN stand-in. Fast handshake campaign under the injected clock.
fn mk_dev(secret_b64: &str, peer_key: &str, peer_vpn: [u8; 4]) -> Dev {
    let wg_raw = unsafe { socket(2, 2, 0) };
    assert!(wg_raw >= 0);
    let sa = sys::sockaddr_in::new([127, 0, 0, 1], 0);
    assert_eq!(
        unsafe { sys::bind(wg_raw, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) },
        0
    );
    let mut sv = [-1i32; 2];
    assert_eq!(unsafe { socketpair(1, 2, 0, &mut sv) }, 0);
    set_nonblock(sv[1]);

    let mut cfg = WgDeviceConfig::new(secret_b64.to_string());
    cfg.hs_retry_ms = 100;
    let feed = Arc::new(WgDeviceFeed::new(cfg));
    feed.feed_tun(sv[0]).expect("tun feed");
    feed.feed_wg_socket(wg_raw).expect("wg socket feed");
    feed.apply_peers(&[WgPeerEntry {
        pub_key_b64: peer_key.to_string(),
        allowed_ips: vec![Route { addr: peer_vpn, prefix_len: 32 }],
    }])
    .expect("device peers");
    assert!(feed.device_up(), "both feeds present → device up");
    // boringtun's ffi installs its silent-SIGSEGV panic hook at the FIRST
    // tunnel creation (i.e. inside the apply above) — re-install the
    // printing hook AFTER it so later failures stay diagnosable.
    show_panics();
    Dev { feed, tun_hand: sv[1], raws: vec![wg_raw, sv[0], sv[1]] }
}

fn pump_dev(dev: &Dev, now: u64) {
    let _ = dev.feed.with_device(|d| {
        d.service_tun(now);
        d.service_udp(now);
        d.tick(now);
    });
}

fn dev_stats(dev: &Dev) -> netbird_core::wg_device::WgDeviceStats {
    dev.feed.with_device(|d| d.stats()).expect("device up")
}

fn dev_peer(dev: &Dev) -> netbird_core::wg_device::WgPeerStatus {
    dev.feed.with_device(|d| d.peers()[0].clone()).expect("device up")
}

/// A real relay client over the loopback fake server (production seams:
/// SystemClock + TcpDialer, plain `rel://`).
fn start_relay_client(server: &RelayTestServer, local_pub_b64: &str) -> RelayClient {
    let cfg = RelayClientConfig::new(
        &[format!("rel://127.0.0.1:{}", server.addr().port())],
        local_pub_b64,
        token_valid_now(),
    )
    .expect("relay config");
    RelayClient::start(cfg).expect("relay client starts")
}

/// A raw remote peer on the fake server (the OTHER endpoint of the relay
/// lane; plays the remote node's relay session).
async fn connect_remote(
    server: &RelayTestServer,
    id: &PeerId,
) -> WsClient<tokio::net::TcpStream> {
    let tcp = tokio::net::TcpStream::connect(server.addr()).await.expect("remote dials");
    let key = random_sec_websocket_key().expect("random key");
    let mut ws = client_handshake(tcp, &server.addr().to_string(), "/relay", &key)
        .await
        .expect("remote ws handshake");
    ws.write_binary(&Frame::Auth { peer_id: id.clone(), token: token_valid_now() }.encode().expect("auth"))
        .await
        .expect("remote auth send");
    match ws.read_message().await.expect("remote auth response") {
        WsMessage::Binary(bytes) => match Frame::decode(&bytes).expect("valid frame") {
            Frame::AuthResponse { .. } => ws,
            other => panic!("remote must be authenticated first, got {other:?}"),
        },
        other => panic!("expected auth response binary, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 1. WG overlay over the relay carrier — byte-exact, counters, no UDP, fd
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn relay_carrier_round_trips_wg_overlay_byte_exact() {
    show_panics();
    let server = RelayTestServer::start(TestServerConfig::default()).await.expect("fake relay");
    let (sec_a, pub_a) = key_pair(0xA1);
    let (sec_b, pub_b) = key_pair(0xB2);

    // Both devices: the ONLY registered path is the relay carrier (no ICE,
    // no endpoint ever lands on either side).
    let a = mk_dev(&sec_a, &pub_b, VPN_B);
    let b = mk_dev(&sec_b, &pub_a, VPN_A);

    // Both nodes run the REAL relay client + the REAL connector orchestrator
    // (RelayCarrier) — exactly the production shape, driven by the harness.
    let client_a = start_relay_client(&server, &pub_a);
    let client_b = start_relay_client(&server, &pub_b);
    wait_for(FUSE, || client_a.state() == RelayState::Ready, "relay client A Ready");
    wait_for(FUSE, || client_b.state() == RelayState::Ready, "relay client B Ready");
    assert_eq!(server.stats().connections_accepted, 2, "two relay sessions, one per node");

    let carrier_a = RelayCarrier::new(a.feed.clone() as Arc<dyn WgPeerApplier>);
    carrier_a.attach_client(client_a.clone());
    carrier_a.set_peers(&[pub_b.clone()]);
    let carrier_b = RelayCarrier::new(b.feed.clone() as Arc<dyn WgPeerApplier>);
    carrier_b.attach_client(client_b.clone());
    carrier_b.set_peers(&[pub_a.clone()]);

    // The lanes: open_conn (PeersOnline — both peers authenticated) → the
    // real RelayWgCarrier handles land on each WG seam.
    assert!(
        timeout(FUSE, carrier_a.ensure_lane(&pub_b)).await.expect("A lane fuse"),
        "the A→B lane must attach (B online at the relay)"
    );
    assert!(
        timeout(FUSE, carrier_b.ensure_lane(&pub_a)).await.expect("B lane fuse"),
        "the B→A lane must attach (A online at the relay)"
    );
    assert_eq!(carrier_a.lane_count(), 1);
    assert_eq!(carrier_b.lane_count(), 1);

    // Inbound pumps: relay frames → reverse-map → device carrier ingress
    // (the production `relay_carrier_pump` recv arm, driven deterministically).
    let pump_a = {
        let carrier = carrier_a.clone();
        tokio::spawn(async move { while carrier.pump_once().await {} })
    };
    let pump_b = {
        let carrier = carrier_b.clone();
        tokio::spawn(async move { while carrier.pump_once().await {} })
    };

    // --- WG sessions must establish THROUGH the relay (handshakes ride the
    // carrier on both sides; neither device ever learns a UDP path).
    let mut now = sys::mono_ms();
    wait_for(FUSE, || {
        pump_dev(&a, now);
        pump_dev(&b, now);
        now += 10;
        a.feed.dataplane_status().map(|s| s.ready).unwrap_or(false)
            && b.feed.dataplane_status().map(|s| s.ready).unwrap_or(false)
    }, "WG sessions establish over the relay carrier on BOTH devices");

    // --- overlay probes BOTH directions, byte-exact
    let probe_a = probe_frame(VPN_A, VPN_B, b"n13-relay-probe-a1");
    let probe_b = probe_frame(VPN_B, VPN_A, b"n13-relay-probe-b1");
    let bytes_before_a = client_a.stats().transport_tx_bytes + client_a.stats().transport_rx_bytes;
    let server_fwd_before = server.stats().transport_bytes_forwarded;

    // NOTE: the probe waits PUMP the devices themselves (service_tun is what
    // drains the probe off the TUN stand-in and drives the carrier path).
    let mut now = sys::mono_ms();
    tun_write(a.tun_hand, &probe_a);
    wait_for(
        FUSE,
        || {
            pump_dev(&a, now);
            pump_dev(&b, now);
            now += 10;
            tun_try_read(b.tun_hand).as_deref() == Some(&probe_a[..])
        },
        "A→B probe arrives byte-exact through the relay carrier",
    );

    let mut now = sys::mono_ms();
    tun_write(b.tun_hand, &probe_b);
    wait_for(
        FUSE,
        || {
            pump_dev(&a, now);
            pump_dev(&b, now);
            now += 10;
            tun_try_read(a.tun_hand).as_deref() == Some(&probe_b[..])
        },
        "B→A probe arrives byte-exact through the relay carrier",
    );

    // --- accounting: the relay carried it, the UDP path did not
    let a_stats = dev_stats(&a);
    let b_stats = dev_stats(&b);
    assert_eq!(dev_peer(&a).endpoint, None, "A never had an ICE endpoint");
    assert_eq!(dev_peer(&b).endpoint, None, "B never had an ICE endpoint");
    assert!(a_stats.tx_packets >= 1, "A encapsulated datagrams");
    assert!(
        a_stats.carrier_tx_packets >= a_stats.tx_packets,
        "every A data datagram left via the relay carrier (carrier_tx also counts \
         protocol replies; tx_packets counts only data/keepalive/flush — and UDP \
         egress would have shown up as tx_packets WITHOUT carrier_tx)"
    );
    assert!(b_stats.tx_packets >= 1);
    assert!(b_stats.carrier_tx_packets >= b_stats.tx_packets);
    assert_eq!(a_stats.direct_restores, 0, "no direct episode ever ran");
    assert!(a_stats.carrier_takeovers >= 1, "the relay episode is observable");
    assert_eq!(a_stats.decrypt_errors, 0, "clean decapsulation both ways");
    assert_eq!(b_stats.decrypt_errors, 0);
    assert_eq!(a_stats.unknown_peer_drops, 0, "nothing arrived off-path");
    assert_eq!(b_stats.unknown_peer_drops, 0);

    // relay bytes grew (client + fake server agree on movement); the device
    // UDP counters did NOT absorb relay bytes (separate carrier counters).
    let bytes_after_a = client_a.stats().transport_tx_bytes + client_a.stats().transport_rx_bytes;
    assert!(bytes_after_a > bytes_before_a, "relay transport_bytes must grow");
    assert!(
        server.stats().transport_bytes_forwarded > server_fwd_before,
        "the fake server forwarded the relay frames"
    );

    // --- fd contract: the relay path holds NO fd; the platform-side stand-in
    // fds stay open through relay traffic AND through the device teardown.
    for (i, raw) in a.raws.iter().enumerate() {
        assert_fd_open(*raw, &format!("A raw #{i} after relay traffic"));
    }
    a.feed.clear();
    for (i, raw) in a.raws.iter().enumerate() {
        assert_fd_open(*raw, &format!("A raw #{i} after carrier teardown (fd contract)"));
    }

    // cleanup: orchestrator teardown detaches the lanes; the clients stop
    // (the server must see the relay Close frames).
    carrier_a.shutdown();
    carrier_b.shutdown();
    client_a.stop();
    client_b.stop();
    wait_for(FUSE, || client_a.state() == RelayState::Dead, "client A dead");
    wait_for(FUSE, || client_b.state() == RelayState::Dead, "client B dead");
    assert_eq!(carrier_a.lane_count(), 0, "orchestrator teardown detaches the lane");
    drop(pump_a);
    drop(pump_b);
    b.feed.clear();
    let _ = server.shutdown().await;
}

// ---------------------------------------------------------------------------
// 2. priority/switching: ICE wins, relay fallback, typed refusals
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn ice_nomination_wins_relay_falls_back_and_refusals_are_typed() {
    show_panics();
    let server = RelayTestServer::start(TestServerConfig::default()).await.expect("fake relay");
    let (sec_a, pub_a) = key_pair(0xC1);
    let (_sec_b, pub_b) = key_pair(0xC2);
    let pid_b = PeerId::from_wg_pubkey_string(&pub_b);
    let dev = mk_dev(&sec_a, &pub_b, VPN_B);

    let client = start_relay_client(&server, &pub_a);
    wait_for(FUSE, || client.state() == RelayState::Ready, "relay Ready");
    let carrier = Arc::new(RelayWgCarrier::new(client.clone(), pid_b.clone()));
    dev.feed.attach_carrier(&pub_b, carrier).expect("carrier attach");

    // --- phase 1: NO ICE endpoint ⇒ the carrier carries the handshake
    let mut now = sys::mono_ms();
    wait_for(FUSE, || {
        pump_dev(&dev, now);
        now += 10;
        dev_stats(&dev).carrier_tx_packets >= 1
    }, "phase 1: egress leaves via the relay carrier (no endpoint)");
    let s1 = dev_stats(&dev);
    assert_eq!(s1.carrier_takeovers, 1, "one UDP→relay episode");
    assert_eq!(s1.direct_restores, 0);
    assert_eq!(dev_peer(&dev).on_carrier, true, "bearer source observable");
    // the handshake left via the carrier and reached the WIRE (the relay
    // session's writer task counts on write): bounded predicate wait
    wait_for(
        FUSE,
        || client.stats().transport_tx_bytes > 0,
        "relay transport_tx_bytes grew with the WG handshake",
    );
    let tx_after_phase1 = client.stats().transport_tx_bytes;

    // --- phase 2: relay client DIES (relay unavailable)...
    client.stop();
    wait_for(FUSE, || client.state() == RelayState::Dead, "relay dead");

    // ...and THEN ICE nominates a direct path: UDP must win and flow, the
    // dead relay must not block or pollute the direct path.
    let udp_peer = bound_lo_socket();
    let (addr, port) = sock_name(udp_peer);
    dev.feed.apply_endpoint(&pub_b, addr, port).expect("ICE endpoint lands (fires the campaign)");
    // the landing itself fires the handshake; poll WITHOUT consuming twice —
    // the received datagram is kept in a slot (stateful predicate closure)
    let mut direct_slot: Option<Vec<u8>> = None;
    wait_for(
        FUSE,
        || {
            if direct_slot.is_none() {
                direct_slot = udp_try_read(udp_peer);
            }
            direct_slot.is_some()
        },
        "phase 2: WG handshake arrives over UDP direct",
    );
    let first_direct = direct_slot.expect("direct datagram");
    assert!(is_wg_datagram(&first_direct), "the direct datagram is WG-shaped");
    let s2 = dev_stats(&dev);
    assert_eq!(s2.direct_restores, 1, "relay→direct restore counted");
    assert_eq!(s2.carrier_takeovers, 1, "no new relay episode yet");
    assert_eq!(s2.carrier_tx_packets, s1.carrier_tx_packets, "the relay lane is untouched");
    assert_eq!(s2.carrier_rejects, 0, "dispatch never consulted the carrier while direct");
    assert_eq!(client.stats().transport_tx_bytes, tx_after_phase1, "relay bytes frozen while direct");
    assert_eq!(dev_peer(&dev).on_carrier, false);

    // --- phase 3: ICE dies (recycle) ⇒ fail-closed fallback to the relay
    // carrier; the dead relay refuses TYPED and the device counts it.
    dev.feed.recycle_endpoint(&pub_b);
    wait_for(FUSE, || {
        pump_dev(&dev, now);
        now += 10;
        dev_stats(&dev).carrier_rejects >= 1
    }, "phase 3: the dead relay lane refuses typed (counted, never silent)");
    let s3 = dev_stats(&dev);
    assert_eq!(s3.carrier_takeovers, 2, "a NEW relay episode started after ICE loss");
    assert_eq!(s3.carrier_tx_packets, s1.carrier_tx_packets, "refused datagrams are NOT counted as sent");
    assert_eq!(dev_peer(&dev).endpoint, None, "endpoint recycled");

    unsafe { close(udp_peer) };
    let _ = server.shutdown().await;
}

// ---------------------------------------------------------------------------
// 3. MTU ceiling: 8782 in, 8783 typed-rejected (before any wire byte)
// ---------------------------------------------------------------------------

/// Always-refusing carrier (the device-side non-silent accounting probe).
struct FailingCarrier;

impl WgEgressCarrier for FailingCarrier {
    fn send_datagram(&self, _datagram: &[u8]) -> Result<(), String> {
        Err("stub-frame-too-large".to_string())
    }
    fn kind(&self) -> &'static str {
        "stub-failing"
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn relay_payload_over_8782_is_rejected_typed() {
    show_panics();
    let server = RelayTestServer::start(TestServerConfig::default()).await.expect("fake relay");
    let (sec_a, pub_a) = key_pair(0xD1);
    let (_sec_b, pub_b) = key_pair(0xD2);
    let dev = mk_dev(&sec_a, &pub_b, VPN_B);

    let client = start_relay_client(&server, &pub_a);
    wait_for(FUSE, || client.state() == RelayState::Ready, "relay Ready");

    // The registered ceiling itself (8820 − 38, spec §7.2).
    assert_eq!(MAX_TRANSPORT_PAYLOAD, 8782);

    let carrier = RelayWgCarrier::new(client.clone(), PeerId::from_wg_pubkey_string(&pub_b));

    // Exactly AT the ceiling: accepted (frame == 8820 == MaxMessageSize).
    let at_limit = vec![7u8; MAX_TRANSPORT_PAYLOAD];
    carrier.send_datagram(&at_limit).expect("8782B payload is accepted");
    // tx_bytes counts ON THE WIRE (the session writer task): a bounded
    // predicate wait, never an immediate read (deterministic, no sleeps).
    let want = MAX_TRANSPORT_PAYLOAD as u64;
    wait_for(FUSE, || client.stats().transport_tx_bytes == want, "at-limit frame reaches the wire");

    // One byte OVER: typed rejection BEFORE any wire byte.
    let over = vec![7u8; MAX_TRANSPORT_PAYLOAD + 1];
    let err = carrier.send_datagram(&over).expect_err("8783B must be refused");
    assert!(
        matches!(err, RelayClientError::FrameTooLarge { limit: 8820, got: 8821 }),
        "typed FrameTooLarge, got {err:?}"
    );
    assert_eq!(
        client.stats().transport_tx_bytes,
        MAX_TRANSPORT_PAYLOAD as u64,
        "the refused payload never reached the wire path"
    );

    // The SAME refusal seen through the DEVICE seam (what WG actually calls):
    // a stable shape token, and on the device side a COUNTED refusal —
    // typed, never a silent drop (WG retransmits).
    let trait_err = WgEgressCarrier::send_datagram(&carrier, &over).unwrap_err();
    assert_eq!(trait_err, "relay-frame-too-large");

    dev.feed
        .attach_carrier(&pub_b, Arc::new(FailingCarrier))
        .expect("failing carrier attach");
    let probe = probe_frame(VPN_A, VPN_B, b"n13-mtu-probe");
    let mut now = sys::mono_ms();
    wait_for(FUSE, || {
        tun_write(dev.tun_hand, &probe);
        pump_dev(&dev, now);
        now += 10;
        dev_stats(&dev).carrier_rejects >= 1
    }, "device counts the carrier refusal (non-silent)");
    let st = dev_stats(&dev);
    assert!(st.carrier_rejects >= 1);
    assert_eq!(st.tx_packets, 0, "a refused datagram is not reported as sent");
    assert_eq!(dev_peer(&dev).on_carrier, true);
    assert_eq!(dev_peer(&dev).carrier_attached, true);

    client.stop();
    dev.feed.clear();
    let _ = server.shutdown().await;
}

// ---------------------------------------------------------------------------
// connector-level: the management mock (the relay_connector_e2e shape)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct MiniMgmt {
    keys: EnvelopeKeyPair,
    sync_script: Arc<Mutex<Vec<SyncResponse>>>,
    live: Arc<
        Mutex<Option<(EnvelopePublicKey, tokio::sync::mpsc::Sender<Result<EncryptedMessage, Status>>)>>,
    >,
}

impl Default for MiniMgmt {
    fn default() -> Self {
        MiniMgmt {
            keys: EnvelopeKeyPair::generate().expect("server key pair"),
            sync_script: Arc::new(Mutex::new(Vec::new())),
            live: Arc::new(Mutex::new(None)),
        }
    }
}

impl MiniMgmt {
    fn script(&self, updates: Vec<SyncResponse>) {
        *self.sync_script.lock().expect("script") = updates;
    }

    fn decrypt_body(&self, env: &EncryptedMessage) -> Result<Vec<u8>, Status> {
        let client_pk = EnvelopePublicKey::from_base64(&env.wg_pub_key)
            .map_err(|_| Status::invalid_argument("envelope wgPubKey is not a NaCl key"))?;
        envelope::open(&client_pk, &self.keys, &env.body)
            .map_err(|_| Status::internal("cannot decrypt request body"))
    }
}

#[tonic::async_trait]
impl ManagementService for MiniMgmt {
    async fn get_server_key(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ServerKeyResponse>, Status> {
        Ok(Response::new(ServerKeyResponse {
            key: self.keys.public_key_base64(),
            expires_at: None,
            version: 0,
        }))
    }

    async fn login(
        &self,
        request: Request<EncryptedMessage>,
    ) -> Result<Response<EncryptedMessage>, Status> {
        let env = request.into_inner();
        let plaintext = self.decrypt_body(&env)?;
        LoginRequest::decode(plaintext.as_slice())
            .map_err(|e| Status::invalid_argument(format!("body is not LoginRequest: {e}")))?;
        let response = LoginResponse::default();
        let client_pk = EnvelopePublicKey::from_base64(&env.wg_pub_key).expect("client pk");
        let sealed = envelope::seal(&client_pk, &self.keys, &response.encode_to_vec())
            .expect("seal login reply");
        Ok(Response::new(EncryptedMessage { wg_pub_key: env.wg_pub_key, body: sealed, version: 0 }))
    }

    async fn sync(
        &self,
        request: Request<EncryptedMessage>,
    ) -> Result<Response<tonic::codegen::BoxStream<EncryptedMessage>>, Status> {
        let env = request.into_inner();
        let plaintext = self.decrypt_body(&env)?;
        SyncRequest::decode(plaintext.as_slice())
            .map_err(|e| Status::invalid_argument(format!("first frame is not SyncRequest: {e}")))?;
        let client_pk = EnvelopePublicKey::from_base64(&env.wg_pub_key).expect("client pk");
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<EncryptedMessage, Status>>(16);
        let scripted: Vec<SyncResponse> =
            std::mem::take(&mut *self.sync_script.lock().expect("script"));
        for resp in scripted {
            let sealed = envelope::seal(&client_pk, &self.keys, &resp.encode_to_vec())
                .expect("seal scripted sync frame");
            tx.send(Ok(EncryptedMessage {
                wg_pub_key: env.wg_pub_key.clone(),
                body: sealed,
                version: 0,
            }))
            .await
            .expect("receiver alive while scripting");
        }
        *self.live.lock().expect("live") = Some((client_pk, tx));
        Ok(Response::new(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))))
    }

    async fn logout(
        &self,
        request: Request<EncryptedMessage>,
    ) -> Result<Response<Empty>, Status> {
        let env = request.into_inner();
        let _ = self.decrypt_body(&env)?;
        Ok(Response::new(Empty {}))
    }
}

async fn spawn_mock(svc: MiniMgmt) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(ManagementServiceServer::new(svc))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .expect("tonic serve");
    });
    addr
}

fn mgmt_peer(key: &str, ip: &str) -> netbird_core::grpc::proto::RemotePeerConfig {
    netbird_core::grpc::proto::RemotePeerConfig {
        wg_pub_key: key.into(),
        allowed_ips: vec![format!("{ip}/32")],
        ssh_config: None,
        fqdn: format!("{key}.example.net"),
        agent_version: "1.0.0".into(),
        lazy_state: 0,
    }
}

/// A sync snapshot with `NetbirdConfig.relay` + a one-peer network map.
fn sync_with_relay(serial: u64, urls: Vec<String>, token_payload: String, peer_key: &str) -> SyncResponse {
    SyncResponse {
        netbird_config: Some(netbird_core::grpc::proto::NetbirdConfig {
            relay: Some(netbird_core::grpc::proto::RelayConfig {
                urls,
                token_payload,
                token_signature: SIG_B64.into(),
            }),
            ..Default::default()
        }),
        network_map: Some(netbird_core::grpc::proto::NetworkMap {
            serial,
            peer_config: Some(netbird_core::grpc::proto::PeerConfig {
                address: "10.64.0.9".into(),
                ..Default::default()
            }),
            remote_peers: vec![mgmt_peer(peer_key, "10.30.30.1")],
            remote_peers_is_empty: false,
            routes: vec![],
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Recording host seam.
#[derive(Default)]
struct RecordingHost {
    serials: Mutex<Vec<u64>>,
}

impl ConfigApplier for RecordingHost {
    fn apply(&self, map: &netbird_core::network_map::NetworkMap) {
        self.serials.lock().expect("serials").push(map.serial);
    }
    fn clear(&self) {}
}

/// A CARRIER-CAPABLE test WG seam: honest bookkeeping of attached lanes
/// (the real devices live in the harness tests above; this seam observes
/// exactly what the connector's carrier orchestration does to the seam).
#[derive(Default)]
struct CarrierStubWg {
    peers: Mutex<Vec<WgPeerEntry>>,
    carriers: Mutex<HashMap<String, Arc<dyn WgEgressCarrier>>>,
    attaches: AtomicU64,
    detaches: AtomicU64,
    ready: AtomicBool,
}

impl CarrierStubWg {
    fn carrier_count(&self) -> usize {
        self.carriers.lock().expect("carriers").len()
    }
    fn attaches(&self) -> u64 {
        self.attaches.load(Ordering::Acquire)
    }
}

impl WgPeerApplier for CarrierStubWg {
    fn apply_peers(&self, peers: &[WgPeerEntry]) -> Result<(), String> {
        *self.peers.lock().expect("peers") = peers.to_vec();
        Ok(())
    }
    fn clear(&self) {
        self.peers.lock().expect("peers").clear();
        self.carriers.lock().expect("carriers").clear();
        self.ready.store(false, Ordering::Release);
    }
    fn tunnel_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
    fn carrier_capable(&self) -> bool {
        true
    }
    fn attach_carrier(
        &self,
        pub_key_b64: &str,
        carrier: Arc<dyn WgEgressCarrier>,
    ) -> Result<(), String> {
        if !self.peers.lock().expect("peers").iter().any(|p| p.pub_key_b64 == pub_key_b64) {
            return Err(format!("peer '{pub_key_b64}' is not registered"));
        }
        self.carriers.lock().expect("carriers").insert(pub_key_b64.to_string(), carrier);
        self.attaches.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }
    fn detach_carrier(&self, pub_key_b64: &str) {
        if self.carriers.lock().expect("carriers").remove(pub_key_b64).is_some() {
            self.detaches.fetch_add(1, Ordering::AcqRel);
        }
    }
    fn detach_all_carriers(&self) {
        let mut carriers = self.carriers.lock().expect("carriers");
        let n = carriers.len();
        carriers.clear();
        drop(carriers);
        self.detaches.fetch_add(n as u64, Ordering::AcqRel);
    }
}

fn test_private_key_b64(byte: u8) -> String {
    base64::engine::general_purpose::STANDARD.encode([byte; 32])
}

fn connector_config_for(addr: std::net::SocketAddr) -> ConnectorConfig {
    ConnectorConfig::from_json(&format!(
        "{{\"management_url\":\"http://{addr}\",\"private_key\":\"{}\",\"allow_unprotected_management\":true}}",
        test_private_key_b64(1)
    ))
    .expect("test config")
}

fn real_clock_materials() -> Arc<RelayMaterials> {
    Arc::new(RelayMaterials {
        clock: Arc::new(netbird_core::relay_client::SystemClock),
        dialer: Arc::new(netbird_core::relay_client::TcpDialer),
        tls: None,
    })
}

fn spawn_connector(
    addr: std::net::SocketAddr,
    wg: Arc<dyn WgPeerApplier>,
    relay_enabled: bool,
) -> (Arc<ConnectorHandle>, Arc<RecordingHost>) {
    let host = Arc::new(RecordingHost::default());
    let config = connector_config_for(addr);
    let handle = ConnectorHandle::spawn_with_relay(
        tokio::runtime::Handle::current(),
        Arc::new(GrpcManagementFactory::new(
            &config,
            EnvelopeKeyPair::from_secret_bytes(&[1u8; 32]),
        )),
        wg,
        host.clone(),
        ConnectorSecrets { setup_key: "SETUP-KEY-TEST-OK".into(), jwt: String::new() },
        PeerMeta {
            hostname: "ohos-n13-test".into(),
            os_name: "harmonyos".into(),
            os_version: "5.0.0".into(),
            netbird_version: "0.1.0".into(),
        },
        ExponentialBackoff::upstream_stream_default(),
        SyncPolicy::production(),
        Duration::from_secs(600),
        Duration::from_secs(3600),
        false, // default-route gate: no force opt-in
        None,  // no protected management socket
        None,  // production ICE orchestrator
        None,  // no signal material
        None,  // no WG device feed (stub seam above)
        HostIceTuning::default(),
        relay_enabled,
        relay_enabled.then(|| RelayAttach {
            wg_pubkey_b64: test_private_key_b64(7),
            materials: real_clock_materials(),
        }),
    );
    (handle, host)
}

// ---------------------------------------------------------------------------
// 4. connector_stop() drops the relay bearer (client AND every lane)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn connector_stop_drops_the_relay_bearer() {
    show_panics();
    let relay_server =
        RelayTestServer::start(TestServerConfig::default()).await.expect("fake relay");
    let remote_key = "cmVtb3RlLXRlYXIta2V5LWZpeHR1cmUtMDA=";
    let mock = MiniMgmt::default();
    mock.script(vec![sync_with_relay(
        1,
        vec![format!("rel://127.0.0.1:{}", relay_server.addr().port())],
        (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            + 3_600)
            .to_string(),
        remote_key,
    )]);
    let addr = spawn_mock(mock).await;

    // the remote peer is already online at the relay → the connector's
    // carrier pump can open the lane right after the first sync
    let pid_remote = PeerId::from_wg_pubkey_string(remote_key);
    let _remote_ws = connect_remote(&relay_server, &pid_remote).await;

    let stub = Arc::new(CarrierStubWg::default());
    let (handle, host) = spawn_connector(addr, stub.clone(), true);

    wait_for(FUSE, || host.serials.lock().expect("serials").contains(&1), "map applied");
    // the production pump: relay Ready → open_conn(remote online) → lane
    // attached to the WG seam
    wait_for(FUSE, || stub.attaches() >= 1, "the carrier pump attached the lane");
    assert_eq!(stub.carrier_count(), 1);
    assert_eq!(handle.relay_carrier_lanes(), Some(1), "lanes observable through the handle");
    // 2 accepted connections: the raw remote + the connector's relay client
    assert_eq!(
        relay_server.stats().connections_accepted,
        2,
        "remote + connector, exactly one relay session each"
    );
    let accepted_before_stop = relay_server.stats().connections_accepted;

    // --- revoke: the connector stop tears client + pump + lanes down
    let stop_json = handle.stop_json();
    assert!(stop_json.contains("\"already_stopped\":false"), "{stop_json}");

    wait_for(FUSE, || handle.status().relay.state == "dead", "relay reaches terminal Dead");
    wait_for(
        FUSE,
        || relay_server.stats().frames_rx(MSG_CLOSE) >= 1,
        "the server observed the relay Close frame",
    );
    assert_eq!(stub.carrier_count(), 0, "no carrier left on the WG seam after stop");
    assert_eq!(handle.relay_carrier_lanes(), Some(0));
    assert_eq!(
        relay_server.stats().connections_accepted,
        accepted_before_stop,
        "no orphan outbound relay connection after stop"
    );
    assert_stays_false(
        Duration::from_millis(500),
        || relay_server.stats().connections_accepted > accepted_before_stop,
        "the torn-down connector must never re-dial the relay",
    );
    let _ = relay_server.shutdown().await;
}

// ---------------------------------------------------------------------------
// 5. relay_enabled=false (hard default): zero relay behavior re-pinned
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn relay_disabled_by_default_never_dials_or_attaches() {
    show_panics();
    let relay_server =
        RelayTestServer::start(TestServerConfig::default()).await.expect("fake relay");
    let remote_key = "ZGlzYWJsZWQtZml4dHVyZS1yZW1vdGUta2V5MDA=";
    let mock = MiniMgmt::default();
    // management DOES advertise relays — the disabled connector must ignore it
    mock.script(vec![sync_with_relay(
        1,
        vec![format!("rel://127.0.0.1:{}", relay_server.addr().port())],
        (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            + 3_600)
            .to_string(),
        remote_key,
    )]);
    let addr = spawn_mock(mock).await;

    let stub = Arc::new(CarrierStubWg::default());
    let (handle, host) = spawn_connector(addr, stub.clone(), false);

    wait_for(FUSE, || host.serials.lock().expect("serials").contains(&1), "map applied");

    let status = handle.status();
    assert!(!status.relay.enabled, "relay must be DISABLED by default");
    assert_eq!(status.relay.state, "disabled");
    assert!(handle.relay_client().is_none(), "no relay client handle when disabled");
    assert!(handle.relay_carrier_stats().is_none(), "no carrier orchestrator when disabled");
    assert!(handle.relay_carrier_lanes().is_none());
    assert_eq!(stub.carrier_count(), 0, "no lane ever attached");
    assert_eq!(stub.attaches(), 0);

    // and NOTHING ever dials the relay server (bounded negative assertion)
    assert_stays_false(
        Duration::from_millis(700),
        || relay_server.stats().connections_accepted > 0,
        "disabled connector must not open any relay connection",
    );
    handle.stop();
    let _ = relay_server.shutdown().await;
}
