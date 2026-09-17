// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N2-H I2 — `WgDevice` TUN frame tee (design
//! `docs/n2h-device-collector-design-20260917.md` §4.1, §7-I2 row, §8).
//!
//! Proven here:
//! - BOTH tee feed points fire: from-os (frames read off the TUN, before
//!   `send_from_tun`'s route decision — an unroutable frame is observed too)
//!   and to-os (decapsulated plaintext written to the TUN, same success
//!   branch as `rx_bytes_to_tun`);
//! - the tee's frame/byte accounting reconciles with the REAL
//!   [`WgDeviceStats`] of the same window: to-os bytes == the
//!   `rx_bytes_to_tun` delta, from-os frames == routed transport frames +
//!   `no_route_drops`, and `tx_bytes` == routed frame bytes + 32 B/frame WG
//!   transport overhead;
//! - the window semantics: frames crossing BEFORE `attach_tun_tee` and AFTER
//!   `detach_tun_tee` are delivered but not observed;
//! - attach/detach is an idempotent handle interface (re-attach replaces and
//!   reports it, detach hands the handle back, a second detach is a no-op);
//! - the `None` (unarmed) path is the status quo: the same deterministic
//!   scenario run with and without a tee yields BYTE-IDENTICAL delivered
//!   frames and an EQUAL full `WgDeviceStats` snapshot — attaching a tee
//!   changes no payload byte and no counter.
//!
//! Harness: two full WireGuard devices in ONE process — real BoringTun
//! tunnels, loopback UDP "cable", and `host_sockets::open_tun_standby_pair`
//! (SOCK_SEQPACKET: one write == one frame == one read, the kernel TUN frame
//! contract; same stand-in as the `n2h.rs` loopback driver). No sleeps
//! anywhere: every wait is a bounded predicate on the INJECTED device clock
//! (loopback delivery inside one process is synchronous), in the shape of the
//! existing `wait_for` helpers. Test keys are explicit synthetic constants.

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

use std::cell::RefCell;
use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine as _;
use boringtun::ffi;

use netbird_core::config;
use netbird_core::connector::{WgPeerApplier, WgPeerEntry};
use netbird_core::host_sockets::{self, FdBag};
use netbird_core::sys;
use netbird_core::tun::TunFd;
use netbird_core::wg_device::{
    TunFrameDir, TunFrameTee, TunTeeCounts, TunTeeTotals, WgDevice, WgDeviceApplier,
    WgDeviceConfig, WgDeviceStats,
};

// ---------------------------------------------------------------------------
// synthetic TEST key material (never real deployment secrets)
// ---------------------------------------------------------------------------

/// Device A test secret: 32 bytes of 0xA5.
const SECRET_A: [u8; 32] = [0xA5; 32];
/// Device B test secret: 32 bytes of 0xB6.
const SECRET_B: [u8; 32] = [0xB6; 32];
/// Tunnel addresses (NetBird-style /32 identities).
const ADDR_A: [u8; 4] = [10, 99, 0, 1];
const ADDR_B: [u8; 4] = [10, 99, 0, 2];

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn key_a() -> String {
    b64(&ffi::x25519_public_key(ffi::x25519_key { key: SECRET_A }).key)
}

fn key_b() -> String {
    b64(&ffi::x25519_public_key(ffi::x25519_key { key: SECRET_B }).key)
}

// ---------------------------------------------------------------------------
// the I2 test tee: a FOREIGN implementation of the seam
// ---------------------------------------------------------------------------

/// Records every observed frame (direction + exact bytes) and delegates the
/// accounting to the built-in [`TunTeeCounts`]. Using a test-side type also
/// proves the seam's point: a collector tee plugs in with NO data-plane edit.
#[derive(Default)]
struct RecordingTee {
    frames: Mutex<Vec<(TunFrameDir, Vec<u8>)>>,
    counts: TunTeeCounts,
}

impl RecordingTee {
    fn new() -> Self {
        RecordingTee::default()
    }

