//! N1BDISC WG — real BoringTun handshake + crypto round trip, in-process.
//!
//! Two REAL BoringTun tunnels live in this process: A (device side) and B
//! (peer side). The "network" between them is a pair of byte buffers — no
//! socket, no external peer, no platform API. The static keys are fixed
//! 32-byte test secrets fed through the frozen `x25519_public_key` export; the
//! handshake itself still draws its ephemeral keys from the crate's own
//! `OsRng` (getrandom syscall), which is one of the things verified on-device.
//!
//! Chain closed at the end: the plaintext A decrypts (b2a direction) is written
//! into the REAL TUN fd (fd_dup) and observed on a local sink socket bound to
//! 0.0.0.0:47003 — decrypted packet -> TUN -> kernel network stack -> socket.
//! 47003 (not D5's 47002) so the still-open D5 sink cannot steal or block it.
//!
//! Every stage emits its own `N1BDISC_WG_*` marker on the frozen channel, so a
//! panic (panic=abort kills the whole extension process) still leaves behind
//! the last stage that was reached.
//!
//! This is explicitly OUTSIDE the frozen P-chain: no ledger transition is
//! emitted for the sink socket (it is closed before returning), and the
//! D4/D5/D8/D-W calls in the ArkTS caller are untouched.
//!
//! `wg_net_probe` (added 2026-09-11) is the same tunnel driven by REAL UDP
//! datagrams: it binds 0.0.0.0:47010, answers the peer's handshake initiation
//! with `sendto`, decrypts the peer's first transport packet, and closes the
//! same TUN/sink loop. `wg_probe` (in-process) and `wg_udp_probe` (plain-UDP
//! reachability) are retained unchanged.
//!
//! `wg_fwd_probe` (added 2026-09-11) is the same tunnel as a REAL bidirectional
//! data plane: after answering the host's handshake it keeps running — `poll`
//! on the WG UDP socket AND the TUN fd, encrypting device-originated IPv4
//! frames towards the host and writing decapsulated plaintext back into the
//! real TUN, with `wireguard_tick` serviced every loop. `wg_probe` and
//! `wg_net_probe` are retained unchanged as recorded on-device regressions.

use std::ffi::CString;

use boringtun::ffi;

use crate::hilog::emit;
use crate::net::{self, d5_packet};
use crate::sys;
use crate::util::{hex_lower, jbool, jinum, jnum, jstr};

// ffi::result_type op codes (boringtun-0.7.1/src/ffi/mod.rs:34-45)
const OP_DONE: i32 = 0;
const OP_NETWORK: i32 = 1;
const OP_ERROR: i32 = 2;
const OP_TUN_V4: i32 = 4;

const BUF: usize = 2048;
/// Sink port for the TUN closure (D5's sink owns 47002 while this runs).
const SINK_PORT: u16 = 47003;
/// Frozen UDP source port of the probe packet (same as D5's).
const SRC_PORT: u16 = 47001;
/// Keepalive interval handed to `new_tunnel` (same value as N1a's pump).
const KEEP_ALIVE: u16 = 25;
/// Ping-pong rounds the handshake chain may take (init -> resp -> keepalive ->
/// done is 3 reads; the bound only exists so a wedged state cannot spin).
const HS_MAX_ROUNDS: u32 = 8;
/// TUN write/poll attempts (each: poll(POLLOUT,500ms) + write + poll(POLLIN,500ms)).
const TUN_ATTEMPTS: u32 = 3;

/// Fixed synthetic 32-byte test secrets — deliberately NOT secret material and
/// not the same as any real deployment key. A = 0x00..0x1f, B = 0xff..0xe0.
const SECRET_A: [u8; 32] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];
const SECRET_B: [u8; 32] = [
    0xff, 0xfe, 0xfd, 0xfc, 0xfb, 0xfa, 0xf9, 0xf8, 0xf7, 0xf6, 0xf5, 0xf4, 0xf3, 0xf2, 0xf1, 0xf0,
    0xef, 0xee, 0xed, 0xec, 0xeb, 0xea, 0xe9, 0xe8, 0xe7, 0xe6, 0xe5, 0xe4, 0xe3, 0xe2, 0xe1, 0xe0,
];

/// One owned BoringTun tunnel (`tunnel_free` exactly once, on drop).
struct Tunnel {
    ptr: *mut core::ffi::c_void,
    name: &'static str,
}

impl Tunnel {
    /// `secret_b64` / `peer_public_b64` are base64 x25519 keys; `index` seeds
    /// the local index space and must be unique per tunnel.
    fn new(secret_b64: &str, peer_public_b64: &str, index: u32, name: &'static str) -> Option<Tunnel> {
        let secret = CString::new(secret_b64).ok()?;
        let peer = CString::new(peer_public_b64).ok()?;
        let ptr = unsafe {
            ffi::new_tunnel(
                secret.as_ptr() as *const _,
                peer.as_ptr() as *const _,
                core::ptr::null(), // no preshared key
                KEEP_ALIVE,
                index,
            )
        };
        if ptr.is_null() {
            return None;
        }
        Some(Tunnel { ptr: ptr as *mut core::ffi::c_void, name })
    }

    fn write(&self, src: &[u8], dst: &mut [u8]) -> (i32, usize) {
        let r = unsafe {
            ffi::wireguard_write(
                self.ptr as *const _,
                src.as_ptr(),
                src.len() as u32,
                dst.as_mut_ptr(),
                dst.len() as u32,
            )
        };
        (r.op as i32, r.size)
    }

    fn read(&self, src: &[u8], dst: &mut [u8]) -> (i32, usize) {
        let r = unsafe {
            ffi::wireguard_read(
                self.ptr as *const _,
                src.as_ptr(),
                src.len() as u32,
                dst.as_mut_ptr(),
                dst.len() as u32,
            )
        };
        (r.op as i32, r.size)
    }

    fn force_handshake(&self, dst: &mut [u8]) -> (i32, usize) {
        let r = unsafe {
            ffi::wireguard_force_handshake(self.ptr as *const _, dst.as_mut_ptr(), dst.len() as u32)
        };
        (r.op as i32, r.size)
    }

    /// Periodic timer service (`wireguard_tick`, recommended ~100 ms cadence):
    /// emits keepalives, retransmits handshakes, rekeys. Produces a datagram
    /// only when one is due (usually `OP_DONE`).
    fn tick(&self, dst: &mut [u8]) -> (i32, usize) {
        let r = unsafe {
            ffi::wireguard_tick(self.ptr as *const _, dst.as_mut_ptr(), dst.len() as u32)
        };
        (r.op as i32, r.size)
    }

    /// `(time_since_last_handshake_seconds, tx_bytes, rx_bytes)`; -1 seconds
    /// means "no session yet" (ffi/mod.rs:381-396 — the unit is SECONDS).
    fn stats(&self) -> (i64, u64, u64) {
        let s = unsafe { ffi::wireguard_stats(self.ptr as *const _) };
        (s.time_since_last_handshake, s.tx_bytes as u64, s.rx_bytes as u64)
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        unsafe { ffi::tunnel_free(self.ptr as *mut _) };
        self.ptr = core::ptr::null_mut();
    }
}

/// base64 of a key through the frozen export (the returned C string is owned by
/// the ffi and must be handed back to `x25519_key_to_str_free`).
fn key_to_b64(k: ffi::x25519_key) -> Option<String> {
    let p = ffi::x25519_key_to_base64(ffi::x25519_key { key: k.key });
    if p.is_null() {
        return None;
    }
    let s = unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned();
    unsafe { ffi::x25519_key_to_str_free(p as *mut _) };
    Some(s)
}

/// D5-shaped 44-byte IPv4/UDP packet, re-pointed at the WG sink port. The IPv4
/// checksum does not cover the UDP header, so only bytes 22-23 change.
fn tun_packet(seq: u16, mb1: bool) -> [u8; 44] {
    let mut p = d5_packet(seq, mb1);
    p[22] = (SINK_PORT >> 8) as u8;
    p[23] = (SINK_PORT & 0xff) as u8;
    p
}

#[derive(Default)]
struct Outcome {
    a_pub8: String,
    b_pub8: String,
    init_op: i32,
    init_len: usize,
    hs_rounds: u32,
    hs_ok: bool,
    hs_time_s: i64,
    hs_time_b_s: i64,
    hs_ms: u64,
    a_tx: u64,
    a_rx: u64,
    b_tx: u64,
    b_rx: u64,
    ct_a2b: usize,
    rx_a2b: usize,
    match_a2b: bool,
    ct_b2a: usize,
    rx_b2a: usize,
    match_b2a: bool,
    tun_written: i64,
    tun_errno: i32,
    sink_bind_errno: i32,
    sink_recv: bool,
    sink_payload_match: bool,
    sink_src: String,
    reason: String,
}

