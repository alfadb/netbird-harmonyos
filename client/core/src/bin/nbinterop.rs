// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # nbinterop — HOST-SIDE interop CLI (N9) — 主机联调专用，非设备路径
//!
//! Drives the REAL protocol stack (`netbird_core::connector` → grpc/sync/
//! signal/ice/peer_conn → wg_device) as a host process against a real
//! self-hosted NetBird server, standing in for the ArkTS shell: this binary
//! creates plain TCP/UDP sockets itself (there is no VpnExtension / protect
//! on the host) and feeds them — BY FD NUMBER, dup-consumed — through the
//! UNCHANGED production seams. The library's fail-closed device path is not
//! touched: see `netbird_core::host_sockets` module docs and
//! `docs/self-hosted-interop-plan.md` for the operating recipe.
//!
//! ## Subcommands
//!
//! ```text
//! nbinterop selftest
//!     Offline checks (config parse, envelope roundtrip, ICE/STUN codec +
//!     loopback gather, two in-process WG devices with a bidirectional
//!     probe). No network beyond loopback binds. exit 0 = all pass, 1 = any
//!     failure. JSON report on stdout, human lines on stderr.
//!
//! nbinterop connect --config <file> [flags]
//!     Full flow: management login → Sync → signal register → ICE → WG
//!     handshake. No kernel TUN (a socketpair stands in; probe frames are
//!     injected by hand). Prints ONE `connector_status()` JSON object per
//!     line on stdout every --interval ms; milestones + logs go to stderr.
//!
//! nbinterop peer --config <file> [flags]
//!     The "other end" for two-process interop on one machine: same engine,
//!     SUCCESS exit (0) as soon as one WG peer session is established
//!     (`wg.peers_with_session >= 1`), instead of running until stopped.
//!
//! nbinterop n2h-peer --config <file> --overlay <ip[:port]> [flags]
//!     N2-H POSITIVE-CONTROL COUNTERPARTY (design §4.3/§7-I6): the host-side
//!     end of the device collector's a2b/b2a legs, riding the SAME tunnel as
//!     `peer` (no new socket, no new port, nothing product-facing):
//!       · echoes every well-formed `N2H-posctl-<nonce>|positive-control`
//!         datagram that arrives THROUGH the tunnel back to its source,
//!         payload VERBATIM — that is the device's a2b receipt;
//!       · periodically injects its OWN posctl probe toward the device overlay
//!         (`--overlay`) under ONE marker announced at start-up — the device
//!         registers that marker for its b2a observation;
//!       · prints a periodic `n2h-peer-stats|…` line carrying the announced
//!         marker and THIS peer's live `unknown_peer_drops` (the device's
//!         `wg_peer` outer-probe receipt, design §4.0/§4.3) on stderr.
//!     `--loopback` runs the offline 对拍 self-check instead (one socketpair
//!     as the tunnel stand-in + a fake device): exit 0 = the echo path and the
//!     status line both verified, no connector and no network involved.
//!
//! nbinterop isolation-check [--out <file>] [--fault-* ...]
//!     N2-H route-exclusion ISOLATION EVIDENCE for one connection session
//!     (host-side; see netbird_core::n2h). Offline dual-instance loopback
//!     run by default: freezes every outer endpoint family (management /
//!     signal / STUN / TURN / relay / WG peer / DNS), fires unique five-tuple
//!     + unique payload probes per family, scans every TUN frame for outer
//!     probe payloads (negative evidence), sends tunnel positive controls,
//!     collects endpoint-side receipts and reconciles interface counters,
//!     then writes the `n2h-isolation-evidence.json` document. Exit 0 =
//!     n2h-pass, 1 = n2h-fail, 20 = n2h-inconclusive. An evidence tool does
//!     NOT change governance: N2-H takes effect only after user approval +
//!     formal revision; per-socket protect stays UNSAT.
//!
//! Flags: --config <file>        config document (see the recipe doc)
//!        --dry-run              parse config + print the plan, NO socket,
//!                               NO resolution, NO connection (exit 0)
//!        --interval <ms>        status print period (default 2000, min 250)
//!        --timeout <s>          give up after N seconds (default 0 = never;
//!                               expiry exits 17, peer exits 18)
//!        --probe-dst <ip>       VPN IP to send probe packets to (the
//!                               peer's managed address)
//!        --probe-interval <ms>  probe period (default 0 = off)
//!        --overlay <ip[:port]>  n2h-peer ONLY: the DEVICE's overlay endpoint
//!                               (b2a destination; port defaults to
//!                               --echo-port). IPv4 literal — never a name
//!        --echo-port <port>     n2h-peer ONLY: the posctl port convention of
//!                               BOTH ends (default 40001 = the collector's
//!                               `posctl_port`); the port the device's a2b
//!                               probe is addressed to on this peer
//!        --posctl-interval <ms> n2h-peer ONLY: b2a probe period (default 1000)
//!        --loopback             n2h-peer ONLY: offline 对拍 self-check of the
//!                               echo path + status line (no --config, no
//!                               connector, no network beyond a socketpair)
//!        --exit-on-terminal     exit when the connector reaches a terminal
//!                               state (exit code = error class)
//!        --wg-port <port>       HOST-ONLY: bind the WG outer UDP socket to
//!                               a FIXED port instead of an ephemeral one
//!                               (host-side analog of the device's fixed
//!                               wg_fwd_open port; for a port-mapped pod)
//!        --ice-port <port>      HOST-ONLY: bind the peer's ICE socket to a
//!                               FIXED port (wildcard 0.0.0.0:<port>) — the
//!                               port the operator's UDP mapping aims at;
//!                               default = ephemeral (unchanged behavior)
//!        --advertise-candidate <ip:port>
//!                               HOST-ONLY: additionally advertise this
//!                               EXTERNALLY reachable address as an extra
//!                               host candidate (repeatable). Needed when
//!                               this process runs behind a port mapping:
//!                               its own interface addresses are not
//!                               reachable from the peer
//!        --verbose              forward HiLog markers to stderr
//!        --json                 accepted on selftest (report is JSON anyway)
//!        --help                 this text
//! ```
//!
//! ## Credential discipline
//!
//! The management URL, private key, setup key and CA come ONLY from the
//! config file or the environment (`NETBIRD_SETUP_KEY`, `NETBIRD_JWT`,
//! `NETBIRD_MANAGEMENT_URL`, `NETBIRD_CA_PEM`). Secret-bearing command-line
//! flags are REFUSED (exit 2). Secrets are never printed: plan/status output
//! carries the public key and lengths only.
//!
//! ## Exit codes
//!
//! `0` success / n2h-pass · `1` selftest failure / n2h-fail · `2` usage · `3`
//! config · `4` credentials · `10..16` connector error classes
//! (network/timeout/auth/request/server/parse/unsupported_url) · `17`
//! `--timeout` elapsed · `18` peer ended without a session · `19` unknown
//! error class · `20` n2h-inconclusive. `n2h-peer --timeout` is the registered
//! STOP form of a run that has no goal to reach: it exits `0`.
//!
//! ## Host link stubs
//!
//! Like the integration tests, the binary satisfies the OHOS-only symbols
//! the rlib references (libhilog_ndk.z.so / libace_napi.z.so do not exist on
//! the host). They are compiled OUT for the OHOS triple, where the real
//! libraries must resolve — the bin is host-only and never cross-built
//! (`required-features = ["cli"]` keeps it out of the OHOS chain entirely).

// host-only: OHOS builds must not define these symbols (the real libraries
// resolve there); an OHOS `--features cli` build therefore fails loudly at
// link time instead of silently stubbing the platform log.
#[cfg(not(target_env = "ohos"))]
mod host_link_stubs {
    use core::ffi::c_void;

