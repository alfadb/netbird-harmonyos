//! D5 tun write -> sink delivery (U2) (gate-plan :573-586).
//!
//! Frozen 44-byte legal packet (IPv4 checksum recomputed over the header, UDP
//! checksum 0) written to fd_dup; sink socket binds 0.0.0.0:47002 and each
//! round does poll(POLLIN, 500ms) + recvfrom with byte-exact identity check
//! (source == frozen src:47001 AND the 16-byte payload equals THIS round's
//! identity). <=5 rounds, total window 10 s.

use crate::hilog::emit;
use crate::ledger::{self, Role};
use crate::net::{self, d5_packet};
use crate::sys;
use crate::util::{jbool, jnum, jstr};

pub fn d5_probe(fd_dup: i32) -> String {
    // MB1 retention flag travels in probe state (set by d4_probe, which the
    // frozen order P3->P4 always executes before D5 on every live path).
    let mb1 = crate::state::with_state(|s| s.mb1_retained);
    emit("N1BDISC_D5_BEGIN");
    let t0 = sys::mono_ms();
    let deadline = t0 + 10_000;

    // sink socket: AF_INET/SOCK_DGRAM bind 0.0.0.0:47002
    let sink = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
    if sink < 0 {
        let e = sys::errno();
        ledger::emit_not_created(Role::D5SinkSocket, &format!("socket-failed-errno-{}", e));
        let u2 = "unobservable(cause=write-failed)";
        emit(&format!("N1BDISC_D5_END|u2={}", u2));
        return format!(
            "{{{},{}}}",
            jstr("u2", u2),
            jstr("error", &format!("sink socket errno {}", e))
        );
    }
    ledger::emit_create(Role::D5SinkSocket, sink);
    let bind_sa = sys::sockaddr_in::new([0, 0, 0, 0], 47002);
    let br = unsafe { sys::bind(sink, &bind_sa, core::mem::size_of::<sys::sockaddr_in>() as u32) };
    let bind_errno = if br == -1 { sys::errno() } else { 0 };

    let frozen_src = net::dst_peer(mb1);
    let mut write_rets: Vec<(i64, i32)> = Vec::new();
    let mut partial_write_count: u64 = 0;
    let mut matched_round: Option<u32> = None;
    let mut recv_src_logged: Option<String> = None;
    let mut recv_count: u64 = 0;

    let mut round: u32 = 0;
    while round < 5 && sys::mono_ms() < deadline {
        round += 1;
        // poll(fd_dup, POLLOUT, 500ms) then write
        let (_pret, _pe, _rev) = sys::poll1(fd_dup, sys::POLLOUT, 500);
        let pkt = d5_packet(round as u16, mb1);
        let (n, e) = sys::write_fd(fd_dup, &pkt);
        emit(&format!(
            "N1BDISC_D5_WRITE|round={}|ret={}|errno={}",
            round, n, e
        ));
        write_rets.push((n as i64, e));
        if n == 0 || (n > 0 && n < 44) {
            partial_write_count += 1;
        }

        // wait on sink
        let (pret, _pe, revents) = sys::poll1(sink, sys::POLLIN, 500);
        let _ = pret;
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
                recv_count += 1;
                let src_str = format!(
                    "{}.{}.{}.{}:{}",
                    from.sin_addr[0],
                    from.sin_addr[1],
                    from.sin_addr[2],
                    from.sin_addr[3],
                    u16::from_be(from.sin_port)
                );
                emit(&format!("N1BDISC_D5_RECV|round={}|src={}", round, src_str));
                recv_src_logged = Some(src_str.clone());
                // identity check (:582): source == frozen src:47001 AND payload
                // == THIS round's 16-byte identity, byte-exact
                let identity_ok = from.sin_addr == frozen_src
                    && u16::from_be(from.sin_port) == 47001
                    && rn >= 16
                    && {
                        let mut expect = [0u8; 16];
                        expect[..8].copy_from_slice(b"N1DISCD5");
                        expect[8] = 0x01; // round byte
                        expect[9] = (round >> 8) as u8; // seq BE16 = this round
                        expect[10] = (round & 0xff) as u8;
                        expect[11..16].fill(0x5a);
                        rbuf[..16] == expect
                    };
                if identity_ok && matched_round.is_none() {
                    matched_round = Some(round);
                }
            }
        }
        if matched_round.is_some() {
            break;
        }
    }

    // U2 three-state judgment (:581-584)
    let any_full = write_rets.iter().any(|(n, _)| *n == 44);
    let all_err = write_rets.iter().all(|(n, _)| *n == -1);
    let u2 = if !any_full {
        if all_err {
            "unobservable(cause=write-failed)"
        } else {
            "unobservable(cause=short-or-zero-io)"
        }
    } else if matched_round.is_some() {
        "observed-true"
    } else {
        "observed-false"
    };

    emit(&format!("N1BDISC_D5_END|u2={}", u2));

    let mut wr = String::new();
    for (i, (n, e)) in write_rets.iter().enumerate() {
        if i > 0 {
            wr.push(';');
        }
        wr.push_str(&format!("{}:{}", n, e));
    }

    let mut j = String::from("{");
    j.push_str(&jstr("u2", u2));
    j.push_str(",");
    j.push_str(&jstr(
        "matched_round",
        &matched_round.map(|r| r.to_string()).unwrap_or_else(|| "none".into()),
    ));
    j.push_str(",");
    j.push_str(&jstr("write_rets", &wr));
    j.push_str(",");
    j.push_str(&jnum("partial_write_count", partial_write_count));
    j.push_str(",");
    j.push_str(&jnum("recv_count", recv_count));
    j.push_str(",");
    match &recv_src_logged {
        Some(s) => j.push_str(&jstr("last_recv_src", s)),
        None => j.push_str("\"last_recv_src\":null"),
    }
    j.push_str(",");
    j.push_str(&jstr("sink_bind_errno", &bind_errno.to_string()));
    j.push_str(",");
    j.push_str(&jbool("mb1", mb1));
    j.push_str(",");
    j.push_str(&jnum("rounds", round as u64));
    j.push_str(",");
    j.push_str(&jnum("elapsed_ms", sys::mono_ms().saturating_sub(t0)));
    j.push('}');
    j
}
