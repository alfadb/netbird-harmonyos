// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # host_sockets — HOST-ONLY interop harness (N9) — 主机联调专用，非设备路径
//!
//! Everything in this module exists for ONE purpose: running the real
//! protocol stack (`connector` → `grpc`/`sync`/`signal`/`ice`/`peer_conn` →
//! `wg_device`) as a host process against a real self-hosted NetBird
//! management/signal server, WITHOUT a HarmonyOS device. See
//! `docs/self-hosted-interop-plan.md` for the operating recipe and
//! `src/bin/nbinterop.rs` for the CLI that drives it.
//!
//! ## Isolation: why this cannot weaken the device path
//!
//! On the device, EVERY socket is created by the ArkTS shell and
//! `VpnConnection.protect(fd)`-ed BEFORE any packet flows; the native side
//! only receives fd NUMBERS through the fail-closed seams
//! (`connector_start_with_socket`, `connector_socket_feed`,
//! `connector_ice_socket_feed`, `connector_signal_socket_feed`,
//! `connector_wg_socket_feed`, `connector_tun_fd_feed`) and consumes each
//! number dup-only. On the host there is no VpnExtension and nothing to
//! protect — so this module plays the SHELL's role: it creates plain TCP/UDP
//! sockets itself and hands them to the SAME seams, which keep validating
//! and dup-consuming every fd exactly as before.
//!
//! The isolation is therefore structural, not behavioral:
//!
//! 1. The library's default paths are untouched. `connector_start` (the
//!    unprotedted-dial entry) STILL refuses with
//!    `management-socket-required` unless the dangerous
//!    `allow_unprotected_management` opt-in is set — asserted by
//!    `tests/host_interop_n9.rs` with this module linked in. The device
//!    requirement "no shell feed ⇒ no start / no dial / no candidates / no
//!    data plane" is unchanged; this module only ever FEEDS the same queues.
//! 2. The device path never calls into this module: no production code
//!    references it; the shipped cdylib simply carries dead code unless the
//!    host CLI (`nbinterop`) is linked.
//! 3. The fd CONTRACT is preserved: sockets created here are fed by NUMBER,
//!    validated by the seams' dup probe, consumed dup-only, and the fed
//!    originals stay open until [`FdBag`]'s Drop at process exit (the
//!    host-side analog of "provider owns the fd").
//!
//! Known host-side divergences from the device shell (both documented in the
//! recipe doc): there is no `protect()` (no VPN service on the host — the
//! interop value is exercising protocol behavior, not kernel routing), and
//! the WG outer socket binds an EPHEMERAL port instead of `wg_fwd_open`'s
//! fixed `NET_PORT` so two CLI instances can interoperate on ONE machine.
//!
//! ## CLI engine support
//!
//! The rest of the module is the shared engine of `nbinterop`: config-file
//! loading with credential discipline (file/env ONLY — argv secrets are
//! refused at the CLI layer), the `--dry-run` plan builder, connector
//! response parsing, exit-code classification, and the offline `selftest`
//! checks. Pure logic is unit-tested here; process-global behavior is
//! covered by `tests/host_interop_n9.rs`.

use std::net::SocketAddr;
use std::time::Duration;

use crate::config::{parse_document, Json};
use crate::sys;
use crate::util::json_escape;

// ---------------------------------------------------------------------------
// exit codes (CLI contract; documented in nbinterop --help and the recipe)
// ---------------------------------------------------------------------------

/// All checks passed / run completed cleanly.
pub const EXIT_OK: i32 = 0;
/// `selftest` reported at least one failed check.
pub const EXIT_SELFTEST_FAILED: i32 = 1;
/// Bad command line (unknown flag, missing value, secret on argv).
pub const EXIT_USAGE: i32 = 2;
/// Config file unreadable or invalid (map for `ConfigError`).
pub const EXIT_CONFIG: i32 = 3;
/// No usable credentials (no setup key / JWT from file or env).
pub const EXIT_CREDENTIALS: i32 = 4;
/// `--timeout` elapsed before the run reached its goal.
pub const EXIT_WAIT_TIMEOUT: i32 = 17;
/// `peer` mode ended without an established WG session.
pub const EXIT_PEER_NOT_ESTABLISHED: i32 = 18;
/// Terminal connector error with an unrecognized error class.
pub const EXIT_UNKNOWN_CLASS: i32 = 19;

/// Map an existing [`crate::connector::ErrorClass`] token to its exit code
/// (10..16 in class order; 0 reserved for success).
pub fn exit_code_for_class(class: &str) -> i32 {
    match class {
        "network" => 10,
        "timeout" => 11,
        "auth" => 12,
        "request" => 13,
        "server" => 14,
        "parse" => 15,
        "unsupported_url" => 16,
        _ => EXIT_UNKNOWN_CLASS,
    }
}

// ---------------------------------------------------------------------------
// host socket creation (the shell's role, host-side)
// ---------------------------------------------------------------------------

/// Socket-creation failures carry a stable token + errno (mirrors the seam
/// error style; never secret material).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSocketError {
    pub token: &'static str,
    pub errno: i32,
}

impl core::fmt::Display for HostSocketError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} (errno={})", self.token, self.errno)
    }
}

impl std::error::Error for HostSocketError {}

/// One pre-bound (0.0.0.0:0, UNCONNECTED) TCP socket — the exact
/// [`crate::mgmtsock::mgmt_socket_open`] helper the device shell uses before
/// `protect()`-ing the fd. Host parity: same helper, minus the protect step.
pub fn open_tcp_prebound() -> Result<i32, HostSocketError> {
    let out = crate::mgmtsock::mgmt_socket_open();
    let doc = parse_document(&out)
        .map_err(|_| HostSocketError { token: "bad-mgmtsock-response", errno: 0 })?;
    let fd = doc_num(&doc, "fd").unwrap_or(-1);
    if fd < 0 {
        return Err(HostSocketError {
            token: "tcp-open-failed",
            errno: doc_num(&doc, "bind_errno").unwrap_or(0) as i32,
        });
    }
    Ok(fd as i32)
}

/// One fresh, UNBOUND AF_INET/SOCK_DGRAM socket — the ICE provider contract
/// ("ONE call = ONE fresh, UNBOUND socket", `crate::ice`): the consumer dups
/// and binds it to the interface address itself.
pub fn open_udp_unbound() -> Result<i32, HostSocketError> {
    let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM | sys::SOCK_CLOEXEC, 0) };
    if fd < 0 {
        return Err(HostSocketError { token: "udp-open-failed", errno: sys::errno() });
    }
    Ok(fd)
}

/// One AF_INET/SOCK_DGRAM socket bound to 0.0.0.0:0 (ephemeral port) — the
/// WG outer socket. Device divergence (documented in the module docs): the
/// device helper `wg_fwd_open` binds the FIXED `NET_PORT`, which two
/// same-host instances cannot share; the host harness binds ephemeral.
pub fn open_udp_ephemeral() -> Result<i32, HostSocketError> {
    let fd = open_udp_unbound()?;
    let sa = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let rc = unsafe { sys::bind(fd, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) };
    if rc != 0 {
        let errno = sys::errno();
        unsafe { sys::close(fd) };
        return Err(HostSocketError { token: "udp-bind-failed", errno });
    }
    Ok(fd)
}

/// The bound port of a raw UDP fd (`getsockname`) — used by the selftest to
/// point two in-process WG devices at each other.
pub fn udp_bound_port(fd: i32) -> Result<u16, HostSocketError> {
    let mut name = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut len = core::mem::size_of::<sys::sockaddr_in>() as u32;
    if unsafe { sys::getsockname(fd, &mut name, &mut len) } != 0 {
        return Err(HostSocketError { token: "getsockname-failed", errno: sys::errno() });
    }
    Ok(u16::from_be(name.sin_port))
}

