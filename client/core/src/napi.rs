//! Handwritten NAPI bindings (no napi-rs dependency; promoted from
//! spikes/n1b-disc-phys-hap/probe/src/napi.rs with the export surface trimmed
//! to the product skeleton: version / abi_probe / fd_status / wg_* / tun_* /
//! phone_log_*).
//! Every EXPORT is synchronous JSON->JSON except `socket_supply_subscribe` /
//! `socket_supply_unsubscribe`, which own the crate's ONE threadsafe function:
//! the socket-supply wake — the pushed form of the official client's platform
//! callback (upstream `client/android/client.go` hands the Go core
//! `tunAdapter.ProtectSocket`, `client/net/protectsocket_android.go` calls it
//! synchronously from the dial path; the host never polls for "do you need a
//! socket?"). The wake carries no payload: the shell drains the request queue
//! on it. There is still ZERO `napi_create_async_work`, and both facts are
//! gated by `client/core/build.sh` at the ELF symbol level. Symbols resolve at
//! load time from the host process's libace_napi.z.so (DT_NEEDED via
//! .cargo/config.toml link args).

use core::ffi::c_void;
use core::time::Duration;
use std::sync::{Arc, Mutex};

pub type NapiEnv = *mut c_void;
pub type NapiValue = *mut c_void;
pub type NapiCallbackInfo = *mut c_void;
pub type NapiFinalize = Option<unsafe extern "C" fn(NapiEnv, *mut c_void, *mut c_void)>;

extern "C" {
    fn napi_module_register(mod_: *mut NapiModule);
    fn napi_create_function(
        env: NapiEnv,
        utf8name: *const u8,
        length: usize,
        cb: NapiCallback,
        data: *mut c_void,
        result: *mut NapiValue,
    ) -> i32;
    fn napi_set_named_property(
        env: NapiEnv,
        object: NapiValue,
        utf8name: *const u8,
        value: NapiValue,
    ) -> i32;
    fn napi_get_cb_info(
        env: NapiEnv,
        info: NapiCallbackInfo,
        argc: *mut usize,
        argv: *mut NapiValue,
        this_arg: *mut NapiValue,
        data: *mut *mut c_void,
    ) -> i32;
    fn napi_create_string_utf8(env: NapiEnv, str_: *const u8, length: usize, result: *mut NapiValue) -> i32;
    fn napi_get_value_string_utf8(
        env: NapiEnv,
        value: NapiValue,
        buf: *mut u8,
        bufsize: usize,
        result: *mut usize,
    ) -> i32;
    fn napi_get_value_int32(env: NapiEnv, value: NapiValue, result: *mut i32) -> i32;
    fn napi_get_value_bool(env: NapiEnv, value: NapiValue, result: *mut bool) -> i32;
    // The socket-supply wake (see the module docs): the crate's only
    // threadsafe-function surface. `napi_threadsafe_function` is an opaque
    // handle, so it crosses as `*mut c_void` and NAPI owns its lifetime.
    fn napi_get_undefined(env: NapiEnv, result: *mut NapiValue) -> i32;
    fn napi_call_function(
        env: NapiEnv,
        recv: NapiValue,
        func: NapiValue,
        argc: usize,
        argv: *const NapiValue,
        result: *mut NapiValue,
    ) -> i32;
    fn napi_create_threadsafe_function(
        env: NapiEnv,
        func: NapiValue,
        async_resource: NapiValue,
        async_resource_name: NapiValue,
        max_queue_size: usize,
        initial_thread_count: usize,
        thread_finalize_data: *mut c_void,
        thread_finalize_cb: NapiFinalize,
        context: *mut c_void,
        call_js_cb: Option<unsafe extern "C" fn(NapiEnv, NapiValue, *mut c_void, *mut c_void)>,
        result: *mut *mut c_void,
    ) -> i32;
    fn napi_call_threadsafe_function(
        func: *mut c_void,
        data: *mut c_void,
        is_blocking: i32,
    ) -> i32;
    fn napi_release_threadsafe_function(func: *mut c_void, mode: i32) -> i32;
}

/// `napi_threadsafe_function_call_mode` (`node_api_types.h`): never block the
/// calling core thread on a wake.
const NAPI_TSFN_NONBLOCKING: i32 = 0;
/// `napi_threadsafe_function_release_mode`: drop wakes still queued at teardown
/// instead of delivering them into a shell that is already tearing down.
const NAPI_TSFN_ABORT: i32 = 1;

// ---------------------------------------------------------------------------
// Host-only fallbacks for the wake's five runtime functions.
//
// On the OHOS target these resolve from the host process's libace_napi.z.so at
// load time, and `client/core/build.sh` asserts them as REAL imports of the
// shipped artifact (`target_env = "ohos"`, the cfg the OHOS targets carry — note
// `target_os` is still "linux" there). A host build has no NAPI runtime: the
// crate's own unit tests drive the wake through `host_stubs` below, and every
// integration-test binary carries its own block of host stubs for the
// pre-existing surface. These INERT fallbacks (always an error status, never a
// fake success: a subscription attempted on the host answers `ok:false`) make
// that surface complete for the ONE threadsafe function without copying five
// more stubs into 40+ test binaries. They are never compiled into the shipped
// .so.
// ---------------------------------------------------------------------------
#[cfg(all(not(test), not(target_env = "ohos")))]
mod host_napi_fallbacks {
    use super::*;

    #[no_mangle]
    pub extern "C" fn napi_create_threadsafe_function(
        _env: NapiEnv,
        _func: NapiValue,
        _async_resource: NapiValue,
        _async_resource_name: NapiValue,
        _max_queue_size: usize,
        _initial_thread_count: usize,
        _thread_finalize_data: *mut c_void,
        _thread_finalize_cb: NapiFinalize,
        _context: *mut c_void,
        _call_js_cb: Option<unsafe extern "C" fn(NapiEnv, NapiValue, *mut c_void, *mut c_void)>,
        _result: *mut *mut c_void,
    ) -> i32 {
        1 // napi_invalid_arg
    }

    #[no_mangle]
    pub extern "C" fn napi_call_threadsafe_function(
        _func: *mut c_void,
        _data: *mut c_void,
        _is_blocking: i32,
    ) -> i32 {
        1
    }

    #[no_mangle]
    pub extern "C" fn napi_release_threadsafe_function(_func: *mut c_void, _mode: i32) -> i32 {
        1
    }

    #[no_mangle]
    pub extern "C" fn napi_get_undefined(_env: NapiEnv, _result: *mut NapiValue) -> i32 {
        1
    }

    #[no_mangle]
    pub extern "C" fn napi_call_function(
        _env: NapiEnv,
        _recv: NapiValue,
        _func: NapiValue,
        _argc: usize,
        _argv: *const NapiValue,
        _result: *mut NapiValue,
    ) -> i32 {
        1
    }
}

#[repr(C)]
struct NapiModule {
    nm_version: i32,
    nm_flags: u32,
    nm_filename: *const u8,
    nm_register_func: Option<unsafe extern "C" fn(NapiEnv, NapiValue) -> NapiValue>,
    nm_modname: *const u8,
    nm_priv: *mut c_void,
    reserved: [*mut c_void; 4],
}

// raw pointers are opaque registration data written once before any read
unsafe impl Sync for NapiModule {}

pub type NapiCallback = unsafe extern "C" fn(NapiEnv, NapiCallbackInfo) -> NapiValue;

unsafe extern "C" fn napi_init(env: NapiEnv, exports: NapiValue) -> NapiValue {
    macro_rules! reg {
        ($name:expr, $f:expr) => {
            let mut r: NapiValue = core::ptr::null_mut();
            let name = concat!($name, "\0").as_bytes();
            let rc = napi_create_function(
                env,
                name.as_ptr(),
                name.len() - 1,
                $f,
                core::ptr::null_mut(),
                &mut r,
            );
            if rc != 0 {
                // registration failure cannot be reported through NAPI itself
            } else {
                let rc = napi_set_named_property(env, exports, name.as_ptr(), r);
                if rc != 0 {
                    // property attach failure cannot be reported through NAPI itself
                }
            }
        };
    }
    reg!("version", napi_version);
    reg!("abi_probe", napi_abi_probe);
    reg!("fd_status", napi_fd_status);
    reg!("wg_probe", napi_wg_probe);
    reg!("wg_udp_probe", napi_wg_udp_probe);
    reg!("wg_net_probe", napi_wg_net_probe);
    reg!("wg_fwd_probe", napi_wg_fwd_probe);
    reg!("wg_fwd_open", napi_wg_fwd_open);
    reg!("wg_fwd_run", napi_wg_fwd_run);
    reg!("tun_open", napi_tun_open);
    reg!("tun_read", napi_tun_read);
    reg!("tun_write", napi_tun_write);
    reg!("tun_poll", napi_tun_poll);
    reg!("tun_close", napi_tun_close);
    reg!("config_validate", napi_config_validate);
    reg!("mgmt_socket_open", napi_mgmt_socket_open);
    reg!("ice_socket_open", napi_ice_socket_open);
    reg!("connector_start", napi_connector_start);
    reg!("connector_start_with_socket", napi_connector_start_with_socket);
    reg!("connector_physical_network_changed", napi_connector_physical_network_changed);
    reg!("connector_management_address_update", napi_connector_management_address_update);
    reg!("connector_protected_dns_start", napi_connector_protected_dns_start);
    reg!("connector_protected_dns_poll", napi_connector_protected_dns_poll);
    reg!("connector_socket_feed", napi_connector_socket_feed);
    reg!("connector_ice_mux_arm", napi_connector_ice_mux_arm);
    reg!("connector_signal_socket_feed", napi_connector_signal_socket_feed);
    reg!("socket_supply_poll", napi_socket_supply_poll);
    reg!("socket_supply_subscribe", napi_socket_supply_subscribe);
    reg!("socket_supply_unsubscribe", napi_socket_supply_unsubscribe);
    reg!("socket_supply_reply", napi_socket_supply_reply);
    reg!("socket_supply_close_fd", napi_socket_supply_close_fd);
    reg!("socket_supply_shutdown", napi_socket_supply_shutdown);
    reg!("socket_supply_dns_servers", napi_socket_supply_dns_servers);
    reg!("connector_wg_socket_feed", napi_connector_wg_socket_feed);
    reg!("connector_tun_fd_feed", napi_connector_tun_fd_feed);
    reg!("connector_tun_release", napi_connector_tun_release);
    reg!("connector_route_set_applied", napi_connector_route_set_applied);
    reg!("connector_status", napi_connector_status);
    reg!("connector_resource_status", napi_connector_resource_status);
    reg!("connector_network_config", napi_connector_network_config);
    reg!("connector_stop", napi_connector_stop);
    reg!("connector_logout", napi_connector_logout);
    reg!("n2_socket_trace_arm", napi_n2_socket_trace_arm);
    reg!("n2_socket_trace_protect", napi_n2_socket_trace_protect);
    reg!("n2_tun_dest_configure", napi_n2_tun_dest_configure);
    reg!("n2_control_send", napi_n2_control_send);
    reg!("n2_control_status", napi_n2_control_status);
    reg!("n2_stimulus", napi_n2_stimulus);
    reg!("n2_a_event", napi_n2_a_event);
    reg!("n2_sandwich_pre", napi_n2_sandwich_pre);
    reg!("n2_sandwich_post", napi_n2_sandwich_post);
    reg!("n2_sandwich_poll", napi_n2_sandwich_poll);
    reg!("n2_cd_plan", napi_n2_cd_plan);
    reg!("n2_cd_prepare", napi_n2_cd_prepare);
    reg!("n2_cd_ready", napi_n2_cd_ready);
    reg!("n2_cd_pre", napi_n2_cd_pre);
    reg!("n2_cd_switch_identified", napi_n2_cd_switch_identified);
    reg!("n2_cd_identity", napi_n2_cd_identity);
    reg!("n2_cd_poll", napi_n2_cd_poll);
    reg!("n2_cd_post", napi_n2_cd_post);
    reg!("n2_ring_configure", napi_n2_ring_configure);
    reg!("n2_ring_append", napi_n2_ring_append);
    reg!("n2_ring_disable", napi_n2_ring_disable);
    reg!("n2_ring_status", napi_n2_ring_status);
    reg!("n2_ring_dump", napi_n2_ring_dump);
    reg!("n2_ring_manifest", napi_n2_ring_manifest);
    reg!("n2_ring_ack", napi_n2_ring_ack);
    reg!("n2h_collector_arm", napi_n2h_collector_arm);
    reg!("n2h_collector_status", napi_n2h_collector_status);
    reg!("n2h_collector_evidence", napi_n2h_collector_evidence);
    reg!("n2h_collector_disarm", napi_n2h_collector_disarm);
    reg!("n2h_probe_socket_open", napi_n2h_probe_socket_open);
    reg!("n2h_probe_socket_commit", napi_n2h_probe_socket_commit);
    reg!("phone_log_configure", napi_phone_log_configure);
    reg!("phone_log_append", napi_phone_log_append);
    reg!("phone_log_status", napi_phone_log_status);
    reg!("phone_log_shutdown", napi_phone_log_shutdown);
    reg!("account_auth_discovery_start", napi_account_auth_discovery_start);
    reg!("account_auth_discovery_poll", napi_account_auth_discovery_poll);
    reg!("account_auth_discovery_cancel", napi_account_auth_discovery_cancel);
    // 2026-10-12: the login action's management step (the JWT registration the
    // official Android flow runs before the engine starts). Appended after the
    // discovery trio — append-only, every pre-existing name keeps its position.
    reg!("account_login_start", napi_account_login_start);
    reg!("account_login_poll", napi_account_login_poll);
    reg!("account_login_cancel", napi_account_login_cancel);
    // 2026-10-13: the interactive SESSION EXTEND's management step (upstream
    // `client/android/session.go:127-155` → `engine_authsession.go:83-107`).
    // Same one-shot job shape as the login action; append-only.
    reg!("account_extend_start", napi_account_extend_start);
    reg!("account_extend_poll", napi_account_extend_poll);
    reg!("account_extend_cancel", napi_account_extend_cancel);
    // Increment 131: dedicated route catalog (append-only at the tail of the
    // registration list — every pre-existing name keeps its position).
    reg!("connector_routes", napi_connector_routes);
    // Increment 132: the live route-selection setter — appended after the
    // catalog getter it mutates (still append-only: every pre-existing name
    // keeps its position and its created function object).
    reg!("connector_route_selection_set", napi_connector_route_selection_set);
    // Additive Rust-heap readout: appended after the route surface (still
    // append-only — every pre-existing name keeps its position and its created
    // function object).
    reg!("alloc_stats", napi_alloc_stats);
    // 0.0.34 peer-status lag fix (device report §5 F1): the edge-triggered
    // status publication surface — the subscribe/unsubscribe pair owning the
    // crate's SECOND threadsafe function, plus the cheap revision getter.
    // Appended at the TAIL of the registration list (append-only: every
    // pre-existing name keeps its position and its created function object).
    reg!("connector_status_subscribe", napi_connector_status_subscribe);
    reg!("connector_status_unsubscribe", napi_connector_status_unsubscribe);
    reg!("connector_status_revision", napi_connector_status_revision);
    exports
}

