//! D-W: the one registered waiter (gate-plan :671-952), mainline waiting APIs,
//! the shared destroy-subprotocol markers, PRE/POST emission and the P12
//! `dw_return_class` derivation (:722-806).
//!
//! Worker sequence (frozen, :674-704 + r18/r20/r21 ordering):
//!   emit `DW_SPAWN|tid=` -> drain loop (5 s box, C6 five transition classes)
//!   -> write drain end into the snapshot -> emit `DW_DRAIN` -> emit `DW_BARRIER`
//!   + set barrier flag (immediately before poll) -> poll(fd_dup, POLLIN, 5000)
//!   -> SINGLE clock read (at_mono_ms = end; elapsed_ms = end − poll start;
//!   ret/errno/revents each read once) -> write the five raw into the snapshot
//!   -> set `dw_snapshot_written` (seq_cst) -> emit `DW_RETURN` (raw only, no
//!   class field — r15) -> emit `DW_EXIT` -> set `dw_worker_terminal` (seq_cst)
//!   -> thread returns.
//!
//! Exactly ONE pthread_create call site in the whole crate (A1). join() is the
//! only unbounded wait (A4 exemption) and is only legal after the terminal flag
//! is set. All other waits = atomic-flag polling + 10 ms clock_nanosleep with a
//! monotonic deadline.

use crate::hilog::emit;
use crate::ledger::{self, ClosedBy, Role};
use crate::state::{
    drain_end_literal, with_state, D6StepResult, DW_BARRIER, DW_FD, DW_HANDLE,
    DW_SNAPSHOT_WRITTEN, DW_TERMINAL, DW_TID, DRAIN_END_EAGAIN, SNAP_AT_MONO_MS, SNAP_DRAIN_END,
    SNAP_DRAIN_ERRNO, SNAP_ELAPSED_MS, SNAP_ERRNO, SNAP_REVENTS, SNAP_RET,
};
use crate::sys;
use crate::util::{jbool, jinum, jnum, jstr, sanitize_marker_field};
use core::ffi::c_void;
use std::sync::atomic::Ordering;

// ---------------------------------------------------------------------------
// worker thread — the ONLY pthread_create target in the crate
// ---------------------------------------------------------------------------

extern "C" fn dw_worker(_arg: *mut c_void) -> *mut c_void {
    let tid = unsafe { sys::gettid() };
    DW_TID.store(tid, Ordering::SeqCst);
    emit(&format!("N1BDISC_DW_SPAWN|tid={}", tid));

    let fd = DW_FD.load(Ordering::SeqCst);

    // --- drain: non-blocking read loop, 5 s monotonic box, C6 transitions ---
    let drain_start = sys::mono_ms();
    let drain_deadline = drain_start + 5_000;
    let mut reads: u64 = 0;
    let mut bytes: u64 = 0;
    let mut eintr_retries: u64 = 0;
    let mut timeout = false;
    let drain_end: i64;
    let mut drain_errno: i32 = 0;
    let mut rbuf = [0u8; 4096];

    loop {
        let (n, e) = sys::read_fd(fd, &mut rbuf);
        if n > 0 {
            reads += 1;
            bytes += n as u64;
        } else if n == 0 {
            // zero read on a non-blocking fd: terminate drain (separate fact
            // from EAGAIN normal termination)
            drain_end = crate::state::DRAIN_END_ZERO_READ;
            break;
        } else if e == sys::EINTR {
            eintr_retries += 1;
        } else if e == sys::EAGAIN {
            drain_end = DRAIN_END_EAGAIN;
            break;
        } else {
            drain_end = crate::state::DRAIN_END_ERRNO;
            drain_errno = e;
            break;
        }
        if sys::mono_ms() >= drain_deadline {
            timeout = true;
            drain_end = crate::state::DRAIN_END_BOX_EXPIRY;
            break;
        }
    }
    let drain_elapsed = sys::mono_ms().saturating_sub(drain_start);
    // dw_drain_end single column: written once, before the DRAIN marker
    SNAP_DRAIN_END.store(drain_end, Ordering::SeqCst);
    SNAP_DRAIN_ERRNO.store(drain_errno, Ordering::SeqCst);
    crate::state::SNAP_DRAIN_EINTR.store(eintr_retries as i64, Ordering::SeqCst);
    emit(&format!(
        "N1BDISC_DW_DRAIN|reads={}|bytes={}|elapsed_ms={}|timeout={}|end={}",
        reads,
        bytes,
        drain_elapsed,
        timeout,
        drain_end_literal(drain_end, drain_errno)
    ));

    // --- barrier: marker + flag immediately before the poll call ---
    emit("N1BDISC_DW_BARRIER");
    DW_BARRIER.store(true, Ordering::SeqCst);

    // --- the observed wait: poll(fd_dup, POLLIN, 5000) ---
    let poll_start = sys::mono_ms();
    let (ret, perrno, revents) = sys::poll1(fd, sys::POLLIN, 5000);
    // single clock read after poll returns; at and elapsed share this source
    let end_clock = sys::mono_ms();
    let at_mono_ms = end_clock;
    let elapsed_ms = end_clock.saturating_sub(poll_start);

    // snapshot write (five raw, single-source locals) + publish
    SNAP_RET.store(ret as i64, Ordering::SeqCst);
    SNAP_ERRNO.store(perrno, Ordering::SeqCst);
    SNAP_REVENTS.store(revents as i64, Ordering::SeqCst);
    SNAP_AT_MONO_MS.store(at_mono_ms as i64, Ordering::SeqCst);
    SNAP_ELAPSED_MS.store(elapsed_ms as i64, Ordering::SeqCst);
    DW_SNAPSHOT_WRITTEN.store(true, Ordering::SeqCst);

    emit(&format!(
        "N1BDISC_DW_RETURN|elapsed_ms={}|ret={}|errno={}|revents={}|at_mono_ms={}",
        elapsed_ms, ret, perrno, revents, at_mono_ms
    ));
    emit("N1BDISC_DW_EXIT");
    DW_TERMINAL.store(true, Ordering::SeqCst);
    core::ptr::null_mut()
}

// ---------------------------------------------------------------------------
// mainline APIs
// ---------------------------------------------------------------------------

pub fn dw_start(fd_dup: i32) -> String {
    crate::state::dw_reset();
    DW_FD.store(fd_dup, Ordering::SeqCst);
    with_state(|s| {
        s.dw_spawned = true;
        s.fd_dup = Some(fd_dup);
    });

    let mut handle: u64 = 0;
    // THE single pthread_create call site (A1)
    let rc = unsafe {
        sys::pthread_create(
            &mut handle as *mut u64,
            core::ptr::null(),
            dw_worker,
            core::ptr::null_mut(),
        )
    };
    if rc != 0 {
        with_state(|s| s.dw_spawned = false);
        return format!(
            "{{{},{}}}",
            jstr("spawned", "false"),
            jstr("error", &format!("pthread_create rc={}", rc))
        );
    }
    DW_HANDLE.store(handle, Ordering::SeqCst);
    // worker tid is published via the DW_SPAWN marker (capture-side fact)
    format!("{{{}}}", jstr("spawned", "true"))
}

