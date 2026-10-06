// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! Direct-path packet-burst characterization: how many WireGuard-shaped
//! datagrams ONE bounded pump tick moves at each of the three proven drain
//! sites, and the floor the runtime fix must reach. All loopback, all on an
//! INJECTED clock — no sleeps (the only wait is a bounded poll-timeout fuse
//! around loopback delivery), no privileged TUN, no root.
//!
//! Constants this file reads back (not re-derived). The PRE-fix values are
//! exactly what git HEAD 658f31c carries; the runtime fix that landed in the
//! working tree (docs/direct-throughput-fix.md) raises the per-tick drain and
//! lowers the pump cadence. Both live constants are read from the crate here,
//! never hardcoded:
//! - `connector::ICE_PUMP_TICK_MS` / `wg_device::WG_PUMP_TICK_MS` — the pump
//!   cadence (pre-fix 100 ms = 10 ticks/s; current tree 10 ms = 100 ticks/s).
//! - `IceSession::drain_socket` — pre-fix `for _ in 0..8`: at most 8 datagrams
//!   per local socket per `run_once`; the landed fix replaced it with ONE
//!   `RECV_BUDGET_PER_RUN` shared by all local sockets, clamped to the queue's
//!   free room. Each tick calls `take_data_rx` once.
//! - `IceSession::DATA_RX_CAP = 64` — the FINITE global queue budget; overflow
//!   drops the NEWEST datagram and counts it in `data_dropped()`
//!   (`push_data_rx`) — never silent loss.
//! - `wg_device::DRAIN_MAX` — frames per `service_tun` and datagrams per
//!   `service_udp`, one call of each per tick (pre-fix 8, current tree 64).
//!
//! Bound arithmetic — a BOUND, not a measured throughput (the per-frame size,
//! 1420 B, the kernel socket buffers and the loopback path all sit between the
//! two). `N` per tick at cadence `T` gives `N/T` pkt/s ⇒ `N/T × 1420 × 8` bit/s:
//!   baseline 100 ms,  8 pkt/tick :    80 pkt/s × 1420 B =  113 600 B/s ≈ 0.91 Mbit/s
//!   desired  100 ms, 32 pkt/tick :   320 pkt/s × 1420 B =  454 400 B/s ≈ 3.64 Mbit/s
//!   desired  100 ms, 64 pkt/tick :   640 pkt/s × 1420 B =  908 800 B/s ≈ 7.27 Mbit/s
//!   landed    10 ms, 64 pkt/tick : 6 400 pkt/s × 1420 B = 9 088 000 B/s ≈ 72.7 Mbit/s
//! (per local socket for the ICE half; `service_tun`/`service_udp` each have
//! their own per-tick cap on the device half.)
//!
//! Test roles:
//! - `baseline_*` documents the PRE-fix shape and is written to stay green on
//!   either side of the fix ([8,8,4] before, [20,0,0] after a ≥20 burst cap).
//! - `desired_*` asserts the POST-fix floor. Against the pre-fix binary they
//!   are RED at 8 per tick (see the run log); against the current tree they
//!   are green because the fix is already in.
//!
//! The TUN half uses the existing injectable seam (`TunFd` over an AF_UNIX
//! SOCK_DGRAM socketpair, the tests/wg_e2e.rs pattern) and the existing
//! recording writer (`TunFrameTee`) — no privileged TUN device, no root.

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
    pub extern "C" fn napi_get_value_int32(_env: *mut c_void, _value: *mut c_void, _result: *mut i32) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(_env: *mut c_void, _value: *mut c_void, _result: *mut bool) -> i32 {
        0
    }
}

use std::net::UdpSocket;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use netbird_core::connector::ICE_PUMP_TICK_MS;
use netbird_core::ice::{Candidate, ProtectedUdpFdSource};
use netbird_core::ice_session::{
    is_wg_datagram, IceCredentials, IceSession, WG_MIN_MSG_SIZE,
};
use netbird_core::sys;
use netbird_core::tun::TunFd;
use netbird_core::wg_device::{TunFrameDir, TunFrameTee, WgDevice, WgDeviceConfig, WG_PUMP_TICK_MS};

