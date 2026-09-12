//! Native library load verification (promoted from
//! spikes/n1b-disc-phys-hap/probe/src/d1.rs; fn renamed `d1_probe` ->
//! `abi_probe`, emitted markers unchanged): dlopen the native member (path
//! passed in by ArkTS — the HAP-internal native library install path) and
//! dlsym each of the 14 frozen symbols BY NAME without calling any.
//!
//! Marker sequence: `N1BDISC_D1_BEGIN` -> `N1BDISC_D1_LOADED|so=<member>`
//! | `N1BDISC_D1_FAIL|err=<dlerror>` (full text also via CHUNK stream=dlerror
//! item=0) -> `N1BDISC_D1_SYM|total=14|resolved=<n>` -> `N1BDISC_D1_END|load=<state>|elapsed_ms=<ms>`.
//! dlopen of the already-loaded self is non-blocking.

use crate::btkeep::BT_SYMBOLS;
use crate::chunk;
use crate::hilog::emit;
use crate::sys::{self, c_char};
use crate::util::{jbool, jnum, jstr, sanitize_marker_field};

pub fn abi_probe(so_path: &str) -> String {
    let t0 = sys::mono_ms();
    emit("N1BDISC_D1_BEGIN");

    // NUL-terminated copy of the path (strip interior NULs defensively)
    let path_bytes: Vec<u8> = so_path.bytes().filter(|b| *b != 0).collect();
    let mut cpath = path_bytes.clone();
    cpath.push(0);

    let handle = unsafe { sys::dlopen(cpath.as_ptr() as *const c_char, sys::RTLD_NOW | sys::RTLD_LOCAL) };
    let load_state: &str;
    let dlerror_text: Option<String>;

    if handle.is_null() {
        // take dlerror BEFORE any other dl* call
        let e = unsafe { sys::dlerror() };
        let text = if e.is_null() {
            String::new()
        } else {
            unsafe {
                std::ffi::CStr::from_ptr(e as *const c_char as *const core::ffi::c_char)
                    .to_string_lossy()
                    .into_owned()
            }
        };
        dlerror_text = Some(text.clone());
        chunk::emit_chunk(chunk::STREAM_DLERROR, 0, &text);
        emit(&format!(
            "N1BDISC_D1_FAIL|err={}",
            sanitize_marker_field(&text, 120)
        ));
        load_state = "failed";
    } else {
        // member name = basename of the path
        let member = path_bytes
            .rsplit(|b| *b == b'/')
            .next()
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .unwrap_or_default();
        emit(&format!("N1BDISC_D1_LOADED|so={}", sanitize_marker_field(&member, 120)));
        dlerror_text = None;
        load_state = "loaded";
    }

    // dlsym resolution of the frozen 14 (never called)
    let mut resolved: u32 = 0;
    let mut unresolved: Vec<&str> = Vec::new();
    if !handle.is_null() {
        for name in BT_SYMBOLS.iter() {
            let mut cname = name.as_bytes().to_vec();
            cname.push(0);
            let sym = unsafe { sys::dlsym(handle, cname.as_ptr() as *const c_char) };
            if sym.is_null() {
                unresolved.push(name);
            } else {
                resolved += 1;
            }
        }
    }
    emit(&format!(
        "N1BDISC_D1_SYM|total={}|resolved={}",
        BT_SYMBOLS.len(),
        resolved
    ));

    let elapsed = sys::mono_ms().saturating_sub(t0);
    emit(&format!(
        "N1BDISC_D1_END|load={}|elapsed_ms={}",
        load_state, elapsed
    ));

    let mut j = String::from("{");
    j.push_str(&jstr(
        "abi_load",
        if load_state == "loaded" {
            "observed-true"
        } else {
            "observed-false"
        },
    ));
    let member = path_bytes
        .rsplit(|b| *b == b'/')
        .next()
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .unwrap_or_default();
    j.push_str(",");
    j.push_str(&jstr("abi_so_member", &member));
    j.push_str(",");
    j.push_str(&jnum("abi_symbols_total", BT_SYMBOLS.len() as u64));
    j.push_str(",");
    j.push_str(&jnum("abi_symbols_resolved", resolved as u64));
    j.push_str(",");
    j.push_str(&jstr(
        "abi_symbols_unresolved_list",
        &unresolved
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join(","),
    ));
    j.push_str(",");
    match &dlerror_text {
        Some(t) => j.push_str(&jstr("abi_dlerror", t)),
        None => j.push_str("\"abi_dlerror\":null"),
    }
    j.push_str(",");
    j.push_str(&jnum("abi_elapsed_ms", elapsed));
    j.push_str(",");
    j.push_str(&jbool("abi_loggable", crate::hilog::loggable()));
    j.push_str("}");
    j
}
