// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N2-H I5 — device-side evidence collector, offline end-to-end
//! (design `docs/n2h-device-collector-design-20260917.md` §7-I5, §8).
//!
//! §8's six scenarios, all offline (no device, no `/proc`, no real endpoint):
//!
//! 1. **clean run** — a full window through `n2h_collector_arm` /
//!    `…_status` / `…_evidence`: the reassembled document is parseable,
//!    matches the host `n2h.rs` document key-for-key and type-for-type,
//!    carries `scanned_sides == ["vpn-tun"]`, both positive-control
//!    directions observed, counters reconciled, verdict `n2h-pass`;
//! 2. **leak counter-example** — an outer probe's payload appears in the TUN
//!    (from-os) → `n2h-fail`, the hit names the probe and the direction is
//!    recorded (D4's fact, in `notes[]` — the frozen renderer has no
//!    per-hit direction field and §3.1/§5.2 forbid changing it);
//! 3. **missing positive control** — the echo peer never answers →
//!    `n2h-inconclusive`, never a pass;
//! 4. **freeze gap** — no signal source → `n2h-inconclusive` and `reasons`
//!    names `'signal'`;
//! 5. **reconciliation broken** — a skewed wg ledger that the frozen BYTE
//!    identity accepts and the design §4.4 packet identity refuses →
//!    `n2h-inconclusive` produced by the COLLECTOR (the judge's
//!    `WindowCounters::reconciled()` is bytes-only and says `true`);
//! 6. **chunk transport** — roundtrip, dropped middle chunk (`Missing`),
//!    mutated payload (`sha256` refusal) and the out-of-range answer.
//!
//! The `/proc` ledger is unavailable offline, so the harness implements the
//! design §3.2 `tee-fallback` caliber through the public
//! [`n2h_collector::tee_fallback_read`] helper — the same document-side
//! caliber, declared in `notes[]` as `counters-source=tee-fallback`. The
//! collector's own state machine, probe sender, tee registration, verdict
//! wiring and chunk encoder are the shipped ones.
//!
//! The collector's window/host registry is process-global (like the
//! connector's), so the scenarios take a serial guard. Every wait is a
//! bounded predicate on the OBSERVED state (no sleep-as-assertion).

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
        _cb: *const c_void,
        _data: *mut c_void,
        _result: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_set_named_property(
        _env: *mut c_void,
        _object: *mut c_void,
        _utf8name: *const u8,
        _value: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_cb_info(
        _env: *mut c_void,
        _info: *mut c_void,
        _argc: *mut usize,
        _argv: *mut *mut c_void,
        _this_arg: *mut c_void,
        _data: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _env: *mut c_void,
        _str_: *const u8,
        _length: usize,
        _result: *mut *mut c_void,
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

use std::collections::BTreeMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use netbird_core::host_sockets::{self, FdBag};
use netbird_core::n2h::{
    self, EndpointKind, Evidence, LoopbackOpts, SinkRecord, TcpSink, UdpSink, WireMarker,
    VERDICT_FAIL, VERDICT_INCONCLUSIVE, VERDICT_PASS,
};
use netbird_core::n2h_collector as nc;
use netbird_core::sys;
use netbird_core::wg_device::{
    TunFrameDir, TunFrameTee, TunTeeTotals, WgDeviceConfig, WgDeviceFeed, WgDeviceStats,
};

// ---------------------------------------------------------------------------
// serial guard + cleanup (the collector registry is process-global)
// ---------------------------------------------------------------------------

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn reset_collector() {
    nc::disarm_json();
    nc::uninstall_host();
}

/// boringtun's ffi installs a process-global panic→SIGSEGV hook the first time
/// a tunnel is created, which SWALLOWS later assertion messages. Re-install a
/// PRINTING hook — unconditionally, because the host loopback reference run
/// creates tunnels in the middle of the clean-run scenario.
fn show_panics() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("\nTEST PANIC: {info}");
    }));
}

/// Bounded predicate wait on real time: 2 ms beats, a fuse that PANICS with
/// the awaited condition, and the condition itself is the observed state (no
/// sleep is ever the assertion).
fn wait_until(fuse_ms: u64, what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_millis(fuse_ms);
    let mut beats = 0u64;
    loop {
        if cond() {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out after {fuse_ms} ms ({beats} beats) waiting for: {what}");
        }
        beats += 1;
        std::thread::sleep(Duration::from_millis(2));
    }
}

// ---------------------------------------------------------------------------
// a minimal strict JSON reader (the crate's own reader is crate-private) —
// enough for §8-1's "same keys, same types" walk and for reading the
// documented NAPI answers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum V {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<V>),
    Obj(Vec<(String, V)>),
}