// .init_array constructor: registers the module when the host runtime dlopens
// this .so (equivalent of the C NAPI_MODULE macro's __attribute__((constructor))).
unsafe extern "C" fn register_module() {
    static MODULE: NapiModule = NapiModule {
        nm_version: 1,
        nm_flags: 0,
        nm_filename: b"netbird_core\0".as_ptr(),
        nm_register_func: Some(napi_init),
        nm_modname: b"netbird_core\0".as_ptr(),
        nm_priv: core::ptr::null_mut(),
        reserved: [core::ptr::null_mut(); 4],
    };
    napi_module_register(&MODULE as *const NapiModule as *mut NapiModule);
}

#[used]
#[link_section = ".init_array"]
static REGISTER_MODULE_FN: unsafe extern "C" fn() = register_module;

// ---------------------------------------------------------------------------
// argument helpers
// ---------------------------------------------------------------------------

/// The exported-argument view. `argv` must be able to hold the LONGEST export
/// signature: `n2_cd_plan` / `n2_cd_switch_identified` take EIGHT arguments
/// (`napi_get_cb_info` never writes more than the capacity it is handed, so a
/// smaller array silently reads the tail as "missing").
struct Args {
    env: NapiEnv,
    argv: [NapiValue; 8],
    argc: usize,
}