/// Bounded barrier wait: <=7 s (drain box 5 s + 2 s scheduling margin), 10 ms
/// poll interval. Timeout -> `dw_entry_confirmed=unobservable(cause=barrier-
/// timeout)` (JSON only; the runner derives the field) and in-wait collection
/// must be skipped by the caller.
pub fn dw_wait_barrier() -> String {
    let t0 = sys::mono_ms();
    let deadline = t0 + 7_000;
    let mut confirmed = false;
    while sys::mono_ms() < deadline {
        if DW_BARRIER.load(Ordering::SeqCst) {
            confirmed = true;
            break;
        }
        sys::sleep_ms(10);
    }
    with_state(|s| s.barrier_confirmed = Some(confirmed));
    format!(
        "{{{},{},{}}}",
        jstr("dw_entry_confirmed", if confirmed { "observed-true" } else { "unobservable(cause=barrier-timeout)" }),
        jbool("confirmed", confirmed),
        jnum("elapsed_ms", sys::mono_ms().saturating_sub(t0))
    )
}

/// Barrier deferral wait (:715): the 7 s barrier box expired without the
/// marker, so destroy is deferred and the main thread keeps the same 10 ms
/// bounded poll of the barrier flag until the worker-terminal box window
/// closes — budget <= 8 s counted from the barrier-box expiry (the caller
/// invokes this immediately after `dw_wait_barrier` returns at that expiry).
/// Marker seen -> barrier confirmed (destroy may run; in-wait collection
/// stays gated on the confirmed flag); still unseen -> destroy stays deferred
/// (the pre-registered SKIP site `SKIP|item=destroy|cause=barrier-never-
/// observed` is the caller's, r9 fifth step).
///
/// P8 budget note (:1059): on this branch the deferral (7 s box + <= 8 s)
/// replaces in-wait collection (2 s) + P9 destroy (10 s), so the serial
/// upper bound the 525 s observation window was derived from still covers
/// the deferred path.
pub fn dw_wait_barrier_defer() -> String {
    let t0 = sys::mono_ms();
    let deadline = t0 + 8_000;
    let mut confirmed = false;
    while sys::mono_ms() < deadline {
        if DW_BARRIER.load(Ordering::SeqCst) {
            confirmed = true;
            break;
        }
        sys::sleep_ms(10);
    }
    if confirmed {
        // destroy no longer deferred; in-wait collection gates on this flag
        with_state(|s| s.barrier_confirmed = Some(true));
    }
    format!(
        "{{{},{},{}}}",
        jstr(
            "dw_entry_confirmed",
            if confirmed {
                "observed-true"
            } else {
                "unobservable(cause=barrier-never-observed)"
            }
        ),
        jbool("confirmed", confirmed),
        jnum("elapsed_ms", sys::mono_ms().saturating_sub(t0))
    )
}

/// in-wait evidence collection (:833-857): <=2 s window, 10 ms interval, reads
/// /proc/self/task/<tid>/stat (state = first token AFTER the last ')') and
/// /proc/self/task/<tid>/syscall (first token = syscall number; poll family
/// frozen set {73}). Satisfying sample stops the window. Each open is a
/// transient ledger fd (openat -> read -> close, role dw_inwait_proc_fd).
pub fn dw_inwait_collect() -> String {
    let tid = DW_TID.load(Ordering::SeqCst);
    if tid == 0 {
        return format!(
            "{{{}}}",
            jstr("dw_inwait_confirmed", "unobservable(cause=barrier-timeout)")
        );
    }
    let barrier_ok = with_state(|s| s.barrier_confirmed == Some(true));
    if !barrier_ok {
        return format!(
            "{{{}}}",
            jstr("dw_inwait_confirmed", "unobservable(cause=barrier-timeout)")
        );
    }

    let t0 = sys::mono_ms();
    let deadline = t0 + 2_000;
    let mut samples: u64 = 0;
    let mut stat_ever_readable = false;
    let mut syscall_ever_readable = false;
    let mut confirmed = false;
    let mut last_errno: i32 = 0;

    let stat_path = format!("/proc/self/task/{}/stat", tid);
    let sysc_path = format!("/proc/self/task/{}/syscall", tid);

    loop {
        let (stat_state, stat_ok, stat_errno) = read_proc_stat(&stat_path);
        if stat_ok {
            stat_ever_readable = true;
        } else if stat_errno != 0 {
            last_errno = stat_errno;
        }
        let (sysc_no, sysc_ok, sysc_errno) = read_proc_syscall(&sysc_path);
        if sysc_ok {
            syscall_ever_readable = true;
        } else if sysc_errno != 0 {
            last_errno = sysc_errno;
        }
        samples += 1;

        // three conditions on ONE sample: state='S' AND syscall readable AND
        // syscall number in the frozen poll set {73} (r16 tightened reading)
        if let (Some(st), Some(no)) = (stat_state, sysc_no) {
            if st == "S" && no == 73 {
                confirmed = true;
                break;
            }
        }

        if sys::mono_ms() >= deadline {
            break;
        }
        sys::sleep_ms(10);
    }

    let src = match (stat_ever_readable, syscall_ever_readable) {
        (true, true) => "both",
        (true, false) => "proc-stat",
        (false, true) => "proc-syscall",
        (false, false) => "unreadable",
    };
    let conf = if confirmed {
        "observed-true"
    } else if !stat_ever_readable {
        // state='S' cannot be established (gate-plan :841)
        "unobservable(cause=proc-introspection-unavailable)"
    } else {
        "observed-false"
    };

    emit(&format!(
        "N1BDISC_DW_INWAIT|src={}|conf={}|samples={}|errno={}",
        src, conf, samples, last_errno
    ));
    with_state(|s| {
        s.inwait = Some(crate::state::InwaitResult {
            src: src.to_string(),
            conf: conf.to_string(),
            samples,
            errno: last_errno,
        })
    });

    format!(
        "{{{},{},{},{},{}}}",
        jstr("dw_inwait_evidence_source", src),
        jstr("dw_inwait_confirmed", conf),
        jnum("dw_inwait_samples", samples),
        jinum("errno", last_errno as i64),
        jnum("elapsed_ms", sys::mono_ms().saturating_sub(t0))
    )
}

/// Read /proc/<...>/stat: returns (state token, readable, errno).
/// State parsing per A7: the token AFTER the last ')' in the line.
fn read_proc_stat(path: &str) -> (Option<String>, bool, i32) {
    let mut cpath = path.as_bytes().to_vec();
    cpath.push(0);
    let fd = unsafe { sys::openat(sys::AT_FDCWD, cpath.as_ptr(), sys::O_RDONLY) };
    if fd < 0 {
        return (None, false, sys::errno());
    }
    ledger::emit_create(Role::DwInwaitProcFd, fd);
    let mut buf = [0u8; 4096];
    let mut text = String::new();
    for _ in 0..4 {
        let (n, _) = sys::read_fd(fd, &mut buf);
        if n <= 0 {
            break;
        }
        text.push_str(&String::from_utf8_lossy(&buf[..n as usize]));
        if text.len() > 8192 {
            break;
        }
    }
    let closed = unsafe { sys::close(fd) };
    if closed == 0 {
        ledger::emit_close(Role::DwInwaitProcFd, fd, ClosedBy::ProbeProtocolClose);
    }

    // A7: locate the LAST ')' then take the next whitespace-delimited token
    let state = parse_proc_stat_state(&text);
    let has = state.is_some();
    (state, has, 0)
}

/// A7 pure half: the state token is the whitespace-delimited token AFTER the
/// LAST ')' of a /proc stat line (`comm` may contain spaces and parentheses —
/// any left-side splitting misaligns the field). None when the line has no ')'.
fn parse_proc_stat_state(text: &str) -> Option<String> {
    text.rfind(')')
        .and_then(|pos| text[pos + 1..].split_whitespace().next().map(|s| s.to_string()))
}