    /// All observed frames in observation order.
    fn frames(&self) -> Vec<(TunFrameDir, Vec<u8>)> {
        self.frames.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Observed frames of ONE direction, in observation order.
    fn dir(&self, dir: TunFrameDir) -> Vec<Vec<u8>> {
        self.frames()
            .into_iter()
            .filter(|(d, _)| *d == dir)
            .map(|(_, f)| f)
            .collect()
    }

    fn totals(&self) -> TunTeeTotals {
        self.counts.totals()
    }
}

impl TunFrameTee for RecordingTee {
    fn on_tun_frame(&self, dir: TunFrameDir, frame: &[u8]) {
        self.frames
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((dir, frame.to_vec()));
        self.counts.on_tun_frame(dir, frame);
    }
}

// ---------------------------------------------------------------------------
// node harness (production applier seam + socketpair TUN stand-in)
// ---------------------------------------------------------------------------

/// One instance: a real `WgDevice` over a loopback UDP socket (its "cable
/// end") and a `host_sockets::open_tun_standby_pair` TUN stand-in. Device
/// access goes through the SAME `WgDeviceApplier` seam the connector uses.
struct Node {
    app: Arc<WgDeviceApplier>,
    /// This node's OWN public key (the peer key registered in the other node).
    key: String,
    /// Bound port of the raw WG socket (the peer's endpoint target).
    wg_port: u16,
    /// TUN stand-in hand end: one write == one frame into the device, one
    /// read == one frame the device delivered.
    tun_hand: i32,
    /// Raw fds this test owns: closed exactly once on drop (the device only
    /// ever touched its own dups — fd contract).
    _bag: FdBag,
}

impl Node {
    fn build(secret: &[u8; 32], peer_key: String, peer_ips: Vec<([u8; 4], u8)>) -> Node {
        let wg_raw = host_sockets::open_udp_ephemeral().expect("wg udp socket");
        let wg_port = host_sockets::udp_bound_port(wg_raw).expect("wg bound port");
        let (tun_raw, tun_hand) =
            host_sockets::open_tun_standby_pair().expect("TUN stand-in socketpair");
        let mut bag = FdBag::new();
        bag.keep(wg_raw);
        bag.keep(tun_raw);
        bag.keep(tun_hand);

        let mut cfg = WgDeviceConfig::new(b64(secret));
        cfg.hs_retry_ms = 100;
        cfg.hs_deadline_ms = 10_000;
        cfg.keepalive_ms = u64::MAX / 2; // never due on the injected clock
        cfg.session_max_ms = u64::MAX / 2; // never expires
        let tun = TunFd::dup_from_raw(tun_raw).expect("tun dup (fd contract)");
        let dev = WgDevice::adopt(cfg, wg_raw, tun).expect("device adopt");
        let app = Arc::new(WgDeviceApplier::new(dev));
        app.apply_peers(&[WgPeerEntry {
            pub_key_b64: peer_key,
            allowed_ips: peer_ips
                .into_iter()
                .map(|(addr, prefix_len)| config::Route { addr, prefix_len })
                .collect(),
        }])
        .expect("apply_peers");
        show_panics();
        Node { app, key: b64(&ffi::x25519_public_key(ffi::x25519_key { key: *secret }).key), wg_port, tun_hand, _bag: bag }
    }

    fn dev<R>(&self, f: impl FnOnce(&mut WgDevice) -> R) -> R {
        self.app.with_device(f)
    }

    fn stats(&self) -> WgDeviceStats {
        self.dev(|d| d.stats())
    }

    fn ready(&self) -> bool {
        self.app.tunnel_ready()
    }

    /// Is a tee attached on this device right now.
    fn armed(&self) -> bool {
        self.dev(|d| d.tun_tee_attached())
    }

    fn attach(&self, tee: Arc<dyn TunFrameTee>) -> bool {
        self.dev(|d| d.attach_tun_tee(tee))
    }