fn cb_args(env: NapiEnv, info: NapiCallbackInfo) -> Args {
    let mut argv: [NapiValue; 8] = [core::ptr::null_mut(); 8];
    let mut argc: usize = 8;
    unsafe {
        napi_get_cb_info(
            env,
            info,
            &mut argc,
            argv.as_mut_ptr(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
    }
    Args { env, argv, argc }
}

impl Args {
    fn i32_at(&self, idx: usize, default: i32) -> i32 {
        if idx >= self.argc {
            return default;
        }
        let mut v: i32 = default;
        unsafe { napi_get_value_int32(self.env, self.argv[idx], &mut v) };
        v
    }
    fn bool_at(&self, idx: usize, default: bool) -> bool {
        if idx >= self.argc {
            return default;
        }
        let mut v = default;
        unsafe { napi_get_value_bool(self.env, self.argv[idx], &mut v) };
        v
    }
    fn string_at(&self, idx: usize) -> String {
        if idx >= self.argc {
            return String::new();
        }
        unsafe {
            let mut written: usize = 0;
            // probe: get required size (bufsize 0 leaves the value unread)
            let rc = napi_get_value_string_utf8(self.env, self.argv[idx], core::ptr::null_mut(), 0, &mut written);
            if rc != 0 || written == 0 {
                return String::new();
            }
            let mut buf = vec![0u8; written + 1];
            let rc = napi_get_value_string_utf8(self.env, self.argv[idx], buf.as_mut_ptr(), buf.len(), &mut written);
            if rc != 0 {
                return String::new();
            }
            String::from_utf8_lossy(&buf[..written]).into_owned()
        }
    }

    /// Bounded `string_at`: `None` when the argument is missing, is not a
    /// string, is empty, or is LONGER than `max_bytes`. The size probe runs
    /// before the copy, so an oversized JS string is refused instead of being
    /// copied into a Rust buffer (bounded argument validation for the
    /// `phone_log_*` surface).
    fn string_at_bounded(&self, idx: usize, max_bytes: usize) -> Option<String> {
        if idx >= self.argc {
            return None;
        }
        unsafe {
            let mut written: usize = 0;
            let rc = napi_get_value_string_utf8(self.env, self.argv[idx], core::ptr::null_mut(), 0, &mut written);
            if rc != 0 || written == 0 || written > max_bytes {
                return None;
            }
            let mut buf = vec![0u8; written + 1];
            let rc = napi_get_value_string_utf8(self.env, self.argv[idx], buf.as_mut_ptr(), buf.len(), &mut written);
            if rc != 0 {
                return None;
            }
            Some(String::from_utf8_lossy(&buf[..written]).into_owned())
        }
    }
}

fn ret_json(env: NapiEnv, json: String) -> NapiValue {
    let mut out: NapiValue = core::ptr::null_mut();
    let bytes = json.as_bytes();
    unsafe {
        napi_create_string_utf8(env, bytes.as_ptr(), bytes.len(), &mut out);
    }
    out
}

// Host-test link surface (cfg(test) only, never part of the cdylib): the test
// binary links the whole crate on the host triple, where libace_napi.z.so does
// not exist. Stubs satisfy the linker; the registration test records attachment
// through napi_set_named_property. The argument stubs are DRIVABLE (`set_string`
// / `set_bool` / `set_call_args`) and the string-creation stub hands back a
// readable handle (`created_string`), so a callback wrapper can be exercised
// end to end on the host — argument arity, string/bool reads and the JSON reply
// — while native runtime behavior itself is still not exercised.
#[cfg(test)]
mod host_stubs {
    use super::*;
    use core::ffi::CStr;
    use std::cell::RefCell;
    use std::collections::HashMap;

    thread_local! {
        // name -> fake function value returned by napi_create_function
        static FAKE_FUNCS: RefCell<HashMap<String, usize>> = RefCell::new(HashMap::new());
        // (object, name, value) recorded by napi_set_named_property
        static SET_CALLS: RefCell<Vec<(usize, String, usize)>> = RefCell::new(Vec::new());
        // NapiValue handle -> UTF-8 payload the string stub hands back
        static FAKE_STRINGS: RefCell<HashMap<usize, Vec<u8>>> = RefCell::new(HashMap::new());
        // callback-info handle -> the argument handles ONE armed invocation sees
        // (`set_call_args`); absent = the pre-existing "no cb info on the host"
        // behavior every other test relies on.
        static CALL_ARGS: RefCell<HashMap<usize, Vec<usize>>> = RefCell::new(HashMap::new());
        // NapiValue handle -> boolean the `napi_get_value_bool` stub writes
        static BOOLS: RefCell<HashMap<usize, bool>> = RefCell::new(HashMap::new());
        // NapiValue handle handed back by `napi_create_string_utf8` -> payload
        static CREATED_STRINGS: RefCell<HashMap<usize, String>> = RefCell::new(HashMap::new());
        static NEXT_STRING_HANDLE: RefCell<usize> = RefCell::new(0);
    }

    pub fn reset() {
        FAKE_FUNCS.with(|m| m.borrow_mut().clear());
        SET_CALLS.with(|c| c.borrow_mut().clear());
        FAKE_STRINGS.with(|m| m.borrow_mut().clear());
        CALL_ARGS.with(|m| m.borrow_mut().clear());
        BOOLS.with(|m| m.borrow_mut().clear());
        CREATED_STRINGS.with(|m| m.borrow_mut().clear());
        NEXT_STRING_HANDLE.with(|c| *c.borrow_mut() = 0);
        TSFS.with(|m| m.borrow_mut().clear());
        NEXT_TSFN.with(|c| *c.borrow_mut() = 0);
        TSFN_CALLS.with(|c| c.borrow_mut().clear());
        TSFN_RELEASES.with(|c| c.borrow_mut().clear());
        JS_CALLS.with(|c| c.borrow_mut().clear());
    }

    /// Register a fake string argument for the `napi_get_value_string_utf8`
    /// stub, so `Args::string_at*` (probe + copy + byte cap) can be driven on
    /// the host without an NAPI runtime.
    pub fn set_string(handle: usize, text: &str) {
        FAKE_STRINGS.with(|m| m.borrow_mut().insert(handle, text.as_bytes().to_vec()));
    }

    /// Register a fake BOOLEAN argument for the `napi_get_value_bool` stub (an
    /// unregistered handle is "not a boolean", so the caller keeps its default).
    pub fn set_bool(handle: usize, value: bool) {
        BOOLS.with(|m| m.borrow_mut().insert(handle, value));
    }

    /// Arm ONE callback invocation: the wrapper called with `info` as its
    /// `NapiCallbackInfo` sees exactly `args` — `argc` = the number of arguments
    /// PASSED (the real `napi_get_cb_info` contract, which is what an optional
    /// argument's presence hinges on) and `argv` filled in order. At most the 8
    /// slots `Args` reserves.
    pub fn set_call_args(info: usize, args: &[usize]) {
        assert!(args.len() <= 8, "the host harness arms at most 8 arguments");
        CALL_ARGS.with(|m| m.borrow_mut().insert(info, args.to_vec()));
    }

    /// The payload of a `NapiValue` handed back by the
    /// `napi_create_string_utf8` stub — i.e. the JSON a `ret_json` wrapper
    /// returned from an armed invocation.
    pub fn created_string(value: usize) -> Option<String> {
        CREATED_STRINGS.with(|m| m.borrow().get(&value).cloned())
    }

    pub fn fake_funcs() -> HashMap<String, usize> {
        FAKE_FUNCS.with(|m| m.borrow().clone())
    }

    pub fn set_calls() -> Vec<(usize, String, usize)> {
        SET_CALLS.with(|c| c.borrow().clone())
    }

    fn c_name(p: *const u8) -> String {
        unsafe { CStr::from_ptr(p as *const core::ffi::c_char) }
            .to_string_lossy()
            .into_owned()
    }

    #[no_mangle]
    pub extern "C" fn napi_module_register(_mod_: *mut NapiModule) {}

    #[no_mangle]
    pub extern "C" fn napi_create_function(
        _env: NapiEnv,
        utf8name: *const u8,
        _length: usize,
        _cb: NapiCallback,
        _data: *mut c_void,
        result: *mut NapiValue,
    ) -> i32 {
        let name = c_name(utf8name);
        // non-null, distinct fake function object per name
        let fake = FAKE_FUNCS.with(|m| {
            let mut m = m.borrow_mut();
            let v = 0x1000usize + m.len() + 1;
            m.insert(name, v);
            v
        });
        unsafe { *result = fake as NapiValue };
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_set_named_property(
        _env: NapiEnv,
        object: NapiValue,
        utf8name: *const u8,
        value: NapiValue,
    ) -> i32 {
        let name = c_name(utf8name);
        SET_CALLS.with(|c| c.borrow_mut().push((object as usize, name, value as usize)));
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_cb_info(
        _env: NapiEnv,
        info: NapiCallbackInfo,
        argc: *mut usize,
        argv: *mut NapiValue,
        _this_arg: *mut NapiValue,
        _data: *mut *mut c_void,
    ) -> i32 {
        // Only an ARMED invocation reports argument info; an unarmed one leaves
        // the caller's argc/argv untouched (the previous stub behavior).
        let Some(args) = CALL_ARGS.with(|m| m.borrow().get(&(info as usize)).cloned()) else {
            return 0;
        };
        unsafe {
            if !argc.is_null() {
                *argc = args.len();
            }
            if !argv.is_null() {
                for (index, handle) in args.iter().enumerate() {
                    *argv.add(index) = *handle as NapiValue;
                }
            }
        }
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _env: NapiEnv,
        str_: *const u8,
        length: usize,
        result: *mut NapiValue,
    ) -> i32 {
        let bytes = if str_.is_null() || length == 0 {
            &[][..]
        } else {
            unsafe { core::slice::from_raw_parts(str_, length) }
        };
        let text = String::from_utf8_lossy(bytes).into_owned();
        // a non-null, distinct handle per created string, readable back through
        // `created_string`
        let handle = NEXT_STRING_HANDLE.with(|c| {
            let mut n = c.borrow_mut();
            *n += 1;
            0x9000_0000usize + *n
        });
        CREATED_STRINGS.with(|m| m.borrow_mut().insert(handle, text));
        unsafe { *result = handle as NapiValue };
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_string_utf8(
        _env: NapiEnv,
        value: NapiValue,
        buf: *mut u8,
        bufsize: usize,
        result: *mut usize,
    ) -> i32 {
        let bytes = FAKE_STRINGS.with(|m| m.borrow().get(&(value as usize)).cloned());
        // A handle with no registered payload is "not a string" (non-zero rc).
        let Some(bytes) = bytes else { return 1 };
        unsafe {
            if buf.is_null() || bufsize == 0 {
                *result = bytes.len();
                return 0;
            }
            let n = bytes.len().min(bufsize - 1);
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, n);
            *buf.add(n) = 0;
            *result = n;
        }
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_int32(_env: NapiEnv, _value: NapiValue, _result: *mut i32) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(_env: NapiEnv, value: NapiValue, result: *mut bool) -> i32 {
        // An unregistered handle is "not a boolean" (non-zero rc): the caller
        // keeps its default, exactly like the real API on a wrong-typed value.
        let Some(v) = BOOLS.with(|m| m.borrow().get(&(value as usize)).copied()) else {
            return 1;
        };
        unsafe { *result = v };
        0
    }

    // ---- the socket-supply wake's threadsafe-function surface ----------------
    // The host has no ArkTS runtime, so these stubs model the runtime's OBSERVABLE
    // contract: `napi_call_threadsafe_function` dispatches the stored `call_js`
    // inline (that is the path under test) and `napi_call_function` records the
    // JS invocation. Everything else (queueing, threads) is the runtime's, not
    // ours to fake.

    type CallJs = unsafe extern "C" fn(NapiEnv, NapiValue, *mut c_void, *mut c_void);

    struct FakeTsfn {
        env: NapiEnv,
        func: NapiValue,
        call_js: Option<CallJs>,
    }

    thread_local! {
        static TSFS: RefCell<HashMap<usize, FakeTsfn>> = RefCell::new(HashMap::new());
        static NEXT_TSFN: RefCell<usize> = RefCell::new(0);
        // (handle, data) per napi_call_threadsafe_function
        static TSFN_CALLS: RefCell<Vec<(usize, usize)>> = RefCell::new(Vec::new());
        // (handle, release mode) per napi_release_threadsafe_function
        static TSFN_RELEASES: RefCell<Vec<(usize, i32)>> = RefCell::new(Vec::new());
        // (recv, func, argc) per napi_call_function
        static JS_CALLS: RefCell<Vec<(usize, usize, usize)>> = RefCell::new(Vec::new());
    }

    pub fn tsfn_calls() -> Vec<(usize, usize)> {
        TSFN_CALLS.with(|c| c.borrow().clone())
    }

    pub fn tsfn_releases() -> Vec<(usize, i32)> {
        TSFN_RELEASES.with(|c| c.borrow().clone())
    }

    pub fn js_calls() -> Vec<(usize, usize, usize)> {
        JS_CALLS.with(|c| c.borrow().clone())
    }

    pub fn live_tsfns() -> usize {
        TSFS.with(|m| m.borrow().len())
    }

    #[no_mangle]
    pub extern "C" fn napi_create_threadsafe_function(
        env: NapiEnv,
        func: NapiValue,
        _async_resource: NapiValue,
        _async_resource_name: NapiValue,
        _max_queue_size: usize,
        _initial_thread_count: usize,
        _thread_finalize_data: *mut c_void,
        _thread_finalize_cb: NapiFinalize,
        _context: *mut c_void,
        call_js_cb: Option<CallJs>,
        result: *mut *mut c_void,
    ) -> i32 {
        let handle = NEXT_TSFN.with(|c| {
            let mut n = c.borrow_mut();
            *n += 1;
            0x4000usize + *n
        });
        TSFS.with(|m| m.borrow_mut().insert(handle, FakeTsfn { env, func, call_js: call_js_cb }));
        unsafe { *result = handle as *mut c_void };
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_call_threadsafe_function(
        func: *mut c_void,
        data: *mut c_void,
        _is_blocking: i32,
    ) -> i32 {
        let handle = func as usize;
        TSFN_CALLS.with(|c| c.borrow_mut().push((handle, data as usize)));
        // An unknown handle is `napi_invalid_arg` (1) — this is what a wake from
        // a thread that never saw the handle gets, and it must not panic.
        let entry = TSFS.with(|m| m.borrow().get(&handle).map(|e| (e.env, e.func, e.call_js)));
        match entry {
            Some((env, func, Some(call_js))) => {
                unsafe { call_js(env, func, core::ptr::null_mut(), data) };
                0
            }
            Some(_) => 0,
            None => 1,
        }
    }

    #[no_mangle]
    pub extern "C" fn napi_release_threadsafe_function(func: *mut c_void, mode: i32) -> i32 {
        let handle = func as usize;
        TSFN_RELEASES.with(|c| c.borrow_mut().push((handle, mode)));
        if TSFS.with(|m| m.borrow_mut().remove(&handle)).is_some() { 0 } else { 1 }
    }

    #[no_mangle]
    pub extern "C" fn napi_get_undefined(_env: NapiEnv, result: *mut NapiValue) -> i32 {
        unsafe { *result = 0x7000usize as NapiValue };
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_call_function(
        _env: NapiEnv,
        recv: NapiValue,
        func: NapiValue,
        argc: usize,
        _argv: *const NapiValue,
        result: *mut NapiValue,
    ) -> i32 {
        JS_CALLS.with(|c| c.borrow_mut().push((recv as usize, func as usize, argc)));
        unsafe { *result = 0x7001usize as NapiValue };
        0
    }
}

#[cfg(test)]
mod tests {
    use super::host_stubs::{created_string, fake_funcs, reset, set_bool, set_call_args, set_calls, set_string};
    use super::*;

    // Real regression promoted from the probe: napi_init must attach the
    // created function objects to the exports object it was handed (returns
    // that same object).
    #[test]
    fn napi_init_attaches_registered_functions_to_exports() {
        reset();
        let env = 0x1 as NapiEnv;
        let exports = 0x2 as NapiValue;
        let out = unsafe { napi_init(env, exports) };
        assert_eq!(out as usize, exports as usize,
                   "napi_init must return the same exports object");
        let calls = set_calls();
        let funcs = fake_funcs();
        for name in [
            "version",
            "abi_probe",
            "fd_status",
            "wg_probe",
            "wg_udp_probe",
            "wg_net_probe",
            "wg_fwd_probe",
            "wg_fwd_open",
            "wg_fwd_run",
            "tun_open",
            "tun_read",
            "tun_write",
            "tun_poll",
            "tun_close",
            "config_validate",
            "mgmt_socket_open",
            "ice_socket_open",
            "connector_start",
            "connector_start_with_socket",
            "connector_socket_feed",
            "connector_ice_mux_arm",
            "connector_signal_socket_feed",
            "connector_wg_socket_feed",
            "connector_tun_fd_feed",
            "connector_tun_release",
            "connector_route_set_applied",
            "connector_status",
            "connector_resource_status",
            "connector_network_config",
            "connector_stop",
            "n2_tun_dest_configure",
            "n2_control_send",
            "n2_control_status",
            "n2_stimulus",
            "n2_a_event",
            "n2_sandwich_pre",
            "n2_sandwich_post",
            "n2_sandwich_poll",
            "n2_cd_plan",
            "n2_cd_prepare",
            "n2_cd_ready",
            "n2_cd_pre",
            "n2_cd_switch_identified",
            "n2_cd_identity",
            "n2_cd_poll",
            "n2_cd_post",
            "n2_ring_configure",
            "n2_ring_append",
            "n2_ring_disable",
            "n2_ring_status",
            "n2_ring_dump",
            "n2_ring_manifest",
            "n2_ring_ack",
            "n2h_collector_arm",
            "n2h_collector_status",
            "n2h_collector_evidence",
            "n2h_collector_disarm",
            "n2h_probe_socket_open",
            "n2h_probe_socket_commit",
            "phone_log_configure",
            "phone_log_append",
            "phone_log_status",
            "phone_log_shutdown",
            "account_auth_discovery_start",
            "account_auth_discovery_poll",
            "account_auth_discovery_cancel",
            "account_login_start",
            "account_login_poll",
            "account_login_cancel",
            "account_extend_start",
            "account_extend_poll",
            "account_extend_cancel",
            "connector_routes",
            "connector_route_selection_set",
            "alloc_stats",
            "connector_status_subscribe",
            "connector_status_unsubscribe",
            "connector_status_revision",
        ] {
            let call = calls.iter().find(|(_, n, _)| n == name)
                .unwrap_or_else(|| panic!("{} not attached to exports", name));
            assert_eq!(call.0, exports as usize, "{} attached to wrong object", name);
            let fake = *funcs.get(name)
                .unwrap_or_else(|| panic!("{} create_function not called", name));
            assert_ne!(fake, 0, "{} fake function must be non-null", name);
            assert_eq!(call.2, fake, "{} value must be the created function object", name);
        }

        // Append-only surface (increments 131/132 + the additive heap readout,
        // then the 0.0.34 status-edge trio): the route pair, `alloc_stats` and
        // the status surface are the LAST registrations, in that order, and
        // every export name is registered exactly once — no pre-existing name
        // lost its position or its created function object to the new binding.
        let names: Vec<&str> = calls.iter().map(|(_, name, _)| name.as_str()).collect();
        assert_eq!(
            names[names.len() - 6..].to_vec(),
            vec![
                "connector_routes",
                "connector_route_selection_set",
                "alloc_stats",
                "connector_status_subscribe",
                "connector_status_unsubscribe",
                "connector_status_revision",
            ],
            "the route surface, the heap readout and the status-edge trio must stay appended at the registration tail"
        );
        let mut seen = std::collections::HashSet::new();
        for name in &names {
            assert!(seen.insert(*name), "{name} registered more than once");
        }
    }

    // -----------------------------------------------------------------------
    // phone_log:* export surface
    //
    // The NAPI callbacks themselves cannot be driven end-to-end through the
    // host stubs (`napi_get_cb_info` is a no-op here), so the bounded bodies
    // they call are exercised directly, and `Args` itself is driven through the
    // string stub. Registration of the four names is covered above.
    // -----------------------------------------------------------------------

    /// Read one flat `"key":value` field out of a reply document.
    fn json_field(json: &str, key: &str) -> String {
        let marker = format!("\"{key}\":");
        let start = json
            .find(&marker)
            .unwrap_or_else(|| panic!("{key} missing in {json}"))
            + marker.len();
        let rest = &json[start..];
        match rest.strip_prefix('"') {
            Some(quoted) => quoted[..quoted.find('"').expect("closing quote")].to_string(),
            None => rest[..rest.find(|c| c == ',' || c == '}').expect("value end")].to_string(),
        }
    }

    #[test]
    fn bounded_string_args_refuse_over_cap_and_non_string() {
        reset();
        set_string(0x51, "files/phone.log");
        set_string(0x52, &"x".repeat(700));
        let mut argv: [NapiValue; 8] = [core::ptr::null_mut(); 8];
        argv[0] = 0x51 as NapiValue;
        argv[1] = 0x52 as NapiValue;
        argv[2] = 0x53 as NapiValue; // never registered = "not a string"
        let args = Args { env: 0x1 as NapiEnv, argv, argc: 3 };

        assert_eq!(args.string_at_bounded(0, PHONE_LOG_PATH_MAX).as_deref(), Some("files/phone.log"));
        assert_eq!(
            args.string_at_bounded(1, PHONE_LOG_PATH_MAX),
            None,
            "an over-cap string must be refused before the copy"
        );
        assert_eq!(
            args.string_at_bounded(1, 4096).as_deref().map(str::len),
            Some(700),
            "the same value passes under a larger cap"
        );
        assert_eq!(args.string_at_bounded(2, PHONE_LOG_PATH_MAX), None, "non-string must be refused");
        assert_eq!(args.string_at_bounded(3, PHONE_LOG_PATH_MAX), None, "missing argument must be refused");
        assert_eq!(args.string_at_bounded(0, 0), None, "a zero cap refuses everything");
    }

    #[test]
    fn phone_log_exports_shapes_and_lifecycle() {
        // The phone log is PROCESS-GLOBAL and this is the only lib test that
        // configures it (tests/phone_log.rs is a separate test binary).
        let dir = std::env::temp_dir().join(format!(
            "nb-napi-phone-log-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("phone.log").to_string_lossy().into_owned();

        // Error replies are a code only — never the argument, never config.
        assert_eq!(
            phone_log_configure_export("no-slash.log"),
            "{\"ok\":false,\"error\":\"invalid-path\"}"
        );
        assert_eq!(phone_log_append_export(None), "{\"ok\":false,\"error\":\"invalid-line\"}");
        assert_eq!(
            phone_log_append_export(Some("before-configure")),
            "{\"ok\":false,\"error\":\"not-configured\"}"
        );

        // Configure once: an install, not an idempotent hit.
        let first = phone_log_configure_export(&path);
        assert!(first.contains("\"ok\":true"), "{first}");
        assert!(first.contains("\"configured\":true"), "{first}");
        assert!(first.contains("\"idempotent\":false"), "{first}");
        assert!(first.contains("\"enabled\":true"), "{first}");
        assert!(first.contains("\"running\":true"), "{first}");
        assert_eq!(json_field(&first, "path"), path, "{first}");
        let session = json_field(&first, "session");
        assert!(!session.is_empty(), "{first}");

        // A repeat on the SAME path is idempotent: same session, no new writer.
        let again = phone_log_configure_export(&path);
        assert!(again.contains("\"idempotent\":true"), "{again}");
        assert_eq!(json_field(&again, "session"), session, "{again}");

        // One line handed to the tee must reach the file. The module's
        // per-second admission budget is PROCESS-WIDE (concurrent lib tests
        // share it), so a line can be refused by design (`rate_suppressed`);
        // retry across fresh windows and fail only if no sentinel ever lands.
        let mut sentinel = String::new();
        let mut found = false;
        for attempt in 0..12 {
            sentinel = format!("NAPI_PHONE_LOG_SENTINEL_{attempt}|roundtrip");
            assert_eq!(
                phone_log_append_export(Some(&sentinel)),
                "{\"ok\":true}",
                "an enabled tee must accept the hand-off"
            );
            crate::phone_log::flush(Duration::from_millis(100));
            found = std::fs::read_to_string(&path).is_ok_and(|text| text.contains(&sentinel));
            if found {
                break;
            }
            std::thread::sleep(Duration::from_millis(120));
        }
        assert!(found, "no sentinel record reached the file ({sentinel})");

        let shutdown = phone_log_shutdown_export();
        assert!(shutdown.contains("\"ok\":true"), "{shutdown}");
        assert!(shutdown.contains("\"stopped\":"), "{shutdown}");
        assert!(shutdown.contains("\"running\":"), "{shutdown}");
        assert!(shutdown.contains("\"pending\":"), "{shutdown}");

        // The bounded request may legitimately report stopped:false; the writer
        // must still reach the stopped state without a 2 s block on the caller.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while crate::phone_log::stats().running && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!crate::phone_log::stats().running, "writer never stopped: {shutdown}");

        let status = phone_log_status_export();
        assert!(status.contains("\"component\":\"phone_log\""), "{status}");
        assert!(status.contains("\"enabled\":false"), "{status}");
        assert!(status.contains("\"running\":false"), "{status}");

        let text = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(text.contains(&sentinel), "{text}");
        assert!(text.contains(&format!("sess={session}")), "{text}");

        // After the stop the tee is off: a late line is refused, not buffered.
        assert_eq!(
            phone_log_append_export(Some("after-shutdown")),
            "{\"ok\":false,\"error\":\"not-configured\"}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // Core 5f NAPI route surface (increments 131/132)
    //
    // Both route callbacks are driven END TO END through the host stubs:
    // `set_call_args` arms one real argument list, so the WRAPPER itself is
    // exercised — argument arity (whether the optional third argument is present
    // at all), the string/bool reads and the JSON it returns. The process-global
    // connector slot is empty here for the same reason the connector unit tests
    // rely on ("unit tests never start one"), so a request that reaches the
    // connector lookup answers `no-connector`; every refusal decided BEFORE that
    // lookup (`kind-invalid` / `route-set-invalid` / `selection-invalid`) is
    // deterministic whatever else runs in this test process. The N2 ring tees
    // every hilog line PROCESS-WIDE, so this group holds the same lock the ring
    // tests hold.
    // -----------------------------------------------------------------------

    /// The JSON a wrapper returned for the armed invocation.
    fn reply(value: NapiValue) -> String {
        created_string(value as usize).expect("ret_json must return a JS string")
    }

    const ACK_NO_CONNECTOR: &str = "{\"ok\":false,\"error\":\"no-connector\"}";
    const ACK_KIND_INVALID: &str = "{\"ok\":false,\"error\":\"kind-invalid\"}";
    const ACK_ROUTE_SET_INVALID: &str = "{\"ok\":false,\"error\":\"route-set-invalid\"}";
    const SET_NO_CONNECTOR: &str = "{\"ok\":false,\"error\":\"no-connector\",\"available\":false,\"changed\":false,\"desired_routes\":[],\"recreate_needed\":false}";
    const SET_SELECTION_INVALID: &str = "{\"ok\":false,\"error\":\"selection-invalid\",\"available\":false,\"changed\":false,\"desired_routes\":[],\"recreate_needed\":false}";

    #[test]
    fn route_set_applied_callback_keeps_the_two_arg_legacy_and_dispatches_the_third_arg_kind() {
        let _quiet = crate::n2_ring::test_guard();
        reset();
        let env = 0x1 as NapiEnv;
        let routes = 0x21usize;
        let recreate_true = 0x22usize;
        let kind_user = 0x23usize;
        let not_a_string = 0x24usize; // no payload registered = "not a string"
        set_string(routes, "{\"routes\":[\"0.0.0.0/0\",\"172.16.0.0/12\"]}");
        set_bool(recreate_true, true);
        set_string(kind_user, "user");

        // 1) TWO arguments (the pre-existing ABI, and the only form an older
        //    shell sends): the legacy boolean entry runs. If the optional
        //    argument were read as present, this would answer `kind-invalid`.
        set_call_args(0xC2, &[routes, recreate_true]);
        assert_eq!(
            reply(unsafe { napi_connector_route_set_applied(env, 0xC2 as NapiCallbackInfo) }),
            ACK_NO_CONNECTOR,
            "2-arg legacy call with isRecreate=true"
        );

        // 2) THREE arguments with the explicit `user` kind: the kind-aware entry
        //    is dispatched and reaches the connector lookup.
        set_call_args(0xC3, &[routes, recreate_true, kind_user]);
        assert_eq!(
            reply(unsafe { napi_connector_route_set_applied(env, 0xC3 as NapiCallbackInfo) }),
            ACK_NO_CONNECTOR,
            "3-arg explicit user kind"
        );

        // 3) THREE arguments whose third argument is NOT a string: refused as an
        //    invalid kind — never a silent fallback onto the budget-spending
        //    `auto` kind, and never the legacy boolean entry (which cannot answer
        //    `kind-invalid` at all). This is the answer that proves the dispatch.
        set_call_args(0xC4, &[routes, recreate_true, not_a_string]);
        assert_eq!(
            reply(unsafe { napi_connector_route_set_applied(env, 0xC4 as NapiCallbackInfo) }),
            ACK_KIND_INVALID,
            "a present non-string kind must be refused"
        );

        // 4) THREE arguments, an unknown token: the same loud refusal.
        let kind_bogus = 0x25usize;
        set_string(kind_bogus, "userish");
        set_call_args(0xC5, &[routes, recreate_true, kind_bogus]);
        assert_eq!(
            reply(unsafe { napi_connector_route_set_applied(env, 0xC5 as NapiCallbackInfo) }),
            ACK_KIND_INVALID,
            "an unknown kind token must be refused"
        );

        // 5) `{"routes":[]}` is a LEGAL applied set (T0 e15ac): it clears the
        //    parser and reaches the connector lookup — `route-set-invalid` would
        //    mean the parse had rejected the empty list.
        let empty_routes = 0x26usize;
        let kind_initial = 0x27usize;
        let recreate_false = 0x28usize;
        set_string(empty_routes, "{\"routes\":[]}");
        set_string(kind_initial, "initial");
        set_bool(recreate_false, false);
        set_call_args(0xC6, &[empty_routes, recreate_true, kind_initial]);
        assert_eq!(
            reply(unsafe { napi_connector_route_set_applied(env, 0xC6 as NapiCallbackInfo) }),
            ACK_NO_CONNECTOR,
            "an explicit empty applied set is legal (kind entry)"
        );
        set_call_args(0xC7, &[empty_routes, recreate_false]);
        assert_eq!(
            reply(unsafe { napi_connector_route_set_applied(env, 0xC7 as NapiCallbackInfo) }),
            ACK_NO_CONNECTOR,
            "an explicit empty applied set is legal (legacy entry, isRecreate=false)"
        );

        // 6) A malformed document is refused BEFORE the connector lookup, so this
        //    answer holds whatever else runs in this test process.
        let bad_routes = 0x29usize;
        set_string(bad_routes, "{\"routes\":[1,2]}");
        set_call_args(0xC8, &[bad_routes, recreate_true, kind_user]);
        assert_eq!(
            reply(unsafe { napi_connector_route_set_applied(env, 0xC8 as NapiCallbackInfo) }),
            ACK_ROUTE_SET_INVALID,
            "a malformed applied set is refused"
        );

        // The kind is validated BEFORE the document (documented check order), so
        // an unknown kind never even reaches the route parser.
        set_call_args(0xC9, &[bad_routes, recreate_true, kind_bogus]);
        assert_eq!(
            reply(unsafe { napi_connector_route_set_applied(env, 0xC9 as NapiCallbackInfo) }),
            ACK_KIND_INVALID,
            "the kind is checked before the document"
        );
    }

    #[test]
    fn route_selection_set_callback_takes_one_raw_selector_string() {
        let _quiet = crate::n2_ring::test_guard();
        reset();
        let env = 0x1 as NapiEnv;

        // ONE string argument, the RAW selector document: the wrapper hands it to
        // the core's own strict parser, so a setter call with no connector answers
        // the documented no-claim `no-connector` document.
        let selector = 0x31usize;
        set_string(selector, "{\"deselected_routes\":[\"HostInternal\"],\"deselect_all\":false}");
        set_call_args(0xD2, &[selector]);
        let doc = reply(unsafe { napi_connector_route_selection_set(env, 0xD2 as NapiCallbackInfo) });
        assert_eq!(doc, SET_NO_CONNECTOR, "raw selector, no connector");
        assert_eq!(json_field(&doc, "error"), "no-connector", "{doc}");
        assert!(!doc.contains("HostInternal"), "never echoes the selector payload: {doc}");

        // A malformed selector is refused by that same parser, before the
        // connector lookup — deterministic in this shared test process.
        let bad = 0x32usize;
        set_string(bad, "{\"deselected_routes\":[1]}");
        set_call_args(0xD3, &[bad]);
        assert_eq!(
            reply(unsafe { napi_connector_route_selection_set(env, 0xD3 as NapiCallbackInfo) }),
            SET_SELECTION_INVALID,
            "a malformed selector mutates nothing"
        );

        // The all-default document (nothing selected/deselected) is legal and
        // still reaches the connector lookup.
        let all_default = 0x33usize;
        set_string(
            all_default,
            "{\"selected_routes\":[],\"deselected_routes\":[],\"deselect_all\":false}",
        );
        set_call_args(0xD4, &[all_default]);
        assert_eq!(
            reply(unsafe { napi_connector_route_selection_set(env, 0xD4 as NapiCallbackInfo) }),
            SET_NO_CONNECTOR,
            "the all-default selector is legal"
        );
    }

    // -----------------------------------------------------------------------
    // additive Rust-heap readout export
    //
    // The callback is driven END TO END through the host stubs (zero
    // arguments), so the registered wrapper, `ret_json` and the document are
    // all exercised. This is a SHAPE assertion only: the lib test binary runs
    // many tests in parallel, so exact counter deltas belong to the dedicated
    // single-test integration binary (tests/alloc_stats.rs), which is
    // deterministic for the reason documented there.
    // -----------------------------------------------------------------------

    #[test]
    fn alloc_stats_callback_returns_the_flat_four_counter_document() {
        reset();
        let env = 0x1 as NapiEnv;
        set_call_args(0xE1, &[]);
        let doc = reply(unsafe { napi_alloc_stats(env, 0xE1 as NapiCallbackInfo) });

        assert!(doc.starts_with("{\"bytes\":"), "{doc}");
        let bytes = json_field(&doc, "bytes").parse::<u64>().expect("bytes must be a number");
        let peak = json_field(&doc, "peak").parse::<u64>().expect("peak must be a number");
        let allocs = json_field(&doc, "allocs").parse::<u64>().expect("allocs must be a number");
        let frees = json_field(&doc, "frees").parse::<u64>().expect("frees must be a number");
        // `snapshot()` clamps peak to bytes at read time, so this holds even
        // when another lib test allocates between the two loads.
        assert!(peak >= bytes, "{doc}");
        // By now this process has certainly allocated and freed (the test
        // harness alone does both before any test body runs).
        assert!(allocs > 0 && frees > 0, "{doc}");
        // Exactly the four documented numeric fields: no `ok`, no extras.
        assert_eq!(doc.matches(':').count(), 4, "{doc}");
        assert_eq!(doc.matches(',').count(), 3, "{doc}");
    }

    // -----------------------------------------------------------------------
    // the socket-supply wake (the shell's event-driven supply path)
    //
    // Driven end to end through the host stubs, which model the runtime's
    // observable contract: subscribe creates ONE threadsafe function from the
    // ArkTS callback and installs the enqueue hook; an enqueue reaches the JS
    // callback with ZERO arguments (the shell drains the queue itself);
    // unsubscribe detaches the hook and releases the handle; a second subscribe
    // replaces and releases the first. Serialized against the other test that
    // installs the process-wide hook (crate::socket_supply::notify_test_lock).
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn supply_subscribe_wakes_on_enqueue_and_unsubscribe_releases_the_hook() {
        use crate::socket_supply::{Purpose, SocketSupply};
        use super::host_stubs::{js_calls, live_tsfns, tsfn_calls, tsfn_releases};

        let _serial = crate::socket_supply::notify_test_lock();
        reset();
        let env = 0x1 as NapiEnv;

        // (a) no ArkTS callback: an ANSWERED refusal, never a throw and never a
        // half-installed subscription.
        let refused = unsafe { napi_socket_supply_subscribe(env, 0xB1 as NapiCallbackInfo) };
        assert!(created_string(refused as usize).expect("refusal reply")
            .contains("callback-required"));
        assert_eq!(live_tsfns(), 0);

        // (b) the ArkTS callback: exactly one threadsafe function, hook installed.
        set_call_args(0xB2, &[0x1234]);
        let ok = unsafe { napi_socket_supply_subscribe(env, 0xB2 as NapiCallbackInfo) };
        assert_eq!(created_string(ok as usize).expect("subscribe reply"), "{\"ok\":true}");
        assert_eq!(live_tsfns(), 1);

        // One real request: enqueue -> wake -> the JS callback runs with no args.
        async fn serve_one(supply: std::sync::Arc<SocketSupply>) -> u64 {
            loop {
                if let Some((id, _purpose)) = supply.poll_request() {
                    let raw = unsafe {
                        crate::sys::socket(crate::sys::AF_INET, crate::sys::SOCK_DGRAM, 0)
                    };
                    assert!(raw >= 0, "socket(2) for a protected fd");
                    assert!(supply.reply(id, raw, "protected").contains("true"));
                    unsafe { crate::sys::close(raw) };
                    return id;
                }
                tokio::task::yield_now().await;
            }
        }

        let supply = std::sync::Arc::new(SocketSupply::new());
        let poller = tokio::spawn(serve_one(supply.clone()));
        let (dialed, fd) = supply.request_with_id(Purpose::Management).await.expect("protected reply");
        drop(fd);
        assert_eq!(poller.await.expect("poller task"), dialed);
        let calls = tsfn_calls();
        assert_eq!(calls.len(), 1, "exactly one wake for one enqueue");
        assert_eq!(calls[0].1, 0, "the wake carries no data");
        assert_eq!(js_calls(), vec![(0x7000, 0x1234, 0)],
            "the ArkTS callback is called with ZERO arguments");

        // (c) unsubscribe: hook detached, handle released in abort mode (wakes
        // still queued at teardown are dropped, not delivered).
        let gone = unsafe { napi_socket_supply_unsubscribe(env, 0xB3 as NapiCallbackInfo) };
        assert_eq!(created_string(gone as usize).expect("unsubscribe reply"), "{\"ok\":true}");
        assert_eq!(live_tsfns(), 0, "the handle was released");
        assert_eq!(tsfn_releases(), vec![(calls[0].0, 1)]);
        let again = unsafe { napi_socket_supply_unsubscribe(env, 0xB4 as NapiCallbackInfo) };
        assert!(created_string(again as usize).expect("second unsubscribe reply")
            .contains("\"subscribed\":false"), "idempotent, and it says so");

        // A cleared subscription means NO wake for a later request — it is still
        // served by the poller, which is what the shell's safety net does.
        let poller = tokio::spawn(serve_one(supply.clone()));
        let (dialed, fd) = supply.request_with_id(Purpose::Dns).await.expect("protected reply");
        drop(fd);
        assert_eq!(poller.await.expect("poller task"), dialed);
        assert_eq!(tsfn_calls().len(), 1, "no wake after unsubscribe");
    }

    // -----------------------------------------------------------------------
    // the status-edge wake (0.0.34 peer-status lag fix, F1)
    //
    // The TRANSPORT is pinned here end to end (subscribe creates ONE threadsafe
    // function; a real edge reaches the ArkTS callback with ZERO arguments; the
    // delivery consumes the coalescing token, so the next edge wakes again;
    // unsubscribe detaches the hook and releases the handle in abort mode). The
    // CAPACITY-1 COALESCING itself ("N edges ⇒ exactly one pending wake") is
    // pinned in `crate::status_events::tests`, where the installed hook does not
    // deliver — the host stub below dispatches `call_js` INLINE, i.e. it models
    // an ArkTS loop that is never busy, which is the one case in which a burst
    // legitimately produces one wake per edge.
    // -----------------------------------------------------------------------

    #[test]
    fn status_wake_subscribes_wakes_with_no_payload_and_unsubscribes() {
        use super::host_stubs::{js_calls, live_tsfns, tsfn_calls, tsfn_releases};
        // ONE process-wide wake installation, shared with the socket-supply
        // wake test and with every fact-site test in this binary: serialize.
        let _serial = crate::status_events::wake_test_lock();
        reset();
        let env = 0x1 as NapiEnv;

        // (a) no ArkTS callback: an ANSWERED refusal — never a throw, never a
        // half-installed subscription.
        let refused = unsafe { napi_connector_status_subscribe(env, 0xC1 as NapiCallbackInfo) };
        assert!(created_string(refused as usize).expect("refusal reply")
            .contains("callback-required"));
        assert_eq!(live_tsfns(), 0);

        // (b) the ArkTS callback: exactly one threadsafe function.
        set_call_args(0xC2, &[0x4321]);
        let ok = unsafe { napi_connector_status_subscribe(env, 0xC2 as NapiCallbackInfo) };
        assert_eq!(created_string(ok as usize).expect("subscribe reply"), "{\"ok\":true}");
        assert_eq!(live_tsfns(), 1);

        // (c) a real edge reaches the JS callback with ZERO arguments, exactly
        // once. The token is PROCESS-wide (as in production), so a concurrent
        // test's edge may legitimately own it microseconds before ours: re-arm
        // and retry — this asserts the plumbing, not the scheduler.
        let mut woke = false;
        for _ in 0..256 {
            crate::status_events::wake_consumed();
            let before = tsfn_calls().len();
            crate::status_events::edge("napi-status-test");
            let raised = tsfn_calls().len() - before;
            if raised == 1 {
                woke = true;
                break;
            }
            assert_eq!(raised, 0, "capacity-1: one edge can never raise two wakes");
        }
        assert!(woke, "no wake in 256 attempts — the status wake is latched off");
        let calls = tsfn_calls();
        assert_eq!(calls.len(), 1, "exactly one wake for one edge");
        assert_eq!(calls[0].1, 0, "the wake carries no data");
        assert_eq!(js_calls(), vec![(0x7000, 0x4321, 0)],
            "the ArkTS callback is called with ZERO arguments");

        // (d) unsubscribe: hook detached, handle released in abort mode; the
        // reply is idempotent and says so.
        let gone = unsafe { napi_connector_status_unsubscribe(env, 0xC3 as NapiCallbackInfo) };
        assert_eq!(created_string(gone as usize).expect("unsubscribe reply"), "{\"ok\":true}");
        assert_eq!(live_tsfns(), 0, "the handle was released");
        assert_eq!(tsfn_releases(), vec![(calls[0].0, 1)]);
        let again = unsafe { napi_connector_status_unsubscribe(env, 0xC4 as NapiCallbackInfo) };
        assert!(created_string(again as usize).expect("second unsubscribe reply")
            .contains("\"subscribed\":false"), "idempotent, and it says so");
        let after = tsfn_calls().len();
        crate::status_events::edge("napi-status-after-unsubscribe");
        assert_eq!(tsfn_calls().len(), after, "no wake after unsubscribe");
    }

    #[test]
    fn status_revision_getter_is_the_cheap_monotonic_change_detector() {
        use super::host_stubs::created_string;
        let env = 0x1 as NapiEnv;
        let read = |env: NapiEnv| -> u64 {
            let doc = unsafe {
                created_string(napi_connector_status_revision(env, core::ptr::null_mut()) as usize)
            }
            .expect("revision reply");
            // the flat one-field document, parsed the same way the shell reads it
            let at = doc.find("\"revision\":").expect("revision field") + 11;
            let rest = &doc[at..];
            let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            rest[..end].parse().expect("revision is a number")
        };
        // No connector is installed here at all: the getter answers from the
        // process-monotonic counter — no slot, no lock, no document.
        let snapshot = crate::status_events::revision();
        let before = read(env);
        assert!(before >= snapshot, "the getter reports the live revision ({before} >= {snapshot})");
        crate::status_events::edge("napi-revision-test");
        let after = read(env);
        assert!(after > before, "monotonic: an edge moved the revision forward ({before} -> {after})");
        // ... and it is a pure read: nothing between two reads moves it back.
        assert!(read(env) >= after);
    }
}

// ---------------------------------------------------------------------------
// exported callbacks (thin wrappers -> module logic -> JSON string)
// ---------------------------------------------------------------------------

unsafe extern "C" fn napi_version(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(
        env,
        "{\"name\":\"netbird_core\",\"version\":\"0.1.0\",\"boringtun\":\"0.7.1\",\"channel\":\"hilog:0x2900/N1BDiscVpn\"}".to_string(),
    )
}

unsafe extern "C" fn napi_abi_probe(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::abi::abi_probe(&a.string_at(0)))
}

unsafe extern "C" fn napi_fd_status(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::ledger::fd_status(a.i32_at(0, -1)))
}

unsafe extern "C" fn napi_wg_probe(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::wg::wg_probe(a.i32_at(0, -1), a.bool_at(1, false)))
}

unsafe extern "C" fn napi_wg_udp_probe(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::wg::wg_udp_probe())
}

unsafe extern "C" fn napi_wg_net_probe(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::wg::wg_net_probe(a.i32_at(0, -1), a.bool_at(1, false)))
}

unsafe extern "C" fn napi_wg_fwd_probe(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::wg::wg_fwd_probe(a.i32_at(0, -1), a.bool_at(1, false)))
}

unsafe extern "C" fn napi_wg_fwd_open(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::wg::wg_fwd_open())
}

unsafe extern "C" fn napi_wg_fwd_run(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::wg::wg_fwd_run(a.i32_at(0, -1), a.i32_at(1, -1), a.bool_at(2, false)))
}

unsafe extern "C" fn napi_tun_open(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::tun::tun_open_json(a.i32_at(0, -1)))
}

unsafe extern "C" fn napi_tun_read(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::tun::tun_read_json(a.i32_at(0, -1)))
}

unsafe extern "C" fn napi_tun_write(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    let s = a.string_at(1);
    ret_json(env, crate::tun::tun_write_json(a.i32_at(0, -1), &s))
}

unsafe extern "C" fn napi_tun_poll(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::tun::tun_poll_json(a.i32_at(0, -1), a.i32_at(1, 0)))
}