/// The TUN stand-in for host runs: a SOCK_DGRAM Unix socketpair. End 0 is
/// FED to the connector (`connector_tun_fd_feed`) — `TunFd::dup_from_raw`
/// accepts any dupable fd and the data-plane pump does raw read/write, which
/// a datagram socketpair satisfies; end 1 stays with the CLI as its "hand"
/// for injecting probe frames and observing delivered ones. NO kernel TUN
/// device is created.
pub fn open_tun_standby_pair() -> Result<(i32, i32), HostSocketError> {
    use std::os::fd::IntoRawFd;
    match std::os::unix::net::UnixStream::pair() {
        Ok((a, b)) => {
            let fa = a.into_raw_fd();
            let fb = b.into_raw_fd();
            Ok((fa, fb))
        }
        Err(e) => Err(HostSocketError {
            token: "socketpair-failed",
            errno: e.raw_os_error().unwrap_or(0),
        }),
    }
}

/// Provider-side fd ownership for one CLI run: every socket this module
/// opens and feeds is remembered here and closed exactly once on Drop (the
/// host-side analog of "the provider owns the fd"; the seams dup-consume
/// their own copies). This bounds the host-side fd budget to what was
/// actually fed.
#[derive(Debug, Default)]
pub struct FdBag {
    fds: Vec<i32>,
}

impl FdBag {
    pub fn new() -> Self {
        FdBag::default()
    }
    /// Remember a fed fd (number kept open for the run; closed on Drop).
    pub fn keep(&mut self, fd: i32) {
        self.fds.push(fd);
    }
    /// How many fds are held (observability only).
    pub fn len(&self) -> usize {
        self.fds.len()
    }
    pub fn is_empty(&self) -> bool {
        self.fds.is_empty()
    }
}

impl Drop for FdBag {
    fn drop(&mut self) {
        for fd in self.fds.drain(..) {
            unsafe { sys::close(fd) };
        }
    }
}

/// Shell-side DNS resolution, host flavor: resolve `host:port` to the first
/// IPv4 address via the OS resolver (the device does this step shell-side
/// before feeding `connect_addr`; IPv4-only matches the config domain).
pub fn resolve_ipv4_host(host: &str, port: u16) -> Result<SocketAddr, HostSocketError> {
    use std::net::ToSocketAddrs;
    match (host, port).to_socket_addrs() {
        Ok(mut it) => match it.find(|a: &SocketAddr| a.is_ipv4()) {
            Some(a) => Ok(a),
            None => Err(HostSocketError { token: "resolve-no-ipv4", errno: 0 }),
        },
        Err(_) => Err(HostSocketError { token: "resolve-failed", errno: 0 }),
    }
}

// ---------------------------------------------------------------------------
// minimal JSON response helpers (crate-internal strict reader)
// ---------------------------------------------------------------------------

fn json_doc(text: &str) -> Option<Json> {
    parse_document(text).ok()
}

fn doc_str(doc: &Json, key: &str) -> Option<String> {
    if let Json::Obj(entries) = doc {
        for (k, v) in entries {
            if k == key {
                if let Json::Str(s) = v {
                    return Some(s.clone());
                }
            }
        }
    }
    None
}

fn doc_num(doc: &Json, key: &str) -> Option<i64> {
    if let Json::Obj(entries) = doc {
        for (k, v) in entries {
            if k == key {
                if let Json::Num(n) = v {
                    return Some(*n as i64);
                }
            }
        }
    }
    None
}

fn doc_bool(doc: &Json, key: &str) -> Option<bool> {
    if let Json::Obj(entries) = doc {
        for (k, v) in entries {
            if k == key {
                if let Json::Bool(b) = v {
                    return Some(*b);
                }
            }
        }
    }
    None
}

/// Dotted-path lookup into nested objects (`"wg.peers_with_session"`,
/// `"ice.connected"`, `"signal.registered"`), arrays indexed numerically.
fn json_lookup<'a>(doc: &'a Json, path: &str) -> Option<&'a Json> {
    let mut cur = doc;
    for part in path.split('.') {
        let next = match cur {
            Json::Obj(entries) => entries.iter().find(|(k, _)| k == part).map(|(_, v)| v),
            Json::Arr(items) => part.parse::<usize>().ok().and_then(|i| items.get(i)),
            _ => None,
        }?;
        cur = next;
    }
    Some(cur)
}

/// `&str`-based field accessors for the CLI layer (`Json` is crate-private):
/// top-level string field of a JSON object document.
pub fn json_field_str(text: &str, key: &str) -> Option<String> {
    doc_str(&json_doc(text)?, key)
}

/// Numeric field at a dotted path (`"wg.tx_packets"`, `"ice.connected.0"`).
pub fn json_path_num(text: &str, path: &str) -> Option<i64> {
    match json_lookup(&json_doc(text)?, path)? {
        Json::Num(n) => Some(*n as i64),
        _ => None,
    }
}

/// Boolean field at a dotted path (`"wg.device_up"`, `"signal.registered"`).
pub fn json_path_bool(text: &str, path: &str) -> Option<bool> {
    match json_lookup(&json_doc(text)?, path)? {
        Json::Bool(b) => Some(*b),
        _ => None,
    }
}

/// String field at a dotted path (`"state"`, `"last_error.class"`).
pub fn json_path_str(text: &str, path: &str) -> Option<String> {
    match json_lookup(&json_doc(text)?, path)? {
        Json::Str(s) => Some(s.clone()),
        _ => None,
    }
}

/// `connector_start_with_socket` answer: `{"started":true,"state":..}` →
/// `Ok(state)`; `{"started":false,"error":token}` → `Err(token)`.
pub fn parse_start_response(text: &str) -> Result<String, String> {
    let doc = parse_document(text).map_err(|_| "unparseable-response".to_string())?;
    if doc_bool(&doc, "started").unwrap_or(false) {
        Ok(doc_str(&doc, "state").unwrap_or_else(|| "unknown".into()))
    } else {
        Err(doc_str(&doc, "error").unwrap_or_else(|| "unknown".into()))
    }
}

/// `connector_*_feed` answer: `{"ok":true,"queued":N}` → `Ok(queued)`;
/// `{"ok":false,"error":token}` → `Err(token)`.
pub fn parse_feed_response(text: &str) -> Result<i64, String> {
    let doc = parse_document(text).map_err(|_| "unparseable-response".to_string())?;
    if doc_bool(&doc, "ok").unwrap_or(false) {
        Ok(doc_num(&doc, "queued").unwrap_or(-1))
    } else {
        Err(doc_str(&doc, "error").unwrap_or_else(|| "unknown".into()))
    }
}

// ---------------------------------------------------------------------------
// connector drive (same seams the NAPI surface exposes, host side)
// ---------------------------------------------------------------------------

/// Production start over a host-created socket: the CLI's analog of
/// `connector_start_with_socket(fd, configJson, credentialsJson, addrJson)`.
/// The seam validates the fd (dup probe) and refuses fail-closed; TLS still
/// verifies the management URL host.
pub fn start_connector_over_host_socket(
    fd: i32,
    config_json: &str,
    secrets_json: &str,
    addr: SocketAddr,
) -> Result<String, String> {
    let addr_json = format!("{{\"connect_addr\":\"{}\"}}", addr);
    parse_start_response(&crate::connector::connector_start_with_socket_json(
        fd,
        config_json,
        secrets_json,
        &addr_json,
    ))
}

