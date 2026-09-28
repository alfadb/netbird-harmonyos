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
}

fn sockets() -> &'static Mutex<HashMap<RawFd, Protect>> {
    SOCKETS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn armed() -> bool { ARMED.load(Ordering::Acquire) }

pub fn arm(enabled: bool) {
    ARMED.store(enabled, Ordering::Release);
    sockets().lock().expect("socket trace").clear();
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

pub fn closed(fd: RawFd) {
    if armed() { sockets().lock().expect("socket trace").remove(&fd); }
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
        if meta.kind != "I" && meta.kind != "Wb" && meta.kind != "Ws" { map.remove(&fd); }
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

    #[test]
    fn receipt_has_all_join_keys_and_inert_default() {
        let meta = Protect { kind: "I".into(), request: "42".into(), original_fd: 7,
            generation: 2, net_id: 31, elapsed_ms: 9, ok: true, io_count: 0 };
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
            let mut meta = Protect { kind: kind.into(), request: "wg_12".into(), original_fd: 7,
                generation: 3, net_id: 21, elapsed_ms: 4, ok: true, io_count: 0 };
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
}