    fn detach(&self) -> bool {
        self.dev(|d| d.detach_tun_tee()).is_some()
    }
}

/// boringtun's ffi installs a process-global panic→SIGSEGV hook on the first
/// tunnel; re-install a PRINTING hook so assertion failures stay diagnosable.
fn show_panics() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        std::panic::set_hook(Box::new(|info| {
            eprintln!("\nTEST PANIC: {info}");
            eprintln!("{:?}", std::backtrace::Backtrace::force_capture());
        }));
    });
}

// ---------------------------------------------------------------------------
// frames, TUN hand ends, injected-clock predicates
// ---------------------------------------------------------------------------

/// One honest IPv4/UDP packet (valid header checksum, recognizable payload).
fn ip_udp_packet(src: [u8; 4], dst: [u8; 4], sport: u16, dport: u16, payload: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(28 + payload.len());
    let total = (20 + 8 + payload.len()) as u16;
    p.extend_from_slice(&[0x45, 0]);
    p.extend_from_slice(&total.to_be_bytes());
    p.extend_from_slice(&[0, 1, 0, 0, 64, 17, 0, 0]);
    p.extend_from_slice(&src);
    p.extend_from_slice(&dst);
    p.extend_from_slice(&sport.to_be_bytes());
    p.extend_from_slice(&dport.to_be_bytes());
    p.extend_from_slice(&((8 + payload.len()) as u16).to_be_bytes());
    p.extend_from_slice(&[0, 0]);
    p.extend_from_slice(payload);
    let mut sum: u32 = 0;
    for w in p[..20].chunks(2) {
        sum += u16::from_be_bytes([w[0], w[1]]) as u32;
    }
    let ck = !(sum + (sum >> 16)) as u16;
    p[10..12].copy_from_slice(&ck.to_be_bytes());
    p
}

fn write_tun(node: &Node, frame: &[u8]) {
    let (n, e) = sys::write_fd(node.tun_hand, frame);
    assert_eq!((n as usize, e), (frame.len(), 0), "TUN-side test write");
}

fn read_tun(node: &Node) -> Option<Vec<u8>> {
    let (ret, _e, rev) = sys::poll1(node.tun_hand, sys::POLLIN, 0);
    if ret <= 0 || (rev & sys::POLLIN) == 0 {
        return None;
    }
    let mut buf = [0u8; 2048];
    let (n, e) = sys::read_fd(node.tun_hand, &mut buf);
    assert!(n > 0, "TUN-side test read errno={e}");
    Some(buf[..n as usize].to_vec())
}

/// One pump step: TUN half + UDP half + tick, all on the INJECTED clock,
/// poll timeout 0 (loopback delivery is synchronous in this process).
fn step(node: &Node, now: u64) {
    node.dev(|d| {
        d.service_tun(now);
        d.service_udp(now);
        d.tick(now);
    });
}

/// Bounded predicate wait on the injected clock — the existing `wait_for`
/// shape: no `sleep`, a fuse that PANICS with the awaited condition. The fuse
/// is a simulated deadline, not a real-time budget.
fn wait_for(
    a: &Node,
    b: &Node,
    now: &mut u64,
    fuse_ms: u64,
    what: &str,
    mut cond: impl FnMut(&Node, &Node) -> bool,
) {
    let deadline = *now + fuse_ms;
    while *now <= deadline {
        step(a, *now);
        step(b, *now);
        if cond(a, b) {
            return;
        }
        *now += 10;
    }
    panic!("timed out (injected clock, fuse {fuse_ms} ms) waiting for {what}");
}

/// Land endpoints both ways and pump until both sessions are established.
fn establish(a: &Node, b: &Node, now: &mut u64) {
    a.dev(|d| d.set_endpoint(b.key.as_str(), [127, 0, 0, 1], b.wg_port, *now))
        .expect("endpoint A→B");
    b.dev(|d| d.set_endpoint(a.key.as_str(), [127, 0, 0, 1], a.wg_port, *now))
        .expect("endpoint B→A");
    wait_for(a, b, now, 10_000, "both WG sessions establish", |a, b| {
        a.ready() && b.ready()
    });
}

