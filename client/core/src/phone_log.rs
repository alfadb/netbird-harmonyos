//! Phone-side persistent diagnostic log — a file tee of the SAME
//! [`crate::hilog::emit`] funnel, independent of the armed-only
//! [`crate::n2_ring`] evidence ring.
//!
//! ## Why it exists
//!
//! `n2_ring` is an EVIDENCE instrument: it is default-off, armed per campaign,
//! ACK-gated and never overwrites a segment. The phone needs something else
//! for ordinary support diagnostics: a small, always-available local copy of
//! what the core already emits on HiLog, so a VPN problem on a device without
//! a live host channel can be read back later. This module is that copy.
//! It never touches `n2_ring` (no call, no shared state, no behavior change):
//! `hilog::emit` calls both tees independently.
//!
//! ## Hot path (emit / network / ArkTS caller)
//!
//! `append` does NO disk I/O and never blocks on the writer: it first passes
//! the fixed per-second admission budget (one short lock, a few arithmetic
//! ops), then prepares the record text (redaction + cap) and hands it to a
//! **bounded** `sync_channel` with `try_send`. A line the budget refuses and a
//! line whose queue is full are both DROPS counted in `dropped` (the budget
//! part is named separately in `rate_suppressed`); back-pressure is never
//! applied to the caller. When the log is not configured the whole tee is one
//! relaxed atomic load. The module never calls `hilog::emit` itself (the
//! writer only does file I/O), so the funnel cannot recurse.
//!
//! ## Admission budget (bounded write rate, not just bounded size)
//!
//! Rotation bounds how much history is KEPT; it does not bound how fast new
//! records reach the flash. A producer stuck in a loop (an ICE log storm while
//! the device is already at `SWAP_FULL`) could therefore churn disk forever
//! inside a 96 MiB file set. So every line is admitted through a fixed
//! ONE-SECOND budget before it is redacted or queued:
//!
//! * [`DEFAULT_RATE_BYTES_PER_SEC`] = 8192 charged bytes per second;
//! * [`DEFAULT_RATE_RECORDS_PER_SEC`] = 64 records per second.
//!
//! Both are per-[`Options`] (`0` = default; a very large value is effectively
//! unlimited). A line is charged the length of its RAW ingress text (clamped
//! to the whole byte budget, so one oversized line consumes the window instead
//! of being refused for ever) — the charge is therefore an ingress-side
//! conservative measure and no buffer is allocated for a line that will be
//! refused. The default budget carries one default-cap (1024-char, ≤ 4 KiB
//! UTF-8) record whole, and the rare memory/lifecycle record is far below it,
//! but it does NOT carry a real STARTUP burst: on the 0.0.26 device runs the
//! first seconds of a connect refused 5.6–16 % of the lines (103 refusals
//! inside the first 6 s, one peer's `N5_ICE|selected-pair` among them) because
//! the bounded lifecycle/selection evidence markers were competing with the
//! ICE/route storm for the same budget.
//!
//! ## Priority admission (evidence markers survive an exhausted budget)
//!
//! The lifecycle/selection EVIDENCE markers named by [`PRIORITY_MARKERS`] are
//! exactly the lines 0.0.26 lost to that burst, and the ones whose absence
//! cannot be reconstructed afterwards. They enter through [`append_evidence`],
//! which offers a SEPARATE, small per-second window
//! ([`DEFAULT_PRIORITY_RATE_BYTES_PER_SEC`] /
//! [`DEFAULT_PRIORITY_RATE_RECORDS_PER_SEC`]) FIRST and falls back to the
//! ordinary one, so evidence is admitted whenever EITHER budget has room.
//!
//! Two entrances, deliberately: the core's own emitters know their markers and
//! call [`append_evidence`] directly (`hilog::emit_evidence`) — the storm lines
//! on the same hot path ([`append`]) pay NOTHING for this, no scan at all. The
//! shell bridge ([`append_shell`]) receives opaque record TEXT built by the
//! ArkTS diagnostics writer, so that one low-rate entry (shell milestones are
//! per-event, never per-packet) classifies the same closed marker set.
//!
//! Nothing else changes: the ordinary budget is NOT widened (a producer storm
//! is capped exactly as before) and the priority allowance is itself a FIXED
//! budget (a marker storm is capped too). Per window the tee therefore admits
//! at most `ordinary + priority` charged bytes / records — the overall bound is
//! the sum of two constants, never unbounded and never carried over. A line is
//! charged to at most one window, both attempts share one timestamp, and the
//! two windows turn over lazily and independently, exactly like the single
//! window below.
//!
//! The window is reset LAZILY by the first append of the next second: there is
//! no timer, no background thread and nothing runs while the tee is idle. An
//! unused second is NOT carried over, so a burst is bounded by about one
//! window's budget (up to two when a burst straddles a window boundary) and the
//! gate can never stay closed: writing resumes with the first append of the
//! next second. There is no backlog and no retry — a line refused now is gone
//! (its drop is counted), the next line is admitted.
//!
//! A refused line is never queued, never written and never blocks the
//! producer: it is counted in `dropped` (every ingress refusal) and named
//! separately in `rate_suppressed` ([`stats`]/[`status_json`]); queue-full
//! drops are the other part (`dropped - rate_suppressed`). No line is ever
//! dropped silently. The persisted `dropped=` field carries the cumulative
//! refusal count, so pressure is visible in the records themselves.
//!
//! ## Durability (honest limits)
//!
//! One record = one `write(2)` on the current segment; there is **no fsync and
//! no ACK**. [`flush`] means "every record accepted before the call was handed
//! to the OS" (the `write` syscall returned) — it is NOT power-loss
//! durability. A process `SIGKILL` can lose whatever is still in the queue
//! (never handed to the writer) and, on a crash with an unflushed page cache,
//! the file tail; a torn trailing record is possible after a write error and
//! readers must tolerate it.
//!
//! ## Files, rotation, sessions
//!
//! The current segment is `<path>`; rotated segments are `<path>.1` …
//! `<path>.(segments-1)` (defaults: 24 MiB each, 4 files ⇒ ≈ 96 MiB total,
//! the VPN share of the phone's 100 MiB running-log budget, `segments` is
//! bounded to 2..=8). Rotation is plain and **without ACK
//! semantics**: when a record would exceed the current segment cap the oldest
//! segment is deleted and the rest are renamed. Total disk use is therefore
//! bounded by `segments * max_bytes` plus at most one record — and the rate at
//! which that disk is written is bounded separately by the admission budget
//! above. `configure` opens the current segment in APPEND mode and never
//! truncates it, so records of previous sessions are retained (up to the
//! rotation budget) and a new `sess=` token distinguishes them.
//!
//! Every persisted line is one self-describing record:
//!
//! ```text
//! PL|sess=<token>|seq=<n>|wall_ms=<n>|mono_ms=<n>|pid=<n>|dropped=<n>|line=<redacted text>
//! ```
//!
//! `line=` is the LAST field and holds the text verbatim (it may contain `|`).
//! `dropped=` is the cumulative refusal count at write time (admission budget
//! + full queue), so pressure is visible in the records themselves (and via
//! [`stats`] / [`status_json`], where the budget part is `rate_suppressed`).
//!
//! ## Redaction at write ingress (best-effort, NOT a guarantee)
//!
//! Before a line enters the queue it is flattened to one line (CR/LF/TAB →
//! space, other control bytes dropped), length-capped and passed through a
//! bounded pattern filter that replaces with `[redacted]` / `[redacted-jwt]`:
//!
//! * values of credential-LABELLED fields (`setup_key`, `setup-key`,
//!   `auth_token`, `access_token`, `refresh_token`, `jwt`, `password`,
//!   `passwd`, `secret`, `client_secret`, `private_key`, `preshared_key`,
//!   `psk`, `api_key`, …) in `key=value`, `key: value` and `"key":"value"`
//!   forms — unless the value is a bare boolean/number/`none`/`null`;
//! * a bare `token=`/`token:` label only when the value is credential-SHAPED
//!   (opaque charset + a digit or separator), so `token_present=false` and
//!   prose values like `reason token: TokenExpired` survive;
//! * JWT-shaped `eyJ….….…` triplets;
//! * `Bearer <token>`;
//! * URL userinfo (`scheme://user:pass@host` ⇒ `scheme://[redacted]@host`).
//!
//! This is a filter over KNOWN forms, not a proof that no secret can ever
//! reach the file: unlabelled secrets, unknown label spellings, secrets split
//! across the hard CHARACTER bound on the raw text ([`MAX_RAW_CHARS`] = 16 384
//! characters, i.e. up to 64 KiB of UTF-8 — a byte bound it is not) and
//! credentials inside arbitrary structures are NOT covered. Endpoint/routing
//! data (hosts, IP:port, relay URLs, `token_present=` flags, `password=none`)
//! is deliberately KEPT — the log is for diagnosis, not for deleting the
//! useful half. This module never reads or dumps configuration or credential
//! files; it only filters text that other code already chose to emit on the
//! funnel.
//!
//! ## Disk errors
//!
//! A failed write/rotate counts the record in `failed`, records an
//! errno-style `last_error` (never a path or content) and retries the next
//! record; after [`MAX_CONSECUTIVE_ERRORS`] consecutive failures the writer
//! stops, marks itself not running and lets the funnel continue untouched.
//! Nothing here ever emits on HiLog, panics or blocks the network path.
//!
//! ## Ownership
//!
//! Exactly one writer thread exists per successful `configure`. A re-configure
//! installs the new logger first, then asks the OLD writer to drain, close and
//! exit within a bounded wait ([`DEFAULT_WAIT`]); the old thread is joined only
//! after its completion ack (so the join itself cannot wait on disk I/O). If a
//! wedged filesystem makes that bounded wait expire, the old thread is
//! detached (it holds one fd) rather than blocking the caller forever —
//! `configure`/`shutdown` never block longer than the timeout they are given.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Default cap of the CURRENT segment (24 MiB × 4 files ⇒ ≈ 96 MiB total).
pub const DEFAULT_MAX_BYTES: u64 = 24 * 1024 * 1024;
/// Default number of kept files (`<path>`, `<path>.1`, …, `<path>.3`).
pub const DEFAULT_SEGMENTS: u32 = 4;
/// Default bounded queue capacity (records, not bytes).
pub const DEFAULT_QUEUE: usize = 256;
/// Default per-record payload cap (characters, after redaction). This is a
/// CHARACTER count, not a byte count: one capped record can hold up to 4 UTF-8
/// bytes per character (≈ 4 KiB of text at the default) plus framing, so the
/// queue's memory footprint is up to ~4× the record count times this number,
/// not `queue * max_line_chars` bytes.
pub const DEFAULT_MAX_LINE_CHARS: usize = 1024;
/// Length of one admission window (milliseconds). One fixed second; the window
/// is reset lazily by the next append, there is no timer.
pub const RATE_WINDOW_MS: u64 = 1000;
/// Default admission budget in charged bytes per window (see module docs).
/// Wide enough to carry one default-cap record (≤ 4 KiB of UTF-8 text plus
/// framing) and ordinary memory/lifecycle records; a storm is capped here.
pub const DEFAULT_RATE_BYTES_PER_SEC: u64 = 8192;
/// Default admission budget in records per window. Bounds the writer's syscall
/// rate for a flood of tiny records while still carrying an ordinary
/// startup/lifecycle burst (tens of lines inside one second) whole.
pub const DEFAULT_RATE_RECORDS_PER_SEC: u64 = 64;
/// Priority admission budget in charged bytes per window: the separate, small
/// allowance that carries the [`PRIORITY_MARKERS`] evidence lines when the
/// ordinary budget is exhausted (half the ordinary byte budget — ~25–40
/// marker lines, still a FIXED bound, so a marker storm cannot exceed it).
pub const DEFAULT_PRIORITY_RATE_BYTES_PER_SEC: u64 = 4096;
/// Priority admission budget in records per window (half the ordinary record
/// budget). The observed startup marker burst is ~10 lines inside one second.
pub const DEFAULT_PRIORITY_RATE_RECORDS_PER_SEC: u64 = 32;
/// Smallest accepted segment cap (tests use tiny caps to force rotation).
pub const MIN_MAX_BYTES: u64 = 1024;
/// Largest accepted segment cap.
pub const MAX_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// Bounds on `segments` (2 files is the minimum meaningful rotation).
pub const MAX_SEGMENTS: u32 = 8;
/// Largest accepted queue capacity.
pub const MAX_QUEUE: usize = 65536;
/// Largest accepted per-record payload cap.
pub const MAX_LINE_CHARS_LIMIT: usize = 8192;
/// Hard bound on the raw text taken from one funnel line BEFORE redaction, so
/// a pathological line cannot allocate an unbounded buffer. It is a CHARACTER
/// count: the raw text can be up to 4 bytes per character (64 KiB of UTF-8).
const MAX_RAW_CHARS: usize = 16 * 1024;
/// Consecutive write failures after which the writer stops retrying.
pub const MAX_CONSECUTIVE_ERRORS: u32 = 5;
/// Default bounded wait for [`flush`]/[`shutdown`]/re-configure.
pub const DEFAULT_WAIT: Duration = Duration::from_secs(2);
/// Absolute ceiling applied to any caller-supplied wait.
const MAX_WAIT: Duration = Duration::from_secs(30);
/// The persistent record prefix.
pub const RECORD_PREFIX: &str = "PL|";