    /// HiLog forwarder flag (set from --verbose).
    pub static FORWARD_HILOG: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);

    #[no_mangle]
    extern "C" fn OH_LOG_Print(
        _log_type: i32,
        _level: i32,
        _domain: u32,
        _tag: *const u8,
        _fmt: *const u8,
        arg: *const c_void,
    ) -> i32 {
        if FORWARD_HILOG.load(std::sync::atomic::Ordering::Relaxed) && !arg.is_null() {
            // the crate passes the marker text as the single %{public}s arg
            let bytes = arg as *const u8;
            let mut len = 0usize;
            unsafe {
                while len < 4096 && *bytes.add(len) != 0 {
                    len += 1;
                }
            }
            let text = unsafe { core::slice::from_raw_parts(bytes, len) };
            eprintln!("hilog|{}", String::from_utf8_lossy(text));
        }
        0
    }

    #[no_mangle]
    extern "C" fn OH_LOG_IsLoggable(_d: u32, _tag: *const u8, _l: i32) -> bool {
        false
    }

    #[no_mangle]
    extern "C" fn napi_module_register(_m: *mut c_void) {}

    #[no_mangle]
    extern "C" fn napi_create_function(
        _e: *mut c_void,
        _n: *const u8,
        _l: usize,
        _cb: *mut c_void,
        _d: *mut c_void,
        r: *mut *mut c_void,
    ) -> i32 {
        unsafe { *r = 0x10 as *mut c_void };
        0
    }

    #[no_mangle]
    extern "C" fn napi_set_named_property(
        _e: *mut c_void,
        _o: *mut c_void,
        _n: *const u8,
        _v: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    extern "C" fn napi_get_cb_info(
        _e: *mut c_void,
        _i: *mut c_void,
        _argc: *mut usize,
        _argv: *mut *mut c_void,
        _this: *mut c_void,
        _data: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    extern "C" fn napi_create_string_utf8(
        _e: *mut c_void,
        _s: *const u8,
        _l: usize,
        _r: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    extern "C" fn napi_get_value_string_utf8(
        _e: *mut c_void,
        _v: *mut c_void,
        _b: *mut u8,
        _bs: usize,
        _r: *mut usize,
    ) -> i32 {
        0
    }

    #[no_mangle]
    extern "C" fn napi_get_value_int32(_e: *mut c_void, _v: *mut c_void, _r: *mut i32) -> i32 {
        0
    }

    #[no_mangle]
    extern "C" fn napi_get_value_bool(_e: *mut c_void, _v: *mut c_void, _r: *mut bool) -> i32 {
        0
    }
}

#[cfg(target_env = "ohos")]
mod host_link_stubs {
    pub static FORWARD_HILOG: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
}

use std::time::{Duration, Instant};

use netbird_core::host_sockets as hs;
use netbird_core::n2h_collector as nc;
use netbird_core::sys;

use host_link_stubs::FORWARD_HILOG;

const USAGE: &str = "\
nbinterop — host-side NetBird interop CLI (host-only; see docs/self-hosted-interop-plan.md)

USAGE:
    nbinterop selftest [--wg-port <port>] [--ice-port <port>]
                       [--advertise-candidate <ip:port>]
    nbinterop connect --config <file> [--dry-run] [--interval <ms>] [--timeout <s>]
                      [--probe-dst <ip>] [--probe-interval <ms>] [--exit-on-terminal]
                      [--wg-port <port>] [--ice-port <port>]
                      [--advertise-candidate <ip:port>]... [--verbose]
    nbinterop peer    --config <file> [same flags]
    nbinterop n2h-peer --config <file> --overlay <ip[:port]> [--echo-port <port>]
                       [--posctl-interval <ms>] [--interval <ms>] [--timeout <s>]
    nbinterop n2h-peer --loopback [--overlay <ip[:port]>] [--echo-port <port>]
                       [--posctl-interval <ms>] [--timeout <s>]
    nbinterop isolation-check [--out <file>] [--loopback]
                              [--fault-leak] [--fault-no-posctl]
                              [--fault-omit-endpoint <kind>]
                              [--config <file>] [--outer-endpoint <kind:host:port>]...

n2h-peer (N2-H positive-control counterparty, design §4.3/§7-I6; opt-in by the
subcommand itself, no product surface, no new socket or port):
    --config <file>       the peer's own NetBird session (same config as `peer`);
                          required unless --loopback
    --overlay <ip[:port]> the DEVICE's overlay endpoint: where the b2a probe is
                          sent and where this peer is addressed as the posctl
                          endpoint. IPv4 literal only, port defaults to
                          --echo-port
    --echo-port <port>    the posctl port convention of both ends (default 40001,
                          the collector's arm-opt `posctl_port`): every
                          well-formed N2H-posctl-<nonce>|positive-control that
                          arrives through the tunnel is echoed back VERBATIM to
                          its source; anything else is ignored
    --posctl-interval <ms> b2a probe period (default 1000)
    --loopback            offline 对拍 self-check of the echo path + the status
                          line: one socketpair stands in for the tunnel and a fake
                          device must observe the b2a probe and must get its a2b
                          probe echoed verbatim (exit 0 = verified)

The peer status lines (stderr, the CLI's existing `nbinterop: <token>|<k=v>`
house shape; stdout stays pure status JSONL):
    nbinterop: n2h-peer|b2a_probe_id=…|b2a_dst=…|echo_port=…|posctl_interval_ms=…|loopback=…
    nbinterop: n2h-peer-stats|b2a_probe_id=…|b2a_sent=…|a2b_seen=…|echoes_sent=…
        |echoes_skipped=…|last_a2b_from=…|own_overlay=…|unknown_peer_drops=…
        |unknown_peer_drops_delta=…
    `b2a_probe_id` is the marker the device collector registers for its b2a
    observation; `unknown_peer_drops` is this peer's live WG counter (the
    device's wg_peer outer-probe receipt, design §4.0) read through the
    collector's installed host seam — `na` while no device is up.

isolation-check (N2-H evidence tool; evidence only — no governance change):
    --out <file>          evidence JSON path (default: n2h-isolation-evidence.json)
    --loopback            offline dual-instance loopback session (default;
                          synthetic fixed TEST keys, loopback sinks only)
    --config <file>       pre-connect freeze check against a real deployment:
                          freezes + probes what is knowable WITHOUT a session
                          (management_url from config + --outer-endpoint);
                          reads no secret material; verdict stays inconclusive
                          (no tunnel yet)
    --outer-endpoint      freeze one more endpoint (repeatable):
                          <kind:host:port>, kind in management|signal|stun|
                          turn|relay|wg_peer|dns
    --fault-leak          FAULT INJECTION (loopback): replay an outer probe
                          payload INTO the tunnel — the tool must return
                          n2h-fail (counter-example A)
    --fault-no-posctl     FAULT INJECTION (loopback): route the positive
                          control outside allowed_ips — must return
                          n2h-inconclusive (counter-example B)
    --fault-omit-endpoint FAULT INJECTION (loopback): simulate a freeze
                          failure for <kind> — must return n2h-inconclusive

HOST-ONLY port-mapping switches (N12a; no effect on the device path):
    --wg-port <port>      WG outer socket binds a FIXED port (default: ephemeral)
    --ice-port <port>     ICE socket binds 0.0.0.0:<port> (default: ephemeral);
                          use the port your operator's UDP mapping aims at
    --advertise-candidate <ip:port>
                          advertise an EXTERNALLY reachable address as an extra
                          host candidate (repeatable) — required when this
                          process's own interface addresses are unreachable from
                          the peer (e.g. a k8s pod behind a host port mapping)

Credentials (management URL / private key / setup key / CA) come ONLY from
the config file or the environment — never the command line.

Exit codes: 0 ok/n2h-pass | 1 selftest failed/n2h-fail | 2 usage | 3 config | 4 credentials
| 10 network | 11 timeout | 12 auth | 13 request | 14 server | 15 parse
| 16 unsupported_url | 17 wait-timeout | 18 peer-no-session | 19 unknown-class
| 20 n2h-inconclusive   (n2h-peer: --timeout is the registered stop form → 0)";

/// Secret-bearing flag prefixes the CLI must never accept.
const SECRET_FLAGS: [&str; 4] = ["--setup-key", "--jwt", "--private-key", "--ca-pem"];

#[derive(Debug)]
struct RunOpts {
    config: String,
    dry_run: bool,
    interval_ms: u64,
    timeout_s: u64,
    probe_dst: Option<String>,
    probe_interval_ms: u64,
    exit_on_terminal: bool,
    verbose: bool,
    /// HOST-ONLY (N12a): fixed port for the WG outer UDP socket.
    wg_port: Option<u16>,
    /// HOST-ONLY (N12a): fixed port for the peer's ICE socket.
    ice_port: Option<u16>,
    /// HOST-ONLY (N12a): externally reachable candidates to advertise.
    advertise: Vec<String>,
    /// N2-H I6: set by `n2h-peer`'s own flags ONLY (any other subcommand
    /// rejects them, so the mode cannot be entered by accident).
    n2h_peer: Option<N2hPeerOpts>,
}

impl Default for RunOpts {
    fn default() -> Self {
        RunOpts {
            config: String::new(),
            dry_run: false,
            interval_ms: 2000,
            timeout_s: 0,
            probe_dst: None,
            probe_interval_ms: 0,
            exit_on_terminal: false,
            verbose: false,
            wg_port: None,
            ice_port: None,
            advertise: Vec::new(),
            n2h_peer: None,
        }
    }
}

// ---------------------------------------------------------------------------
// N2-H I6: `n2h-peer`, the positive-control counterparty (design §4.3)
// ---------------------------------------------------------------------------

/// One TUN-stand-in frame buffer (the existing `recv_probes` bound: a WG inner
/// frame of the platform MTU plus headroom).
const N2H_PEER_FRAME_MAX: usize = 2048;
/// Frames drained per beat (bounded, like `recv_probes`' 8).
const N2H_PEER_DRAIN_MAX: usize = 8;
/// b2a probe period (design §4.3: the peer probes periodically while the device
/// window is open; the collector's own window deadline is 60 s).
const N2H_PEER_POSCTL_INTERVAL_MS: u64 = 1_000;
/// Default fuse of the `--loopback` 对拍 (it has no session to wait for).
const N2H_PEER_LOOPBACK_FUSE_MS: u64 = 5_000;
/// Per-step bounded wait of the 对拍's fake device.
const N2H_PEER_LOOPBACK_STEP_MS: i32 = 1_000;
/// The `--loopback` negative case's observation window (a non-posctl payload
/// must not come back).
const N2H_PEER_LOOPBACK_NEGATIVE_MS: i32 = 300;

/// `n2h-peer` options. Explicitly opt-in: NOTHING here is reachable without
/// the `n2h-peer` subcommand plus its flags (design §6.1's "default off, no
/// product surface" discipline).
#[derive(Debug, Clone, PartialEq, Eq)]
struct N2hPeerOpts {
    /// `--overlay <ip[:port]>`: the DEVICE's overlay endpoint. `None` in
    /// `--loopback` (which then plays both ends on 127.0.0.1) and refused in
    /// the live mode — the peer never guesses which overlay address is the
    /// device.
    overlay: Option<std::net::Ipv4Addr>,
    /// Port part of `--overlay` (default: [`N2hPeerOpts::echo_port`]).
    overlay_port: Option<u16>,
    /// The posctl port convention of BOTH ends: the collector's arm-opt
    /// `posctl_port` (§5.1 default 40001, [`nc::DEFAULT_POSCTL_PORT`]).
    echo_port: u16,
    posctl_interval_ms: u64,
    /// Offline 对拍 mode (`--loopback`): no config, no connector.
    loopback: bool,
}

impl Default for N2hPeerOpts {
    fn default() -> Self {
        N2hPeerOpts {
            overlay: None,
            overlay_port: None,
            echo_port: nc::DEFAULT_POSCTL_PORT,
            posctl_interval_ms: N2H_PEER_POSCTL_INTERVAL_MS,
            loopback: false,
        }
    }
}

impl N2hPeerOpts {
    /// The device overlay endpoint as `(ip, port)` — the b2a destination.
    fn overlay_dst(&self) -> Option<([u8; 4], u16)> {
        let ip = self.overlay?;
        Some((ip.octets(), self.overlay_port.unwrap_or(self.echo_port)))
    }
}

/// Parse `--overlay <ip[:port]>`: a LITERAL IPv4 address with an optional port
/// (design §10-4: this tool is IPv4-only). Refusing names is deliberate — a
/// probe endpoint that re-resolves between runs would not be reproducible, and
/// this value is echoed in the status line the device side reads.
fn parse_overlay_endpoint(s: &str) -> Result<(std::net::Ipv4Addr, Option<u16>), String> {
    let (ip, port) = match s.rsplit_once(':') {
        Some((ip, port)) => (ip, Some(port)),
        None => (s, None),
    };
    let ip: std::net::Ipv4Addr = ip
        .parse()
        .map_err(|_| format!("'{ip}' is not a literal IPv4 address (no names, no IPv6)"))?;
    let port = match port {
        Some(p) => {
            let p: u16 = p.parse().map_err(|_| format!("'{p}' is not a UDP port"))?;
            if p == 0 {
                return Err("port 0 is not a usable endpoint".to_string());
            }
            Some(p)
        }
        None => None,
    };
    Ok((ip, port))
}

/// `a.b.c.d:port` — the endpoint display form of every status line here (the
/// collector's own form, `n2h_collector.rs`'s posctl notes).
fn endpoint_text(ip: [u8; 4], port: u16) -> String {
    format!("{}.{}.{}.{}:{}", ip[0], ip[1], ip[2], ip[3], port)
}

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    // boringtun's ffi installs a process-global panic hook that turns panics
    // into silent SIGSEGV (see tests/wg_e2e.rs show_panics); re-install a
    // printing hook so failures stay diagnosable.
    static PANIC_HOOK: std::sync::Once = std::sync::Once::new();
    PANIC_HOOK.call_once(|| {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            eprintln!("nbinterop PANIC: {info}");
            default(info);
        }));
    });

    let args: Vec<String> = std::env::args().skip(1).collect();
    // hard credential-discipline gate: no secret ever travels on argv
    for a in &args {
        let lower = a.to_ascii_lowercase();
        for f in SECRET_FLAGS {
            if lower == f || lower.starts_with(&format!("{f}=")) {
                eprintln!(
                    "error: '{a}' looks like a secret on the command line — secrets are \
                     only accepted via the config file or environment variables"
                );
                return hs::EXIT_USAGE;
            }
        }
    }
    if args.is_empty() {
        eprintln!("{USAGE}");
        return hs::EXIT_USAGE;
    }
    match args[0].as_str() {
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            hs::EXIT_OK
        }
        "version" | "--version" => {
            println!("nbinterop {} (netbird_core; boringtun 0.7.1)", env!("CARGO_PKG_VERSION"));
            hs::EXIT_OK
        }
        "selftest" => cmd_selftest(&args[1..]),
        "connect" => cmd_engine("connect", &args[1..]),
        "peer" => cmd_engine("peer", &args[1..]),
        "n2h-peer" => cmd_n2h_peer(&args[1..]),
        "isolation-check" => cmd_isolation_check(&args[1..]),
        other => {
            eprintln!("error: unknown subcommand '{other}'\n\n{USAGE}");
            hs::EXIT_USAGE
        }
    }
}

