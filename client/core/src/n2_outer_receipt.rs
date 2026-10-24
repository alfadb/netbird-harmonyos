// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # N2 v4.1 P7 — host-side outer-UDP source receipts (`--outer-receipt`)
//!
//! **AUXILIARY EVIDENCE, APPLICATION LAYER ONLY — not an independent
//! network-side oracle.** Frozen preregistration v4.1 (Q2 ruling) admits the
//! peer process's own `recvfrom` view exactly as「辅助证据（应用层，非独立网络
//! 侧 oracle）」: it names the post-NAT source `ip:port` of every datagram the
//! kernel actually delivered to an outer socket, but it can NOT show the
//! physical interface path, can NOT count loss that happened before the
//! socket, and can NOT exclude an injection from the same host. It therefore
//! never replaces host-side capture and never fills a cell whose capture is
//! missing; its use is to strengthen the cellular `Wb` negative evidence and
//! the peer-side receipt of `Ws`.
//!
//! ## Seams (the outer UDP receive points of the host path)
//!
//! - [`crate::wg_device::WgDevice::service_udp`] — the WG outer UDP socket
//!   (`--wg-port` / ephemeral);
//! - [`crate::ice_mux::IceMux`]'s reader — the ONE shared ICE + WireGuard
//!   socket (since stage 3 it IS the WG outer socket): connectivity checks and,
//!   after nomination, the `Ws`/`Wb` WG datagrams. The retired per-interface
//!   gather socket and the per-session ICE drain are gone (stage 4).
//!
//! ## Output (only while armed; the switch is OFF by default)
//!
//! ```text
//! nbinterop: n2h-outer-receipt|first|src=<ip:port>|local=<ip:port>|type=<stun|wg-init|wg-resp|wg-cookie|wg-data|other>|peer=<known peer short prefix|unknown>|mono_ms=…|unix_ms=…
//! nbinterop: n2h-outer-receipt|summary|src=<ip:port>|packets=…|bytes=…|types=…|local=<ip:port>|peer=…|mono_ms=…|unix_ms=…
//! ```
//!
//! The FIRST datagram of a previously unseen `(local, src)` pair produces
//! exactly one `first` line — **unknown peers included** (`peer=unknown`),
//! which is the point on the cellular legs where the device's NAT source is
//! not the registered endpoint. After that the source is throttled: one
//! `summary` line every [`SUMMARY_MS`] (5 s), or immediately when a NEW
//! datagram type appears for that source ("每 5 s 或变化时"). `types=` lists
//! only the non-zero types, in [`TYPES`] order.
//!
//! ## Zero-output guarantee
//!
//! Unarmed, every seam is one `armed()` atomic load that returns before
//! reading any clock, socket name or peer table: no table, no line. The
//! pre-P7 output is byte-identical.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::sys;

/// One `summary` line per source at most this often (v4.1 P7's 5 s throttle).
const SUMMARY_MS: u64 = 5_000;
/// Bounded ring of emitted lines (the host test's observation surface, see
/// [`emitted_lines`]); long runs stay bounded.
const EMITTED_CAP: usize = 256;
/// RFC 5389 §6 magic cookie: bytes 4..8 of every STUN message.
const STUN_MAGIC: [u8; 4] = [0x21, 0x12, 0xA4, 0x42];

/// The six `type=` tokens, in `types=` report order.
pub const TYPES: [&str; 6] = ["stun", "wg-init", "wg-resp", "wg-cookie", "wg-data", "other"];

static ARMED: AtomicBool = AtomicBool::new(false);
static STATE: OnceLock<Mutex<Sources>> = OnceLock::new();
static EMITTED: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

/// The `type=` of one outer datagram. The STUN magic cookie decides FIRST
/// (never a WG byte-layout guess); everything else whose first byte is a WG
/// message type (1..=4) gets that name, and anything else is `other`.
pub fn classify(datagram: &[u8]) -> &'static str {
    if datagram.len() >= 8 && datagram[4..8] == STUN_MAGIC {
        return "stun";
    }
    match datagram.first() {
        Some(1) => "wg-init",
        Some(2) => "wg-resp",
        Some(3) => "wg-cookie",
        Some(4) => "wg-data",
        _ => "other",
    }
}