const REDACTED: &str = "[redacted]";
const REDACTED_JWT: &str = "[redacted-jwt]";
const TRUNC_MARK: &str = "…[truncated]";

/// Strong credential labels: the value is redacted by LABEL alone (any
/// non-trivial, non-placeholder value).
const STRONG_LABELS: &[&[u8]] = &[
    b"setup_key",
    b"setup-key",
    b"setupkey",
    b"auth_token",
    b"auth-token",
    b"authtoken",
    b"access_token",
    b"refresh_token",
    b"id_token",
    b"session_token",
    b"jwt",
    b"jwt_token",
    b"bearer_token",
    b"password",
    b"passwd",
    b"passphrase",
    b"secret",
    b"client_secret",
    b"private_key",
    b"preshared_key",
    b"psk",
    b"api_key",
    b"apikey",
    b"enrollment_key",
    b"registration_key",
];

/// Weak labels: redacted only when the value is credential-SHAPED (keeps
/// `token_present=false` and prose like `token: TokenExpired` readable).
const WEAK_LABELS: &[&[u8]] = &[b"token"];

// ---------------------------------------------------------------------------
// configuration / stats surface
// ---------------------------------------------------------------------------

/// Tunables for one [`configure_with`]. `Default` is the production shape
/// (24 MiB × 4 files, 256-record queue, 1024-char records, 8192 B/s + 64
/// records/s admission budget, generated session).
#[derive(Clone, Debug)]
pub struct Options {
    /// Cap of the CURRENT segment in bytes (`0` = [`DEFAULT_MAX_BYTES`]).
    pub max_bytes: u64,
    /// Kept files, oldest first (`0` = [`DEFAULT_SEGMENTS`]).
    pub segments: u32,
    /// Bounded queue capacity in records (`0` = [`DEFAULT_QUEUE`]).
    pub queue: usize,
    /// Per-record payload cap in characters (`0` = [`DEFAULT_MAX_LINE_CHARS`]).
    pub max_line_chars: usize,
    /// Admissions per one-second window in charged bytes (`0` =
    /// [`DEFAULT_RATE_BYTES_PER_SEC`], see module docs). Every value is
    /// meaningful (a very large one is effectively unlimited), so there is no
    /// invalid budget to reject.
    pub rate_bytes_per_sec: u64,
    /// Admissions per one-second window in records (`0` =
    /// [`DEFAULT_RATE_RECORDS_PER_SEC`]).
    pub rate_records_per_sec: u64,
    /// Explicit `sess=` token; `None` generates one per configure.
    pub session: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            max_bytes: DEFAULT_MAX_BYTES,
            segments: DEFAULT_SEGMENTS,
            queue: DEFAULT_QUEUE,
            max_line_chars: DEFAULT_MAX_LINE_CHARS,
            rate_bytes_per_sec: DEFAULT_RATE_BYTES_PER_SEC,
            rate_records_per_sec: DEFAULT_RATE_RECORDS_PER_SEC,
            session: None,
        }
    }
}

