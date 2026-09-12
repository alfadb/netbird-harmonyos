//! Raw libc surface of the probe — implements the frozen syscall closed table
//! (gate-plan :353-360). Every extern declared below is a member of the allowed
//! list with the allowed usage shape; nothing else may be added here.
//!
//! Deviation registered in lib.rs DEVIATIONS: `gettid` is required by the frozen
//! protocol itself (`DW_SPAWN|tid=<n>` marker and the two `/proc/self/task/<tid>/…`
//! openat paths) but is not spelled out in the :353-360 table; the OHOS musl
//! sysroot declares `pid_t gettid(void)` in <unistd.h>, so it is called as a plain
//! libc function.

#![allow(non_camel_case_types)]

use core::ffi::c_void;

pub type c_int = i32;
pub type c_char = u8; // aarch64-linux-ohos: `char` is unsigned

// ---------------------------------------------------------------------------
// closed-table externs
// ---------------------------------------------------------------------------

extern "C" {
    // dlopen family (D1 only; handle is kept — dlclose is NOT in the table)
    pub fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    pub fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    pub fn dlerror() -> *mut c_char;

    // fcntl: only F_GETFD / F_GETFL / F_SETFD / F_DUPFD_CLOEXEC, and F_SETFL on
    // the dup copy only (A2). dup(): fallback path only.
    pub fn fcntl(fd: c_int, cmd: c_int, ...) -> c_int;
    pub fn dup(fd: c_int) -> c_int;

    pub fn poll(fds: *mut pollfd, nfds: u64, timeout: c_int) -> c_int;
    pub fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
    pub fn write(fd: c_int, buf: *const c_void, count: usize) -> isize;
    pub fn close(fd: c_int) -> c_int;

    // sockets: only AF_INET / SOCK_DGRAM / 0
    pub fn socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int;
    pub fn bind(fd: c_int, addr: *const sockaddr_in, len: u32) -> c_int;
    pub fn sendto(
        fd: c_int,
        buf: *const c_void,
        len: usize,
        flags: c_int,
        addr: *const sockaddr_in,
        addrlen: u32,
    ) -> isize;
    pub fn recvfrom(
        fd: c_int,
        buf: *mut c_void,
        len: usize,
        flags: c_int,
        addr: *mut sockaddr_in,
        addrlen: *mut u32,
    ) -> isize;

    // clock: CLOCK_MONOTONIC only; clock_nanosleep only 10 ms / 50 ms (A4)
    pub fn clock_gettime(clk_id: c_int, tp: *mut timespec) -> c_int;
    pub fn clock_nanosleep(
        clk_id: c_int,
        flags: c_int,
        req: *const timespec,
        rem: *mut timespec,
    ) -> c_int;

    // threads: pthread_create exactly once (D-W worker, A1); pthread_join only
    // after the worker-terminal atomic flag is set (only unbounded exemption).
    pub fn pthread_create(
        thread: *mut u64,
        attr: *const c_void,
        start_routine: extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
    ) -> c_int;
    pub fn pthread_join(thread: u64, retval: *mut *mut c_void) -> c_int;

    // openat: O_RDONLY only, whitelisted paths only (A6):
    //   /proc/self/task/<tid>/stat and /proc/self/task/<tid>/syscall
    pub fn openat(dirfd: c_int, path: *const c_char, flags: c_int, ...) -> c_int;

    // tid for DW_SPAWN and the /proc paths (see module doc deviation note)
    pub fn gettid() -> i32;

    // musl errno accessor
    fn __errno_location() -> *mut c_int;
}

// ---------------------------------------------------------------------------
// structs
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
pub struct timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct pollfd {
    pub fd: c_int,
    pub events: i16,
    pub revents: i16,
}

/// sockaddr_in built bytewise so no endianness guessing is needed:
/// sin_port = u16::from_be(port), sin_addr = [a, b, c, d].
#[repr(C)]
#[derive(Clone, Copy)]
pub struct sockaddr_in {
    pub sin_family: u16,
    pub sin_port: u16,
    pub sin_addr: [u8; 4],
    pub sin_zero: [u8; 8],
}

