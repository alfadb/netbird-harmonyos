// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! IPv4 A lookups over a fresh, shell-protected UDP socket per DNS server.
//! The protected path is the PRIMARY resolver, but since 0.0.34 (protected-DNS
//! outage REPORT.md §5 N2) it is no longer the ONLY one: one refused or silent
//! lookup must never leave a caller without an address, so every lookup runs
//! through [`resolve_chain`] — the protected attempt (per server, in list
//! order), then ONE retry through the same path when the failure was transient
//! (0.0.50: never after EPERM, whose retry reproduced the refusal 185/185 on
//! the phone and 184/184 on the PC), then the OS/system resolver, then the
//! supply's last-known-good address. Only when all of them fail does the
//! lookup return an error (the shell sees an empty resolution).

use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::os::fd::AsRawFd;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crypto_box::aead::rand_core::RngCore;

use crate::config::{parse_document, Json};
use crate::socket_supply::{
    Purpose, ReprotectOutcome, ReprotectResult, SocketSupply, SupplyMeta, PROTECTED_PATH_UNUSABLE,
};

const QUERY_TIMEOUT: Duration = Duration::from_millis(1500);

/// R1 (design `analysis-20261026-dns-structural` §4.2): the core's bound on the
/// ONE reprotect round trip of a chain. The shell's own box is SHORTER (200 ms,
/// `REPROTECT_BOX_MS` in the extension), so a protect that does not settle in
/// time is answered well before this expires.
const REPROTECT_WAIT: Duration = Duration::from_millis(250);

/// R1: the bound on the ONE diagnostic re-send after a successful reprotect.
/// Its timeout token is its own (`dns:resend-timeout`), so it can never be
/// mistaken for the chain's "the resolver was silent" `dns:timeout`.
const RESEND_WAIT: Duration = Duration::from_millis(250);

/// N2(b) ceiling on the OS/system-resolver fallback. `getaddrinfo` is a
/// blocking libc call on a thread of the runtime's blocking pool; the lookup
/// must never outlive the shell's own poll budget for it (the extension polls
/// `connector_protected_dns_poll` every 100 ms, `NetBirdConnector.ets`
/// `resolveProtectedIp`), so a resolver that hangs is abandoned as
/// `dns:os-resolver-timeout` and the chain falls through to the cache.
const OS_RESOLVE_TIMEOUT: Duration = Duration::from_millis(3000);

/// R2 (design `analysis-20261026-dns-structural` §4.2, AC-R2-1): the OS-stage
/// ceiling once the protected path is KNOWN-UNUSABLE (an EPERM of this chain, or
/// the supply's latched verdict from a previous one).
///
/// The capture's 794/805 OS-stage timeouts were pure waiting: the physical
/// network reported a valid default netId while nothing could egress it, so
/// `getaddrinfo` sat out its full 3 s on EVERY refused lookup. 500 ms is the
/// same order as one protected query attempt and keeps a whole chain under the
/// 1 s target, while a genuinely slow-but-working resolver is still given a
/// real chance (the answer is cached as last-known-good for the next chain).
const OS_RESOLVE_TIMEOUT_UNUSABLE: Duration = Duration::from_millis(500);


/// The DNS service port every protected query goes to. Fixed in production;
/// only the in-crate fake-resolver tests move it (a host test cannot bind a
/// privileged port).
pub const DNS_PORT: u16 = 53;

/// The OS/system-resolver seam of the N2 chain: a FUNCTION POINTER (not a
/// closure) so it can be moved into [`tokio::task::spawn_blocking`] and so the
/// in-crate tests can drive the chain with a deterministic stub. Production
/// always passes [`os_getaddrinfo`].
pub(crate) type OsResolver = fn(&str) -> io::Result<Ipv4Addr>;

/// N2(b): the OS/system resolver, on OHOS too. `crate::ice::resolve_ipv4` is
/// the libc `getaddrinfo` path (dlopen + dlsym, numeric literals short-circuit)
/// that the non-OHOS hosts always had; since 0.0.34 it is the LAST resort on
/// OHOS as well instead of a hard `stun:protected-dns-required` error.
///
/// It DOES bypass `protect()`: the query rides the process's default network
/// instead of a shell-protected socket. That is deliberate and mirrors
/// upstream, whose STUN/relay hostnames go through the OS resolver
/// (`client/internal/stdnet/stdnet.go:112` `net.DefaultResolver.LookupNetIP`,
/// bounded by `dnsResolveTimeout` at `:104-113`) and whose in-tunnel DNS server
/// falls back per query (`client/internal/dns/upstream.go:279` `ServeDNS` →
/// `:316` `tryUpstreamServers` → `:409` `queryUpstream`, health-per-upstream at
/// `:477-507`; `:645` `ExchangeWithFallback`). It carries a NAME LOOKUP only —
/// no payload, no connection, no address of ours — and it is reached only
/// after the protected path and its retry both failed.
fn os_getaddrinfo(host: &str) -> io::Result<Ipv4Addr> {
    match crate::ice::resolve_ipv4(host) {
        Ok(octets) => Ok(Ipv4Addr::from(octets)),
        Err(error) => {
            let token = match &error {
                crate::management::ManagementError::Network(token) => token.clone(),
                other => other.to_string(),
            };
            Err(io::Error::other(format!("dns:os-resolver-failed: {token}")))
        }
    }
}

/// N3 (REPORT.md §5): one syscall-level failure carries the CALL that produced
/// it and the raw OS error beside it. The `Display` of the wrapped error stays
/// in parentheses, so a capture that today greps `Operation not permitted
/// (os error 1)` still finds it — and now also learns whether it came from
/// `connect(2)`, `send(2)` or `recv(2)`, which the 0.0.33 outage could not
/// settle.
fn syscall_error(call: &str, error: io::Error) -> io::Error {
    #[cfg(test)]
    if FORCE_EPERM.with(|force| force.get()) {
        return io::Error::other(format!(
            "dns:{call}-failed errno=1 (Operation not permitted (os error 1))"));
    }
    let errno = match error.raw_os_error() {
        Some(code) => code.to_string(),
        None => "none".to_string(),
    };
    io::Error::other(format!("dns:{call}-failed errno={errno} ({error})"))
}

// Test-only seam for the EPERM skip (0.0.50): a CI host does not refuse a UDP
// syscall with EPERM on demand, so the in-crate EPERM tests relabel the REAL
// syscall failure ([`tests::unix_dgram_fd`]'s EINVAL) into the on-device
// shape. Everything else — the supply handshake, the fresh socket per
// attempt, the whole chain — stays the production path, so the tests' request
// counts are real counts. Each test runs on its own thread, so the flag
// cannot leak into another test.
#[cfg(test)]
thread_local! {
    static FORCE_EPERM: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

// R1 test seam: a host UDP send to a bound local socket cannot be refused with
// EPERM on demand, so the in-crate RECOVERY test relabels the FIRST protected
// send only. Everything after it — the reprotect handshake, the re-send, the
// answering fake server — stays the production path.
#[cfg(test)]
thread_local! {
    static FORCE_SEND_EPERM: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

/// R1: the protected-DNS send, with the test seam in front of the real syscall
/// (production compiles to the plain `send`).
async fn send_query(socket: &tokio::net::UdpSocket, query: &[u8]) -> io::Result<usize> {
    #[cfg(test)]
    if FORCE_SEND_EPERM.with(|once| once.replace(false)) {
        return Err(io::Error::from_raw_os_error(1));
    }
    socket.send(query).await
}

/// R1: the ONE re-send after a successful reprotect — the SAME socket, the SAME
/// query, bounded by [`RESEND_WAIT`]. Re-connecting is idempotent for a connected
/// UDP socket (the peer is unchanged) and covers the case where the first EPERM
/// came from `connect(2)` rather than `send(2)`.
async fn resend_after_reprotect(
    socket: &tokio::net::UdpSocket,
    addr: SocketAddrV4,
    query: &[u8],
) -> io::Result<()> {
    let attempt = async {
        socket.connect(addr).await.map_err(|e| syscall_error("connect", e))?;
        socket.send(query).await.map_err(|e| syscall_error("send", e))?;
        Ok::<(), io::Error>(())
    };
    match tokio::time::timeout(RESEND_WAIT, attempt).await {
        Ok(outcome) => outcome,
        Err(_) => Err(io::Error::other("dns:resend-timeout")),
    }
}

/// N2(a): is this failure worth ONE more protected attempt?
///
/// Yes for the refusal class the outage is made of — a syscall-level errno
/// (`dns:connect-failed` / `dns:send-failed` / `dns:recv-failed`) and a shell
/// supply refusal (`socket-supply-*` / `socket-protect-failed` /
/// `physical-network-changed`): they are intermittent (the SAME resolver IP
/// fails, then succeeds 150 ms later, REPORT.md §2.5) and a second fresh socket
/// is cheap.
///
/// No for `dns:timeout`: every server of the list already spent its own
/// `QUERY_TIMEOUT` (1.5 s) in the first pass, and a second silent pass would
/// double that latency while the OS resolver and the cache are more promising.
/// No for `dns:no-physical-server` (nothing to retry against) and no for a
/// response the parser rejected (a second identical query would be answered
/// identically).
fn is_transient(error: &io::Error) -> bool {
    let text = error.to_string();
    // R2: `socket-supply-unusable` is the supply's "the protected path is known
    // unusable" answer. It is NOT retried: a second socket would be refused for
    // the same reason, and the chain's job is to fail fast onto the stages that
    // can still answer. (Every other `socket-supply*` token keeps its retry.)
    if text.starts_with(crate::socket_supply::SUPPLY_UNUSABLE_TOKEN) { return false; }
    ["dns:connect-failed", "dns:send-failed", "dns:recv-failed",
     "socket-supply", "socket-protect-failed", "physical-network-changed"]
        .iter()
        .any(|token| text.starts_with(token))
}

/// 0.0.50 (`artifacts/analysis-20261019/dns-eperm/REPORT.md`): is this refusal
/// EPERM? The capture shows the ONE retry is EPERM again every single time —
/// 185/185 on the phone, 184/184 on the PC — so a second fresh socket buys
/// 1.5–3 s of latency and nothing else. Matched on the N3 token's
/// `errno=1 (`: the parenthesised text is locale-independent enough for a
/// capture but the errno is the stable part, and the trailing space keeps
/// `errno=10 (` (ECHILD), `errno=13 (` (EACCES) etc. out. All three syscall
/// tokens (`dns:connect-failed` / `dns:send-failed` / `dns:recv-failed`) are
/// covered because any of them can carry errno 1.
///
/// This is the ONLY transient refusal the retry is skipped for: everything
/// else that [`is_transient`] accepts is still retried once, and the chain
/// after stage 1 is untouched (the fail-closed rule is about which stages may
/// answer, not about how the refused protected leg is counted).
fn is_permission_denied(error: &io::Error) -> bool {
    error.to_string().contains("errno=1 (")
}

/// The `retry-skipped` evidence line is emitted ONCE per process: an EPERM
/// outage produced 638 such failures on the phone in one capture, and hundreds
/// of identical lines would drown the per-attempt `protected-dns:` lines that
/// the count itself comes from. One line proves the new branch ran; the missing
/// `source=protected-retry` lines prove the retry did not.
fn retry_skip_line_once() -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static REPORTED: AtomicBool = AtomicBool::new(false);
    !REPORTED.swap(true, Ordering::Relaxed)
}

pub fn parse_servers(json: &str) -> Result<Vec<Ipv4Addr>, &'static str> {
    let Json::Arr(items) = parse_document(json).map_err(|_| "invalid-json")? else {
        return Err("not-array");
    };
    let mut servers = Vec::new();
    for item in items {
        let Json::Str(ip) = item else { return Err("not-string") };
        let ip: Ipv4Addr = ip.parse().map_err(|_| "not-ipv4")?;
        if !servers.contains(&ip) { servers.push(ip); }
    }
    Ok(servers)
}

