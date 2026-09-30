//! Diagnostic-only outer-socket receipts. Never enabled by the collector arm.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{hilog, sys};

static ARMED: AtomicBool = AtomicBool::new(false);
static SOCKETS: OnceLock<Mutex<HashMap<RawFd, Protect>>> = OnceLock::new();
/// The most recent SUCCESSFUL registration of each class, as
/// `(dup_fd, original_fd, remote)` — the `N2_SOCK` join keys of the socket
/// production code is currently using. The P4 A-event trigger and the P5
/// sandwich read their fd/dup identity from HERE, so an event can never be
/// attributed to a descriptor no `N2_SOCK` receipt describes.
///
/// Every class enters on the completion that proves a live socket — its first
/// successful I/O — EXCEPT `Wb`: the base WG socket structurally never sends,
/// so no I/O completion will ever exist for it. `Wb` therefore enters at the
/// CORE TAKEOVER ([`adopted`]: `WgDevice::adopt`/`reattach_socket`, after the
/// duplicated descriptor was validated), i.e. exactly at the point the core
/// owns the base socket and would be able to send on it. That registration is
/// the identity the C/D switch baseline reads; it is dropped by the same
/// closes as every other class (a new takeover replaces it, [`closed`] of the
/// core-owned descriptor removes it, [`arm`] clears the map).
static LIVE: OnceLock<Mutex<HashMap<&'static str, (RawFd, RawFd, SocketAddr)>>> =
    OnceLock::new();

#[derive(Clone, Debug)]
struct Protect {
    kind: String,
    request: String,
    original_fd: RawFd,
    generation: u64,
    net_id: u64,
    elapsed_ms: u64,
    ok: bool,
    /// N2 C/D (Wb): the monotonic instant of the protect SETTLE this
    /// registration reports — the core's own clock when the shell handed it
    /// the protect verdict ([`protect`]). The `Wb` substitute receipt carries
    /// THIS instant and never the later takeover instant: a protect that
    /// settled before the pre HIT stays before it however late the core takes
    /// the socket over.
    protect_mono_ms: u64,
    io_count: u64,
    /// N2 P3 (M/S/R): completed writes AFTER `connect` on this descriptor
    /// (`io_count` above counts the connect call itself).
    post_connect_writes: u64,
    /// N2 P3: `phase=subsequent_io` receipts actually EMITTED for this
    /// descriptor — one increment per emitted line, so a host test observes
    /// the receipt itself and not the log channel. An UNARMED session has no
    /// registration at all: that is the zero-output negative control.
    subsequent_receipts: u64,
    /// N2 P3: throttle anchor (`sys::mono_ms` of the last post-connect
    /// receipt) for the "or every 30 s" rule.
    last_receipt_ms: u64,
    /// N2 P2 (E3): SUCCESSFUL WG `sendto` calls and their wire byte total on
    /// THIS descriptor — the per-fd counter `WgDeviceStats` does not provide.
    send_packets: u64,
    send_bytes: u64,
    /// N2 P2: the counters as last reported by an `N2_FD_SEND` line (the
    /// change detector of the session-period cadence).
    reported_packets: u64,
    reported_bytes: u64,
}

/// N2 P3: one post-connect receipt at the first write, then every this many
/// completed writes (plus the 30 s time rule below).
const POST_CONNECT_RECEIPT_EVERY: u64 = 128;
/// N2 P3: …or one post-connect receipt every 30 s of an active write stream.
const POST_CONNECT_RECEIPT_MS: u64 = 30_000;

/// N2 T7a (item 4): the fd → event-id windows of in-flight A-column events.
/// A write landing inside an open window is receipted UNCONDITIONALLY (the
/// throttle below is bypassed) and its receipt carries `event_id=<id>`, so the
/// one production write a trigger causes is always provable even when it is
/// not the first/every-128th/30-s write of its descriptor.
static EVENT_WINDOWS: OnceLock<Mutex<HashMap<RawFd, String>>> = OnceLock::new();

fn event_windows() -> &'static Mutex<HashMap<RawFd, String>> {
    EVENT_WINDOWS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Open the event window of `fd` under the A-event id `id`. Keyed by the
/// CORE-OWNED descriptor (the `dup_fd` of the receipts), the same number the
/// write wrapper passes to [`post_connect_write`]. A negative fd (no live
/// registration) opens nothing.
pub fn event_window_begin(fd: RawFd, id: &str) {
    if fd < 0 || id.is_empty() { return; }
    event_windows().lock().expect("event windows").insert(fd, id.to_string());
}

/// Close the event window of `fd` (called after the event's trigger returned
/// or its watch completed/timed out).
pub fn event_window_end(fd: RawFd) {
    if fd < 0 { return; }
    event_windows().lock().expect("event windows").remove(&fd);
}

/// The event id of `fd` while a window is open (`None` = outside every event
/// window).
pub fn event_window(fd: RawFd) -> Option<String> {
    event_windows().lock().expect("event windows").get(&fd).cloned()
}

/// N2 P2: session-period `N2_FD_SEND` lines share the `N2_TUN_DEST` rhythm —
/// a 5 s tick gate, then "counters changed or 30 s elapsed".
const FD_SEND_TICK_MS: u64 = 5_000;
const FD_SEND_PERIOD_MS: u64 = 30_000;

/// The seven frozen outer classes (preregistration v4 §1). The closed list
/// behind `protect`'s acceptance and the live-socket registry.
pub const KINDS: [&str; 7] = ["M", "S", "R", "D", "I", "Wb", "Ws"];

/// N2 P2 cadence state (armed-only; reset on every arming change).
#[derive(Default)]
struct FdSendState {
    initialized: bool,
    last_tick_ms: u64,
    last_emit_ms: u64,
}

static FD_SEND_STATE: OnceLock<Mutex<FdSendState>> = OnceLock::new();

fn sockets() -> &'static Mutex<HashMap<RawFd, Protect>> {
    SOCKETS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn fd_send_state() -> &'static Mutex<FdSendState> {
    FD_SEND_STATE.get_or_init(|| Mutex::new(FdSendState::default()))
}

