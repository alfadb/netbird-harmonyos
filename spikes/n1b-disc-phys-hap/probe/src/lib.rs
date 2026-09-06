//! # n1bdisc_probe — N1BDISC physical probe, native side (Rust/NAPI cdylib)
//!
//! Host-only build for `aarch64-unknown-linux-ohos` (SDK26 native llvm); no
//! device, no HDC, no signing. Authoritative spec:
//! `docs/n1b-disc-gate-plan.md@04cf222+CC-1(criteria-change-1-reviewed-pass-2026-09-05)`
//! (frozen, read-only; CC-1 变更后判据行号整体 +2 位移). The HAP
//! skeleton (ArkTS VpnExtensionAbility etc.) is owned by a parallel workstream;
//! this crate is the complete native side and nothing else.
//!
//! ## Channel
//! All facts go out on frozen HiLog: domain `0x2900`, tag `N1BDiscVpn`
//! (`N1BDISC_*` markers + `N1BDISC_CHUNK` fragments). Thread-safe; the D-W
//! worker thread emits directly.
//!
//! ## NAPI surface (module name: `n1bdisc_probe`)
//! Import from ArkTS: `import probe from 'libn1bdisc_probe.so'`.
//! **Every export is synchronous** (blocking) and returns a JSON **string**
//! (parse with `JSON.parse`). fd numbers cross the boundary as `number` (int):
//! `fdOrig` comes from the ArkTS-side `create()` resolve value, `fdDup` from
//! this crate's `d2_lock()` return — both are handed back verbatim to the
//! probe calls below.
//!
//! | export | signature | notes |
//! | --- | --- | --- |
//! | `version()` | `() => string` | crate/spec identity |
//! | `d1_probe(soPath: string)` | `(path) => string` | dlopen+dlsym of the HAP-internal native lib install path; 14 frozen symbols resolved by name, never called; 10 s box (measured) |
//! | `d2_lock(fdOrig: number)` | `(fd) => string` | retention-entry lock sequence S1-S7: F_GETFL baseline → F_DUPFD_CLOEXEC (fallback dup+F_SETFD) → F_GETFL dup → u6 → F_SETFL(O_NONBLOCK) on the DUP copy only → OFD side-effect → MTU prior; returns `fdDup` |
//! | `d2_late_fd(fd: number)` | `(fd) => string` | late-resolve fd: `N1BDISC_D2_LATE_FD` marker + ledger create, NEVER closed |
//! | `d2_entry_attempted(id: string)` | | `N1BDISC_D2_ENTRY\|id=\|phase=attempted` — call at each matrix create attempt |
//! | `d2_entry_outcome(id: string, outcome: string, fd: number)` | | `N1BDISC_D2_ENTRY\|id=\|outcome=\|fd=`; outcome ∈ resolved/rejected/timeout/late-resolved/late-rejected/indeterminate/not_attempted; fd=-1 → `none` |
//! | `d2_late(id: string, kind: string)` | | `N1BDISC_D2_LATE\|id=\|kind=<resolve\|reject>` (late callback isolation) |
//! | `rejtext_emit(item: number, text: string)` | | rejection text via CHUNK `stream=rejtext`; item = matrix id 0..4 (MR1→0 … MB1→4) |
//! | `d4_probe(fdDup: number, mb1: boolean)` | | U1/U3; dest `10.99.0.2:47001` (MR*) / `192.0.2.2:47001` (MB1) |
//! | `d5_probe(fdDup: number)` | | U2; 44 B frozen packet; sink binds `0.0.0.0:47002`; MB1 addresses follow the matrix flag recorded by `d4_probe` |
//! | `d8a_probe(fdDup: number, mb1: boolean)` | | 10-level ladder, per-level checksum |
//! | `d7_probe()` | `() => string` | 20 s frozen load (blocks the calling thread — that is the probe) |
//! | `d8b_probe(fdDup: number, mb1: boolean)` | | storm 10 s / 4 MiB / 50k writes; id fixed 11 |
//! | `dw_start(fdDup: number)` | | THE one `pthread_create`; worker runs the frozen sequence (SPAWN→drain→DRAIN→BARRIER→poll→RETURN→EXIT→terminal flag) |
//! | `dw_wait_barrier()` | `() => string` | ≤7 s bounded poll (10 ms ticks) of the barrier flag |
//! | `dw_wait_barrier_defer()` | `() => string` | barrier deferral wait (:715): after the 7 s box expired, ≤8 s bounded poll counted from that expiry; marker seen → confirmed (destroy may run, in-wait stays gated on the flag), else destroy stays deferred (barrier-never-observed) |
//! | `dw_inwait_collect()` | `() => string` | ≤2 s / 10 ms sampling of `/proc/self/task/<tid>/{stat,syscall}`; state parsed after the LAST `)` (A7); poll syscall set {73} |
//! | `dw_destroy_t()` | `() => string` | emits `N1BDISC_DW_DESTROY_T|mono_ms=` — call immediately BEFORE the ArkTS `destroy()` invocation |
//! | `dw_destroy_c()` | `() => string` | emits `N1BDISC_DW_DESTROY_C|mono_ms=` — call immediately AFTER the invocation, before resolve waiting |
//! | `dw_wait_terminal()` | `() => string` | ≤8 s bounded poll of the worker terminal flag; expiry registers `join-timeout` (join NOT called) |
//! | `dw_join()` | `() => string` | blocking `pthread_join` — only after the flag is set; the sole unbounded exemption. Refuses to join otherwise |
//! | `d6a(fdOrig: number)` | | S1-S3 (orig face; contains the ONLY `close(fd_orig)` site) |
//! | `d6b(fdDup: number)` | | S4-S7 (dup face; run only after worker terminal — join-timeout shape must skip via `skip_emit("D6b","join-timeout-abandoned")`) |
//! | `pre_emit(skipSummary: string)` | | P5T `N1BDISC_PRE` with the P5T-cut ledger digest; `skipSummary` = `item:cause` list or empty for `none` |
//! | `post_emit(destroyResolved: boolean)` | | P12: seven-step first-match chain (a→c→b→d→race window→e→f, :732-733) over one `dw_worker_terminal` decision load → `dw_return_class` derivation → `N1BDISC_POST` (d6_items/dw_outcome/final digest/worker_terminal_at_p12); emits `N1BDISC_DW_RACEWIN|expired=1` only in the late-race F=0-at-box-expiry cell |
//! | `skip_emit(item: string, cause: string)` | | generic `N1BDISC_SKIP|item=|cause=`; `item=="destroy"` / `item=="D-W"` are tracked for the P12 class derivation |
//! | `cleanup_sockets(notCreatedCause: string)` | | P11: close + F_GETFD(EBADF) re-verify of the three probe sockets; uncreated roles get `not-created|cause=` |
//!
//! ## A1-A12 self-check map (native side)
//! - A1 one `pthread_create` (dw.rs `dw_start`), zero thread/async bypasses,
//!   zero `napi_create_threadsafe_function` / `napi_create_async_work`.
//! - A2 `fd_orig`: only `F_GETFD`/`F_GETFL` (d2.rs S1/S6, d6.rs S1/S2) and the
//!   single `close(fd_orig)` in d6.rs S3. No read/write/F_SETFL on fd_orig.
//! - A3 BoringTun data plane never called (see DEVIATIONS #1 for the
//!   address-only reference tension).
//! - A4 every wait is a bounded flag-poll (10 ms `clock_nanosleep`) or a
//!   `poll(2)` with a frozen timeout; the only unbounded call is
//!   `pthread_join` behind the terminal flag. Zero `pthread_timedjoin_np`.
//! - A6 `openat` appears only in dw.rs `read_proc_stat`/`read_proc_syscall`
//!   with the two whitelisted `/proc/self/task/<tid>/…` paths, `O_RDONLY`.
//! - A7 stat parsing anchors on the last `)` (dw.rs `read_proc_stat`).
//! - A11/A12 poll raw single-read/same-source-two-writes and the single
//!   terminal-flag load binding five outputs (dw.rs worker + `post_emit`).
//!
//! ## DEVIATIONS / ambiguities (register for freeze review)
//! 1. **A3 vs. dynsym retention**: gate-plan A3 says "source references no
//!    BoringTun exported symbol beyond dlopen/dlsym/dlerror", but D1 requires
//!    the 14 symbols to RESOLVE in this very `.so` — unreferenced `#[no_mangle]`
//!    sections would be linker-GC'd out of dynsym. Per task instruction the
//!    symbols are declared in an `extern "C"` block and ADDRESS-referenced by a
//!    `#[used]` static (btkeep.rs); nothing is ever called. Alternative that
//!    avoids source references entirely: `-Wl,-u,<sym>` link flags.
//! 2. **gettid** is required by the frozen protocol (DW_SPAWN tid, /proc paths)
//!    but is not spelled in the :353-360 closed table; OHOS musl declares it
//!    in `<unistd.h>` and it is called as a plain libc function.
//! 3. **d1_so_sha256 / d1_cmdline / d1_pid** are NOT collected natively: all
//!    would need syscalls outside the closed table (file open, /proc/self/cmdline,
//!    getpid). Host/runner-side duties; the pid rides the HiLog capture stream.
//! 4. **dw_inwait_proc_fd inst numbering**: each sampling tick re-opens both
//!    /proc files ("openat → read → close"), so a full 2 s unhappy window
//!    creates many transient instances with auto-incrementing inst (first stat
//!    path = inst 1, first syscall path = inst 2, matching the spec's ordering
//!    wording; the "双实例" selftest fixture models the satisfied-first-sample
//!    happy path).
//! 5. **destroy-side close of fd_orig is not probe-observable**: the ledger
//!    emits close markers only for probe-performed closes (ret==0). An fd_orig
//!    closed by the framework stays "open" in the transition stream and is
//!    classified `open-at-exit` at the POST cut — the runner rebuilds from the
//!    same stream, so digests stay consistent; the d6a S3 EBADF shape is
//!    visible via its result marker.
//! 6. **D1_FAIL|err=** carries a sanitized inline copy (separator-escaped,
//!    length-capped); the verbatim dlerror text travels via the CHUNK
//!    `stream=dlerror|item=0` fragment, which is the authoritative transport.
//! 7. **d8a/d8b take an extra `mb1` parameter** beyond the minimum requested
//!    surface: the frozen D8 packet layout inherits D5's src/dst, which differ
//!    under MB1 retention.
//! 8. **legal-domain contradictions produce no class (M-02)**: a poll raw
//!    combination violating the legal-domain gate (E1, :724-725) yields no
//!    `dw_return_class` value (empty field, retired `domain-gate-violation`
//!    literal); the raw is preserved verbatim on `DW_RETURN` and the fail
//!    verdict is the runner-side rebuild's to carry.
//! 9. **POST `dw_outcome` / `d6_items` field layout** is not byte-frozen by the
//!    plan ("全列"); the layout used here is `class=…;join=…;watchdog=…;dist=…;
//!    poll_ret=…;poll_errno=…;poll_revents=…;poll_elapsed_ms=…` and
//!    `D6Sn=result|ret=<n>|errno=<e>` / `D6S7=result|fd=<n>|reuse=<b>` /
//!    `D6Sn=skipped(cause=…)`. The POST line is held conservatively at
//!    ≤1024 B total (d6_items ≤384 B, dw_outcome ≤448 B) — the hilog per-line
//!    truncation threshold has no measured bound (:1691).

#![allow(static_mut_refs)]

pub mod btkeep;
pub mod chunk;
pub mod d1;
pub mod d2;
pub mod d4;
pub mod d5;
pub mod d6;
pub mod d7;
pub mod d8;
pub mod dw;
pub mod hilog;
pub mod ledger;
pub mod napi;
pub mod net;
pub mod state;
pub mod sys;
pub mod util;
