// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! Per-dial request/response bridge. Only a successful shell protect reply
//! transfers a dup into the dial; the shell always closes its original fd.
//!
//! Two ways the shell learns about a request:
//!
//! - `poll_request()` / `poll_json()` — the shell asks (the historical path).
//! - `set_notify()` — the official DIRECTION, pushed: upstream the Go core calls
//!   the platform when it needs a socket (`client/android/client.go` hands it a
//!   `ProtectSocket` callback, `client/net/protectsocket_android.go` calls it
//!   from the dial path) and the host never polls for it. Here the core instead
//!   wakes the shell with the request id the moment the request is enqueued, so
//!   the shell services the queue when it changed rather than at a fixed
//!   cadence. The hook is called with NO lock held, so a hook that immediately
//!   drains the queue (the shell's service loop) cannot deadlock; it is a wake,
//!   not a delivery — the queue stays the single source of truth and is drained
//!   until empty.

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

/// The process-wide enqueue notification (the shell's wake). `None` = nobody is
/// listening, which is the honest state of every host test and of the window
/// before/after the shell subscribes. Guarded, and only ever *called* with the
/// guard dropped — see `notify_enqueued`.
type NotifyHook = Arc<dyn Fn(u64) + Send + Sync>;
static NOTIFY: Mutex<Option<NotifyHook>> = Mutex::new(None);

/// Install, replace or clear the enqueue notification. Replacing is what makes
/// a shell restart (or a fresh `socket_supply_subscribe`) safe: the previous
/// hook is dropped here, never left firing.
pub fn set_notify(hook: Option<NotifyHook>) {
    *NOTIFY.lock().expect("socket supply notify") = hook;
}

/// Wake the shell for request `id`. The hook is cloned out and the lock is
/// RELEASED before the call: the shell's hook drains the queue (`poll_request`)
/// and replies (`reply`) synchronously, both of which take the state lock — so
/// notifying under this lock would deadlock the dial thread. A hook that panics
/// would poison this slot; the shell's hook only schedules ArkTS work.
fn notify_enqueued(id: u64) {
    let hook = NOTIFY.lock().expect("socket supply notify").clone();
    if let Some(hook) = hook {
        hook(id);
    }
}

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
        // Wake the shell with the state lock DROPPED (a serving hook re-enters
        // this module). Every enqueue notifies, unconditionally: a concurrent
        // drain may already have emptied the queue, so a wake suppressed on
        // "the queue is not empty" is exactly the lost notification.
        notify_enqueued(id);
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

/// Serializes the tests that (re)install the process-wide wake hook. The hook is
/// process-global by design (one shell, one subscription), so two such tests
/// running in parallel would clobber each other's hook and make the
/// exactly-once assertions lie. Poison-tolerant: a failed test must not wedge
/// the rest of the suite.
#[cfg(test)]
pub(crate) fn notify_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    match LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// The pushed half of the shell contract: a request entering the queue wakes the
/// installed hook, and the hook's own drain loop (the ArkTS servicing loop) is
/// what empties it.
#[cfg(test)]
mod notify_tests {
    use super::*;
    use std::os::fd::AsRawFd;

    /// Records every id the installed hook was woken for. Ids are process-wide
    /// monotonic, so a test asserts only about the ids IT enqueued — the other
    /// tests of this binary enqueue into their own supplies concurrently, the
    /// same way one process-wide subscription sees every supply of one shell.
    #[derive(Default)]
    struct WakeLog {
        ids: Mutex<Vec<u64>>,
    }

    impl WakeLog {
        fn hook(self: &Arc<Self>) -> NotifyHook {
            let log = Arc::clone(self);
            Arc::new(move |id: u64| log.ids.lock().expect("wake log").push(id))
        }

        fn woken(&self, id: u64) -> usize {
            self.ids.lock().expect("wake log").iter().filter(|seen| **seen == id).count()
        }
    }

    /// A real (dup-able) fd, the shape a protected socket has.
    fn datagram_fd() -> OwnedFd {
        let raw = unsafe { crate::sys::socket(crate::sys::AF_INET, crate::sys::SOCK_DGRAM, 0) };
        assert!(raw >= 0, "socket(2) for a protected fd");
        unsafe { OwnedFd::from_raw_fd(raw) }
    }

    /// ONE shell service pass: drain the queue until it is EMPTY, answering every
    /// request with a protected fd. `poll_request() == None` is the termination
    /// condition the ArkTS servicing loop uses.
    fn serve_until_empty(supply: &Arc<SocketSupply>) -> Vec<u64> {
        let mut served = Vec::new();
        while let Some((id, _purpose)) = supply.poll_request() {
            let fd = datagram_fd();
            assert!(supply.reply(id, fd.as_raw_fd(), "protected").contains("true"));
            served.push(id);
        }
        served
    }

