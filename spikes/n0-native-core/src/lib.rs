//! N0 native core — narrow C ABI smoke over BoringTun 0.7.1 `ffi-bindings`.
//!
//! Scope (docs/n0-native-client-feasibility.md, N0(b)):
//! - fixed BoringTun 0.7.1, `default-features = false`, `features = ["ffi-bindings"]`;
//! - NO `device` feature (socket2 0.4.10 `IovLen` is undefined on OHOS targets;
//!   not patched, per N0 stop condition 1);
//! - NO management/ICE/relay/UI/VPN/TUN/protect surface;
//! - arm64 is cross-compile only; no load claim.
//!
//! The smoke calls the real BoringTun C ABI functions (`x25519_secret_key`,
//! `x25519_public_key`, `x25519_key_to_base64`, `check_base64_encoded_x25519_key`,
//! `new_tunnel`, `wireguard_tick`, `tunnel_free`). Nothing is faked.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use std::ptr;

/// Static version string returned by `n0_probe_version` (never freed).
const N0_PROBE_VERSION: &[u8] = b"n0-native-core/0.1.0+boringtun-0.7.1\0";

/// Fixed BoringTun 0.7.1 crate checksum (crates.io), verified by build.sh.
pub const BORINGTUN_CRATE_SHA256: &str =
    "15dd6a8a89cbe8997f37ca0cf035e6ea4d64cd2ecea4aed83ffb9f99f7126939";

/// Result of the narrow C ABI smoke, filled by `n0_probe_smoke`.
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)] // C ABI name, mirrors boringtun's own ffi types
pub struct n0_smoke_result {
    /// 0 = all smoke checks passed; nonzero = fail-closed.
    pub ok: c_int,
    /// 1 = x25519 secret/public/base64/check passed.
    pub x25519_ok: c_int,
    /// 1 = new_tunnel + wireguard_tick passed (no WIREGUARD_ERROR).
    pub tunnel_ok: c_int,
    /// Last wireguard_tick op (boringtun `result_type`).
    pub tick_op: c_int,
    /// Last wireguard_tick size.
    pub tick_size: usize,
    /// Base64 of the derived public key, NUL-terminated.
    pub public_key_b64: [c_char; 64],
}

/// Returns the static version string. Never returns null; never freed.
#[no_mangle]
pub extern "C" fn n0_probe_version() -> *const c_char {
    N0_PROBE_VERSION.as_ptr() as *const c_char
}

/// Runs the narrow C ABI smoke and fills `out`. Returns `out.ok` (0 = pass).
///
/// # Safety
/// `out` must be a valid, writable `n0_smoke_result`.
#[no_mangle]
pub unsafe extern "C" fn n0_probe_smoke(out: *mut n0_smoke_result) -> c_int {
    if out.is_null() {
        return -1;
    }
    let result = run_smoke();
    *out = result;
    result.ok
}

fn run_smoke() -> n0_smoke_result {
    let mut result = n0_smoke_result {
        ok: 1,
        x25519_ok: 0,
        tunnel_ok: 0,
        tick_op: -1,
        tick_size: 0,
        public_key_b64: [0; 64],
    };

    // 1) Real BoringTun x25519 keygen + derive + base64 + validity check.
    let secret = boringtun::ffi::x25519_secret_key();
    let secret_key_bytes = secret.key; // [u8; 32] is Copy; keep a copy for base64 below
    let public = boringtun::ffi::x25519_public_key(secret);
    let public_b64_ptr = boringtun::ffi::x25519_key_to_base64(public);
    let secret_b64_ptr =
        boringtun::ffi::x25519_key_to_base64(boringtun::ffi::x25519_key { key: secret_key_bytes });
    if public_b64_ptr.is_null() || secret_b64_ptr.is_null() {
        if !public_b64_ptr.is_null() {
            unsafe { boringtun::ffi::x25519_key_to_str_free(public_b64_ptr as *mut c_char) };
        }
        if !secret_b64_ptr.is_null() {
            unsafe { boringtun::ffi::x25519_key_to_str_free(secret_b64_ptr as *mut c_char) };
        }
        return result;
    }

    let public_b64 = unsafe { CStr::from_ptr(public_b64_ptr) };
    let public_b64_bytes = public_b64.to_bytes();
    let valid = unsafe { boringtun::ffi::check_base64_encoded_x25519_key(public_b64_ptr) };
    let len = public_b64_bytes.len();
    if len <= 63 {
        for (i, byte) in public_b64_bytes.iter().enumerate() {
            result.public_key_b64[i] = *byte as c_char;
        }
        result.public_key_b64[len] = 0;
    }
    if valid != 1 || len != 44 {
        unsafe { boringtun::ffi::x25519_key_to_str_free(public_b64_ptr as *mut c_char) };
        unsafe { boringtun::ffi::x25519_key_to_str_free(secret_b64_ptr as *mut c_char) };
        return result;
    }
    result.x25519_ok = 1;

    // 2) Real BoringTun new_tunnel + wireguard_tick (ffi-bindings only; no device feature).
    let tunnel = unsafe {
        boringtun::ffi::new_tunnel(secret_b64_ptr, public_b64_ptr, ptr::null(), 0, 0)
    };
    unsafe { boringtun::ffi::x25519_key_to_str_free(public_b64_ptr as *mut c_char) };
    unsafe { boringtun::ffi::x25519_key_to_str_free(secret_b64_ptr as *mut c_char) };
    if tunnel.is_null() {
        return result;
    }

    let mut dst = [0u8; 2048];
    let tick =
        unsafe { boringtun::ffi::wireguard_tick(tunnel, dst.as_mut_ptr(), dst.len() as u32) };
    let tick_op = tick.op as i32;
    result.tick_op = tick_op;
    result.tick_size = tick.size;
    unsafe { boringtun::ffi::tunnel_free(tunnel) };
    // Fail-closed: only a WIREGUARD_DONE tick with size == 0 is a clean tunnel
    // pass. Any other op (WRITE_TO_NETWORK / WIREGUARD_ERROR / WRITE_TO_TUNNEL_*)
    // or a nonzero size keeps tunnel_ok = 0, so the ArkTS sub-check and the
    // PASS marker (tickOp=0 tickSize=0) can never be satisfied by a partial
    // or error tick.
    if tick_op != boringtun::ffi::result_type::WIREGUARD_DONE as i32 || tick.size != 0 {
        return result;
    }
    result.tunnel_ok = 1;
    result.ok = 0;
    result
}
