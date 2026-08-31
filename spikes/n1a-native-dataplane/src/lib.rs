//! N1a gate data-plane probe — real BoringTun 0.7.1 `ffi-bindings` loopback
//! pump behind a narrow C ABI.
//!
//! Scope (docs/n1a-gate-plan.md; criteria C1-C9 are pre-registered there and
//! must not be changed):
//! - same fixed BoringTun 0.7.1 as N0, `default-features = false`,
//!   `features = ["ffi-bindings"]`; NO `device` feature, no socket2 patch;
//! - NO management/ICE/relay/UI/VPN/TUN/protect surface — the probe's outer
//!   transport is a plain in-process 127.0.0.1 UDP loopback channel;
//! - aarch64 is cross-compile only; no load claim (x86_64 Emulator gate).
//!
//! C ABI:
//! - `n1a_probe_version() -> const char*`       (static, never freed)
//! - `n1a_dataplane_probe() -> *mut n1a_dataplane_result`
//! - `n1a_result_free(*mut n1a_dataplane_result)`  (frees struct + JSON)
//!
//! The pump itself lives in [`pump`]; the same code runs as the host unit
//! test (`cargo test` on x86_64-linux-gnu drives the full real loopback:
//! handshake, 10x200x1024 both directions, tick gaps, backpressure, cleanup)
//! and behind this C ABI on the OHOS x86_64 Emulator.

pub mod pump;

use std::os::raw::{c_char, c_double, c_int};
use std::ptr;

/// Static version string returned by `n1a_probe_version` (never freed).
const N1A_PROBE_VERSION: &[u8] = b"n1a-native-dataplane/0.1.0+boringtun-0.7.1\0";

/// Structured result of one probe run, returned by `n1a_dataplane_probe`.
///
/// Mirrors the N0 convention: `ok == 0` means PASS (fail-closed), and the
/// full machine-readable detail is the NUL-terminated JSON in `json`
/// (owned; freed by `n1a_result_free`).
#[repr(C)]
pub struct n1a_dataplane_result {
    /// 0 = all criteria pass (C5 not-triggered counts as non-failure);
    /// nonzero = fail.
    pub ok: c_int,
    /// c1..c9 statuses: 1 = pass, 0 = fail, 2 = not-triggered (C5 only).
    pub criteria: [c_int; 9],
    /// Verified plaintext packets over both directions (expected 2000).
    pub verified_packets_total: c_int,
    /// Byte mismatches found in C3 (expected 0).
    pub mismatch_count: c_int,
    /// Packets lost in C3 (expected 0).
    pub lost_count: c_int,
    /// 1 when C5 actually observed EAGAIN saturation, 0 when not triggered.
    pub backpressure_triggered: c_int,
    /// C4 throughput over the pure pump window, MiB/s.
    pub throughput_mib_per_sec: c_double,
    /// C4 pure pump time, milliseconds.
    pub pump_ms: c_double,
    /// C7/C8: /proc/self/fd entry count before the pump and after cleanup.
    pub fd_baseline: c_int,
    pub fd_after: c_int,
    /// NUL-terminated JSON detail document (owned by the result).
    pub json: *mut c_char,
}

/// Returns the static version string. Never returns null; never freed.
#[no_mangle]
pub extern "C" fn n1a_probe_version() -> *const c_char {
    N1A_PROBE_VERSION.as_ptr() as *const c_char
}

