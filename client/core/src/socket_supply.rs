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

// ---------------------------------------------------------------------------
// R2 (design `analysis-20261026-dns-structural` §4.2): EPERM is "the protected
// path is UNUSABLE", a first-class state rather than one line of evidence.
// ---------------------------------------------------------------------------

/// R2: the reason token the SHELL's supply gate answers with once its liveness
/// input says the protected path is down, and the `reason=` the persisted
/// `event=socket_supply` record carries (AC-R2-2).
pub const PROTECTED_PATH_UNUSABLE: &str = "protected-path-unusable";

/// R2: the core's error token for a DNS request refused while the path is
/// known-unusable. It is NOT transient: retrying it would ask a path that is
/// already known to be refused for a second socket.
pub const SUPPLY_UNUSABLE_TOKEN: &str = "socket-supply-unusable";

/// R2: how long a NEW DNS request is refused outright after an EPERM.
///
/// A WINDOW, not a latch: when it expires the next DNS request is a PROBE, so a
/// recovered path is discovered without any new channel AND the state can never
/// reproduce the 0.0.27 five-hour refusal (REPORT.md §0.1/§2.4). The latch
/// itself ([`SocketSupply::protected_path_unusable`]) outlives the window; it
/// exists only to keep the OS-resolver stage short until the path answers again.
/// 30 s is deliberately the same scale as the data-plane watchdog's stale
/// window, so a recovered path is re-probed on that same order.
const UNUSABLE_PROBE_WINDOW: Duration = Duration::from_secs(30);

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

/// R1 (design §4.2): is a shell subscribed to the wake channel? The same
/// subscription that serves the supply queue is what drains the reprotect queue,
/// so its presence is exactly "somebody can answer this diagnostic". No
/// subscription → the 250 ms wait is skipped outright.
pub fn shell_listening() -> bool {
    NOTIFY.lock().expect("socket supply notify").is_some()
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

    fn default_kind(self) -> SocketKind {
        match self {
            Self::Dns => SocketKind::Udp,
            _ => SocketKind::Tcp,
        }
    }
}

/// Transport requested from the protected socket provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocketKind {
    Udp,
    Tcp,
}

impl SocketKind {
    fn name(self) -> &'static str {
        match self {
            Self::Udp => "udp",
            Self::Tcp => "tcp",
        }
    }
}

/// R0 ets half (design `analysis-20261026-dns-structural` §4.2, AC-R0-1): the
/// OPTIONAL shell self-report that rides a socket-supply reply. All three
/// values are PLATFORM facts the core cannot see — the physical network id the
/// protect was issued on, the shell's protect generation at that instant, and
/// the protect call's measured duration. `None` means the shell did not report
/// it (the pre-R0 three-argument reply); NOTHING is then printed, so the line
/// stays byte-identical to its pre-R0 shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SupplyMeta {
    /// Platform physical network id at protect time (`net_id=`).
    pub net_id: Option<u64>,
    /// The shell's network/protect generation at protect time
    /// (`protect_generation=`; the core's own counter is `epoch=`).
    pub protect_generation: Option<u64>,
    /// The protect call's measured duration in milliseconds (`protect_ms=`).
    pub protect_ms: Option<u64>,
}

impl SupplyMeta {
    pub fn is_empty(&self) -> bool {
        self.net_id.is_none() && self.protect_generation.is_none() && self.protect_ms.is_none()
    }

    /// The APPEND-ONLY suffix shared by `socket-supply: result` and
    /// `protected-dns: eperm`. The field order is fixed (`net_id` ->
    /// `protect_generation` -> `protect_ms`) and each field appears only when
    /// the shell reported it, so an old shell's reply is unchanged.
    pub fn suffix(&self) -> String {
        let mut out = String::new();
        if let Some(net_id) = self.net_id { out.push_str(&format!(" net_id={net_id}")); }
        if let Some(generation) = self.protect_generation {
            out.push_str(&format!(" protect_generation={generation}"));
        }
        if let Some(ms) = self.protect_ms { out.push_str(&format!(" protect_ms={ms}")); }
        out
    }
}

/// R0: map the THREE OPTIONAL trailing napi reply numbers to a [`SupplyMeta`].
/// The shell sends `-1` for "unknown" (a refused request never reached a
/// protect), which is the same fact as an absent argument — an old shell simply
/// passes nothing at all (the argument defaults to `-1`). A real `0` (net id 0,
/// generation 0, a 0 ms protect) is a VALUE, not absence. The values are the
/// shell's small counters: a network id, its generation counter and the
/// duration of one bounded protect call, all far inside `i32` (the protect call
/// itself is boxed at 5 s in the shell).
pub fn meta_from_reply_args(net_id: i32, protect_generation: i32, protect_ms: i32) -> SupplyMeta {
    let value = |v: i32| if v < 0 { None } else { Some(v as u64) };
    SupplyMeta {
        net_id: value(net_id),
        protect_generation: value(protect_generation),
        protect_ms: value(protect_ms),
    }
}