/// Point-in-time view for `stats`/`status_json` (later NAPI wiring).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// The tee is routing new lines into the queue.
    pub enabled: bool,
    /// The writer thread is alive and accepting.
    pub running: bool,
    /// `sess=` token of the installed logger (empty when never configured).
    pub session: String,
    /// Current segment path (empty when never configured).
    pub path: String,
    /// Records reserved at ingress (admitted + refused).
    pub offered: u64,
    /// Records actually queued (`offered - dropped`).
    pub accepted: u64,
    /// Records refused at ingress: the admission budget plus a full queue.
    pub dropped: u64,
    /// Subset of `dropped` refused by the per-second admission budget; the rest
    /// (`dropped - rate_suppressed`) is queue pressure.
    pub rate_suppressed: u64,
    /// Records handed to the OS (`write(2)` returned OK).
    pub written: u64,
    /// Accepted records NOT persisted (write errors + shutdown races).
    pub failed: u64,
    /// Accepted but not yet written (`accepted - written - failed`).
    pub pending: u64,
    /// Completed segment rotations.
    pub rotations: u64,
    /// Total bytes handed to the OS.
    pub bytes_written: u64,
    /// Bytes currently in the current segment (writer's view).
    pub current_bytes: u64,
    /// Current segment cap.
    pub max_bytes: u64,
    /// Kept files.
    pub segments: u32,
    /// Queue capacity in records.
    pub queue_capacity: usize,
    /// Configured admission budget in charged bytes per window.
    pub rate_bytes_per_sec: u64,
    /// Configured admission budget in records per window.
    pub rate_records_per_sec: u64,
    /// Last I/O error as an errno-style code (empty = none).
    pub last_error: String,
}

// ---------------------------------------------------------------------------
// shared state
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Counters {
    offered: AtomicU64,
    dropped: AtomicU64,
    rate_suppressed: AtomicU64,
    written: AtomicU64,
    failed: AtomicU64,
    rotations: AtomicU64,
    bytes_written: AtomicU64,
    current_bytes: AtomicU64,
    running: AtomicBool,
    last_error: Mutex<String>,
}

/// One fixed one-second admission window. PURE policy: the caller passes the
/// monotone millisecond clock, so the tests drive exact boundaries with no
/// sleep, and the installed logger holds it behind one short mutex.
#[derive(Clone, Copy, Debug, Default)]
struct RateWindow {
    /// Monotone ms at which the current window opened.
    open_ms: u64,
    /// Charged bytes admitted in the current window.
    bytes: u64,
    /// Records admitted in the current window.
    records: u64,
}

impl RateWindow {
    /// Whether a raw `line_len`-byte line may be admitted at `now_ms`. A
    /// refusal leaves the window untouched, so one over-budget line can never
    /// consume the budget of the lines behind it.
    fn admit(&mut self, now_ms: u64, line_len: usize, limit: RateLimit) -> bool {
        if now_ms.saturating_sub(self.open_ms) >= RATE_WINDOW_MS {
            // Lazily open the next second: no timer exists, and an idle logger
            // carries nothing over.
            self.open_ms = now_ms;
            self.bytes = 0;
            self.records = 0;
        }
        // Charged at most the whole window, so an oversized line consumes one
        // window instead of being refused for ever.
        let charge = (line_len as u64).min(limit.bytes);
        if self.records >= limit.records || charge > limit.bytes.saturating_sub(self.bytes) {
            return false;
        }
        self.bytes = self.bytes.saturating_add(charge);
        self.records = self.records.saturating_add(1);
        true
    }
}

/// Resolved admission budget (see [`Options`]). The same shape serves the
/// ordinary window ([`Options::rate_bytes_per_sec`] /
/// [`Options::rate_records_per_sec`]) and the fixed priority window
/// ([`DEFAULT_PRIORITY_RATE_BYTES_PER_SEC`] /
/// [`DEFAULT_PRIORITY_RATE_RECORDS_PER_SEC`]).
#[derive(Clone, Copy, Debug, Default)]
struct RateLimit {
    bytes: u64,
    records: u64,
}

/// The closed set of LIFECYCLE/SELECTION EVIDENCE markers that are offered the
/// separate, small priority window first (see the module docs "Priority
/// admission"). One entry = one substring of the line's text. Each name marks a
/// line the 0.0.26 device runs proved is emitted once per event (never per
/// packet), is the ONLY evidence of that event, and cannot be reconstructed
/// from anything else once it is lost:
///
/// * `VPN_` — the shell lifecycle markers (`VPN_CREATE_BEGIN/RESOLVED/REJECTED`,
///   `VPN_CONNECTION_CREATED`, `VPN_PROTECT_BEGIN/RESOLVED/FAIL_CLOSED`,
///   `VPN_ICE_MUX_ARMED/FAIL`, `VPN_DESTROY_…`), which ride the persisted
///   record as `detail=VPN_…`;
/// * `N5_ICE|selected-pair` — the per-peer ICE pair nomination (the `N5c_ICE` /
///   `N5d_ICE` check/gather lines are storms and deliberately stay ordinary);
/// * `N6_WG_DEVICE|session-established` / `session-expired` — the WG session
///   lifecycle (one line per session);
/// * the four `N13_WG` carrier transitions (direct⇄carrier, attach/detach) —
///   `N13_WG_FEED|…` and the bounded `N13_WG|carrier-reject` lines are NOT
///   transitions and stay ordinary;
/// * `connector: state ->` and `connector: login` — the management state
///   machine and every login outcome (attempt/ok/needs-login/fatal/exhausted).
///
/// Deliberately absent: every high-rate producer (`N5c_ICE|gather-stun`,
/// `N5d_ICE|check-rx`, `N11_ICE|failed`, relay-lane retries, `N5e_ICE|mux`),
/// which must keep competing for the ordinary budget only.
pub const PRIORITY_MARKERS: &[&str] = &[
    "VPN_",
    "N5_ICE|selected-pair",
    "N6_WG_DEVICE|session-established",
    "N6_WG_DEVICE|session-expired",
    "N13_WG|carrier-attach",
    "N13_WG|carrier-detach",
    "N13_WG|carrier->direct",
    "N13_WG|direct->carrier",
    "connector: state ->",
    "connector: login",
];

/// Whether `line` carries one of the [`PRIORITY_MARKERS`]. PURE (no clock, no
/// shared state) and pinned by unit tests. It scans the text, so the CORE never
/// calls it on its hot path: the core's emitters use [`append_evidence`]
/// directly and the shell bridge ([`append_shell`]) is the only caller.
pub fn is_evidence(line: &str) -> bool {
    PRIORITY_MARKERS.iter().any(|marker| line.contains(marker))
}

struct Logger {
    tx: SyncSender<Msg>,
    counters: Arc<Counters>,
    /// Admission gates, checked BEFORE any allocation and before the queue.
    /// `rate` is the ordinary window every line walks first; `priority_rate` is
    /// the small window [`append_evidence`] tries first (see the module docs).
    rate: Mutex<RateWindow>,
    priority_rate: Mutex<RateWindow>,
    limits: RateLimit,
    priority_limits: RateLimit,
    path: String,
    session: String,
    cfg: Options,
    handle: Mutex<Option<JoinHandle<()>>>,
}

enum Msg {
    Line(String),
    Shutdown(mpsc::Sender<()>),
}

/// Fast path: false until [`configure`] succeeded, so the `hilog` tee is one
/// atomic load in the common (unconfigured) case.
static ENABLED: AtomicBool = AtomicBool::new(false);
static LOGGER: OnceLock<Mutex<Option<Arc<Logger>>>> = OnceLock::new();
static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);
static START: OnceLock<Instant> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Arc<Logger>>> {
    LOGGER.get_or_init(|| Mutex::new(None))
}

fn lock_slot() -> std::sync::MutexGuard<'static, Option<Arc<Logger>>> {
    slot().lock().unwrap_or_else(PoisonError::into_inner)
}

fn current() -> Option<Arc<Logger>> {
    lock_slot().clone()
}

/// Whether the tee is routing new lines (`configure`d and not `shutdown`).
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

// ---------------------------------------------------------------------------
// public API
// ---------------------------------------------------------------------------

/// Configure with the production defaults (see [`Options`]).
pub fn configure(path: &str) -> Result<(), &'static str> {
    configure_with(path, &Options::default())
}

