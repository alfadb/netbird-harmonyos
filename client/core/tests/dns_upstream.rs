use netbird_core::socket_supply;
#[path = "../src/dns_upstream.rs"]
mod dns_upstream;
use dns_upstream::{DnsUpstream, DnsUpstreamError};
use std::{
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
};

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
fn query(kind: u16) -> Vec<u8> {
    let mut q = vec![
        0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 1, 4, b't', b'e', b's', b't', 0,
    ];
    q.extend(kind.to_be_bytes());
    q.extend([0, 1]);
    // EDNS, including an unknown option.
    q.extend([0, 0, 41, 16, 0, 0, 0, 0, 0, 0, 5, 0xfd, 0xe8, 0, 1, 42]);
    q
}
fn answer(q: &[u8], kind: u16) -> Vec<u8> {
    let mut a = q.to_vec();
    a[2] = 0x81;
    a[3] = 0x80;
    a[7] = 1;
    // Insert before OPT, with a compressed owner and opaque unknown RDATA.
    let data = match kind {
        1 => vec![1, 2, 3, 4],
        28 => vec![0x11; 16],
        16 => vec![3, b'a', b'b', b'c'],
        _ => vec![0xff, 0xc0, 0, 42],
    };
    let mut rr = vec![0xc0, 12];
    rr.extend(kind.to_be_bytes());
    rr.extend([0, 1, 0, 0, 0, 60]);
    rr.extend((data.len() as u16).to_be_bytes());
    rr.extend(data);
    a.splice(22..22, rr);
    a
}
async fn read_frame(s: &mut TcpStream) -> Vec<u8> {
    let mut p = [0; 2];
    s.read_exact(&mut p).await.unwrap();
    let mut b = vec![0; u16::from_be_bytes(p) as usize];
    s.read_exact(&mut b).await.unwrap();
    b
}
async fn write_frame(s: &mut TcpStream, b: &[u8]) {
    // Split length and body to exercise framing across separate writes.
    let p = (b.len() as u16).to_be_bytes();
    s.write_all(&p[..1]).await.unwrap();
    tokio::task::yield_now().await;
    s.write_all(&p[1..]).await.unwrap();
    for part in b.chunks(997) {
        s.write_all(part).await.unwrap();
    }
}
#[tokio::test]
async fn ordinary_udp_preserves_records_edns_and_filters_unrelated_packets() {
    for kind in [1, 28, 16, 65400] {
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        let q = query(kind);
        let expected = answer(&q, kind);
        let response = expected.clone();
        let sent = q.clone();
        let task = tokio::spawn(async move {
            let mut b = [0; 65535];
            let (n, peer) = server.recv_from(&mut b).await.unwrap();
            assert_eq!(&b[..n], sent);
            let mut wrong = response.clone();
            wrong[0] ^= 1;
            server.send_to(&wrong, peer).await.unwrap();
            wrong = response.clone();
            wrong[13] = b'x';
            server.send_to(&wrong, peer).await.unwrap();
            server.send_to(&response, peer).await.unwrap();
        });
        let supply = Arc::new(socket_supply::SocketSupply::new());
        supply.close();
        let result = DnsUpstream::new(supply.clone())
            .exchange(addr, &q, false)
            .await
            .unwrap();
        assert_eq!(result.wire, expected);
        assert!(!result.used_tcp && !result.protected);
        assert_eq!(supply.poll(), "{\"id\":\"\"}");
        task.await.unwrap();
    }
}
#[tokio::test]
async fn tc_retries_tcp_and_direct_tcp_accepts_full_length_and_nxdomain() {
    for direct in [false, true] {
        let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = tcp.local_addr().unwrap();
        let udp = UdpSocket::bind(addr).await.unwrap();
        let q = query(65400);
        let sent = q.clone();
        let mut expected = answer(&q, 65400);
        expected[3] = 0x83;
        if direct {
            // One valid opaque RR fills the complete 65535-byte TCP payload.
            let data_len = 65535 - 22 - 12 - 16;
            expected = q[..22].to_vec();
            expected[2] = 0x81;
            expected[3] = 0x83;
            expected[7] = 1;
            expected.extend([0xc0, 12, 0xff, 0x78, 0, 1, 0, 0, 0, 60]);
            expected.extend((data_len as u16).to_be_bytes());
            expected.resize(22 + 12 + data_len, 0xa5);
            expected.extend(&q[22..]);
            assert_eq!(expected.len(), 65535);
        }
        let response = expected.clone();
        let task = tokio::spawn(async move {
            if !direct {
                let mut b = [0; 65535];
                let (n, p) = udp.recv_from(&mut b).await.unwrap();
                assert_eq!(&b[..n], sent);
                let mut tc = sent.clone();
                tc[2] = 0x83;
                udp.send_to(&tc, p).await.unwrap();
            }
            let (mut s, _) = tcp.accept().await.unwrap();
            assert_eq!(read_frame(&mut s).await, sent);
            let mut wrong = response.clone();
            wrong[1] ^= 1;
            write_frame(&mut s, &wrong).await;
            write_frame(&mut s, &response).await;
        });
        let result = DnsUpstream::new(Arc::new(socket_supply::SocketSupply::new()))
            .exchange(addr, &q, direct)
            .await
            .unwrap();
        assert_eq!(result.wire, expected);
        assert!(result.used_tcp);
        assert!(!result.protected);
        task.await.unwrap();
    }
}
#[tokio::test]
async fn tcp_eof_and_udp_timeout_are_bounded_without_tcp_retry() {
    let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = tcp.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut s, _) = tcp.accept().await.unwrap();
        read_frame(&mut s).await;
        s.write_all(&[0, 50, 1, 2]).await.unwrap();
    });
    let upstream = DnsUpstream::new(Arc::new(socket_supply::SocketSupply::new()));
    assert_eq!(
        upstream.exchange(addr, &query(1), true).await.unwrap_err(),
        DnsUpstreamError::Eof
    );
    task.await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = udp.local_addr().unwrap();
    let _tcp = TcpListener::bind(addr).await.unwrap();
    let start = std::time::Instant::now();
    assert_eq!(
        upstream.exchange(addr, &query(1), false).await.unwrap_err(),
        DnsUpstreamError::Timeout
    );
    assert!(start.elapsed() < Duration::from_secs(6));
}
async fn provide(supply: Arc<socket_supply::SocketSupply>, kind: &str) {
    let json = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let j = supply.poll();
            if j != "{\"id\":\"\"}" {
                break j;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(json.contains("\"purpose\":\"dns\""));
    assert!(json.contains(&format!("\"kind\":\"{kind}\"")));
    let id = json.split('"').nth(3).unwrap().parse().unwrap();
    let raw = if kind == "udp" {
        netbird_core::host_sockets::open_udp_unbound().unwrap()
    } else {
        netbird_core::host_sockets::open_tcp_prebound().unwrap()
    };
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    assert_eq!(
        supply.reply(id, fd.as_raw_fd(), "protected"),
        "{\"ok\":true}"
    );
}
#[tokio::test]
async fn physical_udp_and_tcp_use_host_supplied_sockets() {
    let ip = "127.77.19.31";
    let udp = UdpSocket::bind(format!("{ip}:53"))
        .await
        .expect("dedicated loopback UDP 53");
    let tcp = TcpListener::bind(format!("{ip}:53"))
        .await
        .expect("dedicated loopback TCP 53");
    let addr = udp.local_addr().unwrap();
    let supply = Arc::new(socket_supply::SocketSupply::new());
    supply.set_dns_servers(vec![ip.parse().unwrap()]);
    let upstream = DnsUpstream::new(supply.clone());
    for direct in [false, true] {
        let q = query(1);
        let expected = answer(&q, 1);
        let response = expected.clone();
        let exchange = upstream.exchange(addr, &q, direct);
        let supplier = provide(supply.clone(), if direct { "tcp" } else { "udp" });
        let serve = async {
            if direct {
                let (mut s, _) = tcp.accept().await.unwrap();
                assert_eq!(read_frame(&mut s).await, q);
                write_frame(&mut s, &response).await;
            } else {
                let mut b = [0; 65535];
                let (n, p) = udp.recv_from(&mut b).await.unwrap();
                assert_eq!(&b[..n], q);
                udp.send_to(&response, p).await.unwrap();
            }
        };
        let (result, _, _) = tokio::join!(exchange, supplier, serve);
        let result = result.unwrap();
        assert!(result.protected);
        assert_eq!(result.used_tcp, direct);
        assert_eq!(result.wire, expected);
    }
}
#[tokio::test]
async fn cancelling_exchange_closes_supplied_tcp_and_rejection_does_not_fallback() {
    let listener = TcpListener::bind("127.77.19.33:53").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let supply = Arc::new(socket_supply::SocketSupply::new());
    supply.set_dns_servers(vec!["127.77.19.33".parse().unwrap()]);
    let task = tokio::spawn({
        let supply = supply.clone();
        async move {
            DnsUpstream::new(supply)
                .exchange(addr, &query(1), true)
                .await
        }
    });
    provide(supply.clone(), "tcp").await;
    let (mut peer, _) = listener.accept().await.unwrap();
    assert_eq!(read_frame(&mut peer).await, query(1));
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    let mut byte = [0];
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), peer.read(&mut byte))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    supply.close();
    assert_eq!(
        DnsUpstream::new(supply)
            .exchange(addr, &query(1), false)
            .await
            .unwrap_err(),
        DnsUpstreamError::Supply
    );
}

