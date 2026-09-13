// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # ice — candidate gathering: host + server-reflexive (N5a)
//!
//! The candidate PRODUCTION half of ICE, upstream-shaped but agent-less:
//! host candidates from the machine's interfaces and srflx candidates from
//! the configured STUN servers, marshaled in the exact string form the
//! signal channel carries in `Body.payload` (pinned commit `791401060d2b`:
//! `client/internal/peer/signaler.go:32-41` — `Payload: candidate.Marshal()`
//! under `Body_CANDIDATE`; receiving side `client/internal/engine.go:2063-
//! 2071` — `ice.UnmarshalCandidate(msg.GetBody().Payload)`). What is NOT
//! here (connectivity checks, pair nomination, keepalive, TURN, renegotiation)
//! is listed at the bottom of this doc and in `docs/n3-ice-notes.md` §N5b.
//!
//! ## Upstream shape this reproduces (file:line, pinned `791401060d2b`)
//!
//! - **Candidate types**: upstream gathers
//!   `{host, server-reflexive, relay}` (`client/internal/peer/ice/agent.go:127-133`,
//!   p2p subset `agent.go:135-137`); prflx is never configured, pion derives
//!   it during checks. N5a produces `{host, srflx}` — TURN/relay is the
//!   explicit N5b boundary.
//! - **STUN servers**: `NetbirdConfig.stuns[].uri` parsed with
//!   `stun.ParseURI` (`client/internal/engine.go:1525-1541 updateSTUNs`;
//!   turns merged in `engine.go:1136-1144 updateNetbirdConfig`, consumed by
//!   the agent at `agent.go:54`). URI shape `stun:host:port` (default port
//!   3478). Our [`parse_stun_uri`] mirrors that; `turn:` URIs are rejected
//!   (`UnsupportedUrl`) — N5a has no relay client.
//! - **VPN interface exclusion**: upstream filters candidate interfaces
//!   twice — the hardcoded `lo` prefix and the prefix-matched
//!   `InterfaceBlackList` (`client/internal/stdnet/filter.go:13-24`), plus a
//!   wgctrl probe rejecting UNLISTED WireGuard interfaces
//!   (`filter.go:26-40`, not reproducible here — our default blacklist
//!   covers the common tunnel names instead). The blacklist default mirrors
//!   `client/internal/profilemanager/config.go:56-59`
//!   `DefaultInterfaceBlacklist` (including `iface.WgInterfaceDefault` =
//!   `"wt0"`, `client/iface/configurer/name.go:6`). [`interface_allowed`]
//!   reproduces the prefix semantics exactly; the wireguard interface — OUR
//!   tunnel — is in the default list, so the tunnel's own addresses never
//!   become candidates.
//! - **Priority / foundation**: RFC 8445 §5.1.2.1 two-stage formula with
//!   pion's type preferences (host 126, prflx 110, srflx 100, relay 0).
//! - **Timeouts**: pion's agent carries 4s keepalive / 6s disconnected /
//!   6s failed (`agent.go:22-24`). Those are agent-state timers — N5b
//!   scope. N5a adds only the per-STUN-exchange deadline
//!   ([`DEFAULT_STUN_TIMEOUT_MS`]).
//!
//! ## Protected sockets (governance §二.4, fail-closed)
//!
//! EVERY UDP socket this module touches comes from [`UdpSocketSource`] —
//! the [`crate::mgmtsock::ManagementSocketProvider`] provider pattern
//! generalized to UDP: the SHELL creates a fresh AF_INET/SOCK_DGRAM socket,
//! protects it (HarmonyOS `VpnConnection.protect`), and hands the fd number
//! over ([`ProtectedUdpFdSource`]; empty → fail-closed
//! [`ManagementError::Network`], never an unprotected dial). The consumer
//! dups the number (`crate::mgmtsock::dup_socket_fd`, the mgmtsock/tun
//! contract: the provided number is BORROWED, never read/written/closed
//! here) and binds/uses/closes exactly its own dup; each interface round
//! takes a FRESH fd — auditable via `ProtectedUdpFdSource::taken()`
//! (the mgmt `taken == attempts` contract). One socket per host candidate
//! interface (upstream mobile shape is one shared UDPMux socket,
//! `client/internal/engine_generic.go:15-16`; per-candidate sockets are the
//! platform adaptation that keeps every fd individually protected — see
//! the notes doc).
//!
//! ## fd / syscall surface additions (N5a)
//!
//! `sys.rs` gains `getsockname` (local port of the bound dup). Interface
//! enumeration and STUN hostname resolution use `getifaddrs`/`getaddrinfo`
//! resolved at RUNTIME via `dlopen("libc.so")+dlsym` (the `abi.rs`/`napi.rs`
//! idiom — no link-time dependency); if the symbols are absent the gather
//! fails closed with an explicit error, never a guessed address. `/dev/
//! urandom` (`O_RDONLY`, fixed path) feeds transaction-id entropy
//! (`stun::random_transaction_id`). Cross-compile surface verified by the
//! repo-external probe `refs/ice-probe` (exit 0, aarch64-unknown-linux-ohos).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::management::ManagementError;
use crate::mgmtsock::{dup_socket_fd, SocketSeamError};
use crate::stun::{self, StunReply, TransactionId};
use crate::sys;