/// Read /proc/<...>/syscall: returns (syscall number, readable, errno).
/// First field is the syscall number; "running" (not blocked) is unreadable
/// for confirmation purposes but readable as a file.
fn read_proc_syscall(path: &str) -> (Option<i64>, bool, i32) {
    let mut cpath = path.as_bytes().to_vec();
    cpath.push(0);
    let fd = unsafe { sys::openat(sys::AT_FDCWD, cpath.as_ptr(), sys::O_RDONLY) };
    if fd < 0 {
        return (None, false, sys::errno());
    }
    ledger::emit_create(Role::DwInwaitProcFd, fd);
    let mut buf = [0u8; 512];
    let mut text = String::new();
    for _ in 0..4 {
        let (n, _) = sys::read_fd(fd, &mut buf);
        if n <= 0 {
            break;
        }
        text.push_str(&String::from_utf8_lossy(&buf[..n as usize]));
        if text.len() > 2048 {
            break;
        }
    }
    let closed = unsafe { sys::close(fd) };
    if closed == 0 {
        ledger::emit_close(Role::DwInwaitProcFd, fd, ClosedBy::ProbeProtocolClose);
    }

    let first = text.split_whitespace().next();
    let no = first.and_then(|t| t.parse::<i64>().ok());
    (no, true, 0)
}

// ---------------------------------------------------------------------------
// shared destroy sub-protocol markers (A9: _T -> destroy() -> _C on the same
// ArkTS-driven control flow; native emits the two anchors around the call)
// ---------------------------------------------------------------------------

pub fn dw_destroy_t() -> String {
    let mono = sys::mono_ms();
    with_state(|s| s.destroy_t_mono = Some(mono));
    emit(&format!("N1BDISC_DW_DESTROY_T|mono_ms={}", mono));
    format!("{{{}}}", jnum("mono_ms", mono))
}

pub fn dw_destroy_c() -> String {
    let mono = sys::mono_ms();
    with_state(|s| s.destroy_c_mono = Some(mono));
    emit(&format!("N1BDISC_DW_DESTROY_C|mono_ms={}", mono));
    format!("{{{}}}", jnum("mono_ms", mono))
}

// ---------------------------------------------------------------------------
// terminal wait + join
// ---------------------------------------------------------------------------

/// P10 terminal-poll: <=8 s, 10 ms interval. Box expiry with the flag unset
/// registers `join-timeout` (worker abandoned; pthread_join is NOT called).
pub fn dw_wait_terminal() -> String {
    let t0 = sys::mono_ms();
    let deadline = t0 + 8_000;
    let mut terminal = false;
    while sys::mono_ms() < deadline {
        if DW_TERMINAL.load(Ordering::SeqCst) {
            terminal = true;
            break;
        }
        sys::sleep_ms(10);
    }
    if !terminal {
        with_state(|s| s.jt_registered = true);
    }
    format!(
        "{{{},{},{}}}",
        jbool("terminal", terminal),
        jstr("dw_join_result", if terminal { "pending-join" } else { "join-timeout" }),
        jnum("elapsed_ms", sys::mono_ms().saturating_sub(t0))
    )
}

/// Blocking pthread_join — ONLY legal after the terminal flag is set; the sole
/// unbounded wait exemption (A4). If the flag is not set, refuses to join
/// (join-timeout semantics already registered) and returns without blocking.
pub fn dw_join() -> String {
    let spawned = with_state(|s| s.dw_spawned);
    if !spawned {
        return format!("{{{}}}", jstr("error", "worker not spawned"));
    }
    if !DW_TERMINAL.load(Ordering::SeqCst) {
        return format!(
            "{{{}}}",
            jstr("error", "terminal flag not set; join refused (join-timeout)")
        );
    }
    let handle = DW_HANDLE.load(Ordering::SeqCst);
    let rc = unsafe { sys::pthread_join(handle, core::ptr::null_mut()) };
    let result = if rc == 0 {
        "joined".to_string()
    } else if rc == sys::ESRCH {
        "ESRCH".to_string()
    } else {
        format!("other+{}", rc)
    };
    with_state(|s| s.join_result = Some(result.clone()));
    format!(
        "{{{},{}}}",
        jstr("dw_join_result", &result),
        jinum("rc", rc as i64)
    )
}

// ---------------------------------------------------------------------------
// P5T PRE marker
// ---------------------------------------------------------------------------

pub fn pre_emit(skip_summary: &str) -> String {
    let digest = ledger::digest(true); // P5T cut: open entries -> open-at-pre
    let ss = if skip_summary.trim().is_empty() {
        "none".to_string()
    } else {
        sanitize_marker_field(skip_summary, 512)
    };
    emit(&format!(
        "N1BDISC_PRE|ledger_digest={}|skip_summary={}",
        digest, ss
    ));
    format!(
        "{{{},{},{}}}",
        jstr("ledger_digest", &digest),
        jstr("skip_summary", &ss),
        jstr("site", "P5T")
    )
}

// ---------------------------------------------------------------------------
// P12: dw_return_class derivation + POST marker
// ---------------------------------------------------------------------------

struct ClassRaw {
    ret: i64,
    errno: i32,
    revents: i64,
    at: u64,
    elapsed: u64,
    drain_end: i64,
}

fn snapshot_raw() -> ClassRaw {
    ClassRaw {
        ret: SNAP_RET.load(Ordering::SeqCst),
        errno: SNAP_ERRNO.load(Ordering::SeqCst),
        revents: SNAP_REVENTS.load(Ordering::SeqCst),
        at: SNAP_AT_MONO_MS.load(Ordering::SeqCst).max(0) as u64,
        elapsed: SNAP_ELAPSED_MS.load(Ordering::SeqCst).max(0) as u64,
        drain_end: SNAP_DRAIN_END.load(Ordering::SeqCst),
    }
}