/// Install (or replace) the phone log on `path`.
///
/// Validation only for the file: an ABSENT file is normal (it is created), an
/// EXISTING one is opened in append mode and never truncated, so previous
/// sessions are retained. On success any previously installed logger is asked
/// to drain and exit within [`DEFAULT_WAIT`]; on error the previous logger (if
/// any) keeps running.
pub fn configure_with(path: &str, opts: &Options) -> Result<(), &'static str> {
    if !valid_path(path) {
        return Err("invalid-path");
    }
    let max_bytes = if opts.max_bytes == 0 { DEFAULT_MAX_BYTES } else { opts.max_bytes };
    if !(MIN_MAX_BYTES..=MAX_MAX_BYTES).contains(&max_bytes) {
        return Err("invalid-max-bytes");
    }
    let segments = if opts.segments == 0 { DEFAULT_SEGMENTS } else { opts.segments };
    if !(2..=MAX_SEGMENTS).contains(&segments) {
        return Err("invalid-segments");
    }
    let queue = if opts.queue == 0 { DEFAULT_QUEUE } else { opts.queue };
    if !(1..=MAX_QUEUE).contains(&queue) {
        return Err("invalid-queue");
    }
    let max_line_chars =
        if opts.max_line_chars == 0 { DEFAULT_MAX_LINE_CHARS } else { opts.max_line_chars };
    if !(128..=MAX_LINE_CHARS_LIMIT).contains(&max_line_chars) {
        return Err("invalid-max-line");
    }
    // Any budget is meaningful (a very large one is effectively unlimited), so
    // these are only defaulted, never rejected.
    let rate_bytes_per_sec = if opts.rate_bytes_per_sec == 0 {
        DEFAULT_RATE_BYTES_PER_SEC
    } else {
        opts.rate_bytes_per_sec
    };
    let rate_records_per_sec = if opts.rate_records_per_sec == 0 {
        DEFAULT_RATE_RECORDS_PER_SEC
    } else {
        opts.rate_records_per_sec
    };
    let cfg = Options {
        max_bytes,
        segments,
        queue,
        max_line_chars,
        rate_bytes_per_sec,
        rate_records_per_sec,
        session: opts.session.clone(),
    };
    let session = sanitize_session(opts.session.as_deref());

    // Open + size the current segment BEFORE dropping the old logger, so a
    // path that cannot be opened leaves the previous logger running.
    let counters = Arc::new(Counters::default());
    let state = WriterState::new(
        PathBuf::from(path),
        &cfg,
        session.clone(),
        Arc::clone(&counters),
    )?;
    let (tx, rx) = mpsc::sync_channel::<Msg>(queue);
    counters.running.store(true, Ordering::Release);
    let handle = match thread::Builder::new()
        .name("phone-log-writer".to_string())
        .spawn(move || writer_thread(state, rx))
    {
        Ok(handle) => handle,
        Err(_) => {
            // Nothing was installed and `ENABLED` was not touched, so a
            // previously running logger keeps routing.
            counters.running.store(false, Ordering::Release);
            return Err("spawn-failed");
        }
    };
    let logger = Arc::new(Logger {
        tx,
        counters,
        rate: Mutex::new(RateWindow::default()),
        priority_rate: Mutex::new(RateWindow::default()),
        limits: RateLimit { bytes: rate_bytes_per_sec, records: rate_records_per_sec },
        priority_limits: RateLimit {
            bytes: DEFAULT_PRIORITY_RATE_BYTES_PER_SEC,
            records: DEFAULT_PRIORITY_RATE_RECORDS_PER_SEC,
        },
        path: path.to_string(),
        session,
        cfg,
        handle: Mutex::new(Some(handle)),
    });

    let old = lock_slot().replace(Arc::clone(&logger));
    ENABLED.store(true, Ordering::Release);
    if let Some(old) = old {
        stop_writer(&old, DEFAULT_WAIT);
    }
    Ok(())
}

/// Tee one ORDINARY funnel line into the phone log (non-blocking; callable from
/// any thread, including the network/ArkTS caller). This is the hot path of the
/// ICE/route storm and does exactly one admission check — the ordinary
/// per-second budget — with no text scan.
///
/// Disk I/O never happens here. An admitted line is redacted + capped at ingress
/// and handed to the bounded queue with `try_send`; a line the budget refuses
/// (or that meets a full queue) is dropped and counted in `stats().dropped`
/// (the budget part also in `rate_suppressed`). The caller is never blocked.
/// Evidence lines go through [`append_evidence`] instead.
pub fn append(line: &str) {
    if line.is_empty() || !ENABLED.load(Ordering::Acquire) {
        return;
    }
    if let Some(logger) = current() {
        logger.offer(line);
    }
}

/// Tee one EVIDENCE line (see [`PRIORITY_MARKERS`]) into the phone log: the
/// small priority per-second window is tried FIRST and the ordinary budget is
/// its fallback, so a lifecycle/selection marker survives a burst that spends
/// the ordinary budget (the 0.0.26 startup gap).
///
/// Same contract as [`append`] otherwise: no disk I/O, no allocation for a
/// refused line, non-blocking, drops counted the same way. Callers that KNOW
/// they are emitting evidence (the core's marker emitters, via
/// `hilog::emit_evidence`) use this; text that merely MIGHT be evidence goes
/// through [`append_shell`]. Nothing is scanned here.
pub fn append_evidence(line: &str) {
    if line.is_empty() || !ENABLED.load(Ordering::Acquire) {
        return;
    }
    if let Some(logger) = current() {
        logger.offer_evidence(line);
    }
}

/// Tee one SHELL diagnostics record — the ArkTS `phone_log_append` bridge
/// (`NetBirdConnector.phoneLogAppend` → `napi::phone_log_append_export`).
///
/// The shell builds its records itself and hands over opaque TEXT, so this entry
/// classifies it against [`PRIORITY_MARKERS`] and routes it to
/// [`append_evidence`] or [`append`]. Only this entry scans: shell records are
/// per-EVENT milestones (a handful per second at most), while the per-packet
/// ICE/route storm reaches the tee through the core's own [`append`] — which
/// stays scan-free by design.
pub fn append_shell(line: &str) {
    if is_evidence(line) {
        append_evidence(line);
    } else {
        append(line);
    }
}

/// Wait (bounded by `timeout`) until every record accepted before the call was
/// handed to the OS. Returns `false` on timeout or when the writer stopped
/// with records still unwritten. NOT an fsync: see the module docs.
pub fn flush(timeout: Duration) -> bool {
    let Some(logger) = current() else { return true };
    let deadline = deadline_after(timeout);
    loop {
        let counters = &logger.counters;
        let target = counters
            .offered
            .load(Ordering::Acquire)
            .saturating_sub(counters.dropped.load(Ordering::Acquire));
        let written = counters.written.load(Ordering::Acquire);
        if written >= target {
            return true;
        }
        if !counters.running.load(Ordering::Acquire) {
            return false;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(1));
    }
}

/// Stop the writer: drains the queue, closes the file and waits (bounded by
/// `timeout`) for the writer's completion ack. Returns `true` when the writer
/// is confirmed stopped. After this call [`append`] is a no-op until the next
/// `configure`; retained [`stats`] still describe the stopped session.
pub fn shutdown(timeout: Duration) -> bool {
    ENABLED.store(false, Ordering::Release);
    match current() {
        Some(logger) => stop_writer(&logger, timeout),
        None => true,
    }
}

/// Snapshot for status reporting (zeros when never configured).
pub fn stats() -> Stats {
    let enabled = enabled();
    let Some(logger) = current() else {
        return Stats { enabled, ..Stats::default() };
    };
    let counters = &logger.counters;
    let offered = counters.offered.load(Ordering::Acquire);
    let dropped = counters.dropped.load(Ordering::Acquire);
    let written = counters.written.load(Ordering::Acquire);
    let failed = counters.failed.load(Ordering::Acquire);
    let accepted = offered.saturating_sub(dropped);
    let last_error = counters.last_error.lock().unwrap_or_else(PoisonError::into_inner).clone();
    Stats {
        enabled,
        running: counters.running.load(Ordering::Acquire),
        session: logger.session.clone(),
        path: logger.path.clone(),
        offered,
        accepted,
        dropped,
        rate_suppressed: counters.rate_suppressed.load(Ordering::Acquire),
        written,
        failed,
        pending: accepted.saturating_sub(written).saturating_sub(failed),
        rotations: counters.rotations.load(Ordering::Acquire),
        bytes_written: counters.bytes_written.load(Ordering::Acquire),
        current_bytes: counters.current_bytes.load(Ordering::Acquire),
        max_bytes: logger.cfg.max_bytes,
        segments: logger.cfg.segments,
        queue_capacity: logger.cfg.queue,
        rate_bytes_per_sec: logger.cfg.rate_bytes_per_sec,
        rate_records_per_sec: logger.cfg.rate_records_per_sec,
        last_error,
    }
}

