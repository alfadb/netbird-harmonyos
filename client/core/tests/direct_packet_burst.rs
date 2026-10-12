// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! Direct-path packet-burst characterization: how many packets ONE bounded
//! pump tick moves at the surviving drain sites, and the floor the runtime fix
//! must reach. All loopback, all on an INJECTED clock — no sleeps (the only wait
//! is a bounded poll-timeout fuse around loopback delivery), no privileged TUN,
//! no root.
//!
//! **STAGE 4 (official-alignment audit §2.7)** deleted the ICE half of this
//! file: the per-session ICE socket drain (`IceSession::drain_socket` /
//! `RECV_BUDGET_PER_RUN` / `DATA_RX_CAP` / `take_data_rx`) no longer exists — the
//! whole ICE + WireGuard data plane now rides the ONE shared mux over the WG
//! outer socket, whose per-pass bound is `ice_mux::MUX_DRAIN_BUDGET`. What is
//! left here is the DEVICE half, which is unchanged and still the production
//! shape:
//! - `wg_device::DRAIN_MAX` — frames per `service_tun` and datagrams per
//!   `service_udp`, one call of each per tick (pre-fix 8, current tree 64).
//! - `connector::ICE_PUMP_TICK_MS` / `wg_device::WG_PUMP_TICK_MS` — the pump
//!   cadence (pre-fix 100 ms = 10 ticks/s; current tree 10 ms = 100 ticks/s),
//!   still pinned as a bounded tick.
//!
//! Bound arithmetic (device half) — a BOUND, not a measured throughput:
//!   baseline 100 ms,  8 pkt/tick :    80 pkt/s × 1280 B =  102 400 B/s ≈ 0.82 Mbit/s
//!   desired  100 ms, 32 pkt/tick :   320 pkt/s × 1280 B =  409 600 B/s ≈ 3.28 Mbit/s
//!   desired  100 ms, 64 pkt/tick :   640 pkt/s × 1280 B =  819 200 B/s ≈ 6.55 Mbit/s
//!   landed    10 ms, 64 pkt/tick : 6 400 pkt/s × 1280 B = 8 192 000 B/s ≈ 65.5 Mbit/s
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

use std::net::UdpSocket;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use netbird_core::connector::ICE_PUMP_TICK_MS;
use netbird_core::ice_session::{is_wg_datagram, WG_MIN_MSG_SIZE};
use netbird_core::sys;
use netbird_core::tun::TunFd;
use netbird_core::wg_device::{TunFrameDir, TunFrameTee, WgDevice, WgDeviceConfig, WG_PUMP_TICK_MS};

extern "C" {
    fn close(fd: i32) -> i32;
    fn socketpair(domain: i32, ty: i32, protocol: i32, sv: *mut [i32; 2]) -> i32;
}

/// Current per-tick drain at the device sites BEFORE the fix (`for _ in 0..8`,
/// `DRAIN_MAX`, src/wg_device.rs:200) — the number the baseline measures and the
/// RED counts report.
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