/// The 13-class judgment table behind the flag gate (:722-806). Derivation
/// order frozen at :726 (r15, sol B-01): legal-domain gate -> 0/0b preclear ->
/// unknown-bit gate -> rows 1-11 — the same order as the runner rebuild
/// (runner/n1bdisc_core.py `derive_dw_return_class`). A legal-domain
/// contradiction produces NO class (empty string, M-02): verdict `fail` is
/// the runner's :724-725 face and the raw is already carried verbatim by
/// `DW_RETURN` — never an out-of-domain class literal.
fn derive_class_13(raw: &ClassRaw, destroy_skip: bool, c_mono: Option<u64>, t_mono: Option<u64>) -> String {
    // step 1: legal domain gate (E1, :724-725) — single-fd poll combos.
    // Contradiction => no class (B-02: this gate precedes 0/0b).
    let revents_i = raw.revents;
    if raw.ret != -1 && raw.ret != 0 && raw.ret != 1 {
        return String::new();
    }
    if raw.ret == 0 && revents_i != 0 {
        return String::new();
    }
    if raw.ret == 1 && revents_i == 0 {
        return String::new();
    }
    if raw.ret == -1 && raw.errno == 0 {
        return String::new();
    }

    // step 2: 0/0b preclear (:730) — SKIP|item=destroy => class 0 (r17: no
    // RETURN required); otherwise _C missing => class 0b.
    if destroy_skip {
        return "destroy-skip-proven".to_string();
    }
    let c = match c_mono {
        Some(v) => v,
        None => return "destroy-call-unobserved".to_string(), // class 0b
    };

    // step 3: unknown-bit gate (:726-729) — reached only with no SKIP and _C
    // present (r15/r16); ret >= 0, known-mask 0x3F.
    if raw.ret >= 0 && (revents_i < 0 || (revents_i & !0x3f) != 0) {
        return "other-revents".to_string();
    }

    let has_in = revents_i & 0x001 != 0;
    let has_err = revents_i & 0x008 != 0;
    let has_hup = revents_i & 0x010 != 0;
    let has_nval = revents_i & 0x020 != 0;

    // row 1-2: ret == -1 (revents meaningless)
    if raw.ret == -1 {
        if raw.errno == sys::EINTR {
            return "interrupted".to_string();
        }
        return "poll-error".to_string();
    }
    // row 3: POLLNVAL
    if has_nval {
        return "fd-invalid".to_string();
    }
    let at_pre_t = t_mono.map(|t| raw.at < t).unwrap_or(false);
    let at_post_c = raw.at > c;
    // row 4
    if revents_i != 0 && at_pre_t {
        return "pre-destroy-ready".to_string();
    }
    // row 5
    if (has_hup || has_err) && raw.elapsed >= 4500 && at_post_c {
        return "late-fd-event".to_string();
    }
    // row 6
    if has_in && raw.elapsed >= 4500 {
        return "late-data".to_string();
    }
    // row 7 (the only destroy-attributable class)
    if (has_hup || has_err) && raw.elapsed < 4500 && at_post_c && raw.drain_end == DRAIN_END_EAGAIN
    {
        return "fd-event-like".to_string();
    }
    // row 8
    if has_in && !has_hup && !has_err && raw.elapsed < 4500 && at_post_c
        && raw.drain_end == DRAIN_END_EAGAIN
    {
        return "data-ready-post-destroy".to_string();
    }
    // row 9
    if revents_i == 0 && raw.elapsed >= 4500 {
        return "timeout-like".to_string();
    }
    // row 10
    if revents_i == 0 && raw.elapsed < 4500 {
        return "spurious-early".to_string();
    }
    // row 11
    "other-revents".to_string()
}

/// Four-step ordered `dw_destroy_distinguishable_from_timeout` (:896-920).
fn derive_distinguishable(
    class: &str,
    destroy_skip_cause: Option<&str>,
    c_present: bool,
    resolved: bool,
) -> String {
    // (0) transparent passthrough for skip / death-closeout / poll-never /
    // flag-race encodings
    if class.starts_with("unobservable(cause=") {
        return class.to_string();
    }
    // M-02: a contradictory raw produces NO class (derive_class_13 returned
    // the empty string) — no derived value here either; the runner's F8(2)
    // comparison carries the fail, the raw is already on DW_RETURN.
    if class.is_empty() {
        return String::new();
    }
    // (1) class 0 / 0b
    if class == "destroy-skip-proven" {
        let cause = destroy_skip_cause.unwrap_or("no-live-connection");
        return format!("unobservable(cause={})", cause);
    }
    if class == "destroy-call-unobserved" {
        return "unobservable(cause=destroy-call-unobserved)".to_string();
    }
    // (2) _C present and no resolve evidence
    if c_present && !resolved {
        return "unobservable(cause=destroy-unresolved)".to_string();
    }
    // (3) destroy resolved: per-class values
    match class {
        "fd-event-like" => "observed-true".to_string(),
        "timeout-like" => "observed-false".to_string(),
        "pre-destroy-ready" | "interrupted" | "poll-error" | "fd-invalid" | "spurious-early" => {
            "unobservable(cause=no-destroy-correlated-event)".to_string()
        }
        "late-fd-event" | "late-data" | "data-ready-post-destroy" | "other-revents" => {
            "unobservable(cause=destroy-uncorrelated-class)".to_string()
        }
        // unreachable: derive_class_13 emits only the enumerated values above
        // (the retired out-of-domain literal is never written — M-02); an
        // unexpected residue yields no derived value.
        _ => String::new(),
    }
}

fn d6_items_string() -> String {
    let (d6a_ran, d6b_ran, steps, destroy_skip, jt, dw_skip): (bool, bool, Vec<D6StepResult>, Option<String>, bool, Option<String>) =
        with_state(|s| {
            (
                s.d6.d6a_ran,
                s.d6.d6b_ran,
                s.d6.steps.clone(),
                s.destroy_skip_cause.clone(),
                s.jt_registered,
                s.dw_skip_cause.clone(),
            )
        });

    let mut parts: Vec<String> = Vec::new();

    let d6a_skip: Option<String> = if d6a_ran {
        None
    } else if destroy_skip.as_deref() == Some("no-live-connection") {
        Some("skipped(cause=no-live-fd)".to_string())
    } else if destroy_skip.as_deref() == Some("barrier-never-observed") {
        Some("skipped(cause=barrier-never-observed)".to_string())
    } else {
        Some("skipped(cause=destroy-unresolved)".to_string())
    };

    for i in 0..3 {
        let tag = format!("D6S{}", i + 1);
        if d6a_ran && steps.len() > i {
            parts.push(format!(
                "{}=result|ret={}|errno={}",
                tag, steps[i].ret, steps[i].errno
            ));
        } else if let Some(c) = &d6a_skip {
            parts.push(format!("{}={}", tag, c));
        } else {
            parts.push(format!("{}=unobservable(cause=step-not-executed)", tag));
        }
    }

    let d6b_skip: Option<String> = if d6b_ran {
        None
    } else if jt {
        Some("skipped(cause=join-timeout-abandoned)".to_string())
    } else if dw_skip.as_deref() == Some("no-live-fd") {
        Some("skipped(cause=no-live-fd)".to_string())
    } else if dw_skip.as_deref() == Some("dup-failed") {
        Some("skipped(cause=dup-failed)".to_string())
    } else if destroy_skip.as_deref() == Some("barrier-never-observed") {
        Some("skipped(cause=barrier-never-observed)".to_string())
    } else {
        Some("skipped(cause=step-not-executed)".to_string())
    };

    for i in 3..7 {
        let tag = format!("D6S{}", i + 1);
        if d6b_ran && steps.len() > i {
            if i == 6 {
                let st = &steps[i];
                parts.push(format!(
                    "{}=result|fd={}|reuse={}",
                    tag,
                    st.fd.map(|f| f.to_string()).unwrap_or_else(|| "-1".into()),
                    st.reuse.unwrap_or(false)
                ));
            } else {
                parts.push(format!(
                    "{}=result|ret={}|errno={}",
                    tag, steps[i].ret, steps[i].errno
                ));
            }
        } else if let Some(c) = &d6b_skip {
            parts.push(format!("{}={}", tag, c));
        } else {
            parts.push(format!("{}=unobservable(cause=step-not-executed)", tag));
        }
    }

    parts.join(";")
}

// P12 derivation budget (m-04): POST must stay a single hilog line, but the
// on-device truncation threshold has no measured bound (gate-plan :1691
// records "hilog 单行截断阈值无实测依据"), so the total line is held
// conservatively at <= 1024 B: fixed framing ("N1BDISC_POST" + separators +
// ledger_digest + worker_terminal_at_p12) = 142 B worst case, leaving
// d6_items <= 384 B (realistic worst ~320 B incl. \x7c escapes) and
// dw_outcome <= 448 B (realistic worst 402 B = the flag-race cell) =>
// absolute worst 974 B. Poll-raw inner caps are 48 B so the frozen
// same-cause literals (longest 44 B, `unobservable(cause=flag-race-window-
// expired)`) survive intact — the runner's cut=false closed table compares
// them byte-exact (:778-779).
const D6_ITEMS_CAP: usize = 384;
const DW_OUTCOME_CAP: usize = 448;