// ---------------------------------------------------------------------------
// arg parsing (hand-rolled; no new dependencies)
// ---------------------------------------------------------------------------

/// Parse the flag list. Returns the options (and whether `--json` appeared);
/// exits via `Err(exit_code)` with a stderr message on any misuse.
fn parse_flags(args: &[String]) -> Result<(RunOpts, bool), i32> {
    let mut o = RunOpts::default();
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        let (key, inline_val) = match args[i].split_once('=') {
            Some((k, v)) => (k.to_string(), Some(v.to_string())),
            None => (args[i].clone(), None),
        };
        let mut consumed_next = false;
        let mut value = |what: &str| -> Result<String, i32> {
            if let Some(v) = inline_val.clone() {
                return Ok(v);
            }
            match args.get(i + 1) {
                Some(v) => {
                    consumed_next = true;
                    Ok(v.clone())
                }
                None => {
                    eprintln!("error: {what} requires a value");
                    Err(hs::EXIT_USAGE)
                }
            }
        };
        match key.as_str() {
            "--config" => {
                o.config = value("--config")?;
            }
            "--dry-run" => o.dry_run = true,
            "--interval" => {
                let v = value("--interval")?;
                o.interval_ms = v.parse().map_err(|_| {
                    eprintln!("error: --interval '{v}' is not a number");
                    hs::EXIT_USAGE
                })?;
                o.interval_ms = o.interval_ms.max(250);
            }
            "--timeout" => {
                let v = value("--timeout")?;
                o.timeout_s = v.parse().map_err(|_| {
                    eprintln!("error: --timeout '{v}' is not a number");
                    hs::EXIT_USAGE
                })?;
            }
            "--probe-dst" => {
                o.probe_dst = Some(value("--probe-dst")?);
            }
            "--probe-interval" => {
                let v = value("--probe-interval")?;
                o.probe_interval_ms = v.parse().map_err(|_| {
                    eprintln!("error: --probe-interval '{v}' is not a number");
                    hs::EXIT_USAGE
                })?;
            }
            "--exit-on-terminal" => o.exit_on_terminal = true,
            "--verbose" => o.verbose = true,
            "--wg-port" | "--ice-port" => {
                let v = value(&key)?;
                let port: u16 = v.parse().map_err(|_| {
                    eprintln!("error: {key} '{v}' is not a valid UDP port");
                    hs::EXIT_USAGE
                })?;
                if port == 0 {
                    eprintln!("error: {key} 0 is meaningless (0 = the ephemeral default; omit the flag)");
                    return Err(hs::EXIT_USAGE);
                }
                if key == "--wg-port" {
                    o.wg_port = Some(port);
                } else {
                    o.ice_port = Some(port);
                }
            }
            "--advertise-candidate" => {
                let v = value("--advertise-candidate")?;
                // fail at the CLI with exit 2, not deep inside the connector
                if let Err(e) = netbird_core::ice::parse_advertised_candidate(&v) {
                    eprintln!("error: --advertise-candidate '{v}': {e}");
                    return Err(hs::EXIT_USAGE);
                }
                o.advertise.push(v);
            }
            "--overlay" => {
                let v = value("--overlay")?;
                match parse_overlay_endpoint(&v) {
                    Ok((ip, port)) => {
                        let peer = o.n2h_peer.get_or_insert_with(N2hPeerOpts::default);
                        peer.overlay = Some(ip);
                        peer.overlay_port = port;
                    }
                    Err(e) => {
                        eprintln!("error: --overlay '{v}': {e}");
                        return Err(hs::EXIT_USAGE);
                    }
                }
            }
            "--echo-port" => {
                let v = value("--echo-port")?;
                let port: u16 = v.parse().map_err(|_| {
                    eprintln!("error: --echo-port '{v}' is not a valid UDP port");
                    hs::EXIT_USAGE
                })?;
                if port == 0 {
                    eprintln!(
                        "error: --echo-port 0 is meaningless (the posctl port convention; \
                         the collector's arm-opt default is {})",
                        nc::DEFAULT_POSCTL_PORT
                    );
                    return Err(hs::EXIT_USAGE);
                }
                o.n2h_peer.get_or_insert_with(N2hPeerOpts::default).echo_port = port;
            }
            "--posctl-interval" => {
                let v = value("--posctl-interval")?;
                let ms: u64 = v.parse().map_err(|_| {
                    eprintln!("error: --posctl-interval '{v}' is not a number");
                    hs::EXIT_USAGE
                })?;
                if ms == 0 {
                    eprintln!("error: --posctl-interval 0 would never send a b2a probe");
                    return Err(hs::EXIT_USAGE);
                }
                o.n2h_peer.get_or_insert_with(N2hPeerOpts::default).posctl_interval_ms = ms;
            }
            "--loopback" => {
                o.n2h_peer.get_or_insert_with(N2hPeerOpts::default).loopback = true;
            }
            "--json" => json = true,
            other => {
                eprintln!("error: unknown flag '{other}'\n\n{USAGE}");
                return Err(hs::EXIT_USAGE);
            }
        }
        i += 1 + usize::from(consumed_next);
    }
    Ok((o, json))
}

// ---------------------------------------------------------------------------
// selftest
// ---------------------------------------------------------------------------

fn cmd_selftest(args: &[String]) -> i32 {
    let (o, _) = match parse_flags(args) {
        Ok(v) => v,
        Err(code) => return code,
    };
    set_hilog_forward(false);
    // N12a: when the host-only port-mapping flags were given, the fixed-port
    // / advertised-candidate check exercises those EXACT values (offline).
    let tuning = hs::CliIceTuning {
        ice_fixed_port: o.ice_port,
        advertised_candidates: o.advertise.clone(),
        wg_fixed_port: o.wg_port,
    };
    let outcomes = hs::run_selftest_tuned(&tuning);
    println!("{}", hs::selftest_json(&outcomes));
    for o in &outcomes {
        if o.ok {
            eprintln!("[selftest] PASS {} — {}", o.name, o.detail);
        } else {
            eprintln!("[selftest] FAIL {} — {}", o.name, o.detail);
        }
    }
    if outcomes.iter().all(|o| o.ok) {
        eprintln!("[selftest] all checks passed");
        hs::EXIT_OK
    } else {
        hs::EXIT_SELFTEST_FAILED
    }
}

