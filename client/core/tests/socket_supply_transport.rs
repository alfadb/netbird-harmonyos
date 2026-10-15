// Host-only linker stubs, as in host_supply_relay; these APIs are never invoked.
mod host_link_stubs {
    use core::ffi::c_void;
    #[no_mangle] pub extern "C" fn napi_module_register(_: *mut c_void) {}
    #[no_mangle] pub extern "C" fn napi_create_function(_: *mut c_void, _: *const u8, _: usize, _: *mut c_void, _: *mut c_void, _: *mut *mut c_void) -> i32 { 0 }
    #[no_mangle] pub extern "C" fn napi_set_named_property(_: *mut c_void, _: *mut c_void, _: *const u8, _: *mut c_void) -> i32 { 0 }
    #[no_mangle] pub extern "C" fn napi_get_cb_info(_: *mut c_void, _: *mut c_void, _: *mut usize, _: *mut *mut c_void, _: *mut c_void, _: *mut *mut c_void) -> i32 { 0 }
    #[no_mangle] pub extern "C" fn napi_create_string_utf8(_: *mut c_void, _: *const u8, _: usize, _: *mut c_void) -> i32 { 0 }
    #[no_mangle] pub extern "C" fn napi_get_value_string_utf8(_: *mut c_void, _: *mut c_void, _: *mut u8, _: usize, _: *mut usize) -> i32 { 0 }
    #[no_mangle] pub extern "C" fn napi_get_value_int32(_: *mut c_void, _: *mut c_void, _: *mut i32) -> i32 { 0 }
    #[no_mangle] pub extern "C" fn napi_get_value_bool(_: *mut c_void, _: *mut c_void, _: *mut bool) -> i32 { 0 }
}

use netbird_core::socket_supply::{Purpose, SocketKind, SocketSupply};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::Arc;
use std::time::Duration;

static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

async fn queued(supply: &SocketSupply) -> (u64, String) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let json = supply.poll();
            if json != "{\"id\":\"\"}" {
                let id = json.split('"').nth(3).unwrap().parse().unwrap();
                return (id, json);
            }
            tokio::task::yield_now().await;
        }
    }).await.unwrap()
}

fn socket_type(fd: i32) -> i32 {
    extern "C" { fn getsockopt(fd: i32, level: i32, option: i32, value: *mut i32, len: *mut u32) -> i32; }
    let mut value = 0;
    let mut len = 4;
    assert_eq!(unsafe { getsockopt(fd, 1, 3, &mut value, &mut len) }, 0);
    value
}

#[tokio::test]
async fn explicit_dns_transport_duplicates_real_socket_and_preserves_defaults() {
    let _serial = TEST_LOCK.lock().unwrap();
    for (purpose, explicit, expected) in [
        (Purpose::Dns, Some(SocketKind::Udp), "udp"),
        (Purpose::Dns, Some(SocketKind::Tcp), "tcp"),
        (Purpose::Dns, None, "udp"),
        (Purpose::Management, None, "tcp"),
        (Purpose::Signal, None, "tcp"),
        (Purpose::Relay, None, "tcp"),
    ] {
        let supply = Arc::new(SocketSupply::new());
        let task = tokio::spawn({ let supply = supply.clone(); async move {
            match explicit {
                Some(kind) => supply.request_socket(purpose, kind).await,
                None => supply.request_with_id(purpose).await.map(|(_, fd)| fd),
            }
        }});
        let (id, json) = queued(&supply).await;
        assert!(json.contains(&format!("\"kind\":\"{expected}\"")));
        let raw = if expected == "udp" {
            netbird_core::host_sockets::open_udp_unbound().unwrap()
        } else { netbird_core::host_sockets::open_tcp_prebound().unwrap() };
        let original = unsafe { OwnedFd::from_raw_fd(raw) };
        assert_eq!(supply.reply(id, raw, "protected"), "{\"ok\":true}");
        let delivered = task.await.unwrap().unwrap();
        assert_ne!(delivered.as_raw_fd(), original.as_raw_fd());
        drop(original);
        assert_eq!(socket_type(delivered.as_raw_fd()), if expected == "udp" { 2 } else { 1 });
        let delivered_raw = delivered.as_raw_fd();
        drop(delivered);
        extern "C" { fn fcntl(fd: i32, cmd: i32, ...) -> i32; }
        assert_eq!(unsafe { fcntl(delivered_raw, 1) }, -1);
        assert_eq!(supply.reply(id, -1, "protected"), "{\"ok\":false,\"error\":\"request-expired\"}");
    }
}

#[tokio::test]
async fn host_supplier_honors_dns_transport() {
    let _serial = TEST_LOCK.lock().unwrap();
    let supply = Arc::new(SocketSupply::new());
    netbird_core::socket_supply::install(supply.clone());
    let mut supplier = netbird_core::host_sockets::HostSocketSupplier::start();
    for (kind, expected) in [(SocketKind::Udp, 2), (SocketKind::Tcp, 1)] {
        let fd = supply.request_socket(Purpose::Dns, kind).await.unwrap();
        assert_eq!(socket_type(fd.as_raw_fd()), expected);
    }
    supplier.stop();
    assert_eq!(supplier.served(), 2);
    assert_eq!(supplier.failed(), 0);
    netbird_core::socket_supply::shutdown();
}

#[tokio::test]
async fn explicit_dns_tcp_rejection_cancellation_and_shutdown_fail_closed() {
    let _serial = TEST_LOCK.lock().unwrap();
    for mode in 0..4 {
        let supply = Arc::new(SocketSupply::new());
        let task = tokio::spawn({ let supply = supply.clone(); async move {
            supply.request_socket(Purpose::Dns, SocketKind::Tcp).await
        }});
        let (id, _) = queued(&supply).await;
        match mode {
            0 => { assert_eq!(supply.reply(id, -1, "protect-failed"), "{\"ok\":false}"); },
            1 => supply.cancel_pending_for_network_change(),
            2 => supply.close(),
            _ => { task.abort(); },
        }
        if mode == 3 { assert!(task.await.unwrap_err().is_cancelled()); }
        else { assert!(task.await.unwrap().is_err()); }
        assert!(supply.reply(id, -1, "protected").contains("request-expired"));
        assert_eq!(supply.poll(), "{\"id\":\"\"}");
    }
}
