// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N2 T7a (item 8) — the executable N2 log-format baseline.
//!
//! `docs/n2-log-formats-20260928.md` is the human-readable field list; THIS
//! test is its machine-checkable half. It calls the REAL formatting functions
//! of the crate once per N2 line and compares the result, byte for byte, with
//! `tests/fixtures/n2-log-samples.txt`.
//!
//! Every volatile input (monotonic/wall clock, fd numbers, the socket's own
//! bound name) is passed IN as an argument by the formatters themselves — the
//! fixture therefore pins the FORMAT, not this run's clocks, which is exactly
//! what an executor/judge needs to parse the device logs.
//!
//! Regenerate after an intentional format change:
//!
//! ```text
//! N2_LOG_SAMPLES_REGEN=1 cargo test --offline --locked --test n2_log_samples
//! ```
//!
//! The file is written only by that explicit opt-in; otherwise a mismatch is a
//! test failure (so a silent format drift can never slip through).

use std::path::PathBuf;

use netbird_core::n2_a_event;
use netbird_core::n2_cd;
use netbird_core::n2_control;
use netbird_core::n2_ring;
use netbird_core::n2_sandwich;
use netbird_core::n2_socket_trace;
use netbird_core::wg_device::{self, N2DestinationTee, TunFrameDir, TunFrameTee};

// Host-process link stubs (repo convention, cf. tests/n2_stimulus.rs):
// libace_napi.z.so / libhilog_ndk.z.so do not exist on the host. No-ops for
// the linker only; never called by these tests.
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

/// The frozen capture destination of every sample.
const DST: [u8; 4] = [119, 29, 29, 119];

/// A 28-byte IPv4/UDP frame to `dst` (the shape the TUN tee scans).
fn udp_frame(dst: [u8; 4]) -> Vec<u8> {
    let mut out = vec![0u8; 28];
    out[0] = 0x45;
    out[2..4].copy_from_slice(&(28u16).to_be_bytes());
    out[8] = 64;
    out[9] = 17;
    out[12..16].copy_from_slice(&[10, 77, 0, 1]);
    out[16..20].copy_from_slice(&dst);
    out[20..22].copy_from_slice(&40_000u16.to_be_bytes());
    out[22..24].copy_from_slice(&53u16.to_be_bytes());
    out[24..26].copy_from_slice(&8u16.to_be_bytes());
    out
}

/// Drive a REAL `N2DestinationTee` through three from-OS frames — one capture
/// target, one decoded-but-not-a-target, one undecodable — so its continuity
/// line is the tee's own output, not a hand-written mirror.
fn tee_stats_sample() -> String {
    let tee = N2DestinationTee::new(vec![DST], 0);
    tee.on_tun_frame(TunFrameDir::FromOs, &udp_frame(DST));
    tee.on_tun_frame(TunFrameDir::FromOs, &udp_frame([10, 0, 0, 1]));
    tee.on_tun_frame(TunFrameDir::FromOs, &[0u8; 5]);
    tee.stats_line(5_000)
}

