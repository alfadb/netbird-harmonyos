//! Caller-driven DNS-only IPv4 stack; no OS I/O or threads.
//!
//! The sole local address is runtime_ip/32. Feed raw unfragmented IPv4
//! UDP/TCP runtime_ip:53 packets, poll, drain requests, supply resolver wire
//! answers, then drain packets. AAAA DNS messages work over this IPv4 stack.
//! This module performs no DNS resolution or upstream selection/routing.
//!
//! Limits: 16 TCP slots, 128 pending requests, 64 frames per direction,
//! 8192-byte TCP socket buffers, <=73729 framing bytes per connection and
//! 1 MiB queued TCP answers. Slots relisten immediately after recycling.
//! TCP DNS messages may contain 65535 bytes. UDP answers exceeding MTU-28 fail
//! explicitly; the resolver must truncate them. IPv4 fragments are unsupported.
//!
//! Caller time is monotonic milliseconds, clamped against regressions and to
//! i64::MAX. Frames, resolver requests and incomplete TCP framing expire after
//! 5 seconds; TCP application idle expires after 30 seconds. Poll drives all
//! expiration. next_poll_ms combines cached smoltcp deadlines and application
//! work/timeouts; call poll after wakeup and after responses. Response errors
//! leave known requests retryable until their deadline.
//! reset discards all state except the monotonically allocated request IDs.
use smoltcp::{
    iface::{Config, Interface, SocketHandle, SocketSet},
    phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken},
    socket::{tcp, udp},
    time::{Duration, Instant},
    wire::{
        HardwareAddress, IpAddress, IpCidr, IpEndpoint, IpProtocol, Ipv4Packet, TcpPacket,
        UdpPacket,
    },
};
use std::{
    collections::{BTreeMap, VecDeque},
    net::Ipv4Addr,
};
pub const MAX_CONNECTIONS: usize = 16;
pub const MAX_REQUESTS: usize = 128;
pub const MAX_PACKETS: usize = 64;
const BUFFER: usize = 8192;
const REQUEST_MS: u64 = 5000;
const IDLE_MS: u64 = 30000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsProtocol {
    Udp,
    Tcp,
}
#[derive(Debug)]
pub struct DnsRequest {
    pub id: u64,
    pub wire: Vec<u8>,
    pub protocol: DnsProtocol,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsTransportError {
    InvalidConfiguration,
    UnknownRequest,
    ResponseTooLarge,
    QueueFull,
}
impl std::fmt::Display for DnsTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DnsTransportError {}
struct MemoryDevice {
    input: VecDeque<(u64, Vec<u8>)>,
    output: VecDeque<(u64, Vec<u8>)>,
    mtu: usize,
}
struct Receive(Vec<u8>);
struct Transmit<'a>(&'a mut VecDeque<(u64, Vec<u8>)>, u64);
impl RxToken for Receive {
    fn consume<R, F: FnOnce(&[u8]) -> R>(self, f: F) -> R {
        f(&self.0)
    }
}
impl TxToken for Transmit<'_> {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, len: usize, f: F) -> R {
        let mut p = vec![0; len];
        let r = f(&mut p);
        self.0.push_back((self.1, p));
        r
    }
}
impl Device for MemoryDevice {
    type RxToken<'a> = Receive;
    type TxToken<'a> = Transmit<'a>;
    fn receive(&mut self, time: Instant) -> Option<(Receive, Transmit<'_>)> {
        if self.output.len() >= MAX_PACKETS {
            return None;
        }
        Some((
            Receive(self.input.pop_front()?.1),
            Transmit(&mut self.output, time.total_millis().max(0) as u64),
        ))
    }
    fn transmit(&mut self, time: Instant) -> Option<Transmit<'_>> {
        (self.output.len() < MAX_PACKETS)
            .then(|| Transmit(&mut self.output, time.total_millis().max(0) as u64))
    }
    fn capabilities(&self) -> DeviceCapabilities {
        let mut c = DeviceCapabilities::default();
        c.medium = Medium::Ip;
        c.max_transmission_unit = self.mtu;
        c
    }
}
enum Destination {
    Udp(IpEndpoint),
    Tcp(usize, u64),
}
struct Pending {
    destination: Destination,
    expires: u64,
}
struct Connection {
    handle: SocketHandle,
    generation: u64,
    active: bool,
    last: u64,
    partial: Option<u64>,
    receive: Vec<u8>,
    send: VecDeque<Vec<u8>>,
    offset: usize,
}
pub struct DnsTransport {
    runtime_ip: Ipv4Addr,
    seed: u64,
    device: MemoryDevice,
    interface: Interface,
    sockets: SocketSet<'static>,
    udp: SocketHandle,
    connections: Vec<Connection>,
    requests: VecDeque<DnsRequest>,
    pending: BTreeMap<u64, Pending>,
    next_id: u64,
    now: u64,
    deadline: Option<u64>,
    queued: usize,
}
impl DnsTransport {
    pub fn new(runtime_ip: Ipv4Addr, mtu: usize, seed: u64) -> Result<Self, DnsTransportError> {
        if !(576..=65535).contains(&mtu)
            || runtime_ip.is_unspecified()
            || runtime_ip.is_multicast()
            || runtime_ip.is_broadcast()
        {
            return Err(DnsTransportError::InvalidConfiguration);
        }
        let mut device = MemoryDevice {
            input: VecDeque::new(),
            output: VecDeque::new(),
            mtu,
        };
        let mut config = Config::new(HardwareAddress::Ip);
        config.random_seed = seed;
        let mut interface = Interface::new(config, &mut device, Instant::from_millis(0));
        interface.update_ip_addrs(|a| {
            a.push(IpCidr::new(IpAddress::Ipv4(runtime_ip), 32))
                .unwrap();
        });
        interface
            .routes_mut()
            .add_default_ipv4_route(runtime_ip)
            .unwrap();
        let mut sockets = SocketSet::new(vec![]);
        let mut socket = udp::Socket::new(
            udp::PacketBuffer::new(
                vec![udp::PacketMetadata::EMPTY; MAX_PACKETS],
                vec![0; mtu * MAX_PACKETS],
            ),
            udp::PacketBuffer::new(
                vec![udp::PacketMetadata::EMPTY; MAX_PACKETS],
                vec![0; mtu * MAX_PACKETS],
            ),
        );
        socket.bind((runtime_ip, 53)).unwrap();
        let udp = sockets.add(socket);
        let mut connections = Vec::new();
        for _ in 0..MAX_CONNECTIONS {
            let mut s = tcp::Socket::new(
                tcp::SocketBuffer::new(vec![0; BUFFER]),
                tcp::SocketBuffer::new(vec![0; BUFFER]),
            );
            s.set_timeout(Some(Duration::from_millis(IDLE_MS)));
            s.listen((runtime_ip, 53)).unwrap();
            connections.push(Connection {
                handle: sockets.add(s),
                generation: 0,
                active: false,
                last: 0,
                partial: None,
                receive: Vec::new(),
                send: VecDeque::new(),
                offset: 0,
            });
        }
        Ok(Self {
            runtime_ip,
            seed,
            device,
            interface,
            sockets,
            udp,
            connections,
            requests: VecDeque::new(),
            pending: BTreeMap::new(),
            next_id: 1,
            now: 0,
            deadline: None,
            queued: 0,
        })
    }
    /// Unfragmented IPv4 only. smoltcp validates transport/IP checksums.
    pub fn is_dns_packet(&self, frame: &[u8]) -> bool {
        let Ok(ip) = Ipv4Packet::new_checked(frame) else {
            return false;
        };
        if ip.version() != 4
            || ip.dst_addr() != self.runtime_ip
            || ip.frag_offset() != 0
            || ip.more_frags()
        {
            return false;
        }
        match ip.next_header() {
            IpProtocol::Udp => {
                UdpPacket::new_checked(ip.payload()).is_ok_and(|p| p.dst_port() == 53)
            }
            IpProtocol::Tcp => {
                TcpPacket::new_checked(ip.payload()).is_ok_and(|p| p.dst_port() == 53)
            }
            _ => false,
        }
    }
    /// Full queues still consume recognized DNS traffic.
    pub fn ingest(&mut self, frame: &[u8], now_ms: u64) -> bool {
        if !self.is_dns_packet(frame) {
            return false;
        }
        self.now = self.now.max(now_ms.min(i64::MAX as u64));
        if frame.len() <= self.device.mtu && self.device.input.len() < MAX_PACKETS {
            self.device.input.push_back((self.now, frame.to_vec()));
        }
        true
    }
    fn request(&mut self, wire: Vec<u8>, protocol: DnsProtocol, destination: Destination) {
        if self.pending.len() >= MAX_REQUESTS
            || self.requests.len() >= MAX_REQUESTS
            || self.next_id == u64::MAX
        {
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.pending.insert(
            id,
            Pending {
                destination,
                expires: self.now.saturating_add(REQUEST_MS),
            },
        );
        self.requests.push_back(DnsRequest { id, wire, protocol });
    }
    fn recycle(&mut self, index: usize) {
        let c = &mut self.connections[index];
        let s = self.sockets.get_mut::<tcp::Socket>(c.handle);
        s.abort();
        s.listen((self.runtime_ip, 53)).unwrap();
        self.queued -= c.send.iter().map(Vec::len).sum::<usize>();
        c.send.clear();
        c.receive.clear();
        c.offset = 0;
        c.partial = None;
        c.active = false;
        c.generation = c.generation.wrapping_add(1);
        self.pending
            .retain(|_, p| !matches!(p.destination, Destination::Tcp(i, _) if i == index));
    }
    /// <=min(budget,64) rounds; each round one ingress, bounded egress,
    /// <=8192 received bytes and <=one complete message per TCP socket.
    pub fn poll(&mut self, now_ms: u64, budget: usize) {
        if budget == 0 {
            return;
        }
        self.now = self.now.max(now_ms.min(i64::MAX as u64));
        self.device
            .input
            .retain(|(t, _)| self.now.saturating_sub(*t) < REQUEST_MS);
        self.device
            .output
            .retain(|(t, _)| self.now.saturating_sub(*t) < REQUEST_MS);
        self.pending.retain(|_, p| p.expires > self.now);
        self.requests.retain(|r| self.pending.contains_key(&r.id));
        for _ in 0..budget.min(MAX_PACKETS) {
            let time = Instant::from_millis(self.now as i64);
            self.interface
                .poll_ingress_single(time, &mut self.device, &mut self.sockets);
            let datagram = self
                .sockets
                .get_mut::<udp::Socket>(self.udp)
                .recv()
                .ok()
                .map(|(w, m)| (w.to_vec(), m.endpoint));
            if let Some((wire, endpoint)) = datagram {
                self.request(wire, DnsProtocol::Udp, Destination::Udp(endpoint));
            }
            for index in 0..self.connections.len() {
                let c = &mut self.connections[index];
                let s = self.sockets.get_mut::<tcp::Socket>(c.handle);
                if s.state() != tcp::State::Listen && !c.active {
                    c.active = true;
                    c.last = self.now;
                }
                if c.active
                    && (s.state() == tcp::State::Closed
                        || self.now.saturating_sub(c.last) >= IDLE_MS
                        || c.partial
                            .is_some_and(|t| self.now.saturating_sub(t) >= REQUEST_MS))
                {
                    self.recycle(index);
                    continue;
                }
                if s.can_recv() {
                    let mut bytes = [0; BUFFER];
                    if let Ok(n) = s.recv_slice(&mut bytes) {
                        c.receive.extend_from_slice(&bytes[..n]);
                        c.last = self.now;
                    }
                }
                if c.receive.len() > 65537 + BUFFER {
                    self.recycle(index);
                    continue;
                }
                let message = if c.receive.len() >= 2 {
                    let len = u16::from_be_bytes([c.receive[0], c.receive[1]]) as usize;
                    if c.receive.len() >= len + 2 {
                        let wire = c.receive[2..len + 2].to_vec();
                        c.receive.drain(..len + 2);
                        Some(wire)
                    } else {
                        None
                    }
                } else {
                    None
                };
                c.partial = if c.receive.is_empty() {
                    None
                } else {
                    Some(c.partial.unwrap_or(self.now))
                };
                if s.can_send() {
                    if let Some(front) = c.send.front() {
                        if let Ok(n) = s.send_slice(&front[c.offset..]) {
                            c.offset += n;
                            c.last = self.now;
                        }
                        if c.offset == front.len() {
                            self.queued -= front.len();
                            c.send.pop_front();
                            c.offset = 0;
                        }
                    }
                }
                let generation = c.generation;
                if message.is_none() && s.state() == tcp::State::CloseWait && c.receive.is_empty() && c.send.is_empty() && !self.pending.values().any(|p| matches!(p.destination, Destination::Tcp(i,g) if i == index && g == generation)) { s.close(); }
                if let Some(wire) = message {
                    self.request(wire, DnsProtocol::Tcp, Destination::Tcp(index, generation));
                }
            }
            self.interface
                .poll_egress(time, &mut self.device, &mut self.sockets);
        }
        self.deadline = self
            .interface
            .poll_at(Instant::from_millis(self.now as i64), &self.sockets)
            .map(|t| (t.total_millis().max(0) as u64).max(self.now));
    }
    pub fn take_requests(&mut self, limit: usize) -> Vec<DnsRequest> {
        self.requests.retain(|r| {
            self.pending
                .get(&r.id)
                .is_some_and(|p| p.expires > self.now)
        });
        self.requests
            .drain(..limit.min(self.requests.len()))
            .collect()
    }
    pub fn respond(&mut self, id: u64, wire: Vec<u8>) -> Result<(), DnsTransportError> {
        let p = self
            .pending
            .get(&id)
            .filter(|p| p.expires > self.now)
            .ok_or(DnsTransportError::UnknownRequest)?;
        if wire.len() > 65535 {
            return Err(DnsTransportError::ResponseTooLarge);
        }
        match p.destination {
            Destination::Udp(endpoint) => {
                if wire.len() + 28 > self.device.mtu {
                    return Err(DnsTransportError::ResponseTooLarge);
                }
                self.sockets
                    .get_mut::<udp::Socket>(self.udp)
                    .send_slice(&wire, endpoint)
                    .map_err(|_| DnsTransportError::QueueFull)?;
            }
            Destination::Tcp(i, g) => {
                let c = &mut self.connections[i];
                if c.generation != g || !self.sockets.get::<tcp::Socket>(c.handle).may_send() {
                    return Err(DnsTransportError::UnknownRequest);
                }
                if self.queued + wire.len() + 2 > 1024 * 1024 || c.send.len() >= MAX_REQUESTS {
                    return Err(DnsTransportError::QueueFull);
                }
                let mut framed = Vec::with_capacity(wire.len() + 2);
                framed.extend_from_slice(&(wire.len() as u16).to_be_bytes());
                framed.extend_from_slice(&wire);
                self.queued += framed.len();
                c.send.push_back(framed);
            }
        }
        self.pending.remove(&id);
        Ok(())
    }
    pub fn take_packets(&mut self, limit: usize) -> Vec<Vec<u8>> {
        self.device
            .output
            .drain(..limit.min(self.device.output.len()))
            .map(|(_, packet)| packet)
            .collect()
    }
    pub fn next_poll_ms(&self, now_ms: u64) -> Option<u64> {
        let now = now_ms.max(self.now);
        if !self.device.input.is_empty()
            || !self.device.output.is_empty()
            || self.connections.iter().any(|c| {
                (!c.send.is_empty() && self.sockets.get::<tcp::Socket>(c.handle).can_send())
                    || (c.receive.len() >= 2
                        && c.receive.len()
                            >= 2 + u16::from_be_bytes([c.receive[0], c.receive[1]]) as usize)
            })
            || self.sockets.get::<udp::Socket>(self.udp).send_queue() > 0
        {
            return Some(now);
        }
        self.deadline
            .into_iter()
            .chain(self.pending.values().map(|p| p.expires))
            .chain(
                self.connections
                    .iter()
                    .filter(|c| c.active)
                    .map(|c| c.last.saturating_add(IDLE_MS)),
            )
            .chain(
                self.connections
                    .iter()
                    .filter_map(|c| c.partial.map(|t| t.saturating_add(REQUEST_MS))),
            )
            .min()
            .map(|t| t.max(now))
    }
    /// Request IDs survive resets to reject old resolver completions.
    pub fn reset(&mut self) {
        let id = self.next_id;
        let now = self.now;
        let mut fresh =
            Self::new(self.runtime_ip, self.device.mtu, self.seed.wrapping_add(id)).unwrap();
        fresh.next_id = id;
        fresh.now = now;
        *self = fresh;
    }
}