/// One-line JSON status document (later NAPI surface; no serde dependency).
pub fn status_json() -> String {
    let s = stats();
    let last_error = if s.last_error.is_empty() {
        "null".to_string()
    } else {
        format!("\"{}\"", json_escape(&s.last_error))
    };
    format!(
        "{{\"component\":\"phone_log\",\"enabled\":{},\"running\":{},\"session\":\"{}\",\
\"path\":\"{}\",\"queue_capacity\":{},\"rate_bytes_per_sec\":{},\"rate_records_per_sec\":{},\
\"offered\":{},\"accepted\":{},\"written\":{},\
\"dropped\":{},\"rate_suppressed\":{},\"failed\":{},\"pending\":{},\"rotations\":{},\"bytes_written\":{},\
\"current_bytes\":{},\"segment_bytes\":{},\"segments\":{},\"last_error\":{}}}",
        s.enabled,
        s.running,
        json_escape(&s.session),
        json_escape(&s.path),
        s.queue_capacity,
        s.rate_bytes_per_sec,
        s.rate_records_per_sec,
        s.offered,
        s.accepted,
        s.written,
        s.dropped,
        s.rate_suppressed,
        s.failed,
        s.pending,
        s.rotations,
        s.bytes_written,
        s.current_bytes,
        s.max_bytes,
        s.segments,
        last_error,
    )
}

impl Logger {
    /// The ORDINARY hot path (every core line): one window, no scan, exactly the
    /// admission the tee had before the priority window existed.
    fn offer(&self, line: &str) {
        // Admission BEFORE any allocation and before the queue: a refused line
        // costs one short lock and a few arithmetic ops, no payload buffer, no
        // queue slot and no disk write.
        let admitted = self
            .rate
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .admit(mono_ms(), line.len(), self.limits);
        self.enqueue(line, admitted);
    }

    /// The EVIDENCE path ([`append_evidence`]): the small priority window is
    /// tried FIRST and the ordinary window is its fallback, so a marker is never
    /// worse off than an ordinary line and the ordinary budget is only touched
    /// when the priority allowance is already spent. Both attempts share one
    /// `now_ms`, so they agree on which second they charge; each is a FIXED
    /// budget, so the aggregate per window is their sum (see the module docs).
    fn offer_evidence(&self, line: &str) {
        let now_ms = mono_ms();
        let admitted = self
            .priority_rate
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .admit(now_ms, line.len(), self.priority_limits)
            || self
                .rate
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .admit(now_ms, line.len(), self.limits);
        self.enqueue(line, admitted);
    }

    /// The shared tail of both paths: count the offer, count a refusal (as a
    /// drop, named separately as rate suppression) or hand the payload to the
    /// bounded queue. Never blocks the caller.
    #[inline]
    fn enqueue(&self, line: &str, admitted: bool) {
        self.counters.offered.fetch_add(1, Ordering::AcqRel);
        if !admitted {
            // Over the per-second budget(s): refused, counted as a drop and
            // named separately; the caller is never blocked.
            self.counters.dropped.fetch_add(1, Ordering::AcqRel);
            self.counters.rate_suppressed.fetch_add(1, Ordering::AcqRel);
            return;
        }
        let payload = prepare_payload(line, self.cfg.max_line_chars);
        if self.tx.try_send(Msg::Line(payload)).is_err() {
            // Full (or the writer already exited): the line is dropped and
            // counted; the caller is never blocked.
            self.counters.dropped.fetch_add(1, Ordering::AcqRel);
        }
    }
}

fn valid_path(path: &str) -> bool {
    !path.is_empty() && path.len() <= 512 && !path.contains('\n') && path.contains('/')
}

fn deadline_after(timeout: Duration) -> Instant {
    Instant::now() + timeout.min(MAX_WAIT)
}

/// Ask one writer to drain, close and exit; bounded wait, ack-gated join.
fn stop_writer(logger: &Arc<Logger>, timeout: Duration) -> bool {
    if !logger.counters.running.load(Ordering::Acquire) {
        reap(logger);
        return true;
    }
    let deadline = deadline_after(timeout);
    let (ack_tx, ack_rx) = mpsc::channel::<()>();
    let mut queued = false;
    loop {
        match logger.tx.try_send(Msg::Shutdown(ack_tx.clone())) {
            Ok(()) => {
                queued = true;
                break;
            }
            Err(TrySendError::Full(_)) => {
                if Instant::now() >= deadline {
                    break;
                }
                thread::sleep(Duration::from_millis(1));
            }
            Err(TrySendError::Disconnected(_)) => break,
        }
    }
    if queued {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if ack_rx.recv_timeout(remaining).is_ok() {
            // The writer acked AFTER closing the file: the join can only wait
            // for the thread to return, never for disk I/O.
            reap(logger);
            return true;
        }
        return false;
    }
    let stopped = !logger.counters.running.load(Ordering::Acquire);
    if stopped {
        reap(logger);
    }
    stopped
}

fn reap(logger: &Arc<Logger>) {
    let handle = logger.handle.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(handle) = handle {
        let _ = handle.join();
    }
}

// ---------------------------------------------------------------------------
// writer thread
// ---------------------------------------------------------------------------

struct WriterState {
    file: Option<File>,
    base: PathBuf,
    segments: u32,
    max_bytes: u64,
    session: String,
    seq: u64,
    pid: u32,
    current_bytes: u64,
    consecutive_errors: u32,
    disabled: bool,
    counters: Arc<Counters>,
}

impl WriterState {
    fn new(
        base: PathBuf,
        cfg: &Options,
        session: String,
        counters: Arc<Counters>,
    ) -> Result<WriterState, &'static str> {
        // Append mode: previous sessions are NEVER truncated.
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&base)
            .map_err(|_| "open-failed")?;
        let current_bytes = file.metadata().map(|meta| meta.len()).unwrap_or(0);
        Ok(WriterState {
            file: Some(file),
            base,
            segments: cfg.segments,
            max_bytes: cfg.max_bytes,
            session,
            seq: 0,
            pid: std::process::id(),
            current_bytes,
            consecutive_errors: 0,
            disabled: false,
            counters,
        })
    }

    fn write_record(&mut self, payload: &str) -> bool {
        if self.disabled {
            return false;
        }
        let seq = self.seq + 1;
        let dropped = self.counters.dropped.load(Ordering::Relaxed);
        let record = render_record(&self.session, seq, self.pid, dropped, payload);
        if self.current_bytes > 0 && self.current_bytes + record.len() as u64 > self.max_bytes {
            if let Err(code) = self.rotate() {
                self.record_failure(&code);
                return !self.disabled;
            }
        }
        let outcome = match self.file.as_mut() {
            Some(file) => file.write_all(record.as_bytes()),
            None => Err(std::io::Error::other("no-file")),
        };
        match outcome {
            Ok(()) => {
                self.seq = seq;
                self.current_bytes += record.len() as u64;
                self.counters.written.fetch_add(1, Ordering::AcqRel);
                self.counters.bytes_written.fetch_add(record.len() as u64, Ordering::AcqRel);
                self.counters.current_bytes.store(self.current_bytes, Ordering::Release);
                self.consecutive_errors = 0;
                true
            }
            Err(err) => {
                self.record_failure(&errno_code(&err));
                !self.disabled
            }
        }
    }

    /// Delete the oldest segment, shift the chain and start a fresh current
    /// segment. Errors degrade (the record is counted failed) instead of
    /// panicking; the handle is kept so later records still try.
    fn rotate(&mut self) -> Result<(), String> {
        let base = self.base.clone();
        let last = self.segments.saturating_sub(1);
        let _ = fs::remove_file(segment_path(&base, last));
        for index in (1..last).rev() {
            let from = segment_path(&base, index);
            if from.exists() {
                fs::rename(&from, segment_path(&base, index + 1)).map_err(|e| errno_code(&e))?;
            }
        }
        fs::rename(&base, segment_path(&base, 1)).map_err(|e| errno_code(&e))?;
        let fresh = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&base)
            .map_err(|e| errno_code(&e))?;
        self.file = Some(fresh);
        self.current_bytes = 0;
        self.counters.rotations.fetch_add(1, Ordering::AcqRel);
        self.counters.current_bytes.store(0, Ordering::Release);
        Ok(())
    }

    fn record_failure(&mut self, code: &str) {
        self.counters.failed.fetch_add(1, Ordering::AcqRel);
        *self.counters.last_error.lock().unwrap_or_else(PoisonError::into_inner) = code.to_string();
        self.consecutive_errors = self.consecutive_errors.saturating_add(1);
        if self.consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
            self.disabled = true;
        }
    }

    /// Best-effort final flush + close. Never emits, never panics.
    fn finish(&mut self) {
        if let Some(mut file) = self.file.take() {
            let _ = file.flush();
        }
        self.counters.running.store(false, Ordering::Release);
    }

    /// Whatever is still queued when we stop is lost: count it, and ack any
    /// shutdown waiter so it does not sit out its timeout.
    fn drain_rest(&mut self, rx: &Receiver<Msg>) {
        loop {
            match rx.try_recv() {
                Ok(Msg::Line(_)) => {
                    self.counters.failed.fetch_add(1, Ordering::AcqRel);
                }
                Ok(Msg::Shutdown(ack)) => {
                    let _ = ack.send(());
                }
                Err(_) => return,
            }
        }
    }
}