/// The one sample of every Rust-side N2 line, in the documented order.
fn samples() -> Vec<(&'static str, String)> {
    let dst = DST;
    let pair = n2_a_event::FdPair { fd: 21, dup_fd: 22, remote: "192.0.2.9:443".into() };
    vec![
        // ---- item 1: the I/Ws sandwich (pre carries the pre marker, post the
        // post one) ---------------------------------------------------------
        ("N2_SANDWICH phase=pre", n2_sandwich::format_line("I", "pre", "n2s-I-evt1-pre",
            "attach@1000", "registered", "ok", -1, -1, "unknown", 3_000, 1_001)),
        ("N2_SANDWICH phase=post", n2_sandwich::format_line("I", "post", "n2s-I-evt1-post",
            "first-send@2500", "observed", "control-hit-after-first-send", 6, 7,
            "192.0.2.9:3478", 90_000, 2_600)),
        // ---- item 2: the control registry ---------------------------------
        ("N2_CONTROL_SENT", n2_control::sent_line("n2s-I-evt1-pre", "119.29.29.119:3478",
            "udp", 51_000, 1_000)),
        ("N2_CONTROL_HIT", n2_control::hit_line("n2s-I-evt1-pre", dst, 3_478, 17, 1_005)),
        ("N2_CONTROL_MISS", n2_control::miss_line("n2s-I-evt1-post", dst, 3_478, 17, 4_005)),
        ("N2_TUN_LEAK", n2_control::leak_line(dst, 3_478, 17, 1, 4_100)),
        // ---- item 3: TUN tee continuity -----------------------------------
        ("N2_TUN_TEE_STATS", tee_stats_sample()),
        ("N2_TUN_GAP", wg_device::gap_line(1_000, 2_000, "before-tun-attach")),
        ("N2_TUN_DEST", wg_device::dest_line(5_000, dst, 17, 53, 3, 1, "10.77.0.1")),
        ("N2_TUN_DEST zero heartbeat", wg_device::dest_zero_line(5_000, dst)),
        ("N2_TUN_FRAMES", wg_device::frames_line(5_000, 9, 2, 7, 0)),
        // ---- items 4/5: the A-column event --------------------------------
        ("N2_A_EVENT phase=begin (immediate)", n2_a_event::format_line("M", "a-M-evt1",
            &pair, "begin", "begin", "management-rpc", "get-server-key", 6_000)),
        ("N2_A_EVENT phase=end", n2_a_event::format_line("M", "a-M-evt1", &pair, "end",
            "sent", "ok", "get-server-key", 6_010)),
        ("N2_A_EVENT phase=begin (S watch)", n2_a_event::format_line("S", "a-S-evt1",
            &pair, "begin", "armed", "signal-heartbeat-watch", "wait_ms=45000", 7_000)),
        ("N2_A_EVENT phase=end observed", n2_a_event::format_line("S", "a-S-evt1", &pair,
            "end", "observed", "watchdog-heartbeat-sent", "natural-heartbeat", 27_000)),
        ("N2_A_EVENT phase=end timeout", n2_a_event::format_line("R", "a-R-evt1", &pair,
            "end", "timeout", "healthcheck-wait-exceeded", "wait_ms=90000", 97_000)),
        ("N2_A_EVENT gate refusal", n2_a_event::format_line("M", "a-M-evt2",
            &n2_a_event::FdPair::unknown(), "end", "rejected", "not-armed", "gate", 8_000)),
        // ---- items 4/6: the socket receipts -------------------------------
        ("N2_SOCK phase=subsequent_io (forced)",
            n2_socket_trace::subsequent_io_line("M", "42", 21, 22, true, 9, 2, "unknown",
                "192.0.2.1:443", 31, 64, 6_005, 1_700_000_000_000, "a-M-evt1")),
        ("N2_SOCK phase=subsequent_io (throttled shape, no event)",
            n2_socket_trace::subsequent_io_line("S", "7", 21, 22, true, 3, 1,
                "192.168.1.2:51000", "192.0.2.1:443", 31, 12, 27_000,
                1_700_000_000_000, "none")),
        ("N2_SOCK phase=first_io", n2_socket_trace::sock_receipt_line("I", "42", 7, 8, true,
            9, 2, 123, 456, "192.168.1.2:5000", "192.0.2.1:3478", "udp", 31, "sent",
            "stun_txn_abcd", "first_io")),
        // N2 frozen E1 (Wb substitute evidence, 2026-09-30): the row the CORE
        // emits on every takeover of a protected base socket — `proto=17`, the
        // validated core dup and the protect-settle instant (`protect_mono_ms`);
        // it deliberately carries NO wall field, because the only wall the core
        // could name at emission is the later takeover clock.
        ("N2_SOCK phase=protect_settle (Wb core takeover)",
            n2_socket_trace::sock_receipt_line("Wb", "wg_12", 21, 22, true, 2, 4, 1_234, 0,
                "192.168.50.199:51820", "unknown", "17", 21, "success", "core-adopt",
                "protect_settle")),
        ("N2_FD_SEND", n2_socket_trace::fd_send_fields_line("Ws", 7, 8, 2, 288, 6_001)),
        // ---- item 6: the data-plane adoption -------------------------------
        ("N7_WG_FEED adopted", wg_device::wg_feed_adopted_line(2, 1, 1, 2_377)),
        // ---- N2 first batch C/D columns (2026-09-29): the App-side switching
        // sandwich — one sample per phase. The Wb row is NO LONGER an exemption
        // from the controls: its receipt is the new generation's protect settle -
        ("N2_CD_SANDWICH phase=plan", n2_cd::format_line("cd-C-M-1", "C", "M", 1, "plan",
            "none", "planned", "ok", "wifi", "unknown", "192.168.50.199:0", "unknown",
            -1, -1, "unknown", n2_cd::SWITCH_WAIT_MS, 1_000)),
        ("N2_CD_SANDWICH phase=plan (D, pre-set on Wi-Fi)",
            n2_cd::format_line("cd-D-M-1", "D", "M", 1, "plan", "none", "planned",
                "await-start-bearer", "wifi", "unknown", "192.168.50.199:0", "unknown",
                -1, -1, "unknown", n2_cd::PREPARE_WAIT_MS, 1_010)),
        ("N2_CD_SANDWICH phase=prepare prepared (D, the Wi-Fi -> cellular hop)",
            n2_cd::format_line("cd-D-M-1", "D", "M", 1, "prepare", "none", "prepared",
                "not-a-d-measurement", "wifi", "cellular", "192.168.50.199:0", "10.77.0.5:0",
                -1, -1, "unknown", n2_cd::PREPARE_WAIT_MS, 1_050)),
        ("N2_CD_SANDWICH phase=prepare ready (D, the cellular-side recovery)",
            n2_cd::format_line("cd-D-M-1", "D", "M", 1, "prepare", "none", "ready",
                "preparation-recovered", "wifi", "cellular", "192.168.50.199:0",
                "10.77.0.5:0", -1, -1, "unknown", n2_cd::SWITCH_WAIT_MS, 1_080)),
        ("N2_CD_SANDWICH phase=pre", n2_cd::format_line("cd-C-M-1", "C", "M", 1, "pre",
            "cd-C-M-1-pre", "hit", "ok", "wifi", "unknown", "192.168.50.199:0", "unknown",
            -1, -1, "unknown", 3_000, 1_100)),
        ("N2_CD_SANDWICH phase=switch", n2_cd::format_line("cd-C-M-1", "C", "M", 1, "switch",
            "none", "switched", "ok", "wifi", "cellular", "192.168.50.199:0", "10.77.0.5:0",
            -1, -1, "unknown", n2_cd::RECEIPT_WAIT_MS, 1_200)),
        ("N2_CD_SANDWICH phase=receipt", n2_cd::format_line("cd-C-M-1", "C", "M", 1, "receipt",
            "none", "receipt", "ok", "wifi", "cellular", "192.168.50.199:0", "10.77.0.5:0",
            21, 22, "192.0.2.9:443", n2_cd::RECEIPT_WAIT_MS, 1_400)),
        ("N2_CD_SANDWICH phase=post", n2_cd::format_line("cd-C-M-1", "C", "M", 1, "post",
            "cd-C-M-1-post", "observed", "control-hit-after-receipt", "wifi", "cellular",
            "192.168.50.199:0", "10.77.0.5:0", 21, 22, "192.0.2.9:443", n2_cd::POST_WAIT_MS,
            1_600)),
        ("N2_CD_SANDWICH phase=receipt exempted (Wb, its new-generation protect settle)",
            n2_cd::format_line("cd-C-Wb-1", "C", "Wb", 1, "receipt", "none", "exempted",
                "wb-protect-settle-success", "wifi", "cellular", "192.168.50.199:0",
                "10.77.0.5:0", 31, 32, "unknown", n2_cd::POST_WAIT_MS, 1_800)),
        // ---- N2 first batch C/D columns (2026-09-29): the evidence ring ----
        ("N2_RING_ARMED", n2_ring::armed_line(n2_ring::DEFAULT_FILE_NAME,
            n2_ring::DEFAULT_MAX_BYTES, n2_ring::DEFAULT_SEGMENTS, 0, 1_000)),
        ("N2_RING_STATS", n2_ring::stats_line(n2_ring::DEFAULT_FILE_NAME, true, 4_096,
            n2_ring::DEFAULT_MAX_BYTES, n2_ring::DEFAULT_SEGMENTS, 12, 3, 1, "errno-28",
            2_000)),
        ("N2_RING_WRITE_FAILED", n2_ring::fail_line(1, "errno-28", 1_500)),
        ("N2_RING_RECOVERED", n2_ring::recovered_line(2, 2_500)),
        ("N2_RING_ROTATED (terse form)", n2_ring::rotated_line(3, 4_194_304, 3_000)),
        ("N2_RING_ROTATED (the span the product writes)", n2_ring::rotated_span_line(
            3, 4_194_304, 11, 40, 30, 40, 3_000)),
        ("N2_RING_LOST", n2_ring::lost_line(5, 6, 2, "write-failed", "errno-28", 1_500)),
        ("N2_RING_OVERFLOW", n2_ring::overflow_line(7, 5, 11, 4, 1_500)),
        ("N2_RING_CHUNK", n2_ring::chunk_line(7, 2, "N2_TUN_DEST|scope=session|frames=3")),
        ("N2_RING_DUMP", n2_ring::dump_summary_line(&n2_ring::DumpPage {
            result: "dumped", reason: "ok", from_seq: 0, to_seq: 9, count: 2,
            frozen_tail_seq: 12, first_seq: 1, last_seq: 12, records: 12, bytes: 512,
            malformed: 0, lost_records: 0, overflow: false, acked_seq: 4,
            session: Some("arm-1".into()), lines: Vec::new() }, 2)),
    ]
}

