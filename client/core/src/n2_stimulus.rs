// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N2 T5a「B 断流刺激」— default-off, evidence-switch-gated outer-stream
//! stimulus (preregistration v4 §4 B column / §7 T5a, frozen decision 4).
//!
//! ## Frozen scope (what this module may and may not claim)
//!
//! M/S/R: close ONLY the current outer socket of that class (the socket the
//! armed [`crate::n2_socket_trace`] last saw complete a connect) and let its
//! OWNER (hyper/tonic, the signal link, the relay client) observe the error
//! and take its existing re-dial path. This is the **local close/error →
//! re-dial** path only: it is NOT evidence about a network-side RST, a
//! half-open connection, a blackhole or a timeout, and never substitutes for
//! the separately authorized AP-side blocking. Nothing here touches the
//! server, and Wi-Fi is never used to fake B.
//!
//! I/Wb/Ws: endpoint-directed break — the connector's [`Sink`] drops the
//! class's current selected endpoint/session so the ICE/negotiation path
//! re-runs (see [`crate::connector`]'s sink for the exact action and its
//! documented limits). D: the sink forces ONE fresh protected DNS query.
//!
//! ## Gating
//!
//! Everything is inert until the N2 evidence ticket arms the socket trace
//! (`n2_socket_trace_arm(true)`): [`observe`] records nothing and [`apply`]
//! answers `result=rejected|reason=not-armed` without a single syscall. The
//! registry is cleared on every arming change, so a disarm leaves no fd and no
//! endpoint behind.
//!
//! ## Evidence discipline
//!
//! Every call emits exactly one
//! `N2_STIMULUS|id=…|kind=…|target_fd=…|remote=…|mono_ms=…|result=…|reason=…|diag=true|is_evidence=false`
//! line: `diag=true|is_evidence=false` is the fixed tail, `result` is one of
//! `applied` / `rejected` / `no-target`, and `target_fd`/`remote` are the outer
//! socket (or selected endpoint) the class last registered — `-1`/`unknown`
//! when there is none. The line correlates with the `N2_SOCK` receipts of the
//! same class: the fd closed here is the fd those receipts report, and the
//! re-dial that follows produces a NEW `N2_SOCK` registration.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use crate::hilog;
use crate::sys;

/// The seven frozen outer classes (preregistration v4 §1).
pub const KINDS: [&str; 7] = ["M", "S", "R", "I", "Wb", "Ws", "D"];

/// What one stimulus does to its class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// M/S/R: `shutdown(2)` the class's current outer socket.
    ShutdownSocket,
    /// I/Wb/Ws: drop the class's current selected endpoint (connector sink).
    DropEndpoint,
    /// D: force one fresh protected DNS query (connector sink).
    RequeryDns,
}

impl Action {
    /// Stable token for the `reason=` field.
    pub fn reason(self) -> &'static str {
        match self {
            Self::ShutdownSocket => "shutdown-socket",
            Self::DropEndpoint => "endpoint-drop",
            Self::RequeryDns => "dns-requery",
        }
    }
}

pub fn action(kind: &str) -> Option<Action> {
    match kind {
        "M" | "S" | "R" => Some(Action::ShutdownSocket),
        "I" | "Wb" | "Ws" => Some(Action::DropEndpoint),
        "D" => Some(Action::RequeryDns),
        _ => None,
    }
}

fn static_kind(kind: &str) -> Option<&'static str> {
    KINDS.iter().copied().find(|known| *known == kind)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// One class's current outer socket. `hold` is this module's OWN duplicate:
/// `shutdown` on it affects the shared open-file description without any
/// dependence on the primary fd still being open (a closed fd NUMBER can be
/// reused, a held duplicate cannot). Sink classes record numbers only.
struct Target {
    fd: RawFd,
    remote: SocketAddr,
    hold: Option<OwnedFd>,
}

