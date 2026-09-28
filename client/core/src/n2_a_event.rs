// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N2 前置项 **P4** — A 列生产事件的触发原语 (preregistration v4.1 §4
//! 「A 列生产事件的操作性定义（v4.1 澄清，不改判据）」, `docs/n2-device-evidence-preregistration-draft-v2-20260928.md:92-119`).
//!
//! ## Frozen scope (what one trigger may and may not do)
//!
//! The general principle (`:98`) is that a trigger may only move the SEND
//! TIME of one real production frame — it must never invent a frame kind the
//! production client would not send, and it must never create a server-side
//! state change:
//!
//! * `M` — ONE **read-only idempotent** real management RPC
//!   (`GetServerKey`, `grpc.rs:430`; the login flow already calls it) **on the
//!   EXISTING management gRPC client**, i.e. the same connection/HTTP-2 channel
//!   and the same pre-create protected fd/dup. NO new connection, NO second
//!   `Sync` stream (the server may replace the original stream / re-push the
//!   network map → recreate risk).
//! * `R` — NO active send at all: arm a watcher for the next
//!   **server-driven** `HealthCheck` and record the production echo
//!   (`relay_client.rs:2117-2120`). Timed out after [`R_WAIT_MS`] (the frozen
//!   bound ≤ 90 s) → `result=timeout`, never a self-made frame.
//! * `D` — the Q1 rule (`:107`): the **natural** first protected query of the
//!   session wins; only when this session has NO earlier `n2_stimulus` and NO
//!   earlier A event may the existing requery primitive fire ONCE
//!   (`force_protected_dns_requery`). Otherwise `rejected|reason=prior-stimulus`.
//! * `S` — **not implemented**. `signal.rs`'s self-addressed HEARTBEAT has no
//!   caller yet (the receive watchdog is N5/P8) and the peer-directed frames
//!   have peer-side side effects, so there is no side-effect-free production
//!   send to trigger: every `S` request is answered
//!   `rejected|reason=no-side-effect-free-production-send`.
//!
//! ## Gating
//!
//! Inert until the N2 evidence ticket arms the socket trace
//! (`n2_socket_trace_arm(true)`): every request answers
//! `rejected|reason=not-armed` without a single syscall or RPC. A disarm
//! ([`reset`], called from [`crate::n2_socket_trace::arm`]) drops the session
//! counters and any armed R watch.
//!
//! ## Evidence discipline
//!
//! Exactly one
//! `N2_A_EVENT|id=…|kind=…|fd=…|dup_fd=…|remote=…|result=…|reason=…|detail=…|mono_ms=…|diag=true|is_evidence=false`
//! line per request (plus the later `observed`/`timeout` completion of an
//! armed R watch). `fd` is the shell ORIGINAL and `dup_fd` the core-owned
//! descriptor of the live outer socket — the same pair the `N2_SOCK` receipts
//! report, taken from the armed trace itself, so an A event can never be
//! attributed to a socket no `N2_SOCK` line describes.

use std::os::fd::RawFd;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use crate::hilog;
use crate::sys;

/// The four kinds this primitive accepts. `M`/`R`/`D` are triggers; `S` is a
/// fixed refusal (see the module docs). The closed list mirrors the frozen
/// A-column rows that have a side-effect-free production send.
pub const KINDS: [&str; 4] = ["M", "R", "D", "S"];

/// The frozen R wait bound (`§4` R row: "等待上限写入 AUTH（建议 ≤ 90 s…）").
pub const R_WAIT_MS: u64 = 90_000;

/// The bounded synchronous `M` round trip. The shell thread blocks on one
/// `GetServerKey`; the client's own per-RPC deadline is 10 s, so this shorter
/// box bounds the operator-visible wait without inventing a new timeout for
/// the production RPC.
pub const M_RPC_WAIT_MS: u64 = 5_000;

/// The fd/dup identity chain of the live outer socket the trigger reuses:
/// `fd` = shell ORIGINAL, `dup_fd` = core-owned descriptor (exactly the
/// `N2_SOCK` convention).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FdPair {
    pub fd: RawFd,
    pub dup_fd: RawFd,
    pub remote: String,
}