/// Stage machine; every early return has already emitted the marker for the
/// stage it stopped in.
fn run(fd_dup: i32, mb1: bool) -> Outcome {
    let mut o = Outcome::default();

    // --- 1. fixed secrets -> real public keys through the frozen ffi --------
    let sk_a = ffi::x25519_key { key: SECRET_A };
    let sk_b = ffi::x25519_key { key: SECRET_B };
    let pk_a = ffi::x25519_public_key(ffi::x25519_key { key: sk_a.key });
    let pk_b = ffi::x25519_public_key(ffi::x25519_key { key: sk_b.key });
    let (a_sec_b64, b_sec_b64, a_pub_b64, b_pub_b64) = match (
        key_to_b64(sk_a),
        key_to_b64(sk_b),
        key_to_b64(pk_a),
        key_to_b64(pk_b),
    ) {
        (Some(sa), Some(sb), Some(pa), Some(pb)) => (sa, sb, pa, pb),
        _ => {
            o.reason = "key-to-base64-null".into();
            return o;
        }
    };
    o.a_pub8 = a_pub_b64[..a_pub_b64.len().min(8)].to_string();
    o.b_pub8 = b_pub_b64[..b_pub_b64.len().min(8)].to_string();
    emit(&format!("N1BDISC_WG_KEYS|a_pub={}|b_pub={}", o.a_pub8, o.b_pub8));

    // --- 2. two tunnels in this process -------------------------------------
    let a = match Tunnel::new(&a_sec_b64, &b_pub_b64, 1, "A") {
        Some(t) => t,
        None => {
            o.reason = "new_tunnel-a-null".into();
            return o;
        }
    };
    let b = match Tunnel::new(&b_sec_b64, &a_pub_b64, 2, "B") {
        Some(t) => t,
        None => {
            o.reason = "new_tunnel-b-null".into();
            return o;
        }
    };
    emit("N1BDISC_WG_TUNNELS|a=1|b=2|keep_alive=25");

    // --- 3. handshake: init -> resp -> keepalive -> done, buffer-to-buffer ---
    let mut hs_x = [0u8; BUF]; // holds whatever the last producing side wrote
    let mut hs_y = [0u8; BUF]; // the next read's output
    let hs_t0 = sys::mono_ms();
    let (mut op, mut len) = a.force_handshake(&mut hs_x);
    o.init_op = op;
    o.init_len = len;
    emit(&format!("N1BDISC_WG_HS_INIT|op={}|len={}", op, len));
    if op != OP_NETWORK {
        o.reason = format!("force-handshake-op-{}", op);
        return o;
    }
    let mut produced_by_a = true;
    while op == OP_NETWORK && o.hs_rounds < HS_MAX_ROUNDS {
        o.hs_rounds += 1;
        let (nop, nlen) = if produced_by_a {
            b.read(&hs_x[..len], &mut hs_y)
        } else {
            a.read(&hs_x[..len], &mut hs_y)
        };
        emit(&format!(
            "N1BDISC_WG_HS_STEP|round={}|consumer={}|op={}|len={}",
            o.hs_rounds,
            if produced_by_a { "b" } else { "a" },
            nop,
            nlen
        ));
        if nop == OP_ERROR {
            o.reason = format!("handshake-step-error-{}", nlen);
            return o;
        }
        op = nop;
        len = nlen;
        produced_by_a = !produced_by_a;
        core::mem::swap(&mut hs_x, &mut hs_y);
    }
    o.hs_ms = sys::mono_ms().saturating_sub(hs_t0);
    let (ta, atx, arx) = a.stats();
    let (tb, btx, brx) = b.stats();
    o.hs_time_s = ta;
    o.hs_time_b_s = tb;
    o.a_tx = atx;
    o.a_rx = arx;
    o.b_tx = btx;
    o.b_rx = brx;
    // Session-established observable: both sides report a last-handshake age
    // (>= 0 seconds; -1 = no session).
    o.hs_ok = ta >= 0 && tb >= 0;
    emit(&format!(
        "N1BDISC_WG_HS|time_ms={}|time_s={}|hs_ms={}|rounds={}|a_time={}|b_time={}|ok={}",
        ta.max(0) as u64 * 1000,
        ta,
        o.hs_ms,
        o.hs_rounds,
        ta,
        tb,
        o.hs_ok
    ));
    emit(&format!(
        "N1BDISC_WG_STATS|a_tx={}|a_rx={}|b_tx={}|b_rx={}",
        atx, arx, btx, brx
    ));

    // --- 4. encrypted round trip A -> B, then B -> A ------------------------
    let mut ct = [0u8; BUF];
    let mut pt = [0u8; BUF];

    let p_a2b = tun_packet(0x0101, mb1);
    let (wop, wlen) = a.write(&p_a2b, &mut ct);
    o.ct_a2b = wlen;
    emit(&format!(
        "N1BDISC_WG_TX|dir=a2b|len={}|ct_len={}|op={}",
        p_a2b.len(),
        wlen,
        wop
    ));
    if wop != OP_NETWORK || wlen == 0 || wlen > BUF {
        o.reason = format!("a2b-write-op-{}-len-{}", wop, wlen);
        return o;
    }
    let (rop, rlen) = b.read(&ct[..wlen], &mut pt);
    o.rx_a2b = rlen;
    o.match_a2b = rop == OP_TUN_V4 && rlen == p_a2b.len() && rlen <= BUF && pt[..rlen] == p_a2b[..];
    emit(&format!(
        "N1BDISC_WG_RX|dir=a2b|len={}|op={}|match={}",
        rlen, rop, o.match_a2b
    ));

    let p_b2a = tun_packet(0x0202, mb1);
    let (wop2, wlen2) = b.write(&p_b2a, &mut ct);
    o.ct_b2a = wlen2;
    emit(&format!(
        "N1BDISC_WG_TX|dir=b2a|len={}|ct_len={}|op={}",
        p_b2a.len(),
        wlen2,
        wop2
    ));
    if wop2 != OP_NETWORK || wlen2 == 0 || wlen2 > BUF {
        o.reason = format!("b2a-write-op-{}-len-{}", wop2, wlen2);
        return o;
    }
    let (rop2, rlen2) = a.read(&ct[..wlen2], &mut pt);
    o.rx_b2a = rlen2;
    o.match_b2a = rop2 == OP_TUN_V4 && rlen2 == p_b2a.len() && rlen2 <= BUF && pt[..rlen2] == p_b2a[..];
    emit(&format!(
        "N1BDISC_WG_RX|dir=b2a|len={}|op={}|match={}",
        rlen2, rop2, o.match_b2a
    ));

    // --- 5. closure: decrypted packet -> REAL TUN fd -> kernel stack --------
    // The b2a plaintext is what a device writes to its interface on the way in.
    if rlen2 < 44 || rlen2 > BUF {
        o.reason = format!("b2a-plaintext-len-{}", rlen2);
        return o;
    }
    let plain = &pt[..rlen2];
    let sink = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
    if sink < 0 {
        o.reason = format!("sink-socket-errno-{}", sys::errno());
        return o;
    }
    let bind_sa = sys::sockaddr_in::new([0, 0, 0, 0], SINK_PORT);
    let br = unsafe { sys::bind(sink, &bind_sa, core::mem::size_of::<sys::sockaddr_in>() as u32) };
    o.sink_bind_errno = if br == -1 { sys::errno() } else { 0 };

    let want_src = net::dst_peer(mb1);
    let mut attempt = 0u32;
    while attempt < TUN_ATTEMPTS && !o.sink_recv {
        attempt += 1;
        let _ = sys::poll1(fd_dup, sys::POLLOUT, 500);
        let (wn, we) = sys::write_fd(fd_dup, plain);
        o.tun_written = wn as i64;
        o.tun_errno = we;
        emit(&format!(
            "N1BDISC_WG_TUN_WRITE|attempt={}|written={}|errno={}|len={}",
            attempt, wn, we, plain.len()
        ));
        let (_pret, _pe, revents) = sys::poll1(sink, sys::POLLIN, 500);
        if (revents & sys::POLLIN) != 0 {
            let mut rbuf = [0u8; 2048];
            let mut from = sys::sockaddr_in::new([0, 0, 0, 0], 0);
            let mut flen = core::mem::size_of::<sys::sockaddr_in>() as u32;
            let rn = unsafe {
                sys::recvfrom(
                    sink,
                    rbuf.as_mut_ptr() as *mut core::ffi::c_void,
                    rbuf.len(),
                    0,
                    &mut from,
                    &mut flen,
                )
            };
            if rn > 0 {
                o.sink_src = format!(
                    "{}.{}.{}.{}:{}",
                    from.sin_addr[0],
                    from.sin_addr[1],
                    from.sin_addr[2],
                    from.sin_addr[3],
                    u16::from_be(from.sin_port)
                );
                // Kernel strips the IP/UDP headers: the socket sees the 16-byte
                // UDP payload of the decrypted packet, plus the frozen source.
                let identity = &plain[28..44];
                o.sink_payload_match =
                    from.sin_addr == want_src && u16::from_be(from.sin_port) == SRC_PORT && rn as usize == identity.len() && rbuf[..identity.len()] == *identity;
                o.sink_recv = o.sink_payload_match;
                emit(&format!(
                    "N1BDISC_WG_TUN_RECV|attempt={}|rn={}|src={}|payload_match={}",
                    attempt, rn, o.sink_src, o.sink_payload_match
                ));
            }
        }
    }
    unsafe { sys::close(sink) };
    emit(&format!(
        "N1BDISC_WG_TUN|written={}|errno={}|bind_errno={}|sink_recv={}|src={}|attempts={}",
        o.tun_written, o.tun_errno, o.sink_bind_errno, o.sink_recv, o.sink_src, attempt
    ));

    o
}