/// R0: how many replied request ids keep their shell meta. The EPERM evidence
/// is collected in the same resolve pass that received the reply, so a small
/// recent window is enough; the retention is evidence only and never read by a
/// decision.
const META_RETAINED: usize = 64;

/// R2: the protected path's fast-fail state, as the shell reads it back through
/// `socket_supply_path_state`. Two DIFFERENT facts are reported because they
/// expire differently:
///
/// - `unusable` is the LATCH (set by an EPERM, cleared by the existing recovery
///   or by a protected answer). It keeps the OS-resolver stage at its short
///   bound, so a fault window cannot re-introduce the 3 s wait per chain;
/// - `refuse_dns` additionally says "refuse a NEW DNS request right now", which
///   is only true inside [`UNUSABLE_PROBE_WINDOW`] so the next request after it
///   is a probe instead of a permanent lockout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathState {
    pub unusable: bool,
    pub refuse_dns: bool,
    pub reason: &'static str,
    pub generation: u64,
    pub age_ms: u128,
}

impl PathState {
    /// The not-unusable document: what the shell must read when nothing has gone
    /// wrong (and what a pre-R2 core's missing export degrades to).
    pub fn usable() -> Self {
        Self { unusable: false, refuse_dns: false, reason: "", generation: 0, age_ms: 0 }
    }

    /// The wire document of the R2 NAPI export. Flat, additive and stable — the
    /// shell parses these five keys by name.
    pub fn json(&self) -> String {
        format!(
            "{{\"unusable\":{},\"refuse_dns\":{},\"reason\":\"{}\",\"generation\":{},\"age_ms\":{}}}",
            self.unusable, self.refuse_dns, self.reason, self.generation, self.age_ms
        )
    }
}

/// R2: the one unusable verdict this supply holds. `window` is carried per
/// entry so a host test can shorten it without a second code path.
#[derive(Debug)]
struct UnusableState {
    reason: &'static str,
    since: Instant,
    generation: u64,
    window: Duration,
}

impl UnusableState {
    fn refusing(&self) -> bool {
        self.since.elapsed() < self.window
    }
}

/// R0 (design `analysis-20261026-dns-structural` §4.2/§5.2 #2): the
/// `socket-supply: result` line. APPEND-ONLY — every pre-existing field
/// (`id=`, `purpose=`, `kind=`, `fd=`, `result=`, `elapsed_ms=`) keeps its
/// position and meaning, `epoch=` (the core-owned
/// [`SocketSupply::generation`] at the reply instant) is tail-appended, and the
/// shell-reported [`SupplyMeta`] group follows it when (and only when) the
/// shell reported one. Pure formatter so the field contract is testable without
/// capturing stderr (`tests/dns_r0_observability.rs`).
pub fn supply_result_line(
    id: u64,
    purpose: Purpose,
    kind: SocketKind,
    fd: i32,
    token: &str,
    elapsed_ms: u128,
    epoch: u64,
    meta: &SupplyMeta,
) -> String {
    format!(
        "socket-supply: result id={id} purpose={} kind={} fd={fd} result={token} elapsed_ms={elapsed_ms} epoch={epoch}{}",
        purpose.name(), kind.name(), meta.suffix())
}

#[derive(Debug)]
struct Pending {
    purpose: Purpose,
    kind: SocketKind,
    started: Instant,
    sender: oneshot::Sender<Result<OwnedFd, SocketSeamError>>,
}

#[derive(Default, Debug)]
struct State {
    closed: bool,
    queue: VecDeque<u64>,
    pending: HashMap<u64, Pending>,
}

// ---------------------------------------------------------------------------
// R1 (design `analysis-20261026-dns-structural` §4.2): reprotect-once.
//
// On the FIRST EPERM of one protected-DNS resolution chain the chain hands the
// shell the very descriptor it is about to re-send on, the shell re-issues the
// ONE write-side SDK call (`VpnConnection.protect(fd)`) on that SAME descriptor,
// and the chain sends the identical query once more, bounded. Everything below
// is a BOUNDED, additive diagnostic: no field is read by a decision, and a
// timeout/refusal simply continues the unchanged fallback chain.
// ---------------------------------------------------------------------------

/// R1: the shell's answer to ONE reprotect request. Only these two outcomes come
/// FROM the shell; [`ReprotectOutcome::Timeout`] / [`ReprotectOutcome::Unavailable`]
/// are decided here (the wait expired / nobody could be asked).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReprotectReply {
    Ok { protect_ms: Option<u64> },
    Failed { errno: i32, protect_ms: Option<u64> },
}

