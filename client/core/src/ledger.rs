//! fd ledger (gate-plan fd 纪律 :364-410, transition markers :368-377,
//! canonical serialization + digest :378-392).
//!
//! Seven frozen roles; `(role, inst)` identifies an instance; every transition
//! (create / close / not-created) emits `N1BDISC_FD|fd=|role=|inst=|action=|
//! at_mono_ms=|by=|cause=` immediately. The canonical digest is the SHA-256 of
//! the canonical serialization evaluated at a cut point (PRE -> `open-at-pre`,
//! POST -> `open-at-exit` for entries still open).
//!
//! Probe-side close markers are emitted only for closes the probe itself
//! performs (ret == 0). `destroy()` closing fd_orig inside the framework is not
//! probe-observable and produces no close marker; such entries stay open in the
//! transition stream and are classified by the cut rule — both the probe digest
//! and the runner rebuild use the same rule, so digests stay comparable.

use crate::hilog::emit;
use crate::sys::mono_ms;
use crate::util::sha256_hex;
use std::sync::Mutex;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    FdOrig,
    FdDup,
    D4SendSocket,
    D5SinkSocket,
    D6bReuseProbeSocket,
    DwInwaitProcFd,
    D2LateFd,
}

impl Role {
    /// Frozen role-set literal (also the tie-break order for canonical sorting).
    pub fn literal(self) -> &'static str {
        match self {
            Role::FdOrig => "fd_orig",
            Role::FdDup => "fd_dup",
            Role::D4SendSocket => "d4_send_socket",
            Role::D5SinkSocket => "d5_sink_socket",
            Role::D6bReuseProbeSocket => "d6b_reuse_probe_socket",
            Role::DwInwaitProcFd => "dw_inwait_proc_fd",
            Role::D2LateFd => "d2_late_fd",
        }
    }

    pub fn order(self) -> u8 {
        match self {
            Role::FdOrig => 0,
            Role::FdDup => 1,
            Role::D4SendSocket => 2,
            Role::D5SinkSocket => 3,
            Role::D6bReuseProbeSocket => 4,
            Role::DwInwaitProcFd => 5,
            Role::D2LateFd => 6,
        }
    }
}

/// `closed_by` frozen seven-value domain (:396-397): {`destroy`,
/// `d6a-probe-close`, `probe-protocol-close`, `process-exit`, `host-forcestop`,
/// `open-at-exit`, `open-at-pre`}. This enum carries the FIVE values the probe
/// side can assign — the first three are the close-marker `by=` values, the
/// last two are the P5T/POST cut literals. `by=destroy` is never emitted by
/// this probe: destroy() closing fd_orig inside the framework is not probe-
/// observable (the only probe fd_orig close is the D6a double-close probe,
/// `d6a-probe-close`); the value is kept so the literal mapping stays total
/// over the frozen domain. The remaining two domain values (`process-exit`,
/// `host-forcestop`) are runner-side cut-point registrations — a pre-only
/// closeout and a fail-cleanup ForceStop respectively — that this enum never
/// produces.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClosedBy {
    Destroy,
    D6aProbeClose,
    ProbeProtocolClose,
    OpenAtExit,
    OpenAtPre,
}

impl ClosedBy {
    pub fn literal(self) -> &'static str {
        match self {
            ClosedBy::Destroy => "destroy",
            ClosedBy::D6aProbeClose => "d6a-probe-close",
            ClosedBy::ProbeProtocolClose => "probe-protocol-close",
            ClosedBy::OpenAtExit => "open-at-exit",
            ClosedBy::OpenAtPre => "open-at-pre",
        }
    }
}

#[derive(Clone, Debug)]
struct Entry {
    role: Role,
    inst: u32,
    fd: Option<i32>,
    created_at: Option<u64>,
    /// sort key for not-created entries (registration moment)
    registered_at: u64,
    closed_at: Option<u64>,
    closed_by: Option<ClosedBy>,
    cause: Option<String>,
}

