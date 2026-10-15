use hickory_proto::{
    op::{Message, Query, ResponseCode},
    rr::{Name, RecordType},
};
use netbird_core::sys as libc;
use netbird_core::{
    dns_resolver::DnsResolverSnapshot,
    dns_runtime::{DnsHandle, DnsRuntime},
    network_map::{CustomZone, DnsConfig, NameServer, NameServerGroup, SimpleRecord},
    socket_supply::SocketSupply,
};
use smoltcp::{
    iface::{Config, Interface, SocketHandle, SocketSet},
    phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken},
    socket::{tcp, udp},
    time::Instant,
    wire::{HardwareAddress, IpAddress, IpCidr},
};
use std::{
    collections::VecDeque,
    net::{Ipv4Addr, UdpSocket},
    sync::{mpsc, Arc},
    time::{Duration, Instant as Clock},
};
const SERVER: Ipv4Addr = Ipv4Addr::new(100, 100, 0, 53);
const CLIENT: Ipv4Addr = Ipv4Addr::new(100, 64, 0, 2);
mod host_link_stubs {
    use core::ffi::c_void;
    #[no_mangle]
    pub extern "C" fn OH_LOG_Print() -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn OH_LOG_IsLoggable() -> bool {
        false
    }
    #[no_mangle]
    pub extern "C" fn napi_module_register(_: *mut c_void) {}
    #[no_mangle]
    pub extern "C" fn napi_create_function(
        _: *mut c_void,
        _: *const u8,
        _: usize,
        _: *mut c_void,
        _: *mut c_void,
        _: *mut *mut c_void,
    ) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn napi_set_named_property(
        _: *mut c_void,
        _: *mut c_void,
        _: *const u8,
        _: *mut c_void,
    ) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn napi_get_cb_info(
        _: *mut c_void,
        _: *mut c_void,
        _: *mut usize,
        _: *mut *mut c_void,
        _: *mut c_void,
        _: *mut *mut c_void,
    ) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _: *mut c_void,
        _: *const u8,
        _: usize,
        _: *mut c_void,
    ) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn napi_get_value_string_utf8(
        _: *mut c_void,
        _: *mut c_void,
        _: *mut u8,
        _: usize,
        _: *mut usize,
    ) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn napi_get_value_int32(_: *mut c_void, _: *mut c_void, _: *mut i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(_: *mut c_void, _: *mut c_void, _: *mut bool) -> i32 {
        0
    }
}
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
    start: Clock,
    udp: SocketHandle,
    last_query: Option<Vec<u8>>,
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
        let mut sockets = SocketSet::new(vec![]);
        let mut u = udp::Socket::new(
            udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 128], vec![0; 128 * 1280]),
            udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 128], vec![0; 128 * 1280]),
        );
        u.bind((CLIENT, 41000)).unwrap();
        let udp = sockets.add(u);
        Self {
            dev,
            iface,
            sockets,
            start: Clock::now(),
            udp,
            last_query: None,
        }
    }
    fn tick(&mut self, h: &DnsHandle, g: u64) {
        let now = Instant::from_millis(self.start.elapsed().as_millis() as i64);
        self.iface
            .poll_egress(now, &mut self.dev, &mut self.sockets);
        while let Some(p) = self.dev.tx.pop_front() {
            assert!(h.try_ingest(&p, g));
        }
        for p in h.take_packets(128) {
            assert_eq!(p.generation, g);
            self.dev.rx.push_back(p.frame);
        }
        for _ in 0..128 {
            self.iface
                .poll_ingress_single(now, &mut self.dev, &mut self.sockets);
        }
        // Real poll drives worker wake delivery; no fixed fast sleep in product.
        let mut fd = libc::pollfd {
            fd: h.wake_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        unsafe {
            libc::poll(&mut fd, 1, 2);
        }
        h.drain_wake();
    }
    fn send(&mut self, id: u16, name: &str) {
        let wire = query(id, name);
        self.sockets
            .get_mut::<udp::Socket>(self.udp)
            .send_slice(&wire, (SERVER, 53))
            .unwrap();
        self.last_query = Some(wire);
    }
    fn receive(&mut self) -> Vec<Message> {
        let s = self.sockets.get_mut::<udp::Socket>(self.udp);
        let mut out = vec![];
        while s.can_recv() {
            let (w, _) = s.recv().unwrap();
            out.push(Message::from_vec(w).unwrap());
        }
        out
    }
    fn wait(&mut self, h: &DnsHandle, g: u64, id: u16) -> Message {
        self.wait_response(h, g, id, true)
    }
    fn wait_once(&mut self, h: &DnsHandle, g: u64, id: u16) -> Message {
        self.wait_response(h, g, id, false)
    }
    fn wait_response(&mut self, h: &DnsHandle, g: u64, id: u16, retransmit: bool) -> Message {
        let until = Clock::now() + Duration::from_secs(2);
        let mut retry = Clock::now() + Duration::from_millis(100);
        loop {
            self.tick(h, g);
            for m in self.receive() {
                if m.id() == id {
                    return m;
                }
            }
            // Overload and network tests retain normal UDP retry semantics.
            if retransmit && Clock::now() >= retry {
                if let Some(wire) = &self.last_query {
                    assert_eq!(u16::from_be_bytes([wire[0], wire[1]]), id);
                    self.sockets
                        .get_mut::<udp::Socket>(self.udp)
                        .send_slice(wire, (SERVER, 53))
                        .unwrap();
                }
                retry = Clock::now() + Duration::from_millis(100);
            }
            assert!(
                Clock::now() < until,
                "response deadline id={id}, stats={:?}",
                h.stats()
            );
        }
    }
    fn tcp(&mut self) -> SocketHandle {
        let mut s = tcp::Socket::new(
            tcp::SocketBuffer::new(vec![0; 8192]),
            tcp::SocketBuffer::new(vec![0; 65536]),
        );
        s.set_nagle_enabled(false);
        s.connect(self.iface.context(), (SERVER, 53), 42000)
            .unwrap();
        self.sockets.add(s)
    }
    fn tcp_query(&mut self, h: &DnsHandle, g: u64, id: u16, name: &str) -> Message {
        let sock = self.tcp();
        let until = Clock::now() + Duration::from_secs(2);
        while !self.sockets.get::<tcp::Socket>(sock).can_send() {
            self.tick(h, g);
            assert!(Clock::now() < until);
        }
        let w = query(id, name);
        let mut frame = (w.len() as u16).to_be_bytes().to_vec();
        frame.extend(w);
        self.sockets
            .get_mut::<tcp::Socket>(sock)
            .send_slice(&frame)
            .unwrap();
        let mut out = vec![];
        loop {
            self.tick(h, g);
            let s = self.sockets.get_mut::<tcp::Socket>(sock);
            while s.can_recv() {
                let mut b = [0; 8192];
                let n = s.recv_slice(&mut b).unwrap();
                out.extend_from_slice(&b[..n]);
            }
            if out.len() >= 2 {
                let n = u16::from_be_bytes([out[0], out[1]]) as usize;
                if out.len() >= n + 2 {
                    return Message::from_vec(&out[2..n + 2]).unwrap();
                }
            }
            assert!(Clock::now() < until, "TCP response timeout");
        }
    }
}
fn query(id: u16, n: &str) -> Vec<u8> {
    let mut q = Message::new();
    q.set_id(id)
        .set_recursion_desired(true)
        .add_query(Query::query(Name::from_ascii(n).unwrap(), RecordType::A));
    q.to_vec().unwrap()
}
fn config() -> DnsConfig {
    DnsConfig {
        service_enable: true,
        name_server_groups: vec![],
        custom_zones: vec![CustomZone {
            domain: "local.test".into(),
            records: vec![SimpleRecord {
                name: "host.local.test".into(),
                r#type: 1,
                class: "IN".into(),
                ttl: 60,
                r_data: "10.1.2.3".into(),
            }],
            search_domain_disabled: false,
            non_authoritative: false,
        }],
    }
}
fn snapshot(c: &DnsConfig) -> Arc<DnsResolverSnapshot> {
    Arc::new(DnsResolverSnapshot::new(Some(c), SERVER).unwrap())
}
fn group(servers: Vec<std::net::SocketAddr>) -> NameServerGroup {
    NameServerGroup {
        primary: false,
        domains: vec!["remote.test".into()],
        search_domains_enabled: false,
        name_servers: servers
            .into_iter()
            .map(|s| NameServer {
                ip: s.ip().to_string(),
                port: s.port(),
                ns_type: 1,
            })
            .collect(),
    }
}
fn runtime(c: &DnsConfig) -> DnsRuntime {
    DnsRuntime::start(SERVER, 1280, snapshot(c), Arc::new(SocketSupply::new()), 1).unwrap()
}
#[test]
fn local_udp_tcp_hot_update_remove_and_stop() {
    let mut c = config();
    let mut r = runtime(&c);
    let h = r.handle();
    let mut client = Client::new();
    client.send(1, "host.local.test");
    let m = client.wait_once(&h, 1, 1);
    assert_eq!(m.answers()[0].data().to_string(), "10.1.2.3");
    assert_eq!(
        client
            .tcp_query(&h, 1, 2, "host.local.test")
            .answers()
            .len(),
        1
    );
    c.custom_zones[0].records[0].r_data = "10.4.5.6".into();
    r.update(snapshot(&c));
    client.send(3, "host.local.test");
    assert_eq!(
        client.wait_once(&h, 1, 3).answers()[0].data().to_string(),
        "10.4.5.6"
    );
    c.custom_zones.clear();
    r.update(snapshot(&c));
    client.send(4, "host.local.test");
    assert_eq!(
        client.wait_once(&h, 1, 4).response_code(),
        ResponseCode::ServFail
    );
    let start = Clock::now();
    r.stop_and_join();
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(!h.stats().running);
    let accepted = h.stats().accepted;
    client.send(5, "host.local.test");
    client.tick(&h, 1);
    assert_eq!(h.stats().accepted, accepted);
    assert!(h.take_packets(128).is_empty());
}
#[test]
fn low_load_single_send_udp_never_drops_on_hot_update() {
    let mut c = config();
    let mut r = runtime(&c);
    let h = r.handle();
    let mut client = Client::new();
    for id in 1000..1128 {
        let address = format!("10.1.2.{}", id % 200 + 1);
        c.custom_zones[0].records[0].r_data = address.clone();
        r.update(snapshot(&c));
        client.send(id, "host.local.test");
        let answer = client.wait_once(&h, 1, id);
        assert_eq!(answer.answers()[0].data().to_string(), address);
    }
    assert_eq!(h.stats().accepted, 128);
    assert_eq!(h.stats().queries, 128);
    assert_eq!(h.stats().dropped_input, 0);
    r.stop_and_join();
}

