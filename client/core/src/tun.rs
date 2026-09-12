//! TUN data plane over the frozen fd contract (docs/native-nx-governance.md
//! §二 条款 2, binding): the platform raw fd is closed EXCLUSIVELY by
//! `VpnConnection.destroy()`; native consumes ONLY a dup copy (F_DUPFD_CLOEXEC
//! preferred, dup() fallback); the raw fd must never reach BoringTun/tun.Device
//! (the boringtun `device` feature stays disabled — TunFd replaces TunSocket).
//!
//! The contract is executable here, not just documented:
//! - the raw fd is stored NOWHERE: its only use is the transient argument of
//!   `TunFd::dup_from_raw`, which re-verifies it is still open after the dup
//!   (a consuming dup would be a contract violation and fails closed);
//! - `close(2)` runs in exactly one place (`close_dup`), guarded by the
//!   `Option<i32>` inside TunFd and by the session table keeping the session
//!   as `None` after close: a second close is rejected with `AlreadyClosed`
//!   before any syscall, use-after-close is rejected with `Closed`;
//! - every dup create/close is a ledger transition (`Role::FdDup`); the
//!   observed platform fd is registered once as `Role::FdOrig` (create-only —
//!   its close belongs to destroy() and is never emitted by native);
//! - O_NONBLOCK is a property of the SHARED open-file description: setting it
//!   on our dup would silently flip the platform fd too, so TunFd never writes
//!   flags — it only reports the observed state (`nonblock_ofd`);
//! - no unbounded blocking wait exists: reads poll in <=50 ms slices and
//!   writes carry a total poll-wait budget, so shutdown/destroy/close is
//!   observed within one slice (EAGAIN / short writes / backpressure are
//!   handled explicitly, EOF surfaces as `TunError::Eof`).

use crate::hilog::emit;
use crate::sys;
use crate::util::{jbool, jinum, jnum, jstr};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

/// One TUN frame read buffer (>= typical TUN MTU 1420; same value as the
/// probe's BUF). A datagram longer than the buffer is truncated by read(2) —
/// keep the buffer >= MTU.
pub const READ_BUF: usize = 2048;
/// Largest frame accepted by the JSON write path (max IP packet).
pub const WRITE_MAX: usize = 65535;
/// poll() slice for every wait (read readiness, write readiness). The bound
/// IS the shutdown-unblock mechanism: no wait ever outlives one slice without
/// re-checking the caller's budget.
pub const POLL_SLICE_MS: u64 = 50;
/// Default total poll-wait budget for one full-frame write (backpressure
/// ceiling for the synchronous NAPI export).
pub const WRITE_MAX_WAIT_MS: u64 = 2000;

// ---------------------------------------------------------------------------
// errors — every contract violation has its own variant
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunError {
    /// Session (or TunFd) already closed — use after close.
    Closed,
    /// Explicit close attempted on an already-closed session — double close.
    AlreadyClosed,
    /// Unknown session id (never opened in this process).
    NoSuchSession,
    /// EAGAIN: fd is non-blocking and not ready.
    WouldBlock,
    /// Write budget exhausted mid-frame (backpressure); bytes already accepted.
    Backpressure { written: usize },
    /// read(2) == 0: peer end gone / device torn down (shutdown unblock).
    Eof,
    /// EBADF for OUR dup: foreign close or fd-number reuse detected.
    BadFd,
    /// Other errno from the kernel.
    Io(i32),
}

impl TunError {
    /// Stable name for the JSON error field.
    pub fn name(&self) -> &'static str {
        match self {
            TunError::Closed => "closed",
            TunError::AlreadyClosed => "already-closed",
            TunError::NoSuchSession => "no-session",
            TunError::WouldBlock => "wouldblock",
            TunError::Backpressure { .. } => "backpressure",
            TunError::Eof => "eof",
            TunError::BadFd => "badfd",
            TunError::Io(_) => "io",
        }
    }

    /// errno carried by the variant (0 = pure contract error, no syscall ran).
    pub fn errno(&self) -> i32 {
        match self {
            TunError::Closed
            | TunError::AlreadyClosed
            | TunError::NoSuchSession
            | TunError::Eof => 0,
            TunError::WouldBlock => sys::EAGAIN,
            TunError::Backpressure { .. } => sys::EAGAIN,
            TunError::BadFd => sys::EBADF,
            TunError::Io(e) => *e,
        }
    }
}

