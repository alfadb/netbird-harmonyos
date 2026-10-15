// Reuse the real smoltcp client and DNS fixtures rather than duplicating a TCP stack.
include!("dns_runtime.rs");

mod wg_seam {
    use super::*;
    use base64::Engine as _;
    use netbird_core::{
        connector::{WgPeerApplier, WgPeerEntry},
        tun::TunFd,
        wg_device::{TunTeeCounts, WgDevice, WgDeviceConfig, WgDeviceFeed, WgPeerSpec},
    };
    use std::{
        os::{fd::AsRawFd, unix::net::UnixDatagram},
        sync::atomic::{AtomicBool, Ordering},
    };

    fn cfg(byte: u8) -> WgDeviceConfig {
        WgDeviceConfig::new(base64::engine::general_purpose::STANDARD.encode([byte; 32]))
    }
    fn key(byte: u8) -> String {
        base64::engine::general_purpose::STANDARD.encode(
            boringtun::ffi::x25519_public_key(boringtun::ffi::x25519_key { key: [byte; 32] }).key,
        )
    }
    fn pair() -> (UnixDatagram, UnixDatagram) {
        let (a, b) = UnixDatagram::pair().unwrap();
        a.set_nonblocking(true).unwrap();
        b.set_nonblocking(true).unwrap();
        (a, b)
    }
    fn drain(end: &UnixDatagram) -> Vec<Vec<u8>> {
        let mut out = vec![];
        loop {
            let mut b = [0; 2048];
            match end.recv(&mut b) {
                Ok(n) => out.push(b[..n].to_vec()),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("TUN recv: {e}"),
            }
        }
        out
    }
    struct Link {
        feed: Arc<WgDeviceFeed>,
        _wg: UdpSocket,
        _tun: UnixDatagram,
        os: UnixDatagram,
        peer: WgDevice,
        _peer_wg: UdpSocket,
        _peer_tun: UnixDatagram,
        peer_os: UnixDatagram,
    }
    impl Link {
        fn new(handle: Arc<DnsHandle>) -> Self {
            let wg = UdpSocket::bind("127.0.0.1:0").unwrap();
            let peer_wg = UdpSocket::bind("127.0.0.1:0").unwrap();
            wg.set_nonblocking(true).unwrap();
            peer_wg.set_nonblocking(true).unwrap();
            let (tun, os) = pair();
            let (peer_tun, peer_os) = pair();
            let feed = Arc::new(WgDeviceFeed::new(cfg(0xa5)));
            // Frozen API must buffer before there is a device.
            feed.set_dns_handle(Some(handle));
            feed.apply_peers(&[WgPeerEntry {
                pub_key_b64: key(0xb6),
                allowed_ips: vec![netbird_core::config::Route {
                    addr: [0, 0, 0, 0],
                    prefix_len: 0,
                }],
            }])
            .unwrap();
            feed.feed_wg_socket(wg.as_raw_fd()).unwrap();
            feed.feed_tun(tun.as_raw_fd()).unwrap();
            let mut peer = WgDevice::adopt(
                cfg(0xb6),
                peer_wg.as_raw_fd(),
                TunFd::dup_from_raw(peer_tun.as_raw_fd()).unwrap(),
            )
            .unwrap();
            peer.set_peers(&[WgPeerSpec::new(key(0xa5), vec![([0, 0, 0, 0], 0)])])
                .unwrap();
            peer.set_endpoint(
                &key(0xa5),
                [127, 0, 0, 1],
                wg.local_addr().unwrap().port(),
                libc::mono_ms(),
            )
            .unwrap();
            feed.apply_endpoint(
                &key(0xb6),
                [127, 0, 0, 1],
                peer_wg.local_addr().unwrap().port(),
            )
            .unwrap();
            let mut link = Self {
                feed,
                _wg: wg,
                _tun: tun,
                os,
                peer,
                _peer_wg: peer_wg,
                _peer_tun: peer_tun,
                peer_os,
            };
            let until = Clock::now() + Duration::from_secs(2);
            while !link.feed.tunnel_ready() || !link.peer.tunnel_ready() {
                link.step();
                assert!(Clock::now() < until, "WG handshake deadline");
            }
            link
        }
        fn step(&mut self) {
            let now = libc::mono_ms();
            self.feed.with_device(|d| {
                d.service_tun(now);
                d.service_udp(now);
                d.service_dns_tx();
                d.tick(now);
            });
            self.peer.service_tun(now);
            self.peer.service_udp(now);
            self.peer.tick(now);
        }
        fn client_step(&mut self, client: &mut Client) {
            let now = Instant::from_millis(client.start.elapsed().as_millis() as i64);
            client
                .iface
                .poll_egress(now, &mut client.dev, &mut client.sockets);
            while let Some(p) = client.dev.tx.pop_front() {
                self.os.send(&p).unwrap();
            }
            self.step();
            client.dev.rx.extend(drain(&self.os));
            for _ in 0..128 {
                client
                    .iface
                    .poll_ingress_single(now, &mut client.dev, &mut client.sockets);
            }
        }
        fn wait_udp(&mut self, client: &mut Client, id: u16) -> Message {
            let until = Clock::now() + Duration::from_secs(2);
            loop {
                self.client_step(client);
                if let Some(m) = client.receive().into_iter().find(|m| m.id() == id) {
                    return m;
                }
                assert!(Clock::now() < until, "WG DNS UDP deadline");
                WgDeviceFeed::pump_wait(&self.feed.pump_wait_plan(libc::mono_ms(), false));
            }
        }
        fn blocked_dns(&mut self, h: &Arc<DnsHandle>, id: u16) {
            // Fill the actual TUN's OS-facing queue, not a mocked writer.
            let filler = [0xa7; 1280];
            loop {
                match self._tun.send(&filler) {
                    Ok(_) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) => panic!("fill TUN: {e}"),
                }
            }
            h.drain_wake();
            let mut client = Client::new();
            client.send(id, "host.local.test");
            client.iface.poll_egress(
                Instant::from_millis(0),
                &mut client.dev,
                &mut client.sockets,
            );
            while let Some(frame) = client.dev.tx.pop_front() {
                self.os.send(&frame).unwrap();
            }
            self.feed.with_device(|d| d.service_tun(libc::mono_ms()));
            let mut plan = self.feed.pump_wait_plan(libc::mono_ms(), false);
            // Isolate the production DNS wake source from WG handshake traffic
            // and the intentionally full TUN; no reset wake remains here.
            plan.tun_fd = None;
            plan.wg_fd = None;
            plan.timeout_ms = 2000;
            assert!(WgDeviceFeed::pump_wait(&plan) > 0, "worker completion wake");
            self.feed.with_device(|d| {
                assert_eq!(d.service_dns_tx(), 0);
                assert_eq!(d.dns_pending_tx(), 1);
            });
            let plan = self.feed.pump_wait_plan(libc::mono_ms(), false);
            assert!(plan.tun_writable);
            assert!(
                plan.dns_handle.is_none(),
                "blocked output must wait for capacity"
            );
        }
        fn ordinary(&mut self, marker: u8) {
            let mut frame = vec![0u8; 36];
            frame[0] = 0x45;
            frame[2..4].copy_from_slice(&36u16.to_be_bytes());
            frame[8] = 64;
            frame[9] = 17;
            frame[12..16].copy_from_slice(&CLIENT.octets());
            frame[16..20].copy_from_slice(&[10, 78, 0, 2]);
            frame[22..24].copy_from_slice(&53u16.to_be_bytes());
            frame[35] = marker;
            self.os.send(&frame).unwrap();
            self.peer_os.send(&frame).unwrap();
            let until = Clock::now() + Duration::from_secs(1);
            let (mut a, mut b) = (false, false);
            loop {
                self.step();
                a |= drain(&self.os).iter().any(|p| p == &frame);
                b |= drain(&self.peer_os).iter().any(|p| p == &frame);
                if a && b {
                    break;
                }
                assert!(Clock::now() < until, "bidirectional encrypted WG deadline");
            }
        }
    }

    #[test]
    fn wg_dns_real_tun_udp_tcp_and_blackhole_keeps_encrypted_data_flowing() {
        let blackhole = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut c = config();
        c.name_server_groups
            .push(group(vec![blackhole.local_addr().unwrap()]));
        let mut runtime = runtime(&c);
        let h = runtime.handle();
        let mut link = Link::new(h.clone());
        let tee = Arc::new(TunTeeCounts::new());
        link.feed.with_device(|d| d.attach_tun_tee(tee.clone()));
        let mut client = Client::new();
        client.send(101, "host.local.test");
        assert_eq!(
            link.wait_udp(&mut client, 101).answers()[0]
                .data()
                .to_string(),
            "10.1.2.3"
        );
        let sock = client.tcp();
        let until = Clock::now() + Duration::from_secs(3);
        while !client.sockets.get::<tcp::Socket>(sock).can_send() {
            link.client_step(&mut client);
            assert!(Clock::now() < until);
        }
        let wire = query(102, "host.local.test");
        let mut framed = (wire.len() as u16).to_be_bytes().to_vec();
        framed.extend(wire);
        client
            .sockets
            .get_mut::<tcp::Socket>(sock)
            .send_slice(&framed)
            .unwrap();
        let mut answer = vec![];
        loop {
            link.client_step(&mut client);
            let s = client.sockets.get_mut::<tcp::Socket>(sock);
            while s.can_recv() {
                let mut b = [0; 8192];
                let n = s.recv_slice(&mut b).unwrap();
                answer.extend_from_slice(&b[..n]);
            }
            if answer.len() >= 2
                && answer.len() >= 2 + u16::from_be_bytes([answer[0], answer[1]]) as usize
            {
                break;
            }
            assert!(Clock::now() < until, "WG TCP DNS response deadline");
            WgDeviceFeed::pump_wait(&link.feed.pump_wait_plan(libc::mono_ms(), false));
        }
        assert_eq!(Message::from_vec(&answer[2..]).unwrap().answers().len(), 1);
        // Submit a real upstream query and prove the socket actually received it.
        client.send(103, "blocked.remote.test");
        link.client_step(&mut client);
        blackhole
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut buf = [0; 2048];
        blackhole.recv_from(&mut buf).unwrap();
        for i in 0..20 {
            link.ordinary(i);
        }
        assert!(h.stats().queries >= 3);
        assert!(link.feed.with_device(|d| d.stats().dns_consumed).unwrap() >= 4);
        assert!(tee.totals().from_os_frames >= 24);
        link.feed.set_dns_handle(None);
        runtime.stop_and_join();
    }

    #[test]
    fn wg_dns_generation_replace_release_clear_and_wake_poll() {
        let mut runtime = runtime(&config());
        let h = runtime.handle();
        let mut link = Link::new(h.clone());
        let mut client = Client::new();
        let socket = link.feed.socket_fd();
        let sessions = link
            .feed
            .with_device(|d| d.stats().handshake_initiations)
            .unwrap();
        h.drain_wake();
        client.send(201, "host.local.test");
        let now = Instant::from_millis(0);
        client
            .iface
            .poll_egress(now, &mut client.dev, &mut client.sockets);
        let request = client.dev.tx.pop_front().unwrap();
        link.os.send(&request).unwrap();
        link.feed.with_device(|d| d.service_tun(libc::mono_ms()));
        // Completion itself must wake production poll, with other fds excluded.
        let mut plan = link.feed.pump_wait_plan(libc::mono_ms(), false);
        plan.timeout_ms = 2000;
        plan.tun_fd = None;
        plan.wg_fd = None;
        assert!(plan.dns_handle.is_some());
        assert!(WgDeviceFeed::pump_wait(&plan) > 0);
        let old_gen = link.feed.with_device(|d| d.dns_generation()).unwrap();
        let (tun, os) = pair();
        link.feed.feed_tun(tun.as_raw_fd()).unwrap();
        assert!(link.feed.with_device(|d| d.dns_generation()).unwrap() > old_gen);
        assert_eq!(link.feed.socket_fd(), socket);
        assert_eq!(
            link.feed
                .with_device(|d| d.stats().handshake_initiations)
                .unwrap(),
            sessions
        );
        link._tun = tun;
        link.os = os;
        // Old output, queued or in flight, cannot reach the replacement fd.
        for _ in 0..10 {
            link.step();
        }
        assert!(drain(&link.os).is_empty());
        client.send(202, "host.local.test");
        assert_eq!(link.wait_udp(&mut client, 202).id(), 202);
        link.ordinary(99);
        let gen = link.feed.with_device(|d| d.dns_generation()).unwrap();
        link.feed.release_tun().unwrap();
        assert!(link.feed.with_device(|d| d.dns_generation()).unwrap() > gen);
        assert!(link.feed.with_device(|d| d.tun_fd()).unwrap().is_none());
        link.feed.with_device(|d| d.service_dns_tx());
        assert!(drain(&link.os).is_empty());
        link.feed.clear();
        assert!(!link.feed.device_up());
        assert!(link
            .feed
            .pump_wait_plan(libc::mono_ms(), false)
            .dns_handle
            .is_none());
        // Held wait plans keep their handle/fd valid after detach and clear.
        assert!(plan.dns_handle.unwrap().wake_fd() >= 0);
        runtime.stop_and_join();
    }

    #[test]
    fn wg_dns_backpressure_is_bounded_and_pending_frames_die_with_the_tun() {
        let mut runtime = runtime(&config());
        let h = runtime.handle();
        let mut link = Link::new(h.clone());
        link.blocked_dns(&h, 250);
        let drops = link.feed.with_device(|d| d.stats().dns_tx_drops).unwrap();
        link.feed.with_device(|d| {
            d.service_dns_tx();
            assert_eq!(d.dns_pending_tx(), 1);
            d.service_dns_tx();
            assert_eq!(d.dns_pending_tx(), 0);
            assert_eq!(d.stats().dns_tx_drops, drops + 1);
            assert_eq!(d.stats().dns_tx_would_block, 3);
        });
        drain(&link.os);
        link.blocked_dns(&h, 251);
        let socket = link.feed.socket_fd();
        let (tun, os) = pair();
        link.feed.feed_tun(tun.as_raw_fd()).unwrap();
        link._tun = tun;
        link.os = os;
        link.step();
        assert!(
            drain(&link.os).is_empty(),
            "old queued response crossed TUN generation"
        );
        assert_eq!(link.feed.socket_fd(), socket);
        assert!(link.feed.tunnel_ready());
        link.ordinary(101);
        link.blocked_dns(&h, 252);
        drain(&link.os);
        link.feed.release_tun().unwrap();
        link.step();
        assert!(
            drain(&link.os).is_empty(),
            "release wrote a queued response"
        );
        assert_eq!(link.feed.socket_fd(), socket);
        assert!(link.feed.tunnel_ready());
        link.feed.feed_tun(link._tun.as_raw_fd()).unwrap();
        link.blocked_dns(&h, 253);
        drain(&link.os);
        let held = link.feed.pump_wait_plan(libc::mono_ms(), false);
        link.feed.clear();
        assert!(drain(&link.os).is_empty(), "clear wrote a queued response");
        assert!(!link.feed.device_up());
        assert!(link
            .feed
            .pump_wait_plan(libc::mono_ms(), false)
            .dns_handle
            .is_none());
        drop(held);
        runtime.stop_and_join();
    }

    #[test]
    fn wg_dns_production_pump_delivers_worker_completion() {
        let mut runtime = runtime(&config());
        let h = runtime.handle();
        let link = Link::new(h);
        let stop = Arc::new(AtomicBool::new(false));
        link.feed.spawn_data_plane_pump(stop.clone());
        let mut client = Client::new();
        client.send(240, "host.local.test");
        client.iface.poll_egress(
            Instant::from_millis(0),
            &mut client.dev,
            &mut client.sockets,
        );
        while let Some(p) = client.dev.tx.pop_front() {
            link.os.send(&p).unwrap();
        }
        let (ret, _, _) = libc::poll1(link.os.as_raw_fd(), libc::POLLIN, 1000);
        assert!(ret > 0);
        client.dev.rx.extend(drain(&link.os));
        for _ in 0..128 {
            client.iface.poll_ingress_single(
                Instant::from_millis(1),
                &mut client.dev,
                &mut client.sockets,
            );
        }
        assert_eq!(client.receive()[0].id(), 240);
        stop.store(true, Ordering::Release);
        link.feed.clear();
        runtime.stop_and_join();
    }
}
