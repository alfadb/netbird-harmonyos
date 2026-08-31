//! N1a data-plane pump — the real BoringTun 0.7.1 ffi loopback pump.
//!
//! Everything here calls the real BoringTun C ABI functions via
//! `boringtun::ffi` (`x25519_secret_key`, `x25519_public_key`,
//! `x25519_key_to_base64`, `new_tunnel`, `wireguard_write`, `wireguard_read`,
//! `wireguard_tick`, `wireguard_force_handshake`, `wireguard_stats`,
//! `tunnel_free`). Nothing is faked: no simulated handshake, no bypass of the
//! encryption path, no fabricated counters.
//!
//! Source-derived semantics (boringtun-0.7.1, read before writing this code;
//! paths relative to `src/` of the crate):
//!
//! - `ffi/mod.rs::wireguard_stats` returns `stats { time_since_last_handshake,
//!   tx_bytes, rx_bytes, .. }` where `time_since_last_handshake == -1` means
//!   "no handshake occurred" (it maps `Tunn::time_since_last_handshake() ->
//!   Option<Duration>`, `noise/mod.rs:314-324`). **stats has no explicit
//!   `state == connected` field in 0.7.1.** The equivalent observable used
//!   for C2 is therefore `time_since_last_handshake >= 0`, which is exactly
//!   `sessions[current].is_some()`. On top of that, a first real
//!   `wireguard_write` -> channel -> `wireguard_read` round after the stats
//!   check is kept as the requested belt-and-braces establishment proof.
//! - The full four-leg exchange (init -> response -> keepalive) is required:
//!   the responder stores its session when it processes the handshake
//!   initiation (`noise/mod.rs:318-341`), the initiator when it processes the
//!   response and *emits a transport keepalive as a network write*
//!   (`noise/mod.rs:343-369`); the responder only adopts `current` when that
//!   keepalive data packet arrives (`handle_data` -> `set_current_session`).
//!   Dropping the keepalive leg leaves the responder's stats observably
//!   unestablished, so the pump always shuttles it.
//! - `decapsulate` validates the inner packet as IPv4/IPv6 and *truncates it
//!   to the IP total_length field* (`validate_decapsulated_packet`,
//!   `noise/mod.rs:464-507`); synthetic plaintext packets are therefore
//!   well-formed 1024-byte IPv4 packets with `total_length == 1024`.
//! - `encapsulate` panics if `dst < src.len() + 32` or `dst < 148`
//!   (`noise/mod.rs:244-249` doc); all ffi destination buffers here are 2048 B.
//! - The per-tunnel rate limiter rate-limits handshake messages only
//!   (`noise/rate_limiter.rs::verify_packet`), and every `wireguard_tick`
//!   call resets its counter (`noise/timers.rs::update_timers` ->
//!   `rate_limiter.reset_count`), so the 4000-packet data phase can never
//!   trip it, and the handshake phase stays far below the limit.

use std::ffi::CString;
use std::fs;
use std::io;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

use boringtun;

/// Probe version string (also embedded in the JSON result).
pub const PROBE_VERSION: &str = "n1a-native-dataplane/0.1.0+boringtun-0.7.1";
/// Fixed BoringTun 0.7.1 crate checksum (crates.io), verified by build.sh.
pub const BORINGTUN_CRATE_SHA256: &str =
    "15dd6a8a89cbe8997f37ca0cf035e6ea4d64cd2ecea4aed83ffb9f99f7126939";

// ---------------------------------------------------------------------------
// Pre-registered constants (docs/n1a-gate-plan.md C2-C6; values are fixed by
// the gate plan and must not be tuned after a measurement).
// ---------------------------------------------------------------------------

/// C3: 10 rounds x 200 packets per direction.
pub const ROUNDS: usize = 10;
pub const PACKETS_PER_ROUND: usize = 200;
/// C3: payload size per synthetic packet (bytes).
pub const PAYLOAD_SZ: usize = 1024;
/// C4: pre-registered throughput floor, MiB/s.
pub const THROUGHPUT_FLOOR_MIB_S: f64 = 5.0;
/// C2: handshake establishment timebox.
pub const HANDSHAKE_TIMEBOX: Duration = Duration::from_secs(30);
/// C6: number of no-data quiet gaps, each driven with real tick calls.
pub const TICK_GAPS: usize = 3;
/// C5: backpressure burst size (packets) against a shrunk socket buffer.
pub const BACKPRESSURE_BURST: usize = 512;
/// C5: backpressure timebox; exceeding it is treated as a deadlock => fail.
pub const BACKPRESSURE_TIMEBOX: Duration = Duration::from_secs(10);

/// One ffi destination buffer size; >= 148 (handshake) and >= 1024 + 32
/// (data + poly1305 tag + transport header) per `encapsulate`'s contract.
pub(crate) const BUF_SZ: usize = 2048;

// Criterion status values shared with the C ABI and the JSON result.
pub const CRIT_FAIL: i32 = 0;
pub const CRIT_PASS: i32 = 1;
pub const CRIT_NOT_TRIGGERED: i32 = 2;

/// Synthetic-packet direction markers carried in bytes 20..24 of the payload.
const DIR_A2B: u32 = 0;
const DIR_B2A: u32 = 1;
const DIR_BACKPRESSURE: u32 = 2;

const OP_DONE: i32 = boringtun::ffi::result_type::WIREGUARD_DONE as i32;
const OP_NETWORK: i32 = boringtun::ffi::result_type::WRITE_TO_NETWORK as i32;
const OP_ERROR: i32 = boringtun::ffi::result_type::WIREGUARD_ERROR as i32;
const OP_TUN_V4: i32 = boringtun::ffi::result_type::WRITE_TO_TUNNEL_IPV4 as i32;
const OP_TUN_V6: i32 = boringtun::ffi::result_type::WRITE_TO_TUNNEL_IPV6 as i32;

/// A pump failure with the criterion it harms ("c1".."c9").
#[derive(Debug)]
pub struct ProbeError {
    pub criterion: &'static str,
    pub message: String,
}

impl ProbeError {
    fn new(criterion: &'static str, message: impl Into<String>) -> Self {
        ProbeError { criterion, message: message.into() }
    }
}

type PhaseResult<T> = Result<T, ProbeError>;

// ---------------------------------------------------------------------------
// BoringTun ffi tunnel handle
// ---------------------------------------------------------------------------

/// Opaque stand-in for `*mut parking_lot::Mutex<boringtun::noise::Tunn>`.
///
/// The concrete type is not nameable from this crate (parking_lot is not a
/// direct dependency), so the pointer is carried opaquely and cast back at
/// each `boringtun::ffi` call site with `as *const _` / `as *mut _`, which
/// infers the concrete ffi parameter type. Thin-pointer layout equivalent.
#[repr(C)]
struct OpaqueTunn {
    _opaque: [u8; 0],
}

type TunnelPtr = *mut OpaqueTunn;

/// Owned BoringTun tunnel; freed exactly once by `tunnel_free` on drop (C8).
struct Tunnel {
    ptr: TunnelPtr,
    /// Short label used in error messages ("A" / "B").
    name: &'static str,
}

impl Tunnel {
    /// `secret_b64` / `peer_public_b64` are 44-char base64 x25519 keys, as
    /// required by `new_tunnel` (`ffi/mod.rs:244-312`). `index` must be
    /// unique per tunnel (it seeds the local index space: `index << 8`).
    fn new(
        secret_b64: &str,
        peer_public_b64: &str,
        index: u32,
        name: &'static str,
    ) -> PhaseResult<Tunnel> {
        let secret = CString::new(secret_b64)
            .map_err(|_| ProbeError::new("c2", format!("{name}: secret key not ASCII")))?;
        let public = CString::new(peer_public_b64)
            .map_err(|_| ProbeError::new("c2", format!("{name}: peer key not ASCII")))?;
        let ptr = unsafe {
            boringtun::ffi::new_tunnel(
                secret.as_ptr() as *const _,
                public.as_ptr() as *const _,
                std::ptr::null(), // no preshared key
                // Frozen criteria C6: keep_alive = 1 on both tunnels, so
                // `wireguard_tick` after a no-data gap emits a REAL persistent
                // keepalive (update_timers) that the pump shuttles through the
                // channel to the peer. This is the path C6 verifies.
                1,
                index,
            )
        };
        if ptr.is_null() {
            return Err(ProbeError::new("c2", format!("new_tunnel({name}) returned NULL")));
        }
        Ok(Tunnel { ptr: ptr as TunnelPtr, name })
    }