/// Push one frame into `a`'s TUN hand end and wait until the SAME bytes fall
/// out of `b`'s TUN hand end (the read IS the consumption, so it happens
/// inside the predicate).
fn deliver(a: &Node, b: &Node, now: &mut u64, frame: &[u8]) -> Vec<u8> {
    write_tun(a, frame);
    let got: RefCell<Option<Vec<u8>>> = RefCell::new(None);
    wait_for(a, b, now, 5_000, "the frame crosses the tunnel and is delivered", |_a, b| {
        if got.borrow().is_none() {
            *got.borrow_mut() = read_tun(b);
        }
        got.borrow().is_some()
    });
    got.into_inner().expect("delivered frame")
}

// ---------------------------------------------------------------------------
// 1. both directions, byte-exact, and the accounting vs WgDeviceStats
// ---------------------------------------------------------------------------

#[test]
fn both_directions_enter_the_tee_and_match_device_stats() {
    let a = Node::build(&SECRET_A, key_b(), vec![(ADDR_B, 32)]);
    let b = Node::build(&SECRET_B, key_a(), vec![(ADDR_A, 32)]);
    let mut now = 1_000u64;
    establish(&a, &b, &mut now);

    // -- window semantics, part 1: a frame BEFORE the attach is delivered on
    //    the wire but NOT observed (frames_scanned covers the window only) --
    let pre = ip_udp_packet(ADDR_A, ADDR_B, 39_000, 39_001, b"pre-attach");
    assert_eq!(deliver(&a, &b, &mut now, &pre), pre, "the pre-attach frame crosses");
    assert!(!a.armed() && !b.armed(), "no tee is attached yet");

    // -- arm both devices; the interface is an idempotent handle ------------
    let a_tee = Arc::new(RecordingTee::new());
    let b_tee = Arc::new(RecordingTee::new());
    assert!(!a.attach(a_tee.clone()), "first attach replaces nothing");
    assert!(a.armed(), "attach must be observable");
    assert!(
        a.attach(Arc::new(TunTeeCounts::new())),
        "a re-attach reports the tee it replaced"
    );
    assert!(a.attach(a_tee.clone()), "…and the recording tee replaces the placeholder");
    assert!(!b.attach(b_tee.clone()));

    // counters at window open: every reconciliation below is a DELTA from
    // here, so handshake-time activity stays out of the window
    let a0 = a.stats();
    let b0 = b.stats();

    // -- from-os: two routed frames A→B, then one unroutable frame ----------
    let f1 = ip_udp_packet(ADDR_A, ADDR_B, 41_000, 41_001, b"a2b-one");
    let f2 = ip_udp_packet(ADDR_A, ADDR_B, 41_002, 41_003, b"a2b-two-larger-payload");
    assert_eq!(deliver(&a, &b, &mut now, &f1), f1, "frame 1 delivered byte-exact");
    assert_eq!(deliver(&a, &b, &mut now, &f2), f2, "frame 2 delivered byte-exact");

    // dst 10.5.5.5 matches no allowed_ips ⇒ no_route_drops, and the tee must
    // still have observed the frame (the §4.1 ordering: feed BEFORE routing)
    let unroutable = ip_udp_packet(ADDR_A, [10, 5, 5, 5], 41_004, 41_005, b"no-route");
    let drop_target = a.stats().no_route_drops + 1;
    write_tun(&a, &unroutable);
    wait_for(&a, &b, &mut now, 5_000, "the unroutable frame is read + counted as a drop", |a, _b| {
        a.stats().no_route_drops == drop_target
    });

    // -- to-os: one frame B→A ----------------------------------------------
    let r1 = ip_udp_packet(ADDR_B, ADDR_A, 42_000, 42_001, b"b2a-return");
    assert_eq!(deliver(&b, &a, &mut now, &r1), r1, "the return frame is delivered");

    // -- BOTH directions are in the tee, byte-exact, in frame order ---------
    assert_eq!(
        a_tee.dir(TunFrameDir::FromOs),
        vec![f1.clone(), f2.clone(), unroutable.clone()],
        "every from-os frame is observed, incl. the unrouted one"
    );
    assert_eq!(a_tee.dir(TunFrameDir::ToOs), vec![r1.clone()], "the to-os delivery is observed");
    assert_eq!(b_tee.dir(TunFrameDir::ToOs), vec![f1.clone(), f2.clone()]);
    assert_eq!(b_tee.dir(TunFrameDir::FromOs), vec![r1.clone()]);

    // -- frame/byte accounting vs the REAL WgDeviceStats --------------------
    let at = a_tee.totals();
    let a1 = a.stats();
    assert_eq!(at.from_os_frames, 3, "2 routed + 1 unroutable from-os frame");
    assert_eq!(at.from_os_bytes, (f1.len() + f2.len() + unroutable.len()) as u64);
    assert_eq!(at.to_os_frames, 1);
    assert_eq!(at.to_os_bytes, r1.len() as u64);
    assert_eq!(
        at.to_os_bytes,
        a1.rx_bytes_to_tun - a0.rx_bytes_to_tun,
        "to-os bytes == rx_bytes_to_tun delta"
    );
    assert_eq!(a1.tun_write_errors - a0.tun_write_errors, 0, "every delivery was written");
    assert_eq!(a1.no_route_drops - a0.no_route_drops, 1);
    assert_eq!(a1.short_frame_drops - a0.short_frame_drops, 0);
    assert_eq!(a1.send_errors - a0.send_errors, 0);

    // every observed from-os frame is accounted for by exactly one outcome:
    // a routed transport datagram (tx_packets minus handshake/keepalive
    // datagrams, which are not TUN-born) or a no-route drop
    let routed_transport = (a1.tx_packets - a0.tx_packets)
        - (a1.handshake_initiations - a0.handshake_initiations)
        - (a1.keepalives_sent - a0.keepalives_sent);
    assert_eq!(routed_transport, 2, "exactly the two routable frames went out");
    assert_eq!(
        routed_transport + (a1.no_route_drops - a0.no_route_drops),
        at.from_os_frames,
        "routed transport frames + no-route drops == from-os frames observed"
    );
    // and the routed bytes carry exactly the +32 B/frame WG transport overhead
    assert_eq!(
        a1.tx_bytes - a0.tx_bytes,
        at.from_os_bytes - unroutable.len() as u64 + 32 * routed_transport,
        "tx_bytes == routed frame bytes + 32 B/frame"
    );

    // the B side mirrors it: one TUN-born frame out, two deliveries in
    let bt = b_tee.totals();
    let b1 = b.stats();
    assert_eq!(bt.from_os_frames, 1);
    assert_eq!(bt.from_os_bytes, r1.len() as u64);
    assert_eq!(bt.to_os_frames, 2);
    assert_eq!(bt.to_os_bytes, (f1.len() + f2.len()) as u64);
    assert_eq!(
        bt.to_os_bytes,
        b1.rx_bytes_to_tun - b0.rx_bytes_to_tun,
        "to-os bytes == rx_bytes_to_tun delta"
    );
    assert_eq!(
        bt.from_os_frames,
        b1.tx_packets - b0.tx_packets,
        "the return frame is B's only TUN-born datagram"
    );
    assert_eq!(b1.tx_bytes - b0.tx_bytes, r1.len() as u64 + 32);
}