    /// One test, three scenarios, because the hook is process-wide: installing it
    /// from two parallel tests would make them clobber each other's subscription
    /// (which is itself the replace path asserted below).
    #[tokio::test]
    async fn wake_fires_per_enqueue_survives_the_enqueue_drain_race_and_is_replaceable() {
        let _serial = notify_test_lock();
        set_notify(None);
        let log = Arc::new(WakeLog::default());
        set_notify(Some(log.hook()));

        // (a) BURST + RACE: 32 dials enqueue concurrently while the shell drains
        // with `serve_until_empty`. Every enqueue must wake exactly once — even
        // the ones whose request a concurrent pass already drained (a wake
        // suppressed on "the queue is not empty" is the lost notification).
        let supply = Arc::new(SocketSupply::new());
        let burst: usize = 32;
        let mut dials = Vec::new();
        for index in 0..burst {
            let supply = Arc::clone(&supply);
            let purpose = if index % 2 == 0 { Purpose::Management } else { Purpose::Dns };
            dials.push(tokio::spawn(async move { supply.request_with_id(purpose).await }));
        }
        let mut drained = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while drained.len() < burst && tokio::time::Instant::now() < deadline {
            drained.extend(serve_until_empty(&supply));
            if drained.len() < burst { tokio::time::sleep(Duration::from_millis(1)).await; }
        }
        assert_eq!(drained.len(), burst, "the drain loop must see every burst request");
        let mut dialed = Vec::new();
        for dial in dials {
            let (id, fd) = dial.await.expect("dial task").expect("protected reply");
            drop(fd);
            dialed.push(id);
        }
        dialed.sort_unstable();
        drained.sort_unstable();
        assert_eq!(drained, dialed, "drain loop and dials must agree, id for id");
        for id in &dialed {
            assert_eq!(log.woken(*id), 1, "request {id}: wake exactly once");
        }
        assert!(supply.poll_request().is_none(), "the burst left nothing queued");
        assert!(dialed.iter().all(|id| log.woken(*id) == 1), "no duplicate wakes");

        // (b) The hook may SERVE the request itself (the ArkTS callback drains the
        // queue). That re-enters this module from inside the enqueue call, so the
        // wake must happen with the state lock dropped — under the lock this
        // would deadlock and only the timeout below would report it.
        let serving = Arc::new(SocketSupply::new());
        let served = Arc::new(Mutex::new(Vec::<u64>::new()));
        {
            let supply = Arc::clone(&serving);
            let served = Arc::clone(&served);
            set_notify(Some(Arc::new(move |_id: u64| {
                let mut log = served.lock().expect("served log");
                while let Some((queued, _purpose)) = supply.poll_request() {
                    let fd = datagram_fd();
                    assert!(supply.reply(queued, fd.as_raw_fd(), "protected").contains("true"));
                    log.push(queued);
                }
            })));
        }
        let (served_id, served_fd) =
            tokio::time::timeout(Duration::from_secs(5), serving.request_with_id(Purpose::Relay))
                .await
                .expect("a hook that serves inside the wake must not deadlock")
                .expect("protected reply");
        drop(served_fd);
        assert_eq!(*served.lock().expect("served log"), vec![served_id]);
        assert!(serving.poll_request().is_none(), "serving inside the wake drained the queue");
        assert_eq!(log.woken(served_id), 0, "the replaced hook is no longer called");

        // (c) CLEARED (shell teardown): nobody is woken, yet an explicit poll still
        // serves the request — the queue, never the wake, is the source of truth.
        set_notify(None);
        let polled = Arc::new(SocketSupply::new());
        let server = tokio::spawn({
            let supply = Arc::clone(&polled);
            async move {
                loop {
                    if let Some((id, _purpose)) = supply.poll_request() {
                        let fd = datagram_fd();
                        assert!(supply.reply(id, fd.as_raw_fd(), "protected").contains("true"));
                        return id;
                    }
                    tokio::task::yield_now().await;
                }
            }
        });
        let (polled_id, polled_fd) =
            polled.request_with_id(Purpose::Signal).await.expect("polled reply");
        drop(polled_fd);
        assert_eq!(server.await.expect("poll server"), polled_id);
        assert_eq!(log.woken(polled_id), 0, "a cleared hook is never called again");
        assert_eq!(served.lock().expect("served log").len(), 1, "only scenario (b) served");
        assert!(polled.poll_request().is_none());
    }
}
