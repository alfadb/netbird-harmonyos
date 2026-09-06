//! The 14 frozen BoringTun ffi symbols (gate-plan :522-524) declared via
//! `extern "C"` blocks and address-referenced by a `#[used]` static so the
//! linker cannot GC them; all 14 must remain in dynsym for D1 (dlopen+dlsym
//! resolves them by name in this very .so).
//!
//! DATA PLANE IS NEVER CALLED (gate-plan :302): nothing in this crate invokes
//! any of these functions — the only use of each name is taking its address.
//! See DEVIATIONS in src/lib.rs for the A3 reading tension (address-only
//! references vs. the literal "no symbol references" wording) and the
//! alternative `-Wl,-u,<sym>` mechanism.
//!
//! Linkage anchor: rustc drops a dependency rlib from the cdylib link line
//! when the crate never references it through Rust metadata — the extern-block
//! aliases below are linker-level and invisible to that analysis, which would
//! leave the 14 symbols as undefined U imports (nothing defines them at load
//! time). The type-only re-export below marks boringtun as a used dependency
//! WITHOUT calling any ffi export symbol or any crypto at runtime.

/// Type-only metadata anchor onto `boringtun::x25519` (the crate's plain Rust
/// re-export module). Never called; no ffi export symbol is referenced.
pub fn _bt_metadata_anchor(_k: Option<boringtun::x25519::PublicKey>) {}

// Zero-arg alias declarations: each `#[link_name]` references the SAME symbol
// as the real frozen export, so the relocation below pins the symbol in
// dynsym. The true signatures (verified against boringtun-0.7.1/src/ffi/mod.rs)
// are documented inline; the aliases are never called.
extern "C" {
    // () -> x25519_key
    #[link_name = "x25519_secret_key"]
    fn _keep_x25519_secret_key();
    // (x25519_key) -> x25519_key
    #[link_name = "x25519_public_key"]
    fn _keep_x25519_public_key();
    // (x25519_key) -> *const c_char
    #[link_name = "x25519_key_to_base64"]
    fn _keep_x25519_key_to_base64();
    // (x25519_key) -> *const c_char
    #[link_name = "x25519_key_to_hex"]
    fn _keep_x25519_key_to_hex();
    // (*mut c_char)
    #[link_name = "x25519_key_to_str_free"]
    fn _keep_x25519_key_to_str_free();
    // (*const c_char) -> c_int
    #[link_name = "check_base64_encoded_x25519_key"]
    fn _keep_check_base64_encoded_x25519_key();
    // (unsafe extern "C" fn(*const c_char)) -> bool
    #[link_name = "set_logging_function"]
    fn _keep_set_logging_function();
    // (*const c_char, *const c_char, *const c_char, u16, c_uint) -> *mut c_void
    #[link_name = "new_tunnel"]
    fn _keep_new_tunnel();
    // (*mut c_void)
    #[link_name = "tunnel_free"]
    fn _keep_tunnel_free();
    // (*const c_void, *const u8, u32, *mut u8, u32) -> wireguard_result
    #[link_name = "wireguard_write"]
    fn _keep_wireguard_write();
    // (*const c_void, *const u8, u32, *mut u8, u32) -> wireguard_result
    #[link_name = "wireguard_read"]
    fn _keep_wireguard_read();
    // (*const c_void, *mut u8, u32) -> wireguard_result
    #[link_name = "wireguard_tick"]
    fn _keep_wireguard_tick();
    // (*const c_void, *mut u8, u32) -> wireguard_result
    #[link_name = "wireguard_force_handshake"]
    fn _keep_wireguard_force_handshake();
    // (*const c_void) -> bt_stats
    #[link_name = "wireguard_stats"]
    fn _keep_wireguard_stats();
}

/// The frozen 14-symbol list (order per gate-plan :522).
pub const BT_SYMBOLS: [&str; 14] = [
    "x25519_secret_key",
    "x25519_public_key",
    "x25519_key_to_base64",
    "x25519_key_to_hex",
    "x25519_key_to_str_free",
    "check_base64_encoded_x25519_key",
    "set_logging_function",
    "new_tunnel",
    "tunnel_free",
    "wireguard_write",
    "wireguard_read",
    "wireguard_tick",
    "wireguard_force_handshake",
    "wireguard_stats",
];

/// Compile-time forced references: address-taken function pointers prevent the
/// linker from GC-ing the 14 `#[no_mangle]` exports out of the cdylib's
/// dynsym. Addresses only — never dereferenced, never called.
#[used]
pub static BT_FFI_KEEP: [unsafe extern "C" fn(); 14] = [
    _keep_x25519_secret_key,
    _keep_x25519_public_key,
    _keep_x25519_key_to_base64,
    _keep_x25519_key_to_hex,
    _keep_x25519_key_to_str_free,
    _keep_check_base64_encoded_x25519_key,
    _keep_set_logging_function,
    _keep_new_tunnel,
    _keep_tunnel_free,
    _keep_wireguard_write,
    _keep_wireguard_read,
    _keep_wireguard_tick,
    _keep_wireguard_force_handshake,
    _keep_wireguard_stats,
];