// ---------------------------------------------------------------------------
// 2. the None/unarmed path is the status quo, byte for byte
// ---------------------------------------------------------------------------

/// One full deterministic run of the same frame sequence.
struct Run {
    /// Frames read off each TUN hand end (byte-exact, in delivery order).
    a_recv: Vec<Vec<u8>>,
    b_recv: Vec<Vec<u8>>,
    a_stats: WgDeviceStats,
    b_stats: WgDeviceStats,
    /// Armed runs only: (totals at window close, totals after the post-window
    /// frame, frames observed in total).
    window: Option<(TunTeeTotals, TunTeeTotals, usize)>,
}

/// Same scenario, with or without an attached tee: establish, two A→B frames,
/// one B→A frame, one unroutable A frame, then (armed: detach) and one final
/// A→B frame AFTER the window.
fn scenario(armed: bool) -> Run {
    let a = Node::build(&SECRET_A, key_b(), vec![(ADDR_B, 32)]);
    let b = Node::build(&SECRET_B, key_a(), vec![(ADDR_A, 32)]);
    let mut now = 1_000u64;
    establish(&a, &b, &mut now);

    // ONE tee per device (each device has its own observation window)
    let window_tee: Option<Arc<RecordingTee>> = if armed {
        let a_tee = Arc::new(RecordingTee::new());
        let b_tee = Arc::new(RecordingTee::new());
        assert!(!a.attach(a_tee.clone()), "arm A");
        assert!(!b.attach(b_tee), "arm B");
        Some(a_tee)
    } else {
        assert!(!a.armed() && !b.armed(), "the unarmed run holds NO tee (status quo)");
        None
    };

    let f1 = ip_udp_packet(ADDR_A, ADDR_B, 43_000, 43_001, b"one");
    let f2 = ip_udp_packet(ADDR_A, ADDR_B, 43_002, 43_003, b"two-with-more-bytes");
    let r1 = ip_udp_packet(ADDR_B, ADDR_A, 43_004, 43_005, b"return");
    let unroutable = ip_udp_packet(ADDR_A, [10, 5, 5, 5], 43_006, 43_007, b"no-route");
    let post = ip_udp_packet(ADDR_A, ADDR_B, 43_008, 43_009, b"after-the-window");

    let b_recv = vec![
        deliver(&a, &b, &mut now, &f1),
        deliver(&a, &b, &mut now, &f2),
    ];
    let a_recv = vec![deliver(&b, &a, &mut now, &r1)];
    let drop_target = a.stats().no_route_drops + 1;
    write_tun(&a, &unroutable);
    wait_for(&a, &b, &mut now, 5_000, "the unroutable frame is dropped", |a, _b| {
        a.stats().no_route_drops == drop_target
    });

    // window close: detach is part of the I2 interface
    let at_close = window_tee.as_ref().map(|tee| {
        let totals = tee.totals();
        assert!(a.detach(), "detach hands the tee back");
        assert!(!a.armed(), "detach clears the handle");
        assert!(!a.detach(), "a second detach is a no-op");
        (totals, tee.frames().len())
    });

    // AFTER the window: delivered identically in both runs, never observed
    let mut b_recv = b_recv;
    b_recv.push(deliver(&a, &b, &mut now, &post));

    let window = at_close.map(|(totals, observed)| {
        let tee = window_tee.as_ref().expect("armed run has a tee");
        (totals, tee.totals(), observed)
    });
    Run { a_recv, b_recv, a_stats: a.stats(), b_stats: b.stats(), window }
}

