//! Connection state machine (R3 skeleton): explicit transition table over
//! Disconnected / Connecting / Connected / Reconnecting / Failed / Closed.
//!
//! Pure logic, zero I/O, zero channels: a [`StateMachine`] instance is just
//! `state + last transition`. It never touches sockets, the fd ledger, or the
//! hilog — the connection owner drives events and acts on the result.
//!
//! ## Concurrency policy
//! The machine has NO interior mutability, NO global/static state and NO
//! locks of its own; every mutation goes through `&mut self`
//! ([`StateMachine::transition`]). That makes it reentrancy-trivially safe
//! (no reentrancy is possible: methods do not call out) and `Send + Sync` by
//! construction. Concurrent access = wrap it in `std::sync::Mutex` (or
//! `RwLock` for the read-mostly case) at the owner; the unit test pins the
//! auto-trait. Do NOT share one machine across threads without a lock.
//!
//! ## Event semantics
//! - `Connect`        — user requests a connection (from Disconnected or Failed)
//! - `Established`    — handshake completed (from Connecting or Reconnecting)
//! - `Lost`           — peer/connection lost; an initial-connect timeout is
//!                      reported as `Lost` too (the attempt died before it
//!                      established) so both cases share the retry loop
//! - `RetryExhausted` — reconnect budget spent (from Reconnecting only)
//! - `Disconnect`     — user-driven stop (from Connecting/Connected/Reconnecting)
//! - `FatalError`     — unrecoverable local error (same origins as Disconnect)
//! - `Close`          — final teardown of the machine; valid from every state
//!                      except Closed; Closed is terminal and accepts nothing

/// Connection state. Closed is terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConnState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Failed,
    Closed,
}

impl ConnState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConnState::Disconnected => "disconnected",
            ConnState::Connecting => "connecting",
            ConnState::Connected => "connected",
            ConnState::Reconnecting => "reconnecting",
            ConnState::Failed => "failed",
            ConnState::Closed => "closed",
        }
    }
}

/// Driver event. Anything not listed in [`next_state`] is an illegal
/// transition and returns [`StateError::InvalidTransition`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConnEvent {
    Connect,
    Established,
    Lost,
    RetryExhausted,
    Disconnect,
    FatalError,
    Close,
}

impl ConnEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConnEvent::Connect => "connect",
            ConnEvent::Established => "established",
            ConnEvent::Lost => "lost",
            ConnEvent::RetryExhausted => "retry_exhausted",
            ConnEvent::Disconnect => "disconnect",
            ConnEvent::FatalError => "fatal_error",
            ConnEvent::Close => "close",
        }
    }
}

/// One completed transition: origin, triggering event, destination and a
/// stable human-readable reason (this is the "最近一次转移原因" query).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition {
    pub from: ConnState,
    pub event: ConnEvent,
    pub to: ConnState,
    pub reason: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateError {
    /// The (current state, event) pair is not in the transition table.
    InvalidTransition { from: ConnState, event: ConnEvent },
}

impl core::fmt::Display for StateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StateError::InvalidTransition { from, event } => write!(
                f,
                "illegal transition: event '{}' not allowed in state '{}'",
                event.as_str(),
                from.as_str()
            ),
        }
    }
}

impl std::error::Error for StateError {}