struct LedgerInner {
    entries: Vec<Entry>,
    next_inst: [u32; 7],
}

static LEDGER: Mutex<Option<LedgerInner>> = Mutex::new(None);

fn with_ledger<R>(f: impl FnOnce(&mut LedgerInner) -> R) -> R {
    let mut g = LEDGER.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        *g = Some(LedgerInner {
            entries: Vec::new(),
            next_inst: [1; 7],
        });
    }
    f(g.as_mut().unwrap())
}

/// Emit the transition marker line for a state change.
fn fd_marker(fd: &str, role: &str, inst: u32, action: &str, at: u64, by: &str, cause: &str) {
    emit(&format!(
        "N1BDISC_FD|fd={fd}|role={role}|inst={inst}|action={action}|at_mono_ms={at}|by={by}|cause={cause}"
    ));
}

/// Register a creation and emit its marker. Returns the assigned inst.
pub fn emit_create(role: Role, fd: i32) -> u32 {
    let at = mono_ms();
    let inst = with_ledger(|l| {
        let inst = l.next_inst[role.order() as usize];
        l.next_inst[role.order() as usize] += 1;
        l.entries.push(Entry {
            role,
            inst,
            fd: Some(fd),
            created_at: Some(at),
            registered_at: at,
            closed_at: None,
            closed_by: None,
            cause: None,
        });
        inst
    });
    fd_marker(&fd.to_string(), role.literal(), inst, "create", at, "none", "none");
    inst
}

/// Register a probe-performed close (only called when close() returned 0) and
/// emit its marker.
///
/// m-05 (:406(c)): a close with no matching open instance is an integrity
/// failure — the close marker MUST still reach the capture so the runner's
/// `(role, inst)` pairing state machine registers the fail (`close-before-
/// create` / `state-conflict`). Silently dropping the marker would hide the
/// contradiction from the rebuild.
pub fn emit_close(role: Role, fd: i32, by: ClosedBy) {
    let at = mono_ms();
    let inst = with_ledger(|l| {
        // first open entry of this role (state machine: first close wins)
        let idx = l
            .entries
            .iter()
            .position(|e| e.role == role && e.fd == Some(fd) && e.closed_at.is_none());
        match idx {
            Some(i) => {
                l.entries[i].closed_at = Some(at);
                l.entries[i].closed_by = Some(by);
                Some(l.entries[i].inst)
            }
            None => None,
        }
    });
    match inst {
        Some(inst) => {
            fd_marker(&fd.to_string(), role.literal(), inst, "close", at, by.literal(), "none");
        }
        None => {
            // no open instance matched: emit the orphan close (never-created
            // instance number for this role) and let the runner's pairing
            // state machine carry the :406(c) integrity failure
            let orphan_inst = with_ledger(|l| l.next_inst[role.order() as usize]);
            fd_marker(&fd.to_string(), role.literal(), orphan_inst, "close", at, by.literal(), "none");
        }
    }
}

/// Register an uncreated role (branch cause) and emit its marker. Idempotent
/// per role: only the first not-created registration is kept.
pub fn emit_not_created(role: Role, cause: &str) {
    let at = mono_ms();
    let fresh = with_ledger(|l| {
        if l.entries.iter().any(|e| e.role == role) {
            false
        } else {
            let inst = l.next_inst[role.order() as usize];
            l.next_inst[role.order() as usize] += 1;
            l.entries.push(Entry {
                role,
                inst,
                fd: None,
                created_at: None,
                registered_at: at,
                closed_at: None,
                closed_by: None,
                cause: Some(cause.to_string()),
            });
            true
        }
    });
    if fresh {
        let inst = with_ledger(|l| {
            l.entries
                .iter()
                .find(|e| e.role == role && e.fd.is_none())
                .map(|e| e.inst)
                .unwrap_or(1)
        });
        fd_marker("none", role.literal(), inst, "not-created", at, "none", cause);
    }
}