impl FdPair {
    pub fn unknown() -> Self {
        Self { fd: -1, dup_fd: -1, remote: "unknown".into() }
    }
}

/// The connector-side seam a trigger needs. Installed with the connector
/// session and removed with it; no connector ⇒ `error|reason=no-connector`
/// (never a fabricated send).
pub trait Trigger: Send + Sync + 'static {
    /// ONE real `GetServerKey` on the EXISTING management client (same
    /// connection). `Err(token)` is reported verbatim.
    fn management_round_trip(&self) -> Result<(), String>;
    /// The existing D requery primitive (one fresh protected query).
    fn dns_requery(&self) -> Result<(), String>;
    /// Arm the [`R_WAIT_MS`] timeout for the watch `id` (the session's own
    /// runtime sleeps and then calls [`r_watch_timeout`] with the SAME `seq`,
    /// so a stale timer from a previous watch can never time out a later one).
    fn arm_healthcheck_watch(&self, id: &str, wait_ms: u64, seq: u64) -> Result<(), String>;
}

static TRIGGER: Mutex<Option<Arc<dyn Trigger>>> = Mutex::new(None);

fn trigger() -> Option<Arc<dyn Trigger>> {
    TRIGGER.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

pub fn install_trigger(trigger: Arc<dyn Trigger>) {
    *TRIGGER.lock().unwrap_or_else(PoisonError::into_inner) = Some(trigger);
}

pub fn clear_trigger() {
    *TRIGGER.lock().unwrap_or_else(PoisonError::into_inner) = None;
}

pub fn trigger_installed() -> bool {
    trigger().is_some()
}

/// One armed R watch: the request id, its monotonic arming sequence (so a
/// stale timeout timer can never complete a LATER watch) and the relay fd/dup
/// identity captured at arm time (re-resolved at the observation if a newer
/// session registered).
struct RWatch {
    id: String,
    seq: u64,
    pair: FdPair,
}

#[derive(Default)]
struct State {
    /// A events this arming actually dispatched (`sent`/`requery`/`armed`) —
    /// the D row's "本会话尚无任何 A 事件" (Q1) input.
    a_events: u64,
    r_watch: Option<RWatch>,
    /// Monotonic R-watch arming sequence (never reused inside one arming).
    r_seq: u64,
    last_line: Option<String>,
    last_result: Option<&'static str>,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

fn state() -> MutexGuard<'static, State> {
    STATE.get_or_init(|| Mutex::new(State::default())).lock().unwrap_or_else(PoisonError::into_inner)
}

/// Drop every session counter and the armed R watch. Called on every arming
/// change (`n2_socket_trace::arm`), so `armed=false` truly leaves nothing.
pub fn reset() {
    *state() = State::default();
    clear_trigger();
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn static_kind(kind: &str) -> Option<&'static str> {
    KINDS.iter().copied().find(|known| *known == kind)
}

fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-' | '_') { c } else { '_' })
        .collect()
}

/// What one [`apply`] (or one R completion) did — the emitted line plus its
/// parsed fields, so the NAPI surface can answer the shell without re-parsing.
pub struct Outcome {
    pub result: &'static str,
    pub reason: String,
    pub kind: String,
    pub fd: RawFd,
    pub dup_fd: RawFd,
    pub line: String,
}

/// The live outer socket of a class, as the armed trace knows it: the same
/// `(dup_fd, original_fd, remote)` the `N2_SOCK` receipts carry.
fn live_pair(kind: &str) -> FdPair {
    match crate::n2_socket_trace::live_of(kind) {
        Some((dup_fd, fd, remote)) => {
            FdPair { fd, dup_fd, remote: remote.to_string() }
        }
        None => FdPair::unknown(),
    }
}