fn set_hilog_forward(on: bool) {
    FORWARD_HILOG.store(on, std::sync::atomic::Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// isolation-check (N2-H evidence tool)
// ---------------------------------------------------------------------------

/// `nbinterop isolation-check`: run the N2-H isolation-evidence pipeline and
/// emit `n2h-isolation-evidence.json`. Default (no --config) is the offline
/// dual-instance loopback session; `--config` runs a pre-connect endpoint
/// freeze check (management_url + --outer-endpoint, no session). Fault flags
/// arm the counter-example scenarios (they are INJECTIONS and are recorded as
/// such inside the evidence document).
fn cmd_isolation_check(args: &[String]) -> i32 {
    let mut out_path = String::from("n2h-isolation-evidence.json");
    let mut config: Option<String> = None;
    let mut outer: Vec<(netbird_core::n2h::EndpointKind, String, u16)> = Vec::new();
    let mut fault_leak = false;
    let mut fault_no_posctl = false;
    let mut fault_omit: Option<netbird_core::n2h::EndpointKind> = None;

    let mut i = 0;
    while i < args.len() {
        let (key, inline) = match args[i].split_once('=') {
            Some((k, v)) => (k.to_string(), Some(v.to_string())),
            None => (args[i].clone(), None),
        };
        let mut consumed_next = false;
        let mut value = |what: &str| -> Result<String, i32> {
            if let Some(v) = inline.clone() {
                return Ok(v);
            }
            match args.get(i + 1) {
                Some(v) => {
                    consumed_next = true;
                    Ok(v.clone())
                }
                None => {
                    eprintln!("error: {what} requires a value");
                    Err(hs::EXIT_USAGE)
                }
            }
        };
        match key.as_str() {
            "--out" => match value("--out") {
                Ok(v) => out_path = v,
                Err(code) => return code,
            },
            "--loopback" => {}
            "--config" => match value("--config") {
                Ok(v) => config = Some(v),
                Err(code) => return code,
            },
            "--fault-leak" => fault_leak = true,
            "--fault-no-posctl" => fault_no_posctl = true,
            "--fault-omit-endpoint" => {
                let v = match value("--fault-omit-endpoint") {
                    Ok(v) => v,
                    Err(code) => return code,
                };
                match netbird_core::n2h::EndpointKind::from_name(&v) {
                    Some(k) => fault_omit = Some(k),
                    None => {
                        eprintln!(
                            "error: --fault-omit-endpoint '{v}': kind must be one of {}",
                            kinds_help()
                        );
                        return hs::EXIT_USAGE;
                    }
                }
            }
            "--outer-endpoint" => {
                let v = match value("--outer-endpoint") {
                    Ok(v) => v,
                    Err(code) => return code,
                };
                match parse_outer_endpoint(&v) {
                    Ok(e) => outer.push(e),
                    Err(e) => {
                        eprintln!("error: --outer-endpoint '{v}': {e}");
                        return hs::EXIT_USAGE;
                    }
                }
            }
            "--json" | "--verbose" => {}
            other => {
                eprintln!("error: unknown flag '{other}' for isolation-check\n\n{USAGE}");
                return hs::EXIT_USAGE;
            }
        }
        i += 1 + usize::from(consumed_next);
    }

    let (faults_used, mode_name) = (
        fault_leak || fault_no_posctl || fault_omit.is_some(),
        config.is_some(),
    );
    if faults_used && mode_name {
        eprintln!("error: --fault-* flags apply only to the offline loopback mode (no --config)");
        return hs::EXIT_USAGE;
    }

    let evidence = if let Some(cfg) = &config {
        match netbird_core::n2h::run_preconnect_freeze(cfg, &outer) {
            Ok(ev) => ev,
            Err(e) => {
                eprintln!("error: {e}");
                return hs::EXIT_CONFIG;
            }
        }
    } else {
        let opts = netbird_core::n2h::LoopbackOpts {
            omit_kind: fault_omit,
            leak_outer_probe_into_tunnel: fault_leak,
            suppress_positive_control: fault_no_posctl,
        };
        match netbird_core::n2h::run_loopback_check(&opts) {
            Ok(ev) => ev,
            Err(e) => {
                eprintln!("error: loopback evidence run failed: {e}");
                return hs::EXIT_UNKNOWN_CLASS;
            }
        }
    };

    let json = evidence.to_json();
    if let Err(e) = std::fs::write(&out_path, format!("{json}\n")) {
        eprintln!("error: cannot write evidence file {out_path}: {e}");
        return hs::EXIT_UNKNOWN_CLASS;
    }
    println!("{json}");
    eprintln!(
        "[isolation-check] verdict={} evidence={} probes={} endpoints={} receipts={} counters-windows={}",
        evidence.verdict,
        out_path,
        evidence.probes.len(),
        evidence.frozen_endpoints.len(),
        evidence.endpoint_side.len(),
        evidence.counters.len(),
    );
    for r in &evidence.reasons {
        eprintln!("[isolation-check] reason: {r}");
    }
    match evidence.verdict.as_str() {
        netbird_core::n2h::VERDICT_PASS => hs::EXIT_OK,
        netbird_core::n2h::VERDICT_FAIL => hs::EXIT_SELFTEST_FAILED,
        _ => hs::EXIT_INCONCLUSIVE,
    }
}

fn kinds_help() -> String {
    netbird_core::n2h::EndpointKind::ALL
        .iter()
        .map(|k| k.name())
        .collect::<Vec<_>>()
        .join("|")
}

/// Parse `<kind>:<host>:<port>` (host may be a DNS name; the transport is
/// derived from the kind).
fn parse_outer_endpoint(
    s: &str,
) -> Result<(netbird_core::n2h::EndpointKind, String, u16), String> {
    let (kind, rest) = s
        .split_once(':')
        .ok_or_else(|| format!("expected <kind:host:port>, got '{s}'"))?;
    let kind = netbird_core::n2h::EndpointKind::from_name(kind)
        .ok_or_else(|| format!("unknown kind '{kind}' (use one of {})", kinds_help()))?;
    let (host, port) = rest
        .rsplit_once(':')
        .ok_or_else(|| format!("expected <kind:host:port>, got '{s}'"))?;
    if host.is_empty() {
        return Err(format!("empty host in '{s}'"));
    }
    let port: u16 = port
        .parse()
        .map_err(|_| format!("bad port '{port}' in '{s}'"))?;
    Ok((kind, host.to_string(), port))
}

// ---------------------------------------------------------------------------
// n2h-peer (N2-H I6: the device collector's positive-control counterparty)
// ---------------------------------------------------------------------------

/// `nbinterop n2h-peer` — see the module docs. The subcommand IS the opt-in:
/// no code path here is reachable from `connect`/`peer`/`selftest`, and nothing
/// product-facing changes (design §6.1). `--loopback` runs the offline 对拍
/// self-check instead of a session.
fn cmd_n2h_peer(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--loopback" || a.starts_with("--loopback=")) {
        return cmd_n2h_peer_loopback(args);
    }
    cmd_engine("n2h-peer", args)
}

/// Live state of the peer-side posctl endpoint, plus the counters the periodic
/// status line reports. Only the engine's single loop thread touches it (the
/// bin spawns no threads of its own in this mode), so plain `&mut` state is the
/// correct minimal form — no atomics, no lock (the `ProbeStats` precedent).
struct N2hPeerState {
    opts: N2hPeerOpts,
    /// The ONE announced b2a marker, stable for the whole run: the device
    /// registers the counterparty marker ONCE per window (`build_posctl` reads
    /// `peer_report()` a single time), so a fresh nonce per probe could never be
    /// observed — announcement and probes must agree on this value.
    marker: String,
    b2a_sent: u64,
    /// Well-formed posctl probes that arrived THROUGH the tunnel (the device's
    /// a2b leg reaching this peer — proof of the outer path).
    a2b_seen: u64,
    echoes_sent: u64,
    /// Posctl probes seen but not echoed (no learned overlay address yet, or the
    /// TUN-stand-in write failed): recorded, never silently dropped.
    echoes_skipped: u64,
    last_a2b_from: Option<String>,
    last_posctl_mono: u64,
    /// `unknown_peer_drops` at the FIRST readable sample (the delta is
    /// operator-facing; the device computes its own baseline at window open).
    drops_baseline: Option<u64>,
    own_absent_noted: bool,
    session_not_ready_noted: bool,
}

impl N2hPeerState {
    /// Fresh run state: the marker is `N2H-posctl-<24 hex nonce>` — exactly the
    /// shape the device's `build_posctl` builds and registers with the TUN
    /// scanner (shared code: [`netbird_core::n2h::probe_id`] +
    /// [`nc::POSCTL_KIND_TOKEN`]).
    fn new(opts: N2hPeerOpts) -> Result<N2hPeerState, String> {
        let nonce = netbird_core::n2h::fresh_nonce_hex()?;
        Ok(N2hPeerState {
            opts,
            marker: netbird_core::n2h::probe_id(nc::POSCTL_KIND_TOKEN, &nonce),
            b2a_sent: 0,
            a2b_seen: 0,
            echoes_sent: 0,
            echoes_skipped: 0,
            last_a2b_from: None,
            last_posctl_mono: 0,
            drops_baseline: None,
            own_absent_noted: false,
            session_not_ready_noted: false,
        })
    }

    /// The start-up announcement, printed BEFORE any probe so a capture window
    /// can never miss the marker (design §4.3: the device must be told which
    /// marker to register for b2a).
    fn announce_line(&self) -> String {
        let dst = match self.opts.overlay_dst() {
            Some((ip, port)) => endpoint_text(ip, port),
            None => "na".to_string(),
        };
        format!(
            "nbinterop: n2h-peer|b2a_probe_id={}|b2a_dst={dst}|echo_port={}|\
             posctl_interval_ms={}|loopback={}",
            self.marker, self.opts.echo_port, self.opts.posctl_interval_ms, self.opts.loopback
        )
    }

    /// Take the `unknown_peer_drops` sample of one status line: the first
    /// readable value becomes the run's baseline; `None` stays `None` (the line
    /// prints `na` — never a fabricated zero).
    fn sample_drops(&mut self, drops: Option<u64>) -> (Option<u64>, Option<u64>) {
        match drops {
            Some(v) => {
                let base = *self.drops_baseline.get_or_insert(v);
                (Some(v), Some(v.saturating_sub(base)))
            }
            None => (None, None),
        }
    }

    /// The periodic counterparty status line (design §4.3): ONE `|`-separated
    /// line on stderr in the CLI's existing `nbinterop: <token>|…` house shape
    /// (stdout is reserved for the status JSONL). It carries the announced b2a
    /// marker AND this peer's live `unknown_peer_drops` — the device's `wg_peer`
    /// outer-probe receipt (§4.0's `peer-device-offpath-drop`).
    fn status_line(
        &self,
        own: Option<[u8; 4]>,
        drops: (Option<u64>, Option<u64>),
    ) -> String {
        let own_text = match own {
            Some(ip) => endpoint_text(ip, self.opts.echo_port),
            None => "na".to_string(),
        };
        let (abs, delta) = match drops {
            (Some(a), Some(d)) => (a.to_string(), d.to_string()),
            _ => ("na".to_string(), "na".to_string()),
        };
        format!(
            "nbinterop: n2h-peer-stats|b2a_probe_id={}|b2a_sent={}|a2b_seen={}|echoes_sent={}|\
             echoes_skipped={}|last_a2b_from={}|own_overlay={own_text}|unknown_peer_drops={abs}|\
             unknown_peer_drops_delta={delta}",
            self.marker,
            self.b2a_sent,
            self.a2b_seen,
            self.echoes_sent,
            self.echoes_skipped,
            self.last_a2b_from.as_deref().unwrap_or("na"),
        )
    }

    /// One last status line at exit (the run's final counters).
    fn report_final(&self, own: Option<[u8; 4]>) {
        let drops = match n2h_peer_drops() {
            Some(v) => (Some(v), self.drops_baseline.map(|b| v.saturating_sub(b))),
            None => (None, None),
        };
        eprintln!("{}", self.status_line(own, drops));
    }
}

/// The counterparty's own `unknown_peer_drops` — the receipt of the device's
/// `wg_peer` outer probe (design §4.0's `peer-device-offpath-drop`) — read
/// through the collector's INSTALLED HOST SEAM (the connector's live WG feed;
/// `n2h_collector::installed_host`, design §7-I6's "reuse n2h_collector"). The
/// CLI never reads connector state itself: the seam is the one public path, and
/// `None` (no connector / no device up) is reported as `na`, never as a zero.
fn n2h_peer_drops() -> Option<u64> {
    nc::installed_host().and_then(|h| h.wg_stats()).map(|s| s.unknown_peer_drops)
}

/// One `n2h-peer` beat:
/// 1. drain the tunnel DELIVERY side and echo every well-formed posctl probe
///    back to its source, payload verbatim (§4.3's a2b receipt) — every beat, so
///    the echo is prompt;
/// 2. once per `--posctl-interval`, with a live session, inject the b2a probe
///    into the tunnel and print the status line.
fn n2h_peer_tick(
    state: &mut N2hPeerState,
    own: Option<[u8; 4]>,
    session_ready: bool,
    tun_hand: i32,
    now: u64,
) {
    n2h_peer_drain_and_echo(state, own, tun_hand);
    if now.saturating_sub(state.last_posctl_mono) < state.opts.posctl_interval_ms {
        return;
    }
    state.last_posctl_mono = now;
    if session_ready {
        n2h_peer_send_b2a(state, own, tun_hand);
    } else if !state.session_not_ready_noted {
        state.session_not_ready_noted = true;
        eprintln!(
            "[n2h-peer] no live WG session yet (wg.ready=false): the b2a probe waits — \
             nothing is injected into a tunnel that is not up"
        );
    }
    let drops = state.sample_drops(n2h_peer_drops());
    eprintln!("{}", state.status_line(own, drops));
}

/// Drain up to [`N2H_PEER_DRAIN_MAX`] frames the tunnel delivered, and echo
/// every well-formed posctl probe (`N2H-posctl-<nonce>|positive-control`) back
/// to its source with the five-tuple SWAPPED and the payload UNTOUCHED — the
/// device's to-os tee matches the marker, so verbatim is what closes its a2b
/// leg (design §4.3/D5).
///
/// Anything that is not a well-formed posctl probe is left alone: this endpoint
/// is not a reflector (the strict test is [`nc::posctl_marker`], the device
/// sender's own contract).
fn n2h_peer_drain_and_echo(state: &mut N2hPeerState, own: Option<[u8; 4]>, tun_hand: i32) {
    let mut buf = [0u8; N2H_PEER_FRAME_MAX];
    for _ in 0..N2H_PEER_DRAIN_MAX {
        // poll 0 = non-blocking: never wait in the data path
        let Some(n) = read_frame_within(tun_hand, &mut buf, 0) else { return };
        let frame = &buf[..n];
        let Some(view) = netbird_core::n2h::parse_ipv4_frame(frame) else { continue };
        if view.proto != 17 || nc::posctl_marker(view.payload).is_none() {
            continue;
        }
        let from = endpoint_text(view.src, view.sport);
        state.a2b_seen += 1;
        state.last_a2b_from = Some(from.clone());
        let Some(own) = own else {
            state.echoes_skipped += 1;
            if !state.own_absent_noted {
                state.own_absent_noted = true;
                eprintln!(
                    "[n2h-peer] a posctl probe arrived but this peer has no overlay address yet \
                     (network map not applied): not echoing — the source is never invented"
                );
            }
            continue;
        };
        let echo = hs::build_ipv4_udp_packet(own, view.src, view.dport, view.sport, view.payload);
        let (wn, errno) = sys::write_fd(tun_hand, &echo);
        if wn <= 0 {
            state.echoes_skipped += 1;
            eprintln!("[n2h-peer] echo write failed (errno={errno}) to {from}");
        } else {
            state.echoes_sent += 1;
            eprintln!(
                "[n2h-peer] echoed a2b probe back to {from} ({} bytes, verbatim)",
                view.payload.len()
            );
        }
    }
}

/// Inject ONE b2a posctl probe toward the device overlay (design §4.3): the
/// announced marker's payload, from this peer's overlay address on the posctl
/// port, into the TUN stand-in — i.e. through the real WG session.
fn n2h_peer_send_b2a(state: &mut N2hPeerState, own: Option<[u8; 4]>, tun_hand: i32) {
    let Some((dst_ip, dst_port)) = state.opts.overlay_dst() else { return };
    let Some(own) = own else {
        if !state.own_absent_noted {
            state.own_absent_noted = true;
            eprintln!(
                "[n2h-peer] no overlay address yet (network map not applied): the b2a probe waits"
            );
        }
        return;
    };
    let payload = nc::posctl_payload(&state.marker);
    let frame = hs::build_ipv4_udp_packet(
        own,
        dst_ip,
        state.opts.echo_port,
        dst_port,
        payload.as_bytes(),
    );
    let (n, errno) = sys::write_fd(tun_hand, &frame);
    if n <= 0 {
        eprintln!(
            "[n2h-peer] b2a inject failed (errno={errno}) to {}",
            endpoint_text(dst_ip, dst_port)
        );
        return;
    }
    state.b2a_sent += 1;
}

/// `Some(frame_len)` when a frame is readable on `fd` within `wait_ms`
/// (bounded poll + read); `None` = the wait expired or the fd had nothing
/// usable. `wait_ms = 0` makes it the non-blocking data-path read; the 对拍's
/// fake device uses it with a real wait so every step is a bounded
/// observation, never a sleep-as-assertion.
fn read_frame_within(fd: i32, buf: &mut [u8], wait_ms: i32) -> Option<usize> {
    let (ret, _errno, rev) = sys::poll1(fd, sys::POLLIN, wait_ms);
    if ret <= 0 || (rev & sys::POLLIN) == 0 {
        return None;
    }
    let (n, _errno) = sys::read_fd(fd, buf);
    if n <= 0 {
        None
    } else {
        Some(n as usize)
    }
}

// ---- the offline `--loopback` 对拍 (design §7-I6's CLI unit test) ----

/// Every field is an OBSERVATION of the fake device, never an expectation
/// written down as a result.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct N2hPeerLoopbackReport {
    /// ① the peer's ACTIVE b2a probe reached the device side.
    b2a_observed: bool,
    /// ② the device's a2b probe came back.
    a2b_echo_observed: bool,
    /// ② …byte-identical to what was sent.
    echo_verbatim: bool,
    /// ② …with the five-tuple swapped (src↔dst, sport↔dport).
    echo_tuple_swapped: bool,
    /// ③ a NON-posctl payload was echoed back (a reflection bug — must stay
    /// false).
    non_posctl_echo: bool,
    frames_seen: u64,
}