#[test]
fn unarmed_path_is_byte_for_byte_the_status_quo() {
    let plain = scenario(false);
    let armed = scenario(true);

    // 1. the delivered bytes are IDENTICAL, frame for frame — an attached tee
    //    does not alter one payload byte in either direction
    assert_eq!(armed.a_recv, plain.a_recv, "A's delivered frames are identical");
    assert_eq!(armed.b_recv, plain.b_recv, "B's delivered frames are identical");

    // 2. the FULL counter snapshots are EQUAL — the tee adds no counter and
    //    perturbs no existing one
    assert_eq!(armed.a_stats, plain.a_stats, "WgDeviceStats at A unchanged");
    assert_eq!(armed.b_stats, plain.b_stats, "WgDeviceStats at B unchanged");

    // 3. the unarmed run held no tee at all: `None` IS the status-quo path
    assert!(plain.window.is_none(), "no tee was ever attached in the unarmed run");

    // 4. and the armed run observed exactly the window frames: 2 A→B + 1 B→A
    //    + 1 unroutable, while the post-detach frame stayed invisible
    let (at_close, after, observed) = armed.window.expect("the armed run has a tee");
    assert_eq!(observed, 4, "every frame inside the window was observed");
    assert_eq!(at_close.from_os_frames, 3, "2 routed + 1 unroutable");
    assert_eq!(at_close.to_os_frames, 1, "the return delivery");
    assert_eq!(after, at_close, "the frame after detach was NOT observed");
}