unsafe extern "C" fn napi_tun_close(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::tun::tun_close_json(a.i32_at(0, -1)))
}

unsafe extern "C" fn napi_config_validate(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    let s = a.string_at(0);
    ret_json(env, crate::config::config_validate_json(&s))
}

// N3-5 connector lifecycle. All three exports are synchronous JSON->JSON
// (the lifecycle itself runs on the crate-wide tokio runtime and is observed
// by polling connector_status — no threadsafe-function / async-work).
unsafe extern "C" fn napi_connector_start(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    let config = a.string_at(0);
    let credentials = a.string_at(1);
    ret_json(env, crate::connector::connector_start_json(&config, &credentials))
}

// N3-7: production start over a shell-protected management socket. The fd
// crosses as a number; native dup-consumes it per dial (never uses or closes
// the original — fd contract in crate::mgmtsock).
unsafe extern "C" fn napi_connector_start_with_socket(
    env: NapiEnv,
    info: NapiCallbackInfo,
) -> NapiValue {
    let a = cb_args(env, info);
    let fd = a.i32_at(0, -1);
    let config = a.string_at(1);
    let credentials = a.string_at(2);
    let connect_addr = a.string_at(3);
    ret_json(
        env,
        crate::connector::connector_start_with_socket_json(fd, &config, &credentials, &connect_addr),
    )
}