fn writer_thread(mut state: WriterState, rx: Receiver<Msg>) {
    loop {
        match rx.recv() {
            Ok(Msg::Line(payload)) => {
                if !state.write_record(&payload) {
                    break;
                }
            }
            Ok(Msg::Shutdown(ack)) => {
                let _ = ack.send(());
                break;
            }
            Err(_) => break,
        }
        let mut shutdown_ack = None;
        loop {
            match rx.try_recv() {
                Ok(Msg::Line(payload)) => {
                    if !state.write_record(&payload) {
                        break;
                    }
                }
                Ok(Msg::Shutdown(ack)) => {
                    shutdown_ack = Some(ack);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        if let Some(ack) = shutdown_ack {
            let _ = ack.send(());
            break;
        }
        if state.disabled {
            break;
        }
    }
    state.drain_rest(&rx);
    state.finish();
}

fn render_record(session: &str, seq: u64, pid: u32, dropped: u64, payload: &str) -> String {
    format!(
        "{RECORD_PREFIX}sess={session}|seq={seq}|wall_ms={}|mono_ms={}|pid={pid}|dropped={dropped}|line={payload}\n",
        wall_ms(),
        mono_ms(),
    )
}

fn segment_path(base: &Path, index: u32) -> PathBuf {
    if index == 0 {
        return base.to_path_buf();
    }
    let mut name = base.as_os_str().to_os_string();
    name.push(format!(".{index}"));
    PathBuf::from(name)
}

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn mono_ms() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

fn errno_code(err: &std::io::Error) -> String {
    match err.raw_os_error() {
        Some(code) => format!("errno-{code}"),
        None => format!("io-{:?}", err.kind()),
    }
}

fn sanitize_session(given: Option<&str>) -> String {
    let raw = match given {
        Some(text) if !text.is_empty() => take_chars(text, 64),
        _ => format!(
            "{}-{}-{}",
            wall_ms(),
            std::process::id(),
            SESSION_COUNTER.fetch_add(1, Ordering::AcqRel)
        ),
    };
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("session");
    }
    out
}

// ---------------------------------------------------------------------------
// ingress preparation: flatten + redact + cap
// ---------------------------------------------------------------------------

fn prepare_payload(line: &str, max_line_chars: usize) -> String {
    let raw = take_chars(line, MAX_RAW_CHARS);
    let flat = if raw.bytes().any(|b| b < 0x20 || b == 0x7f) {
        flatten_control(&raw)
    } else {
        raw
    };
    let redacted = redact(&flat);
    cap_line(&redacted, max_line_chars)
}

fn flatten_control(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\n' | '\r' | '\t' => out.push(' '),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {}
            c => out.push(c),
        }
    }
    out
}

/// Keep the first `n` characters (never splits UTF-8).
fn take_chars(input: &str, n: usize) -> String {
    match input.char_indices().nth(n) {
        None => input.to_string(),
        Some((index, _)) => input[..index].to_string(),
    }
}

/// Cap to `n` characters, marking a real truncation.
fn cap_line(input: &str, n: usize) -> String {
    if input.chars().count() <= n {
        return input.to_string();
    }
    let keep = n.saturating_sub(TRUNC_MARK.chars().count());
    let mut out = take_chars(input, keep);
    out.push_str(TRUNC_MARK);
    out
}

/// Best-effort pattern filter (module docs list exactly what it covers and
/// what it does not). Idempotent on text that is already redacted.
fn redact(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    while i < bytes.len() {
        // URL userinfo: scheme://user:pass@host -> scheme://[redacted]@host.
        if bytes[i] == b':'
            && bytes.get(i + 1) == Some(&b'/')
            && bytes.get(i + 2) == Some(&b'/')
        {
            let authority_start = i + 3;
            let authority_end = authority_end(bytes, authority_start);
            if let Some(at) = rfind_byte(bytes, authority_start, authority_end, b'@') {
                out.push_str(&input[i..authority_start]);
                out.push_str(REDACTED);
                out.push_str(&input[at..authority_end]);
                i = authority_end;
                continue;
            }
        }
        // JWT-shaped triplet.
        if bytes[i] == b'e'
            && bytes.get(i + 1) == Some(&b'y')
            && bytes.get(i + 2) == Some(&b'J')
            && (i == 0 || !is_word_byte(bytes[i - 1]))
        {
            if let Some(end) = jwt_end(bytes, i) {
                out.push_str(REDACTED_JWT);
                i = end;
                continue;
            }
        }
        // Bearer token.
        if i + 7 <= bytes.len() && eq_ignore_ascii(&bytes[i..i + 7], b"bearer ") {
            let value_start = skip_spaces(bytes, i + 7);
            let value_end = value_end(bytes, value_start);
            if value_end - value_start >= 8 && !is_placeholder(&input[value_start..value_end]) {
                out.push_str(&input[i..value_start]);
                out.push_str(REDACTED);
                i = value_end;
                continue;
            }
        }
        // Credential-labelled value.
        if let Some(found) = label_value_at(bytes, i) {
            let value = &input[found.value_start..found.value_end];
            let hit = if found.strong {
                is_strong_secret(value)
            } else {
                looks_like_secret(value)
            };
            if hit {
                out.push_str(&input[i..found.value_start]);
                out.push_str(REDACTED);
                out.push_str(&input[found.value_end..found.resume]);
                i = found.resume;
                continue;
            }
        }
        let len = utf8_len(bytes[i]);
        out.push_str(&input[i..i + len]);
        i += len;
    }
    out
}

struct LabelValue {
    value_start: usize,
    value_end: usize,
    resume: usize,
    strong: bool,
}

fn label_value_at(bytes: &[u8], i: usize) -> Option<LabelValue> {
    if i >= bytes.len() || !bytes[i].is_ascii_alphabetic() {
        return None;
    }
    if i > 0 && is_word_byte(bytes[i - 1]) {
        return None;
    }
    let mut j = i;
    while j < bytes.len()
        && j - i < 24
        && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'-')
    {
        j += 1;
    }
    let word = &bytes[i..j];
    let strong = STRONG_LABELS.iter().any(|label| eq_ignore_ascii(word, label));
    let weak = !strong && WEAK_LABELS.iter().any(|label| eq_ignore_ascii(word, label));
    if !strong && !weak {
        return None;
    }
    let mut k = j;
    while k < bytes.len() && bytes[k] == b' ' {
        k += 1;
    }
    let value_start = if k < bytes.len() && (bytes[k] == b'=' || bytes[k] == b':') {
        k + 1
    } else if k + 1 < bytes.len() && bytes[k] == b'"' && bytes[k + 1] == b':' {
        // JSON form `"label":"value"` (the opening quote was copied already).
        k + 2
    } else {
        return None;
    };
    let value_start = skip_spaces(bytes, value_start);
    if value_start >= bytes.len() {
        return None;
    }
    if bytes[value_start] == b'"' {
        let inner = value_start + 1;
        let close = find_byte(bytes, inner, b'"')?;
        Some(LabelValue { value_start: inner, value_end: close, resume: close + 1, strong })
    } else {
        let end = value_end(bytes, value_start);
        if end == value_start {
            return None;
        }
        Some(LabelValue { value_start, value_end: end, resume: end, strong })
    }
}

fn is_strong_secret(value: &str) -> bool {
    value.chars().count() >= 4 && !is_trivial(value) && !is_placeholder(value)
}

fn looks_like_secret(value: &str) -> bool {
    let len = value.chars().count();
    if !(8..=512).contains(&len) || is_placeholder(value) {
        return false;
    }
    let mut has_digit = false;
    let mut has_alpha = false;
    let mut has_mark = false;
    for byte in value.bytes() {
        match byte {
            b'0'..=b'9' => has_digit = true,
            b'a'..=b'z' | b'A'..=b'Z' => has_alpha = true,
            b'-' | b'_' | b'.' | b'/' | b'+' | b'=' | b'~' => has_mark = true,
            _ => return false,
        }
    }
    has_alpha && (has_digit || has_mark)
}