/// Whether this role has at least one created (non not-created) entry.
pub fn is_created(role: Role) -> bool {
    with_ledger(|l| l.entries.iter().any(|e| e.role == role && e.fd.is_some()))
}

/// The fd number of the most recent still-open entry of this role, if any.
pub fn live_fd(role: Role) -> Option<i32> {
    with_ledger(|l| {
        l.entries
            .iter()
            .filter(|e| e.role == role && e.fd.is_some() && e.closed_at.is_none())
            .next_back()
            .and_then(|e| e.fd)
    })
}

/// Canonical serialization + SHA-256 digest at a cut point.
/// Rules (:378-386): one line per entry, fields
/// `role#k|fd|created_at_mono_ms|closed_by|closed_at_mono_ms|cause`,
/// decimal without leading zeros, sorted by creation moment (not-created by
/// registration moment; ties by role order, then inst), `\n` joined, no trailing
/// newline. Open entries: closed_by = cut literal, closed_at = none.
pub fn digest(cut_pre: bool) -> String {
    let cut = if cut_pre {
        ClosedBy::OpenAtPre
    } else {
        ClosedBy::OpenAtExit
    };
    let mut lines: Vec<(u64, u8, u32, String)> = with_ledger(|l| {
        l.entries
            .iter()
            .map(|e| {
                let sort_at = e.created_at.unwrap_or(e.registered_at);
                let line = match (&e.fd, &e.created_at, &e.cause) {
                    (Some(fd), Some(cat), None) => {
                        let (cby, cat_close) = match (e.closed_by, e.closed_at) {
                            (Some(b), Some(t)) => (b.literal(), t.to_string()),
                            _ => (cut.literal(), "none".to_string()),
                        };
                        format!(
                            "{}#{}|{}|{}|{}|{}|none",
                            e.role.literal(),
                            e.inst,
                            fd,
                            cat,
                            cby,
                            cat_close
                        )
                    }
                    (None, None, Some(c)) => {
                        format!("{}#{}|none|none|none|none|{}", e.role.literal(), e.inst, c)
                    }
                    _ => format!("{}#{}|none|none|none|none|none", e.role.literal(), e.inst),
                };
                (sort_at, e.role.order(), e.inst, line)
            })
            .collect()
    });
    lines.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));
    let joined = lines
        .iter()
        .map(|l| l.3.clone())
        .collect::<Vec<_>>()
        .join("\n");
    sha256_hex(joined.as_bytes())
}

/// Human-readable canonical serialization (for JSON diagnostics).
pub fn canonical_lines() -> String {
    let mut lines: Vec<(u64, u8, u32, String)> = with_ledger(|l| {
        l.entries
            .iter()
            .map(|e| {
                let sort_at = e.created_at.unwrap_or(e.registered_at);
                let line = match (&e.fd, &e.created_at, &e.cause) {
                    (Some(fd), Some(cat), None) => {
                        let (cby, cat_close) = match (e.closed_by, e.closed_at) {
                            (Some(b), Some(t)) => (b.literal(), t.to_string()),
                            _ => ("open", "none".to_string()),
                        };
                        format!(
                            "{}#{}|{}|{}|{}|{}|none",
                            e.role.literal(),
                            e.inst,
                            fd,
                            cat,
                            cby,
                            cat_close
                        )
                    }
                    (None, None, Some(c)) => {
                        format!("{}#{}|none|none|none|none|{}", e.role.literal(), e.inst, c)
                    }
                    _ => format!("{}#{}|none|none|none|none|none", e.role.literal(), e.inst),
                };
                (sort_at, e.role.order(), e.inst, line)
            })
            .collect()
    });
    lines.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));
    lines.iter().map(|l| l.3.clone()).collect::<Vec<_>>().join("\n")
}

