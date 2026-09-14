// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N2-H isolation-evidence END-TO-END tests (offline, host-only).
//!
//! The full evidence pipeline (`netbird_core::n2h::run_loopback_check`) is
//! driven against a dual-instance loopback session: two REAL WireGuard
//! devices (synthetic fixed test keys) over loopback UDP with the socketpair
//! TUN stand-in, mock endpoint sinks for every outer family (management /
//! signal / STUN / TURN / relay / DNS), unique five-tuple + unique payload
//! probes, TUN package-level negative scan, tunnel positive controls,
//! endpoint-side receipts and counter reconciliation. Poll loops with bounded
//! deadlines only — no long sleeps as the primary mechanism.
//!
//! Proven here (the four required scenarios):
//! 1. clean run → `verdict = n2h-pass`, `tun_negative.hit == false`, positive
//!    controls observed inside the tunnel, all endpoint receipts observed,
//!    counters reconciled;
//! 2. **反例 A** — an outer probe payload replayed INTO the tunnel (a real
//!    route leak: it genuinely traverses the WG session and lands on the
//!    peer's TUN) MUST be caught → `n2h-fail` with the hit evidence naming
//!    the leaked probe id;
//! 3. **反例 B** — the positive control sent but routed OUTSIDE the peer's
//!    allowed_ips (never appears in the tunnel) MUST yield
//!    `n2h-inconclusive` — a missing positive control can never pass;
//! 4. a missing frozen endpoint (signal not obtained at freeze time) MUST
//!    yield `n2h-inconclusive`, name the missing kind, and skip session
//!    establishment (fail-closed per the N2-H draft).

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
        _object: *const c_void,
        _name: *const c_void,
        _value: *const c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_cb_info(
        _env: *mut c_void,
        _cbinfo: *const c_void,
        _argc: *mut usize,
        _argv: *mut *mut c_void,
        _this: *mut c_void,
        _data: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _env: *mut c_void,
        _str_: *const c_void,
        _len: usize,
        _result: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_string_utf8(
        _env: *mut c_void,
        _value: *const c_void,
        _buf: *mut c_void,
        _bufsize: usize,
        _result: *mut usize,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_int32(
        _env: *mut c_void,
        _value: *const c_void,
        _result: *mut i32,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(
        _env: *mut c_void,
        _value: *const c_void,
        _result: *mut bool,
    ) -> i32 {
        0
    }
}

use netbird_core::n2h::{
    self, EndpointKind, Evidence, LoopbackOpts, VERDICT_FAIL, VERDICT_INCONCLUSIVE, VERDICT_PASS,
};

/// boringtun's ffi installs a process-global panic→SIGSEGV hook the first
/// time `new_tunnel` runs (same shape as tests/wg_e2e.rs `show_panics`);
/// re-install a PRINTING hook so assertion failures stay diagnosable.
fn show_panics() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        std::panic::set_hook(Box::new(|info| {
            eprintln!("\nTEST PANIC: {info}");
        }));
    });
}

/// The evidence document must never carry the forbidden record tokens (原义务
/// 记 UNSAT, never waived/N/A) and must carry the fixed governance fields.
/// (Strict-reader parseability is pinned by the lib unit test
/// `n2h::tests::evidence_json_is_parseable_and_forbidden_word_free` —
/// `config::parse_document` is crate-private.)
fn assert_document_wellformed(ev: &Evidence) {
    let json = ev.to_json();
    for forbidden in ["protect pass", "N2 pass", "等同逐 socket protect", "waived", "N/A"] {
        assert!(!json.contains(forbidden), "forbidden token '{forbidden}' in evidence");
    }
    assert!(json.contains("UNSAT/未满足"), "unsat_note must be present");
    assert!(json.contains("route-exclusion"));
    assert!(json.contains("residual_scope"));
    assert!(json.contains("\"verdict\":"));
    assert!(json.contains("\"unsat_note\":"));
}

/// 场景 1 — 正常路径: dual-instance loopback → n2h-pass with a clean
/// negative scan, observed positive controls, endpoint receipts and
/// reconciled counters.
#[test]
fn loopback_pass_produces_full_evidence() {
    show_panics();
    let ev = n2h::run_loopback_check(&LoopbackOpts::default()).expect("loopback run");
    assert_eq!(ev.verdict, VERDICT_PASS, "reasons: {:?}", ev.reasons);
    assert!(!ev.tun_negative.hit, "outer probes must not appear in the tunnel");
    assert!(ev.tun_negative.frames_scanned > 0, "frames must actually be scanned");
    assert!(
        ev.tunnel_positive_control.observed_in_tunnel(),
        "positive controls must be observed inside the tunnel"
    );
    // every present endpoint has a sent probe with a unique five-tuple AND a
    // unique payload marker, and an endpoint-side receipt
    let mut tuples = std::collections::HashSet::new();
    let mut markers = std::collections::HashSet::new();
    for p in &ev.probes {
        assert!(p.sent, "probe {} must be sent", p.probe_id);
        assert_eq!(p.expected_path, "outer");
        assert!(tuples.insert(format!("{:?}", p.tuple)), "five-tuple must be unique");
        assert!(
            markers.insert(p.marker.as_ref().map(|m| format!("{m:?}"))),
            "payload marker must be unique"
        );
        let observed = ev.endpoint_side.iter().any(|r| r.probe_id == p.probe_id && r.observed);
        assert!(observed, "receipt observed for {}", p.probe_id);
    }
    assert_eq!(ev.probes.len(), 7, "one probe per outer family");
    // positive control probes carry expected_path=tunnel and a tunnel five-tuple
    for p in &ev.tunnel_positive_control.probes {
        assert_eq!(p.expected_path, "tunnel");
        assert!(p.tuple.dst.starts_with("10.77.0."), "positive control aims at the overlay");
    }
    // counters: two windows, deltas nonzero in the tunnel direction, reconciled
    assert_eq!(ev.counters.len(), 2);
    for w in &ev.counters {
        assert!(w.reconciled(), "reconciliation on {}: {:?}", w.side, ev.reasons);
    }
    let a = ev.counters.iter().find(|w| w.side == "a").expect("side a");
    assert!(a.delta().wg_rx_bytes_to_tun > 0, "tunnel traffic flowed into A's TUN");
    let b = ev.counters.iter().find(|w| w.side == "b").expect("side b");
    assert!(
        b.delta().wg_unknown_peer_drops == 1,
        "the wg_peer outer probe must be counted exactly once at B's outer socket"
    );
    assert_document_wellformed(&ev);
}