fn question(host: &str, id: u16) -> io::Result<Vec<u8>> {
    let host = host.trim_end_matches('.');
    if host.is_empty() || host.len() > 253 { return Err(io::Error::other("dns:invalid-host")); }
    let mut out = vec![0u8; 12];
    out[..2].copy_from_slice(&id.to_be_bytes());
    out[2] = 1; // recursion desired
    out[5] = 1; // one question
    for label in host.split('.') {
        if label.is_empty() || label.len() > 63 || !label.bytes().all(|b|
            b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
            return Err(io::Error::other("dns:invalid-host"));
        }
        out.push(label.len() as u8);
        out.extend(label.bytes().map(|b| b.to_ascii_lowercase()));
    }
    out.extend_from_slice(&[0, 0, 1, 0, 1]); // root, A, IN
    Ok(out)
}

// Compression pointers may point backward into the question or another answer.
fn name(packet: &[u8], offset: &mut usize) -> io::Result<Vec<u8>> {
    let mut pos = *offset;
    let mut jumped = false;
    let mut out = Vec::new();
    for _ in 0..128 {
        let len = *packet.get(pos).ok_or_else(|| io::Error::other("dns:truncated"))?;
        if len & 0xc0 == 0xc0 {
            let next = *packet.get(pos + 1).ok_or_else(|| io::Error::other("dns:truncated"))?;
            let pointer = ((len as usize & 0x3f) << 8) | next as usize;
            if pointer >= packet.len() { return Err(io::Error::other("dns:bad-pointer")); }
            if !jumped { *offset = pos + 2; jumped = true; }
            pos = pointer;
            continue;
        }
        if len & 0xc0 != 0 { return Err(io::Error::other("dns:bad-label")); }
        pos += 1;
        if len == 0 {
            if !jumped { *offset = pos; }
            return Ok(out);
        }
        let end = pos + len as usize;
        let label = packet.get(pos..end).ok_or_else(|| io::Error::other("dns:truncated"))?;
        if out.len() + label.len() + 1 > 253 { return Err(io::Error::other("dns:bad-name")); }
        if !out.is_empty() { out.push(b'.'); }
        out.extend(label.iter().map(u8::to_ascii_lowercase));
        pos = end;
    }
    Err(io::Error::other("dns:compression-loop"))
}

fn response(packet: &[u8], query: &[u8]) -> io::Result<Ipv4Addr> {
    if packet.len() < 12 || query.len() < 17 { return Err(io::Error::other("dns:truncated")); }
    if packet[..2] != query[..2] { return Err(io::Error::other("dns:id-mismatch")); }
    if packet[2] & 0x80 == 0 || packet[2] & 0x7a != 0 || packet[3] & 0x0f != 0 {
        return Err(io::Error::other("dns:invalid-response"));
    }
    let qd = u16::from_be_bytes([packet[4], packet[5]]);
    let an = u16::from_be_bytes([packet[6], packet[7]]);
    if qd != 1 { return Err(io::Error::other("dns:question-count")); }
    let mut off = 12;
    let mut qoff = 12;
    let qname = name(query, &mut qoff)?;
    if name(packet, &mut off)? != qname || packet.get(off..off + 4) != Some(&[0, 1, 0, 1]) {
        return Err(io::Error::other("dns:question-mismatch"));
    }
    off += 4;
    let mut aliases = vec![qname];
    let mut answers = Vec::new();
    for _ in 0..an {
        let owner = name(packet, &mut off)?;
        let header = packet.get(off..off + 10).ok_or_else(|| io::Error::other("dns:truncated"))?;
        let typ = u16::from_be_bytes([header[0], header[1]]);
        let class = u16::from_be_bytes([header[2], header[3]]);
        let len = u16::from_be_bytes([header[8], header[9]]) as usize;
        off += 10;
        let data = packet.get(off..off + len).ok_or_else(|| io::Error::other("dns:truncated"))?;
        if class == 1 && typ == 5 {
            let mut cname_off = off;
            let target = name(packet, &mut cname_off)?;
            if cname_off != off + len { return Err(io::Error::other("dns:bad-cname")); }
            aliases.push(target.clone());
            answers.push((owner, None, Some(target)));
        } else if class == 1 && typ == 1 && len == 4 {
            answers.push((owner, Some(Ipv4Addr::new(data[0], data[1], data[2], data[3])), None));
        }
        off += len;
    }
    // Follow only aliases rooted in the queried name, independent of RR ordering.
    let mut reachable = vec![aliases.remove(0)];
    for _ in 0..answers.len() {
        for (owner, _, target) in &answers {
            if reachable.contains(owner) {
                if let Some(target) = target { if !reachable.contains(target) { reachable.push(target.clone()); } }
            }
        }
    }
    answers.into_iter().find_map(|(owner, ip, _)| reachable.contains(&owner).then_some(ip).flatten())
        .ok_or_else(|| io::Error::other("dns:no-a-record"))
}

pub async fn resolve(host: &str, supply: &Arc<SocketSupply>) -> io::Result<Ipv4Addr> {
    resolve_on_port(host, supply, DNS_PORT).await
}

/// [`resolve`] with an explicit destination port: production callers pass
/// [`DNS_PORT`]; [`crate::ice::ProtectedUdpFdSource`] passes its (production-
/// constant, test-movable) port so the fake-resolver tests exercise the very
/// same socket path. Since 0.0.34 it enters the N2 fallback chain, so a caller
/// only ever sees an error when the protected path, its retry, the OS resolver
/// AND the last-known-good cache all failed.
pub(crate) async fn resolve_on_port(host: &str, supply: &Arc<SocketSupply>, port: u16) -> io::Result<Ipv4Addr> {
    if let Ok(ip) = host.parse::<Ipv4Addr>() { return Ok(ip); }
    let servers: Vec<_> = supply.dns_servers().into_iter()
        .map(|ip| SocketAddrV4::new(ip, port)).collect();
    resolve_chain(host, supply, &servers, os_getaddrinfo).await
}

/// 3-D (code plan `analysis-20261025-codeplan` §3.2 D): the facts of ONE
/// refused protected attempt that the structured EPERM record carries. Purely
/// evidence — it is collected during stage 1/2 and printed by the chain, never
/// read by a decision (EPERM still does not retry). `pub(crate)` only because
/// the R2 [`EpermSummary`] constructor names it.
pub(crate) struct EpermAttempt {
    /// 1 = the primary protected pass, 2 = the single transient retry.
    stage: u8,
    /// 1-based index in the server list of that stage ("尝试序号").
    attempt: usize,
    server: SocketAddrV4,
    /// R0: the `SocketSupply` request id this attempt was served under — the
    /// join key to the `socket-supply: result id=` line that handed out the
    /// refused descriptor. Evidence only.
    request_id: u64,
    /// R0: the CORE-OWNED dup descriptor the attempt sent on (the shell's
    /// original number is not visible here); `id`+`fd` is the pair the N2/DNS
    /// receipts already use. Evidence only.
    request_fd: i32,
    /// The socket supply's network generation AT THE MOMENT of the refusal
    /// ([`SocketSupply::generation`]) — this is the fact that lets a capture
    /// correlate the EPERM with network-generation changes.
    generation: u64,
    /// R0 (ets half): the shell-reported protect identity of the request this
    /// attempt was served under — the SAME `net_id` / `protect_generation` /
    /// `protect_ms` group its `socket-supply: result id=` line printed, read
    /// back by [`SocketSupply::request_meta`]. `SupplyMeta::default()` (and the
    /// byte-identical pre-R0 line) when the shell reported nothing.
    meta: SupplyMeta,
    /// The full syscall token incl. the original errno text
    /// (`dns:send-failed errno=1 (Operation not permitted (os error 1))`).
    detail: String,
}

/// 3-D + R0: ONE structured EPERM evidence line. A stable, additive token
/// (`protected-dns: eperm`) beside the existing per-server result lines: the
/// latter count the refusals, this one binds each refusal to the network
/// generation, the attempt that produced it AND (R0) the supply request id /
/// core-owned dup fd of the refused socket / the shell's protect-time identity
/// group. Pure formatting (the in-crate test and `tests/obs_contracts.rs` /
/// `tests/dns_r0_observability.rs` pin the exact field set).
#[allow(clippy::too_many_arguments)]
pub fn eperm_line(
    host: &str,
    stage: u8,
    attempt: usize,
    server: SocketAddrV4,
    request_id: u64,
    request_fd: i32,
    generation: u64,
    meta: &SupplyMeta,
    detail: &str,
) -> String {
    format!(
        "protected-dns: eperm host={host} stage={stage} attempt={attempt} server={server} \
         id={request_id} fd={request_fd} errno=1 generation={generation}{} result={detail}",
        meta.suffix()
    )
}

/// 3-D: append (and drain) the EPERM facts collected by ONE protected pass as
/// structured records. Called immediately after each pass, so an EPERM on an
/// earlier server is still recorded when a later server failed differently or a
/// later stage answered the lookup.
fn push_eperm_lines(lines: &mut Vec<String>, host: &str, eperms: &mut Vec<EpermAttempt>) {
    for fact in eperms.drain(..) {
        lines.push(eperm_line(
            host,
            fact.stage,
            fact.attempt,
            fact.server,
            fact.request_id,
            fact.request_fd,
            fact.generation,
            &fact.meta,
            &fact.detail,
        ));
    }
}

/// R2: what one refused protected PASS contributes to the chain-level verdict —
/// how many attempts were refused EPERM and the id range that identifies them.
/// Read BEFORE [`push_eperm_lines`] drains the facts.
#[derive(Clone, Copy, Default)]
pub struct EpermSummary {
    pub attempts: usize,
    pub first_id: u64,
    pub last_id: u64,
}

impl EpermSummary {
    pub(crate) fn of(facts: &[EpermAttempt]) -> Self {
        Self {
            attempts: facts.len(),
            first_id: facts.first().map(|fact| fact.request_id).unwrap_or(0),
            last_id: facts.last().map(|fact| fact.request_id).unwrap_or(0),
        }
    }

    pub(crate) fn refused_eperm(&self) -> bool { self.attempts > 0 }
}

/// R2 (design §4.2/§5.2 #5): ONE first-class state record per chain that judged
/// the protected path unusable — the `retry-skipped` evidence line upgraded to a
/// state, with the id range of the refused attempts so the refusal can be joined
/// to its `socket-supply: result` lines without stitching per-server rows.
/// APPEND-ONLY vocabulary: `result=` stays the last field, the detail is the
/// protected path's own token, and the line is emitted once per chain.
pub fn eperm_state_line(
    host: &str,
    state: &str,
    summary: &EpermSummary,
    generation: u64,
    elapsed_ms: u128,
    detail: &str,
) -> String {
    format!(
        "protected-dns: eperm-state host={host} state={state} attempts={} \
         first_id={} last_id={} generation={generation} elapsed_ms={elapsed_ms} result={detail}",
        summary.attempts, summary.first_id, summary.last_id
    )
}

/// R1 (design `analysis-20261026-dns-structural` §4.2): the per-CHAIN latch of
/// the reprotect experiment — "每条解析链最多一次，不是每 server 一次". The first
/// EPERM anywhere in the chain (stage 1 or the stage-2 retry) spends it; every
/// later refused attempt is recorded exactly as before R1.
#[derive(Default)]
struct ReprotectGate {
    used: bool,
}

/// R1: the ONE reprotect experiment of one refused attempt, collected inside the
/// I/O block so the records below carry the SAME `id`/`fd` as the attempt's
/// `protected-dns: eperm` fact.
struct ReprotectAttempt {
    /// The original EPERM token (the reason the experiment ran); it stays a
    /// recorded EPERM fact even when the re-send recovers the lookup.
    trigger: String,
    /// The shell's answer (or the core-side bound outcome).
    reprotect: ReprotectResult,
    /// The ONE re-send on the same descriptor — only when the reprotect said ok.
    resend: Option<(io::Result<()>, u128)>,
}