impl N2hPeerLoopbackReport {
    /// All three legs held.
    fn ok(&self) -> bool {
        self.b2a_observed
            && self.a2b_echo_observed
            && self.echo_verbatim
            && self.echo_tuple_swapped
            && !self.non_posctl_echo
    }

    fn to_json(&self) -> String {
        format!(
            "{{\"tool\":\"nbinterop\",\"check\":\"n2h-peer-loopback\",\"ok\":{},\
             \"b2a_observed\":{},\"a2b_echo_observed\":{},\"echo_verbatim\":{},\
             \"echo_tuple_swapped\":{},\"non_posctl_echo\":{},\"frames_seen\":{}}}",
            self.ok(),
            self.b2a_observed,
            self.a2b_echo_observed,
            self.echo_verbatim,
            self.echo_tuple_swapped,
            self.non_posctl_echo,
            self.frames_seen
        )
    }
}

/// `nbinterop n2h-peer --loopback`: the offline 对拍 of design §7-I6
/// ("CLI 单测（loopback 对拍）"), runnable on any host with no connector, no
/// config and no network beyond ONE AF_UNIX socketpair that stands in for the
/// tunnel. The PEER side runs the SHIPPED [`n2h_peer_tick`]; a FAKE DEVICE on
/// the far end plays the device collector's two posctl legs (see
/// [`N2hPeerLoopbackReport`]). Human lines — including the REAL status lines —
/// go to stderr, one JSON summary to stdout; exit 0 = all three legs held.
fn cmd_n2h_peer_loopback(args: &[String]) -> i32 {
    let (o, _json) = match parse_flags(args) {
        Ok(v) => v,
        Err(code) => return code,
    };
    if !o.config.is_empty() {
        eprintln!("error: --loopback starts no connector: drop --config\n\n{USAGE}");
        return hs::EXIT_USAGE;
    }
    let opts = o.n2h_peer.clone().unwrap_or_default();
    let fuse_ms = if o.timeout_s > 0 { o.timeout_s * 1000 } else { N2H_PEER_LOOPBACK_FUSE_MS };
    let (code, report) = match run_n2h_peer_loopback(&opts, fuse_ms) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: n2h-peer --loopback: {e}");
            return hs::EXIT_UNKNOWN_CLASS;
        }
    };
    println!("{}", report.to_json());
    eprintln!(
        "[n2h-peer] loopback 对拍: ok={} b2a_observed={} a2b_echo={} verbatim={} tuple_swapped={} \
         non_posctl_echo={} frames_seen={}",
        report.ok(),
        report.b2a_observed,
        report.a2b_echo_observed,
        report.echo_verbatim,
        report.echo_tuple_swapped,
        report.non_posctl_echo,
        report.frames_seen
    );
    code
}

/// The 对拍 itself: the real tick against a socketpair whose far end is the
/// fake device. Bounded by `fuse_ms` (the fake device's own steps are bounded
/// too, so the join always returns).
fn run_n2h_peer_loopback(
    opts: &N2hPeerOpts,
    fuse_ms: u64,
) -> Result<(i32, N2hPeerLoopbackReport), String> {
    // the loopback stand-in: this process plays itself as 127.0.0.1 and the
    // device as `--overlay` (default: the same loopback pair, posctl port)
    let own: [u8; 4] = [127, 0, 0, 1];
    let dev: ([u8; 4], u16) = opts
        .overlay_dst()
        .unwrap_or(([127, 0, 0, 1], opts.echo_port));
    let peer_port = opts.echo_port;
    let device_sport = 40_002u16; // the device's ephemeral posctl socket port
    // the state needs the EFFECTIVE destination (live mode always has one:
    // `cmd_engine` refuses an n2h-peer run without `--overlay`)
    let mut opts = opts.clone();
    opts.overlay = Some(std::net::Ipv4Addr::from(dev.0));
    opts.overlay_port = Some(dev.1);
    let mut state = N2hPeerState::new(opts)?;
    let a2b_payload = nc::posctl_payload(&netbird_core::n2h::probe_id(
        nc::POSCTL_KIND_TOKEN,
        &netbird_core::n2h::fresh_nonce_hex()?,
    ));

    let (peer_hand, dev_hand) = hs::open_tun_standby_pair()
        .map_err(|e| format!("TUN stand-in pair: {} (errno={})", e.token, e.errno))?;
    let mut bag = hs::FdBag::new();
    bag.keep(peer_hand);
    bag.keep(dev_hand);

    eprintln!("{}", state.announce_line());
    let fake = std::thread::spawn(move || {
        n2h_peer_fake_device(dev_hand, dev, (own, peer_port), device_sport, a2b_payload.as_bytes())
    });

    let start = Instant::now();
    while !fake.is_finished() && start.elapsed().as_millis() < fuse_ms as u128 {
        n2h_peer_tick(&mut state, Some(own), true, peer_hand, sys::mono_ms());
        sys::sleep_ms(20);
    }
    let report = match fake.join() {
        Ok(r) => r,
        Err(_) => return Err("the fake device panicked".to_string()),
    };
    state.report_final(Some(own));
    let code = if report.ok() { hs::EXIT_OK } else { hs::EXIT_SELFTEST_FAILED };
    Ok((code, report))
}

/// The fake device of the `--loopback` 对拍 (its own thread; every step is a
/// bounded poll):
/// ① it must see the peer's b2a probe arrive on the posctl port (the announced
///    marker, a well-formed `N2H-posctl-…|positive-control` payload);
/// ② it sends an a2b posctl probe FROM its own port and must get it back
///    verbatim with the five-tuple swapped;
/// ③ it sends one non-posctl payload and must get NOTHING back.
fn n2h_peer_fake_device(
    hand: i32,
    dev: ([u8; 4], u16),
    peer: ([u8; 4], u16),
    device_sport: u16,
    a2b_payload: &[u8],
) -> N2hPeerLoopbackReport {
    let mut report = N2hPeerLoopbackReport::default();
    let mut buf = [0u8; N2H_PEER_FRAME_MAX];

    // ① the peer's active b2a probe
    let deadline = Instant::now() + Duration::from_millis(N2H_PEER_LOOPBACK_STEP_MS as u64);
    while !report.b2a_observed && Instant::now() < deadline {
        let Some(n) = read_frame_within(hand, &mut buf, 100) else { continue };
        report.frames_seen += 1;
        let Some(view) = netbird_core::n2h::parse_ipv4_frame(&buf[..n]) else { continue };
        if view.proto == 17
            && view.dst == dev.0
            && view.dport == dev.1
            && nc::posctl_marker(view.payload).is_some()
        {
            report.b2a_observed = true;
            eprintln!(
                "[n2h-peer --loopback] device side observed the b2a probe {} ({} -> {})",
                String::from_utf8_lossy(view.payload),
                endpoint_text(view.src, view.sport),
                endpoint_text(view.dst, view.dport)
            );
        }
    }

    // ② the a2b probe + the verbatim echo
    let a2b = hs::build_ipv4_udp_packet(dev.0, peer.0, device_sport, peer.1, a2b_payload);
    let (wn, errno) = sys::write_fd(hand, &a2b);
    if wn as usize == a2b.len() {
        let deadline = Instant::now() + Duration::from_millis(N2H_PEER_LOOPBACK_STEP_MS as u64);
        while !report.a2b_echo_observed && Instant::now() < deadline {
            let Some(n) = read_frame_within(hand, &mut buf, 100) else { continue };
            report.frames_seen += 1;
            let Some(view) = netbird_core::n2h::parse_ipv4_frame(&buf[..n]) else { continue };
            if view.payload != a2b_payload {
                continue;
            }
            report.a2b_echo_observed = true;
            // the payload equality above IS the verbatim check
            report.echo_verbatim = true;
            report.echo_tuple_swapped = view.src == peer.0
                && view.sport == peer.1
                && view.dst == dev.0
                && view.dport == device_sport;
            eprintln!(
                "[n2h-peer --loopback] device side received the echo {} -> {} ({} bytes, verbatim)",
                endpoint_text(view.src, view.sport),
                endpoint_text(view.dst, view.dport),
                view.payload.len()
            );
        }
    } else {
        eprintln!("[n2h-peer --loopback] a2b write failed (errno={errno})");
    }

    // ③ the negative case: a non-posctl payload must NOT come back
    let junk = b"nbinterop-probe";
    let junk_frame = hs::build_ipv4_udp_packet(dev.0, peer.0, device_sport, peer.1, junk);
    let _ = sys::write_fd(hand, &junk_frame);
    let deadline = Instant::now() + Duration::from_millis(N2H_PEER_LOOPBACK_NEGATIVE_MS as u64);
    while Instant::now() < deadline {
        let Some(n) = read_frame_within(hand, &mut buf, 50) else { continue };
        report.frames_seen += 1;
        if let Some(view) = netbird_core::n2h::parse_ipv4_frame(&buf[..n]) {
            if view.payload == junk {
                report.non_posctl_echo = true;
                eprintln!(
                    "[n2h-peer --loopback] the peer echoed a NON-posctl payload (reflection bug)"
                );
            }
        }
    }
    report
}