fn record(kind: &str, id: &str, pair: &FdPair, result: &'static str, reason: &str,
    detail: &str) -> Outcome {
    // `detail` is built internally (never operator text), so it is emitted
    // verbatim — the `wait_ms=90000` shape is part of the documented line.
    let line = format!(
        "N2_A_EVENT|id={}|kind={}|fd={}|dup_fd={}|remote={}|result={}|reason={}|detail={}|mono_ms={}|diag=true|is_evidence=false",
        clean(id), clean(kind), pair.fd, pair.dup_fd, clean(&pair.remote), result,
        clean(reason), detail, sys::mono_ms());
    hilog::emit(&line);
    let mut state = state();
    state.last_line = Some(line.clone());
    state.last_result = Some(result);
    Outcome { result, reason: reason.to_string(), kind: kind.to_string(),
        fd: pair.fd, dup_fd: pair.dup_fd, line }
}

/// Apply one A-column trigger. Always emits exactly one `N2_A_EVENT` line,
/// whatever the outcome (the R watch's `observed`/`timeout` completion emits a
/// second line later).
pub fn apply(kind: &str, id: &str) -> Outcome {
    if !crate::n2_socket_trace::armed() {
        return record(kind, id, &FdPair::unknown(), "rejected", "not-armed", "gate");
    }
    if static_kind(kind).is_none() {
        return record(kind, id, &FdPair::unknown(), "rejected", "invalid-kind", "gate");
    }
    // `S` has no side-effect-free production send (frozen §4 S row (iii)):
    // refused by name whatever else the request carries.
    if kind == "S" {
        return record(kind, id, &FdPair::unknown(), "rejected",
            "no-side-effect-free-production-send", "no-production-frame");
    }
    if !valid_id(id) {
        return record(kind, id, &FdPair::unknown(), "rejected", "invalid-id", "gate");
    }
    // D's Q1 gate is a frozen refusal and comes before any seam lookup.
    if kind == "D" {
        let stimulus_seen = crate::n2_stimulus::applied() > 0;
        let a_seen = state().a_events > 0;
        if stimulus_seen || a_seen {
            return record(kind, id, &FdPair::unknown(), "rejected", "prior-stimulus",
                if stimulus_seen { "prior-n2-stimulus" } else { "prior-a-event" });
        }
    }
    let Some(trigger) = trigger() else {
        return record(kind, id, &FdPair::unknown(), "error", "no-connector", "no-trigger-seam");
    };
    match kind {
        "M" => {
            let pair = live_pair("M");
            if pair.dup_fd < 0 {
                // No armed M registration: there is no pre-create protected
                // management fd/dup this event could be attributed to.
                return record(kind, id, &pair, "error", "no-live-management-connection",
                    "no-n2-sock-registration");
            }
            match trigger.management_round_trip() {
                Ok(()) => {
                    // std Mutex is NOT reentrant: one guard per statement.
                    {
                        let mut state = state();
                        state.a_events = state.a_events.saturating_add(1);
                    }
                    record(kind, id, &pair, "sent", "ok", "get-server-key")
                }
                Err(reason) => record(kind, id, &pair, "error", &reason, "get-server-key-failed"),
            }
        }
        "D" => {
            match trigger.dns_requery() {
                Ok(()) => {
                    {
                        let mut state = state();
                        state.a_events = state.a_events.saturating_add(1);
                    }
                    // The authoritative device record of the D event is the
                    // `N2_DNS` receipt the requery itself emits (fd/dup/query
                    // id/same-ID response); the trigger line records the
                    // dispatch, so no fd is claimed here.
                    record(kind, id, &FdPair::unknown(), "requery", "ok", "receipt-in-N2_DNS")
                }
                Err(reason) => record(kind, id, &FdPair::unknown(), "error", &reason,
                    "requery-refused"),
            }
        }
        "R" => {
            // std Mutex is NOT reentrant: read the flag, release, then record.
            let watch_open = state().r_watch.is_some();
            if watch_open {
                return record(kind, id, &FdPair::unknown(), "rejected",
                    "watch-already-armed", "one-watch-per-session");
            }
            let pair = live_pair("R");
            let seq = {
                let mut state = state();
                state.r_seq = state.r_seq.saturating_add(1);
                state.r_seq
            };
            if let Err(reason) = trigger.arm_healthcheck_watch(id, R_WAIT_MS, seq) {
                return record(kind, id, &pair, "error", &reason, "watch-arm-refused");
            }
            {
                let mut state = state();
                state.a_events = state.a_events.saturating_add(1);
                state.r_watch = Some(RWatch { id: id.to_string(), seq, pair: pair.clone() });
            }            record(kind, id, &pair, "armed", "healthcheck-watch", &format!("wait_ms={R_WAIT_MS}"))
        }
        _ => record(kind, id, &FdPair::unknown(), "rejected", "invalid-kind", "gate"),
    }
}