fn live_map() -> &'static Mutex<HashMap<&'static str, (RawFd, RawFd, SocketAddr)>> {
    LIVE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The live outer socket of a class: `(dup_fd, original_fd, remote)`. `None`
/// until the class's liveness point registered one — a successful `first_io`
/// for every sending class, the core takeover ([`adopted`]) for `Wb`, which
/// structurally never sends — and again after that descriptor closes (a closed
/// number is never reported).
pub fn live_of(kind: &str) -> Option<(RawFd, RawFd, SocketAddr)> {
    live_map().lock().expect("live sockets").get(kind).copied()
}

/// The IDENTITY of a class's live registration as the `N2_SOCK` receipts name
/// it: `(fd = shell original, dup_fd = core-owned descriptor,
/// protect_generation, net_id)`.
///
/// This is the read-only seam the N2 C/D switch report needs — the shell
/// measures the platform net id, but the `dup_fd` is core-owned, so the
/// baseline the receipt verdict must differ from can only come from here. It is
/// the SAME tuple the `N2_SOCK` receipt of that registration carries (`fd=`,
/// `dup_fd=`, `protect_generation=`, `net_id=`), never a shell self-report.
///
/// `None` while unarmed, and while the class has no live registration (no
/// successful I/O yet — for `Wb`, no completed core takeover yet): the caller
/// then reports "not measured" and the plan fail-closes instead of guessing a
/// baseline.
pub fn live_identity(kind: &str) -> Option<(RawFd, RawFd, u64, u64)> {
    let (dup_fd, original_fd, _) = live_of(kind)?;
    let map = sockets().lock().expect("socket trace");
    let meta = map.get(&dup_fd)?;
    Some((original_fd, dup_fd, meta.generation, meta.net_id))
}

pub fn armed() -> bool { ARMED.load(Ordering::Acquire) }

pub fn arm(enabled: bool) {
    ARMED.store(enabled, Ordering::Release);
    sockets().lock().expect("socket trace").clear();
    live_map().lock().expect("live sockets").clear();
    event_windows().lock().expect("event windows").clear();
    *fd_send_state().lock().expect("fd send state") = FdSendState::default();
    // N2 T5a: the stimulus registry (and its connector sink) lives exactly as
    // long as one arming — a disarm leaves no fd and no endpoint behind.
    crate::n2_stimulus::reset();
    // N2 P4/P5: the A-event counters/trigger and the sandwich state live
    // exactly as long as one arming too.
    crate::n2_a_event::reset();
    crate::n2_sandwich::reset();
    // N2 C/D first batch: the switching-sandwich plan and the app-internal
    // evidence ring live exactly as long as one arming — a disarm leaves no
    // plan behind and stops the ring (the FILE stays for the operator to
    // fetch).
    crate::n2_cd::reset();
    if !enabled {
        crate::n2_ring::disable();
    }
}

pub fn protect(fd: RawFd, kind: &str, request: &str, generation: u64, net_id: u64, elapsed_ms: u64, ok: bool) {
    if !armed() { return; }
    if !KINDS.contains(&kind) { return; }
    let meta = Protect {
        kind: kind.into(), request: request.into(), original_fd: fd,
        generation, net_id, elapsed_ms, ok, protect_mono_ms: sys::mono_ms(), io_count: 0,
        post_connect_writes: 0, subsequent_receipts: 0, last_receipt_ms: 0,
        send_packets: 0, send_bytes: 0, reported_packets: 0, reported_bytes: 0,
    };
    sockets().lock().expect("socket trace").insert(fd, meta);
    // N2 C/D wiring: a bare protect is NOT a receipt. The shell has not handed
    // the socket to the core yet, so at this instant no core-owned descriptor
    // exists and the receipt's `dup_fd` could only be the shell's own number —
    // which is not the taken-over chain. `Wb`'s substitute receipt is fed by
    // [`adopted`] instead, at the takeover that actually produced the core
    // descriptor, carrying this call's protect settle instant.
}

/// N2 C/D (`Wb`): the core has TAKEN OVER the shell-protected socket `from`
/// into its own descriptor `to` — `WgDevice::adopt` / `reattach_socket`,
/// called only after the duplicated descriptor was validated. Mirrors the
/// registration exactly as [`duplicated`] does and then, for an armed `Wb`
/// registration:
///
/// * publishes the class's live base-fd identity under the CORE-OWNED
///   `to`/shell-original/`protect_generation` triple — `Wb` never sends, so
///   no successful I/O (and therefore no `first_io` registration) will ever
///   exist for it;
/// * feeds the switching sandwich's substitute receipt with the ADOPTED
///   descriptor as `dup_fd` and the registration's own protect-settle instant
///   (never this takeover's clock) as the receipt time;
/// * reports the class's OWN real `N2_SOCK|phase=protect_settle` row — the
///   frozen E1 substitute-evidence row — on EVERY `Wb` takeover, whether or not
///   a C/D plan measures this class (see [`takeover`]).
///
/// A protect that FAILED publishes no identity and reports `status=failed` (it
/// can never be presented as a success); its receipt is fed with
/// `protect_ok=false` so the state machine refuses it by name (`protect-failed`)
/// instead of silently having no line. A dup / feed / adopt that failed never
/// reaches this call at all — so no row of such a socket can exist.
pub fn adopted(from: RawFd, to: RawFd) {
    if let Some(line) = takeover(from, to) {
        #[cfg(test)]
        LAST_ADOPT_LINE.with(|cell| *cell.borrow_mut() = Some(line.clone()));
        hilog::emit(&line);
    }
}

// Host-test observation of the row `adopted` emitted: `hilog::emit` writes to
// the platform channel (stderr on the host) and is not readable in-process, so
// the takeover stores its OWN row here. Production never touches it.
#[cfg(test)]
thread_local! {
    static LAST_ADOPT_LINE: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// The row the last [`adopted`] on THIS thread emitted (`None` = none) — the
/// host-test observation point of the emitted protect-settle row itself.
#[cfg(test)]
pub fn last_adopt_line() -> Option<String> {
    LAST_ADOPT_LINE.with(|cell| cell.borrow().clone())
}

/// The takeover body behind [`adopted`], returning the row it reports;
/// [`adopted`] emits that row and (host tests only) stores it for
/// [`last_adopt_line`].
///
/// `None` = nothing to report: unarmed, no registration for `from`, or a
/// registration of another class (whose liveness is a real I/O, so its own
/// `N2_SOCK` row comes from [`first_io`]).
fn takeover(from: RawFd, to: RawFd) -> Option<String> {
    if !armed() { return None; }
    let meta = {
        let mut map = sockets().lock().expect("socket trace");
        let source = map.get(&from).cloned();
        map.remove(&to);
        match source {
            Some(source) => { map.insert(to, source.clone()); Some(source) }
            None => None,
        }
    };
    let Some(meta) = meta else { return None };
    if meta.kind != "Wb" { return None; }
    if meta.ok {
        // The base socket is unconnected (its destination is per peer), so the
        // live entry's `remote` carries no meaning for `Wb`; no A-event class
        // reads it. The identity the C/D switch baseline needs is fd/dup/gen.
        live_map().lock().expect("live sockets").insert(
            "Wb",
            (to, meta.original_fd, SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)),
        );
    }
    let local = local_addr(to).map(|addr| addr.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    if crate::n2_cd::awaiting_kind("Wb") {
        feed_cd_receipt(&meta, to, meta.protect_mono_ms, false, true, "protect_settle", "unknown",
            &local);
    }
    // N2 frozen E1 (Wb substitute evidence): `fd` is the shell ORIGINAL,
    // `dup_fd` the VALIDATED core-owned descriptor, `proto=17` as the frozen E1
    // spells it, and the verdict is the ACTUAL protect outcome (a failed protect
    // reports `status=failed` and, above, publishes no live identity). The time
    // is the registration's own protect-settle instant, never this takeover's
    // later clock.
    Some(sock_receipt_line(&meta.kind, &meta.request, meta.original_fd, to, meta.ok,
        meta.elapsed_ms, meta.generation, meta.protect_mono_ms, 0, &local, "unknown", "17",
        meta.net_id, if meta.ok { "success" } else { "failed" }, "core-adopt",
        "protect_settle"))
}

pub fn duplicated(from: RawFd, to: RawFd) {
    if !armed() { return; }
    let mut map = sockets().lock().expect("socket trace");
    let source = map.get(&from).cloned();
    map.remove(&to);
    if let Some(source) = source { map.insert(to, source); }
}

pub fn selected_wg(fd: RawFd) {
    if !armed() { return; }
    if let Some(meta) = sockets().lock().expect("socket trace").get_mut(&fd) {
        if meta.kind == "I" {
            meta.kind = "Ws".into();
            meta.io_count = 0;
        }
    }
}

/// A descriptor is gone: drop its registration, and — N2 P2 — report the
/// FINAL per-fd send counters of a WG class (`Wb`/`Ws`) before removing it.
/// The close-time line is the last chance to read the counter, so an operator
/// sees the end-of-life total even when no session-period line followed the
/// last send.
pub fn closed(fd: RawFd) {
    if !armed() { return; }
    // N2 P4/P5: a closed descriptor is no longer a live outer socket — the
    // A-event trigger must never report a number that has been reused.
    live_map().lock().expect("live sockets").retain(|_, (dup, _, _)| *dup != fd);
    let line = {
        let mut map = sockets().lock().expect("socket trace");
        let line = map
            .get(&fd)
            .filter(|meta| matches!(meta.kind.as_str(), "Wb" | "Ws"))
            .map(|meta| fd_send_line(meta, fd, sys::mono_ms()));
        map.remove(&fd);
        line
    };
    if let Some(line) = line { hilog::emit(&line); }
}

pub fn overlay_local(addr: SocketAddr) -> bool {
    matches!(addr.ip(), IpAddr::V4(ip) if ip.octets()[0..2] == [100, 108])
}

pub fn local_addr(fd: RawFd) -> Option<SocketAddr> {
    let mut addr = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut len = std::mem::size_of::<sys::sockaddr_in>() as u32;
    if unsafe { sys::getsockname(fd, &mut addr, &mut len) } != 0 { return None; }
    Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(addr.sin_addr)), u16::from_be(addr.sin_port)))
}