    fn write(&self, src: &[u8], dst: &mut [u8; BUF_SZ]) -> (i32, usize) {
        let r = unsafe {
            boringtun::ffi::wireguard_write(
                self.ptr as *const _,
                src.as_ptr(),
                src.len() as u32,
                dst.as_mut_ptr(),
                dst.len() as u32,
            )
        };
        (r.op as i32, r.size)
    }

    fn read(&self, src: &[u8], dst: &mut [u8; BUF_SZ]) -> (i32, usize) {
        let r = unsafe {
            boringtun::ffi::wireguard_read(
                self.ptr as *const _,
                src.as_ptr(),
                src.len() as u32,
                dst.as_mut_ptr(),
                dst.len() as u32,
            )
        };
        (r.op as i32, r.size)
    }

    fn tick(&self, dst: &mut [u8; BUF_SZ]) -> (i32, usize) {
        let r = unsafe {
            boringtun::ffi::wireguard_tick(self.ptr as *const _, dst.as_mut_ptr(), dst.len() as u32)
        };
        (r.op as i32, r.size)
    }

    fn force_handshake(&self, dst: &mut [u8; BUF_SZ]) -> PhaseResult<usize> {
        let r = unsafe {
            boringtun::ffi::wireguard_force_handshake(
                self.ptr as *const _,
                dst.as_mut_ptr(),
                dst.len() as u32,
            )
        };
        let op = r.op as i32;
        if op != OP_NETWORK {
            return Err(ProbeError::new(
                "c2",
                format!(
                    "wireguard_force_handshake({}) op={op} size={} ({})",
                    self.name,
                    r.size,
                    wg_error_name(r.size)
                ),
            ));
        }
        Ok(r.size)
    }

    /// Returns `(time_since_last_handshake, tx_bytes, rx_bytes)`; see the
    /// module docs for why `time_since_last_handshake >= 0` is the C2
    /// session-established observable.
    fn stats(&self) -> PhaseResult<(i64, u64, u64)> {
        let s = unsafe { boringtun::ffi::wireguard_stats(self.ptr as *const _) };
        Ok((s.time_since_last_handshake, s.tx_bytes as u64, s.rx_bytes as u64))
    }

    fn established(&self) -> PhaseResult<bool> {
        Ok(self.stats()?.0 >= 0)
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        // C8: exactly one real tunnel_free per tunnel.
        unsafe { boringtun::ffi::tunnel_free(self.ptr as *mut _) };
        self.ptr = std::ptr::null_mut();
    }
}

// ---------------------------------------------------------------------------
// UDP loopback channel ("non-VPN loopback fd" pair)
// ---------------------------------------------------------------------------

/// Two UDP sockets bound on 127.0.0.1 and connected to each other: one
/// in-process loopback channel standing in for WireGuard's outer UDP
/// transport. No VPN/TUN/platform API is involved (N1a scope).
struct Channel {
    a: UdpSocket,
    b: UdpSocket,
}

impl Channel {
    fn new(crit: &'static str) -> PhaseResult<Channel> {
        let a = UdpSocket::bind("127.0.0.1:0")
            .map_err(|e| ProbeError::new(crit, format!("bind A failed: {e}")))?;
        let b = UdpSocket::bind("127.0.0.1:0")
            .map_err(|e| ProbeError::new(crit, format!("bind B failed: {e}")))?;
        let b_addr = b
            .local_addr()
            .map_err(|e| ProbeError::new(crit, format!("local_addr B: {e}")))?;
        let a_addr = a
            .local_addr()
            .map_err(|e| ProbeError::new(crit, format!("local_addr A: {e}")))?;
        a.connect(b_addr).map_err(|e| ProbeError::new(crit, format!("connect A->B: {e}")))?;
        b.connect(a_addr).map_err(|e| ProbeError::new(crit, format!("connect B->A: {e}")))?;
        // Blocking sockets with a generous read timeout: in the integrity
        // phase every write is answered by exactly one datagram, so a timeout
        // here means "peer did not answer" and is surfaced as an error.
        for (name, s) in [("A", &a), ("B", &b)] {
            s.set_read_timeout(Some(Duration::from_secs(5)))
                .map_err(|e| ProbeError::new(crit, format!("set_read_timeout {name}: {e}")))?;
        }
        Ok(Channel { a, b })
    }
}

// ---------------------------------------------------------------------------
// Synthetic IPv4 plaintext packets with (direction, round, seq) markers
// ---------------------------------------------------------------------------

/// Builds the deterministic 1024-byte plaintext for one probe packet.
///
/// Layout (survives `validate_decapsulated_packet` on the receiving tunnel,
/// see module docs):
/// - bytes 0..20: minimal well-formed IPv4 header (version 4, IHL 5,
///   total_length = 1024, real RFC 1071 header checksum);
/// - bytes 20..32: big-endian `(direction, round, seq)` marker;
/// - bytes 32..1024: xorshift32 pattern seeded by the marker, so every
///   packet differs in essentially all bytes and byte-equality is strong.
pub fn synth_packet(direction: u32, round: u32, seq: u32) -> [u8; PAYLOAD_SZ] {
    let mut p = [0u8; PAYLOAD_SZ];
    p[0] = 0x45; // IPv4, IHL 5
    p[1] = 0x00; // DSCP/ECN
    p[2..4].copy_from_slice(&(PAYLOAD_SZ as u16).to_be_bytes()); // total length
    p[4..6].copy_from_slice(&((seq & 0xffff) as u16).to_be_bytes()); // identification
    p[6..8].copy_from_slice(&0x4000u16.to_be_bytes()); // DF, no fragment offset
    p[8] = 64; // TTL
    p[9] = 253; // protocol 253 (experimental, RFC 3692) — synthetic marker
    p[12..16].copy_from_slice(&[10, 200, 1 + direction as u8, 1]); // src
    p[16..20].copy_from_slice(&[10, 200, 2 - direction as u8, 1]); // dst
    p[20..24].copy_from_slice(&direction.to_be_bytes());
    p[24..28].copy_from_slice(&round.to_be_bytes());
    p[28..32].copy_from_slice(&seq.to_be_bytes());
    let mut seed = direction
        .wrapping_mul(0x9E37_79B9)
        ^ round.wrapping_mul(0x85EB_CA6B)
        ^ seq.wrapping_mul(0xC2B2_AE35)
        ^ 0x1B87_3CA5;
    for chunk in p[32..].chunks_exact_mut(4) {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        chunk.copy_from_slice(&seed.to_be_bytes());
    }
    let checksum = ipv4_header_checksum(&p[..20]);
    p[10..12].copy_from_slice(&checksum.to_be_bytes());
    p
}

/// RFC 1071 internet checksum over an even-length byte slice.
fn ipv4_header_checksum(header: &[u8]) -> u16 {
    let mut sum = 0u32;
    for i in (0..header.len()).step_by(2) {
        let hi = header[i] as u32;
        let lo = if i + 1 < header.len() { header[i + 1] as u32 } else { 0 };
        sum += (hi << 8) | lo;
    }
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// Parses the `(direction, round, seq)` marker of a received plaintext
/// packet (bytes 20..32).
fn parse_marker(crit: &'static str, plain: &[u8]) -> PhaseResult<(u32, u32, u32)> {
    if plain.len() < 32 {
        return Err(ProbeError::new(crit, format!("plaintext too short: {}", plain.len())));
    }
    let be = |off: usize| u32::from_be_bytes([plain[off], plain[off + 1], plain[off + 2], plain[off + 3]]);
    Ok((be(20), be(24), be(28)))
}

// ---------------------------------------------------------------------------
// Result model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct Stats {
    pub time_since_last_handshake: i64,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
}

