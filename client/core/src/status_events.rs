//! Edge-triggered peer-status publication (0.0.34 peer-status lag fix, F1).
//!
//! WHY THIS EXISTS (device-report `peer-status-lag/REPORT.md` §0/§1): peer
//! status is computed LIVE on every read (`connector_status()` →
//! `connector::ConnectorStatus::snapshot`), but the cross-process channel to
//! the UI was PERIODIC ONLY — the VPN extension wrote `netbird-status.json` on
//! its 5 s watcher tick and the peer page read it on its own 2 s tick, i.e. up
//! to 7.0 s (mean 3.5 s) of pure timer quantization while direct↔relay edges
//! happen ~102/hour on the phone with a 2.6 s median gap. The official client
//! has no such timer: every rendered peer-status write does its own EDGE
//! DETECTION and raises a NOTIFICATION, and the notification is a
//! capacity-1 COALESCING wake (`client/internal/peer/status.go:491`/`:521`,
//! `:531`/`:568`, `:604`/`:368` → `hasStatusOrRelayedChange` `:662` →
//! `notifier.go:180-186 peerListChanged` → `:205-213 wakePeerListDelivererLocked`
//! = `select { case ch <- struct{}{}: default: }` → `:232 OnPeersListChanged`).
//!
//! This module is that notifier, in the shape this crate can carry across the
//! process boundary:
//!
//! * [`edge`] — the ONE entry point every fact site calls when a RENDERED
//!   status fact changed. It bumps the process-monotonic [`revision`] and
//!   raises at most ONE pending wake (capacity-1 token, the upstream
//!   coalescing rule): N edges inside one delivery window produce exactly ONE
//!   runtime wake, and the shell reads the CURRENT state on that wake, so the
//!   coalesced burst loses nothing. Pure counters never call it — a counter
//!   tick is not an edge (otherwise the whole mechanism degrades into the
//!   timer it replaces).
//! * [`revision`] — the cheap change detector for the (retained) heartbeat
//!   tick and the tail field of the frozen status document
//!   (`status_revision`): a reader can tell "the document changed" without
//!   diffing it.
//! * [`set_wake`] — installs the shell's wake (the crate's SECOND threadsafe
//!   function, next to `socket_supply`'s; `napi.rs::napi_connector_status_subscribe`).
//!   `None` = no shell: edges still bump the revision, no wake is raised.
//!
//! Teardown safety is the socket-supply pattern: the wake is a NON-BLOCKING
//! threadsafe-function call from whichever core thread saw the edge, the token
//! is consumed at the DELIVERY point ([`wake_consumed`], called by the NAPI
//! call_js callback — the exact place upstream's deliverer goroutine receives
//! from its channel), and the shell detaches the hook BEFORE releasing the
//! runtime handle.
//!
//! The revision is PROCESS-monotonic and is never reset: the shell reader
//! compares "changed / unchanged", so a reset could only ever lose an edge,
//! never gain one.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// The shell's wake. `true` = the wake was handed to the runtime (the shell
/// will run); `false` = it could not be (no callback installed, or the runtime
/// refused the call) — the token is then released so the NEXT edge tries again
/// instead of leaving the mechanism latched off.
pub type WakeHook = Arc<dyn Fn() -> bool + Send + Sync>;

/// The installed shell wake (`None` = no shell subscribed).
static WAKE: Mutex<Option<WakeHook>> = Mutex::new(None);
/// Process-monotonic rendered-state revision (`status_revision`).
static REVISION: AtomicU64 = AtomicU64::new(0);
/// The capacity-1 coalescing token. `true` = a wake is in flight and has not
/// been delivered yet, so further edges need no second wake.
static WAKE_PENDING: AtomicBool = AtomicBool::new(false);

/// The current [`REVISION`]. Cheap: one atomic load, no document, no lock.
pub fn revision() -> u64 {
    REVISION.load(Ordering::Acquire)
}