/// Resupply one management TCP socket (reconnect dials take a fresh one).
/// Returns the source queue depth after the feed (watermark input).
pub fn feed_management(fd: i32) -> Result<i64, String> {
    parse_feed_response(&crate::connector::connector_socket_feed_json(fd))
}

/// Resupply one UNBOUND UDP socket for ICE (gather rounds + check sockets).
pub fn feed_ice(fd: i32) -> Result<i64, String> {
    parse_feed_response(&crate::connector::connector_ice_socket_feed_json(fd))
}

/// Resupply one signal TCP socket together with the resolved signal address
/// (the FIRST resolved address wins for the link's lifetime).
pub fn feed_signal(fd: i32, addr: SocketAddr) -> Result<i64, String> {
    let addr_json = format!("{{\"connect_addr\":\"{}\"}}", addr);
    parse_feed_response(&crate::connector::connector_signal_socket_feed_json(fd, &addr_json))
}

/// One-time WG outer UDP socket feed (idempotent for the same fd number
/// while the device is up; a different number is refused — unchanged N8
/// semantics).
pub fn feed_wg_socket(fd: i32) -> Result<(), String> {
    parse_feed_response(&crate::connector::connector_wg_socket_feed_json(fd)).map(|_| ())
}

/// One-time TUN stand-in feed (socketpair end; NO kernel TUN). While the
/// device is up this REPLACES the platform TUN — unchanged N8 semantics.
pub fn feed_tun(fd: i32) -> Result<(), String> {
    parse_feed_response(&crate::connector::connector_tun_fd_feed_json(fd)).map(|_| ())
}

/// Status snapshot (`connector_status()` structure, verbatim).
pub fn status_json() -> String {
    crate::connector::connector_status_json()
}

/// Shell network-config snapshot (contains the sync-delivered signal URI).
pub fn network_config_json() -> String {
    crate::connector::connector_network_config_json()
}

/// Idempotent stop (Sync close → best-effort logout → seam teardown).
pub fn stop_connector() {
    let _ = crate::connector::connector_stop_json();
}

/// Extract the sync-delivered signal URI from the network-config snapshot
/// (`None` before the first NetworkMap).
pub fn signal_uri_from_network_config(text: &str) -> Option<String> {
    let doc = parse_document(text).ok()?;
    if !doc_bool(&doc, "available")? {
        return None;
    }
    doc_str(&doc, "signal").filter(|s| !s.is_empty())
}

/// Own tunnel address from the network-config snapshot (`"100.64.0.5"`),
/// used as the probe packet's source.
pub fn own_address_from_network_config(text: &str) -> Option<[u8; 4]> {
    let doc = parse_document(text).ok()?;
    if !doc_bool(&doc, "available")? {
        return None;
    }
    let s = doc_str(&doc, "address")?;
    parse_ipv4(&s)
}

/// Parse a strict dotted-quad IPv4 literal.
pub fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut parts = s.trim().split('.');
    for octet in out.iter_mut() {
        let p = parts.next()?;
        if p.is_empty() || p.len() > 3 || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        *octet = p.parse::<u8>().ok()?;
    }
    if parts.next().is_some() {
        return None;
    }
    Some(out)
}

/// Parse the sync-delivered signal URI into `(host, port)`. Accepted shapes:
/// `host:port` (what `NetbirdConfig.signal.uri` carries) and the tolerated
/// `rel://host:port` / trailing-`/` spellings. Bracketed IPv6 is explicitly
/// unsupported (same IPv4-only boundary as `crate::config`).
pub fn parse_signal_uri(uri: &str) -> Result<(String, u16), String> {
    let s = uri.trim();
    let s = s.strip_prefix("rel://").unwrap_or(s);
    let s = s.trim_end_matches('/');
    let (host, port) = s.rsplit_once(':').ok_or_else(|| {
        format!("signal URI '{uri}' lacks a :port (expected host:port)")
    })?;
    if host.contains('[') || host.contains(']') {
        return Err(format!("signal URI '{uri}': IPv6 literals are not supported"));
    }
    if host.is_empty() {
        return Err(format!("signal URI '{uri}': empty host"));
    }
    let port: u16 = port.parse().map_err(|_| {
        format!("signal URI '{uri}': bad port '{port}'")
    })?;
    Ok((host.to_string(), port))
}

/// Parse the management URL into `(host, port)` for host-side resolution.
/// `https://` defaults to 443, `http://` to 80 when the URL omits the port.
pub fn parse_management_endpoint(url: &str) -> Result<(bool, String, u16), String> {
    let (tls, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return Err(format!(
            "management_url '{url}': expected https:// or http:// scheme"
        ));
    };
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    let authority = authority.rsplit_once('@').map(|(_, a)| a).unwrap_or(authority);
    if authority.contains('[') {
        return Err(format!("management_url '{url}': IPv6 literals are not supported"));
    }
    match authority.rsplit_once(':') {
        Some((host, port)) => {
            if host.is_empty() {
                return Err(format!("management_url '{url}': empty host"));
            }
            let port: u16 = port
                .parse()
                .map_err(|_| format!("management_url '{url}': bad port '{port}'"))?;
            Ok((tls, host.to_string(), port))
        }
        None => {
            if authority.is_empty() {
                return Err(format!("management_url '{url}': empty host"));
            }
            Ok((tls, authority.to_string(), if tls { 443 } else { 80 }))
        }
    }
}

// ---------------------------------------------------------------------------
// probe traffic (双向测试包) over the TUN stand-in
// ---------------------------------------------------------------------------

/// One honest IPv4/UDP packet (valid header checksum, UDP checksum 0 — legal
/// for IPv4 and not validated inside WireGuard). This is the payload the CLI
/// writes into its TUN stand-in hand; the data-plane pump reads it as if it
/// came from the kernel TUN and ships it through the real WG session.
pub fn build_ipv4_udp_packet(
    src: [u8; 4],
    dst: [u8; 4],
    sport: u16,
    dport: u16,
    payload: &[u8],
) -> Vec<u8> {
    let mut p = Vec::with_capacity(28 + payload.len());
    let total = (20 + 8 + payload.len()) as u16;
    p.extend_from_slice(&[0x45, 0]);
    p.extend_from_slice(&total.to_be_bytes());
    p.extend_from_slice(&[0, 1, 0, 0, 64, 17, 0, 0]); // id, flags, ttl 64, proto UDP, cksum 0
    p.extend_from_slice(&src);
    p.extend_from_slice(&dst);
    p.extend_from_slice(&sport.to_be_bytes());
    p.extend_from_slice(&dport.to_be_bytes());
    p.extend_from_slice(&((8 + payload.len()) as u16).to_be_bytes());
    p.extend_from_slice(&[0, 0]); // udp cksum 0
    p.extend_from_slice(payload);
    let mut sum: u32 = 0;
    for w in p[..20].chunks(2) {
        sum += u16::from_be_bytes([w[0], w[1]]) as u32;
    }
    let ck = !(sum + (sum >> 16)) as u16;
    p[10..12].copy_from_slice(&ck.to_be_bytes());
    p
}

// ---------------------------------------------------------------------------
// CLI configuration (credential discipline: file / env ONLY)
// ---------------------------------------------------------------------------

/// CLI-layer failure with its exit code.
#[derive(Debug, Clone)]
pub struct CliError {
    pub exit_code: i32,
    pub message: String,
}

impl CliError {
    fn new(exit_code: i32, message: impl Into<String>) -> Self {
        CliError { exit_code, message: message.into() }
    }
}