#[test]
fn blackhole_does_not_block_local_or_tcp_and_stop_cancels() {
    let blackhole = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut c = config();
    c.name_server_groups
        .push(group(vec![blackhole.local_addr().unwrap()]));
    let mut r = runtime(&c);
    let h = r.handle();
    let mut client = Client::new();
    client.send(10, "x.remote.test");
    client.tick(&h, 1);
    let start = Clock::now();
    client.send(11, "host.local.test");
    assert_eq!(client.wait(&h, 1, 11).answers().len(), 1);
    assert_eq!(
        client
            .tcp_query(&h, 1, 12, "host.local.test")
            .answers()
            .len(),
        1
    );
    assert!(start.elapsed() < Duration::from_secs(1));
    let start = Clock::now();
    r.stop_and_join();
    assert!(start.elapsed() < Duration::from_secs(1));
}
#[test]
fn reset_cancels_late_response_and_new_generation_works() {
    reset_cancels_late_response(2);
}
#[test]
fn reset_cancels_late_response_with_reused_generation() {
    reset_cancels_late_response(1);
}
fn reset_cancels_late_response(generation: u64) {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut c = config();
    c.name_server_groups
        .push(group(vec![socket.local_addr().unwrap()]));
    let (seen_rx_tx, seen_rx) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let mock = std::thread::spawn(move || {
        let mut b = [0; 2048];
        let (n, p) = socket.recv_from(&mut b).unwrap();
        seen_rx_tx.send(()).unwrap();
        wait.recv_timeout(Duration::from_secs(2)).unwrap();
        b[2] |= 128;
        b[3] = 128;
        socket.send_to(&b[..n], p).unwrap();
    });
    let mut r = runtime(&c);
    let h = r.handle();
    let mut client = Client::new();
    client.send(20, "x.remote.test");
    client.tick(&h, 1);
    seen_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    r.reset_tun(generation);
    release.send(()).unwrap();
    mock.join().unwrap();
    client.send(21, "host.local.test");
    let m = client.wait_once(&h, generation, 21);
    assert_eq!(m.answers().len(), 1);
    for _ in 0..20 {
        client.tick(&h, generation);
        assert!(client.receive().iter().all(|m| m.id() != 20));
    }
    r.stop_and_join();
}
#[test]
fn groups_race_and_servers_retry_then_preserve_nxdomain() {
    let blackhole = UdpSocket::bind("127.0.0.1:0").unwrap();
    let first = UdpSocket::bind("127.0.0.1:0").unwrap();
    let second = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut c = config();
    c.name_server_groups
        .push(group(vec![blackhole.local_addr().unwrap()]));
    c.name_server_groups.push(group(vec![
        first.local_addr().unwrap(),
        second.local_addr().unwrap(),
    ]));
    let mocks: Vec<_> = [(first, 2u8), (second, 3u8)]
        .into_iter()
        .map(|(s, code)| {
            std::thread::spawn(move || {
                s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut b = [0; 2048];
                let (n, p) = s.recv_from(&mut b).unwrap();
                b[2] = 0x81;
                b[3] = 0x80 | code;
                s.send_to(&b[..n], p).unwrap();
            })
        })
        .collect();
    let mut r = runtime(&c);
    let h = r.handle();
    let mut client = Client::new();
    let start = Clock::now();
    client.send(30, "x.remote.test");
    assert_eq!(
        client.wait(&h, 1, 30).response_code(),
        ResponseCode::NXDomain
    );
    assert!(start.elapsed() < Duration::from_secs(1));
    for mock in mocks {
        mock.join().unwrap();
    }
    r.stop_and_join();
}
#[test]
fn overload_stays_bounded_and_reset_control_is_not_dropped() {
    let blackhole = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut c = config();
    c.name_server_groups
        .push(group(vec![blackhole.local_addr().unwrap()]));
    let mut r = runtime(&c);
    let h = r.handle();
    let mut client = Client::new();
    client.send(90, "x.remote.test");
    client.iface.poll_egress(
        Instant::from_millis(1),
        &mut client.dev,
        &mut client.sockets,
    );
    let frame = client.dev.tx.pop_front().unwrap();
    for _ in 0..10_000 {
        assert!(h.try_ingest(&frame, 1));
    }
    assert_eq!(h.stats().accepted + h.stats().dropped_input, 10_000);
    assert!(h.stats().dropped_input > 0);
    let start = Clock::now();
    h.reset_tun(2);
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(h.take_packets(128).is_empty());
    let accepted = h.stats().accepted;
    assert!(h.try_ingest(&frame, 1));
    assert_eq!(h.stats().accepted, accepted);
    client.send(91, "host.local.test");
    assert_eq!(client.wait(&h, 2, 91).answers().len(), 1);
    r.stop_and_join();
}
#[test]
fn external_cname_uses_explicit_physical_supply() {
    use std::os::fd::AsRawFd;
    let ip = Ipv4Addr::new(127, 88, 53, 41);
    let s = UdpSocket::bind((ip, 53)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let mock = std::thread::spawn(move || {
        let mut b = [0; 2048];
        let (n, p) = s.recv_from(&mut b).unwrap();
        assert_eq!(
            Message::from_vec(&b[..n]).unwrap().queries()[0]
                .name()
                .to_ascii(),
            "external.remote.test."
        );
        let mut reply = b[..n].to_vec();
        reply[2] = 0x81;
        reply[3] = 0x80;
        reply[7] = 1;
        reply.extend([0xc0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 192, 0, 2, 7]);
        s.send_to(&reply, p).unwrap();
    });
    let mut c = config();
    c.custom_zones[0].records[0].r#type = 5;
    c.custom_zones[0].records[0].r_data = "external.remote.test.".into();
    // With no matching group or primary, use the supplied physical DNS list.
    let supply = Arc::new(SocketSupply::new());
    supply.set_dns_servers(vec![ip]);
    let mut r = DnsRuntime::start(SERVER, 1280, snapshot(&c), supply.clone(), 1).unwrap();
    let h = r.handle();
    let mut client = Client::new();
    client.send(80, "host.local.test");
    let until = Clock::now() + Duration::from_secs(2);
    loop {
        client.tick(&h, 1);
        if let Some((id, purpose)) = supply.poll_request() {
            assert_eq!(purpose, netbird_core::socket_supply::Purpose::Dns);
            let fd = UdpSocket::bind("127.0.0.1:0").unwrap();
            assert_eq!(
                supply.reply(id, fd.as_raw_fd(), "protected"),
                "{\"ok\":true}"
            );
            break;
        }
        assert!(
            Clock::now() < until,
            "physical continuation must request a protected socket"
        );
    }
    let reply = client.wait(&h, 1, 80);
    assert_eq!(reply.answers().len(), 2);
    assert_eq!(reply.answers()[0].record_type(), RecordType::CNAME);
    assert_eq!(reply.answers()[1].data().to_string(), "192.0.2.7");
    mock.join().unwrap();
    r.stop_and_join();
}
#[test]
fn real_tcp_upstream_full_answer_and_udp_fitting() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut c = config();
    c.name_server_groups
        .push(group(vec![listener.local_addr().unwrap()]));
    let mock = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut len = [0; 2];
        stream.read_exact(&mut len).unwrap();
        let mut q = vec![0; u16::from_be_bytes(len) as usize];
        stream.read_exact(&mut q).unwrap();
        let mut reply = q;
        reply[2] = 0x81;
        reply[3] = 0x80;
        reply[6..8].copy_from_slice(&100u16.to_be_bytes());
        for n in 0..100u8 {
            reply.extend([0xc0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 10, 1, 2, n]);
        }
        stream
            .write_all(&(reply.len() as u16).to_be_bytes())
            .unwrap();
        stream.write_all(&reply).unwrap();
    });
    let mut r = runtime(&c);
    let h = r.handle();
    let mut client = Client::new();
    let reply = client.tcp_query(&h, 1, 50, "x.remote.test");
    assert_eq!(reply.answers().len(), 100);
    mock.join().unwrap();
    r.stop_and_join();
    let mut c = config();
    c.custom_zones[0].records = (0..100)
        .map(|n| SimpleRecord {
            name: "host.local.test".into(),
            r#type: 1,
            class: "IN".into(),
            ttl: 60,
            r_data: format!("10.1.2.{n}"),
        })
        .collect();
    let mut r = runtime(&c);
    let h = r.handle();
    let mut client = Client::new();
    client.send(51, "host.local.test");
    let reply = client.wait(&h, 1, 51);
    assert!(reply.truncated());
    assert!(reply.to_vec().unwrap().len() <= 512);
    assert_eq!(
        client
            .tcp_query(&h, 1, 52, "host.local.test")
            .answers()
            .len(),
        100
    );
    r.stop_and_join();
}
#[test]
fn classified_answer_preserves_opaque_wire() {
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut c = config();
    c.name_server_groups
        .push(group(vec![s.local_addr().unwrap()]));
    let (tx, rx) = mpsc::channel();
    let mock = std::thread::spawn(move || {
        s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut b = [0; 2048];
        let (n, p) = s.recv_from(&mut b).unwrap();
        let mut reply = b[..n].to_vec();
        reply[2] = 0x81;
        reply[3] = 0x80;
        reply[7] = 1;
        // Uncompressed owner: parsing/reserializing would change bytes.
        reply.extend_from_slice(&b[12..n - 4]);
        reply.extend([0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 192, 0, 2, 99]);
        tx.send(reply.clone()).unwrap();
        s.send_to(&reply, p).unwrap();
    });
    let mut r = runtime(&c);
    let h = r.handle();
    let mut client = Client::new();
    client.send(60, "x.remote.test");
    let until = Clock::now() + Duration::from_secs(2);
    let actual = loop {
        client.tick(&h, 1);
        let u = client.sockets.get_mut::<udp::Socket>(client.udp);
        if u.can_recv() {
            break u.recv().unwrap().0.to_vec();
        }
        assert!(Clock::now() < until);
    };
    assert_eq!(actual, rx.recv_timeout(Duration::from_secs(1)).unwrap());
    mock.join().unwrap();
    r.stop_and_join();
}
#[test]
fn private_failure_never_requests_physical_supply_and_custom_zone_wins() {
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut c = config();
    c.name_server_groups
        .push(group(vec![s.local_addr().unwrap()]));
    let mock = std::thread::spawn(move || {
        s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut b = [0; 2048];
        let (n, p) = s.recv_from(&mut b).unwrap();
        b[2] = 0x81;
        b[3] = 0x85;
        s.send_to(&b[..n], p).unwrap();
    });
    let supply = Arc::new(SocketSupply::new());
    supply.set_dns_servers(vec![Ipv4Addr::new(192, 0, 2, 1)]);
    let mut r = DnsRuntime::start(SERVER, 1280, snapshot(&c), supply.clone(), 1).unwrap();
    let h = r.handle();
    let mut client = Client::new();
    client.send(70, "x.remote.test");
    assert_eq!(
        client.wait(&h, 1, 70).response_code(),
        ResponseCode::ServFail
    );
    assert!(supply.poll_request().is_none());
    c.name_server_groups[0].primary = true;
    r.update(snapshot(&c));
    client.send(71, "host.local.test");
    assert_eq!(client.wait(&h, 1, 71).answers().len(), 1);
    mock.join().unwrap();
    r.stop_and_join();
}
#[test]
fn physical_dns_absence_and_protection_failure_return_servfail() {
    let supply = Arc::new(SocketSupply::new());
    let mut r = DnsRuntime::start(SERVER, 1280, snapshot(&config()), supply.clone(), 1).unwrap();
    let h = r.handle();
    let mut client = Client::new();
    client.send(100, "unavailable.example");
    assert_eq!(
        client.wait(&h, 1, 100).response_code(),
        ResponseCode::ServFail
    );
    assert!(supply.poll_request().is_none());

    // The next query observes the physical list changed after runtime start.
    let server = UdpSocket::bind((Ipv4Addr::new(127, 88, 53, 42), 53)).unwrap();
    server.set_nonblocking(true).unwrap();
    supply.set_dns_servers(vec![Ipv4Addr::new(127, 88, 53, 42)]);
    client.send(101, "unavailable.example");
    let until = Clock::now() + Duration::from_secs(2);
    loop {
        client.tick(&h, 1);
        if let Some((id, purpose)) = supply.poll_request() {
            assert_eq!(purpose, netbird_core::socket_supply::Purpose::Dns);
            assert_eq!(supply.reply(id, -1, "protect-failed"), "{\"ok\":false}");
            break;
        }
        assert!(Clock::now() < until, "protected socket request deadline");
    }
    assert_eq!(
        client.wait(&h, 1, 101).response_code(),
        ResponseCode::ServFail
    );
    let mut buffer = [0; 512];
    assert_eq!(
        server.recv_from(&mut buffer).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(supply.poll_request().is_none());
    r.stop_and_join();
}
fn cname_config() -> DnsConfig {
    let mut c = config();
    c.custom_zones[0].records[0].r#type = 5;
    c.custom_zones[0].records[0].r_data = "external.remote.test.".into();
    c
}
fn answer_wire(query: &[u8], code: ResponseCode) -> Vec<u8> {
    let mut reply = Message::from_vec(query).unwrap();
    reply.set_message_type(hickory_proto::op::MessageType::Response)
        .set_response_code(code);
    if code == ResponseCode::NoError {
        reply.add_answer(hickory_proto::rr::Record::from_rdata(
            reply.queries()[0].name().clone(), 60,
            hickory_proto::rr::RData::A(hickory_proto::rr::rdata::A(Ipv4Addr::new(192, 0, 2, 7)))));
    }
    reply.to_vec().unwrap()
}
#[test]
fn external_cname_target_uses_private_group_and_primary_udp_tcp() {
    use std::{io::{Read, Write}, net::TcpListener};
    for primary in [false, true] {
        for tcp in [false, true] {
            let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
            let listener = TcpListener::bind(udp.local_addr().unwrap()).unwrap();
            let mut c = cname_config();
            let mut target_group = group(vec![udp.local_addr().unwrap()]);
            if primary {
                target_group.primary = true;
                target_group.domains.clear();
            }
            c.name_server_groups.push(target_group);
            let mock = std::thread::spawn(move || {
                let (wire, peer) = if tcp {
                    listener.set_nonblocking(true).unwrap();
                    let deadline = Clock::now() + Duration::from_secs(2);
                    let mut stream = loop {
                        match listener.accept() {
                            Ok((stream, _)) => break stream,
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                assert!(Clock::now() < deadline);
                                std::thread::yield_now();
                            }
                            Err(e) => panic!("{e}"),
                        }
                    };
                    stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                    let mut len = [0; 2];
                    stream.read_exact(&mut len).unwrap();
                    let mut wire = vec![0; u16::from_be_bytes(len) as usize];
                    stream.read_exact(&mut wire).unwrap();
                    (wire, Some(stream))
                } else {
                    udp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                    let mut wire = [0; 2048];
                    let (n, addr) = udp.recv_from(&mut wire).unwrap();
                    let reply = answer_wire(&wire[..n], ResponseCode::NoError);
                    udp.send_to(&reply, addr).unwrap();
                    (wire[..n].to_vec(), None)
                };
                assert_eq!(Message::from_vec(&wire).unwrap().queries()[0].name().to_ascii(), "external.remote.test.");
                if let Some(mut stream) = peer {
                    let reply = answer_wire(&wire, ResponseCode::NoError);
                    stream.write_all(&(reply.len() as u16).to_be_bytes()).unwrap();
                    stream.write_all(&reply).unwrap();
                }
            });
            let supply = Arc::new(SocketSupply::new());
            supply.set_dns_servers(vec![Ipv4Addr::new(127, 88, 53, 99)]);
            let mut r = DnsRuntime::start(SERVER, 1280, snapshot(&c), supply.clone(), 1).unwrap();
            let h = r.handle();
            let mut client = Client::new();
            let reply = if tcp { client.tcp_query(&h, 1, 200, "host.local.test") } else {
                client.send(200, "host.local.test"); client.wait_once(&h, 1, 200)
            };
            assert_eq!(reply.id(), 200);
            assert_eq!(reply.queries()[0].name().to_ascii(), "host.local.test.");
            assert!(!reply.authoritative());
            assert_eq!(reply.answers().len(), 2);
            assert_eq!(reply.answers()[1].data().to_string(), "192.0.2.7");
            assert!(supply.poll_request().is_none());
            mock.join().unwrap();
            r.stop_and_join();
        }
    }
}
#[test]
fn external_cname_private_nxdomain_and_failure_never_use_physical() {
    for code in [ResponseCode::NXDomain, ResponseCode::Refused] {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut c = cname_config();
        c.name_server_groups.push(group(vec![socket.local_addr().unwrap()]));
        let mock = std::thread::spawn(move || {
            socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut buffer = [0; 2048];
            let (n, peer) = socket.recv_from(&mut buffer).unwrap();
            assert_eq!(Message::from_vec(&buffer[..n]).unwrap().queries()[0].name().to_ascii(), "external.remote.test.");
            socket.send_to(&answer_wire(&buffer[..n], code), peer).unwrap();
        });
        let supply = Arc::new(SocketSupply::new());
        supply.set_dns_servers(vec![Ipv4Addr::new(127, 88, 53, 99)]);
        let mut r = DnsRuntime::start(SERVER, 1280, snapshot(&c), supply.clone(), 1).unwrap();
        let h = r.handle();
        let mut client = Client::new();
        client.send(201, "host.local.test");
        let reply = client.wait_once(&h, 1, 201);
        assert_eq!(reply.response_code(), if code == ResponseCode::Refused { ResponseCode::ServFail } else { code });
        assert!(supply.poll_request().is_none());
        mock.join().unwrap();
        r.stop_and_join();
    }
}
#[test]
fn external_cname_nonauthoritative_nxdomain_falls_back_for_original_owner() {
    let target = UdpSocket::bind("127.0.0.1:0").unwrap();
    let owner = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut c = cname_config();
    c.custom_zones[0].non_authoritative = true;
    // The resolver follows official single-zone authoritative semantics.
    let mut extra = config().custom_zones.remove(0);
    extra.domain = "other.test".into();
    extra.records[0].name = "host.other.test".into();
    c.custom_zones.push(extra);
    c.name_server_groups.push(group(vec![target.local_addr().unwrap()]));
    let mut owner_group = group(vec![owner.local_addr().unwrap()]);
    owner_group.domains = vec!["local.test".into()];
    c.name_server_groups.push(owner_group);
    let mocks: Vec<_> = [(target, "external.remote.test.", ResponseCode::NXDomain), (owner, "host.local.test.", ResponseCode::NoError)]
        .into_iter().map(|(socket, name, code)| std::thread::spawn(move || {
            socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut buffer = [0; 2048];
            let (n, peer) = socket.recv_from(&mut buffer).unwrap();
            let query = Message::from_vec(&buffer[..n]).unwrap();
            assert_eq!(query.id(), 202);
            assert_eq!(query.queries()[0].name().to_ascii(), name);
            socket.send_to(&answer_wire(&buffer[..n], code), peer).unwrap();
        })).collect();
    let supply = Arc::new(SocketSupply::new());
    supply.set_dns_servers(vec![Ipv4Addr::new(127, 88, 53, 99)]);
    let mut r = DnsRuntime::start(SERVER, 1280, snapshot(&c), supply.clone(), 1).unwrap();
    let h = r.handle();
    let mut client = Client::new();
    client.send(202, "host.local.test");
    let reply = client.wait_once(&h, 1, 202);
    assert_eq!(reply.response_code(), ResponseCode::NoError);
    assert_eq!(reply.answers().len(), 1);
    assert_eq!(reply.answers()[0].name().to_ascii(), "host.local.test.");
    assert!(supply.poll_request().is_none());
    for mock in mocks { mock.join().unwrap(); }
    r.stop_and_join();
}
#[test]
fn external_cname_chains_preserve_aliases_and_fail_bounded_on_cycles_or_ninth_hop() {
    for (limit, cycle) in [(2usize, false), (7, false), (8, false), (2, true)] {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut c = cname_config();
        c.name_server_groups.push(group(vec![socket.local_addr().unwrap()]));
        let mock = std::thread::spawn(move || {
            socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            for index in 0..limit {
                let mut buffer = [0; 2048];
                let (n, peer) = socket.recv_from(&mut buffer).unwrap();
                let mut reply = Message::from_vec(&buffer[..n]).unwrap();
                let name = reply.queries()[0].name().clone();
                reply.set_message_type(hickory_proto::op::MessageType::Response);
                if index + 1 == limit && !cycle && limit < 8 {
                    socket.send_to(&answer_wire(&buffer[..n], ResponseCode::NoError), peer).unwrap();
                } else {
                    let next = if cycle && index + 1 == limit { "external.remote.test".to_string() }
                        else { format!("hop{index}.remote.test") };
                    reply.add_answer(hickory_proto::rr::Record::from_rdata(name, 60,
                        hickory_proto::rr::RData::CNAME(hickory_proto::rr::rdata::CNAME(Name::from_ascii(next).unwrap()))));
                    socket.send_to(&reply.to_vec().unwrap(), peer).unwrap();
                }
            }
            socket.set_nonblocking(true).unwrap();
            let mut buffer = [0; 2048];
            assert_eq!(socket.recv_from(&mut buffer).unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
        });
        let supply = Arc::new(SocketSupply::new());
        supply.set_dns_servers(vec![Ipv4Addr::new(127, 88, 53, 99)]);
        let mut r = DnsRuntime::start(SERVER, 1280, snapshot(&c), supply.clone(), 1).unwrap();
        let h = r.handle();
        let mut client = Client::new();
        let start = Clock::now();
        client.send(203, "host.local.test");
        let reply = client.wait_once(&h, 1, 203);
        assert!(start.elapsed() < Duration::from_secs(1));
        if cycle || limit == 8 {
            assert_eq!(reply.response_code(), ResponseCode::ServFail);
        } else {
            assert_eq!(reply.response_code(), ResponseCode::NoError);
            assert_eq!(reply.answers().len(), limit + 1);
            assert_eq!(reply.answers().last().unwrap().name().to_ascii(), format!("hop{}.remote.test.", limit - 2));
            assert_eq!(reply.answers().iter().filter(|r| r.record_type() == RecordType::CNAME).count(), limit);
        }
        assert!(supply.poll_request().is_none());
        mock.join().unwrap();
        r.stop_and_join();
    }
}
#[test]
fn local_cname_eight_hops_succeed_but_longer_chains_and_cycles_fail() {
    for (length, cycle) in [(8usize, false), (9, false), (2, true)] {
        let mut c = config();
        c.custom_zones[0].records.clear();
        for n in 0..length {
            let name = if n == 0 { "host.local.test".to_string() } else { format!("hop{n}.local.test") };
            let target = if cycle && n + 1 == length { "host.local.test.".to_string() } else { format!("hop{}.local.test.", n + 1) };
            c.custom_zones[0].records.push(SimpleRecord {
                name, r#type: 5, class: "IN".into(), ttl: 60, r_data: target,
            });
        }
        if !cycle {
            c.custom_zones[0].records.push(SimpleRecord {
                name: format!("hop{length}.local.test"), r#type: 1,
                class: "IN".into(), ttl: 60, r_data: "192.0.2.7".into(),
            });
        }
        let mut r = runtime(&c);
        let h = r.handle();
        let mut client = Client::new();
        client.send(204, "host.local.test");
        let reply = client.wait_once(&h, 1, 204);
        assert_eq!(reply.response_code(), if length == 8 { ResponseCode::NoError } else { ResponseCode::ServFail });
        if length == 8 { assert_eq!(reply.answers().len(), 9); }
        r.stop_and_join();
    }
    let mut c = cname_config();
    c.name_server_groups.push(group(vec![std::net::SocketAddr::from((SERVER, 53))]));
    let supply = Arc::new(SocketSupply::new());
    supply.set_dns_servers(vec![Ipv4Addr::new(127, 88, 53, 99)]);
    let mut r = DnsRuntime::start(SERVER, 1280, snapshot(&c), supply.clone(), 1).unwrap();
    let h = r.handle();
    let mut client = Client::new();
    client.send(205, "host.local.test");
    assert_eq!(client.wait_once(&h, 1, 205).response_code(), ResponseCode::ServFail);
    assert!(supply.poll_request().is_none());
    r.stop_and_join();
}
#[test]
fn wake_poll_drain_and_drop_lifetime() {
    let r = runtime(&config());
    let h = r.handle();
    let mut c = Client::new();
    c.send(40, "host.local.test");
    // Emit request without draining output or wake notifications.
    let now = Instant::from_millis(1);
    c.iface.poll_egress(now, &mut c.dev, &mut c.sockets);
    while let Some(p) = c.dev.tx.pop_front() {
        assert!(h.try_ingest(&p, 1));
    }
    let mut fd = libc::pollfd {
        fd: h.wake_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    assert_eq!(unsafe { libc::poll(&mut fd, 1, 1000) }, 1);
    let start = Clock::now();
    h.drain_wake();
    assert!(start.elapsed() < Duration::from_millis(100));
    assert_eq!(
        unsafe { libc::poll(&mut fd, 1, 0) },
        1,
        "queued output must rearm wake"
    );
    assert!(!h.take_packets(128).is_empty());
    h.drain_wake();
    assert_eq!(unsafe { libc::poll(&mut fd, 1, 0) }, 0);
    drop(r);
    assert!(!h.stats().running);
    assert!(unsafe { libc::fcntl(h.wake_fd(), libc::F_GETFD) } >= 0);
}