/// `wg_probe(fdDup: number, mb1: boolean) -> string` (JSON).
pub fn wg_probe(fd_dup: i32, mb1: bool) -> String {
    emit("N1BDISC_WG_BEGIN|");
    let t0 = sys::mono_ms();
    let o = run(fd_dup, mb1);

    let mut fails: Vec<&str> = Vec::new();
    if !o.hs_ok {
        fails.push("hs");
    }
    if !o.match_a2b {
        fails.push("a2b");
    }
    if !o.match_b2a {
        fails.push("b2a");
    }
    if !o.sink_recv {
        fails.push("tun");
    }
    let verdict = if fails.is_empty() { "pass" } else { "fail" };
    let reason = if !o.reason.is_empty() {
        o.reason.clone()
    } else if fails.is_empty() {
        "ok".to_string()
    } else {
        fails.join("+")
    };
    let elapsed = sys::mono_ms().saturating_sub(t0);
    emit(&format!(
        "N1BDISC_WG_END|verdict={}|reason={}|elapsed_ms={}",
        verdict, reason, elapsed
    ));

    let mut j = String::from("{");
    j.push_str(&jstr("a_pub8", &o.a_pub8));
    j.push_str(",");
    j.push_str(&jstr("b_pub8", &o.b_pub8));
    j.push_str(",");
    j.push_str(&jnum("init_op", o.init_op.max(0) as u64));
    j.push_str(",");
    j.push_str(&jnum("init_len", o.init_len as u64));
    j.push_str(",");
    j.push_str(&jnum("hs_rounds", o.hs_rounds as u64));
    j.push_str(",");
    j.push_str(&jbool("hs_ok", o.hs_ok));
    j.push_str(",");
    j.push_str(&format!("\"hs_time_s\":{}", o.hs_time_s));
    j.push_str(",");
    j.push_str(&jnum("hs_ms", o.hs_ms));
    j.push_str(",");
    j.push_str(&jnum("a_tx", o.a_tx));
    j.push_str(",");
    j.push_str(&jnum("a_rx", o.a_rx));
    j.push_str(",");
    j.push_str(&jnum("b_tx", o.b_tx));
    j.push_str(",");
    j.push_str(&jnum("b_rx", o.b_rx));
    j.push_str(",");
    j.push_str(&jnum("ct_a2b", o.ct_a2b as u64));
    j.push_str(",");
    j.push_str(&jnum("rx_a2b", o.rx_a2b as u64));
    j.push_str(",");
    j.push_str(&jbool("match_a2b", o.match_a2b));
    j.push_str(",");
    j.push_str(&jnum("ct_b2a", o.ct_b2a as u64));
    j.push_str(",");
    j.push_str(&jnum("rx_b2a", o.rx_b2a as u64));
    j.push_str(",");
    j.push_str(&jbool("match_b2a", o.match_b2a));
    j.push_str(",");
    j.push_str(&format!("\"tun_written\":{}", o.tun_written));
    j.push_str(",");
    j.push_str(&format!("\"tun_errno\":{}", o.tun_errno));
    j.push_str(",");
    j.push_str(&format!("\"sink_bind_errno\":{}", o.sink_bind_errno));
    j.push_str(",");
    j.push_str(&jbool("sink_recv", o.sink_recv));
    j.push_str(",");
    j.push_str(&jstr("sink_src", &o.sink_src));
    j.push_str(",");
    j.push_str(&jstr("verdict", verdict));
    j.push_str(",");
    j.push_str(&jstr("reason", &reason));
    j.push_str(",");
    j.push_str(&jnum("elapsed_ms", elapsed));
    j.push('}');
    j
}

/// JSON result of the UDP reachability probe (flat fields only).
#[allow(clippy::too_many_arguments)]
fn udp_json(
    verdict: &str,
    bind_rc: i32,
    bind_errno: i32,
    recv_len: isize,
    src: &str,
    sent: isize,
    send_errno: i32,
    waited_ms: u64,
) -> String {
    let mut j = String::from("{");
    j.push_str(&jstr("verdict", verdict));
    j.push_str(",");
    j.push_str(&format!("\"bind_rc\":{}", bind_rc));
    j.push_str(",");
    j.push_str(&format!("\"bind_errno\":{}", bind_errno));
    j.push_str(",");
    j.push_str(&format!("\"recv_len\":{}", recv_len));
    j.push_str(",");
    j.push_str(&jstr("src", src));
    j.push_str(",");
    j.push_str(&format!("\"sent\":{}", sent));
    j.push_str(",");
    j.push_str(&format!("\"send_errno\":{}", send_errno));
    j.push_str(",");
    j.push_str(&jnum("waited_ms", waited_ms));
    j.push('}');
    j
}

/// UDP reachability probe (out-of-gate, added 2026-09-11): can a UDP datagram
/// sent from the host to this device's address be received by a socket bound
/// inside this VPN extension process, and can the extension send a reply back
/// to the sender?
///
/// Binds 0.0.0.0:47010, waits up to 90 s for the FIRST datagram only, replies
/// to its source with a 16-byte literal, then closes the socket. Every stage
/// emits an `N1BDISC_WG_UDP_*` marker, so a timeout is distinguishable from a
/// socket/bind failure. Outside the frozen P-chain: no ledger transition, and
/// the caller's D4/D5/D8/D-W sequence is untouched.
///
/// `wg_udp_probe() -> string` (JSON).
pub fn wg_udp_probe() -> String {
    const PORT: u16 = 47010;
    const WAIT_MS: u64 = 90_000;
    /// 16-byte reply payload carrying a recognizable literal.
    const REPLY: &[u8] = b"N1BDISCUDP-47010";

    let t0 = sys::mono_ms();

    let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
    if fd < 0 {
        let e = sys::errno();
        emit(&format!(
            "N1BDISC_WG_UDP_BEGIN|port={}|bind_rc=-1|errno={}",
            PORT, e
        ));
        let waited = sys::mono_ms().saturating_sub(t0);
        emit(&format!(
            "N1BDISC_WG_UDP_END|verdict=socket_fail|waited_ms={}|errno={}",
            waited, e
        ));
        return udp_json("socket_fail", -1, e, -1, "", -1, e, waited);
    }

    let bind_sa = sys::sockaddr_in::new([0, 0, 0, 0], PORT);
    let bind_rc = unsafe {
        sys::bind(
            fd,
            &bind_sa,
            core::mem::size_of::<sys::sockaddr_in>() as u32,
        )
    };
    let bind_errno = if bind_rc == -1 { sys::errno() } else { 0 };
    emit(&format!(
        "N1BDISC_WG_UDP_BEGIN|port={}|bind_rc={}|errno={}",
        PORT, bind_rc, bind_errno
    ));

    let mut verdict = "timeout";
    let mut recv_len: isize = -1;
    let mut src = String::new();
    let mut sent: isize = -1;
    let mut send_errno = 0;

    if bind_rc == 0 {
        let deadline = t0 + WAIT_MS;
        loop {
            let now = sys::mono_ms();
            if now >= deadline {
                break;
            }
            let (ret, e, revents) = sys::poll1(fd, sys::POLLIN, (deadline - now) as i32);
            if ret > 0 {
                if (revents & sys::POLLIN) == 0 {
                    verdict = "poll_no_pollin";
                    break;
                }
                let mut rbuf = [0u8; 2048];
                let mut from = sys::sockaddr_in::new([0, 0, 0, 0], 0);
                let mut flen = core::mem::size_of::<sys::sockaddr_in>() as u32;
                let rn = unsafe {
                    sys::recvfrom(
                        fd,
                        rbuf.as_mut_ptr() as *mut core::ffi::c_void,
                        rbuf.len(),
                        0,
                        &mut from,
                        &mut flen,
                    )
                };
                if rn < 0 {
                    verdict = "recv_error";
                    send_errno = sys::errno();
                    break;
                }
                recv_len = rn;
                src = format!(
                    "{}.{}.{}.{}:{}",
                    from.sin_addr[0],
                    from.sin_addr[1],
                    from.sin_addr[2],
                    from.sin_addr[3],
                    u16::from_be(from.sin_port)
                );
                emit(&format!("N1BDISC_WG_UDP_RECV|len={}|src={}", recv_len, src));
                let flen_out = core::mem::size_of::<sys::sockaddr_in>() as u32;
                let sn = unsafe {
                    sys::sendto(
                        fd,
                        REPLY.as_ptr() as *const core::ffi::c_void,
                        REPLY.len(),
                        0,
                        &from,
                        flen_out,
                    )
                };
                sent = sn;
                send_errno = if sn == -1 { sys::errno() } else { 0 };
                emit(&format!(
                    "N1BDISC_WG_UDP_REPLY|sent={}|errno={}",
                    sent, send_errno
                ));
                verdict = if sn as usize == REPLY.len() {
                    "pass"
                } else {
                    "recv_no_reply"
                };
                break;
            }
            if ret == 0 {
                break; // 90 s elapsed with nothing received
            }
            if e != sys::EINTR {
                verdict = "poll_error";
                send_errno = e;
                break;
            }
        }
    } else {
        verdict = "bind_fail";
    }

    unsafe { sys::close(fd) };
    let waited = sys::mono_ms().saturating_sub(t0);
    emit(&format!(
        "N1BDISC_WG_UDP_END|verdict={}|waited_ms={}|recv_len={}|src={}|sent={}|send_errno={}|bind_rc={}|bind_errno={}",
        verdict, waited, recv_len, src, sent, send_errno, bind_rc, bind_errno
    ));

    udp_json(
        verdict, bind_rc, bind_errno, recv_len, &src, sent, send_errno, waited,
    )
}

