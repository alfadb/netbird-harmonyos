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
//! `0` success · `1` selftest failure · `2` usage · `3` config · `4`
//! credentials · `10..16` connector error classes (network/timeout/auth/
//! request/server/parse/unsupported_url) · `17` `--timeout` elapsed ·
//! `18` peer ended without a session · `19` unknown error class.
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
    nbinterop selftest
    nbinterop connect --config <file> [--dry-run] [--interval <ms>] [--timeout <s>]
                      [--probe-dst <ip>] [--probe-interval <ms>] [--exit-on-terminal] [--verbose]
    nbinterop peer    --config <file> [same flags]

Credentials (management URL / private key / setup key / CA) come ONLY from
the config file or the environment — never the command line.

Exit codes: 0 ok | 1 selftest failed | 2 usage | 3 config | 4 credentials
| 10 network | 11 timeout | 12 auth | 13 request | 14 server | 15 parse
| 16 unsupported_url | 17 wait-timeout | 18 peer-no-session | 19 unknown-class";

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
    let (_, _) = match parse_flags(args) {
        Ok(v) => v,
        Err(code) => return code,
    };
    set_hilog_forward(false);
    let outcomes = hs::run_selftest();
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

    let loaded = match hs::load_cli_config(&o.config) {
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
    let wg_fd = match hs::open_udp_ephemeral() {
        Ok(fd) => fd,
        Err(e) => {
            eprintln!("error: wg socket: {e}");
            hs::stop_connector();
            return hs::exit_code_for_class("network");
        }
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
    eprintln!("[nbinterop] wg outer socket (ephemeral port) + tun stand-in fed");

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

        if !printed || now.saturating_sub(last_print) >= o.interval_ms as u128 {
            printed = true;
            last_print = now;
            let status = hs::status_json();
            println!("{status}");
            seen.report(&status);

            if mode == "peer" && status_num(&status, "wg.peers_with_session") >= 1 {
                eprintln!("[nbinterop] peer goal reached: wg session established — exiting 0");
                hs::stop_connector();
                return hs::EXIT_OK;
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
