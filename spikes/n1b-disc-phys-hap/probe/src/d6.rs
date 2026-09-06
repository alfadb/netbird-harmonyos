//! D6 post-destroy synchronous attempts (U4, per sub-item) — gate-plan :954-985.
//!
//! D6a (orig side, immediately after destroy() resolves, same call stack):
//!   S1 fcntl(fd_orig, F_GETFD) / S2 fcntl(fd_orig, F_GETFL) / S3 close(fd_orig)
//!   — the ONLY close(fd_orig) call site in the crate (A2); ledger close with
//!   by=d6a-probe-close when it returns 0.
//! D6b (dup side, after worker terminal; join-timeout shape skips the whole
//! segment — driven by the ArkTS mainline per the r18 re-ruling):
//!   S4 fcntl(fd_dup, F_GETFD) / S5 read(fd_dup, buf, 2048) (non-blocking since
//!   D2.5; unbounded blocking read forbidden) / S6 close(fd_dup) /
//!   S7 socket(AF_INET, SOCK_DGRAM, 0) + fd-number reuse observation.
//! Every step emits its pre-marker BEFORE executing (reached-ness is judged by
//! pre-marker existence) and its result-marker after; `u4_*` per-subitem values
//! here are "observed-true" when the step executed (result marker exists with a
//! registered ret) — the pre-marker/death-evidence split stays runner-side.

use crate::hilog::emit;
use crate::ledger::{self, ClosedBy, Role};
use crate::state::{with_state, D6StepResult};
use crate::sys;
use crate::util::{hex_lower, jbool, jinum, jstr};

pub fn d6a(fd_orig: i32) -> String {
    with_state(|s| {
        s.fd_orig = Some(fd_orig);
        s.d6.d6a_ran = true;
        while s.d6.steps.len() < 7 {
            s.d6.steps.push(D6StepResult { ret: 0, errno: 0, fd: None, reuse: None });
        }
    });

    // S1: fcntl(fd_orig, F_GETFD)
    emit("N1BDISC_D6S1_B");
    let r1 = unsafe { sys::fcntl(fd_orig, sys::F_GETFD) };
    let e1 = if r1 == -1 { sys::errno() } else { 0 };
    emit(&format!("N1BDISC_D6S1_R|ret={}|errno={}", r1, e1));

    // S2: fcntl(fd_orig, F_GETFL)
    emit("N1BDISC_D6S2_B");
    let r2 = unsafe { sys::fcntl(fd_orig, sys::F_GETFL) };
    let e2 = if r2 == -1 { sys::errno() } else { 0 };
    emit(&format!("N1BDISC_D6S2_R|ret={}|errno={}", r2, e2));

    // S3: close(fd_orig) — double-close probe after destroy already performed
    // its duty. THE ONLY close(fd_orig) call site in the probe (A2).
    emit("N1BDISC_D6S3_B");
    let r3 = unsafe { sys::close(fd_orig) };
    let e3 = if r3 == -1 { sys::errno() } else { 0 };
    emit(&format!("N1BDISC_D6S3_R|ret={}|errno={}", r3, e3));
    if r3 == 0 {
        ledger::emit_close(Role::FdOrig, fd_orig, ClosedBy::D6aProbeClose);
    }

    with_state(|s| {
        s.d6.steps[0] = D6StepResult { ret: r1 as i64, errno: e1, fd: None, reuse: None };
        s.d6.steps[1] = D6StepResult { ret: r2 as i64, errno: e2, fd: None, reuse: None };
        s.d6.steps[2] = D6StepResult { ret: r3 as i64, errno: e3, fd: None, reuse: None };
    });

    format!(
        "{{{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}}}",
        jstr("d6a_ran", "true"),
        jstr("u4_orig_getfd", "observed-true"),
        jinum("s1_ret", r1 as i64),
        jinum("s1_errno", e1 as i64),
        jstr("u4_orig_getfl", "observed-true"),
        jinum("s2_ret", r2 as i64),
        jinum("s2_errno", e2 as i64),
        jstr("u4_orig_close", "observed-true"),
        jinum("s3_ret", r3 as i64),
        jinum("s3_errno", e3 as i64),
        jstr("u4_post_destroy_sync_observable_note", "summary is runner-side (MJ-5 per-subitem primary)"),
        jinum("fd_orig", fd_orig as i64),
        jstr("fd_orig_close_ledger", if r3 == 0 { "by=d6a-probe-close" } else { "open (destroy-side close unobservable)" }),
        jstr("site", "P9-post-resolve"),
        jstr("s3_ebadf", if e3 == sys::EBADF { "true" } else { "false" })
    )
}

