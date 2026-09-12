//! HiLog output channel (frozen): domain 0x2900, tag "N1BDiscVpn".
//!
//! Raw `extern "C"` binding against libhilog_ndk.z.so (no heavyweight deps).
//! Every marker goes through `emit()`. The log format string is the constant
//! `"%{public}s"` and the marker text is passed as the single argument, so
//! marker payloads can never be interpreted as printf format directives.
//! OH_LOG_Print is thread-safe; the D-W worker thread calls it directly.

const LOG_APP: i32 = 0; // LogType
const LOG_INFO: i32 = 4; // LogLevel
pub const DOMAIN: u32 = 0x2900;
pub const TAG: &[u8] = b"N1BDiscVpn\0";
const FMT: &[u8] = b"%{public}s\0";

extern "C" {
    fn OH_LOG_Print(
        log_type: i32,
        level: i32,
        domain: u32,
        tag: *const u8,
        fmt: *const u8,
        ...
    ) -> i32;
    fn OH_LOG_IsLoggable(domain: u32, tag: *const u8, level: i32) -> bool;
}

/// Emit one marker line on the frozen HiLog channel.
/// Strips interior NUL bytes (cannot cross the C string boundary) and drops
/// the trailing NUL. Safe to call from any thread, including the D-W worker.
pub fn emit(line: &str) {
    let owned: Vec<u8> = line.bytes().filter(|b| *b != 0).collect();
    let mut buf = owned;
    buf.push(0u8);
    unsafe {
        OH_LOG_Print(
            LOG_APP,
            LOG_INFO,
            DOMAIN,
            TAG.as_ptr(),
            FMT.as_ptr(),
            buf.as_ptr(),
        );
    }
}

/// Whether the frozen channel is loggable at INFO (diagnostic only).
pub fn loggable() -> bool {
    unsafe { OH_LOG_IsLoggable(DOMAIN, TAG.as_ptr(), LOG_INFO) }
}

// Host-test link surface (cfg(test) only, never part of the cdylib): the
// test binary links the whole crate on the host triple, where
// libhilog_ndk.z.so does not exist. These no-op definitions satisfy the
// linker; pure-function tests never assert on them.
#[cfg(test)]
pub mod host_stubs {
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
}