// ---------------------------------------------------------------------------
// connect / peer engine
// ---------------------------------------------------------------------------

fn cmd_engine(mode: &'static str, args: &[String]) -> i32 {
    let (o, _) = match parse_flags(args) {
        Ok(v) => v,
        Err(code) => return code,
    };
    if mode == "n2h-peer" {
        // I6 fail-closed inputs, checked BEFORE anything is dialled: the
        // `n2h-peer` mode exists only with its flags (never inherited by
        // accident), and the device overlay endpoint is never guessed.
        match o.n2h_peer.as_ref() {
            None => {
                eprintln!("error: n2h-peer requires --overlay <ip[:port]> (the DEVICE's overlay endpoint)\n\n{USAGE}");
                return hs::EXIT_USAGE;
            }
            Some(p) if p.loopback => {
                eprintln!("error: n2h-peer --loopback starts no connector: drop --config\n\n{USAGE}");
                return hs::EXIT_USAGE;
            }
            Some(p) if p.overlay.is_none() => {
                eprintln!("error: n2h-peer requires --overlay <ip[:port]> (the DEVICE's overlay endpoint)\n\n{USAGE}");
                return hs::EXIT_USAGE;
            }
            Some(_) => {}
        }
    } else if o.n2h_peer.is_some() {
        eprintln!("error: the n2h-peer flags (--overlay/--echo-port/--posctl-interval/--loopback) belong to the n2h-peer subcommand\n\n{USAGE}");
        return hs::EXIT_USAGE;
    }
    if o.config.is_empty() {
        eprintln!("error: --config <file> is required for {mode}\n\n{USAGE}");
        return hs::EXIT_USAGE;
    }
    set_hilog_forward(o.verbose);

    // N12a HOST-ONLY: the port-mapping flags ride into the connector config
    // (ice_fixed_port / advertised_candidates); defaults inject nothing.
    let tuning = hs::CliIceTuning {
        ice_fixed_port: o.ice_port,
        advertised_candidates: o.advertise.clone(),
        wg_fixed_port: o.wg_port,
    };
    let loaded = match hs::load_cli_config_tuned(&o.config, &tuning) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: {e}");
            return e.exit_code;
        }
    };
    let plan = hs::plan_json("dry-run", &loaded.summary);
    if o.dry_run {
        // STRICT dry run: config parsed + plan printed. No resolution, no
        // sockets, no connections.
        println!("{plan}");
        return hs::EXIT_OK;
    }
    eprintln!("[nbinterop] {mode} plan: {plan}");
    run_engine(mode, &o, &loaded)
}

fn run_engine(mode: &'static str, o: &RunOpts, loaded: &hs::LoadedConfig) -> i32 {
    let mut bag = hs::FdBag::new();

    // ---- resolve + start over a host-created management socket ----
    let (_, mhost, mport) = match hs::parse_management_endpoint(&loaded.summary.management_url) {
        Ok(v) => v,
        Err(m) => {
            eprintln!("error: {m}");
            return hs::EXIT_CONFIG;
        }
    };
    let maddr = match hs::resolve_ipv4_host(&mhost, mport) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: management host '{mhost}' resolve failed: {e}");
            return hs::exit_code_for_class("network");
        }
    };
    eprintln!("[nbinterop] management {mhost}:{mport} -> {maddr}");
    let mgmt_fd = match hs::open_tcp_prebound() {
        Ok(fd) => fd,
        Err(e) => {
            eprintln!("error: management socket: {e}");
            return hs::exit_code_for_class("network");
        }
    };
    bag.keep(mgmt_fd);
    let state = match hs::start_connector_over_host_socket(
        mgmt_fd,
        &loaded.config_json,
        &loaded.secrets_json,
        maddr,
    ) {
        Ok(s) => s,
        Err(token) => {
            eprintln!("error: connector start refused: {token}");
            return match token.as_str() {
                "invalid-config" => hs::EXIT_CONFIG,
                "invalid-credentials" => hs::EXIT_CREDENTIALS,
                t if t.starts_with("socket-fd") => hs::exit_code_for_class("network"),
                _ => hs::EXIT_UNKNOWN_CLASS,
            };
        }
    };
    eprintln!("[nbinterop] connector started (state={state}) over host-fed management socket");

    // ---- data-plane feeds: WG outer UDP socket + TUN stand-in ----
    // N12a HOST-ONLY: --wg-port binds a FIXED port (the mapped outer port);
    // the default stays ephemeral (unchanged behavior).
    let wg_fd = match o.wg_port {
        Some(port) => match hs::open_udp_bound(port) {
            Ok(fd) => {
                eprintln!("[nbinterop] wg outer socket bound to fixed port {port}");
                fd
            }
            Err(e) => {
                eprintln!("error: wg socket (fixed port {port}): {e}");
                hs::stop_connector();
                return hs::exit_code_for_class("network");
            }
        },
        None => match hs::open_udp_ephemeral() {
            Ok(fd) => fd,
            Err(e) => {
                eprintln!("error: wg socket: {e}");
                hs::stop_connector();
                return hs::exit_code_for_class("network");
            }
        },
    };
    bag.keep(wg_fd);
    let (tun_fed, tun_hand) = match hs::open_tun_standby_pair() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: tun stand-in socketpair: {e}");
            hs::stop_connector();
            return hs::exit_code_for_class("network");
        }
    };
    bag.keep(tun_fed);
    bag.keep(tun_hand);
    if let Err(token) = hs::feed_wg_socket(wg_fd).and_then(|_| hs::feed_tun(tun_fed)) {
        eprintln!("error: wg feed refused: {token}");
        hs::stop_connector();
        return hs::exit_code_for_class("network");
    }
    eprintln!(
        "[nbinterop] wg outer socket ({}) + tun stand-in fed",
        match o.wg_port {
            Some(p) => format!("fixed port {p}"),
            None => "ephemeral port".to_string(),
        }
    );
    // N12a HOST-ONLY milestone: make the port-mapping posture observable.
    if o.ice_port.is_some() || !o.advertise.is_empty() {
        eprintln!(
            "[nbinterop] host-only port-mapping mode: ice_port={:?} advertise={:?}",
            o.ice_port, o.advertise
        );
    }

    // ---- feeder state ----
    let mut mgmt_queued: i64 = 0;
    let mut ice_queued: i64 = 0;
    let mut sig_queued: i64 = 0;
    let mut sig_addr: Option<std::net::SocketAddr> = None;
    let mut own_addr: Option<[u8; 4]> = None;
    let mut seen = Milestones::default();

    // N12a fixed-port mode: ONE shared `0.0.0.0:<ice_port>` socket for the
    // whole run (upstream shape, see ice_session::add_local_candidate_on).
    // The feeder hands out DUPS of it, so every gather round sees the same
    // already-bound socket and the port survives renegotiations — feeding
    // fresh sockets instead wedged the port after the first teardown
    // ("ice-bind-failed (errno=98)" forever: dup shares the socket object, so
    // the feeder's original stayed bound).
    let ice_shared: Option<i32> = match o.ice_port {
        Some(port) => match hs::open_udp_bound(port) {
            Ok(fd) => {
                eprintln!("[nbinterop] ice shared socket bound to 0.0.0.0:{port} (fed as dups)");
                Some(fd)
            }
            Err(e) => {
                eprintln!("error: ice shared socket (fixed port {port}): {e}");
                hs::stop_connector();
                return hs::exit_code_for_class("network");
            }
        },
        None => None,
    };

    // prime the queues (mirrors the shell feeding its first sockets)
    top_up_management(&mut bag, &mut mgmt_queued);
    top_up_ice(&mut bag, &mut ice_queued, ice_shared);

    let start = Instant::now();
    let mut last_print: u128 = 0;
    let mut last_maint: u128 = 0;
    let mut last_probe: u128 = 0;
    let mut printed = false;
    // N13 D2: sent/acked probe receipts (see `ProbeStats`).
    let mut probe = ProbeStats {
        enabled: o.probe_interval_ms > 0,
        ..Default::default()
    };
    // N2-H I6: the peer-side posctl endpoint, ONLY when `n2h-peer` asked for it
    // (every other mode leaves this `None` and runs the loop unchanged). The
    // b2a marker is announced HERE, before the first probe, so the device side
    // can register it for its own window.
    let mut n2h_peer = match o.n2h_peer.as_ref() {
        Some(p) => match N2hPeerState::new(p.clone()) {
            Ok(state) => {
                eprintln!("{}", state.announce_line());
                Some(state)
            }
            Err(e) => {
                eprintln!("error: n2h-peer entropy unavailable: {e}");
                hs::stop_connector();
                return hs::EXIT_UNKNOWN_CLASS;
            }
        },
        None => None,
    };

    loop {
        std::thread::sleep(Duration::from_millis(250));
        let now = start.elapsed().as_millis();

        if now.saturating_sub(last_maint) >= 1000 {
            last_maint = now;
            maintain(
                &mut bag,
                &mut mgmt_queued,
                &mut ice_queued,
                &mut sig_queued,
                &mut sig_addr,
                &mut own_addr,
                ice_shared,
            );
        }

        // N13 D2: the send tick is also the stats tick — ONE `probe-stats`
        // line per probe interval (the drain below can hand us up to 8 frames
        // per round, so a per-frame line would be spam).
        let mut probe_tick = false;
        if o.probe_interval_ms > 0
            && now.saturating_sub(last_probe) >= o.probe_interval_ms as u128
        {
            last_probe = now;
            probe_tick = true;
            send_probe(o, own_addr, tun_hand, &mut probe);
        }
        // N11 ⑥ evidence: frames that arrived THROUGH the tunnel land on the
        // TUN stand-in hand — drain + log them (payload-level proof).
        if o.probe_interval_ms > 0 {
            recv_probes(o, tun_hand, &mut probe);
        }
        if probe_tick {
            probe.report();
        }

        // N2-H I6: the peer-side posctl endpoint (echo every beat, probe +
        // status line once per --posctl-interval). `seen.wg_ready` is the
        // session gate: nothing is injected into a tunnel that is not up.
        if let Some(state) = n2h_peer.as_mut() {
            n2h_peer_tick(state, own_addr, seen.wg_ready, tun_hand, now as u64);
        }

        if !printed || now.saturating_sub(last_print) >= o.interval_ms as u128 {
            printed = true;
            last_print = now;
            let status = hs::status_json();
            println!("{status}");
            seen.report(&status);

            // The peer exits once a WG session exists — unless a probe target
            // was asked for, in which case the session is only the MEANS and
            // the run must stay up to keep probing (device validation: the
            // bidirectional probe needs the peer alive on both ends).
            if mode == "peer"
                && o.probe_dst.is_none()
                && status_num(&status, "wg.peers_with_session") >= 1
            {
                eprintln!("[nbinterop] peer goal reached: wg session established — exiting 0");
                hs::stop_connector();
                probe.report();
                return hs::EXIT_OK;
            }
            if mode == "peer"
                && o.probe_dst.is_some()
                && !seen.probe_session_note
                && status_num(&status, "wg.peers_with_session") >= 1
            {
                seen.probe_session_note = true;
                eprintln!(
                    "[nbinterop] wg session established — staying up for probes (dst={})",
                    o.probe_dst.as_deref().unwrap_or("?")
                );
            }
            if status_bool(&status, "terminal") {
                let class = status_str(&status, "last_error.class").unwrap_or_default();
                let note = format!(
                    "[nbinterop] connector terminal (class='{class}')"
                );
                if o.exit_on_terminal {
                    eprintln!("{note}");
                    hs::stop_connector();
                    probe.report();
                    return if class.is_empty() {
                        hs::EXIT_UNKNOWN_CLASS
                    } else {
                        hs::exit_code_for_class(&class)
                    };
                }
                eprintln!("{note} — still printing; --exit-on-terminal exits here");
            }
        }

        if o.timeout_s > 0 && now >= o.timeout_s as u128 * 1000 {
            if let Some(state) = n2h_peer.as_ref() {
                // `n2h-peer` has no goal to reach: `--timeout` IS the
                // registered stop form (§6.4-6), so a bounded run that did
                // what it was asked for exits 0 (and states its final
                // counters one last time).
                eprintln!(
                    "[n2h-peer] --timeout {}s elapsed; stopping (the registered stop form)",
                    o.timeout_s
                );
                state.report_final(own_addr);
                hs::stop_connector();
                return hs::EXIT_OK;
            }
            eprintln!(
                "[nbinterop] --timeout {}s elapsed before the goal; stopping",
                o.timeout_s
            );
            hs::stop_connector();
            probe.report();
            return if mode == "peer" {
                hs::EXIT_PEER_NOT_ESTABLISHED
            } else {
                hs::EXIT_WAIT_TIMEOUT
            };
        }
    }
}