impl core::fmt::Display for CliError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

/// Human-readable I/O failure with explicit remediation (the task's
/// "配置文件权限不足时给出明确提示").
pub fn cli_io_message(what: &str, path: &str, e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => format!(
            "{what} not readable: {path} — permission denied; \
             fix with `chmod 600 {path}` (or run as a user that can read it)"
        ),
        std::io::ErrorKind::NotFound => format!("{what} not found: {path}"),
        _ => format!("{what} unreadable: {path} — {e}"),
    }
}

/// Credential-bearing environment variables (ALL optional). Secrets never
/// travel on the command line; the file is the other accepted source.
pub const ENV_SETUP_KEY: &str = "NETBIRD_SETUP_KEY";
pub const ENV_JWT: &str = "NETBIRD_JWT";
pub const ENV_MANAGEMENT_URL: &str = "NETBIRD_MANAGEMENT_URL";
pub const ENV_CA_PEM: &str = "NETBIRD_CA_PEM";

/// Snapshot of the optional overrides, read from the process environment.
#[derive(Debug, Default, Clone)]
pub struct EnvSecrets {
    pub setup_key: Option<String>,
    pub jwt: Option<String>,
    pub management_url: Option<String>,
    pub ca_pem: Option<String>,
}

impl EnvSecrets {
    pub fn from_process() -> Self {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        EnvSecrets {
            setup_key: get(ENV_SETUP_KEY),
            jwt: get(ENV_JWT),
            management_url: get(ENV_MANAGEMENT_URL),
            ca_pem: get(ENV_CA_PEM),
        }
    }
}

/// Redacted config facts safe for plan output and logs (public key + URL are
/// not secrets; setup key / private key / CA are reduced to length only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSummary {
    pub management_url: String,
    pub tls: bool,
    pub hostname: String,
    pub public_key_b64: String,
    pub setup_key_source: &'static str,
    pub setup_key_len: usize,
    pub jwt_source: &'static str,
    pub jwt_len: usize,
}

/// Everything `connect`/`peer` need to drive the connector.
#[derive(Clone)]
pub struct LoadedConfig {
    /// Pass-through document for `ConnectorConfig::from_json` (CLI-only
    /// fields stripped; `ca_pem_file` expanded; env overrides applied).
    pub config_json: String,
    /// `{"setup_key":"...","jwt":"..."}` for `ConnectorSecrets::from_json`.
    /// NEVER printed; `Debug` on this struct is derived and the field IS
    /// secret-bearing, so this struct deliberately does not implement a
    /// derived Debug of secrets — see the manual redaction below.
    pub secrets_json: String,
    pub summary: ConfigSummary,
}

impl core::fmt::Debug for LoadedConfig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // secret discipline: `secrets_json` is REDACTED in all formatting
        f.debug_struct("LoadedConfig")
            .field("config_json", &"<config>")
            .field("secrets_json", &"REDACTED")
            .field("summary", &self.summary)
            .finish()
    }
}

/// Serialize one [`Json`] value back to compact JSON. Objects are NOT
/// supported inside CLI config values (the pass-through fields are scalars
/// and string arrays); this keeps the emitter 30 lines instead of a full
/// serializer.
fn emit_json_value(v: &Json) -> Result<String, CliError> {
    match v {
        Json::Null => Ok("null".into()),
        Json::Bool(b) => Ok(b.to_string()),
        Json::Num(n) => Ok(n.to_string()),
        Json::Str(s) => Ok(format!("\"{}\"", json_escape(s))),
        Json::Arr(items) => {
            let mut out = String::from("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&emit_json_value(item)?);
            }
            out.push(']');
            Ok(out)
        }
        Json::Obj(_) => Err(CliError::new(
            EXIT_CONFIG,
            "config field values may not be nested JSON objects \
             (use scalars or arrays of strings)",
        )),
    }
}

/// Read the config file with explicit permission-failure messaging, and warn
/// on stderr when the file is group/world-readable (credential hygiene).
fn read_config_file(path: &str) -> Result<String, CliError> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        CliError::new(EXIT_CONFIG, cli_io_message("config file", path, &e))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mode = meta.permissions().mode();
            if mode & 0o077 != 0 {
                eprintln!(
                    "warning: config file '{path}' is group/world-readable (mode {:o}); \
                     `chmod 600 {path}` is recommended — it carries a private key",
                    mode & 0o7777
                );
            }
        }
    }
    Ok(text)
}

/// Load + validate a CLI config file into the connector documents.
///
/// File shape = the `ConnectorConfig::from_json` document PLUS three
/// CLI-only conveniences (stripped from the pass-through): `setup_key`,
/// `jwt` / `jwt_token`, and `ca_pem_file` (path whose contents become
/// `ca_pem` — self-hosted deployments run their own CA). Environment
/// overrides (`NETBIRD_SETUP_KEY`, `NETBIRD_JWT`, `NETBIRD_MANAGEMENT_URL`,
/// `NETBIRD_CA_PEM`) WIN over the file so secrets can stay out of files
/// entirely.
pub fn load_cli_config(path: &str) -> Result<LoadedConfig, CliError> {
    let text = read_config_file(path)?;
    let doc = parse_document(&text).map_err(|e| {
        CliError::new(EXIT_CONFIG, format!("config file '{path}': invalid JSON: {e}"))
    })?;
    let entries = match doc {
        Json::Obj(entries) => entries,
        _ => {
            return Err(CliError::new(
                EXIT_CONFIG,
                format!("config file '{path}': expected a JSON object"),
            ))
        }
    };
    load_cli_entries(entries, &EnvSecrets::from_process())
        .map_err(|e| CliError::new(e.exit_code, format!("config file '{path}': {}", e.message)))
}