/// R1: what ONE reprotect experiment produced, as the `protected-dns:` record
/// reports it. The four rows of the design's judgement table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReprotectOutcome {
    /// The shell re-issued `protect(fd)` and it resolved.
    Ok,
    /// The shell re-issued it and the platform refused; `errno` is the shell's
    /// error code (`-1` when the throw carried none).
    Failed { errno: i32 },
    /// A shell was subscribed but did not answer inside the core's bound.
    Timeout,
    /// Nobody could be asked: no shell subscribed, the supply was closed, or the
    /// pending-request bound was reached.
    Unavailable,
}

impl ReprotectOutcome {
    pub fn is_ok(&self) -> bool { matches!(self, Self::Ok) }
}

/// R1: the bounded result of ONE reprotect request. `id` is the request id (0
/// when nothing was enqueued), `protect_ms` the shell's own measurement of the
/// platform call, `elapsed_ms` the core-side round trip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReprotectResult {
    pub id: u64,
    pub outcome: ReprotectOutcome,
    pub protect_ms: Option<u64>,
    pub elapsed_ms: u128,
}

/// R1: ONE reprotect request. The descriptor is BORROWED from the calling
/// attempt (the core keeps sending on it while it waits); this entry never
/// closes it — closing a descriptor the shell may still hold the NUMBER of is
/// exactly the reuse the hand-back is designed to avoid.
#[derive(Debug)]
struct ReprotectPending {
    fd: i32,
    started: Instant,
    /// `true` once the shell polled it (it is now the shell's to protect).
    claimed: bool,
    /// The wait's sender; `None` after the core's bound expired, so a late
    /// answer reports `request-expired` instead of resolving anything.
    sender: Option<oneshot::Sender<ReprotectReply>>,
}

#[derive(Default, Debug)]
struct ReprotectState {
    /// Unclaimed request ids in arrival order.
    queue: VecDeque<u64>,
    pending: HashMap<u64, ReprotectPending>,
}

/// R1: an unclaimed request older than this is withdrawn (the shell's 5 s safety
/// net still serves one whose wake was lost; a request nobody ever takes must
/// not linger and let a late poll protect an already-closed descriptor number).
const REPROTECT_EXPIRE: Duration = Duration::from_secs(5);
/// R1: bound on the requests one deaf shell can leave unclaimed at a time.
const REPROTECT_MAX_PENDING: usize = 8;

/// R1: drop the requests nobody can answer any more (expired, or already
/// answered through another path) and the queue aliases they left behind.
fn prune_reprotect(state: &mut ReprotectState) {
    state.pending.retain(|_, entry| entry.started.elapsed() < REPROTECT_EXPIRE);
    let ReprotectState { queue, pending } = state;
    queue.retain(|id| pending.contains_key(id));
}

#[derive(Default, Debug)]
pub struct SocketSupply {
    state: Mutex<State>,
    dns_servers: Mutex<Vec<Ipv4Addr>>,
    /// OBSERVATION-ONLY network generation (3-D, code plan §3.2 D): advanced by
    /// every reported physical-network change (the connector calls
    /// [`Self::cancel_pending_for_network_change`] on that exact path). Read by
    /// the protected-DNS EPERM evidence so a capture can tell whether a refusal
    /// was bound to a generation change; NEVER read by a decision — the EPERM
    /// no-retry rule and the whole fallback chain are untouched.
    network_generation: AtomicU64,
    /// N2(c) (0.0.34, protected-DNS outage REPORT.md §5): the LAST address this
    /// supply successfully resolved for a host. It belongs to the supply — the
    /// resolution context — rather than to the process, so it dies with the
    /// installation it was learned on and a host/unit test never inherits
    /// another test's answer. Only ever consulted as the FINAL fallback of
    /// [`crate::protected_dns`]'s chain, after the protected path (and its
    /// retry) and the OS resolver both failed; a live lookup always wins.
    last_good: Mutex<HashMap<String, Ipv4Addr>>,
    /// R0 (ets half): the shell-reported [`SupplyMeta`] of the most recent
    /// `META_RETAINED` replied requests, BY REQUEST ID, so the protected-DNS
    /// EPERM record of a socket handed out under that reply can print the SAME
    /// `net_id` / `protect_generation` / `protect_ms` group. Evidence only,
    /// bounded, never read by a decision.
    reply_meta: Mutex<VecDeque<(u64, SupplyMeta)>>,
    /// R1: the ONE reprotect request queue (shell polls, shell answers by id).
    /// Diagnostic only; independent of the supply queue so an old shell that
    /// never drains it cannot delay a dial.
    reprotect: Mutex<ReprotectState>,
    /// R2: the protected path's fast-fail state — `Some` from the first EPERM
    /// until the existing recovery (a reported network change), a successful
    /// protected answer, or a fresh protected-dial answer clears it. Never read
    /// by a non-DNS purpose: management/signal/relay keep being supplied.
    unusable: Mutex<Option<UnusableState>>,
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