/// Per-source running receipt.
#[derive(Default)]
struct Source {
    packets: u64,
    bytes: u64,
    /// Counts in [`TYPES`] order.
    types: [u64; TYPES.len()],
    /// The peer short prefix last attributed to this source (or `unknown`).
    peer: String,
    /// Throttle anchor: `mono_ms` of the last emitted summary.
    last_summary_ms: u64,
    /// The type counts the last emitted line (first or summary) carried —
    /// the "变化时" change detector.
    reported: [u64; TYPES.len()],
}

/// The source table, keyed by `(local, src)` so the same remote port on two
/// different local sockets stays two sources.
#[derive(Default)]
struct Sources {
    map: HashMap<(Option<SocketAddr>, SocketAddr), Source>,
}

impl Sources {
    /// Record one datagram and return the lines it produces (usually none).
    /// Pure in its arguments: the clock is passed in, so a unit test pins
    /// the throttle exactly.
    fn observe(
        &mut self,
        local: Option<SocketAddr>,
        src: SocketAddr,
        datagram: &[u8],
        peer: &str,
        mono_ms: u64,
        unix_ms: u128,
    ) -> Vec<String> {
        let kind = classify(datagram);
        let ki = TYPES.iter().position(|t| *t == kind).unwrap_or(TYPES.len() - 1);
        let peer = clean(peer);
        let entry = self.map.entry((local, src)).or_default();
        let first = entry.packets == 0;
        entry.packets += 1;
        entry.bytes += datagram.len() as u64;
        entry.types[ki] += 1;
        entry.peer = peer.clone();
        let local_text = local.map_or_else(|| "unknown".to_string(), |a| a.to_string());
        if first {
            entry.last_summary_ms = mono_ms;
            entry.reported = entry.types;
            return vec![format!(
                "n2h-outer-receipt|first|src={src}|local={local_text}|type={kind}|peer={peer}|\
                 mono_ms={mono_ms}|unix_ms={unix_ms}"
            )];
        }
        let changed = (0..TYPES.len()).any(|i| entry.reported[i] == 0 && entry.types[i] > 0);
        if changed || mono_ms.saturating_sub(entry.last_summary_ms) >= SUMMARY_MS {
            entry.last_summary_ms = mono_ms;
            entry.reported = entry.types;
            return vec![format!(
                "n2h-outer-receipt|summary|src={src}|packets={}|bytes={}|types={}|local={local_text}|\
                 peer={peer}|mono_ms={mono_ms}|unix_ms={unix_ms}",
                entry.packets,
                entry.bytes,
                types_text(&entry.types)
            )];
        }
        Vec::new()
    }
}

/// The `types=` field: only the non-zero types, `TYPES` order, comma-joined.
fn types_text(counts: &[u64; TYPES.len()]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (i, kind) in TYPES.iter().enumerate() {
        if counts[i] > 0 {
            parts.push(format!("{kind}:{}", counts[i]));
        }
    }
    parts.join(",")
}

/// A peer label is a public-key short prefix (8 alnum chars) or `unknown`;
/// the filter keeps a hostile-looking key from injecting field separators.
fn clean(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .take(16)
        .collect()
}

fn state() -> &'static Mutex<Sources> {
    STATE.get_or_init(|| Mutex::new(Sources::default()))
}

fn emitted() -> &'static Mutex<Vec<String>> {
    EMITTED.get_or_init(|| Mutex::new(Vec::new()))
}

/// Whether the outer-source receipt is armed (the ONE gate every seam reads).
pub fn armed() -> bool {
    ARMED.load(Ordering::Acquire)
}

/// Arm/disarm the receipt. Either transition drops the whole source table and
/// the emitted ring, so a disarmed run leaves nothing behind.
pub fn arm(enabled: bool) {
    ARMED.store(enabled, Ordering::Release);
    state().lock().expect("outer receipt sources").map.clear();
    emitted().lock().expect("outer receipt lines").clear();
}

/// Record one datagram received on an outer UDP socket and print the receipt
/// lines it produces. Callers check [`armed`] before computing the socket name
/// and the peer label; this function re-checks, so an unarmed call is a no-op
/// by construction (the zero-output negative control).
pub fn note_udp(local: Option<SocketAddr>, src: SocketAddr, datagram: &[u8], peer: &str) {
    // Host tests: `note_udp` also runs on pump/reader threads owned by OTHER
    // tests in the same lib binary, and those threads append into the
    // process-global emitted ring whenever the receipt is armed. A test that
    // arms the receipt and asserts on exact line counts owns the ring, so it
    // holds this gate for its whole body; emitters take it for one receipt.
    // The re-check below matters: a test may have disarmed (and cleared the
    // ring) while this emitter waited for the gate, so the earlier
    // `armed()` check is no longer sufficient.
    #[cfg(test)]
    let _gate = emit_gate::lock();
    if !armed() {
        return;
    }
    let mono_ms = sys::mono_ms();
    let unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |t| t.as_millis());
    let lines = state()
        .lock()
        .expect("outer receipt sources")
        .observe(local, src, datagram, peer, mono_ms, unix_ms);
    for line in lines {
        {
            let mut ring = emitted().lock().expect("outer receipt lines");
            if ring.len() >= EMITTED_CAP {
                ring.remove(0);
            }
            ring.push(line.clone());
        }
        eprintln!("nbinterop: {line}");
    }
}