// ---------------------------------------------------------------------------
// feeder maintenance (the shell's resupply loops, host side)
// ---------------------------------------------------------------------------

/// Keep ≥2 management sockets queued (reconnect dials take a fresh one).
fn top_up_management(bag: &mut hs::FdBag, queued: &mut i64) {
    if *queued >= 2 {
        return;
    }
    match hs::open_tcp_prebound() {
        Ok(fd) => match hs::feed_management(fd) {
            Ok(q) => {
                *queued = q;
                bag.keep(fd);
            }
            Err(token) => {
                unsafe { sys::close(fd) };
                eprintln!("[feeder] management feed refused: {token}");
            }
        },
        Err(e) => eprintln!("[feeder] management socket open failed: {e}"),
    }
}

/// Keep ≥4 unbound UDP sockets queued for ICE (gather rounds + check
/// sockets take fresh fds; two interface rounds must never starve).
fn top_up_ice(bag: &mut hs::FdBag, queued: &mut i64, shared: Option<i32>) {
    // Refresh the depth from the LIVE status first. The local counter is only
    // updated when we feed, so trusting it alone stops resupply exactly when
    // the queue has drained — the CLI then sat in endless
    // "protected-udp: no-protected-socket" while believing 4 were queued.
    let live = status_num(&hs::status_json(), "ice_sockets.queued");
    if live >= 0 {
        *queued = live;
    }
    if *queued >= 4 {
        return;
    }
    for _ in 0..2 {
        // Fixed-port mode: feed a DUP of the shared bound socket (never a new
        // unbound one — the shared socket is what keeps the mapped port
        // reachable across renegotiations).
        let opened: Result<i32, String> = match shared {
            Some(master) => {
                let dup = unsafe { sys::dup(master) };
                if dup < 0 {
                    Err(format!("dup of shared ice socket failed (errno={})", sys::errno()))
                } else {
                    Ok(dup)
                }
            }
            None => hs::open_udp_unbound().map_err(|e| format!("{} (errno={})", e.token, e.errno)),
        };
        match opened {
            Ok(fd) => match hs::feed_ice(fd) {
                Ok(q) => {
                    *queued = q;
                    bag.keep(fd);
                }
                Err(token) => {
                    unsafe { sys::close(fd) };
                    eprintln!("[feeder] ice feed refused: {token}");
                    break;
                }
            },
            Err(e) => {
                eprintln!("[feeder] ice socket open failed: {e}");
                break;
            }
        }
    }
}

