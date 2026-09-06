//! D2 retention-entry lock sequence, native steps 2.1-2.7 (gate-plan :489-509)
//! plus the late-fd registration (:471, ledger role `d2_late_fd`).
//!
//! ArkTS resolves `create()` and hands the returned fd number to `d2_lock`;
//! fd numbers cross the NAPI boundary as int (documented in lib.rs). This
//! function registers fd_orig in the ledger, takes the F_DUPFD_CLOEXEC dup
//! (fallback dup()+F_SETFD, both paths registered verbatim), snapshots initial
//! flags BEFORE any F_SETFL, then sets O_NONBLOCK on the dup copy only (A2).

use crate::hilog::emit;
use crate::ledger::{self, Role};
use crate::state::{with_state, ProbeState};
use crate::sys;
use crate::util::{jstr, jinum};

pub fn d2_lock(fd_orig: i32) -> String {
    let mut j = String::from("{");
    let mut ok = true;

    // ledger: fd_orig create (the create itself happened in ArkTS/framework;
    // registration point = first native touch, per the retention lock sequence)
    ledger::emit_create(Role::FdOrig, fd_orig);
    with_state(|s: &mut ProbeState| s.fd_orig = Some(fd_orig));

    // S1: fcntl(fd_orig, F_GETFL) — initial flags baseline BEFORE any F_SETFL
    let s1 = unsafe { sys::fcntl(fd_orig, sys::F_GETFL) };
    let s1_errno = if s1 == -1 { sys::errno() } else { 0 };
    emit(&format!("N1BDISC_D2_S1|ret={}|errno={}", s1, s1_errno));

    // S2: fcntl(fd_orig, F_DUPFD_CLOEXEC, 0); fallback dup()+F_SETFD(FD_CLOEXEC)
    let mut fd_dup: i32 = -1;
    let d2 = unsafe { sys::fcntl(fd_orig, sys::F_DUPFD_CLOEXEC, 0i64) };
    if d2 >= 0 {
        fd_dup = d2;
        emit(&format!("N1BDISC_D2_S2|path=f_dupfd_cloexec|fd={}|errno=0", fd_dup));
    } else {
        let d2_errno = sys::errno();
        let d = sys_dup(fd_orig);
        if d >= 0 {
            let sf = unsafe { sys::fcntl(d, sys::F_SETFD, sys::FD_CLOEXEC as i64) };
            let sf_errno = if sf == -1 { sys::errno() } else { 0 };
            fd_dup = d;
            emit(&format!(
                "N1BDISC_D2_S2|path=dup_fallback|fd={}|errno={}",
                fd_dup, d2_errno
            ));
            // fallback path detail (F_SETFD result) carried in JSON only
            j.push_str(&jstr("d2_fallback_setfd_errno", &sf_errno.to_string()));
            j.push_str(",");
        } else {
            let d_errno = sys::errno();
            emit(&format!(
                "N1BDISC_D2_S2|path=dup_fallback|fd=-1|errno={}",
                d_errno
            ));
            ok = false;
        }
    }

    let mut s3 = -1i64;
    let mut s3_errno = 0i64;
    let mut u6 = "unobservable";
    let mut s5 = -1i64;
    let mut s5_errno = 0i64;
    let mut of = "unobservable";

    if ok {
        ledger::emit_create(Role::FdDup, fd_dup);
        with_state(|s| s.fd_dup = Some(fd_dup));

        // S3: fcntl(fd_dup, F_GETFL)
        s3 = unsafe { sys::fcntl(fd_dup, sys::F_GETFL) } as i64;
        s3_errno = if s3 == -1 { sys::errno() as i64 } else { 0 };
        emit(&format!("N1BDISC_D2_S3|ret={}|errno={}", s3, s3_errno));

        // S4: u6 judgment on O_NONBLOCK bit in the S3 baseline
        if s3 != -1 {
            u6 = if (s3 as i32 & sys::O_NONBLOCK) != 0 {
                "o_nonblock_present"
            } else {
                "o_nonblock_absent"
            };
        }
        emit(&format!("N1BDISC_D2_S4|u6={}", u6));

        // S5: fcntl(fd_dup, F_SETFL, O_NONBLOCK) — dup copy ONLY (A2)
        s5 = unsafe { sys::fcntl(fd_dup, sys::F_SETFL, sys::O_NONBLOCK as i64) } as i64;
        s5_errno = if s5 == -1 { sys::errno() as i64 } else { 0 };
        emit(&format!("N1BDISC_D2_S5|ret={}|errno={}", s5, s5_errno));

        // S6: fcntl(fd_orig, F_GETFL) — OFD side-effect observation
        let s6 = unsafe { sys::fcntl(fd_orig, sys::F_GETFL) };
        if s6 == -1 {
            of = "unobservable";
        } else {
            of = if (s6 & sys::O_NONBLOCK) != 0 {
                "observed-true"
            } else {
                "observed-false"
            };
        }
        emit(&format!("N1BDISC_D2_S6|of={}", of));
    }

    // S7: MTU oracle prior — none preregistered
    emit("N1BDISC_D2_S7|prior=no-preregistered-oracle");

    j.push_str(&jstr("d2_getfl_orig_initial", &s1.to_string()));
    j.push_str(",");
    j.push_str(&jstr("d2_getfl_orig_errno", &s1_errno.to_string()));
    j.push_str(",");
    j.push_str(&jstr("d2_dup_path", if d2 >= 0 { "f_dupfd_cloexec" } else if ok { "dup_fallback" } else { "dup-failed" }));
    j.push_str(",");
    j.push_str(&jinum("fd_dup", if ok { fd_dup as i64 } else { -1 }));
    j.push_str(",");
    j.push_str(&jstr("d2_getfl_dup_initial", &s3.to_string()));
    j.push_str(",");
    j.push_str(&jstr("d2_getfl_dup_errno", &s3_errno.to_string()));
    j.push_str(",");
    j.push_str(&jstr("u6_initial_flags_and_isblocking_effect", u6));
    j.push_str(",");
    j.push_str(&jstr("u6_nonblocking_initial", if u6 == "o_nonblock_present" { "observed-true" } else if u6 == "o_nonblock_absent" { "observed-false" } else { "unobservable" }));
    j.push_str(",");
    j.push_str(&jstr("d2_setfl_dup_ret", &s5.to_string()));
    j.push_str(",");
    j.push_str(&jstr("d2_setfl_dup_errno", &s5_errno.to_string()));
    j.push_str(",");
    j.push_str(&jstr("of_nonblock_shared_to_orig", of));
    j.push_str(",");
    j.push_str(&jstr("mtu_api_oracle", "unregistered"));
    j.push_str(",");
    j.push_str(&jstr("mtu_oracle_exists", "unobservable(cause=no-preregistered-oracle)"));
    j.push_str("}");
    j
}