// ---------------------------------------------------------------------------
// wg_net_probe — the same BoringTun tunnel driven by REAL UDP datagrams
// ---------------------------------------------------------------------------

/// Device-side (this process) fixed test secret: 32 bytes of 0x11. Synthetic,
/// not deployment material (same convention as SECRET_A/SECRET_B above).
const NET_DEV_SECRET: [u8; 32] = [0x11; 32];
/// Peer (host) fixed test secret: 32 bytes of 0x22. The peer's PUBLIC key is
/// derived from it here, through the same frozen `x25519_public_key` export, so
/// the two sides cannot end up with different keys.
const NET_HOST_SECRET: [u8; 32] = [0x22; 32];
/// UDP port this probe binds and the peer sends to. Owned solely by this probe:
/// `wg_udp_probe` uses the same port and is therefore no longer called from the
/// ArkTS chain (see the note at its call site).
const NET_PORT: u16 = 47010;
/// Whole-loop bound: no datagram for 90 s -> verdict=timeout.
const NET_WAIT_MS: u64 = 90_000;
/// One poll slice, so the 90 s bound is re-checked every 500 ms.
const NET_POLL_MS: i32 = 500;
/// Local index seeded into this tunnel's index space.
const NET_INDEX: u32 = 1;
/// Sequence frozen into the inner packet the peer must send (bytes 37-38, BE).
const NET_SEQ: u16 = 0x0001;

/// The 44-byte inner IPv4/UDP packet the peer is expected to send through the
/// tunnel: D5's frozen IPv4/UDP framing (`net::d5_packet`, payload magic
/// "N1DISCD5") with the UDP destination re-pointed at this probe's sink port
/// (47003 — 47002 is held by the still-open D5 sink) and the peer's 5-byte text
/// payload ("hello") in the last 5 bytes. The IPv4 checksum covers only the
/// 20-byte header, so the dport/payload edits leave it valid.
fn net_packet(seq: u16, mb1: bool) -> [u8; 44] {
    let mut p = d5_packet(seq, mb1);
    p[22] = (SINK_PORT >> 8) as u8;
    p[23] = (SINK_PORT & 0xff) as u8;
    p[39..44].copy_from_slice(b"hello");
    p
}

/// `a.b.c.d:port` of a sockaddr_in (network-order port decoded).
fn sa_str(sa: &sys::sockaddr_in) -> String {
    format!(
        "{}.{}.{}.{}:{}",
        sa.sin_addr[0],
        sa.sin_addr[1],
        sa.sin_addr[2],
        sa.sin_addr[3],
        u16::from_be(sa.sin_port)
    )
}

#[derive(Default)]
struct NetOutcome {
    dev_pub: String,
    host_pub: String,
    bind_rc: i32,
    bind_errno: i32,
    recv_count: u32,
    recv_len: isize,
    peer: String,
    send_count: u32,
    send_len: isize,
    send_errno: i32,
    hs_time_s: i64,
    hs_ok: bool,
    decrypt_len: usize,
    decrypt_first8: String,
    plain_match: bool,
    tun_written: i64,
    tun_errno: i32,
    sink_bind_errno: i32,
    sink_recv: bool,
    sink_payload_match: bool,
    sink_src: String,
    verdict: String,
    reason: String,
    waited_ms: u64,
}