unsafe extern "C" fn napi_connector_physical_network_changed(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::connector_physical_network_changed_json(&a.string_at(0)))
}

unsafe extern "C" fn napi_connector_management_address_update(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::connector_management_address_update_json(&a.string_at(0)))
}

unsafe extern "C" fn napi_connector_protected_dns_start(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::connector_protected_dns_start_json(&a.string_at(0)))
}

unsafe extern "C" fn napi_connector_protected_dns_poll(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::connector_protected_dns_poll_json(a.i32_at(0, -1) as u64))
}

// N3-7: shell-side resupply of fresh protected sockets (reconnect dials).
unsafe extern "C" fn napi_connector_socket_feed(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    let fd = a.i32_at(0, -1);
    ret_json(env, crate::connector::connector_socket_feed_json(fd))
}

// STAGE 3 (official-alignment §2.7): arm the ONE shared ICE mux OVER the WG
// device's own outer socket and start its reader thread — there is no second
// socket to open or protect any more (upstream `createOrUpdateMux`,
// `ice_bind.go:291-316`). Fail-closed: no mux ⇒ no local candidates.
unsafe extern "C" fn napi_connector_ice_mux_arm(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::connector_ice_mux_arm_json())
}

// N5d: shell-side resupply of PROTECTED signal sockets plus the
// shell-resolved signal address (fail-closed: no feed ⇒ the signal dial
// fails, no unprotected fallback). fd first, `{"connect_addr":"ip:port"}`
// JSON second — both required.
unsafe extern "C" fn napi_connector_signal_socket_feed(
    env: NapiEnv,
    info: NapiCallbackInfo,
) -> NapiValue {
    let a = cb_args(env, info);
    let fd = a.i32_at(0, -1);
    let addr = a.string_at(1);
    ret_json(env, crate::connector::connector_signal_socket_feed_json(fd, &addr))
}

unsafe extern "C" fn napi_socket_supply_poll(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::socket_supply::poll_json())
}

// ---------------------------------------------------------------------------
// The socket-supply wake — the event-driven replacement of the shell's 100 ms
// poll, and this crate's ONLY threadsafe function.
//
// The mechanism is the one already measured in-tree for exactly this runtime:
// `spikes/r1-api24-hap/entry/src/main/cpp/e1_c_probe.cpp` created a threadsafe
// function from an ArkTS callback, called it from producer pthreads and saw the
// consumer run on the ArkTS main thread with no fd/thread growth
// (`docs/evidence/e1-c-bridge-api24-emulator-2026-07-17.md`, reviewed-pass:
// 3000 ordered callbacks). The ArkTS/N-API contract is identical here.
//
// Ownership: the threadsafe function keeps the ArkTS callback alive, so only its
// handle is stored. `SUPPLY_WAKE` is held ACROSS `napi_call_threadsafe_function`
// and `napi_release_threadsafe_function`, which is what makes teardown safe: a
// release can only run once no wake is inside the runtime's call.
// ---------------------------------------------------------------------------