/// Pure core of [`load_cli_config`] (unit-testable without files/env).
/// Crate-private by design: `Json` is not part of the public surface; the
/// CLI consumes [`load_cli_config`].
fn load_cli_entries(
    entries: Vec<(String, Json)>,
    env: &EnvSecrets,
) -> Result<LoadedConfig, CliError> {
    // pull the CLI-only fields out, apply env overrides, rebuild pass-through
    let mut setup_key = String::new();
    let mut setup_key_source: &'static str = "none";
    let mut jwt = String::new();
    let mut jwt_source: &'static str = "none";
    let mut ca_pem_file: Option<String> = None;
    let mut passthrough: Vec<String> = Vec::with_capacity(entries.len());
    let mut management_url = String::new();
    let mut hostname = String::from("nbinterop");
    let mut ca_pem_from_file: Option<String> = None;

    for (key, val) in &entries {
        match key.as_str() {
            "setup_key" => {
                setup_key = as_string(val, "setup_key")?;
                setup_key_source = "file";
            }
            "jwt" | "jwt_token" => {
                jwt = as_string(val, key)?;
                jwt_source = "file";
            }
            "ca_pem_file" => {
                ca_pem_file = Some(as_string(val, "ca_pem_file")?);
            }
            "management_url" => management_url = as_string(val, "management_url")?,
            "hostname" => hostname = as_string(val, "hostname")?,
            "ca_pem" => {
                // keep in pass-through; a ca_pem_file expands to this key
                passthrough.push(format!("\"ca_pem\":{}", emit_json_value(val)?));
            }
            other => {
                passthrough.push(format!(
                    "\"{}\":{}",
                    json_escape(other),
                    emit_json_value(val)?
                ));
            }
        }
    }

    if let Some(f) = ca_pem_file {
        let pem = std::fs::read_to_string(&f).map_err(|e| {
            CliError::new(EXIT_CONFIG, cli_io_message("CA PEM file", &f, &e))
        })?;
        // an explicit ca_pem_file replaces any inline ca_pem (last wins, so
        // strip the inline one instead of shipping duplicate keys)
        passthrough.retain(|e| !e.starts_with("\"ca_pem\":"));
        ca_pem_from_file = Some(pem);
    }
    if let Some(pem) = ca_pem_from_file.as_ref() {
        passthrough.push(format!("\"ca_pem\":\"{}\"", json_escape(pem)));
    }
    if let Some(url) = env.management_url.as_ref() {
        management_url = url.clone();
    }
    if !management_url.is_empty() {
        passthrough.retain(|e| !e.starts_with("\"management_url\":"));
        passthrough.push(format!("\"management_url\":\"{}\"", json_escape(&management_url)));
    }
    if let Some(pem) = env.ca_pem.as_ref() {
        passthrough.retain(|e| !e.starts_with("\"ca_pem\":"));
        passthrough.push(format!("\"ca_pem\":\"{}\"", json_escape(pem)));
    }

    if env.setup_key.is_some() {
        setup_key = env.setup_key.clone().unwrap();
        setup_key_source = "env";
    }
    if let Some(j) = env.jwt.as_ref() {
        jwt = j.clone();
        jwt_source = "env";
    }

    // early validation through the REAL parser (also derives the public key)
    let config_json = format!("{{{}}}", passthrough.join(","));
    let cfg = crate::connector::ConnectorConfig::from_json(&config_json)
        .map_err(|e| CliError::new(EXIT_CONFIG, format!("invalid config: {e}")))?;

    let mut secrets_parts: Vec<String> = Vec::new();
    if !setup_key.is_empty() {
        secrets_parts.push(format!("\"setup_key\":\"{}\"", json_escape(&setup_key)));
    }
    if !jwt.is_empty() {
        secrets_parts.push(format!("\"jwt\":\"{}\"", json_escape(&jwt)));
    }
    if secrets_parts.is_empty() {
        return Err(CliError::new(
            EXIT_CREDENTIALS,
            format!(
                "no credentials: put a setup key in the config file ('setup_key') \
                 or in the {ENV_SETUP_KEY} environment variable"
            ),
        ));
    }

    Ok(LoadedConfig {
        config_json,
        secrets_json: format!("{{{}}}", secrets_parts.join(",")),
        summary: ConfigSummary {
            management_url: cfg.management_url.clone(),
            tls: cfg.management_url.starts_with("https://"),
            hostname,
            public_key_b64: cfg.keys.public_key_base64(),
            setup_key_source,
            setup_key_len: setup_key.len(),
            jwt_source: jwt_source,
            jwt_len: jwt.len(),
        },
    })
}

fn as_string(v: &Json, field: &str) -> Result<String, CliError> {
    match v {
        Json::Str(s) => Ok(s.clone()),
        _ => Err(CliError::new(
            EXIT_CONFIG,
            format!("config field '{field}': expected a string"),
        )),
    }
}

// ---------------------------------------------------------------------------
// dry-run plan
// ---------------------------------------------------------------------------

/// The `--dry-run` plan (and the stderr preamble of real runs): every step
/// the engine WOULD take, in order, with the redacted config summary. The
/// resolver/socket steps are listed as pending — a dry run performs NO
/// resolution, NO socket creation and NO connection.
pub fn plan_json(mode: &str, s: &ConfigSummary) -> String {
    let steps = [
        "parse-config",
        "resolve-management-host",
        "open-mgmt-tcp-socket",
        "connector-start-with-socket (login)",
        "feed-mgmt-socket-watermark",
        "wait-sync-network-map",
        "resolve-signal-uri",
        "open-signal-tcp-socket + feed",
        "feed-ice-udp-watermark",
        "feed-wg-udp-socket",
        "open-tun-standby-socketpair + feed",
        "wait-ice-selected-pair",
        "wait-wg-handshake",
        if mode == "peer" { "wait-peer-session (peer goal)" } else { "optional-probe-traffic" },
    ];
    let mut arr = String::from("[");
    for (i, step) in steps.iter().enumerate() {
        if i > 0 {
            arr.push(',');
        }
        arr.push_str(&format!("\"{step}\""));
    }
    arr.push(']');
    format!(
        "{{\"mode\":\"{}\",\"dry_run\":{},\"management_url\":\"{}\",\"tls\":{},\
         \"hostname\":\"{}\",\"public_key\":\"{}\",\"setup_key\":\"{}(len={})\",\
         \"jwt\":\"{}(len={})\",\"steps\":{}}}",
        mode,
        mode == "dry-run",
        json_escape(&s.management_url),
        s.tls,
        json_escape(&s.hostname),
        s.public_key_b64,
        s.setup_key_source,
        s.setup_key_len,
        s.jwt_source,
        s.jwt_len,
        arr,
    )
}

// ---------------------------------------------------------------------------
// selftest (offline; loopback-only socket use)
// ---------------------------------------------------------------------------

/// One selftest check result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckOutcome {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

/// Run ALL offline selftest checks. No network beyond loopback binds; no
/// management/signal server contact; no kernel TUN.
pub fn run_selftest() -> Vec<CheckOutcome> {
    vec![
        check_config_parse(),
        check_envelope_roundtrip(),
        check_ice_stun_offline(),
        check_wg_loopback_pair(),
    ]
}

/// JSON rendering of the selftest report (`{"checks":[...],"passed":N,"failed":M}`).
pub fn selftest_json(outcomes: &[CheckOutcome]) -> String {
    let mut checks = String::from("[");
    for (i, o) in outcomes.iter().enumerate() {
        if i > 0 {
            checks.push(',');
        }
        checks.push_str(&format!(
            "{{\"name\":\"{}\",\"ok\":{},\"detail\":\"{}\"}}",
            o.name,
            o.ok,
            json_escape(&o.detail)
        ));
    }
    checks.push(']');
    let failed = outcomes.iter().filter(|o| !o.ok).count();
    format!(
        "{{\"checks\":{},\"passed\":{},\"failed\":{}}}",
        checks,
        outcomes.len() - failed,
        failed
    )
}

fn check(name: &'static str, r: Result<String, String>) -> CheckOutcome {
    match r {
        Ok(detail) => CheckOutcome { name, ok: true, detail },
        Err(detail) => CheckOutcome { name, ok: false, detail },
    }
}

/// Check 1: connector config + secrets parsing (the real parsers, fresh
/// generated key — no file or network access).
fn check_config_parse() -> CheckOutcome {
    check("config-parse", config_parse_check())
}

fn config_parse_check() -> Result<String, String> {
    let kp = crate::envelope::EnvelopeKeyPair::generate()
        .map_err(|e| format!("key generation failed: {e:?}"))?;
    let cfg_json = format!(
        "{{\"management_url\":\"http://127.0.0.1:33073\",\"private_key\":\"{}\",\
         \"hostname\":\"nbinterop-selftest\"}}",
        kp.secret_base64()
    );
    let cfg = crate::connector::ConnectorConfig::from_json(&cfg_json)
        .map_err(|e| format!("ConnectorConfig rejected a valid document: {e}"))?;
    if cfg.management_url != "http://127.0.0.1:33073" {
        return Err("management_url roundtrip mismatch".into());
    }
    if cfg.keys.public_key_base64() != kp.public_key_base64() {
        return Err("public key derivation mismatch".into());
    }
    let secrets = crate::connector::ConnectorSecrets::from_json(
        "{\"setup_key\":\"selftest-not-a-real-key\"}",
    )
    .map_err(|e| format!("ConnectorSecrets rejected a valid document: {e}"))?;
    if secrets.setup_key != "selftest-not-a-real-key" {
        return Err("setup_key roundtrip mismatch".into());
    }
    if crate::connector::ConnectorSecrets::from_json("{\"setup_key\":\"\"}").is_ok() {
        return Err("empty credentials must be rejected".into());
    }
    Ok(format!(
        "config+secrets parsed; pub key {}",
        cfg.keys.public_key_base64()
    ))
}