/// The relay client observed a server-driven `HealthCheck` and the production
/// code answered it (`relay_client.rs` EchoHealthCheck branch). Emits the
/// frozen `observed` completion ONCE for the armed watch.
pub fn healthcheck_answered() -> Option<Outcome> {
    if !crate::n2_socket_trace::armed() {
        return None;
    }
    let watch = state().r_watch.take()?;
    let pair = {
        let live = live_pair("R");
        if live.dup_fd >= 0 { live } else { watch.pair }
    };
    Some(record("R", &watch.id, &pair, "observed", "server-healthcheck-answered",
        "production-echo"))
}

/// The [`R_WAIT_MS`] box expired with no observation. Emits the frozen
/// `timeout` completion; a stale `(id, seq)` (already observed/replaced) is a
/// no-op, so a timer armed for an earlier watch can never complete a later one.
pub fn r_watch_timeout(id: &str, seq: u64) -> Option<Outcome> {
    if !crate::n2_socket_trace::armed() {
        return None;
    }
    let watch = {
        let mut state = state();
        match state.r_watch.as_ref() {
            Some(watch) if watch.id == id && watch.seq == seq => state.r_watch.take(),
            _ => None,
        }
    }?;
    Some(record("R", &watch.id, &watch.pair, "timeout", "healthcheck-wait-exceeded",
        &format!("wait_ms={R_WAIT_MS}")))
}

// ---------------------------------------------------------------------------
// read-only inspection (host tests + the campaign judge's join keys)
// ---------------------------------------------------------------------------

/// A events this arming dispatched.
pub fn a_events() -> u64 {
    state().a_events
}

/// Whether an R watch is armed and not yet completed.
pub fn r_watch_open() -> bool {
    state().r_watch.is_some()
}

/// The last `N2_A_EVENT` line this process emitted (`None` = none yet).
pub fn last_line() -> Option<String> {
    state().last_line.clone()
}

/// The last result token this process reported (host-test view of
/// `observed`/`timeout` completions whose line is emitted from the relay path).
pub fn last_result() -> Option<&'static str> {
    state().last_result
}

