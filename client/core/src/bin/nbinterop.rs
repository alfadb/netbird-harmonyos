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
//! error class · `20` n2h-inconclusive.
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
    nbinterop isolation-check [--out <file>] [--loopback]
                              [--fault-leak] [--fault-no-posctl]
                              [--fault-omit-endpoint <kind>]
                              [--config <file>] [--outer-endpoint <kind:host:port>]...

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
| 20 n2h-inconclusive";

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
        }
    }
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
// connect / peer engine
// ---------------------------------------------------------------------------

fn cmd_engine(mode: &'static str, args: &[String]) -> i32 {
    let (o, _) = match parse_flags(args) {
        Ok(v) => v,
        Err(code) => return code,
    };
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

    // prime the queues (mirrors the shell feeding its first sockets)
    top_up_management(&mut bag, &mut mgmt_queued);
    top_up_ice(&mut bag, &mut ice_queued);

    let start = Instant::now();
    let mut last_print: u128 = 0;
    let mut last_maint: u128 = 0;
    let mut last_probe: u128 = 0;
    let mut printed = false;

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
            );
        }

        if o.probe_interval_ms > 0
            && now.saturating_sub(last_probe) >= o.probe_interval_ms as u128
        {
            last_probe = now;
            send_probe(o, own_addr, tun_hand);
        }
        // N11 ⑥ evidence: frames that arrived THROUGH the tunnel land on the
        // TUN stand-in hand — drain + log them (payload-level proof).
        if o.probe_interval_ms > 0 {
            recv_probes(tun_hand);
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
            eprintln!(
                "[nbinterop] --timeout {}s elapsed before the goal; stopping",
                o.timeout_s
            );
            hs::stop_connector();
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
fn top_up_ice(bag: &mut hs::FdBag, queued: &mut i64) {
    if *queued >= 4 {
        return;
    }
    for _ in 0..2 {
        match hs::open_udp_unbound() {
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
) {
    top_up_management(bag, mgmt_queued);
    top_up_ice(bag, ice_queued);

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

/// Inject one probe packet through the TUN stand-in hand (the pump reads it
/// as if it came from the kernel TUN and ships it through the WG session).
fn send_probe(o: &RunOpts, own_addr: Option<[u8; 4]>, tun_hand: i32) {
    let Some(dst) = o.probe_dst.as_deref().and_then(hs::parse_ipv4) else {
        return;
    };
    let Some(src) = own_addr else {
        return; // own tunnel address not synced yet
    };
    let pkt = hs::build_ipv4_udp_packet(src, dst, 40000, 40001, b"nbinterop-probe");
    let (n, errno) = sys::write_fd(tun_hand, &pkt);
    if n <= 0 {
        eprintln!("[probe] write failed (errno={errno})");
    }
}

/// N11 ⑥ evidence: drain the TUN stand-in hand NON-BLOCKINGLY and log every
/// decapsulated frame that arrived THROUGH the tunnel (proof the peer's
/// probe payload reached our TUN: src/dst + the payload magic). poll-before-
/// read with timeout 0; EAGAIN just means "nothing this round". The hand is
/// a plain socketpair end (blocking), so reads are bounded by the poll gate
/// and the 8-frame cap.
fn recv_probes(tun_hand: i32) {
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