struct Registry {
    targets: HashMap<&'static str, Target>,
    /// How many outer-socket registrations each class observed (the counter
    /// increments on the SAME `first_io` call that emits the `N2_SOCK`
    /// receipt — host tests use it to prove the re-dial produced a new one).
    observed: HashMap<&'static str, u64>,
    applied: u64,
}

static STATE: OnceLock<Mutex<Registry>> = OnceLock::new();

fn state() -> MutexGuard<'static, Registry> {
    STATE
        .get_or_init(|| {
            Mutex::new(Registry { targets: HashMap::new(), observed: HashMap::new(), applied: 0 })
        })
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// The connector-side action for the classes no socket-level syscall can
/// break (I/Wb/Ws) plus D. Installed by the connector session, removed with
/// it; no connector ⇒ a Sink class answers `no-target|reason=no-sink`.
pub trait Sink: Send + Sync + 'static {
    /// Act on one class and return how many live targets were acted on
    /// (`0` = nothing to break → `no-target`).
    fn act(&self, kind: &str) -> usize;
}

static SINK: Mutex<Option<Arc<dyn Sink>>> = Mutex::new(None);

fn sink() -> Option<Arc<dyn Sink>> {
    SINK.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

pub fn install_sink(sink: Arc<dyn Sink>) {
    *SINK.lock().unwrap_or_else(PoisonError::into_inner) = Some(sink);
}

pub fn clear_sink() {
    *SINK.lock().unwrap_or_else(PoisonError::into_inner) = None;
}

pub fn sink_installed() -> bool {
    sink().is_some()
}

/// Drop every registration and the connector sink. Called on every arming
/// change (`n2_socket_trace::arm`), so `armed=false` truly leaves nothing.
pub fn reset() {
    let mut state = state();
    state.targets.clear();
    state.observed.clear();
    state.applied = 0;
    drop(state);
    clear_sink();
}

/// Register the class's current outer socket. Called from the SAME
/// [`crate::n2_socket_trace::first_io`] call site that emits the `N2_SOCK`
/// receipt; a success status only (a failed connect/dial registers nothing).
pub fn observe(kind: &str, fd: RawFd, remote: SocketAddr) {
    if !crate::n2_socket_trace::armed() || fd < 0 {
        return;
    }
    let Some(action) = action(kind) else { return };
    let Some(key) = static_kind(kind) else { return };
    match action {
        // One successful connect = one registration. The fd NUMBER may be
        // reused by a later dial, so identity comes from the held duplicate:
        // re-dup and replace on every observation.
        Action::ShutdownSocket => {
            let Ok(dup) = crate::mgmtsock::dup_socket_fd_untraced(fd) else { return };
            // SAFETY: `dup` is a fresh descriptor owned by this module alone.
            let hold = unsafe { OwnedFd::from_raw_fd(dup) };
            let mut state = state();
            bump(&mut state, key);
            state.targets.insert(key, Target { fd, remote, hold: Some(hold) });
        }
        // A send-time observation only (many per socket): record only when the
        // endpoint actually changed, and never hold an fd.
        Action::DropEndpoint | Action::RequeryDns => {
            let mut state = state();
            if let Some(current) = state.targets.get(key) {
                if current.fd == fd && current.remote == remote {
                    return;
                }
            }
            bump(&mut state, key);
            state.targets.insert(key, Target { fd, remote, hold: None });
        }
    }
}

fn bump(state: &mut Registry, key: &'static str) {
    let count = state.observed.get(key).copied().unwrap_or(0);
    state.observed.insert(key, count.saturating_add(1));
}

/// What one [`apply`] did — the emitted line plus its parsed fields, so the
/// NAPI surface can answer the shell without re-parsing the marker.
pub struct Outcome {
    pub result: &'static str,
    pub reason: String,
    pub target_fd: i32,
    pub remote: String,
    pub line: String,
}

/// Apply one stimulus: `kind` ∈ [`KINDS`], `id` = the caller's stimulus id
/// (same character set as the positive-control markers). Always emits exactly
/// one `N2_STIMULUS` line, whatever the outcome.
pub fn apply(kind: &str, id: &str) -> Outcome {
    if static_kind(kind).is_none() {
        return record(kind, id, -1, "unknown".into(), "rejected", "invalid-kind");
    }
    if !valid_id(id) {
        return record(kind, id, -1, "unknown".into(), "rejected", "invalid-id");
    }
    if !crate::n2_socket_trace::armed() {
        return record(kind, id, -1, "unknown".into(), "rejected", "not-armed");
    }
    let key = static_kind(kind).expect("kind checked");
    let action = action(kind).expect("kind checked");
    let Some((fd, remote)) = state().targets.get(key).map(|t| (t.fd, t.remote)) else {
        return record(kind, id, -1, "unknown".into(), "no-target", "no-outer-socket");
    };
    let remote_text = remote.to_string();
    match action {
        Action::ShutdownSocket => {
            // Consume the registration: ONE stimulus breaks ONE socket; the
            // next stimulus needs the re-dial's fresh registration (that is
            // what makes the three repetitions independently attributable).
            let hold = state().targets.remove(key).and_then(|t| t.hold);
            let Some(hold) = hold else {
                return record(kind, id, fd, remote_text, "no-target", "no-outer-socket");
            };
            let rc = unsafe { sys::shutdown(hold.as_raw_fd(), sys::SHUT_RDWR) };
            if rc == 0 {
                record_applied(kind, id, fd, remote_text, action)
            } else {
                let reason = format!("shutdown-failed-errno-{}", sys::errno());
                record(kind, id, fd, remote_text, "rejected", &reason)
            }
        }
        Action::DropEndpoint | Action::RequeryDns => {
            let Some(sink) = sink() else {
                return record(kind, id, fd, remote_text, "no-target", "no-sink");
            };
            if sink.act(kind) == 0 {
                return record(kind, id, fd, remote_text, "no-target", "no-live-endpoint");
            }
            record_applied(kind, id, fd, remote_text, action)
        }
    }
}

fn record_applied(kind: &str, id: &str, fd: i32, remote: String, action: Action) -> Outcome {
    {
        // One lock at a time: the guard must not be alive while another is
        // taken (std Mutex is not reentrant).
        let mut state = state();
        state.applied = state.applied.saturating_add(1);
    }
    record(kind, id, fd, remote, "applied", action.reason())
}

fn record(kind: &str, id: &str, fd: i32, remote: String, result: &'static str, reason: &str) -> Outcome {
    let line = format!(
        "N2_STIMULUS|id={}|kind={}|target_fd={}|remote={}|mono_ms={}|result={}|reason={}|diag=true|is_evidence=false",
        clean(id), clean(kind), fd, clean(&remote), sys::mono_ms(), result, clean(reason));
    hilog::emit(&line);
    Outcome { result, reason: reason.to_string(), target_fd: fd, remote, line }
}

fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-' | '_') { c } else { '_' })
        .collect()
}