/// The fixture file with one line of context per sample.
fn render() -> String {
    let mut out = String::from(
        "# N2 T7a (item 8) N2 log-format baseline — generated by\n\
         # client/core/tests/n2_log_samples.rs from the REAL formatters.\n\
         # One sample per Rust-side N2 line; volatile inputs are fixed arguments.\n\
         # Shell-side lines (N2_*_SHELL, phase=open/protect_begin/protect_settle,\n\
         # VPN_WG_DATAPLANE_FED, VPN_NETWORK_*, N2_RING_DUMP_SHELL, the arm-ticket lines)\n\
         # are NOT producible from Rust and are documented in docs/n2-log-formats-20260928.md.\n\
         # N2_RING_MANIFEST embeds the live clock (manifest_line_now) and therefore has no\n\
         # fixed-argument formatter; its field sequence is listed in that doc §9.\n");
    for (label, line) in samples() {
        out.push_str(&format!("# {label}\n{line}\n"));
    }
    out
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/n2-log-samples.txt")
}

#[test]
fn n2_log_samples_fixture_matches_the_real_formatters() {
    let path = fixture_path();
    let rendered = render();
    if std::env::var("N2_LOG_SAMPLES_REGEN").is_ok() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).expect("fixtures dir");
        }
        std::fs::write(&path, &rendered).expect("write fixture");
        eprintln!("regenerated {}", path.display());
        return;
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("read {}: {error} (regenerate with N2_LOG_SAMPLES_REGEN=1)", path.display())
    });
    assert_eq!(on_disk, rendered,
        "{} is stale — regenerate with N2_LOG_SAMPLES_REGEN=1 and review the diff", path.display());
}