fn sys_dup(fd: i32) -> i32 {
    unsafe { sys::dup(fd) }
}

/// Late-resolve fd registration (:471): emit `N1BDISC_D2_LATE_FD|fd=|at_mono_ms=`
/// + ledger create for role `d2_late_fd` — registered, NEVER closed (closing
/// duty belongs to process exit and the host-side ForceStop).
pub fn d2_late_fd(fd: i32) -> String {
    let at = sys::mono_ms();
    emit(&format!("N1BDISC_D2_LATE_FD|fd={}|at_mono_ms={}", fd, at));
    ledger::emit_create(Role::D2LateFd, fd);
    format!("{{{},\"late_fd_orphaned\":true}}", jstr("fd", &fd.to_string()))
}

// ---------------------------------------------------------------------------
// ArkTS-driven create-matrix markers routed through the native channel so the
// complete frozen literal set (:1095-1097) is available from one emitter.
// ---------------------------------------------------------------------------

/// `N1BDISC_D2_ENTRY|id=<id>|phase=attempted` (:510).
pub fn d2_entry_attempted(id: &str) -> String {
    emit(&format!(
        "N1BDISC_D2_ENTRY|id={}|phase=attempted",
        crate::util::sanitize_marker_field(id, 32)
    ));
    format!("{{{},\"emitted\":true}}", jstr("id", id))
}

/// `N1BDISC_D2_ENTRY|id=<id>|outcome=<…>|fd=<n|none>` (:511); outcome domain is
/// validated against the frozen enumeration.
pub fn d2_entry_outcome(id: &str, outcome: &str, fd: i32) -> String {
    const OK: [&str; 7] = [
        "resolved",
        "rejected",
        "timeout",
        "late-resolved",
        "late-rejected",
        "indeterminate",
        "not_attempted",
    ];
    if !OK.contains(&outcome) {
        return format!(
            "{{{}}}",
            jstr("error", "outcome outside frozen domain")
        );
    }
    let fd_s = if fd >= 0 { fd.to_string() } else { "none".to_string() };
    emit(&format!(
        "N1BDISC_D2_ENTRY|id={}|outcome={}|fd={}",
        crate::util::sanitize_marker_field(id, 32),
        outcome,
        fd_s
    ));
    format!("{{{},\"emitted\":true}}", jstr("id", id))
}

/// `N1BDISC_D2_LATE|id=<id>|kind=<resolve|reject>` (:470).
pub fn d2_late(id: &str, kind: &str) -> String {
    if kind != "resolve" && kind != "reject" {
        return format!("{{{}}}", jstr("error", "kind outside frozen domain"));
    }
    emit(&format!(
        "N1BDISC_D2_LATE|id={}|kind={}",
        crate::util::sanitize_marker_field(id, 32),
        kind
    ));
    format!("{{{},\"emitted\":true}}", jstr("id", id))
}

/// Rejection-text transport for the create matrix: CHUNK stream=rejtext with
/// item = matrix id number (MR1→0 … MB1→4, :421).
pub fn rejtext_emit(item: i32, text: &str) -> String {
    if !(0..=4).contains(&item) {
        return format!("{{{}}}", jstr("error", "item outside 0..=4 matrix domain"));
    }
    crate::chunk::emit_chunk(crate::chunk::STREAM_REJTEXT, item as u32, text);
    format!("{{{},\"emitted\":true}}", jinum("item", item as i64))
}