/// The receipt lines emitted so far (bounded, newest last). The host test
/// observes the receipt ITSELF here instead of scraping stderr; nothing in the
/// data path reads this back.
pub fn emitted_lines() -> Vec<String> {
    emitted().lock().expect("outer receipt lines").clone()
}

/// The crate-wide lock every in-crate test that touches the process-global
/// arming/source table must hold (unit tests run in parallel in one binary).
#[cfg(test)]
pub(crate) fn global_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = LOCK.get_or_init(|| Mutex::new(()));
    match lock.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Host-test emission gate (see [`note_udp`]): emitters owned by other tests
/// run on their own threads and append into the same process-global emitted
/// ring, so the test that owns the ring excludes them for its whole body.
/// RE-ENTRANT because that owner's own `note_udp` calls run on its thread.
/// Never part of the release build (the arm/emit path has no gate there).
#[cfg(test)]
pub(crate) mod emit_gate {
    use std::cell::{Cell, RefCell};
    use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    thread_local! {
        /// The underlying guard of the outermost acquisition on this thread.
        static HELD: RefCell<Option<MutexGuard<'static, ()>>> = const { RefCell::new(None) };
        static DEPTH: Cell<usize> = const { Cell::new(0) };
    }

    pub(crate) struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            let left = DEPTH.with(|depth| {
                let left = depth.get().saturating_sub(1);
                depth.set(left);
                left
            });
            if left == 0 {
                HELD.with(|held| {
                    let _ = held.borrow_mut().take();
                });
            }
        }
    }

    pub(crate) fn lock() -> Guard {
        let first = DEPTH.with(|depth| {
            let before = depth.get();
            depth.set(before + 1);
            before == 0
        });
        if first {
            let guard = LOCK
                .get_or_init(|| Mutex::new(()))
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            HELD.with(|held| *held.borrow_mut() = Some(guard));
        }
        Guard
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4(a: [u8; 4], port: u16) -> SocketAddr {
        SocketAddr::from((a, port))
    }

    /// A minimal STUN Binding Request: type 0x0001 + length + magic cookie.
    fn stun_msg() -> Vec<u8> {
        let mut m = vec![0u8; 20];
        m[1] = 0x01;
        m[4..8].copy_from_slice(&STUN_MAGIC);
        m
    }

    #[test]
    fn classify_uses_the_stun_cookie_first_then_the_wg_message_type() {
        assert_eq!(classify(&stun_msg()), "stun");
        // a WG packet whose payload happens to carry the cookie bytes is STUN
        // only if the cookie sits exactly at 4..8 — this one does not
        let mut wg = vec![4u8; 32];
        wg[0] = 4;
        assert_eq!(classify(&wg), "wg-data");
        assert_eq!(classify(&[1u8; 148]), "wg-init");
        assert_eq!(classify(&[2u8; 148]), "wg-resp");
        assert_eq!(classify(&[3u8; 148]), "wg-cookie");
        assert_eq!(classify(&[4u8; 148]), "wg-data");
        assert_eq!(classify(&[0u8; 148]), "other");
        assert_eq!(classify(&[5u8; 148]), "other");
        assert_eq!(classify(&[0xFFu8; 148]), "other");
        assert_eq!(classify(&[]), "other");
        // cookie at 4..8 but only 7 bytes long: no STUN header, WG type wins
        assert_eq!(classify(&[1u8, 0, 0, 0, 0x21, 0x12, 0xA4]), "wg-init");
    }

    #[test]
    fn first_datagram_of_a_new_source_prints_one_line_and_unknown_peers_are_recorded() {
        let mut s = Sources::default();
        let out = s.observe(
            Some(v4([0, 0, 0, 0], 51820)),
            v4([192, 168, 50, 199], 41234),
            &stun_msg(),
            "unknown",
            1_000,
            1_700_000_000_000,
        );
        assert_eq!(out.len(), 1, "{out:?}");
        let line = &out[0];
        assert!(line.starts_with("n2h-outer-receipt|first|"), "{line}");
        assert!(line.contains("|src=192.168.50.199:41234|"), "{line}");
        assert!(line.contains("|local=0.0.0.0:51820|"), "{line}");
        assert!(line.contains("|type=stun|"), "{line}");
        assert!(line.contains("|peer=unknown|"), "{line}");
        assert!(line.contains("|mono_ms=1000|"), "{line}");
        assert!(line.contains("|unix_ms=1700000000000"), "{line}");

        // a SECOND new source prints its own first line (not a summary)
        let out = s.observe(
            Some(v4([0, 0, 0, 0], 51820)),
            v4([10, 98, 0, 142], 5555),
            &[1u8; 148],
            "abc12345",
            1_001,
            1_700_000_000_001,
        );
        assert_eq!(out.len(), 1, "{out:?}");
        assert!(out[0].contains("|src=10.98.0.142:5555|"), "{}", out[0]);
        assert!(out[0].contains("|type=wg-init|"), "{}", out[0]);
        assert!(out[0].contains("|peer=abc12345|"), "{}", out[0]);
    }

    #[test]
    fn summaries_are_throttled_to_five_seconds_or_a_new_type() {
        let mut s = Sources::default();
        let local = Some(v4([0, 0, 0, 0], 51820));
        let src = v4([192, 168, 50, 199], 41234);
        let first = s.observe(local, src, &[4u8; 100], "unknown", 1_000, 1);
        assert_eq!(first.len(), 1);
        // same type, inside the window: nothing
        assert!(s.observe(local, src, &[4u8; 100], "unknown", 1_001, 2).is_empty());
        assert!(s.observe(local, src, &[4u8; 100], "unknown", 5_999, 3).is_empty());
        // a NEW type reports immediately ("或变化时")
        let changed = s.observe(local, src, &stun_msg(), "unknown", 6_000, 4);
        assert_eq!(changed.len(), 1, "{changed:?}");
        assert!(changed[0].contains("|summary|"), "{}", changed[0]);
        assert!(changed[0].contains("|packets=4|"), "{}", changed[0]);
        assert!(changed[0].contains("|bytes=320|"), "{}", changed[0]);
        assert!(changed[0].contains("|types=stun:1,wg-data:3|"), "{}", changed[0]);
        // same type mix again: silent until the 5 s window since THAT summary
        assert!(s.observe(local, src, &[4u8; 100], "unknown", 6_001, 5).is_empty());
        let due = s.observe(local, src, &[4u8; 100], "unknown", 11_000, 6);
        assert_eq!(due.len(), 1, "{due:?}");
        assert!(due[0].contains("|packets=6|"), "{}", due[0]);
        assert!(due[0].contains("|types=stun:1,wg-data:5|"), "{}", due[0]);
    }

    #[test]
    fn a_late_known_peer_attribution_is_reported_on_the_next_summary() {
        let mut s = Sources::default();
        let local = Some(v4([0, 0, 0, 0], 51820));
        let src = v4([10, 98, 0, 142], 5555);
        assert!(s.observe(local, src, &[1u8; 148], "unknown", 0, 0)[0].contains("|peer=unknown|"));
        // the endpoint registration lands: the next line names the peer
        let out = s.observe(local, src, &stun_msg(), "deadbeef", 1, 1);
        assert_eq!(out.len(), 1, "{out:?}");
        assert!(out[0].contains("|peer=deadbeef|"), "{}", out[0]);
    }

    #[test]
    fn disarmed_note_udp_emits_nothing() {
        let _g = global_lock();
        // This test owns the global emitted ring for exact-count assertions;
        // parallel pump/reader threads from other tests emit into it through
        // `note_udp` whenever the receipt is armed, so exclude them.
        let _gate = emit_gate::lock();
        let local = Some(v4([0, 0, 0, 0], 51820));
        let src = v4([192, 168, 50, 199], 41234);
        arm(true);
        assert!(armed());
        note_udp(local, src, &stun_msg(), "unknown");
        assert_eq!(emitted_lines().len(), 1, "the armed control must emit");

        // the negative control: same seam, switch off
        arm(false);
        assert!(!armed());
        assert!(emitted_lines().is_empty(), "disarming clears the ring");
        note_udp(local, src, &stun_msg(), "unknown");
        assert!(emitted_lines().is_empty(), "no line while disarmed");
    }
}