// ---------------------------------------------------------------------------
// STUN server model (netbird_config.stuns)
// ---------------------------------------------------------------------------

/// One parsed `stun:host[:port]` URI — the element upstream stores as
/// `*stun.URI` (engine.go:1529-1537 `stun.ParseURI`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StunServer {
    /// Host literal or DNS name (resolved per gather round, IPv4 only).
    pub host: String,
    /// Port; 3478 when the URI omitted it (pion stun default).
    pub port: u16,
}

/// Parse a `stun:` URI from `netbird_config.stuns[].uri`.
///
/// Mirrors upstream `stun.ParseURI` for the STUN case (engine.go:1529-1531):
/// scheme required, default port 3478. `turn:` is a hard
/// [`ManagementError::UnsupportedUrl`] — TURN/relay is N5b scope and a
/// silently-ignored relay server would look like a working STUN set.
/// IPv6 literals are rejected (UDP4-only gather this increment; upstream has
/// the same lever as `DisableIPv6Discovery`, agent.go:64-68).
pub fn parse_stun_uri(uri: &str) -> Result<StunServer, ManagementError> {
    let rest = match uri.trim().split_once(':') {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("stun") => rest,
        Some((scheme, _)) if scheme.eq_ignore_ascii_case("turn") => {
            return Err(ManagementError::UnsupportedUrl(format!(
                "turn server '{uri}' not usable in N5a (stun-only gathering)"
            )));
        }
        _ => {
            return Err(ManagementError::UnsupportedUrl(format!(
                "stun uri '{uri}' lacks a stun: scheme"
            )));
        }
    };
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) => {
            if h.starts_with('[') {
                return Err(ManagementError::UnsupportedUrl(format!(
                    "ipv6 stun server '{uri}' unsupported (udp4-only gathering)"
                )));
            }
            let port: u16 = p
                .parse()
                .map_err(|_| ManagementError::Request { status: 0, message: format!("stun uri '{uri}' has a bad port") })?;
            if port == 0 {
                return Err(ManagementError::Request { status: 0, message: format!("stun uri '{uri}' port 0") });
            }
            (h, port)
        }
        None => (rest, 3478),
    };
    if host.is_empty() {
        return Err(ManagementError::Request { status: 0, message: format!("stun uri '{uri}' has no host") });
    }
    Ok(StunServer { host: host.to_string(), port })
}

// ---------------------------------------------------------------------------
// candidate model — the Body.payload wire form
// ---------------------------------------------------------------------------

/// RFC 8839 §5.1.2 candidate types. `Prflx`/`Relay` exist here so remote
/// candidates round-trip losslessly (peers may send them); THIS module only
/// ever produces [`CandidateType::Host`] / [`CandidateType::Srflx`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateType {
    Host,
    Srflx,
    Prflx,
    Relay,
}

impl CandidateType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CandidateType::Host => "host",
            CandidateType::Srflx => "srflx",
            CandidateType::Prflx => "prflx",
            CandidateType::Relay => "relay",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "host" => Some(CandidateType::Host),
            "srflx" => Some(CandidateType::Srflx),
            "prflx" => Some(CandidateType::Prflx),
            "relay" => Some(CandidateType::Relay),
            _ => None,
        }
    }

    /// pion's type preferences feeding the RFC 8445 §5.1.2.1 formula.
    fn type_preference(&self) -> u32 {
        match self {
            CandidateType::Host => 126,
            CandidateType::Prflx => 110,
            CandidateType::Srflx => 100,
            CandidateType::Relay => 0,
        }
    }
}

/// One ICE candidate, exactly the fields the `Body.payload` string carries
/// (`candidate.Marshal()` / `ice.UnmarshalCandidate`, upstream
/// signaler.go:32-41 / engine.go:2064).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub foundation: String,
    pub component: u32,
    /// Only "udp" is produced or accepted (upstream agent NetworkTypes are
    /// UDP4/UDP6, agent.go:53; TCP candidates are not in this protocol path).
    pub transport: String,
    pub priority: u32,
    /// Dotted-quad IPv4 string (UDP4-only gather).
    pub address: String,
    pub port: u16,
    pub typ: CandidateType,
    /// srflx/prflx/relay only: the base address/port (`raddr`/`rport`).
    pub related_address: Option<String>,
    pub related_port: Option<u16>,
}

/// RFC 8445 §5.1.2.1 local-preference ceiling and component id.
const LOCAL_PREFERENCE: u32 = 65535;
const COMPONENT_ID: u32 = 1;

