//! # netbird_core — NetBird HarmonyOS client core (Rust/NAPI cdylib)
//!
//! Product engineering skeleton. The modules below are promoted from the
//! verified spike chain (host-only cross build for
//! `aarch64-unknown-linux-ohos`, SDK26 native llvm; no device, no HDC, no
//! signing):
//!
//! - `wg`     — real BoringTun handshake + UDP + bidirectional forwarding
//!              probes, verbatim from spikes/n1b-disc-phys-hap/probe/src/wg.rs
//! - `ledger` — fd ledger (transition markers + canonical digest), verbatim
//!              from the same probe, plus the minimal `fd_status` F_GETFD
//!              check used by the entry module's fd contract
//! - `tun`    — TUN data plane over the dup-copy fd contract (R2-A): `TunFd`
//!              holds exactly one dup of the platform fd (F_DUPFD_CLOEXEC,
//!              dup() fallback), read/write with EAGAIN + short-write +
//!              bounded-backpressure handling, poll-based shutdown unblock,
//!              close-once with double-close/use-after-close as explicit
//!              errors; ledger transitions for every dup create/close; the
//!              platform raw fd is never stored nor closed by native
//! - `abi`    — dlopen+dlsym verification of the 14 frozen BoringTun ffi
//!              symbols in the loaded .so, adapted from the probe's d1.rs
//! - `napi`   — handwritten raw NAPI glue (no napi-rs; all exports
//!              synchronous, zero threadsafe-function / async-work)
//! - `btkeep`, `chunk`, `hilog`, `net`, `sys`, `util` — support modules,
//!              verbatim from the probe
//!
//! ## Channel
//! All facts go out on the frozen HiLog: domain `0x2900`, tag `N1BDiscVpn`
//! (`hilog.rs`; the probe-era tag is kept so the promoted marker stream stays
//! byte-compatible with the verified capture tooling).
//!
//! ## NAPI surface (module name: `netbird_core`)
//! Import from ArkTS: `import core from 'libnetbird_core.so'`.
//! Every export is synchronous (blocking) and returns a JSON **string**
//! (parse with `JSON.parse`). fd numbers cross the boundary as `number` (int).
//!
//! | export | signature | notes |
//! | --- | --- | --- |
//! | `version()` | `() => string` | crate identity |
//! | `abi_probe(soPath: string)` | `(path) => string` | dlopen+dlsym of the HAP-internal native lib install path; 14 frozen symbols resolved by name, never called |
//! | `fd_status(fd: number)` | `(fd) => string` | minimal F_GETFD liveness check (`{open,errno}`), replacement for the e3 C fdprobe |
//! | `wg_probe(fdDup: number, mb1: boolean)` | | in-process double-tunnel handshake + crypto round trip + TUN closure |
//! | `wg_udp_probe()` | `() => string` | plain-UDP reachability |
//! | `wg_net_probe(fdDup: number, mb1: boolean)` | | tunnel driven by REAL UDP datagrams |
//! | `wg_fwd_probe(fdDup: number, mb1: boolean)` | | real bidirectional data plane (blocking) |
//! | `wg_fwd_open()` | `() => string` | pre-bound WG UDP socket handle (fd < 0 = bind failed) |
//! | `wg_fwd_run(fd: number, fdDup: number, mb1: boolean)` | | fwd loop on a caller-owned (protectable) socket |
//! | `tun_open(fdOrig: number)` | `(fd) => string` | open a TUN session on a DUP copy of the platform fd (F_DUPFD_CLOEXEC, dup() fallback; raw fd never stored, never closed by native) -> `{ok,session,fd,nonblock_ofd}` |
//! | `tun_read(session: number)` | `(session) => string` | one read(2) on the session's dup -> `{ok,n,hex}` / `{eagain:true}` / `{eof:true}` / `{ok:false,error,errno}` |
//! | `tun_write(session: number, hexFrame: string)` | | full-frame write (short writes, EAGAIN + POLLOUT budget) -> `{ok,n}` / `{ok:false,error:"backpressure",written,errno}` |
//! | `tun_poll(session: number, timeoutMs: number)` | | poll(POLLIN) readiness; POLLNVAL -> `error:"badfd"` (foreign close / destroy detection) |
//! | `tun_close(session: number)` | | close the session's dup exactly once; second close -> `error:"already-closed"`, use after close -> `error:"closed"` |
//!
//! NOT yet promoted (still live in the spikes): the D2/D4/D5/D6/D7/D8/D-W fd
//! retention probe stages and `state.rs` (n1b), the n1a ffi data pump
//! (`pump.rs`), and the e3 C fdprobe (superseded here by `fd_status`).

#![allow(static_mut_refs)]

pub mod abi;
pub mod btkeep;
pub mod chunk;
pub mod hilog;
pub mod ledger;
pub mod napi;
pub mod net;
pub mod sys;
pub mod tun;
pub mod util;
pub mod wg;
