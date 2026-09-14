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
//! - `config` — client configuration model (R3): peer config + validation
//!              (keys, CIDR, MTU, endpoint), own strict JSON reader; NO
//!              management/signal protocol logic
//! - `state`  — connection state machine (R3): explicit transition table,
//!              illegal transitions are errors, pure logic / no I/O
//! - `credential` — credential store trait (R3) + a DEVELOPMENT-ONLY,
//!              NON-SECURE in-memory implementation; platform secure storage
//!              (HarmonyOS) is a future implementation point: NOT implemented,
//!              NOT verified
//! - `management` — NetBird management REST API client skeleton (N3-1):
//!              HttpTransport trait + plain-text TCP transport (TEST BASELINE
//!              ONLY), TLS transport + https parsing (N3-2: rustls/ring,
//!              INJECTED trust root — no system store), login/session, peer
//!              list and node-config fetch; setup-key register remains a
//!              TODO(未确认) mock-only REST contract — the REAL registration
//!              path is the `grpc` module
//! - `grpc`    — NetBird management gRPC channel + `Login` (N3-2): tonic +
//!              rustls (injected trust root), stubs generated at build time
//!              from the in-repo verbatim `proto/management.proto`
//!              (proto/README.md: provenance + license);
//!              `ManagementService/Login(setupKey, meta, jwtToken,
//!              peerKeys)` → decoded LoginResponse session info; the
//!              upstream EncryptedMessage NaCl body-encryption layer is
//!              explicitly NOT implemented (module docs)
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
//! | `config_validate(json: string)` | `(json) => string` | validate a client-config JSON document -> `{valid:true,endpoint,mtu,routes,default_route,dns_servers,preshared_key,listen_port}` / `{valid:false,error}` (never echoes key material) |
//! | `mgmt_socket_open()` | `() => string` | N3-7: open + bind (NO connect) a TCP management socket so the shell can `VpnConnection.protect(fd)` BEFORE any packet flows -> `{fd,bind_rc,bind_errno}` (wg_fwd_open pattern, TCP variant) |
//! | `connector_start(configJson, setupKeyJson?)` | | start the N3-5 connection lifecycle (async worker; returns immediately) -> `{started:true,state}` / `{started:false,error}` — **N3-7: REFUSED by default** (`management-socket-required`); unprotected direct dial only with the explicit config opt-in `allow_unprotected_management:true` (debug, NOT upstream, dangerous). Poll `connector_status` |
//! | `connector_start_with_socket(fd, configJson, setupKeyJson?, addrJson)` | | N3-7 production start: protected management socket fd (shell-resolved DNS + `mgmt_socket_open` + `VpnConnection.protect` BEFORE this call); `addrJson` = `{"connect_addr":"ip:port"}`. Native dups the fd per dial (`F_DUPFD_CLOEXEC`), never uses/closes the original; every reconnect takes a FRESH protected socket. Fail-closed refusals: `socket-fd-missing` / `socket-fd-invalid` / `socket-addr-invalid` |
//! | `connector_socket_feed(fd)` | `(fd) => string` | N3-7 shell-side resupply of a fresh protected socket for reconnect dials -> `{ok:true,queued:N}` / `{ok:false,error}` |
//! | `connector_ice_socket_feed(fd)` | `(fd) => string` | N5c shell-side resupply of a fresh protected UDP socket for the per-peer ICE sessions (gather + per-candidate check sockets) -> `{ok:true,queued:N}` / `{ok:false,error}` (fail-closed: no feed ⇒ no candidates, peers stay Idle) |
//! | `connector_signal_socket_feed(fd, addrJson)` | `(fd, addr) => string` | N5d shell-side resupply of a fresh PROTECTED signal TCP socket plus the shell-resolved signal address (`addrJson` = `{"connect_addr":"ip:port"}`, REQUIRED) — the real signal link dials only over such sockets (initial AND every reconnect); the FIRST fed address wins for the link's lifetime. Starts the link once the sync-delivered `netbird_config.signal` URI + this address both exist -> `{ok:true,queued:N}` / `{ok:false,error}` (fail-closed: no feed ⇒ dial fails, never unprotected) |
//! | `connector_wg_socket_feed(fd)` | `(fd) => string` | N7 shell-side feed of the PROTECTED WG outer UDP socket (native-opened via `wg_fwd_open`, then `VpnConnection.protect`ed BEFORE any datagram flows — §二.4). First of the two feeds the real WireGuard data plane needs; the fd number is validated by a dup probe and stored BORROWED (device dups at adopt, never uses/closes the original). -> `{ok:true,device_up:<bool>}` / `{ok:false,error:"no-connector"\|"no-wg-device"\|"socket-fd-missing"\|"socket-fd-invalid"}` (fail-closed: without BOTH feeds the data plane never starts, `tunnel_ready=false`, default route HELD — no unprotected fallback) |
//! | `connector_tun_fd_feed(fd)` | `(fd) => string` | N7 shell-side feed of the platform TUN fd from `VpnConnection.create()`. The shell KEEPS the raw fd (its close belongs exclusively to `VpnConnection.destroy()`); native consumes ONLY a dup copy (`TunFd::dup_from_raw`) at device construction. Second of the two data-plane feeds -> same JSON shapes as `connector_wg_socket_feed`. **N8**: while the device is up this REPLACES the platform TUN fd (controlled recreate — new dup adopted, old dup deactivated, WG sessions kept); a WG-socket feed while up is idempotent for the same number and refused (`socket-fd-conflict`) for a different one |
//! | `connector_route_set_applied(routesJson, isRecreate)` | `(routes, recreate) => string` | N8 shell ACK that `routes` (`{"routes":["0.0.0.0/0","a.b.c.d/p"]}`) is the route set now APPLIED to the platform VpnConfig at (re)create() time; `isRecreate=false` = initial create, `true` = a completed controlled recreate (counts against `recreate.count`, arms the cooldown). Arms the desired-vs-applied comparison behind `connector_status().recreate` -> `{ok:true,recreate:{required,count,exhausted,cooling_down,reason}}` / `{ok:false,error:"no-connector"\|"route-set-invalid"}` |
//! | `connector_status()` | `() => string` | connector snapshot -> `{running,state,started_at_unix,last_update_unix,peer_count,route_count,reconnects,last_error,session_expiry,session_expires_at_unix,session_renew_attempts,wg_apply_failed,wg_apply_errors,logout_ok,terminal,ice:{peers,idle,gathering,checking,connected,disconnected,failed,endpoints_applied,reachable,last_error},signal:{registered,reconnects,last_error},wg:{fed_socket,fed_tun,device_up,ready,peers_with_session,handshakes,tx_packets,rx_packets,dropped_no_route,decrypt_errors},recreate:{required,count,exhausted,cooling_down,reason}}` (error = class + status code only, never key material; N3-7 `terminal` = worker ended by itself — fatal/exhausted — the shell tears the VPN down on it; N5c `ice` = per-peer ICE summary; N5d `signal` = real signal link state; N7 `wg` = REAL data-plane status of the device-backed seam; N8 `recreate` = controlled-recreate trigger: `required=true` when the desired route set diverges from the shell-ACKed applied set, bounded by count limit + cooldown) |
//! | `connector_network_config()` | `() => string` | read-only shell snapshot of the last applied network map -> `{available:true,serial,address,address_prefix_len,interface_dns,signal,routes:[{network,is_default}],dns:{service_enable,servers:[{ip,port}]},peer_count,peers:[{pub_key,allowed_ips}],default_route:{allowed,reason}}` / `{available:false,reason:"no-network-map"}` (public keys only — no secret material; N3-6; N3-7: `0.0.0.0/0` is exported only when the default-route gate allows, else held with a `default-route-held:*` reason; N5d `signal` = the `netbird_config.signal` URI for the shell to resolve + protect + feed; N7: the gate is re-decided against LIVE `tunnel_ready` at read time, so feeds/handshakes completing between syncs are reflected without waiting for the next map) |
//! | `connector_stop()` | `() => string` | idempotent stop (Sync stream close → best-effort logout → cleanup) -> `{ok,already_stopped,state}` |
//!
//! NOT yet promoted (still live in the spikes): the D2/D4/D5/D6/D7/D8/D-W fd
//! retention probe stages and the n1b fd-retention `state.rs` (unrelated to
//! this crate's `state.rs`, which is the R3 connection state machine), plus
//! the n1a ffi data pump (`pump.rs`), and the e3 C fdprobe (superseded here
//! by `fd_status`).