/// Install (`Some`) / detach (`None`) the shell's wake. Detaching is what
/// teardown does BEFORE releasing the runtime handle
/// (`napi.rs::napi_connector_status_unsubscribe`), so no wake can enter a
/// released handle. A fresh subscription starts with an EMPTY token slot: the
/// shell reads the live status itself on subscribe, and a stale token would
/// only swallow the first edge's wake.
pub fn set_wake(hook: Option<WakeHook>) {
    *WAKE.lock().expect("status wake") = hook;
    WAKE_PENDING.store(false, Ordering::Release);
}

/// Consume the coalescing token. Called ON the ArkTS thread when the runtime
/// starts delivering the wake (the NAPI call_js callback), i.e. BEFORE the
/// shell's own callback runs: an edge that lands during that callback raises a
/// fresh wake (delivered after it returns), and an edge that landed before it
/// is already part of the state the callback is about to read. Same semantics
/// as upstream's deliverer receiving from its capacity-1 channel before it
/// calls `OnPeersListChanged` (`notifier.go:215-233`).
pub fn wake_consumed() {
    WAKE_PENDING.store(false, Ordering::Release);
}

/// ONE rendered status fact changed (`reason` = a stable token, for the
/// crate's tests; it is deliberately NOT logged — edges are the hot path and
/// an evidence line per edge would spend the phone-log budget the report's
/// §3.5 measurement can revisit later).
///
/// Non-blocking by construction: the wake is a NON-BLOCKING
/// threadsafe-function call, so a busy ArkTS thread can never stall a core
/// thread that just observed a real transition. A wake that could not be
/// queued is covered by the next edge AND by the retained 5 s heartbeat.
pub fn edge(reason: &'static str) {
    REVISION.fetch_add(1, Ordering::AcqRel);
    record_thread_edge(reason);
    // Capacity-1 coalescing: the first edge of a burst wins the token and
    // raises the wake; every further edge folds into it (upstream
    // `notifier.go:205-213`). `swap` returns the PREVIOUS value.
    if WAKE_PENDING.swap(true, Ordering::AcqRel) {
        return;
    }
    // Cloned out of the lock: the hook runs the threadsafe-function call, and
    // holding this mutex across it would serialize every edge on it (the
    // socket-supply wake holds its own handle mutex across the call, which is
    // correct for a per-request wake; a coalescing edge does not need that and
    // must never block a core thread behind a delivery).
    let hook = { WAKE.lock().expect("status wake").clone() };
    let queued = hook.map(|hook| hook()).unwrap_or(false);
    if !queued {
        // No shell (or the runtime refused): release the token so the next
        // edge raises a real wake the moment a shell is there.
        WAKE_PENDING.store(false, Ordering::Release);
    }
}

// ---------------------------------------------------------------------------
// test-only per-thread ledger
//
// The crate's unit tests run in PARALLEL threads inside one test binary, so an
// EXACT per-event assertion (`this transition bumped the revision exactly
// once`) cannot be made against the process-wide counter: another test's
// transition would inflate the delta. Every test drives its own fact change
// synchronously on its OWN thread, so the ledger below (bumped by `edge`, only
// in `cfg(test)` builds) makes the assertions exact and race-free. It exists
// only in the test binary — the shipped .so carries no ledger at all.
// ---------------------------------------------------------------------------

#[cfg(test)]
thread_local! {
    static THREAD_EDGES: std::cell::Cell<u64> = std::cell::Cell::new(0);
    static THREAD_LAST_REASON: std::cell::Cell<&'static str> = std::cell::Cell::new("");
}

#[cfg(test)]
fn record_thread_edge(reason: &'static str) {
    THREAD_EDGES.with(|c| c.set(c.get() + 1));
    THREAD_LAST_REASON.with(|c| c.set(reason));
}

#[cfg(not(test))]
#[inline(always)]
fn record_thread_edge(_reason: &'static str) {}

/// Test-only: edges raised by THIS thread (see the ledger note above).
#[cfg(test)]
pub fn thread_edges() -> u64 {
    THREAD_EDGES.with(|c| c.get())
}

/// Test-only: the reason token of this thread's last edge (`""` = none).
#[cfg(test)]
pub fn thread_last_reason() -> &'static str {
    THREAD_LAST_REASON.with(|c| c.get())
}

