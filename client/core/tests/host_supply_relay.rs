// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! Host-side socket supply, the part diag-0048 R2 was missing (separate test
//! process: `socket_supply`'s poll/reply slot is process-global).
//!
//! What is pinned here:
//!
//! 1. `HostSocketSupplier` answers the seams' own supply requests with plain
//!    host sockets (`Purpose::Relay` TCP, `Purpose::Dns` UDP) — the exact
//!    device-shell role, minus protect;
//! 2. a `ProtectedRelayDialer` (the PRODUCTION dialer) can therefore open a
//!    real TCP stream against a LOCAL loopback listener — no real relay is
//!    contacted, no external network is used;
//! 3. the two fail-closed pre-images are preserved: no supply at all still
//!    yields `no-protected-socket`, and a hostname relay URL with NO installed
//!    DNS servers still yields `dns:no-physical-server` *before* any supply
//!    request is raised — which is exactly the 0048 host-side failure mode
//!    (840 `[n1b]` lines in `host-peer.log`, ZERO `socket-supply: request`);
//! 4. the `/etc/resolv.conf` parser behind `host_dns_servers()`.

// Host-process link stubs (repo convention, cf. tests/host_interop_n9.rs):
// libace_napi.z.so / libhilog_ndk.z.so do not exist on the host. No-ops for
// the linker only; never called by these tests.
mod host_link_stubs {
    use core::ffi::c_void;