/// Runs the full N1a data-plane pump (real BoringTun ffi + UDP loopback
/// channel) and returns an owned result, or NULL on allocation failure.
///
/// The call is synchronous and single-threaded; worst-case duration is
/// bounded by the pre-registered timeboxes (30 s handshake + 10 s
/// backpressure) plus the fixed data/tick work.
///
/// # Safety
/// The returned pointer (if non-null) must be released exactly once with
/// `n1a_result_free`. No other thread may free it.
#[no_mangle]
pub unsafe extern "C" fn n1a_dataplane_probe() -> *mut n1a_dataplane_result {
    let outcome = std::panic::catch_unwind(pump::run_probe).unwrap_or_else(|_| panic_outcome());
    let json = match std::ffi::CString::new(pump::to_json(&outcome)) {
        Ok(j) => j,
        Err(_) => match std::ffi::CString::new(
            "{\"ok\":false,\"verdict\":\"fail\",\"error\":\"json marshal failed\"}",
        ) {
            Ok(j) => j,
            Err(_) => return ptr::null_mut(),
        },
    };
    let json_ptr = json.into_raw();
    if json_ptr.is_null() {
        return ptr::null_mut();
    }
    let boxed = Box::new(n1a_dataplane_result {
        ok: if outcome.ok { 0 } else { 1 },
        criteria: outcome.criteria.map(|v| v as c_int),
        verified_packets_total: outcome.verified_packets_total as c_int,
        mismatch_count: outcome.mismatch_count as c_int,
        lost_count: outcome.lost_count as c_int,
        backpressure_triggered: if outcome.backpressure_triggered { 1 } else { 0 },
        throughput_mib_per_sec: outcome.throughput_mib_s,
        pump_ms: outcome.pump_ms,
        fd_baseline: outcome.fd_baseline.map(|v| v as c_int).unwrap_or(-1),
        fd_after: outcome.fd_after.map(|v| v as c_int).unwrap_or(-1),
        json: json_ptr,
    });
    Box::into_raw(boxed)
}

/// Outcome substituted when the probe itself panics (defensive; the pump is
/// written to avoid panics, and boringtun's ffi installs a SIGSEGV panic hook
/// on first `new_tunnel`, so this path is expected to be unreachable).
fn panic_outcome() -> pump::ProbeOutcome {
    let mut o = pump::ProbeOutcome::new(None, None);
    o.error = Some("probe panicked".to_string());
    o
}

/// Frees a result returned by `n1a_dataplane_probe` (struct + JSON string).
///
/// # Safety
/// `result` must be null or a pointer handed out by `n1a_dataplane_probe`
/// that has not been freed yet.
#[no_mangle]
pub unsafe extern "C" fn n1a_result_free(result: *mut n1a_dataplane_result) {
    if result.is_null() {
        return;
    }
    let boxed = Box::from_raw(result);
    if !boxed.json.is_null() {
        drop(std::ffi::CString::from_raw(boxed.json));
    }
    drop(boxed);
}