struct SupplyWake(*mut c_void);
// The handle belongs to the ArkTS runtime; `SUPPLY_WAKE`'s mutex serializes
// every use of it.
unsafe impl Send for SupplyWake {}
static SUPPLY_WAKE: Mutex<Option<SupplyWake>> = Mutex::new(None);

/// Runs ON the ArkTS thread. Invokes the shell's registered callback with NO
/// arguments: the wake is a "the queue changed, service it" edge, so no JS
/// string/array is allocated per request (the shell reads the queue itself).
unsafe extern "C" fn supply_wake_call_js(
    env: NapiEnv,
    js_callback: NapiValue,
    _context: *mut c_void,
    _data: *mut c_void,
) {
    wake_call_js_no_args(env, js_callback, "socket-supply");
}

/// The shared body of both wake call_js hooks (socket supply and the 0.0.34
/// status edge): call the shell's callback with ZERO arguments. A throw inside
/// the shell's callback can only fail this call; it must never abort the wake
/// machinery — the socket-supply safety-net poll / the retained 5 s status
/// heartbeat still covers the wake — so it is reported and dropped.
unsafe fn wake_call_js_no_args(env: NapiEnv, js_callback: NapiValue, label: &str) {
    if env.is_null() || js_callback.is_null() {
        return;
    }
    let mut undefined: NapiValue = core::ptr::null_mut();
    if napi_get_undefined(env, &mut undefined) != 0 {
        return;
    }
    let mut ignored: NapiValue = core::ptr::null_mut();
    let rc = napi_call_function(env, undefined, js_callback, 0, core::ptr::null_mut(), &mut ignored);
    if rc != 0 {
        crate::hilog::emit(&format!("{label}: wake callback rc={rc}"));
    }
}

/// The notify hook installed into `crate::socket_supply`: called from whichever
/// (tokio) thread enqueued the request, never from the ArkTS thread.
/// Non-blocking on purpose — the shell drains the queue on every wake, so a wake
/// that could not be queued is covered by the next request and by the shell's
/// slow safety-net poll; the DIAL must never wait for the ArkTS loop.
fn supply_wake(_id: u64) {
    let guard = SUPPLY_WAKE.lock().expect("supply wake");
    let Some(wake) = guard.as_ref() else { return };
    let rc = unsafe {
        napi_call_threadsafe_function(wake.0, core::ptr::null_mut(), NAPI_TSFN_NONBLOCKING)
    };
    if rc != 0 {
        crate::hilog::emit(&format!("socket-supply: wake rejected rc={rc}"));
    }
}

fn release_wake(wake: SupplyWake) {
    let rc = unsafe { napi_release_threadsafe_function(wake.0, NAPI_TSFN_ABORT) };
    if rc != 0 {
        crate::hilog::emit(&format!("socket-supply: wake release rc={rc}"));
    }
}

/// `socket_supply_subscribe(callback) => '{"ok":true}'` — install the ArkTS wake
/// (the shell calls this once per live connector). An existing subscription is
/// replaced and released, so a restarted shell can subscribe again without
/// leaking the previous callback. Failures are answered, never thrown: this is a
/// latency optimization, and the caller keeps a safety-net poll.
unsafe extern "C" fn napi_socket_supply_subscribe(
    env: NapiEnv,
    info: NapiCallbackInfo,
) -> NapiValue {
    let a = cb_args(env, info);
    if a.argc == 0 || a.argv[0].is_null() {
        return ret_json(env, "{\"ok\":false,\"error\":\"callback-required\"}".into());
    }
    let label = b"socket-supply-wake\0";
    let mut name: NapiValue = core::ptr::null_mut();
    if napi_create_string_utf8(env, label.as_ptr(), label.len() - 1, &mut name) != 0 {
        return ret_json(env, "{\"ok\":false,\"error\":\"resource-name-failed\"}".into());
    }
    let mut handle: *mut c_void = core::ptr::null_mut();
    let rc = napi_create_threadsafe_function(
        env,
        a.argv[0],
        core::ptr::null_mut(),
        name,
        // unbounded queue: a wake is a few bytes and the shell coalesces them by
        // draining; bounding it would turn a busy JS thread into a dropped wake.
        0,
        // initial_thread_count = 1 ("the native side may wake at any time"). The
        // counted thread is the ArkTS thread that created it, exactly like the
        // reviewed E1 probe; the many core threads are callers, not owners.
        1,
        core::ptr::null_mut(),
        None,
        core::ptr::null_mut(),
        Some(supply_wake_call_js),
        &mut handle,
    );
    if rc != 0 || handle.is_null() {
        return ret_json(env, "{\"ok\":false,\"error\":\"threadsafe-function-failed\"}".into());
    }
    let previous = SUPPLY_WAKE.lock().expect("supply wake").replace(SupplyWake(handle));
    if let Some(previous) = previous {
        release_wake(previous);
    }
    crate::socket_supply::set_notify(Some(Arc::new(supply_wake)));
    ret_json(env, "{\"ok\":true}".into())
}

/// `socket_supply_unsubscribe() => '{"ok":true[,"subscribed":false]}'` — shell
/// teardown: stop waking, then release the callback. The hook is detached
/// BEFORE the handle is taken (and holding the same mutex), so no wake can enter
/// a released handle.
unsafe extern "C" fn napi_socket_supply_unsubscribe(
    env: NapiEnv,
    _info: NapiCallbackInfo,
) -> NapiValue {
    crate::socket_supply::set_notify(None);
    let taken = SUPPLY_WAKE.lock().expect("supply wake").take();
    match taken {
        Some(wake) => {
            release_wake(wake);
            ret_json(env, "{\"ok\":true}".into())
        }
        None => ret_json(env, "{\"ok\":true,\"subscribed\":false}".into()),
    }
}

unsafe extern "C" fn napi_socket_supply_reply(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::socket_supply::reply_json(&a.string_at(0), a.i32_at(1, -1), &a.string_at(2)))
}

unsafe extern "C" fn napi_socket_supply_close_fd(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::socket_supply::close_fd_json(a.i32_at(0, -1)))
}

unsafe extern "C" fn napi_socket_supply_shutdown(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    crate::socket_supply::shutdown();
    ret_json(env, "{\"ok\":true}".into())
}

unsafe extern "C" fn napi_socket_supply_dns_servers(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::socket_supply::set_dns_servers_json(&a.string_at(0)))
}

// ---------------------------------------------------------------------------
// 0.0.34 (peer-status lag REPORT §5 F1): the status-edge wake
//
// The crate's SECOND threadsafe function, in the SAME shape as the socket-supply
// wake above (created from the ArkTS callback, invoked with ZERO arguments from
// whichever core thread saw the edge, released in abort mode at teardown) —
// because that shape is the one already measured in-tree for this runtime.
//
// What differs is the COALESCING: the socket-supply wake is one wake per
// request (the shell drains a queue), while a status edge is a "the rendered
// state changed, republish" notification and upstream raises exactly ONE
// pending wake per burst (`peer/notifier.go:205-213`, a capacity-1 channel
// send with `default:`). The capacity-1 token therefore lives in
// `crate::status_events` — `status_wake` below is only the transport, called
// at most once per pending token — and the token is consumed HERE, at the
// delivery point (the runtime accepted the call and is about to run the
// callback), which is exactly where upstream's deliverer goroutine receives
// from its channel before it calls `OnPeersListChanged` (`:215-233`).
//
// Ownership/teardown: identical to the supply wake. Only the handle is stored
// (the threadsafe function keeps the ArkTS callback alive); `STATUS_WAKE` is
// held across `napi_call_threadsafe_function` / `napi_release_threadsafe_function`;
// unsubscribe detaches the hook BEFORE taking the handle, so no wake can enter
// a released handle.
// ---------------------------------------------------------------------------

struct StatusWake(*mut c_void);
unsafe impl Send for StatusWake {}
static STATUS_WAKE: Mutex<Option<StatusWake>> = Mutex::new(None);

unsafe extern "C" fn status_wake_call_js(
    env: NapiEnv,
    js_callback: NapiValue,
    _context: *mut c_void,
    _data: *mut c_void,
) {
    // The pending token is consumed at the DELIVERY point, BEFORE the shell's
    // callback runs: edges raised during that callback then raise a fresh wake
    // (delivered after it returns), and edges raised before it are already part
    // of the state the callback is about to read — nothing is lost.
    crate::status_events::wake_consumed();
    wake_call_js_no_args(env, js_callback, "status-edge");
}

/// The wake installed into `crate::status_events`: called from whichever core
/// thread saw the edge, never from the ArkTS thread. NON-BLOCKING — a busy
/// ArkTS thread must never stall a data-plane thread that just observed a real
/// transition. `true` = handed to the runtime; `false` = refused (no handle,
/// or a closing runtime), and `status_events::edge` then releases the
/// coalescing token so the next edge tries again.
fn status_wake() -> bool {
    let guard = STATUS_WAKE.lock().expect("status wake");
    let Some(wake) = guard.as_ref() else { return false };
    let rc = unsafe {
        napi_call_threadsafe_function(wake.0, core::ptr::null_mut(), NAPI_TSFN_NONBLOCKING)
    };
    if rc != 0 {
        crate::hilog::emit(&format!("status-edge: wake rejected rc={rc}"));
        return false;
    }
    true
}

fn release_status_wake(wake: StatusWake) {
    let rc = unsafe { napi_release_threadsafe_function(wake.0, NAPI_TSFN_ABORT) };
    if rc != 0 {
        crate::hilog::emit(&format!("status-edge: wake release rc={rc}"));
    }
}

/// `connector_status_subscribe(callback) => '{"ok":true}'` — install the
/// ArkTS edge wake (the shell calls this once per live watcher). An existing
/// subscription is replaced and released, so a restarted shell can subscribe
/// again without leaking the previous callback. Failures are answered, never
/// thrown: this is a LATENCY optimization — the retained 5 s heartbeat
/// publication keeps the document fresh without it.
unsafe extern "C" fn napi_connector_status_subscribe(
    env: NapiEnv,
    info: NapiCallbackInfo,
) -> NapiValue {
    let a = cb_args(env, info);
    if a.argc == 0 || a.argv[0].is_null() {
        return ret_json(env, "{\"ok\":false,\"error\":\"callback-required\"}".into());
    }
    let label = b"status-edge-wake\0";
    let mut name: NapiValue = core::ptr::null_mut();
    if napi_create_string_utf8(env, label.as_ptr(), label.len() - 1, &mut name) != 0 {
        return ret_json(env, "{\"ok\":false,\"error\":\"resource-name-failed\"}".into());
    }
    let mut handle: *mut c_void = core::ptr::null_mut();
    let rc = napi_create_threadsafe_function(
        env,
        a.argv[0],
        core::ptr::null_mut(),
        name,
        // unbounded queue: the capacity-1 COALESCING is core-side (the pending
        // token in `status_events`), so at most one wake is ever in flight and
        // bounding this queue could only add a drop path of its own.
        0,
        // initial_thread_count = 1: the ArkTS thread that created it (same as
        // the supply wake); the many core threads are callers, not owners.
        1,
        core::ptr::null_mut(),
        None,
        core::ptr::null_mut(),
        Some(status_wake_call_js),
        &mut handle,
    );
    if rc != 0 || handle.is_null() {
        return ret_json(env, "{\"ok\":false,\"error\":\"threadsafe-function-failed\"}".into());
    }
    let previous = STATUS_WAKE.lock().expect("status wake").replace(StatusWake(handle));
    if let Some(previous) = previous {
        release_status_wake(previous);
    }
    crate::status_events::set_wake(Some(Arc::new(status_wake)));
    ret_json(env, "{\"ok\":true}".into())
}

/// `connector_status_unsubscribe() => '{"ok":true[,"subscribed":false]}'` —
/// shell teardown: stop waking, then release the callback. The hook is
/// detached BEFORE the handle is taken (and holding the same mutex), so no
/// wake can enter a released handle.
unsafe extern "C" fn napi_connector_status_unsubscribe(
    env: NapiEnv,
    _info: NapiCallbackInfo,
) -> NapiValue {
    crate::status_events::set_wake(None);
    let taken = STATUS_WAKE.lock().expect("status wake").take();
    match taken {
        Some(wake) => {
            release_status_wake(wake);
            ret_json(env, "{\"ok\":true}".into())
        }
        None => ret_json(env, "{\"ok\":true,\"subscribed\":false}".into()),
    }
}

/// `connector_status_revision() => '{"revision":N}'` — the CHEAP change
/// detector (see `crate::connector::connector_status_revision_json`): one
/// atomic load, no document, no lock, no connector slot.
unsafe extern "C" fn napi_connector_status_revision(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::connector_status_revision_json())
}

// N7: shell-side feed of the PROTECTED WG outer UDP socket (wg_fwd_open +
// VpnConnection.protect BEFORE any datagram) — first of the two feeds the
// real WireGuard data plane needs. The fd crosses as a number; native
// dup-consumes it at device construction (never uses/closes the original).
unsafe extern "C" fn napi_connector_wg_socket_feed(
    env: NapiEnv,
    info: NapiCallbackInfo,
) -> NapiValue {
    let a = cb_args(env, info);
    let fd = a.i32_at(0, -1);
    ret_json(env, crate::connector::connector_wg_socket_feed_json(fd))
}