/// One maintenance round (~1/s): queue watermarks + signal material once the
/// sync-delivered signal URI exists. Transient failures warn once per cause
/// and never kill the run.
fn maintain(
    bag: &mut hs::FdBag,
    mgmt_queued: &mut i64,
    ice_queued: &mut i64,
    sig_queued: &mut i64,
    sig_addr: &mut Option<std::net::SocketAddr>,
    own_addr: &mut Option<[u8; 4]>,
    ice_shared: Option<i32>,
) {
    top_up_management(bag, mgmt_queued);
    top_up_ice(bag, ice_queued, ice_shared);

    let net = hs::network_config_json();
    if let Some(addr) = hs::own_address_from_network_config(&net) {
        if own_addr.is_none() {
            // one-shot diagnostics: the probe source/target pair. The peer
            // VPN addresses are the WG allowed_ips (public runtime
            // material) — the operator aims --probe-dst at one of them.
            let peers = hs::peer_vpn_addresses_from_network_config(&net);
            eprintln!("[milestone] own tunnel address {addr:?}; peer vpn addresses: {peers:?}");
        }
        *own_addr = Some(addr);
    }
    if sig_addr.is_none() {
        if let Some(uri) = hs::signal_uri_from_network_config(&net) {
            match hs::parse_signal_uri(&uri) {
                Ok((host, port)) => match hs::resolve_ipv4_host(&host, port) {
                    Ok(a) => {
                        eprintln!("[milestone] signal '{uri}' -> {a}");
                        *sig_addr = Some(a);
                    }
                    Err(e) => eprintln!("[feeder] signal resolve '{host}' failed: {e}"),
                },
                Err(e) => eprintln!("[feeder] {e}"),
            }
        }
    }
    if let Some(addr) = *sig_addr {
        if *sig_queued < 2 {
            match hs::open_tcp_prebound() {
                Ok(fd) => match hs::feed_signal(fd, addr) {
                    Ok(q) => {
                        let first = *sig_queued == 0;
                        *sig_queued = q;
                        bag.keep(fd);
                        if first {
                            eprintln!("[milestone] signal socket fed (queued={q}) — link starts");
                        }
                    }
                    Err(token) => {
                        unsafe { sys::close(fd) };
                        eprintln!("[feeder] signal feed refused: {token}");
                    }
                },
                Err(e) => eprintln!("[feeder] signal socket open failed: {e}"),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// N13 D2: probe delivery counters (sent / acked)
// ---------------------------------------------------------------------------

/// Process-local probe delivery counters (volatile: no persistence, not in the
/// status JSON). The ONLY writers are `send_probe` / `recv_probes`, both called
/// back-to-back from `run_engine`'s single main-loop thread (the bin spawns no
/// threads of its own), so plain `&mut` state is the correct minimal form — no
/// atomics and no lock.
#[derive(Default)]
struct ProbeStats {
    /// Probes injected into the TUN stand-in hand (write returned > 0).
    sent: u64,
    /// Received frames carrying the probe fingerprint FROM our `--probe-dst`
    /// target: the peer's own probe reached this TUN, i.e. a receipt. The
    /// tunnel has no transport-level delivery ack (see `wg_device`'s carrier
    /// contract), so mutual application-level probing IS the delivery proof.
    acked: u64,
    /// Injections the TUN stand-in refused (never counted as `sent`).
    send_failed: u64,
    /// Emit the `probe-stats` line only when probing was asked for
    /// (`--probe-interval > 0`), so a non-probe run's stderr is unchanged.
    enabled: bool,
}

impl ProbeStats {
    /// One pipe-separated line, used periodically (at most once per probe
    /// interval, never per packet) and for the final value at exit.
    fn report(&self) {
        if self.enabled {
            eprintln!(
                "nbinterop: probe-stats|sent={}|acked={}|send_failed={}",
                self.sent, self.acked, self.send_failed
            );
        }
    }
}

/// Inject one probe packet through the TUN stand-in hand (the pump reads it
/// as if it came from the kernel TUN and ships it through the WG session).
/// N13 D2: an accepted write is one `sent`; a refused write is one
/// `send_failed`. The two early returns (no `--probe-dst`, own tunnel address
/// not synced yet) are not attempts at all, so they count as neither.
fn send_probe(o: &RunOpts, own_addr: Option<[u8; 4]>, tun_hand: i32, probe: &mut ProbeStats) {
    let Some(dst) = o.probe_dst.as_deref().and_then(hs::parse_ipv4) else {
        return;
    };
    let Some(src) = own_addr else {
        return; // own tunnel address not synced yet
    };
    let pkt = hs::build_ipv4_udp_packet(src, dst, 40000, 40001, b"nbinterop-probe");
    let (n, errno) = sys::write_fd(tun_hand, &pkt);
    if n <= 0 {
        probe.send_failed += 1;
        eprintln!("[probe] write failed (errno={errno})");
    } else {
        probe.sent += 1;
    }
}

/// N11 ⑥ evidence: drain the TUN stand-in hand NON-BLOCKINGLY and log every
/// decapsulated frame that arrived THROUGH the tunnel (proof the peer's
/// probe payload reached our TUN: src/dst + the payload magic). poll-before-
/// read with timeout 0; EAGAIN just means "nothing this round". The hand is
/// a plain socketpair end (blocking), so reads are bounded by the poll gate
/// and the 8-frame cap.
///
/// N13 D2 receipt rule: a frame whose payload starts with the
/// `nbinterop-probe` fingerprint AND whose source is our `--probe-dst` target
/// is one `acked` — we aimed a probe at that peer, and the peer's own probe
/// arrived here, so the tunnel is proven mutual in both directions.
fn recv_probes(o: &RunOpts, tun_hand: i32, probe: &mut ProbeStats) {
    let ack_from = o.probe_dst.as_deref().and_then(hs::parse_ipv4);
    let mut got = 0usize;
    while got < 8 {
        let (ret, _e, rev) = sys::poll1(tun_hand, 0x0001 /* POLLIN */, 0);
        if ret <= 0 || (rev & 0x0001) == 0 {
            return;
        }
        let mut buf = [0u8; 2048];
        let (n, _errno) = sys::read_fd(tun_hand, &mut buf);
        if n <= 0 {
            return;
        }
        got += 1;
        let n = n as usize;
        let frame = &buf[..n];
        if frame.len() >= 28 {
            let src = std::net::Ipv4Addr::new(frame[12], frame[13], frame[14], frame[15]);
            let dst = std::net::Ipv4Addr::new(frame[16], frame[17], frame[18], frame[19]);
            let payload = String::from_utf8_lossy(&frame[28..]);
            if ack_from == Some(src.octets()) && frame[28..].starts_with(b"nbinterop-probe") {
                probe.acked += 1;
            }
            eprintln!("[probe-recv] {src} -> {dst} len={n} payload={payload:?}");
        } else {
            eprintln!("[probe-recv] short frame len={n}");
        }
    }
}

// ---------------------------------------------------------------------------
// status field shortcuts + milestone reporting (stderr)
// ---------------------------------------------------------------------------

fn status_bool(text: &str, path: &str) -> bool {
    hs::json_path_bool(text, path).unwrap_or(false)
}

fn status_num(text: &str, path: &str) -> i64 {
    hs::json_path_num(text, path).unwrap_or(0)
}

fn status_str(text: &str, path: &str) -> Option<String> {
    hs::json_path_str(text, path)
}

#[derive(Default)]
struct Milestones {
    state: Option<String>,
    net_map: bool,
    signal_registered: bool,
    wg_device_up: bool,
    wg_ready: bool,
    ice_connected: i64,
    /// One-shot note for a probe run that stays up past session establish.
    probe_session_note: bool,
}

impl Milestones {
    /// Print state transitions to stderr (the recipe's "expected key
    /// markers"); stdout stays pure status JSONL.
    fn report(&mut self, status: &str) {
        if let Some(state) = status_str(status, "state") {
            if self.state.as_deref() != Some(state.as_str()) {
                eprintln!("[milestone] state -> {state}");
                self.state = Some(state);
            }
        }
        if !self.net_map && status_bool(status, "running") {
            let net = hs::network_config_json();
            if status_bool(&net, "available") {
                let peers = status_num(&net, "peer_count");
                let sig = hs::json_field_str(&net, "signal").unwrap_or_default();
                eprintln!("[milestone] network-map applied (peers={peers}, signal='{sig}')");
                self.net_map = true;
            }
        }
        let registered = status_bool(status, "signal.registered");
        if registered && !self.signal_registered {
            eprintln!("[milestone] signal registered");
            self.signal_registered = true;
        }
        let device_up = status_bool(status, "wg.device_up");
        if device_up && !self.wg_device_up {
            eprintln!("[milestone] wg device up (socketpair TUN stand-in)");
            self.wg_device_up = true;
        }
        let ready = status_bool(status, "wg.ready");
        if ready && !self.wg_ready {
            eprintln!("[milestone] wg tunnel ready (>=1 established session)");
            self.wg_ready = true;
        }
        let connected = status_num(status, "ice.connected");
        if connected > self.ice_connected {
            eprintln!("[milestone] ice connected peers = {connected}");
        }
        self.ice_connected = connected;
    }
}

// ---------------------------------------------------------------------------
// I6 tests — the `n2h-peer` echo path and status line, offline (design §7-I6)
// ---------------------------------------------------------------------------

/// Run with `cargo test --offline --locked --features cli --bin nbinterop`
/// (the bin is `required-features = ["cli"]`, so the default `cargo test` does
/// not build it). Nothing here needs a connector, a config or the network: the
/// 对拍 runs over ONE AF_UNIX socketpair.
#[cfg(test)]
mod n2h_peer_tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// The design §7-I6 acceptance: the echo path and the peer status line,
    /// end to end, with a fake device on the far end of the tunnel stand-in.
    #[test]
    fn the_loopback_check_verifies_the_echo_path_and_the_status_line() {
        let opts = N2hPeerOpts { posctl_interval_ms: 100, ..Default::default() };
        let (code, report) = run_n2h_peer_loopback(&opts, 5_000).expect("loopback run");
        assert!(report.ok(), "{report:?}");
        assert_eq!(code, hs::EXIT_OK, "{report:?}");
        assert!(report.b2a_observed, "the peer's active probe reached the device side");
        assert!(report.a2b_echo_observed, "the device's probe came back");
        assert!(report.echo_verbatim && report.echo_tuple_swapped, "{report:?}");
        assert!(!report.non_posctl_echo, "the peer must not be a reflector");
        println!("[I6-cli] loopback 对拍: {}", report.to_json());
    }

    /// …and the same run through the CLI entry (`--loopback` is the opt-in, the
    /// exit code is the verdict).
    #[test]
    fn the_cli_loopback_entry_exits_zero_on_a_healthy_run() {
        let code = cmd_n2h_peer(&args(&[
            "--loopback",
            "--posctl-interval",
            "100",
            "--timeout",
            "4",
        ]));
        assert_eq!(code, hs::EXIT_OK);
    }

    /// One non-posctl frame in ⇒ nothing out (the endpoint is not a reflector);
    /// one well-formed posctl probe in ⇒ the SAME bytes out with the five-tuple
    /// swapped.
    #[test]
    fn a_posctl_probe_is_echoed_verbatim_and_anything_else_is_ignored() {
        let (hand, far) = hs::open_tun_standby_pair().expect("TUN stand-in pair");
        let mut bag = hs::FdBag::new();
        bag.keep(hand);
        bag.keep(far);
        let mut state = N2hPeerState::new(N2hPeerOpts::default()).expect("state");
        let own = [127, 0, 0, 1];
        let mut buf = [0u8; N2H_PEER_FRAME_MAX];

        // ① a foreign payload (the CLI's own probe fingerprint) is NOT echoed
        let junk = hs::build_ipv4_udp_packet(own, [127, 0, 0, 9], 41_000, 40_001, b"nbinterop-probe");
        assert_eq!(sys::write_fd(far, &junk).0 as usize, junk.len());
        for _ in 0..5 {
            n2h_peer_drain_and_echo(&mut state, Some(own), hand);
            sys::sleep_ms(10);
        }
        assert_eq!((state.a2b_seen, state.echoes_sent), (0, 0));
        assert_eq!(read_frame_within(far, &mut buf, 50), None, "nothing may come back");

        // ② a well-formed posctl probe (any addressed port, the marker is the
        //    receipt) comes back byte-identical with the tuple swapped
        let marker = netbird_core::n2h::probe_id(nc::POSCTL_KIND_TOKEN, &"cd".repeat(12));
        let payload = nc::posctl_payload(&marker);
        let probe =
            hs::build_ipv4_udp_packet([127, 0, 0, 7], own, 41_000, 40_001, payload.as_bytes());
        assert_eq!(sys::write_fd(far, &probe).0 as usize, probe.len());
        let mut echoed: Option<Vec<u8>> = None;
        for _ in 0..100 {
            n2h_peer_drain_and_echo(&mut state, Some(own), hand);
            if let Some(n) = read_frame_within(far, &mut buf, 10) {
                echoed = Some(buf[..n].to_vec());
                break;
            }
            sys::sleep_ms(10);
        }
        let echoed = echoed.expect("the echo came back");
        assert_eq!(&echoed[28..], payload.as_bytes(), "payload verbatim");
        let view = netbird_core::n2h::parse_ipv4_frame(&echoed).expect("echo parses");
        assert_eq!(
            (view.src, view.sport, view.dst, view.dport),
            (own, 40_001, [127, 0, 0, 7], 41_000),
            "five-tuple swapped"
        );
        assert_eq!((state.a2b_seen, state.echoes_sent), (1, 1));
        println!("[I6-cli] verbatim echo: {} bytes, tuple {view:?}", echoed.len());
    }

    /// The status line is the device-side channel: the announced marker, the
    /// destinations, and `unknown_peer_drops` reported as `na` when no
    /// connector/device exists (never a fabricated zero), with the absolute
    /// value plus the delta once it is readable.
    #[test]
    fn the_status_line_announces_the_marker_and_reports_the_drops_gap_honestly() {
        let opts = N2hPeerOpts {
            overlay: Some(std::net::Ipv4Addr::new(100, 64, 0, 7)),
            overlay_port: Some(40_001),
            ..Default::default()
        };
        let mut state = N2hPeerState::new(opts).expect("state");
        assert!(state.marker.starts_with("N2H-posctl-"), "{}", state.marker);
        assert_eq!(state.marker.len(), "N2H-posctl-".len() + 24);
        let marker_payload = nc::posctl_payload(&state.marker);
        assert_eq!(nc::posctl_marker(marker_payload.as_bytes()), Some(state.marker.as_str()));

        let announce = state.announce_line();
        assert!(announce.starts_with("nbinterop: n2h-peer|"), "{announce}");
        assert!(announce.contains(&format!("b2a_probe_id={}", state.marker)), "{announce}");
        assert!(announce.contains("b2a_dst=100.64.0.7:40001"), "{announce}");
        assert!(announce.contains("echo_port=40001"), "{announce}");
        assert!(announce.contains("loopback=false"), "{announce}");

        // no connector in this process ⇒ `na`, never a zero
        let none = state.sample_drops(None);
        assert_eq!(none, (None, None));
        let line = state.status_line(Some([100, 64, 0, 8]), none);
        assert!(line.starts_with("nbinterop: n2h-peer-stats|"), "{line}");
        assert!(line.contains(&format!("b2a_probe_id={}", state.marker)), "{line}");
        assert!(line.contains("own_overlay=100.64.0.8:40001"), "{line}");
        assert!(line.contains("unknown_peer_drops=na|unknown_peer_drops_delta=na"), "{line}");
        assert!(line.contains("last_a2b_from=na"), "{line}");

        // a readable counter: absolute value + delta since the first sample
        assert_eq!(state.sample_drops(Some(7)), (Some(7), Some(0)));
        let sample = state.sample_drops(Some(9));
        assert_eq!(sample, (Some(9), Some(2)));
        let line = state.status_line(Some([100, 64, 0, 8]), sample);
        assert!(line.contains("unknown_peer_drops=9|unknown_peer_drops_delta=2"), "{line}");
        println!("[I6-cli] {announce}");
        println!("[I6-cli] {line}");
    }

    /// The flags are parsed fail-closed, and the mode is never entered by
    /// accident (design §6's "default off, explicit opt-in").
    #[test]
    fn n2h_peer_flags_are_parsed_fail_closed() {
        let (o, _) = parse_flags(&args(&["--overlay", "100.64.0.7"])).expect("parses");
        let peer = o.n2h_peer.clone().expect("n2h-peer opts");
        assert_eq!(peer.overlay_dst(), Some(([100, 64, 0, 7], 40_001)));
        assert!(!peer.loopback);

        let (o, _) = parse_flags(&args(&[
            "--overlay=100.64.0.7:40002",
            "--echo-port",
            "40003",
            "--posctl-interval",
            "250",
            "--loopback",
        ]))
        .expect("parses");
        let peer = o.n2h_peer.clone().expect("n2h-peer opts");
        assert_eq!(peer.overlay_dst(), Some(([100, 64, 0, 7], 40_002)));
        assert_eq!(peer.echo_port, 40_003);
        assert_eq!(peer.posctl_interval_ms, 250);
        assert!(peer.loopback);

        for bad in ["::1", "peer.example", "100.64.0.7:0", "100.64.0.7:http", "300.1.1.1", "100.64.0.7:"] {
            assert!(
                parse_flags(&args(&["--overlay", bad])).is_err(),
                "--overlay {bad} must be refused"
            );
        }
        for bad in [
            ["--echo-port", "0"],
            ["--echo-port", "70000"],
            ["--posctl-interval", "0"],
            ["--posctl-interval", "soon"],
        ] {
            assert!(parse_flags(&args(&bad)).is_err(), "{bad:?} must be refused");
        }

        // no n2h-peer flag at all ⇒ the mode is not entered
        let (o, _) = parse_flags(&args(&["--config", "x.conf"])).expect("parses");
        assert!(o.n2h_peer.is_none());
    }

    /// The mode's own gates: a live run needs `--overlay` (checked before any
    /// config or socket work), `--loopback` starts no connector, and the
    /// n2h-peer flags are refused by every other subcommand.
    #[test]
    fn n2h_peer_requires_its_own_flags_and_never_borrows_another_mode() {
        assert_eq!(
            cmd_engine("n2h-peer", &args(&["--config", "does-not-exist.conf"])),
            hs::EXIT_USAGE
        );
        assert_eq!(
            cmd_n2h_peer(&args(&["--loopback", "--config", "does-not-exist.conf"])),
            hs::EXIT_USAGE
        );
        assert_eq!(
            cmd_engine("peer", &args(&["--config", "x.conf", "--overlay", "100.64.0.7"])),
            hs::EXIT_USAGE
        );
        assert_eq!(cmd_engine("connect", &args(&["--echo-port", "40001"])), hs::EXIT_USAGE);
        println!("[I6-cli] mode gates: n2h-peer flags are opt-in and refused elsewhere");
    }
}