/// Full structured outcome of one probe run.
pub struct ProbeOutcome {
    pub ok: bool,
    /// c1..c9 status: CRIT_PASS / CRIT_FAIL / CRIT_NOT_TRIGGERED (C5 only).
    pub criteria: [i32; 9],
    pub error: Option<String>,
    /// C1: reaching the probe body means the native member is loaded and the
    /// BoringTun C ABI is callable (see `run_probe`).
    pub probe_entered: bool,
    // C2
    pub handshake_established: bool,
    pub handshake_ms: f64,
    pub handshake_attempts: u32,
    pub stats_a_after: Option<Stats>,
    pub stats_b_after: Option<Stats>,
    pub first_roundtrip_ok: bool,
    // C3
    pub expected_packets_total: usize,
    pub verified_packets_total: usize,
    pub mismatch_count: u64,
    pub lost_count: u64,
    pub first_mismatch: Option<String>,
    /// Frozen C3(d) byte accounting: per-tunnel tx/rx stats deltas over the
    /// C3 window, asserted equal to the per-direction inner plaintext bytes.
    pub tx_bytes_delta_a: u64,
    pub rx_bytes_delta_a: u64,
    pub tx_bytes_delta_b: u64,
    pub rx_bytes_delta_b: u64,
    pub byte_accounting_ok: bool,
    // C4
    pub verified_plaintext_bytes: u64,
    pub pump_ms: f64,
    pub throughput_mib_s: f64,
    // C5
    pub backpressure_triggered: bool,
    pub eagain_count: u64,
    pub send_attempts: u64,
    pub successful_sends: u64,
    pub retransmit_rounds: u64,
    pub kernel_queue_drops: u64,
    pub bp_received: usize,
    pub bp_verified: usize,
    pub bp_corrupted: u64,
    pub bp_extra: u64,
    pub so_sndbuf: Option<i32>,
    pub so_rcvbuf: Option<i32>,
    pub backpressure_ms: f64,
    // C6
    pub tick_gap_count: usize,
    pub tick_call_count: u64,
    pub tick_network_packets: u64,
    pub tick_session_ok_after_each: bool,
    pub post_gap_burst_ok: bool,
    // C7/C8
    pub fd_baseline: Option<usize>,
    pub fd_after: Option<usize>,
    pub thread_baseline: Option<usize>,
    pub thread_after: Option<usize>,
    pub tunnels_freed: u32,
    pub sockets_closed: u32,
    // timing
    pub total_ms: f64,
}

impl ProbeOutcome {
    /// Public constructor: fail-closed defaults (every criterion failed,
    /// no stats recorded). Also used by the C ABI's defensive panic path.
    pub fn new(fd_baseline: Option<usize>, thread_baseline: Option<usize>) -> Self {
        ProbeOutcome {
            ok: false,
            // Fail-closed default: every criterion starts failed and is
            // flipped to pass only by real evidence.
            criteria: [CRIT_FAIL; 9],
            error: None,
            probe_entered: true,
            handshake_established: false,
            handshake_ms: 0.0,
            handshake_attempts: 0,
            stats_a_after: None,
            stats_b_after: None,
            first_roundtrip_ok: false,
            expected_packets_total: 2 * ROUNDS * PACKETS_PER_ROUND,
            verified_packets_total: 0,
            mismatch_count: 0,
            lost_count: 0,
            first_mismatch: None,
            tx_bytes_delta_a: 0,
            rx_bytes_delta_a: 0,
            tx_bytes_delta_b: 0,
            rx_bytes_delta_b: 0,
            byte_accounting_ok: false,
            verified_plaintext_bytes: 0,
            pump_ms: 0.0,
            throughput_mib_s: 0.0,
            backpressure_triggered: false,
            eagain_count: 0,
            send_attempts: 0,
            successful_sends: 0,
            retransmit_rounds: 0,
            kernel_queue_drops: 0,
            bp_received: 0,
            bp_verified: 0,
            bp_corrupted: 0,
            bp_extra: 0,
            so_sndbuf: None,
            so_rcvbuf: None,
            backpressure_ms: 0.0,
            tick_gap_count: 0,
            tick_call_count: 0,
            tick_network_packets: 0,
            tick_session_ok_after_each: true,
            post_gap_burst_ok: false,
            fd_baseline,
            fd_after: None,
            thread_baseline,
            thread_after: None,
            tunnels_freed: 0,
            sockets_closed: 0,
            total_ms: 0.0,
        }
    }

    /// Aggregation vocabulary (frozen): C5 pass is recorded as
    /// `pass-induced`; the C9 HiLog marker separately uses the C9
    /// enumeration `induced` (see napi overlay). All other criteria use
    /// pass/not-triggered/fail.
    fn criterion_name(index: usize, status: i32) -> &'static str {
        match status {
            CRIT_PASS if index == 4 => "pass-induced",
            CRIT_PASS => "pass",
            CRIT_NOT_TRIGGERED => "not-triggered",
            _ => "fail",
        }
    }

    /// Aggregated verdict: pass iff every criterion is pass, or (C5 only)
    /// not-triggered ("未触发≠失败").
    fn finalize(&mut self) {
        self.ok = self.criteria.iter().enumerate().all(|(i, &s)| {
            s == CRIT_PASS || (s == CRIT_NOT_TRIGGERED && i == 4)
        });
    }
}

// ---------------------------------------------------------------------------
// Pump primitives
// ---------------------------------------------------------------------------

/// One `wireguard_write` -> encrypted datagram -> channel send step.
///
/// Frozen C3 machine conditions asserted per packet:
/// (a) `op == WRITE_TO_NETWORK` and `size >= src.len() + 32`
///     (DATA_OVERHEAD_SZ = 32);
/// (b) the channel payload is NOT the plaintext (first 32 bytes compared);
/// (c) the channel payload's first 4 bytes are LE `u32 == 4`
///     (WireGuard DATA message type).
fn send_data(
    crit: &'static str,
    tunnel: &Tunnel,
    sock: &UdpSocket,
    plaintext: &[u8],
    net: &mut [u8; BUF_SZ],
) -> PhaseResult<()> {
    let (op, size) = tunnel.write(plaintext, net);
    if op != OP_NETWORK {
        return Err(ProbeError::new(
            crit,
            format!(
                "{}: wireguard_write op={op} size={size} ({})",
                tunnel.name,
                wg_error_name(size)
            ),
        ));
    }
    // (a) real encrypted DATA datagram: overhead is DATA_OVERHEAD_SZ = 32.
    if size < plaintext.len() + 32 {
        return Err(ProbeError::new(
            crit,
            format!(
                "{}: ciphertext size {size} < inner {} + 32 (overhead)",
                tunnel.name,
                plaintext.len()
            ),
        ));
    }
    // (b) the datagram on the wire must not be the plaintext itself
    //     (compare the first 32 bytes of both).
    if net[..32] == plaintext[..32] {
        return Err(ProbeError::new(
            crit,
            format!("{}: channel payload equals plaintext (no encryption)", tunnel.name),
        ));
    }
    // (c) first 4 bytes: LE u32 == 4 (DATA message type).
    let msg_type = u32::from_le_bytes([net[0], net[1], net[2], net[3]]);
    if msg_type != 4 {
        return Err(ProbeError::new(
            crit,
            format!(
                "{}: channel payload LE type {msg_type} != 4 (DATA)",
                tunnel.name
            ),
        ));
    }
    sock.send(&net[..size])
        .map_err(|e| ProbeError::new(crit, format!("{}: channel send failed: {e}", tunnel.name)))?;
    Ok(())
}