/// Stage machine; every exit path has already emitted its own marker and left
/// `verdict`/`reason` set.
fn run_net(fd_dup: i32, mb1: bool) -> NetOutcome {
    let mut o = NetOutcome::default();
    let t0 = sys::mono_ms();

    // --- 1. socket + bind 0.0.0.0:47010 -------------------------------------
    let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
    if fd < 0 {
        o.bind_rc = -1;
        o.bind_errno = sys::errno();
        emit(&format!(
            "N1BDISC_WG_NET_BEGIN|port={}|bind_rc=-1|errno={}",
            NET_PORT, o.bind_errno
        ));
        o.verdict = "socket_fail".into();
        o.reason = "socket".into();
        o.waited_ms = sys::mono_ms().saturating_sub(t0);
        return o;
    }
    let bind_sa = sys::sockaddr_in::new([0, 0, 0, 0], NET_PORT);
    o.bind_rc =
        unsafe { sys::bind(fd, &bind_sa, core::mem::size_of::<sys::sockaddr_in>() as u32) };
    o.bind_errno = if o.bind_rc == -1 { sys::errno() } else { 0 };
    emit(&format!(
        "N1BDISC_WG_NET_BEGIN|port={}|bind_rc={}|errno={}",
        NET_PORT, o.bind_rc, o.bind_errno
    ));
    if o.bind_rc != 0 {
        unsafe { sys::close(fd) };
        o.verdict = "bind_fail".into();
        o.reason = "bind".into();
        o.waited_ms = sys::mono_ms().saturating_sub(t0);
        return o;
    }

    // --- 2. fixed secrets -> real public keys -> one tunnel ------------------
    let dev_sk = ffi::x25519_key { key: NET_DEV_SECRET };
    let dev_pk = ffi::x25519_public_key(ffi::x25519_key { key: NET_DEV_SECRET });
    let host_pk = ffi::x25519_public_key(ffi::x25519_key { key: NET_HOST_SECRET });
    let (dev_sec_b64, dev_pub_b64, host_pub_b64) =
        match (key_to_b64(dev_sk), key_to_b64(dev_pk), key_to_b64(host_pk)) {
            (Some(s), Some(dp), Some(hp)) => (s, dp, hp),
            _ => {
                unsafe { sys::close(fd) };
                o.verdict = "key_b64_null".into();
                o.reason = "key-to-base64-null".into();
                o.waited_ms = sys::mono_ms().saturating_sub(t0);
                return o;
            }
        };
    o.dev_pub = dev_pub_b64.clone();
    o.host_pub = host_pub_b64.clone();
    emit(&format!(
        "N1BDISC_WG_NET_KEYS|dev_pub={}|host_pub={}",
        dev_pub_b64, host_pub_b64
    ));

    let tunnel = match Tunnel::new(&dev_sec_b64, &host_pub_b64, NET_INDEX, "NET") {
        Some(t) => t,
        None => {
            unsafe { sys::close(fd) };
            o.verdict = "new_tunnel_null".into();
            o.reason = "new_tunnel-null".into();
            o.waited_ms = sys::mono_ms().saturating_sub(t0);
            return o;
        }
    };
    emit(&format!(
        "N1BDISC_WG_NET_TUNNEL|keep_alive={}|idx={}",
        KEEP_ALIVE, NET_INDEX
    ));

    let expected = net_packet(NET_SEQ, mb1);

    // --- 3. bounded receive loop: real datagrams -> tunnel -------------------
    let deadline = t0 + NET_WAIT_MS;
    let mut rbuf = [0u8; BUF];
    let mut out = [0u8; BUF];
    let mut decrypted = false;

    while sys::mono_ms() < deadline {
        let (ret, e, revents) = sys::poll1(fd, sys::POLLIN, NET_POLL_MS);
        if ret == 0 {
            continue; // 500 ms slice elapsed; re-check the deadline
        }
        if ret < 0 {
            if e == sys::EINTR {
                continue;
            }
            o.verdict = "poll_error".into();
            o.reason = format!("poll-{}", e);
            break;
        }
        if (revents & sys::POLLIN) == 0 {
            continue;
        }

        let mut from = sys::sockaddr_in::new([0, 0, 0, 0], 0);
        let mut flen = core::mem::size_of::<sys::sockaddr_in>() as u32;
        let rn = unsafe {
            sys::recvfrom(
                fd,
                rbuf.as_mut_ptr() as *mut core::ffi::c_void,
                rbuf.len(),
                0,
                &mut from,
                &mut flen,
            )
        };
        if rn < 0 {
            o.verdict = "recv_error".into();
            o.reason = format!("recvfrom-{}", sys::errno());
            break;
        }
        let n = rn as usize;
        if n == 0 || n > BUF {
            o.verdict = "recv_bad_len".into();
            o.reason = format!("recv-len-{}", n);
            break;
        }
        o.recv_count += 1;
        o.recv_len = rn;
        o.peer = sa_str(&from);
        emit(&format!("N1BDISC_WG_NET_RECV|len={}|src={}", rn, o.peer));

        let (op, len) = tunnel.read(&rbuf[..n], &mut out);
        let (hs_time, _tx, _rx) = tunnel.stats();
        o.hs_time_s = hs_time;
        o.hs_ok = hs_time >= 0;
        emit(&format!("N1BDISC_WG_NET_HS|time={}|ok={}", hs_time, o.hs_ok));

        match op {
            OP_NETWORK => {
                if len == 0 || len > BUF {
                    o.verdict = "net_out_bad_len".into();
                    o.reason = format!("network-out-len-{}", len);
                    break;
                }
                let addrlen = core::mem::size_of::<sys::sockaddr_in>() as u32;
                // Reply to the source of the datagram just processed — that is
                // the peer's endpoint (ip:port), recorded above as `o.peer`.
                let sn = unsafe {
                    sys::sendto(
                        fd,
                        out.as_ptr() as *const core::ffi::c_void,
                        len,
                        0,
                        &from,
                        addrlen,
                    )
                };
                o.send_count += 1;
                o.send_len = sn;
                o.send_errno = if sn == -1 { sys::errno() } else { 0 };
                emit(&format!(
                    "N1BDISC_WG_NET_SEND|len={}|op={}|sent={}|errno={}",
                    len, op, sn, o.send_errno
                ));
                if sn < 0 {
                    o.verdict = "send_error".into();
                    o.reason = format!("sendto-{}", o.send_errno);
                    break;
                }
            }
            OP_TUN_V4 => {
                o.decrypt_len = len;
                o.decrypt_first8 = hex_lower(&out[..len.min(8)]);
                o.plain_match = len == expected.len() && out[..len] == expected[..];
                emit(&format!(
                    "N1BDISC_WG_NET_DECRYPT|len={}|first8={}|match={}",
                    len, o.decrypt_first8, o.plain_match
                ));
                decrypted = true;
                break;
            }
            _ => {
                // OP_DONE (keepalive absorbed), OP_ERROR, or a new op: record it
                // rather than guess. OP_ERROR is terminal.
                emit(&format!("N1BDISC_WG_NET_OP|op={}|len={}", op, len));
                if op == OP_ERROR {
                    o.verdict = "tunnel_error".into();
                    o.reason = format!("wg-read-op-{}", op);
                    break;
                }
            }
        }
    }

    // --- 4. closure: decrypted packet -> REAL TUN fd -> kernel stack ---------
    if decrypted && o.decrypt_len >= 44 {
        let plain = &out[..o.decrypt_len];
        let sink = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
        if sink < 0 {
            o.verdict = "sink_socket_fail".into();
            o.reason = format!("sink-socket-{}", sys::errno());
        } else {
            let sink_sa = sys::sockaddr_in::new([0, 0, 0, 0], SINK_PORT);
            let br = unsafe {
                sys::bind(
                    sink,
                    &sink_sa,
                    core::mem::size_of::<sys::sockaddr_in>() as u32,
                )
            };
            o.sink_bind_errno = if br == -1 { sys::errno() } else { 0 };
            let want_src = net::dst_peer(mb1);
            let identity = &plain[28..44];
            let mut attempt = 0u32;
            while attempt < TUN_ATTEMPTS && !o.sink_recv {
                attempt += 1;
                let _ = sys::poll1(fd_dup, sys::POLLOUT, 500);
                let (wn, we) = sys::write_fd(fd_dup, plain);
                o.tun_written = wn as i64;
                o.tun_errno = we;
                emit(&format!(
                    "N1BDISC_WG_NET_TUN_WRITE|attempt={}|written={}|errno={}|len={}",
                    attempt,
                    wn,
                    we,
                    plain.len()
                ));
                let (_pret, _pe, revents) = sys::poll1(sink, sys::POLLIN, 500);
                if (revents & sys::POLLIN) != 0 {
                    let mut srbuf = [0u8; 2048];
                    let mut sfrom = sys::sockaddr_in::new([0, 0, 0, 0], 0);
                    let mut sflen = core::mem::size_of::<sys::sockaddr_in>() as u32;
                    let srn = unsafe {
                        sys::recvfrom(
                            sink,
                            srbuf.as_mut_ptr() as *mut core::ffi::c_void,
                            srbuf.len(),
                            0,
                            &mut sfrom,
                            &mut sflen,
                        )
                    };
                    if srn > 0 {
                        o.sink_src = sa_str(&sfrom);
                        o.sink_payload_match = sfrom.sin_addr == want_src
                            && u16::from_be(sfrom.sin_port) == SRC_PORT
                            && srn as usize == identity.len()
                            && srbuf[..identity.len()] == *identity;
                        o.sink_recv = o.sink_payload_match;
                        emit(&format!(
                            "N1BDISC_WG_NET_TUN_RECV|attempt={}|rn={}|src={}|payload_match={}",
                            attempt, srn, o.sink_src, o.sink_payload_match
                        ));
                    }
                }
            }
            unsafe { sys::close(sink) };
        }
        emit(&format!(
            "N1BDISC_WG_NET_TUN|written={}|errno={}|bind_errno={}|sink_recv={}|src={}",
            o.tun_written, o.tun_errno, o.sink_bind_errno, o.sink_recv, o.sink_src
        ));
        if o.verdict.is_empty() {
            if o.plain_match && o.sink_recv {
                o.verdict = "pass".into();
                o.reason = "ok".into();
            } else {
                let mut fails: Vec<&str> = Vec::new();
                if !o.plain_match {
                    fails.push("plaintext");
                }
                if !o.sink_recv {
                    fails.push("tun");
                }
                o.verdict = "fail".into();
                o.reason = fails.join("+");
            }
        }
    } else if decrypted {
        o.verdict = "fail".into();
        o.reason = format!("plaintext-too-short-{}", o.decrypt_len);
    } else if o.verdict.is_empty() {
        o.verdict = "timeout".into();
        o.reason = if o.recv_count == 0 { "no-packet" } else { "no-plaintext" }.into();
    }

    unsafe { sys::close(fd) };
    o.waited_ms = sys::mono_ms().saturating_sub(t0);
    emit(&format!(
        "N1BDISC_WG_NET_END|verdict={}|reason={}|elapsed_ms={}|waited_ms={}",
        o.verdict, o.reason, o.waited_ms, o.waited_ms
    ));
    o
}

/// `wg_net_probe(fdDup: number, mb1: boolean) -> string` (JSON).
pub fn wg_net_probe(fd_dup: i32, mb1: bool) -> String {
    emit("N1BDISC_WG_NET_ENTER|");
    let o = run_net(fd_dup, mb1);

    let mut j = String::from("{");
    j.push_str(&jstr("dev_pub", &o.dev_pub));
    j.push_str(",");
    j.push_str(&jstr("host_pub", &o.host_pub));
    j.push_str(",");
    j.push_str(&format!("\"bind_rc\":{}", o.bind_rc));
    j.push_str(",");
    j.push_str(&format!("\"bind_errno\":{}", o.bind_errno));
    j.push_str(",");
    j.push_str(&jnum("recv_count", o.recv_count as u64));
    j.push_str(",");
    j.push_str(&format!("\"recv_len\":{}", o.recv_len));
    j.push_str(",");
    j.push_str(&jstr("peer", &o.peer));
    j.push_str(",");
    j.push_str(&jnum("send_count", o.send_count as u64));
    j.push_str(",");
    j.push_str(&format!("\"send_len\":{}", o.send_len));
    j.push_str(",");
    j.push_str(&format!("\"send_errno\":{}", o.send_errno));
    j.push_str(",");
    j.push_str(&jinum("hs_time_s", o.hs_time_s));
    j.push_str(",");
    j.push_str(&jbool("hs_ok", o.hs_ok));
    j.push_str(",");
    j.push_str(&jnum("decrypt_len", o.decrypt_len as u64));
    j.push_str(",");
    j.push_str(&jstr("decrypt_first8", &o.decrypt_first8));
    j.push_str(",");
    j.push_str(&jbool("match", o.plain_match));
    j.push_str(",");
    j.push_str(&jinum("tun_written", o.tun_written));
    j.push_str(",");
    j.push_str(&format!("\"tun_errno\":{}", o.tun_errno));
    j.push_str(",");
    j.push_str(&format!("\"sink_bind_errno\":{}", o.sink_bind_errno));
    j.push_str(",");
    j.push_str(&jbool("sink_recv", o.sink_recv));
    j.push_str(",");
    j.push_str(&jbool("sink_payload_match", o.sink_payload_match));
    j.push_str(",");
    j.push_str(&jstr("sink_src", &o.sink_src));
    j.push_str(",");
    j.push_str(&jstr("verdict", &o.verdict));
    j.push_str(",");
    j.push_str(&jstr("reason", &o.reason));
    j.push_str(",");
    j.push_str(&jnum("elapsed_ms", o.waited_ms));
    j.push('}');
    j
}

// ---------------------------------------------------------------------------
// wg_fwd_probe — the same tunnel as a REAL bidirectional VPN data plane
// ---------------------------------------------------------------------------