/// R1: ONE reprotect record — WHERE the experiment ran (host/stage/attempt/
/// server), WHICH socket (`id`/`fd`, the same pair the EPERM record carries),
/// what the shell answered and how long each leg took. APPEND-ONLY beside the
/// pre-R1 records; `result=` stays the last field, as everywhere else.
pub fn reprotect_line(
    host: &str,
    stage: u8,
    attempt: usize,
    server: SocketAddrV4,
    request_id: u64,
    request_fd: i32,
    reprotect: &ReprotectResult,
) -> String {
    let (token, errno, protect_ms) = match &reprotect.outcome {
        ReprotectOutcome::Ok => ("ok", None, reprotect.protect_ms),
        ReprotectOutcome::Failed { errno } => ("fail", Some(*errno), reprotect.protect_ms),
        ReprotectOutcome::Timeout => ("timeout", None, None),
        ReprotectOutcome::Unavailable => ("unavailable", None, None),
    };
    let errno = errno.map(|code| format!(" errno={code}")).unwrap_or_default();
    let protect_ms = protect_ms.map(|ms| format!(" protect_ms={ms}")).unwrap_or_default();
    format!(
        "protected-dns: reprotect host={host} stage={stage} attempt={attempt} server={server} \
         id={request_id} fd={request_fd} reprotect_id={}{errno}{protect_ms} elapsed_ms={} result={token}",
        reprotect.id, reprotect.elapsed_ms
    )
}

/// R1: ONE re-send record — the second `send(2)` on the SAME descriptor. Its
/// `result=` is the syscall's own token (so a repeated EPERM is readable
/// verbatim) or `ok`.
#[allow(clippy::too_many_arguments)]
pub fn resend_line(
    host: &str,
    stage: u8,
    attempt: usize,
    server: SocketAddrV4,
    request_id: u64,
    request_fd: i32,
    result: &io::Result<()>,
    elapsed_ms: u128,
) -> String {
    let token = match result {
        Ok(()) => "ok".to_string(),
        Err(error) => error.to_string(),
    };
    format!(
        "protected-dns: resend host={host} stage={stage} attempt={attempt} server={server} \
         id={request_id} fd={request_fd} elapsed_ms={elapsed_ms} result={token}"
    )
}

/// N2 (REPORT.md §5): the fallback chain around one protected lookup. Every
/// caller goes through it, so no single refused (or silent) lookup can leave a
/// caller — the management dial, the relay dial, the STUN gather — without an
/// address.
///
/// Order (fixed, and the reason the chain is written linearly):
///   1. the protected path over the shell-fed server list — the primary;
///   2. N2(a): ONE retry through the same path when the failure was transient,
///      except EPERM (0.0.50: the retry was EPERM again 185/185 phone, 184/184
///      PC — pure latency);
///   3. N2(b): the OS/system resolver (`getaddrinfo`, bounded);
///   4. N2(c): the supply's last-known-good address for this host.
///
/// A success at ANY stage is remembered as that host's last-known-good address.
/// Only when all of them fail does the error come back — the shell's
/// `resolveProtectedIp` then sees an empty answer, exactly as before.
pub(crate) async fn resolve_chain(
    host: &str,
    supply: &Arc<SocketSupply>,
    servers: &[SocketAddrV4],
    os: OsResolver,
) -> io::Result<Ipv4Addr> {
    let (outcome, lines) = resolve_chain_traced(host, supply, servers, os).await;
    for line in &lines {
        crate::hilog::emit(line);
    }
    outcome
}

/// [`resolve_chain`] with its fallback lines RETURNED instead of emitted, so
/// the in-crate tests can assert the chain's decisions (which fallback
/// answered, which token exhausted it) without capturing stderr. The per-server
/// `protected-dns:` result lines are still emitted by [`resolve_on_servers`]
/// itself, unchanged.
pub(crate) async fn resolve_chain_traced(
    host: &str,
    supply: &Arc<SocketSupply>,
    servers: &[SocketAddrV4],
    os: OsResolver,
) -> (io::Result<Ipv4Addr>, Vec<String>) {
    let started = Instant::now();
    let mut lines: Vec<String> = Vec::new();
    let elapsed = |started: &Instant| started.elapsed().as_millis();
    // 3-D: EPERM facts collected by the protected passes (evidence only).
    let mut eperms: Vec<EpermAttempt> = Vec::new();
    // R1: the ONE reprotect experiment of THIS chain and its records. The gate is
    // chain-scoped on purpose: the design allows "每条解析链最多一次", never per
    // server, so the second refused server is left byte-for-byte as before.
    let mut reprotect_gate = ReprotectGate::default();
    let mut r1_lines: Vec<String> = Vec::new();

    // Stage 1 — the protected path. Unchanged shape: one fresh protected socket
    // and one query per server, in the order the shell fed them.
    let stage_one =
        resolve_on_servers_traced(host, supply, servers, 1, &mut eperms, &mut reprotect_gate, &mut r1_lines).await;
    // R2: read the pass's EPERM facts BEFORE they are drained into their own
    // records — the chain-level verdict is "ANY attempt of this pass was refused
    // EPERM", not "the LAST server failed with it".
    let stage_one_eperms = EpermSummary::of(&eperms);
    push_eperm_lines(&mut lines, host, &mut eperms);
    lines.append(&mut r1_lines);
    let mut last = match stage_one {
        Ok(ip) => {
            // R2: a protected answer is liveness evidence — the path works, so
            // any "unusable" verdict (including one from a previous chain) is
            // spent.
            supply.clear_unusable("protected-answer");
            supply.remember_address(host, ip);
            return (Ok(ip), lines);
        }
        Err(error) => error,
    };
    // The protected path's own reason is kept separately: it is the one a
    // 0.0.33 capture could only read as "Operation not permitted", and it must
    // survive to the exhaustion line even when the OS fallback is the LAST
    // failure of the chain (N3's per-syscall token).
    let mut protected_error = last.to_string();

    // R2 (design §4.2): the pass refused with EPERM ⇒ "the protected path is
    // unusable", recorded ONCE as a state (not just the per-attempt evidence),
    // latched on the supply (which also refuses NEW DNS requests for the probe
    // window and cancels in-flight ones) and reported as the one state line.
    // A chain that saw no EPERM itself (e.g. it was already fast-rejected)
    // records nothing here — the state is still whatever the supply holds.
    if stage_one_eperms.refused_eperm() {
        note_protected_path_unusable(supply, &mut lines, host, &stage_one_eperms,
            &protected_error, elapsed(&started));
    }

    // Stage 2 (N2a) — one retry, transient failures only, never for EPERM.
    if is_transient(&last) && !is_permission_denied(&last) {
        let retry =
            resolve_on_servers_traced(host, supply, servers, 2, &mut eperms, &mut reprotect_gate, &mut r1_lines)
                .await;
        let retry_eperms = EpermSummary::of(&eperms);
        push_eperm_lines(&mut lines, host, &mut eperms);
        lines.append(&mut r1_lines);
        match retry {
            Ok(ip) => {
                supply.clear_unusable("protected-answer");
                supply.remember_address(host, ip);
                lines.push(format!(
                    "protected-dns: fallback host={host} source=protected-retry ip={ip} elapsed_ms={}",
                    elapsed(&started)));
                return (Ok(ip), lines);
            }
            Err(error) => {
                lines.push(format!(
                    "protected-dns: fallback host={host} source=protected-retry result={error} elapsed_ms={}",
                    elapsed(&started)));
                protected_error = error.to_string();
                last = error;
                // The RETRY is what refused EPERM this time (the first pass
                // failed for a non-EPERM reason): the same verdict applies.
                if retry_eperms.refused_eperm() && !stage_one_eperms.refused_eperm() {
                    note_protected_path_unusable(supply, &mut lines, host, &retry_eperms,
                        &protected_error, elapsed(&started));
                }
            }
        }
    }

    // 0.0.50 (dns-eperm): EPERM is classified transient but is NOT retried —
    // the retry reproduced errno=1 on 185/185 phone and 184/184 PC attempts,
    // so it only delays the stages that can answer. One line per process
    // (see `retry_skip_line_once`) records that the skip ran; the protected
    // leg's own result line still carries `errno=1`, which is where the count
    // comes from.
    //
    // 3-D (code plan §3.2 D, evidence only): the structured `protected-dns:
    // eperm` records were already appended right after each protected pass (see
    // `push_eperm_lines`), so an EPERM on an earlier server survives even when a
    // LATER server failed differently or another stage answered. Only the
    // once-per-process skip line stays gated exactly as before; the
    // retry/degrade decision is unchanged.
    if is_transient(&last) && is_permission_denied(&last) && retry_skip_line_once() {
        lines.push(format!(
            "protected-dns: retry-skipped host={host} reason={last} elapsed_ms={} generation={}",
            elapsed(&started),
            supply.generation()
        ));
    }

    // Stage 3 (N2b) — the OS/system resolver. R2: while the protected path is
    // known-unusable its ceiling is 500 ms instead of 3 s (the capture's
    // 794/805 OS timeouts were pure waiting on a netId that only EXISTED).
    let os_bound = if supply.protected_path_unusable() {
        OS_RESOLVE_TIMEOUT_UNUSABLE
    } else {
        OS_RESOLVE_TIMEOUT
    };
    match resolve_via_os(host, os, os_bound).await {
        Ok(ip) => {
            supply.remember_address(host, ip);
            lines.push(format!(
                "protected-dns: fallback host={host} source=os-resolver ip={ip} elapsed_ms={}",
                elapsed(&started)));
            return (Ok(ip), lines);
        }
        Err(error) => {
            lines.push(format!(
                "protected-dns: fallback host={host} source=os-resolver result={error} elapsed_ms={}",
                elapsed(&started)));
            last = error;
        }
    }

    // Stage 4 (N2c) — the last address this supply resolved for this host.
    if let Some(ip) = supply.last_known_address(host) {
        lines.push(format!(
            "protected-dns: fallback host={host} source=last-known-good ip={ip} elapsed_ms={}",
            elapsed(&started)));
        return (Ok(ip), lines);
    }

    // Nothing answered. ONE aggregate error keeps BOTH offline reasons: the
    // protected path's (the one a 0.0.33 capture could only read as "Operation
    // not permitted" — including the N3 per-syscall token) and the OS
    // resolver's. Callers that used to match the protected token
    // (`socket-protect-failed`, `dns:no-physical-server`) still find it here.
    let aggregate = format!("dns:fallbacks-exhausted protected={protected_error} os={last}");
    lines.push(format!(
        "protected-dns: fallback host={host} source=none elapsed_ms={} result={aggregate}",
        elapsed(&started)));
    (Err(io::Error::other(aggregate)), lines)
}

/// N2(b): run the OS resolver off the async worker, bounded by `bound` (the
/// pre-R2 [`OS_RESOLVE_TIMEOUT`], or the short [`OS_RESOLVE_TIMEOUT_UNUSABLE`]
/// once the protected path is known-unusable). A `getaddrinfo` that outlives the
/// bound is abandoned (its blocking thread finishes on its own); the chain then
/// uses the cache.
async fn resolve_via_os(host: &str, os: OsResolver, bound: Duration) -> io::Result<Ipv4Addr> {
    let name = host.to_string();
    let call = tokio::task::spawn_blocking(move || os(&name));
    match tokio::time::timeout(bound, call).await {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(join_error)) => Err(io::Error::other(format!("dns:os-resolver-join-failed: {join_error}"))),
        Err(_) => Err(io::Error::other("dns:os-resolver-timeout")),
    }
}

/// R2: latch "the protected path is unusable" on the supply and push the ONE
/// state record of this chain. Pure bookkeeping around
/// [`SocketSupply::mark_unusable`] so both protected passes (primary and the
/// single retry) report the verdict identically.
fn note_protected_path_unusable(
    supply: &Arc<SocketSupply>,
    lines: &mut Vec<String>,
    host: &str,
    summary: &EpermSummary,
    detail: &str,
    elapsed_ms: u128,
) {
    supply.mark_unusable(PROTECTED_PATH_UNUSABLE);
    lines.push(eperm_state_line(
        host,
        "unusable",
        summary,
        supply.generation(),
        elapsed_ms,
        detail,
    ));
}