impl Candidate {
    /// Host candidate on a local interface address (foundation derived from
    /// the base address — equal-address interfaces share a base, hence a
    /// foundation, which is the RFC's requirement, not a limitation).
    pub fn host_candidate(addr: [u8; 4], port: u16) -> Self {
        let address = ipv4_str(addr);
        let priority = priority_for(CandidateType::Host);
        Candidate {
            foundation: foundation("udp", "host", &address, None),
            component: COMPONENT_ID,
            transport: "udp".into(),
            priority,
            address,
            port,
            typ: CandidateType::Host,
            related_address: None,
            related_port: None,
        }
    }

    /// srflx candidate over `base` from a STUN mapping (RFC 8445 §5.1.3):
    /// `raddr`/`rport` point back at the base so the remote peer can pair.
    pub fn srflx_candidate(base: &Candidate, mapped_addr: [u8; 4], mapped_port: u16) -> Self {
        let address = ipv4_str(mapped_addr);
        let priority = priority_for(CandidateType::Srflx);
        Candidate {
            foundation: foundation("udp", "srflx", &address, Some(&base.address)),
            component: COMPONENT_ID,
            transport: "udp".into(),
            priority,
            address,
            port: mapped_port,
            typ: CandidateType::Srflx,
            related_address: Some(base.address.clone()),
            related_port: Some(base.port),
        }
    }

    /// The wire form `Body.payload` carries (pion `Candidate.Marshal()`):
    /// `candidate:<foundation> <component> udp <priority> <address> <port>
    /// typ <type> [raddr <addr> rport <port>] generation 0` — the RFC 8839
    /// candidate-attribute grammar; trailing extension attributes are legal
    /// and parsers ignore what they do not need.
    pub fn marshal(&self) -> String {
        let mut s = format!(
            "candidate:{} {} {} {} {} {} typ {}",
            self.foundation, self.component, self.transport, self.priority,
            self.address, self.port, self.typ.as_str()
        );
        if let (Some(ra), Some(rp)) = (&self.related_address, self.related_port) {
            s.push_str(&format!(" raddr {ra} rport {rp}"));
        }
        s.push_str(" generation 0");
        s
    }

    /// Inverse of [`Candidate::marshal`]; also accepts the `a=`-prefixed and
    /// prefix-less forms and ignores unknown extension attributes. Anything
    /// else is a [`ManagementError::Parse`] with a stable `candidate:*`
    /// token (remote peers can send arbitrary bytes; a bad candidate drops,
    /// nothing panics).
    pub fn unmarshal(payload: &str) -> Result<Self, ManagementError> {
        let bad = |tok: &'static str, detail: String| {
            ManagementError::Parse(format!("candidate:{tok} '{detail}'"))
        };
        let s = payload.trim();
        let s = s.strip_prefix("a=").unwrap_or(s);
        let s = s.strip_prefix("candidate:").ok_or_else(|| bad("missing-prefix", payload.to_string()))?;
        let mut toks = s.split_whitespace();
        let foundation = toks.next().ok_or_else(|| bad("empty", payload.to_string()))?.to_string();
        let take_u32 = |t: Option<&str>, what: &str| {
            t.and_then(|t| t.parse::<u32>().ok())
                .ok_or_else(|| bad("bad-field", format!("{what} in '{payload}'")))
        };
        let component = take_u32(toks.next(), "component")?;
        let transport = toks
            .next()
            .ok_or_else(|| bad("short", payload.to_string()))?
            .to_ascii_lowercase();
        if transport != "udp" {
            return Err(bad("transport", transport));
        }
        let priority = take_u32(toks.next(), "priority")?;
        let address = toks
            .next()
            .ok_or_else(|| bad("short", payload.to_string()))?
            .to_string();
        if address.is_empty() {
            return Err(bad("address", payload.to_string()));
        }
        let port_raw = take_u32(toks.next(), "port")?;
        if port_raw > u16::MAX as u32 {
            return Err(bad("port", payload.to_string()));
        }
        let port = port_raw as u16;
        if toks.next() != Some("typ") {
            return Err(bad("no-typ", payload.to_string()));
        }
        let typ = CandidateType::parse(toks.next().ok_or_else(|| bad("no-type", payload.to_string()))?)
            .ok_or_else(|| bad("type", payload.to_string()))?;
        let mut related_address = None;
        let mut related_port = None;
        while let Some(key) = toks.next() {
            match key {
                "raddr" => {
                    related_address =
                        Some(toks.next().ok_or_else(|| bad("raddr", payload.to_string()))?.to_string());
                }
                "rport" => {
                    let v = take_u32(toks.next(), "rport")?;
                    if v > u16::MAX as u32 {
                        return Err(bad("port", payload.to_string()));
                    }
                    related_port = Some(v as u16);
                }
                // generation, ufrag, network-cost, ... — legal extensions.
                _value => {
                    let _ = toks.next();
                }
            }
        }
        match (related_address.is_some(), related_port.is_some()) {
            (false, false) => {}
            (true, true) => {}
            _ => return Err(bad("related-pair", payload.to_string())),
        }
        Ok(Candidate {
            foundation,
            component,
            transport,
            priority,
            address,
            port,
            typ,
            related_address,
            related_port,
        })
    }
}

