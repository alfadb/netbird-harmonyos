// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! Per-dial request/response bridge. Only a successful shell protect reply
//! transfers a dup into the dial; the shell always closes its original fd.

use std::collections::{HashMap, VecDeque};
use std::net::Ipv4Addr;
use std::os::fd::{FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::sync::oneshot;

use crate::hilog;
use crate::mgmtsock::{dup_socket_fd, SocketSeamError};

const WAIT: Duration = Duration::from_secs(8);
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Management,
    Signal,
    Relay,
    Dns,
}

impl Purpose {
    fn name(self) -> &'static str {
        match self {
            Self::Management => "management",
            Self::Signal => "signal",
            Self::Relay => "relay",
            Self::Dns => "dns",
        }
    }

    fn kind(self) -> &'static str {
        match self {
            Self::Dns => "udp",
            _ => "tcp",
        }
    }
}

#[derive(Debug)]
struct Pending {
    purpose: Purpose,
    started: Instant,
    sender: oneshot::Sender<Result<OwnedFd, SocketSeamError>>,
}

#[derive(Default, Debug)]
struct State {
    closed: bool,
    queue: VecDeque<u64>,
    pending: HashMap<u64, Pending>,
}

#[derive(Default, Debug)]
pub struct SocketSupply {
    state: Mutex<State>,
    dns_servers: Mutex<Vec<Ipv4Addr>>,
}

struct RequestCleanup<'a> {
    supply: &'a SocketSupply,
    id: u64,
}

impl Drop for RequestCleanup<'_> {
    fn drop(&mut self) {
        self.supply.state.lock().expect("socket supply").pending.remove(&self.id);
    }
}

impl SocketSupply {
    pub fn new() -> Self { Self::default() }

    pub fn set_dns_servers(&self, servers: Vec<Ipv4Addr>) {
        *self.dns_servers.lock().expect("dns servers") = servers;
    }

    pub fn dns_servers(&self) -> Vec<Ipv4Addr> {
        self.dns_servers.lock().expect("dns servers").clone()
    }

    pub async fn request(&self, purpose: Purpose) -> Result<OwnedFd, SocketSeamError> {
        self.request_with_id(purpose).await.map(|(_, fd)| fd)
    }

