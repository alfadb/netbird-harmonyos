// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N2 first batch C/D columns host integration (2026-09-29 revision; no device,
//! no network beyond loopback).
//!
//! What this file proves through the SHIPPED surfaces (the connector JSON
//! bridges the shell calls, the armed `N2DestinationTee`, and the socket-trace
//! call sites that produce the `N2_SOCK` receipts):
//!
//! 1. **The pre/post HITs come from the REAL armed tee.**
//!    `N2DestinationTee::consume_control` is the only thing that can mark a
//!    pre/post HIT; a shell claim of `hit` without it is
//!    `unsat|pre-hit-unobserved`, and the production-recovery gate stays shut.
//! 2. **The receipt is fed by the CORE, not by the shell.** A new network
//!    registration completes its `connect` (refused `connect-only`) and its
//!    first BUSINESS write is accepted as the receipt — from the socket-trace
//!    call site's own arguments, with no shell report in the path at all. The
//!    `Wb` row takes its substitute receipt from the new generation's
//!    `protect_settle`, likewise fed by the core.
//! 3. **The D column is pre-set on Wi-Fi** and advances
//!    `plan → prepare → ready → switch_identified`, with the preparation hop
//!    never counted as the D measurement and the minimum dwell enforced.
//! 4. **A missing receipt times out** on the plan's own written bound, and the
//!    named refusal is recorded on the `N2_CD_SANDWICH` line.
//! 5. **Unarmed is a silent no-op** and the ring file stays inert.
//!
//! Everything is diagnostic: every line ends `|diag=true|is_evidence=false`,
//! and nothing here is N2/N6 evidence (the campaign needs the separately
//! authorized AUTH).

use std::net::UdpSocket;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::sync::{Mutex, MutexGuard, PoisonError};

use netbird_core::connector;
use netbird_core::n2_cd::{self, Identity};
use netbird_core::n2_ring;
use netbird_core::n2_socket_trace;
use netbird_core::sys;
use netbird_core::tun::TunFd;
use netbird_core::wg_device::{N2DestinationTee, WgDevice, WgDeviceConfig};

// Host-process link stubs (repo convention, cf. tests/n2_log_samples.rs):
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

/// The N2 modules share process-global arming state; one lock per test binary.
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

fn disarm() {
    n2_socket_trace::arm(false);
    n2_cd::reset();
}

fn arm_clean() {
    disarm();
    n2_socket_trace::arm(true);
}

/// A real IPv4/UDP frame carrying `payload` to `dst:dport` — the shape the
/// destination tee scans.
fn udp_frame(dst: [u8; 4], sport: u16, dport: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 20 + 8 + payload.len()];
    out[0] = 0x45;
    let len = out.len() as u16;
    out[2..4].copy_from_slice(&len.to_be_bytes());
    out[8] = 64;
    out[9] = 17; // UDP
    out[12..16].copy_from_slice(&[10, 0, 0, 1]);
    out[16..20].copy_from_slice(&dst);
    out[20..22].copy_from_slice(&sport.to_be_bytes());
    out[22..24].copy_from_slice(&dport.to_be_bytes());
    out[24..26].copy_from_slice(&((8 + payload.len()) as u16).to_be_bytes());
    out[28..].copy_from_slice(payload);
    out
}

/// One live outer socket of `kind`, registered EXACTLY as the production path
/// registers one: the shell reports its own protected fd, the core takes its own
/// dup through the real seam (`mgmtsock::dup_socket_fd`, which mirrors the
/// registration onto the dup) and the completion emits the `N2_SOCK` receipt.
///
/// [`identity`](LiveSocket::identity) is therefore the tuple the shell reads off
/// that receipt — `fd=` shell original, `dup_fd=` core-owned descriptor,
/// `protect_generation=` — which is exactly what the switch bridge must carry as
/// the OLD registration's identity.
struct LiveSocket {
    _original: UdpSocket,
    _dup: OwnedFd,
    fd: i32,
    dup_fd: i32,
    generation: u64,
}

impl LiveSocket {
    fn new(kind: &str, generation: u64, net_id: u64, remote: &str) -> LiveSocket {
        let original = UdpSocket::bind("127.0.0.1:0").expect("bind");
        let fd = original.as_raw_fd();
        n2_socket_trace::protect(fd, kind, "42", generation, net_id, 3, true);
        let dup_fd = netbird_core::mgmtsock::dup_socket_fd(fd).expect("protected dup");
        // SAFETY: `dup_socket_fd` returned a fresh owned descriptor.
        let dup = unsafe { OwnedFd::from_raw_fd(dup_fd) };
        n2_socket_trace::first_io(dup_fd, remote.parse().expect("remote"), "connected", "connect");
        LiveSocket { _original: original, _dup: dup, fd, dup_fd, generation }
    }

    fn identity(&self) -> Identity {
        Identity::new(self.fd, self.dup_fd, self.generation)
    }

    /// Complete one business write after the connect — the `M`/`S`/`R` row's
    /// production receipt.
    fn write(&self, remote: &str, bytes: usize) -> Option<String> {
        n2_socket_trace::post_connect_write(self.dup_fd, remote.parse().expect("remote"), bytes)
    }
}

impl Drop for LiveSocket {
    fn drop(&mut self) {
        n2_socket_trace::closed(self.dup_fd);
    }
}

/// A registration observed by the armed tee: register the marker on the frozen
/// destination and hand the matching frame to `consume_control` (the TUN read
/// loop's own call site).
fn tee_hit(tee: &N2DestinationTee, dst: [u8; 4], dport: u16, marker: &str, sport: u16) -> bool {
    let registered = tee.register_control(marker, dst, dport, 17, 0, sys::mono_ms());
    assert!(registered.is_ok(), "{registered:?}");
    let frame = udp_frame(dst, sport, dport, format!("N2CTRL:{marker}").as_bytes());
    tee.consume_control(&frame, sys::mono_ms())
}