/// P12 derivation output: the five A12-bound outputs (cut/class/RACEWIN/
/// watchdog/poll raw). `cut` = worker_terminal_at_p12, the FLAG read the
/// deciding cell bound its value to.
struct PostOutcome {
    class: String,
    poll_ret: String,
    poll_errno: String,
    poll_revents: String,
    poll_elapsed: String,
    watchdog: String,
    racewin: bool,
    cut: bool,
}

/// The seven-step first-match chain of the `dw_return_class` domain
/// declaration, pure (gate-plan :732-733 frozen order (a) -> (c) -> (b) ->
/// (d) -> late-race window -> (e) -> (f); r22/r25 output bindings):
/// - (a) `!spawned` -> skip-table assignment;
/// - (c) `SKIP|item=destroy` presence alone -> class 0 destroy-skip-proven,
///   BEFORE (d)/(e)/race window and regardless of RETURN/FLAG (:736-737);
/// - (b) pre-only death closeout encoding (:735) is runner-side (process
///   death with judgment inputs unavailable and no SKIP|item=destroy); the
///   P12 caller runs on a live process, so that cell has no device-side
///   derivation — its position is carried between (c) and (d) here;
/// - (d) JT ∧ F=0 ∧ SIG unseen -> poll-never-returned, NO RACEWIN (:738-749);
/// - race window: JT ∧ F=0 ∧ SIG seen -> the caller ran the 1000 ms box and
///   passes the fresh expiry FLAG read in `f_after_box`; F=0 at expiry ->
///   flag-race + RACEWIN, the ONLY cell with racewin=true (:756-770);
/// - (e)/(f) flag set -> snapshot readable behind the flag -> 13-class domain
///   (:750-752); (e)'s "capture has no DW_RETURN" face is a capture-side
///   contradiction exercised by the runner's F8(2) rule — on-device F=1
///   guarantees RETURN was emitted before the flag (frozen worker order).
fn derive_post_outcome(
    spawned: bool,
    jt: bool,
    f: bool,
    sig: bool,
    f_after_box: Option<bool>,
    destroy_skip: Option<&str>,
    dw_skip: Option<&str>,
    c_mono: Option<u64>,
    t_mono: Option<u64>,
    raw: &ClassRaw,
) -> PostOutcome {
    // ⑤ cut-imputed: from the same F load as class/cut (r25) — never via an
    // EXIT-presence observation
    let watchdog_cut_imputed = "unobservable(cause=marker-gap-indeterminate)".to_string();

    // (e)/(f): flag set -> 13-class domain; cut=true, watchdog ④ (flag
    // semantics = all worker output complete, EXIT emitted)
    fn thirteen(
        raw: &ClassRaw,
        c_mono: Option<u64>,
        t_mono: Option<u64>,
    ) -> PostOutcome {
        PostOutcome {
            class: derive_class_13(raw, false, c_mono, t_mono),
            poll_ret: raw.ret.to_string(),
            poll_errno: raw.errno.to_string(),
            poll_revents: raw.revents.to_string(),
            poll_elapsed: raw.elapsed.to_string(),
            watchdog: "observed-false".to_string(), // ④
            racewin: false,
            cut: true,
        }
    }

    // (a) D-W skipped wholesale: skip-table assignment (:734) — field-level
    // full assignment; a co-present SKIP|item=destroy does NOT divert to
    // class 0 ((a) precedes (c), :737).
    if !spawned {
        let u = format!("unobservable(cause={})", dw_skip.unwrap_or("no-live-fd"));
        return PostOutcome {
            class: u.clone(),
            poll_ret: u.clone(),
            poll_errno: u.clone(),
            poll_revents: u.clone(),
            poll_elapsed: u.clone(),
            watchdog: u,
            racewin: false,
            cut: f,
        };
    }

    // (c) class 0 (r17): with the flag set the snapshot is readable and the
    // raw values are reported verbatim; with F=0 (the real barrier-never-
    // observed shape — the worker never reached poll) the raw has no value
    // and the same-cause encoding is carried.
    if let Some(cause) = destroy_skip {
        let u = format!("unobservable(cause={})", cause);
        let (pr, pe, pv, pel, wd) = if f {
            (
                raw.ret.to_string(),
                raw.errno.to_string(),
                raw.revents.to_string(),
                raw.elapsed.to_string(),
                "observed-false".to_string(), // ④
            )
        } else {
            (
                u.clone(),
                u.clone(),
                u.clone(),
                u.clone(),
                watchdog_cut_imputed, // ⑤ cut-imputed
            )
        };
        return PostOutcome {
            class: "destroy-skip-proven".to_string(),
            poll_ret: pr,
            poll_errno: pe,
            poll_revents: pv,
            poll_elapsed: pel,
            watchdog: wd,
            racewin: false,
            cut: f,
        };
    }

    if !f {
        // (d) five preconditions (:738-749). post_emit runs only after the
        // P10 terminal-poll box, so F=0 implies the box expired (JT
        // registered); the structurally-unreachable !jt residue keeps the
        // same honest encoding (an unpublished snapshot is never read).
        if !sig || !jt {
            let u = "unobservable(cause=poll-never-returned)".to_string();
            return PostOutcome {
                class: u.clone(),
                poll_ret: u.clone(),
                poll_errno: u.clone(),
                poll_revents: u.clone(),
                poll_elapsed: u.clone(),
                watchdog: watchdog_cut_imputed, // ⑤ cut-imputed
                racewin: false,
                cut: false,
            };
        }
        // late-race window (:756-770): the 1000 ms box already ran in the
        // caller; the box-expiry FLAG check (:758) decides this cell.
        return match f_after_box {
            Some(true) => thirteen(raw, c_mono, t_mono),
            _ => {
                let u = "unobservable(cause=flag-race-window-expired)".to_string();
                PostOutcome {
                    class: u.clone(),
                    poll_ret: u.clone(),
                    poll_errno: u.clone(),
                    poll_revents: u.clone(),
                    poll_elapsed: u.clone(),
                    watchdog: watchdog_cut_imputed, // ⑤ cut-imputed
                    racewin: true,                  // RACEWIN emission point: this cell ONLY
                    cut: false,
                }
            }
        };
    }
    thirteen(raw, c_mono, t_mono)
}

/// Fallback for the P12 `dw_join_result` POST column when no join state was
/// written (`dw_join` never ran / never recorded). Producer-conformant to the
/// ten-value domain (gate-plan :870) and the skip-table full assignment
/// (:443/:734): D-W skipped wholesale (worker never spawned) -> the skip
/// literal verbatim, NOT `pending`. The residual tail (`spawned && !jt` with
/// no join state) is structurally unreachable on the frozen ets flow
/// (terminal observed -> `dw_join` always records; terminal not observed ->
/// jt registered) — keep `pending` there so the runner's strict ten-value
/// parse fails closed (F8(2)) if a bug ever makes it reachable.
fn join_result_fallback(spawned: bool, jt: bool, dw_skip: Option<&str>) -> String {
    if !spawned {
        return format!("unobservable(cause={})", dw_skip.unwrap_or("no-live-fd"));
    }
    if jt {
        "join-timeout".to_string()
    } else {
        "pending".to_string()
    }
}