    pub async fn request_with_id(&self, purpose: Purpose) -> Result<(u64, OwnedFd), SocketSeamError> {
        let (sender, receiver) = oneshot::channel();
        let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed) + 1;
        {
            let mut state = self.state.lock().expect("socket supply");
            if state.closed {
                return Err(SocketSeamError::Supply("socket-supply-closed"));
            }
            state.pending.insert(id, Pending { purpose, started: Instant::now(), sender });
            state.queue.push_back(id);
        }
        let _cleanup = RequestCleanup { supply: self, id };
        hilog::emit(&format!("socket-supply: request id={id} purpose={} kind={}", purpose.name(), purpose.kind()));
        match tokio::time::timeout(WAIT, receiver).await {
            Ok(Ok(result)) => result.map(|fd| (id, fd)),
            Ok(Err(_)) => Err(SocketSeamError::Supply("socket-supply-closed")),
            Err(_) => {
                self.state.lock().expect("socket supply").pending.remove(&id);
                hilog::emit(&format!("socket-supply: result id={id} purpose={} result=timeout elapsed_ms={}", purpose.name(), WAIT.as_millis()));
                Err(SocketSeamError::Supply("socket-supply-timeout"))
            }
        }
    }

    pub fn poll_request(&self) -> Option<(u64, Purpose)> {
        let mut state = self.state.lock().expect("socket supply");
        while let Some(id) = state.queue.pop_front() {
            if let Some(pending) = state.pending.get(&id) {
                return Some((id, pending.purpose));
            }
        }
        None
    }

    pub fn poll(&self) -> String {
        match self.poll_request() {
            Some((id, purpose)) => format!("{{\"id\":\"{id}\",\"kind\":\"{}\",\"purpose\":\"{}\"}}",
                purpose.kind(), purpose.name()),
            None => "{\"id\":\"\"}".into(),
        }
    }

    pub fn reply(&self, id: u64, fd: i32, result: &str) -> String {
        let pending = self.state.lock().expect("socket supply").pending.remove(&id);
        let Some(pending) = pending else {
            return "{\"ok\":false,\"error\":\"request-expired\"}".into();
        };
        let outcome = if result == "protected" && fd >= 0 {
            dup_socket_fd(fd).map(|dup| unsafe { OwnedFd::from_raw_fd(dup) })
        } else {
            Err(SocketSeamError::Supply(match result {
                "protect-failed" => "socket-protect-failed",
                _ => "socket-supply-rejected",
            }))
        };
        let token = match &outcome { Ok(_) => "protected", Err(e) => e.token() };
        hilog::emit(&format!("socket-supply: result id={id} purpose={} kind={} fd={fd} result={token} elapsed_ms={}",
            pending.purpose.name(), pending.purpose.kind(), pending.started.elapsed().as_millis()));
        let ok = outcome.is_ok();
        let delivered = pending.sender.send(outcome).is_ok();
        format!("{{\"ok\":{}}}", ok && delivered)
    }

    pub fn cancel_pending_for_network_change(&self) {
        let mut state = self.state.lock().expect("socket supply");
        state.queue.clear();
        for (_, pending) in state.pending.drain() {
            let _ = pending.sender.send(Err(SocketSeamError::Supply("physical-network-changed")));
        }
    }

    /// 006 resource snapshot: `(pending requests, queued requests)` copied
    /// under a NON-BLOCKING lock of this supply's state. `None` = another
    /// thread holds the state lock right now → the counts are UNKNOWN (the
    /// caller must render them as `null`, never as a false 0). The critical
    /// section is two `len()` reads and is never held across an await.
    pub fn counts_try(&self) -> Option<(usize, usize)> {
        let state = try_lock_poison(&self.state)?;
        Some((state.pending.len(), state.queue.len()))
    }

    pub fn close(&self) {
        let mut state = self.state.lock().expect("socket supply");
        state.closed = true;
        state.queue.clear();
        for (_, pending) in state.pending.drain() {
            let _ = pending.sender.send(Err(SocketSeamError::Supply("socket-supply-closed")));
        }
    }
}

fn slot() -> &'static Mutex<Option<Arc<SocketSupply>>> {
    static SLOT: OnceLock<Mutex<Option<Arc<SocketSupply>>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// 006 resource snapshot helper: a NON-BLOCKING, poison-tolerant lock probe.
/// `None` = another thread holds the mutex right now (WouldBlock) → the
/// caller reports the value as unknown (`null`). A poisoned mutex is
/// recovered exactly like `lock_poison` would (panic=abort in release, so
/// poison is a host/test artifact); nothing here can block.
fn try_lock_poison<T>(m: &Mutex<T>) -> Option<std::sync::MutexGuard<'_, T>> {
    match m.try_lock() {
        Ok(guard) => Some(guard),
        Err(std::sync::TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(std::sync::TryLockError::WouldBlock) => None,
    }
}

/// 006 resource snapshot: what the GLOBAL installed supply slot holds.
/// Counts only — never a request id, an fd or a purpose payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupplyCounts {
    /// `false` = no supply installed (no protected management source was
    /// attached, or `connector_stop_json` shut it down). A KNOWN state with
    /// zero requests — not the same fact as "could not read".
    pub installed: bool,
    /// Requests awaiting a shell protect reply. `None` = the supply's own
    /// state lock was busy → unknown.
    pub pending: Option<usize>,
    /// Requests the shell has not polled yet. `None` = unknown (same rule).
    pub queued: Option<usize>,
}