/// The shell ORIGINAL fd registered for a core-owned descriptor (`dup_fd` in
/// the `N2_SOCK` receipts). `None` = the armed trace has no registration for
/// this descriptor, so no original can be reported for it.
pub fn original_fd(fd: RawFd) -> Option<RawFd> {
    sockets().lock().expect("socket trace").get(&fd).map(|meta| meta.original_fd)
}

fn clean(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-' | '_') { c } else { '_' }).collect()
}

/// The `N2_SOCK` receipt line with every volatile input explicit — the pure
/// formatter behind [`format_receipt`], the `Wb` takeover row ([`takeover`]) and
/// the N2 log-format baseline (N2 T7a item 8).
///
/// The time fields are phase-dependent. An I/O completion (`first_io` /
/// `subsequent_io` / a failed dial) reports its completion instants as
/// `first_mono_ms`/`first_wall_ms`. The `Wb` `phase=protect_settle` row is the
/// one receipt whose only provable instant is the core's monotonic clock at the
/// shell's protect VERDICT ([`protect`], `meta.protect_mono_ms`): the takeover
/// that reports it happens strictly LATER, so an adopt-time wall clock could
/// never be presented as the protect wall. That phase therefore carries
/// `protect_mono_ms=<that instant>` — the field the frozen E1 ordering compares
/// against the tee's own `sys::mono_ms` anchors — and NO wall field at all.
#[allow(clippy::too_many_arguments)]
pub fn sock_receipt_line(kind: &str, request_id: &str, fd: RawFd, dup_fd: RawFd, protect_ok: bool,
    protect_ms: u64, protect_generation: u64, mono_ms: u64, wall_ms: u128, local: &str,
    remote: &str, proto: &str, net_id: u64, status: &str, detail: &str, phase: &str) -> String {
    let time = if phase == "protect_settle" {
        format!("protect_mono_ms={mono_ms}")
    } else {
        format!("first_mono_ms={mono_ms}|first_wall_ms={wall_ms}")
    };
    format!("N2_SOCK|phase={phase}|kind={kind}|request_id={}|fd={fd}|dup_fd={dup_fd}|protect={}|protect_ms={protect_ms}|protect_generation={protect_generation}|{time}|local={local}|remote={remote}|proto={proto}|net_id={net_id}|status={}|detail={}|diag=true|is_evidence=false",
        clean(request_id), if protect_ok { "success" } else { "failed" },
        clean(status), clean(detail))
}

/// The `phase=subsequent_io` post-connect write receipt with every volatile
/// input explicit — the pure formatter behind [`post_connect_write`] (N2 T7a
/// items 4/6/8): `event_id` is the A-event id while a window is open (`none`
/// outside every window).
#[allow(clippy::too_many_arguments)]
pub fn subsequent_io_line(kind: &str, request_id: &str, fd: RawFd, dup_fd: RawFd, protect_ok: bool,
    protect_ms: u64, protect_generation: u64, local: &str, remote: &str, net_id: u64, bytes: usize,
    mono_ms: u64, wall_ms: u128, event_id: &str) -> String {
    format!("N2_SOCK|phase=subsequent_io|kind={kind}|request_id={}|fd={fd}|dup_fd={dup_fd}|protect={}|protect_ms={protect_ms}|protect_generation={protect_generation}|local={local}|remote={remote}|proto=tcp|net_id={net_id}|bytes={bytes}|mono_ms={mono_ms}|wall_ms={wall_ms}|status=sent|detail=post_connect_write|event_id={event_id}|diag=true|is_evidence=false",
        clean(request_id), if protect_ok { "success" } else { "failed" })
}

fn format_receipt(meta: &Protect, dup: RawFd, remote: SocketAddr, local: Option<SocketAddr>,
    mono_ms: u64, wall_ms: u128, status: &str, detail: &str, phase: &str) -> String {
    sock_receipt_line(&meta.kind, &meta.request, meta.original_fd, dup, meta.ok, meta.elapsed_ms,
        meta.generation, mono_ms, wall_ms,
        &local.map(|a| a.to_string()).unwrap_or_else(|| "unknown".into()),
        &remote.to_string(),
        if meta.kind == "M" || meta.kind == "S" || meta.kind == "R" { "tcp" } else { "udp" },
        meta.net_id, status, detail, phase)
}

fn receipt_due(meta: &Protect) -> bool {
    !matches!(meta.kind.as_str(), "Wb" | "Ws") || meta.io_count == 0 ||
        (meta.io_count - 1) % 128 == 0
}

pub fn io_begin(fd: RawFd, remote: SocketAddr) {
    if !armed() { return; }
    let meta = sockets().lock().expect("socket trace").get(&fd).cloned();
    let Some(meta) = meta else { return };
    if !receipt_due(&meta) { return; }
    let wall_ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |t| t.as_millis());
    let phase = if meta.io_count == 0 { "first_io_begin" } else { "subsequent_io_begin" };
    hilog::emit(&format!("N2_SOCK|phase={phase}|kind={}|request_id={}|fd={}|dup_fd={fd}|remote={remote}|mono_ms={}|wall_ms={wall_ms}|net_id={}|diag=true|is_evidence=false",
        meta.kind, clean(&meta.request), meta.original_fd, sys::mono_ms(), meta.net_id));
}

pub fn first_io(fd: RawFd, remote: SocketAddr, status: &str, detail: &str) {
    if !armed() { return; }
    let (meta, subsequent, emit_receipt) = {
        let mut map = sockets().lock().expect("socket trace");
        let Some(meta) = map.get_mut(&fd) else { return };
        let subsequent = meta.io_count > 0;
        let emit_receipt = receipt_due(meta);
        meta.io_count = meta.io_count.saturating_add(1);
        let copy = meta.clone();
        // N2 P3: a CONNECTED outer TCP fd (M/S/R) stays registered until its
        // close so post-connect writes can still be attributed to it — the
        // stream wrapper calls `closed()` when the descriptor goes away. A
        // FAILED connect closed its number immediately (never inspect a
        // reused number), and D is the one-shot protected-DNS UDP socket
        // whose entry must not outlive its query.
        if meta.kind == "D"
            || (status == "failed" && matches!(meta.kind.as_str(), "M" | "S" | "R"))
        {
            map.remove(&fd);
        }
        (copy, subsequent, emit_receipt)
    };
    // N2 T5a: the SAME completion that produces the `N2_SOCK` receipt is the
    // stimulus registry's registration point for this class — one call site,
    // so "the re-dial produced a new N2_SOCK registration" and "the stimulus
    // has a new target" are the same observation. Failed I/O registers
    // nothing. (The trace lock is already released here.)
    if status != "failed" {
        crate::n2_stimulus::observe(&meta.kind, fd, remote);
        // N2 P4/P5: THIS same call site feeds the live-socket registry (the
        // fd/dup identity the `N2_A_EVENT`/`N2_SANDWICH` lines report) and the
        // sandwich's first I/Ws send observation — so "the `N2_SOCK` receipt
        // exists" and "the A trigger has an fd" are one and the same fact.
        if let Some(key) = KINDS.iter().copied().find(|known| *known == meta.kind) {
            live_map().lock().expect("live sockets").insert(key, (fd, meta.original_fd, remote));
        }
        crate::n2_sandwich::observe_io(&meta.kind, fd, meta.original_fd, remote, status);
        // N2 C/D wiring: the SAME successful completion is the switching
        // sandwich's receipt candidate — the facts are THIS call's own
        // arguments (identity/net/protect/io time), never a shell self-report.
        // `M`/`S`/`R` are refused by name here (`connect-only`): their receipt
        // is the post-connect WRITE, below.
        if crate::n2_cd::awaiting_kind(&meta.kind) {
            let local = local_addr(fd).map(|addr| addr.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            feed_cd_receipt(&meta, fd, sys::mono_ms(), false, false, detail,
                &remote.to_string(), &local);
        }
    }
    if !emit_receipt { return; }
    // Failed connect may already have closed the owned fd; never inspect a reused number.
    let local = if status == "failed" { None } else { local_addr(fd) };
    let mono_ms = sys::mono_ms();
    let wall_ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |t| t.as_millis());
    let phase = if subsequent { "subsequent_io" } else { "first_io" };
    hilog::emit(&format_receipt(&meta, fd, remote, local, mono_ms, wall_ms, status, detail, phase));
    if let Some(local) = local.filter(|a| overlay_local(*a)) {
        hilog::emit(&format!("N2_SOCK_OVERLAY_LOCAL|kind={}|request_id={}|fd={}|dup_fd={fd}|local={local}|remote={remote}|diag=true|is_evidence=false",
            meta.kind, clean(&meta.request), meta.original_fd));
    }
}