// ---------------------------------------------------------------------------
// Host tests (cargo test on x86_64-linux-gnu): the strongest self-test — the
// full real pump runs on the host with genuine BoringTun tunnels and a
// genuine loopback UDP channel.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::pump;

    fn parse_expected_fields(json: &str) -> Vec<(&'static str, bool)> {
        // Minimal structural checks without a JSON dependency: every key the
        // NAPI/ohosTest layers rely on must be present.
        ["\"version\"", "\"ok\"", "\"verdict\"", "\"criteria\"", "\"handshake\"",
         "\"integrity\"", "\"throughput\"", "\"backpressure\"", "\"tick\"",
         "\"resources\"", "\"timing\""]
            .iter()
            .map(|k| (*k, json.contains(k)))
            .collect()
    }

    /// Full real pump on the host: C2 handshake + C3 10x200x1024 both
    /// directions byte-verified + C4 throughput + C5 backpressure + C6 tick
    /// gaps + C7/C8 resource baseline. Print the JSON (cargo test -- --nocapture)
    /// to read the measured throughput and timing numbers.
    #[test]
    fn full_pump_probe_passes_on_host() {
        let outcome = pump::run_probe();
        let json = pump::to_json(&outcome);
        println!("N1A host probe JSON: {json}");

        assert!(outcome.probe_entered, "probe must run");
        assert_eq!(outcome.criteria[0], pump::CRIT_PASS, "c1 (lib load) must pass on host");
        assert_eq!(
            outcome.criteria[1], pump::CRIT_PASS,
            "c2 handshake must pass: error={:?}",
            outcome.error
        );
        assert!(outcome.handshake_established);
        assert!(outcome.first_roundtrip_ok);
        let sa = outcome.stats_a_after.expect("stats A recorded");
        let sb = outcome.stats_b_after.expect("stats B recorded");
        assert!(sa.time_since_last_handshake >= 0 && sb.time_since_last_handshake >= 0,
            "both tunnels must report an established session");

        assert_eq!(outcome.criteria[2], pump::CRIT_PASS, "c3 integrity must pass: {:?} {}", outcome.first_mismatch, json);
        assert_eq!(outcome.verified_packets_total, outcome.expected_packets_total);
        assert_eq!(outcome.mismatch_count, 0);
        assert_eq!(outcome.lost_count, 0);
        assert_eq!(outcome.verified_plaintext_bytes, 2 * 10 * 200 * 1024);
        // Frozen C3(d): per-direction tx/rx byte accounting must be exact.
        let expected_inner: u64 = 10 * 200 * 1024;
        assert!(outcome.byte_accounting_ok, "c3 byte accounting must be asserted: {}", json);
        assert_eq!(outcome.tx_bytes_delta_a, expected_inner);
        assert_eq!(outcome.rx_bytes_delta_b, expected_inner);
        assert_eq!(outcome.tx_bytes_delta_b, expected_inner);
        assert_eq!(outcome.rx_bytes_delta_a, expected_inner);

        assert_eq!(outcome.criteria[3], pump::CRIT_PASS, "c4 throughput must pass");
        assert!(outcome.throughput_mib_s >= pump::THROUGHPUT_FLOOR_MIB_S);

        // C5: pass-induced or honestly not-triggered; corrupted packets
        // always fail. Frozen semantics: induced is decided ONLY by real
        // errno EAGAIN/ENOBUFS; on this loopback topology sendto usually
        // succeeds while the kernel silently drops full-queue datagrams, so
        // the honest host expectation is not-triggered with
        // kernel_queue_drops as a pure observation.
        assert!(
            outcome.criteria[4] == pump::CRIT_PASS || outcome.criteria[4] == pump::CRIT_NOT_TRIGGERED,
            "c5 must be pass-induced or honestly not-triggered: {}",
            json
        );
        assert_eq!(outcome.bp_corrupted, 0);
        assert_eq!(outcome.bp_received, pump::BACKPRESSURE_BURST);
        assert_eq!(
            outcome.backpressure_triggered,
            outcome.eagain_count > 0,
            "induced must exactly track errno hits (EAGAIN/ENOBUFS); \
             kernel drops are observation-only"
        );

        assert_eq!(outcome.criteria[5], pump::CRIT_PASS, "c6 tick gaps must pass");
        assert!(outcome.tick_gap_count >= 3);
        assert!(outcome.tick_call_count >= 6);
        // Frozen C6: keep_alive=1 means each >=1s quiet gap's real
        // wireguard_tick must emit persistent keepalives; the probe-side
        // counter must record >= 3 ticks returning WRITE_TO_NETWORK.
        assert!(
            outcome.tick_network_packets >= 3,
            "persistent keepalive path must fire (keep_alive=1): {}",
            outcome.tick_network_packets
        );
        assert!(outcome.tick_session_ok_after_each);
        assert!(outcome.post_gap_burst_ok);

        assert_eq!(outcome.criteria[6], pump::CRIT_PASS, "c7 resources must pass: {}", json);
        assert_eq!(outcome.criteria[7], pump::CRIT_PASS, "c8 cleanup must pass: {}", json);
        let (fb, fa, tb, ta) = (
            outcome.fd_baseline.expect("fd baseline"),
            outcome.fd_after.expect("fd after"),
            outcome.thread_baseline.expect("thread baseline"),
            outcome.thread_after.expect("thread after"),
        );
        // Frozen C7: T3 == T0 EXACT equality (growth or shrinkage both fail).
        // Run the suite serially (--test-threads=1) so no unrelated harness
        // thread churns the process-wide /proc counts mid-probe; the criteria
        // are not relaxed for host-test convenience.
        assert_eq!(fa, fb, "fd count must be exactly equal: {fb} -> {fa}");
        assert_eq!(ta, tb, "thread count must be exactly equal: {tb} -> {ta}");
        assert_eq!(outcome.tunnels_freed, 2);
        assert_eq!(outcome.sockets_closed, 2);

        assert!(outcome.ok, "aggregate verdict must be pass: {}", json);
    }

    /// Synthetic packets are well-formed IPv4 with a valid header checksum,
    /// deterministic, and carry the (direction, round, seq) marker.
    #[test]
    fn synth_packets_are_deterministic_marked_ipv4() {
        for &(dir, round, seq) in &[
            (0u32, 0u32, 0u32),
            (1, 9, 199),
            (2, 0, 511),
            (0, 10, 12345),
        ] {
            let p = pump::synth_packet(dir, round, seq);
            assert_eq!(p[0] >> 4, 4, "IPv4 version nibble");
            assert_eq!(p[0] & 0x0f, 5, "IHL 5");
            assert_eq!(u16::from_be_bytes([p[2], p[3]]), 1024, "total_length == payload");
            assert_eq!(&p[20..24], &dir.to_be_bytes(), "direction marker");
            assert_eq!(&p[24..28], &round.to_be_bytes(), "round marker");
            assert_eq!(&p[28..32], &seq.to_be_bytes(), "seq marker");
            // Deterministic rebuild.
            assert_eq!(p, pump::synth_packet(dir, round, seq), "packet must be deterministic");
            // Distinct (dir, round, seq) produce distinct payloads.
            let q = pump::synth_packet(dir, round, seq + 1);
            assert_ne!(p[32..], q[32..], "payload pattern must depend on the marker");
        }
    }

    /// The JSON document is structurally sound: balanced braces/brackets,
    /// no control characters outside strings, and every layer's required key
    /// is present.
    #[test]
    fn json_document_is_structurally_sound() {
        let outcome = pump::run_probe();
        let json = pump::to_json(&outcome);

        let mut depth_objects = 0i64;
        let mut depth_arrays = 0i64;
        let mut in_string = false;
        let mut escaped = false;
        for c in json.chars() {
            if in_string {
                match c {
                    '\\' => escaped = !escaped,
                    '"' if !escaped => in_string = false,
                    _ => {}
                }
                if c != '\\' {
                    escaped = false;
                }
                continue;
            }
            match c {
                '"' => in_string = true,
                '{' => depth_objects += 1,
                '}' => depth_objects -= 1,
                '[' => depth_arrays += 1,
                ']' => depth_arrays -= 1,
                _ => {}
            }
            assert!(depth_objects >= 0 && depth_arrays >= 0, "unbalanced JSON: {json}");
        }
        assert_eq!(depth_objects, 0, "object braces must balance");
        assert_eq!(depth_arrays, 0, "array brackets must balance");
        assert!(!in_string, "strings must be closed");

        for (key, present) in parse_expected_fields(&json) {
            assert!(present, "JSON must contain {key}: {json}");
        }
        assert!(json.contains("\"verdict\":\"pass\"") || json.contains("\"verdict\":\"fail\""));
    }

    /// A 44-char base64 key roundtrip through the real ffi pair is what the
    /// pump feeds `new_tunnel` (sanity check of the key-generation helper
    /// path used by the probe).
    #[test]
    fn ffi_keypair_produces_valid_base64_keys() {
        let secret = boringtun::ffi::x25519_secret_key();
        let secret_bytes = secret.key;
        let public = boringtun::ffi::x25519_public_key(secret);
        let secret_ptr = boringtun::ffi::x25519_key_to_base64(boringtun::ffi::x25519_key { key: secret_bytes });
        let public_ptr = boringtun::ffi::x25519_key_to_base64(public);
        assert!(!secret_ptr.is_null() && !public_ptr.is_null());
        unsafe {
            let secret_b64 = std::ffi::CStr::from_ptr(secret_ptr).to_bytes();
            let public_b64 = std::ffi::CStr::from_ptr(public_ptr).to_bytes();
            assert_eq!(secret_b64.len(), 44);
            assert_eq!(public_b64.len(), 44);
            assert_eq!(boringtun::ffi::check_base64_encoded_x25519_key(public_ptr), 1);
            assert_ne!(secret_b64, public_b64);
            boringtun::ffi::x25519_key_to_str_free(secret_ptr as *mut _);
            boringtun::ffi::x25519_key_to_str_free(public_ptr as *mut _);
        }
    }
}