extern "C" {
    fn socket(domain: i32, ty: i32, protocol: i32) -> i32;
    fn close(fd: i32) -> i32;
    fn socketpair(domain: i32, ty: i32, protocol: i32, sv: *mut [i32; 2]) -> i32;
}

/// `IceSession::DATA_RX_CAP` (src/ice_session.rs:781) — private in the crate,
/// pinned here so the finite budget assertion is explicit.
const DATA_RX_CAP: usize = 64;
/// Current per-tick drain at every site BEFORE the fix (`for _ in 0..8`,
/// src/ice_session.rs:1414; `DRAIN_MAX`, src/wg_device.rs:200) — the number the
/// baseline measures and the RED counts report.
const BASELINE_TICK_DRAIN: usize = 8;
/// Bounded readiness fuse for loopback delivery (see [`fence_readable`]).
const DELIVERY_FUSE_MS: u64 = 2_000;

// ---------------------------------------------------------------------------
// wire shapes
// ---------------------------------------------------------------------------

/// A WG-shaped datagram: little-endian type 4 (transport data) in a packet of
/// at least `WG_MIN_MSG_SIZE`, with a test-unique receiver index at bytes 4..8
/// (the upstream `isWireGuardMsg` shape, ice_bind.go:403-421).
fn wg_like(id: u32) -> Vec<u8> {
    let mut p = vec![0u8; WG_MIN_MSG_SIZE];
    p[0..4].copy_from_slice(&4u32.to_le_bytes());
    p[4..8].copy_from_slice(&id.to_le_bytes());
    p[8..12].copy_from_slice(&0x5151_5151u32.to_le_bytes());
    p
}

/// The test identity carried by [`wg_like`].
fn id_of(datagram: &[u8]) -> u32 {
    u32::from_le_bytes([datagram[4], datagram[5], datagram[6], datagram[7]])
}

/// One honest IPv4/UDP TUN frame with a test-unique payload (inner checksums
/// are not validated by WireGuard; the existing harnesses leave them zero).
fn tun_frame(id: u32) -> Vec<u8> {
    let mut f = Vec::with_capacity(32);
    let total: u16 = 20 + 8 + 4;
    f.push(0x45);
    f.push(0);
    f.extend_from_slice(&total.to_be_bytes());
    f.extend_from_slice(&[0, 1, 0, 0, 64, 17, 0, 0]); // id, flags, ttl, proto UDP, cksum
    f.extend_from_slice(&[10, 99, 0, 9]); // src
    f.extend_from_slice(&[10, 99, 0, 2]); // dst (no peer registered)
    f.extend_from_slice(&5555u16.to_be_bytes());
    f.extend_from_slice(&5556u16.to_be_bytes());
    f.extend_from_slice(&(8u16 + 4).to_be_bytes());
    f.extend_from_slice(&[0, 0]); // udp checksum
    f.extend_from_slice(&id.to_be_bytes());
    f
}

// ---------------------------------------------------------------------------
// loopback / fd helpers
// ---------------------------------------------------------------------------

/// Bounded readiness fuse. Loopback delivery happens inside the sending
/// syscall in this process, so this returns on the first `poll`. 50 ms slices,
/// `DELIVERY_FUSE_MS` ceiling: a datagram that never lands is a hard failure,
/// never a slow test.
fn fence_readable(fds: &[i32]) {
    let start = sys::mono_ms();
    for (i, &fd) in fds.iter().enumerate() {
        loop {
            let (ret, _e, rev) = sys::poll1(fd, sys::POLLIN, 50);
            if ret > 0 && (rev & sys::POLLIN) != 0 {
                break;
            }
            assert!(
                sys::mono_ms().saturating_sub(start) < DELIVERY_FUSE_MS,
                "socket #{i} (fd {fd}) never became readable within the bounded network timeout"
            );
        }
    }
}

