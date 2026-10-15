// SPDX-License-Identifier: AGPL-3.0-or-later
//! One-server DNS wire exchange with protected sockets only for physical DNS.
use crate::socket_supply::{Purpose, SocketKind, SocketSupply};
use hickory_proto::{
    op::{Header, MessageType, Query},
    serialize::binary::{BinDecodable, BinDecoder},
};
use std::{
    io,
    net::SocketAddr,
    os::fd::{FromRawFd, IntoRawFd},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpSocket, UdpSocket},
};

// Official upstream.go:92 UpstreamTimeout; shared by supply, I/O and TC retry.
const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_WIRE: usize = u16::MAX as usize;

#[derive(Debug)]
pub struct DnsExchange {
    pub wire: Vec<u8>,
    pub used_tcp: bool,
    pub protected: bool,
}
/// Domain-free categories suitable for counters; no packet is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsUpstreamError {
    InvalidQuery,
    Supply,
    Connect,
    Send,
    Receive,
    Eof,
    Timeout,
}
impl std::fmt::Display for DnsUpstreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidQuery => "dns-invalid-query",
            Self::Supply => "dns-supply",
            Self::Connect => "dns-connect",
            Self::Send => "dns-send",
            Self::Receive => "dns-receive",
            Self::Eof => "dns-eof",
            Self::Timeout => "dns-timeout",
        })
    }
}
impl std::error::Error for DnsUpstreamError {}
pub struct DnsUpstream {
    supply: Arc<SocketSupply>,
}