/// Check 2: management envelope NaCl roundtrip + tamper rejection.
fn check_envelope_roundtrip() -> CheckOutcome {
    check("envelope-roundtrip", envelope_roundtrip_check())
}

fn envelope_roundtrip_check() -> Result<String, String> {
    use crate::envelope::{self, EnvelopeKeyPair, EnvelopePublicKey};
    let a = EnvelopeKeyPair::generate().map_err(|e| format!("keygen: {e:?}"))?;
    let b = EnvelopeKeyPair::generate().map_err(|e| format!("keygen: {e:?}"))?;
    let msg = b"nbinterop selftest envelope payload";
    let pub_b = EnvelopePublicKey::from_bytes(&b.public_key_bytes());
    let pub_a = EnvelopePublicKey::from_bytes(&a.public_key_bytes());
    let wire = envelope::seal(&pub_b, &a, msg).map_err(|e| format!("seal: {e:?}"))?;
    let plain = envelope::open(&pub_a, &b, &wire).map_err(|e| format!("open: {e:?}"))?;
    if plain != msg {
        return Err("decrypted payload mismatch".into());
    }
    let mut tampered = wire.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;
    if envelope::open(&pub_a, &b, &tampered).is_ok() {
        return Err("tampered ciphertext authenticated (must fail)".into());
    }
    Ok(format!("{}-byte wire sealed+opened; tamper rejected", wire.len()))
}

/// Check 3: STUN URI parsing, candidate marshal/unmarshal, and a full
/// offline gather over an injected interface + a fed loopback UDP socket
/// (`StaticInterfaces` + loopback bind — NO packet leaves the process).
fn check_ice_stun_offline() -> CheckOutcome {
    check("ice-stun-offline", ice_stun_offline_check())
}

fn ice_stun_offline_check() -> Result<String, String> {
    use crate::ice::{
        gather_candidates, parse_stun_uri, Candidate, GatherConfig, ProtectedUdpFdSource,
        StaticInterfaces, DEFAULT_INTERFACE_BLACKLIST, InterfaceAddr,
    };
    let srv = parse_stun_uri("stun:stun.example.invalid:3478")
        .map_err(|e| format!("stun URI rejected: {e:?}"))?;
    if srv.host != "stun.example.invalid" || srv.port != 3478 {
        return Err("stun URI fields mismatch".into());
    }
    if parse_stun_uri("turn:turn.example.invalid:3478").is_ok() {
        return Err("turn: URI must be rejected (no relay in this path)".into());
    }
    let cand = Candidate::host_candidate([127, 0, 0, 1], 51820);
    let wire = cand.marshal();
    let back =
        Candidate::unmarshal(&wire).map_err(|e| format!("candidate unmarshal: {e:?}"))?;
    if back.address != cand.address || back.port != cand.port || back.typ != cand.typ {
        return Err("candidate marshal roundtrip mismatch".into());
    }

    let fd = open_udp_unbound().map_err(|e| format!("udp socket: {e}"))?;
    let source = ProtectedUdpFdSource::new_with_fd(fd);
    let cfg = GatherConfig {
        blacklist: &DEFAULT_INTERFACE_BLACKLIST,
        servers: &[],
        timeout_ms: 200,
    };
    let ifaces = StaticInterfaces(vec![InterfaceAddr {
        name: "nbselftest0".into(),
        addr: [127, 0, 0, 1],
    }]);
    let res =
        gather_candidates(&cfg, &ifaces, &source).map_err(|e| format!("gather failed: {e:?}"))?;
    if res.host.len() != 1 || res.host[0].address != "127.0.0.1" {
        return Err(format!(
            "expected exactly one 127.0.0.1 host candidate, got {:?}",
            res.host.iter().map(|c| c.address.clone()).collect::<Vec<_>>()
        ));
    }
    if !res.srflx.is_empty() {
        return Err("no STUN servers configured — srflx must be empty".into());
    }
    if source.taken() != 1 {
        return Err(format!("source taken={}, expected 1", source.taken()));
    }
    unsafe { sys::close(fd) };
    Ok(format!(
        "stun uri ok; candidate wire ok; host gather ok ({} candidate)",
        res.host.len()
    ))
}

/// Check 4: two in-process WG devices complete a REAL BoringTun handshake
/// over loopback UDP and carry one probe packet EACH WAY through their TUN
/// stand-ins. Same construction as `tests/wg_e2e.rs`, driven live.
fn check_wg_loopback_pair() -> CheckOutcome {
    check("wg-loopback-pair", run_wg_loopback_pair())
}

