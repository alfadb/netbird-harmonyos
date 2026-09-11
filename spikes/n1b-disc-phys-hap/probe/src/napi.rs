//! Handwritten NAPI bindings (no napi-rs dependency; A1-clean: this file
//! contains ZERO napi_create_threadsafe_function / napi_create_async_work —
//! all exports are synchronous). Symbols resolve at load time from the host
//! process's libace_napi.z.so (DT_NEEDED via .cargo/config.toml link args).

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
    reg!("d1_probe", napi_d1_probe);
    reg!("d2_lock", napi_d2_lock);
    reg!("d2_late_fd", napi_d2_late_fd);
    reg!("d2_entry_attempted", napi_d2_entry_attempted);
    reg!("d2_entry_outcome", napi_d2_entry_outcome);
    reg!("d2_late", napi_d2_late);
    reg!("rejtext_emit", napi_rejtext_emit);
    reg!("d4_probe", napi_d4_probe);
    reg!("d5_probe", napi_d5_probe);
    reg!("d8a_probe", napi_d8a_probe);
    reg!("d7_probe", napi_d7_probe);
    reg!("d8b_probe", napi_d8b_probe);
    reg!("dw_start", napi_dw_start);
    reg!("dw_wait_barrier", napi_dw_wait_barrier);
    reg!("dw_wait_barrier_defer", napi_dw_wait_barrier_defer);
    reg!("dw_inwait_collect", napi_dw_inwait_collect);
    reg!("dw_destroy_t", napi_dw_destroy_t);
    reg!("dw_destroy_c", napi_dw_destroy_c);
    reg!("dw_wait_terminal", napi_dw_wait_terminal);
    reg!("dw_join", napi_dw_join);
    reg!("d6a", napi_d6a);
    reg!("d6b", napi_d6b);
    reg!("pre_emit", napi_pre_emit);
    reg!("post_emit", napi_post_emit);
    reg!("skip_emit", napi_skip_emit);
    reg!("cleanup_sockets", napi_cleanup_sockets);
    exports
}

// .init_array constructor: registers the module when the host runtime dlopens
// this .so (equivalent of the C NAPI_MODULE macro's __attribute__((constructor))).
unsafe extern "C" fn register_module() {
    static MODULE: NapiModule = NapiModule {
        nm_version: 1,
        nm_flags: 0,
        nm_filename: b"n1bdisc_probe\0".as_ptr(),
        nm_register_func: Some(napi_init),
        nm_modname: b"n1bdisc_probe\0".as_ptr(),
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

    // Real regression: napi_init must attach the created function objects to the
    // exports object it was handed (returns that same object). Fails on the old
    // behavior where reg! dropped the created value without set_named_property.
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
        for name in ["version", "d1_probe"] {
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
        "{\"name\":\"n1bdisc_probe\",\"version\":\"0.1.0\",\"spec\":\"docs/n1b-disc-gate-plan.md@04cf222+CC-1(criteria-change-1-reviewed-pass-2026-09-05)\",\"channel\":\"hilog:0x2900/N1BDiscVpn\"}".to_string(),
    )
}

unsafe extern "C" fn napi_d1_probe(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::d1::d1_probe(&a.string_at(0)))
}

unsafe extern "C" fn napi_d2_lock(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::d2::d2_lock(a.i32_at(0, -1)))
}

unsafe extern "C" fn napi_d2_late_fd(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::d2::d2_late_fd(a.i32_at(0, -1)))
}

unsafe extern "C" fn napi_d2_entry_attempted(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::d2::d2_entry_attempted(&a.string_at(0)))
}

unsafe extern "C" fn napi_d2_entry_outcome(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(
        env,
        crate::d2::d2_entry_outcome(&a.string_at(0), &a.string_at(1), a.i32_at(2, -1)),
    )
}

unsafe extern "C" fn napi_d2_late(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::d2::d2_late(&a.string_at(0), &a.string_at(1)))
}

unsafe extern "C" fn napi_rejtext_emit(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::d2::rejtext_emit(a.i32_at(0, -1), &a.string_at(1)))
}

unsafe extern "C" fn napi_d4_probe(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::d4::d4_probe(a.i32_at(0, -1), a.bool_at(1, false)))
}

unsafe extern "C" fn napi_d5_probe(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::d5::d5_probe(a.i32_at(0, -1)))
}

unsafe extern "C" fn napi_d8a_probe(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::d8::d8a_probe(a.i32_at(0, -1), a.bool_at(1, false)))
}

unsafe extern "C" fn napi_d7_probe(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::d7::d7_probe())
}

unsafe extern "C" fn napi_d8b_probe(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::d8::d8b_probe(a.i32_at(0, -1), a.bool_at(1, false)))
}

unsafe extern "C" fn napi_dw_start(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::dw::dw_start(a.i32_at(0, -1)))
}

unsafe extern "C" fn napi_dw_wait_barrier(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::dw::dw_wait_barrier())
}

unsafe extern "C" fn napi_dw_wait_barrier_defer(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::dw::dw_wait_barrier_defer())
}

unsafe extern "C" fn napi_dw_inwait_collect(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::dw::dw_inwait_collect())
}

unsafe extern "C" fn napi_dw_destroy_t(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::dw::dw_destroy_t())
}

unsafe extern "C" fn napi_dw_destroy_c(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::dw::dw_destroy_c())
}

unsafe extern "C" fn napi_dw_wait_terminal(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::dw::dw_wait_terminal())
}

unsafe extern "C" fn napi_dw_join(env: NapiEnv, _info: NapiCallbackInfo) -> NapiValue {
    ret_json(env, crate::dw::dw_join())
}

unsafe extern "C" fn napi_d6a(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::d6::d6a(a.i32_at(0, -1)))
}

unsafe extern "C" fn napi_d6b(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::d6::d6b(a.i32_at(0, -1)))
}

unsafe extern "C" fn napi_pre_emit(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::dw::pre_emit(&a.string_at(0)))
}

unsafe extern "C" fn napi_post_emit(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::dw::post_emit(a.bool_at(0, false)))
}

unsafe extern "C" fn napi_skip_emit(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    ret_json(env, crate::dw::skip_emit(&a.string_at(0), &a.string_at(1)))
}

unsafe extern "C" fn napi_cleanup_sockets(env: NapiEnv, info: NapiCallbackInfo) -> NapiValue {
    let a = cb_args(env, info);
    let cause = {
        let c = a.string_at(0);
        if c.is_empty() {
            "not-run".to_string()
        } else {
            c
        }
    };
    ret_json(env, crate::dw::cleanup_sockets(&cause))
}