/// THE transition table — the single source of truth. `match (from, event)`
/// arms ARE the table; every unlisted pair falls through to the error arm.
/// 18 legal (state, event) pairs out of 6 x 7 = 42.
pub fn next_state(from: ConnState, event: ConnEvent) -> Result<(ConnState, &'static str), StateError> {
    match (from, event) {
        // --- start / restart -----------------------------------------------
        (ConnState::Disconnected, ConnEvent::Connect) => {
            Ok((ConnState::Connecting, "connect requested"))
        }
        (ConnState::Failed, ConnEvent::Connect) => {
            Ok((ConnState::Connecting, "retry after failure"))
        }
        // --- establishment ---------------------------------------------------
        (ConnState::Connecting, ConnEvent::Established) => {
            Ok((ConnState::Connected, "handshake established"))
        }
        (ConnState::Reconnecting, ConnEvent::Established) => {
            Ok((ConnState::Connected, "reconnect established"))
        }
        // --- loss / retry loop ------------------------------------------------
        (ConnState::Connecting, ConnEvent::Lost) => {
            Ok((ConnState::Reconnecting, "handshake timed out; retry scheduled"))
        }
        (ConnState::Connected, ConnEvent::Lost) => {
            Ok((ConnState::Reconnecting, "connection lost; reconnect scheduled"))
        }
        (ConnState::Reconnecting, ConnEvent::RetryExhausted) => {
            Ok((ConnState::Failed, "reconnect attempts exhausted"))
        }
        // --- user stop / fatal -------------------------------------------------
        (ConnState::Connecting, ConnEvent::Disconnect)
        | (ConnState::Connected, ConnEvent::Disconnect)
        | (ConnState::Reconnecting, ConnEvent::Disconnect) => {
            Ok((ConnState::Disconnected, "disconnect requested"))
        }
        (ConnState::Connecting, ConnEvent::FatalError)
        | (ConnState::Connected, ConnEvent::FatalError)
        | (ConnState::Reconnecting, ConnEvent::FatalError) => {
            Ok((ConnState::Failed, "fatal error"))
        }
        // --- final teardown (Closed is terminal: no arm accepts anything) ------
        (ConnState::Disconnected, ConnEvent::Close) => {
            Ok((ConnState::Closed, "closed while disconnected"))
        }
        (ConnState::Connecting, ConnEvent::Close)
        | (ConnState::Connected, ConnEvent::Close)
        | (ConnState::Reconnecting, ConnEvent::Close) => {
            Ok((ConnState::Closed, "closed while active"))
        }
        (ConnState::Failed, ConnEvent::Close) => {
            Ok((ConnState::Closed, "closed after failure"))
        }
        (from, event) => Err(StateError::InvalidTransition { from, event }),
    }
}

/// The machine itself: current state + last completed transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateMachine {
    state: ConnState,
    last: Option<Transition>,
}

impl StateMachine {
    pub fn new() -> Self {
        StateMachine {
            state: ConnState::Disconnected,
            last: None,
        }
    }

    /// Current state.
    pub fn state(&self) -> ConnState {
        self.state
    }

    /// Last completed transition, `None` on a fresh machine.
    pub fn last_transition(&self) -> Option<Transition> {
        self.last
    }