fn map_errno(e: i32) -> TunError {
    if e == sys::EAGAIN {
        TunError::WouldBlock
    } else if e == sys::EBADF {
        TunError::BadFd
    } else {
        TunError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// TunFd — owns exactly one dup copy
// ---------------------------------------------------------------------------

/// Owned dup copy of the platform TUN fd. The raw fd is never stored; Drop
/// closes only the dup (the raw fd keeps belonging to VpnConnection.destroy).
#[derive(Debug)]
pub struct TunFd {
    fd: Option<i32>,
    /// O_NONBLOCK observed on the shared open-file description at dup time
    /// (F_GETFL read-only). Informative only: flipping it here would flip the
    /// platform fd too, which is why TunFd never writes file flags.
    nonblock_ofd: bool,
}

impl TunFd {
    /// Dup `fd_raw` into an owned copy (F_DUPFD_CLOEXEC first, dup() fallback
    /// + F_SETFD FD_CLOEXEC). Executable raw-fd contract:
    /// 1. F_GETFD probe must succeed (raw fd open) — else the `BadFd`/
    ///    `Io(errno)` error (this is also what opening after destroy() yields);
    /// 2. after the dup, the raw fd must STILL be open — a dup that consumed
    ///    the caller's fd fails closed (and the fresh dup is closed again);
    /// 3. the copy carries FD_CLOEXEC;
    /// 4. the copy and the observed platform fd enter the ledger ownership
    ///    table (FdOrig once per process, create-only; FdDup create).
    pub fn dup_from_raw(fd_raw: i32) -> Result<TunFd, TunError> {
        if unsafe { sys::fcntl(fd_raw, sys::F_GETFD) } == -1 {
            return Err(map_errno(sys::errno()));
        }
        let mut via_dupfd = true;
        let mut fd_dup = unsafe { sys::fcntl(fd_raw, sys::F_DUPFD_CLOEXEC, 0) };
        if fd_dup == -1 {
            via_dupfd = false;
            fd_dup = unsafe { sys::dup(fd_raw) };
        }
        if fd_dup == -1 {
            return Err(map_errno(sys::errno()));
        }
        if unsafe { sys::fcntl(fd_raw, sys::F_GETFD) } == -1 {
            let e = sys::errno();
            unsafe { sys::close(fd_dup) };
            return Err(map_errno(e));
        }
        if !via_dupfd {
            unsafe { sys::fcntl(fd_dup, sys::F_SETFD, sys::FD_CLOEXEC) };
        }
        let fl = unsafe { sys::fcntl(fd_dup, sys::F_GETFL) };
        let nonblock_ofd = fl != -1 && (fl & sys::O_NONBLOCK) != 0;

        // ownership table: the platform fd is registered once (observed,
        // never to be closed by native); every dup is its own entry
        if !crate::ledger::is_created(crate::ledger::Role::FdOrig) {
            crate::ledger::emit_create(crate::ledger::Role::FdOrig, fd_raw);
        }
        crate::ledger::emit_create(crate::ledger::Role::FdDup, fd_dup);
        emit(&format!(
            "N1BDISC_TUN_OPEN|raw={fd_raw}|dup={fd_dup}|via={}|nonblock_ofd={nonblock_ofd}",
            if via_dupfd { "dupfd_cloexec" } else { "dup_setfd" }
        ));
        Ok(TunFd {
            fd: Some(fd_dup),
            nonblock_ofd,
        })
    }

    /// The dup fd number while open (`None` once closed).
    pub fn fd(&self) -> Option<i32> {
        self.fd
    }

    /// O_NONBLOCK state of the shared open-file description at dup time.
    pub fn nonblock_ofd(&self) -> bool {
        self.nonblock_ofd
    }

    /// Whether the dup is still open (not closed by `close`/`Drop`).
    pub fn is_open(&self) -> bool {
        self.fd.is_some()
    }

    fn check(&self) -> Result<i32, TunError> {
        self.fd.ok_or(TunError::Closed)
    }

    /// One read(2) on the dup. `WouldBlock` on EAGAIN (non-blocking, not
    /// ready), `Eof` on n == 0 (peer/shutdown), `BadFd` on EBADF (foreign
    /// close / number reuse), `Io(errno)` otherwise. EINTR retried bounded.
    pub fn read_frame(&self, buf: &mut [u8]) -> Result<usize, TunError> {
        let fd = self.check()?;
        if buf.is_empty() {
            return Ok(0);
        }
        let mut spins = 0u32;
        loop {
            let (n, e) = sys::read_fd(fd, buf);
            if n >= 0 {
                return if n == 0 {
                    Err(TunError::Eof)
                } else {
                    Ok(n as usize)
                };
            }
            if e == sys::EINTR && spins < 8 {
                spins += 1;
                continue;
            }
            return Err(map_errno(e));
        }
    }

    /// Bounded read: one read attempt, then poll(POLLIN) in <=`POLL_SLICE_MS`
    /// slices until `timeout_ms` is consumed. The slicing is what lets a
    /// concurrent shutdown/close unblock the pump — nothing here blocks
    /// longer than one slice without re-checking.
    pub fn read_frame_timeout(&self, buf: &mut [u8], timeout_ms: u64) -> Result<usize, TunError> {
        if timeout_ms == 0 {
            return self.read_frame(buf);
        }
        let fd = self.check()?;
        let mut waited: u64 = 0;
        loop {
            match self.read_frame(buf) {
                Err(TunError::WouldBlock) => {}
                other => return other,
            }
            if waited >= timeout_ms {
                return Err(TunError::WouldBlock);
            }
            let slice = POLL_SLICE_MS.min(timeout_ms - waited);
            let (ret, pe, rev) = sys::poll1(fd, sys::POLLIN, slice as i32);
            if (rev & sys::POLLNVAL) != 0 {
                return Err(TunError::BadFd);
            }
            if ret == -1 && pe != sys::EINTR {
                return Err(TunError::Io(pe));
            }
            waited += slice;
        }
    }

    /// Full-frame write with short-write loop, EINTR/EAGAIN handling and a
    /// total poll-wait budget of `max_wait_ms` (0 => fail on the first
    /// EAGAIN with the partial count). `Ok(n)` always means `n == frame.len()`.
    pub fn write_frame_budget(&self, frame: &[u8], max_wait_ms: u64) -> Result<usize, TunError> {
        let fd = self.check()?;
        if frame.is_empty() {
            return Ok(0);
        }
        let mut off = 0usize;
        let mut waited = 0u64;
        let mut spins = 0u32;
        loop {
            let (n, e) = sys::write_fd(fd, &frame[off..]);
            if n > 0 {
                off += n as usize;
                if off == frame.len() {
                    return Ok(off);
                }
                continue; // short write: push the remainder
            }
            if e == sys::EINTR && spins < 8 {
                spins += 1;
                continue;
            }
            // ONLY EAGAIN (or a 0-byte accept) may wait on POLLOUT within the
            // budget — every other error (EPIPE after peer close, EIO, EBADF)
            // must surface immediately instead of burning the budget.
            if e != sys::EAGAIN && n != 0 {
                return Err(map_errno(e));
            }
            if waited >= max_wait_ms {
                return Err(TunError::Backpressure { written: off });
            }
            let slice = POLL_SLICE_MS.min(max_wait_ms - waited);
            let (ret, pe, rev) = sys::poll1(fd, sys::POLLOUT, slice as i32);
            if (rev & sys::POLLNVAL) != 0 {
                return Err(TunError::BadFd);
            }
            if ret == -1 && pe != sys::EINTR {
                return Err(TunError::Io(pe));
            }
            waited += slice;
        }
    }

    /// Full-frame write with the default backpressure budget.
    pub fn write_frame(&self, frame: &[u8]) -> Result<usize, TunError> {
        self.write_frame_budget(frame, WRITE_MAX_WAIT_MS)
    }

    /// poll() readiness on the dup (revents), `BadFd` on POLLNVAL — the
    /// executable detection of a dup closed behind our back.
    pub fn poll_ready(&self, events: i16, timeout_ms: i32) -> Result<i16, TunError> {
        let fd = self.check()?;
        let (ret, e, revents) = sys::poll1(fd, events, timeout_ms);
        if (revents & sys::POLLNVAL) != 0 {
            return Err(TunError::BadFd);
        }
        if ret == -1 {
            return Err(TunError::Io(e));
        }
        Ok(revents)
    }

    /// Contract close: consumes the TunFd and closes the dup EXACTLY once.
    /// On an already-closed TunFd this is the double-close violation and
    /// returns `AlreadyClosed` before any syscall.
    pub fn close(mut self) -> Result<i32, TunError> {
        match self.fd.take() {
            Some(fd) => {
                close_dup(fd, "explicit");
                Ok(fd)
            }
            None => Err(TunError::AlreadyClosed),
        }
    }
}

impl Drop for TunFd {
    fn drop(&mut self) {
        if let Some(fd) = self.fd.take() {
            close_dup(fd, "drop");
        }
    }
}

/// The ONLY site that runs close(2) on a dup. Ledger close marker only for a
/// close that actually succeeded (ret == 0, probe convention); a failed close
/// (foreign close / number reuse) emits the errno so the capture shows the
/// mismatch instead of silently keeping a bogus open entry.
fn close_dup(fd: i32, via: &str) {
    let ret = unsafe { sys::close(fd) };
    if ret == 0 {
        crate::ledger::emit_close(
            crate::ledger::Role::FdDup,
            fd,
            crate::ledger::ClosedBy::ProbeProtocolClose,
        );
        emit(&format!("N1BDISC_TUN_CLOSE|dup={fd}|via={via}"));
    } else {
        emit(&format!(
            "N1BDISC_TUN_CLOSE|dup={fd}|via={via}|close_errno={}",
            sys::errno()
        ));
    }
}

// ---------------------------------------------------------------------------
// session table — the ArkTS-facing ownership registry (ids, not raw fds)
// ---------------------------------------------------------------------------

static SESSIONS: Mutex<Option<BTreeMap<u32, Option<TunFd>>>> = Mutex::new(None);
static NEXT_SESSION: AtomicU32 = AtomicU32::new(1);

fn with_sessions<R>(f: impl FnOnce(&mut BTreeMap<u32, Option<TunFd>>) -> R) -> R {
    let mut g = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        *g = Some(BTreeMap::new());
    }
    f(g.as_mut().unwrap())
}

/// Open a session over `fd_raw` (dup copy). Returns (session, fd_dup,
/// nonblock_ofd).
pub fn open_session(fd_raw: i32) -> Result<(u32, i32, bool), TunError> {
    let tun = TunFd::dup_from_raw(fd_raw)?;
    let fd = tun.fd().unwrap_or(-1);
    let nb = tun.nonblock_ofd();
    let id = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    with_sessions(|m| {
        m.insert(id, Some(tun));
    });
    Ok((id, fd, nb))
}

fn with_session<R>(id: u32, f: impl FnOnce(&TunFd) -> Result<R, TunError>) -> Result<R, TunError> {
    with_sessions(|m| match m.get(&id) {
        None => Err(TunError::NoSuchSession),
        Some(None) => Err(TunError::Closed),
        Some(Some(t)) => f(t),
    })
}

/// One read(2) on the session's dup (use after close => `Closed`).
pub fn session_read(id: u32, buf: &mut [u8]) -> Result<usize, TunError> {
    with_session(id, |t| t.read_frame(buf))
}

/// Bounded read on the session's dup.
pub fn session_read_timeout(id: u32, buf: &mut [u8], timeout_ms: u64) -> Result<usize, TunError> {
    with_session(id, |t| t.read_frame_timeout(buf, timeout_ms))
}

/// Full-frame write on the session's dup.
pub fn session_write(id: u32, frame: &[u8]) -> Result<usize, TunError> {
    with_session(id, |t| t.write_frame(frame))
}

/// poll(POLLIN) readiness on the session's dup (POLLNVAL => `BadFd`).
pub fn session_poll(id: u32, timeout_ms: i32) -> Result<i16, TunError> {
    with_session(id, |t| t.poll_ready(sys::POLLIN, timeout_ms))
}

/// Close the session's dup exactly once. Second close => `AlreadyClosed`
/// (no syscall, no new ledger transition); unknown id => `NoSuchSession`.
/// The closed session STAYS in the table as `None` so later use is reported
/// as `Closed` instead of "no session".
pub fn session_close(id: u32) -> Result<i32, TunError> {
    with_sessions(|m| match m.get_mut(&id) {
        None => Err(TunError::NoSuchSession),
        Some(slot) => match slot.take() {
            Some(t) => t.close(),
            None => Err(TunError::AlreadyClosed),
        },
    })
}

// ---------------------------------------------------------------------------
// JSON surface (called from napi.rs; every export stays synchronous)
// ---------------------------------------------------------------------------

fn err_json(e: &TunError) -> String {
    format!(
        "{{{},{},{}}}",
        jbool("ok", false),
        jstr("error", e.name()),
        jinum("errno", e.errno() as i64)
    )
}

/// `tun_open(fdOrig)` -> `{ok,session,fd,nonblock_ofd}` | `{ok:false,error,errno}`.
pub fn tun_open_json(fd_raw: i32) -> String {
    match open_session(fd_raw) {
        Ok((session, fd, nb)) => format!(
            "{{{},{},{},{}}}",
            jbool("ok", true),
            jnum("session", session as u64),
            jinum("fd", fd as i64),
            jbool("nonblock_ofd", nb)
        ),
        Err(e) => err_json(&e),
    }
}

/// `tun_read(session)` -> `{ok,n,hex}` | `{eagain:true}` | `{eof:true}` | error.
pub fn tun_read_json(session: i32) -> String {
    if session < 0 {
        return err_json(&TunError::NoSuchSession);
    }
    let mut buf = [0u8; READ_BUF];
    match session_read(session as u32, &mut buf) {
        Ok(n) => format!(
            "{{{},{},{}}}",
            jbool("ok", true),
            jnum("n", n as u64),
            jstr("hex", &crate::util::hex_lower(&buf[..n]))
        ),
        Err(TunError::WouldBlock) => "{\"eagain\":true}".to_string(),
        Err(TunError::Eof) => "{\"eof\":true}".to_string(),
        Err(e) => err_json(&e),
    }
}

/// `tun_write(session, hexFrame)` -> `{ok,n}` |
/// `{ok:false,error:"backpressure",written,errno}` | error.
pub fn tun_write_json(session: i32, hex_frame: &str) -> String {
    if session < 0 {
        return err_json(&TunError::NoSuchSession);
    }
    let frame = match hex_decode(hex_frame) {
        Some(f) => f,
        None => {
            return format!(
                "{{{},{},{}}}",
                jbool("ok", false),
                jstr("error", "bad-hex"),
                jinum("errno", 0)
            )
        }
    };
    if frame.len() > WRITE_MAX {
        return format!(
            "{{{},{},{},{}}}",
            jbool("ok", false),
            jstr("error", "too-long"),
            jnum("len", frame.len() as u64),
            jinum("errno", sys::EMSGSIZE as i64)
        );
    }
    match session_write(session as u32, &frame) {
        Ok(n) => format!("{{{},{}}}", jbool("ok", true), jnum("n", n as u64)),
        Err(TunError::Backpressure { written }) => format!(
            "{{{},{},{},{}}}",
            jbool("ok", false),
            jstr("error", "backpressure"),
            jnum("written", written as u64),
            jinum("errno", sys::EAGAIN as i64)
        ),
        Err(e) => err_json(&e),
    }
}

/// `tun_poll(session, timeoutMs)` -> `{ok,revents,in,hup,nval}` | error.
pub fn tun_poll_json(session: i32, timeout_ms: i32) -> String {
    if session < 0 {
        return err_json(&TunError::NoSuchSession);
    }
    match session_poll(session as u32, timeout_ms) {
        Ok(rev) => format!(
            "{{{},{},{},{},{}}}",
            jbool("ok", true),
            jinum("revents", rev as i64),
            jbool("in", (rev & sys::POLLIN) != 0),
            jbool("hup", (rev & sys::POLLHUP) != 0),
            jbool("nval", (rev & sys::POLLNVAL) != 0)
        ),
        Err(e) => err_json(&e),
    }
}

/// `tun_close(session)` -> `{ok,fd}` | `{ok:false,error:"already-closed"}` | error.
pub fn tun_close_json(session: i32) -> String {
    if session < 0 {
        return err_json(&TunError::NoSuchSession);
    }
    match session_close(session as u32) {
        Ok(fd) => format!("{{{},{}}}", jbool("ok", true), jinum("fd", fd as i64)),
        Err(e) => err_json(&e),
    }
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len() / 2);
    let mut i = 0;
    while i < b.len() {
        let hi = (b[i] as char).to_digit(16)?;
        let lo = (b[i + 1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
        i += 2;
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// The fd-contract test suite lives in tests/tun_fd_contract.rs (integration
// process): the ledger and the session table are process-global, and the
// ledger unit tests assert digest determinism across their emission points —
// a separate test PROCESS removes any interleaving between the two suites.
// ---------------------------------------------------------------------------