fn is_trivial(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "true" | "false" | "none" | "null" | "nil" | "unset" | "missing" | "empty" | "absent"
            | "unknown" | "redacted"
    ) {
        return true;
    }
    !lower.is_empty() && lower.bytes().all(|b| b.is_ascii_digit() || b == b'.')
}

fn is_placeholder(value: &str) -> bool {
    // Matches every marker this filter writes, including a value that a
    // delimiter truncated (`[redacted` before the closing bracket), so a
    // second pass over already-redacted text is a fixed point.
    value.to_ascii_lowercase().contains("redacted") || matches!(value, "***" | "…")
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn eq_ignore_ascii(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.eq_ignore_ascii_case(y))
}

fn utf8_len(byte: u8) -> usize {
    if byte < 0x80 {
        1
    } else if byte >= 0xf0 {
        4
    } else if byte >= 0xe0 {
        3
    } else if byte >= 0xc0 {
        2
    } else {
        1
    }
}

fn skip_spaces(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn is_value_delimiter(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(byte, b'|' | b',' | b';' | b'"' | b'\'' | b')' | b'}' | b']' | b'<' | b'>')
}

fn value_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() && !is_value_delimiter(bytes[i]) {
        i += 1;
    }
    i
}

fn is_authority_delimiter(byte: u8) -> bool {
    matches!(byte, b'/' | b'?' | b'#' | b'"' | b'\'' | b'|' | b',' | b'<' | b'>')
        || byte.is_ascii_whitespace()
}

fn authority_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() && !is_authority_delimiter(bytes[i]) {
        i += 1;
    }
    i
}

fn find_byte(bytes: &[u8], start: usize, needle: u8) -> Option<usize> {
    (start..bytes.len()).find(|index| bytes[*index] == needle)
}

fn rfind_byte(bytes: &[u8], start: usize, end: usize, needle: u8) -> Option<usize> {
    (start..end.min(bytes.len())).rev().find(|index| bytes[*index] == needle)
}

/// A JWT-shaped `eyJ…` triplet: ≥ 24 bytes, ≥ 3 segments of ≥ 4 bytes.
fn jwt_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut end = start;
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric()
            || matches!(bytes[end], b'-' | b'_' | b'.'))
    {
        end += 1;
    }
    let token = &bytes[start..end];
    if token.len() < 24 {
        return None;
    }
    let mut dots = 0usize;
    let mut segment = 0usize;
    let mut min_segment = usize::MAX;
    for byte in token {
        if *byte == b'.' {
            if segment < 4 {
                return None;
            }
            min_segment = min_segment.min(segment);
            segment = 0;
            dots += 1;
        } else {
            segment += 1;
        }
    }
    if dots < 2 || segment < 4 || min_segment < 4 {
        return None;
    }
    Some(end)
}

