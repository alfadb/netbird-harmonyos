// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! R1 (upstream-alignment `analysis-20261025-upstream-compare` §8): a peer
//! whose RELAY carrier is attached and whose relay transport is live is
//! "Connected" in the official client even when ICE has not selected a pair
//! yet (`client/internal/peer/conn.go:819-825` `evalStatus`:
//! `statusRelay == Connected || statusICE == Connected`).
//!
//! The local client's `reachable` fact is deliberately UNCHANGED (ICE
//! selected + endpoint landed — the actual forwarding path), so this file
//! pins the ADDITIVE availability view:
//!
//! - `ice.peers_online` = peers available through EITHER transport
//!   (ICE-direct reachable OR relay-only), the official "Connected" count;
//! - `ice.peers_relay_only` = the subset available ONLY through relay;
//! - `ice.reachable` stays the ICE-direct count (existing semantics);
//! - a peer whose relay transport is DOWN (lane still attached across a
//!   transient relay outage) is NOT online — the lane existing is not proof
//!   that the relay path currently carries traffic;
//! - the per-peer three-state token is `ice-direct` / `relay-only` /
//!   `offline`, and its log line is the same token set.
//!
//! No forwarding path is exercised or changed here: the orchestrator is
//! driven by a recording WG seam and an unwired signal mock, so only the
//! status/observability surface is under test.

// Host-test link surface (same as tests/peer_conn_e2e.rs): the test binary
// links the whole crate rlib on the host triple, where libace_napi.z.so /
// libhilog_ndk.z.so do not exist. No-ops for the linker.
mod host_link_stubs {
    use core::ffi::c_void;

