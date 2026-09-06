//! Cross-call probe state. All NAPI entry points run on the ArkTS JS thread;
//! the D-W worker communicates through the seq_cst atomics below (r18/r20/r21
//! snapshot mechanism), never through this mutex.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU64, Ordering};
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// D-W worker shared atomics (all seq_cst per r22 memory-order spec)
// ---------------------------------------------------------------------------

/// Barrier flag: set by the worker immediately before its poll() call
/// ("紧贴 poll 调用之前"), right after emitting DW_BARRIER.
pub static DW_BARRIER: AtomicBool = AtomicBool::new(false);
/// Snapshot write-completion signal `dw_snapshot_written` (r21): set after the
/// six snapshot fields are written, BEFORE DW_RETURN is emitted.
pub static DW_SNAPSHOT_WRITTEN: AtomicBool = AtomicBool::new(false);
/// Worker terminal flag `dw_worker_terminal` (r18): set after DW_EXIT emission;
/// semantics = "all worker output complete". Publish boundary for the snapshot.
pub static DW_TERMINAL: AtomicBool = AtomicBool::new(false);

pub static DW_TID: AtomicI32 = AtomicI32::new(0);
pub static DW_FD: AtomicI32 = AtomicI32::new(-1);
pub static DW_HANDLE: AtomicU64 = AtomicU64::new(0);

// snapshot domain (five raw + dw_drain_end single column, r18/r20)
pub static SNAP_RET: AtomicI64 = AtomicI64::new(0);
pub static SNAP_ERRNO: AtomicI32 = AtomicI32::new(0);
pub static SNAP_REVENTS: AtomicI64 = AtomicI64::new(0);
pub static SNAP_AT_MONO_MS: AtomicI64 = AtomicI64::new(0);
pub static SNAP_ELAPSED_MS: AtomicI64 = AtomicI64::new(0);
/// drain end code: 0=eagain 1=zero-read 2=errno-<n> 3=box-expiry (see DRAIN_END_*)
pub static SNAP_DRAIN_END: AtomicI64 = AtomicI64::new(-1);
/// errno for drain_end code 2 (0 otherwise)
pub static SNAP_DRAIN_ERRNO: AtomicI32 = AtomicI32::new(0);
/// EINTR retry count during drain (C6: registered separately, not a DRAIN
/// marker field — exposed via post_emit JSON)
pub static SNAP_DRAIN_EINTR: AtomicI64 = AtomicI64::new(0);

pub const DRAIN_END_EAGAIN: i64 = 0;
pub const DRAIN_END_ZERO_READ: i64 = 1;
pub const DRAIN_END_ERRNO: i64 = 2;
pub const DRAIN_END_BOX_EXPIRY: i64 = 3;

pub fn drain_end_literal(code: i64, errno: i32) -> String {
    match code {
        DRAIN_END_EAGAIN => "eagain".to_string(),
        DRAIN_END_ZERO_READ => "zero-read".to_string(),
        DRAIN_END_ERRNO => format!("errno-{}", errno),
        DRAIN_END_BOX_EXPIRY => "box-expiry".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Reset D-W statics before a fresh dw_start (single campaign: mostly a
/// defensive reset; the process is expected to run the sequence once).
pub fn dw_reset() {
    DW_BARRIER.store(false, Ordering::SeqCst);
    DW_SNAPSHOT_WRITTEN.store(false, Ordering::SeqCst);
    DW_TERMINAL.store(false, Ordering::SeqCst);
    DW_TID.store(0, Ordering::SeqCst);
    DW_FD.store(-1, Ordering::SeqCst);
    DW_HANDLE.store(0, Ordering::SeqCst);
    SNAP_RET.store(0, Ordering::SeqCst);
    SNAP_ERRNO.store(0, Ordering::SeqCst);
    SNAP_REVENTS.store(0, Ordering::SeqCst);
    SNAP_AT_MONO_MS.store(0, Ordering::SeqCst);
    SNAP_ELAPSED_MS.store(0, Ordering::SeqCst);
    SNAP_DRAIN_END.store(-1, Ordering::SeqCst);
    SNAP_DRAIN_ERRNO.store(0, Ordering::SeqCst);
    SNAP_DRAIN_EINTR.store(0, Ordering::SeqCst);
}

// ---------------------------------------------------------------------------
// probe state (JS thread only)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct D6StepResult {
    pub ret: i64,
    pub errno: i32,
    pub fd: Option<i32>,
    pub reuse: Option<bool>,
}

#[derive(Clone, Debug, Default)]
pub struct D6Results {
    pub d6a_ran: bool,
    pub d6b_ran: bool,
    pub steps: Vec<D6StepResult>, // index 0..=6 for S1..S7
}

#[derive(Clone, Debug)]
pub struct InwaitResult {
    pub src: String,
    pub conf: String,
    pub samples: u64,
    pub errno: i32,
}

#[derive(Clone, Debug, Default)]
pub struct ProbeState {
    pub fd_orig: Option<i32>,
    pub fd_dup: Option<i32>,
    /// MB1 retention (E3-form, no routes): set by the first probe call that
    /// receives the mb1 flag (d4_probe per frozen order); read by d5/d8a/d8b.
    pub mb1_retained: bool,
    pub d6: D6Results,
    // destroy sub-protocol tracking
    pub destroy_t_mono: Option<u64>,
    pub destroy_c_mono: Option<u64>,
    /// cause of an emitted `N1BDISC_SKIP|item=destroy` (class-0 evidence)
    pub destroy_skip_cause: Option<String>,
    /// cause of an emitted `N1BDISC_SKIP|item=D-W` (no-live-fd / dup-failed)
    pub dw_skip_cause: Option<String>,
    pub dw_spawned: bool,
    pub jt_registered: bool, // P10 terminal-poll box expired (join-timeout)
    pub join_result: Option<String>,
    pub barrier_confirmed: Option<bool>,
    pub inwait: Option<InwaitResult>,
    pub racewin_emitted: bool,
    pub sig_zero_after_box: bool,
}

pub static STATE: Mutex<ProbeState> = Mutex::new(ProbeState::EMPTY);

impl ProbeState {
    pub const EMPTY: ProbeState = ProbeState {
        fd_orig: None,
        fd_dup: None,
        mb1_retained: false,
        d6: D6Results {
            d6a_ran: false,
            d6b_ran: false,
            steps: Vec::new(),
        },
        destroy_t_mono: None,
        destroy_c_mono: None,
        destroy_skip_cause: None,
        dw_skip_cause: None,
        dw_spawned: false,
        jt_registered: false,
        join_result: None,
        barrier_confirmed: None,
        inwait: None,
        racewin_emitted: false,
        sig_zero_after_box: false,
    };
}

pub fn with_state<R>(f: impl FnOnce(&mut ProbeState) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut g)
}