/// Wait until the monotonic clock is STRICTLY later than `after`.
///
/// The receipt verdict requires `io_mono_ms > pre_hit_ms` (never relaxed), and
/// the host clock ticks in whole milliseconds: a receipt measured in the same
/// tick as the pre HIT must be refused. This waits for the next tick instead of
/// inventing a timestamp — the ordering the production seconds-long gap gives
/// for free.
fn tick_past(after: u64) {
    while sys::mono_ms() <= after {}
}

/// The shell half of a `Wb` base socket: a real, bound UDP socket registered
/// through the SHIPPED armed protect call (`n2_socket_trace::protect`, the call
/// the ArkTS protect wrapper makes). The caller keeps the socket alive; this
/// alone is NOT a live registration.
fn protected_base(generation: u64, net_id: u64, protect_ok: bool) -> UdpSocket {
    let original = UdpSocket::bind("127.0.0.1:0").expect("bind");
    n2_socket_trace::protect(original.as_raw_fd(), "Wb", "wg", generation, net_id, 2, protect_ok);
    original
}

/// The CORE half: the REAL takeover of a protected base socket —
/// `WgDevice::adopt`, which duplicates the protected raw fd and only then
/// registers the class's liveness. `Wb` never sends, so this whole chain
/// contains no I/O at all.
///
/// The TUN half only has to be a live descriptor for `TunFd`'s fd contract (the
/// device is never pumped here), and the config's secret is never parsed
/// because no peer is added. The returned `UnixStream` keeps that half open.
fn adopt_base(original: &UdpSocket) -> (WgDevice, UnixStream) {
    let (tun_raw, tun_keep) = UnixStream::pair().expect("socketpair");
    let tun = TunFd::dup_from_raw(tun_raw.as_raw_fd()).expect("tun dup");
    let device = WgDevice::adopt(WgDeviceConfig::new("n2-cd-host-test"), original.as_raw_fd(), tun)
        .expect("the core takes the protected socket over");
    drop(tun_raw);
    (device, tun_keep)
}

/// The numeric value of `|name=<digits>|` in a `N2_*` line.
fn field_u64(line: &str, name: &str) -> u64 {
    let key = format!("|{name}=");
    let start = line.find(&key).unwrap_or_else(|| panic!("no {name} in {line}")) + key.len();
    let rest = &line[start..];
    let end = rest.find('|').unwrap_or(rest.len());
    rest[..end].parse().unwrap_or_else(|_| panic!("{name} is not numeric in {line}"))
}

/// Whether the `Wb` identity seam reports "no live registration".
fn wb_has_no_identity() -> bool {
    connector::n2_cd_identity_json("Wb").contains("\"ok\":false")
}

#[test]
fn the_real_tee_hits_drive_the_sandwich_and_the_core_owns_the_receipt() {
    let _guard = serial();
    disarm();

    // UNARMED: the shipped bridge is inert and the ring file writes nothing.
    let unarmed = connector::n2_cd_plan_json("cd-C-M-1", "C", "M", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "");
    assert!(unarmed.contains("\"result\":\"rejected\""), "{unarmed}");
    assert!(unarmed.contains("\"reason\":\"not-armed\""), "{unarmed}");
    assert!(n2_cd::last_line().is_none(), "zero output while unarmed");
    assert!(connector::n2_cd_poll_json().contains("\"step\":\"idle\""));
    assert!(!n2_ring::enabled());

    arm_clean();
    let dst = [192, 0, 2, 9];
    let tee = N2DestinationTee::new(vec![dst], sys::mono_ms());

    // The OLD network's live M registration: the identity baseline.
    let old = LiveSocket::new("M", 1, 21, "192.0.2.9:443");
    let baseline: Identity = old.identity();

    let planned = connector::n2_cd_plan_json("cd-C-M-1", "C", "M", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "");
    assert!(planned.contains("\"result\":\"planned\""), "{planned}");
    assert!(planned.contains("\"pre_marker\":\"cd-C-M-1-pre\""), "{planned}");
    assert!(planned.contains("\"post_marker\":\"cd-C-M-1-post\""), "{planned}");

    // The switch: the shell's direction verdict, the NEW network's netId and the
    // OLD registration's identity as the `N2_SOCK` receipt names it.
    let switched = connector::n2_cd_switch_identified_json("cd-C-M-1", "", "cellular",
        "10.77.0.5:0", 31, baseline.fd, baseline.dup_fd, baseline.generation as i64);
    assert!(switched.contains("\"result\":\"switched\""), "{switched}");
    assert!(n2_cd::last_line().expect("switch line").contains(&format!(
        "|net_id=31|protect_generation=1|identity={}:{}:1|", baseline.fd, baseline.dup_fd)),
        "the switch line carries the OLD registration's identity baseline");

    // The pre is owed on the NEW network.
    let ask = connector::n2_cd_poll_json();
    assert!(ask.contains("\"step\":\"pre\""), "{ask}");
    assert!(ask.contains("\"marker\":\"cd-C-M-1-pre\""), "{ask}");
    assert!(ask.contains("\"reason\":\"send-pre-now\""), "{ask}");

    // The REAL armed tee consumes the pre marker: only that opens the gate.
    assert!(tee_hit(&tee, dst, 443, "cd-C-M-1-pre", 44_444), "pre HIT");
    let pre = connector::n2_cd_pre_json("cd-C-M-1", "hit", "ok");
    assert!(pre.contains("\"result\":\"hit\""), "{pre}");
    assert!(pre.contains("\"reason\":\"gate-open\""), "{pre}");
    assert!(n2_cd::released());
    let recover = connector::n2_cd_poll_json();
    assert!(recover.contains("\"step\":\"recover\""), "{recover}");
    assert!(recover.contains("\"reason\":\"pre-hit-observed\""), "{recover}");
    // the receipt must be strictly LATER than the pre HIT on the clock
    tick_past(sys::mono_ms());

    // A business write on the OLD registration can never be the new network's
    // receipt: the CORE's own call site refuses it by name, non-terminally.
    old.write("192.0.2.9:443", 64);
    let stale = n2_cd::last_line().expect("the old-registration refusal");
    assert!(stale.contains("|phase=receipt|marker=none|result=rejected|reason=net-id-mismatch|"),
        "{stale}");
    assert_eq!(n2_cd::active_phase(), "released", "a refusal keeps the plan waiting");

    // The NEW registration: its connect completion is refused by name…
    let new = LiveSocket::new("M", 2, 31, "192.0.2.9:443");
    let connect_only = n2_cd::last_line().expect("the connect refusal");
    assert!(connect_only.contains("|phase=receipt|marker=none|result=rejected|reason=connect-only|"),
        "{connect_only}");
    // …and its first BUSINESS WRITE becomes the receipt with NO shell report in
    // the path: the fact comes from the socket-trace call site's own arguments.
    let sock_line = new.write("192.0.2.9:443", 128).expect("the N2_SOCK receipt is due");
    assert!(sock_line.starts_with("N2_SOCK|phase=subsequent_io|"), "{sock_line}");
    let receipt = n2_cd::last_line().expect("the core-fed receipt");
    assert!(receipt.contains("|phase=receipt|marker=none|result=receipt|reason=ok|"), "{receipt}");
    assert!(receipt.contains(&format!("|fd={}|dup_fd={}|", new.fd, new.dup_fd)), "{receipt}");
    assert!(receipt.contains(&format!("|identity={}:{}:2|", new.fd, new.dup_fd)), "{receipt}");
    assert!(receipt.contains("|net_id=31|protect_generation=2|"), "{receipt}");
    assert!(receipt.contains("|detail=post_connect_write|"), "{receipt}");
    assert!(receipt.contains("|event_mono_ms="), "{receipt}");
    assert_eq!(n2_cd::active_phase(), "receipt-issued");

    // The post control is offered once, then the tee's own HIT closes it.
    let offer = connector::n2_cd_poll_json();
    assert!(offer.contains("\"step\":\"receipt\""), "{offer}");
    assert!(offer.contains("\"marker\":\"cd-C-M-1-post\""), "{offer}");
    assert!(offer.contains(&format!("\"fd\":{}", new.fd)), "{offer}");
    assert!(offer.contains(&format!("\"dup_fd\":{}", new.dup_fd)), "{offer}");
    assert!(offer.contains("\"remote\":\"192.0.2.9:443\""), "{offer}");
    assert!(tee_hit(&tee, dst, 443, "cd-C-M-1-post", 44_445), "post HIT");
    let post = connector::n2_cd_post_json("cd-C-M-1", "observed", "ok");
    assert!(post.contains("\"result\":\"observed\""), "{post}");
    let line = n2_cd::last_line().expect("the post line");
    assert!(line.contains("|phase=post|marker=cd-C-M-1-post|result=observed|reason=control-hit-after-receipt|"),
        "{line}");
    assert_eq!(n2_cd::active_id(), "", "the plan is retired");
    assert_eq!(n2_cd::plan_count(), 1);
    assert!(connector::n2_cd_poll_json().contains("\"step\":\"idle\""));
    disarm();
}