// Only decode questions. Unknown records, EDNS and even TC-partial answer
// sections remain opaque; forwarding never reserializes the original wire.
fn identity(wire: &[u8]) -> Option<(Header, Vec<Query>)> {
    let mut decoder = BinDecoder::new(wire);
    let header = Header::read(&mut decoder).ok()?;
    let mut questions = Vec::new();
    for _ in 0..header.query_count() {
        questions.push(Query::read(&mut decoder).ok()?);
    }
    Some((header, questions))
}
fn matches(wire: &[u8], header: &Header, questions: &[Query]) -> bool {
    identity(wire).is_some_and(|(h, q)| {
        h.message_type() == MessageType::Response
            && h.id() == header.id()
            && h.op_code() == header.op_code()
            && q == questions
    })
}
impl DnsUpstream {
    pub fn new(supply: Arc<SocketSupply>) -> Self {
        Self { supply }
    }
    fn physical(&self, server: SocketAddr) -> bool {
        server.port() == 53
            && match server.ip() {
                std::net::IpAddr::V4(ip) => self.supply.dns_servers().contains(&ip),
                _ => false,
            }
    }
    pub async fn exchange(
        &self,
        server: SocketAddr,
        query: &[u8],
        tcp: bool,
    ) -> Result<DnsExchange, DnsUpstreamError> {
        if query.len() > MAX_WIRE {
            return Err(DnsUpstreamError::InvalidQuery);
        }
        let (header, questions) = identity(query).ok_or(DnsUpstreamError::InvalidQuery)?;
        if header.message_type() != MessageType::Query || questions.is_empty() {
            return Err(DnsUpstreamError::InvalidQuery);
        }
        tokio::time::timeout(EXCHANGE_TIMEOUT, async {
            let protected = self.physical(server);
            let mut used_tcp = tcp;
            let wire = if tcp {
                self.tcp(server, query, &header, &questions, protected)
                    .await?
            } else {
                let wire = self
                    .udp(server, query, &header, &questions, protected)
                    .await?;
                if wire[2] & 2 != 0 {
                    used_tcp = true;
                    self.tcp(server, query, &header, &questions, protected)
                        .await?
                } else {
                    wire
                }
            };
            Ok(DnsExchange {
                wire,
                used_tcp,
                protected,
            })
        })
        .await
        .map_err(|_| DnsUpstreamError::Timeout)?
    }
    async fn udp(
        &self,
        server: SocketAddr,
        query: &[u8],
        header: &Header,
        questions: &[Query],
        protected: bool,
    ) -> Result<Vec<u8>, DnsUpstreamError> {
        let socket = if protected {
            let fd = self
                .supply
                .request_socket(Purpose::Dns, SocketKind::Udp)
                .await
                .map_err(|_| DnsUpstreamError::Supply)?;
            let socket = std::net::UdpSocket::from(fd);
            socket
                .set_nonblocking(true)
                .map_err(|_| DnsUpstreamError::Connect)?;
            UdpSocket::from_std(socket).map_err(|_| DnsUpstreamError::Connect)?
        } else {
            let bind = if server.is_ipv4() {
                SocketAddr::from(([0, 0, 0, 0], 0))
            } else {
                SocketAddr::from(([0u16; 8], 0))
            };
            UdpSocket::bind(bind)
                .await
                .map_err(|_| DnsUpstreamError::Connect)?
        };
        socket
            .connect(server)
            .await
            .map_err(|_| DnsUpstreamError::Connect)?;
        if socket
            .send(query)
            .await
            .map_err(|_| DnsUpstreamError::Send)?
            != query.len()
        {
            return Err(DnsUpstreamError::Send);
        }
        let mut wire = vec![0; MAX_WIRE];
        loop {
            let len = socket
                .recv(&mut wire)
                .await
                .map_err(|_| DnsUpstreamError::Receive)?;
            if matches(&wire[..len], header, questions) {
                wire.truncate(len);
                return Ok(wire);
            }
        }
    }
    async fn tcp(
        &self,
        server: SocketAddr,
        query: &[u8],
        header: &Header,
        questions: &[Query],
        protected: bool,
    ) -> Result<Vec<u8>, DnsUpstreamError> {
        let socket = if protected {
            let fd = self
                .supply
                .request_socket(Purpose::Dns, SocketKind::Tcp)
                .await
                .map_err(|_| DnsUpstreamError::Supply)?;
            let stream = std::net::TcpStream::from(fd);
            stream
                .set_nonblocking(true)
                .map_err(|_| DnsUpstreamError::Connect)?;
            // Direct ownership transfer: every error/cancellation drops the fd.
            unsafe { TcpSocket::from_raw_fd(stream.into_raw_fd()) }
        } else {
            (if server.is_ipv4() {
                TcpSocket::new_v4()
            } else {
                TcpSocket::new_v6()
            })
            .map_err(|_| DnsUpstreamError::Connect)?
        };
        let mut stream = socket
            .connect(server)
            .await
            .map_err(|_| DnsUpstreamError::Connect)?;
        stream
            .write_all(&(query.len() as u16).to_be_bytes())
            .await
            .map_err(|_| DnsUpstreamError::Send)?;
        stream
            .write_all(query)
            .await
            .map_err(|_| DnsUpstreamError::Send)?;
        loop {
            let mut prefix = [0; 2];
            stream.read_exact(&mut prefix).await.map_err(read_error)?;
            let mut wire = vec![0; u16::from_be_bytes(prefix) as usize];
            stream.read_exact(&mut wire).await.map_err(read_error)?;
            if matches(&wire, header, questions) {
                return Ok(wire);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_selection_is_exact_and_tracks_updates() {
        let supply = Arc::new(SocketSupply::new());
        supply.set_dns_servers(vec!["192.0.2.53".parse().unwrap()]);
        let upstream = DnsUpstream::new(supply.clone());
        assert!(upstream.physical("192.0.2.53:53".parse().unwrap()));
        for addr in [
            "192.0.2.53:5353",
            "8.8.8.8:53",
            "[::ffff:192.0.2.53]:53",
            "[::1]:53",
        ] {
            assert!(!upstream.physical(addr.parse().unwrap()));
        }
        supply.set_dns_servers(vec![]);
        assert!(!upstream.physical("192.0.2.53:53".parse().unwrap()));
    }
}
fn read_error(error: io::Error) -> DnsUpstreamError {
    if error.kind() == io::ErrorKind::UnexpectedEof {
        DnsUpstreamError::Eof
    } else {
        DnsUpstreamError::Receive
    }
}