/// N2 fd-reuse window: a registration lifted OUT of the fd table by
/// [`detach`] while its number is still ours. `TcpSocket::connect` consumes
/// the socket, so a failed dial releases the number before the dialer
/// resumes; the failure completion ([`dial_failed`]) and the success path
/// ([`reattach`]) therefore work from this snapshot and never look the number
/// up again.
pub struct Detached {
    fd: RawFd,
    meta: Protect,
}

/// Remove and return the registration of `fd` (`None` while unarmed or when
/// the number holds no registration). The descriptor itself is untouched —
/// the caller still owns it and must close it.
pub fn detach(fd: RawFd) -> Option<Detached> {
    if !armed() { return None; }
    sockets().lock().expect("socket trace").remove(&fd).map(|meta| Detached { fd, meta })
}

/// Put a [`Detached`] registration back under its number: the dial SUCCEEDED
/// and the returned stream owns exactly that descriptor.
pub fn reattach(detached: Option<Detached>) {
    if !armed() { return; }
    if let Some(Detached { fd, meta }) = detached {
        sockets().lock().expect("socket trace").insert(fd, meta);
    }
}

/// Complete a FAILED dial from the registration [`detach`]ed before the
/// descriptor was released: emit exactly the `status=failed` receipt the
/// fd-keyed [`first_io`] would have emitted for it, and return that line
/// (the unit test's observation point — the production caller ignores it).
/// A failed I/O registers nothing: no stimulus, no live socket, no sandwich
/// observation, and the registration is already gone (it was detached while
/// the number was still ours).
///
/// The fd TABLE is deliberately never read here — the number is free again
/// and may already belong to another socket, so a lookup could mark THAT
/// socket failed and tear its registration out (`first_io`'s fd-keyed
/// removal).
pub fn dial_failed(detached: Option<Detached>, remote: SocketAddr, detail: &str) -> Option<String> {
    let Detached { fd, meta } = detached?;
    if !armed() || !receipt_due(&meta) { return None; }
    let phase = if meta.io_count > 0 { "subsequent_io" } else { "first_io" };
    // A failed connect never inspects the (already closed) number for a local
    // address — same rule as the fd-keyed path.
    let mono_ms = sys::mono_ms();
    let wall_ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |t| t.as_millis());
    let line = format_receipt(&meta, fd, remote, None, mono_ms, wall_ms, "failed", detail, phase);
    hilog::emit(&line);
    Some(line)
}

/// N2 P3 — one COMPLETED write after `connect` on a connected outer TCP
/// socket (M/S/R: gRPC/HTTP-2 frames, the signal stream, the relay WS).
///
/// Emits the FIRST write after connect, then every
/// [`POST_CONNECT_RECEIPT_EVERY`] completed writes or every
/// [`POST_CONNECT_RECEIPT_MS`] (whichever comes first). N2 T7a: a write inside
/// an open A-event window ([`event_window_begin`]) is ALWAYS emitted — the
/// throttle is bypassed and the line carries `event_id=<id>`. Returns `None` —
/// the caller emits NOTHING — when unarmed, when `fd` has no registration,
/// when the class is not an outer TCP socket, or when the throttle says not
/// due.
///
/// The line keeps the `N2_SOCK` join keys (`kind`, `fd` = shell original,
/// `dup_fd` = core-owned descriptor, `request_id`, `protect*`, `net_id`,
/// `local`, `remote`, `proto=tcp`, `diag`/`is_evidence`) and adds the fields
/// this receipt exists for: `bytes`, `mono_ms`, `wall_ms`, `event_id`
/// (`none` outside every event window). The bytes and the order of the write
/// itself are NOT touched by the receipt.
pub fn post_connect_write(fd: RawFd, remote: SocketAddr, bytes: usize) -> Option<String> {
    if !armed() { return None; }
    let now = sys::mono_ms();
    let (copy, due, event_id) = {
        let mut map = sockets().lock().expect("socket trace");
        let meta = map.get_mut(&fd)?;
        if !matches!(meta.kind.as_str(), "M" | "S" | "R") { return None; }
        meta.post_connect_writes = meta.post_connect_writes.saturating_add(1);
        // N2 T7a (item 4): inside an A-event window the receipt is MANDATORY.
        let event_id = event_window(fd);
        let due = event_id.is_some()
            || meta.post_connect_writes == 1
            || meta.post_connect_writes % POST_CONNECT_RECEIPT_EVERY == 0
            || now.saturating_sub(meta.last_receipt_ms) >= POST_CONNECT_RECEIPT_MS;
        if due {
            meta.last_receipt_ms = now;
            meta.subsequent_receipts = meta.subsequent_receipts.saturating_add(1);
        }
        (meta.clone(), due, event_id)
    };
    // N2 C/D wiring: a COMPLETED business write after the connect is the
    // `subsequent_io` receipt the `M`/`S`/`R` rows require. It is fed from THIS
    // call's own facts and independently of the `N2_SOCK` throttle (`due`), so
    // a throttled receipt line can never hide the one receipt the sandwich
    // needs. The `getsockname` is only taken when either of the two wants it.
    let cd = crate::n2_cd::awaiting_kind(&copy.kind);
    let local = if due || cd {
        local_addr(fd).map(|a| a.to_string()).unwrap_or_else(|| "unknown".into())
    } else {
        "unknown".to_string()
    };
    if cd {
        feed_cd_receipt(&copy, fd, now, true, false, "post_connect_write", &remote.to_string(),
            &local);
    }
    if !due { return None; }
    let wall_ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |t| t.as_millis());
    Some(subsequent_io_line(&copy.kind, &copy.request, copy.original_fd, fd, copy.ok,
        copy.elapsed_ms, copy.generation, &local, &remote.to_string(), copy.net_id, bytes, now,
        wall_ms, &event_id.unwrap_or_else(|| "none".to_string())))
}