    #[no_mangle]
    pub extern "C" fn OH_LOG_Print(
        _log_type: i32,
        _level: i32,
        _domain: u32,
        _tag: *const u8,
        _fmt: *const u8,
        _arg: *const c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn OH_LOG_IsLoggable(_domain: u32, _tag: *const u8, _level: i32) -> bool {
        false
    }

    #[no_mangle]
    pub extern "C" fn napi_module_register(_mod_: *mut c_void) {}

    #[no_mangle]
    pub extern "C" fn napi_create_function(
        _env: *mut c_void,
        _utf8name: *const u8,
        _length: usize,
        _cb: *const u8,
        _data: *mut c_void,
        _result: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_set_named_property(
        _env: *mut c_void,
        _name: *const c_void,
        _value: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _env: *mut c_void,
        _str_: *const u8,
        _len: usize,
        _result: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_cb_info(
        _env: *mut c_void,
        _cbinfo: *mut c_void,
        _argc: *mut usize,
        _argv: *mut *mut c_void,
        _this: *mut c_void,
        _data: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_string_utf8(
        _env: *mut c_void,
        _value: *mut c_void,
        _buf: *mut u8,
        _bufsize: usize,
        _result: *mut usize,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_int32(
        _env: *mut c_void,
        _value: *mut c_void,
        _result: *mut i32,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(
        _env: *mut c_void,
        _value: *mut c_void,
        _result: *mut bool,
    ) -> i32 {
        0
    }
}

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use netbird_core::connector::{WgPeerApplier, WgPeerEntry};
use netbird_core::ice::{InterfaceAddr, ProtectedUdpFdSource, StaticInterfaces};
use netbird_core::management::ManagementError;
use netbird_core::n2_ring;
use netbird_core::peer_conn::{
    peer_transport, peer_transport_line, PeerIceDeps, PeerIceOrchestrator, PeerSignalKind,
    PeerTransport, RelayPlaneLink, SignalExchange,
};

const KEY: &str = "relaykey01";

/// WG seam that only answers the one question this file is about: is the
/// peer's relay carrier attached to the data plane?
#[derive(Default)]
struct CarrierWg {
    attached: Mutex<Vec<String>>,
}

impl CarrierWg {
    fn attach(&self, key: &str) {
        self.attached.lock().expect("attached").push(key.to_string());
    }
}

impl WgPeerApplier for CarrierWg {
    fn apply_peers(&self, _peers: &[WgPeerEntry]) -> Result<(), String> {
        Ok(())
    }
    fn clear(&self) {
        self.attached.lock().expect("attached").clear();
    }
    fn carrier_attached(&self, pub_key_b64: &str) -> bool {
        self.attached.lock().expect("attached").iter().any(|k| k == pub_key_b64)
    }
}

/// The signal seam is never used: these tests only read the status surface.
struct NoSignal;
impl SignalExchange for NoSignal {
    fn send(
        &self,
        _to_key: &str,
        _kind: PeerSignalKind,
        _payload: &str,
        _port: u32,
        _session_id: Option<&[u8]>,
    ) -> Result<(), ManagementError> {
        Ok(())
    }
}

fn orchestrator(
    carrier_attached: bool,
    relay_transport_up: Option<bool>,
) -> (PeerIceOrchestrator, Arc<CarrierWg>, Arc<RelayPlaneLink>) {
    let wg = Arc::new(CarrierWg::default());
    if carrier_attached {
        wg.attach(KEY);
    }
    let mut orch = PeerIceOrchestrator::new(PeerIceDeps {
        ifaces: Arc::new(StaticInterfaces(vec![InterfaceAddr {
            name: "eth0".into(),
            addr: [127, 0, 0, 1],
        }])),
        socks: Arc::new(ProtectedUdpFdSource::new_with_fd(-1).expect("udp source")),
        signal: Arc::new(NoSignal),
        wg: wg.clone(),
        tie_breaker: Some(7),
        fixed_local_port: None,
        advertised_candidates: Vec::new(),
    });
    // The relay bridge exists in production for every ICE-capable start
    // (`connector.rs` `set_relay_plane`). `None` models a harness with no
    // relay plane at all: the lane attach is then the only relay fact, the
    // same fallback `guard_tick` already uses.
    let link = RelayPlaneLink::new();
    if let Some(up) = relay_transport_up {
        link.set_transport_up(up);
        orch.set_relay_plane(link.clone());
    }
    orch.set_peers(&[KEY.to_string()]);
    (orch, wg, link)
}

/// A peer with the relay lane attached and the relay transport Ready — but
/// NO ICE selection (the orchestrator was never pumped) — must count as
/// online, while `reachable` (the ICE-direct fact) stays false.
#[test]
fn relay_only_peer_counts_as_online_but_stays_ice_unreachable() {
    let (orch, _wg, _link) = orchestrator(true, Some(true));
    let summary = orch.summary();
    assert_eq!(summary.peers, 1);
    assert_eq!(
        summary.reachable, 0,
        "reachable keeps its ICE-direct-only meaning (Connected + endpoint landed)"
    );
    let json = summary.to_json();
    assert!(
        json.contains("\"peers_online\":1"),
        "a relay-ready, ICE-unselected peer is Connected in the official client: {json}"
    );
    assert!(
        json.contains("\"peers_relay_only\":1"),
        "the relay-only subset must be visible in the summary: {json}"
    );
}

/// Across a transient relay outage the lane may still be attached (the
/// guard's `carrier_attached` fact deliberately survives it), but the relay
/// transport is DOWN — that peer is not online and not relay-only.
#[test]
fn attached_lane_with_relay_transport_down_is_not_online() {
    let (orch, _wg, _link) = orchestrator(true, Some(false));
    let json = orch.summary().to_json();
    assert_eq!(orch.summary().reachable, 0);
    assert!(json.contains("\"peers_online\":0"), "relay transport is down: {json}");
    assert!(json.contains("\"peers_relay_only\":0"), "no usable relay path: {json}");
}

/// No carrier, no ICE: the peer is offline, and neither availability count
/// may invent one.
#[test]
fn peer_without_any_transport_is_offline() {
    let (orch, _wg, _link) = orchestrator(false, Some(true));
    let json = orch.summary().to_json();
    assert_eq!(orch.summary().reachable, 0);
    assert!(json.contains("\"peers_online\":0"), "no transport at all: {json}");
    assert!(json.contains("\"peers_relay_only\":0"), "no relay path: {json}");
}

/// The classifier is the single source of the three-state token: direct wins
/// when BOTH transports are usable, and a relay lane alone is NOT enough
/// while the shared relay transport is down.
#[test]
fn transport_classifier_prefers_direct_and_requires_a_live_relay() {
    assert_eq!(peer_transport(true, false, false), PeerTransport::IceDirect);
    assert_eq!(peer_transport(true, true, true), PeerTransport::IceDirect, "direct wins");
    assert_eq!(peer_transport(false, true, true), PeerTransport::RelayOnly);
    assert_eq!(
        peer_transport(false, true, false),
        PeerTransport::Offline,
        "an attached lane across a relay outage is not a usable path"
    );
    assert_eq!(peer_transport(false, false, true), PeerTransport::Offline);
    assert_eq!(PeerTransport::IceDirect.as_str(), "ice-direct");
    assert_eq!(PeerTransport::RelayOnly.as_str(), "relay-only");
    assert_eq!(PeerTransport::Offline.as_str(), "offline");
    assert!(PeerTransport::IceDirect.online() && PeerTransport::RelayOnly.online());
    assert!(!PeerTransport::Offline.online());
}

/// Per-peer publishing carries the same three-state token, including the
/// no-relay-plane fallback (lane attach is then the only relay fact, the
/// same fallback `guard_tick` uses).
#[test]
fn per_peer_detail_carries_the_three_state_transport() {
    let (relay_only, _wg, _link) = orchestrator(true, Some(true));
    let detail = relay_only.peer_ice_detail(KEY).expect("peer detail");
    assert_eq!(detail.transport, PeerTransport::RelayOnly);
    assert!(!detail.reachable, "reachable stays ICE-direct-only");

    let (fallback, _wg, _link) = orchestrator(true, None);
    assert_eq!(
        fallback.peer_ice_detail(KEY).expect("peer detail").transport,
        PeerTransport::RelayOnly,
        "unwired relay plane: the attached lane is the only relay fact"
    );

    let (offline, _wg, _link) = orchestrator(false, Some(true));
    assert_eq!(
        offline.peer_ice_detail(KEY).expect("peer detail").transport,
        PeerTransport::Offline
    );
}

/// The change line names the same three tokens and the online projection, so
/// a device log can be grepped for `transport=relay-only` directly.
#[test]
fn peer_transport_log_line_pins_the_three_state_token() {
    assert_eq!(
        peer_transport_line(KEY, PeerTransport::IceDirect),
        "N5c_ICE|peer-transport|peer=relaykey|transport=ice-direct|online=1"
    );
    assert_eq!(
        peer_transport_line(KEY, PeerTransport::RelayOnly),
        "N5c_ICE|peer-transport|peer=relaykey|transport=relay-only|online=1"
    );
    assert_eq!(
        peer_transport_line(KEY, PeerTransport::Offline),
        "N5c_ICE|peer-transport|peer=relaykey|transport=offline|online=0"
    );
}

// ---------------------------------------------------------------------------
// Emission point: the change line really leaves the ICE pump when the
// transport fact moves (not just the pure builder above). The evidence ring is
// process-global inside this test binary, so this ONE test serializes on it.
// ---------------------------------------------------------------------------

fn ring_serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

struct TempRing(PathBuf);

impl TempRing {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir()
            .join(format!("peer-relay-avail-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        TempRing(dir)
    }

    fn path(&self) -> String {
        self.0.join("ring.log").to_string_lossy().into_owned()
    }

    fn text(&self) -> String {
        fs::read_to_string(self.0.join("ring.log")).unwrap_or_default()
    }
}

impl Drop for TempRing {
    fn drop(&mut self) {
        n2_ring::disable();
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `offline` → `relay-only` across two pump passes must leave BOTH change lines
/// in the log tee, so a device capture can distinguish the three states at the
/// exact moment the fact moves.
#[test]
fn transport_transitions_are_emitted_by_the_ice_pump() {
    let _guard = ring_serial();
    let ring = TempRing::new("emit");
    n2_ring::configure(&ring.path(), n2_ring::DEFAULT_MAX_BYTES, n2_ring::DEFAULT_SEGMENTS)
        .expect("configure evidence ring");

    let (mut orch, wg, link) = orchestrator(false, Some(true));
    orch.run_once(1_000).expect("first pass");
    wg.attach(KEY);
    // Production order: the connector attaches the lane on the WG seam, then
    // bumps the bridge's carrier epoch + wakes the pump (`commit_lane_open`).
    link.relay_lane_attached();
    orch.run_once(2_000).expect("second pass");

    let text = ring.text();
    assert!(
        text.contains("N5c_ICE|peer-transport|peer=relaykey|transport=offline|online=0"),
        "the first observation must be logged: {text}"
    );
    assert!(
        text.contains("N5c_ICE|peer-transport|peer=relaykey|transport=relay-only|online=1"),
        "the attach transition must be logged: {text}"
    );
}