#[tokio::test]
async fn tcp_read_timeout_uses_the_same_total_budget() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let exchange = async {
        DnsUpstream::new(Arc::new(socket_supply::SocketSupply::new()))
            .exchange(addr, &query(1), true)
            .await
            .unwrap_err()
    };
    let serve = async {
        let (mut peer, _) = listener.accept().await.unwrap();
        read_frame(&mut peer).await;
        // A partial length prefix must not restart the exchange deadline.
        peer.write_all(&[0]).await.unwrap();
        let mut byte = [0];
        assert_eq!(peer.read(&mut byte).await.unwrap(), 0);
    };
    let (error, _) = tokio::time::timeout(Duration::from_secs(6), async {
        tokio::join!(exchange, serve)
    })
    .await
    .unwrap();
    assert_eq!(error, DnsUpstreamError::Timeout);
}

#[tokio::test]
async fn supply_deadline_cleans_pending_and_invalid_query_is_rejected() {
    let supply = Arc::new(socket_supply::SocketSupply::new());
    supply.set_dns_servers(vec!["127.77.19.32".parse().unwrap()]);
    let upstream = DnsUpstream::new(supply.clone());
    assert_eq!(
        upstream
            .exchange("127.77.19.32:53".parse().unwrap(), &query(1), true)
            .await
            .unwrap_err(),
        DnsUpstreamError::Timeout
    );
    assert_eq!(supply.poll(), "{\"id\":\"\"}");
    assert_eq!(
        upstream
            .exchange("127.0.0.1:1".parse().unwrap(), &[], false)
            .await
            .unwrap_err(),
        DnsUpstreamError::InvalidQuery
    );
}