// N7: shell-side feed of the platform TUN fd (VpnConnection.create) — the
// shell KEEPS the raw fd (destroy() closes it); native consumes ONLY a dup
// copy (TunFd contract).
unsafe extern "C" fn napi_connector_tun_fd_feed(
    env: NapiEnv,
    info: NapiCallbackInfo,
) -> NapiValue {
    let a = cb_args(env, info);
    let fd = a.i32_at(0, -1);
    ret_json(env, crate::connector::connector_tun_fd_feed_json(fd))
}

// roam-1005-0124: shell-side pre-destroy TUN release (the recreate leg's
// first step) — closes the device's current TUN dup, sessions kept.
unsafe extern "C" fn napi_connector_tun_release(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::connector_tun_release_json())
}

// N8: shell ACK of the route set applied to the platform VpnConfig at
// (re)create() time. `{"routes":[...]}` JSON first, isRecreate bool second —
// arms the controlled-recreate comparison (connector_status `recreate`).
//
// Core 5f (ADDITIVE, arity-preserving): an OPTIONAL THIRD argument carries the
// ACK `kind` as a STRING — `initial` | `auto` | `user`. ABSENT (the two-argument
// call, the only form a shell older than Core 5f sends) keeps the legacy
// boolean entry byte-identical (`initial` when false, `auto` when true);
// PRESENT dispatches to the kind-aware entry, where a non-string or an unknown
// token is REFUSED with `kind-invalid` — never a silent fallback onto the
// budget-spending `auto` kind. Either way the reply is the frozen
// `{"ok":true,"recreate":{…}}` / `{"ok":false,"error":"…"}` document: the
// arguments never cross back, and no secret material is involved.
unsafe extern "C" fn napi_connector_route_set_applied(
    env: NapiEnv,
    info: NapiCallbackInfo,
) -> NapiValue {
    let a = cb_args(env, info);
    let routes = a.string_at(0);
    let is_recreate = a.bool_at(1, false);
    let json = if a.argc >= 3 {
        crate::connector::connector_route_set_applied_kind_json(&routes, &a.string_at(2))
    } else {
        crate::connector::connector_route_set_applied_json(&routes, is_recreate)
    };
    ret_json(env, json)
}

// N3-7: open + bind (NO connect) a TCP management socket for the shell
// protect gate (the wg_fwd_open split, TCP variant).
unsafe extern "C" fn napi_mgmt_socket_open(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::mgmtsock::mgmt_socket_open())
}

// N5c/N7 device-path gap: open an UNBOUND UDP socket for the shell protect
// gate; the ICE consumer dups it, binds the copy per interface and reads the
// port back (see mgmtsock::udp_socket_open).
unsafe extern "C" fn napi_ice_socket_open(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::mgmtsock::udp_socket_open())
}

unsafe extern "C" fn napi_connector_status(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::connector_status_json())
}

// 006: ADDITIVE read-only resource snapshot (separate function, separate
// document). The frozen `connector_status` JSON/order is untouched; this one
// reports only counts of resources core still owns (generation, ICE sessions /
// live local sockets, protected-ICE queue, socket-supply pending/queued, relay
// lanes/opens/foreign clients, pending DNS receivers) with `null` for any
// value whose leaf lock was busy. Numeric counts only — never a peer key, a
// relay URL, an fd number or a secret. Called by the shell bridge once/60 s.
unsafe extern "C" fn napi_connector_resource_status(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::connector_resource_status_json())
}

// N3-6: read-only shell snapshot of the last applied network map (tunnel
// address, routes + default marks, DNS, peer summary — public keys only).
unsafe extern "C" fn napi_connector_network_config(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::connector_network_config_json())
}

unsafe extern "C" fn napi_connector_stop(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::connector_stop_json())
}

// Explicit user logout: ONE management Logout RPC on the live connector's
// logged-in client (deregisters + deletes the peer server-side). The ONLY
// Logout entry — connector_stop never logs out on any stop path.
unsafe extern "C" fn napi_connector_logout(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::connector_logout_json())
}

unsafe extern "C" fn napi_n2_socket_trace_arm(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    crate::n2_socket_trace::arm(cb_args(env, info).bool_at(0, false));
    ret_json(env, "{\"ok\":true}".into())
}

unsafe extern "C" fn napi_n2_socket_trace_protect(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    let fields: Vec<_> = a.string_at(2).split(',').map(str::to_owned).collect();
    let fd = a.i32_at(0, -1);
    if fields.len() == 4 {
        if let (Ok(generation), Ok(net_id), Ok(elapsed_ms)) = (
            fields[1].parse::<u64>(), fields[2].parse::<u64>(), fields[3].parse::<u64>()) {
            crate::n2_socket_trace::protect(fd, &a.string_at(1), &fields[0],
                generation, net_id, elapsed_ms, a.bool_at(3, false));
        }
    }
    // N2 T7a (item 6): the armed protect wrapper logs `local=` on its
    // `phase=protect_settle` line, so this reply carries the socket's own
    // bound name (`getsockname`; `unknown` when the trace is unarmed or the
    // number cannot be inspected — never an invented address).
    let local = if crate::n2_socket_trace::armed() {
        crate::n2_socket_trace::local_addr(fd).map(|addr| addr.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    } else {
        "unknown".to_string()
    };
    ret_json(env, format!("{{{},{}}}", crate::util::jbool("ok", true),
        crate::util::jstr("local", &local)))
}

unsafe extern "C" fn napi_n2_tun_dest_configure(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_tun_dest_configure_json(&a.string_at(0)))
}

// I5 N2-H device-side evidence collector (design
// docs/n2h-device-collector-design-20260917.md §5.1): four synchronous
// JSON-string exports, no callbacks, no threads created at the NAPI boundary
// (the window runs on a core-owned thread started inside `arm`). Inert until
// the control file arms them (§6.1): no arm ⇒ every export answers the
// documented not-armed / no-evidence shape and `connector_status()` — the
// frozen document — is untouched (§5.2).
unsafe extern "C" fn napi_n2_control_send(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_control_send_json(a.i32_at(0, -1),
        &a.string_at(1), &a.string_at(2), &a.string_at(3)))
}

unsafe extern "C" fn napi_n2_control_status(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_control_status_json(&a.string_at(0)))
}

// N2 T5a「B 断流刺激」(preregistration v4 §4 B / §7 T5a): `kind` in
// {M,S,R,I,Wb,Ws,D} + the stimulus id. Refused (and still logged) unless the
// evidence ticket armed the socket trace; every call emits one
// `N2_STIMULUS|…` line with the fixed `diag=true|is_evidence=false` tail.
unsafe extern "C" fn napi_n2_stimulus(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_stimulus_json(&a.string_at(0), &a.string_at(1)))
}

// N2 P4「A 列生产事件触发原语」(v4.1 §4 A 列操作性定义): the shell's armed-only
// entry (`--ps n2_a_event '{"kind":"M|R|D","id":"…"}'`). One call = one
// trigger; it never probes the connector unless an accepted evidence ticket
// armed the trace. `S` is always refused with its frozen reason.
unsafe extern "C" fn napi_n2_a_event(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_a_event_json(&a.string_at(0), &a.string_at(1)))
}

// N2 P5 (v4.1 §4 I/Ws rows): the shell's automatic sandwich orchestration —
// register a PRE leg (with its own pre AND post control markers, N2 T7a item
// 1) before sending the pre control, poll for the legs whose first production
// send was observed, and close a leg with the post outcome.
unsafe extern "C" fn napi_n2_sandwich_pre(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_sandwich_pre_json(&a.string_at(0), &a.string_at(1),
        &a.string_at(2)))
}

unsafe extern "C" fn napi_n2_sandwich_post(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_sandwich_post_json(&a.string_at(0), &a.string_at(1),
        &a.string_at(2), &a.string_at(3)))
}

unsafe extern "C" fn napi_n2_sandwich_poll(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::n2_sandwich_poll_json())
}

// N2 first batch C/D columns (2026-09-29): the App-side switching sandwich. The
// host issues the plan while Wi-Fi is up; the shell then drives the D
// preparation (`prepare`/`ready`), the pre control, the IDENTIFIED switch
// report, the receipt poll and the post control. Every entry is inert (silent)
// while unarmed.
#[allow(clippy::too_many_arguments)]
unsafe extern "C" fn napi_n2_cd_plan(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_cd_plan_json(&a.string_at(0), &a.string_at(1),
        &a.string_at(2), a.i32_at(3, 0), &a.string_at(4), &a.string_at(5), &a.string_at(6),
        &a.string_at(7)))
}

// D column: the Wi-Fi → cellular preparation hop (recorded, never the D
// measurement) and its recovery on the cellular side.
unsafe extern "C" fn napi_n2_cd_prepare(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_cd_prepare_json(&a.string_at(0), &a.string_at(1),
        &a.string_at(2), &a.string_at(3)))
}

unsafe extern "C" fn napi_n2_cd_ready(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_cd_ready_json(&a.string_at(0)))
}

unsafe extern "C" fn napi_n2_cd_pre(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_cd_pre_json(&a.string_at(0), &a.string_at(1),
        &a.string_at(2)))
}

// The switch report WITH the identity baseline: `(net_id_after, fd, dup_fd,
// generation)` of the OLD network's live registration, as the `N2_SOCK` receipt
// names them (`fd=` shell original, `dup_fd=` core-owned copy,
// `protect_generation=`). -1 in the fd/dup pair = not measured (fail-closed).
#[allow(clippy::too_many_arguments)]
unsafe extern "C" fn napi_n2_cd_switch_identified(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_cd_switch_identified_json(&a.string_at(0), &a.string_at(1),
        &a.string_at(2), &a.string_at(3), a.i32_at(4, -1) as i64, a.i32_at(5, -1),
        a.i32_at(6, -1), a.i32_at(7, -1) as i64))
}

// N2 C/D: the identity of the live registration of one outer class, as the
// `N2_SOCK` receipts name it (`{ok,fd,dup_fd,generation,net_id}`). Read-only:
// the shell fills the switch report's `identity_fd`/`identity_dup_fd`/
// `identity_generation` from it and passes `-1` for every field while the
// answer is not `ok` (the core then fail-closes the plan by name).
unsafe extern "C" fn napi_n2_cd_identity(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_cd_identity_json(&a.string_at(0)))
}

unsafe extern "C" fn napi_n2_cd_poll(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::n2_cd_poll_json())
}

unsafe extern "C" fn napi_n2_cd_post(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_cd_post_json(&a.string_at(0), &a.string_at(1),
        &a.string_at(2)))
}

// N2 first batch C/D columns (2026-09-29): the armed-only app-internal evidence
// ring file (configure / forward one shell line / disable / counters) plus the
// B4 retrieval surface: the seq-paged dump, the manifest, and the EXPLICIT
// session-bound host ACK (`{"session":…,"up_to_seq":…}`) that alone moves the
// retrieved watermark gating rotation.
unsafe extern "C" fn napi_n2_ring_configure(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_ring_configure_json(&a.string_at(0), a.i32_at(1, 0) as i64,
        a.i32_at(2, 0) as i64))
}

unsafe extern "C" fn napi_n2_ring_append(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_ring_append_json(&a.string_at(0)))
}

unsafe extern "C" fn napi_n2_ring_disable(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::n2_ring_disable_json())
}

unsafe extern "C" fn napi_n2_ring_status(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::n2_ring_status_json())
}

unsafe extern "C" fn napi_n2_ring_dump(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_ring_dump_json(&a.string_at(0)))
}

unsafe extern "C" fn napi_n2_ring_manifest(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::n2_ring_manifest_json())
}

unsafe extern "C" fn napi_n2_ring_ack(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::n2_ring_ack_json(&a.string_at(0)))
}

unsafe extern "C" fn napi_n2h_collector_arm(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::n2h_collector::arm_json(&a.string_at(0)))
}

unsafe extern "C" fn napi_n2h_collector_status(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::n2h_collector::status_json())
}

unsafe extern "C" fn napi_n2h_collector_evidence(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::n2h_collector::evidence_json(a.i32_at(0, -1) as i64))
}

unsafe extern "C" fn napi_n2h_collector_disarm(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::n2h_collector::disarm_json())
}

// 2026-09-26 protect-probe seam (protect-bypass plan §4/§6, DIAGNOSTIC-ONLY,
// opt-in): the ice_socket_open hand-off shape applied to the protected arm.
//   `n2h_probe_socket_open()` -> {ok,fd,handoff_id}: ONE unbound, native-owned
//     UDP fd for the shell to run ITS protect wrapper on;
//   `n2h_probe_socket_commit(fd, result)` with result in
//     "protected"/"unprotected"/"error": the shell's protect outcome for that
//     very fd, so the core's probe line can carry it. The shell must NOT close
//     the fd (the core owns and closes it) and must call both BEFORE
//     `n2h_collector_arm` with the `protect_probe` option.
unsafe extern "C" fn napi_n2h_probe_socket_open(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::n2h_collector::protect_probe_open_json())
}

unsafe extern "C" fn napi_n2h_probe_socket_commit(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(
        env,
        crate::n2h_collector::protect_probe_commit_json(a.i32_at(0, -1), &a.string_at(1)),
    )
}