/// 006 resource snapshot: the installed supply's request counters, read with
/// non-blocking locks only. `None` = the GLOBAL slot lock is busy, so even
/// "installed or not" is unknown; `Some(..)` always answers, and when no
/// supply is installed the request counts are a known `0`.
pub fn installed_counts_try() -> Option<SupplyCounts> {
    let slot = try_lock_poison(slot())?;
    match slot.as_ref() {
        None => Some(SupplyCounts { installed: false, pending: Some(0), queued: Some(0) }),
        Some(supply) => Some(match supply.counts_try() {
            Some((pending, queued)) => SupplyCounts {
                installed: true,
                pending: Some(pending),
                queued: Some(queued),
            },
            None => SupplyCounts { installed: true, pending: None, queued: None },
        }),
    }
}

pub fn install(supply: Arc<SocketSupply>) {
    let mut slot = slot().lock().expect("socket supply slot");
    if let Some(old) = slot.replace(supply) { old.close(); }
}

pub fn shutdown() {
    if let Some(old) = slot().lock().expect("socket supply slot").take() { old.close(); }
}

pub fn poll_json() -> String {
    slot().lock().expect("socket supply slot").as_ref()
        .map(|s| s.poll()).unwrap_or_else(|| "{\"id\":\"\"}".into())
}

pub fn reply_json(id: &str, fd: i32, result: &str) -> String {
    let Ok(id) = id.parse::<u64>() else { return "{\"ok\":false,\"error\":\"invalid-id\"}".into(); };
    slot().lock().expect("socket supply slot").as_ref()
        .map(|s| s.reply(id, fd, result))
        .unwrap_or_else(|| "{\"ok\":false,\"error\":\"socket-supply-closed\"}".into())
}

pub fn set_dns_servers_json(json: &str) -> String {
    let Some(supply) = slot().lock().expect("socket supply slot").as_ref().cloned() else {
        return "{\"ok\":false,\"error\":\"socket-supply-closed\"}".into();
    };
    match crate::protected_dns::parse_servers(json) {
        Ok(servers) => {
            let count = servers.len();
            supply.set_dns_servers(servers);
            format!("{{\"ok\":true,\"count\":{count}}}")
        }
        Err(_) => "{\"ok\":false,\"error\":\"invalid-dns-servers\"}".into(),
    }
}

pub fn close_fd_json(fd: i32) -> String {
    if fd < 0 { return "{\"ok\":false}".into(); }
    crate::n2_socket_trace::closed(fd);
    format!("{{\"ok\":{}}}", unsafe { crate::sys::close(fd) } == 0)
}

#[cfg(test)]
mod resource_tests {
    use super::*;

    /// 006: the resource-snapshot probes answer "busy" as `None` (unknown) and
    /// "no supply installed" as a KNOWN zero — never as a false 0. This is the
    /// ONLY test touching the process-global supply slot.
    #[test]
    fn counts_try_reports_busy_as_unknown_and_absent_as_known_zero() {
        shutdown();
        let absent = installed_counts_try().expect("uncontended global slot");
        assert!(!absent.installed, "nothing is installed after shutdown");
        assert_eq!(absent.pending, Some(0), "absent is a known zero, not unknown");
        assert_eq!(absent.queued, Some(0));

        let supply = Arc::new(SocketSupply::new());
        install(supply.clone());
        let installed = installed_counts_try().expect("uncontended global slot");
        assert!(installed.installed);
        assert_eq!(installed.pending, Some(0));
        assert_eq!(installed.queued, Some(0));

        // A busy leaf is UNKNOWN: `counts_try` answers None and the global
        // answer degrades its counts to null instead of reporting a false 0.
        {
            let _held = supply.state.lock().expect("state lock");
            assert_eq!(supply.counts_try(), None, "a busy leaf is unknown");
            let busy = installed_counts_try().expect("global slot is free");
            assert!(busy.installed, "the supply itself is still installed");
            assert_eq!(busy.pending, None, "busy must be null, never 0");
            assert_eq!(busy.queued, None);
        }
        assert_eq!(supply.counts_try(), Some((0, 0)), "released → measured again");

        // A busy GLOBAL slot makes even "installed or not" unknown.
        {
            let _held_slot = slot().lock().expect("slot lock");
            assert!(installed_counts_try().is_none(), "a busy global slot is unknown");
        }

        shutdown();
        assert!(!installed_counts_try().expect("uncontended").installed);
    }
}