    /// N2(c): remember `ip` as the last address this supply resolved for
    /// `host`. Called by the protected-DNS chain on every successful answer
    /// (protected, retried-protected or OS-resolver) — never on a guess.
    pub fn remember_address(&self, host: &str, ip: Ipv4Addr) {
        if host.is_empty() { return; }
        self.last_good.lock().expect("last good").insert(host.to_string(), ip);
    }

    /// N2(c): the last address this supply resolved for `host`, if any.
    pub fn last_known_address(&self, host: &str) -> Option<Ipv4Addr> {
        self.last_good.lock().expect("last good").get(host).copied()
    }

    /// How many hosts have a last-known-good address (observability/tests).
    pub fn last_known_addresses(&self) -> usize {
        self.last_good.lock().expect("last good").len()
    }

    /// 3-D: the supply's network generation (observation only). It starts at 0
    /// and advances by one for every reported physical-network change — the
    /// same call that cancels the in-flight protect requests
    /// ([`Self::cancel_pending_for_network_change`]). The protected-DNS EPERM
    /// evidence records it so a capture can answer "was this EPERM bound to a
    /// generation change?"; no decision consults it.
    pub fn generation(&self) -> u64 {
        self.network_generation.load(Ordering::Relaxed)
    }

    /// R0 (ets half): the shell-reported [`SupplyMeta`] of the reply that
    /// handed out request `id` — the SAME group the `socket-supply: result`
    /// line printed for it, so a `protected-dns: eperm` record can state the
    /// `net_id` / `protect_generation` / `protect_ms` the socket's protect was
    /// issued on. `None` = that reply carried no meta (the pre-R0 shell), or the
    /// id is older than the retained window. Evidence only.
    pub fn request_meta(&self, id: u64) -> Option<SupplyMeta> {
        self.reply_meta.lock().expect("socket supply meta")
            .iter().rev().find(|(request, _)| *request == id).map(|(_, meta)| *meta)
    }

    pub async fn request(&self, purpose: Purpose) -> Result<OwnedFd, SocketSeamError> {
        self.request_with_id(purpose).await.map(|(_, fd)| fd)
    }

    pub async fn request_socket(&self, purpose: Purpose, kind: SocketKind) -> Result<OwnedFd, SocketSeamError> {
        self.request_socket_with_id(purpose, kind).await.map(|(_, fd)| fd)
    }

    pub async fn request_with_id(&self, purpose: Purpose) -> Result<(u64, OwnedFd), SocketSeamError> {
        self.request_socket_with_id(purpose, purpose.default_kind()).await
    }

    /// R2 (design `analysis-20261026-dns-structural` §4.2): RECORD that the
    /// protected path is unusable, and make it cost nothing any more.
    ///
    /// - the latch is set (it keeps the OS-resolver stage at its short bound);
    /// - every in-flight **DNS** request is cancelled AT ONCE with
    ///   [`SUPPLY_UNUSABLE_TOKEN`] instead of waiting out the 8 s supply bound;
    /// - every NEW DNS request is refused instantly for
    ///   [`UNUSABLE_PROBE_WINDOW`] (the shell is not even woken);
    /// - management / signal / relay are untouched — the conservative scope the
    ///   design asks for, and the reason this can never become the 0.0.27 latch
    ///   that refused everything.
    ///
    /// It is orthogonal to [`Self::generation`]: marking does NOT advance the
    /// network epoch (only a reported physical-network change does). Returns how
    /// many in-flight DNS requests were cancelled.
    pub fn mark_unusable(&self, reason: &'static str) -> usize {
        let generation = self.generation();
        {
            let mut unusable = self.unusable.lock().expect("socket supply unusable");
            *unusable = Some(UnusableState {
                reason,
                since: Instant::now(),
                generation,
                window: UNUSABLE_PROBE_WINDOW,
            });
        }
        let cancelled = self.cancel_pending_dns();
        hilog::emit(&format!(
            "socket-supply: unusable reason={reason} generation={generation} cancelled={cancelled}"));
        cancelled
    }

    /// R2: the path is proven usable again (a protected answer, or the shell's
    /// reported recovery). Idempotent; logs exactly the transition.
    pub fn clear_unusable(&self, reason: &'static str) {
        let cleared = self.unusable.lock().expect("socket supply unusable").take();
        if let Some(state) = cleared {
            hilog::emit(&format!(
                "socket-supply: usable reason={reason} previous={} age_ms={} generation={}",
                state.reason, state.since.elapsed().as_millis(), self.generation()));
        }
    }

    /// R2: is the protected path currently known-unusable (the latch)? This is
    /// what bounds the OS-resolver stage; it stays set across the refusal window
    /// until the path answers or the shell reports a recovery.
    pub fn protected_path_unusable(&self) -> bool {
        self.unusable.lock().expect("socket supply unusable").is_some()
    }