// ---------------------------------------------------------------------------
// minimal fd liveness check (product addition; replacement for the e3 C
// fdprobe): one F_GETFD on the caller-owned fd, JSON `{open,errno}`. No
// ledger transition is emitted — the fd belongs to the ArkTS caller.
// ---------------------------------------------------------------------------

pub fn fd_status(fd: i32) -> String {
    let ret = unsafe { crate::sys::fcntl(fd, crate::sys::F_GETFD) };
    let mut j = String::from("{");
    j.push_str(&crate::util::jbool("open", ret != -1));
    j.push_str(",");
    j.push_str(&crate::util::jinum("errno", if ret == -1 { crate::sys::errno() as i64 } else { 0 }));
    j.push_str("}");
    j
}

// ---------------------------------------------------------------------------
// host-unit tests (M-06): digest determinism + cut-point distinctness. The
// ledger is a process-global, so the tests serialize on a lock and use the
// never-closed `d2_late_fd` role to guarantee an open entry regardless of
// test order.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use crate::sys::mono_ms;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn ensure_open_entry() {
        // d2_late_fd is registered create-only and never closed (:366), so it
        // is open at BOTH cut points once created
        let fd = 100 + (mono_ms() % 1000) as i32;
        emit_create(Role::D2LateFd, fd);
    }

    #[test]
    fn digest_is_deterministic_for_a_fixed_state() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        ensure_open_entry();
        let pre_a = digest(true);
        let pre_b = digest(true);
        let exit_a = digest(false);
        let exit_b = digest(false);
        assert_eq!(pre_a, pre_b, "same state, same cut => same digest");
        assert_eq!(exit_a, exit_b, "same state, same cut => same digest");
        assert_eq!(pre_a.len(), 64);
        assert!(pre_a.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn cut_points_differ_when_an_entry_is_open() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        ensure_open_entry();
        // an open entry is registered open-at-pre vs open-at-exit — the two
        // cut literals differ, so the digests must differ (:388-392 禁止跨时点比较)
        let pre = digest(true);
        let exit = digest(false);
        assert_ne!(pre, exit);

        // the cut literals only enter OPEN entries: a closed entry carries its
        // marker by= identically at both cuts (invariant under the cut point)
        let fd = 200 + (mono_ms() % 1000) as i32;
        let inst = emit_create(Role::DwInwaitProcFd, fd);
        let before = digest(true);
        emit_close(Role::DwInwaitProcFd, fd, ClosedBy::ProbeProtocolClose);
        assert_ne!(before, digest(true), "the close must change the canonical text");
        let lines = canonical_lines();
        let needle = format!("dw_inwait_proc_fd#{}|{}|", inst, fd);
        let closed_line = lines
            .lines()
            .find(|l| l.starts_with(&needle))
            .expect("closed entry present in canonical text");
        assert!(
            closed_line.contains("probe-protocol-close") && !closed_line.contains("open-at-"),
            "closed entry keeps by=probe-protocol-close at both cuts: {}",
            closed_line
        );
        // the pre-cut digest still differs from the exit digest (the never-
        // closed d2_late_fd entries remain open)
        assert_ne!(digest(true), digest(false));
    }

    #[test]
    fn emit_close_without_open_entry_still_emits_marker_state() {
        // m-05: an unmatched close must not be swallowed — the ledger keeps
        // no bogus entry, and the marker emission (orphan inst) goes to the
        // capture so the runner's :406(c) pairing state machine can fail it.
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let fd = 300 + (mono_ms() % 1000) as i32;
        let entries_before = with_ledger(|l| l.entries.len());
        // no create for this fd: emit_close takes the orphan path
        emit_close(Role::D5SinkSocket, fd, ClosedBy::ProbeProtocolClose);
        let entries_after = with_ledger(|l| l.entries.len());
        assert_eq!(entries_before, entries_after, "orphan close must not fabricate a ledger entry");
    }
}