/// OP_TUN_V6 (boringtun-0.7.1/src/ffi/mod.rs result_type): decapsulated IPv6
/// plaintext. Handled like OP_TUN_V4 (write back into the TUN fd).
const OP_TUN_V6: i32 = 5;
/// One poll slice of the forwarding loop; `wireguard_tick` runs once per slice
/// (recommended ~100 ms; 250 ms keeps keepalive/retransmit timers accurate
/// without spinning the loop).
const FWD_POLL_MS: i32 = 250;
/// Handshake wait bound: no host datagram that establishes a session within
/// 90 s -> verdict=timeout, reason=handshake-timeout.
const FWD_HS_WAIT_MS: u64 = 90_000;
/// Forwarding loop bound: 90 s or the event cap below, whichever comes first.
/// There is deliberately NO early exit on the first full round trip: after a
/// packet is written into the TUN the kernel needs time to answer (echo reply,
/// local delivery), so the loop must keep polling until a real bound is hit.
const FWD_LOOP_MS: u64 = 90_000;
/// Loop also ends after this many processed events (UDP datagrams + TUN reads
/// + sink datagrams) so a chatty kernel cannot extend the probe forever.
const FWD_EVENT_CAP: u32 = 200_000;
/// Max TUN frames drained per readable poll, so a kernel transmit burst cannot
/// starve the UDP half of the loop (each drain re-polls on the next iteration).
const FWD_TUN_DRAIN: u32 = 8;

#[derive(Default)]
struct FwdOutcome {
    dev_pub: String,
    host_pub: String,
    bind_rc: i32,
    bind_errno: i32,
    hs_time_s: i64,
    hs_ok: bool,
    endpoint: String,
    recv: u32,
    sent: u32,
    send_errno: i32,
    tun_rx: u32,
    tun_rx_skip: u32,
    tun_write: u32,
    tun_last_written: i64,
    tun_last_errno: i32,
    sink_bind_rc: i32,
    sink_bind_errno: i32,
    sink_recv: u32,
    verdict: String,
    reason: String,
    start_ms: u64,
    elapsed_ms: u64,
}

/// Send one ciphertext datagram to the recorded peer endpoint; counts a
/// success into `sent`, records errno on failure. Returns sendto's rc.
fn fwd_send(fd: i32, peer: &sys::sockaddr_in, out: &[u8], len: usize, o: &mut FwdOutcome) -> isize {
    let sn = unsafe {
        sys::sendto(
            fd,
            out.as_ptr() as *const core::ffi::c_void,
            len,
            0,
            peer,
            core::mem::size_of::<sys::sockaddr_in>() as u32,
        )
    };
    if sn >= 0 {
        o.sent += 1;
    } else {
        o.send_errno = sys::errno();
    }
    sn
}

/// Write decapsulated plaintext back into the REAL TUN fd; counts a successful
/// (n > 0) write into `tun_write`. Panic-free: caller guarantees
/// `len <= out.len()`.
fn fwd_tun_write(fd_dup: i32, len: usize, out: &[u8], o: &mut FwdOutcome) {
    let (wn, we) = sys::write_fd(fd_dup, &out[..len]);
    o.tun_last_written = wn as i64;
    o.tun_last_errno = we;
    if wn > 0 {
        o.tun_write += 1;
    }
    emit(&format!(
        "N1BDISC_WG_FWD_TUN_WRITE|len={}|written={}|errno={}",
        len, wn, we
    ));
}

