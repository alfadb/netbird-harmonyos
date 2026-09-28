// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! IPv4 A lookups over a fresh, shell-protected UDP socket per DNS server.
//! No system resolver is consulted when a supply is present.

use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::os::fd::AsRawFd;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crypto_box::aead::rand_core::RngCore;

use crate::config::{parse_document, Json};
use crate::socket_supply::{Purpose, SocketSupply};

const QUERY_TIMEOUT: Duration = Duration::from_millis(1500);

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
    if let Ok(ip) = host.parse::<Ipv4Addr>() { return Ok(ip); }
    let servers: Vec<_> = supply.dns_servers().into_iter()
        .map(|ip| SocketAddrV4::new(ip, 53)).collect();
    resolve_on_servers(host, supply, &servers).await
}

async fn resolve_on_servers(host: &str, supply: &Arc<SocketSupply>, servers: &[SocketAddrV4]) -> io::Result<Ipv4Addr> {
    if servers.is_empty() { return Err(io::Error::other("dns:no-physical-server")); }
    let mut last = io::Error::other("dns:no-answer");
    for &addr in servers {
        let started = Instant::now();
        let mut id = [0u8; 2];
        crypto_box::aead::OsRng.fill_bytes(&mut id);
        let query = question(host, u16::from_be_bytes(id))?;
        let mut request_id = 0;
        let mut request_fd = -1;
        let outcome: io::Result<Ipv4Addr> = async {
            let (id, fd) = supply.request_with_id(Purpose::Dns).await.map_err(|e| io::Error::other(e.token()))?;
            request_id = id;
            let raw = fd.as_raw_fd();
            request_fd = raw;
            crate::ice::set_nonblock(raw).map_err(|e| io::Error::other(e.token()))?;
            let socket = tokio::net::UdpSocket::from_std(UdpSocket::from(fd))?;
            tokio::time::timeout(QUERY_TIMEOUT, async {
                socket.connect(addr).await?;
                crate::n2_socket_trace::io_begin(socket.as_raw_fd(), addr.into());
                let sent = socket.send(&query).await;
                crate::n2_socket_trace::first_io(socket.as_raw_fd(), addr.into(),
                    if sent.is_ok() { "sent" } else { "failed" }, "dns_query");
                sent?;
                let mut buf = [0u8; 1500];
                let size = socket.recv(&mut buf).await?;
                response(&buf[..size], &query)
            }).await.map_err(|_| io::Error::other("dns:timeout"))?
        }.await;
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
}
