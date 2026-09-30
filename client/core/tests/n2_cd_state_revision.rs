// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N2 C/D columns — **state-machine revision** host tests (2026-09-29; no
//! device, no network beyond loopback). Companion of
//! `~/n2h-kit/status-20260927/N2-CD-RING-T0-REVIEW.md`; the frozen criteria are
//! `docs/n2-device-evidence-preregistration-draft-v2-20260928.md` (v4.1) and are
//! NOT changed here.
//!
//! What this file proves, in the frozen vocabulary:
//!
//! 1. **D is pre-set on Wi-Fi and its preparation hop is not the D
//!    measurement.** The plan is accepted over the Wi-Fi hdc link
//!    (`await-start-bearer`), the Wi-Fi → cellular hop is recorded as
//!    `phase=prepare|result=prepared|reason=not-a-d-measurement` with NO control
//!    and NO gate, and only after the preparation's recovery does the plan wait
//!    for the FORMAL cellular → Wi-Fi switch.
//! 2. **The pre control belongs to the NEW network.** A pre reported before the
//!    formal switch is `rejected|pre-before-switch`; a HIT the tee saw on the
//!    old network is never recorded and never opens the gate. Only the FORMAL
//!    switch makes `poll` ask for the pre (`step=pre`).
//! 3. **The production-recovery gate opens ONLY on a real tee HIT.** `step=recover`
//!    (and `released()`) appear only after the armed `N2DestinationTee` consumed
//!    the `<id>-pre` marker; a shell claim of `hit` alone is
//!    `unsat|pre-hit-unobserved`.
//! 4. **The receipt is an identity verdict, not a global counter.** A receipt
//!    must carry a NEW `(fd, dup_fd, protect_generation)` identity, the NEW
//!    network's `net_id`, `protect=success` and an I/O after the pre HIT;
//!    `M`/`S`/`R` additionally need a real `phase=subsequent_io` business write
//!    (a `connect` completion is refused). The old identity, a different fd
//!    NUMBER with the old generation, a wrong netId, a connect-only write and an
//!    unmeasured netId/generation/I/O time are each refused by name.
//! 5. **Wb still owes both controls** and its `exempted` receipt needs the new
//!    generation's `protect_settle|status=success`; the post control follows it.
//! 6. **The legacy entries are fail-closed.** `switch` (the five-argument
//!    compat entry) carries no identity facts and closes
//!    `unsat|switch-identity-required`; a repeated plan id is refused.
//!
//! Everything is diagnostic: every line ends `|diag=true|is_evidence=false`, and
//! nothing here is N2/N6 evidence (the campaign needs the separately authorized
//! AUTH). The test drives the core's INPUT API directly (no device socket is
//! registered): the wiring that must feed it is reported separately — see the
//! module docs of `client/core/src/n2_cd.rs` and the delivery note.

use std::net::UdpSocket;
use std::os::fd::AsRawFd;
use std::sync::{Mutex, MutexGuard};

use netbird_core::n2_cd::{self, Identity, Receipt};
use netbird_core::n2_socket_trace;
use netbird_core::wg_device::N2DestinationTee;

// Host-process link stubs (repo convention, cf. tests/n2_cd.rs): the platform
// libraries libace_napi.z.so / libhilog_ndk.z.so do not exist on the host.
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
        _value: *mut c_void,
        _buf: *mut u8,
        _bufsize: usize,
        _result: *mut usize,
        _env: *mut c_void,
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
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn arm_clean() {
    n2_socket_trace::arm(false);
    n2_cd::reset();
    n2_socket_trace::arm(true);
}

fn disarm() {
    n2_socket_trace::arm(false);
    n2_cd::reset();
}

/// The identity the kind's live registration had on the OLD network (the switch
/// baseline).
fn baseline() -> Identity {
    Identity::new(21, 22, 1)
}

/// A valid receipt on the NEW network: a new registration (41/42, generation 2)
/// whose business write completed at `io_mono_ms`.
fn business_receipt(io_mono_ms: u64) -> Receipt<'static> {
    Receipt { identity: Identity::new(41, 42, 2), net_id: 31, protect_ok: true,
        io_mono_ms, subsequent_io: true, protect_settle: false, detail: "post_connect_write",
        remote: "192.0.2.9:443", local: "10.77.0.5:51000", request_id: "42" }
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