fn run_wg_loopback_pair() -> Result<String, String> {
    use crate::tun::TunFd;
    use crate::wg_device::{WgDevice, WgDeviceConfig, WgPeerSpec};

    // synthetic keys — never real deployment secrets (same pattern as
    // tests/wg_e2e.rs; derived through boringtun's own x25519 export)
    const SECRET_A: [u8; 32] = [0xA5; 32];
    const SECRET_B: [u8; 32] = [0xB6; 32];
    const ADDR_A: [u8; 4] = [10, 99, 0, 1];
    const ADDR_B: [u8; 4] = [10, 99, 0, 2];

    let pub_of = |secret: &[u8; 32]| -> String {
        let k = boringtun::ffi::x25519_public_key(boringtun::ffi::x25519_key { key: *secret });
        crate::util::base64(&k.key)
    };
    let b64 = |bytes: &[u8]| crate::util::base64(bytes);
    let pub_a = pub_of(&SECRET_A);
    let pub_b = pub_of(&SECRET_B);

    let mut bag = FdBag::new();
    let fa = open_udp_ephemeral().map_err(|e| format!("udp A: {e}"))?;
    bag.keep(fa);
    let fb = open_udp_ephemeral().map_err(|e| format!("udp B: {e}"))?;
    bag.keep(fb);
    let port_a = udp_bound_port(fa).map_err(|e| format!("port A: {e}"))?;
    let port_b = udp_bound_port(fb).map_err(|e| format!("port B: {e}"))?;
    let (tun_a, hand_a) = open_tun_standby_pair().map_err(|e| format!("tun A: {e}"))?;
    bag.keep(tun_a);
    bag.keep(hand_a);
    let (tun_b, hand_b) = open_tun_standby_pair().map_err(|e| format!("tun B: {e}"))?;
    bag.keep(tun_b);
    bag.keep(hand_b);

    let mut cfg_a = WgDeviceConfig::new(b64(&SECRET_A));
    cfg_a.hs_retry_ms = 100;
    cfg_a.hs_deadline_ms = 5_000;
    cfg_a.keepalive_ms = u64::MAX / 2;
    cfg_a.session_max_ms = u64::MAX / 2;
    let mut cfg_b = WgDeviceConfig::new(b64(&SECRET_B));
    cfg_b.hs_retry_ms = 100;
    cfg_b.hs_deadline_ms = 5_000;
    cfg_b.keepalive_ms = u64::MAX / 2;
    cfg_b.session_max_ms = u64::MAX / 2;

    let tun_a_fd = TunFd::dup_from_raw(tun_a).map_err(|e| format!("tun dup A: {e:?}"))?;
    let tun_b_fd = TunFd::dup_from_raw(tun_b).map_err(|e| format!("tun dup B: {e:?}"))?;
    let mut dev_a = WgDevice::adopt(cfg_a, fa, tun_a_fd).map_err(|e| format!("adopt A: {e}"))?;
    let mut dev_b = WgDevice::adopt(cfg_b, fb, tun_b_fd).map_err(|e| format!("adopt B: {e}"))?;
    dev_a
        .set_peers(&[WgPeerSpec::new(pub_b.clone(), vec![(ADDR_B, 32)])])
        .map_err(|e| format!("peers A: {e}"))?;
    dev_a
        .set_endpoint(&pub_b, [127, 0, 0, 1], port_b, sys::mono_ms())
        .map_err(|e| format!("endpoint A: {e}"))?;
    dev_b
        .set_peers(&[WgPeerSpec::new(pub_a.clone(), vec![(ADDR_A, 32)])])
        .map_err(|e| format!("peers B: {e}"))?;
    dev_b
        .set_endpoint(&pub_a, [127, 0, 0, 1], port_a, sys::mono_ms())
        .map_err(|e| format!("endpoint B: {e}"))?;

    // drive the data planes; A injects immediately, B answers once traffic
    // came back so both sessions establish and BOTH directions carry a probe
    let deadline = std::time::Instant::now() + Duration::from_secs(4);
    let mut a_rx = false;
    let mut b_rx = false;
    let mut last_inject = 0u64;
    while std::time::Instant::now() < deadline && !(a_rx && b_rx) {
        let now = sys::mono_ms();
        if now.saturating_sub(last_inject) >= 150 {
            last_inject = now;
            let pkt = build_ipv4_udp_packet(ADDR_A, ADDR_B, 40000, 40001, b"nbinterop-probe-a2b");
            let (n, _) = sys::write_fd(hand_a, &pkt);
            if n <= 0 {
                return Err("probe inject into A's TUN stand-in failed".into());
            }
            if b_rx {
                let back = build_ipv4_udp_packet(ADDR_B, ADDR_A, 40001, 40000, b"nbinterop-probe-b2a");
                sys::write_fd(hand_b, &back);
            }
        }
        dev_a.service_tun(now);
        dev_a.service_udp(now);
        dev_a.tick(now);
        dev_b.service_tun(now);
        dev_b.service_udp(now);
        dev_b.tick(now);
        // observe delivered frames on both hands (drain via poll(0): the
        // hands are BLOCKING fds and an empty read(2) would hang); a
        // delivered frame is the full IPv4 packet — match the payload
        // marker anywhere inside it
        const MARK_A2B: &[u8] = b"nbinterop-probe-a2b";
        const MARK_B2A: &[u8] = b"nbinterop-probe-b2a";
        let mut buf = [0u8; 2048];
        for (hand, mark, seen_flag) in
            [(hand_b, MARK_A2B, &mut b_rx), (hand_a, MARK_B2A, &mut a_rx)]
        {
            loop {
                let (r, _, rev) = sys::poll1(hand, sys::POLLIN, 0);
                if r <= 0 || (rev & sys::POLLIN) == 0 {
                    break;
                }
                let (n, _) = sys::read_fd(hand, &mut buf);
                if n <= 0 {
                    break;
                }
                if buf[..n as usize].windows(mark.len()).any(|w| w == mark) {
                    *seen_flag = true;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let ready_a = dev_a.tunnel_ready();
    let ready_b = dev_b.tunnel_ready();
    if !(a_rx && b_rx && ready_a && ready_b) {
        return Err(format!(
            "no full bidirectional exchange in 4s (a_rx={a_rx} b_rx={b_rx} ready_a={ready_a} ready_b={ready_b})"
        ));
    }
    Ok("two WG devices handshook over loopback; probe packets delivered BOTH ways".into())
}

// ---------------------------------------------------------------------------
// tests (pure logic; loopback sockets only)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_cover_all_six_classes_plus_local_failures() {
        assert_eq!(exit_code_for_class("network"), 10);
        assert_eq!(exit_code_for_class("timeout"), 11);
        assert_eq!(exit_code_for_class("auth"), 12);
        assert_eq!(exit_code_for_class("request"), 13);
        assert_eq!(exit_code_for_class("server"), 14);
        assert_eq!(exit_code_for_class("parse"), 15);
        assert_eq!(exit_code_for_class("unsupported_url"), 16);
        assert_eq!(exit_code_for_class("mystery"), EXIT_UNKNOWN_CLASS);
    }

    #[test]
    fn signal_and_management_uri_parsing() {
        assert_eq!(
            parse_signal_uri("signal.example:10000").unwrap(),
            ("signal.example".into(), 10000)
        );
        assert_eq!(
            parse_signal_uri("rel://signal.example:10000").unwrap(),
            ("signal.example".into(), 10000)
        );
        assert_eq!(
            parse_signal_uri("rel://signal.example:10000/").unwrap(),
            ("signal.example".into(), 10000)
        );
        assert!(parse_signal_uri("signal.example").is_err(), "missing port");
        assert!(parse_signal_uri("rel://[::1]:10000").is_err(), "ipv6 unsupported");
        assert!(parse_signal_uri("rel://host:notaport").is_err(), "bad port");

        assert_eq!(
            parse_management_endpoint("https://mgmt.example:33073").unwrap(),
            (true, "mgmt.example".into(), 33073)
        );
        assert_eq!(
            parse_management_endpoint("http://127.0.0.1:33073/").unwrap(),
            (false, "127.0.0.1".into(), 33073)
        );
        assert_eq!(
            parse_management_endpoint("https://mgmt.example").unwrap(),
            (true, "mgmt.example".into(), 443)
        );
        assert!(parse_management_endpoint("mgmt.example:443").is_err(), "scheme required");
    }

    #[test]
    fn cli_io_messages_name_the_remediation() {
        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let m = cli_io_message("config file", "/tmp/secret.json", &denied);
        assert!(m.contains("permission denied"), "{m}");
        assert!(m.contains("chmod 600 /tmp/secret.json"), "{m}");
        let missing = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert!(cli_io_message("config file", "/tmp/x.json", &missing).contains("not found"));
    }

    #[test]
    fn load_cli_entries_splits_secrets_and_applies_env() {
        let entries = parse_document(
            &format!(
                "{{\"management_url\":\"https://mgmt.example:33073\",\"ca_pem\":\"PEM-DATA\",\
                 \"private_key\":\"{}\",\"hostname\":\"interop-a\",\"setup_key\":\"FILE-KEY\"}}",
                crate::util::base64(&[7u8; 32])
            ),
        )
        .unwrap();
        match entries {
            Json::Obj(entries) => {
                let loaded = load_cli_entries(entries, &EnvSecrets::default()).unwrap();
                // pass-through keeps the connector fields, drops the secrets
                assert!(loaded.config_json.contains("\"management_url\""));
                assert!(loaded.config_json.contains("\"ca_pem\":\"PEM-DATA\""));
                assert!(!loaded.config_json.contains("setup_key"), "{}", loaded.config_json);
                // secrets live in their own (never-printed) document
                assert!(loaded.secrets_json.contains("FILE-KEY"));
                assert_eq!(loaded.summary.setup_key_source, "file");
                assert_eq!(loaded.summary.setup_key_len, "FILE-KEY".len());
                assert_eq!(loaded.summary.public_key_b64.len() > 0, true);
                assert!(loaded.summary.tls);

                // env wins over file, source recorded
                let entries2 = match parse_document(
                    &format!(
                        "{{\"management_url\":\"https://file.example:1\",\"ca_pem\":\"PEM\",\
                         \"private_key\":\"{}\",\"setup_key\":\"FILE-KEY\"}}",
                        crate::util::base64(&[7u8; 32])
                    ),
                )
                .unwrap()
                {
                    Json::Obj(e) => e,
                    other => panic!("expected object, got {other:?}"),
                };
                let env = EnvSecrets {
                    setup_key: Some("ENV-KEY".into()),
                    management_url: Some("https://env.example:2".into()),
                    ..EnvSecrets::default()
                };
                let loaded2 = load_cli_entries(entries2, &env).unwrap();
                assert!(loaded2.config_json.contains("env.example:2"));
                assert!(!loaded2.config_json.contains("file.example"));
                assert!(loaded2.secrets_json.contains("ENV-KEY"));
                assert!(!loaded2.secrets_json.contains("FILE-KEY"));
                assert_eq!(loaded2.summary.setup_key_source, "env");
            }
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn load_cli_entries_requires_credentials() {
        let entries = parse_document(
            &format!(
                "{{\"management_url\":\"http://127.0.0.1:1\",\"private_key\":\"{}\"}}",
                crate::util::base64(&[7u8; 32])
            ),
        )
        .unwrap();
        let entries = match entries {
            Json::Obj(e) => e,
            other => panic!("expected object, got {other:?}"),
        };
        let err = load_cli_entries(entries, &EnvSecrets::default()).unwrap_err();
        assert_eq!(err.exit_code, EXIT_CREDENTIALS);
        assert!(err.message.contains(ENV_SETUP_KEY), "{}", err.message);
    }

    #[test]
    fn plan_is_secret_free_and_ordered() {
        let summary = ConfigSummary {
            management_url: "https://mgmt.example:33073".into(),
            tls: true,
            hostname: "interop-a".into(),
            public_key_b64: "PUBKEY".into(),
            setup_key_source: "env",
            setup_key_len: 30,
            jwt_source: "none",
            jwt_len: 0,
        };
        let plan = plan_json("dry-run", &summary);
        assert!(plan.contains("\"dry_run\":true"));
        assert!(plan.contains("connector-start-with-socket"));
        assert!(plan.contains("open-tun-standby-socketpair"));
        assert!(plan.contains("\"setup_key\":\"env(len=30)\""));
        assert!(!plan.contains("SECRET"), "{plan}");
        let run_plan = plan_json("connect", &summary);
        assert!(run_plan.contains("\"dry_run\":false"));
    }

    #[test]
    fn probe_packet_is_a_wellformed_ipv4_udp_frame() {
        let pkt = build_ipv4_udp_packet([10, 99, 0, 1], [10, 99, 0, 2], 40000, 40001, b"probe");
        assert_eq!(pkt.len(), 28 + 5);
        assert_eq!(pkt[0], 0x45);
        assert_eq!(u16::from_be_bytes([pkt[2], pkt[3]]), 33);
        assert_eq!(&pkt[12..16], &[10, 99, 0, 1]);
        assert_eq!(&pkt[16..20], &[10, 99, 0, 2]);
        assert_eq!(pkt[20 + 8..], *b"probe");
        // header checksum field is nonzero and verifies to zero
        let ck = u16::from_be_bytes([pkt[10], pkt[11]]);
        assert_ne!(ck, 0);
        let mut sum: u32 = 0;
        for w in pkt[..20].chunks(2) {
            sum += u16::from_be_bytes([w[0], w[1]]) as u32;
        }
        let verify = !((sum + (sum >> 16)) as u16);
        assert_eq!(verify, 0, "header checksum must verify");
    }

    #[test]
    fn response_parsers_map_tokens() {
        assert_eq!(
            parse_start_response("{\"started\":true,\"state\":\"connecting\"}").unwrap(),
            "connecting"
        );
        assert_eq!(
            parse_start_response("{\"started\":false,\"error\":\"management-socket-required\"}")
                .unwrap_err(),
            "management-socket-required"
        );
        assert_eq!(parse_feed_response("{\"ok\":true,\"queued\":3}").unwrap(), 3);
        assert_eq!(
            parse_feed_response("{\"ok\":false,\"error\":\"socket-fd-missing\"}").unwrap_err(),
            "socket-fd-missing"
        );
    }

    #[test]
    fn network_config_extractors() {
        let doc = "{\"available\":true,\"address\":\"100.64.0.5\",\"signal\":\"rel://sig.example:10000\"}";
        assert_eq!(
            signal_uri_from_network_config(doc).as_deref(),
            Some("rel://sig.example:10000")
        );
        assert_eq!(own_address_from_network_config(doc), Some([100, 64, 0, 5]));
        assert_eq!(signal_uri_from_network_config("{\"available\":false,\"reason\":\"no-network-map\"}"), None);
        assert_eq!(
            signal_uri_from_network_config("{\"available\":true,\"signal\":null}"),
            None
        );
    }

    #[test]
    fn json_lookup_walks_nested_shapes() {
        let doc =
            parse_document("{\"wg\":{\"peers_with_session\":2},\"ice\":{\"connected\":[1,2]}}")
                .unwrap();
        assert_eq!(json_lookup(&doc, "wg.peers_with_session"), Some(&Json::Num(2)));
        assert_eq!(json_lookup(&doc, "ice.connected.0"), Some(&Json::Num(1)));
        assert_eq!(json_lookup(&doc, "ice.connected.9"), None);
        assert_eq!(json_lookup(&doc, "missing.path"), None);
    }

    #[test]
    fn loopback_socket_creators_follow_their_contracts() {
        let tcp = open_tcp_prebound().expect("tcp prebound");
        assert!(tcp >= 0);
        let udp = open_udp_unbound().expect("udp unbound");
        assert!(udp >= 0);
        let port = udp_bound_port(udp).expect("unbound socket has no port yet");
        let _ = port; // unbound ⇒ OS-assigned 0 until the consumer binds
        unsafe { sys::close(udp) };

        let wga = open_udp_ephemeral().expect("udp ephemeral");
        let wport = udp_bound_port(wga).expect("bound port");
        assert!(wport > 0, "ephemeral bind must assign a port");
        unsafe { sys::close(wga) };
        unsafe { sys::close(tcp) };

        let (fed, hand) = open_tun_standby_pair().expect("socketpair");
        let (n, _) = sys::write_fd(hand, b"ping");
        assert_eq!(n, 4);
        let mut buf = [0u8; 8];
        let (rn, _) = sys::read_fd(fed, &mut buf);
        assert_eq!(rn, 4);
        assert_eq!(&buf[..4], b"ping");
        unsafe { sys::close(fed) };
        unsafe { sys::close(hand) };
    }

    #[test]
    fn fd_bag_closes_every_kept_fd() {
        let a = open_tcp_prebound().unwrap();
        let b = open_udp_unbound().unwrap();
        {
            let mut bag = FdBag::new();
            bag.keep(a);
            bag.keep(b);
            assert_eq!(bag.len(), 2);
            // still open inside the scope
            assert!(unsafe { sys::fcntl(a, sys::F_GETFD) } != -1);
        }
        // closed exactly once by Drop
        assert_eq!(unsafe { sys::fcntl(a, sys::F_GETFD) }, -1);
        assert_eq!(unsafe { sys::fcntl(b, sys::F_GETFD) }, -1);
        assert_eq!(sys::errno(), 9 /* EBADF */);
    }
}
