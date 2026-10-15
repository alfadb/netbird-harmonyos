#[path = "../src/dns_transport.rs"]
mod dns_transport;
use dns_transport::*;
use smoltcp::{
    iface::{Config, Interface, SocketHandle, SocketSet},
    phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken},
    socket::{tcp, udp},
    time::Instant,
    wire::{HardwareAddress, IpAddress, IpCidr, IpProtocol, Ipv4Packet, TcpPacket, UdpPacket},
};
use std::{collections::VecDeque, net::Ipv4Addr};
const SERVER: Ipv4Addr = Ipv4Addr::new(100, 100, 0, 53);
const CLIENT: Ipv4Addr = Ipv4Addr::new(100, 64, 0, 2);
struct Dev {
    rx: VecDeque<Vec<u8>>,
    tx: VecDeque<Vec<u8>>,
}
struct Rx(Vec<u8>);
struct Tx<'a>(&'a mut VecDeque<Vec<u8>>);
impl RxToken for Rx {
    fn consume<R, F: FnOnce(&[u8]) -> R>(self, f: F) -> R {
        f(&self.0)
    }
}
impl TxToken for Tx<'_> {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, n: usize, f: F) -> R {
        let mut p = vec![0; n];
        let r = f(&mut p);
        self.0.push_back(p);
        r
    }
}
impl Device for Dev {
    type RxToken<'a> = Rx;
    type TxToken<'a> = Tx<'a>;
    fn receive(&mut self, _: Instant) -> Option<(Rx, Tx<'_>)> {
        Some((Rx(self.rx.pop_front()?), Tx(&mut self.tx)))
    }
    fn transmit(&mut self, _: Instant) -> Option<Tx<'_>> {
        Some(Tx(&mut self.tx))
    }
    fn capabilities(&self) -> DeviceCapabilities {
        let mut c = DeviceCapabilities::default();
        c.medium = Medium::Ip;
        c.max_transmission_unit = 1280;
        c
    }
}
struct Client {
    dev: Dev,
    iface: Interface,
    sockets: SocketSet<'static>,
    now: u64,
}
impl Client {
    fn new() -> Self {
        let mut dev = Dev {
            rx: VecDeque::new(),
            tx: VecDeque::new(),
        };
        let mut iface = Interface::new(
            Config::new(HardwareAddress::Ip),
            &mut dev,
            Instant::from_millis(0),
        );
        iface.update_ip_addrs(|a| {
            a.push(IpCidr::new(IpAddress::Ipv4(CLIENT), 32)).unwrap();
        });
        iface.routes_mut().add_default_ipv4_route(CLIENT).unwrap();
        Self {
            dev,
            iface,
            sockets: SocketSet::new(vec![]),
            now: 0,
        }
    }
    fn tcp(&mut self, port: u16) -> SocketHandle {
        let mut s = tcp::Socket::new(
            tcp::SocketBuffer::new(vec![0; 2048]),
            tcp::SocketBuffer::new(vec![0; 8192]),
        );
        s.set_nagle_enabled(false);
        s.connect(self.iface.context(), (SERVER, 53), port).unwrap();
        self.sockets.add(s)
    }
    fn pump(&mut self, server: &mut DnsTransport, rounds: usize) {
        for _ in 0..rounds {
            self.now += 10;
            let t = Instant::from_millis(self.now as i64);
            self.iface.poll_egress(t, &mut self.dev, &mut self.sockets);
            while let Some(p) = self.dev.tx.pop_front() {
                assert!(server.ingest(&p, self.now));
            }
            server.poll(self.now, 4);
            for p in server.take_packets(64) {
                check(&p);
                self.dev.rx.push_back(p);
            }
            for _ in 0..64 {
                self.iface
                    .poll_ingress_single(t, &mut self.dev, &mut self.sockets);
            }
        }
    }
    fn send(&mut self, h: SocketHandle, data: &[u8]) {
        assert_eq!(
            self.sockets
                .get_mut::<tcp::Socket>(h)
                .send_slice(data)
                .unwrap(),
            data.len()
        );
    }
    fn read(&mut self, h: SocketHandle) -> Vec<u8> {
        let s = self.sockets.get_mut::<tcp::Socket>(h);
        let mut out = vec![];
        while s.can_recv() {
            let mut b = [0; 2048];
            let n = s.recv_slice(&mut b).unwrap();
            out.extend_from_slice(&b[..n]);
        }
        out
    }
}
fn check(p: &[u8]) {
    let ip = Ipv4Packet::new_checked(p).unwrap();
    assert!(ip.verify_checksum());
    assert_eq!(ip.src_addr(), SERVER);
    assert_eq!(ip.dst_addr(), CLIENT);
    match ip.next_header() {
        IpProtocol::Tcp => {
            let t = TcpPacket::new_checked(ip.payload()).unwrap();
            assert_eq!(t.src_port(), 53);
            assert!(t.verify_checksum(&IpAddress::Ipv4(SERVER), &IpAddress::Ipv4(CLIENT)));
        }
        IpProtocol::Udp => {
            let u = UdpPacket::new_checked(ip.payload()).unwrap();
            assert_eq!(u.src_port(), 53);
            assert!(u.verify_checksum(&IpAddress::Ipv4(SERVER), &IpAddress::Ipv4(CLIENT)));
        }
        _ => panic!(),
    }
}
fn query(id: u16) -> Vec<u8> {
    let mut w = vec![0; 12];
    w[..2].copy_from_slice(&id.to_be_bytes());
    w[2] = 1;
    w
}
fn framed(w: &[u8]) -> Vec<u8> {
    let mut f = (w.len() as u16).to_be_bytes().to_vec();
    f.extend_from_slice(w);
    f
}
#[test]
fn udp_roundtrip_limits_and_reset() {
    let mut server = DnsTransport::new(SERVER, 1280, 7).unwrap();
    let mut c = Client::new();
    let mut u = udp::Socket::new(
        udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 4], vec![0; 8192]),
        udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 4], vec![0; 8192]),
    );
    u.bind((CLIENT, 41000)).unwrap();
    u.send_slice(&query(123), (SERVER, 53)).unwrap();
    let h = c.sockets.add(u);
    c.pump(&mut server, 5);
    let r = server.take_requests(1).pop().unwrap();
    assert_eq!(r.protocol, DnsProtocol::Udp);
    assert_eq!(r.wire, query(123));
    assert_eq!(
        server.respond(r.id, vec![0; 1253]),
        Err(DnsTransportError::ResponseTooLarge)
    );
    server.respond(r.id, query(123)).unwrap();
    assert_eq!(
        server.respond(r.id, query(123)),
        Err(DnsTransportError::UnknownRequest)
    );
    c.pump(&mut server, 5);
    let (w, m) = c.sockets.get_mut::<udp::Socket>(h).recv().unwrap();
    assert_eq!(w, query(123));
    assert_eq!(m.endpoint.port, 53);
    c.sockets
        .get_mut::<udp::Socket>(h)
        .send_slice(&query(124), (SERVER, 53))
        .unwrap();
    c.pump(&mut server, 5);
    let r = server.take_requests(1).pop().unwrap();
    server.reset();
    assert_eq!(
        server.respond(r.id, query(124)),
        Err(DnsTransportError::UnknownRequest)
    );
    assert!(server.take_packets(64).is_empty());
    assert!(!server.ingest(&[0; 40], c.now));
}
#[test]
fn tcp_concurrent_split_coalesced_partial_answer_and_fin() {
    let mut server = DnsTransport::new(SERVER, 1280, 9).unwrap();
    let mut c = Client::new();
    let a = c.tcp(42000);
    let b = c.tcp(42001);
    c.pump(&mut server, 20);
    assert_eq!(
        c.sockets.get::<tcp::Socket>(a).state(),
        tcp::State::Established
    );
    assert_eq!(
        c.sockets.get::<tcp::Socket>(b).state(),
        tcp::State::Established
    );
    let q = framed(&query(1));
    c.send(a, &q[..1]);
    c.pump(&mut server, 5);
    assert!(server.take_requests(10).is_empty());
    assert!(server
        .next_poll_ms(c.now)
        .is_some_and(|deadline| deadline > c.now));
    let mut rest = q[1..].to_vec();
    rest.extend(framed(&query(2)));
    c.send(a, &rest);
    c.send(b, &framed(&query(3)));
    c.pump(&mut server, 10);
    let requests = server.take_requests(10);
    assert_eq!(requests.len(), 3);
    for r in requests {
        assert_eq!(r.protocol, DnsProtocol::Tcp);
        if r.wire == query(1) {
            let mut answer = vec![0x5a; 20000];
            answer[..2].copy_from_slice(&[0, 1]);
            server.respond(r.id, answer).unwrap();
        } else {
            server.respond(r.id, r.wire).unwrap();
        }
    }
    let mut got = vec![];
    for _ in 0..100 {
        c.pump(&mut server, 3);
        got.extend(c.read(a));
    }
    let mut expected = vec![0x5a; 20000];
    expected[..2].copy_from_slice(&[0, 1]);
    let mut all = framed(&expected);
    all.extend(framed(&query(2)));
    assert_eq!(got, all);
    assert_eq!(c.read(b), framed(&query(3)));
    c.sockets.get_mut::<tcp::Socket>(a).close();
    c.sockets.get_mut::<tcp::Socket>(b).close();
    c.pump(&mut server, 40);
    assert!(!c.sockets.get::<tcp::Socket>(a).may_recv());
    assert!(!c.sockets.get::<tcp::Socket>(b).may_recv());
}
#[test]
fn expired_response_and_packet_filter() {
    let mut server = DnsTransport::new(SERVER, 1280, 9).unwrap();
    let mut c = Client::new();
    let a = c.tcp(43000);
    c.pump(&mut server, 10);
    c.send(a, &framed(&query(9)));
    c.pump(&mut server, 5);
    let r = server.take_requests(1).pop().unwrap();
    server.poll(c.now + 5001, 1);
    assert_eq!(
        server.respond(r.id, query(9)),
        Err(DnsTransportError::UnknownRequest)
    );
    server.reset();
    assert_eq!(server.next_poll_ms(c.now), None);
    let mut other = Client::new();
    let h = other.tcp(44000);
    other
        .iface
        .poll_egress(Instant::from_millis(0), &mut other.dev, &mut other.sockets);
    let mut p = other.dev.tx.pop_front().unwrap();
    assert!(server.is_dns_packet(&p));
    p[16..20].copy_from_slice(&[1, 2, 3, 4]);
    assert!(!server.ingest(&p, 0));
    let _ = h;
}