impl V {
    fn get(&self, key: &str) -> Option<&V> {
        match self {
            V::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn str_of(&self) -> Option<&str> {
        match self {
            V::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Raw token of a JSON number (the documents render integer counters).
    fn num_of(&self) -> Option<&str> {
        match self {
            V::Num(n) => Some(n),
            _ => None,
        }
    }

    fn uint_of(&self) -> Option<u64> {
        self.num_of().and_then(|n| n.parse().ok())
    }

    fn bool_of(&self) -> Option<bool> {
        match self {
            V::Bool(b) => Some(*b),
            _ => None,
        }
    }

    fn arr_of(&self) -> &[V] {
        match self {
            V::Arr(items) => items,
            _ => &[],
        }
    }

    /// Path of a nested field, e.g. `doc.at("tun_negative").at("hit")`.
    fn at(&self, key: &str) -> &V {
        self.get(key).unwrap_or(&V::Null)
    }
}

fn parse_json(text: &str) -> V {
    let bytes = text.as_bytes();
    let mut pos = 0usize;
    let v = parse_value(bytes, &mut pos);
    skip_ws(bytes, &mut pos);
    assert_eq!(pos, bytes.len(), "trailing bytes after the JSON value: {text}");
    v
}

fn skip_ws(b: &[u8], pos: &mut usize) {
    while *pos < b.len() && matches!(b[*pos], b' ' | b'\t' | b'\n' | b'\r') {
        *pos += 1;
    }
}

fn parse_value(b: &[u8], pos: &mut usize) -> V {
    skip_ws(b, pos);
    match b.get(*pos) {
        Some(b'{') => {
            *pos += 1;
            let mut fields = Vec::new();
            skip_ws(b, pos);
            if b.get(*pos) == Some(&b'}') {
                *pos += 1;
                return V::Obj(fields);
            }
            loop {
                skip_ws(b, pos);
                let key = match parse_value(b, pos) {
                    V::Str(s) => s,
                    other => panic!("object key must be a string, got {other:?}"),
                };
                skip_ws(b, pos);
                assert_eq!(b.get(*pos), Some(&b':'), "expected ':' in object");
                *pos += 1;
                let val = parse_value(b, pos);
                fields.push((key, val));
                skip_ws(b, pos);
                match b.get(*pos) {
                    Some(b',') => *pos += 1,
                    Some(b'}') => {
                        *pos += 1;
                        return V::Obj(fields);
                    }
                    other => panic!("expected ',' or '}}' in object, got {other:?}"),
                }
            }
        }
        Some(b'[') => {
            *pos += 1;
            let mut items = Vec::new();
            skip_ws(b, pos);
            if b.get(*pos) == Some(&b']') {
                *pos += 1;
                return V::Arr(items);
            }
            loop {
                items.push(parse_value(b, pos));
                skip_ws(b, pos);
                match b.get(*pos) {
                    Some(b',') => *pos += 1,
                    Some(b']') => {
                        *pos += 1;
                        return V::Arr(items);
                    }
                    other => panic!("expected ',' or ']' in array, got {other:?}"),
                }
            }
        }
        Some(b'"') => {
            *pos += 1;
            let mut s = String::new();
            loop {
                match b.get(*pos) {
                    Some(b'"') => {
                        *pos += 1;
                        return V::Str(s);
                    }
                    Some(b'\\') => {
                        *pos += 1;
                        match b.get(*pos) {
                            Some(b'"') => s.push('"'),
                            Some(b'\\') => s.push('\\'),
                            Some(b'/') => s.push('/'),
                            Some(b'n') => s.push('\n'),
                            Some(b'r') => s.push('\r'),
                            Some(b't') => s.push('\t'),
                            Some(b'b') => s.push('\u{8}'),
                            Some(b'f') => s.push('\u{c}'),
                            Some(b'u') => {
                                let hex = std::str::from_utf8(&b[*pos + 1..*pos + 5])
                                    .expect("\\u escape is ASCII");
                                let code = u32::from_str_radix(hex, 16).expect("\\u escape is hex");
                                s.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                                *pos += 4;
                            }
                            other => panic!("unknown escape {other:?}"),
                        }
                        *pos += 1;
                    }
                    Some(_) => {
                        // copy one UTF-8 scalar verbatim
                        let start = *pos;
                        let len = utf8_len(b[start]);
                        s.push_str(std::str::from_utf8(&b[start..start + len]).expect("utf8"));
                        *pos += len;
                    }
                    None => panic!("unterminated string"),
                }
            }
        }
        Some(b't') => {
            *pos += 4;
            V::Bool(true)
        }
        Some(b'f') => {
            *pos += 5;
            V::Bool(false)
        }
        Some(b'n') => {
            *pos += 4;
            V::Null
        }
        Some(_) => {
            let start = *pos;
            while *pos < b.len()
                && matches!(b[*pos], b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
            {
                *pos += 1;
            }
            V::Num(std::str::from_utf8(&b[start..*pos]).expect("num utf8").to_string())
        }
        None => panic!("unexpected end of JSON"),
    }
}

fn utf8_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first >> 5 == 0b110 {
        2
    } else if first >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

/// §8-1's shared assertion: every path of a document with its JSON type. An
/// array contributes `path` = `"array"` plus the element paths (`path[]…`)
/// when it HAS elements, so an array that is empty in one document is visible
/// as exactly that (and nothing else).
fn key_walk(v: &V, path: &str, out: &mut BTreeMap<String, &'static str>) {
    match v {
        V::Null => {
            out.insert(path.to_string(), "null");
        }
        V::Bool(_) => {
            out.insert(path.to_string(), "bool");
        }
        V::Num(_) => {
            out.insert(path.to_string(), "number");
        }
        V::Str(_) => {
            out.insert(path.to_string(), "string");
        }
        V::Arr(items) => {
            out.insert(path.to_string(), "array");
            for item in items {
                key_walk(item, &format!("{path}[]"), out);
            }
        }
        V::Obj(fields) => {
            for (k, val) in fields {
                key_walk(val, &format!("{path}.{k}"), out);
            }
        }
    }
}

fn schema_of(doc: &V) -> BTreeMap<String, &'static str> {
    let mut out = BTreeMap::new();
    key_walk(doc, "$", &mut out);
    out
}

/// The device document must be key-for-key and type-for-type identical to the
/// host `n2h.rs` document. The ONE tolerated difference is an element path
/// (`…[]…`) whose owning array is COMPLETELY EMPTY in the other document
/// (a clean host run has no `notes`, a clean device run has no `reasons`):
/// an empty array carries no element to compare, so the array itself is
/// compared instead.
fn assert_same_schema(device: &BTreeMap<String, &'static str>, host: &BTreeMap<String, &'static str>) {
    for (path, kind) in device {
        if let Some(host_kind) = host.get(path) {
            assert_eq!(
                kind, host_kind,
                "path {path} has a different type in the device and host documents"
            );
        }
    }
    let mut device_only: Vec<&String> =
        device.keys().filter(|k| !host.contains_key(*k)).collect();
    let mut host_only: Vec<&String> = host.keys().filter(|k| !device.contains_key(*k)).collect();
    device_only.sort();
    host_only.sort();
    let check = |path: &str, other: &BTreeMap<String, &'static str>, which: &str| {
        let owner = match path.rfind("[]") {
            Some(i) => &path[..i],
            None => panic!("schema path {path} exists only in the {which} document (no array owner)"),
        };
        assert_eq!(
            other.get(owner).copied(),
            Some("array"),
            "{which}-only element path {path}: its array {owner} is not an array in the other document"
        );
        let prefix = format!("{owner}[]");
        assert!(
            other.keys().all(|k| !k.starts_with(&prefix)),
            "{which}-only element path {path}: the other document's array {owner} is NOT empty \
             (a real schema difference)"
        );
    };
    for path in &device_only {
        check(path, host, "device");
    }
    for path in &host_only {
        check(path, device, "host");
    }
}

// ---------------------------------------------------------------------------
// the harness host: a fake device seam (design U9's injectable side)
// ---------------------------------------------------------------------------

/// Device frame accounting around the collector's own tee: the tee-fallback
/// caliber's "independent byte count of frames handed to the TUN" (§3.2).
struct CountingTee {
    inner: Arc<dyn TunFrameTee>,
    totals: Mutex<TunTeeTotals>,
}

impl CountingTee {
    fn totals(&self) -> TunTeeTotals {
        *self.totals.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl TunFrameTee for CountingTee {
    fn on_tun_frame(&self, dir: TunFrameDir, frame: &[u8]) {
        {
            let mut t = self.totals.lock().unwrap_or_else(|e| e.into_inner());
            match dir {
                TunFrameDir::FromOs => {
                    t.from_os_frames += 1;
                    t.from_os_bytes += frame.len() as u64;
                }
                TunFrameDir::ToOs => {
                    t.to_os_frames += 1;
                    t.to_os_bytes += frame.len() as u64;
                }
            }
        }
        self.inner.on_tun_frame(dir, frame);
    }
}

/// The offline endpoints: real loopback sinks per outer family plus the
/// counterparty's two sockets (its overlay app port for the a2b positive
/// control, its outer socket for the wg_peer off-path drop).
struct SinkSet {
    mgmt: TcpSink,
    signal: TcpSink,
    relay: TcpSink,
    stun: UdpSink,
    dns: UdpSink,
    peer_overlay: UdpSocket,
    peer_outer: UdpSocket,
}

#[derive(Debug, Clone, Copy)]
struct SinkAddrs {
    mgmt: SocketAddr,
    signal: SocketAddr,
    relay: SocketAddr,
    stun: SocketAddr,
    dns: SocketAddr,
    peer_overlay: SocketAddr,
    peer_outer: SocketAddr,
}

impl SinkSet {
    fn bind() -> SinkSet {
        let peer_overlay = UdpSocket::bind("127.0.0.1:0").expect("peer overlay socket");
        peer_overlay.set_nonblocking(true).expect("nonblocking peer overlay");
        let peer_outer = UdpSocket::bind("127.0.0.1:0").expect("peer outer socket");
        peer_outer.set_nonblocking(true).expect("nonblocking peer outer");
        SinkSet {
            mgmt: TcpSink::bind().expect("mgmt sink"),
            signal: TcpSink::bind().expect("signal sink"),
            relay: TcpSink::bind().expect("relay sink"),
            stun: UdpSink::bind(true).expect("stun sink"),
            dns: UdpSink::bind(false).expect("dns sink"),
            peer_overlay,
            peer_outer,
        }
    }

    fn addrs(&self) -> SinkAddrs {
        SinkAddrs {
            mgmt: self.mgmt.local_addr().expect("mgmt addr"),
            signal: self.signal.local_addr().expect("signal addr"),
            relay: self.relay.local_addr().expect("relay addr"),
            stun: self.stun.local_addr().expect("stun addr"),
            dns: self.dns.local_addr().expect("dns addr"),
            peer_overlay: self.peer_overlay.local_addr().expect("peer overlay addr"),
            peer_outer: self.peer_outer.local_addr().expect("peer outer addr"),
        }
    }

    fn drain_endpoints(&mut self) {
        self.mgmt.drain();
        self.signal.drain();
        self.relay.drain();
        self.stun.drain();
        self.dns.drain();
    }
}

/// The offline [`nc::CollectorHost`]: everything the collector needs, with the
/// device's own facts where the device has them and the harness's sinks where
/// the device has real services.
struct OfflineHost {
    facts: nc::HostFreezeFacts,
    /// Readiness of the fake session (flippable while installed, so the arm
    /// gate is exercised through the same host instance).
    device_up: AtomicBool,
    ready: AtomicBool,
    posctl_port: u16,
    tee: Mutex<Option<Arc<CountingTee>>>,
    sinks: Mutex<SinkSet>,
    /// The counterparty's `unknown_peer_drops` counter (§4.0's wg_peer
    /// receipt): datagrams arriving at its OUTER socket.
    peer_drops: AtomicU64,
    /// The b2a marker the counterparty announces (§4.3, its status channel).
    b2a: Mutex<Option<String>>,
    /// Whether the harness's sinks answer endpoint-side receipts
    /// (`sink-record`, D6's offline-only method).
    receipts: AtomicBool,
    /// The a2b payload the counterparty's overlay socket received.
    posctl_payload: Mutex<Option<Vec<u8>>>,
    /// Peer-side frames already fed (bounded re-feeding: the marker may be
    /// registered a beat after the datagram left).
    peer_feeds: AtomicU64,
    /// Skew applied to every wg-ledger read AFTER the first (`/proc` fixture
    /// tampering): the FIRST read is the clean window-open edge.
    after_skew_packets: AtomicU64,
    paired_reads: AtomicU64,
}

impl OfflineHost {
    fn new(facts: nc::HostFreezeFacts, sinks: SinkSet, receipts: bool) -> Arc<OfflineHost> {
        let posctl_port = sinks.addrs().peer_overlay.port();
        Arc::new(OfflineHost {
            facts,
            device_up: AtomicBool::new(true),
            ready: AtomicBool::new(true),
            posctl_port,
            tee: Mutex::new(None),
            sinks: Mutex::new(sinks),
            peer_drops: AtomicU64::new(0),
            b2a: Mutex::new(None),
            receipts: AtomicBool::new(receipts),
            posctl_payload: Mutex::new(None),
            peer_feeds: AtomicU64::new(0),
            after_skew_packets: AtomicU64::new(0),
            paired_reads: AtomicU64::new(0),
        })
    }

    fn tee_handle(&self) -> Option<Arc<CountingTee>> {
        self.tee.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn announce_b2a(&self, id: &str) {
        *self.b2a.lock().unwrap_or_else(|e| e.into_inner()) = Some(id.to_string());
    }

    fn skew_after_first_read(&self, packets: u64) {
        self.after_skew_packets.store(packets, Ordering::Release);
    }

    fn paired_read_count(&self) -> u64 {
        self.paired_reads.load(Ordering::Acquire)
    }

    /// Drain the counterparty's overlay socket and (echo) feed the frames the
    /// real peer would produce: the a2b entrance frame (the OS routed the
    /// probe into the TUN) and the echo return, plus the peer-initiated b2a
    /// payload.
    ///
    /// Feeds only while the collector is in its `observing` phase — BEFORE
    /// that the posctl markers are not registered yet, so an early feed would
    /// be scanned but never attributable. Re-feeding every beat afterwards
    /// covers the register-a-beat-later race; the count is bounded so a long
    /// window cannot grow the scanner without limit.
    fn pump_peer(&self, echo: bool) -> u64 {
        let mut buf = [0u8; 2048];
        loop {
            let got = {
                let sinks = self.sinks.lock().unwrap_or_else(|e| e.into_inner());
                sinks.peer_overlay.recv_from(&mut buf)
            };
            match got {
                Ok((n, _from)) => {
                    let payload = buf[..n].to_vec();
                    if payload.starts_with(b"N2H-posctl-") {
                        *self.posctl_payload.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(payload);
                    }
                }
                Err(_) => break,
            }
        }
        if !echo {
            return 0;
        }
        if status().at("phase").str_of() != Some("observing") {
            return 0;
        }
        if self.peer_feeds.load(Ordering::Acquire) >= 120 {
            return 0;
        }
        self.peer_feeds.fetch_add(1, Ordering::AcqRel);
        let mut fed = 0u64;
        let Some(tee) = self.tee_handle() else { return 0 };
        if let Some(payload) =
            self.posctl_payload.lock().unwrap_or_else(|e| e.into_inner()).clone()
        {
            tee.on_tun_frame(
                TunFrameDir::FromOs,
                &ipv4_udp_frame(DEV, PEER, 40_000, self.posctl_port, &payload),
            );
            tee.on_tun_frame(
                TunFrameDir::ToOs,
                &ipv4_udp_frame(PEER, DEV, self.posctl_port, 40_000, &payload),
            );
            fed += 2;
        }
        if let Some(id) = self.b2a.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            tee.on_tun_frame(
                TunFrameDir::ToOs,
                &ipv4_udp_frame(PEER, DEV, self.posctl_port, 40_000, id.as_bytes()),
            );
            fed += 1;
        }
        fed
    }

    /// Every `N2H-` marker the harness sinks have seen (drains them first).
    fn observed_marker(&self, prefix: &str) -> Option<Vec<u8>> {
        let mut sinks = self.sinks.lock().unwrap_or_else(|e| e.into_inner());
        sinks.drain_endpoints();
        let marker = WireMarker::Ascii(prefix.to_string());
        for found in [
            sinks.mgmt.observed(&marker),
            sinks.signal.observed(&marker),
            sinks.relay.observed(&marker),
            sinks.stun.observed(&marker),
            sinks.dns.observed(&marker),
        ] {
            if let Some(rec) = found {
                return Some(rec.payload.clone());
            }
        }
        None
    }
}

fn receipt_fact(label: &str, rec: Option<&SinkRecord>) -> Option<nc::ReceiptFact> {
    rec.map(|r| nc::ReceiptFact {
        physical_src: r.src.to_string(),
        physical_src_source: "endpoint-reported".into(),
        method: "sink-record".into(),
        detail: format!("{label} received the probe payload ({} bytes)", r.payload.len()),
    })
}

impl nc::CollectorHost for OfflineHost {
    fn dataplane(&self) -> Result<nc::HostDataplane, String> {
        Ok(nc::HostDataplane {
            device_up: self.device_up.load(Ordering::Acquire),
            ready: self.ready.load(Ordering::Acquire),
        })
    }

    fn freeze_facts(&self) -> Result<nc::HostFreezeFacts, String> {
        Ok(self.facts.clone())
    }

    fn attach_tee(&self, tee: Arc<dyn TunFrameTee>) -> Result<(), String> {
        let counting = Arc::new(CountingTee {
            inner: tee,
            totals: Mutex::new(TunTeeTotals::default()),
        });
        *self.tee.lock().unwrap_or_else(|e| e.into_inner()) = Some(counting);
        Ok(())
    }

    fn detach_tee(&self) -> Result<(), String> {
        *self.tee.lock().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(())
    }

    fn paired_read(&self, at_mono: u64, at_unix: i64) -> Result<nc::PairedRead, String> {
        let read_index = self.paired_reads.fetch_add(1, Ordering::AcqRel);
        let totals = self.tee_handle().ok_or("no tee attached".to_string())?.totals();
        let skew = if read_index == 0 {
            0
        } else {
            self.after_skew_packets.load(Ordering::Acquire)
        };
        // the wg ledger mirrors the tee (the harness's substitute for the
        // device's core counters), then the fixture tampering is applied
        let wg = WgDeviceStats {
            rx_packets: totals.to_os_frames + skew,
            rx_bytes_to_tun: totals.to_os_bytes,
            tx_packets: totals.from_os_frames,
            tx_bytes: totals.from_os_bytes,
            ..WgDeviceStats::default()
        };
        Ok(nc::tee_fallback_read(&totals, wg, at_mono, at_unix))
    }

    fn peer_report(&self) -> nc::PeerReport {
        {
            let sinks = self.sinks.lock().unwrap_or_else(|e| e.into_inner());
            let mut buf = [0u8; 512];
            loop {
                match sinks.peer_outer.recv_from(&mut buf) {
                    Ok(_) => {
                        self.peer_drops.fetch_add(1, Ordering::AcqRel);
                    }
                    Err(_) => break,
                }
            }
        }
        nc::PeerReport {
            b2a_probe_id: self.b2a.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            unknown_peer_drops: Some(self.peer_drops.load(Ordering::Acquire)),
        }
    }

    fn probe_receipt(&self, _probe_id: &str, marker: &WireMarker) -> Option<nc::ReceiptFact> {
        if !self.receipts.load(Ordering::Acquire) {
            return None;
        }
        let mut sinks = self.sinks.lock().unwrap_or_else(|e| e.into_inner());
        sinks.drain_endpoints();
        if let Some(f) = receipt_fact("management-sink", sinks.mgmt.observed(marker)) {
            return Some(f);
        }
        if let Some(f) = receipt_fact("signal-sink", sinks.signal.observed(marker)) {
            return Some(f);
        }
        if let Some(f) = receipt_fact("relay-sink", sinks.relay.observed(marker)) {
            return Some(f);
        }
        if let Some(f) = receipt_fact("stun-sink", sinks.stun.observed(marker)) {
            return Some(f);
        }
        if let Some(f) = receipt_fact("dns-sink", sinks.dns.observed(marker)) {
            return Some(f);
        }
        None
    }
}

// ---------------------------------------------------------------------------
// frames + the offline freeze/opts fixtures
// ---------------------------------------------------------------------------

/// The device's overlay address (descriptive: the fed frames are synthesized).
const DEV: [u8; 4] = [10, 77, 0, 1];
/// The counterparty's overlay address. The a2b probe is SENT to
/// `peer_overlay[0]:posctl_port`, so offline it must be a literal the harness
/// can actually receive on: loopback, shared by the in-process echo peer.
const PEER: [u8; 4] = [127, 0, 0, 1];
const B2A_ID: &str = "N2H-posctl-aaaabbbbccccddddeeeeffff";

fn ipv4_udp_frame(src: [u8; 4], dst: [u8; 4], sport: u16, dport: u16, payload: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(28 + payload.len());
    p.extend_from_slice(&[0x45, 0x00]);
    p.extend_from_slice(&((20 + 8 + payload.len()) as u16).to_be_bytes());
    p.extend_from_slice(&[0, 1, 0, 0, 64, 17, 0, 0]);
    p.extend_from_slice(&src);
    p.extend_from_slice(&dst);
    p.extend_from_slice(&sport.to_be_bytes());
    p.extend_from_slice(&dport.to_be_bytes());
    p.extend_from_slice(&((8 + payload.len()) as u16).to_be_bytes());
    p.extend_from_slice(&[0, 0]);
    p.extend_from_slice(payload);
    p
}

fn injected(host: &str, addr: SocketAddr) -> nc::InjectedEndpoint {
    nc::InjectedEndpoint::with_resolved(host, addr.port(), &addr.to_string())
}

/// The core-session freeze facts an offline device session has: relay (TCP
/// sink), STUN (UDP sink answering Binding Requests), DNS (UDP sink), the WG
/// peer's outer endpoint (the counterparty's outer socket), the peer's overlay
/// address (loopback — the a2b positive control is a real datagram to it) and
/// this device's overlay address. management/signal ride the SHELL exclusion
/// set instead (D7) — the shell-exclusion ∪ core union the design asks for.
/// `turn` carries a URI so the document shows the normative reasoned absence
/// (§4.0).
fn offline_facts(addrs: &SinkAddrs, with_signal: bool) -> nc::HostFreezeFacts {
    nc::HostFreezeFacts {
        management_dial: None,
        signal_config: if with_signal {
            Some(injected("signal.example", addrs.signal))
        } else {
            None
        },
        relay_advertised: vec![injected("relay.example", addrs.relay)],
        stun_uris: vec![injected("stun.example", addrs.stun)],
        turn_uris: vec![nc::InjectedEndpoint::new("turn.example", 3478)],
        wg_peer_endpoints: vec![injected("198.51.100.4", addrs.peer_outer)],
        dns_servers: vec![injected("10.0.0.53", addrs.dns)],
        peer_overlay: vec![PEER],
        own_overlay: Some(DEV),
    }
}

/// The arm options: the shell's own exclusion resolution (C2's artifact) for
/// management/signal, the documented window knobs, and the peer overlay port
/// the harness's echo peer answers on.
fn offline_opts(addrs: &SinkAddrs, with_signal: bool, probe_wait_ms: u64) -> String {
    let signal = if with_signal {
        format!(
            ",{{\"kind\":\"signal\",\"host\":\"signal.example\",\"port\":{},\"resolved\":\"{}\"}}",
            addrs.signal.port(),
            addrs.signal
        )
    } else {
        String::new()
    };
    format!(
        "{{\"shell_exclusions\":[{{\"kind\":\"management\",\"host\":\"mgmt.example\",\
         \"port\":{},\"resolved\":\"{}\"}}{signal}],\"posctl_port\":{},\"probe_wait_ms\":{probe_wait_ms}}}",
        addrs.mgmt.port(),
        addrs.mgmt,
        addrs.peer_overlay.port()
    )
}

/// Arm and assert the §5.1 success shape.
fn arm_ok(host: &Arc<OfflineHost>, opts: &str) -> V {
    let answer = parse_json(&nc::arm_json(opts));
    assert_eq!(answer.at("ok").bool_of(), Some(true), "arm answer: {answer:?}");
    assert_eq!(answer.at("armed").bool_of(), Some(true));
    assert!(answer.at("opened_mono").uint_of().unwrap_or(0) > 0, "opened_mono: {answer:?}");
    let _ = host;
    answer
}

fn status() -> V {
    parse_json(&nc::status_json())
}

fn wait_closed(host: &Arc<OfflineHost>, echo: bool, fuse_ms: u64) -> V {
    wait_until(fuse_ms, "the collector window closes", || {
        host.pump_peer(echo);
        status().at("phase").str_of() == Some("closed")
    });
    status()
}

/// The closed window's document, reassembled from the NAPI chunk surface
/// (the host-side step of §2.4: join by seq, verify the full-document digest).
fn closed_document() -> (V, String) {
    let head = parse_json(&nc::evidence_json(-1));
    let total = head.at("total").uint_of().expect("total is a number") as usize;
    assert!(total >= 1, "a closed window always has at least one chunk");
    assert_eq!(head.at("seq").num_of(), Some("-1"));
    assert_eq!(head.at("b64").str_of(), Some(""));
    let mut lines = Vec::new();
    for i in 0..total {
        let chunk = parse_json(&nc::evidence_json(i as i64));
        let seq = chunk.at("seq").uint_of().unwrap() as usize;
        assert_eq!(seq, i);
        assert_eq!(chunk.at("total").uint_of().unwrap() as usize, total);
        assert_eq!(
            chunk.at("sha256").str_of(),
            head.at("sha256").str_of(),
            "every line declares the full-document digest"
        );
        lines.push(format!(
            "N2H_EVIDENCE|seq={i}/{total}|sha256={}|b64={}",
            chunk.at("sha256").str_of().unwrap(),
            chunk.at("b64").str_of().unwrap()
        ));
    }
    let refs: Vec<&str> = lines.iter().map(|l| l.as_str()).collect();
    let raw = nc::decode_evidence(&refs).expect("the chunk lines reassemble");
    let text = String::from_utf8(raw).expect("the document is UTF-8");
    (parse_json(&text), text)
}

// ---------------------------------------------------------------------------
// scenario 1 — the clean run, and the schema walk against the host document
// ---------------------------------------------------------------------------

#[test]
fn clean_device_window_passes_and_matches_the_host_document_schema() {
    let _guard = serial();
    show_panics();
    reset_collector();
    let sinks = SinkSet::bind();
    let addrs = sinks.addrs();
    let host = OfflineHost::new(offline_facts(&addrs, true), sinks, true);
    host.announce_b2a(B2A_ID);
    nc::install_host(host.clone() as Arc<dyn nc::CollectorHost>);

    let answer = arm_ok(&host, &offline_opts(&addrs, true, 300));
    assert!(answer.at("opened_mono").uint_of().unwrap() > 0);

    // while the window runs: the §5.1 running shapes
    let running = parse_json(&nc::evidence_json(0));
    assert_eq!(running.at("phase").str_of(), Some("running"));
    let st = status();
    assert_eq!(st.at("ok").bool_of(), Some(true));
    assert_eq!(st.at("armed").bool_of(), Some(true));
    assert!(
        ["armed", "probing", "observing"].contains(&st.at("phase").str_of().unwrap()),
        "phase: {st:?}"
    );
    assert!(st.at("window").at("opened_mono").uint_of().is_some());
    assert!(st.at("window").at("deadline_mono").uint_of().is_some());

    // a second arm while the window is open is refused (§5.1)
    let again = parse_json(&nc::arm_json(&offline_opts(&addrs, true, 300)));
    assert_eq!(again.at("error").str_of(), Some("already-armed"));

    // benign window frames (both directions) so the scan is non-empty
    let mut benign_fed = false;
    wait_until(30_000, "the clean window closes with both controls observed", || {
        host.pump_peer(true);
        if !benign_fed {
            if let Some(tee) = host.tee_handle() {
                tee.on_tun_frame(
                    TunFrameDir::FromOs,
                    &ipv4_udp_frame(DEV, PEER, 11_111, 22_222, b"clean-window-from-os"),
                );
                tee.on_tun_frame(
                    TunFrameDir::ToOs,
                    &ipv4_udp_frame(PEER, DEV, 22_222, 11_111, b"clean-window-to-os"),
                );
                benign_fed = true;
            }
        }
        status().at("phase").str_of() == Some("closed")
    });

    let st = status();
    assert_eq!(st.at("armed").bool_of(), Some(false));
    assert_eq!(st.at("verdict").str_of(), Some(VERDICT_PASS), "status: {st:?}");
    assert!(st.at("frames_scanned").uint_of().unwrap() >= 5);
    assert_eq!(st.at("hits").uint_of(), Some(0));
    assert!(st.at("window").at("closed_mono").uint_of().unwrap() > 0);

    let (doc, text) = closed_document();
    assert_eq!(doc.at("tool").str_of(), Some("n2h-isolation-evidence"));
    assert_eq!(doc.at("schema").uint_of(), Some(1));
    assert_eq!(doc.at("mode").str_of(), Some("device"), "D1");
    assert_eq!(doc.at("verdict").str_of(), Some(VERDICT_PASS), "reasons: {:?}", doc.at("reasons"));
    // B-5: the literal single-interface enumeration
    assert_eq!(doc.at("tun_negative").at("scanned_sides").arr_of().len(), 1);
    assert_eq!(doc.at("tun_negative").at("scanned_sides").arr_of()[0].str_of(), Some("vpn-tun"));
    assert_eq!(doc.at("tun_negative").at("hit").bool_of(), Some(false));
    assert!(doc.at("tun_negative").at("frames_scanned").uint_of().unwrap() >= 5);
    // D8: ONE counters element on the platform interface, reconciled
    assert_eq!(doc.at("counters").arr_of().len(), 1);
    let counters = &doc.at("counters").arr_of()[0];
    assert_eq!(counters.at("side").str_of(), Some("vpn-tun"));
    assert_eq!(counters.at("reconciled").bool_of(), Some(true));
    assert!(counters.at("delta").at("tun").at("delivered_bytes").num_of().is_some());
    // the design §3.2 caliber is DECLARED
    let notes: Vec<&str> = doc.at("notes").arr_of().iter().filter_map(|n| n.str_of()).collect();
    assert!(notes.contains(&"counters-source=tee-fallback"), "notes: {notes:?}");
    // both positive-control directions observed, a2b through BOTH points
    assert_eq!(doc.at("tunnel_positive_control").at("observed_in_tunnel").bool_of(), Some(true));
    let posctl = doc.at("tunnel_positive_control").at("probes").arr_of();
    let directions: Vec<&str> = posctl.iter().filter_map(|p| p.at("direction").str_of()).collect();
    assert!(directions.contains(&"a2b"), "{directions:?}");
    assert!(directions.contains(&"b2a"), "{directions:?}");
    for p in posctl {
        assert_eq!(p.at("sent").bool_of(), Some(true), "posctl {p:?}");
        assert!(p.at("observed_in_tunnel").get("side").is_some(), "observed hit: {p:?}");
        assert_eq!(p.at("expected_path").str_of(), Some("tunnel"));
    }
    // the freeze covers every class, with the normative turn absence
    let kinds: Vec<&str> = doc
        .at("frozen_endpoints")
        .arr_of()
        .iter()
        .filter_map(|f| f.at("kind").str_of())
        .collect();
    for kind in EndpointKind::ALL {
        assert!(kinds.contains(&kind.name()), "kind {} not frozen: {kinds:?}", kind.name());
    }
    let frozen = doc.at("frozen_endpoints").arr_of();
    let turn = frozen.iter().find(|f| f.at("kind").str_of() == Some("turn")).unwrap();
    assert_eq!(turn.at("present").bool_of(), Some(false));
    assert!(!turn.at("absent_reason").str_of().unwrap().is_empty());
    // every present endpoint was probed and every sent probe has a receipt
    for f in frozen.iter().filter(|f| f.at("present").bool_of() == Some(true)) {
        let kind = f.at("kind").str_of().unwrap();
        assert!(
            doc.at("probes").arr_of().iter().any(|p| p.at("kind").str_of() == Some(kind)),
            "no probe for {kind}"
        );
    }
    for p in doc.at("probes").arr_of() {
        assert_eq!(p.at("sent").bool_of(), Some(true), "probe not sent: {p:?}");
        let id = p.at("probe_id").str_of().unwrap();
        assert!(
            doc.at("endpoint_side")
                .arr_of()
                .iter()
                .any(|r| r.at("probe_id").str_of() == Some(id)
                    && r.at("observed").bool_of() == Some(true)),
            "no observed receipt for {id}"
        );
    }

    // ---- §8-1's shared assertion: the SAME key walk on both documents ----
    // (both runs are clean: no hits, no reasons — so the walks are comparable)
    let host_doc =
        n2h::run_loopback_check(&LoopbackOpts::default()).expect("host loopback run");
    // boringtun's ffi re-installs its own panic hook when the loopback run
    // creates its tunnels: put the printing hook back so a failure below is
    // readable instead of being swallowed
    show_panics();
    assert_eq!(host_doc.verdict, VERDICT_PASS, "the host reference must be a pass");
    let host_json = host_doc.to_json();
    let device_schema = schema_of(&doc);
    let host_schema = schema_of(&parse_json(&host_json));
    assert_same_schema(&device_schema, &host_schema);
    println!(
        "[I5-1] clean run: {} bytes doc, {} chunks, verdict={}, frames_scanned={}, \
         schema paths={} vs host {} (identical types on every shared path)",
        text.len(),
        nc::encode_evidence(text.as_bytes()).len(),
        doc.at("verdict").str_of().unwrap(),
        doc.at("tun_negative").at("frames_scanned").uint_of().unwrap(),
        device_schema.len(),
        host_schema.len()
    );
}

// ---------------------------------------------------------------------------
// scenario 2 — a leaked outer probe payload in the tunnel ⇒ n2h-fail
// ---------------------------------------------------------------------------

#[test]
fn an_outer_probe_payload_in_the_tunnel_is_fail() {
    let _guard = serial();
    reset_collector();
    let sinks = SinkSet::bind();
    let addrs = sinks.addrs();
    let host = OfflineHost::new(offline_facts(&addrs, true), sinks, true);
    host.announce_b2a(B2A_ID);
    nc::install_host(host.clone() as Arc<dyn nc::CollectorHost>);
    arm_ok(&host, &offline_opts(&addrs, true, 300));

    // the management probe's own payload, straight off the harness sink
    let mut leaked: Option<String> = None;
    let mut benign_fed = false;
    wait_until(30_000, "the management probe is sent and replayed into the TUN", || {
        host.pump_peer(true);
        if !benign_fed {
            if let Some(tee) = host.tee_handle() {
                tee.on_tun_frame(
                    TunFrameDir::ToOs,
                    &ipv4_udp_frame(PEER, DEV, 3333, 4444, b"benign"),
                );
                benign_fed = true;
            }
        }
        if leaked.is_none() {
            if let Some(payload) = host.observed_marker("N2H-management-") {
                let tee = host.tee_handle().expect("tee attached");
                // 反例: the outer probe's payload crosses the TUN in the
                // from-os direction (a genuine route leak, design §4.1)
                tee.on_tun_frame(
                    TunFrameDir::FromOs,
                    &ipv4_udp_frame(DEV, PEER, 40_000, 40_001, &payload),
                );
                leaked = Some(String::from_utf8(payload).expect("probe id is ASCII"));
            }
        }
        status().at("phase").str_of() == Some("closed")
    });

    let id = leaked.expect("the leak was injected");
    assert!(id.starts_with("N2H-management-"), "{id}");
    let st = status();
    assert_eq!(st.at("verdict").str_of(), Some(VERDICT_FAIL), "status: {st:?}");
    assert_eq!(st.at("hits").uint_of(), Some(1));
    let (doc, _text) = closed_document();
    assert_eq!(doc.at("verdict").str_of(), Some(VERDICT_FAIL), "reasons: {:?}", doc.at("reasons"));
    assert_eq!(doc.at("tun_negative").at("hit").bool_of(), Some(true));
    let hits = doc.at("tun_negative").at("hits").arr_of();
    assert_eq!(hits.len(), 1, "exactly the injected leak");
    assert_eq!(hits[0].at("probe_id").str_of(), Some(id.as_str()));
    assert_eq!(hits[0].at("side").str_of(), Some("vpn-tun"), "D3/D4's single interface");
    let reasons: Vec<&str> =
        doc.at("reasons").arr_of().iter().filter_map(|r| r.str_of()).collect();
    assert!(
        reasons.iter().any(|r| r.contains(&id)),
        "the fail reason names the leaked probe: {reasons:?}"
    );
    // D4's direction fact (the frozen renderer carries no direction field)
    let notes: Vec<&str> = doc.at("notes").arr_of().iter().filter_map(|n| n.str_of()).collect();
    assert!(
        notes.iter().any(|n| n.contains(&format!("probe_id={id}")) && n.contains("direction=from-os")),
        "notes: {notes:?}"
    );
    println!("[I5-2] leak detected: id={id}, verdict={}, hits={}", doc.at("verdict").str_of().unwrap(), hits.len());
}

// ---------------------------------------------------------------------------
// scenario 3 — the echo peer never answers ⇒ n2h-inconclusive (never a pass)
// ---------------------------------------------------------------------------

#[test]
fn a_missing_positive_control_is_inconclusive() {
    let _guard = serial();
    reset_collector();
    let sinks = SinkSet::bind();
    let addrs = sinks.addrs();
    let host = OfflineHost::new(offline_facts(&addrs, true), sinks, true);
    host.announce_b2a(B2A_ID);
    nc::install_host(host.clone() as Arc<dyn nc::CollectorHost>);
    arm_ok(&host, &offline_opts(&addrs, true, 200));

    // the peer records the a2b datagram (so the probe DID leave) and never
    // echoes; no b2a payload is ever delivered either. Benign window frames
    // keep the scan non-empty, so the ONLY gap is the control itself.
    let tee = host.tee_handle().expect("the armed window holds the tee");
    tee.on_tun_frame(
        TunFrameDir::FromOs,
        &ipv4_udp_frame(DEV, PEER, 55_555, 45_555, b"benign-from-os"),
    );
    tee.on_tun_frame(
        TunFrameDir::ToOs,
        &ipv4_udp_frame(PEER, DEV, 45_555, 55_555, b"benign-to-os"),
    );
    let st = wait_closed(&host, false, 20_000);
    assert_eq!(st.at("phase").str_of(), Some("closed"));
    assert_eq!(st.at("verdict").str_of(), Some(VERDICT_INCONCLUSIVE), "status: {st:?}");
    let (doc, _text) = closed_document();
    assert_eq!(doc.at("verdict").str_of(), Some(VERDICT_INCONCLUSIVE));
    assert_eq!(doc.at("tunnel_positive_control").at("observed_in_tunnel").bool_of(), Some(false));
    for p in doc.at("tunnel_positive_control").at("probes").arr_of() {
        assert_eq!(p.at("sent").bool_of(), Some(true), "the control was sent: {p:?}");
        assert_eq!(p.at("observed_in_tunnel").str_of(), None, "never observed: {p:?}");
    }
    let reasons: Vec<&str> =
        doc.at("reasons").arr_of().iter().filter_map(|r| r.str_of()).collect();
    assert!(
        reasons.iter().any(|r| r.contains("never observed")),
        "reason explains the missing observation: {reasons:?}"
    );
    // the negative scan is still clean — inconclusive is about the CONTROL
    assert_eq!(doc.at("tun_negative").at("hit").bool_of(), Some(false));
    assert!(doc.at("tun_negative").at("frames_scanned").uint_of().unwrap() > 0);
    assert!(
        !reasons.iter().any(|r| r.contains("zero frames scanned")),
        "the scan was not empty: {reasons:?}"
    );
    println!("[I5-3] no echo → verdict={}, reasons={}", doc.at("verdict").str_of().unwrap(), reasons.len());
}

// ---------------------------------------------------------------------------
// scenario 4 — a freeze gap (signal) ⇒ n2h-inconclusive naming 'signal'
// ---------------------------------------------------------------------------

#[test]
fn a_missing_signal_freeze_is_inconclusive_and_names_the_kind() {
    let _guard = serial();
    reset_collector();
    let sinks = SinkSet::bind();
    let addrs = sinks.addrs();
    let host = OfflineHost::new(offline_facts(&addrs, false), sinks, true);
    host.announce_b2a(B2A_ID);
    nc::install_host(host.clone() as Arc<dyn nc::CollectorHost>);
    arm_ok(&host, &offline_opts(&addrs, false, 200));

    wait_closed(&host, true, 20_000);
    let (doc, _text) = closed_document();
    assert_eq!(doc.at("verdict").str_of(), Some(VERDICT_INCONCLUSIVE), "reasons: {:?}", doc.at("reasons"));
    let signal = doc
        .at("frozen_endpoints")
        .arr_of()
        .iter()
        .find(|f| f.at("kind").str_of() == Some("signal"))
        .expect("the gap is recorded, not omitted")
        .clone();
    assert_eq!(signal.at("present").bool_of(), Some(false));
    assert!(!signal.at("absent_reason").str_of().unwrap().is_empty());
    assert_eq!(signal.at("host").str_of(), Some(""));
    let reasons: Vec<&str> =
        doc.at("reasons").arr_of().iter().filter_map(|r| r.str_of()).collect();
    assert!(
        reasons.iter().any(|r| r.contains("'signal'")),
        "the missing kind is named: {reasons:?}"
    );
    assert!(
        !doc.at("probes").arr_of().iter().any(|p| p.at("kind").str_of() == Some("signal")),
        "no probe for an unfrozen kind"
    );
    println!("[I5-4] freeze gap: verdict={}, signal reason='{}'", doc.at("verdict").str_of().unwrap(), signal.at("absent_reason").str_of().unwrap());
}

// ---------------------------------------------------------------------------
// scenario 5 — a pairing the frozen byte identity accepts and design §4.4's
// packet identity refuses ⇒ n2h-inconclusive, decided by the COLLECTOR
// ---------------------------------------------------------------------------

#[test]
fn a_packet_identity_break_the_judge_cannot_see_is_inconclusive() {
    let _guard = serial();
    reset_collector();
    let sinks = SinkSet::bind();
    let addrs = sinks.addrs();
    let host = OfflineHost::new(offline_facts(&addrs, true), sinks, true);
    host.announce_b2a(B2A_ID);
    // every wg-ledger read AFTER the window-open edge is one frame ahead of
    // the kernel ledger: bytes stay equal, packets do not
    host.skew_after_first_read(1);
    nc::install_host(host.clone() as Arc<dyn nc::CollectorHost>);
    arm_ok(&host, &offline_opts(&addrs, true, 200));
    // the window thread takes the clean window-open edge first
    wait_until(5_000, "the window-open counters edge is taken", || host.paired_read_count() >= 1);

    wait_closed(&host, true, 20_000);
    let (doc, _text) = closed_document();
    assert_eq!(doc.at("verdict").str_of(), Some(VERDICT_INCONCLUSIVE), "reasons: {:?}", doc.at("reasons"));
    // the frozen judge's own view: bytes-only, so it reports reconciled
    let counters = &doc.at("counters").arr_of()[0];
    assert_eq!(
        counters.at("reconciled").bool_of(),
        Some(true),
        "WindowCounters::reconciled is bytes-only — the judge alone would pass this pairing"
    );
    let delta = counters.at("delta");
    assert_eq!(
        delta.at("wg").at("rx_bytes_to_tun").num_of(),
        delta.at("tun").at("delivered_bytes").num_of(),
        "the byte identity holds on the recorded delta"
    );
    assert_ne!(
        delta.at("wg").at("rx_packets").num_of(),
        delta.at("tun").at("delivered_frames").num_of(),
        "the packet identity is the one that broke"
    );
    let reasons: Vec<&str> =
        doc.at("reasons").arr_of().iter().filter_map(|r| r.str_of()).collect();
    assert!(
        reasons.iter().any(|r| r.contains("design §4.4 second identity")),
        "the collector states the break itself: {reasons:?}"
    );
    let notes: Vec<&str> = doc.at("notes").arr_of().iter().filter_map(|n| n.str_of()).collect();
    assert!(notes.contains(&"counters-identity=packets-broken"), "notes: {notes:?}");
    assert!(
        notes.contains(&format!("counters-pairing-resamples={}", nc::MAX_PAIRING_RESAMPLES).as_str()),
        "the bounded resample was spent: {notes:?}"
    );
    assert_eq!(doc.at("tun_negative").at("hit").bool_of(), Some(false));
    println!(
        "[I5-5] packet-identity break: verdict={}, counters.reconciled={}, resamples={}",
        doc.at("verdict").str_of().unwrap(),
        counters.at("reconciled").bool_of().unwrap(),
        nc::MAX_PAIRING_RESAMPLES
    );
}

// ---------------------------------------------------------------------------
// scenario 6 — the N2H_EVIDENCE chunk transport: roundtrip + the two failures
// ---------------------------------------------------------------------------

#[test]
fn chunk_transport_roundtrips_and_refuses_missing_or_mutated_lines() {
    let _guard = serial();
    reset_collector();
    let sinks = SinkSet::bind();
    let addrs = sinks.addrs();
    let host = OfflineHost::new(offline_facts(&addrs, true), sinks, true);
    host.announce_b2a(B2A_ID);
    nc::install_host(host.clone() as Arc<dyn nc::CollectorHost>);
    arm_ok(&host, &offline_opts(&addrs, true, 200));
    wait_closed(&host, true, 20_000);

    let (doc, text) = closed_document();
    assert_eq!(doc.at("mode").str_of(), Some("device"));
    let raw = text.as_bytes();
    let digest = netbird_core::util::sha256_hex(raw);
    let head = parse_json(&nc::evidence_json(-1));
    assert_eq!(head.at("sha256").str_of(), Some(digest.as_str()));
    let chunks = nc::encode_evidence(raw);
    let total = chunks.len();
    assert_eq!(head.at("total").uint_of().unwrap() as usize, total);

    // every NAPI chunk equals the encoder's own chunk for that index
    for (i, expected) in chunks.iter().enumerate() {
        let got = parse_json(&nc::evidence_json(i as i64));
        assert_eq!(got.at("seq").uint_of().unwrap() as usize, expected.seq);
        assert_eq!(got.at("sha256").str_of(), Some(expected.sha256.as_str()));
        assert_eq!(got.at("b64").str_of(), Some(expected.b64.as_str()));
    }
    // out of range is refused, never padded
    let oob = parse_json(&nc::evidence_json(total as i64));
    assert_eq!(oob.at("error").str_of(), Some("chunk-out-of-range"));

    // a dropped middle chunk is DETECTED (hilog line loss)
    let lines: Vec<String> = chunks.iter().map(|c| c.line()).collect();
    if total >= 2 {
        let mut missing: Vec<&str> = lines.iter().map(|l| l.as_str()).collect();
        missing.remove(1);
        assert_eq!(
            nc::decode_evidence(&missing).unwrap_err(),
            nc::ChunkError::Missing { seq: 1 }
        );
    } else {
        assert_eq!(nc::decode_evidence(&[]).unwrap_err(), nc::ChunkError::NoChunks);
    }
    // a mutated payload byte is refused by the declared full-document digest
    let mut tampered = lines[lines.len() - 1].clone();
    let b64_at = tampered.find("|b64=").expect("b64 field") + "|b64=".len();
    if tampered.len() > b64_at {
        let flipped = if tampered.as_bytes()[b64_at] == b'A' { 'B' } else { 'A' };
        tampered.replace_range(b64_at..b64_at + 1, &flipped.to_string());
        let refs: Vec<&str> = if total >= 2 {
            let mut v: Vec<&str> = lines[..lines.len() - 1].iter().map(|l| l.as_str()).collect();
            v.push(tampered.as_str());
            v
        } else {
            vec![tampered.as_str()]
        };
        assert!(
            matches!(
                nc::decode_evidence(&refs).unwrap_err(),
                nc::ChunkError::Sha256Mismatch { .. } | nc::ChunkError::DigestDisagreement { .. }
            ),
            "a mutated payload must never reassemble"
        );
    }
    // a re-serve is the replay: the same index answers byte-identically (§2.4-R2)
    let again = nc::evidence_json(0);
    assert_eq!(nc::evidence_json(0), again);
    // …and a FINALIZED window survives disarm (only an unclosed product is
    // discarded): the replay surface stays available until the next arm
    assert_eq!(parse_json(&nc::disarm_json()).at("ok").bool_of(), Some(true));
    let kept = parse_json(&nc::evidence_json(-1));
    assert_eq!(
        kept.at("sha256").str_of(),
        Some(digest.as_str()),
        "the closed document stays replayable after disarm"
    );
    assert_eq!(status().at("armed").bool_of(), Some(false));
    println!(
        "[I5-6] chunk transport: doc={} bytes, chunks={}, sha256={}…, missing/mutated lines refused, \
         finalized document retained across disarm",
        raw.len(),
        total,
        &digest[..16]
    );
}

// ---------------------------------------------------------------------------
// the arm gate: fail-closed without a ready session (§2.2), no window left
// ---------------------------------------------------------------------------

#[test]
fn the_arm_gate_is_fail_closed_on_an_unready_session() {
    let _guard = serial();
    reset_collector();
    let sinks = SinkSet::bind();
    let addrs = sinks.addrs();
    let host = OfflineHost::new(offline_facts(&addrs, true), sinks, true);
    let opts = offline_opts(&addrs, true, 200);

    // no host seam at all
    let answer = parse_json(&nc::arm_json(&opts));
    assert_eq!(answer.at("error").str_of(), Some("no-connector"));

    // host present, device not fed
    host.device_up.store(false, Ordering::Release);
    nc::install_host(host.clone() as Arc<dyn nc::CollectorHost>);
    let answer = parse_json(&nc::arm_json(&opts));
    assert_eq!(answer.at("error").str_of(), Some("device-not-up"));

    // device up, no established session
    host.device_up.store(true, Ordering::Release);
    host.ready.store(false, Ordering::Release);
    let answer = parse_json(&nc::arm_json(&opts));
    assert_eq!(answer.at("error").str_of(), Some("tunnel-not-ready"));
    // a refused arm attaches no tee, i.e. no window can have started (the
    // "the registry slot stays empty" half is pinned by the crate unit test,
    // which can inspect the slot directly)
    assert!(host.tee_handle().is_none(), "a refused arm must not attach a tee");

    // a ready session arms; a disarm of the OPEN window detaches the tee and
    // discards it (scenario 6 pins the other branch: a window that already
    // CLOSED is retained for replay)
    host.ready.store(true, Ordering::Release);
    arm_ok(&host, &opts);
    assert!(host.tee_handle().is_some(), "the armed window holds the tee");
    let disarmed = parse_json(&nc::disarm_json());
    assert_eq!(disarmed.at("ok").bool_of(), Some(true));
    assert!(host.tee_handle().is_none(), "disarm detaches synchronously");
    assert_eq!(status().at("error").str_of(), Some("not-armed"));
    assert_eq!(parse_json(&nc::evidence_json(-1)).at("error").str_of(), Some("no-evidence"));
    println!("[I5-arm] gate: no-connector → device-not-up → tunnel-not-ready → armed → disarmed");
}

// ---------------------------------------------------------------------------
// the PRODUCTION host (`WgFeedHost`, design U9) against a REAL WgDeviceFeed
// ---------------------------------------------------------------------------

#[test]
fn the_production_wg_feed_host_gates_the_arm_and_scans_real_tun_frames() {
    let _guard = serial();
    show_panics();
    reset_collector();
    let wg_raw = host_sockets::open_udp_ephemeral().expect("wg socket");
    let (tun_raw, tun_hand) = host_sockets::open_tun_standby_pair().expect("TUN stand-in");
    let mut bag = FdBag::new();
    bag.keep(wg_raw);
    bag.keep(tun_raw);
    bag.keep(tun_hand);
    let mut cfg = WgDeviceConfig::new(netbird_core::util::base64(&[0xA5u8; 32]));
    cfg.keepalive_ms = u64::MAX / 2;
    let feed = Arc::new(WgDeviceFeed::new(cfg));
    let host: Arc<nc::WgFeedHost> =
        Arc::new(nc::WgFeedHost::new(feed.clone()).with_hdc_capture_path(None));
    nc::install_host(host.clone() as Arc<dyn nc::CollectorHost>);

    // no feed at all: the production host reports the truth, fail-closed
    let answer = parse_json(&nc::arm_json("{}"));
    assert_eq!(answer.at("error").str_of(), Some("device-not-up"));

    feed.feed_wg_socket(wg_raw).expect("wg socket feed");
    feed.feed_tun(tun_raw).expect("tun feed");
    assert!(feed.device_up(), "the real device is up after both feeds");
    // device up, NO established session ⇒ tunnel-not-ready (fail-closed §2.2)
    let answer = parse_json(&nc::arm_json("{}"));
    assert_eq!(answer.at("error").str_of(), Some("tunnel-not-ready"));

    // the tee attaches THROUGH the feed (design U9's production side) and
    // observes the real TUN frames the device reads
    let tee = Arc::new(nc::CollectorTee::new(nc::system_clock()));
    nc::CollectorHost::attach_tee(&*host, tee.clone() as Arc<dyn TunFrameTee>)
        .expect("tee attach through the feed");
    assert_eq!(feed.with_device(|d| d.tun_tee_attached()), Some(true));

    let marker = "N2H-management-0123456789abcdef01234567";
    let frame = ipv4_udp_frame(DEV, PEER, 40_000, 40_001, marker.as_bytes());
    let (n, errno) = sys::write_fd(tun_hand, &frame);
    assert_eq!((n as usize, errno), (frame.len(), 0), "TUN-side write");
    let now = sys::mono_ms();
    let _ = feed.with_device(|d| {
        d.service_tun(now);
    });
    wait_until(5_000, "the real device reads the frame into the collector tee", || {
        tee.frames_scanned() > 0
    });
    assert_eq!(tee.totals().from_os_frames, 1);
    // the marker was registered as an outer probe: the hit IS a violation
    tee.register(marker, WireMarker::Ascii(marker.to_string()), false);
    let (n2, errno2) = sys::write_fd(tun_hand, &frame);
    assert_eq!((n2 as usize, errno2), (frame.len(), 0));
    let now = sys::mono_ms();
    let _ = feed.with_device(|d| {
        d.service_tun(now);
    });
    wait_until(5_000, "the second frame is scanned as an outer hit", || {
        tee.outer_hit_count() > 0
    });
    // the device caliber has no `/proc` row here: the counters gap is an
    // honest Err (the collector then records it), never a fabricated pairing
    let read = nc::CollectorHost::paired_read(&*host, now, 1_756_000_000);
    assert!(read.is_err(), "no vpn-tun row in this process: {read:?}");
    // detach is idempotent through the feed
    nc::CollectorHost::detach_tee(&*host).expect("detach");
    nc::CollectorHost::detach_tee(&*host).expect("detach is idempotent");
    assert_eq!(feed.with_device(|d| d.tun_tee_attached()), Some(false));
    println!(
        "[I5-prod] WgFeedHost: gate device-not-up → tunnel-not-ready; tee saw {} frame(s), {} hit(s)",
        tee.frames_scanned(),
        tee.outer_hit_count()
    );
}

// ---------------------------------------------------------------------------
// the PRODUCTION wiring (§7-I5 gap): a started connector installs the seam
// ---------------------------------------------------------------------------

/// The wiring gap this increment closes: before `connector.rs` called
/// `install_collector_host`, NOTHING installed the host seam in the shipped
/// process, so a real device round answered `no-connector` forever and the
/// window could not be opened at all.
///
/// Proven here against the REAL production start entry
/// (`connector_start_with_socket`, N3-7) and the REAL connector singleton:
/// ① a started connector installs the production host (and the arm gate then
///    reports the feed's state instead of `no-connector`);
/// ② the host freezes MANAGEMENT from the connector's INTERNAL dial address —
///    a value neither public document carries (§5.2 contract intact);
/// ③ stun has no sync-delivered `netbird_config` in this process, so it must
///    be the honest REASONED absence, never an invented endpoint (the
///    populated internal read is unit-tested in `crate::connector` and
///    `crate::n2h_collector::tests`);
/// ④ `connector_stop` removes the seam: its lifetime is the session's.
#[test]
fn a_started_connector_installs_the_production_host_with_internal_freeze_sources() {
    use netbird_core::connector::{
        connector_network_config_json, connector_start_with_socket_json, connector_status_json,
        connector_stop_json, core_freeze_inputs,
    };
    use std::os::fd::AsRawFd;

    let _guard = serial();
    show_panics();
    reset_collector();
    assert!(nc::installed_host().is_none(), "no connector ⇒ no host seam");
    assert_eq!(nc::arm_json(""), "{\"ok\":false,\"error\":\"no-connector\"}");

    // the shell-side shape: a resolved management address + a real (dup-able)
    // fd for the protected-socket start path
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let dial = listener.local_addr().expect("addr");
    let cfg = format!(
        "{{\"management_url\":\"http://{dial}\",\"private_key\":\"{}\"}}",
        netbird_core::util::base64(&[0x11u8; 32])
    );
    let started = connector_start_with_socket_json(
        listener.as_raw_fd(),
        &cfg,
        "{\"setup_key\":\"SETUP-N2H-WIRE\"}",
        &format!("{{\"connect_addr\":\"{dial}\"}}"),
    );
    assert!(started.contains("\"started\":true"), "start refused: {started}");

    // ① the production start path installed the seam (the §7-I5 gap itself)
    let host = nc::installed_host().expect("a started connector must install the collector host");
    assert_eq!(
        parse_json(&nc::arm_json("{}")).at("error").str_of(),
        Some("device-not-up"),
        "with the seam installed the gate reads the REAL feed (no shell-fed device yet)"
    );

    // ② the internal sources reach the host's freeze facts
    let core = core_freeze_inputs();
    assert_eq!(core.management_dial, Some(dial), "the factory's dial address");
    assert!(core.stuns.is_empty(), "no management Sync in this process");
    let facts = nc::CollectorHost::freeze_facts(&*host).expect("connector documents parse");
    assert_eq!(
        facts.management_dial.as_ref().map(|e| e.resolved.clone()),
        Some(dial.to_string()),
        "management must freeze from the INTERNAL dial address"
    );
    // §5.2: neither public document carries that internal value
    for (name, doc) in [
        ("connector_status", connector_status_json()),
        ("connector_network_config", connector_network_config_json()),
    ] {
        assert!(
            !doc.contains(&dial.to_string()),
            "{name} must keep its frozen shape (no internal dial address): {doc}"
        );
    }

    // ③ assembly: management present with its source token; stun a REASONED
    //    absence here (nothing to freeze is recorded, never fabricated)
    let frozen = nc::assemble_freezes(&nc::FreezeSources {
        management_dial: facts.management_dial.clone(),
        stun_uris: facts.stun_uris.clone(),
        frozen_at_unix: 1_756_000_000,
        frozen_at_mono: 42,
        ..nc::FreezeSources::default()
    });
    for kind in EndpointKind::ALL {
        assert!(
            frozen.iter().any(|f| f.kind == Some(kind)),
            "{kind:?} must be represented (present or reasoned absence)"
        );
    }
    let management = frozen
        .iter()
        .find(|f| f.kind == Some(EndpointKind::Management))
        .expect("management entry");
    assert!(management.present, "{management:?}");
    assert_eq!(management.source, nc::SOURCE_CORE_DIAL_ADDRESS);
    assert_eq!(management.resolved, dial.to_string());
    let stun = frozen.iter().find(|f| f.kind == Some(EndpointKind::Stun)).expect("stun entry");
    assert!(!stun.present, "no sync ⇒ no stun source to freeze: {stun:?}");
    assert!(!stun.absent_reason.is_empty(), "the gap must be reasoned: {stun:?}");

    // ④ stop removes the seam — no connector state outlives the session
    let stopped = connector_stop_json();
    assert!(stopped.contains("\"ok\":true"), "{stopped}");
    assert!(nc::installed_host().is_none(), "a stopped connector retains no host seam");
    assert_eq!(nc::arm_json(""), "{\"ok\":false,\"error\":\"no-connector\"}");
    println!(
        "[I5-wire] start ⇒ seam installed (arm gate=device-not-up); internal dial={dial}; \
         management freeze present source={}; stun absent='{}'; stop ⇒ seam removed",
        management.source, stun.absent_reason
    );
}

// ---------------------------------------------------------------------------
// the NAPI surface with no window and no host (§6.1/§5.1)
// ---------------------------------------------------------------------------

/// The never-armed shapes (`status` → `not-armed`, `evidence` → `no-evidence`)
/// are pinned by the crate unit test
/// `n2h_collector::tests::the_unarmed_napi_surface_is_inert_and_idempotent`,
/// which can clear the process slot directly. Here only the answers that are
/// independent of any earlier scenario's window are asserted: the options
/// refusal, the missing host seam, and disarm idempotency.
#[test]
fn the_napi_entries_answer_the_documented_shapes_without_a_window_or_host() {
    let _guard = serial();
    reset_collector();
    assert_eq!(nc::arm_json("{"), "{\"ok\":false,\"error\":\"invalid-opts\"}");
    assert_eq!(nc::arm_json("{\"posctl_port\":0}"), "{\"ok\":false,\"error\":\"invalid-opts\"}");
    assert_eq!(nc::arm_json(""), "{\"ok\":false,\"error\":\"no-connector\"}");
    assert_eq!(nc::disarm_json(), "{\"ok\":true}");
    assert_eq!(nc::disarm_json(), "{\"ok\":true}", "disarm is idempotent");
    println!(
        "[I5-unarmed] invalid-opts / no-connector / disarm={} (never-armed shapes: unit test)",
        nc::disarm_json()
    );
}

// silence the unused-import warning for the evidence type used in helpers
#[allow(dead_code)]
fn _evidence_marker(_e: &Evidence) {}