/// 场景 2 — 反例 A: the outer probe payload injected INTO the tunnel must be
/// caught (n2h-fail) with the hit evidence naming the leaked probe.
#[test]
fn leaked_outer_probe_is_caught_as_fail() {
    show_panics();
    let opts = LoopbackOpts { leak_outer_probe_into_tunnel: true, ..LoopbackOpts::default() };
    let ev = n2h::run_loopback_check(&opts).expect("loopback run");
    assert_eq!(ev.verdict, VERDICT_FAIL, "reasons: {:?}", ev.reasons);
    assert!(ev.tun_negative.hit, "the leak must show up as a tunnel hit");
    assert_eq!(ev.tun_negative.hits.len(), 1, "exactly the injected leak");
    let hit = &ev.tun_negative.hits[0];
    assert!(hit.probe_id.starts_with("N2H-"), "hit names the leaked probe: {hit:?}");
    // the hit frame traveled the tunnel: overlay src->dst on the peer's side
    assert_eq!(hit.src, [10, 77, 0, 1]);
    assert_eq!(hit.dst, [10, 77, 0, 2]);
    assert_eq!(hit.side, "b", "the leak lands on the peer's TUN");
    // the fail reason quotes the leaked probe id
    assert!(
        ev.reasons.iter().any(|r| r.contains(&hit.probe_id)),
        "fail reason names the probe: {:?}",
        ev.reasons
    );
    assert_document_wellformed(&ev);
}

/// 场景 3 — 反例 B: positive control sent but never observable in the tunnel
/// (routed outside allowed_ips) → n2h-inconclusive, NEVER pass.
#[test]
fn missing_positive_control_is_inconclusive() {
    show_panics();
    let opts = LoopbackOpts { suppress_positive_control: true, ..LoopbackOpts::default() };
    let ev = n2h::run_loopback_check(&opts).expect("loopback run");
    assert_eq!(ev.verdict, VERDICT_INCONCLUSIVE, "reasons: {:?}", ev.reasons);
    assert!(!ev.tunnel_positive_control.observed_in_tunnel());
    // the control WAS injected (sent) — it just can never traverse
    assert!(
        ev.tunnel_positive_control.probes.iter().any(|p| p.sent && p.observed.is_none()),
        "sent-but-unobserved positive control expected"
    );
    assert!(
        ev.reasons.iter().any(|r| r.contains("never observed")),
        "reason explains the missing observation: {:?}",
        ev.reasons
    );
    // the negative scan is still clean — inconclusive is about the CONTROL
    assert!(!ev.tun_negative.hit);
    assert_document_wellformed(&ev);
}

/// 场景 4 — 端点枚举缺失: the signal endpoint not obtained at freeze time →
/// n2h-inconclusive, the missing kind is named, and the session is NOT
/// established (fail-closed, per the N2-H draft).
#[test]
fn missing_signal_endpoint_is_inconclusive_and_fails_closed() {
    show_panics();
    let opts = LoopbackOpts { omit_kind: Some(EndpointKind::Signal), ..LoopbackOpts::default() };
    let ev = n2h::run_loopback_check(&opts).expect("loopback run");
    assert_eq!(ev.verdict, VERDICT_INCONCLUSIVE, "reasons: {:?}", ev.reasons);
    // the freeze entry records the absence WITH its reason
    let sig = ev
        .frozen_endpoints
        .iter()
        .find(|f| f.kind == Some(EndpointKind::Signal))
        .expect("signal freeze entry");
    assert!(!sig.present);
    assert!(sig.frozen_at_unix > 0, "the freeze attempt is still timestamped");
    assert!(!sig.absent_reason.is_empty(), "absence reason recorded");
    // the missing kind is named in the verdict reasons
    assert!(
        ev.reasons.iter().any(|r| r.contains("'signal'")),
        "missing kind named: {:?}",
        ev.reasons
    );
    // fail-closed: no session ⇒ no positive control was sent
    assert!(
        ev.tunnel_positive_control.probes.iter().all(|p| !p.sent),
        "no positive control without a session"
    );
    // the probes that COULD be frozen were still sent + received
    assert!(ev.probes.iter().all(|p| p.sent));
    assert!(ev.endpoint_side.iter().filter(|r| r.observed).count() >= 5);
    assert_document_wellformed(&ev);
}