/// One channel recv -> `wireguard_read` step; returns the decrypted plaintext
/// length (plaintext left in `plain`). Handles the "repeat the call until
/// Done" protocol from `decapsulate`'s contract by looping WRITE_TO_NETWORK
/// payloads back onto the channel, bounded.
fn recv_data(
    crit: &'static str,
    tunnel: &Tunnel,
    sock: &UdpSocket,
    net: &mut [u8; BUF_SZ],
    plain: &mut [u8; BUF_SZ],
) -> PhaseResult<usize> {
    for _ in 0..8 {
        let n = sock
            .recv(net)
            .map_err(|e| ProbeError::new(crit, format!("{}: channel recv failed: {e}", tunnel.name)))?;
        let (op, size) = tunnel.read(&net[..n], plain);
        match op {
            // Frozen C3: only WRITE_TO_TUNNEL_IPV4 is an accepted inner
            // delivery. A V6 return is a fail, never silently accepted.
            OP_TUN_V4 => return Ok(size),
            OP_TUN_V6 => {
                return Err(ProbeError::new(
                    crit,
                    format!(
                        "{}: received inner packet as IPv6 (op=WRITE_TO_TUNNEL_IPV6); \
                         frozen criteria accept WRITE_TO_TUNNEL_IPV4 only",
                        tunnel.name
                    ),
                ));
            }
            OP_NETWORK => {
                sock.send(&plain[..size]).map_err(|e| {
                    ProbeError::new(crit, format!("{}: channel re-send failed: {e}", tunnel.name))
                })?;
            }
            OP_DONE => {}
            OP_ERROR => {
                return Err(ProbeError::new(
                    crit,
                    format!(
                        "{}: wireguard_read error code {size} ({})",
                        tunnel.name,
                        wg_error_name(size)
                    ),
                ));
            }
            op => {
                return Err(ProbeError::new(crit, format!("{}: unexpected read op {op}", tunnel.name)));
            }
        }
    }
    Err(ProbeError::new(crit, format!("{}: read loop did not yield plaintext", tunnel.name)))
}