impl sockaddr_in {
    pub fn new(addr: [u8; 4], port: u16) -> Self {
        sockaddr_in {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: addr,
            sin_zero: [0u8; 8],
        }
    }
}

// ---------------------------------------------------------------------------
// constants (values per target sysroot headers)
// ---------------------------------------------------------------------------

pub const CLOCK_MONOTONIC: c_int = 1;
pub const AF_INET: c_int = 2;
pub const SOCK_DGRAM: c_int = 2;
pub const POLLIN: i16 = 0x001;
pub const POLLOUT: i16 = 0x004;
pub const POLLERR: i16 = 0x008;
pub const POLLHUP: i16 = 0x010;
pub const POLLNVAL: i16 = 0x020;
pub const F_GETFD: c_int = 1;
pub const F_SETFD: c_int = 2;
pub const F_GETFL: c_int = 3;
pub const F_SETFL: c_int = 4;
pub const F_DUPFD_CLOEXEC: c_int = 1030; // asm-generic fcntl.h
pub const FD_CLOEXEC: c_int = 1;
pub const O_NONBLOCK: c_int = 0o4000;
pub const O_RDONLY: c_int = 0;
pub const AT_FDCWD: c_int = -100;
pub const RTLD_NOW: c_int = 2;
pub const RTLD_LOCAL: c_int = 0;
// errno values (linux aarch64)
pub const EAGAIN: c_int = 11;
pub const EBADF: c_int = 9;
pub const EINTR: c_int = 4;
pub const EMSGSIZE: c_int = 90;
pub const ESRCH: c_int = 3;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Current errno (musl TLS).
pub fn errno() -> c_int {
    unsafe { *__errno_location() }
}

/// CLOCK_MONOTONIC reading in the millisecond domain (gate-plan "毫域").
/// Monotonic readings are non-negative; saturate defensively at 0.
pub fn mono_ms() -> u64 {
    let mut ts = timespec { tv_sec: 0, tv_nsec: 0 };
    let r = unsafe { clock_gettime(CLOCK_MONOTONIC, &mut ts) };
    debug_assert_eq!(r, 0);
    let ms = ts.tv_sec as i64 * 1000 + (ts.tv_nsec as i64) / 1_000_000;
    if ms < 0 {
        0
    } else {
        ms as u64
    }
}

/// clock_nanosleep(CLOCK_MONOTONIC) — the ONLY sleep primitive (A4).
/// Two frozen durations: 10 ms (bounded poll waits) and 50 ms (D7 outer-block
/// clock gating). This function takes whole milliseconds and is only called
/// with those two values.
pub fn sleep_ms(ms: u64) {
    let req = timespec {
        tv_sec: (ms / 1000) as i64,
        tv_nsec: ((ms % 1000) * 1_000_000) as i64,
    };
    unsafe {
        // EINTR is not expected for relative sleeps against absolute deadlines
        // (all waits re-check their own monotonic deadline each tick), ignore.
        clock_nanosleep(CLOCK_MONOTONIC, 0, &req, core::ptr::null_mut());
    }
}

/// poll() on a single fd; returns (ret, errno, revents).
pub fn poll1(fd: c_int, events: i16, timeout_ms: c_int) -> (c_int, c_int, i16) {
    let mut fds = pollfd {
        fd,
        events,
        revents: 0,
    };
    let ret = unsafe { poll(&mut fds as *mut pollfd, 1, timeout_ms) };
    let e = if ret == -1 { errno() } else { 0 };
    (ret, e, fds.revents)
}

/// read() into buf; returns (n, errno).
pub fn read_fd(fd: c_int, buf: &mut [u8]) -> (isize, c_int) {
    let n = unsafe { read(fd, buf.as_mut_ptr() as *mut c_void, buf.len()) };
    let e = if n == -1 { errno() } else { 0 };
    (n, e)
}

/// write() from buf; returns (n, errno).
pub fn write_fd(fd: c_int, buf: &[u8]) -> (isize, c_int) {
    let n = unsafe { write(fd, buf.as_ptr() as *const c_void, buf.len()) };
    let e = if n == -1 { errno() } else { 0 };
    (n, e)
}