/// RFC 8445 §5.1.2.1: `(2^24)*type_pref + (2^8)*local_pref + (256 - component)`.
fn priority_for(typ: CandidateType) -> u32 {
    (1 << 24) * typ.type_preference() + (1 << 8) * LOCAL_PREFERENCE + (256 - COMPONENT_ID)
}

/// FNV-1a 32-bit over (transport, type, address, related base) — a
/// deterministic, dependency-free foundation token. Equal inputs MUST yield
/// equal foundations (RFC 8445 §5.1.1.1 groups identical bases).
fn foundation(transport: &str, typ: &str, address: &str, base: Option<&str>) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for chunk in [transport, typ, address, base.unwrap_or("-")] {
        for b in chunk.bytes() {
            hash ^= b as u32;
            hash = hash.wrapping_mul(0x0100_0193);
        }
        hash ^= 0x2f; // field separator
    }
    format!("{hash:08x}")
}

fn ipv4_str(addr: [u8; 4]) -> String {
    format!("{}.{}.{}.{}", addr[0], addr[1], addr[2], addr[3])
}

// ---------------------------------------------------------------------------
// interface source seam (host candidates)
// ---------------------------------------------------------------------------

/// One IPv4 interface address worth a host candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceAddr {
    pub name: String,
    pub addr: [u8; 4],
}

/// Where interface addresses come from. A seam, not a hardcode: production
/// uses [`SystemInterfaces`] (getifaddrs); tests and future shell-side
/// injection use [`StaticInterfaces`].
pub trait InterfaceSource: Send + Sync {
    fn list(&self) -> Result<Vec<InterfaceAddr>, ManagementError>;
}

/// Static list (tests; shell-supplied lists if a future increment swaps the
/// enumeration point).
pub struct StaticInterfaces(pub Vec<InterfaceAddr>);

impl InterfaceSource for StaticInterfaces {
    fn list(&self) -> Result<Vec<InterfaceAddr>, ManagementError> {
        Ok(self.0.clone())
    }
}

/// Production source: `getifaddrs` via `dlopen("libc.so")+dlsym` (no
/// link-time dependency; absent symbol → explicit error, fail-closed).
/// IPv4 entries only (UDP4 gather). musl `struct ifaddrs` layout, promoted
/// from the repo-external probe (refs/ice-probe, exit 0 on
/// aarch64-unknown-linux-ohos).
pub struct SystemInterfaces;

#[repr(C)]
struct IfAddrs {
    ifa_next: *mut IfAddrs,
    ifa_name: *const sys::c_char,
    ifa_flags: u32,
    ifa_addr: *const SockAddr,
    ifa_netmask: *const SockAddr,
    ifa_dstaddr: *const SockAddr,
    ifa_data: *mut core::ffi::c_void,
}

#[repr(C)]
struct SockAddr {
    sa_family: u16,
    sa_data: [u8; 14],
}

type GetIfAddrs = unsafe extern "C" fn(*mut *mut IfAddrs) -> sys::c_int;
type FreeIfAddrs = unsafe extern "C" fn(*mut IfAddrs);

impl InterfaceSource for SystemInterfaces {
    fn list(&self) -> Result<Vec<InterfaceAddr>, ManagementError> {
        let local = |tok: &'static str| ManagementError::Request { status: 0, message: tok.to_string() };
        unsafe {
            let libc = sys::dlopen(b"libc.so\0".as_ptr(), sys::RTLD_NOW);
            if libc.is_null() {
                return Err(local("interface-enum: dlopen libc.so failed"));
            }
            let getifaddrs: GetIfAddrs =
                core::mem::transmute(sys::dlsym(libc, b"getifaddrs\0".as_ptr()));
            let freeifaddrs: FreeIfAddrs =
                core::mem::transmute(sys::dlsym(libc, b"freeifaddrs\0".as_ptr()));
            if core::mem::transmute::<_, *mut core::ffi::c_void>(getifaddrs).is_null()
                || core::mem::transmute::<_, *mut core::ffi::c_void>(freeifaddrs).is_null()
            {
                return Err(local("interface-enum: getifaddrs unavailable"));
            }
            let mut head: *mut IfAddrs = core::ptr::null_mut();
            if getifaddrs(&mut head) != 0 {
                return Err(local("interface-enum: getifaddrs failed"));
            }
            let mut out = Vec::new();
            let mut cur = head;
            while !cur.is_null() {
                let ia = &*cur;
                if !ia.ifa_name.is_null() && !ia.ifa_addr.is_null() {
                    let sin = &*(ia.ifa_addr as *const sys::sockaddr_in);
                    if sin.sin_family == sys::AF_INET as u16 {
                        // Truncate at NUL ourselves — no CStr, no c_char
                        // signedness games (probe-pinned idiom).
                        let raw = core::slice::from_raw_parts(ia.ifa_name as *const u8, 16);
                        let len = raw.iter().position(|&b| b == 0).unwrap_or(16);
                        out.push(InterfaceAddr {
                            name: String::from_utf8_lossy(&raw[..len]).into_owned(),
                            addr: sin.sin_addr,
                        });
                    }
                }
                cur = ia.ifa_next;
            }
            freeifaddrs(head);
            Ok(out)
        }
    }
}

