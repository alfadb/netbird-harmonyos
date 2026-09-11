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

use std::ffi::CString;

use boringtun::ffi;

use crate::hilog::emit;
use crate::net::{self, d5_packet};
use crate::sys;
use crate::util::{jbool, jnum, jstr};

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
