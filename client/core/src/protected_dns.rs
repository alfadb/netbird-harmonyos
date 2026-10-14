// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! IPv4 A lookups over a fresh, shell-protected UDP socket per DNS server.
//! The protected path is the PRIMARY resolver, but since 0.0.34 (protected-DNS
//! outage REPORT.md §5 N2) it is no longer the ONLY one: one refused or silent
//! lookup must never leave a caller without an address, so every lookup runs
//! through [`resolve_chain`] — the protected attempt (per server, in list
//! order), then ONE retry through the same path when the failure was transient,
//! then the OS/system resolver, then the supply's last-known-good address. Only
//! when all of them fail does the lookup return an error (the shell sees an
//! empty resolution).

use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::os::fd::AsRawFd;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crypto_box::aead::rand_core::RngCore;

use crate::config::{parse_document, Json};
use crate::socket_supply::{Purpose, SocketSupply};

const QUERY_TIMEOUT: Duration = Duration::from_millis(1500);

/// N2(b) ceiling on the OS/system-resolver fallback. `getaddrinfo` is a
/// blocking libc call on a thread of the runtime's blocking pool; the lookup
/// must never outlive the shell's own poll budget for it (the extension polls
/// `connector_protected_dns_poll` every 100 ms, `NetBirdConnector.ets`
/// `resolveProtectedIp`), so a resolver that hangs is abandoned as
/// `dns:os-resolver-timeout` and the chain falls through to the cache.
const OS_RESOLVE_TIMEOUT: Duration = Duration::from_millis(3000);

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
    let errno = match error.raw_os_error() {
        Some(code) => code.to_string(),
        None => "none".to_string(),
    };
    io::Error::other(format!("dns:{call}-failed errno={errno} ({error})"))
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
    ["dns:connect-failed", "dns:send-failed", "dns:recv-failed",
     "socket-supply", "socket-protect-failed", "physical-network-changed"]
        .iter()
        .any(|token| text.starts_with(token))
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

/// N2 (REPORT.md §5): the fallback chain around one protected lookup. Every
/// caller goes through it, so no single refused (or silent) lookup can leave a
/// caller — the management dial, the relay dial, the STUN gather — without an
/// address.
///
/// Order (fixed, and the reason the chain is written linearly):
///   1. the protected path over the shell-fed server list — the primary;
///   2. N2(a): ONE retry through the same path when the failure was transient;
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

    // Stage 1 — the protected path. Unchanged shape: one fresh protected socket
    // and one query per server, in the order the shell fed them.
    let mut last = match resolve_on_servers(host, supply, servers).await {
        Ok(ip) => {
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

    // Stage 2 (N2a) — one retry, transient failures only.
    if is_transient(&last) {
        match resolve_on_servers(host, supply, servers).await {
            Ok(ip) => {
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
            }
        }
    }

    // Stage 3 (N2b) — the OS/system resolver.
    match resolve_via_os(host, os).await {
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

/// N2(b): run the OS resolver off the async worker, bounded by
/// [`OS_RESOLVE_TIMEOUT`]. A `getaddrinfo` that outlives the bound is abandoned
/// (its blocking thread finishes on its own); the chain then uses the cache.
async fn resolve_via_os(host: &str, os: OsResolver) -> io::Result<Ipv4Addr> {
    let name = host.to_string();
    let call = tokio::task::spawn_blocking(move || os(&name));
    match tokio::time::timeout(OS_RESOLVE_TIMEOUT, call).await {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(join_error)) => Err(io::Error::other(format!("dns:os-resolver-join-failed: {join_error}"))),
        Err(_) => Err(io::Error::other("dns:os-resolver-timeout")),
    }
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

async fn resolve_on_servers(host: &str, supply: &Arc<SocketSupply>, servers: &[SocketAddrV4]) -> io::Result<Ipv4Addr> {
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
    for &addr in servers {
        let started = Instant::now();
        let mut id = [0u8; 2];
        crypto_box::aead::OsRng.fill_bytes(&mut id);
        let query_id = u16::from_be_bytes(id);
        let query = question(host, query_id)?;
        let mut request_id = 0;
        let mut request_fd = -1;
        let mut facts = DnsFacts::new();
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
                socket.connect(addr).await.map_err(|e| syscall_error("connect", e))?;
                crate::n2_socket_trace::io_begin(socket.as_raw_fd(), addr.into());
                let sent = socket.send(&query).await;
                facts.sent_mono_ms = crate::sys::mono_ms();
                crate::n2_socket_trace::first_io(socket.as_raw_fd(), addr.into(),
                    if sent.is_ok() { "sent" } else { "failed" }, "dns_query");
                sent.map_err(|e| syscall_error("send", e))?;
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
        // N2 D-row: one armed-only `N2_DNS` receipt per production query, before
        // the (unchanged) `protected-dns:` result line below.
        emit_dns_facts(host, addr, query_id, request_fd, &facts, &outcome,
            started.elapsed().as_millis());
        match outcome {
            Ok(ip) => {
                crate::hilog::emit(&format!("protected-dns: id={request_id} host={host} fd={request_fd} server={addr} elapsed_ms={} result={ip}", started.elapsed().as_millis()));
                return Ok(ip);
            }
            Err(err) => {
                crate::hilog::emit(&format!("protected-dns: id={request_id} host={host} fd={request_fd} server={addr} elapsed_ms={} result={err}", started.elapsed().as_millis()));
                last = err;
            }
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
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

    /// N2: the protected path refuses (both the attempt and its retry) → the OS
    /// resolver answers.
    #[tokio::test]
    async fn a_protected_refusal_falls_back_to_the_os_resolver() {
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
    /// hosts-file name and fails, bounded, for a name that does not exist.
    #[tokio::test]
    async fn the_real_os_resolver_path_resolves_localhost_and_is_bounded() {
        assert_eq!(resolve_via_os("localhost", os_getaddrinfo).await.unwrap(), Ipv4Addr::LOCALHOST);
        assert!(resolve_via_os("no-such-host.invalid", os_getaddrinfo).await.is_err());
    }
}