    #[no_mangle]
    pub extern "C" fn OH_LOG_Print(
        _t: i32,
        _l: i32,
        _d: u32,
        _tag: *const u8,
        _fmt: *const u8,
        _arg: *const c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn OH_LOG_IsLoggable(_d: u32, _tag: *const u8, _l: i32) -> bool {
        false
    }

    #[no_mangle]
    pub extern "C" fn napi_module_register(_m: *mut c_void) {}

    #[no_mangle]
    pub extern "C" fn napi_create_function(
        _e: *mut c_void,
        _n: *const u8,
        _l: usize,
        _cb: *mut c_void,
        _d: *mut c_void,
        r: *mut *mut c_void,
    ) -> i32 {
        unsafe { *r = 0x10 as *mut c_void };
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_set_named_property(
        _e: *mut c_void,
        _o: *mut c_void,
        _n: *const u8,
        _v: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_cb_info(
        _e: *mut c_void,
        _i: *mut c_void,
        _argc: *mut usize,
        _argv: *mut *mut c_void,
        _this: *mut c_void,
        _data: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _e: *mut c_void,
        _s: *const u8,
        _l: usize,
        _r: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_string_utf8(
        _e: *mut c_void,
        _v: *mut c_void,
        _b: *mut u8,
        _bs: usize,
        _r: *mut usize,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_int32(
        _e: *mut c_void,
        _v: *mut c_void,
        _r: *mut i32,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(
        _e: *mut c_void,
        _v: *mut c_void,
        _r: *mut bool,
    ) -> i32 {
        0
    }
}

use std::net::{Ipv4Addr, TcpListener};
use std::os::fd::AsRawFd;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use base64::Engine as _;

use netbird_core::host_sockets as hs;
use netbird_core::relay_client::{ProtectedRelayDialer, RelayDialer, RelayUrl};
use netbird_core::socket_supply::{Purpose, SocketSupply};

/// The supply slot is process-global: every test here serializes on it.
fn supply_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = LOCK.get_or_init(|| Mutex::new(()));
    match lock.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("test runtime")
}

#[test]
fn parse_resolv_conf_reads_ipv4_nameservers_in_order() {
    let text = "\
# comment
nameserver 1.1.1.1
nameserver\t8.8.8.8
nameserver 1.1.1.1
nameserver fe80::1
nameserver 2
nameserver6 9.9.9.9
nameserver
nameserver 10.0.0.53 extra-token
search example.test
";
    assert_eq!(
        hs::parse_resolv_conf(text),
        vec![
            Ipv4Addr::new(1, 1, 1, 1),
            Ipv4Addr::new(8, 8, 8, 8),
            Ipv4Addr::new(10, 0, 0, 53),
        ]
    );
}

/// The ASSEMBLY `nbinterop` actually performs, without touching any network:
/// start the connector over a host-fed management socket (exactly
/// `hs::start_connector_over_host_socket`), let it install its supply into the
/// process-global slot, then answer a request raised ON THAT SUPPLY from the
/// host responder. `connector_protected_dns_start_json` is a public seam that
/// raises a real `Purpose::Dns` request on the connector's own supply, so the
/// test observes the connector→responder wiring end to end (loopback only:
/// the DNS server is the local host address, and whether the query is answered
/// is irrelevant — the SUPPLY step is the assertion).
#[test]
fn connector_installed_supply_is_answered_by_the_host_responder() {
    let _guard = supply_lock();
    netbird_core::connector::connector_stop_json();
    let fd = hs::open_tcp_prebound().expect("host tcp socket");
    let config = format!(
        "{{\"management_url\":\"http://127.0.0.1:1\",\"private_key\":\"{}\"}}",
        base64::engine::general_purpose::STANDARD.encode([0x05u8; 32])
    );
    let secrets = "{\"setup_key\":\"interop-test-key-not-real\"}";
    let state = hs::start_connector_over_host_socket(
        fd,
        &config,
        secrets,
        "127.0.0.1:1".parse().expect("addr"),
    )
    .expect("host-path start must succeed with a REAL fd");
    // `start` returns the state at the instant `handle.status()` is read. The
    // `Connect` transition is applied synchronously in `spawn`
    // (connector.rs:3874), but the worker runs on the multi-thread runtime and
    // dials the deliberately dead management address `127.0.0.1:1`; its first
    // failed attempt applies `ConnEvent::Lost` (connector.rs:4256), which the
    // state machine defines as a legal `connecting -> reconnecting` transition
    // ("handshake timed out; retry scheduled", state.rs:131-133). Which of the
    // two is observed is pure thread scheduling, not connector behaviour: both
    // mean "started, waiting to dial management". The supply wiring below is
    // the assertion; the state check only guards against a terminal state.
    assert!(
        state == "connecting" || state == "reconnecting",
        "started connector must be connecting or retrying, got {state}"
    );

    // The connector installed its supply globally (connector.rs `install`): a
    // reply for an unknown id reaches an INSTALLED supply ("request-expired"),
    // never the no-supply slot token ("socket-supply-closed").
    let probe = netbird_core::socket_supply::reply_json("18446744073709551615", -1, "protected");
    assert!(
        probe.contains("request-expired"),
        "connector supply installed into the global slot: {probe}"
    );

    hs::feed_dns_servers(&[Ipv4Addr::LOCALHOST]).expect("dns servers installed on the supply");
    let rt = runtime();
    rt.block_on(async {
        let mut supplier = hs::HostSocketSupplier::start();
        let asked = netbird_core::connector::connector_protected_dns_start_json("host.invalid");
        assert!(asked.contains("\"ok\":true"), "dns lookup started: {asked}");
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while supplier.served() == 0 && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            supplier.served() >= 1,
            "the connector-raised Purpose::Dns request was answered by the host responder \
             (served={} failed={})",
            supplier.served(),
            supplier.failed()
        );
        supplier.stop();
    });
    netbird_core::connector::connector_stop_json();
    netbird_core::socket_supply::shutdown();
}

#[test]
fn host_supplier_answers_relay_and_dns_requests() {
    let _guard = supply_lock();
    let rt = runtime();
    rt.block_on(async {
        let supply = Arc::new(SocketSupply::new());
        netbird_core::socket_supply::install(supply.clone());
        let mut supplier = hs::HostSocketSupplier::start();

        let relay = tokio::time::timeout(Duration::from_secs(5), supply.request(Purpose::Relay))
            .await
            .expect("relay request answered within the fuse")
            .expect("relay fd delivered");
        assert!(relay.as_raw_fd() >= 0, "real TCP descriptor");

        let dns = tokio::time::timeout(Duration::from_secs(5), supply.request(Purpose::Dns))
            .await
            .expect("dns request answered within the fuse")
            .expect("dns fd delivered");
        assert!(dns.as_raw_fd() >= 0, "real UDP descriptor");

        assert_eq!(supplier.served(), 2, "both requests answered with a socket");
        assert_eq!(supplier.failed(), 0);
        supplier.stop();
        netbird_core::socket_supply::shutdown();
    });
}

/// The load-bearing case: the PRODUCTION relay dialer takes a socket from the
/// host supply and completes a real TCP connect — to a LOCAL listener, so no
/// relay (or any external host) is involved.
#[test]
fn protected_relay_dial_opens_a_host_socket_against_a_local_listener() {
    let _guard = supply_lock();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let port = listener.local_addr().expect("listener addr").port();
    let (accepted_tx, accepted_rx) = std::sync::mpsc::channel::<()>();
    let acceptor = std::thread::spawn(move || {
        if listener.accept().is_ok() {
            let _ = accepted_tx.send(());
        }
    });

    let rt = runtime();
    rt.block_on(async {
        let supply = Arc::new(SocketSupply::new());
        netbird_core::socket_supply::install(supply.clone());
        let mut supplier = hs::HostSocketSupplier::start();
        let dialer = ProtectedRelayDialer::new(Some(supply.clone()));
        // An IP literal: the protected resolver short-circuits, so the ONLY
        // thing this dial needs from the supply is the relay TCP socket.
        let url = RelayUrl::parse(&format!("rel://127.0.0.1:{port}")).expect("relay url");
        let stream = dialer
            .dial(&url)
            .await
            .expect("relay dial must obtain a socket instead of failing class=dial");
        drop(stream);
        assert!(supplier.served() >= 1, "one Purpose::Relay request answered");
        supplier.stop();
        netbird_core::socket_supply::shutdown();
    });

    accepted_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("loopback listener accepted the relay dial");
    acceptor.join().expect("acceptor thread");
}

/// Pre-image 1: no supply at all (the `connector_start_json` entry, NOT the
/// path `nbinterop` uses) stays typed `no-protected-socket`.
#[test]
fn relay_dial_without_any_supply_stays_fail_closed() {
    let rt = runtime();
    rt.block_on(async {
        let dialer = ProtectedRelayDialer::new(None);
        let url = RelayUrl::parse("rel://127.0.0.1:9").expect("relay url");
        let err = match dialer.dial(&url).await {
            Ok(_) => panic!("no supply must be refused"),
            Err(e) => e,
        };
        assert!(
            format!("{err}").contains("no-protected-socket"),
            "typed fail-closed token: {err}"
        );
    });
}

/// Pre-image 2 (the actual 0048 host-side failure): the supply IS attached —
/// `connector.rs` passes it into `RelayMaterials` — but with no installed DNS
/// servers a HOSTNAME relay URL dies at the protected resolver, BEFORE any
/// supply request is raised. That is why a frozen log can carry 840 `[n1b]`
/// lines and zero `socket-supply: request` lines.
#[test]
fn hostname_relay_url_without_dns_servers_fails_at_the_resolver() {
    let _guard = supply_lock();
    let rt = runtime();
    rt.block_on(async {
        let supply = Arc::new(SocketSupply::new());
        netbird_core::socket_supply::install(supply.clone());
        let dialer = ProtectedRelayDialer::new(Some(supply.clone()));
        let url = RelayUrl::parse("rels://relay.invalid:443").expect("relay url");
        let err = match dialer.dial(&url).await {
            Ok(_) => panic!("no dns servers must fail at the resolver"),
            Err(e) => e,
        };
        assert!(
            format!("{err}").contains("dns:no-physical-server"),
            "typed resolver token: {err}"
        );
        assert!(
            supply.poll_request().is_none(),
            "the failure happens BEFORE any Purpose::Dns/Relay request"
        );
        netbird_core::socket_supply::shutdown();
    });
}