pub fn d6b(fd_dup: i32) -> String {
    with_state(|s| {
        s.fd_dup = Some(fd_dup);
        s.d6.d6b_ran = true;
        while s.d6.steps.len() < 7 {
            s.d6.steps.push(D6StepResult { ret: 0, errno: 0, fd: None, reuse: None });
        }
    });

    // S4: fcntl(fd_dup, F_GETFD)
    emit("N1BDISC_D6S4_B");
    let r4 = unsafe { sys::fcntl(fd_dup, sys::F_GETFD) };
    let e4 = if r4 == -1 { sys::errno() } else { 0 };
    emit(&format!("N1BDISC_D6S4_R|ret={}|errno={}", r4, e4));

    // S5: read(fd_dup, buf, 2048) — non-blocking since D2.5; the numeric errno
    // placeholder is 0 on success (frozen: never empty)
    emit("N1BDISC_D6S5_B");
    let mut buf = [0u8; 2048];
    let (r5, e5) = sys::read_fd(fd_dup, &mut buf);
    let first16 = if r5 > 0 {
        hex_lower(&buf[..core::cmp::min(16, r5 as usize)])
    } else {
        String::new()
    };
    emit(&format!("N1BDISC_D6S5_R|ret={}|errno={}", r5, e5));

    // S6: close(fd_dup)
    emit("N1BDISC_D6S6_B");
    let r6 = unsafe { sys::close(fd_dup) };
    let e6 = if r6 == -1 { sys::errno() } else { 0 };
    emit(&format!("N1BDISC_D6S6_R|ret={}|errno={}", r6, e6));
    if r6 == 0 {
        ledger::emit_close(Role::FdDup, fd_dup, ClosedBy::ProbeProtocolClose);
    }

    // S7: socket(AF_INET, SOCK_DGRAM, 0) + fd-number reuse observation
    emit("N1BDISC_D6S7_B");
    let s7fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
    let s7e = if s7fd == -1 { sys::errno() } else { 0 };
    let reuse = s7fd >= 0 && s7fd == fd_dup;
    emit(&format!("N1BDISC_D6S7_R|fd={}|reuse={}", s7fd, reuse));
    if s7fd >= 0 {
        ledger::emit_create(Role::D6bReuseProbeSocket, s7fd);
    } else {
        ledger::emit_not_created(Role::D6bReuseProbeSocket, &format!("socket-failed-errno-{}", s7e));
    }

    with_state(|s| {
        s.d6.steps[3] = D6StepResult { ret: r4 as i64, errno: e4, fd: None, reuse: None };
        s.d6.steps[4] = D6StepResult { ret: r5 as i64, errno: e5, fd: None, reuse: None };
        s.d6.steps[5] = D6StepResult { ret: r6 as i64, errno: e6, fd: None, reuse: None };
        s.d6.steps[6] = D6StepResult { ret: s7fd as i64, errno: s7e, fd: Some(s7fd), reuse: Some(reuse) };
    });

    format!(
        "{{{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}}}",
        jstr("d6b_ran", "true"),
        jstr("u4_dup_getfd", "observed-true"),
        jinum("s4_ret", r4 as i64),
        jinum("s4_errno", e4 as i64),
        jstr("u4_dup_read", "observed-true"),
        jinum("s5_ret", r5 as i64),
        jinum("s5_errno", e5 as i64),
        jstr("s5_first16_hex", &first16),
        jstr("u4_dup_close", "observed-true"),
        jinum("s6_ret", r6 as i64),
        jinum("s6_errno", e6 as i64),
        jstr("u4_dup_fd_reuse", if reuse { "observed-true" } else { "observed-false" }),
        jinum("s7_fd", s7fd as i64),
        jinum("s7_errno", s7e as i64),
        jbool("reuse", reuse),
        jinum("fd_dup", fd_dup as i64),
        jstr("s6_ebadf", if e6 == sys::EBADF { "true" } else { "false" }),
        jstr("fd_dup_close_ledger", if r6 == 0 { "by=probe-protocol-close" } else { "open" }),
        jstr("site", "P10-post-terminal")
    )
}