// N2 D-row (preregistration v4 §3 evidence 3 / §4 D): the facts of ONE
// production protected query that the armed `N2_DNS` receipt reports. The
// ANSWER CONTENT is deliberately not carried here — the receipt exposes the
// transaction id, the resolver, the descriptor pair, the timing and the
// id-comparison verdict only.
#[derive(Clone, Copy, Debug)]
struct DnsFacts {
    sent_mono_ms: u64,
    response_id: i32,
    rcode: i32,
    answer_count: i32,
    has_response: bool,
}

impl DnsFacts {
    fn new() -> Self {
        Self { sent_mono_ms: 0, response_id: -1, rcode: -1, answer_count: -1, has_response: false }
    }
}

/// The response header fields as received (id / RCODE / ANCOUNT); a datagram
/// too short to carry a header keeps `-1` for what could not be decoded.
fn response_facts(packet: &[u8]) -> (i32, i32, i32) {
    let response_id = if packet.len() >= 2 {
        u16::from_be_bytes([packet[0], packet[1]]) as i32
    } else { -1 };
    let (rcode, answer_count) = if packet.len() >= 12 {
        ((packet[3] & 0x0f) as i32, u16::from_be_bytes([packet[6], packet[7]]) as i32)
    } else { (-1, -1) };
    (response_id, rcode, answer_count)
}

/// The D-row verdict of one production query — the query-id vs response-id
/// comparison the frozen §4 D row asks for:
///   * `matched`    — a response arrived, its id EQUALS the sent id, and the
///                    full A-record validation passed;
///   * `mismatched` — a datagram arrived whose transaction id differs from the
///                    sent id (never a match, whatever its body says);
///   * `timeout`    — no datagram within the query timeout;
///   * `error`      — anything else (send/recv failure, or a same-id response
///                    the parser rejected: bad question, no A record, …).
fn dns_response_class(ok: bool, facts: &DnsFacts, query_id: u16, error: &str) -> &'static str {
    if ok { return "matched"; }
    if error == "dns:timeout" { return "timeout"; }
    if facts.has_response && facts.response_id >= 0 && facts.response_id != query_id as i32 {
        return "mismatched";
    }
    "error"
}

fn emit_dns_facts(host: &str, addr: SocketAddrV4, query_id: u16, request_fd: i32,
    facts: &DnsFacts, outcome: &io::Result<Ipv4Addr>, elapsed_ms: u128) {
    if request_fd < 0 { return; }
    let error_token = outcome.as_ref().err().map(|e| e.to_string()).unwrap_or_default();
    let receipt = crate::n2_socket_trace::DnsReceipt {
        query_id,
        qname: host.to_string(),
        server: addr.into(),
        // `fd` follows the N2_SOCK convention (the shell original); when the
        // armed trace has no registration for this descriptor there is no
        // original to report, so the core-owned descriptor is reported for both
        // fields rather than an invented number.
        fd: crate::n2_socket_trace::original_fd(request_fd).unwrap_or(request_fd),
        dup_fd: request_fd,
        sent_mono_ms: facts.sent_mono_ms,
        response: dns_response_class(outcome.is_ok(), facts, query_id, &error_token),
        rcode: facts.rcode,
        answer_count: facts.answer_count,
        elapsed_ms,
    };
    // Armed-only: `None` (zero output) whenever the evidence switch is off.
    if let Some(line) = crate::n2_socket_trace::dns_receipt(&receipt) {
        crate::hilog::emit(&line);
    }
}

/// Test-facing convenience wrapper: the same lookup without collecting the 3-D
/// EPERM facts. Production always goes through [`resolve_on_servers_traced`].
#[cfg(test)]
async fn resolve_on_servers(host: &str, supply: &Arc<SocketSupply>, servers: &[SocketAddrV4]) -> io::Result<Ipv4Addr> {
    resolve_on_servers_traced(host, supply, servers, 1, &mut Vec::new(),
        &mut ReprotectGate::default(), &mut Vec::new()).await
}

/// The same lookup as `resolve_on_servers`, plus the 3-D EPERM evidence
/// collection: every refused attempt whose token is EPERM is recorded
/// (stage/attempt/server/generation) in `eperm`, in attempt order, and (R1) the
/// first such refusal of the CHAIN may spend the one reprotect experiment. The
/// lookup itself — one fresh protected socket and one query per server, the
/// error it returns, the timing — is IDENTICAL to the pre-3-D shape: no
/// decision consults the evidence, and an experiment that cannot run leaves the
/// attempt exactly as it was.
#[allow(clippy::too_many_arguments)]
async fn resolve_on_servers_traced(
    host: &str,
    supply: &Arc<SocketSupply>,
    servers: &[SocketAddrV4],
    stage: u8,
    eperm: &mut Vec<EpermAttempt>,
    gate: &mut ReprotectGate,
    r1: &mut Vec<String>,
) -> io::Result<Ipv4Addr> {
    if servers.is_empty() {
        // Report it like every other outcome of this function: the pre-F3
        // shape returned before the result line, so "the shell fed us an
        // empty server list" was the one resolution failure that left no
        // trace anywhere in the log.
        crate::hilog::emit(&format!(
            "protected-dns: id=0 host={host} fd=-1 server=- elapsed_ms=0 result=dns:no-physical-server"
        ));
        return Err(io::Error::other("dns:no-physical-server"));
    }
    let mut last = io::Error::other("dns:no-answer");
    for (index, &addr) in servers.iter().enumerate() {
        let started = Instant::now();
        let mut id = [0u8; 2];
        crypto_box::aead::OsRng.fill_bytes(&mut id);
        let query_id = u16::from_be_bytes(id);
        let query = question(host, query_id)?;
        let mut request_id = 0;
        let mut request_fd = -1;
        let mut facts = DnsFacts::new();
        let mut reprotect_attempt: Option<ReprotectAttempt> = None;
        let outcome: io::Result<Ipv4Addr> = async {
            let (id, fd) = supply.request_with_id(Purpose::Dns).await.map_err(|e| io::Error::other(e.token()))?;
            request_id = id;
            let raw = fd.as_raw_fd();
            request_fd = raw;
            crate::ice::set_nonblock(raw).map_err(|e| io::Error::other(e.token()))?;
            let socket = tokio::net::UdpSocket::from_std(UdpSocket::from(fd))?;
            tokio::time::timeout(QUERY_TIMEOUT, async {
                // N3: each of the three I/O calls names itself and carries the
                // raw OS error, so the next capture can tell `connect(2)` from
                // `send(2)` from `recv(2)` (the 0.0.33 outage could not).
                let mut send_error: Option<io::Error> = None;
                if let Err(error) = socket.connect(addr).await.map_err(|e| syscall_error("connect", e)) {
                    send_error = Some(error);
                } else {
                    crate::n2_socket_trace::io_begin(socket.as_raw_fd(), addr.into());
                    let sent = send_query(&socket, &query).await;
                    facts.sent_mono_ms = crate::sys::mono_ms();
                    crate::n2_socket_trace::first_io(socket.as_raw_fd(), addr.into(),
                        if sent.is_ok() { "sent" } else { "failed" }, "dns_query");
                    send_error = sent.map_err(|e| syscall_error("send", e)).err();
                }
                // R1 (design `analysis-20261026-dns-structural` §4.2): the FIRST
                // EPERM of this chain spends the ONE experiment — hand the shell
                // the descriptor this attempt is sending on, and if it re-protects
                // it, send the identical query once more on that SAME socket. The
                // EPERM no-retry rule is untouched: a failed/timed-out experiment
                // falls straight through to the unchanged stage 3 -> 4.
                let eperm_token = send_error.as_ref()
                    .filter(|error| is_permission_denied(error))
                    .map(|error| error.to_string());
                if let Some(trigger) = eperm_token {
                    if !gate.used {
                        gate.used = true;
                        let reprotect = supply.reprotect(raw, REPROTECT_WAIT).await;
                        let mut resend: Option<(io::Result<()>, u128)> = None;
                        if reprotect.outcome.is_ok() {
                            let resend_started = Instant::now();
                            let resent = resend_after_reprotect(&socket, addr, &query).await;
                            match &resent {
                                Ok(()) => send_error = None,
                                // Carry the re-send's own token on, so the attempt's
                                // result line and the exit path stay unchanged.
                                Err(error) => send_error = Some(io::Error::other(error.to_string())),
                            }
                            resend = Some((resent, resend_started.elapsed().as_millis()));
                        }
                        reprotect_attempt = Some(ReprotectAttempt { trigger, reprotect, resend });
                    }
                }
                if let Some(error) = send_error { return Err(error); }
                let mut buf = [0u8; 1500];
                let size = socket.recv(&mut buf).await.map_err(|e| syscall_error("recv", e))?;
                facts.has_response = true;
                let (response_id, rcode, answer_count) = response_facts(&buf[..size]);
                facts.response_id = response_id;
                facts.rcode = rcode;
                facts.answer_count = answer_count;
                response(&buf[..size], &query)
            }).await.map_err(|_| io::Error::other("dns:timeout"))?
        }.await;
        // R1: the experiment's TWO records, printed with the SAME `id`/`fd` as
        // the attempt's `protected-dns: eperm` fact (machine join). Emitted even
        // when the experiment recovered the lookup, which is exactly the
        // judgement the diagnostic exists for.
        if let Some(attempt) = &reprotect_attempt {
            r1.push(reprotect_line(host, stage, index + 1, addr, request_id, request_fd,
                &attempt.reprotect));
            if let Some((resend, elapsed_ms)) = &attempt.resend {
                r1.push(resend_line(host, stage, index + 1, addr, request_id, request_fd,
                    resend, *elapsed_ms));
            }
        }
        // N2 D-row: one armed-only `N2_DNS` receipt per production query, before
        // the (unchanged) `protected-dns:` result line below.
        emit_dns_facts(host, addr, query_id, request_fd, &facts, &outcome,
            started.elapsed().as_millis());
        match outcome {
            Ok(ip) => {
                crate::hilog::emit(&format!("protected-dns: id={request_id} host={host} fd={request_fd} server={addr} elapsed_ms={} result={ip}", started.elapsed().as_millis()));
                // R1: the refusal that STARTED the experiment is still an EPERM
                // fact (the judgement table reads it with the two R1 records),
                // even though the re-send recovered the lookup.
                if let Some(attempt) = &reprotect_attempt {
                    eperm.push(eperm_attempt(stage, index + 1, addr, request_id, request_fd, supply,
                        attempt.trigger.clone()));
                }
                return Ok(ip);
            }
            Err(err) => {
                crate::hilog::emit(&format!("protected-dns: id={request_id} host={host} fd={request_fd} server={addr} elapsed_ms={} result={err}", started.elapsed().as_millis()));
                // 3-D: capture the refusal's evidence facts BEFORE the error is
                // moved on. Only EPERM earns a record (the class this change is
                // about); the generation is read at the refusal instant, and R0
                // adds the request id / core-owned dup fd of THIS attempt so the
                // record joins the supply line by id. R1: when the experiment ran,
                // its TRIGGER is the EPERM of record (the final error may be the
                // re-send's instead), and an attempt recorded from the trigger is
                // never recorded twice.
                if let Some(attempt) = &reprotect_attempt {
                    eperm.push(eperm_attempt(stage, index + 1, addr, request_id, request_fd, supply,
                        attempt.trigger.clone()));
                } else if is_permission_denied(&err) {
                    eperm.push(eperm_attempt(stage, index + 1, addr, request_id, request_fd, supply,
                        err.to_string()));
                }
                last = err;
            }
        }
    }
    Err(last)
}