/// Drives one side until the channel goes quiet (recv timeout), looping every
/// WRITE_TO_NETWORK payload back to the peer. Returns the number of protocol
/// packets sent. Used by the handshake and tick phases.
fn drive_quiet(
    crit: &'static str,
    tunnel: &Tunnel,
    sock: &UdpSocket,
    quiet_timeout: Duration,
    net: &mut [u8; BUF_SZ],
    plain: &mut [u8; BUF_SZ],
) -> PhaseResult<u64> {
    sock.set_read_timeout(Some(quiet_timeout))
        .map_err(|e| ProbeError::new(crit, format!("set_read_timeout: {e}")))?;
    let mut sent = 0u64;
    loop {
        match sock.recv(net) {
            Ok(n) => {
                let (op, size) = tunnel.read(&net[..n], plain);
                match op {
                    OP_NETWORK => {
                        sent += 1;
                        sock.send(&plain[..size]).map_err(|e| {
                            ProbeError::new(crit, format!("{}: channel send failed: {e}", tunnel.name))
                        })?;
                    }
                    OP_DONE => {}
                    OP_ERROR => {
                        return Err(ProbeError::new(
                            crit,
                            format!(
                                "{}: wireguard_read error code {size} ({})",
                                tunnel.name,
                                wg_error_name(size)
                            ),
                        ));
                    }
                    OP_TUN_V4 | OP_TUN_V6 => {
                        // Unexpected plaintext during handshake/tick phases;
                        // the data phases are the only place plaintext is
                        // consumed, so this would be a protocol anomaly.
                        return Err(ProbeError::new(
                            crit,
                            format!("{}: unexpected plaintext during drive", tunnel.name),
                        ));
                    }
                    op => {
                        return Err(ProbeError::new(
                            crit,
                            format!("{}: unexpected read op {op}", tunnel.name),
                        ));
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => {
                return Ok(sent);
            }
            Err(e) => {
                return Err(ProbeError::new(
                    crit,
                    format!("{}: channel recv failed: {e}", tunnel.name),
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Probe phases
// ---------------------------------------------------------------------------

/// C2: force_handshake from A, tick + channel shuttling both directions until
/// both tunnels' real stats report an established session, then one full
/// write -> channel -> read round as the requested establishment proof.
fn phase_handshake(
    a: &Tunnel,
    b: &Tunnel,
    chan: &Channel,
    net: &mut [u8; BUF_SZ],
    plain: &mut [u8; BUF_SZ],
) -> PhaseResult<(f64, u32, Stats, Stats)> {
    let started = Instant::now();
    let deadline = started + HANDSHAKE_TIMEBOX;
    let quiet = Duration::from_millis(200);
    let mut attempts = 0u32;

    loop {
        let a_est = a.established()?;
        let b_est = b.established()?;
        if a_est && b_est {
            break;
        }
        if Instant::now() > deadline {
            return Err(ProbeError::new(
                "c2",
                format!(
                    "handshake timebox (30s) exceeded: established(A)={a_est} established(B)={b_est} attempts={attempts}"
                ),
            ));
        }
        attempts += 1;
        // A initiates (force=true so a stuck in-progress handshake is retried).
        let size = a.force_handshake(net)?;
        chan.a
            .send(&net[..size])
            .map_err(|e| ProbeError::new("c2", format!("send handshake init: {e}")))?;
        // Shuttle until quiet. B processes the init (stores its session) and
        // answers; A processes the response (stores its session) and emits
        // the keepalive leg; B adopts `current` on that keepalive. Re-drive
        // until both sides stay quiet so the chain fully settles.
        for _ in 0..8 {
            let out_b = drive_quiet("c2", b, &chan.b, quiet, net, plain)?;
            let out_a = drive_quiet("c2", a, &chan.a, quiet, net, plain)?;
            if out_a == 0 && out_b == 0 {
                break;
            }
        }
    }

    // Record the real stats of both sides.
    let (ta, txa, rxa) = a.stats()?;
    let (tb, txb, rxb) = b.stats()?;
    if ta < 0 || tb < 0 {
        return Err(ProbeError::new(
            "c2",
            format!("stats report no established session: A.time={ta} B.time={tb}"),
        ));
    }
    let stats_a = Stats { time_since_last_handshake: ta, tx_bytes: txa, rx_bytes: rxa };
    let stats_b = Stats { time_since_last_handshake: tb, tx_bytes: txb, rx_bytes: rxb };

    // Establishment proof: one full real data roundtrip A -> B (the plan's
    // "握手后首轮 write→read 成功" complement to the stats observable).
    let probe_pkt = synth_packet(DIR_A2B, 0, 0);
    send_data("c2", a, &chan.a, &probe_pkt, net)?;
    let got = recv_data("c2", b, &chan.b, net, plain)?;
    if got != PAYLOAD_SZ || plain[..got] != probe_pkt[..] {
        return Err(ProbeError::new("c2", "post-handshake write->read roundtrip mismatch"));
    }

    Ok((started.elapsed().as_secs_f64() * 1000.0, attempts, stats_a, stats_b))
}

/// C3 + C4: 10 rounds x 200 packets x 1024 B in each direction, strict
/// per-packet ping-pong (send one, receive exactly that one), whole-packet
/// byte equality. Returns `(pump_ms, verified_plaintext_bytes)`.
fn phase_integrity(
    a: &Tunnel,
    b: &Tunnel,
    chan: &Channel,
    net: &mut [u8; BUF_SZ],
    plain: &mut [u8; BUF_SZ],
    outcome: &mut ProbeOutcome,
) -> PhaseResult<(f64, u64)> {
    let started = Instant::now();
    let mut verified_bytes = 0u64;

    // Frozen C3(d) byte accounting: snapshot both tunnels' real tx/rx stats
    // immediately before the first plaintext write and re-read after the
    // last verified inner packet; each delta must equal the per-direction
    // inner plaintext byte total.
    let (.., txa0, rxa0) = a.stats()?;
    let (.., txb0, rxb0) = b.stats()?;
    let expected_inner_bytes =
        (ROUNDS * PACKETS_PER_ROUND * PAYLOAD_SZ) as u64; // per direction

    // Restore the long data-phase read timeout (handshake left it at 200ms).
    for (name, s) in [("A", &chan.a), ("B", &chan.b)] {
        s.set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| ProbeError::new("c3", format!("set_read_timeout {name}: {e}")))?;
    }

    for (dir, tx, rx, tx_sock, rx_sock) in [
        (DIR_A2B, a, b, &chan.a, &chan.b),
        (DIR_B2A, b, a, &chan.b, &chan.a),
    ] {
        for round in 0..ROUNDS {
            for seq in 0..PACKETS_PER_ROUND {
                let pkt = synth_packet(dir, round as u32, seq as u32);
                send_data("c3", tx, tx_sock, &pkt, net)?;
                let got = recv_data("c3", rx, rx_sock, net, plain)?;
                if got == PAYLOAD_SZ && plain[..got] == pkt[..] {
                    outcome.verified_packets_total += 1;
                    verified_bytes += got as u64;
                } else {
                    outcome.mismatch_count += 1;
                    if outcome.first_mismatch.is_none() {
                        let marker = parse_marker("c3", &plain[..got.min(plain.len())])
                            .map(|(d, r, s)| format!("dir={d} round={r} seq={s}"))
                            .unwrap_or_else(|_| "unparseable".to_string());
                        outcome.first_mismatch = Some(format!(
                            "dir={dir} round={round} seq={seq}: got {got} bytes, marker={marker}"
                        ));
                    }
                }
            }
        }
    }

    // Byte-accounting assertions (fail-closed): per direction the sender's
    // tx delta and the receiver's rx delta must equal the inner plaintext
    // bytes actually pumped (keepalives carry no inner bytes, so they cannot
    // skew the accounting).
    let (.., txa1, rxa1) = a.stats()?;
    let (.., txb1, rxb1) = b.stats()?;
    outcome.tx_bytes_delta_a = txa1 - txa0;
    outcome.rx_bytes_delta_a = rxa1 - rxa0;
    outcome.tx_bytes_delta_b = txb1 - txb0;
    outcome.rx_bytes_delta_b = rxb1 - rxb0;
    if outcome.tx_bytes_delta_a != expected_inner_bytes
        || outcome.rx_bytes_delta_b != expected_inner_bytes
        || outcome.tx_bytes_delta_b != expected_inner_bytes
        || outcome.rx_bytes_delta_a != expected_inner_bytes
    {
        return Err(ProbeError::new(
            "c3",
            format!(
                "tx/rx byte accounting mismatch: A.tx={} B.rx={} B.tx={} A.rx={} expected={expected_inner_bytes}",
                outcome.tx_bytes_delta_a,
                outcome.rx_bytes_delta_b,
                outcome.tx_bytes_delta_b,
                outcome.rx_bytes_delta_a
            ),
        ));
    }
    outcome.byte_accounting_ok = true;

    let pump_ms = started.elapsed().as_secs_f64() * 1000.0;
    Ok((pump_ms, verified_bytes))
}

/// C6: >= 3 no-data quiet gaps, each driven with real `wireguard_tick` calls
/// on both tunnels, session stats re-checked after every gap, plus a small
/// post-gap data burst proving the sessions still carry traffic.
fn phase_tick_gaps(
    a: &Tunnel,
    b: &Tunnel,
    chan: &Channel,
    net: &mut [u8; BUF_SZ],
    plain: &mut [u8; BUF_SZ],
    aux: &mut [u8; BUF_SZ],
    outcome: &mut ProbeOutcome,
) -> PhaseResult<()> {
    for gap in 1..=TICK_GAPS {
        // Frozen criteria C6: each no-data quiet gap is >= 1 s (1100 ms
        // leaves margin) so the keep_alive=1 timer genuinely expires and the
        // real `wireguard_tick` below emits a persistent keepalive
        // (op == WRITE_TO_NETWORK) that the channel delivers to the peer.
        std::thread::sleep(Duration::from_millis(1100));
        for (tunnel, sock) in [(a, &chan.a), (b, &chan.b)] {
            let (op, size) = tunnel.tick(aux);
            outcome.tick_call_count += 1;
            match op {
                OP_NETWORK => {
                    // A real protocol packet produced by update_timers
                    // (keepalive/retransmit path); deliver it to the peer.
                    outcome.tick_network_packets += 1;
                    sock.send(&aux[..size])
                        .map_err(|e| ProbeError::new("c6", format!("tick send: {e}")))?;
                }
                OP_DONE => {}
                OP_ERROR => {
                    return Err(ProbeError::new(
                        "c6",
                        format!(
                            "{}: wireguard_tick error code {size} ({})",
                            tunnel.name,
                            wg_error_name(size)
                        ),
                    ));
                }
                op => {
                    return Err(ProbeError::new(
                        "c6",
                        format!("{}: unexpected tick op {op}", tunnel.name),
                    ));
                }
            }
        }
        // Absorb any tick-produced protocol packets on both sides. A peer
        // reading a keepalive gets OP_DONE (consumed, nothing re-sent), so
        // this does NOT touch the tick counter: per the frozen criteria the
        // probe-side counter records only ticks that RETURNED
        // WRITE_TO_NETWORK (persistent keepalives actually emitted).
        let quiet = Duration::from_millis(50);
        drive_quiet("c6", b, &chan.b, quiet, net, plain)?;
        drive_quiet("c6", a, &chan.a, quiet, net, plain)?;
        outcome.tick_gap_count += 1;
        // The session must survive every gap (stats observable still valid).
        if !a.established()? || !b.established()? {
            outcome.tick_session_ok_after_each = false;
            return Err(ProbeError::new("c6", format!("session broken after tick gap {gap}")));
        }
    }

    // Post-gap proof burst: 8 byte-verified packets in each direction.
    for dir in [DIR_A2B, DIR_B2A] {
        let (tx, rx, tx_sock, rx_sock) = if dir == DIR_A2B {
            (a, b, &chan.a, &chan.b)
        } else {
            (b, a, &chan.b, &chan.a)
        };
        for seq in 0..8u32 {
            let pkt = synth_packet(dir, 0xF0, seq);
            send_data("c6", tx, tx_sock, &pkt, net)?;
            let got = recv_data("c6", rx, rx_sock, net, plain)?;
            if got != PAYLOAD_SZ || plain[..got] != pkt[..] {
                outcome.post_gap_burst_ok = false;
                return Err(ProbeError::new("c6", format!("post-gap burst mismatch seq={seq}")));
            }
        }
    }
    outcome.post_gap_burst_ok = true;
    Ok(())
}

/// C5 drain helper: receive and verify up to `max_packets` queued datagrams
/// from B. Returns the number drained (0 when the queue is empty/EAGAIN).
fn bp_drain(
    b: &Tunnel,
    chan_b: &UdpSocket,
    net: &mut [u8; BUF_SZ],
    plain: &mut [u8; BUF_SZ],
    received: &mut [u64; (BACKPRESSURE_BURST + 63) / 64],
    outcome: &mut ProbeOutcome,
) -> PhaseResult<usize> {
    let mut drained = 0usize;
    while drained < BACKPRESSURE_BURST {
        match chan_b.recv(net) {
            Ok(n) => {
                let (op, size) = b.read(&net[..n], plain);
                // Frozen criteria accept WRITE_TO_TUNNEL_IPV4 only (the
                // synthetic packets are IPv4); anything else is a fail.
                if op != OP_TUN_V4 {
                    return Err(ProbeError::new(
                        "c5",
                        format!("drain: unexpected read op {op} (size={size})"),
                    ));
                }
                let (dir, round, seq) = parse_marker("c5", &plain[..size])?;
                if dir != DIR_BACKPRESSURE || round != 0 || seq as usize >= BACKPRESSURE_BURST {
                    return Err(ProbeError::new(
                        "c5",
                        format!("drain: unexpected marker dir={dir} round={round} seq={seq}"),
                    ));
                }
                let expected = synth_packet(DIR_BACKPRESSURE, 0, seq);
                if size != PAYLOAD_SZ || plain[..size] != expected[..] {
                    outcome.bp_corrupted += 1;
                } else {
                    let slot = &mut received[(seq / 64) as usize];
                    let mask = 1u64 << (seq % 64);
                    if *slot & mask == 0 {
                        *slot |= mask;
                        outcome.bp_verified += 1;
                    } else {
                        outcome.bp_extra += 1;
                    }
                }
                drained += 1;
            }
            Err(_) => break, // queue empty (nonblocking WouldBlock)
        }
    }
    Ok(drained)
}

// ---------------------------------------------------------------------------
// Socket buffer control (C5) — raw setsockopt/getsockopt because std's
// UdpSocket exposes no socket-buffer API.
// ---------------------------------------------------------------------------

fn set_socket_buffer(sock: &UdpSocket, opt: i32, size: i32) -> bool {
    use std::os::unix::io::AsRawFd;
    unsafe {
        libc::setsockopt(
            sock.as_raw_fd(),
            libc::SOL_SOCKET,
            opt,
            &size as *const i32 as *const libc::c_void,
            std::mem::size_of::<i32>() as libc::socklen_t,
        ) == 0
    }
}

fn get_socket_buffer(sock: &UdpSocket, opt: i32) -> Option<i32> {
    use std::os::unix::io::AsRawFd;
    let mut val: i32 = 0;
    let mut len: libc::socklen_t = std::mem::size_of::<i32>() as libc::socklen_t;
    unsafe {
        let r = libc::getsockopt(
            sock.as_raw_fd(),
            libc::SOL_SOCKET,
            opt,
            &mut val as *mut i32 as *mut libc::c_void,
            &mut len,
        );
        if r == 0 {
            Some(val)
        } else {
            None
        }
    }
}

/// C5: shrink the channel socket buffers, blast a burst larger than the
/// receive buffer, observe the real saturation behavior, and complete all
/// rounds inside the timebox without losing or corrupting any received
/// packet (the pre-registered pass condition).
///
/// Frozen C5 trigger semantics (criteria r2): `backpressure_induced` is set
/// **only** by a real `errno in {EAGAIN, ENOBUFS}` returned by `sendto`.
/// A loopback peer whose receive queue is full typically makes the kernel
/// silently drop the datagram and still report success; those drops are
/// counted in `kernel_queue_drops` as a **pure observation field** and never
/// participate in the induced decision. Zero errno hits => the sub-item is
/// honestly reported as `not-triggered` (never as a verified backpressure).
///
/// UDP is datagram-oriented: writes are all-or-nothing, so partial writes
/// are structurally impossible and reported as zero.
fn phase_backpressure(
    a: &Tunnel,
    b: &Tunnel,
    chan: &Channel,
    net: &mut [u8; BUF_SZ],
    plain: &mut [u8; BUF_SZ],
    outcome: &mut ProbeOutcome,
) -> PhaseResult<()> {
    const MAX_RETRANSMIT_ROUNDS: u64 = 4096;

    let started = Instant::now();

    // Pre-registered trigger: small SO_SNDBUF on the sender and SO_RCVBUF on
    // the receiver. The kernel clamps to its own minimums; the effective
    // values are read back and reported.
    let _ = set_socket_buffer(&chan.a, libc::SO_SNDBUF, 4096);
    let _ = set_socket_buffer(&chan.b, libc::SO_RCVBUF, 4096);
    outcome.so_sndbuf = get_socket_buffer(&chan.a, libc::SO_SNDBUF);
    outcome.so_rcvbuf = get_socket_buffer(&chan.b, libc::SO_RCVBUF);

    chan.a
        .set_nonblocking(true)
        .map_err(|e| ProbeError::new("c5", format!("set_nonblocking A: {e}")))?;
    chan.b
        .set_nonblocking(true)
        .map_err(|e| ProbeError::new("c5", format!("set_nonblocking B: {e}")))?;

    let deadline = started + BACKPRESSURE_TIMEBOX;
    let mut received = [0u64; (BACKPRESSURE_BURST + 63) / 64];
    let mut successful_sends = 0u64;

    loop {
        let missing: Vec<usize> = (0..BACKPRESSURE_BURST)
            .filter(|&s| received[s / 64] >> (s % 64) & 1 == 0)
            .collect();
        if missing.is_empty() {
            break;
        }
        if Instant::now() > deadline {
            return Err(ProbeError::new(
                "c5",
                format!(
                    "backpressure timebox (10s) exceeded (deadlock): delivered={} of {BURST}",
                    outcome.bp_verified,
                    BURST = BACKPRESSURE_BURST
                ),
            ));
        }
        outcome.retransmit_rounds += 1;
        if outcome.retransmit_rounds > MAX_RETRANSMIT_ROUNDS {
            return Err(ProbeError::new(
                "c5",
                format!("retransmit rounds exceeded {MAX_RETRANSMIT_ROUNDS} (deadlock)"),
            ));
        }
        for &seq in &missing {
            if Instant::now() > deadline {
                return Err(ProbeError::new(
                    "c5",
                    format!(
                        "backpressure timebox (10s) exceeded (deadlock): delivered={} of {BURST}",
                        outcome.bp_verified,
                        BURST = BACKPRESSURE_BURST
                    ),
                ));
            }
            let pkt = synth_packet(DIR_BACKPRESSURE, 0, seq as u32);
            let (op, size) = a.write(&pkt, net);
            if op != OP_NETWORK {
                return Err(ProbeError::new(
                    "c5",
                    format!("bp write op={op} size={size} ({})", wg_error_name(size)),
                ));
            }
            outcome.send_attempts += 1;
            match chan.a.send(&net[..size]) {
                Ok(_) => successful_sends += 1,
                Err(e)
                    if e.raw_os_error() == Some(libc::EAGAIN)
                        || e.raw_os_error() == Some(libc::EWOULDBLOCK)
                        || e.raw_os_error() == Some(libc::ENOBUFS)
                        || e.kind() == io::ErrorKind::WouldBlock =>
                {
                    // The ONLY frozen trigger signal: a real errno in
                    // {EAGAIN, ENOBUFS} from sendto. (WouldBlock kind is
                    // EAGAIN on this platform; the raw checks keep the gate
                    // honest across mappings.)
                    outcome.eagain_count += 1;
                    bp_drain(b, &chan.b, net, plain, &mut received, outcome)?;
                }
                Err(e) => return Err(ProbeError::new("c5", format!("bp send: {e}"))),
            }
        }
        // Drain everything that landed in this round (queue -> verified).
        while bp_drain(b, &chan.b, net, plain, &mut received, outcome)? > 0 {}
    }

    // Queue is empty here (final drain above), so every successful send is
    // accounted: delivered (verified) + duplicates + corrupted; the rest
    // were kernel drops while the receive queue was full.
    outcome.bp_received = outcome.bp_verified + outcome.bp_corrupted as usize;
    outcome.kernel_queue_drops = successful_sends
        - (outcome.bp_verified as u64 + outcome.bp_extra + outcome.bp_corrupted);
    outcome.successful_sends = successful_sends;
    // Frozen C5: induced is decided ONLY by real errno hits (EAGAIN/ENOBUFS).
    // Kernel queue drops are a pure observation field and never induce.
    outcome.backpressure_triggered = outcome.eagain_count > 0;
    outcome.backpressure_ms = started.elapsed().as_secs_f64() * 1000.0;

    // Fail-closed accounting: every packet must be delivered intact within
    // the timebox; received packets must never be lost or corrupted
    // (duplicates are a benign ARQ artifact and are counted, not failed).
    if outcome.bp_received != BACKPRESSURE_BURST || outcome.bp_corrupted != 0 {
        return Err(ProbeError::new(
            "c5",
            format!(
                "backpressure accounting: delivered={} expected={} corrupted={} duplicates={} drops={}",
                outcome.bp_received,
                BACKPRESSURE_BURST,
                outcome.bp_corrupted,
                outcome.bp_extra,
                outcome.kernel_queue_drops
            ),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Resource snapshots (C7/C8)
// ---------------------------------------------------------------------------

fn snapshot_dir_count(path: &str) -> Option<usize> {
    // /proc/self/{fd,task} listing; the transient readdir fd appears in both
    // the baseline and the post snapshots, so the counts stay comparable.
    Some(fs::read_dir(path).ok()?.count())
}

// ---------------------------------------------------------------------------
// WireGuard error-code names (boringtun-0.7.1 noise/errors.rs declaration
// order, implicit discriminants 0..=16)
// ---------------------------------------------------------------------------

fn wg_error_name(code: usize) -> &'static str {
    const NAMES: [&str; 17] = [
        "DestinationBufferTooSmall",
        "IncorrectPacketLength",
        "UnexpectedPacket",
        "WrongPacketType",
        "WrongIndex",
        "WrongKey",
        "InvalidTai64nTimestamp",
        "WrongTai64nTimestamp",
        "InvalidMac",
        "InvalidAeadTag",
        "InvalidCounter",
        "DuplicateCounter",
        "InvalidPacket",
        "NoCurrentSession",
        "LockFailed",
        "ConnectionExpired",
        "UnderLoad",
    ];
    NAMES.get(code).copied().unwrap_or("Unknown")
}

// ---------------------------------------------------------------------------
// Top-level probe
// ---------------------------------------------------------------------------

struct Handles {
    a: Tunnel,
    b: Tunnel,
    chan: Channel,
}

/// Runs the complete N1a data-plane probe and returns the structured outcome.
///
/// Phases run in the pre-registered order (handshake -> integrity ->
/// tick gaps -> backpressure). A phase failure records its error, fails its
/// criterion, and stops; unmeasured criteria keep their fail-closed default.
/// Cleanup (tunnel_free x2 + socket close) and the post-run resource
/// snapshots always run.
pub fn run_probe() -> ProbeOutcome {
    let started = Instant::now();
    let fd_baseline = snapshot_dir_count("/proc/self/fd");
    let thread_baseline = snapshot_dir_count("/proc/self/task");
    let mut outcome = ProbeOutcome::new(fd_baseline, thread_baseline);

    // C1 (library load): reaching this code means the native member carrying
    // this probe is loaded, executing, and able to call the real BoringTun C
    // ABI. The authoritative load proof for the Emulator gate is the presence
    // of this result (NAPI marker / ohosTest completion): a library that
    // fails to dlopen can never emit it, so a missing result fails upstream.
    outcome.criteria[0] = CRIT_PASS;

    // Setup: two key pairs via the real ffi; peers are mutual inverses
    // (A's peer public key == B's public key and vice versa).
    let handles = (|| -> PhaseResult<Handles> {
        let keypair = |name: &'static str| -> PhaseResult<(String, String)> {
            let secret = boringtun::ffi::x25519_secret_key();
            let secret_bytes = secret.key;
            let public = boringtun::ffi::x25519_public_key(secret);
            let secret_b64_ptr =
                boringtun::ffi::x25519_key_to_base64(boringtun::ffi::x25519_key { key: secret_bytes });
            let public_b64_ptr = boringtun::ffi::x25519_key_to_base64(public);
            if secret_b64_ptr.is_null() || public_b64_ptr.is_null() {
                return Err(ProbeError::new(
                    "c2",
                    format!("x25519_key_to_base64({name}) returned null"),
                ));
            }
            let secret_b64 = unsafe { std::ffi::CStr::from_ptr(secret_b64_ptr) }
                .to_string_lossy()
                .into_owned();
            let public_b64 = unsafe { std::ffi::CStr::from_ptr(public_b64_ptr) }
                .to_string_lossy()
                .into_owned();
            unsafe { boringtun::ffi::x25519_key_to_str_free(secret_b64_ptr as *mut _) };
            unsafe { boringtun::ffi::x25519_key_to_str_free(public_b64_ptr as *mut _) };
            Ok((secret_b64, public_b64))
        };
        let (secret_a, public_a) = keypair("A")?;
        let (secret_b, public_b) = keypair("B")?;
        let a = Tunnel::new(&secret_a, &public_b, 1, "A")?;
        let b = Tunnel::new(&secret_b, &public_a, 2, "B")?;
        let chan = Channel::new("c2")?;
        Ok(Handles { a, b, chan })
    })();

    let handles = match handles {
        Ok(h) => h,
        Err(e) => {
            outcome.error = Some(format!("[{}] {}", e.criterion, e.message));
            finish(&mut outcome, started);
            return outcome;
        }
    };

    let mut net = [0u8; BUF_SZ];
    let mut plain = [0u8; BUF_SZ];
    let mut aux = [0u8; BUF_SZ];

    let phase_result = run_phases(&handles, &mut net, &mut plain, &mut aux, &mut outcome);

    // C8: cleanup always — tunnel_free x2 (via Drop), then close both
    // sockets, then the post-run snapshots in `finish`.
    drop(handles);
    outcome.tunnels_freed = 2;
    outcome.sockets_closed = 2;

    if let Err(e) = phase_result {
        let idx = criterion_index(e.criterion);
        outcome.criteria[idx] = CRIT_FAIL;
        outcome.error = Some(format!("[{}] {}", e.criterion, e.message));
    }

    finish(&mut outcome, started);
    outcome
}

/// Sequential pre-registered phases; each phase flips its own criterion to
/// pass only on real evidence.
fn run_phases(
    h: &Handles,
    net: &mut [u8; BUF_SZ],
    plain: &mut [u8; BUF_SZ],
    aux: &mut [u8; BUF_SZ],
    outcome: &mut ProbeOutcome,
) -> PhaseResult<()> {
    // C2: handshake establishment.
    let (hms, attempts, sa, sb) = phase_handshake(&h.a, &h.b, &h.chan, net, plain)?;
    outcome.handshake_ms = hms;
    outcome.handshake_attempts = attempts;
    outcome.stats_a_after = Some(sa);
    outcome.stats_b_after = Some(sb);
    outcome.first_roundtrip_ok = true;
    outcome.handshake_established = true;
    outcome.criteria[1] = CRIT_PASS;

    // C3 + C4: integrity + throughput.
    let (pump_ms, verified_bytes) = phase_integrity(&h.a, &h.b, &h.chan, net, plain, outcome)?;
    outcome.pump_ms = pump_ms;
    outcome.verified_plaintext_bytes = verified_bytes;
    outcome.throughput_mib_s = if pump_ms > 0.0 {
        verified_bytes as f64 / (pump_ms / 1000.0) / (1024.0 * 1024.0)
    } else {
        0.0
    };
    outcome.lost_count = outcome.expected_packets_total as u64
        - outcome.verified_packets_total as u64
        - outcome.mismatch_count;
    if outcome.mismatch_count == 0
        && outcome.lost_count == 0
        && outcome.verified_packets_total == outcome.expected_packets_total
    {
        outcome.criteria[2] = CRIT_PASS;
    }
    if outcome.throughput_mib_s >= THROUGHPUT_FLOOR_MIB_S {
        outcome.criteria[3] = CRIT_PASS;
    }

    // C6: tick paths at >= 3 no-data gaps.
    phase_tick_gaps(&h.a, &h.b, &h.chan, net, plain, aux, outcome)?;
    // Frozen C6: the probe-side tick counter must record >= 3 ticks that
    // returned op == WRITE_TO_NETWORK (real persistent keepalives with
    // keep_alive=1, delivered through the channel and consumed by the peer).
    if outcome.tick_gap_count >= TICK_GAPS
        && outcome.tick_network_packets >= 3
        && outcome.tick_session_ok_after_each
        && outcome.post_gap_burst_ok
    {
        outcome.criteria[5] = CRIT_PASS;
    }

    // C5: backpressure (last traffic phase; it shrinks socket buffers and
    // leaves them that way — nothing but cleanup follows).
    phase_backpressure(&h.a, &h.b, &h.chan, net, plain, outcome)?;
    if outcome.bp_corrupted == 0 && outcome.bp_received == BACKPRESSURE_BURST {
        if outcome.backpressure_triggered {
            outcome.criteria[4] = CRIT_PASS;
        } else {
            // No saturation observed on this loopback: honestly reported,
            // and per the gate plan ("未触发≠失败") this is not a failure.
            outcome.criteria[4] = CRIT_NOT_TRIGGERED;
        }
    }

    Ok(())
}

fn criterion_index(name: &str) -> usize {
    match name {
        "c1" => 0,
        "c2" => 1,
        "c3" => 2,
        "c4" => 3,
        "c5" => 4,
        "c6" => 5,
        "c7" => 6,
        "c8" => 7,
        _ => 8,
    }
}

/// Post-run accounting: resource snapshots + C7/C8/C9 verdicts + verdict
/// aggregation. Runs on every path (success and early exit).
///
/// Frozen C7: the post-cleanup T3 snapshot counts must be EXACTLY equal to
/// the pre-pump T0 baseline (`T3 == T0`); any difference — growth or
/// shrinkage — fails. (A downward drift is not "non-leak"; the frozen text
/// forbids reading it as pass.) The exact counts are recorded in the JSON.
fn finish(outcome: &mut ProbeOutcome, started: Instant) {
    outcome.fd_after = snapshot_dir_count("/proc/self/fd");
    outcome.thread_after = snapshot_dir_count("/proc/self/task");

    // C7: fd AND thread counts must equal the pre-pump baseline exactly.
    // /proc unavailable => fail closed.
    let (exact, fd_exact) = match (
        outcome.fd_baseline,
        outcome.fd_after,
        outcome.thread_baseline,
        outcome.thread_after,
    ) {
        (Some(fb), Some(fa), Some(tb), Some(ta)) => (fa == fb && ta == tb, fa == fb),
        _ => (false, false),
    };
    outcome.criteria[6] = if exact { CRIT_PASS } else { CRIT_FAIL };

    // C8: tunnels freed (tunnel_free x2), sockets closed, fd count back
    // exactly at the pre-pump baseline.
    let c8 = fd_exact && outcome.tunnels_freed == 2 && outcome.sockets_closed == 2;
    outcome.criteria[7] = if c8 { CRIT_PASS } else { CRIT_FAIL };

    // C9 (result page): the structured result (JSON + NAPI object + HiLog
    // marker + ohosTest PASS/FAIL page) is produced unconditionally from this
    // outcome; c9 passes when a complete verdict exists, which is always the
    // case once this function runs. The visible page itself is rendered by
    // the ohosTest entry from the same fields.
    outcome.criteria[8] = CRIT_PASS;

    outcome.total_ms = started.elapsed().as_secs_f64() * 1000.0;
    outcome.finalize();
}

// ---------------------------------------------------------------------------
// JSON serialization (hand-rolled; all strings are probe-controlled)
// ---------------------------------------------------------------------------

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn opt_num(v: Option<i64>) -> String {
    match v {
        Some(v) => v.to_string(),
        None => "null".to_string(),
    }
}

fn stats_json(s: &Option<Stats>) -> String {
    match s {
        Some(s) => format!(
            "{{\"time_since_last_handshake\":{},\"tx_bytes\":{},\"rx_bytes\":{}}}",
            s.time_since_last_handshake, s.tx_bytes, s.rx_bytes
        ),
        None => "null".to_string(),
    }
}

/// Renders the outcome as the full JSON detail document.
pub fn to_json(o: &ProbeOutcome) -> String {
    let err = match &o.error {
        Some(e) => format!("\"{}\"", json_escape(e)),
        None => "null".to_string(),
    };
    let mismatch = match &o.first_mismatch {
        Some(m) => format!("\"{}\"", json_escape(m)),
        None => "null".to_string(),
    };
    let mut s = String::with_capacity(2048);
    s.push('{');
    s.push_str(&format!("\"version\":\"{}\",", json_escape(PROBE_VERSION)));
    s.push_str(&format!("\"ok\":{},", o.ok));
    s.push_str(&format!("\"verdict\":\"{}\",", if o.ok { "pass" } else { "fail" }));
    s.push_str(&format!("\"error\":{err},"));
    s.push_str("\"criteria\":{");
    for (i, key) in ["c1", "c2", "c3", "c4", "c5", "c6", "c7", "c8", "c9"].iter().enumerate() {
        s.push_str(&format!("\"{}\":\"{}\",", key, ProbeOutcome::criterion_name(i, o.criteria[i])));
    }
    s.pop(); // trailing comma
    s.push_str("},");
    s.push_str(&format!("\"boringtun\":\"0.7.1\","));
    s.push_str(&format!("\"boringtun_crate_sha256\":\"{}\",", BORINGTUN_CRATE_SHA256));
    s.push_str(&format!(
        "\"handshake\":{{\"established\":{},\"elapsed_ms\":{:.3},\"attempts\":{},\"first_roundtrip_ok\":{},\"stats_a\":{},\"stats_b\":{}}},",
        o.handshake_established,
        o.handshake_ms,
        o.handshake_attempts,
        o.first_roundtrip_ok,
        stats_json(&o.stats_a_after),
        stats_json(&o.stats_b_after),
    ));
    s.push_str(&format!(
        "\"integrity\":{{\"rounds\":{},\"packets_per_round\":{},\"payload_bytes\":{},\"expected_packets_total\":{},\"verified_packets_total\":{},\"mismatches\":{},\"lost\":{},\"first_mismatch\":{},\"byte_accounting\":{{\"expected_inner_bytes_per_direction\":{},\"tx_bytes_delta_a\":{},\"rx_bytes_delta_b\":{},\"tx_bytes_delta_b\":{},\"rx_bytes_delta_a\":{},\"ok\":{}}}}},",
        ROUNDS,
        PACKETS_PER_ROUND,
        PAYLOAD_SZ,
        o.expected_packets_total,
        o.verified_packets_total,
        o.mismatch_count,
        o.lost_count,
        mismatch,
        (ROUNDS * PACKETS_PER_ROUND * PAYLOAD_SZ) as u64,
        o.tx_bytes_delta_a,
        o.rx_bytes_delta_b,
        o.tx_bytes_delta_b,
        o.rx_bytes_delta_a,
        o.byte_accounting_ok,
    ));
    s.push_str(&format!(
        "\"throughput\":{{\"verified_plaintext_bytes\":{},\"pump_ms\":{:.3},\"mib_per_second\":{:.6},\"floor_mib_per_second\":{}}},",
        o.verified_plaintext_bytes, o.pump_ms, o.throughput_mib_s, THROUGHPUT_FLOOR_MIB_S,
    ));
    s.push_str(&format!(
        "\"backpressure\":{{\"induced\":{},\"eagain_count\":{},\"send_attempts\":{},\"successful_sends\":{},\"retransmit_rounds\":{},\"kernel_queue_drops\":{},\"delivered_unique\":{},\"verified\":{},\"corrupted\":{},\"duplicates\":{},\"so_sndbuf\":{},\"so_rcvbuf\":{},\"elapsed_ms\":{:.3},\"partial_writes\":0,\"note\":\"induced is decided only by real errno EAGAIN/ENOBUFS from sendto; kernel_queue_drops (full receive queue, silent datagram drop with send success) is a pure observation field and never participates in the induced decision; zero errno hits = not-triggered; UDP datagrams have no partial writes\"}},",
        o.backpressure_triggered,
        o.eagain_count,
        o.send_attempts,
        o.successful_sends,
        o.retransmit_rounds,
        o.kernel_queue_drops,
        o.bp_received,
        o.bp_verified,
        o.bp_corrupted,
        o.bp_extra,
        opt_num(o.so_sndbuf.map(|v| v as i64)),
        opt_num(o.so_rcvbuf.map(|v| v as i64)),
        o.backpressure_ms,
    ));
    s.push_str(&format!(
        "\"tick\":{{\"quiet_gaps\":{},\"tick_calls\":{},\"network_packets\":{},\"session_ok_after_each_gap\":{},\"post_gap_burst_ok\":{}}},",
        o.tick_gap_count,
        o.tick_call_count,
        o.tick_network_packets,
        o.tick_session_ok_after_each,
        o.post_gap_burst_ok,
    ));
    s.push_str(&format!(
        "\"resources\":{{\"fd_baseline\":{},\"fd_after\":{},\"thread_baseline\":{},\"thread_after\":{},\"tunnels_freed\":{},\"sockets_closed\":{},\"fd_back_to_baseline\":{}}},",
        opt_num(o.fd_baseline.map(|v| v as i64)),
        opt_num(o.fd_after.map(|v| v as i64)),
        opt_num(o.thread_baseline.map(|v| v as i64)),
        opt_num(o.thread_after.map(|v| v as i64)),
        o.tunnels_freed,
        o.sockets_closed,
        o.fd_baseline.is_some() && o.fd_after <= o.fd_baseline,
    ));
    s.push_str(&format!(
        "\"timing\":{{\"handshake_ms\":{:.3},\"pump_ms\":{:.3},\"backpressure_ms\":{:.3},\"total_ms\":{:.3}}}",
        o.handshake_ms, o.pump_ms, o.backpressure_ms, o.total_ms,
    ));
    s.push('}');
    s
}