#[test]
fn bounded_input_and_reset_ids() {
    let mut server = DnsTransport::new(SERVER, 1280, 11).unwrap();
    let mut c = Client::new();
    let mut u = udp::Socket::new(
        udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 1], vec![0; 1280]),
        udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 1], vec![0; 1280]),
    );
    u.bind((CLIENT, 45000)).unwrap();
    u.send_slice(&query(55), (SERVER, 53)).unwrap();
    c.sockets.add(u);
    c.iface
        .poll_egress(Instant::from_millis(0), &mut c.dev, &mut c.sockets);
    let p = c.dev.tx.pop_front().unwrap();
    for _ in 0..MAX_PACKETS + 20 {
        assert!(server.ingest(&p, 0));
    }
    server.poll(0, 0);
    assert!(server.take_requests(128).is_empty());
    server.poll(0, 1);
    let old = server.take_requests(128);
    assert_eq!(old.len(), 1);
    server.poll(0, usize::MAX);
    assert_eq!(server.take_requests(128).len(), MAX_PACKETS - 1);
    server.reset();
    assert!(server.ingest(&p, 1));
    server.poll(1, 1);
    let fresh = server.take_requests(1).pop().unwrap();
    assert!(fresh.id > old[0].id);
    assert_eq!(
        server.respond(old[0].id, query(55)),
        Err(DnsTransportError::UnknownRequest)
    );
    server.respond(fresh.id, query(55)).unwrap();
    let mut wrong = p.clone();
    wrong[22..24].copy_from_slice(&54u16.to_be_bytes());
    assert!(!server.ingest(&wrong, 2));
    server.reset();
    assert!(server.ingest(&p, 10));
    server.poll(5010, 1);
    assert!(server.take_requests(1).is_empty());
    for _ in 0..3 {
        for _ in 0..MAX_PACKETS {
            assert!(server.ingest(&p, 5011));
        }
        server.poll(5011, MAX_PACKETS);
    }
    let full = server.take_requests(usize::MAX);
    assert_eq!(full.len(), MAX_REQUESTS);
    server.respond(full[0].id, query(55)).unwrap();
    server.poll(5011, 1);
    server.poll(10011, 1);
    assert!(server.take_packets(usize::MAX).is_empty());
}
