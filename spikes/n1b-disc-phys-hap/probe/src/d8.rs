//! D8a MTU write-return ladder + D8b backpressure/partial-write storm
//! (gate-plan :588-616).
//!
//! D8a: L in {128,512,1024,1200,1280,1352,1400,1401,1480,1500}; per level one
//! poll(POLLOUT, 500ms) + write(fd_dup, pkt, L) with L == total_length; the
//! IPv4 checksum is recomputed per level (total_length changes). Ret class per
//! level: n==L / 0<n<L / -1 EAGAIN / -1 EMSGSIZE / -1 other(errno).
//!
//! D8b storm: frozen 1024 B packet with id FIXED at 11 (checksum computed once
//! and reused — the Identification field sits inside the checksummed header);
//! three fuses: 10 s wall clock (monotonic), 4 MiB cumulative, 50 000 write
//! calls. Fuses are fuses — hitting one is a registered stop, never a fail,
//! and never induced. Markers: `N1BDISC_D8_MTU|len=|ret=|errno=` per level,
//! `N1BDISC_D8_STORM_BEGIN|ws=<n>` / `N1BDISC_D8_STORM_END|eagain=|partial=|
//! bytes=|calls=|caps_hit=|we=<n>`.

use crate::hilog::emit;
use crate::net::d8_packet;
use crate::sys;
use crate::util::{jbool, jnum, jstr};

pub const LADDER: [usize; 10] = [128, 512, 1024, 1200, 1280, 1352, 1400, 1401, 1480, 1500];

pub fn d8a_probe(fd_dup: i32, mb1: bool) -> String {
    crate::state::with_state(|s| {
        s.mb1_retained = mb1 || s.mb1_retained;
    });

    let mut last_success: Option<usize> = None;
    let mut ladder_out: Vec<String> = Vec::new();
    let mut level_ok = [false; 10];
    let t0 = sys::mono_ms();

    for (idx, &l) in LADDER.iter().enumerate() {
        let id = (idx + 1) as u16; // id = level index 1..10, network order in builder
        let pkt = d8_packet(l, id, mb1);
        let (_pret, _pe, _rev) = sys::poll1(fd_dup, sys::POLLOUT, 500);
        let (n, e) = sys::write_fd(fd_dup, &pkt);
        emit(&format!("N1BDISC_D8_MTU|len={}|ret={}|errno={}", l, n, e));

        let class = if n == l as isize {
            "n==L"
        } else if n > 0 {
            "0<n<L"
        } else if n == -1 && e == sys::EAGAIN {
            "-1/EAGAIN"
        } else if n == -1 && e == sys::EMSGSIZE {
            "-1/EMSGSIZE"
        } else if n == -1 {
            "-1/other"
        } else {
            "zero"
        };
        let ok = n == l as isize;
        level_ok[idx] = ok;
        if ok {
            last_success = Some(l);
        }
        ladder_out.push(format!("{}:{}:{}:{}", l, n, e, class));
    }

    let lsl = match last_success {
        Some(l) => l.to_string(),
        None => "none".to_string(),
    };

    // write_return_boundary_consistent_with_1400 (:595): last_success == 1400
    // AND the 1401 level's ret is in a failure class (anything but n==L).
    // Literal-consistency registration only — no MTU-oracle claim (:596-597).
    let boundary_consistent = if mb1 {
        "unobservable(cause=mb1-no-declared-mtu)".to_string()
    } else {
        let idx1401 = LADDER.iter().position(|l| *l == 1401).unwrap_or(9);
        match last_success {
            Some(1400) if !level_ok[idx1401] => "observed-true".to_string(),
            _ => "observed-false".to_string(),
        }
    };

    format!(
        "{{{},{},{},{},{},{}}}",
        jstr("d8_write_boundary_last_success_len", &lsl),
        jstr("write_return_boundary_consistent_with_1400", &boundary_consistent),
        jstr("ladder", &ladder_out.join(";")),
        jnum("levels", LADDER.len() as u64),
        jbool("mb1", mb1),
        jnum("elapsed_ms", sys::mono_ms().saturating_sub(t0))
    )
}

pub fn d8b_probe(fd_dup: i32, mb1: bool) -> String {
    crate::state::with_state(|s| {
        s.mb1_retained = mb1 || s.mb1_retained;
    });

    const CAP_TIME_MS: u64 = 10_000;
    const CAP_BYTES: u64 = 4 * 1024 * 1024;
    const CAP_CALLS: u64 = 50_000;
    const STORM_LEN: usize = 1024;

    let ws = sys::mono_ms();
    emit(&format!("N1BDISC_D8_STORM_BEGIN|ws={}", ws));
    let deadline = ws + CAP_TIME_MS;

    // frozen 1024 B packet, id fixed 11, checksum computed once and reused
    let pkt = d8_packet(STORM_LEN, 11, mb1);

    let mut bytes_total: u64 = 0;
    let mut calls: u64 = 0;
    let mut eagain = false;
    let mut partial = false;
    let mut zero_writes: u64 = 0;
    let mut stop_errno: i32 = 0;
    let mut caps: Vec<&str> = Vec::new();

    loop {
        let (n, e) = sys::write_fd(fd_dup, &pkt);
        calls += 1;
        if n > 0 {
            bytes_total += n as u64;
            if (n as usize) < STORM_LEN {
                partial = true;
            }
        } else if n == 0 {
            zero_writes += 1;
        } else {
            // -1: first EAGAIN stops the storm; any other error stops too
            if e == sys::EAGAIN {
                eagain = true;
            }
            stop_errno = e;
            break;
        }
        // fuse checks (never induced); a partial/zero write is an observation,
        // the storm continues until EAGAIN or a fuse
        if bytes_total >= CAP_BYTES {
            caps.push("bytes");
        }
        if calls >= CAP_CALLS {
            caps.push("calls");
        }
        if sys::mono_ms() >= deadline {
            caps.push("time");
        }
        if !caps.is_empty() {
            break;
        }
    }

    let we = sys::mono_ms();
    let caps_hit = if caps.is_empty() {
        "none".to_string()
    } else {
        caps.join(",")
    };
    let eagain_s = if eagain { "observed-true" } else { "observed-false" };
    let partial_s = if partial { "observed-true" } else { "observed-false" };
    let not_triggered_stmt = if !eagain {
        "attempted, not induced on this fd"
    } else {
        "none"
    };
    emit(&format!(
        "N1BDISC_D8_STORM_END|eagain={}|partial={}|bytes={}|calls={}|caps_hit={}|we={}",
        eagain_s, partial_s, bytes_total, calls, caps_hit, we
    ));

    format!(
        "{{{},{},{},{},{},{},{},{},{},{},{},{}}}",
        jstr("eagain_observed", eagain_s),
        jstr("partial_write_observed", partial_s),
        jnum("bytes_written_total", bytes_total),
        jnum("write_calls", calls),
        jnum("window_start_monotonic", ws),
        jnum("window_end_monotonic", we),
        jstr("caps_hit", &caps_hit),
        jstr("stop_errno", &stop_errno.to_string()),
        jnum("zero_writes", zero_writes),
        jstr("not_triggered_statement", not_triggered_stmt),
        jbool("mb1", mb1),
        jnum("elapsed_ms", we.saturating_sub(ws))
    )
}