#![allow(static_mut_refs)]

pub mod abi;
// N3-4: management reconnect backoff (upstream defaultBackoff shape,
// injectable clock/rng).
pub mod backoff;
pub mod btkeep;
pub mod chunk;
pub mod config;
// N3-5: connection lifecycle orchestration (login + Sync session + data
// plane seams) with injectable management/sync/data-plane dependencies.
pub mod connector;
pub mod credential;
// N3-3: management message-body NaCl envelope (GetServerKey + crypto_box).
pub mod envelope;
pub mod grpc;
pub mod hilog;
// N9: HOST-ONLY interop harness sockets + CLI engine support
// (docs/self-hosted-interop-plan.md). 主机联调专用，非设备路径: this module
// creates plain sockets itself (no VpnConnection.protect exists on the host)
// and feeds them through the UNCHANGED production seams
// (`connector_start_with_socket` / `connector_*_feed`). It never relaxes the
// device-path fail-closed invariants: the device still starts only through
// shell-fed fds (`connector_start` keeps refusing by default), the device
// path never calls into this module, and the shipped cdylib behavior is
// unchanged. Compiled into the rlib so `cargo test` can cover the pure
// logic; only `cargo build --features cli --bin nbinterop` consumes it.
pub mod host_sockets;
// N5a: ICE candidate gathering (host + srflx via STUN) — candidate model in
// the Body.payload wire form, protected-UDP provider, fail-closed seams.
pub mod ice;
// N5b: ICE session layer — credentials, trickle, connectivity checks with
// MESSAGE-INTEGRITY/FINGERPRINT, pair state machine, nomination/selected
// pair, keepalive/disconnect timers (injected clock).
pub mod ice_session;
pub mod ledger;
pub mod management;
// N2-H (host-side evidence tool): route-exclusion ISOLATION EVIDENCE for one
// connection session — frozen outer endpoints, per-protocol unique probes,
// TUN package-level NEGATIVE evidence, tunnel POSITIVE CONTROL, endpoint-side
// delivery proof, counter reconciliation, verdict n2h-pass/fail/inconclusive.
// Host-side analysis only: the device path never calls into this module and
// the shipped cdylib behavior is unchanged; it does NOT change governance
// (N2-H takes effect only after user approval + formal revision — module docs).
pub mod n2h;
// N3-7: protected management socket seam (fd provider + per-dial dup-only
// consumption + tonic connector service) — fail-closed gap 1 fix.
pub mod mgmtsock;
// N3-4: minimal NetworkMap model decoded from Sync frames.
pub mod network_map;
pub mod napi;
pub mod net;
// N5c: per-peer ICE orchestration — signal OFFER/ANSWER/候选 exchange,
// selected pair → WG endpoint landing, per-peer states (injected clock).
pub mod peer_conn;
// N4a: signal service channel (peer discovery / candidate exchange) — gRPC
// bidi stream over the protected socket seam, per-peer NaCl envelopes.
pub mod signal;
pub mod state;
// N5a: minimal STUN client codec (RFC 5389 Binding + XOR-MAPPED-ADDRESS)
// feeding ice.rs srflx gathering; RFC 5769 vectors pinned in tests.
pub mod stun;
// N3-4: management Sync session (first frame, frame decode, reconnect).
pub mod sync;
pub mod sys;
pub mod tun;
pub mod util;
// N6: multi-peer WireGuard data plane (real BoringTun tunnels + protected
// UDP socket + TUN dup) — the real driver behind the connector WG seam.
pub mod wg_device;
pub mod wg;
