//! Handwritten NAPI bindings (no napi-rs dependency; promoted from
//! spikes/n1b-disc-phys-hap/probe/src/napi.rs with the export surface trimmed
//! to the product skeleton: version / abi_probe / fd_status / wg_* / tun_*).
//! A1-clean: this file contains ZERO napi_create_threadsafe_function /
//! napi_create_async_work — all exports are synchronous. Symbols resolve at
//! load time from the host process's libace_napi.z.so (DT_NEEDED via
//! .cargo/config.toml link args).

use core::ffi::c_void;

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
    reg!("connector_start", napi_connector_start);
    reg!("connector_start_with_socket", napi_connector_start_with_socket);
    reg!("connector_socket_feed", napi_connector_socket_feed);
    reg!("connector_ice_socket_feed", napi_connector_ice_socket_feed);
    reg!("connector_signal_socket_feed", napi_connector_signal_socket_feed);
    reg!("connector_wg_socket_feed", napi_connector_wg_socket_feed);
    reg!("connector_tun_fd_feed", napi_connector_tun_fd_feed);
    reg!("connector_status", napi_connector_status);
    reg!("connector_network_config", napi_connector_network_config);
    reg!("connector_stop", napi_connector_stop);
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

struct Args {
    env: NapiEnv,
    argv: [NapiValue; 4],
    argc: usize,
}

fn cb_args(env: NapiEnv, info: NapiCallbackInfo) -> Args {
    let mut argv: [NapiValue; 4] = [core::ptr::null_mut(); 4];
    let mut argc: usize = 4;
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
// through napi_set_named_property, while native runtime behavior is not exercised.
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
    }

    pub fn reset() {
        FAKE_FUNCS.with(|m| m.borrow_mut().clear());
        SET_CALLS.with(|c| c.borrow_mut().clear());
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
        _info: NapiCallbackInfo,
        _argc: *mut usize,
        _argv: *mut NapiValue,
        _this_arg: *mut NapiValue,
        _data: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _env: NapiEnv,
        _str_: *const u8,
        _length: usize,
        _result: *mut NapiValue,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_string_utf8(
        _env: NapiEnv,
        _value: NapiValue,
        _buf: *mut u8,
        _bufsize: usize,
        _result: *mut usize,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_int32(_env: NapiEnv, _value: NapiValue, _result: *mut i32) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(_env: NapiEnv, _value: NapiValue, _result: *mut bool) -> i32 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::host_stubs::{fake_funcs, reset, set_calls};
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
            "connector_start",
            "connector_start_with_socket",
            "connector_socket_feed",
            "connector_ice_socket_feed",
            "connector_signal_socket_feed",
            "connector_wg_socket_feed",
            "connector_tun_fd_feed",
            "connector_status",
            "connector_network_config",
            "connector_stop",
        ] {
            let call = calls.iter().find(|(_, n, _)| n == name)
                .unwrap_or_else(|| panic!("{} not attached to exports", name));
            assert_eq!(call.0, exports as usize, "{} attached to wrong object", name);
            let fake = *funcs.get(name)
                .unwrap_or_else(|| panic!("{} create_function not called", name));
            assert_ne!(fake, 0, "{} fake function must be non-null", name);
            assert_eq!(call.2, fake, "{} value must be the created function object", name);
        }
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

// N3-7: shell-side resupply of fresh protected sockets (reconnect dials).
unsafe extern "C" fn napi_connector_socket_feed(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    let fd = a.i32_at(0, -1);
    ret_json(env, crate::connector::connector_socket_feed_json(fd))
}

// N5c: shell-side resupply of fresh protected UDP sockets for the per-peer
// ICE sessions (fail-closed: no feed ⇒ no candidates).
unsafe extern "C" fn napi_connector_ice_socket_feed(
    env: NapiEnv,
    info: NapiCallbackInfo,
) -> NapiValue {
    let a = cb_args(env, info);
    let fd = a.i32_at(0, -1);
    ret_json(env, crate::connector::connector_ice_socket_feed_json(fd))
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

// N3-7: open + bind (NO connect) a TCP management socket for the shell
// protect gate (the wg_fwd_open split, TCP variant).
unsafe extern "C" fn napi_mgmt_socket_open(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::mgmtsock::mgmt_socket_open())
}

unsafe extern "C" fn napi_connector_status(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::connector_status_json())
}

// N3-6: read-only shell snapshot of the last applied network map (tunnel
// address, routes + default marks, DNS, peer summary — public keys only).
unsafe extern "C" fn napi_connector_network_config(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::connector_network_config_json())
}

unsafe extern "C" fn napi_connector_stop(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::connector::connector_stop_json())
}