/// The fixture must actually contain every line the doc promises (a silently
/// dropped sample is a format baseline with a hole in it).
#[test]
fn the_fixture_covers_every_rust_side_n2_line() {
    let rendered = render();
    for needle in ["N2_SANDWICH|", "N2_CONTROL_SENT|", "N2_CONTROL_HIT|", "N2_CONTROL_MISS|",
        "N2_TUN_LEAK|", "N2_TUN_TEE_STATS|", "N2_TUN_GAP|", "N2_TUN_DEST|", "N2_TUN_FRAMES|",
        "N2_A_EVENT|", "N2_SOCK|", "N2_FD_SEND|", "N7_WG_FEED|adopted|",
        "N2_CD_SANDWICH|", "N2_RING_ARMED|", "N2_RING_STATS|", "N2_RING_WRITE_FAILED|",
        "N2_RING_RECOVERED|", "N2_RING_ROTATED|", "N2_RING_LOST|", "N2_RING_OVERFLOW|",
        "N2_RING_CHUNK|", "N2_RING_DUMP|"] {
        assert!(rendered.contains(needle), "the baseline is missing {needle}");
    }
    // item 1 + item 4: the two fields the review asked for are actually there.
    assert!(rendered.contains("|phase=post|marker=n2s-I-evt1-post|"), "post marker");
    assert!(rendered.contains("|phase=pre|marker=n2s-I-evt1-pre|"), "pre marker");
    assert!(rendered.contains("|event_id=a-M-evt1|diag=true|is_evidence=false"),
        "the forced receipt carries the event id");
    assert!(rendered.contains("N2_CONTROL_HIT|id=n2s-I-evt1-pre|marker=n2s-I-evt1-pre|"),
        "the control hit carries both the request id and the matched marker");
    assert!(rendered.contains("N2_TUN_GAP|from_mono=1000|to_mono=2000|reason=before-tun-attach|"),
        "the gap field names");
    assert!(rendered.contains("N7_WG_FEED|adopted|peers=2|endpoints=1|egress_replayed=1|mono_ms=2377"),
        "the adopted line fields");
    // N2 frozen E1 (Wb substitute evidence, 2026-09-30): the core's own
    // protect-settle row is pinned — the frozen E1 join fields, `proto=17`, the
    // protect instant, and NO wall field (an adopt-time wall must never be
    // presented as the protect wall).
    let settle = rendered
        .lines()
        .find(|line| line.starts_with("N2_SOCK|phase=protect_settle|"))
        .expect("the core protect-settle row is in the baseline");
    assert!(settle.contains(
        "|kind=Wb|request_id=wg_12|fd=21|dup_fd=22|protect=success|protect_ms=2|protect_generation=4|protect_mono_ms=1234|local=192.168.50.199:51820|remote=unknown|proto=17|net_id=21|status=success|detail=core-adopt|diag=true|is_evidence=false"),
        "the frozen E1 Wb protect-settle row: {settle}");
    assert!(!settle.contains("wall_ms") && !settle.contains("first_mono_ms"),
        "the protect-settle row claims no wall clock: {settle}");
    // the shell-side lines are named in the header (they cannot be produced by
    // a Rust formatter) so the baseline states the boundary explicitly
    assert!(rendered.contains("# Shell-side lines (N2_*_SHELL, phase=open/protect_begin/protect_settle,"),
        "the shell-side boundary note");
}