/// `DefaultInterfaceBlacklist` verbatim from upstream
/// (`client/internal/profilemanager/config.go:56-59`): the tunnel interface
/// (`wt0` first — `client/iface/configurer/name.go:6`) plus the usual
/// virtual/tunnel/docker names. Prefix-matched by [`interface_allowed`].
pub const DEFAULT_INTERFACE_BLACKLIST: [&str; 14] = [
    "wt0", "wt", "utun", "tun0", "zt", "ZeroTier", "wg", "ts",
    "Tailscale", "tailscale", "docker", "veth", "br-", "lo",
];

/// Upstream `stdnet.InterfaceFilter` prefix semantics
/// (`client/internal/stdnet/filter.go:13-24`): `lo*` always rejected
/// (hardcoded, filter.go:15-18), then every blacklist entry matches by
/// PREFIX (filter.go:19-24 — `"wt"` rejects `wt0`, `wth0`, ...). Upstream's
/// second layer — rejecting unlisted WireGuard interfaces via wgctrl
/// (filter.go:26-40) — is Go-runtime bound; coverage comes from the
/// blacklist itself (it contains "wt", "wg", "utun", "tun0", ...).
pub fn interface_allowed(name: &str, blacklist: &[&str]) -> bool {
    if name.starts_with("lo") {
        return false;
    }
    for prefix in blacklist {
        if name.starts_with(prefix) {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// protected UDP socket source (provider pattern, generalized to UDP)
// ---------------------------------------------------------------------------

/// Source of pre-protected UDP sockets — the UDP twin of
/// [`crate::mgmtsock::ManagementSocketProvider`]. Contract: ONE call = ONE
/// fresh, UNBOUND, AF_INET/SOCK_DGRAM socket the shell created AND
/// protected; the fd number is BORROWED (consumer dups it and never closes
/// the original); `Err` = fail-closed, the gather records the error and
/// produces NO candidate on that path (governance §二.4 — STUN/ICE sockets
/// protect before use, and there is no unprotected fallback).
pub trait UdpSocketSource: Send + Sync {
    fn take_fd(&self) -> Result<i32, SocketSeamError>;
}

/// Queue-backed production source — same shape, counters and audit surface
/// as mgmtsock's `ProtectedSocketFdSource` (feed on demand, `taken()`
/// audited against attempts in tests).
#[derive(Debug)]
pub struct ProtectedUdpFdSource {
    queue: Mutex<VecDeque<i32>>,
    taken: AtomicU64,
}

impl ProtectedUdpFdSource {
    pub fn new_with_fd(fd: i32) -> Self {
        let mut q = VecDeque::with_capacity(2);
        if fd >= 0 {
            q.push_back(fd);
        }
        ProtectedUdpFdSource { queue: Mutex::new(q), taken: AtomicU64::new(0) }
    }

    /// Shell-side resupply: push another fresh protected UDP socket fd.
    pub fn feed(&self, fd: i32) {
        if fd >= 0 {
            self.queue.lock().expect("udp fd queue").push_back(fd);
        }
    }

    /// Sockets currently queued (observability, mgmt parity).
    pub fn pending(&self) -> usize {
        self.queue.lock().expect("udp fd queue").len()
    }

    /// How many sockets handed out — pinned to the gather's socket attempts
    /// (every interface round re-acquires; never reuses a consumed fd).
    pub fn taken(&self) -> u64 {
        self.taken.load(Ordering::Acquire)
    }
}

impl UdpSocketSource for ProtectedUdpFdSource {
    fn take_fd(&self) -> Result<i32, SocketSeamError> {
        let fd = self.queue.lock().expect("udp fd queue").pop_front();
        match fd {
            Some(fd) => {
                self.taken.fetch_add(1, Ordering::AcqRel);
                Ok(fd)
            }
            None => Err(SocketSeamError::NoSocket),
        }
    }
}

/// `SocketSeamError` → the shared taxonomy (Network class; stable tokens).
fn seam_to_management(e: SocketSeamError) -> ManagementError {
    ManagementError::Network(format!("protected-udp: {} (errno={})", e.token(), e.errno()))
}

/// `O_NONBLOCK` on the DUP copy — mgmtsock's documented idiom (the shared
/// open-file description flips the provider's flag too; providers must not
/// do blocking I/O after hand-over).
fn set_nonblock(fd: sys::c_int) -> Result<(), SocketSeamError> {
    let fl = unsafe { sys::fcntl(fd, sys::F_GETFL) };
    if fl == -1 {
        return Err(SocketSeamError::NonBlock { errno: sys::errno() });
    }
    if unsafe { sys::fcntl(fd, sys::F_SETFL, fl | sys::O_NONBLOCK) } == -1 {
        return Err(SocketSeamError::NonBlock { errno: sys::errno() });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// gather orchestration
// ---------------------------------------------------------------------------

/// Per-STUN-exchange deadline (the only timer in N5a; pion's agent-level
/// keepalive/disconnected/failed timers — agent.go:22-24 — are N5b).
pub const DEFAULT_STUN_TIMEOUT_MS: u64 = 3000;

/// Gather parameters. `blacklist` defaults to [`DEFAULT_INTERFACE_BLACKLIST`]
/// at the call site; `servers` is the parsed `netbird_config.stuns`.
pub struct GatherConfig<'a> {
    pub blacklist: &'a [&'a str],
    pub servers: &'a [StunServer],
    pub timeout_ms: u64,
}

/// Everything one gather round produced. Partial success IS success: one
/// dead interface / server / provider slot must not sink the others
/// (upstream pion logs per-candidate failures and keeps gathering) — but
/// nothing is silently swallowed either, every failure is listed in
/// [`GatherResult::errors`].
#[derive(Debug, Clone, PartialEq)]
pub struct GatherResult {
    /// Host candidates (one per usable interface address).
    pub host: Vec<Candidate>,
    /// srflx candidates; `raddr`/`rport` reference their base host
    /// candidate.
    pub srflx: Vec<Candidate>,
    /// (context, error) for every failed interface / server attempt,
    /// context = `"<iface>"` or `"<iface>/<host>:<port>"`.
    pub errors: Vec<(String, ManagementError)>,
}

/// Gather host + srflx candidates.
///
/// Fail-closed points, in order: interface enumeration unavailable → hard
/// `Err` (no candidates without it); provider empty → per-interface
/// recorded error, and if NOTHING was gathered the whole call is `Err`
/// (the first error) so callers never mistake "no protected sockets" for
/// "no interfaces". An allowed interface with no usable addresses simply
/// yields nothing (an empty result with no errors is a legitimate outcome —
/// e.g. no IPv4 up-interfaces).
pub fn gather_candidates(
    cfg: &GatherConfig,
    ifaces: &dyn InterfaceSource,
    socks: &dyn UdpSocketSource,
) -> Result<GatherResult, ManagementError> {
    let all = ifaces.list()?;
    let mut host = Vec::new();
    let mut srflx = Vec::new();
    let mut errors: Vec<(String, ManagementError)> = Vec::new();

    // Resolve STUN hostnames once per gather round (IPv4); a dead hostname
    // is a recorded per-server error, not a gather failure.
    let mut resolved: Vec<([u8; 4], u16, &StunServer)> = Vec::new();
    for server in cfg.servers {
        match resolve_ipv4(&server.host) {
            Ok(ip) => resolved.push((ip, server.port, server)),
            Err(e) => errors.push((format!("-/{}", server.host), e)),
        }
    }

    let mut seen: Vec<(String, [u8; 4])> = Vec::new();
    for iface in all {
        if !interface_allowed(&iface.name, cfg.blacklist) {
            continue;
        }
        if seen.iter().any(|(n, a)| *n == iface.name && *a == iface.addr) {
            continue; // getifaddrs repeats entries (multi-flag)
        }
        seen.push((iface.name.clone(), iface.addr));
        if let Err(e) =
            gather_interface(&iface, &resolved, socks, cfg.timeout_ms, &mut host, &mut srflx, &mut errors)
        {
            errors.push((iface.name.clone(), e));
        }
    }

    if host.is_empty() && srflx.is_empty() && !errors.is_empty() {
        return Err(errors.remove(0).1);
    }
    Ok(GatherResult { host, srflx, errors })
}

/// One interface = one fresh protected fd = one bound dup = one host
/// candidate + its STUN exchanges. Every failure path closes exactly the
/// dup this function created and records an error — the provider's original
/// number is never touched (borrowed-number contract).
fn gather_interface(
    iface: &InterfaceAddr,
    servers: &[([u8; 4], u16, &StunServer)],
    socks: &dyn UdpSocketSource,
    timeout_ms: u64,
    host_out: &mut Vec<Candidate>,
    srflx_out: &mut Vec<Candidate>,
    errors_out: &mut Vec<(String, ManagementError)>,
) -> Result<(), ManagementError> {
    let provided = socks.take_fd().map_err(seam_to_management)?;
    let dup = dup_socket_fd(provided).map_err(seam_to_management)?;
    let cleanup_and = |dup: i32, e: ManagementError, errors_out: &mut Vec<(String, ManagementError)>| {
        unsafe { sys::close(dup) };
        errors_out.push((iface.name.clone(), e));
    };

    if let Err(e) = set_nonblock(dup) {
        cleanup_and(dup, seam_to_management(e), errors_out);
        return Ok(());
    }
    // Bind to THE interface address (host candidate binding) with an
    // ephemeral port; getsockname is the port the host candidate carries
    // (the srflx base).
    let local = sys::sockaddr_in::new(iface.addr, 0);
    if unsafe { sys::bind(dup, &local, core::mem::size_of::<sys::sockaddr_in>() as u32) } == -1 {
        cleanup_and(dup, ManagementError::Network(format!("stun-bind-failed (errno={})", sys::errno())), errors_out);
        return Ok(());
    }
    let mut name_addr = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut name_len = core::mem::size_of::<sys::sockaddr_in>() as u32;
    if unsafe { sys::getsockname(dup, &mut name_addr, &mut name_len) } == -1 {
        cleanup_and(dup, ManagementError::Network(format!("stun-getsockname-failed (errno={})", sys::errno())), errors_out);
        return Ok(());
    }
    let bound_port = u16::from_be(name_addr.sin_port);
    let base = Candidate::host_candidate(iface.addr, bound_port);
    host_out.push(base.clone());

    if servers.is_empty() {
        unsafe { sys::close(dup) };
        return Ok(());
    }

    // One transaction per server, all riding THIS interface's socket; the
    // dispatch loop below matches responses to servers by transaction id,
    // so interleaved answers can never be lost (single Outstanding per
    // socket would drop the other server's reply).
    let mut outstanding: Vec<(usize, TransactionId)> = Vec::new();
    let mut targets: Vec<(usize, sys::sockaddr_in, TransactionId)> = Vec::new();
    for (idx, (ip, port, server)) in servers.iter().enumerate() {
        let txn = match stun::random_transaction_id() {
            Ok(t) => t,
            Err(e) => {
                errors_out.push((format!("{}/{}:{}", iface.name, server.host, server.port), e));
                continue;
            }
        };
        targets.push((idx, sys::sockaddr_in::new(*ip, *port), txn));
        outstanding.push((idx, txn));
    }

    for (idx, addr, txn) in &targets {
        let req = stun::build_binding_request(*txn);
        let mut sent = false;
        for _retry in 0..3 {
            let n = unsafe {
                sys::sendto(
                    dup,
                    req.as_ptr() as *const core::ffi::c_void,
                    req.len(),
                    0,
                    addr,
                    core::mem::size_of::<sys::sockaddr_in>() as u32,
                )
            };
            if n >= 0 {
                sent = true;
                break;
            }
            if sys::errno() != sys::EINTR {
                break;
            }
        }
        if !sent {
            let errno = sys::errno();
            outstanding.retain(|(i, _)| i != idx);
            errors_out.push((
                format!("{}/{}:{}", iface.name, servers[*idx].2.host, servers[*idx].2.port),
                ManagementError::Network(format!("stun-send-failed (errno={errno})")),
            ));
        }
    }

    let deadline = sys::mono_ms().saturating_add(timeout_ms);
    let mut buf = [0u8; 1500];
    while !outstanding.is_empty() {
        let now = sys::mono_ms();
        if now >= deadline {
            break;
        }
        let wait = core::cmp::min(10, deadline - now) as sys::c_int; // 10ms ticks
        let (ret, poll_errno, revents) = sys::poll1(dup, sys::POLLIN, wait);
        if ret < 0 {
            if poll_errno == sys::EINTR {
                continue;
            }
            for (idx, _) in outstanding.drain(..) {
                errors_out.push((
                    format!("{}/{}:{}", iface.name, servers[idx].2.host, servers[idx].2.port),
                    ManagementError::Network(format!("stun-poll-failed (errno={poll_errno})")),
                ));
            }
            break;
        }
        if revents & sys::POLLIN == 0 {
            continue;
        }
        let mut src = sys::sockaddr_in::new([0, 0, 0, 0], 0);
        let mut src_len = core::mem::size_of::<sys::sockaddr_in>() as u32;
        let n = unsafe {
            sys::recvfrom(dup, buf.as_mut_ptr() as *mut core::ffi::c_void, buf.len(), 0, &mut src, &mut src_len)
        };
        if n <= 0 {
            continue; // EAGAIN / spurious wakeup — the deadline decides
        }
        let datagram = &buf[..n as usize];
        // Which outstanding transaction does this datagram answer? Unknown
        // or duplicate ids are dropped (stale datagrams are normal on a
        // reused port range).
        let txn_of = |msg: &[u8]| -> Option<usize> {
            if msg.len() < stun::HEADER_LEN {
                return None;
            }
            outstanding.iter().position(|(_, t)| msg[8..20] == t.0)
        };
        let Some(pos) = txn_of(datagram) else { continue };
        let (idx, txn) = outstanding.remove(pos);
        let server = servers[idx].2;
        let ctx = format!("{}/{}:{}", iface.name, server.host, server.port);
        match stun::parse_binding_response(datagram, &txn) {
            Ok(StunReply::Mapped { addr, port }) => {
                srflx_out.push(Candidate::srflx_candidate(&base, addr, port));
            }
            Ok(StunReply::Error(code)) => {
                errors_out.push((ctx, ManagementError::Server { status: code }));
            }
            Err(e) => errors_out.push((ctx, e)),
        }
    }
    for (idx, _) in outstanding.drain(..) {
        let server = servers[idx].2;
        errors_out.push((
            format!("{}/{}:{}", iface.name, server.host, server.port),
            ManagementError::Timeout,
        ));
    }
    unsafe { sys::close(dup) };
    Ok(())
}

// ---------------------------------------------------------------------------
// DNS resolve (dlopen getaddrinfo — see module docs)
// ---------------------------------------------------------------------------

#[repr(C)]
struct AddrInfo {
    ai_flags: sys::c_int,
    ai_family: sys::c_int,
    ai_socktype: sys::c_int,
    ai_protocol: sys::c_int,
    ai_addrlen: u32,
    ai_addr: *const SockAddr,
    ai_canonname: *const sys::c_char,
    ai_next: *const AddrInfo,
}

type GetAddrInfo =
    unsafe extern "C" fn(*const sys::c_char, *const sys::c_char, *const AddrInfo, *mut *const AddrInfo) -> sys::c_int;
type FreeAddrInfo = unsafe extern "C" fn(*const AddrInfo);

/// First IPv4 address of `host` (numeric literals included — getaddrinfo
/// handles both). AF_INET + SOCK_DGRAM hint. Absent symbol / failure →
/// [`ManagementError::Network`] with a stable token (fail-closed: no
/// address, no STUN query).
pub fn resolve_ipv4(host: &str) -> Result<[u8; 4], ManagementError> {
    let fail = |tok: String| ManagementError::Network(tok);
    // Numeric IPv4 fast path — no libc round trip for literals (mock tests
    // and literal-address deployments).
    let octets: Vec<&str> = host.split('.').collect();
    if octets.len() == 4 {
        let mut addr = [0u8; 4];
        let mut ok = true;
        for (i, o) in octets.iter().enumerate() {
            match o.parse::<u8>() {
                Ok(v) => addr[i] = v,
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        // Guard against "1.2.3.04"-style zero-padded or "1.2.3.4x" forms:
        // parse::<u8> already rejects those; require round-trip equality.
        if ok && ipv4_str(addr) == host {
            return Ok(addr);
        }
    }
    unsafe {
        let libc = sys::dlopen(b"libc.so\0".as_ptr(), sys::RTLD_NOW);
        if libc.is_null() {
            return Err(fail("stun-resolve: dlopen libc.so failed".into()));
        }
        let getaddrinfo: GetAddrInfo =
            core::mem::transmute(sys::dlsym(libc, b"getaddrinfo\0".as_ptr()));
        let freeaddrinfo: FreeAddrInfo =
            core::mem::transmute(sys::dlsym(libc, b"freeaddrinfo\0".as_ptr()));
        if core::mem::transmute::<_, *mut core::ffi::c_void>(getaddrinfo).is_null()
            || core::mem::transmute::<_, *mut core::ffi::c_void>(freeaddrinfo).is_null()
        {
            return Err(fail("stun-resolve: getaddrinfo unavailable".into()));
        }
        let mut hostz = host.as_bytes().to_vec();
        hostz.push(0);
        let hint = AddrInfo {
            ai_flags: 0,
            ai_family: sys::AF_INET,
            ai_socktype: sys::SOCK_DGRAM,
            ai_protocol: 0,
            ai_addrlen: 0,
            ai_addr: core::ptr::null(),
            ai_canonname: core::ptr::null(),
            ai_next: core::ptr::null(),
        };
        let mut res: *const AddrInfo = core::ptr::null();
        if getaddrinfo(hostz.as_ptr(), core::ptr::null(), &hint, &mut res) != 0 {
            return Err(fail(format!("stun-resolve: getaddrinfo failed for '{host}'")));
        }
        let mut out = Err(fail(format!("stun-resolve: no IPv4 address for '{host}'")));
        let mut cur = res;
        while !cur.is_null() {
            let ai = &*cur;
            if !ai.ai_addr.is_null() && (*ai.ai_addr).sa_family == sys::AF_INET as u16 {
                let sin = &*(ai.ai_addr as *const sys::sockaddr_in);
                out = Ok(sin.sin_addr);
                break;
            }
            cur = ai.ai_next;
        }
        freeaddrinfo(res);
        out
    }
}

// ---------------------------------------------------------------------------
// NOT in this module (N5b boundary, tracked in docs/n3-ice-notes.md):
// connectivity checks (Binding request/response with USE-CANDIDATE over
// candidate pairs), prflx derivation, pair nomination/selection, the agent
// state machine (4s keepalive / 6s disconnected / 6s failed), WG endpoint
// configuration (`ConfigureWGEndpoint`, conn.go:477), TURN/relay candidates,
// offer/answer signaling + ufrag/pwd credentials, renegotiation.
// ---------------------------------------------------------------------------
