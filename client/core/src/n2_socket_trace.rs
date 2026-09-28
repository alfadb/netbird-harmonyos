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

#[derive(Clone, Debug)]
struct Protect {
    kind: String,
    request: String,
    original_fd: RawFd,
    generation: u64,
    net_id: u64,
    elapsed_ms: u64,
    ok: bool,
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
/// N2 P2: session-period `N2_FD_SEND` lines share the `N2_TUN_DEST` rhythm —
/// a 5 s tick gate, then "counters changed or 30 s elapsed".
const FD_SEND_TICK_MS: u64 = 5_000;
const FD_SEND_PERIOD_MS: u64 = 30_000;

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

pub fn armed() -> bool { ARMED.load(Ordering::Acquire) }

pub fn arm(enabled: bool) {
    ARMED.store(enabled, Ordering::Release);
    sockets().lock().expect("socket trace").clear();
    *fd_send_state().lock().expect("fd send state") = FdSendState::default();
    // N2 T5a: the stimulus registry (and its connector sink) lives exactly as
    // long as one arming — a disarm leaves no fd and no endpoint behind.
    crate::n2_stimulus::reset();
}

pub fn protect(fd: RawFd, kind: &str, request: &str, generation: u64, net_id: u64, elapsed_ms: u64, ok: bool) {
    if !armed() { return; }
    if !matches!(kind, "M" | "S" | "R" | "D" | "I" | "Wb" | "Ws") { return; }
    sockets().lock().expect("socket trace").insert(fd, Protect {
        kind: kind.into(), request: request.into(), original_fd: fd,
        generation, net_id, elapsed_ms, ok, io_count: 0,
        post_connect_writes: 0, subsequent_receipts: 0, last_receipt_ms: 0,
        send_packets: 0, send_bytes: 0, reported_packets: 0, reported_bytes: 0,
    });
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

fn format_receipt(meta: &Protect, dup: RawFd, remote: SocketAddr, local: Option<SocketAddr>,
    mono_ms: u64, wall_ms: u128, status: &str, detail: &str, phase: &str) -> String {
    format!("N2_SOCK|phase={phase}|kind={}|request_id={}|fd={}|dup_fd={dup}|protect={}|protect_ms={}|protect_generation={}|first_mono_ms={mono_ms}|first_wall_ms={wall_ms}|local={}|remote={remote}|proto={}|net_id={}|status={}|detail={}|diag=true|is_evidence=false",
        meta.kind, clean(&meta.request), meta.original_fd, if meta.ok { "success" } else { "failed" },
        meta.elapsed_ms, meta.generation, local.map(|a| a.to_string()).unwrap_or_else(|| "unknown".into()),
        if meta.kind == "M" || meta.kind == "S" || meta.kind == "R" { "tcp" } else { "udp" },
        meta.net_id, clean(status), clean(detail))
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

/// N2 P3 — one COMPLETED write after `connect` on a connected outer TCP
/// socket (M/S/R: gRPC/HTTP-2 frames, the signal stream, the relay WS).
///
/// Emits the FIRST write after connect, then every
/// [`POST_CONNECT_RECEIPT_EVERY`] completed writes or every
/// [`POST_CONNECT_RECEIPT_MS`] (whichever comes first). Returns `None` — the
/// caller emits NOTHING — when unarmed, when `fd` has no registration, when
/// the class is not an outer TCP socket, or when the throttle says not due.
///
/// The line keeps the `N2_SOCK` join keys (`kind`, `fd` = shell original,
/// `dup_fd` = core-owned descriptor, `request_id`, `protect*`, `net_id`,
/// `local`, `remote`, `proto=tcp`, `diag`/`is_evidence`) and adds the fields
/// this receipt exists for: `bytes`, `mono_ms`, `wall_ms`. The bytes and the
/// order of the write itself are NOT touched by the receipt.
pub fn post_connect_write(fd: RawFd, remote: SocketAddr, bytes: usize) -> Option<String> {
    if !armed() { return None; }
    let now = sys::mono_ms();
    let mut map = sockets().lock().expect("socket trace");
    let meta = map.get_mut(&fd)?;
    if !matches!(meta.kind.as_str(), "M" | "S" | "R") { return None; }
    meta.post_connect_writes = meta.post_connect_writes.saturating_add(1);
    let due = meta.post_connect_writes == 1
        || meta.post_connect_writes % POST_CONNECT_RECEIPT_EVERY == 0
        || now.saturating_sub(meta.last_receipt_ms) >= POST_CONNECT_RECEIPT_MS;
    if !due { return None; }
    meta.last_receipt_ms = now;
    meta.subsequent_receipts = meta.subsequent_receipts.saturating_add(1);
    let copy = meta.clone();
    drop(map);
    let wall_ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |t| t.as_millis());
    Some(format!(
        "N2_SOCK|phase=subsequent_io|kind={}|request_id={}|fd={}|dup_fd={fd}|protect={}|protect_ms={}|protect_generation={}|local={}|remote={remote}|proto=tcp|net_id={}|bytes={bytes}|mono_ms={now}|wall_ms={wall_ms}|status=sent|detail=post_connect_write|diag=true|is_evidence=false",
        copy.kind, clean(&copy.request), copy.original_fd,
        if copy.ok { "success" } else { "failed" }, copy.elapsed_ms, copy.generation,
        local_addr(fd).map(|a| a.to_string()).unwrap_or_else(|| "unknown".into()),
        copy.net_id))
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
fn fd_send_line(meta: &Protect, dup: RawFd, mono_ms: u64) -> String {
    format!(
        "N2_FD_SEND|kind={}|fd={}|dup_fd={dup}|packets={}|bytes={}|mono_ms={mono_ms}|diag=true|is_evidence=false",
        meta.kind, meta.original_fd, meta.send_packets, meta.send_bytes)
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
            generation: 2, net_id: 31, elapsed_ms: 9, ok: true, io_count: 0,
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
            "|detail=post_connect_write|", "|diag=true|is_evidence=false"] {
            assert!(first.contains(field), "{field} in {first}");
        }
        assert!(post_connect_write(fd, remote, 1).is_none(), "throttled");
        assert_eq!(subsequent_receipts(fd), 1);
        // writes 3..=127 stay silent; the 128th completed write is due again
        for _ in 0..125 {
            assert!(post_connect_write(fd, remote, 1).is_none());
        }
        let again = post_connect_write(fd, remote, 2).expect("128th completed write");
        assert!(again.contains("|bytes=2|"), "{again}");
        assert_eq!(subsequent_receipts_for_original(fd), 2);

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