/// Stage machine; every exit path leaves `verdict`/`reason` set and has already
/// emitted the markers for the stage it stopped in. `fd_dup` is the O_NONBLOCK
/// dup of the real TUN fd (d2 S5), safe to read inside the poll loop.
///
/// Socket stage shapes: `pre = None` opens + binds `0.0.0.0:47010` here
/// (wg_fwd_probe); `pre = Some((fd, bind_rc, bind_errno))` adopts a socket
/// already opened + bound by `wg_fwd_open` — that gap is where ArkTS calls
/// `VpnConnection.protect(fd)` so the WG outer datagrams bypass the VPN
/// (added 2026-09-12; the pre-opened socket is closed by the same exit paths
/// as before). Marker semantics are unchanged: exactly one
/// `N1BDISC_WG_FWD_BIND` per successful run, emitted by whichever stage bound.
fn run_fwd(fd_dup: i32, pre: Option<(i32, i32, i32)>) -> FwdOutcome {
    let mut o = FwdOutcome::default();
    o.start_ms = sys::mono_ms();
    let t0 = o.start_ms;

    // --- 1. UDP socket bound 0.0.0.0:47010 ----------------------------------
    let fd: i32 = match pre {
        Some((fd, bind_rc, bind_errno)) => {
            o.bind_rc = bind_rc;
            o.bind_errno = bind_errno;
            fd
        }
        None => {
            let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            if fd < 0 {
                o.bind_rc = -1;
                o.bind_errno = sys::errno();
                o.verdict = "socket_fail".into();
                o.reason = "socket".into();
                return o;
            }
            let bind_sa = sys::sockaddr_in::new([0, 0, 0, 0], NET_PORT);
            o.bind_rc = unsafe {
                sys::bind(fd, &bind_sa, core::mem::size_of::<sys::sockaddr_in>() as u32)
            };
            o.bind_errno = if o.bind_rc == -1 { sys::errno() } else { 0 };
            emit(&format!(
                "N1BDISC_WG_FWD_BIND|port={}|rc={}|errno={}",
                NET_PORT, o.bind_rc, o.bind_errno
            ));
            if o.bind_rc != 0 {
                unsafe { sys::close(fd) };
                o.verdict = "bind_fail".into();
                o.reason = "bind".into();
                return o;
            }
            fd
        }
    };

    // --- 2. fixed test secrets -> real public keys -> one tunnel -------------
    let dev_sk = ffi::x25519_key { key: NET_DEV_SECRET };
    let dev_pk = ffi::x25519_public_key(ffi::x25519_key { key: NET_DEV_SECRET });
    let host_pk = ffi::x25519_public_key(ffi::x25519_key { key: NET_HOST_SECRET });
    let (dev_sec_b64, dev_pub_b64, host_pub_b64) =
        match (key_to_b64(dev_sk), key_to_b64(dev_pk), key_to_b64(host_pk)) {
            (Some(s), Some(dp), Some(hp)) => (s, dp, hp),
            _ => {
                unsafe { sys::close(fd) };
                o.verdict = "key_b64_null".into();
                o.reason = "key-to-base64-null".into();
                return o;
            }
        };
    o.dev_pub = dev_pub_b64;
    o.host_pub = host_pub_b64.clone();
    emit(&format!(
        "N1BDISC_WG_FWD_KEYS|dev_pub={}|host_pub={}",
        o.dev_pub, host_pub_b64
    ));

    let tunnel = match Tunnel::new(&dev_sec_b64, &host_pub_b64, NET_INDEX, "FWD") {
        Some(t) => t,
        None => {
            unsafe { sys::close(fd) };
            o.verdict = "new_tunnel_null".into();
            o.reason = "new_tunnel-null".into();
            return o;
        }
    };
    emit(&format!(
        "N1BDISC_WG_FWD_TUNNEL|keep_alive={}|idx={}",
        KEEP_ALIVE, NET_INDEX
    ));

    // Peer endpoint, refreshed by every recvfrom; all sends go to the last
    // source observed (the host's WG port).
    let mut peer = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut have_peer = false;
    let mut rbuf = [0u8; BUF];
    let mut out = [0u8; BUF];

    // --- 3. bounded handshake wait: answer the host's initiation -------------
    // Responder role, same as wg_net_probe (the host initiates). A decapsulated
    // transport packet that slips in during this phase is still written to the
    // TUN — it is real data-plane traffic within an established session.
    let hs_deadline = t0 + FWD_HS_WAIT_MS;
    while !o.hs_ok && sys::mono_ms() < hs_deadline {
        let (ret, e, revents) = sys::poll1(fd, sys::POLLIN, FWD_POLL_MS);
        if ret == 0 {
            continue; // slice elapsed; re-check the deadline
        }
        if ret < 0 {
            if e == sys::EINTR {
                continue;
            }
            o.verdict = "poll_error".into();
            o.reason = format!("hs-poll-{}", e);
            break;
        }
        if (revents & sys::POLLIN) == 0 {
            continue;
        }
        let mut from = sys::sockaddr_in::new([0, 0, 0, 0], 0);
        let mut flen = core::mem::size_of::<sys::sockaddr_in>() as u32;
        let rn = unsafe {
            sys::recvfrom(
                fd,
                rbuf.as_mut_ptr() as *mut core::ffi::c_void,
                rbuf.len(),
                0,
                &mut from,
                &mut flen,
            )
        };
        if rn <= 0 {
            continue; // spurious wakeup or zero-length datagram
        }
        o.recv += 1;
        peer = from;
        have_peer = true;
        o.endpoint = sa_str(&from);
        emit(&format!(
            "N1BDISC_WG_FWD_RECV|phase=hs|len={}|src={}",
            rn, o.endpoint
        ));

        let n = rn as usize; // rn > 0 and <= rbuf.len() by recvfrom semantics
        let (op, len) = tunnel.read(&rbuf[..n], &mut out);
        match op {
            OP_NETWORK if len > 0 && len <= BUF => {
                let sn = fwd_send(fd, &peer, &out, len, &mut o);
                emit(&format!(
                    "N1BDISC_WG_FWD_OP|phase=hs|len={}|op={}|sent={}",
                    len, op, sn
                ));
            }
            OP_TUN_V4 | OP_TUN_V6 if len > 0 && len <= BUF => {
                fwd_tun_write(fd_dup, len, &out, &mut o);
            }
            OP_ERROR => emit(&format!("N1BDISC_WG_FWD_OP|phase=hs|len={}|op={}", len, op)),
            _ => {}
        }

        // Session is installed the moment the initiation validates
        // (boringtun handle_handshake_init -> set_current_session), so stats
        // flips to >= 0 right after the response is queued.
        let (hs_time, _tx, _rx) = tunnel.stats();
        o.hs_time_s = hs_time;
        o.hs_ok = hs_time >= 0;
    }
    emit(&format!(
        "N1BDISC_WG_FWD_HS|time={}|ok={}",
        o.hs_time_s, o.hs_ok
    ));
    if !o.hs_ok {
        if o.verdict.is_empty() {
            o.verdict = "timeout".into();
            o.reason = "handshake-timeout".into();
        }
        unsafe { sys::close(fd) };
        return o;
    }

    // --- 4. bidirectional forwarding loop ------------------------------------
    // Local sink socket 0.0.0.0:47003 (same port wg_net_probe used; that probe
    // is commented out of the .ets call chain, so nothing else owns the port).
    // Kernel-local delivery evidence: a host datagram decrypted into the TUN
    // with dst=10.99.0.1 is delivered by the kernel to this socket WITHOUT a
    // route lookup, so it proves the tunnel -> kernel-stack path even when the
    // main route table has no VPN route for the reply (per-UID routing keeps
    // kernel-originated packets out of the VPN network domain). A bind failure
    // is recorded truthfully and is NOT fatal: the ICMP half keeps running.
    let sink_fd: i32 = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
    if sink_fd < 0 {
        o.sink_bind_rc = -1;
        o.sink_bind_errno = sys::errno();
        emit(&format!(
            "N1BDISC_WG_FWD_SINK_BIND|port={}|rc=-1|errno={}",
            SINK_PORT, o.sink_bind_errno
        ));
    } else {
        let sink_sa = sys::sockaddr_in::new([0, 0, 0, 0], SINK_PORT);
        o.sink_bind_rc = unsafe {
            sys::bind(sink_fd, &sink_sa, core::mem::size_of::<sys::sockaddr_in>() as u32)
        };
        o.sink_bind_errno = if o.sink_bind_rc == -1 { sys::errno() } else { 0 };
        emit(&format!(
            "N1BDISC_WG_FWD_SINK_BIND|port={}|rc={}|errno={}",
            SINK_PORT, o.sink_bind_rc, o.sink_bind_errno
        ));
    }
    let loop_deadline = sys::mono_ms() + FWD_LOOP_MS;
    let mut events = 0u32;
    while o.verdict.is_empty() {
        if sys::mono_ms() >= loop_deadline {
            o.reason = "deadline".into();
            break;
        }
        if events >= FWD_EVENT_CAP {
            o.reason = "packet-cap".into();
            break;
        }

        // Keepalives / handshake retransmits / rekeys; send whatever is due.
        let (top, tlen) = tunnel.tick(&mut out);
        if top == OP_NETWORK && tlen > 0 && tlen <= BUF && have_peer {
            let sn = fwd_send(fd, &peer, &out, tlen, &mut o);
            emit(&format!(
                "N1BDISC_WG_FWD_TICK|op={}|len={}|sent={}",
                top, tlen, sn
            ));
        } else if top == OP_ERROR {
            emit(&format!("N1BDISC_WG_FWD_TICK_ERR|op={}|len={}", top, tlen));
        }

        // Poll ALL fds: [0] = WG UDP socket, [1] = real TUN fd, [2] = local
        // 47003 sink. (poll1 covers one fd; the closed-table `poll` primitive
        // takes the array. A failed sink socket keeps fd < 0 and is dropped
        // from the set via nfds so poll never visits it.)
        let mut fds = [
            sys::pollfd { fd, events: sys::POLLIN, revents: 0 },
            sys::pollfd { fd: fd_dup, events: sys::POLLIN, revents: 0 },
            sys::pollfd { fd: sink_fd, events: sys::POLLIN, revents: 0 },
        ];
        let nfds = if sink_fd >= 0 { 3 } else { 2 };
        let ret = unsafe { sys::poll(fds.as_mut_ptr(), nfds, FWD_POLL_MS) };
        if ret < 0 {
            let e = sys::errno();
            if e == sys::EINTR {
                continue;
            }
            o.verdict = "fail".into();
            o.reason = format!("poll-{}", e);
            break;
        }

        // --- TUN readable: device-originated plaintext -> encrypt -> host ----
        if (fds[1].revents & sys::POLLIN) != 0 {
            for _ in 0..FWD_TUN_DRAIN {
                // fd_dup is O_NONBLOCK: read returns immediately once drained.
                let (rn, re) = sys::read_fd(fd_dup, &mut rbuf);
                if rn <= 0 {
                    if re != sys::EAGAIN && re != 0 {
                        emit(&format!("N1BDISC_WG_FWD_TUN_ERR|rn={}|errno={}", rn, re));
                    }
                    break;
                }
                events += 1;
                let n = rn as usize; // <= rbuf.len() by read semantics
                // Only IPv4 with IHL==5 goes into the tunnel. The kernel also
                // emits its own control frames on this interface (observed:
                // 116-byte IPv6 MLDv2 reports, version nibble 6); forwarding
                // those would pollute the tunnel, so skip and count them.
                let (ver, ihl) = ((rbuf[0] >> 4), (rbuf[0] & 0x0f));
                if n < 20 || ver != 4 || ihl != 5 {
                    o.tun_rx_skip += 1;
                    emit(&format!(
                        "N1BDISC_WG_FWD_SKIP|len={}|ver={}|ihl={}",
                        n, ver, ihl
                    ));
                    continue;
                }
                o.tun_rx += 1;
                emit(&format!(
                    "N1BDISC_WG_FWD_TUN_RX|len={}|first8={}",
                    n,
                    hex_lower(&rbuf[..n.min(8)])
                ));
                let (wop, wlen) = tunnel.write(&rbuf[..n], &mut out);
                if wop == OP_NETWORK && wlen > 0 && wlen <= BUF {
                    if have_peer {
                        let sn = fwd_send(fd, &peer, &out, wlen, &mut o);
                        emit(&format!(
                            "N1BDISC_WG_FWD_SEND|len={}|ct_len={}|sent={}",
                            n, wlen, sn
                        ));
                    }
                } else {
                    // No session yet, or a transient encapsulate error: record
                    // and keep looping; this is not fatal to the probe.
                    emit(&format!(
                        "N1BDISC_WG_FWD_ENC|op={}|len={}|pt_len={}",
                        wop, wlen, n
                    ));
                }
            }
        }

        // --- UDP readable: host datagram -> decapsulate ----------------------
        if (fds[0].revents & sys::POLLIN) != 0 && o.verdict.is_empty() {
            let mut from = sys::sockaddr_in::new([0, 0, 0, 0], 0);
            let mut flen = core::mem::size_of::<sys::sockaddr_in>() as u32;
            let rn = unsafe {
                sys::recvfrom(
                    fd,
                    rbuf.as_mut_ptr() as *mut core::ffi::c_void,
                    rbuf.len(),
                    0,
                    &mut from,
                    &mut flen,
                )
            };
            if rn > 0 {
                events += 1;
                o.recv += 1;
                peer = from;
                have_peer = true;
                o.endpoint = sa_str(&from);
                emit(&format!(
                    "N1BDISC_WG_FWD_RECV|phase=fwd|len={}|src={}",
                    rn, o.endpoint
                ));
                let n = rn as usize;
                let (op, len) = tunnel.read(&rbuf[..n], &mut out);
                match op {
                    OP_TUN_V4 | OP_TUN_V6 if len > 0 && len <= BUF => {
                        fwd_tun_write(fd_dup, len, &out, &mut o);
                    }
                    OP_NETWORK if len > 0 && len <= BUF => {
                        let sn = fwd_send(fd, &peer, &out, len, &mut o);
                        emit(&format!("N1BDISC_WG_FWD_OP|len={}|op={}|sent={}", len, op, sn));
                    }
                    OP_ERROR => {
                        o.verdict = "fail".into();
                        o.reason = format!("wg-read-error-{}", len);
                    }
                    _ => emit(&format!("N1BDISC_WG_FWD_OP|len={}|op={}", len, op)),
                }
            }
        }

        // --- sink readable: kernel delivered a tunnel packet locally ---------
        if sink_fd >= 0 && (fds[2].revents & sys::POLLIN) != 0 {
            let mut from = sys::sockaddr_in::new([0, 0, 0, 0], 0);
            let mut flen = core::mem::size_of::<sys::sockaddr_in>() as u32;
            let rn = unsafe {
                sys::recvfrom(
                    sink_fd,
                    rbuf.as_mut_ptr() as *mut core::ffi::c_void,
                    rbuf.len(),
                    0,
                    &mut from,
                    &mut flen,
                )
            };
            if rn > 0 {
                // A processed datagram like the UDP/TUN events above, so the
                // event cap stays a bound on total work.
                events += 1;
                o.sink_recv += 1;
                emit(&format!(
                    "N1BDISC_WG_FWD_SINK|len={}|src={}",
                    rn,
                    sa_str(&from)
                ));
            } else if rn < 0 {
                emit(&format!("N1BDISC_WG_FWD_SINK_ERR|errno={}", sys::errno()));
            }
        }

        // No early exit on the first full round trip (removed 2026-09-12): the
        // former `tun_write > 0 && sent > 0 -> reason=roundtrip` stop fired
        // ~1 ms after the first TUN write, before the kernel could answer, so
        // echo replies were never read back. The loop now ends ONLY on the
        // deadline or the event cap (plus genuine poll/read failures).
    }

    if sink_fd >= 0 {
        unsafe { sys::close(sink_fd) };
    }
    unsafe { sys::close(fd) };
    if o.verdict.is_empty() {
        // pass = decrypted data made it back into the real TUN; partial =
        // something moved but never the TUN closure; timeout = nothing moved.
        if o.tun_write > 0 {
            o.verdict = "pass".into();
        } else if o.recv > 0 || o.tun_rx > 0 || o.sent > 0 {
            o.verdict = "partial".into();
        } else {
            o.verdict = "timeout".into();
        }
        if o.reason.is_empty() {
            o.reason = "loop-end".into();
        }
    }
    o
}