// ---------------------------------------------------------------------------
// phone_log: phone-side persistent diagnostic log (crate::phone_log)
//
// The native module ALREADY tees every `hilog::emit` line into the file (native
// logs need no shell involvement). These four exports are the ArkTS-facing
// counterpart:
//
//   phone_log_configure(path) -> JSON  install/replace the file tee
//   phone_log_append(line)    -> JSON  hand ONE shell line to the tee (never blocks)
//   phone_log_status()        -> JSON  the module's own status document
//   phone_log_shutdown()      -> JSON  bounded stop REQUEST, reports pending
//
// All four return a JSON string (this file's convention) and none of them ever
// echoes the argument it was handed: an error is a code (`invalid-path`,
// `shutdown-pending`, …), never the path, the line, or any config content. This
// surface never reads configuration files.
// ---------------------------------------------------------------------------

/// Byte cap on the configure path argument. The module's own `valid_path` rule
/// is `len() <= 512`; a longer argument is refused before it is copied.
const PHONE_LOG_PATH_MAX: usize = 512;
/// Byte cap on one append line. Generous — the module caps a persisted record
/// at 16 Ki *characters* at ingress — but hard, so a pathological JS string is
/// refused instead of copied and pushed through redaction.
const PHONE_LOG_LINE_MAX: usize = 64 * 1024;
/// How long `phone_log_shutdown` may wait for the writer's completion ack.
/// Deliberately a few milliseconds: the call runs on the ArkTS event thread.
/// In the normal case (idle queue) the writer acks in microseconds and the
/// reply says `stopped:true`; when the queue is full or the writer is mid-write
/// the reply says `stopped:false` and the caller reads `phone_log_status()`
/// again later — it never parks the event thread on disk I/O.
const PHONE_LOG_SHUTDOWN_WAIT: Duration = Duration::from_millis(5);

/// `{"ok":false,"error":"<code>"}` — a code only, never the argument.
fn phone_log_error(code: &str) -> String {
    format!("{{{},{}}}", crate::util::jbool("ok", false), crate::util::jstr("error", code))
}

/// Success reply of `phone_log_configure`: the state read back from the module
/// (never the argument echoed) plus whether the call was a same-path no-op.
fn phone_log_configured_json(idempotent: bool) -> String {
    let s = crate::phone_log::stats();
    format!(
        "{{{},{},{},{},{},{},{},{}}}",
        crate::util::jbool("ok", true),
        crate::util::jbool("configured", true),
        crate::util::jbool("idempotent", idempotent),
        crate::util::jbool("enabled", s.enabled),
        crate::util::jbool("running", s.running),
        crate::util::jstr("session", &s.session),
        crate::util::jstr("path", &s.path),
        crate::util::jnum("queue_capacity", s.queue_capacity as u64),
    )
}

/// Bounded configure body (no NAPI types, so the unit test can drive it).
///
/// ONCE PER PATH: the module replaces a logger by installing the new writer
/// first and only then draining the old one, so calling it again on the path
/// that is already installed would briefly put TWO writers on the same file
/// (interleaved records, racing rotation). When the installed path is exactly
/// the requested one this answers from the live state and starts nothing:
///   * tee enabled                   -> `idempotent:true`, running session kept;
///   * tee disabled, writer draining -> refused (`shutdown-pending`), so no
///     second writer is opened on that file. A caller that needs the tee again
///     waits for `phone_log_status().running == false` (normally milliseconds,
///     no busy polling) and retries this export.
fn phone_log_configure_export(path: &str) -> String {
    let current = crate::phone_log::stats();
    if current.path == path {
        if current.enabled {
            return phone_log_configured_json(true);
        }
        if current.running {
            return phone_log_error("shutdown-pending");
        }
    }
    match crate::phone_log::configure(path) {
        Ok(()) => phone_log_configured_json(false),
        Err(code) => phone_log_error(code),
    }
}

/// Bounded append body. `None` = missing / not a string / empty / over the byte
/// cap; nothing is copied past the probe and the argument is never echoed.
/// `{"ok":true}` means "handed to the tee at ingress", NOT "on disk": the
/// module admits lines through a per-second budget and a bounded queue and
/// DROPS the overflow by design (counted in `dropped`, the budget part in
/// `rate_suppressed`; both visible via `phone_log_status()`), and an
/// unconfigured/stopped tee answers `not-configured`.
///
/// This is the SHELL entrance (the ArkTS diagnostics sink), so it routes through
/// [`crate::phone_log::append_shell`]: the shell hands over opaque record text,
/// and that entry classifies the small lifecycle/selection marker set against
/// the module's own priority window (0.0.27). The core's per-packet storm never
/// comes through here — it uses `hilog::emit`/`phone_log::append`, which pay no
/// scan.
fn phone_log_append_export(line: Option<&str>) -> String {
    let Some(line) = line else {
        return phone_log_error("invalid-line");
    };
    if !crate::phone_log::enabled() {
        return phone_log_error("not-configured");
    }
    crate::phone_log::append_shell(line);
    format!("{{{}}}", crate::util::jbool("ok", true))
}

/// Bounded, never-blocking shutdown REQUEST. `stopped:true` = the writer
/// confirmed it drained, closed the file and exited. `stopped:false` = not
/// confirmed inside the bounded wait: the request is either already queued
/// behind the backlog or could not be enqueued yet — `running` says whether the
/// writer is still alive and `pending` how many accepted records have not
/// reached the OS. The caller does NOT busy-poll: it reads `phone_log_status()`
/// later (or on its next lifecycle tick) and calls this export again only while
/// `running` is still true.
fn phone_log_shutdown_export() -> String {
    let confirmed = crate::phone_log::shutdown(PHONE_LOG_SHUTDOWN_WAIT);
    let s = crate::phone_log::stats();
    // A writer that already exited (or exited while the ack was in flight) is
    // stopped even when the bounded wait did not observe the ack.
    let stopped = confirmed || !s.running;
    format!(
        "{{{},{},{},{}}}",
        crate::util::jbool("ok", true),
        crate::util::jbool("stopped", stopped),
        crate::util::jbool("running", s.running),
        crate::util::jnum("pending", s.pending),
    )
}

/// The module's status document, verbatim: `component` / `enabled` / `running`
/// / `session` / `path` / the counters (`offered`, `accepted`, `written`,
/// `dropped`, `failed`, `pending`, `rotations`, …) / `last_error` (null or an
/// errno-style code — never a path or content).
fn phone_log_status_export() -> String {
    crate::phone_log::status_json()
}

/// `phone_log_configure(path)` — install the phone log on `path` (the module's
/// rotation/redaction rules apply; this call does not read any config).
unsafe extern "C" fn napi_phone_log_configure(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    match a.string_at_bounded(0, PHONE_LOG_PATH_MAX) {
        Some(path) => ret_json(env, phone_log_configure_export(&path)),
        None => ret_json(env, phone_log_error("invalid-path")),
    }
}

/// `phone_log_append(line)` — non-blocking hand-off; see the body docs.
unsafe extern "C" fn napi_phone_log_append(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    let line = a.string_at_bounded(0, PHONE_LOG_LINE_MAX);
    ret_json(env, phone_log_append_export(line.as_deref()))
}

/// `phone_log_status()` — the module's status document, verbatim.
unsafe extern "C" fn napi_phone_log_status(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, phone_log_status_export())
}

/// `phone_log_shutdown()` — bounded stop request; see the body docs.
unsafe extern "C" fn napi_phone_log_shutdown(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, phone_log_shutdown_export())
}

const ACCOUNT_AUTH_INPUT_MAX: usize = 16 * 1024;

unsafe extern "C" fn napi_account_auth_discovery_start(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let args = cb_args(env, info);
    match args.string_at_bounded(0, ACCOUNT_AUTH_INPUT_MAX) {
        Some(request) => ret_json(env, crate::account_auth::account_auth_discovery_start_json(&request)),
        None => ret_json(env, "{\"ok\":false,\"error\":\"invalid-input\"}".into()),
    }
}

unsafe extern "C" fn napi_account_auth_discovery_poll(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let args = cb_args(env, info);
    let id = args.i32_at(0, 0);
    ret_json(env, crate::account_auth::account_auth_discovery_poll_json(id.max(0) as u64))
}

unsafe extern "C" fn napi_account_auth_discovery_cancel(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let args = cb_args(env, info);
    let id = args.i32_at(0, 0);
    ret_json(env, crate::account_auth::account_auth_discovery_cancel_json(id.max(0) as u64))
}

// 2026-10-12: the login action's management step (same one-shot job shape as
// the discovery trio). `account_login_start(requestJson)` takes
// `{"config": <connector start config>, "jwt": <token exchange result>}`; the
// JWT lives only inside the job and is never written anywhere.
unsafe extern "C" fn napi_account_login_start(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let args = cb_args(env, info);
    match args.string_at_bounded(0, ACCOUNT_AUTH_INPUT_MAX) {
        Some(request) => ret_json(env, crate::account_auth::account_login_start_json(&request)),
        None => ret_json(env, "{\"ok\":false,\"error\":\"invalid-input\"}".into()),
    }
}

unsafe extern "C" fn napi_account_login_poll(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let args = cb_args(env, info);
    let id = args.i32_at(0, 0);
    ret_json(env, crate::account_auth::account_login_poll_json(id.max(0) as u64))
}

unsafe extern "C" fn napi_account_login_cancel(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let args = cb_args(env, info);
    let id = args.i32_at(0, 0);
    ret_json(env, crate::account_auth::account_login_cancel_json(id.max(0) as u64))
}

// 2026-10-13: the interactive session extend's management step (same one-shot
// job shape as the login action). `account_extend_start(requestJson)` takes
// `{"config": <connector start config>, "jwt": <token exchange result>}`; the
// JWT lives only inside that call and is never written anywhere. The poll
// answer is `{"state":"done","outcome":"success","session_expiry":…,
// "session_expires_at_unix":…}` — the server's own 3-state deadline.
unsafe extern "C" fn napi_account_extend_start(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let args = cb_args(env, info);
    match args.string_at_bounded(0, ACCOUNT_AUTH_INPUT_MAX) {
        Some(request) => ret_json(env, crate::account_auth::account_extend_start_json(&request)),
        None => ret_json(env, "{\"ok\":false,\"error\":\"invalid-input\"}".into()),
    }
}

unsafe extern "C" fn napi_account_extend_poll(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let args = cb_args(env, info);
    let id = args.i32_at(0, 0);
    ret_json(env, crate::account_auth::account_extend_poll_json(id.max(0) as u64))
}

unsafe extern "C" fn napi_account_extend_cancel(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let args = cb_args(env, info);
    let id = args.i32_at(0, 0);
    ret_json(env, crate::account_auth::account_extend_cancel_json(id.max(0) as u64))
}

// Increment 131: the ArkTS-facing route catalog, a ZERO-argument read-only
// getter over the core owner's `connector::connector_routes_json()` (decision B
// dedicated document — `connector_status()` / `connector_network_config()`
// keep their frozen shapes and order). The body is a side-effect-free mutex
// snapshot: no gate refresh, no recreate bookkeeping, no network/IO, and the
// caller CANNOT select/deselect through THIS getter (`selection` is read-only
// state — the live setter is the separate `connector_route_selection_set`) and
// no secret material (NetIDs, prefixes, counts only).
// Shape:
// `{available,serial,selection:{selected_routes,deselected_routes,deselect_all},`
// `networks:[{net_id,networks,candidate_count,enabled,is_exit}]}`.
// `available:false` + `serial:0` + `networks:[]` = no route catalog (no
// connector, before the first map, after stop, or a map that advertised no
// routes) — never a fabricated empty-but-active state.
unsafe extern "C" fn napi_connector_routes(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::connector_routes_json())
}

// Increment 132 (Core 5f): the ArkTS-facing LIVE route-selection setter — ONE
// string argument, the RAW selector document exactly as `connector_routes()`
// reports it back (`{"selected_routes":[…],"deselected_routes":[…],`
// `"deselect_all":bool}`), handed VERBATIM to the core owner's default parser
// (`connector::connector_route_selection_set_json` → `RouteSelector::from_json`,
// the same strict parser the start config and the persisted document use): this
// wrapper adds no encoding step, no second parser and no field of its own, and
// a refused document mutates nothing.
//
// The reply is the core document's typed JSON (fixed key order):
// `{"ok":bool,"error"?:"no-connector"|"selection-invalid"|"wg-apply-failed",`
// `"available":bool,"changed":bool,"desired_routes":[…],"recreate_needed":bool}`
// — the ArkTS side's `RouteSelectionSetResult`. `selection-invalid` is decided
// BEFORE the connector lookup, so it is the same answer with or without a live
// connector. Nothing is logged here and no payload or field value is echoed:
// routing material only (NetID-level selection, prefixes), never a key, an fd
// or a secret.
unsafe extern "C" fn napi_connector_route_selection_set(
    env: NapiEnv,
    info: NapiCallbackInfo,
) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::connector::connector_route_selection_set_json(&a.string_at(0)))
}

// Additive, tail-appended Rust-heap readout (`alloc_stats()`): ZERO arguments,
// read-only, synchronous. Returns the flat document
// `{"bytes":n,"peak":n,"allocs":n,"frees":n}` — live bytes / high-water mark /
// cumulative allocation and deallocation events of the process's RUST global
// allocator (the counters installed in lib.rs; `crate::alloc_stats` module docs
// define each number and what is NOT counted: ArkTS/JS heap, libace_napi and
// every C/C++ allocation stay invisible here — this is not process RSS).
// The read takes four relaxed atomic loads and takes NO lock; the only
// allocation is the small reply string (created after the counters were read).
// A1-clean: nothing threadsafe / async is involved.
unsafe extern "C" fn napi_alloc_stats(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::alloc_stats::snapshot_json())
}