// ---------------------------------------------------------------------------
// read-only inspection (host tests + the connector sink's own bookkeeping)
// ---------------------------------------------------------------------------

/// The class's current outer socket: `(target_fd, remote)`.
pub fn target(kind: &str) -> Option<(i32, SocketAddr)> {
    let key = static_kind(kind)?;
    state().targets.get(key).map(|t| (t.fd, t.remote))
}

/// How many outer-socket registrations this class observed since arming.
pub fn observed(kind: &str) -> u64 {
    static_kind(kind).and_then(|key| state().observed.get(key).copied()).unwrap_or(0)
}

/// How many stimuli ended `result=applied` since arming.
pub fn applied() -> u64 {
    state().applied
}

/// Host-test serialization: the arming flag, the registry and the sink are
/// process-global, so every lib unit test that mutates them (this module's and
/// `n2_socket_trace`'s) takes this one lock — otherwise the harness's parallel
/// test threads would arm/disarm each other.
#[cfg(test)]
pub(crate) fn test_guard() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset_all() {
        crate::n2_socket_trace::arm(false);
        reset();
    }

    #[test]
    fn every_kind_has_an_action_and_unknown_kinds_are_refused() {
        let _guard = test_guard();
        for kind in KINDS {
            assert!(action(kind).is_some(), "{kind}");
        }
        assert!(action("W").is_none());
        assert!(action("m").is_none());
        for kind in ["W", "", "MM", "d"] {
            let out = apply(kind, "stim-1");
            assert_eq!(out.result, "rejected", "{kind}");
            assert_eq!(out.reason, "invalid-kind", "{kind}");
            assert_eq!(out.target_fd, -1);
        }
    }

    #[test]
    fn unarmed_is_refused_before_any_lookup() {
        let _guard = test_guard();
        reset_all();
        // A target recorded while armed must not survive a disarm.
        let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        crate::n2_socket_trace::arm(true);
        observe("I", sock.as_raw_fd(), "127.0.0.1:3478".parse().unwrap());
        assert!(target("I").is_some());
        crate::n2_socket_trace::arm(false);
        assert!(target("I").is_none());
        let out = apply("I", "stim-x");
        assert_eq!(out.result, "rejected");
        assert_eq!(out.reason, "not-armed");
        assert!(out.line.contains("|result=rejected|reason=not-armed|diag=true|is_evidence=false"));
        reset_all();
    }

    #[test]
    fn armed_without_a_registration_is_no_target() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        for kind in KINDS {
            let out = apply(kind, "stim-y");
            assert_eq!(out.result, "no-target", "{kind}");
            assert_eq!(out.target_fd, -1);
        }
        reset_all();
    }

    #[test]
    fn sink_kinds_use_the_connector_sink_and_never_hold_an_fd() {
        let _guard = test_guard();
        reset_all();
        struct Counting(std::sync::atomic::AtomicUsize, &'static str);
        impl Sink for Counting {
            fn act(&self, kind: &str) -> usize {
                assert_eq!(kind, self.1);
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
            }
        }
        crate::n2_socket_trace::arm(true);
        let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        for kind in ["I", "Wb", "Ws", "D"] {
            install_sink(Arc::new(Counting(Default::default(), kind)));
            observe(kind, sock.as_raw_fd(), "198.51.100.7:3478".parse().unwrap());
            let out = apply(kind, "stim-sink");
            assert_eq!(out.result, "applied", "{kind}");
            assert_eq!(out.reason, action(kind).unwrap().reason());
            assert!(out.line.contains(&format!("|kind={kind}|target_fd={}|", sock.as_raw_fd())));
            assert!(out.line.ends_with("diag=true|is_evidence=false"));
        }
        // A sink that finds nothing → no-target (the fd is untouched: sink
        // classes never own one).
        struct Nothing;
        impl Sink for Nothing {
            fn act(&self, _kind: &str) -> usize { 0 }
        }
        install_sink(Arc::new(Nothing));
        assert_eq!(apply("Wb", "stim-none").result, "no-target");
        // No sink at all is its own honest token.
        clear_sink();
        let out = apply("I", "stim-nosink");
        assert_eq!(out.result, "no-target");
        assert_eq!(out.reason, "no-sink");
        reset_all();
    }

    #[test]
    fn stimulus_ids_follow_the_control_marker_rule() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        for bad in ["", "with space", "a|b", "dot.ted", &"x".repeat(65)] {
            let out = apply("M", bad);
            assert_eq!(out.result, "rejected", "{bad}");
            assert_eq!(out.reason, "invalid-id", "{bad}");
        }
        let out = apply("M", "ok_id-12");
        assert_eq!(out.result, "no-target");
        assert_eq!(out.reason, "no-outer-socket");
        reset_all();
    }
}