    /// Apply `event`. Illegal transitions leave the machine untouched and
    /// return [`StateError::InvalidTransition`].
    pub fn transition(&mut self, event: ConnEvent) -> Result<ConnState, StateError> {
        let (to, reason) = next_state(self.state, event)?;
        self.last = Some(Transition {
            from: self.state,
            event,
            to,
            reason,
        });
        self.state = to;
        Ok(to)
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_send_sync<T: Send + Sync>() {}
    #[test]
    fn machine_is_send_sync() {
        _assert_send_sync::<StateMachine>();
        _assert_send_sync::<ConnState>();
    }

    #[test]
    fn fresh_machine_is_disconnected_without_reason() {
        let m = StateMachine::new();
        assert_eq!(m.state(), ConnState::Disconnected);
        assert_eq!(m.last_transition(), None);
    }

    #[test]
    fn happy_path_connect_establish_disconnect_close() {
        let mut m = StateMachine::new();
        assert_eq!(m.transition(ConnEvent::Connect).unwrap(), ConnState::Connecting);
        assert_eq!(m.transition(ConnEvent::Established).unwrap(), ConnState::Connected);
        assert_eq!(m.transition(ConnEvent::Disconnect).unwrap(), ConnState::Disconnected);
        assert_eq!(m.transition(ConnEvent::Close).unwrap(), ConnState::Closed);
        let t = m.last_transition().unwrap();
        assert_eq!(t.from, ConnState::Disconnected);
        assert_eq!(t.event, ConnEvent::Close);
        assert_eq!(t.to, ConnState::Closed);
        assert_eq!(t.reason, "closed while disconnected");
    }

    #[test]
    fn reconnect_loop_returns_to_connected_then_user_stop() {
        let mut m = StateMachine::new();
        m.transition(ConnEvent::Connect).unwrap();
        m.transition(ConnEvent::Established).unwrap();
        assert_eq!(m.transition(ConnEvent::Lost).unwrap(), ConnState::Reconnecting);
        let t = m.last_transition().unwrap();
        assert_eq!(t.reason, "connection lost; reconnect scheduled");
        assert_eq!(m.transition(ConnEvent::Established).unwrap(), ConnState::Connected);
        let t = m.last_transition().unwrap();
        assert_eq!(t.reason, "reconnect established");
        assert_eq!(m.transition(ConnEvent::Disconnect).unwrap(), ConnState::Disconnected);
    }

    #[test]
    fn initial_connect_timeout_enters_retry_loop_then_fails_then_recovers() {
        let mut m = StateMachine::new();
        m.transition(ConnEvent::Connect).unwrap();
        // initial attempt died before establishing -> documented as Lost
        assert_eq!(m.transition(ConnEvent::Lost).unwrap(), ConnState::Reconnecting);
        let t = m.last_transition().unwrap();
        assert_eq!(t.reason, "handshake timed out; retry scheduled");
        assert_eq!(
            m.transition(ConnEvent::RetryExhausted).unwrap(),
            ConnState::Failed
        );
        let t = m.last_transition().unwrap();
        assert_eq!(t.reason, "reconnect attempts exhausted");
        // Failed is recoverable by an explicit new Connect
        assert_eq!(m.transition(ConnEvent::Connect).unwrap(), ConnState::Connecting);
        let t = m.last_transition().unwrap();
        assert_eq!(t.reason, "retry after failure");
        assert_eq!(m.transition(ConnEvent::Established).unwrap(), ConnState::Connected);
    }

    #[test]
    fn fatal_error_from_active_states_lands_in_failed() {
        let mut m = StateMachine::new();
        m.transition(ConnEvent::Connect).unwrap();
        m.transition(ConnEvent::Established).unwrap();
        assert_eq!(m.transition(ConnEvent::FatalError).unwrap(), ConnState::Failed);

        let mut m = StateMachine::new();
        m.transition(ConnEvent::Connect).unwrap();
        assert_eq!(m.transition(ConnEvent::FatalError).unwrap(), ConnState::Failed);

        let mut m = StateMachine::new();
        m.transition(ConnEvent::Connect).unwrap();
        m.transition(ConnEvent::Lost).unwrap();
        assert_eq!(m.transition(ConnEvent::FatalError).unwrap(), ConnState::Failed);
    }

    #[test]
    fn illegal_transitions_rejected_and_leave_state_untouched() {
        // >= 3 distinct illegal pairs required; we pin 6.
        let cases: &[(ConnState, ConnEvent)] = &[
            // Connect while already connected / mid-flight
            (ConnState::Connected, ConnEvent::Connect),
            (ConnState::Connecting, ConnEvent::Connect),
            // Established without an attempt in flight
            (ConnState::Disconnected, ConnEvent::Established),
            (ConnState::Failed, ConnEvent::Established),
            // Retry budget only means something inside Reconnecting
            (ConnState::Connected, ConnEvent::RetryExhausted),
            (ConnState::Failed, ConnEvent::RetryExhausted),
        ];
        for &(state, ev) in cases {
            let mut m = StateMachine::new();
            // drive the machine to `state` over its legal path
            drive_to(&mut m, state);
            let err = m.transition(ev).unwrap_err();
            assert!(
                matches!(err, StateError::InvalidTransition { .. }),
                "({state:?}, {ev:?}) must be an illegal transition, got {err}"
            );
            assert_eq!(m.state(), state, "state must be untouched after rejection");
        }
    }

    /// Drive a fresh machine to `target` using only legal transitions.
    fn drive_to(m: &mut StateMachine, target: ConnState) {
        use ConnEvent as E;
        use ConnState as S;
        let path: &[(S, &[E])] = &[
            (S::Disconnected, &[]),
            (S::Connecting, &[E::Connect]),
            (S::Connected, &[E::Connect, E::Established]),
            (S::Reconnecting, &[E::Connect, E::Established, E::Lost]),
            (S::Failed, &[E::Connect, E::Established, E::Lost, E::RetryExhausted]),
        ];
        for &(want, events) in path {
            if want == target {
                for &e in events {
                    m.transition(e).expect("path to target must be legal");
                }
                assert_eq!(m.state(), target);
                return;
            }
        }
        panic!("no legal path driver for {target:?}");
    }

    #[test]
    fn terminal_closed_accepts_nothing() {
        for event in [
            ConnEvent::Connect,
            ConnEvent::Established,
            ConnEvent::Lost,
            ConnEvent::RetryExhausted,
            ConnEvent::Disconnect,
            ConnEvent::FatalError,
            ConnEvent::Close,
        ] {
            let mut m = StateMachine::new();
            m.transition(ConnEvent::Close).unwrap();
            let err = m.transition(event).unwrap_err();
            assert!(matches!(err, StateError::InvalidTransition { .. }), "{err}");
            assert_eq!(m.state(), ConnState::Closed);
        }
    }

    #[test]
    fn disconnect_and_fatal_invalid_from_disconnected_failed_closed() {
        let mut m = StateMachine::new();
        assert!(m.transition(ConnEvent::Disconnect).is_err());
        assert!(m.transition(ConnEvent::FatalError).is_err());
        m.transition(ConnEvent::Connect).unwrap();
        m.transition(ConnEvent::FatalError).unwrap(); // -> Failed
        assert!(m.transition(ConnEvent::Disconnect).is_err());
        assert!(m.transition(ConnEvent::Lost).is_err());
        m.transition(ConnEvent::Close).unwrap();
        assert!(m.transition(ConnEvent::Disconnect).is_err());
    }

    #[test]
    fn last_transition_is_queryable_at_every_step() {
        let mut m = StateMachine::new();
        assert_eq!(m.last_transition(), None);
        m.transition(ConnEvent::Connect).unwrap();
        let t = m.last_transition().unwrap();
        assert_eq!((t.from, t.event, t.to), (ConnState::Disconnected, ConnEvent::Connect, ConnState::Connecting));
        assert_eq!(t.reason, "connect requested");
        m.transition(ConnEvent::Established).unwrap();
        let t = m.last_transition().unwrap();
        assert_eq!(t.from, ConnState::Connecting);
        assert_eq!(t.to, ConnState::Connected);
        // the query is read-only: state unchanged by asking twice
        assert_eq!(m.state(), ConnState::Connected);
        assert_eq!(m.last_transition(), m.last_transition());
    }

    #[test]
    fn table_exhaustive_pair_count_is_pinned() {
        // guards against accidental table edits: count legal pairs directly.
        let states = [
            ConnState::Disconnected,
            ConnState::Connecting,
            ConnState::Connected,
            ConnState::Reconnecting,
            ConnState::Failed,
            ConnState::Closed,
        ];
        let events = [
            ConnEvent::Connect,
            ConnEvent::Established,
            ConnEvent::Lost,
            ConnEvent::RetryExhausted,
            ConnEvent::Disconnect,
            ConnEvent::FatalError,
            ConnEvent::Close,
        ];
        let legal = states
            .iter()
            .flat_map(|&s| events.iter().map(move |&e| (s, e)))
            .filter(|&(s, e)| next_state(s, e).is_ok())
            .count();
        assert_eq!(states.len() * events.len(), 42);
        assert_eq!(legal, 18, "transition table changed size");
    }
}
