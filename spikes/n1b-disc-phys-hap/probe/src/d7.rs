//! D7 live watchdog probe (U7) — frozen 20 s synchronous load, pseudocode
//! verbatim (gate-plan :623-640, MJ-12):
//!
//! ```text
//! buf: [u8; 4096], initially all 0x00; x: u64 = 0; j: u64 = 0
//! sink: volatile write (prevents dead-code elimination)
//! start_ms = clock_gettime(CLOCK_MONOTONIC) (ms domain)
//! deadline_ms = start_ms + 20000
//! loop {                                  // outer block: exactly one clock read each
//!   for k in 0..4096 {                    // inner: fixed 4096 iterations, zero syscalls
//!     off = (j * 8) mod 4096
//!     v   = u64 little-endian load buf[off..off+8]
//!     x   = x.wrapping_mul(2654435761).wrapping_add(v)   // Knuth multiplier
//!     u64 little-endian store buf[off..off+8] = x
//!     j += 1
//!   }
//!   volatile_sink_write(x)
//!   clock_nanosleep(CLOCK_MONOTONIC, 50ms)
//!   now_ms = clock_gettime(CLOCK_MONOTONIC)
//!   if now_ms >= deadline_ms { break }
//! }
//! ```
//!
//! Clock-read upper bound 20 Hz is structural: the ONLY clock read sits at the
//! outer-block tail, after a 50 ms sleep. No fd/lock/allocation inside the task.
//! Markers: `N1BDISC_D7_BEGIN|dur_ms=20000|load=frozen-int-mix-4k|start_mono_ms=<n>`
//! / `N1BDISC_D7_END|iters=<n>|elapsed_ms=<n>` (elapsed = last read − start).

use crate::hilog::emit;
use crate::sys::{mono_ms, sleep_ms};
use crate::util::{jnum, jstr};

// volatile sink (anti-optimization target)
static mut D7_SINK: u64 = 0;

pub fn d7_probe() -> String {
    let start_ms = mono_ms();
    emit(&format!(
        "N1BDISC_D7_BEGIN|dur_ms=20000|load=frozen-int-mix-4k|start_mono_ms={}",
        start_ms
    ));
    let deadline_ms: u64 = start_ms + 20_000;

    let mut buf = [0u8; 4096];
    let mut x: u64 = 0;
    let mut j: u64 = 0;
    let mut iters: u64 = 0;

    let elapsed_ms = loop {
        // inner block: fixed 4096 iterations, zero syscalls
        for _k in 0..4096usize {
            let off = ((j * 8) % 4096) as usize;
            let v = u64::from_le_bytes([
                buf[off],
                buf[off + 1],
                buf[off + 2],
                buf[off + 3],
                buf[off + 4],
                buf[off + 5],
                buf[off + 6],
                buf[off + 7],
            ]);
            x = x.wrapping_mul(2654435761).wrapping_add(v);
            buf[off..off + 8].copy_from_slice(&x.to_le_bytes());
            j += 1;
        }
        // volatile sink write
        unsafe {
            core::ptr::write_volatile(core::ptr::addr_of_mut!(D7_SINK), x);
        }
        // 50 ms clock gating (the only sleep length used here per A4)
        sleep_ms(50);
        // the task's ONLY clock-read point: exactly once per outer block
        let now_ms = mono_ms();
        iters += 1;
        if now_ms >= deadline_ms {
            break now_ms.saturating_sub(start_ms);
        }
    };
    emit(&format!(
        "N1BDISC_D7_END|iters={}|elapsed_ms={}",
        iters, elapsed_ms
    ));

    let u7 = if elapsed_ms < 20_000 {
        // contradiction with the pseudocode exit condition — probe-side raw
        // registration; F8 verdict handling is runner-side (:647)
        "unobservable(cause=d7-early-exit-anomaly)"
    } else if elapsed_ms <= 25_000 {
        "observed-true"
    } else {
        "unobservable(cause=d7-elapsed-overshoot-beyond-grace)"
    };

    format!(
        "{{{},{},{},{},{},{},{}}}",
        jstr("u7_long_task_watchdog_behavior", u7),
        jnum("iters", iters),
        jnum("elapsed_ms", elapsed_ms),
        jnum("start_mono_ms", start_ms),
        jstr("d7_anomaly", if elapsed_ms < 20_000 { "early-exit-with-end-marker" } else if elapsed_ms > 25_000 { "elapsed-overshoot-observed" } else { "none" }),
        jnum("j_final", j),
        jnum("x_final", x)
    )
}
