//! D4 socket->tun delivery (U1) + first matching-frame dump (U3)
//! (gate-plan :534-572).
//!
//! Own AF_INET/SOCK_DGRAM socket (`d4_send_socket`) sends the frozen 16-byte
//! control payload to 10.99.0.2:47001 (MR* retained) / 192.0.2.2:47001 (MB1);
//! <=20 sends, each followed by poll(fd_dup, POLLIN, 500ms) + non-blocking
//! read; every read is parsed at BOTH offset-0 and offset-4 (version=4, IHL=5)
//! and matched against proto=17 + dst + dport=47001 + the 16-byte identity of
//! one of the sent packets. Total window 10 s.

use crate::chunk;
use crate::hilog::emit;
use crate::ledger::{self, Role};
use crate::net::{self, d4_payload, offsets_parsable, parse_ipv4_at, readlen_vs_total, u3_prefix_class};
use crate::sys;
use crate::util::{hex_lower, jbool, jnum, jstr};

const READ_BUF: usize = 65536;

pub fn d4_probe(fd_dup: i32, mb1: bool) -> String {
    crate::state::with_state(|s| s.mb1_retained = mb1 || s.mb1_retained);
    emit("N1BDISC_D4_BEGIN");
    let t0 = sys::mono_ms();
    let deadline = t0 + 10_000;

    // send socket
    let sock = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
    if sock < 0 {
        let e = sys::errno();
        // socket creation failed: no fd role instance; register not-created with errno cause
        ledger::emit_not_created(Role::D4SendSocket, &format!("socket-failed-errno-{}", e));
        let u1 = "unobservable(cause=send-failed)";
        emit(&format!("N1BDISC_D4_END|u1={}", u1));
        return format!(
            "{{{},{},{}}}",
            jstr("u1", u1),
            jstr("u1_no_route_control", "unobservable(cause=send-failed)"),
            jstr("error", &format!("socket errno {}", e))
        );
    }
    ledger::emit_create(Role::D4SendSocket, sock);

    let dest = net::dst_peer(mb1);
    let dest_sa = sys::sockaddr_in::new(dest, 47001);

    let mut sent_payloads: Vec<[u8; 16]> = Vec::new();
    let mut send_rets: Vec<(i64, i32)> = Vec::new(); // (ret, errno)
    let mut matched = false;
    let mut match_offset: String = "none".to_string();
    let mut first_frame: Option<Vec<u8>> = None;
    let mut foreign_count: u64 = 0;
    let mut first_foreign_hex: Option<String> = None;
    let mut reads_logged: u64 = 0;

    let mut seq: u16 = 1;
    let mut sends: u32 = 0;
    let mut buf = vec![0u8; READ_BUF];

    while sys::mono_ms() < deadline {
        // send while budget remains
        if sends < 20 {
            let payload = d4_payload(seq);
            let n = unsafe {
                sys::sendto(
                    sock,
                    payload.as_ptr() as *const core::ffi::c_void,
                    16,
                    0,
                    &dest_sa,
                    core::mem::size_of::<sys::sockaddr_in>() as u32,
                )
            };
            let e = if n == -1 { sys::errno() } else { 0 };
            emit(&format!(
                "N1BDISC_D4_SENT|n={}|ret={}|errno={}",
                sends + 1,
                n,
                e
            ));
            send_rets.push((n as i64, e));
            sent_payloads.push(payload);
            sends += 1;
            seq = seq.wrapping_add(1);
        }

        // poll + non-blocking read after each send (and until window closes)
        let (pret, _pe, revents) = sys::poll1(fd_dup, sys::POLLIN, 500);
        let _ = pret;
        if (revents & sys::POLLIN) != 0 {
            let (n, _re) = sys::read_fd(fd_dup, &mut buf);
            if n > 0 {
                let frame = &buf[..n as usize];
                reads_logged += 1;
                let (o0, o4, offs) = offsets_parsable(frame);
                emit(&format!("N1BDISC_D4_READ|len={}|off={}", n, offs));

                // double-offset match against the frozen identity
                let mut this_match: Option<&'static str> = None;
                for (off, ok) in [(0usize, o0), (4usize, o4)] {
                    if !ok {
                        continue;
                    }
                    if let Some(h) = parse_ipv4_at(frame, off) {
                        if h.proto == 17
                            && h.dst == dest
                            && h.udp_dport == 47001
                            && sent_payloads.iter().any(|p| p.as_slice() == h.udp_payload)
                        {
                            this_match = match this_match {
                                None => Some(if off == 0 { "0" } else { "4" }),
                                Some("0") if off == 4 => Some("both"),
                                Some("4") if off == 0 => Some("both"),
                                other => other,
                            };
                        }
                    }
                }

                if this_match.is_some() {
                    if !matched {
                        matched = true;
                        match_offset = this_match.unwrap().to_string();
                        first_frame = Some(frame.to_vec());
                    }
                } else {
                    foreign_count += 1;
                    if first_foreign_hex.is_none() {
                        let hex = hex_lower(&frame[..core::cmp::min(64, frame.len())]);
                        first_foreign_hex = Some(hex.clone());
                        chunk::emit_chunk(chunk::STREAM_FOREIGN, 0, &hex);
                    }
                }
            }
        }
        // U1/U3 decisive data acquired on the first match — stop early (the
        // <=20 sends / 10 s window are upper bounds, not quotas)
        if matched {
            break;
        }
    }

    // U1 three-state judgment (:540-547)
    let any_full = send_rets.iter().any(|(n, _)| *n == 16);
    let all_err = send_rets.iter().all(|(n, _)| *n == -1);
    let (u1_main, u1_control) = if mb1 {
        // MB1 retained: main field is protocol-fixed; control field carries the observation
        let ctrl = if !any_full {
            if all_err {
                "unobservable(cause=send-failed)"
            } else {
                "unobservable(cause=short-or-zero-io)"
            }
        } else if matched {
            "observed-true"
        } else {
            "observed-false"
        };
        ("unobservable(cause=mb1-no-route)", ctrl)
    } else if !any_full {
        if all_err {
            ("unobservable(cause=send-failed)", "unobservable(cause=send-failed)")
        } else {
            ("unobservable(cause=short-or-zero-io)", "unobservable(cause=short-or-zero-io)")
        }
    } else if matched {
        ("observed-true", "observed-true")
    } else {
        ("observed-false", "observed-false")
    };

    emit(&format!("N1BDISC_D4_END|u1={}", u1_main));

    // U3 evaluation on the FIRST matching frame (decisive data source)
    let mut u3 = String::from("\"u3\":{");
    match &first_frame {
        None => {
            u3.push_str(&jstr("u3_state", "unobservable(cause=no-controlled-read)"));
            u3.push_str(",");
            u3.push_str(&jstr("u3_pi_header_present", "unobservable(cause=no-controlled-read)"));
            u3.push_str(",");
            u3.push_str(&jstr("u3_prefix_format", "unobservable(cause=no-controlled-read)"));
        }
        Some(frame) => {
            let (o0, o4, _) = offsets_parsable(frame);
            let (class, prefix4) = u3_prefix_class(frame, o0, o4);
            let first64 = hex_lower(&frame[..core::cmp::min(64, frame.len())]);
            chunk::emit_chunk(chunk::STREAM_U3HEX, 0, &first64);
            // decisive offset for readlen-vs-total: offset-4 when both
            let decisive = if o4 { 4usize } else if o0 { 0usize } else { 4usize };
            let rel = readlen_vs_total(frame, decisive);
            let rel_off0 = readlen_vs_total(frame, 0);
            u3.push_str(&jnum("u3_first_read_len", frame.len() as u64));
            u3.push_str(",");
            u3.push_str(&jstr("u3_first64_hex", &first64));
            u3.push_str(",");
            u3.push_str(&jstr("u3_pi_header_present", class));
            u3.push_str(",");
            u3.push_str(&jstr(
                "u3_prefix_format",
                match class {
                    "tun_pi-like" => "observed-true",
                    "no-prefix" | "other-prefix" => "observed-false",
                    "ambiguous" => "unobservable(cause=prefix-ambiguous)",
                    _ => "unobservable(cause=frame-unparsable)",
                },
            ));
            u3.push_str(",");
            u3.push_str(&jstr("u3_prefix4_hex", &hex_lower(&prefix4)));
            u3.push_str(",");
            u3.push_str(&jstr("u3_readlen_vs_total_length", rel));
            u3.push_str(",");
            u3.push_str(&jstr("u3_readlen_vs_total_length_off0", rel_off0));
        }
    }
    u3.push('}');

    let mut send_rets_str = String::new();
    for (i, (n, e)) in send_rets.iter().enumerate() {
        if i > 0 {
            send_rets_str.push(';');
        }
        send_rets_str.push_str(&format!("{}:{}", n, e));
    }

    let mut j = String::from("{");
    j.push_str(&jstr("u1", u1_main));
    j.push_str(",");
    j.push_str(&jstr("u1_no_route_control", u1_control));
    j.push_str(",");
    j.push_str(&jstr("u1_match_offset", &match_offset));
    j.push_str(",");
    j.push_str(&u3);
    j.push_str(",");
    j.push_str(&jnum("foreign_packets_observed", foreign_count));
    j.push_str(",");
    match &first_foreign_hex {
        Some(h) => j.push_str(&jstr("foreign_first64_hex", h)),
        None => j.push_str("\"foreign_first64_hex\":null"),
    }
    j.push_str(",");
    j.push_str(&jnum("sends", sends as u64));
    j.push_str(",");
    j.push_str(&jnum("reads", reads_logged));
    j.push_str(",");
    j.push_str(&jstr("send_rets", &send_rets_str));
    j.push_str(",");
    j.push_str(&jbool("mb1", mb1));
    j.push_str(",");
    j.push_str(&jnum("elapsed_ms", sys::mono_ms().saturating_sub(t0)));
    j.push('}');
    j
}