/// The shell cannot invent the switch baseline: the `dup_fd` is core-owned. The
/// `n2_cd_identity` bridge is the read-only seam it reads — and it must answer
/// EXACTLY the `N2_SOCK` triple of the class's live registration (`fd=` shell
/// original, `dup_fd=` core-owned descriptor, `protect_generation=`) plus that
/// registration's `net_id`, or `ok:false` when there is nothing to report (the
/// shell then passes -1/-1/-1 and the core fail-closes by name).
#[test]
fn the_switch_baseline_is_read_from_the_cores_own_live_registration() {
    let _guard = serial();
    disarm();

    // Unarmed: nothing to report, and the caller's -1s make the core refuse.
    let unarmed = connector::n2_cd_identity_json("M");
    assert!(unarmed.contains("\"ok\":false"), "{unarmed}");
    assert!(unarmed.contains("\"error\":\"no-live-registration\""), "{unarmed}");

    arm_clean();
    // A registration by itself is NOT a live socket: the identity exists only
    // after the completion the `N2_SOCK` receipt of that class is keyed on.
    let pending = UdpSocket::bind("127.0.0.1:0").expect("bind");
    n2_socket_trace::protect(pending.as_raw_fd(), "M", "42", 7, 31, 3, true);
    assert!(connector::n2_cd_identity_json("M").contains("\"ok\":false"),
        "no successful I/O yet");

    let live = LiveSocket::new("M", 7, 31, "192.0.2.9:443");
    let identity = connector::n2_cd_identity_json("M");
    assert!(identity.contains("\"ok\":true"), "{identity}");
    assert!(identity.contains(&format!("\"fd\":{}", live.fd)), "{identity}");
    assert!(identity.contains(&format!("\"dup_fd\":{}", live.dup_fd)), "{identity}");
    assert!(identity.contains("\"generation\":7"), "{identity}");
    assert!(identity.contains("\"net_id\":31"), "{identity}");
    // …and it is the identity the switch report hands to the core.
    assert_eq!(live.identity(), Identity::new(live.fd, live.dup_fd, 7));
    // Another class's live socket is a different registration: none here.
    assert!(connector::n2_cd_identity_json("S").contains("\"ok\":false"));
    // The close drops the live registration with it.
    drop(live);
    assert!(connector::n2_cd_identity_json("M").contains("\"ok\":false"),
        "a closed descriptor is never reported");
    disarm();
}