fn any_readable(fds: &[i32]) -> bool {
    fds.iter().any(|&fd| {
        let (ret, _e, rev) = sys::poll1(fd, sys::POLLIN, 0);
        ret > 0 && (rev & sys::POLLIN) != 0
    })
}

/// Bound loopback UDP socket; returns (raw fd, bound port).
fn udp_socket_lo() -> (i32, u16) {
    let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
    assert!(fd >= 0, "socket() failed errno={}", sys::errno());
    let sa = sys::sockaddr_in::new([127, 0, 0, 1], 0);
    assert_eq!(
        unsafe { sys::bind(fd, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) },
        0,
        "bind() failed errno={}",
        sys::errno()
    );
    let mut name = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut len = core::mem::size_of::<sys::sockaddr_in>() as u32;
    assert_eq!(unsafe { sys::getsockname(fd, &mut name, &mut len) }, 0);
    (fd, u16::from_be(name.sin_port))
}

/// Datagram socketpair: end 0 plays the platform TUN fd (the device dups it),
/// end 1 is the test hand (one write == one frame).
fn tun_socketpair() -> (i32, i32) {
    let mut sv = [-1i32; 2];
    assert_eq!(
        unsafe { socketpair(1 /* AF_UNIX */, 2 /* SOCK_DGRAM */, 0, &mut sv) },
        0,
        "socketpair failed errno={}",
        sys::errno()
    );
    (sv[0], sv[1])
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn assert_exact_ids(mut got: Vec<u32>, want: std::ops::Range<u32>, ctx: &str) {
    got.sort_unstable();
    let mut expected: Vec<u32> = want.collect();
    expected.sort_unstable();
    assert_eq!(got, expected, "{ctx}");
}

// ---------------------------------------------------------------------------
// ICE half: IceSession over real loopback sockets
// ---------------------------------------------------------------------------

struct IceBurst {
    session: IceSession,
    raw_fds: Vec<i32>,
    ports: Vec<u16>,
    sender: UdpSocket,
}

impl Drop for IceBurst {
    fn drop(&mut self) {
        for fd in self.raw_fds.drain(..) {
            unsafe { close(fd) };
        }
    }
}

impl IceBurst {
    /// One session with `local_socks` real loopback candidate sockets, started
    /// against a bound-but-never-read remote candidate (the required remote
    /// credentials/candidate; the checks it receives are irrelevant here).
    fn new(local_socks: usize) -> Self {
        let local = IceCredentials {
            ufrag: "AAAAAAAAAAAAAAAA".into(),
            pwd: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef".into(),
        };
        let remote = IceCredentials {
            ufrag: "BBBBBBBBBBBBBBBB".into(),
            pwd: "ABCDEFGHIJKLMNOPQRSTUVWXYZghijkl".into(),
        };
        let mut session = IceSession::new(local, true, Some(0x5151)).expect("ice session");
        let provider = ProtectedUdpFdSource::new_with_fd(-1).expect("protected fd source");
        let mut raw_fds = Vec::new();
        let mut ports = Vec::new();
        for _ in 0..local_socks {
            let fd = unsafe { socket(2 /* AF_INET */, 2 /* SOCK_DGRAM */, 0) };
            assert!(fd >= 0, "socket() failed errno={}", sys::errno());
            raw_fds.push(fd);
            provider.feed(fd).expect("feed protected fd");
            let bound = session
                .add_local_candidate(Candidate::host_candidate([127, 0, 0, 1], 0), &provider)
                .expect("bind local candidate");
            ports.push(bound.port);
        }
        let sender = UdpSocket::bind("127.0.0.1:0").expect("sender bind");
        let sender_port = sender.local_addr().expect("sender addr").port();
        session.add_remote_candidate(Candidate::host_candidate([127, 0, 0, 1], sender_port));
        session.set_remote_credentials(remote).expect("remote credentials");
        session.start().expect("session start");
        IceBurst { session, raw_fds, ports, sender }
    }

    /// Queue `count` WG-shaped datagrams on local socket `sock_idx`, ids
    /// `first_id..first_id+count`.
    fn send_burst(&self, sock_idx: usize, count: u32, first_id: u32) {
        let port = self.ports[sock_idx];
        for i in 0..count {
            self.sender
                .send_to(&wg_like(first_id + i), ("127.0.0.1", port))
                .expect("send_to loopback candidate");
        }
    }

    /// One connector-scheduled ICE pump tick.
    fn tick(&mut self, now_ms: u64) {
        self.session.run_once(now_ms).expect("ice pump");
    }

    fn take_ids(&mut self) -> Vec<u32> {
        self.session.take_data_rx().into_iter().map(|(_, _, d)| id_of(&d)).collect()
    }
}

/// The production pump ticks this file's bound arithmetic is computed from.
/// The pump cadence this file's bound arithmetic is computed from must stay a
/// bounded tick (the design bound is 100 ms; the landed fix runs 10 ms), and
/// both pumps must be a real cadence — not 0 (busy loop) and not minutes.
#[test]
fn pump_cadence_is_a_bounded_tick() {
    assert!(
        (1..=100).contains(&ICE_PUMP_TICK_MS),
        "connector ICE pump cadence must stay a bounded tick; got {ICE_PUMP_TICK_MS}"
    );
    assert!(
        (1..=100).contains(&WG_PUMP_TICK_MS),
        "wg_device data-plane pump cadence must stay a bounded tick; got {WG_PUMP_TICK_MS}"
    );
}

/// The burst really takes the WG demux branch (not the STUN machinery).
#[test]
fn harness_datagrams_are_wg_shaped_not_stun() {
    let d = wg_like(7);
    assert_eq!(d.len(), WG_MIN_MSG_SIZE, "an empty-payload WG transport packet is 32 bytes");
    assert!(
        is_wg_datagram(&d),
        "the burst must take the WG demux branch (isWireGuardMsg), not the STUN parser"
    );
    assert_ne!(d[4..8], [0x21, 0x12, 0xA4, 0x42], "must not carry the STUN magic cookie");
}

/// BASELINE (green today): 20 WG-shaped datagrams queued on one local socket
/// are NOT dropped — one bounded ICE pump tick drains 8, the next 8, the third
/// 4. Written to accept the post-fix single-tick shape too (see the assert).
#[test]
fn baseline_twenty_packet_burst_is_spread_over_three_bounded_ticks() {
    let mut ice = IceBurst::new(1);
    ice.send_burst(0, 20, 0);
    fence_readable(&ice.raw_fds);

    let mut shape = Vec::new();
    let mut ids = Vec::new();
    for t in 0..3u64 {
        ice.tick(1000 + t * ICE_PUMP_TICK_MS);
        let tick_ids = ice.take_ids();
        shape.push(tick_ids.len());
        ids.extend(tick_ids);
    }
    assert_eq!(shape.iter().sum::<usize>(), 20, "all 20 datagrams must be delivered");
    assert_eq!(
        ice.session.data_dropped(),
        0,
        "the pre-fix per-tick cap loses NO user datagram — it spreads the burst over ticks"
    );
    assert!(
        shape == vec![8, 8, 4] || shape == vec![20, 0, 0],
        "20-datagram burst shape changed: {shape:?} (pre-fix [8,8,4]; post-fix, with a burst cap >= 20, [20,0,0])"
    );
    assert_exact_ids(ids, 0..20, "every datagram of the burst arrives exactly once");
}

/// DESIRED (RED today: 8): one bounded ICE pump tick must drain a whole
/// 32-datagram burst on one local socket, without packet loss.
#[test]
fn desired_one_bounded_tick_drains_32_packet_burst() {
    let mut ice = IceBurst::new(1);
    ice.send_burst(0, 32, 0);
    fence_readable(&ice.raw_fds);

    ice.tick(1000);
    let ids = ice.take_ids();
    assert_eq!(
        ice.session.data_dropped(),
        0,
        "a 32-datagram burst fits the finite global queue budget ({DATA_RX_CAP}): it must not be dropped"
    );
    assert_exact_ids(
        ids,
        0..32,
        "one bounded ICE pump tick must drain the whole 32-datagram burst without loss \
         (current per-socket cap drains 8 per tick)",
    );
}

/// DESIRED (RED today: 8): the same at the full finite global queue budget —
/// one bounded tick drains 64 datagrams on one local socket.
#[test]
fn desired_one_bounded_tick_drains_64_packet_burst() {
    let mut ice = IceBurst::new(1);
    ice.send_burst(0, 64, 0);
    fence_readable(&ice.raw_fds);

    ice.tick(1000);
    let ids = ice.take_ids();
    assert_eq!(
        ice.session.data_dropped(),
        0,
        "64 datagrams exactly fill the finite global queue budget ({DATA_RX_CAP}): the newest must not be dropped"
    );
    assert_exact_ids(
        ids,
        0..64,
        "one bounded ICE pump tick must drain a 64-datagram burst (= DATA_RX_CAP) without loss \
         (current per-socket cap drains 8 per tick)",
    );
}

/// DESIRED (RED today: 8+8=16 across two sockets): with MULTIPLE local sockets
/// readable, one bounded tick must move a real burst — at least 32 datagrams
/// in total — while the FINITE global queue budget (64) is never exceeded and
/// every offered datagram is either queued or counted (`data_dropped`), never
/// silently lost.
#[test]
fn desired_multi_socket_burst_respects_finite_global_queue_budget() {
    const PER_SOCKET: u32 = 40;
    const OFFERED: usize = 2 * PER_SOCKET as usize; // 80 > DATA_RX_CAP

    let mut ice = IceBurst::new(2);
    ice.send_burst(0, PER_SOCKET, 0);
    ice.send_burst(1, PER_SOCKET, PER_SOCKET);
    fence_readable(&ice.raw_fds);

    let mut seen: Vec<u32> = Vec::new();
    let mut first_tick = 0usize;
    let mut exhausted = false;
    for t in 0..(OFFERED / BASELINE_TICK_DRAIN + 1) as u64 {
        ice.tick(1000 + t * ICE_PUMP_TICK_MS);
        let ids = ice.take_ids();
        assert!(
            ids.len() <= DATA_RX_CAP,
            "tick {t} queued {} datagrams — the finite global budget ({DATA_RX_CAP}) must never be exceeded",
            ids.len()
        );
        if t == 0 {
            first_tick = ids.len();
        }
        seen.extend(ids);
        if !any_readable(&ice.raw_fds) {
            exhausted = true;
            break;
        }
    }
    let dropped = ice.session.data_dropped() as usize;

    assert!(
        first_tick >= 32,
        "one bounded tick across 2 readable local sockets must drain >= 32 datagrams in total; \
         the current 8-per-socket cap drains 8+8 = {} (RED)",
        first_tick
    );
    assert!(
        exhausted,
        "the offered burst must be drained-or-counted within the bounded tick budget"
    );
    assert_eq!(
        seen.len() + dropped,
        OFFERED,
        "every offered datagram is either queued or counted in data_dropped ({dropped}) — never silent loss"
    );
    let mut uniq = seen.clone();
    uniq.sort_unstable();
    uniq.dedup();
    assert_eq!(uniq.len(), seen.len(), "no datagram may be delivered twice");
    for id in &uniq {
        assert!(*id < OFFERED as u32, "unknown datagram id {id} (not part of the offered burst)");
    }
}

// ---------------------------------------------------------------------------
// device half: WgDevice over a real loopback WG socket + injectable TUN
// ---------------------------------------------------------------------------

/// The existing recording writer seam: captures every frame the device hands
/// off in the from-OS direction, in order.
#[derive(Default)]
struct RecordingTee {
    seen: Mutex<Vec<Vec<u8>>>,
}

impl RecordingTee {
    fn from_os(&self) -> Vec<Vec<u8>> {
        self.seen.lock().expect("tee lock").clone()
    }
}

impl TunFrameTee for RecordingTee {
    fn on_tun_frame(&self, dir: TunFrameDir, frame: &[u8]) {
        if dir == TunFrameDir::FromOs {
            self.seen.lock().expect("tee lock").push(frame.to_vec());
        }
    }
}

struct WgBurst {
    dev: WgDevice,
    wg_raw: i32,
    wg_port: u16,
    tun_raw: i32,
    tun_end: i32,
    tee: Arc<RecordingTee>,
    sender: UdpSocket,
}

impl Drop for WgBurst {
    fn drop(&mut self) {
        for fd in [self.wg_raw, self.tun_raw, self.tun_end] {
            unsafe { close(fd) };
        }
    }
}

impl WgBurst {
    fn new() -> Self {
        let (wg_raw, wg_port) = udp_socket_lo();
        let (tun_raw, tun_end) = tun_socketpair();
        let tun = TunFd::dup_from_raw(tun_raw).expect("tun dup");
        let mut cfg = WgDeviceConfig::new(b64(&[7u8; 32]));
        cfg.keepalive_ms = u64::MAX / 2; // timers never due in these tests
        cfg.session_max_ms = u64::MAX / 2;
        let mut dev = WgDevice::adopt(cfg, wg_raw, tun).expect("device adopt");
        let tee = Arc::new(RecordingTee::default());
        let dyn_tee: Arc<dyn TunFrameTee> = tee.clone();
        dev.attach_tun_tee(dyn_tee);
        WgBurst {
            dev,
            wg_raw,
            wg_port,
            tun_raw,
            tun_end,
            tee,
            sender: UdpSocket::bind("127.0.0.1:0").expect("sender bind"),
        }
    }

    /// Queue `count` TUN frames on the platform side (one write == one frame).
    fn write_tun_burst(&self, count: u32) {
        for i in 0..count {
            let f = tun_frame(i);
            let (n, e) = sys::write_fd(self.tun_end, &f);
            assert_eq!((n as usize, e), (f.len(), 0), "TUN-side test write");
        }
    }

    /// Queue `count` WG-shaped datagrams on the device's outer UDP socket.
    fn send_wg_burst(&self, count: u32) {
        for i in 0..count {
            self.sender
                .send_to(&wg_like(i), ("127.0.0.1", self.wg_port))
                .expect("send_to device socket");
        }
    }
}

/// DESIRED (RED today: 8): one bounded `service_tun` call must drain a whole
/// 32- and 64-frame TUN burst; the recording tee proves every drained frame is
/// observed exactly once, in order, with no duplication.
#[test]
fn desired_one_bounded_service_tun_tick_drains_the_frame_burst() {
    for burst in [32u32, 64] {
        let mut wg = WgBurst::new();
        wg.write_tun_burst(burst);

        let n = wg.dev.service_tun(1000);
        let seen = wg.tee.from_os();

        assert_eq!(
            n as u32, burst,
            "one bounded service_tun tick must drain the whole {burst}-frame TUN burst \
             (pre-fix `DRAIN_MAX` = {BASELINE_TICK_DRAIN} drains 8)"
        );
        assert_eq!(
            seen.len() as u32,
            burst,
            "the recording tee must observe every drained frame exactly once"
        );
        assert_eq!(
            seen,
            (0..burst).map(tun_frame).collect::<Vec<_>>(),
            "the drained frames must be the offered frames, in order, without duplication"
        );
    }
}

/// DESIRED (RED today: 8): one bounded `service_udp` call must drain a whole
/// 32- and 64-datagram burst off the device's outer UDP socket.
#[test]
fn desired_one_bounded_service_udp_tick_drains_the_datagram_burst() {
    for burst in [32u32, 64] {
        let mut wg = WgBurst::new();
        wg.send_wg_burst(burst);
        fence_readable(&[wg.wg_raw]);

        let n = wg.dev.service_udp(1000);
        assert_eq!(
            n as u32, burst,
            "one bounded service_udp tick must drain the whole {burst}-datagram burst \
             (pre-fix `DRAIN_MAX` = {BASELINE_TICK_DRAIN} drains 8)"
        );
    }
}