/// 3-D + R0 + R1: build the EPERM evidence fact of ONE refused attempt (the one
/// place the `net_id`/`protect_generation` group is read back from the supply).
#[allow(clippy::too_many_arguments)]
fn eperm_attempt(
    stage: u8,
    attempt: usize,
    server: SocketAddrV4,
    request_id: u64,
    request_fd: i32,
    supply: &Arc<SocketSupply>,
    detail: String,
) -> EpermAttempt {
    EpermAttempt {
        stage,
        attempt,
        server,
        request_id,
        request_fd,
        generation: supply.generation(),
        meta: supply.request_meta(request_id).unwrap_or_default(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::net::UdpSocket as AsyncUdpSocket;

    fn answer(query: &[u8], ip: [u8; 4]) -> Vec<u8> {
        let mut data = query.to_vec();
        data[2] = 0x81;
        data[3] = 0x80;
        data[7] = 1;
        data.extend_from_slice(&[0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 1, 0, 4]);
        data.extend_from_slice(&ip);
        data
    }

    #[test]
    fn validates_a_answer_and_rejects_bad_id_and_truncation() {
        let q = question("relay.example", 0x1234).unwrap();
        let valid = answer(&q, [192, 0, 2, 7]);
        assert_eq!(response(&valid, &q).unwrap(), Ipv4Addr::new(192, 0, 2, 7));
        let mut bad = valid.clone();
        bad[1] ^= 1;
        assert!(response(&bad, &q).is_err());
        bad = valid.clone();
        bad[2] |= 2;
        assert!(response(&bad, &q).is_err());
        assert!(response(&valid[..valid.len() - 1], &q).is_err());
    }

    async fn supply_requests(supply: Arc<SocketSupply>, count: usize, protected: bool) {
        for _ in 0..count {
            let (id, purpose) = loop {
                if let Some(req) = supply.poll_request() { break req; }
                tokio::task::yield_now().await;
            };
            assert_eq!(purpose, Purpose::Dns);
            if protected {
                let raw = unsafe { crate::sys::socket(crate::sys::AF_INET, crate::sys::SOCK_DGRAM, 0) };
                assert!(raw >= 0);
                let fd = unsafe { OwnedFd::from_raw_fd(raw) };
                assert!(supply.reply(id, fd.as_raw_fd(), "protected").contains("true"));
            } else {
                supply.reply(id, -1, "protect-failed");
            }
        }
    }

    #[tokio::test]
    async fn protected_fd_queries_fake_server() {
        let _guard = crate::n2_stimulus::test_guard();
        let server = AsyncUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = match server.local_addr().unwrap() { std::net::SocketAddr::V4(a) => a, _ => unreachable!() };
        let supply = Arc::new(SocketSupply::new());
        let provider = tokio::spawn(supply_requests(supply.clone(), 1, true));
        let responder = tokio::spawn(async move {
            let mut buf = [0u8; 512];
            let (n, peer) = server.recv_from(&mut buf).await.unwrap();
            server.send_to(&answer(&buf[..n], [203, 0, 113, 9]), peer).await.unwrap();
        });
        let result = resolve_on_servers("relay.example", &supply, &[address]).await.unwrap();
        assert_eq!(result, Ipv4Addr::new(203, 0, 113, 9));
        provider.await.unwrap();
        responder.await.unwrap();
    }

    #[tokio::test]
    async fn retries_after_timeout_with_fresh_fd() {
        let _guard = crate::n2_stimulus::test_guard();
        let silent = AsyncUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let responder = AsyncUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let first = match silent.local_addr().unwrap() { std::net::SocketAddr::V4(a) => a, _ => unreachable!() };
        let second = match responder.local_addr().unwrap() { std::net::SocketAddr::V4(a) => a, _ => unreachable!() };
        let supply = Arc::new(SocketSupply::new());
        let provider = tokio::spawn(supply_requests(supply.clone(), 2, true));
        let server = tokio::spawn(async move {
            let mut buf = [0u8; 512];
            let (n, peer) = responder.recv_from(&mut buf).await.unwrap();
            responder.send_to(&answer(&buf[..n], [198, 51, 100, 4]), peer).await.unwrap();
        });
        let result = resolve_on_servers("stun.example", &supply, &[first, second]).await.unwrap();
        assert_eq!(result, Ipv4Addr::new(198, 51, 100, 4));
        provider.await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn rejected_supply_and_missing_servers_send_nothing() {
        let _guard = crate::n2_stimulus::test_guard();
        let server = AsyncUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = match server.local_addr().unwrap() { std::net::SocketAddr::V4(a) => a, _ => unreachable!() };
        let supply = Arc::new(SocketSupply::new());
        assert!(resolve("stun.example", &supply).await.is_err());
        assert!(supply.poll_request().is_none());
        let provider = tokio::spawn(supply_requests(supply.clone(), 1, false));
        assert!(resolve_on_servers("stun.example", &supply, &[addr]).await.is_err());
        provider.await.unwrap();
        let mut buf = [0u8; 512];
        assert!(tokio::time::timeout(Duration::from_millis(50), server.recv_from(&mut buf)).await.is_err());
    }

    // ---------------- N2_DNS (preregistration v4 §4 D row) -----------------

    fn facts_for(packet: Option<&[u8]>) -> DnsFacts {
        let mut facts = DnsFacts::new();
        if let Some(packet) = packet {
            facts.has_response = true;
            let (response_id, rcode, answer_count) = response_facts(packet);
            facts.response_id = response_id;
            facts.rcode = rcode;
            facts.answer_count = answer_count;
        }
        facts
    }

    #[test]
    fn dns_verdict_matched_mismatched_timeout_and_error() {
        let query_id = 0x1234;
        let q = question("relay.example", query_id).unwrap();
        let good = answer(&q, [192, 0, 2, 7]);

        // A full A-record answer with the sent id is the ONLY `matched`.
        let matched = facts_for(Some(&good));
        assert_eq!(dns_response_class(true, &matched, query_id, ""), "matched");
        // A same-id body the parser rejected is never a match.
        assert_eq!(dns_response_class(false, &matched, query_id, "dns:no-a-record"), "error");

        // A datagram whose transaction id differs from the sent one.
        let mut wrong = good.clone();
        wrong[1] ^= 1;
        let mismatched = facts_for(Some(&wrong));
        assert_eq!(mismatched.response_id, (query_id as i32) ^ 1);
        assert_eq!(dns_response_class(false, &mismatched, query_id, "dns:id-mismatch"), "mismatched");
        // Even if the body happened to parse, the id verdict still rules.
        assert_eq!(dns_response_class(false, &mismatched, query_id, "dns:question-mismatch"), "mismatched");

        // A silent resolver.
        let timeout = facts_for(None);
        assert_eq!(dns_response_class(false, &timeout, query_id, "dns:timeout"), "timeout");
        // A send/recv failure that is not a timeout.
        assert_eq!(dns_response_class(false, &timeout, query_id, "dns:no-physical-server"), "error");
    }

    #[test]
    fn dns_response_facts_decode_header_and_stay_honest_when_short() {
        let q = question("relay.example", 0x1234).unwrap();
        let good = answer(&q, [192, 0, 2, 7]);
        assert_eq!(response_facts(&good), (0x1234, 0, 1));
        // A datagram too short for a header: the id is only reported when the
        // two bytes are there, and rcode/ANCOUNT stay -1.
        assert_eq!(response_facts(&[0x12]), (-1, -1, -1));
        assert_eq!(response_facts(&[0x12, 0x34, 0x81]), (0x1234, -1, -1));
        // RCODE is the low nibble of byte 3 (NXDOMAIN = 3, no answers).
        let mut nx = good.clone();
        nx[3] = 0x83;
        nx[7] = 0;
        assert_eq!(response_facts(&nx), (0x1234, 3, 0));
    }

    // ------------- N2/N3 fallback chain (0.0.34, REPORT.md §5) --------------

    /// One socket address no test ever binds: only the fd handed out by the
    /// supply decides what happens (a refusal never reaches the syscall).
    fn unreached() -> SocketAddrV4 {
        SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 53)
    }

    /// Serve `count` DNS requests with `fd_of`'s socket (the supply dups it).
    async fn supply_requests_with(supply: Arc<SocketSupply>, count: usize, fd_of: fn() -> OwnedFd) {
        for _ in 0..count {
            let (id, purpose) = loop {
                if let Some(req) = supply.poll_request() { break req; }
                tokio::task::yield_now().await;
            };
            assert_eq!(purpose, Purpose::Dns);
            let fd = fd_of();
            assert!(supply.reply(id, fd.as_raw_fd(), "protected").contains("true"));
        }
    }

    /// An AF_UNIX datagram socket: `connect(2)` with an AF_INET address fails
    /// AT THE SYSCALL (family mismatch), deterministically and without any
    /// network — the shape that proves the per-syscall token end to end.
    fn unix_dgram_fd() -> OwnedFd {
        let raw = unsafe { crate::sys::socket(1 /* AF_UNIX */, crate::sys::SOCK_DGRAM, 0) };
        assert!(raw >= 0, "socket(AF_UNIX, SOCK_DGRAM)");
        unsafe { OwnedFd::from_raw_fd(raw) }
    }

    fn os_resolves_the_test_host(host: &str) -> io::Result<Ipv4Addr> {
        assert_eq!(host, "os-fallback.test", "the chain must ask for the same name");
        Ok(Ipv4Addr::new(198, 51, 100, 7))
    }

    fn os_failing(_host: &str) -> io::Result<Ipv4Addr> {
        Err(io::Error::other("dns:os-resolver-failed: test"))
    }

    /// N3: the token names the syscall AND keeps the raw OS error, so a capture
    /// that greps the old `Operation not permitted (os error 1)` still matches.
    #[test]
    fn syscall_tokens_split_the_call_and_keep_the_raw_errno() {
        let connect = syscall_error("connect", io::Error::from_raw_os_error(1)).to_string();
        assert_eq!(connect, "dns:connect-failed errno=1 (Operation not permitted (os error 1))");
        let refusal = syscall_error("send", io::Error::from_raw_os_error(1)).to_string();
        assert!(refusal.starts_with("dns:send-failed errno=1 ("));
        assert!(refusal.contains("Operation not permitted (os error 1)"));
        let recv = syscall_error("recv", io::Error::from_raw_os_error(111)).to_string();
        assert!(recv.starts_with("dns:recv-failed errno=111 ("));
        // No errno (a pure Rust error) is reported as `none`, never invented.
        let plain = syscall_error("recv", io::Error::other("dns:truncated")).to_string();
        assert_eq!(plain, "dns:recv-failed errno=none (dns:truncated)");
    }

    /// N2(a): what earns the one retry — and what does not.
    #[test]
    fn only_the_intermittent_refusal_class_is_retried() {
        for token in ["dns:connect-failed errno=1 (Operation not permitted (os error 1))",
            "dns:send-failed errno=1 (Operation not permitted (os error 1))",
            "dns:recv-failed errno=1 (Operation not permitted (os error 1))",
            "socket-supply-rejected", "socket-protect-failed", "socket-supply-timeout",
            "physical-network-changed"] {
            assert!(is_transient(&io::Error::other(token)), "{token} must be retried");
        }
        // A silent resolver already spent its 1.5 s per server; a second pass
        // would double the latency for nothing.
        assert!(!is_transient(&io::Error::other("dns:timeout")));
        // Nothing to retry against / an answer the parser rejected.
        assert!(!is_transient(&io::Error::other("dns:no-physical-server")));
        assert!(!is_transient(&io::Error::other("dns:no-a-record")));
    }

    // ---------------- 0.0.50: EPERM is not retried (dns-eperm) --------------

    /// 0.0.50: EPERM (errno=1) is the one transient refusal the retry must
    /// skip, from ANY of the three syscall tokens — and `errno=10 (` (ECHILD)
    /// must not be mistaken for it.
    #[test]
    fn only_errno_one_counts_as_permission_denied() {
        let _guard = crate::n2_stimulus::test_guard();
        for call in ["connect", "send", "recv"] {
            let error = syscall_error(call, io::Error::from_raw_os_error(1));
            assert!(error.to_string().contains("errno=1 ("), "{error}");
            assert!(is_permission_denied(&error), "{call} EPERM must skip the retry");
        }
        // `errno=1` is a PREFIX of `errno=10`; only the whole `errno=1 (` token
        // may match, so an ECHILD failure keeps its retry.
        let echild = io::Error::other("dns:recv-failed errno=10 (No child processes (os error 10))");
        assert!(!is_permission_denied(&echild));
        assert!(!is_permission_denied(&syscall_error("send", io::Error::from_raw_os_error(13))));
        assert!(!is_permission_denied(&io::Error::other("socket-protect-failed")));
        assert!(!is_permission_denied(&io::Error::other("dns:timeout")));
    }

    /// Make the syscall failures of THIS test thread report EPERM (see the
    /// `FORCE_EPERM` test seam); production never sets it.
    fn force_eperm(on: bool) {
        FORCE_EPERM.with(|force| force.set(on));
    }

    /// Serve protected requests until `stop` is set, returning how many were
    /// served. Unlike the fixed-count providers, this can ASSERT the attempt
    /// count without hanging when the code under test — correctly — asks for
    /// fewer sockets than the test's upper bound.
    async fn serve_until_stopped(
        supply: Arc<SocketSupply>,
        stop: Arc<AtomicBool>,
        fd_of: fn() -> OwnedFd,
    ) -> usize {
        let mut served = 0;
        loop {
            if let Some((id, purpose)) = supply.poll_request() {
                assert_eq!(purpose, Purpose::Dns);
                let fd = fd_of();
                assert!(supply.reply(id, fd.as_raw_fd(), "protected").contains("true"));
                served += 1;
                continue;
            }
            if stop.load(Ordering::SeqCst) { return served; }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    /// 0.0.50: EPERM on the FIRST attempt is not retried — the supply is
    /// requested exactly ONCE — and the chain still falls through to the OS
    /// resolver, which answers.
    #[tokio::test]
    async fn eperm_first_attempt_is_not_retried_and_os_resolver_still_answers() {
        let _guard = crate::n2_stimulus::test_guard();
        force_eperm(true);
        let supply = Arc::new(SocketSupply::new());
        let stop = Arc::new(AtomicBool::new(false));
        let provider = tokio::spawn(serve_until_stopped(supply.clone(), stop.clone(), unix_dgram_fd));
        let (outcome, lines) =
            resolve_chain_traced("os-fallback.test", &supply, &[unreached()], os_resolves_the_test_host).await;
        stop.store(true, Ordering::SeqCst);
        assert_eq!(provider.await.unwrap(), 1, "EPERM must not be retried (one supply request)");
        assert_eq!(outcome.unwrap(), Ipv4Addr::new(198, 51, 100, 7));
        assert!(lines.iter().all(|l| !l.contains("source=protected-retry")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("source=os-resolver ip=198.51.100.7")), "{lines:?}");
        // 3-D: the EPERM path also carries the structured evidence record.
        assert!(
            lines.iter().any(|l| l.contains("protected-dns: eperm host=os-fallback.test stage=1 attempt=1 server=")
                && l.contains("errno=1 generation=0")),
            "{lines:?}"
        );
        force_eperm(false);
    }

    /// 3-D (code plan §3.2 D): every refused protected attempt leaves ONE
    /// structured EPERM record carrying the socket-supply generation and the
    /// server/attempt sequence, so a capture can answer "was this refusal
    /// bound to a network-generation change?". Evidence only: the attempt
    /// count stays one per server (no retry).
    #[tokio::test]
    async fn eperm_evidence_reports_every_refused_attempt_with_the_network_generation() {
        let _guard = crate::n2_stimulus::test_guard();
        force_eperm(true);
        let supply = Arc::new(SocketSupply::new());
        // One reported physical-network change: the evidence generation is 1.
        supply.cancel_pending_for_network_change();
        let stop = Arc::new(AtomicBool::new(false));
        let provider = tokio::spawn(serve_until_stopped(supply.clone(), stop.clone(), unix_dgram_fd));
        let servers = [
            unreached(),
            SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 99), DNS_PORT),
        ];
        let (outcome, lines) =
            resolve_chain_traced("eperm-gen.test", &supply, &servers, os_failing).await;
        stop.store(true, Ordering::SeqCst);
        assert_eq!(provider.await.unwrap(), 2, "each server is tried once, EPERM is never retried");
        assert!(outcome.is_err(), "no stage can answer");
        let records: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("protected-dns: eperm "))
            .collect();
        assert_eq!(records.len(), 2, "one record per refused server attempt: {lines:?}");
        assert!(
            records[0].contains("host=eperm-gen.test stage=1 attempt=1 server=")
                && records[0].contains("errno=1 generation=1"),
            "{}",
            records[0]
        );
        assert!(
            records[1].contains("stage=1 attempt=2 server=192.0.2.99:53")
                && records[1].contains("generation=1"),
            "{}",
            records[1]
        );
        // R0: each record binds the supply request id and the CORE-OWNED dup fd
        // of the attempt that was refused (both real: the supply served the
        // socket before the forced refusal, so the id is non-zero and the fd is
        // a live descriptor number).
        for record in &records {
            let id = record
                .split_whitespace()
                .find_map(|token| token.strip_prefix("id="))
                .unwrap_or_else(|| panic!("record carries id=: {record}"));
            let fd = record
                .split_whitespace()
                .find_map(|token| token.strip_prefix("fd="))
                .unwrap_or_else(|| panic!("record carries fd=: {record}"));
            assert!(id.parse::<u64>().map(|id| id > 0).unwrap_or(false), "{record}");
            assert!(fd.parse::<i32>().map(|fd| fd >= 0).unwrap_or(false), "{record}");
        }
        // The once-per-process skip line carries the same generation when it is
        // this process's first EPERM (other tests in this binary may have run
        // it already, hence the conditional).
        if let Some(skip) = lines.iter().find(|l| l.contains("retry-skipped")) {
            assert!(skip.contains("generation=1"), "{skip}");
        }
        force_eperm(false);
    }

    /// R0 (ets half, AC-R0-1): the shell-reported protect identity rides the
    /// REPLY, is kept BY REQUEST ID, and is printed on the EPERM record of the
    /// very socket that reply handed out — so the supply line and the refusal
    /// line name one `net_id` instead of being joined by (host, server, time).
    /// A pre-R0 reply (no meta) keeps the byte-identical pre-R0 record, which
    /// the sibling tests above already pin.
    #[tokio::test]
    async fn eperm_record_carries_the_supply_replys_shell_meta_group() {
        let _guard = crate::n2_stimulus::test_guard();
        force_eperm(true);
        let supply = Arc::new(SocketSupply::new());
        let stop = Arc::new(AtomicBool::new(false));
        let meta = SupplyMeta { net_id: Some(297), protect_generation: Some(7), protect_ms: Some(34) };
        let provider = tokio::spawn({
            let supply = supply.clone();
            let stop = stop.clone();
            async move {
                let mut served = 0usize;
                loop {
                    if let Some((id, purpose)) = supply.poll_request() {
                        assert_eq!(purpose, Purpose::Dns);
                        let fd = unix_dgram_fd();
                        assert!(supply
                            .reply_with_meta(id, fd.as_raw_fd(), "protected", &meta)
                            .contains("true"));
                        served += 1;
                        continue;
                    }
                    if stop.load(Ordering::SeqCst) { return served; }
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }
        });
        let (outcome, lines) =
            resolve_chain_traced("eperm-meta.test", &supply, &[unreached()], os_failing).await;
        stop.store(true, Ordering::SeqCst);
        assert_eq!(provider.await.unwrap(), 1, "EPERM is never retried");
        assert!(outcome.is_err(), "no stage can answer");
        let record = lines
            .iter()
            .find(|l| l.contains("protected-dns: eperm "))
            .unwrap_or_else(|| panic!("an EPERM record: {lines:?}"));
        assert!(
            record.contains(
                "errno=1 generation=0 net_id=297 protect_generation=7 protect_ms=34 result="),
            "{record}"
        );
        force_eperm(false);
    }

    /// 0.0.50: with EPERM the chain still walks stage 4 (last-known-good) and,
    /// when that is empty too, reports the protected EPERM in the aggregate.
    /// Both legs cost exactly ONE supply request.
    #[tokio::test]
    async fn eperm_still_reaches_last_known_good_and_exhaustion() {
        let _guard = crate::n2_stimulus::test_guard();
        force_eperm(true);

        let cached = Arc::new(SocketSupply::new());
        cached.remember_address("cached.test", Ipv4Addr::new(203, 0, 113, 9));
        let stop = Arc::new(AtomicBool::new(false));
        let provider = tokio::spawn(serve_until_stopped(cached.clone(), stop.clone(), unix_dgram_fd));
        let (outcome, lines) =
            resolve_chain_traced("cached.test", &cached, &[unreached()], os_failing).await;
        stop.store(true, Ordering::SeqCst);
        assert_eq!(provider.await.unwrap(), 1, "EPERM must not be retried (one supply request)");
        assert_eq!(outcome.unwrap(), Ipv4Addr::new(203, 0, 113, 9));
        assert!(lines.iter().any(|l| l.contains("source=last-known-good ip=203.0.113.9")), "{lines:?}");

        let dead = Arc::new(SocketSupply::new());
        let stop = Arc::new(AtomicBool::new(false));
        let provider = tokio::spawn(serve_until_stopped(dead.clone(), stop.clone(), unix_dgram_fd));
        let (outcome, lines) = resolve_chain_traced("dead.test", &dead, &[unreached()], os_failing).await;
        stop.store(true, Ordering::SeqCst);
        assert_eq!(provider.await.unwrap(), 1, "EPERM must not be retried (one supply request)");
        let error = outcome.unwrap_err().to_string();
        assert!(error.starts_with("dns:fallbacks-exhausted"), "{error}");
        assert!(error.contains("protected=dns:connect-failed errno=1 ("), "{error}");
        assert!(error.contains("os=dns:os-resolver-failed"), "{error}");
        assert!(lines.iter().all(|l| !l.contains("source=protected-retry")), "{lines:?}");
        force_eperm(false);
    }

    /// 0.0.50 control: a NON-EPERM syscall refusal (EINVAL here) still earns
    /// the one retry — exactly TWO supply requests.
    #[tokio::test]
    async fn a_non_eperm_syscall_refusal_is_still_retried_once() {
        let _guard = crate::n2_stimulus::test_guard();
        let supply = Arc::new(SocketSupply::new());
        let stop = Arc::new(AtomicBool::new(false));
        let provider = tokio::spawn(serve_until_stopped(supply.clone(), stop.clone(), unix_dgram_fd));
        let (outcome, lines) =
            resolve_chain_traced("os-fallback.test", &supply, &[unreached()], os_resolves_the_test_host).await;
        stop.store(true, Ordering::SeqCst);
        assert_eq!(provider.await.unwrap(), 2, "a non-EPERM syscall refusal still retries once");
        assert_eq!(outcome.unwrap(), Ipv4Addr::new(198, 51, 100, 7));
        assert!(lines.iter().any(|l| l.contains("source=protected-retry")), "{lines:?}");
    }

    /// N2: the protected path refuses (both the attempt and its retry) → the OS
    /// resolver answers.
    #[tokio::test]
    async fn a_protected_refusal_falls_back_to_the_os_resolver() {
        let _guard = crate::n2_stimulus::test_guard();
        let supply = Arc::new(SocketSupply::new());
        let provider = tokio::spawn(supply_requests(supply.clone(), 2, false));
        let (outcome, lines) =
            resolve_chain_traced("os-fallback.test", &supply, &[unreached()], os_resolves_the_test_host).await;
        assert_eq!(outcome.unwrap(), Ipv4Addr::new(198, 51, 100, 7));
        assert!(lines.iter().any(|l| l.contains("source=protected-retry")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("source=os-resolver ip=198.51.100.7")), "{lines:?}");
        // An OS-resolved answer is itself worth remembering.
        assert_eq!(supply.last_known_address("os-fallback.test"), Some(Ipv4Addr::new(198, 51, 100, 7)));
        provider.await.unwrap();
    }

    /// N2(c): protected AND the OS resolver fail → the last-known-good address
    /// answers, and the lookup is NOT empty.
    #[tokio::test]
    async fn the_last_known_address_is_the_final_fallback() {
        let _guard = crate::n2_stimulus::test_guard();
        let supply = Arc::new(SocketSupply::new());
        supply.remember_address("cached.test", Ipv4Addr::new(203, 0, 113, 9));
        let provider = tokio::spawn(supply_requests(supply.clone(), 2, false));
        let (outcome, lines) =
            resolve_chain_traced("cached.test", &supply, &[unreached()], os_failing).await;
        assert_eq!(outcome.unwrap(), Ipv4Addr::new(203, 0, 113, 9));
        assert!(lines.iter().any(|l| l.contains("source=last-known-good ip=203.0.113.9")), "{lines:?}");
        provider.await.unwrap();
    }

    /// N2/N3: EVERY fallback fails → the lookup is empty and the exhaustion
    /// line carries the per-syscall token of the last protected failure.
    #[tokio::test]
    async fn all_fallbacks_failing_return_empty_with_the_syscall_token() {
        let _guard = crate::n2_stimulus::test_guard();
        let supply = Arc::new(SocketSupply::new());
        let provider = tokio::spawn(supply_requests_with(supply.clone(), 2, unix_dgram_fd));
        let (outcome, lines) =
            resolve_chain_traced("dead.test", &supply, &[unreached()], os_failing).await;
        let error = outcome.unwrap_err().to_string();
        // The single aggregate still names every stage that failed ...
        assert!(error.starts_with("dns:fallbacks-exhausted"), "{error}");
        // ... including the N3 per-syscall token of the protected path, even
        // though the OS resolver was the LAST failure of the chain.
        assert!(error.contains("protected=dns:connect-failed errno="), "{error}");
        assert!(error.contains("os=dns:os-resolver-failed"), "{error}");
        let last = lines.last().expect("the exhaustion line");
        assert!(last.contains("source=none"), "{last}");
        assert!(last.contains("protected=dns:connect-failed errno="), "{last}");
        assert_eq!(supply.last_known_address("dead.test"), None, "a failure remembers nothing");
        provider.await.unwrap();
    }

    /// N2: a protected success is remembered, and the chain reports it as the
    /// protected answer (no fallback line).
    #[tokio::test]
    async fn a_protected_success_is_remembered_as_the_last_known_address() {
        let _guard = crate::n2_stimulus::test_guard();
        let server = AsyncUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = match server.local_addr().unwrap() { std::net::SocketAddr::V4(a) => a, _ => unreachable!() };
        let supply = Arc::new(SocketSupply::new());
        let provider = tokio::spawn(supply_requests(supply.clone(), 1, true));
        let responder = tokio::spawn(async move {
            let mut buf = [0u8; 512];
            let (n, peer) = server.recv_from(&mut buf).await.unwrap();
            server.send_to(&answer(&buf[..n], [203, 0, 113, 11]), peer).await.unwrap();
        });
        let (outcome, lines) =
            resolve_chain_traced("remembered.test", &supply, &[address], os_failing).await;
        assert_eq!(outcome.unwrap(), Ipv4Addr::new(203, 0, 113, 11));
        assert!(lines.is_empty(), "the primary path needs no fallback line: {lines:?}");
        assert_eq!(supply.last_known_address("remembered.test"), Some(Ipv4Addr::new(203, 0, 113, 11)));
        assert_eq!(supply.last_known_addresses(), 1);
        provider.await.unwrap();
        responder.await.unwrap();
    }

    /// N2(b): the REAL OS-resolver path (the same one OHOS now uses) resolves a
    /// hosts-file name and fails, bounded, for a name that does not exist. R2
    /// added the bound as a parameter; both bounds are exercised (the short one
    /// must not break a resolver that answers, the long one keeps its ceiling).
    #[tokio::test]
    async fn the_real_os_resolver_path_resolves_localhost_and_is_bounded() {
        for bound in [OS_RESOLVE_TIMEOUT, OS_RESOLVE_TIMEOUT_UNUSABLE] {
            assert_eq!(
                resolve_via_os("localhost", os_getaddrinfo, bound).await.unwrap(),
                Ipv4Addr::LOCALHOST
            );
            assert!(resolve_via_os("no-such-host.invalid", os_getaddrinfo, bound).await.is_err());
        }
    }

    // ------------- R1: reprotect-once (design §4.2) -------------

    /// R1: a subscribed shell that serves EVERY reprotect request of `supply` the
    /// instant the core wakes it — the real wake contract, no polling loop, so the
    /// diagnostic costs no wall time. Returns the number of requests it answered
    /// (the "once per chain, not per server" count).
    fn listen_and_serve_reprotect(supply: &Arc<SocketSupply>, result: &'static str) -> Arc<AtomicUsize> {
        let served = Arc::new(AtomicUsize::new(0));
        let supply = supply.clone();
        let counted = served.clone();
        crate::socket_supply::set_notify(Some(Arc::new(move |_id: u64| {
            while let Some((id, _fd)) = supply.poll_reprotect() {
                assert!(supply.reprotect_reply(id, result, 0, 3).contains("true"),
                    "the shell's reprotect answer must be accepted");
                counted.fetch_add(1, Ordering::SeqCst);
            }
        })));
        served
    }

    /// R1: a subscribed shell that never answers — the "reprotect 请求本身超时" row.
    fn listen_but_never_answer() {
        crate::socket_supply::set_notify(Some(Arc::new(|_id: u64| {})));
    }

    /// The wake hook is process-wide: every R1 test must clear it before it lets
    /// go of the serialization lock, or the next test inherits a listener.
    struct ClearListener;

    impl Drop for ClearListener {
        fn drop(&mut self) {
            crate::socket_supply::set_notify(None);
        }
    }

    /// R1: make the NEXT protected-DNS send fail as EPERM (the
    /// `FORCE_SEND_EPERM` test seam) — the only way a host test can produce a
    /// `send(2)` refusal while the socket, the resolver address and the fake
    /// server all stay REAL.
    fn force_send_eperm_once() {
        FORCE_SEND_EPERM.with(|once| once.set(true));
    }

    /// The `key=value` token of a `protected-dns:` record (`""` when absent).
    fn token<'a>(line: &'a str, key: &str) -> &'a str {
        line.split_whitespace().find_map(|part| part.strip_prefix(key)).unwrap_or("")
    }

    fn r1_lines<'a>(lines: &'a [String], kind: &str) -> Vec<&'a String> {
        let needle = format!("protected-dns: {kind} ");
        lines.iter().filter(|line| line.starts_with(&needle)).collect()
    }

    /// R1 (design §4.2): the ONE experiment of a chain that RECOVERS — the send
    /// was refused EPERM, the shell re-protected the SAME core descriptor, and
    /// the ONE re-send got through. The lookup answers on the protected path
    /// (no fallback line), both R1 records and the trigger EPERM are emitted, and
    /// all three lines name the same `id`/`fd`.
    #[tokio::test]
    async fn reprotect_once_recovers_a_send_eperm_and_the_chain_answers() {
        let _serial = crate::socket_supply::notify_test_lock();
        let _clear = ClearListener;
        let _guard = crate::n2_stimulus::test_guard();
        let server = AsyncUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = match server.local_addr().unwrap() { std::net::SocketAddr::V4(a) => a, _ => unreachable!() };
        let supply = Arc::new(SocketSupply::new());
        let served = listen_and_serve_reprotect(&supply, "ok");
        force_send_eperm_once();
        let provider = tokio::spawn(supply_requests(supply.clone(), 1, true));
        let responder = tokio::spawn(async move {
            let mut buf = [0u8; 512];
            let (n, peer) = server.recv_from(&mut buf).await.unwrap();
            server.send_to(&answer(&buf[..n], [203, 0, 113, 21]), peer).await.unwrap();
        });
        let (outcome, lines) =
            resolve_chain_traced("recover.test", &supply, &[address], os_failing).await;
        provider.await.unwrap();
        responder.await.unwrap();
        assert_eq!(outcome.unwrap(), Ipv4Addr::new(203, 0, 113, 21),
            "the re-send after the reprotect answers on the protected path");
        assert_eq!(served.load(Ordering::SeqCst), 1, "ONE reprotect for the chain: {lines:?}");
        assert!(lines.iter().all(|line| !line.contains("source=")),
            "stage 1 answered, so no fallback line: {lines:?}");

        let reprotect = r1_lines(&lines, "reprotect");
        assert_eq!(reprotect.len(), 1, "{lines:?}");
        assert!(reprotect[0].starts_with(
            "protected-dns: reprotect host=recover.test stage=1 attempt=1 server="), "{}", reprotect[0]);
        assert!(reprotect[0].contains(" protect_ms=3 "), "{}", reprotect[0]);
        assert!(reprotect[0].ends_with("result=ok"), "{}", reprotect[0]);
        let resend = r1_lines(&lines, "resend");
        assert_eq!(resend.len(), 1, "{lines:?}");
        assert!(resend[0].ends_with("result=ok"), "{}", resend[0]);
        // The refusal that STARTED the experiment is still a recorded EPERM fact
        // (the judgement table reads it together with the two R1 records), even
        // though the lookup itself ended in success.
        let eperm = r1_lines(&lines, "eperm");
        assert_eq!(eperm.len(), 1, "{lines:?}");
        assert!(eperm[0].contains(" errno=1 "), "{}", eperm[0]);
        // Machine join: one id/fd pair across all three records.
        for record in [reprotect[0], resend[0]] {
            assert_eq!(token(record, "id="), token(eperm[0], "id="), "{record}");
            assert_eq!(token(record, "fd="), token(eperm[0], "fd="), "{record}");
        }
        assert_ne!(token(reprotect[0], "reprotect_id="), "0");
    }

    /// R1: the re-send is refused EPERM again → both R1 records are emitted (the
    /// "标记在、路径不通" row), the chain STILL asks for exactly one socket and
    /// continues into the unchanged stage 3.
    #[tokio::test]
    async fn a_resend_that_is_refused_again_is_recorded_and_the_chain_still_falls_back() {
        let _serial = crate::socket_supply::notify_test_lock();
        let _clear = ClearListener;
        let _guard = crate::n2_stimulus::test_guard();
        force_eperm(true);
        let supply = Arc::new(SocketSupply::new());
        let served = listen_and_serve_reprotect(&supply, "ok");
        let stop = Arc::new(AtomicBool::new(false));
        let provider = tokio::spawn(serve_until_stopped(supply.clone(), stop.clone(), unix_dgram_fd));
        let (outcome, lines) =
            resolve_chain_traced("os-fallback.test", &supply, &[unreached()], os_resolves_the_test_host).await;
        stop.store(true, Ordering::SeqCst);
        assert_eq!(provider.await.unwrap(), 1, "the EPERM chain still asks for exactly one socket");
        assert_eq!(outcome.unwrap(), Ipv4Addr::new(198, 51, 100, 7));
        assert_eq!(served.load(Ordering::SeqCst), 1);
        let reprotect = r1_lines(&lines, "reprotect");
        assert_eq!(reprotect.len(), 1, "{lines:?}");
        assert!(reprotect[0].ends_with("result=ok"), "{}", reprotect[0]);
        let resend = r1_lines(&lines, "resend");
        assert_eq!(resend.len(), 1, "{lines:?}");
        assert!(resend[0].contains("result=dns:connect-failed errno=1 ("), "{}", resend[0]);
        let eperm = r1_lines(&lines, "eperm");
        assert_eq!(eperm.len(), 1, "{lines:?}");
        assert!(lines.iter().any(|line| line.contains("source=os-resolver ip=198.51.100.7")), "{lines:?}");
        assert!(lines.iter().all(|line| !line.contains("source=protected-retry")), "{lines:?}");
        force_eperm(false);
    }

    /// R1: the experiment is PER CHAIN, not per server — with two EPERM servers
    /// in one stage the shell is asked exactly ONCE, and only the first attempt
    /// carries the R1/re-send records; both refusals keep their EPERM evidence.
    #[tokio::test]
    async fn the_reprotect_experiment_runs_once_per_chain_not_once_per_server() {
        let _serial = crate::socket_supply::notify_test_lock();
        let _clear = ClearListener;
        let _guard = crate::n2_stimulus::test_guard();
        force_eperm(true);
        let supply = Arc::new(SocketSupply::new());
        let served = listen_and_serve_reprotect(&supply, "ok");
        let stop = Arc::new(AtomicBool::new(false));
        let provider = tokio::spawn(serve_until_stopped(supply.clone(), stop.clone(), unix_dgram_fd));
        let servers = [unreached(), SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 99), DNS_PORT)];
        let (outcome, lines) = resolve_chain_traced("once-per-chain.test", &supply, &servers, os_failing).await;
        stop.store(true, Ordering::SeqCst);
        assert_eq!(provider.await.unwrap(), 2, "each server is still tried once");
        assert!(outcome.is_err());
        assert_eq!(served.load(Ordering::SeqCst), 1, "ONE experiment for the whole chain: {lines:?}");
        assert_eq!(r1_lines(&lines, "reprotect").len(), 1, "{lines:?}");
        assert_eq!(r1_lines(&lines, "resend").len(), 1, "{lines:?}");
        assert_eq!(r1_lines(&lines, "eperm").len(), 2, "both refusals keep their evidence: {lines:?}");
        force_eperm(false);
    }

    /// R1: a subscribed shell that never answers → the bound decides, the record
    /// says `timeout`, and the chain sends no re-send (it continues to stage 3).
    #[tokio::test]
    async fn a_silent_shell_records_the_reprotect_timeout_and_no_resend_is_sent() {
        let _serial = crate::socket_supply::notify_test_lock();
        let _clear = ClearListener;
        let _guard = crate::n2_stimulus::test_guard();
        force_eperm(true);
        listen_but_never_answer();
        let supply = Arc::new(SocketSupply::new());
        let stop = Arc::new(AtomicBool::new(false));
        let provider = tokio::spawn(serve_until_stopped(supply.clone(), stop.clone(), unix_dgram_fd));
        let (outcome, lines) =
            resolve_chain_traced("os-fallback.test", &supply, &[unreached()], os_resolves_the_test_host).await;
        stop.store(true, Ordering::SeqCst);
        provider.await.unwrap();
        assert_eq!(outcome.unwrap(), Ipv4Addr::new(198, 51, 100, 7));
        let reprotect = r1_lines(&lines, "reprotect");
        assert_eq!(reprotect.len(), 1, "{lines:?}");
        assert!(reprotect[0].ends_with("result=timeout"), "{}", reprotect[0]);
        assert_ne!(token(reprotect[0], "reprotect_id="), "0", "the record names the request");
        assert!(r1_lines(&lines, "resend").is_empty(), "no answer ⇒ no re-send: {lines:?}");
        assert_eq!(r1_lines(&lines, "eperm").len(), 1, "{lines:?}");
        force_eperm(false);
    }

    /// R1: no shell is subscribed → the experiment is `unavailable` at once (no
    /// wait, no re-send), and the fallback chain is byte-for-byte the pre-R1 one.
    #[tokio::test]
    async fn without_a_listening_shell_the_experiment_is_unavailable_without_a_wait() {
        let _serial = crate::socket_supply::notify_test_lock();
        let _clear = ClearListener;
        let _guard = crate::n2_stimulus::test_guard();
        crate::socket_supply::set_notify(None);
        force_eperm(true);
        let supply = Arc::new(SocketSupply::new());
        let stop = Arc::new(AtomicBool::new(false));
        let provider = tokio::spawn(serve_until_stopped(supply.clone(), stop.clone(), unix_dgram_fd));
        let (outcome, lines) =
            resolve_chain_traced("os-fallback.test", &supply, &[unreached()], os_resolves_the_test_host).await;
        stop.store(true, Ordering::SeqCst);
        provider.await.unwrap();
        assert_eq!(outcome.unwrap(), Ipv4Addr::new(198, 51, 100, 7));
        let reprotect = r1_lines(&lines, "reprotect");
        assert_eq!(reprotect.len(), 1, "{lines:?}");
        assert!(reprotect[0].ends_with("reprotect_id=0 elapsed_ms=0 result=unavailable"),
            "{}", reprotect[0]);
        assert!(r1_lines(&lines, "resend").is_empty(), "{lines:?}");
        assert_eq!(r1_lines(&lines, "eperm").len(), 1, "{lines:?}");
        force_eperm(false);
    }

    // ------------- R2: EPERM means "the protected path is unusable" ----------

    /// R2: an OS resolver that outlives the new bound but NOT the pre-R2 3 s
    /// one — the measured fault shape (794/805 OS-stage timeouts were pure
    /// waiting). The sleep is a real `getaddrinfo` that hangs.
    fn os_hanging_1200ms(_host: &str) -> io::Result<Ipv4Addr> {
        std::thread::sleep(Duration::from_millis(1200));
        Err(io::Error::other("dns:os-resolver-failed: test"))
    }

    /// R2 (design §4.2, AC-R2-1): ONE chain that sees EPERM marks the path
    /// unusable, records that FIRST-CLASS state once, and bounds the OS stage
    /// at 500 ms instead of 3 s. Without the change this chain costs the full
    /// hang (~1.2 s here, up to 3 s on the device).
    #[tokio::test]
    async fn eperm_marks_the_path_unusable_and_bounds_the_os_stage() {
        let _guard = crate::n2_stimulus::test_guard();
        force_eperm(true);
        let supply = Arc::new(SocketSupply::new());
        let stop = Arc::new(AtomicBool::new(false));
        let provider = tokio::spawn(serve_until_stopped(supply.clone(), stop.clone(), unix_dgram_fd));
        let servers = [unreached(), SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 99), DNS_PORT)];
        let started = Instant::now();
        let (outcome, lines) =
            resolve_chain_traced("unusable.test", &supply, &servers, os_hanging_1200ms).await;
        let elapsed = started.elapsed();
        stop.store(true, Ordering::SeqCst);
        assert_eq!(provider.await.unwrap(), 2, "each server is still tried once");
        assert!(outcome.is_err(), "no stage can answer");
        assert!(
            supply.protected_path_unusable(),
            "an EPERM of this chain must latch the 'protected path unusable' state: {lines:?}"
        );
        assert!(
            elapsed < Duration::from_millis(900),
            "the OS stage must be bounded at 500 ms after EPERM, the chain took {elapsed:?}"
        );
        assert_eq!(r1_lines(&lines, "eperm").len(), 2, "{lines:?}");
        let state = r1_lines(&lines, "eperm-state");
        assert_eq!(state.len(), 1, "one state record per chain, not per server: {lines:?}");
        assert!(state[0].contains("host=unusable.test state=unusable attempts=2"), "{}", state[0]);
        assert!(
            token(state[0], "first_id=").parse::<u64>().map(|id| id > 0).unwrap_or(false)
                && token(state[0], "last_id=").parse::<u64>().map(|id| id > 0).unwrap_or(false),
            "{}",
            state[0]
        );
        assert!(state[0].starts_with("protected-dns: eperm-state "), "{}", state[0]);
        assert!(state[0].contains("result=dns:connect-failed errno=1 ("), "{}", state[0]);
        force_eperm(false);
    }

    /// R2 (design §4.2, AC-R2-1/2): a chain that starts while the path is
    /// already known-unusable does NOT receive a protected socket at all — the
    /// supply refuses instantly — so the chain costs one fast refusal plus the
    /// SHORT OS bound and answers from the last-known-good address.
    #[tokio::test]
    async fn a_known_unusable_path_is_refused_instantly_and_keeps_the_short_os_bound() {
        let _guard = crate::n2_stimulus::test_guard();
        let supply = Arc::new(SocketSupply::new());
        supply.remember_address("cached.test", Ipv4Addr::new(203, 0, 113, 9));
        supply.mark_unusable(crate::socket_supply::PROTECTED_PATH_UNUSABLE);
        let started = Instant::now();
        let (outcome, lines) =
            resolve_chain_traced("cached.test", &supply, &[unreached()], os_hanging_1200ms).await;
        let elapsed = started.elapsed();
        assert_eq!(outcome.unwrap(), Ipv4Addr::new(203, 0, 113, 9), "{lines:?}");
        assert!(supply.poll_request().is_none(), "no socket is even requested: {lines:?}");
        // The refusal is the supply's own state answer (the per-attempt
        // `protected-dns:` line lives on hilog, not in the traced vector), and the
        // OS stage timed out at the R2 500 ms bound instead of the pre-R2 3 s.
        assert!(supply.path_state().refuse_dns, "the DNS fast refusal is in force");
        assert!(
            lines.iter().any(|line| line.contains("dns:os-resolver-timeout")),
            "the OS stage was bounded: {lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.contains("source=last-known-good ip=203.0.113.9")),
            "{lines:?}"
        );
        assert!(
            lines.iter().all(|line| !line.contains("source=protected-retry")),
            "a known-unusable path is not retried: {lines:?}"
        );
        assert_eq!(r1_lines(&lines, "eperm-state").len(), 0, "this chain saw no EPERM itself");
        assert!(
            elapsed < Duration::from_millis(900),
            "the latched state keeps the OS bound short, the chain took {elapsed:?}"
        );
    }

    /// R2: the fast failure is a WINDOW, not a permanent latch — after it the
    /// next chain is a PROBE, and a protected answer is the liveness evidence
    /// that clears the state.
    #[tokio::test]
    async fn a_protected_answer_after_the_probe_window_clears_the_unusable_state() {
        let _guard = crate::n2_stimulus::test_guard();
        let server = AsyncUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = match server.local_addr().unwrap() { std::net::SocketAddr::V4(a) => a, _ => unreachable!() };
        let supply = Arc::new(SocketSupply::new());
        supply.mark_unusable(crate::socket_supply::PROTECTED_PATH_UNUSABLE);
        supply.set_unusable_probe_window(Duration::ZERO);
        assert!(supply.protected_path_unusable(), "the latch still bounds the OS stage");
        let provider = tokio::spawn(supply_requests(supply.clone(), 1, true));
        let responder = tokio::spawn(async move {
            let mut buf = [0u8; 512];
            let (n, peer) = server.recv_from(&mut buf).await.unwrap();
            server.send_to(&answer(&buf[..n], [203, 0, 113, 31]), peer).await.unwrap();
        });
        let (outcome, lines) =
            resolve_chain_traced("probe.test", &supply, &[address], os_failing).await;
        provider.await.unwrap();
        responder.await.unwrap();
        assert_eq!(outcome.unwrap(), Ipv4Addr::new(203, 0, 113, 31), "{lines:?}");
        assert!(
            !supply.protected_path_unusable(),
            "a protected answer is liveness evidence and clears the R2 state: {lines:?}"
        );
        assert!(!supply.path_state().refuse_dns);
    }

    /// R2: the fast-refusal token is terminal for the chain — retrying it would
    /// only ask a path that is already known-unusable for a second socket. The
    /// pre-existing transient tokens keep their one retry.
    #[test]
    fn the_unusable_token_is_not_retried() {
        assert!(!is_transient(&io::Error::other("socket-supply-unusable")));
        for token in ["socket-supply-rejected", "socket-supply-timeout",
            "socket-supply-closed", "socket-protect-failed", "physical-network-changed"] {
            assert!(is_transient(&io::Error::other(token)), "{token} must stay transient");
        }
    }
}