#[test]
fn a_d_plan_is_pre_set_on_wifi_and_advances_through_prepare_ready_and_the_identified_switch() {
    let _guard = serial();
    disarm();
    arm_clean();

    // D is PRE-SET while the host still has Wi-Fi: no control is owed yet.
    let planned = connector::n2_cd_plan_json("cd-D-M-1", "D", "M", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "");
    assert!(planned.contains("\"result\":\"planned\""), "{planned}");
    let line = n2_cd::last_line().expect("the plan line");
    assert!(line.contains("|phase=plan|marker=none|result=planned|reason=await-start-bearer|"),
        "{line}");
    assert!(line.contains(&format!("|wait_ms={}|", n2_cd::PREPARE_WAIT_MS)), "{line}");
    assert!(line.contains("|target=192.0.2.9:443|proto=tcp|"),
        "frozen L42: the target/proto are pre-registered before any control: {line}");
    let waiting = connector::n2_cd_poll_json();
    assert!(waiting.contains("\"step\":\"wait\""), "{waiting}");
    assert!(waiting.contains("\"reason\":\"await-start-bearer\""), "{waiting}");
    assert!(waiting.contains("\"marker\":\"\""), "no control before the recovery: {waiting}");

    // The Wi-Fi → cellular preparation hop: recorded, never the D measurement.
    let prepared = connector::n2_cd_prepare_json("cd-D-M-1", "", "cellular", "10.77.0.5:0");
    assert!(prepared.contains("\"result\":\"prepared\""), "{prepared}");
    assert!(n2_cd::last_line().expect("the prepare line")
        .contains("|phase=prepare|marker=none|result=prepared|reason=not-a-d-measurement|"));
    assert_eq!(n2_cd::active_phase(), "preparation");
    assert!(!n2_cd::released(), "the preparation never opens the gate");
    assert!(connector::n2_cd_poll_json()
        .contains("\"reason\":\"await-preparation-recovery\""));

    // A formal switch before the recovery is `unsat` by name.
    let early = connector::n2_cd_switch_identified_json("cd-D-M-1", "", "wifi",
        "192.168.50.199:0", 21, 11, 12, 1);
    assert!(early.contains("\"result\":\"unsat\""), "{early}");
    assert!(early.contains("\"reason\":\"switch-before-preparation-complete\""), "{early}");
    assert_eq!(n2_cd::active_id(), "");

    // Rep 2: the recovery lands and the plan is in position on cellular. The
    // FORMAL switch must dwell: one inside `DWELL_MIN` of the recovery (its
    // instant is >= the clock read below, so this window is provably inside the
    // minimum) is a flap, not the D event.
    assert!(connector::n2_cd_plan_json("cd-D-M-2", "D", "M", 2, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").contains("\"result\":\"planned\""));
    assert!(connector::n2_cd_prepare_json("cd-D-M-2", "", "cellular", "10.77.0.5:0")
        .contains("\"result\":\"prepared\""));
    let before_ready = sys::mono_ms();
    let ready = connector::n2_cd_ready_json("cd-D-M-2");
    assert!(ready.contains("\"result\":\"ready\""), "{ready}");
    assert!(n2_cd::last_line().expect("the ready line")
        .contains("|phase=prepare|marker=none|result=ready|reason=preparation-recovered|"));
    assert_eq!(n2_cd::active_phase(), "ready");
    let flap = n2_cd::switch_identified(before_ready + n2_cd::DWELL_MIN_MS - 1, "cd-D-M-2",
        "", "wifi", "192.168.50.199:0", 21, Some(Identity::new(11, 12, 1)));
    assert_eq!(flap.result, "unsat", "{}", flap.line);
    assert_eq!(flap.reason, "dwell-min-not-reached");
    assert_eq!(n2_cd::active_id(), "");

    // Rep 3: the whole D column through to a real receipt. The preparation and
    // its recovery are driven on an injectable (back-dated) clock — the same
    // device the state-machine tests use — so the SHIPPED switch bridge, whose
    // clock is real, is already past `DWELL_MIN`; the switch, the tee HITs and
    // the receipt then all ride the real clock in the frozen order.
    let old = LiveSocket::new("M", 1, 31, "192.0.2.9:443");
    let baseline: Identity = old.identity();
    let t0 = sys::mono_ms().saturating_sub(n2_cd::DWELL_MIN_MS + 1_000);
    assert_eq!(n2_cd::plan(t0, "cd-D-M-3", "D", "M", 3, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").result, "planned");
    assert_eq!(n2_cd::prepare(t0 + 1, "cd-D-M-3", "", "cellular", "10.77.0.5:0").result,
        "prepared");
    assert_eq!(n2_cd::ready(t0 + 2, "cd-D-M-3").result, "ready");
    let switched = connector::n2_cd_switch_identified_json("cd-D-M-3", "", "wifi",
        "192.168.50.199:0", 21, baseline.fd, baseline.dup_fd, baseline.generation as i64);
    assert!(switched.contains("\"result\":\"switched\""), "{switched}");
    assert_eq!(n2_cd::active_phase(), "switched");

    let dst = [192, 0, 2, 9];
    let tee = N2DestinationTee::new(vec![dst], sys::mono_ms());
    assert!(tee_hit(&tee, dst, 443, "cd-D-M-3-pre", 45_000), "pre HIT on the NEW network");
    assert!(connector::n2_cd_pre_json("cd-D-M-3", "hit", "ok").contains("\"result\":\"hit\""));
    assert!(n2_cd::released());
    tick_past(sys::mono_ms());
    let new = LiveSocket::new("M", 2, 21, "192.0.2.9:443");
    new.write("192.0.2.9:443", 128);
    let receipt = n2_cd::last_line().expect("the D-column receipt");
    assert!(receipt.contains("|phase=receipt|marker=none|result=receipt|reason=ok|"), "{receipt}");
    assert!(receipt.contains(&format!("|identity={}:{}:2|", new.fd, new.dup_fd)), "{receipt}");
    assert!(receipt.contains("|net_id=21|"), "{receipt}");
    let offer = connector::n2_cd_poll_json();
    assert!(offer.contains("\"step\":\"receipt\""), "{offer}");
    assert!(offer.contains("\"marker\":\"cd-D-M-3-post\""), "{offer}");
    assert!(tee_hit(&tee, dst, 443, "cd-D-M-3-post", 45_001), "post HIT");
    assert!(connector::n2_cd_post_json("cd-D-M-3", "observed", "ok")
        .contains("\"result\":\"observed\""));
    assert_eq!(n2_cd::active_id(), "");
    disarm();
}

/// `Wb` has NO production send, so its liveness cannot come from an I/O
/// completion: it comes from the CORE TAKEOVER (`WgDevice::adopt` duplicating
/// the validated, shell-protected base fd). This drives the real
/// protect → dup → adopt path and pins:
///
/// * a bare protect (shell stage) is NOT a live registration — the old early
///   feed would have handed the sandwich the shell's own fd number as if it
///   were the taken-over chain;
/// * the identity the switch baseline reads is the shipped seam's triple
///   (`fd=` shell original, `dup_fd=` core-owned descriptor, generation,
///   net_id) and it survives with ZERO production sends;
/// * a re-arm and the close of the core's own descriptor both drop it.
#[test]
fn a_wb_base_identity_exists_only_after_the_real_protect_dup_adopt_chain() {
    let _guard = serial();
    disarm();
    arm_clean();

    let original = protected_base(4, 21, true);
    let shell_fd = original.as_raw_fd();
    assert!(wb_has_no_identity(), "a bare protect is not a takeover");

    // The REAL takeover: `WgDevice::adopt` dups the validated protected fd.
    let (device, tun_keep) = adopt_base(&original);
    let identity = n2_socket_trace::live_identity("Wb").expect("the base socket is taken over");
    assert_eq!(identity.0, shell_fd, "fd = the shell original");
    assert_eq!(identity.2, 4, "the protect generation");
    assert_eq!(identity.3, 21, "the measured net id");
    let dup_fd = identity.1;
    assert_ne!(dup_fd, shell_fd, "dup_fd = the core-owned descriptor");
    let json = connector::n2_cd_identity_json("Wb");
    for field in [format!("\"fd\":{shell_fd}"), format!("\"dup_fd\":{dup_fd}"),
        "\"generation\":4".into(), "\"net_id\":21".into()] {
        assert!(json.contains(&field), "{field} in {json}");
    }

    // ZERO production sends: no `sendto` happened anywhere in this chain, so
    // the E3 per-fd counters of both registrations stay at zero.
    assert_eq!(n2_socket_trace::fd_send_totals(dup_fd), Some((0, 0)));
    let rows = n2_socket_trace::fd_send_totals_for_original(shell_fd);
    assert!(rows.iter().any(|(fd, packets, bytes)| *fd == dup_fd && *packets == 0 && *bytes == 0),
        "{rows:?}");
    assert!(rows.iter().all(|(_, packets, bytes)| *packets == 0 && *bytes == 0), "{rows:?}");

    // An arming change is a fresh session: nothing survives it.
    n2_socket_trace::arm(true);
    assert!(wb_has_no_identity(), "a re-arm clears the live base socket");

    // …and the close of the CORE's own descriptor drops the live identity too.
    let original2 = protected_base(4, 21, true);
    let shell2 = original2.as_raw_fd();
    let (device2, tun2) = adopt_base(&original2);
    assert!(n2_socket_trace::live_identity("Wb").is_some());
    drop(device2);
    assert!(wb_has_no_identity(), "the core descriptor's close drops the identity");

    drop(device);
    drop(tun_keep);
    drop(tun2);
    n2_socket_trace::closed(shell2);
    n2_socket_trace::closed(shell_fd);
    disarm();
}

/// The `Wb` substitute receipt, fed by the SHIPPED seams: the switch baseline
/// is READ from the core's own live registration (never fabricated by the
/// caller), and the receipt binds the ADOPTED descriptor plus the registration's
/// OWN protect-settle instant — measured by bracketing the protect call and
/// taking the socket over in a strictly later tick. Both controls are still
/// owed; `exempted` only covers the "real production send" criterion.
#[test]
fn a_wb_plan_takes_its_substitute_receipt_from_the_real_takeover() {
    let _guard = serial();
    disarm();
    arm_clean();

    // The OLD generation's base socket, really taken over.
    let old = protected_base(1, 21, true);
    let (old_device, old_tun) = adopt_base(&old);
    let baseline = n2_socket_trace::live_identity("Wb").expect("old generation live");
    assert_eq!(baseline.0, old.as_raw_fd());

    assert!(connector::n2_cd_plan_json("cd-C-Wb-1", "C", "Wb", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").contains("\"result\":\"planned\""));
    let switched = connector::n2_cd_switch_identified_json("cd-C-Wb-1", "", "cellular",
        "10.77.0.5:0", 31, baseline.0, baseline.1, baseline.2 as i64);
    assert!(switched.contains("\"result\":\"switched\""), "{switched}");
    assert!(n2_cd::last_line().expect("switch line").contains(&format!(
        "|identity={}:{}:{}|", baseline.0, baseline.1, baseline.2)),
        "the switch line carries the REAL old registration's identity");

    // Wb×C/D is exempted from the "real production send" criterion ONLY: both
    // controls are still owed, starting with the pre on the NEW network.
    let ask = connector::n2_cd_poll_json();
    assert!(ask.contains("\"step\":\"pre\""), "{ask}");
    assert!(ask.contains("\"marker\":\"cd-C-Wb-1-pre\""), "{ask}");
    let dst = [192, 0, 2, 9];
    let tee = N2DestinationTee::new(vec![dst], sys::mono_ms());
    assert!(tee_hit(&tee, dst, 443, "cd-C-Wb-1-pre", 46_000), "pre HIT");
    assert!(connector::n2_cd_pre_json("cd-C-Wb-1", "hit", "ok").contains("\"result\":\"hit\""));
    assert!(n2_cd::released());
    let after_pre = sys::mono_ms();
    tick_past(after_pre);

    // The new generation's protect settle. Bracket the shipped protect call:
    // the receipt must carry THAT instant, not the later takeover's clock.
    let before_protect = sys::mono_ms();
    let new = protected_base(2, 31, true);
    let new_fd = new.as_raw_fd();
    let settled = sys::mono_ms();
    assert_eq!(n2_cd::active_phase(), "released", "a bare protect is not the receipt");
    tick_past(settled);
    let (new_device, new_tun) = adopt_base(&new);
    let receipt = n2_cd::last_line().expect("the Wb substitute receipt");
    assert!(receipt.contains(
        "|phase=receipt|marker=none|result=exempted|reason=wb-protect-settle-success|"), "{receipt}");
    let taken = n2_socket_trace::live_identity("Wb").expect("the new generation is live");
    assert_eq!(taken.0, new_fd);
    assert_ne!(taken.1, new_fd, "dup_fd is the core-owned descriptor, not the shell fd");
    assert_eq!(taken.2, 2);
    assert!(receipt.contains(&format!("|fd={new_fd}|dup_fd={}|", taken.1)), "{receipt}");
    assert!(receipt.contains(&format!("|identity={new_fd}:{}:2|", taken.1)), "{receipt}");
    assert!(receipt.contains("|net_id=31|protect_generation=2|"), "{receipt}");
    assert!(receipt.contains("|detail=protect_settle|"), "{receipt}");
    let event_ms = field_u64(&receipt, "event_mono_ms");
    assert!((before_protect..=settled).contains(&event_ms),
        "the receipt carries the protect settle instant ({event_ms} not in [{before_protect},{settled}])");
    assert!(event_ms < sys::mono_ms(), "the takeover happened strictly later");
    assert_eq!(n2_cd::active_phase(), "receipt-issued");

    // …and the post control is still owed.
    let offer = connector::n2_cd_poll_json();
    assert!(offer.contains("\"step\":\"receipt\""), "{offer}");
    assert!(offer.contains("\"marker\":\"cd-C-Wb-1-post\""), "{offer}");
    assert!(tee_hit(&tee, dst, 443, "cd-C-Wb-1-post", 46_001), "post HIT");
    assert!(connector::n2_cd_post_json("cd-C-Wb-1", "observed", "ok")
        .contains("\"result\":\"observed\""));
    assert_eq!(n2_cd::active_id(), "");

    drop(new_device);
    drop(old_device);
    drop(new_tun);
    drop(old_tun);
    disarm();
}

/// The protect instant is the receipt's clock, so a socket protected BEFORE the
/// pre control reached the tee can NEVER be rescued by taking it over later:
/// the late adopt feeds the receipt with the old protect settle and the core
/// refuses it `io-before-pre-hit`. Only a protection that really settled after
/// the pre HIT is accepted.
#[test]
fn a_wb_protect_before_the_pre_hit_cannot_be_rescued_by_a_late_takeover() {
    let _guard = serial();
    disarm();
    arm_clean();

    let old = protected_base(1, 21, true);
    let (old_device, old_tun) = adopt_base(&old);
    let baseline = n2_socket_trace::live_identity("Wb").expect("old generation live");

    assert!(connector::n2_cd_plan_json("cd-C-Wb-1", "C", "Wb", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").contains("\"result\":\"planned\""));
    assert!(connector::n2_cd_switch_identified_json("cd-C-Wb-1", "", "cellular",
        "10.77.0.5:0", 31, baseline.0, baseline.1, baseline.2 as i64)
        .contains("\"result\":\"switched\""));

    // The new generation's socket is protected BEFORE the pre HIT.
    let early = protected_base(2, 31, true);
    let early_done = sys::mono_ms();
    tick_past(early_done);

    let dst = [192, 0, 2, 9];
    let tee = N2DestinationTee::new(vec![dst], sys::mono_ms());
    assert!(tee_hit(&tee, dst, 443, "cd-C-Wb-1-pre", 46_000), "pre HIT");
    assert!(connector::n2_cd_pre_json("cd-C-Wb-1", "hit", "ok").contains("\"result\":\"hit\""));
    assert!(n2_cd::released());

    // The LATE takeover of that already-protected socket must not be able to
    // back-date the receipt: its clock is the protect settle, which precedes
    // the pre HIT.
    let (early_device, early_tun) = adopt_base(&early);
    let refused = n2_cd::last_line().expect("the receipt refusal");
    assert!(refused.contains("|phase=receipt|marker=none|result=rejected|reason=io-before-pre-hit|"),
        "{refused}");
    assert_eq!(n2_cd::active_phase(), "released", "the plan is NOT exempted");
    assert_ne!(n2_cd::active_phase(), "receipt-issued");

    // A protection that really settled AFTER the pre HIT is accepted.
    let after_refusal = sys::mono_ms();
    tick_past(after_refusal);
    let fresh = protected_base(3, 31, true);
    let (fresh_device, fresh_tun) = adopt_base(&fresh);
    let good = n2_cd::last_line().expect("the accepted receipt");
    assert!(good.contains("|result=exempted|reason=wb-protect-settle-success|"), "{good}");
    assert_eq!(n2_cd::active_phase(), "receipt-issued");

    drop(fresh_device);
    drop(early_device);
    drop(old_device);
    drop(fresh_tun);
    drop(early_tun);
    drop(old_tun);
    disarm();
}

/// Wb×C/D (approved R2(a)): with the REAL takeover seam the accepted substitute
/// receipt's `event_mono_ms` IS that generation's core `protect_mono_ms`, and
/// the post HIT is judged against it STRICTLY (`pre_hit_ms < protect_mono_ms <
/// post_hit_ms`). A post HIT in the SAME millisecond as the protect settle is
/// refused `post-before-receipt` — the older `post_hit_ms >= receipt_ms` callback
/// rule alone would have passed it — while a post HIT one millisecond later
/// closes the sandwich `observed`.
#[test]
fn a_wb_post_hit_must_be_strictly_later_than_the_real_protect_settle() {
    let _guard = serial();
    disarm();
    arm_clean();

    // --- Same-millisecond post HIT against the real protect settle: refused. ---
    let old = protected_base(1, 21, true);
    let (old_device, old_tun) = adopt_base(&old);
    let baseline = n2_socket_trace::live_identity("Wb").expect("old generation live");
    assert!(connector::n2_cd_plan_json("cd-C-Wb-1", "C", "Wb", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").contains("\"result\":\"planned\""));
    assert!(connector::n2_cd_switch_identified_json("cd-C-Wb-1", "", "cellular",
        "10.77.0.5:0", 31, baseline.0, baseline.1, baseline.2 as i64)
        .contains("\"result\":\"switched\""));
    let dst = [192, 0, 2, 9];
    let tee = N2DestinationTee::new(vec![dst], sys::mono_ms());
    assert!(tee_hit(&tee, dst, 443, "cd-C-Wb-1-pre", 46_000), "pre HIT");
    assert!(connector::n2_cd_pre_json("cd-C-Wb-1", "hit", "ok").contains("\"result\":\"hit\""));
    assert!(n2_cd::released());
    let after_pre = sys::mono_ms();
    tick_past(after_pre);
    let new = protected_base(2, 31, true);
    let (new_device, new_tun) = adopt_base(&new);
    let receipt = n2_cd::last_line().expect("the Wb substitute receipt");
    assert!(receipt.contains(
        "|phase=receipt|marker=none|result=exempted|reason=wb-protect-settle-success|"), "{receipt}");
    // The receipt's own instant IS the new generation's protect settle (the
    // takeover feeds it verbatim), and the plan now owes the post.
    let settle = field_u64(&receipt, "event_mono_ms");
    assert_eq!(n2_cd::active_phase(), "receipt-issued");
    assert!(connector::n2_cd_poll_json().contains("\"step\":\"receipt\""));

    // The tee HIT lands in the SETTLE millisecond itself.
    n2_cd::control_hit("cd-C-Wb-1-post", settle);
    let same_ms = connector::n2_cd_post_json("cd-C-Wb-1", "observed", "ok");
    assert!(same_ms.contains("\"result\":\"unsat\""), "{same_ms}");
    assert!(same_ms.contains("\"reason\":\"post-before-receipt\""), "{same_ms}");
    let line = n2_cd::last_line().expect("the post line");
    assert!(line.contains(
        "|phase=post|marker=cd-C-Wb-1-post|result=unsat|reason=post-before-receipt|"), "{line}");
    assert_eq!(n2_cd::active_id(), "", "the post verdict is terminal");

    // --- A post HIT strictly later than the real protect settle: observed. ---
    let old2 = protected_base(3, 31, true);
    let (old2_device, old2_tun) = adopt_base(&old2);
    let baseline2 = n2_socket_trace::live_identity("Wb").expect("next baseline live");
    assert!(connector::n2_cd_plan_json("cd-C-Wb-2", "C", "Wb", 2, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").contains("\"result\":\"planned\""));
    assert!(connector::n2_cd_switch_identified_json("cd-C-Wb-2", "", "cellular",
        "10.77.0.5:0", 41, baseline2.0, baseline2.1, baseline2.2 as i64)
        .contains("\"result\":\"switched\""));
    let tee2 = N2DestinationTee::new(vec![dst], sys::mono_ms());
    assert!(tee_hit(&tee2, dst, 443, "cd-C-Wb-2-pre", 46_002), "pre HIT");
    assert!(connector::n2_cd_pre_json("cd-C-Wb-2", "hit", "ok").contains("\"result\":\"hit\""));
    let after_pre2 = sys::mono_ms();
    tick_past(after_pre2);
    let new2 = protected_base(4, 41, true);
    let (new2_device, new2_tun) = adopt_base(&new2);
    let receipt2 = n2_cd::last_line().expect("the second Wb substitute receipt");
    assert!(receipt2.contains("|result=exempted|reason=wb-protect-settle-success|"), "{receipt2}");
    let settle2 = field_u64(&receipt2, "event_mono_ms");
    assert!(connector::n2_cd_poll_json().contains("\"step\":\"receipt\""));
    // Exactly one millisecond after the settle: the strict relation holds.
    n2_cd::control_hit("cd-C-Wb-2-post", settle2 + 1);
    let observed = connector::n2_cd_post_json("cd-C-Wb-2", "observed", "ok");
    assert!(observed.contains("\"result\":\"observed\""), "{observed}");

    drop(new2_device);
    drop(old2_device);
    drop(new_device);
    drop(old_device);
    drop(new2_tun);
    drop(old2_tun);
    drop(new_tun);
    drop(old_tun);
    disarm();
}

/// No failure in the chain produces an `exempted`: an old generation
/// re-appearing through the same takeover seam is refused by name, a FAILED
/// protect publishes no identity and is refused `protect-failed`, and a takeover
/// the core never validated (the dup itself fails) reaches the receipt seam not
/// at all.
#[test]
fn a_wb_exempted_receipt_needs_a_successful_protect_and_a_validated_takeover() {
    let _guard = serial();
    disarm();
    arm_clean();

    let old = protected_base(1, 21, true);
    let (old_device, old_tun) = adopt_base(&old);
    let old_fd = old.as_raw_fd();
    let baseline = n2_socket_trace::live_identity("Wb").expect("old generation live");
    let old_dup = baseline.1;

    assert!(connector::n2_cd_plan_json("cd-C-Wb-1", "C", "Wb", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").contains("\"result\":\"planned\""));
    assert!(connector::n2_cd_switch_identified_json("cd-C-Wb-1", "", "cellular",
        "10.77.0.5:0", 31, baseline.0, baseline.1, baseline.2 as i64)
        .contains("\"result\":\"switched\""));
    let dst = [192, 0, 2, 9];
    let tee = N2DestinationTee::new(vec![dst], sys::mono_ms());
    assert!(tee_hit(&tee, dst, 443, "cd-C-Wb-1-pre", 46_000), "pre HIT");
    assert!(connector::n2_cd_pre_json("cd-C-Wb-1", "hit", "ok").contains("\"result\":\"hit\""));
    assert!(n2_cd::released());

    // 1) The OLD protect generation re-appearing (same fd/dup, the new net id,
    // a fresh protect settle after the pre HIT) is refused `generation-unchanged`.
    let before_re = sys::mono_ms();
    tick_past(before_re);
    n2_socket_trace::protect(old_fd, "Wb", "wg", baseline.2, 31, 2, true);
    n2_socket_trace::adopted(old_fd, old_dup);
    let same_generation = n2_cd::last_line().expect("the same-generation refusal");
    assert!(same_generation.contains(
        "|phase=receipt|marker=none|result=rejected|reason=generation-unchanged|"), "{same_generation}");
    assert_eq!(n2_cd::active_phase(), "released");

    // 2) …and the same descriptor re-protected under a NEW generation is refused
    // `identity-unchanged`.
    n2_socket_trace::protect(old_fd, "Wb", "wg", 9, 31, 2, true);
    n2_socket_trace::adopted(old_fd, old_dup);
    let same_identity = n2_cd::last_line().expect("the same-identity refusal");
    assert!(same_identity.contains(
        "|phase=receipt|marker=none|result=rejected|reason=identity-unchanged|"), "{same_identity}");
    assert_eq!(n2_cd::active_phase(), "released");

    // 3) A FAILED protect: the receipt is still fed (so the failure is visible)
    // but refused by name, and NO identity is published for it.
    let failed = protected_base(2, 31, false);
    let (failed_device, failed_tun) = adopt_base(&failed);
    let protect_failed = n2_cd::last_line().expect("the protect-failed refusal");
    assert!(protect_failed.contains(
        "|phase=receipt|marker=none|result=rejected|reason=protect-failed|"), "{protect_failed}");
    assert_eq!(n2_socket_trace::live_identity("Wb").map(|i| i.0), Some(old_fd),
        "a failed protect publishes no identity (the old registration is untouched)");
    assert_eq!(n2_cd::active_phase(), "released");

    // 4) A takeover the core never validated: the dup fails, so `adopt` returns
    // before the traced takeover — no receipt, no identity change.
    let no_receipt = n2_cd::last_line().expect("last line");
    let (tun_raw, _tun_keep) = UnixStream::pair().expect("socketpair");
    let tun = TunFd::dup_from_raw(tun_raw.as_raw_fd()).expect("tun dup");
    assert!(WgDevice::adopt(WgDeviceConfig::new("n2-cd-host-test"), -1, tun).is_err(),
        "a missing fd fails the dup");
    assert_eq!(n2_cd::last_line().as_deref(), Some(no_receipt.as_str()),
        "a failed dup/adopt emits nothing at all");
    assert_eq!(n2_socket_trace::live_identity("Wb").map(|i| i.0), Some(old_fd));

    // 5) The real new generation is still accepted afterwards.
    let after_all = sys::mono_ms();
    tick_past(after_all);
    let good = protected_base(4, 31, true);
    let (good_device, good_tun) = adopt_base(&good);
    let accepted = n2_cd::last_line().expect("the accepted receipt");
    assert!(accepted.contains("|result=exempted|reason=wb-protect-settle-success|"), "{accepted}");
    assert_eq!(n2_cd::active_phase(), "receipt-issued");

    drop(good_device);
    drop(failed_device);
    drop(old_device);
    drop(good_tun);
    drop(failed_tun);
    drop(old_tun);
    disarm();
}

#[test]
fn a_shell_hit_without_the_tee_is_unsat_and_a_missing_receipt_times_out() {
    let _guard = serial();
    disarm();
    arm_clean();

    // A `hit` the tee never consumed cannot complete a sandwich: the plan closes
    // `unsat` and the production-recovery gate never opens.
    assert!(connector::n2_cd_plan_json("cd-C-R-1", "C", "R", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").contains("\"result\":\"planned\""));
    assert!(connector::n2_cd_switch_identified_json("cd-C-R-1", "", "cellular", "10.77.0.5:0",
        31, 21, 22, 1).contains("\"result\":\"switched\""));
    let claimed = connector::n2_cd_pre_json("cd-C-R-1", "hit", "ok");
    assert!(claimed.contains("\"result\":\"unsat\""), "{claimed}");
    assert!(claimed.contains("\"reason\":\"pre-hit-unobserved\""), "{claimed}");
    assert!(!n2_cd::released(), "the shell's claim alone never opens the gate");
    assert_eq!(n2_cd::active_id(), "");

    // A REAL pre HIT with NO production registration on the new network: the
    // plan's own written fuse closes it `unsat|receipt-wait-exceeded`.
    let dst = [192, 0, 2, 9];
    let tee = N2DestinationTee::new(vec![dst], sys::mono_ms());
    assert!(connector::n2_cd_plan_json("cd-C-I-1", "C", "I", 2, "192.0.2.9:3478", "wifi",
        "192.168.50.199:0", "").contains("\"result\":\"planned\""));
    assert!(connector::n2_cd_switch_identified_json("cd-C-I-1", "", "cellular", "10.77.0.5:0",
        31, 21, 22, 1).contains("\"result\":\"switched\""));
    assert!(tee_hit(&tee, dst, 3478, "cd-C-I-1-pre", 47_000), "pre HIT");
    // the release instant is bounded by the two clock reads around the report:
    // strictly inside the fuse for the first poll, past it for the second
    let before_pre = sys::mono_ms();
    assert!(connector::n2_cd_pre_json("cd-C-I-1", "hit", "ok").contains("\"result\":\"hit\""));
    let after_pre = sys::mono_ms();
    assert!(n2_cd::released());
    // the watcher's own clock drives the bound (the shipped poll reads the real
    // one, so the bound is exercised on the injectable entry)
    assert_eq!(n2_cd::poll(before_pre + n2_cd::RECEIPT_WAIT_MS - 1).step, "recover");
    let expired = n2_cd::poll(after_pre + n2_cd::RECEIPT_WAIT_MS);
    assert_eq!(expired.step, "closed");
    assert_eq!(expired.reason, "receipt-wait-exceeded");
    assert!(expired.line
        .contains("|phase=receipt|marker=none|result=unsat|reason=receipt-wait-exceeded|"),
        "{}", expired.line);
    assert_eq!(n2_cd::active_id(), "");
    assert!(connector::n2_cd_poll_json().contains("\"step\":\"idle\""));
    disarm();
}