    /// R2: must a NEW DNS request be refused AT THIS INSTANT (inside the probe
    /// window)? `None` = serve it (normally, or as the window's probe).
    fn dns_rejection(&self) -> Option<&'static str> {
        let unusable = self.unusable.lock().expect("socket supply unusable");
        match &*unusable {
            Some(state) if state.refusing() => Some(state.reason),
            _ => None,
        }
    }

    /// R2: the document the shell reads (the `socket_supply_path_state` NAPI
    /// export).
    pub fn path_state(&self) -> PathState {
        let unusable = self.unusable.lock().expect("socket supply unusable");
        let Some(state) = &*unusable else {
            return PathState { generation: self.generation(), ..PathState::usable() };
        };
        PathState {
            unusable: true,
            refuse_dns: state.refusing(),
            reason: state.reason,
            generation: state.generation,
            age_ms: state.since.elapsed().as_millis(),
        }
    }

    pub fn path_state_json(&self) -> String {
        self.path_state().json()
    }

    /// R2 test seam: shorten/lengthen the refusal window of the live state, so
    /// the "the window ends and the next request probes" path is testable
    /// without sleeping 30 s. Production never calls it.
    #[cfg(test)]
    pub fn set_unusable_probe_window(&self, window: Duration) {
        if let Some(state) = self.unusable.lock().expect("socket supply unusable").as_mut() {
            state.window = window;
        }
    }

    /// R2: cancel the DNS requests that are still waiting for their protect
    /// reply — they belong to the path that just refused. Management / signal /
    /// relay requests (and the reprotect queue) are kept.
    fn cancel_pending_dns(&self) -> usize {
        let mut state = self.state.lock().expect("socket supply");
        let State { queue, pending, .. } = &mut *state;
        let victims: Vec<u64> = pending.iter()
            .filter(|(_, entry)| entry.purpose == Purpose::Dns)
            .map(|(id, _)| *id)
            .collect();
        for id in &victims {
            if let Some(entry) = pending.remove(id) {
                let _ = entry.sender.send(Err(SocketSeamError::Supply(SUPPLY_UNUSABLE_TOKEN)));
            }
        }
        queue.retain(|id| pending.contains_key(id));
        victims.len()
    }

    async fn request_socket_with_id(&self, purpose: Purpose, kind: SocketKind) -> Result<(u64, OwnedFd), SocketSeamError> {
        let (sender, receiver) = oneshot::channel();
        let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed) + 1;
        // R2: a DNS request while the path is known-unusable is answered HERE,
        // before the shell is woken and before the 8 s wait can start. The line
        // is the ordinary `socket-supply: result` shape with `fd=-1` and
        // `elapsed_ms=0`, so the existing parsers see a completed refusal.
        if purpose == Purpose::Dns && self.dns_rejection().is_some() {
            hilog::emit(&supply_result_line(
                id, purpose, kind, -1, SUPPLY_UNUSABLE_TOKEN, 0,
                self.generation(), &SupplyMeta::default()));
            return Err(SocketSeamError::Supply(SUPPLY_UNUSABLE_TOKEN));
        }
        {
            let mut state = self.state.lock().expect("socket supply");
            if state.closed {
                return Err(SocketSeamError::Supply("socket-supply-closed"));
            }
            state.pending.insert(id, Pending { purpose, kind, started: Instant::now(), sender });
            state.queue.push_back(id);
        }
        // Wake the shell with the state lock DROPPED (a serving hook re-enters
        // this module). Every enqueue notifies, unconditionally: a concurrent
        // drain may already have emptied the queue, so a wake suppressed on
        // "the queue is not empty" is exactly the lost notification.
        notify_enqueued(id);
        let _cleanup = RequestCleanup { supply: self, id };
        hilog::emit(&format!("socket-supply: request id={id} purpose={} kind={}", purpose.name(), kind.name()));
        match tokio::time::timeout(WAIT, receiver).await {
            Ok(Ok(result)) => result.map(|fd| (id, fd)),
            Ok(Err(_)) => Err(SocketSeamError::Supply("socket-supply-closed")),
            Err(_) => {
                self.state.lock().expect("socket supply").pending.remove(&id);
                hilog::emit(&format!("socket-supply: result id={id} purpose={} result=timeout elapsed_ms={} epoch={}",
                    purpose.name(), WAIT.as_millis(), self.generation()));
                Err(SocketSeamError::Supply("socket-supply-timeout"))
            }
        }
    }

    pub fn poll_request(&self) -> Option<(u64, Purpose)> {
        self.poll_socket_request().map(|(id, purpose, _)| (id, purpose))
    }

    fn poll_socket_request(&self) -> Option<(u64, Purpose, SocketKind)> {
        let mut state = self.state.lock().expect("socket supply");
        while let Some(id) = state.queue.pop_front() {
            if let Some(pending) = state.pending.get(&id) {
                return Some((id, pending.purpose, pending.kind));
            }
        }
        None
    }

    pub fn poll(&self) -> String {
        match self.poll_socket_request() {
            Some((id, purpose, kind)) => format!("{{\"id\":\"{id}\",\"kind\":\"{}\",\"purpose\":\"{}\"}}",
                kind.name(), purpose.name()),
            None => "{\"id\":\"\"}".into(),
        }
    }

    pub fn reply(&self, id: u64, fd: i32, result: &str) -> String {
        self.reply_with_meta(id, fd, result, &SupplyMeta::default())
    }

    /// [`Self::reply`] plus the OPTIONAL R0 shell self-report (the ets half's
    /// `net_id` / `networkEpoch` / `protect_ms`). The group is printed on the
    /// result line and KEPT BY REQUEST ID for the same request's EPERM evidence;
    /// a default (empty) group is the pre-R0 reply and changes nothing.
    pub fn reply_with_meta(&self, id: u64, fd: i32, result: &str, meta: &SupplyMeta) -> String {
        let pending = self.state.lock().expect("socket supply").pending.remove(&id);
        let Some(pending) = pending else {
            return "{\"ok\":false,\"error\":\"request-expired\"}".into();
        };
        let outcome = if result == "protected" && fd >= 0 {
            dup_socket_fd(fd).map(|dup| unsafe { OwnedFd::from_raw_fd(dup) })
        } else {
            Err(SocketSeamError::Supply(match result {
                "protect-failed" => "socket-protect-failed",
                // R2: the shell's OWN liveness gate refused the DNS supply, and
                // that refusal is the same terminal fact as the core-side fast
                // failure — never a transient `socket-supply-rejected` that the
                // chain would retry against an unusable path.
                PROTECTED_PATH_UNUSABLE => SUPPLY_UNUSABLE_TOKEN,
                _ => "socket-supply-rejected",
            }))
        };
        if !meta.is_empty() {
            let mut retained = self.reply_meta.lock().expect("socket supply meta");
            retained.push_back((id, *meta));
            while retained.len() > META_RETAINED { retained.pop_front(); }
        }
        let token = match &outcome { Ok(_) => "protected", Err(e) => e.token() };
        hilog::emit(&supply_result_line(
            id, pending.purpose, pending.kind, fd, token,
            pending.started.elapsed().as_millis(), self.generation(), meta));
        let ok = outcome.is_ok();
        let delivered = pending.sender.send(outcome).is_ok();
        format!("{{\"ok\":{}}}", ok && delivered)
    }

    /// R1 (design `analysis-20261026-dns-structural` §4.2): hand the shell the
    /// descriptor the caller is sending on and wait, BOUNDED, for ONE
    /// `protect(fd)` result. The wait and the withdrawal below are the whole
    /// contract:
    ///
    /// - no subscribed shell → [`ReprotectOutcome::Unavailable`] at once (the
    ///   diagnostic must not cost 250 ms when nobody can answer);
    /// - the bound expires BEFORE the shell polled it → the request is
    ///   WITHDRAWN, so a late poll can never protect a descriptor number the
    ///   attempt has since closed;
    /// - the shell polled it but the bound expired first → the request stays
    ///   (the shell holds the number already) and the late answer reports
    ///   `request-expired`.
    ///
    /// The descriptor is only BORROWED: this never closes it.
    pub async fn reprotect(&self, fd: i32, wait: Duration) -> ReprotectResult {
        let unavailable = ReprotectResult {
            id: 0,
            outcome: ReprotectOutcome::Unavailable,
            protect_ms: None,
            elapsed_ms: 0,
        };
        if fd < 0 || !shell_listening() { return unavailable; }
        let started = Instant::now();
        // The SAME monotonic id space as the supply requests: a wake is keyed by
        // id, so a second, colliding counter would alias wakes and break the
        // exactly-once wake contract the shell's drain loop relies on.
        let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed) + 1;
        let (sender, receiver) = oneshot::channel();
        {
            let mut state = self.reprotect.lock().expect("socket supply reprotect");
            prune_reprotect(&mut state);
            if state.pending.len() >= REPROTECT_MAX_PENDING { return unavailable; }
            state.pending.insert(id, ReprotectPending {
                fd, started, claimed: false, sender: Some(sender),
            });
            state.queue.push_back(id);
        }
        // Woken with NO lock held: a serving hook re-enters this module.
        notify_enqueued(id);
        match tokio::time::timeout(wait, receiver).await {
            Ok(Ok(reply)) => {
                let elapsed_ms = started.elapsed().as_millis();
                let (outcome, protect_ms) = match reply {
                    ReprotectReply::Ok { protect_ms } => (ReprotectOutcome::Ok, protect_ms),
                    ReprotectReply::Failed { errno, protect_ms } =>
                        (ReprotectOutcome::Failed { errno }, protect_ms),
                };
                ReprotectResult { id, outcome, protect_ms, elapsed_ms }
            }
            Ok(Err(_)) => ReprotectResult {
                id,
                outcome: ReprotectOutcome::Unavailable,
                protect_ms: None,
                elapsed_ms: started.elapsed().as_millis(),
            },
            Err(_) => {
                let mut state = self.reprotect.lock().expect("socket supply reprotect");
                if let Some(entry) = state.pending.get_mut(&id) {
                    entry.sender = None;
                    if !entry.claimed {
                        state.pending.remove(&id);
                        state.queue.retain(|queued| *queued != id);
                    }
                }
                ReprotectResult {
                    id,
                    outcome: ReprotectOutcome::Timeout,
                    protect_ms: None,
                    elapsed_ms: started.elapsed().as_millis(),
                }
            }
        }
    }

    /// R1: the shell's request poll — `(id, fd)` of the oldest unclaimed request,
    /// or `None` when there is nothing to protect. Answering it with
    /// [`Self::reprotect_reply`] is what resolves the core's wait.
    pub fn poll_reprotect(&self) -> Option<(u64, i32)> {
        let mut state = self.reprotect.lock().expect("socket supply reprotect");
        prune_reprotect(&mut state);
        while let Some(id) = state.queue.pop_front() {
            let Some(entry) = state.pending.get_mut(&id) else { continue };
            if entry.claimed { continue; }
            entry.claimed = true;
            return Some((id, entry.fd));
        }
        None
    }

    /// R1: the shell's answer to request `id`. `result` is the wire token (`ok`
    /// or `fail`); `errno`/`protect_ms` are `-1` when the shell has no such
    /// value. A stale id answers `request-expired` — the core already moved on
    /// and a late answer must not be applied to anything.
    pub fn reprotect_reply(&self, id: u64, result: &str, errno: i32, protect_ms: i32) -> String {
        let mut state = self.reprotect.lock().expect("socket supply reprotect");
        let Some(entry) = state.pending.remove(&id) else {
            return "{\"ok\":false,\"error\":\"request-expired\"}".into();
        };
        state.queue.retain(|queued| *queued != id);
        let protect_ms = if protect_ms < 0 { None } else { Some(protect_ms as u64) };
        let reply = if result == "ok" {
            ReprotectReply::Ok { protect_ms }
        } else {
            ReprotectReply::Failed { errno, protect_ms }
        };
        match entry.sender {
            Some(sender) => {
                let _ = sender.send(reply);
                "{\"ok\":true}".into()
            }
            // The core's bound expired first; the answer is honest but late.
            None => "{\"ok\":false,\"error\":\"request-expired\"}".into(),
        }
    }

    pub fn cancel_pending_for_network_change(&self) {
        // 3-D (observation only): this is the connector's physical-network-change
        // path, so it is also where the evidence generation advances. The
        // cancellation semantics below are unchanged.
        self.network_generation.fetch_add(1, Ordering::Relaxed);
        // R2: this IS the existing recovery channel (the shell's data-plane
        // watchdog reports a physical network change through it), so the new
        // "protected path unusable" state is cleared here — R2 adds no second
        // recovery path of its own.
        self.clear_unusable("physical-network-changed");
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
        drop(state);
        // R1: a closed supply can answer no reprotect. Dropping the waiters is
        // what makes their `Ok(Err(_))` branch report `unavailable` instead of
        // hanging out the bound.
        let mut reprotect = self.reprotect.lock().expect("socket supply reprotect");
        reprotect.queue.clear();
        reprotect.pending.clear();
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

/// R2 (design `analysis-20261026-dns-structural` §4.2): the protected path's
/// fast-fail state, as the shell's supply gate reads it
/// (`socket_supply_path_state`). No supply installed is the honest NOT-unusable
/// answer (a pre-R2 core answers nothing at all and the shell degrades to the
/// same document), never an error.
pub fn path_state_json() -> String {
    slot().lock().expect("socket supply slot").as_ref()
        .map(|supply| supply.path_state_json())
        .unwrap_or_else(|| PathState::usable().json())
}

pub fn reply_json(id: &str, fd: i32, result: &str) -> String {
    reply_json_with_meta(id, fd, result, &SupplyMeta::default())
}

/// R0 (ets half): [`reply_json`] with the THREE OPTIONAL shell-reported reply
/// numbers already mapped by [`meta_from_reply_args`]. The pre-R0 napi export
/// (three arguments) reaches [`reply_json`] and stays byte-identical.
pub fn reply_json_with_meta(
    id: &str,
    fd: i32,
    result: &str,
    meta: &SupplyMeta,
) -> String {
    let Ok(id) = id.parse::<u64>() else { return "{\"ok\":false,\"error\":\"invalid-id\"}".into(); };
    slot().lock().expect("socket supply slot").as_ref()
        .map(|s| s.reply_with_meta(id, fd, result, meta))
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

/// R1: the shell's reprotect poll document — `{"id":"<n>","fd":"<n>"}` for one
/// pending request or the empty id when there is none. Same shape convention as
/// [`poll_json`] (ids/fds are STRINGS so a u64 id survives any JS number).
pub fn poll_reprotect_json() -> String {
    match slot().lock().expect("socket supply slot").as_ref() {
        Some(supply) => match supply.poll_reprotect() {
            Some((id, fd)) => format!("{{\"id\":\"{id}\",\"fd\":\"{fd}\"}}"),
            None => "{\"id\":\"\",\"fd\":\"\"}".into(),
        },
        None => "{\"id\":\"\",\"fd\":\"\"}".into(),
    }
}

/// R1: the shell's reprotect answer document. Unknown id → `request-expired`;
/// no installed supply → `socket-supply-closed`; invalid id → `invalid-id` —
/// the same vocabulary the supply reply already uses.
pub fn reprotect_reply_json(id: &str, result: &str, errno: i32, protect_ms: i32) -> String {
    let Ok(id) = id.parse::<u64>() else { return "{\"ok\":false,\"error\":\"invalid-id\"}".into(); };
    slot().lock().expect("socket supply slot").as_ref()
        .map(|supply| supply.reprotect_reply(id, result, errno, protect_ms))
        .unwrap_or_else(|| "{\"ok\":false,\"error\":\"socket-supply-closed\"}".into())
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

/// R2 (design `analysis-20261026-dns-structural` §4.2): the fast-fail STATE of
/// the protected path, at the supply that owns the request queue.
#[cfg(test)]
mod r2_tests {
    use super::*;
    use std::os::fd::{AsRawFd, FromRawFd};

    fn datagram_fd() -> OwnedFd {
        let raw = unsafe { crate::sys::socket(crate::sys::AF_INET, crate::sys::SOCK_DGRAM, 0) };
        assert!(raw >= 0, "socket(2) for a protected fd");
        unsafe { OwnedFd::from_raw_fd(raw) }
    }

    async fn next_request(supply: &SocketSupply) -> (u64, Purpose) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some((id, purpose)) = supply.poll_request() {
                    return (id, purpose);
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("a queued supply request")
    }

    /// R2: the refusal is a WINDOW, not a permanent latch — the latch stays
    /// (it is what bounds the OS stage) while `refuse_dns` ends, so the next
    /// DNS request is a PROBE that can clear the state.
    #[test]
    fn the_latch_and_the_refusal_window_are_two_different_facts() {
        let supply = Arc::new(SocketSupply::new());
        supply.mark_unusable(PROTECTED_PATH_UNUSABLE);
        assert!(supply.protected_path_unusable());
        assert!(supply.path_state().refuse_dns);
        supply.set_unusable_probe_window(Duration::ZERO);
        assert!(supply.protected_path_unusable(), "the latch is still set");
        assert!(!supply.path_state().refuse_dns, "the refusal window is over");
        supply.clear_unusable("protected-answer");
        assert!(!supply.protected_path_unusable(), "a protected answer clears the latch");
        assert!(supply.path_state().reason.is_empty());
    }

    /// R2: after the window a DNS request is served again (the probe) — the
    /// fast failure must never be able to lock protected DNS out forever.
    #[tokio::test]
    async fn a_dns_probe_after_the_window_is_served_and_clears_the_latch() {
        let supply = Arc::new(SocketSupply::new());
        supply.mark_unusable(PROTECTED_PATH_UNUSABLE);
        supply.set_unusable_probe_window(Duration::ZERO);
        let dial = tokio::spawn({
            let supply = supply.clone();
            async move { supply.request_with_id(Purpose::Dns).await }
        });
        let (id, purpose) = next_request(&supply).await;
        assert_eq!(purpose, Purpose::Dns);
        let fd = datagram_fd();
        assert!(supply.reply(id, fd.as_raw_fd(), "protected").contains("true"));
        let (_, delivered) = dial.await.expect("dial task").expect("the probe is served");
        drop(delivered);
        supply.clear_unusable("protected-answer");
        assert!(!supply.path_state().unusable);
        assert!(!supply.path_state().refuse_dns);
    }

    /// R2: the SHELL's own refusal reason (`protected-path-unusable`, produced
    /// by its liveness gate) reaches the core as the SAME terminal token as the
    /// core-side fast refusal — never as a transient `socket-supply-rejected`
    /// that would be retried against a path already known-unusable.
    #[tokio::test]
    async fn the_shells_dns_refusal_reason_maps_to_the_fast_fail_token() {
        let supply = Arc::new(SocketSupply::new());
        let dial = tokio::spawn({
            let supply = supply.clone();
            async move { supply.request_with_id(Purpose::Dns).await }
        });
        let (id, _purpose) = next_request(&supply).await;
        assert_eq!(supply.reply(id, -1, PROTECTED_PATH_UNUSABLE), "{\"ok\":false}");
        let refused = dial.await.expect("dial task").expect_err("the supply is refused");
        assert_eq!(refused.token(), SUPPLY_UNUSABLE_TOKEN);
    }
}