/// Test-only: the crate's threadsafe-function surface (both wakes, one
/// process-wide hook each) is ONE shared test resource, and the host stubs'
/// TLS ledger belongs to whichever test drives it. Tests that install a wake
/// hook take THIS lock — the same one the socket-supply wake test takes.
#[cfg(test)]
pub fn wake_test_lock() -> std::sync::MutexGuard<'static, ()> {
    crate::socket_supply::notify_test_lock()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    // The installed hook is process-wide, so a CONCURRENT test's edge (every
    // fact-site test raises edges) runs through it too. Counting in a
    // THREAD-LOCAL keeps this thread's assertion exact: a foreign edge's call
    // lands in the foreign thread's cell.
    thread_local! {
        static HOOK_CALLS: Cell<u64> = Cell::new(0);
    }

    fn counting_hook(queued: bool) -> WakeHook {
        Arc::new(move || {
            HOOK_CALLS.with(|c| c.set(c.get() + 1));
            queued
        })
    }

    fn hook_calls() -> u64 {
        HOOK_CALLS.with(|c| c.get())
    }

    /// A concurrent test may have taken the capacity-1 token microseconds
    /// before this thread's edge; the token then legitimately coalesces OUR
    /// edge away (that is the mechanism working). Re-arm and retry so the
    /// assertion is about the wake, not about the scheduler.
    fn edge_waking_this_thread(reason: &'static str) -> u64 {
        for _ in 0..256 {
            wake_consumed();
            let before = hook_calls();
            edge(reason);
            let raised = hook_calls() - before;
            if raised == 1 {
                return raised;
            }
            assert_eq!(raised, 0, "capacity-1: one edge can never raise two wakes");
        }
        panic!("no wake in 256 attempts — the wake machinery is latched off");
    }

    #[test]
    fn edge_bumps_the_revision_and_raises_one_wake() {
        let _serial = wake_test_lock();
        set_wake(Some(counting_hook(true)));
        let before_edges = thread_edges();
        let before_rev = revision();
        assert_eq!(edge_waking_this_thread("unit-a"), 1, "one edge ⇒ one wake");
        assert_eq!(thread_edges(), before_edges + 1, "exactly one edge on this thread");
        assert_eq!(thread_last_reason(), "unit-a");
        assert!(revision() > before_rev, "revision is monotonic (never reset)");
        set_wake(None);
    }

    #[test]
    fn a_burst_coalesces_into_exactly_one_pending_wake() {
        let _serial = wake_test_lock();
        set_wake(Some(counting_hook(true)));
        for _ in 0..64 {
            wake_consumed();
            let before = hook_calls();
            let before_edges = thread_edges();
            let before_rev = revision();
            // 20 edges with NO delivery in between (the ArkTS loop has not
            // run yet): the first takes the token, the rest fold into it.
            for _ in 0..20 {
                edge("burst");
            }
            let raised = hook_calls() - before;
            if raised == 1 {
                assert!(revision() > before_rev, "every edge still bumps the revision");
                assert_eq!(thread_edges(), before_edges + 20, "every edge is still counted");
                set_wake(None);
                return;
            }
            assert_eq!(raised, 0, "a burst can never raise more than one wake");
        }
        panic!("no wake in 64 bursts — the wake machinery is latched off");
    }

    #[test]
    fn a_refused_wake_releases_the_token_for_the_next_edge() {
        let _serial = wake_test_lock();
        // Models the runtime refusing the call (closing runtime): the token
        // must not stay latched, or no later edge could ever wake the shell.
        set_wake(Some(counting_hook(false)));
        for _ in 0..256 {
            wake_consumed();
            let before = hook_calls();
            edge("refused-a");
            if hook_calls() - before == 0 {
                continue; // a concurrent test owned the token: retry
            }
            edge("refused-b");
            let raised = hook_calls() - before;
            if raised == 2 {
                set_wake(None);
                return;
            }
            assert_eq!(raised, 1, "a refused wake must not consume a second edge");
        }
        panic!("no wake pair — the wake machinery is latched off");
    }

    #[test]
    fn no_shell_means_no_wake_but_the_revision_still_moves() {
        let _serial = wake_test_lock();
        set_wake(None);
        let before = revision();
        edge("no-shell");
        assert!(revision() > before);
    }
}