/// N2 C/D wiring — hand ONE trace observation of `meta`'s registration to the
/// switching sandwich as a receipt candidate.
///
/// The facts are this module's OWN (`Protect` plus the call site's arguments),
/// never a shell self-report: `original_fd` is the shell's protected number and
/// `dup_fd` the descriptor the trace keys the registration by, which is exactly
/// the `fd=`/`dup_fd=` pair of the `N2_SOCK` receipt the same observation
/// emits. The verdict — including every named refusal — is [`crate::n2_cd`]'s;
/// the callers gate on [`crate::n2_cd::awaiting_kind`] first, so nothing is
/// built (no `getsockname`, no lock) while no plan measures this class.
#[allow(clippy::too_many_arguments)]
fn feed_cd_receipt(meta: &Protect, dup_fd: RawFd, io_mono_ms: u64, subsequent_io: bool,
    protect_settle: bool, detail: &str, remote: &str, local: &str) {
    let receipt = crate::n2_cd::Receipt {
        identity: crate::n2_cd::Identity::new(meta.original_fd, dup_fd, meta.generation),
        net_id: meta.net_id,
        protect_ok: meta.ok,
        io_mono_ms,
        subsequent_io,
        protect_settle,
        detail,
        remote,
        local,
        request_id: &meta.request,
    };
    let _ = crate::n2_cd::receipt_candidate(io_mono_ms, &meta.kind, &receipt);
}

/// N2 P3: how many `phase=subsequent_io` receipts were emitted for the
/// registration under `fd` (`0` = none, including "no registration").
pub fn subsequent_receipts(fd: RawFd) -> u64 {
    sockets().lock().expect("socket trace").get(&fd).map_or(0, |m| m.subsequent_receipts)
}

/// N2 P3: the same count summed over every core-owned descriptor registered
/// against the shell ORIGINAL `original` — the host-test view, where only the
/// shell number is known (the internal dup is created inside the dial).
pub fn subsequent_receipts_for_original(original: RawFd) -> u64 {
    sockets()
        .lock()
        .expect("socket trace")
        .values()
        .filter(|meta| meta.original_fd == original)
        .map(|meta| meta.subsequent_receipts)
        .sum()
}

/// N2 P2 (E3) — count one SUCCESSFUL WG `sendto` on `fd`: one packet and its
/// wire bytes. Nothing is recorded for a failed `sendto` (nothing left the
/// socket) and nothing at all while unarmed.
pub fn wg_sent(fd: RawFd, bytes: u64) {
    if !armed() { return; }
    if let Some(meta) = sockets().lock().expect("socket trace").get_mut(&fd) {
        meta.send_packets = meta.send_packets.saturating_add(1);
        meta.send_bytes = meta.send_bytes.saturating_add(bytes);
    }
}

/// One `N2_FD_SEND` line for a WG-class descriptor (see [`fd_send_due_lines`]).
/// The fields stay exactly `packets`/`bytes`/`mono_ms` (N2 T7a item 6: the
/// judge aligns on this shape).
fn fd_send_line(meta: &Protect, dup: RawFd, mono_ms: u64) -> String {
    fd_send_fields_line(&meta.kind, meta.original_fd, dup, meta.send_packets,
        meta.send_bytes, mono_ms)
}

/// The same line with every field explicit — the pure formatter behind
/// [`fd_send_line`] (N2 T7a item 8).
pub fn fd_send_fields_line(kind: &str, fd: RawFd, dup_fd: RawFd, packets: u64, bytes: u64,
    mono_ms: u64) -> String {
    format!(
        "N2_FD_SEND|kind={kind}|fd={fd}|dup_fd={dup_fd}|packets={packets}|bytes={bytes}|mono_ms={mono_ms}|diag=true|is_evidence=false")
}

/// N2 P2 — the session-period `N2_FD_SEND` lines, one per REGISTERED WG-class
/// descriptor (`Wb` base / `Ws` selected). Every registration is reported,
/// including a zero-count one: the base socket's `packets=0` line is the
/// explicit no-send negative evidence this counter exists for.
///
/// Cadence follows `N2_TUN_DEST`: a [`FD_SEND_TICK_MS`] tick gate, then
/// "any counter changed or [`FD_SEND_PERIOD_MS`] elapsed". The first call
/// after arming is the baseline report (so the zero lines appear without
/// waiting a period). `now_ms` is the caller's (injected) session clock;
/// empty while unarmed.
pub fn fd_send_due_lines(now_ms: u64) -> Vec<String> {
    if !armed() { return Vec::new(); }
    let mut state = fd_send_state().lock().expect("fd send state");
    let first = !state.initialized;
    if !first && now_ms.saturating_sub(state.last_tick_ms) < FD_SEND_TICK_MS {
        return Vec::new();
    }
    state.initialized = true;
    state.last_tick_ms = now_ms;
    let mut map = sockets().lock().expect("socket trace");
    let changed = map.values().any(|meta| {
        matches!(meta.kind.as_str(), "Wb" | "Ws")
            && (meta.send_packets != meta.reported_packets
                || meta.send_bytes != meta.reported_bytes)
    });
    if !first && !changed && now_ms.saturating_sub(state.last_emit_ms) < FD_SEND_PERIOD_MS {
        return Vec::new();
    }
    state.last_emit_ms = now_ms;
    let mut keys: Vec<RawFd> = map
        .iter()
        .filter(|(_, meta)| matches!(meta.kind.as_str(), "Wb" | "Ws"))
        .map(|(fd, _)| *fd)
        .collect();
    keys.sort_unstable(); // deterministic line order (the map has none)
    let mut lines = Vec::with_capacity(keys.len());
    for fd in keys {
        if let Some(meta) = map.get_mut(&fd) {
            lines.push(fd_send_line(meta, fd, now_ms));
            meta.reported_packets = meta.send_packets;
            meta.reported_bytes = meta.send_bytes;
        }
    }
    lines
}

/// N2 P2: the counters registered for `fd` (`packets`, `bytes`); `None` = no
/// registration.
pub fn fd_send_totals(fd: RawFd) -> Option<(u64, u64)> {
    sockets()
        .lock()
        .expect("socket trace")
        .get(&fd)
        .map(|meta| (meta.send_packets, meta.send_bytes))
}

/// N2 P2: every WG-class descriptor registered against the shell ORIGINAL
/// `original` as `(dup_fd, packets, bytes)`, sorted by `dup_fd` — the
/// host-side view of the base/selected registration chain when only the
/// shell number is known (the internal dups are created inside the core).
pub fn fd_send_totals_for_original(original: RawFd) -> Vec<(RawFd, u64, u64)> {
    let map = sockets().lock().expect("socket trace");
    let mut rows: Vec<(RawFd, u64, u64)> = map
        .iter()
        .filter(|(_, meta)| {
            meta.original_fd == original && matches!(meta.kind.as_str(), "Wb" | "Ws")
        })
        .map(|(fd, meta)| (*fd, meta.send_packets, meta.send_bytes))
        .collect();
    rows.sort_unstable();
    rows
}

/// One production protected DNS query's D-row receipt facts (preregistration
/// v4 §4 D row: "the protected fd records the sent query and a same-ID
/// response"). `fd`/`dup_fd` follow the `N2_SOCK` convention: the shell
/// original and the core-owned descriptor.
#[derive(Clone, Debug)]
pub struct DnsReceipt {
    pub query_id: u16,
    pub qname: String,
    pub server: SocketAddr,
    pub fd: RawFd,
    pub dup_fd: RawFd,
    pub sent_mono_ms: u64,
    /// `matched` / `mismatched` / `timeout` / `error` (the query-id vs
    /// response-id comparison result; `mismatched` means a datagram arrived
    /// whose transaction id differs from the one that was sent).
    pub response: &'static str,
    /// Response header RCODE, `-1` when no response body was decoded.
    pub rcode: i32,
    /// Response header ANCOUNT, `-1` when no response body was decoded. The
    /// ANSWER CONTENT (addresses) is deliberately never logged.
    pub answer_count: i32,
    pub elapsed_ms: u128,
}