pub fn post_emit(destroy_resolved: bool) -> String {
    let (spawned, jt, destroy_skip, dw_skip, t_mono, c_mono) = with_state(|s| {
        (
            s.dw_spawned,
            s.jt_registered,
            s.destroy_skip_cause.clone(),
            s.dw_skip_cause.clone(),
            s.destroy_t_mono,
            s.destroy_c_mono,
        )
    });

    // Seven-step chain input reads. THE single decision load (A12): every
    // cell's five outputs bind to the one `f` seq_cst load of
    // dw_worker_terminal below; the race-window cell additionally uses the
    // fresh FLAG read at its 1000 ms box expiry (the :758 sanctioned
    // re-read), carried in `f_after_box` — no other FLAG read exists.
    let f = DW_TERMINAL.load(Ordering::SeqCst);
    let sig = DW_SNAPSHOT_WRITTEN.load(Ordering::SeqCst);

    // Late-race window box (r20/r21): entry JT ∧ F=0 ∧ SIG seen — evaluated
    // AFTER chain steps (a)/(c), so a barrier-never-observed SKIP never runs
    // this box. Frozen 1000 ms bound, 10 ms bounded poll (:757-761).
    let f_after_box = if spawned && destroy_skip.is_none() && jt && !f && sig {
        let box_deadline = sys::mono_ms() + 1_000;
        while sys::mono_ms() < box_deadline {
            if DW_TERMINAL.load(Ordering::SeqCst) {
                break;
            }
            sys::sleep_ms(10);
        }
        // contradiction detection only (SIG never resets; SIG=0 here is an
        // F8(3) implementation-bug face — recorded raw for the runner)
        if !DW_SNAPSHOT_WRITTEN.load(Ordering::SeqCst) {
            with_state(|s| s.sig_zero_after_box = true);
        }
        Some(DW_TERMINAL.load(Ordering::SeqCst)) // :758 expiry F=FLAG check
    } else {
        None
    };

    // snapshot read gate: only behind the terminal flag (:761 快照门禁读)
    let raw = if f_after_box.unwrap_or(f) {
        snapshot_raw()
    } else {
        ClassRaw {
            ret: 0,
            errno: 0,
            revents: 0,
            at: 0,
            elapsed: 0,
            drain_end: -1,
        }
    };

    let out = derive_post_outcome(
        spawned,
        jt,
        f,
        sig,
        f_after_box,
        destroy_skip.as_deref(),
        dw_skip.as_deref(),
        c_mono,
        t_mono,
        &raw,
    );

    // r22 frozen emission order in the race cell: RACEWIN -> cause -> POST.
    // The emission point lives ONLY in that cell (racewin=true there and
    // nowhere else); POST below is the next and last N1BDISC marker.
    if out.racewin {
        emit("N1BDISC_DW_RACEWIN|expired=1");
        with_state(|s| s.racewin_emitted = true);
    }

    let resolved = destroy_resolved || with_state(|s| s.d6.d6a_ran);
    let join_result = with_state(|s| s.join_result.clone())
        .unwrap_or_else(|| join_result_fallback(spawned, jt, dw_skip.as_deref()));

    let distinguishable = derive_distinguishable(
        &out.class,
        destroy_skip.as_deref(),
        c_mono.is_some(),
        resolved,
    );

    let d6_items = d6_items_string();
    let final_digest = ledger::digest(false); // final cut: open -> open-at-exit

    let dw_outcome = format!(
        "class={};join={};watchdog={};dist={};poll_ret={};poll_errno={};poll_revents={};poll_elapsed_ms={}",
        sanitize_marker_field(&out.class, 128),
        sanitize_marker_field(&join_result, 64),
        sanitize_marker_field(&out.watchdog, 96),
        sanitize_marker_field(&distinguishable, 128),
        sanitize_marker_field(&out.poll_ret, 48),
        sanitize_marker_field(&out.poll_errno, 48),
        sanitize_marker_field(&out.poll_revents, 48),
        sanitize_marker_field(&out.poll_elapsed, 48)
    );
    emit(&format!(
        "N1BDISC_POST|d6_items={}|dw_outcome={}|ledger_digest={}|worker_terminal_at_p12={}",
        sanitize_marker_field(&d6_items, D6_ITEMS_CAP),
        sanitize_marker_field(&dw_outcome, DW_OUTCOME_CAP),
        final_digest,
        out.cut
    ));

    let drain_end_code = SNAP_DRAIN_END.load(Ordering::SeqCst);
    let drain_end_errno = SNAP_DRAIN_ERRNO.load(Ordering::SeqCst);
    let drain_end_lit = drain_end_literal(drain_end_code, drain_end_errno);
    let drain_eintr = crate::state::SNAP_DRAIN_EINTR.load(Ordering::SeqCst);
    let sig_zero = with_state(|s| s.sig_zero_after_box);

    format!(
        "{{{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}}}",
        jstr("dw_return_class", &out.class),
        jstr("dw_join_result", &join_result),
        jstr("dw_watchdog_killed", &out.watchdog),
        jstr("dw_destroy_distinguishable_from_timeout", &distinguishable),
        jstr("dw_poll_ret", &out.poll_ret),
        jstr("dw_poll_errno", &out.poll_errno),
        jstr("dw_poll_revents", &out.poll_revents),
        jstr("dw_poll_return_elapsed_ms", &out.poll_elapsed),
        jstr("dw_drain_end", &drain_end_lit),
        jinum("dw_drain_errno", drain_end_errno as i64),
        jinum("dw_drain_eintr_retries", drain_eintr),
        jstr("d6_items", &d6_items),
        jstr("ledger_digest", &final_digest),
        jbool("worker_terminal_at_p12", out.cut),
        jbool("racewin_emitted", out.racewin),
        jbool("sig_zero_after_box", sig_zero),
        jstr("site", "P12")
    )
}

// ---------------------------------------------------------------------------
// skip markers + P11 socket cleanup
// ---------------------------------------------------------------------------

pub fn skip_emit(item: &str, cause: &str) -> String {
    emit(&format!(
        "N1BDISC_SKIP|item={}|cause={}",
        sanitize_marker_field(item, 64),
        sanitize_marker_field(cause, 96)
    ));
    with_state(|s| {
        if item == "destroy" {
            s.destroy_skip_cause = Some(cause.to_string());
        } else if item == "D-W" {
            s.dw_skip_cause = Some(cause.to_string());
        }
    });
    format!("{{{},\"emitted\":true}}", jstr("item", item))
}

/// P11: close d4_send_socket / d5_sink_socket / d6b_reuse_probe_socket one by
/// one and re-verify each with F_GETFD (EBADF expected). Roles never created
/// get their `not-created|cause=<branch>` ledger registration here.
pub fn cleanup_sockets(not_created_cause: &str) -> String {
    let roles = [
        (Role::D4SendSocket, "d4_send_socket"),
        (Role::D5SinkSocket, "d5_sink_socket"),
        (Role::D6bReuseProbeSocket, "d6b_reuse_probe_socket"),
    ];
    let mut parts: Vec<String> = Vec::new();
    for (role, name) in roles {
        match ledger::live_fd(role) {
            Some(fd) => {
                let r = unsafe { sys::close(fd) };
                let close_errno = if r == -1 { sys::errno() } else { 0 };
                if r == 0 {
                    ledger::emit_close(role, fd, ClosedBy::ProbeProtocolClose);
                }
                let (vr, ve) = if r == 0 {
                    let v = unsafe { sys::fcntl(fd, sys::F_GETFD) };
                    (v, if v == -1 { sys::errno() } else { 0 })
                } else {
                    (-1, close_errno)
                };
                parts.push(format!(
                    "{}:fd={}:close_ret={}:close_errno={}:getfd_ret={}:getfd_errno={}:ebadf_verified={}",
                    name,
                    fd,
                    r,
                    close_errno,
                    vr,
                    ve,
                    vr == -1 && ve == sys::EBADF
                ));
            }
            None => {
                ledger::emit_not_created(role, not_created_cause);
                parts.push(format!("{}:not-created:cause={}", name, not_created_cause));
            }
        }
    }
    format!("{{{},\"done\":true}}", jstr("cleanup", &parts.join(";")))
}