fn json_escape(input: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(input.len() + 2);
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// unit tests: pure ingress preparation (no global state, so they can run in
// parallel with the rest of the lib test binary)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_strong_labelled_values() {
        let text = redact("setup_key=SETUP-SENTINEL-9f3a2b7c endpoint=100.64.0.9:51820");
        assert!(!text.contains("SETUP-SENTINEL-9f3a2b7c"), "{text}");
        assert!(text.contains("[redacted]"), "{text}");
        assert!(text.contains("endpoint=100.64.0.9:51820"), "{text}");
    }

    #[test]
    fn redacts_quoted_json_labelled_values() {
        let text = redact("{\"password\":\"hunter2\",\"host\":\"peer.example.net\"}");
        assert!(!text.contains("hunter2"), "{text}");
        assert!(text.contains("\"password\":\"[redacted]\""), "{text}");
        assert!(text.contains("peer.example.net"), "{text}");
    }

    #[test]
    fn keeps_boolean_and_flag_values() {
        let text = redact("token_present=false password=none private_key=null token: none");
        assert_eq!(text, "token_present=false password=none private_key=null token: none");
    }

    #[test]
    fn weak_token_label_needs_a_credential_shape() {
        // Prose survives ...
        let prose = redact("relay token unusable (reason token: TokenExpired)");
        assert!(prose.contains("TokenExpired"), "{prose}");
        // ... an opaque, credential-shaped value does not.
        let token = redact("token: ab12cd34ef56gh78");
        assert!(!token.contains("ab12cd34ef56gh78"), "{token}");
        assert!(token.contains("[redacted]"), "{token}");
    }

    #[test]
    fn redacts_jwt_triplets() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dBjftJeZ4CVPmB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let text = redact(&format!("jwt={jwt} done"));
        assert!(!text.contains(jwt), "{text}");
        assert!(text.contains("[redacted]"), "{text}");
        assert!(text.contains("done"), "{text}");
    }

    #[test]
    fn redacts_url_userinfo_and_keeps_host() {
        let text = redact("url=rels://alice:hunter2@relay.example.org:28443/path");
        assert!(!text.contains("hunter2"), "{text}");
        assert!(!text.contains("alice"), "{text}");
        assert!(text.contains("relay.example.org:28443"), "{text}");
        assert!(text.contains("[redacted]@relay.example.org"), "{text}");
    }

    #[test]
    fn redacts_bearer_tokens() {
        let text = redact("authorization: Bearer BEARER-SENTINEL-77889900");
        assert!(!text.contains("BEARER-SENTINEL-77889900"), "{text}");
        assert!(text.contains("Bearer [redacted]"), "{text}");
    }

    #[test]
    fn redaction_is_idempotent() {
        let once = redact(
            "setup_key=SETUP-SENTINEL-9f3a2b7c url=rels://alice:hunter2@relay.example.org:28443 \
             auth=Bearer BEARER-SENTINEL-77889900",
        );
        let twice = redact(&once);
        assert_eq!(once, twice);
        assert!(!twice.contains("SENTINEL"), "{twice}");
    }

    #[test]
    fn flattens_controls_and_caps_with_a_marker() {
        let payload = prepare_payload("line one\nline two\tend", 1024);
        assert_eq!(payload, "line one line two end");
        let long = "x".repeat(4096);
        let capped = prepare_payload(&long, 128);
        assert!(capped.ends_with(TRUNC_MARK), "{}", &capped[capped.len() - 32..]);
        assert!(capped.chars().count() <= 128);
        // Multi-byte text is never split.
        let cjk = "认".repeat(4096);
        let capped = prepare_payload(&cjk, 128);
        assert!(capped.chars().count() <= 128);
        assert!(capped.starts_with('认'));
    }

    #[test]
    fn record_shape_is_self_describing() {
        let record = render_record("sess-1", 7, 4242, 3, "hello|world");
        assert!(record.starts_with("PL|sess=sess-1|seq=7|"), "{record}");
        assert!(record.contains("|pid=4242|dropped=3|line=hello|world"), "{record}");
        assert!(record.contains("|wall_ms="), "{record}");
        assert!(record.contains("|mono_ms="), "{record}");
        assert!(record.ends_with('\n'), "{record}");
    }

    #[test]
    fn session_tokens_are_sanitized() {
        assert_eq!(sanitize_session(Some("abc-123_x.y")), "abc-123_x.y");
        assert_eq!(sanitize_session(Some("a|b c")), "a_b_c");
        assert!(!sanitize_session(None).is_empty());
        assert_ne!(sanitize_session(None), sanitize_session(None));
    }

    // -----------------------------------------------------------------------
    // admission budget: deterministic, clock injected (no sleep, no global
    // state, so these run in parallel with the rest of the lib test binary)
    // -----------------------------------------------------------------------

    #[test]
    fn rate_window_bounds_bytes_and_records_then_recovers() {
        let limit = RateLimit { bytes: 100, records: 4 };
        let mut window = RateWindow::default();
        // Four 25-byte lines fill both budgets exactly.
        for i in 0..4 {
            assert!(window.admit(0, 25, limit), "line {i} must be admitted");
        }
        assert!(!window.admit(0, 25, limit), "the byte budget must refuse");
        assert!(!window.admit(0, 1, limit), "the record budget must refuse");
        assert!(!window.admit(999, 1, limit), "999 ms is still the same window");
        // The next second opens a fresh window with nothing carried over, so
        // the line that was just refused is admitted again.
        assert!(window.admit(1000, 60, limit), "the next window must admit again");
        assert!(!window.admit(1000, 60, limit), "the fresh window has its own budget");
        assert_eq!(window.bytes, 60);
        assert_eq!(window.records, 1);
    }

    #[test]
    fn rate_window_refusal_keeps_the_budget_for_the_lines_behind_it() {
        let limit = RateLimit { bytes: 100, records: 8 };
        let mut window = RateWindow::default();
        assert!(window.admit(0, 60, limit));
        assert!(!window.admit(0, 60, limit), "60 + 60 exceeds the window");
        assert_eq!(window.bytes, 60, "a refused line must not consume budget");
        assert_eq!(window.records, 1);
        // The smaller line behind the refused one still fits.
        assert!(window.admit(0, 40, limit));
        assert!(!window.admit(0, 1, limit));
    }

    #[test]
    fn rate_window_lets_one_oversized_line_through_per_window() {
        let limit = RateLimit { bytes: 100, records: 8 };
        let mut window = RateWindow::default();
        // A line far bigger than the whole window is charged at most the
        // window: it is not refused for ever, it just consumes the second.
        assert!(window.admit(0, 4096, limit));
        assert_eq!(window.bytes, 100);
        assert_eq!(window.records, 1);
        assert!(!window.admit(0, 1, limit));
        assert!(window.admit(1000, 1, limit), "the next window admits again");
    }

    #[test]
    fn rate_window_record_budget_is_independent_of_the_byte_budget() {
        let limit = RateLimit { bytes: u64::MAX, records: 3 };
        let mut window = RateWindow::default();
        for i in 0..3 {
            assert!(window.admit(0, 1, limit), "line {i}");
        }
        assert!(!window.admit(0, 1, limit), "the record budget must bind alone");
        assert_eq!(window.bytes, 3);
    }

    #[test]
    fn default_budget_carries_a_default_cap_record() {
        // The default window must be able to carry one full default-cap record
        // (a CHARACTER cap, so up to 4 UTF-8 bytes per character) plus framing.
        // Otherwise a legitimate long line could never be logged without
        // raising `rate_bytes_per_sec`.
        let frame = render_record("s", 1, 1, 0, "").len() as u64;
        let capped = DEFAULT_MAX_LINE_CHARS as u64 * 4 + frame;
        assert!(
            capped <= DEFAULT_RATE_BYTES_PER_SEC,
            "one capped record is {capped} bytes but the window is {DEFAULT_RATE_BYTES_PER_SEC}"
        );
        assert!(DEFAULT_RATE_RECORDS_PER_SEC >= 1);
    }

    // -----------------------------------------------------------------------
    // priority admission: the closed marker set (pure classification) and the
    // separate, small window its lines are charged to
    // -----------------------------------------------------------------------

    #[test]
    fn priority_markers_are_the_closed_evidence_set() {
        // Every listed class, in the exact text the core/shell emits on device.
        for line in [
            "ts=2026-10-12T11:24:33.020Z pid=7775 event=vpn_protect_begin \
             detail=VPN_PROTECT_BEGIN:stage=protect",
            "N5_ICE|selected-pair|192.168.50.9:34486|endpoint_changed=true|peer=t7tldQA3",
            "N6_WG_DEVICE|session-established|peer=t7tldQA3",
            "N6_WG_DEVICE|session-expired|peer=t7tldQA3",
            "N13_WG|carrier-attach|kind=relay|replaced=false|peer=t7tldQA3",
            "N13_WG|carrier-detach|peer=t7tldQA3",
            "N13_WG|carrier->direct|ice-selected-path|peer=t7tldQA3",
            "N13_WG|direct->carrier|no-ice-endpoint|peer=t7tldQA3",
            "connector: state -> connected",
            "connector: login attempt=login result=ok",
            "connector: login ok (peer address assigned)",
        ] {
            assert!(is_evidence(line), "{line}");
        }
        // The high-rate neighbours and the near-misses stay ORDINARY: they must
        // keep competing for the ordinary budget (a storm cannot buy priority
        // by resembling a marker). `append_shell` — the only caller of
        // `is_evidence` — routes exactly these to the ordinary path.
        for line in [
            "N5c_ICE|gather-stun|peer=t7tldQA3|outcome=mapped|iface=mux|ms=0",
            "N5d_ICE|check-rx|peer=t7tldQA3|remote=192.168.50.9:34486|rtt_ms=29",
            "N5e_ICE|mux|read=11|xor_hit=3|periodic=1",
            "N6_WG_DEVICE|peers=20",
            "N13_WG|carrier-reject|kind=relay|reason=no-lane|count=100",
            "N13_WG_FEED|replay-carrier-failed|peer=t7tldQA3",
            "N11_ICE|failed|peer=t7tldQA3",
            "connector: signal frame routed (kind=offer)",
            "ROTLINE|i=1|pad=xxxx",
            "",
        ] {
            assert!(!is_evidence(line), "{line}");
        }
    }

    #[test]
    fn priority_window_is_separate_and_the_aggregate_bound_is_the_sum() {
        let ordinary = RateLimit { bytes: 100, records: 4 };
        let priority = RateLimit { bytes: 40, records: 2 };
        let mut ordinary_window = RateWindow::default();
        let mut priority_window = RateWindow::default();
        // The ordinary window is spent on ordinary lines ...
        for i in 0..4 {
            assert!(ordinary_window.admit(0, 25, ordinary), "ordinary line {i}");
        }
        assert!(!ordinary_window.admit(0, 1, ordinary), "the ordinary budget is exhausted");
        // ... while a priority line is still admitted by its OWN window ...
        assert!(priority_window.admit(0, 20, priority), "priority line 1");
        assert!(priority_window.admit(0, 20, priority), "priority line 2");
        // ... which is a fixed budget too: the allowance is bounded, not a hole.
        assert!(!priority_window.admit(0, 1, priority), "the priority allowance is bounded too");
        assert_eq!(priority_window.bytes, 40);
        assert_eq!(priority_window.records, 2);
        // Aggregate per window = the SUM of two fixed budgets, never unbounded.
        assert!(ordinary_window.bytes + priority_window.bytes <= ordinary.bytes + priority.bytes);
        assert!(
            ordinary_window.records + priority_window.records
                <= ordinary.records + priority.records
        );
        // The production shape keeps the ordinary bound unchanged and the
        // priority allowance strictly smaller than it.
        assert!(DEFAULT_PRIORITY_RATE_BYTES_PER_SEC < DEFAULT_RATE_BYTES_PER_SEC);
        assert!(DEFAULT_PRIORITY_RATE_RECORDS_PER_SEC < DEFAULT_RATE_RECORDS_PER_SEC);
    }

    #[test]
    fn default_priority_budget_carries_the_observed_startup_marker_burst() {
        // The 0.0.26 device runs refused evidence lines inside the first
        // seconds: one lifecycle/selection marker burst of ~10 lines (6 shell
        // `VPN_*` markers, the WG sessions coming up, the pair nominations and
        // the connector state/login lines) must fit ONE priority window.
        let burst = [
            "ts=1 pid=2 event=vpn_connection_created detail=VPN_CONNECTION_CREATED",
            "ts=1 pid=2 event=vpn_create_begin detail=VPN_CREATE_BEGIN:phase=create",
            "ts=1 pid=2 event=vpn_create_resolved detail=VPN_CREATE_RESOLVED:state=accepted",
            "ts=1 pid=2 event=vpn_protect_begin detail=VPN_PROTECT_BEGIN:stage=protect",
            "ts=1 pid=2 event=vpn_protect_resolved detail=VPN_PROTECT_RESOLVED:stage=protect",
            "ts=1 pid=2 event=vpn_ice_mux_armed detail=VPN_ICE_MUX_ARMED:stage=arm",
            "connector: state -> connected",
            "connector: login attempt=login result=ok",
            "N6_WG_DEVICE|session-established|peer=t7tldQA3",
            "N5_ICE|selected-pair|192.168.50.9:34486|endpoint_changed=true|peer=t7tldQA3",
            "N13_WG|carrier->direct|ice-selected-path|peer=t7tldQA3",
        ];
        let bytes: usize = burst.iter().map(|line| line.len()).sum();
        assert!((burst.len() as u64) <= DEFAULT_PRIORITY_RATE_RECORDS_PER_SEC, "{bytes} bytes");
        assert!((bytes as u64) <= DEFAULT_PRIORITY_RATE_BYTES_PER_SEC, "{bytes} bytes");
    }
}