pub fn dns_receipt_line(r: &DnsReceipt) -> String {
    format!("N2_DNS|query_id={}|qname={}|server={}|fd={}|dup_fd={}|sent_mono_ms={}|response={}|rcode={}|answer_count={}|elapsed_ms={}|diag=true|is_evidence=false",
        r.query_id, clean(&r.qname), r.server, r.fd, r.dup_fd, r.sent_mono_ms,
        clean(r.response), r.rcode, r.answer_count, r.elapsed_ms)
}

/// The D-row receipt, gated exactly like every other `N2_` diagnostic: an
/// UNARMED session produces ZERO output for a production DNS query (`None`),
/// and the caller emits nothing.
pub fn dns_receipt(r: &DnsReceipt) -> Option<String> {
    if !armed() { return None; }
    Some(dns_receipt_line(r))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A registration literal with every counter at its start value.
    fn meta_of(kind: &str, original_fd: RawFd) -> Protect {
        Protect { kind: kind.into(), request: "42".into(), original_fd,
            generation: 2, net_id: 31, elapsed_ms: 9, ok: true, protect_mono_ms: 0,
            io_count: 0,
            post_connect_writes: 0, subsequent_receipts: 0, last_receipt_ms: 0,
            send_packets: 0, send_bytes: 0, reported_packets: 0, reported_bytes: 0 }
    }

    #[test]
    fn receipt_has_all_join_keys_and_inert_default() {
        let mut meta = meta_of("I", 7);
        meta.request = "42".into();
        meta.generation = 2;
        meta.net_id = 31;
        meta.elapsed_ms = 9;
        let line = format_receipt(&meta, 8, "192.0.2.1:3478".parse().unwrap(),
            Some("192.168.1.2:5000".parse().unwrap()), 123, 456, "sent", "txn_abcd", "first_io");
        for field in ["kind=I", "request_id=42", "fd=7", "dup_fd=8", "protect=success",
            "protect_ms=9", "protect_generation=2", "first_mono_ms=123", "first_wall_ms=456",
            "local=192.168.1.2:5000", "remote=192.0.2.1:3478", "proto=udp", "net_id=31",
            "diag=true|is_evidence=false"] { assert!(line.contains(field), "{field}"); }
    }

    #[test]
    fn protected_udp_dup_retains_identity_after_shell_close() {
        // The arming flag (and, through it, the N2 T5a stimulus registry) is
        // process-global: serialize against the other lib tests that mutate it.
        let _guard = crate::n2_stimulus::test_guard();
        use std::net::UdpSocket;
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        let original = UdpSocket::bind("127.0.0.1:0").unwrap();
        let fd = original.as_raw_fd();
        arm(true);
        protect(fd, "I", "91", 7, 28, 2, true);
        let dup = crate::mgmtsock::dup_socket_fd(fd).unwrap();
        let owned = unsafe { OwnedFd::from_raw_fd(dup) };
        closed(fd);
        assert_eq!(sockets().lock().unwrap().get(&dup).unwrap().original_fd, fd);
        assert_eq!(local_addr(dup), Some(original.local_addr().unwrap()));
        io_begin(dup, "192.0.2.1:3478".parse().unwrap());
        first_io(dup, "192.0.2.1:3478".parse().unwrap(), "sent", "stun_txn_test");
        assert_eq!(sockets().lock().unwrap().get(&dup).unwrap().io_count, 1);
        first_io(dup, "192.0.2.2:3478".parse().unwrap(), "sent", "stun_txn_again");
        assert_eq!(sockets().lock().unwrap().get(&dup).unwrap().io_count, 2);
        selected_wg(dup);
        let selected = sockets().lock().unwrap().get(&dup).unwrap().clone();
        assert_eq!(selected.kind, "Ws");
        assert_eq!(selected.original_fd, fd);
        assert_eq!(selected.io_count, 0);
        first_io(dup, "192.0.2.3:51820".parse().unwrap(), "sent", "wg_sendto");
        assert_eq!(sockets().lock().unwrap().get(&dup).unwrap().io_count, 1);
        closed(dup);
        assert!(!sockets().lock().unwrap().contains_key(&dup));
        drop(owned);
        arm(false);
    }

    #[test]
    fn wg_base_and_selected_receipts_preserve_dup_identity_and_throttle() {
        for kind in ["Wb", "Ws"] {
            let mut meta = meta_of(kind, 7);
            meta.request = "wg_12".into();
            meta.generation = 3;
            meta.net_id = 21;
            meta.elapsed_ms = 4;
            let line = format_receipt(&meta, 8, "192.0.2.1:51820".parse().unwrap(),
                Some("192.168.1.2:51000".parse().unwrap()), 123, 456, "sent", "wg_sendto", "first_io");
            for field in [format!("kind={kind}"), "fd=7|dup_fd=8".into(),
                "phase=first_io".into(), "detail=wg_sendto".into(), "proto=udp".into()] {
                assert!(line.contains(&field), "{field}");
            }
            assert!(receipt_due(&meta));
            meta.io_count = 1;
            assert!(receipt_due(&meta)); // first subsequent_io receipt
            meta.io_count = 2;
            assert!(!receipt_due(&meta));
            meta.io_count = 129;
            assert!(receipt_due(&meta));
        }
    }

    #[test]
    fn overlay_local_is_exact_prefix() {
        assert!(overlay_local("100.108.255.1:53".parse().unwrap()));
        assert!(!overlay_local("100.109.0.1:53".parse().unwrap()));
        assert!(!overlay_local("192.168.0.1:53".parse().unwrap()));
    }

    fn dns_receipt_for(response: &'static str, rcode: i32, answer_count: i32) -> DnsReceipt {
        DnsReceipt {
            query_id: 0x1234,
            qname: "relay.example".into(),
            server: "203.0.113.53:53".parse().unwrap(),
            fd: 41,
            dup_fd: 42,
            sent_mono_ms: 900,
            response,
            rcode,
            answer_count,
            elapsed_ms: 12,
        }
    }

    // N2 D-row (§4 D): the receipt is armed-only — an unarmed session must
    // produce ZERO output for a production query.
    #[test]
    fn dns_receipt_is_armed_only() {
        let _guard = crate::n2_stimulus::test_guard();
        arm(false);
        assert!(dns_receipt(&dns_receipt_for("matched", 0, 1)).is_none());
        assert!(dns_receipt(&dns_receipt_for("timeout", -1, -1)).is_none());
        arm(false);
    }

    // N2 D-row (§4 D): every documented field of a MATCHED query, including
    // the descriptor pair and the diagnostic tail.
    #[test]
    fn dns_receipt_matched_carries_every_field() {
        let _guard = crate::n2_stimulus::test_guard();
        arm(true);
        let line = dns_receipt(&dns_receipt_for("matched", 0, 2)).expect("armed");
        assert!(line.starts_with("N2_DNS|"), "{line}");
        for field in ["query_id=4660", "qname=relay.example", "server=203.0.113.53:53",
            "fd=41", "dup_fd=42", "sent_mono_ms=900", "response=matched", "rcode=0",
            "answer_count=2", "elapsed_ms=12", "diag=true|is_evidence=false"] {
            assert!(line.contains(field), "{field} in {line}");
        }
        arm(false);
    }

    // N2 D-row (§4 D): a datagram whose transaction id differs from the sent
    // one is `mismatched`; a silent server is `timeout`. Neither may be
    // rendered as a match, and a missing body keeps rcode/ANCOUNT honest (-1).
    #[test]
    fn dns_receipt_reports_mismatched_and_timeout_verdicts() {
        let _guard = crate::n2_stimulus::test_guard();
        arm(true);
        let mismatched = dns_receipt(&dns_receipt_for("mismatched", 0, 1)).expect("armed");
        assert!(mismatched.contains("|response=mismatched|rcode=0|answer_count=1|"), "{mismatched}");
        let timeout = dns_receipt(&dns_receipt_for("timeout", -1, -1)).expect("armed");
        assert!(timeout.contains("|response=timeout|rcode=-1|answer_count=-1|"), "{timeout}");
        assert!(!timeout.contains("response=matched"));
        arm(false);
    }

    // N2 D-row join key: the core-owned DNS descriptor reports the shell
    // original that the armed protect receipt registered for it.
    #[test]
    fn dns_descriptor_reports_the_registered_shell_original() {
        let _guard = crate::n2_stimulus::test_guard();
        arm(true);
        protect(7, "D", "42", 0, 31, 9, true);
        assert_eq!(original_fd(7), Some(7));
        duplicated(7, 8);
        assert_eq!(original_fd(8), Some(7));
        assert_eq!(original_fd(9), None);
        closed(8);
        arm(false);
    }

    // N2 C/D: the switch baseline is the LIVE registration's `N2_SOCK` triple
    // (`fd` = shell original, `dup_fd` = core-owned descriptor,
    // `protect_generation`) plus its net id — the read-only seam the shell's
    // `n2_cd_switch_identified` report is fed from. Unarmed, or without a
    // successful I/O on the registration, there is no identity: the plan then
    // fail-closes instead of receiving an invented baseline.
    #[test]
    fn live_identity_is_the_n2_sock_triple_of_the_live_registration() {
        let _guard = crate::n2_stimulus::test_guard();
        use std::net::UdpSocket;
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let fd = sock.as_raw_fd();
        let remote: SocketAddr = "192.0.2.1:443".parse().unwrap();
        arm(false);
        assert_eq!(live_identity("M"), None, "unarmed: no identity");
        arm(true);
        protect(fd, "M", "7", 3, 31, 1, true);
        assert_eq!(live_identity("M"), None, "a registration alone is not live");
        // The shell's fd is duplicated into the core-owned descriptor and the
        // shell's original goes away — the identity keeps both numbers.
        let dup = crate::mgmtsock::dup_socket_fd(fd).unwrap();
        let owned = unsafe { OwnedFd::from_raw_fd(dup) };
        closed(fd);
        first_io(dup, remote, "connected", "connect");
        assert_eq!(live_identity("M"), Some((fd, dup, 3, 31)));
        // Another class's live socket is a different identity.
        assert_eq!(live_identity("S"), None);
        closed(dup);
        assert_eq!(live_identity("M"), None, "a closed descriptor is never reported");
        drop(owned);
        arm(false);
    }

    // N2 C/D (`Wb`): the base socket structurally never sends, so its liveness
    // is the CORE TAKEOVER ([`adopted`]) — a bare protect registers nothing
    // live, a dup alone is still not the takeover, the FAILED protect publishes
    // no identity, and a closed core descriptor is dropped. Classes whose
    // liveness is a real I/O keep their `first_io` registration path.
    #[test]
    fn wb_live_identity_comes_from_the_takeover_and_dies_with_the_descriptor() {
        let _guard = crate::n2_stimulus::test_guard();
        use std::net::UdpSocket;
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let fd = sock.as_raw_fd();
        arm(false);
        assert_eq!(live_identity("Wb"), None, "unarmed: no identity");
        arm(true);
        protect(fd, "Wb", "wg", 4, 21, 2, true);
        assert_eq!(live_identity("Wb"), None, "a bare protect is not a takeover");
        let dup = crate::mgmtsock::dup_socket_fd(fd).unwrap();
        let owned = unsafe { OwnedFd::from_raw_fd(dup) };
        assert_eq!(live_identity("Wb"), None, "a mirrored dup is not the takeover either");
        adopted(fd, dup);
        assert_eq!(live_identity("Wb"), Some((fd, dup, 4, 21)));
        closed(dup);
        assert_eq!(live_identity("Wb"), None, "the core descriptor's close drops it");
        drop(owned);

        // A protect that FAILED publishes no identity, and `adopted` never
        // turns a non-`Wb` registration into a live socket.
        let failed = UdpSocket::bind("127.0.0.1:0").unwrap();
        let failed_fd = failed.as_raw_fd();
        protect(failed_fd, "Wb", "wg", 5, 21, 2, false);
        let failed_dup = crate::mgmtsock::dup_socket_fd(failed_fd).unwrap();
        let failed_owned = unsafe { OwnedFd::from_raw_fd(failed_dup) };
        adopted(failed_fd, failed_dup);
        assert_eq!(live_identity("Wb"), None, "a failed protect is not a live socket");
        closed(failed_dup);
        closed(failed_fd);
        drop(failed_owned);

        let tcp = UdpSocket::bind("127.0.0.1:0").unwrap();
        let tcp_fd = tcp.as_raw_fd();
        protect(tcp_fd, "M", "7", 4, 21, 2, true);
        let tcp_dup = crate::mgmtsock::dup_socket_fd(tcp_fd).unwrap();
        let tcp_owned = unsafe { OwnedFd::from_raw_fd(tcp_dup) };
        adopted(tcp_fd, tcp_dup);
        assert_eq!(live_identity("M"), None, "M still needs its own successful I/O");
        closed(tcp_dup);
        closed(tcp_fd);
        drop(tcp_owned);
        arm(false);
    }

    // N2 frozen E1 (Wb substitute evidence): EVERY core takeover of a protected
    // base socket reports its OWN real `N2_SOCK|phase=protect_settle` row — with
    // or without a C/D plan — carrying the shell ORIGINAL fd, the VALIDATED core
    // descriptor, `proto=17`, the ACTUAL protect verdict and the protect-settle
    // monotonic instant (`protect_mono_ms`, never the later takeover clock). A
    // failed protect reports failure and publishes no identity; the no-send
    // counters stay at zero.
    #[test]
    fn wb_takeover_reports_a_protect_settle_row_with_the_core_dup_and_its_own_settle_instant() {
        let _guard = crate::n2_stimulus::test_guard();
        use std::net::UdpSocket;
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let fd = sock.as_raw_fd();
        let port = sock.local_addr().unwrap().port();
        arm(true);
        protect(fd, "Wb", "wg_12", 4, 21, 2, true);
        let dup = crate::mgmtsock::dup_socket_fd(fd).unwrap();
        let owned = unsafe { OwnedFd::from_raw_fd(dup) };
        // No C/D plan measures this class: the row is NOT conditional on one.
        assert!(!crate::n2_cd::awaiting_kind("Wb"));
        let settle_mono = sockets().lock().unwrap().get(&dup).unwrap().protect_mono_ms;
        // Make the takeover clock provably LATER than the protect verdict, so a
        // takeover-time stamp could not have produced the row's time field.
        std::thread::sleep(std::time::Duration::from_millis(5));
        adopted(fd, dup);
        let line = last_adopt_line().expect("a Wb takeover emits its protect-settle row");
        assert!(line.starts_with("N2_SOCK|phase=protect_settle|kind=Wb|request_id=wg_12|"),
            "{line}");
        for field in [
            format!("|fd={fd}|dup_fd={dup}|"),
            "|protect=success|protect_ms=2|protect_generation=4|".to_string(),
            format!("|protect_mono_ms={settle_mono}|"),
            format!("|local=127.0.0.1:{port}|"),
            "|remote=unknown|proto=17|net_id=21|status=success|detail=core-adopt|".to_string(),
            "|diag=true|is_evidence=false".to_string(),
        ] {
            assert!(line.contains(&field), "{field} in {line}");
        }
        assert!(!line.contains("first_mono_ms") && !line.contains("wall_ms"),
            "no completion/adopt wall clock is claimed: {line}");
        assert!(sys::mono_ms() > settle_mono, "the emit clock is later than the verdict");
        // The identity is published and the no-send counters are untouched.
        assert_eq!(live_identity("Wb"), Some((fd, dup, 4, 21)));
        assert_eq!(fd_send_totals(dup), Some((0, 0)));
        closed(dup);
        closed(fd);
        drop(owned);

        // A FAILED protect is reported as a failure — never as a success — and
        // publishes no live identity.
        let failed = UdpSocket::bind("127.0.0.1:0").unwrap();
        let failed_fd = failed.as_raw_fd();
        protect(failed_fd, "Wb", "wg_13", 5, 21, 2, false);
        let failed_dup = crate::mgmtsock::dup_socket_fd(failed_fd).unwrap();
        let failed_owned = unsafe { OwnedFd::from_raw_fd(failed_dup) };
        adopted(failed_fd, failed_dup);
        let failed_line = last_adopt_line().expect("a failed protect is still reported");
        assert!(failed_line.contains("|protect=failed|protect_ms=2|"), "{failed_line}");
        assert!(failed_line.contains("|status=failed|detail=core-adopt|"), "{failed_line}");
        assert!(!failed_line.contains("status=success"), "{failed_line}");
        assert_eq!(live_identity("Wb"), None, "a failed protect publishes no identity");
        closed(failed_dup);
        closed(failed_fd);
        drop(failed_owned);
        arm(false);
    }

    // N2 P3: an armed connected M/S/R fd keeps its registration past the
    // connect receipt, the first post-connect write produces
    // `phase=subsequent_io` with the required fields, and later writes are
    // throttled (first + every 128). Unarmed: no registration, no receipt.
    #[test]
    fn tcp_post_connect_writes_are_armed_only_and_throttled() {
        let _guard = crate::n2_stimulus::test_guard();
        use std::net::UdpSocket;
        use std::os::fd::AsRawFd;
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let fd = sock.as_raw_fd();
        let remote: SocketAddr = "192.0.2.1:443".parse().unwrap();

        // Negative control: unarmed ⇒ the registration never exists and the
        // receipt is never produced.
        arm(false);
        protect(fd, "M", "7", 1, 31, 3, true);
        assert_eq!(original_fd(fd), None);
        assert!(post_connect_write(fd, remote, 64).is_none());
        assert_eq!(subsequent_receipts_for_original(fd), 0);

        arm(true);
        protect(fd, "M", "7", 1, 31, 3, true);
        first_io(fd, remote, "connected", "connect");
        // P3 fix: a CONNECTED TCP fd survives its connect receipt.
        assert_eq!(original_fd(fd), Some(fd));

        let first = post_connect_write(fd, remote, 64).expect("first post-connect write");
        assert!(first.starts_with("N2_SOCK|phase=subsequent_io|"), "{first}");
        for field in ["|kind=M|", "|request_id=7|", "|fd=", "|dup_fd=", "|protect=success|",
            "|protect_generation=1|", "|remote=192.0.2.1:443|", "|proto=tcp|", "|net_id=31|",
            "|bytes=64|", "|mono_ms=", "|wall_ms=", "|status=sent|",
            "|detail=post_connect_write|", "|event_id=none|", "|diag=true|is_evidence=false"] {
            assert!(first.contains(field), "{field} in {first}");
        }
        assert!(post_connect_write(fd, remote, 1).is_none(), "throttled");
        assert_eq!(subsequent_receipts(fd), 1);
        // N2 T7a (item 4): a write INSIDE an open event window is mandatory —
        // the throttle is bypassed and the receipt names the event id.
        event_window_begin(fd, "a-M-1");
        assert_eq!(event_window(fd).as_deref(), Some("a-M-1"));
        let forced = post_connect_write(fd, remote, 3).expect("forced inside the window");
        assert!(forced.contains("|bytes=3|"), "{forced}");
        assert!(forced.contains("|event_id=a-M-1|"), "{forced}");
        assert_eq!(subsequent_receipts(fd), 2);
        event_window_end(fd);
        assert_eq!(event_window(fd), None);
        assert!(post_connect_write(fd, remote, 1).is_none(), "throttle resumes");
        // writes 5..=127 stay silent; the 128th completed write is due again
        for _ in 0..123 {
            assert!(post_connect_write(fd, remote, 1).is_none());
        }
        let again = post_connect_write(fd, remote, 2).expect("128th completed write");
        assert!(again.contains("|bytes=2|"), "{again}");
        assert!(again.contains("|event_id=none|"), "{again}");
        assert_eq!(subsequent_receipts_for_original(fd), 3);

        // N2 T7a: a disarm drops every open window (no event outlives its arming).
        event_window_begin(fd, "a-M-2");
        arm(false);
        assert_eq!(event_window(fd), None);
        arm(true);
        assert_eq!(event_window(fd), None);

        // A class that is not an outer TCP socket never gets this receipt.
        protect(4_000_001, "Wb", "wg", 1, 31, 3, true);
        assert!(post_connect_write(4_000_001, remote, 9).is_none());
        closed(4_000_001);

        closed(fd);
        assert_eq!(original_fd(fd), None, "close removes the registration");
        arm(false);
    }

    // N2 P2 (E3): per-fd WG send counting. The base (`Wb`) descriptor is
    // reported even at zero — that zero line is the no-send negative
    // evidence — while the counters only ever move on the fd that sent.
    #[test]
    fn fd_send_lines_report_zero_count_base_and_per_fd_totals() {
        let _guard = crate::n2_stimulus::test_guard();
        use std::net::UdpSocket;
        use std::os::fd::AsRawFd;
        let base = UdpSocket::bind("127.0.0.1:0").unwrap();
        let selected = UdpSocket::bind("127.0.0.1:0").unwrap();
        let base_fd = base.as_raw_fd();
        let ws_fd = selected.as_raw_fd();

        // Unarmed: nothing is counted, nothing is reported.
        arm(false);
        wg_sent(base_fd, 100);
        assert_eq!(fd_send_totals(base_fd), None);
        assert!(fd_send_due_lines(1_000).is_empty());

        arm(true);
        protect(base_fd, "Wb", "wg", 1, 31, 2, true);
        protect(ws_fd, "I", "ice", 1, 31, 2, true);
        selected_wg(ws_fd); // the ICE dup handed to WG is a `Ws`
        let baseline = fd_send_due_lines(1_000);
        assert_eq!(baseline.len(), 2, "{baseline:?}");
        assert!(baseline.iter().any(|line| line.starts_with("N2_FD_SEND|kind=Wb|")
            && line.contains("|packets=0|bytes=0|")), "{baseline:?}");
        assert!(baseline.iter().any(|line| line.starts_with("N2_FD_SEND|kind=Ws|")
            && line.contains("|packets=0|bytes=0|")), "{baseline:?}");

        // 5 s tick gate, unchanged counters ⇒ no line.
        assert!(fd_send_due_lines(2_000).is_empty());

        // Sends happen on the SELECTED descriptor only.
        wg_sent(ws_fd, 144);
        wg_sent(ws_fd, 144);
        let next = fd_send_due_lines(6_001);
        let ws = next.iter().find(|line| line.starts_with("N2_FD_SEND|kind=Ws|"))
            .expect("Ws line");
        assert!(ws.contains("|packets=2|bytes=288|"), "{ws}");
        assert!(ws.contains(&format!("|mono_ms=6001|")), "{ws}");
        let wb = next.iter().find(|line| line.starts_with("N2_FD_SEND|kind=Wb|"))
            .expect("Wb line");
        assert!(wb.contains("|packets=0|bytes=0|"), "{wb}");
        assert!(wb.ends_with("|diag=true|is_evidence=false"), "{wb}");
        assert_eq!(fd_send_totals(ws_fd), Some((2, 288)));
        assert_eq!(fd_send_totals(base_fd), Some((0, 0)));
        assert_eq!(fd_send_totals(999_999), None);

        // fd close reports the final counter and drops the registration.
        closed(ws_fd);
        assert_eq!(fd_send_totals(ws_fd), None);
        arm(false);
    }
}