// ---------------------------------------------------------------------------
// host-unit tests (M-06): the derivations above are pure functions — no
// syscalls, no device paths; `cargo test` runs them on the host target.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(ret: i64, errno: i32, revents: i64, at: u64, elapsed: u64, drain_end: i64) -> ClassRaw {
        ClassRaw {
            ret,
            errno,
            revents,
            at,
            elapsed,
            drain_end,
        }
    }

    // ① derive_class_13 — four-step order fully pinned (:726 r15 order:
    // legal-domain gate -> 0/0b -> unknown-bit gate -> rows 1-11).

    #[test]
    fn legal_domain_gate_precedes_0_0b() {
        // B-02 fork input: _C missing AND contradictory raw -> fail (no
        // class), NOT class 0b.
        assert_eq!(derive_class_13(&raw(0, 0, 0x01, 100, 10, 0), false, None, None), "");
        // and NOT class 0 either when the destroy SKIP is present (the
        // runner's rebuild fails the same input).
        assert_eq!(derive_class_13(&raw(0, 0, 0x01, 100, 10, 0), true, None, None), "");
        // each of the four legal-domain contradictions yields no class
        assert_eq!(derive_class_13(&raw(2, 0, 0, 100, 10, 0), false, Some(50), Some(40)), "");
        assert_eq!(derive_class_13(&raw(0, 0, 0x01, 100, 10, 0), false, Some(50), Some(40)), "");
        assert_eq!(derive_class_13(&raw(1, 0, 0x00, 100, 10, 0), false, Some(50), Some(40)), "");
        assert_eq!(derive_class_13(&raw(-1, 0, 0x00, 100, 10, 0), false, Some(50), Some(40)), "");
        // domain-gate-violation literal is retired (M-02)
        assert!(!derive_class_13(&raw(2, 0, 0, 0, 0, 0), false, None, None).contains("domain"));
    }

    #[test]
    fn class_0_and_0b_precede_unknown_bit_gate() {
        // r15's exact hole: ret=1, revents=64 (unknown bit), _C missing ->
        // class 0b, NOT other-revents.
        assert_eq!(
            derive_class_13(&raw(1, 0, 0x40, 100, 10, 0), false, None, None),
            "destroy-call-unobserved"
        );
        // the same input with _C present IS routed to other-revents
        assert_eq!(
            derive_class_13(&raw(1, 0, 0x40, 100, 10, 0), false, Some(50), Some(40)),
            "other-revents"
        );
        // SKIP => class 0 regardless of _C/RETURN (r17)
        assert_eq!(
            derive_class_13(&raw(1, 0, 0x01, 100, 10, 0), true, None, None),
            "destroy-skip-proven"
        );
        assert_eq!(
            derive_class_13(&raw(1, 0, 0x01, 100, 10, 0), true, Some(50), Some(40)),
            "destroy-skip-proven"
        );
    }

    #[test]
    fn rows_one_to_eleven_truth_table() {
        let c = Some(1000u64);
        let t = Some(900u64);
        let eagain = DRAIN_END_EAGAIN;
        // row 1/2: ret=-1 (revents meaningless; EINTR=4)
        assert_eq!(derive_class_13(&raw(-1, 4, 0x10, 2000, 100, eagain), false, c, t), "interrupted");
        assert_eq!(derive_class_13(&raw(-1, 11, 0x10, 2000, 100, eagain), false, c, t), "poll-error");
        // row 3: POLLNVAL precedes row 4
        assert_eq!(derive_class_13(&raw(1, 0, 0x20, 800, 100, eagain), false, c, t), "fd-invalid");
        // row 4: non-empty revents, at strictly < T
        assert_eq!(derive_class_13(&raw(1, 0, 0x01, 899, 100, eagain), false, c, t), "pre-destroy-ready");
        // band edges: at == T is not pre, at == C is not post (strict bands)
        assert_eq!(derive_class_13(&raw(1, 0, 0x01, 900, 100, eagain), false, c, t), "other-revents");
        assert_eq!(derive_class_13(&raw(1, 0, 0x01, 1000, 100, eagain), false, c, t), "other-revents");
        // row 5: HUP/ERR ∧ elapsed >= 4500 ∧ at > C
        assert_eq!(derive_class_13(&raw(1, 0, 0x010, 2000, 4500, eagain), false, c, t), "late-fd-event");
        // row 6: POLLIN ∧ elapsed >= 4500
        assert_eq!(derive_class_13(&raw(1, 0, 0x001, 2000, 4500, eagain), false, c, t), "late-data");
        // row 7: HUP ∧ elapsed < 4500 ∧ post ∧ drain eagain — the only
        // destroy-attributable class
        assert_eq!(derive_class_13(&raw(1, 0, 0x010, 2000, 4499, eagain), false, c, t), "fd-event-like");
        // row 7 tightened: drain not eagain -> falls through
        assert_eq!(
            derive_class_13(&raw(1, 0, 0x010, 2000, 4499, crate::state::DRAIN_END_ZERO_READ), false, c, t),
            "other-revents"
        );
        // row 8: POLLIN (no HUP/ERR) ∧ < 4500 ∧ post ∧ eagain
        assert_eq!(
            derive_class_13(&raw(1, 0, 0x001, 2000, 4499, eagain), false, c, t),
            "data-ready-post-destroy"
        );
        // row 9/10: empty revents at the 4500 boundary
        assert_eq!(derive_class_13(&raw(0, 0, 0x0, 2000, 4500, eagain), false, c, t), "timeout-like");
        assert_eq!(derive_class_13(&raw(0, 0, 0x0, 2000, 4499, eagain), false, c, t), "spurious-early");
        // row 11: mask-internal combination not matched above (POLLPRI only)
        assert_eq!(derive_class_13(&raw(1, 0, 0x002, 2000, 100, eagain), false, c, t), "other-revents");
    }

    // ② derive_post_outcome — seven-step chain x cut-state cells.

    #[test]
    fn join_fallback_skip_case_uses_skip_literal_not_pending() {
        // B4 probe-side: D-W skipped wholesale (never spawned) -> the skip
        // literal verbatim (:443/:734/:870 ten-value domain), never `pending`.
        assert_eq!(
            join_result_fallback(false, false, Some("no-live-fd")),
            "unobservable(cause=no-live-fd)"
        );
        assert_eq!(
            join_result_fallback(false, false, Some("dup-failed")),
            "unobservable(cause=dup-failed)"
        );
        assert_eq!(
            join_result_fallback(false, false, None),
            "unobservable(cause=no-live-fd)"
        );
        // spawned + JT registered (terminal box expired, join not called)
        // -> join-timeout (:746/:1177).
        assert_eq!(join_result_fallback(true, true, None), "join-timeout");
        assert_eq!(
            join_result_fallback(true, true, Some("no-live-fd")),
            "join-timeout"
        );
        // structurally unreachable residual tail stays `pending` — runner
        // strict ten-value parse fails closed on it (F8(2)).
        assert_eq!(join_result_fallback(true, false, None), "pending");
    }

    #[test]
    fn chain_a_dw_skip_assignment() {
        let r = raw(0, 0, 0, 0, 0, -1);
        for cause in ["no-live-fd", "dup-failed"] {
            let out = derive_post_outcome(
                false, false, false, false, None, None, Some(cause), Some(1000), Some(900), &r,
            );
            let u = format!("unobservable(cause={})", cause);
            assert_eq!(out.class, u);
            assert_eq!(out.poll_ret, u);
            assert_eq!(out.watchdog, u); // skip named list (:894)
            assert_eq!(out.racewin, false);
        }
        // (a) precedes (c): a co-present destroy SKIP does NOT divert to class 0
        let out = derive_post_outcome(
            false, false, false, false, None, Some("no-live-connection"), Some("no-live-fd"),
            Some(1000), Some(900), &r,
        );
        assert_eq!(out.class, "unobservable(cause=no-live-fd)");
    }

    #[test]
    fn chain_c_barrier_never_hung_worker_destroy_skip_proven_no_racewin() {
        // barrier-never-observed SKIP + worker hung (JT registered, F=0):
        // class 0 BEFORE (d)/race window — no RACEWIN, no flag-race.
        let r = raw(0, 0, 0, 0, 0, -1);
        let out = derive_post_outcome(
            true,
            true,
            false,
            false,
            None,
            Some("barrier-never-observed"),
            None,
            None,
            None,
            &r,
        );
        assert_eq!(out.class, "destroy-skip-proven");
        assert_eq!(out.racewin, false);
        assert_eq!(out.cut, false);
        assert_eq!(out.watchdog, "unobservable(cause=marker-gap-indeterminate)");
        assert_eq!(out.poll_ret, "unobservable(cause=barrier-never-observed)");
        // distinguishable step (1) maps the SKIP cause verbatim
        assert_eq!(
            derive_distinguishable(&out.class, Some("barrier-never-observed"), false, false),
            "unobservable(cause=barrier-never-observed)"
        );
        // defensive F=1 variant: class 0, raw verbatim, watchdog ④
        let r2 = raw(0, 0, 0, 2000, 4600, DRAIN_END_EAGAIN);
        let out2 = derive_post_outcome(
            true,
            true,
            true,
            true,
            None,
            Some("barrier-never-observed"),
            None,
            None,
            None,
            &r2,
        );
        assert_eq!(out2.class, "destroy-skip-proven");
        assert_eq!(out2.poll_elapsed, "4600");
        assert_eq!(out2.watchdog, "observed-false");
        assert_eq!(out2.cut, true);
    }

    #[test]
    fn chain_d_sig_unset_jt_poll_never_no_racewin() {
        // JT ∧ F=0 ∧ SIG unset -> poll-never-returned, NO RACEWIN (:738-749).
        let r = raw(0, 0, 0, 0, 0, -1);
        let out = derive_post_outcome(
            true, true, false, false, None, None, None, Some(1000), Some(900), &r,
        );
        assert_eq!(out.class, "unobservable(cause=poll-never-returned)");
        assert_eq!(out.poll_ret, "unobservable(cause=poll-never-returned)");
        assert_eq!(out.watchdog, "unobservable(cause=marker-gap-indeterminate)"); // ⑤
        assert_eq!(out.racewin, false);
        assert_eq!(out.cut, false);
        // (0) passthrough of the four-step distinguishable table
        assert_eq!(
            derive_distinguishable(&out.class, None, true, true),
            "unobservable(cause=poll-never-returned)"
        );
    }

    #[test]
    fn chain_race_box_expired_f0_racewin_flag_race() {
        // SIG seen, 1000 ms box expired with F=0 -> RACEWIN + flag-race;
        // this is the ONLY cell with racewin=true (:756-770).
        let r = raw(0, 0, 0, 0, 0, -1);
        let out = derive_post_outcome(
            true, true, false, true, Some(false), None, None, Some(1000), Some(900), &r,
        );
        assert_eq!(out.class, "unobservable(cause=flag-race-window-expired)");
        for v in [&out.poll_ret, &out.poll_errno, &out.poll_revents, &out.poll_elapsed] {
            assert_eq!(v, "unobservable(cause=flag-race-window-expired)");
        }
        assert_eq!(out.watchdog, "unobservable(cause=marker-gap-indeterminate)"); // ⑤
        assert_eq!(out.racewin, true);
        assert_eq!(out.cut, false);
    }

    #[test]
    fn chain_race_box_flag_set_reads_snapshot_13class() {
        // box expiry FLAG check (:758): F=1 -> read the snapshot -> 13-class
        // domain, cut=true, no RACEWIN.
        let r = raw(1, 0, 0x001, 2000, 4499, DRAIN_END_EAGAIN);
        let out = derive_post_outcome(
            true, true, false, true, Some(true), None, None, Some(1000), Some(900), &r,
        );
        assert_eq!(out.class, "data-ready-post-destroy");
        assert_eq!(out.poll_ret, "1");
        assert_eq!(out.poll_elapsed, "4499");
        assert_eq!(out.watchdog, "observed-false"); // ④
        assert_eq!(out.racewin, false);
        assert_eq!(out.cut, true);
    }

    #[test]
    fn chain_f_terminal_flag_13class() {
        // (e)/(f): F=1 at entry -> 13-class domain, cut=true.
        let r = raw(-1, 4, 0x0, 2000, 100, DRAIN_END_EAGAIN);
        let out = derive_post_outcome(
            true, false, true, true, None, None, None, Some(1000), Some(900), &r,
        );
        assert_eq!(out.class, "interrupted");
        assert_eq!(out.poll_ret, "-1");
        assert_eq!(out.poll_errno, "4");
        assert_eq!(out.watchdog, "observed-false"); // ④ EXIT emitted
        assert_eq!(out.racewin, false);
        assert_eq!(out.cut, true);
        // distinguishable step (3): interrupted -> no correlated event
        assert_eq!(
            derive_distinguishable(&out.class, None, true, true),
            "unobservable(cause=no-destroy-correlated-event)"
        );
    }

    #[test]
    fn contradictory_raw_carries_no_class_through_post_outcome() {
        // F=1 with a contradictory snapshot (parse-defect face): no class is
        // produced (M-02) — the runner's F8(2) rebuild carries the fail.
        let r = raw(1, 0, 0x0, 2000, 100, DRAIN_END_EAGAIN); // ret=1 ∧ empty revents
        let out = derive_post_outcome(
            true, false, true, true, None, None, None, Some(1000), Some(900), &r,
        );
        assert_eq!(out.class, "");
        assert_eq!(derive_distinguishable(&out.class, None, true, true), "");
    }

    // ④ read_proc_stat state parsing (A7 half-edge): comm with spaces and
    // parentheses — the state token sits after the LAST ')'.

    #[test]
    fn proc_stat_state_after_last_paren() {
        // comm "te st" (space inside, quoted by comm parens)
        let line = "1 (te st) S 1 1 0 0 -1 4194560 0 0 0 0 0 0 0 0 20 0 1 0 12 0";
        assert_eq!(parse_proc_stat_state(line).as_deref(), Some("S"));
        // comm containing parentheses: "(a(b)c)d)" — only the LAST ')' counts
        let line = "42 ((a(b)c)d) R 1 1 0 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 34 0";
        assert_eq!(parse_proc_stat_state(line).as_deref(), Some("R"));
        // a left-side (third-field) split would read "S" from "a(b)c)d) S" —
        // pin the correct token instead: state after the last ')'
        let line = "7 (p o) l) Z 2 0 0 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 56 0";
        assert_eq!(parse_proc_stat_state(line).as_deref(), Some("Z"));
        // no ')' at all -> unreadable face
        assert_eq!(parse_proc_stat_state("garbage without parens"), None);
        assert_eq!(parse_proc_stat_state(""), None);
    }
}