/// Host-test serialization: the arming flag, this module's state and the
/// connector trigger are process-global, so every lib unit test that mutates
/// them takes the SAME lock the other N2 modules use (`n2_stimulus`'s) —
/// otherwise the harness's parallel test threads would arm/disarm each other.
#[cfg(test)]
pub(crate) fn test_guard() -> MutexGuard<'static, ()> {
    crate::n2_stimulus::test_guard()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Counting {
        rpc: AtomicUsize,
        requery: AtomicUsize,
        watch: AtomicUsize,
        fail: Mutex<Option<&'static str>>,
        watches: Mutex<Vec<(String, u64, u64)>>,
    }

    impl Trigger for Counting {
        fn management_round_trip(&self) -> Result<(), String> {
            self.rpc.fetch_add(1, Ordering::SeqCst);
            match *self.fail.lock().unwrap() {
                Some(token) => Err(token.to_string()),
                None => Ok(()),
            }
        }
        fn dns_requery(&self) -> Result<(), String> {
            self.requery.fetch_add(1, Ordering::SeqCst);
            match *self.fail.lock().unwrap() {
                Some(token) => Err(token.to_string()),
                None => Ok(()),
            }
        }
        fn arm_healthcheck_watch(&self, id: &str, wait_ms: u64, seq: u64) -> Result<(), String> {
            self.watch.fetch_add(1, Ordering::SeqCst);
            self.watches.lock().unwrap().push((id.to_string(), wait_ms, seq));
            Ok(())
        }
    }

    fn reset_all() {
        crate::n2_socket_trace::arm(false);
        reset();
    }

    /// Register a live outer socket of `kind` on a real host UDP fd so the
    /// module's `live_pair` lookup has the same identity chain `N2_SOCK` uses.
    fn register(kind: &str, fd: RawFd) {
        crate::n2_socket_trace::protect(fd, kind, "req-1", 0, 31, 5, true);
        crate::n2_socket_trace::io_begin(fd, "192.0.2.9:443".parse().unwrap());
        crate::n2_socket_trace::first_io(fd, "192.0.2.9:443".parse().unwrap(), "connected", "connect");
    }

    #[test]
    fn unarmed_is_refused_before_any_trigger_call() {
        let _guard = test_guard();
        reset_all();
        let counting = Arc::new(Counting::default());
        install_trigger(counting.clone());
        for kind in ["M", "R", "D", "S"] {
            let out = apply(kind, "a-1");
            assert_eq!(out.result, "rejected", "{kind}");
            assert_eq!(out.reason, "not-armed", "{kind}");
            assert!(out.line.ends_with("diag=true|is_evidence=false"));
        }
        assert_eq!(counting.rpc.load(Ordering::SeqCst), 0);
        assert_eq!(counting.requery.load(Ordering::SeqCst), 0);
        assert_eq!(counting.watch.load(Ordering::SeqCst), 0);
        reset_all();
    }

    #[test]
    fn kind_s_is_always_refused_with_its_named_reason() {
        let _guard = test_guard();
        reset_all();
        install_trigger(Arc::new(Counting::default()));
        crate::n2_socket_trace::arm(true);
        let out = apply("S", "a-S-1");
        assert_eq!(out.result, "rejected");
        assert_eq!(out.reason, "no-side-effect-free-production-send");
        assert!(out.line.contains("|kind=S|fd=-1|dup_fd=-1|"));
        // even with an id the marker rule would refuse, `S` keeps its own name
        assert_eq!(apply("S", "a b").reason, "no-side-effect-free-production-send");
        assert_eq!(apply("Z", "a-Z-1").reason, "invalid-kind");
        assert_eq!(apply("M", "with space").reason, "invalid-id");
        reset_all();
    }

    #[test]
    fn m_reuses_the_live_socket_and_never_invents_one() {
        let _guard = test_guard();
        reset_all();
        // Production order: the ticket arms the trace, then the connector
        // installs its trigger seam (arming clears any previous seam).
        crate::n2_socket_trace::arm(true);
        let counting = Arc::new(Counting::default());
        install_trigger(counting.clone());
        // armed but no M registration: the event cannot be attributed
        let none = apply("M", "a-M-0");
        assert_eq!(none.result, "error");
        assert_eq!(none.reason, "no-live-management-connection");
        assert_eq!(counting.rpc.load(Ordering::SeqCst), 0, "no RPC without a live fd");
        // a real registered M socket: fd/dup come FROM the trace
        let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let fd = sock.as_raw_fd();
        register("M", fd);
        let some = apply("M", "a-M-1");
        assert_eq!(some.result, "sent");
        assert_eq!(some.fd, fd);
        assert_eq!(some.dup_fd, fd);
        assert!(some.line.contains(&format!("|fd={fd}|dup_fd={fd}|remote=192.0.2.9:443|")));
        assert_eq!(counting.rpc.load(Ordering::SeqCst), 1);
        assert_eq!(a_events(), 1);
        // a failing RPC is reported as an error, never as sent
        *counting.fail.lock().unwrap() = Some("grpc-unavailable");
        let failed = apply("M", "a-M-2");
        assert_eq!(failed.result, "error");
        assert_eq!(failed.reason, "grpc-unavailable");
        assert_eq!(a_events(), 1, "a failed RPC is not an A event");
        reset_all();
    }

    #[test]
    fn d_refuses_after_any_prior_stimulus_or_a_event() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        let counting = Arc::new(Counting::default());
        install_trigger(counting.clone());
        // no prior anything: the requery primitive fires once
        let first = apply("D", "a-D-1");
        assert_eq!(first.result, "requery");
        assert_eq!(first.reason, "ok");
        assert_eq!(counting.requery.load(Ordering::SeqCst), 1);
        assert!(first.line.contains("|detail=receipt-in-N2_DNS|"));
        // a prior A EVENT refuses the next D
        assert_eq!(apply("D", "a-D-2").reason, "prior-stimulus");
        assert_eq!(counting.requery.load(Ordering::SeqCst), 1);
        reset_all();
    }

    #[test]
    fn d_prior_stimulus_is_named_from_the_stimulus_counter() {
        let _guard = test_guard();
        reset_all();
        install_trigger(Arc::new(Counting::default()));
        crate::n2_socket_trace::arm(true);
        struct OneSink;
        impl crate::n2_stimulus::Sink for OneSink {
            fn act(&self, _kind: &str) -> usize { 1 }
        }
        crate::n2_stimulus::install_sink(Arc::new(OneSink));
        let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        crate::n2_stimulus::observe("D", sock.as_raw_fd(), "192.0.2.9:53".parse().unwrap());
        assert_eq!(crate::n2_stimulus::apply("D", "b-D-1").result, "applied");
        let out = apply("D", "a-D-1");
        assert_eq!(out.result, "rejected");
        assert_eq!(out.reason, "prior-stimulus");
        assert!(out.line.contains("|detail=prior-n2-stimulus|"), "{}", out.line);
        reset_all();
    }

    #[test]
    fn r_arms_one_watch_and_completes_observed_or_timeout() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        let counting = Arc::new(Counting::default());
        install_trigger(counting.clone());
        let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let fd = sock.as_raw_fd();
        register("R", fd);
        let armed = apply("R", "a-R-1");
        assert_eq!(armed.result, "armed");
        assert_eq!(armed.reason, "healthcheck-watch");
        assert!(armed.line.contains("|detail=wait_ms=90000|"), "{}", armed.line);
        assert!(r_watch_open());
        assert_eq!(counting.watches.lock().unwrap().as_slice(),
            [("a-R-1".to_string(), R_WAIT_MS, 1)]);
        // a second watch while one is armed is refused
        assert_eq!(apply("R", "a-R-2").reason, "watch-already-armed");
        // the production echo completes it EXACTLY once
        let observed = healthcheck_answered().expect("armed watch");
        assert_eq!(observed.result, "observed");
        assert_eq!(observed.reason, "server-healthcheck-answered");
        assert_eq!(observed.fd, fd);
        assert!(healthcheck_answered().is_none(), "one observation per watch");
        assert!(!r_watch_open());
        // timeout path: the frozen token, and a stale (id, seq) is a no-op
        assert_eq!(apply("R", "a-R-3").result, "armed");
        let seq = counting.watches.lock().unwrap().last().expect("watch").2;
        assert!(r_watch_timeout("a-R-other", seq).is_none());
        assert!(r_watch_timeout("a-R-3", seq + 99).is_none(), "stale timer");
        let timed_out = r_watch_timeout("a-R-3", seq).expect("armed watch");
        assert_eq!(timed_out.result, "timeout");
        assert_eq!(timed_out.reason, "healthcheck-wait-exceeded");
        assert!(timed_out.line.contains("|detail=wait_ms=90000|"), "{}", timed_out.line);
        assert!(r_watch_timeout("a-R-3", seq).is_none());
        reset_all();
    }

    #[test]
    fn no_trigger_seam_is_an_honest_error_and_disarm_leaves_nothing() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        clear_trigger();
        assert_eq!(apply("M", "a-M-1").reason, "no-connector");
        assert_eq!(apply("D", "a-D-1").reason, "no-connector");
        clear_trigger();
        // arming change drops counters, any armed watch and the last line
        install_trigger(Arc::new(Counting::default()));
        assert_eq!(apply("R", "a-R-1").result, "armed");
        assert!(last_line().is_some());
        crate::n2_socket_trace::arm(false);
        assert!(!r_watch_open());
        assert_eq!(a_events(), 0);
        assert!(last_line().is_none(), "disarm leaves no line behind");
        crate::n2_socket_trace::arm(true);
        assert_eq!(a_events(), 0);
        assert!(last_line().is_none());
        reset_all();
    }
}