/// Register a fresh live outer socket of `kind` through the SHIPPED trace API,
/// as the production path does. Used only by the "the global counter is not the
/// criterion" test: a NEW live registration alone must NOT produce a receipt.
fn new_live_socket(kind: &str) -> (UdpSocket, i32) {
    let socket = UdpSocket::bind("127.0.0.1:0").expect("bind");
    let fd = socket.as_raw_fd();
    n2_socket_trace::protect(fd, kind, "42", 2, 31, 3, true);
    n2_socket_trace::first_io(fd, "192.0.2.9:443".parse().expect("addr"), "sent", "connect");
    (socket, fd)
}

#[test]
fn d_is_pre_set_on_wifi_and_the_preparation_hop_is_not_the_d_measurement() {
    let _guard = serial();
    disarm();
    arm_clean();

    // The plan is accepted while the host still has the Wi-Fi hdc link. D is
    // PRE-SET: no pre control is owed yet.
    let planned = n2_cd::plan(1_000, "cd-D-M-1", "D", "M", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "");
    assert_eq!(planned.result, "planned", "{}", planned.line);
    assert!(planned.line.contains("|phase=plan|marker=none|result=planned|reason=await-start-bearer|"),
        "{}", planned.line);
    assert!(planned.line.contains("|target=192.0.2.9:443|proto=tcp|"),
        "frozen L42: target/proto are pre-registered before any control: {}", planned.line);
    assert_eq!(n2_cd::active_phase(), "await-start-bearer");
    let step = n2_cd::poll(1_100);
    assert_eq!(step.step, "wait");
    assert_eq!(step.reason, "await-start-bearer");
    assert_eq!(step.marker, "", "no control is owed before the preparation");

    // The Wi-Fi → cellular preparation hop. It is NOT the D measurement: no
    // control, no event, no gate.
    let prepared = n2_cd::prepare(1_200, "cd-D-M-1", "", "cellular", "10.77.0.5:0");
    assert_eq!(prepared.result, "prepared", "{}", prepared.line);
    assert!(prepared.line.contains("|phase=prepare|marker=none|result=prepared|reason=not-a-d-measurement|"),
        "{}", prepared.line);
    assert_eq!(n2_cd::active_phase(), "preparation");
    assert!(!n2_cd::released(), "the preparation never opens the gate");
    assert_eq!(n2_cd::poll(1_250).step, "wait");
    assert_eq!(n2_cd::poll(1_250).reason, "await-preparation-recovery");

    // A formal switch before the preparation is complete is unsat by name.
    let early = n2_cd::switch_identified(1_300, "cd-D-M-1", "", "wifi", "192.168.50.199:0",
        21, Some(Identity::new(11, 12, 1)));
    assert_eq!(early.result, "unsat");
    assert_eq!(early.reason, "switch-before-preparation-complete");
    assert_eq!(n2_cd::active_id(), "");

    // Rep 2: the preparation recovery, the minimum dwell, then the FORMAL
    // cellular → Wi-Fi switch, whose pre is owed on the NEW (Wi-Fi) network.
    assert_eq!(n2_cd::plan(2_000, "cd-D-M-2", "D", "M", 2, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").result, "planned");
    assert_eq!(n2_cd::prepare(2_100, "cd-D-M-2", "", "cellular", "10.77.0.5:0").result,
        "prepared");
    let ready = n2_cd::ready(2_200, "cd-D-M-2");
    assert_eq!(ready.result, "ready", "{}", ready.line);
    assert!(ready.line.contains("|phase=prepare|marker=none|result=ready|reason=preparation-recovered|"),
        "{}", ready.line);
    assert_eq!(n2_cd::active_phase(), "ready");
    // a flap back to Wi-Fi inside DWELL_MIN is not the D event
    let flap = n2_cd::switch_identified(2_200 + n2_cd::DWELL_MIN_MS - 1, "cd-D-M-2", "",
        "wifi", "192.168.50.199:0", 21, Some(Identity::new(11, 12, 1)));
    assert_eq!(flap.result, "unsat");
    assert_eq!(flap.reason, "dwell-min-not-reached");
    disarm();
}

#[test]
fn an_old_network_pre_and_an_old_network_hit_never_release_the_production_path() {
    let _guard = serial();
    disarm();
    arm_clean();

    assert_eq!(n2_cd::plan(1_000, "cd-C-M-1", "C", "M", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").result, "planned");
    // The tee consumed the marker while the plan is still on the OLD network:
    // that observation must not even be recorded as a usable pre HIT.
    n2_cd::control_hit("cd-C-M-1-pre", 1_050);
    let early = n2_cd::pre(1_100, "cd-C-M-1", "hit", "ok");
    assert_eq!(early.result, "rejected");
    assert_eq!(early.reason, "pre-before-switch");
    assert!(!n2_cd::released(), "the gate stays shut on the old network");
    assert_eq!(n2_cd::active_id(), "cd-C-M-1", "the plan is still in position");

    // The FORMAL switch: the identity baseline and the new network's netId are
    // required inputs.
    let switched = n2_cd::switch_identified(1_200, "cd-C-M-1", "", "cellular", "10.77.0.5:0",
        31, Some(baseline()));
    assert_eq!(switched.result, "switched", "{}", switched.line);
    assert!(switched.line.contains("|net_id=31|protect_generation=1|identity=21:22:1|"),
        "{}", switched.line);
    let ask = n2_cd::poll(1_300);
    assert_eq!(ask.step, "pre", "the pre is owed on the NEW network");
    assert_eq!(ask.marker, "cd-C-M-1-pre");
    assert!(!n2_cd::released());

    // The old HIT is not a witness for the new network.
    let late = n2_cd::pre(1_400, "cd-C-M-1", "hit", "ok");
    assert_eq!(late.result, "unsat");
    assert_eq!(late.reason, "pre-hit-unobserved");
    assert!(!n2_cd::released());
    assert_eq!(n2_cd::active_id(), "", "the plan is closed, the gate never opened");
    disarm();
}

#[test]
fn a_pre_the_tee_never_hit_never_releases_and_a_missing_pre_times_out() {
    let _guard = serial();
    disarm();
    arm_clean();

    // A shell `hit` claim the tee never observed: unsat, gate shut.
    assert_eq!(n2_cd::plan(1_000, "cd-C-R-1", "C", "R", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").result, "planned");
    assert_eq!(n2_cd::switch_identified(1_010, "cd-C-R-1", "", "cellular", "10.77.0.5:0", 31,
        Some(baseline())).result, "switched");
    let claimed = n2_cd::pre(1_020, "cd-C-R-1", "hit", "ok");
    assert_eq!(claimed.result, "unsat");
    assert_eq!(claimed.reason, "pre-hit-unobserved");
    assert!(!n2_cd::released());

    // A pre control rejected in flight: unsat too (the gate must not open).
    assert_eq!(n2_cd::plan(2_000, "cd-C-R-2", "C", "R", 2, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").result, "planned");
    assert_eq!(n2_cd::switch_identified(2_010, "cd-C-R-2", "", "cellular", "10.77.0.5:0", 31,
        Some(baseline())).result, "switched");
    let failed = n2_cd::pre(2_020, "cd-C-R-2", "rejected", "control-send-failed");
    assert_eq!(failed.result, "unsat");
    assert_eq!(failed.reason, "control-send-failed");
    assert!(failed.line.contains("|phase=pre|marker=cd-C-R-2-pre|result=unsat|reason=control-send-failed|"),
        "{}", failed.line);
    assert!(!n2_cd::released());

    // The shell never reports the pre at all: the plan-level fuse closes it.
    assert_eq!(n2_cd::plan(3_000, "cd-C-I-1", "C", "I", 3, "192.0.2.9:3478", "wifi",
        "192.168.50.199:0", "").result, "planned");
    assert_eq!(n2_cd::switch_identified(3_010, "cd-C-I-1", "", "cellular", "10.77.0.5:0", 31,
        Some(baseline())).result, "switched");
    assert_eq!(n2_cd::poll(3_010).step, "pre");
    let expired = n2_cd::poll(3_010 + n2_cd::PRE_WAIT_MS);
    assert_eq!(expired.step, "closed");
    assert_eq!(expired.reason, "pre-wait-exceeded");
    assert!(!n2_cd::released());
    assert_eq!(n2_cd::active_id(), "");
    disarm();
}

#[test]
fn the_real_tee_gates_the_release_and_the_post_and_the_old_counter_is_not_the_criterion() {
    let _guard = serial();
    disarm();
    arm_clean();

    let dst = [192, 0, 2, 9];
    let tee = N2DestinationTee::new(vec![dst], 1_000);

    assert_eq!(n2_cd::plan(1_000, "cd-C-M-1", "C", "M", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").result, "planned");
    assert_eq!(n2_cd::switch_identified(1_010, "cd-C-M-1", "", "cellular", "10.77.0.5:0", 31,
        Some(baseline())).result, "switched");
    assert_eq!(n2_cd::poll(1_015).step, "pre");

    // A NEW live registration of the class, exactly as the production path
    // registers one (the shipped trace API): under the OLD implementation this
    // alone produced the "receipt". It must not produce anything now — the
    // process-global registration counter is not identity.
    let (socket, fd) = new_live_socket("M");
    let after_registration = n2_cd::poll(1_018);
    assert_eq!(after_registration.step, "pre", "a live registration is not a receipt");
    assert!(!n2_cd::released());
    assert_eq!(n2_cd::active_phase(), "switched");

    // Only the armed tee's own consumption of the marker is the pre HIT.
    let pre_registered = tee.register_control("cd-C-M-1-pre", dst, 443, 17, 0, 1_020);
    assert!(pre_registered.is_ok(), "{pre_registered:?}");
    let pre_frame = udp_frame(dst, 44_444, 443, b"N2CTRL:cd-C-M-1-pre");
    assert!(tee.consume_control(&pre_frame, 1_022), "the tee consumed the pre");
    let hit = n2_cd::pre(1_025, "cd-C-M-1", "hit", "ok");
    assert_eq!(hit.result, "hit", "{}", hit.line);
    assert!(hit.line.contains("|phase=pre|marker=cd-C-M-1-pre|result=hit|reason=gate-open|"),
        "{}", hit.line);
    assert!(n2_cd::released(), "the tee HIT is what opens the gate");
    let recover = n2_cd::poll(1_030);
    assert_eq!(recover.step, "recover");
    assert_eq!(recover.reason, "pre-hit-observed");

    // The valid receipt, then the post (also from the real tee).
    let good = n2_cd::receipt(1_040, "cd-C-M-1", &business_receipt(1_035));
    assert_eq!(good.result, "receipt", "{}", good.line);
    let offer = n2_cd::poll(1_050);
    assert_eq!(offer.step, "receipt");
    assert_eq!(offer.marker, "cd-C-M-1-post");
    let post_registered = tee.register_control("cd-C-M-1-post", dst, 443, 17, 0, 1_055);
    assert!(post_registered.is_ok(), "{post_registered:?}");
    let post_frame = udp_frame(dst, 44_445, 443, b"N2CTRL:cd-C-M-1-post");
    assert!(tee.consume_control(&post_frame, 1_060), "the tee consumed the post");
    let post = n2_cd::post(1_065, "cd-C-M-1", "observed", "ok");
    assert_eq!(post.result, "observed", "{}", post.line);
    assert!(post.line.contains("|phase=post|marker=cd-C-M-1-post|result=observed|reason=control-hit-after-receipt|"),
        "{}", post.line);
    assert_eq!(n2_cd::active_id(), "");
    assert_eq!(n2_cd::plan_count(), 1);
    // the live registration the test created never became a receipt
    assert!(good.line.contains("|fd=41|dup_fd=42|"), "{}", good.line);
    n2_socket_trace::closed(fd);
    drop(socket);
    disarm();
}

#[test]
fn an_old_identity_or_a_connect_only_receipt_is_refused_and_the_plan_keeps_waiting() {
    let _guard = serial();
    disarm();
    arm_clean();

    assert_eq!(n2_cd::plan(1_000, "cd-C-M-1", "C", "M", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").result, "planned");
    assert_eq!(n2_cd::switch_identified(1_010, "cd-C-M-1", "", "cellular", "10.77.0.5:0", 31,
        Some(baseline())).result, "switched");
    n2_cd::control_hit("cd-C-M-1-pre", 1_020);
    assert_eq!(n2_cd::pre(1_030, "cd-C-M-1", "hit", "ok").result, "hit");
    assert!(n2_cd::released());

    let cases: [(&str, Receipt<'static>); 8] = [
        // the switch baseline itself (same fd, same dup, same generation)
        ("generation-unchanged",
            Receipt { identity: baseline(), ..business_receipt(1_100) }),
        // a DIFFERENT fd NUMBER with the switch's generation: a number alone is
        // not proof of a new connection (numbers are reused)
        ("generation-unchanged",
            Receipt { identity: Identity::new(99, 100, 1), ..business_receipt(1_100) }),
        // the same descriptor re-protected (new generation, same numbers)
        ("identity-unchanged",
            Receipt { identity: Identity::new(21, 22, 2), ..business_receipt(1_100) }),
        // the right identity, the WRONG network
        ("net-id-mismatch", Receipt { net_id: 77, ..business_receipt(1_100) }),
        // an unmeasured network / generation / I/O time is refused, never guessed
        ("net-id-unmeasured", Receipt { net_id: 0, ..business_receipt(1_100) }),
        ("generation-unmeasured",
            Receipt { identity: Identity::new(41, 42, 0), ..business_receipt(1_100) }),
        ("io-time-unmeasured", Receipt { io_mono_ms: 0, ..business_receipt(0) }),
        // a completed connect is not a business write
        ("connect-only", Receipt { subsequent_io: false, ..business_receipt(1_100) }),
    ];
    for (reason, candidate) in cases {
        let refused = n2_cd::receipt(1_150, "cd-C-M-1", &candidate);
        assert_eq!(refused.result, "rejected", "{reason}: {}", refused.line);
        assert_eq!(refused.reason, reason, "{}", refused.line);
        assert!(refused.line.contains(&format!("|result=rejected|reason={reason}|")),
            "{}", refused.line);
        assert_eq!(n2_cd::active_phase(), "released",
            "a refusal is NON-terminal: the plan keeps waiting for a valid receipt");
    }
    // protect failure and an I/O before the pre HIT are refused too
    let unprotected = Receipt { protect_ok: false, ..business_receipt(1_100) };
    assert_eq!(n2_cd::receipt(1_160, "cd-C-M-1", &unprotected).reason, "protect-failed");
    let early_io = Receipt { io_mono_ms: 1_020, ..business_receipt(1_020) };
    assert_eq!(n2_cd::receipt(1_170, "cd-C-M-1", &early_io).reason, "io-before-pre-hit");
    assert_eq!(n2_cd::active_id(), "cd-C-M-1");

    // …and the plan is still completable by a valid receipt afterwards.
    let good = n2_cd::receipt(1_200, "cd-C-M-1", &business_receipt(1_150));
    assert_eq!(good.result, "receipt", "{}", good.line);
    assert!(good.line.contains("|net_id=31|protect_generation=2|identity=41:42:2|"), "{}",
        good.line);
    assert!(good.line.contains("|event_mono_ms=1150|local=10.77.0.5:51000|request_id=42|"),
        "{}", good.line);
    assert_eq!(n2_cd::poll(1_300).marker, "cd-C-M-1-post");
    disarm();
}

#[test]
fn a_wb_plan_still_owes_both_controls_and_needs_the_new_generation_protect_settle() {
    let _guard = serial();
    disarm();
    arm_clean();

    assert_eq!(n2_cd::plan(1_000, "cd-C-Wb-1", "C", "Wb", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").result, "planned");
    assert_eq!(n2_cd::switch_identified(1_010, "cd-C-Wb-1", "", "cellular", "10.77.0.5:0", 31,
        Some(Identity::new(51, 52, 1))).result, "switched");
    // Wb×C/D are exempted from the "real production send" criterion ONLY: the
    // switch no longer hands out `exempted`, the pre control is still owed.
    let ask = n2_cd::poll(1_020);
    assert_eq!(ask.step, "pre", "Wb still owes the pre control");
    assert_eq!(ask.marker, "cd-C-Wb-1-pre");
    n2_cd::control_hit("cd-C-Wb-1-pre", 1_030);
    assert_eq!(n2_cd::pre(1_040, "cd-C-Wb-1", "hit", "ok").result, "hit");
    assert!(n2_cd::released());

    // The base socket is structurally send-free, so the substitute is the new
    // generation's `phase=protect_settle|status=success`; without it nothing
    // completes.
    let settle = Receipt { identity: Identity::new(61, 62, 2), net_id: 31, protect_ok: true,
        io_mono_ms: 1_050, subsequent_io: false, protect_settle: false, detail: "protect_settle",
        remote: "unknown", local: "192.168.50.199:0", request_id: "wb" };
    let missing = n2_cd::receipt(1_060, "cd-C-Wb-1", &settle);
    assert_eq!(missing.result, "rejected");
    assert_eq!(missing.reason, "wb-protect-settle-missing");
    assert_eq!(n2_cd::active_phase(), "released");
    // the old generation is not the new settle either
    let old_generation = Receipt { identity: Identity::new(51, 52, 1), protect_settle: true,
        ..settle };
    assert_eq!(n2_cd::receipt(1_070, "cd-C-Wb-1", &old_generation).reason, "generation-unchanged");

    let settled = Receipt { protect_settle: true, ..settle };
    let exempted = n2_cd::receipt(1_080, "cd-C-Wb-1", &settled);
    assert_eq!(exempted.result, "exempted", "{}", exempted.line);
    assert!(exempted.line.contains("|result=exempted|reason=wb-protect-settle-success|"),
        "{}", exempted.line);
    // the post control is still owed, and a post without its own tee HIT is unsat
    let offer = n2_cd::poll(1_090);
    assert_eq!(offer.step, "receipt");
    assert_eq!(offer.marker, "cd-C-Wb-1-post");
    let unobserved = n2_cd::post(1_100, "cd-C-Wb-1", "observed", "ok");
    assert_eq!(unobserved.result, "unsat");
    assert_eq!(unobserved.reason, "post-hit-unobserved");
    disarm();
}

#[test]
fn a_repeated_plan_id_is_refused_and_the_legacy_switch_entry_is_fail_closed() {
    let _guard = serial();
    disarm();
    arm_clean();

    // D plans are accepted only on Wi-Fi (the host's hdc link) and only with a
    // measured start bearer.
    let wrong = n2_cd::plan(1_000, "cd-D-M-1", "D", "M", 1, "192.0.2.9:443", "cellular",
        "10.77.0.5:0", "");
    assert_eq!(wrong.result, "rejected");
    assert_eq!(wrong.reason, "start-bearer-not-wifi");
    let unknown = n2_cd::plan(1_010, "cd-D-M-1", "D", "M", 1, "192.0.2.9:443", "unknown",
        "unknown", "");
    assert_eq!(unknown.reason, "start-bearer-not-wifi");

    // A repeated id is refused both while the plan is live and after it closed.
    assert_eq!(n2_cd::plan(2_000, "cd-C-M-1", "C", "M", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").result, "planned");
    assert_eq!(n2_cd::plan(2_010, "cd-C-M-1", "C", "M", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").reason, "plan-already-active");
    assert_eq!(n2_cd::post(2_020, "cd-C-M-1", "observed", "ok").reason, "post-before-receipt");
    assert_eq!(n2_cd::plan(2_030, "cd-C-M-1", "C", "M", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").reason, "plan-already-used");

    // The legacy five-argument switch carries no identity facts: it is
    // fail-closed and the production-recovery gate never opens.
    assert_eq!(n2_cd::plan(3_000, "cd-C-M-2", "C", "M", 2, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").result, "planned");
    let legacy = n2_cd::switch(3_010, "cd-C-M-2", "", "cellular", "10.77.0.5:0");
    assert_eq!(legacy.result, "unsat");
    assert_eq!(legacy.reason, "switch-identity-required");
    assert!(!n2_cd::released());
    assert_eq!(n2_cd::active_id(), "");
    // the direction verdict the shell measured is still recorded verbatim
    assert_eq!(n2_cd::plan(4_000, "cd-C-M-3", "C", "M", 3, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").result, "planned");
    let mismatch = n2_cd::switch_identified(4_010, "cd-C-M-3", "direction-mismatch", "wifi",
        "192.168.50.199:0", 21, Some(baseline()));
    assert_eq!(mismatch.result, "unsat");
    assert_eq!(mismatch.reason, "direction-mismatch");
    disarm();
}

#[test]
fn an_unarmed_session_emits_nothing_at_all() {
    let _guard = serial();
    disarm();

    assert!(n2_cd::plan(1_000, "cd-C-M-1", "C", "M", 1, "192.0.2.9:443", "wifi",
        "192.168.50.199:0", "").line.is_empty());
    assert!(n2_cd::prepare(1_010, "cd-D-M-1", "", "cellular", "10.77.0.5:0").line.is_empty());
    assert!(n2_cd::ready(1_020, "cd-D-M-1").line.is_empty());
    assert!(n2_cd::switch_identified(1_030, "cd-C-M-1", "", "cellular", "10.77.0.5:0", 31,
        Some(baseline())).line.is_empty());
    assert!(n2_cd::pre(1_040, "cd-C-M-1", "hit", "ok").line.is_empty());
    assert!(n2_cd::receipt(1_050, "cd-C-M-1", &business_receipt(1_045)).line.is_empty());
    assert!(n2_cd::post(1_060, "cd-C-M-1", "observed", "ok").line.is_empty());
    assert_eq!(n2_cd::poll(1_070).step, "idle");
    assert_eq!(n2_cd::active_id(), "");
    assert_eq!(n2_cd::active_phase(), "");
    assert_eq!(n2_cd::plan_count(), 0);
    assert!(n2_cd::last_line().is_none(), "zero output while unarmed");
    disarm();
}