/// `wg_fwd_probe(fdDup: number, mb1: boolean) -> string` (JSON). Real
/// bidirectional data plane over one BoringTun tunnel: answers the host's
/// handshake, then forwards in BOTH directions between the real TUN fd, the WG
/// UDP socket and a local 47003 sink until the 90 s deadline, the 200_000-event
/// cap, or a fatal error (no early exit on the first round trip). Outside the
/// frozen P-chain: no ledger transition, and the
/// caller's D4/D5/D8/D-W sequence is untouched.
pub fn wg_fwd_probe(fd_dup: i32, mb1: bool) -> String {
    emit("N1BDISC_WG_FWD_ENTER|");
    let _ = mb1; // kept for signature parity with wg_net_probe; unused here
    let mut o = run_fwd(fd_dup, None);
    o.elapsed_ms = sys::mono_ms().saturating_sub(o.start_ms);
    emit(&format!(
        "N1BDISC_WG_FWD_END|verdict={}|tun_rx={}|sent={}|recv={}|tun_write={}|sink_recv={}|elapsed_ms={}|reason={}",
        o.verdict, o.tun_rx, o.sent, o.recv, o.tun_write, o.sink_recv, o.elapsed_ms, o.reason
    ));

    let mut j = String::from("{");
    j.push_str(&jstr("dev_pub", &o.dev_pub));
    j.push_str(",");
    j.push_str(&jstr("host_pub", &o.host_pub));
    j.push_str(",");
    j.push_str(&format!("\"bind_rc\":{}", o.bind_rc));
    j.push_str(",");
    j.push_str(&format!("\"bind_errno\":{}", o.bind_errno));
    j.push_str(",");
    j.push_str(&jinum("hs_time_s", o.hs_time_s));
    j.push_str(",");
    j.push_str(&jbool("hs_ok", o.hs_ok));
    j.push_str(",");
    j.push_str(&jstr("endpoint", &o.endpoint));
    j.push_str(",");
    j.push_str(&jnum("recv", o.recv as u64));
    j.push_str(",");
    j.push_str(&jnum("sent", o.sent as u64));
    j.push_str(",");
    j.push_str(&format!("\"send_errno\":{}", o.send_errno));
    j.push_str(",");
    j.push_str(&jnum("tun_rx", o.tun_rx as u64));
    j.push_str(",");
    j.push_str(&jnum("tun_rx_skip", o.tun_rx_skip as u64));
    j.push_str(",");
    j.push_str(&jnum("tun_write", o.tun_write as u64));
    j.push_str(",");
    j.push_str(&jinum("tun_last_written", o.tun_last_written));
    j.push_str(",");
    j.push_str(&format!("\"tun_last_errno\":{}", o.tun_last_errno));
    j.push_str(",");
    j.push_str(&format!("\"sink_bind_rc\":{}", o.sink_bind_rc));
    j.push_str(",");
    j.push_str(&format!("\"sink_bind_errno\":{}", o.sink_bind_errno));
    j.push_str(",");
    j.push_str(&jnum("sink_recv", o.sink_recv as u64));
    j.push_str(",");
    j.push_str(&jstr("verdict", &o.verdict));
    j.push_str(",");
    j.push_str(&jstr("reason", &o.reason));
    j.push_str(",");
    j.push_str(&jnum("elapsed_ms", o.elapsed_ms));
    j.push('}');
    j
}

/// `wg_fwd_open() -> string` (JSON `{fd, bind_rc, bind_errno}`). Split-variant
/// stage 1 (added 2026-09-12): create the WG UDP socket and bind
/// `0.0.0.0:47010` WITHOUT running anything, so ArkTS can call
/// `VpnConnection.protect(fd)` on it before a single datagram flows. Marker
/// semantics mirror run_fwd exactly — `N1BDISC_WG_FWD_BIND` with truthful
/// rc/errno whenever socket() succeeded, silence on socket() failure; no new
/// marker literal is introduced. The caller owns the fd: either hand it to
/// `wg_fwd_run` (which closes it on every exit path) or close it ArkTS-side
/// when skipping the run.
pub fn wg_fwd_open() -> String {
    let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
    if fd < 0 {
        let e = sys::errno();
        return format!("{{\"fd\":-1,\"bind_rc\":-1,\"bind_errno\":{}}}", e);
    }
    let bind_sa = sys::sockaddr_in::new([0, 0, 0, 0], NET_PORT);
    let bind_rc =
        unsafe { sys::bind(fd, &bind_sa, core::mem::size_of::<sys::sockaddr_in>() as u32) };
    let bind_errno = if bind_rc == -1 { sys::errno() } else { 0 };
    emit(&format!(
        "N1BDISC_WG_FWD_BIND|port={}|rc={}|errno={}",
        NET_PORT, bind_rc, bind_errno
    ));
    if bind_rc != 0 {
        unsafe { sys::close(fd) };
    }
    format!(
        "{{\"fd\":{},\"bind_rc\":{},\"bind_errno\":{}}}",
        fd, bind_rc, bind_errno
    )
}

/// `wg_fwd_run(fd: number, fdDup: number, mb1: boolean) -> string` (same JSON
/// shape as `wg_fwd_probe`). Split-variant stages 2-4 (added 2026-09-12):
/// handshake + bidirectional forward loop over a socket ALREADY opened and
/// bound by `wg_fwd_open` (and ideally protected via `VpnConnection.protect`).
/// `fd < 0` degrades to the monolithic shape (run_fwd opens + binds its own
/// socket), i.e. wg_fwd_probe behavior minus the protected gap. The adopted
/// socket is closed on every exit path, exactly as in the monolithic path.
/// Outside the frozen P-chain: no ledger transition, D4/D5/D8/D-W untouched.
pub fn wg_fwd_run(fd: i32, fd_dup: i32, mb1: bool) -> String {
    emit("N1BDISC_WG_FWD_ENTER|");
    let _ = mb1; // kept for signature parity with wg_fwd_probe; unused here
    let pre = if fd >= 0 { Some((fd, 0, 0)) } else { None };
    let mut o = run_fwd(fd_dup, pre);
    o.elapsed_ms = sys::mono_ms().saturating_sub(o.start_ms);
    emit(&format!(
        "N1BDISC_WG_FWD_END|verdict={}|tun_rx={}|sent={}|recv={}|tun_write={}|sink_recv={}|elapsed_ms={}|reason={}",
        o.verdict, o.tun_rx, o.sent, o.recv, o.tun_write, o.sink_recv, o.elapsed_ms, o.reason
    ));

    let mut j = String::from("{");
    j.push_str(&format!("\"bind_rc\":{}", o.bind_rc));
    j.push_str(",");
    j.push_str(&format!("\"bind_errno\":{}", o.bind_errno));
    j.push_str(",");
    j.push_str(&jbool("hs_ok", o.hs_ok));
    j.push_str(",");
    j.push_str(&jstr("endpoint", &o.endpoint));
    j.push_str(",");
    j.push_str(&jnum("recv", o.recv as u64));
    j.push_str(",");
    j.push_str(&jnum("sent", o.sent as u64));
    j.push_str(",");
    j.push_str(&jnum("tun_rx", o.tun_rx as u64));
    j.push_str(",");
    j.push_str(&jnum("tun_write", o.tun_write as u64));
    j.push_str(",");
    j.push_str(&format!("\"sink_bind_rc\":{}", o.sink_bind_rc));
    j.push_str(",");
    j.push_str(&format!("\"sink_bind_errno\":{}", o.sink_bind_errno));
    j.push_str(",");
    j.push_str(&jnum("sink_recv", o.sink_recv as u64));
    j.push_str(",");
    j.push_str(&jstr("verdict", &o.verdict));
    j.push_str(",");
    j.push_str(&jstr("reason", &o.reason));
    j.push_str(",");
    j.push_str(&jnum("elapsed_ms", o.elapsed_ms));
    j.push('}');
    j
}
